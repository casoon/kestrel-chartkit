use std::collections::VecDeque;

use crate::indicator::atr::TrueRangeSmoothing;
use crate::indicator::definition::{IndicatorDefinition, ParamSpec};
use crate::indicator::kernels::AtrKernel;
use crate::indicator::vwap::Vwap;
use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum SlopeState {
    StronglyFalling,
    ModeratelyFalling,
    Flat,
    ModeratelyRising,
    StronglyRising,
}

/// VWAP Regime Engine output (plan Anhang A, "Ergänzung (zehntes Video, VWAP)"): turns VWAP
/// from a simple over/under line into a regime signal via slope, normalized distance, and
/// how often/how persistently price sits on one side.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VwapRegimeOutput {
    pub slope_atr: f64,
    pub slope_state: SlopeState,
    pub distance_atr: f64,
    pub z_score: f64,
    /// Crosses per bar over the tracked window, 0.0..1.0 — high = Balance, low = Trend.
    pub cross_frequency: f64,
    /// Share of bars on the current side of VWAP, 0.0..1.0.
    pub price_persistence: f64,
}

pub fn classify_slope(slope_atr: f64, flat_threshold: f64, strong_threshold: f64) -> SlopeState {
    if slope_atr >= strong_threshold {
        SlopeState::StronglyRising
    } else if slope_atr >= flat_threshold {
        SlopeState::ModeratelyRising
    } else if slope_atr <= -strong_threshold {
        SlopeState::StronglyFalling
    } else if slope_atr <= -flat_threshold {
        SlopeState::ModeratelyFalling
    } else {
        SlopeState::Flat
    }
}

/// Tracks cross-frequency and price-persistence relative to VWAP over a rolling window — feed
/// it `price - vwap` each bar (e.g. from `indicator::vwap::Vwap`'s output).
pub struct VwapRegimeTracker {
    window: usize,
    diffs: VecDeque<f64>,
}

impl VwapRegimeTracker {
    pub fn new(window: usize) -> Self {
        Self {
            window,
            diffs: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.diffs.clear();
    }

    /// [`Self::update`] from the raw VWAP slope per bar: `slope_atr = slope / atr`, `0` for an ATR
    /// of zero. The one place that turns a price slope into ATRs, for the catalog entry and for
    /// [`crate::engine::pipeline::update_vwap_regime`] alike.
    pub(crate) fn update_with_slope(
        &mut self,
        price_minus_vwap: f64,
        atr: f64,
        sigma: f64,
        slope: f64,
        flat_threshold: f64,
        strong_threshold: f64,
    ) -> VwapRegimeOutput {
        let slope_atr = if atr > 0.0 { slope / atr } else { 0.0 };
        self.update(
            price_minus_vwap,
            atr,
            sigma,
            slope_atr,
            flat_threshold,
            strong_threshold,
        )
    }

    #[allow(clippy::too_many_arguments)]
    pub fn update(
        &mut self,
        price_minus_vwap: f64,
        atr: f64,
        sigma: f64,
        slope_atr: f64,
        flat_threshold: f64,
        strong_threshold: f64,
    ) -> VwapRegimeOutput {
        self.diffs.push_back(price_minus_vwap);
        if self.diffs.len() > self.window {
            self.diffs.pop_front();
        }

        let mut crosses = 0usize;
        for pair in self.diffs.iter().collect::<Vec<_>>().windows(2) {
            if (*pair[0] >= 0.0) != (*pair[1] >= 0.0) {
                crosses += 1;
            }
        }
        let above = self.diffs.iter().filter(|d| **d >= 0.0).count();
        let n = self.diffs.len().max(1);
        let cross_frequency = crosses as f64 / n as f64;
        let side_count = above.max(n - above);
        let price_persistence = side_count as f64 / n as f64;

        VwapRegimeOutput {
            slope_atr,
            slope_state: classify_slope(slope_atr, flat_threshold, strong_threshold),
            distance_atr: if atr > 0.0 {
                price_minus_vwap / atr
            } else {
                0.0
            },
            z_score: if sigma > 0.0 {
                price_minus_vwap / sigma
            } else {
                0.0
            },
            cross_frequency,
            price_persistence,
        }
    }
}

/// The catalog entry `vwap_regime`: the rolling [`Vwap`] (with its sigma and slope), a Wilder ATR
/// and the [`VwapRegimeTracker`] over the distance of the close from the VWAP.
///
/// `value` is the distance of the close from the VWAP in ATRs; `extra` has `vwap`, `slope_atr`
/// (the VWAP slope per bar in ATRs), `slope_state` (-2 strongly falling, -1 moderately falling,
/// 0 flat, 1 moderately rising, 2 strongly rising, by `flat_threshold` and `strong_threshold` on
/// `slope_atr`; also the `state` label), `z_score` (the distance in VWAP sigmas), `cross_frequency`
/// and `price_persistence` over the last `regime_window` bars. A bar without a VWAP, a VWAP slope
/// or an ATR yields no output and does not enter the window; an ATR of zero reads as zero
/// distance and slope. The VWAP needs real volume. [`Indicator::reset`] starts over.
///
/// The first output comes with the first bar after `slope_lookback` published VWAPs and `atr_len`
/// bars; bars without volume delay it, so `warmup_period` is the earliest bar.
pub struct VwapRegimeEngine {
    window: usize,
    slope_lookback: usize,
    atr_len: usize,
    regime_window: usize,
    flat_threshold: f64,
    strong_threshold: f64,
    vwap: Vwap,
    atr: AtrKernel,
    tracker: VwapRegimeTracker,
}

impl VwapRegimeEngine {
    pub fn new(
        window: usize,
        slope_lookback: usize,
        atr_len: usize,
        regime_window: usize,
        flat_threshold: f64,
        strong_threshold: f64,
    ) -> Self {
        Self {
            window,
            slope_lookback,
            atr_len,
            regime_window,
            flat_threshold,
            strong_threshold,
            vwap: Vwap::new(window, slope_lookback),
            atr: AtrKernel::new(TrueRangeSmoothing::Rma, atr_len),
            tracker: VwapRegimeTracker::new(regime_window),
        }
    }
}

impl Indicator for VwapRegimeEngine {
    fn name(&self) -> &str {
        "vwap_regime"
    }

