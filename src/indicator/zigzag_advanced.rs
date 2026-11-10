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
///
/// Over the last `2 · depth + 1` bars the middle bar is a pivot high when no other high is above
/// its own and a pivot low when no other low is below its own; ties count, and one bar can be
/// both. A pivot of the same kind as the running (unconfirmed) node — or the very first pivot —
/// extends the leg: the running node moves to it if it is more extreme, and a first pivot starts
/// one. A pivot of the other kind confirms the running node and starts a new running node at its
/// own price, once it lies beyond the running node on its own side by at least the deviation
/// threshold — a pivot high above a running low, a pivot low below a running high — and the pivot
/// bar lies at least `backstep` bars after the pivot bar of the last confirmation (checked once
/// per bar, before either pivot is applied). Each confirmation raises an alert with the direction
/// of the turn: `bull_swing_confirmed` for a confirmed swing low, `bear_swing_confirmed` for a
/// confirmed swing high.
///
/// The move is signed. In [`ZigZagDeviationMode::Percent`] `(price - running) / |running|` for a
/// pivot high and `(running - price) / |running|` for a pivot low must reach `pct / 100`. In
/// [`ZigZagDeviationMode::AtrMultiple`] `price - running` (pivot high) or `running - price`
/// (pivot low) must reach `mult · ATR`, a Wilder ATR of the true range over `atr_len` as of the
/// current bar; nothing confirms before the ATR exists. A pivot on the wrong side — a pivot high
/// at or below the running low, as after a gap down behind which the lows keep falling —
/// confirms nothing at any threshold: the running low stays until a lower pivot low replaces it.
///
/// On a bar that is both a pivot high and a pivot low, the pivot of the running node's kind is
/// applied first (it may move the running node to this bar), then the other (it may confirm it).
/// Before the first node such a bar starts none, since nothing says which extreme came first.
///
/// With these rules [`Self::nodes`] is a pivot sequence in the sense of [`super::elliott`] after
/// every bar: highs and lows alternate, timestamps do not decrease, every high lies above and
/// every low below the node before it. A series and its mirror image (highs for lows, every
/// price negated) give mirrored nodes.
///
/// `value`: the price of the newest node, or the middle bar's close before any; `state`:
/// `"running"` while the newest node is unconfirmed, `"confirmed"` otherwise. First output with
/// bar `2 · depth + 1`.
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
                // A price distance: `try_extend` compares it with the signed price move.
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
    ///
    /// The first node is kept. After that a node of the kind of the last kept node replaces it if
    /// it is more extreme; a node of the other kind is kept if it lies beyond the last kept node
    /// on its own side (a high above it, a low below it) by at least `deviation_pct` percent of
    /// that node's price, and is dropped otherwise. The result is a pivot sequence for any input
    /// in time order, and at `deviation_pct = 0` it is the input itself if that already is one.
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

            if signed_move(node.is_high, node.price, last.price, true) >= threshold
                && beyond(node.is_high, node.price, last.price)
            {
                reduced.push(node);
            }
        }

        reduced
    }

    /// Projects a node sequence onto a higher timeframe's bucket grid: per bucket the most
    /// extreme high node and the most extreme low node (the first one on a tie), put in time
    /// order and then passed through [`Self::reduce`] at 0 %, so that the result is a pivot
    /// sequence again. That last step merges neighbouring nodes of one kind into the more extreme
    /// one and drops a node that does not lie beyond its predecessor on its own side — both occur
    /// when a bucket's extremes are taken out of their sequence.
    pub fn project_to_timeframe(
        nodes: &[ZigZagNode],
        target_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Vec<ZigZagNode> {
        use std::collections::BTreeMap;

        // Keyed by bucket and kind; the position keeps the input order among equal timestamps.
        let mut buckets: BTreeMap<(i64, bool), (usize, ZigZagNode)> = BTreeMap::new();
        for (position, &node) in nodes.iter().enumerate() {
            let bucket = target_tf.bucket_start(node.timestamp, utc_offset_seconds);
            let key = (bucket, node.is_high);
            buckets
                .entry(key)
                .and_modify(|existing| {
                    let more_extreme = (node.is_high && node.price > existing.1.price)
                        || (!node.is_high && node.price < existing.1.price);
                    if more_extreme {
                        *existing = (position, node);
                    }
                })
                .or_insert((position, node));
        }

        let mut projected: Vec<(usize, ZigZagNode)> = buckets.into_values().collect();
        projected.sort_by_key(|(position, n)| (n.timestamp, *position));
        let projected: Vec<ZigZagNode> = projected.into_iter().map(|(_, n)| n).collect();
        Self::reduce(&projected, 0.0)
    }
}

