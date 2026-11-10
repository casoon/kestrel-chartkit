use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// ZigZag Swing Node.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ZigZagNode {
    pub timestamp: i64,
    pub price: f64,
    pub is_high: bool,
}

/// ZigZag over fixed-depth pivots with a percent deviation.
///
/// Over the last `2 · depth + 1` bars the middle bar is a pivot high when no other high is above
/// its own and a pivot low when no other low is below its own; ties count, and one bar can be
/// both. The first pivot starts the first leg. After that a pivot of the kind of the newest node
/// moves that node to itself if it is more extreme. A pivot of the other kind adds a node, and
/// turns the leg, if it lies beyond the newest node on its own side by at least `deviation_pct`
/// percent of that node's price: `(high - last) / |last|` for a pivot high after a low,
/// `(last - low) / |last|` for a pivot low after a high. A pivot high at or below the newest low
/// (or a pivot low at or above the newest high) adds nothing.
///
/// On a bar that is both a pivot high and a pivot low, the pivot of the newest node's kind is
/// applied first, then the other; before the first node such a bar starts none.
///
/// [`Self::nodes`] therefore is a pivot sequence in the sense of [`super::elliott`] after every
/// bar: highs and lows alternate, timestamps do not decrease, every high lies above and every low
/// below the node before it.
///
/// `value` and `extra["last_pivot_price"]`: the price of the newest node (0 before any);
/// `extra["direction"]`: 1 while the newest node is a high, -1 while it is a low, 0 before any.
/// First output with bar `2 · depth + 1`. No alerts.
#[derive(Debug, Clone)]
pub struct ZigZagEngine {
    depth: usize,
    deviation_pct: f64,
    bars: VecDeque<Bar>,
    nodes: Vec<ZigZagNode>,
    current_direction: i8, // 1 = Bullish leg (up), -1 = Bearish leg (down)
    last_pivot_price: f64,
    last_pivot_ts: i64,
}

impl ZigZagEngine {
    pub fn new(depth: usize, deviation_pct: f64) -> Self {
        Self {
            depth: depth.max(2),
            deviation_pct: deviation_pct.max(0.0001),
            bars: VecDeque::with_capacity(depth * 2),
            nodes: Vec::new(),
            current_direction: 0,
            last_pivot_price: 0.0,
            last_pivot_ts: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(12, 5.0) // 12 depth, 5.0% deviation
    }

    pub fn nodes(&self) -> &[ZigZagNode] {
        &self.nodes
    }

    /// Applies one pivot of the middle bar: starts the first leg, moves the newest node, or adds
    /// a node of the other kind once it lies the deviation beyond the newest one.
    fn apply_pivot(&mut self, is_high: bool, price: f64, timestamp: i64) {
        let same = if is_high { 1 } else { -1 };
        if self.current_direction == same {
            let more_extreme = if is_high {
                price > self.last_pivot_price
            } else {
                price < self.last_pivot_price
            };
            if more_extreme {
                self.last_pivot_price = price;
                self.last_pivot_ts = timestamp;
                if let Some(last_node) = self.nodes.last_mut() {
                    last_node.timestamp = timestamp;
                    last_node.price = price;
                }
            }
            return;
        }

        if self.current_direction != 0 {
            let last = self.last_pivot_price;
            let distance = if is_high { price - last } else { last - price };
            // The move as a fraction of the newest node's price; at a price of zero any move to
            // the pivot's own side counts.
            let reached = if last != 0.0 {
                distance / last.abs() >= self.deviation_pct / 100.0
            } else {
                true
            };
            if !(distance > 0.0 && reached) {
                return;
            }
        }
        self.current_direction = same;
        self.last_pivot_price = price;
        self.last_pivot_ts = timestamp;
        self.nodes.push(ZigZagNode {
            timestamp,
            price,
            is_high,
        });
    }
}

impl Indicator for ZigZagEngine {
    fn name(&self) -> &str {
        "zigzag"
    }

    fn warmup_period(&self) -> usize {
        self.depth * 2 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.nodes.clear();
        self.current_direction = 0;
        self.last_pivot_price = 0.0;
        self.last_pivot_ts = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.depth * 2 + 1 {
            self.bars.pop_front();
        }

        if self.bars.len() < self.depth * 2 + 1 {
            return None;
        }

        let mid_idx = self.depth;
        let mid_bar = &self.bars[mid_idx];

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
        let (timestamp, high, low) = (mid_bar.timestamp, mid_bar.high, mid_bar.low);

        // A bar that is both: the pivot of the newest node's kind first, and none before the
        // first node.
        let order: &[bool] = match (is_pivot_high, is_pivot_low, self.current_direction) {
            (true, true, 0) | (false, false, _) => &[],
            (true, true, 1) => &[true, false],
            (true, true, _) => &[false, true],
            (true, false, _) => &[true],
            (false, true, _) => &[false],
        };
        for &is_high in order {
            self.apply_pivot(is_high, if is_high { high } else { low }, timestamp);
        }

        let mut extra = HashMap::new();
        extra.insert("direction".to_string(), self.current_direction as f64);
        extra.insert("last_pivot_price".to_string(), self.last_pivot_price);

        Some(IndicatorOutput::with_extra(self.last_pivot_price, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const ZIGZAG_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "zigzag",
        aliases: &[],
        description: "ZigZag Swing Leg Engine",
        params: &[
            super::definition::ParamSpec::count("depth", 12, 2, 10000)
                .meaning("bars a swing point must dominate"),
            super::definition::ParamSpec::number("deviation_pct", 5.0, 0.01, 100.0)
                .meaning("minimum price move, in percent, that starts a new leg"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(ZigZagEngine::new(
                p.count("depth"),
                p.number("deviation_pct"),
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zigzag_nodes() {
        let mut zz = ZigZagEngine::new(3, 1.0);
        for i in 0..30 {
            let price = if (i / 5) % 2 == 0 {
                100.0 + (i % 5) as f64 * 2.0
            } else {
                110.0 - (i % 5) as f64 * 2.0
            };
            let bar = Bar::new(i as i64, price, price + 1.0, price - 1.0, price, 1000.0);
            zz.on_bar(&bar);
        }
        assert!(!zz.nodes().is_empty());
    }
}
