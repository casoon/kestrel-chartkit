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

/// Advanced ZigZag Engine tracking multi-pivot swing legs and trend reversals.
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

        let dev_thresh = self.deviation_pct / 100.0;

        if is_pivot_high {
            if self.current_direction <= 0 {
                let change = if self.last_pivot_price > 0.0 {
                    (mid_bar.high - self.last_pivot_price) / self.last_pivot_price
                } else {
                    1.0
                };
                if change >= dev_thresh || self.current_direction == 0 {
                    self.current_direction = 1;
                    self.last_pivot_price = mid_bar.high;
                    self.last_pivot_ts = mid_bar.timestamp;
                    self.nodes.push(ZigZagNode {
                        timestamp: mid_bar.timestamp,
                        price: mid_bar.high,
                        is_high: true,
                    });
                }
            } else if mid_bar.high > self.last_pivot_price {
                // Update higher high node
                self.last_pivot_price = mid_bar.high;
                self.last_pivot_ts = mid_bar.timestamp;
                if let Some(last_node) = self.nodes.last_mut() {
                    if last_node.is_high {
                        last_node.timestamp = mid_bar.timestamp;
                        last_node.price = mid_bar.high;
                    }
                }
            }
        }

        if is_pivot_low {
            if self.current_direction >= 0 {
                let change = if self.last_pivot_price > 0.0 {
                    (self.last_pivot_price - mid_bar.low) / self.last_pivot_price
                } else {
                    1.0
                };
                if change >= dev_thresh || self.current_direction == 0 {
                    self.current_direction = -1;
                    self.last_pivot_price = mid_bar.low;
                    self.last_pivot_ts = mid_bar.timestamp;
                    self.nodes.push(ZigZagNode {
                        timestamp: mid_bar.timestamp,
                        price: mid_bar.low,
                        is_high: false,
                    });
                }
            } else if mid_bar.low < self.last_pivot_price {
                // Update lower low node
                self.last_pivot_price = mid_bar.low;
                self.last_pivot_ts = mid_bar.timestamp;
                if let Some(last_node) = self.nodes.last_mut() {
                    if !last_node.is_high {
                        last_node.timestamp = mid_bar.timestamp;
                        last_node.price = mid_bar.low;
                    }
                }
            }
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
