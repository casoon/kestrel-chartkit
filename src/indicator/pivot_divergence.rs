use super::cci::Cci;
use super::divergence::{DivergenceKind, OscillatorAnchor, PivotDivergenceEngine};
use super::rsi::Rsi;
use super::stoch_rsi::StochRsi;
use super::wavetrend::WaveTrendEngine;
use super::williams_r::WilliamsR;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// The oscillator a [`PivotDivergence`] compares price pivots against.
///
/// Each one in its plain, unsmoothed form (smoothing length 1), with its usual lengths:
/// RSI 14, WaveTrend 10/21 (`wt1`), Stochastic RSI 14/14/3 (`%K`), Williams %R 14 (ascending
/// 0..100 as in [`WilliamsR`]), CCI 20.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DivergenceOscillator {
    Rsi,
    WaveTrend,
    StochRsi,
    WilliamsR,
    Cci,
}

impl DivergenceOscillator {
    /// From a numeric code — for callers whose parameters are plain numbers:
    /// 0 RSI, 1 WaveTrend, 2 Stochastic RSI, 3 Williams %R, 4 CCI.
    pub fn from_code(code: u32) -> Option<Self> {
        Some(match code {
            0 => Self::Rsi,
            1 => Self::WaveTrend,
            2 => Self::StochRsi,
            3 => Self::WilliamsR,
            4 => Self::Cci,
            _ => return None,
        })
    }

    fn build(self) -> Box<dyn Indicator> {
        match self {
            Self::Rsi => Box::new(Rsi::new(14, 1, 3, 50.0, 70.0, 30.0, 5, true, 100, 4, 10.0)),
            Self::WaveTrend => Box::new(WaveTrendEngine::with_defaults()),
            Self::StochRsi => Box::new(StochRsi::new(
                14, 14, 3, 3, 50.0, 80.0, 20.0, 5, true, 50, 50, 4, 10.0,
            )),
            Self::WilliamsR => Box::new(WilliamsR::new(
                14, 1, 3, 50.0, 80.0, 20.0, 5, true, 50, 4, 10.0,
            )),
            Self::Cci => Box::new(Cci::new(20, 1, 3, 5, -100.0, 100.0, true, 100, 4, 25.0)),
        }
    }
}

/// Price-pivot-anchored divergence on a selectable oscillator.
///
/// Feeds the oscillator's main value into a [`PivotDivergenceEngine`]: a strict price pivot
/// (`left`/`right` bars on each side) is compared against up to `max_prior_pivots` earlier pivots
/// of the same type within `[min_distance, max_distance]` bars, the oscillator read exactly at the
/// pivot bar ([`OscillatorAnchor::AtPivot`]). The best-scoring candidate per side is reported:
///
/// | price | oscillator | alert |
/// |---|---|---|
/// | lower low | higher low | `bull_divergence` |
/// | higher high | lower high | `bear_divergence` |
/// | higher low | lower low | `bull_hidden_divergence` |
/// | lower high | higher high | `bear_hidden_divergence` |
///
/// Alerts fire on the **confirmation** bar, `right` bars after the pivot — never earlier, so the
/// series stays free of lookahead. Alert strength is the engine's quality score (0..1).
///
/// `value` is the oscillator. On a bar that reports a divergence, `extra` describes it — enough
/// to draw the line between the two price pivots; otherwise these keys are absent:
///
/// - `pivot_price`: the pivot's low (bullish) or high (bearish);
/// - `previous_price`: the same for the earlier pivot it is compared with;
/// - `pivot_age`: bars from this (confirmation) bar back to the pivot (`right`);
/// - `bars_between`: bars from the earlier pivot to the pivot;
/// - `direction`: `1` bullish, `-1` bearish.
///
/// Should one bar report several divergences, `extra` describes the last of them.
pub struct PivotDivergence {
    oscillator: DivergenceOscillator,
    inner: Box<dyn Indicator>,
    engine: PivotDivergenceEngine,
    left: usize,
    right: usize,
    alerts: Vec<IndicatorAlert>,
}

impl PivotDivergence {
    pub fn new(
        oscillator: DivergenceOscillator,
        left: usize,
        right: usize,
        min_distance: usize,
        max_distance: usize,
        max_prior_pivots: usize,
    ) -> Self {
        Self {
            oscillator,
            inner: oscillator.build(),
            engine: Self::engine(left, right, min_distance, max_distance, max_prior_pivots),
            left: left.max(1),
            right: right.max(1),
            alerts: Vec::new(),
        }
    }

