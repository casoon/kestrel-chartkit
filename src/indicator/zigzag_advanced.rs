//! Advanced ZigZag: backstep, an ATR-scaled deviation mode, an explicitly exposed running
//! (unconfirmed) leg, per-node confirmation status, recursive/dual-degree levels, and
//! higher-timeframe projection — the capabilities [`super::zigzag::ZigZagEngine`]'s fixed-depth,
//! percent-only pivot detector does not offer. Complements rather than replaces it.

use std::collections::VecDeque;

use crate::model::Bar;
use crate::timeframe::Timeframe;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// How the minimum-swing deviation threshold is expressed.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ZigZagDeviationMode {
    /// Percent of the prior pivot's price.
    Percent(f64),
    /// Multiple of the engine's internal ATR.
    AtrMultiple(f64),
}

/// A single ZigZag swing point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZigZagNode {
    pub timestamp: i64,
    pub price: f64,
    pub is_high: bool,
    /// `false` while this node is still the extreme of the currently-forming leg (may still move
    /// as new bars extend it); `true` once price has reversed past the deviation threshold,
    /// permanently fixing this node.
    pub confirmed: bool,
}

/// Advanced ZigZag engine with backstep, ATR-mode deviation, and explicit confirmation status.
pub struct AdvancedZigZagEngine {
    depth: usize,
    backstep: usize,
    deviation: ZigZagDeviationMode,
    atr: Rma,
    prev_close: Option<f64>,
    bars: VecDeque<Bar>,
    bar_index: usize,
    nodes: Vec<ZigZagNode>,
    current_direction: i8,
    last_confirmed_bar_index: Option<usize>,
    alerts: Vec<IndicatorAlert>,
}

