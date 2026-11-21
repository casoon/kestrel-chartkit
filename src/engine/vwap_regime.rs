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
    /// Crosses per bar over the tracked window, at most (n-1)/n for n bars — high = Balance,
    /// low = Trend. A cross is a change between the last non-zero side and a new non-zero side.
    pub cross_frequency: f64,
    /// Share of bars on the dominant side of the VWAP, 0.0..1.0; bars on the VWAP count on neither
    /// side (they count in the divisor only).
    pub price_persistence: f64,
}

/// Slope state from the slope in ATRs. A slope of exactly 0 (also `-0.0`) is always `Flat`,
/// whatever the thresholds.
pub fn classify_slope(slope_atr: f64, flat_threshold: f64, strong_threshold: f64) -> SlopeState {
    if slope_atr >= strong_threshold {
        SlopeState::StronglyRising
    } else if slope_atr > 0.0 && slope_atr >= flat_threshold {
        SlopeState::ModeratelyRising
    } else if slope_atr <= -strong_threshold {
        SlopeState::StronglyFalling
    } else if slope_atr < 0.0 && slope_atr <= -flat_threshold {
        SlopeState::ModeratelyFalling
    } else {
        SlopeState::Flat
    }
}

/// Tracks cross-frequency and price-persistence relative to VWAP over a rolling window — feed
/// it `price - vwap` each bar (e.g. from `indicator::vwap::Vwap`'s output).
///
/// A diff of exactly 0 is on neither side. Crosses compare each non-zero side with the last
/// non-zero side in the window, so a 0 neither counts nor resets. The tracker returns `0.0` for
/// `distance_atr` with an ATR of zero and for `z_score` with a sigma of zero: that is the raw-API
/// behaviour; the catalog entry [`VwapRegimeEngine`] does not publish those readings.
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
        let mut above = 0usize;
        let mut below = 0usize;
        let mut last_side = 0i8;
        for &d in &self.diffs {
            let side = if d > 0.0 {
                above += 1;
                1
            } else if d < 0.0 {
                below += 1;
                -1
            } else {
                0
            };
            if side != 0 {
                if last_side != 0 && side != last_side {
                    crosses += 1;
                }
                last_side = side;
            }
        }
        let n = self.diffs.len().max(1);
        let cross_frequency = crosses as f64 / n as f64;
        let price_persistence = above.max(below) as f64 / n as f64;

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

/// The catalog entry `kestrel_vwap_regime`: the rolling [`Vwap`] (with its sigma and slope), a Wilder ATR
/// and the [`VwapRegimeTracker`] over the distance of the close from the VWAP.
///
/// `value` is the distance of the close from the VWAP in ATRs; `extra` has `vwap`, `slope_atr`
/// (the VWAP slope per bar in ATRs), `slope_state` (-2 strongly falling, -1 moderately falling,
/// 0 flat, 1 moderately rising, 2 strongly rising, by `flat_threshold` and `strong_threshold` on
/// `slope_atr`; also the `state` label), `z_score` (the distance in VWAP sigmas, absent while the
/// VWAP sigma is zero), `cross_frequency` and `price_persistence` over the last `regime_window`
/// bars. The slope is per published VWAP. A bar without a VWAP, a VWAP slope or an ATR, or with
/// an ATR of zero, yields no output and does not enter the window. The VWAP needs real volume. [`Indicator::reset`] starts over.
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
        "kestrel_vwap_regime"
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
        if atr <= 0.0 {
            return None;
        }
        let sigma = vwap.extra.get("sigma").copied().unwrap_or(0.0);
        let reading = self.tracker.update_with_slope(
            bar.close - vwap.value,
            atr,
            sigma,
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
            ("cross_frequency", reading.cross_frequency),
            ("price_persistence", reading.price_persistence),
        ] {
            output.extra.insert(key.to_string(), value);
        }
        if sigma > 0.0 {
            output.extra.insert("z_score".to_string(), reading.z_score);
        }
        Some(output)
    }
}

