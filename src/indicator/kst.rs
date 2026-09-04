use super::smoothing::Sma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

/// Know Sure Thing (KST) Engine.
/// KST = SMA(ROC(10),10)*1 + SMA(ROC(15),10)*2 + SMA(ROC(20),10)*3 + SMA(ROC(30),15)*4
#[derive(Debug, Clone)]
pub struct KstEngine {
    closes: VecDeque<f64>,
    sma1: Sma,
    sma2: Sma,
    sma3: Sma,
    sma4: Sma,
    signal_sma: Sma,
}

impl KstEngine {
    pub fn new() -> Self {
        Self {
            closes: VecDeque::with_capacity(31),
            sma1: Sma::new(10),
            sma2: Sma::new(10),
            sma3: Sma::new(10),
            sma4: Sma::new(15),
            signal_sma: Sma::new(9),
        }
    }
}

impl Default for KstEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for KstEngine {
    fn name(&self) -> &str {
        "kst"
    }

    fn warmup_period(&self) -> usize {
        54
    }

    fn reset(&mut self) {
        self.closes.clear();
        self.sma1.reset();
        self.sma2.reset();
        self.sma3.reset();
        self.sma4.reset();
        self.signal_sma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.closes.push_back(bar.close);
        if self.closes.len() > 31 {
            self.closes.pop_front();
        }

        if self.closes.len() < 31 {
            return None;
        }

        let c_curr = bar.close;
        let c_10 = self.closes[self.closes.len() - 11];
        let c_15 = self.closes[self.closes.len() - 16];
        let c_20 = self.closes[self.closes.len() - 21];
        let c_30 = self.closes[self.closes.len() - 31];

        let roc10 = if c_10 > 0.0 {
            (c_curr - c_10) / c_10 * 100.0
        } else {
            0.0
        };
        let roc15 = if c_15 > 0.0 {
            (c_curr - c_15) / c_15 * 100.0
        } else {
            0.0
        };
        let roc20 = if c_20 > 0.0 {
            (c_curr - c_20) / c_20 * 100.0
        } else {
            0.0
        };
        let roc30 = if c_30 > 0.0 {
            (c_curr - c_30) / c_30 * 100.0
        } else {
            0.0
        };

        let rc1 = self.sma1.update(roc10);
        let rc2 = self.sma2.update(roc15);
        let rc3 = self.sma3.update(roc20);
        let rc4 = self.sma4.update(roc30);

        let (r1, r2, r3, r4) = match (rc1, rc2, rc3, rc4) {
            (Some(r1), Some(r2), Some(r3), Some(r4)) => (r1, r2, r3, r4),
            _ => return None,
        };

        let kst_val = r1 * 1.0 + r2 * 2.0 + r3 * 3.0 + r4 * 4.0;
        let sig_val = self.signal_sma.update(kst_val)?;

        let mut extra = HashMap::new();
        extra.insert("kst".to_string(), kst_val);
        extra.insert("signal".to_string(), sig_val);
        extra.insert("hist".to_string(), kst_val - sig_val);

        Some(IndicatorOutput::with_extra(kst_val, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_kst_basic() {
        let mut kst = KstEngine::new();
        let mut out = None;
        for i in 0..100 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = kst.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
