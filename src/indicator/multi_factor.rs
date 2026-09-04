use super::buy_sell_pressure::BuySellPressureEstimator;
use super::rsi::Rsi;
use super::trend_quality::TrendQualityScoreEngine;
use super::volatility_regime::VolatilityRegimeDetector;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Composite Multi-Factor Market Score Engine (-1.0 .. +1.0).
/// Combines Trend Quality, Momentum (RSI), Volume/Pressure, and Volatility Regime.
#[derive(Debug, Clone)]
pub struct MultiFactorMarketScore {
    trend: TrendQualityScoreEngine,
    rsi: Rsi,
    pressure: BuySellPressureEstimator,
    volatility: VolatilityRegimeDetector,
}

impl MultiFactorMarketScore {
    pub fn new(period: usize) -> Self {
        Self {
            trend: TrendQualityScoreEngine::new(period),
            rsi: Rsi::with_period(period),
            pressure: BuySellPressureEstimator::new(period),
            volatility: VolatilityRegimeDetector::new(period, 2.0, 1.5),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14)
    }
}

impl Indicator for MultiFactorMarketScore {
    fn name(&self) -> &str {
        "multi_factor"
    }

    fn warmup_period(&self) -> usize {
        self.trend
            .warmup_period()
            .max(self.rsi.warmup_period())
            .max(self.pressure.warmup_period())
            .max(self.volatility.warmup_period())
    }

    fn reset(&mut self) {
        self.trend.reset();
        self.rsi.reset();
        self.pressure.reset();
        self.volatility.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let trend_out = self.trend.on_bar(bar);
        let rsi_out = self.rsi.on_bar(bar);
        let pressure_out = self.pressure.on_bar(bar);
        let vol_out = self.volatility.on_bar(bar);
        let (Some(trend_out), Some(rsi_out), Some(pressure_out), Some(vol_out)) =
            (trend_out, rsi_out, pressure_out, vol_out)
        else {
            return None;
        };
        let trend_score = trend_out.value / 100.0;
        let rsi_norm = (rsi_out.value - 50.0) / 50.0;
        let pressure_score = pressure_out.value / 100.0;

        let vol_state = vol_out.value; // -1 = Squeeze, 0 = Normal, 1 = Expansion

        // Weighted factor combination
        let raw_composite = trend_score * 0.35 + rsi_norm * 0.25 + pressure_score * 0.40;

        // Dampen composite during Squeeze
        let final_score = if vol_state < 0.0 {
            raw_composite * 0.5
        } else {
            raw_composite
        }
        .clamp(-1.0, 1.0);

        let mut extra = HashMap::new();
        extra.insert("trend_factor".to_string(), trend_score);
        extra.insert("rsi_factor".to_string(), rsi_norm);
        extra.insert("pressure_factor".to_string(), pressure_score);
        extra.insert("volatility_factor".to_string(), vol_state);

        Some(IndicatorOutput::with_extra(final_score, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_factor_market_score() {
        let mut mf = MultiFactorMarketScore::with_defaults();
        let mut out = None;
        for i in 0..150 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = mf.on_bar(&b);
        }
        assert!(out.is_some());
        let val = out.unwrap().value;
        assert!((-1.0..=1.0).contains(&val));
    }
}