    fn engine(
        left: usize,
        right: usize,
        min_distance: usize,
        max_distance: usize,
        max_prior_pivots: usize,
    ) -> PivotDivergenceEngine {
        PivotDivergenceEngine::with_confirmation(left, right, min_distance, max_distance)
            .with_max_prior_pivots(max_prior_pivots)
            .with_oscillator_anchor(OscillatorAnchor::AtPivot)
    }

    pub fn oscillator(&self) -> DivergenceOscillator {
        self.oscillator
    }
}

impl Indicator for PivotDivergence {
    fn name(&self) -> &str {
        "pivot_divergence"
    }

    fn warmup_period(&self) -> usize {
        self.inner.warmup_period() + self.left + self.right
    }

    fn reset(&mut self) {
        self.inner.reset();
        self.engine.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let osc = self.inner.on_bar(bar)?.value;
        let mut out = IndicatorOutput::new(osc);
        for event in self.engine.update(bar, osc) {
            let (kind, note) = match event.kind {
                DivergenceKind::RegularBullish => ("bull_divergence", "Bullish divergence"),
                DivergenceKind::RegularBearish => ("bear_divergence", "Bearish divergence"),
                DivergenceKind::HiddenBullish => {
                    ("bull_hidden_divergence", "Hidden bullish divergence")
                }
                DivergenceKind::HiddenBearish => {
                    ("bear_hidden_divergence", "Hidden bearish divergence")
                }
            };
            let bullish = matches!(
                event.kind,
                DivergenceKind::RegularBullish | DivergenceKind::HiddenBullish
            );
            out.extra
                .insert("pivot_price".to_string(), event.pivot_price);
            out.extra
                .insert("previous_price".to_string(), event.previous_price);
            out.extra.insert("pivot_age".to_string(), self.right as f64);
            out.extra
                .insert("bars_between".to_string(), event.bars_between as f64);
            out.extra
                .insert("direction".to_string(), if bullish { 1.0 } else { -1.0 });
            self.alerts
                .push(IndicatorAlert::new(kind, note, event.quality_score));
        }
        Some(out)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip_and_unknown_is_none() {
        assert_eq!(
            DivergenceOscillator::from_code(1),
            Some(DivergenceOscillator::WaveTrend)
        );
        assert_eq!(DivergenceOscillator::from_code(5), None);
    }

    /// The extras of a reported divergence point at the two price pivots: `pivot_age` bars
    /// back lies the bar whose low/high is `pivot_price`, `bars_between` before that the one
    /// with `previous_price`.
    #[test]
    fn extras_locate_both_pivots() {
        // Two overlaid waves (unequal periods) produce lower lows with higher oscillator lows
        // and the reverse — both kinds of divergence.
        let bars: Vec<Bar> = (0..600)
            .map(|i| {
                let x = i as f64;
                let mid = 100.0 + (x / 9.0).sin() * 6.0 + (x / 47.0).cos() * 5.0 - x * 0.01;
                Bar::new(
                    1_700_000_000 + i * 900,
                    mid,
                    mid + 0.8,
                    mid - 0.9,
                    mid + 0.1,
                    0.0,
                )
            })
            .collect();
        let mut div = PivotDivergence::new(DivergenceOscillator::Rsi, 5, 5, 8, 80, 5);
        let mut seen = 0;
        for (i, bar) in bars.iter().enumerate() {
            let Some(out) = div.on_bar(bar) else {
                continue;
            };
            let Some(&pivot_price) = out.extra.get("pivot_price") else {
                continue;
            };
            seen += 1;
            let pivot = i - out.extra["pivot_age"] as usize;
            let previous = pivot - out.extra["bars_between"] as usize;
            let bullish = out.extra["direction"] > 0.0;
            let at = |j: usize| if bullish { bars[j].low } else { bars[j].high };
            assert_eq!(pivot_price, at(pivot), "pivot at bar {pivot}");
            assert_eq!(
                out.extra["previous_price"],
                at(previous),
                "previous pivot at bar {previous}"
            );
            assert_eq!(div.alerts()[0].kind.starts_with("bull"), bullish);
        }
        assert!(seen > 2, "the waves must produce divergences: {seen}");
    }
}
