use std::collections::HashMap;
use std::collections::VecDeque;

use crate::model::Bar;

use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Rolling Volume Weighted Average Price with standard-deviation bands and slope.
///
/// Note: this is a rolling VWAP over `window` bars, not a session-anchored VWAP — the
/// `Bar` model carries no session-boundary marker, so a true session-reset VWAP needs to be
/// driven by the consumer (e.g. calling `reset()` on session open). See plan Anhang A,
/// "Designfrage für die spätere Umsetzung: Session-Fenster als Parameter".
pub struct Vwap {
    window: usize,
    slope_lookback: usize,
    prices_x_volume: VecDeque<f64>,
    volumes: VecDeque<f64>,
    vwap_history: VecDeque<f64>,
}

impl Vwap {
    pub fn new(window: usize, slope_lookback: usize) -> Self {
        Self {
            window,
            slope_lookback,
            prices_x_volume: VecDeque::new(),
            volumes: VecDeque::new(),
            vwap_history: VecDeque::new(),
        }
    }

    /// ~one RTH session at 1-minute bars.
    pub fn with_defaults() -> Self {
        Self::new(390, 20)
    }
}

impl Indicator for Vwap {
    fn name(&self) -> &str {
        "vwap"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.prices_x_volume.clear();
        self.volumes.clear();
        self.vwap_history.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let typical = bar.typical_price();
        self.prices_x_volume.push_back(typical * bar.volume);
        self.volumes.push_back(bar.volume);
        if self.prices_x_volume.len() > self.window {
            self.prices_x_volume.pop_front();
            self.volumes.pop_front();
        }

        let vol_sum: f64 = self.volumes.iter().sum();
        if vol_sum <= 0.0 {
            return None;
        }
        let pv_sum: f64 = self.prices_x_volume.iter().sum();
        let vwap = pv_sum / vol_sum;

        // Volume-weighted variance for sigma bands (plan Anhang A: Z_VWAP = (Price-VWAP)/sigma).
        let mut weighted_sq_dev = 0.0;
        for (pv, vol) in self.prices_x_volume.iter().zip(self.volumes.iter()) {
            if *vol <= 0.0 {
                continue;
            }
            let price = pv / vol;
            weighted_sq_dev += vol * (price - vwap).powi(2);
        }
        let sigma = (weighted_sq_dev / vol_sum).sqrt();

        self.vwap_history.push_back(vwap);
        if self.vwap_history.len() > self.slope_lookback + 1 {
            self.vwap_history.pop_front();
        }

        let mut extra = HashMap::new();
        extra.insert("sigma".to_string(), sigma);
        extra.insert("upper_1sigma".to_string(), vwap + sigma);
        extra.insert("lower_1sigma".to_string(), vwap - sigma);
        extra.insert("upper_2sigma".to_string(), vwap + 2.0 * sigma);
        extra.insert("lower_2sigma".to_string(), vwap - 2.0 * sigma);

        if self.vwap_history.len() > self.slope_lookback {
            let past = self.vwap_history[0];
            let slope = (vwap - past) / self.slope_lookback as f64;
            extra.insert("slope".to_string(), slope);
        }

        Some(IndicatorOutput::with_extra(vwap, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub fn build_vwap(params: &HashMap<String, f64>) -> Vwap {
    let window = params.get("window").copied().unwrap_or(390.0) as usize;
    let slope_lookback = params.get("slope_lookback").copied().unwrap_or(20.0) as usize;
    Vwap::new(window, slope_lookback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, close: f64, volume: f64) -> Bar {
        Bar::new(t, close, close, close, close, volume)
    }

    #[test]
    fn flat_price_series_has_vwap_equal_to_price() {
        let mut vwap = Vwap::new(10, 5);
        let mut last = None;
        for i in 0..10 {
            last = vwap.on_bar(&bar(i, 100.0, 10.0));
        }
        let out = last.expect("expected output once volume accumulated");
        assert!((out.value - 100.0).abs() < 1e-9);
        assert!((out.extra["sigma"]).abs() < 1e-9);
    }

    #[test]
    fn rising_prices_produce_positive_slope() {
        let mut vwap = Vwap::new(50, 5);
        let mut last = None;
        for i in 0..20 {
            let price = 100.0 + i as f64;
            last = vwap.on_bar(&bar(i, price, 10.0));
        }
        let out = last.expect("expected output");
        assert!(out.extra["slope"] > 0.0);
    }
}
