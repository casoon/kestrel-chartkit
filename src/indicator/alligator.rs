use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

/// Williams Alligator Engine (Jaw, Teeth, Lips).
/// Jaw = 13 SMMA (RMA) on hl2, Teeth = 8 SMMA on hl2, Lips = 5 SMMA on hl2.
#[derive(Debug, Clone)]
pub struct AlligatorEngine {
    jaw_rma: Rma,
    teeth_rma: Rma,
    lips_rma: Rma,
    jaw_window: VecDeque<f64>,
    teeth_window: VecDeque<f64>,
    lips_window: VecDeque<f64>,
}

impl AlligatorEngine {
    pub fn new() -> Self {
        Self {
            jaw_rma: Rma::new(13),
            teeth_rma: Rma::new(8),
            lips_rma: Rma::new(5),
            jaw_window: VecDeque::with_capacity(9),
            teeth_window: VecDeque::with_capacity(6),
            lips_window: VecDeque::with_capacity(4),
        }
    }
}

impl Default for AlligatorEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AlligatorEngine {
    fn name(&self) -> &str {
        "alligator"
    }

    fn warmup_period(&self) -> usize {
        13 + 8
    }

    fn reset(&mut self) {
        self.jaw_rma.reset();
        self.teeth_rma.reset();
        self.lips_rma.reset();
        self.jaw_window.clear();
        self.teeth_window.clear();
        self.lips_window.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let hl2 = (bar.high + bar.low) / 2.0;

        let raw_jaw = self.jaw_rma.update(hl2);
        let raw_teeth = self.teeth_rma.update(hl2);
        let raw_lips = self.lips_rma.update(hl2);

        if let (Some(j), Some(t), Some(l)) = (raw_jaw, raw_teeth, raw_lips) {
            // Apply forward offsets: Jaw 8, Teeth 5, Lips 3. A window holding N+1 values and
            // reading `front()` yields the value computed N pushes ago, i.e. the desired forward
            // offset of N bars -- so the window must be allowed to grow to N+1 before trimming.
            self.jaw_window.push_back(j);
            if self.jaw_window.len() > 9 {
                self.jaw_window.pop_front();
            }

            self.teeth_window.push_back(t);
            if self.teeth_window.len() > 6 {
                self.teeth_window.pop_front();
            }

            self.lips_window.push_back(l);
            if self.lips_window.len() > 4 {
                self.lips_window.pop_front();
            }

            let jaw_val = *self.jaw_window.front().unwrap();
            let teeth_val = *self.teeth_window.front().unwrap();
            let lips_val = *self.lips_window.front().unwrap();

            let mut extra = HashMap::new();
            extra.insert("jaw".to_string(), jaw_val);
            extra.insert("teeth".to_string(), teeth_val);
            extra.insert("lips".to_string(), lips_val);

            Some(IndicatorOutput::with_extra(lips_val, extra))
        } else {
            None
        }
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alligator_basic() {
        let mut gator = AlligatorEngine::new();
        let mut out = None;
        for i in 0..40 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = gator.on_bar(&b);
        }
        assert!(out.is_some());
        let o = out.unwrap();
        assert!(o.extra.contains_key("jaw"));
        assert!(o.extra.contains_key("teeth"));
        assert!(o.extra.contains_key("lips"));
    }
}