    fn warmup_period(&self) -> usize {
        (self.slope_lookback + 1).max(self.atr_len)
    }

    fn reset(&mut self) {
        *self = Self::new(
            self.window,
            self.slope_lookback,
            self.atr_len,
            self.regime_window,
            self.flat_threshold,
            self.strong_threshold,
        );
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let atr = self.atr.push(bar);
        let vwap = self.vwap.on_bar(bar)?;
        let slope = vwap.extra.get("slope").copied()?;
        let atr = atr?;
        let reading = self.tracker.update_with_slope(
            bar.close - vwap.value,
            atr,
            vwap.extra.get("sigma").copied().unwrap_or(0.0),
            slope,
            self.flat_threshold,
            self.strong_threshold,
        );
        let code = match reading.slope_state {
            SlopeState::StronglyFalling => -2.0,
            SlopeState::ModeratelyFalling => -1.0,
            SlopeState::Flat => 0.0,
            SlopeState::ModeratelyRising => 1.0,
            SlopeState::StronglyRising => 2.0,
        };
        let label = match reading.slope_state {
            SlopeState::StronglyFalling => "strongly_falling",
            SlopeState::ModeratelyFalling => "moderately_falling",
            SlopeState::Flat => "flat",
            SlopeState::ModeratelyRising => "moderately_rising",
            SlopeState::StronglyRising => "strongly_rising",
        };
        let mut output = IndicatorOutput::new(reading.distance_atr).with_state(label);
        for (key, value) in [
            ("vwap", vwap.value),
            ("slope_atr", reading.slope_atr),
            ("slope_state", code),
            ("z_score", reading.z_score),
            ("cross_frequency", reading.cross_frequency),
            ("price_persistence", reading.price_persistence),
        ] {
            output.extra.insert(key.to_string(), value);
        }
        Some(output)
    }
}

/// Registry definition of `vwap_regime`.
pub const VWAP_REGIME_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "vwap_regime",
    aliases: &[],
    description: "VWAP as a regime reading: the distance of the close from the rolling VWAP in ATRs as value, with vwap, slope_atr, slope_state (-2..2), z_score, cross_frequency and price_persistence as extras; needs real volume",
    params: &[
        ParamSpec::count("window", 390, 1, 10000).meaning("bars of the rolling VWAP"),
        ParamSpec::count("slope_lookback", 20, 1, 10000)
            .meaning("bars back for the VWAP slope"),
        ParamSpec::count("atr_len", 14, 1, 10000).meaning("Wilder ATR length the distances are measured in"),
        ParamSpec::count("regime_window", 20, 1, 10000)
            .meaning("bars over which crossings of the VWAP and the share on one side are counted"),
        ParamSpec::number("flat_threshold", 0.1, 0.0, 1000.0)
            .meaning("slope in ATRs per bar from which the VWAP counts as moderately rising or falling"),
        ParamSpec::number("strong_threshold", 0.3, 0.0, 1000.0)
            .meaning("slope in ATRs per bar from which the VWAP counts as strongly rising or falling"),
    ],
    checks: &[crate::indicator::definition::Check::Less {
        lower: "flat_threshold",
        upper: "strong_threshold",
    }],
    build: |p| {
        Ok(Box::new(VwapRegimeEngine::new(
            p.count("window"),
            p.count("slope_lookback"),
            p.count("atr_len"),
            p.count("regime_window"),
            p.number("flat_threshold"),
            p.number("strong_threshold"),
        )))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slope_classification_boundaries() {
        assert_eq!(classify_slope(0.5, 0.1, 0.3), SlopeState::StronglyRising);
        assert_eq!(classify_slope(0.2, 0.1, 0.3), SlopeState::ModeratelyRising);
        assert_eq!(classify_slope(0.0, 0.1, 0.3), SlopeState::Flat);
        assert_eq!(
            classify_slope(-0.2, 0.1, 0.3),
            SlopeState::ModeratelyFalling
        );
        assert_eq!(classify_slope(-0.5, 0.1, 0.3), SlopeState::StronglyFalling);
    }

    #[test]
    fn persistent_one_sided_series_has_low_cross_frequency_high_persistence() {
        let mut tracker = VwapRegimeTracker::new(20);
        let mut out = None;
        for _ in 0..20 {
            out = Some(tracker.update(1.0, 2.0, 0.5, 0.2, 0.1, 0.3));
        }
        let out = out.unwrap();
        assert!((out.cross_frequency).abs() < 1e-9);
        assert!((out.price_persistence - 1.0).abs() < 1e-9);
        assert!((out.distance_atr - 0.5).abs() < 1e-9);
        assert!((out.z_score - 2.0).abs() < 1e-9);
    }

    #[test]
    fn alternating_series_has_high_cross_frequency() {
        let mut tracker = VwapRegimeTracker::new(20);
        let mut out = None;
        for i in 0..20 {
            let diff = if i % 2 == 0 { 1.0 } else { -1.0 };
            out = Some(tracker.update(diff, 2.0, 0.5, 0.0, 0.1, 0.3));
        }
        let out = out.unwrap();
        assert!(out.cross_frequency > 0.8);
    }
}
