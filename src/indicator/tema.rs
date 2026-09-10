use super::smoothing::Ema;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Triple Exponential Moving Average (TEMA) over the closing price.
///
/// `TEMA = 3 * e1 - 3 * e2 + e3` over three chained exponential averages of `period` — `e1` over
/// the close, `e2` over `e1`, `e3` over `e2` — each the shared [`Ema`] with its first-sample
/// seed. All three run from the first close; only the output is withheld until the `period`-th
/// bar.
///
/// That chaining differs from [`super::moving_averages::DemaEngine`], whose second stage starts
/// with the first *published* value of the first. The two produce different early values and
/// converge as the seeds decay.
///
/// First output: with the `period`-th bar. [`Indicator::reset`] clears all three averages.
#[derive(Debug, Clone)]
pub struct TemaEngine {
    period: usize,
    ema1: Ema,
    ema2: Ema,
    ema3: Ema,
    count: usize,
}

impl TemaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period: period.max(1),
            ema1: Ema::new(period),
            ema2: Ema::new(period),
            ema3: Ema::new(period),
            count: 0,
        }
    }
}

impl Indicator for TemaEngine {
    fn name(&self) -> &str {
        "tema"
    }

    fn warmup_period(&self) -> usize {
        self.period * 3
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.ema3.reset();
        self.count = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let e1 = self.ema1.update(bar.close)?;
        let e2 = self.ema2.update(e1)?;
        let e3 = self.ema3.update(e2)?;

        if self.count < self.period {
            return None;
        }

        let tema_val = 3.0 * e1 - 3.0 * e2 + e3;
        Some(IndicatorOutput::new(tema_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tema_basic() {
        let mut tema = TemaEngine::new(5);
        let mut out = None;
        for i in 0..20 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = tema.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