impl AdvancedZigZagEngine {
    pub fn new(
        depth: usize,
        backstep: usize,
        deviation: ZigZagDeviationMode,
        atr_len: usize,
    ) -> Self {
        let depth = depth.max(1);
        Self {
            depth,
            backstep,
            deviation,
            atr: Rma::new(atr_len.max(1)),
            prev_close: None,
            bars: VecDeque::with_capacity(depth * 2 + 1),
            bar_index: 0,
            nodes: Vec::new(),
            current_direction: 0,
            last_confirmed_bar_index: None,
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(3, 2, ZigZagDeviationMode::Percent(1.0), 14)
    }

    /// Confirmed swing history, oldest first.
    pub fn nodes(&self) -> &[ZigZagNode] {
        &self.nodes
    }

    /// The still-forming leg's current extreme, if any (may not yet be in [`Self::nodes`] or may
    /// be the unconfirmed last entry there).
    pub fn current_leg(&self) -> Option<&ZigZagNode> {
        self.nodes.last().filter(|n| !n.confirmed)
    }

    fn deviation_threshold(&self, atr: Option<f64>) -> f64 {
        match self.deviation {
            ZigZagDeviationMode::Percent(pct) => pct / 100.0,
            ZigZagDeviationMode::AtrMultiple(mult) => {
                // Expressed as a fraction of price for uniform comparison with the Percent mode;
                // callers using ATR mode should compare `mult * atr` directly if they need the
                // absolute price distance instead.
                match atr {
                    Some(a) if a > 0.0 => mult * a,
                    _ => f64::INFINITY, // ATR not warmed up yet: no pivot can confirm
                }
            }
        }
    }

    /// Recursively re-simplifies an already-reduced node sequence at a coarser deviation
    /// threshold, the "dual-/recursive levels" swing-degree technique: apply the same
    /// alternating-extreme simplification to the higher-degree input instead of raw bars.
    pub fn reduce(nodes: &[ZigZagNode], deviation_pct: f64) -> Vec<ZigZagNode> {
        if nodes.is_empty() {
            return Vec::new();
        }
        let threshold = deviation_pct / 100.0;
        let mut reduced: Vec<ZigZagNode> = vec![nodes[0]];

        for &node in &nodes[1..] {
            let last = *reduced.last().expect("seeded with nodes[0]");
            if node.is_high == last.is_high {
                // Same-type extreme: keep whichever is more extreme.
                let replace = (node.is_high && node.price > last.price)
                    || (!node.is_high && node.price < last.price);
                if replace {
                    *reduced.last_mut().unwrap() = node;
                }
                continue;
            }

            let change = if last.price != 0.0 {
                (node.price - last.price).abs() / last.price.abs()
            } else {
                f64::INFINITY
            };
            if change >= threshold {
                reduced.push(node);
            }
        }

        reduced
    }

    /// Projects a node sequence onto a higher timeframe's bucket grid, keeping only the most
    /// extreme high/low node per bucket per side — the confirmed HTF-equivalent swing points an
    /// LTF zigzag implies.
    pub fn project_to_timeframe(
        nodes: &[ZigZagNode],
        target_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Vec<ZigZagNode> {
        use std::collections::BTreeMap;

        let mut buckets: BTreeMap<(i64, bool), ZigZagNode> = BTreeMap::new();
        for &node in nodes {
            let bucket = target_tf.bucket_start(node.timestamp, utc_offset_seconds);
            let key = (bucket, node.is_high);
            buckets
                .entry(key)
                .and_modify(|existing| {
                    let more_extreme = (node.is_high && node.price > existing.price)
                        || (!node.is_high && node.price < existing.price);
                    if more_extreme {
                        *existing = node;
                    }
                })
                .or_insert(node);
        }

        let mut projected: Vec<ZigZagNode> = buckets.into_values().collect();
        projected.sort_by_key(|n| n.timestamp);
        projected
    }
}

impl Indicator for AdvancedZigZagEngine {
    fn name(&self) -> &str {
        "zigzag_advanced"
    }

    fn warmup_period(&self) -> usize {
        self.depth * 2 + 1
    }

    fn reset(&mut self) {
        self.atr.reset();
        self.prev_close = None;
        self.bars.clear();
        self.bar_index = 0;
        self.nodes.clear();
        self.current_direction = 0;
        self.last_confirmed_bar_index = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let tr = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        let atr = self.atr.update(tr);

        self.bars.push_back(bar.clone());
        if self.bars.len() > self.depth * 2 + 1 {
            self.bars.pop_front();
        }
        let current_bar_index = self.bar_index;
        self.bar_index += 1;

        if self.bars.len() < self.depth * 2 + 1 {
            return None;
        }

        let mid_idx = self.depth;
        let mid_bar = self.bars[mid_idx].clone();
        let mid_bar_index = current_bar_index - self.depth;

        let is_pivot_high = self
            .bars
            .iter()
            .enumerate()
            .all(|(i, b)| i == mid_idx || b.high <= mid_bar.high);
        let is_pivot_low = self
            .bars
            .iter()
            .enumerate()
            .all(|(i, b)| i == mid_idx || b.low >= mid_bar.low);

        let threshold = self.deviation_threshold(atr);

        let backstep_ok = self
            .last_confirmed_bar_index
            .map(|last| mid_bar_index >= last + self.backstep)
            .unwrap_or(true);

        if is_pivot_high {
            self.try_extend(
                true,
                mid_bar.high,
                mid_bar.timestamp,
                mid_bar_index,
                threshold,
                backstep_ok,
            );
        }
        if is_pivot_low {
            self.try_extend(
                false,
                mid_bar.low,
                mid_bar.timestamp,
                mid_bar_index,
                threshold,
                backstep_ok,
            );
        }

        let leg_price = self.nodes.last().map(|n| n.price).unwrap_or(mid_bar.close);
        Some(
            IndicatorOutput::new(leg_price).with_state(if self.current_leg().is_some() {
                "running"
            } else {
                "confirmed"
            }),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

impl AdvancedZigZagEngine {
    #[allow(clippy::too_many_arguments)]
    fn try_extend(
        &mut self,
        is_high: bool,
        price: f64,
        timestamp: i64,
        bar_index: usize,
        threshold: f64,
        backstep_ok: bool,
    ) {
        let opposite_direction = if is_high { 1 } else { -1 };

        if self.current_direction == opposite_direction || self.current_direction == 0 {
            // Extending/starting a leg in this direction: update the running (unconfirmed) node.
            let should_replace = match self.nodes.last() {
                Some(last) if !last.confirmed && last.is_high == is_high => {
                    (is_high && price > last.price) || (!is_high && price < last.price)
                }
                _ => true,
            };
            if should_replace {
                if let Some(last) = self
                    .nodes
                    .last_mut()
                    .filter(|n| !n.confirmed && n.is_high == is_high)
                {
                    *last = ZigZagNode {
                        timestamp,
                        price,
                        is_high,
                        confirmed: false,
                    };
                } else {
                    self.nodes.push(ZigZagNode {
                        timestamp,
                        price,
                        is_high,
                        confirmed: false,
                    });
                }
                self.current_direction = opposite_direction;
            }
            return;
        }

        // Opposite-direction pivot: only confirms the running leg (and starts a new one) once it
        // clears both the deviation threshold and the backstep spacing from the last confirmation.
        let last_price = self.nodes.last().map(|n| n.price);
        let change = match last_price {
            Some(lp) if lp != 0.0 => (price - lp).abs() / lp.abs(),
            _ => f64::INFINITY,
        };

        if change >= threshold && backstep_ok {
            if let Some(last) = self.nodes.last_mut() {
                last.confirmed = true;
            }
            self.nodes.push(ZigZagNode {
                timestamp,
                price,
                is_high,
                confirmed: false,
            });
            self.current_direction = opposite_direction;
            self.last_confirmed_bar_index = Some(bar_index);
            self.alerts.push(IndicatorAlert::new(
                "zigzag_pivot_confirmed",
                if is_high {
                    "ZigZag confirmed a swing low"
                } else {
                    "ZigZag confirmed a swing high"
                },
                0.6,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let price = if (i / 5) % 2 == 0 {
                    100.0 + (i % 5) as f64 * 4.0
                } else {
                    120.0 - (i % 5) as f64 * 4.0
                };
                Bar::new(i as i64 * 60, price, price + 1.0, price - 1.0, price, 100.0)
            })
            .collect()
    }

    #[test]
    fn test_produces_confirmed_and_running_nodes() {
        let mut engine = AdvancedZigZagEngine::new(2, 1, ZigZagDeviationMode::Percent(1.0), 5);
        for bar in sine_bars(40) {
            engine.on_bar(&bar);
        }
        assert!(!engine.nodes().is_empty());
        assert!(engine.nodes().iter().any(|n| n.confirmed));
    }

    #[test]
    fn test_backstep_suppresses_pivots_too_close_together() {
        let lenient = {
            let mut e = AdvancedZigZagEngine::new(2, 0, ZigZagDeviationMode::Percent(0.01), 5);
            for bar in sine_bars(40) {
                e.on_bar(&bar);
            }
            e.nodes().iter().filter(|n| n.confirmed).count()
        };
        let strict = {
            let mut e = AdvancedZigZagEngine::new(2, 20, ZigZagDeviationMode::Percent(0.01), 5);
            for bar in sine_bars(40) {
                e.on_bar(&bar);
            }
            e.nodes().iter().filter(|n| n.confirmed).count()
        };
        assert!(
            strict <= lenient,
            "a large backstep must never confirm more pivots than a near-zero one"
        );
    }

    #[test]
    fn test_atr_mode_requires_warm_atr_before_confirming() {
        let mut engine =
            AdvancedZigZagEngine::new(2, 0, ZigZagDeviationMode::AtrMultiple(0.5), 100);
        for bar in sine_bars(20) {
            engine.on_bar(&bar);
        }
        // ATR (len=100) never warms up within 20 bars, so the deviation threshold stays
        // infinite and no pivot can confirm.
        assert!(engine.nodes().iter().all(|n| !n.confirmed));
    }

    #[test]
    fn test_reduce_produces_a_coarser_recursive_level() {
        let base = vec![
            ZigZagNode {
                timestamp: 0,
                price: 100.0,
                is_high: false,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 1,
                price: 102.0,
                is_high: true,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 2,
                price: 101.0,
                is_high: false,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 3,
                price: 110.0,
                is_high: true,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 4,
                price: 95.0,
                is_high: false,
                confirmed: true,
            },
        ];
        // A large deviation must collapse the small 100->102->101 wiggle, keeping only the
        // genuinely large swings.
        let coarse = AdvancedZigZagEngine::reduce(&base, 5.0);
        assert!(coarse.len() < base.len());
        assert_eq!(coarse.first().unwrap().price, 100.0);
        assert_eq!(coarse.last().unwrap().price, 95.0);
    }

    #[test]
    fn test_project_to_timeframe_keeps_most_extreme_per_bucket() {
        let nodes = vec![
            ZigZagNode {
                timestamp: 0,
                price: 100.0,
                is_high: true,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 60,
                price: 105.0,
                is_high: true,
                confirmed: true,
            },
            ZigZagNode {
                timestamp: 120,
                price: 102.0,
                is_high: true,
                confirmed: true,
            },
        ];
        // All three fall inside the same 5-minute (300s) bucket starting at t=0.
        let projected = AdvancedZigZagEngine::project_to_timeframe(&nodes, Timeframe::Minute(5), 0);
        assert_eq!(projected.len(), 1);
        assert_eq!(
            projected[0].price, 105.0,
            "must keep the highest high within the bucket"
        );
    }
}
