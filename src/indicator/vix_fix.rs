use std::collections::HashMap;

use crate::indicator::smoothing::{ExtremeWindow, Sma};
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Williams VIX Fix Advanced indicator.
/// Measures market synthetic fear/volatility spikes to identify market bottoms.
pub struct WilliamsVixFix {
    bband_len: usize,
    mult: f64,

    close_window: ExtremeWindow,
    wvf_window: Vec<f64>,
    sma: Sma,
    alerts: Vec<IndicatorAlert>,
}

impl WilliamsVixFix {
    pub fn new(pd: usize, bband_len: usize, mult: f64) -> Self {
        Self {
            bband_len,
            mult,
            close_window: ExtremeWindow::new(pd),
            wvf_window: Vec::with_capacity(bband_len),
            sma: Sma::new(bband_len),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for WilliamsVixFix {
    fn name(&self) -> &str {
        "vix_fix"
    }

    fn warmup_period(&self) -> usize {
        // Two nested `bband_len`-sized windows gate the first output: `self.sma` must fill
        // first (bband_len calls), and only once it does does `wvf_window` start accumulating
        // towards its own `bband_len` (see the two gates in `on_bar`) -- so the first non-`None`
        // output only arrives after roughly twice `bband_len` bars, not after one.
        self.bband_len.saturating_mul(2).saturating_sub(1)
    }

    fn reset(&mut self) {
        self.close_window.reset();
        self.wvf_window.clear();
        self.sma.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let highest_close = self
            .close_window
            .push(bar.close)
            .map(|(_, high)| high)
            .unwrap_or(bar.close);
        self.alerts.clear();

        let wvf = if highest_close > 0.0 {
            ((highest_close - bar.low) / highest_close) * 100.0
        } else {
            0.0
        };

        let sma_val = self.sma.update(wvf)?;

        self.wvf_window.push(wvf);
        if self.wvf_window.len() > self.bband_len {
            self.wvf_window.remove(0);
        }

        if self.wvf_window.len() < self.bband_len {
            return None;
        }

        let mean = sma_val;
        let variance = self
            .wvf_window
            .iter()
            .map(|v| (v - mean).powi(2))
            .sum::<f64>()
            / self.bband_len as f64;
        let std_dev = variance.sqrt();

        let upper_band = mean + self.mult * std_dev;
        let is_spike = wvf >= upper_band;

        if is_spike {
            self.alerts.push(IndicatorAlert::new(
                "panic_bottom",
                format!(
                    "Williams VIX Fix Spike ({:.2} >= Upper Band {:.2}) - Market Bottom Zone",
                    wvf, upper_band
                ),
                1.0,
            ));
        }

        Some(IndicatorOutput::new(wvf).with_secondary(upper_band))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_vix_fix(params: &HashMap<String, f64>) -> WilliamsVixFix {
    let pd = params.get("pd").copied().unwrap_or(22.0) as usize;
    let bband_len = params.get("bband_len").copied().unwrap_or(20.0) as usize;
    let mult = params.get("mult").copied().unwrap_or(2.0);
    WilliamsVixFix::new(pd, bband_len, mult)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;

    /// `warmup_period()` previously defaulted to 0 (never overridden), which was silently wrong:
    /// two nested `bband_len`-sized windows (`sma`, then `wvf_window`) must each fill before the
    /// first output, so the real warmup is close to `2 * bband_len`. This was only caught once
    /// `vix_fix` became reachable from `catalog()` (finding 04) and the generic warmup-contract
    /// robustness test exercised it.
    #[test]
    fn test_warmup_period_matches_first_non_none_output() {
        let bband_len = 5;
        let mut ind = WilliamsVixFix::new(3, bband_len, 2.0);
        let declared_warmup = ind.warmup_period();

        let mut first_some_index = None;
        for i in 0..40 {
            let price = 100.0 + (i as f64 * 0.37).sin() * 3.0;
            let bar = Bar::new(i, price + 1.0, price + 1.5, price - 1.5, price, 100.0);
            if ind.on_bar(&bar).is_some() && first_some_index.is_none() {
                first_some_index = Some(i as usize);
            }
        }

        let first_some_index = first_some_index.expect("expected an output within 40 bars");
        assert!(
            declared_warmup >= first_some_index,
            "declared warmup_period {declared_warmup} understates the actual first output at bar {first_some_index}"
        );
        // The declared warmup should be a tight bound, not a wildly conservative one.
        assert!(declared_warmup - first_some_index <= 1);
    }
}