/// Whether `price` lies beyond `reference` on its own side: a high above it, a low below it.
fn beyond(is_high: bool, price: f64, reference: f64) -> bool {
    if is_high {
        price > reference
    } else {
        price < reference
    }
}

/// The move from `reference` to a pivot at `price`, positive when the pivot lies on its own side
/// (a high above, a low below): the price distance, or with `relative` that distance as a
/// fraction of `|reference|`. With a reference of zero the fraction is infinite on the pivot's
/// side and negative infinite otherwise.
fn signed_move(is_high: bool, price: f64, reference: f64, relative: bool) -> f64 {
    let distance = if is_high {
        price - reference
    } else {
        reference - price
    };
    if !relative {
        distance
    } else if reference != 0.0 {
        distance / reference.abs()
    } else if distance > 0.0 {
        f64::INFINITY
    } else {
        f64::NEG_INFINITY
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

        // A bar that is both: the pivot of the running node's kind first, and none before the
        // first node.
        let running_is_high = self.nodes.last().map(|n| n.is_high);
        let order: &[bool] = match (is_pivot_high, is_pivot_low, running_is_high) {
            (true, true, Some(true)) => &[true, false],
            (true, true, Some(false)) => &[false, true],
            (true, true, None) | (false, false, _) => &[],
            (true, false, _) => &[true],
            (false, true, _) => &[false],
        };
        for &is_high in order {
            self.try_extend(
                is_high,
                if is_high { mid_bar.high } else { mid_bar.low },
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
        // lies beyond the running node on its own side by the deviation threshold and clears the
        // backstep spacing from the last confirmation. Percent mode compares the relative move,
        // ATR mode the price distance, each signed and with the threshold of its own unit.
        let Some(running) = self.nodes.last().map(|n| n.price) else {
            return;
        };
        let relative = matches!(self.deviation, ZigZagDeviationMode::Percent(_));
        let change = signed_move(is_high, price, running, relative);

        if change >= threshold && beyond(is_high, price, running) && backstep_ok {
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
            // A confirmed swing low is a turn up (`bull_`), a swing high a turn down.
            self.alerts.push(IndicatorAlert::new(
                if is_high {
                    "bull_swing_confirmed"
                } else {
                    "bear_swing_confirmed"
                },
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

/// Registry definition of `zigzag_advanced` (plan 49). Both entry points take the same
/// parameters: `deviation_mode` (by name, or `0`/`1` in the `f64` map) and `deviation_pct`, the
/// deviation in percent (up to 100) or in ATR multiples (up to 1000).
pub const ZIGZAG_ADVANCED_DEFINITION: super::definition::IndicatorDefinition = super::definition::IndicatorDefinition {
    name: "zigzag_advanced",
    aliases: &[],
    description: "ZigZag with backstep, running-leg/confirmation status, and ATR-mode deviation (deviation_mode=percent|atr_multiple, in build_checked as 0|1; deviation_pct is the deviation in percent or in ATR multiples, deviation_value its former typed name); AdvancedZigZagEngine::reduce/project_to_timeframe for recursive levels and HTF projection",
    params: &[
        super::definition::ParamSpec::count("depth", 3, 1, 10000).meaning("bars on each side of a pivot"),
        super::definition::ParamSpec::count("backstep", 2, 0, 10000)
            .meaning("bars within which a newer pivot replaces an older one"),
        super::definition::ParamSpec::choice("deviation_mode", "percent", &["percent", "atr_multiple"], true)
            .meaning("unit of the deviation: percent of price or multiples of the ATR"),
        super::definition::ParamSpec::number("deviation_pct", 1.0, 0.001, 1000.0)
            .range_by(
                "deviation_mode",
                &[("percent", 0.001, 100.0), ("atr_multiple", 0.001, 1000.0)],
            )
            .legacy_alias("deviation_value")
            .meaning("minimum move for a new leg, in the unit of deviation_mode"),
        super::definition::ParamSpec::count("atr_len", 14, 1, 10000).meaning("bars in the ATR"),
    ],
    checks: &[],
    build: |p| {
        let value = p.number("deviation_pct");
        let deviation = match p.choice("deviation_mode") {
            "atr_multiple" => ZigZagDeviationMode::AtrMultiple(value),
            _ => ZigZagDeviationMode::Percent(value),
        };
        Ok(Box::new(AdvancedZigZagEngine::new(
            p.count("depth"),
            p.count("backstep"),
            deviation,
            p.count("atr_len"),
        )))
    },
};

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

    fn node(timestamp: i64, price: f64, is_high: bool) -> ZigZagNode {
        ZigZagNode {
            timestamp,
            price,
            is_high,
            confirmed: true,
        }
    }

    #[test]
    fn test_a_pivot_on_the_wrong_side_confirms_nothing_at_any_threshold() {
        // A rise to 110, a low of 100, two inside bars, then a gap down to bars whose highs peak
        // at 90 while the lows keep falling: the pivot high of 90 lies below the running low.
        let spans = [
            (106.0, 104.0),
            (108.0, 106.0),
            (110.0, 108.0),
            (106.0, 103.0),
            (102.0, 100.0),
            (101.5, 100.5),
            (101.0, 100.6),
            (88.0, 86.0),
            (90.0, 85.5),
            (89.0, 85.0),
            (88.0, 84.5),
        ];
        for deviation in [
            ZigZagDeviationMode::Percent(0.0),
            ZigZagDeviationMode::Percent(5.0),
            ZigZagDeviationMode::AtrMultiple(0.1),
        ] {
            let mut engine = AdvancedZigZagEngine::new(1, 0, deviation, 2);
            for (i, (high, low)) in spans.iter().enumerate() {
                engine.on_bar(&Bar::new(i as i64 * 60, *low, *high, *low, *high, 1.0));
            }
            let nodes: Vec<(f64, bool, bool)> = engine
                .nodes()
                .iter()
                .map(|n| (n.price, n.is_high, n.confirmed))
                .collect();
            assert_eq!(
                nodes,
                [(110.0, true, true), (100.0, false, false)],
                "{deviation:?}"
            );
        }
    }

    #[test]
    fn test_a_bar_that_is_both_pivots_extends_the_running_node_first() {
        // Running low 100; an outside bar with a lower low (96) and a high (108) above it. The
        // low moves to the bar, then its high confirms it.
        let spans = [
            (104.0, 102.0),
            (103.0, 100.0),
            (104.0, 101.0),
            (108.0, 96.0),
            (105.0, 99.0),
        ];
        let mut engine = AdvancedZigZagEngine::new(1, 0, ZigZagDeviationMode::Percent(1.0), 2);
        for (i, (high, low)) in spans.iter().enumerate() {
            engine.on_bar(&Bar::new(i as i64 * 60, *low, *high, *low, *high, 1.0));
        }
        let nodes: Vec<(i64, f64, bool, bool)> = engine
            .nodes()
            .iter()
            .map(|n| (n.timestamp, n.price, n.is_high, n.confirmed))
            .collect();
        assert_eq!(
            nodes,
            [(180, 96.0, false, true), (180, 108.0, true, false)],
            "one confirmation on the outside bar, low before high"
        );

        // Before the first node such a bar starts none: flat bars are pivot high and low at once.
        let mut flat = AdvancedZigZagEngine::new(1, 0, ZigZagDeviationMode::Percent(1.0), 2);
        for i in 0..6 {
            flat.on_bar(&Bar::new(i * 60, 100.0, 100.0, 100.0, 100.0, 1.0));
        }
        assert!(flat.nodes().is_empty());
    }

    #[test]
    fn test_reduce_drops_a_node_on_the_wrong_side() {
        // Not a pivot sequence: the high of 90 lies below the low of 100 before it.
        let nodes = [
            node(0, 120.0, true),
            node(1, 100.0, false),
            node(2, 90.0, true),
            node(3, 80.0, false),
            node(4, 95.0, true),
        ];
        let reduced = AdvancedZigZagEngine::reduce(&nodes, 0.0);
        let prices: Vec<f64> = reduced.iter().map(|n| n.price).collect();
        assert_eq!(prices, [120.0, 80.0, 95.0]);
    }

    #[test]
    fn test_project_to_timeframe_returns_a_pivot_sequence() {
        // Two 5-minute buckets of a pivot sequence. Per bucket the extremes are 200/100 and
        // 35/20; taken out of their sequence, the high of 35 follows the low of 100 it lies below.
        let nodes = [
            node(0, 200.0, true),
            node(60, 100.0, false),
            node(120, 150.0, true),
            node(300, 30.0, false),
            node(360, 35.0, true),
            node(420, 20.0, false),
        ];
        let projected = AdvancedZigZagEngine::project_to_timeframe(&nodes, Timeframe::Minute(5), 0);
        let prices: Vec<(i64, f64)> = projected.iter().map(|n| (n.timestamp, n.price)).collect();
        assert_eq!(prices, [(0, 200.0), (420, 20.0)]);
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
