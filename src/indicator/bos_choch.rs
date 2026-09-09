use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Structure Event Type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum StructureEventKind {
    BullishBos,
    BearishBos,
    BullishChoch,
    BearishChoch,
}

/// Break of Structure (BOS) and Change of Character (CHoCH) Detection Engine.
#[derive(Debug, Clone)]
pub struct BosChochEngine {
    pivot_len: usize,
    bars: VecDeque<Bar>,
    last_pivot_high: Option<f64>,
    last_pivot_low: Option<f64>,
    current_trend: i8, // 1 = Bullish, -1 = Bearish
    last_event: Option<StructureEventKind>,
}

impl BosChochEngine {
    pub fn new(pivot_len: usize) -> Self {
        Self {
            pivot_len: pivot_len.max(2),
            bars: VecDeque::with_capacity(pivot_len * 2 + 1),
            last_pivot_high: None,
            last_pivot_low: None,
            current_trend: 0,
            last_event: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5)
    }

    pub fn last_event(&self) -> Option<StructureEventKind> {
        self.last_event
    }
}

impl Indicator for BosChochEngine {
    fn name(&self) -> &str {
        "bos_choch"
    }

    fn warmup_period(&self) -> usize {
        self.pivot_len * 2 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.last_pivot_high = None;
        self.last_pivot_low = None;
        self.current_trend = 0;
        self.last_event = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.pivot_len * 2 + 1 {
            self.bars.pop_front();
        }

        if self.bars.len() < self.pivot_len * 2 + 1 {
            return None;
        }

        let mid_idx = self.pivot_len;
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

        if is_pivot_high {
            self.last_pivot_high = Some(mid_bar.high);
        }
        if is_pivot_low {
            self.last_pivot_low = Some(mid_bar.low);
        }

        self.last_event = None;

        if let Some(ph) = self.last_pivot_high {
            if bar.close > ph {
                if self.current_trend <= 0 {
                    self.current_trend = 1;
                    self.last_event = Some(StructureEventKind::BullishChoch);
                } else {
                    self.last_event = Some(StructureEventKind::BullishBos);
                }
                self.last_pivot_high = None; // Reset until next pivot
            }
        }

        if let Some(pl) = self.last_pivot_low {
            if bar.close < pl {
                if self.current_trend >= 0 {
                    self.current_trend = -1;
                    self.last_event = Some(StructureEventKind::BearishChoch);
                } else {
                    self.last_event = Some(StructureEventKind::BearishBos);
                }
                self.last_pivot_low = None; // Reset until next pivot
            }
        }

        let event_code = match self.last_event {
            Some(StructureEventKind::BullishBos) => 1.0,
            Some(StructureEventKind::BullishChoch) => 2.0,
            Some(StructureEventKind::BearishBos) => -1.0,
            Some(StructureEventKind::BearishChoch) => -2.0,
            None => 0.0,
        };

        let mut extra = HashMap::new();
        extra.insert("trend".to_string(), self.current_trend as f64);
        extra.insert("event_code".to_string(), event_code);

        Some(IndicatorOutput::with_extra(event_code, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut alerts = Vec::new();
        if let Some(event) = self.last_event {
            let note = match event {
                StructureEventKind::BullishBos => "Bullish Break of Structure (BOS)",
                StructureEventKind::BullishChoch => "Bullish Change of Character (CHoCH)",
                StructureEventKind::BearishBos => "Bearish Break of Structure (BOS)",
                StructureEventKind::BearishChoch => "Bearish Change of Character (CHoCH)",
            };
            alerts.push(IndicatorAlert::new("structure_break", note, 0.9));
        }
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bos_choch_detection() {
        let mut engine = BosChochEngine::new(3);
        for i in 0..30 {
            let price = 100.0 + (i as f64 * 1.5);
            let bar = Bar::new(i, price, price + 1.0, price - 1.0, price, 1000.0);
            engine.on_bar(&bar);
        }
        assert!(engine
            .on_bar(&Bar::new(30, 150.0, 155.0, 149.0, 154.0, 1000.0))
            .is_some());
    }
}
