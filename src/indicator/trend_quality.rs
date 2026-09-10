use super::adx::Adx;
use super::efficiency::LegEfficiencyEngine;
use super::moving_averages::EmaEngine;
use super::volume_indicators::RvolEngine;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

/// Trend Quality Score: a signed product of four trend factors, `-100..=100`.
///
/// All four inputs run on every bar with the same `period`:
///
/// - `direction`: `+1` if the EMA(`period`) of the close (first-sample seeded, see
///   [`EmaEngine`]) rose against the previous bar's, `ema > prev`; otherwise `-1`. A flat EMA
///   counts as `-1`, and so does the first output, which has no previous value to compare with.
/// - `efficiency`: the Kaufman Efficiency Ratio over `period` ([`LegEfficiencyEngine`]), `0..=1`.
/// - `strength`: `clamp(ADX / 50, 0, 1)`, the ADX from [`Adx::with_period`]`(period)`: Wilder DI
///   and ADX smoothing both over `period`.
/// - `participation`: `clamp(RVOL / 2, 0.2, 1)` with [`RvolEngine`]`(period)`.
///
/// `value = clamp(direction · efficiency · strength · participation · 100, -100, 100)`, the four
/// factors in `extra` under their names. There is no separate persistence factor.
///
/// First output once all four inputs have one, which the ADX sets: with bar `2 · period`.
#[derive(Debug, Clone)]
pub struct TrendQualityScoreEngine {
    ema: EmaEngine,
    efficiency: LegEfficiencyEngine,
    adx: Adx,
    rvol: RvolEngine,
    prev_ema: Option<f64>,
}

impl TrendQualityScoreEngine {
    pub fn new(period: usize) -> Self {
        Self {
            ema: EmaEngine::new(period),
            efficiency: LegEfficiencyEngine::new(period),
            adx: Adx::with_period(period),
            rvol: RvolEngine::new(period),
            prev_ema: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(14)
    }
}

impl Indicator for TrendQualityScoreEngine {
    fn name(&self) -> &str {
        "trend_quality"
    }

    fn warmup_period(&self) -> usize {
        self.ema
            .warmup_period()
            .max(self.efficiency.warmup_period())
            .max(self.adx.warmup_period())
            .max(self.rvol.warmup_period())
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.efficiency.reset();
        self.adx.reset();
        self.rvol.reset();
        self.prev_ema = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let ema_out = self.ema.on_bar(bar);
        let efficiency_out = self.efficiency.on_bar(bar);
        let adx_out = self.adx.on_bar(bar);
        let rvol_out = self.rvol.on_bar(bar);
        let (Some(ema_out), Some(efficiency_out), Some(adx_out), Some(rvol_out)) =
            (ema_out, efficiency_out, adx_out, rvol_out)
        else {
            return None;
        };
        let ema_val = ema_out.value;
        let eff_val = efficiency_out.value;
        let rvol_val = rvol_out.value;

        let slope = match self.prev_ema {
            Some(prev) => (ema_val - prev) / prev.max(1e-8),
            None => 0.0,
        };
        self.prev_ema = Some(ema_val);

        let direction = if slope > 0.0 { 1.0 } else { -1.0 };
        let strength = (adx_out.value / 50.0).clamp(0.0, 1.0); // ADX normalized
        let participation = (rvol_val / 2.0).clamp(0.2, 1.0); // RVOL normalized

        let raw_score = direction * eff_val * strength * participation * 100.0;
        let quality_score = raw_score.clamp(-100.0, 100.0);

        let mut extra = HashMap::new();
        extra.insert("direction".to_string(), direction);
        extra.insert("efficiency".to_string(), eff_val);
        extra.insert("strength".to_string(), strength);
        extra.insert("participation".to_string(), participation);

        Some(IndicatorOutput::with_extra(quality_score, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trend_quality_score() {
        let mut tq = TrendQualityScoreEngine::with_defaults();
        let mut out = None;
        for i in 0..100 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + i as f64, 1000.0);
            out = tq.on_bar(&b);
        }
        assert!(out.is_some());
    }
}