/// Registry definition of `kestrel_vwap_regime`; `vwap_regime` is its alias.
pub const VWAP_REGIME_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "kestrel_vwap_regime",
    aliases: &["vwap_regime"],
    description: "Kestrel VWAP as a regime reading: the distance of the close from the rolling VWAP in ATRs as value, with vwap, slope_atr, slope_state (-2..2), cross_frequency and price_persistence as extras, plus z_score while the VWAP sigma is positive; needs real volume",
    params: &[
        ParamSpec::count("window", 390, 1, 10000).meaning("bars of the rolling VWAP"),
        ParamSpec::count("slope_lookback", 20, 1, 10000)
            .meaning("published VWAPs back for the VWAP slope (the slope is per published VWAP)"),
        ParamSpec::count("atr_len", 14, 1, 10000).meaning("Wilder ATR length the distances are measured in"),
        ParamSpec::count("regime_window", 20, 1, 10000)
            .meaning("bars (those that produced output) over which crossings of the VWAP and the share on the dominant side are counted"),
        ParamSpec::number("flat_threshold", 0.1, 0.0, 1000.0)
            .meaning("slope in ATRs per bar from which the VWAP counts as moderately rising or falling"),
        ParamSpec::number("strong_threshold", 0.3, 0.0, 1000.0)
            .meaning("slope in ATRs per bar from which the VWAP counts as strongly rising or falling"),
    ],
    checks: &[crate::indicator::definition::Check::Less {
        lower: "flat_threshold",
        upper: "strong_threshold",
    }],
    facts: crate::indicator::definition::IndicatorFacts::unitless()
        .needs_real_volume()
        .session_sensitive()
        .range_dependent(),
    verification: crate::indicator::definition::Verification::Open(
        "no independent second implementation yet",
    ),
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
    fn zero_slope_is_flat_at_every_threshold() {
        assert_eq!(classify_slope(0.0, 0.0, 0.3), SlopeState::Flat);
        assert_eq!(classify_slope(-0.0, 0.0, 0.3), SlopeState::Flat);
        assert_eq!(classify_slope(0.01, 0.0, 0.3), SlopeState::ModeratelyRising);
        assert_eq!(
            classify_slope(-0.01, 0.0, 0.3),
            SlopeState::ModeratelyFalling
        );
    }

    #[test]
    fn tie_at_the_vwap_mirrors() {
        let run = |sign: f64| {
            let mut tracker = VwapRegimeTracker::new(20);
            let mut out = None;
            for d in [1.0, 0.0, -1.0, 0.0, 1.0, 1.0, 0.0] {
                out = Some(tracker.update(sign * d, 2.0, 0.5, sign * 0.2, 0.1, 0.3));
            }
            out.unwrap()
        };
        let (a, b) = (run(1.0), run(-1.0));
        assert_eq!(a.cross_frequency, b.cross_frequency);
        assert_eq!(a.price_persistence, b.price_persistence);
        assert_eq!(a.cross_frequency, 2.0 / 7.0);
        assert_eq!(a.price_persistence, 3.0 / 7.0);
        assert_eq!(a.slope_state, SlopeState::ModeratelyRising);
        assert_eq!(b.slope_state, SlopeState::ModeratelyFalling);
        assert_eq!(a.distance_atr, -b.distance_atr);
    }

    fn bar(i: i64, high: f64, low: f64, close: f64) -> Bar {
        Bar {
            timestamp: i,
            open: close,
            high,
            low,
            close,
            volume: 5.0,
        }
    }

    #[test]
    fn zero_atr_gives_no_output() {
        let mut engine = VwapRegimeEngine::new(10, 3, 3, 5, 0.1, 0.3);
        for i in 0..20 {
            assert!(engine.on_bar(&bar(i, 10.0, 10.0, 10.0)).is_none());
        }
    }

    #[test]
    fn zero_sigma_has_no_z_score() {
        // A constant typical price (sigma 0) with a range that gives a positive ATR.
        let mut engine = VwapRegimeEngine::new(10, 3, 3, 5, 0.1, 0.3);
        let mut last = None;
        for i in 0..20 {
            last = engine.on_bar(&bar(i, 11.0, 9.0, 10.0));
        }
        let out = last.expect("output");
        assert!(!out.extra.contains_key("z_score"));
        assert!(out.extra.contains_key("cross_frequency"));
        // A varying price gives a positive sigma and a z_score.
        let mut engine = VwapRegimeEngine::new(10, 3, 3, 5, 0.1, 0.3);
        let mut last = None;
        for i in 0..30 {
            let c = 100.0 + (i as f64 * 0.7).sin() * 3.0;
            last = engine.on_bar(&bar(i, c + 1.0, c - 1.0, c));
        }
        assert!(last.expect("output").extra.contains_key("z_score"));
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
