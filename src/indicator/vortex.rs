use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

/// Vortex Indicator (+VI and -VI) Engine.
#[derive(Debug, Clone)]
pub struct VortexEngine {
    period: usize,
    prev_bar: Option<Bar>,
    vm_plus_window: VecDeque<f64>,
    vm_minus_window: VecDeque<f64>,
    tr_window: VecDeque<f64>,
}

impl VortexEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            prev_bar: None,
            vm_plus_window: VecDeque::with_capacity(period),
            vm_minus_window: VecDeque::with_capacity(period),
            tr_window: VecDeque::with_capacity(period),
        }
    }
}

impl Indicator for VortexEngine {
    fn name(&self) -> &str {
        "vortex"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.prev_bar = None;
        self.vm_plus_window.clear();
        self.vm_minus_window.clear();
        self.tr_window.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev = match &self.prev_bar {
            Some(p) => p.clone(),
            None => {
                self.prev_bar = Some(bar.clone());
                return None;
            }
        };
        self.prev_bar = Some(bar.clone());

        let vm_plus = (bar.high - prev.low).abs();
        let vm_minus = (bar.low - prev.high).abs();
        let tr = (bar.high - bar.low)
            .max((bar.high - prev.close).abs())
            .max((bar.low - prev.close).abs());

        self.vm_plus_window.push_back(vm_plus);
        self.vm_minus_window.push_back(vm_minus);
        self.tr_window.push_back(tr);

        if self.vm_plus_window.len() > self.period {
            self.vm_plus_window.pop_front();
            self.vm_minus_window.pop_front();
            self.tr_window.pop_front();
        }

        if self.vm_plus_window.len() < self.period {
            return None;
        }

        let sum_vm_plus: f64 = self.vm_plus_window.iter().sum();
        let sum_vm_minus: f64 = self.vm_minus_window.iter().sum();
        let sum_tr: f64 = self.tr_window.iter().sum::<f64>().max(1e-8);

        let vi_plus = sum_vm_plus / sum_tr;
        let vi_minus = sum_vm_minus / sum_tr;

        let mut extra = HashMap::new();
        extra.insert("vi_plus".to_string(), vi_plus);
        extra.insert("vi_minus".to_string(), vi_minus);

        Some(IndicatorOutput::with_extra(vi_plus, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vortex_indicator() {
        let mut vi = VortexEngine::new(14);
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = vi.on_bar(&b);
        }
        assert!(out.is_some());
        let o = out.unwrap();
        assert!(o.extra.contains_key("vi_plus"));
        assert!(o.extra.contains_key("vi_minus"));
    }
}
