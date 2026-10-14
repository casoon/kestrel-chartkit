use std::collections::HashMap;

use crate::indicator::smoothing::{ExtremeWindow, Sma};
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Williams VIX Fix: how far the low sits below the recent highest close, in percent.
///
/// `WVF = (highest_close - low) / highest_close * 100`, `highest_close` over the last `pd` closes —
/// the current close until `pd` closes have been seen. The spike band is
/// `SMA(bband_len)(WVF) + mult * sd`, `sd` the population deviation of the last `bband_len` WVF
/// values around that SMA; a WVF at or above it raises an alert.
///
/// The original only looks for panic lows. Its mirror for tops — the inverse VIX Fix — runs
/// alongside: `inverse = (high - lowest_close) / high * 100`, `lowest_close` over the same `pd`
/// closes, with its own band of the same form; at or above it raises `blowoff_top`. Divided by the
/// high (not by the lowest close) it is the exact mirror of the WVF: on a series mirrored in log
/// price (`p' = C / p`) `inverse` equals the WVF of the original, so tops and lows are found alike.
///
/// `value`: the WVF; `secondary`: the upper band; `extra["inverse"]`, `extra["inverse_band"]`.
/// First output: once the band exists, i.e. with the `2 * bband_len - 1`-th bar.
/// [`Indicator::reset`] clears all windows.
pub struct WilliamsVixFix {
    bband_len: usize,
    mult: f64,

    close_window: ExtremeWindow,
    wvf_window: Vec<f64>,
    sma: Sma,
    inv_window: Vec<f64>,
    inv_sma: Sma,
    alerts: Vec<IndicatorAlert>,
}

/// `mean + mult * sd` of `window` (population deviation around `mean`).
fn band(window: &[f64], mean: f64, mult: f64) -> f64 {
    let variance = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / window.len() as f64;
    mean + mult * variance.sqrt()
}

impl WilliamsVixFix {
    pub fn new(pd: usize, bband_len: usize, mult: f64) -> Self {
        Self {
            bband_len,
            mult,
            close_window: ExtremeWindow::new(pd),
            wvf_window: Vec::with_capacity(bband_len),
            sma: Sma::new(bband_len),
            inv_window: Vec::with_capacity(bband_len),
            inv_sma: Sma::new(bband_len),
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
        self.inv_window.clear();
        self.inv_sma.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let (lowest_close, highest_close) = self
            .close_window
            .push(bar.close)
            .unwrap_or((bar.close, bar.close));
        self.alerts.clear();

        let wvf = if highest_close > 0.0 {
            ((highest_close - bar.low) / highest_close) * 100.0
        } else {
            0.0
        };
        let inverse = if bar.high > 0.0 {
            ((bar.high - lowest_close) / bar.high) * 100.0
        } else {
            0.0
        };

        let sma_val = self.sma.update(wvf);
        let inv_sma = self.inv_sma.update(inverse);
        let (Some(mean), Some(inv_mean)) = (sma_val, inv_sma) else {
            return None;
        };

        for (w, v) in [(&mut self.wvf_window, wvf), (&mut self.inv_window, inverse)] {
            w.push(v);
            if w.len() > self.bband_len {
                w.remove(0);
            }
        }
        if self.wvf_window.len() < self.bband_len {
            return None;
        }

        let upper_band = band(&self.wvf_window, mean, self.mult);
        let inverse_band = band(&self.inv_window, inv_mean, self.mult);

        if wvf >= upper_band {
            self.alerts.push(IndicatorAlert::new(
                "panic_bottom",
                format!(
                    "Williams VIX Fix Spike ({:.2} >= Upper Band {:.2}) - Market Bottom Zone",
                    wvf, upper_band
                ),
                1.0,
            ));
        }
        if inverse >= inverse_band {
            self.alerts.push(IndicatorAlert::new(
                "blowoff_top",
                format!(
                    "Inverse VIX Fix Spike ({:.2} >= Upper Band {:.2}) - Market Top Zone",
                    inverse, inverse_band
                ),
                1.0,
            ));
        }

        let mut extra = HashMap::new();
        extra.insert("inverse".to_string(), inverse);
        extra.insert("inverse_band".to_string(), inverse_band);
        Some(IndicatorOutput::with_extra(wvf, extra).with_secondary(upper_band))
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

pub const VIX_FIX_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "vix_fix",
        aliases: &["wvf"],
        description: "Williams Vix Fix volatility spike detector: panic lows (WVF, alert panic_bottom) and, mirrored, blow-off tops (inverse VIX Fix, extra inverse/inverse_band, alert blowoff_top)",
        params: &[
            super::definition::ParamSpec::count("pd", 22, 1, 10000).meaning("bars in the highest-close (and lowest-close) lookback"),
            super::definition::ParamSpec::count("bband_len", 20, 1, 10000).meaning("bars in the Bollinger band over the Vix Fix"),
            super::definition::ParamSpec::number("mult", 2.0, 0.01, 100.0).meaning("standard deviations of the upper band"),
        ],
        checks: &[],
        build: |p| Ok(Box::new(WilliamsVixFix::new(p.count("pd"), p.count("bband_len"), p.number("mult")))),
    };

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Bar;

    /// On a series mirrored in log price the inverse is the WVF of the original, and
    /// `blowoff_top` fires exactly where `panic_bottom` fired.
    #[test]
    fn inverse_is_the_mirror_of_the_wvf() {
        let bars: Vec<Bar> = (0..200)
            .map(|i| {
                let c = 100.0 + 10.0 * (i as f64 * 0.21).sin() + 0.05 * i as f64;
                let o = c - 0.4 * (i as f64 * 0.7).cos();
                let h = c.max(o) + 0.3 + if i % 37 == 5 { 4.0 } else { 0.0 };
                let l = c.min(o) - 0.5 - if i % 23 == 11 { 6.0 } else { 0.0 };
                Bar::new(i, o, h, l, c, 0.0)
            })
            .collect();
        let k = 1.0e4;
        let mirrored: Vec<Bar> = bars
            .iter()
            .map(|b| {
                Bar::new(
                    b.timestamp,
                    k / b.open,
                    k / b.low,
                    k / b.high,
                    k / b.close,
                    0.0,
                )
            })
            .collect();
        let (mut a, mut b) = (
            WilliamsVixFix::new(22, 20, 2.0),
            WilliamsVixFix::new(22, 20, 2.0),
        );
        let mut bottoms = 0;
        for (x, y) in bars.iter().zip(&mirrored) {
            let (ox, oy) = (a.on_bar(x), b.on_bar(y));
            if let (Some(ox), Some(oy)) = (ox, oy) {
                assert!((ox.value - oy.extra["inverse"]).abs() < 1e-9);
                assert!((ox.secondary.unwrap() - oy.extra["inverse_band"]).abs() < 1e-9);
            }
            let ka: Vec<String> = a.alerts().into_iter().map(|x| x.kind).collect();
            let kb: Vec<String> = b.alerts().into_iter().map(|x| x.kind).collect();
            assert_eq!(
                ka.contains(&"panic_bottom".into()),
                kb.contains(&"blowoff_top".into())
            );
            assert_eq!(
                ka.contains(&"blowoff_top".into()),
                kb.contains(&"panic_bottom".into())
            );
            bottoms += usize::from(ka.contains(&"panic_bottom".into()));
        }
        assert!(bottoms > 0, "the series must contain panic lows");
    }

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
