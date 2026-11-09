use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::{HashMap, VecDeque};

/// Relative Vigor Index (Ehlers).
///
/// `num_t = swma(close - open)`, `den_t = swma(high - low)` with the symmetric four-bar weighting
/// `swma(x)_t = (x_(t-3) + 2 x_(t-2) + 2 x_(t-1) + x_t) / 6`;
/// `RVI = sum(num, period) / sum(den, period)`, `0` when the range sum is `0` (then every
/// close-open span is `0` as well). `extra["signal"]` is `swma(RVI)` and `extra["rvi"]` repeats
/// the value. `RVI` lies in `-1..=1`; mirrored bars give `-RVI`.
///
/// First output: once the signal exists, i.e. with the `period + 6`-th bar. [`Indicator::reset`]
/// clears all state.
#[derive(Debug, Clone)]
pub struct RviEngine {
    period: usize,
    spans: VecDeque<(f64, f64)>,
    weighted: VecDeque<(f64, f64)>,
    rvis: VecDeque<f64>,
}

fn swma(x: [f64; 4]) -> f64 {
    (x[0] + 2.0 * x[1] + 2.0 * x[2] + x[3]) / 6.0
}

impl RviEngine {
    pub fn new(period: usize) -> Self {
        let period = period.max(1);
        Self {
            period,
            spans: VecDeque::with_capacity(4),
            weighted: VecDeque::with_capacity(period + 1),
            rvis: VecDeque::with_capacity(4),
        }
    }
}

impl Indicator for RviEngine {
    fn name(&self) -> &str {
        "rvi"
    }

    fn warmup_period(&self) -> usize {
        self.period + 6
    }

    fn reset(&mut self) {
        self.spans.clear();
        self.weighted.clear();
        self.rvis.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.spans
            .push_back((bar.close - bar.open, bar.high - bar.low));
        if self.spans.len() > 4 {
            self.spans.pop_front();
        }
        if self.spans.len() < 4 {
            return None;
        }
        let s = &self.spans;
        let num = swma([s[0].0, s[1].0, s[2].0, s[3].0]);
        let den = swma([s[0].1, s[1].1, s[2].1, s[3].1]);
        self.weighted.push_back((num, den));
        if self.weighted.len() > self.period {
            self.weighted.pop_front();
        }
        if self.weighted.len() < self.period {
            return None;
        }
        // Summed afresh each bar: a running sum would leave rounding residue where it should be 0.
        let (num_sum, den_sum) = self
            .weighted
            .iter()
            .fold((0.0, 0.0), |(n, d), &(a, b)| (n + a, d + b));
        let rvi_val = if den_sum == 0.0 {
            0.0
        } else {
            num_sum / den_sum
        };

        self.rvis.push_back(rvi_val);
        if self.rvis.len() > 4 {
            self.rvis.pop_front();
        }
        if self.rvis.len() < 4 {
            return None;
        }
        let r = &self.rvis;
        let sig_val = swma([r[0], r[1], r[2], r[3]]);

        let mut extra = HashMap::new();
        extra.insert("rvi".to_string(), rvi_val);
        extra.insert("signal".to_string(), sig_val);

        Some(IndicatorOutput::with_extra(rvi_val, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const RVI_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "rvi",
        aliases: &[],
        description: "Relative Vigor Index",
        params: &[super::definition::ParamSpec::count("period", 10, 1, 10000)
            .meaning("bars summed in numerator and denominator")],
        checks: &[],
        build: |p| Ok(Box::new(RviEngine::new(p.count("period")))),
    };

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rvi_basic() {
        let mut rvi = RviEngine::new(10);
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 104.0, 1000.0);
            out = rvi.on_bar(&b);
        }
        assert!(out.is_some());
    }

    /// Ehlers' construction: the 1-2-2-1 weighted close-open and high-low over the last four bars,
    /// summed over `period`, numerator over denominator; the signal is the same weighting of the
    /// RVI. Asymmetric bars, so a plain average of per-bar ratios gives a different number.
    #[test]
    fn rvi_is_the_ratio_of_weighted_sums() {
        let bars = [
            (10.0, 11.0, 9.0, 10.8),
            (10.8, 13.0, 10.5, 12.9),
            (12.9, 13.1, 12.0, 12.2),
            (12.2, 12.4, 11.9, 12.3),
            (12.3, 14.0, 12.1, 13.8),
            (13.8, 13.9, 13.0, 13.1),
            (13.1, 13.5, 12.4, 12.5),
            (12.5, 14.5, 12.5, 14.4),
        ];
        let swma = |x: &[f64]| (x[0] + 2.0 * x[1] + 2.0 * x[2] + x[3]) / 6.0;
        let num: Vec<f64> = bars.iter().map(|b| b.3 - b.0).collect();
        let den: Vec<f64> = bars.iter().map(|b| b.1 - b.2).collect();
        let period = 2;
        let n: Vec<f64> = num.windows(4).map(swma).collect();
        let d: Vec<f64> = den.windows(4).map(swma).collect();
        let rvi: Vec<f64> = n
            .windows(period)
            .zip(d.windows(period))
            .map(|(a, b)| a.iter().sum::<f64>() / b.iter().sum::<f64>())
            .collect();
        let expected_line = *rvi.last().unwrap();
        let expected_signal = swma(&rvi[rvi.len() - 4..]);

        let mut engine = RviEngine::new(period);
        assert_eq!(engine.warmup_period(), period + 6);
        let outs: Vec<_> = bars
            .iter()
            .enumerate()
            .map(|(i, b)| engine.on_bar(&Bar::new(i as i64, b.0, b.1, b.2, b.3, 1.0)))
            .collect();
        assert_eq!(outs.iter().position(Option::is_some), Some(period + 5));
        let last = outs.last().unwrap().as_ref().unwrap();
        assert!((last.value - expected_line).abs() < 1e-12);
        assert!((last.extra["signal"] - expected_signal).abs() < 1e-12);
    }
}
