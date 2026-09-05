//! Chandelier Flip Radar: a Chandelier-Exit-style ATR ratchet stop extended with an adaptive
//! volatility-regime multiplier, a body-filtered distinction between a "real" direction flip and
//! a "weak" one, bull/bear trap detection (a wick breaches the opposite stop without a
//! confirming close), and a continuous `-3..3` risk state — a richer risk-management layer on
//! top of [`super::chandelier_exit::ChandelierExitEngine`]'s base ratchet primitive.
//!
//! Kept as a separate engine rather than folded into [`super::chandelier_exit::
//! ChandelierExitEngine`], matching the precedent already set by the `volume_profile`/
//! `volume_profile_extended`/`volume_profile_persistent` family (a richer variant lives
//! alongside the base engine, not inside it) — the adaptive multiplier in particular has to sit
//! *between* the raw-ATR computation and the stop computation on the same bar, which isn't a
//! seam the base engine exposes, so this reimplements the shared ratchet arithmetic rather than
//! wrapping it. Note this engine's ratchet condition (`prev_direction == 1` gates whether
//! `long_stop` may only tighten) differs slightly from `ChandelierExitEngine`'s
//! (`prev_close > long_stop_prev`) — both are independently-ported readings of the same Pine
//! idiom; this one follows the source below.
//!
//! Ported from the sibling `kestrel` repo's `crates/core/src/indicators/
//! chandelier_flip_radar.rs` (itself a scoped port of `indicators/trend_direction/
//! chandelier_flip_radar/chandelier_flip_radar.pine` v1.6.1 — signal core only, no K-means/
//! conviction mode/chart objects/MTF confluence). See
//! `kestrel/plan/kestrel-chartkit-migration.md` for the comparison that identified this as worth
//! porting.

use crate::model::Bar;
use crate::series::Series;

use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

use std::collections::HashMap;

/// Reusable Chandelier Flip Radar engine — see the module doc comment for how this relates to
/// [`super::chandelier_exit::ChandelierExitEngine`].
pub struct ChandelierFlipRadarEngine {
    length: usize,
    atr_mult: f64,
    use_close_extremes: bool,
    simple_adaptive: bool,
    body_filter_atr: f64,
    danger_dist_atr: f64,
    warn_dist_atr: f64,

    tr_rma: Rma,
    atr_sma: Sma,
    ema5: Ema,
    highs: Series<f64>,
    lows: Series<f64>,

    prev_close: Option<f64>,
    prev_long_stop: Option<f64>,
    prev_short_stop: Option<f64>,
    direction: i8,
    prev_direction: i8,

    alerts: Vec<IndicatorAlert>,
}

impl ChandelierFlipRadarEngine {
    /// `length` sizes the highest-high/lowest-low lookback and the internal ATR. `atr_mult`
    /// scales the ATR offset from those extremes. `use_close_extremes` swaps the highest/lowest
    /// source from bar high/low to bar close (Pine default: close-based). `simple_adaptive`
    /// scales `atr_mult` by 1.2x/0.85x when the current raw ATR is >30% above/>20% below its own
    /// 100-bar SMA (a volatility-regime signal). `body_filter_atr` gates a "real" direction flip
    /// from a "weak" one: a flip only counts if `|close - open| >= atr_raw * body_filter_atr`
    /// (`0.0` disables the filter — every structural flip counts as real). `danger_dist_atr`/
    /// `warn_dist_atr` bucket the continuous risk state by how many ATRs price sits from the
    /// active stop.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        length: usize,
        atr_mult: f64,
        use_close_extremes: bool,
        simple_adaptive: bool,
        body_filter_atr: f64,
        danger_dist_atr: f64,
        warn_dist_atr: f64,
    ) -> Self {
        let length = length.max(1);
        Self {
            length,
            atr_mult,
            use_close_extremes,
            simple_adaptive,
            body_filter_atr,
            danger_dist_atr,
            warn_dist_atr,
            tr_rma: Rma::new(length),
            atr_sma: Sma::new(100),
            ema5: Ema::new(5),
            highs: Series::new(length),
            lows: Series::new(length),
            prev_close: None,
            prev_long_stop: None,
            prev_short_stop: None,
            direction: 1,
            prev_direction: 1,
            alerts: Vec::new(),
        }
    }

    /// Pine defaults: `length=30`, `atr_mult=4.5`, close-based extremes, adaptive mode off,
    /// `body_filter_atr=0.80`, `danger_dist_atr=0.35`, `warn_dist_atr=0.75`.
    pub fn with_defaults() -> Self {
        Self::new(30, 4.5, true, false, 0.80, 0.35, 0.75)
    }
}

/// Maps the engine's signed `-3..3` risk-state number to a machine-readable label for
/// [`IndicatorOutput::state`]: `1..3` long (danger/caution/healthy), `-1..-3` short.
fn state_label(state: i32) -> &'static str {
    match state {
        3 => "long_healthy",
        2 => "long_caution",
        1 => "long_danger",
        -1 => "short_danger",
        -2 => "short_caution",
        _ => "short_healthy",
    }
}

impl Indicator for ChandelierFlipRadarEngine {
    fn name(&self) -> &str {
        "chandelier_flip_radar"
    }

    fn warmup_period(&self) -> usize {
        self.length
    }

    fn reset(&mut self) {
        self.tr_rma.reset();
        self.atr_sma.reset();
        self.ema5.reset();
        self.highs.reset();
        self.lows.reset();
        self.prev_close = None;
        self.prev_long_stop = None;
        self.prev_short_stop = None;
        self.direction = 1;
        self.prev_direction = 1;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let tr = match self.prev_close {
            None => bar.high - bar.low,
            Some(prev_close) => (bar.high - bar.low)
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs()),
        };
        let atr_raw = self.tr_rma.update(tr);
        let atr_avg = self.atr_sma.update(atr_raw.unwrap_or(tr));

        let high_src = if self.use_close_extremes {
            bar.close
        } else {
            bar.high
        };
        let low_src = if self.use_close_extremes {
            bar.close
        } else {
            bar.low
        };
        self.highs.push(high_src);
        self.lows.push(low_src);
        let ch_hi = self.highs.highest(self.length);
        let ch_lo = self.lows.lowest(self.length);
        let ema5 = self.ema5.update(bar.close);

        let (atr_raw, ch_hi, ch_lo) = match (atr_raw, ch_hi, ch_lo) {
            (Some(atr), Some(hi), Some(lo)) => (atr, hi, lo),
            _ => {
                self.prev_close = Some(bar.close);
                return None;
            }
        };

        let dyn_mult = if self.simple_adaptive {
            let atr_avg = atr_avg.unwrap_or(atr_raw);
            let vol_ratio = if atr_avg != 0.0 {
                atr_raw / atr_avg
            } else {
                1.0
            };
            if vol_ratio > 1.3 {
                self.atr_mult * 1.2
            } else if vol_ratio < 0.8 {
                self.atr_mult * 0.85
            } else {
                self.atr_mult
            }
        } else {
            self.atr_mult
        };
        let atr = dyn_mult * atr_raw;

        let long_stop_raw = ch_hi - atr;
        let short_stop_raw = ch_lo + atr;
        let long_stop_prev = self.prev_long_stop.unwrap_or(long_stop_raw);
        let short_stop_prev = self.prev_short_stop.unwrap_or(short_stop_raw);

        let long_stop = if self.prev_direction == 1 {
            long_stop_raw.max(long_stop_prev)
        } else {
            long_stop_raw
        };
        let short_stop = if self.prev_direction == -1 {
            short_stop_raw.min(short_stop_prev)
        } else {
            short_stop_raw
        };

        let body_ok = self.body_filter_atr <= 0.0
            || (bar.close - bar.open).abs() >= atr_raw * self.body_filter_atr;
        let flip_long = bar.close > short_stop_prev && body_ok;
        let flip_short = bar.close < long_stop_prev && body_ok;

        let old_direction = self.direction;
        self.direction = if flip_long {
            1
        } else if flip_short {
            -1
        } else {
            self.direction
        };

        let buy_signal = self.direction == 1 && old_direction == -1;
        let sell_signal = self.direction == -1 && old_direction == 1;
        let weak_long = self.direction == -1 && bar.close > short_stop_prev && !body_ok;
        let weak_short = self.direction == 1 && bar.close < long_stop_prev && !body_ok;

        let trigger_stop = if self.direction == 1 {
            long_stop_prev
        } else {
            short_stop_prev
        };
        let dist_atr = if atr_raw != 0.0 {
            (bar.close - trigger_stop).abs() / atr_raw
        } else {
            0.0
        };
        let warn_level = self.warn_dist_atr.max(self.danger_dist_atr + 0.05);
        let danger = dist_atr < self.danger_dist_atr;
        let warn = !danger && dist_atr < warn_level;
        let pullback_long = self.direction == 1 && bar.close < ema5;
        let pullback_short = self.direction == -1 && bar.close > ema5;
        let state: i32 = if self.direction == 1 {
            if danger {
                1
            } else if warn || pullback_long {
                2
            } else {
                3
            }
        } else if danger {
            -1
        } else if warn || pullback_short {
            -2
        } else {
            -3
        };

        let bull_trap =
            self.direction == -1 && bar.high > short_stop_prev && bar.close <= short_stop_prev;
        let bear_trap =
            self.direction == 1 && bar.low < long_stop_prev && bar.close >= long_stop_prev;

        if buy_signal {
            self.alerts.push(IndicatorAlert::new(
                "bull_flip",
                "Chandelier Flip Radar: flipped long",
                1.0,
            ));
        }
        if sell_signal {
            self.alerts.push(IndicatorAlert::new(
                "bear_flip",
                "Chandelier Flip Radar: flipped short",
                1.0,
            ));
        }
        if weak_long {
            self.alerts.push(IndicatorAlert::new(
                "bull_weak_flip",
                "Chandelier Flip Radar: weak long flip (body filter not met)",
                1.0,
            ));
        }
        if weak_short {
            self.alerts.push(IndicatorAlert::new(
                "bear_weak_flip",
                "Chandelier Flip Radar: weak short flip (body filter not met)",
                1.0,
            ));
        }
        if bull_trap {
            self.alerts.push(IndicatorAlert::new(
                "bear_bull_trap",
                "Chandelier Flip Radar: bull trap (wick above short stop, close back inside)",
                (1.0 - dist_atr).clamp(0.0, 1.0),
            ));
        }
        if bear_trap {
            self.alerts.push(IndicatorAlert::new(
                "bull_bear_trap",
                "Chandelier Flip Radar: bear trap (wick below long stop, close back inside)",
                (1.0 - dist_atr).clamp(0.0, 1.0),
            ));
        }

        self.prev_direction = self.direction;
        self.prev_long_stop = Some(long_stop);
        self.prev_short_stop = Some(short_stop);
        self.prev_close = Some(bar.close);

        let active_stop = if self.direction == 1 {
            long_stop
        } else {
            short_stop
        };

        let mut extra = HashMap::new();
        extra.insert("long_stop".to_string(), long_stop);
        extra.insert("short_stop".to_string(), short_stop);
        extra.insert("dist_atr".to_string(), dist_atr);
        extra.insert("multiplier".to_string(), dyn_mult);
        extra.insert("risk_state".to_string(), state as f64);

        Some(IndicatorOutput::with_extra(active_stop, extra).with_state(state_label(state)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values below were independently derived from a fresh Python transcription of
    // this same documented formula (not by running this Rust code) — see
    // `kestrel/plan/kestrel-chartkit-migration.md` for the source formula this ports.

    fn trending_up_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64 * 2.0;
                Bar::new(
                    i as i64 * 60,
                    base,
                    base + 3.0,
                    base - 3.0,
                    base + 1.0,
                    100.0,
                )
            })
            .collect()
    }

    #[test]
    fn base_ratchet_matches_independently_derived_reference() {
        // length=5, atr_mult=3.0, high/low-based extremes, no adaptive, no body filter.
        let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, false, 0.0, 0.35, 0.75);
        let bars = trending_up_bars(15);
        let mut last = None;
        for bar in &bars {
            if let Some(out) = engine.on_bar(bar) {
                last = Some(out);
            }
        }
        let out = last.unwrap();
        assert!(
            (out.value - 113.0).abs() < 1e-9,
            "active_stop: {}",
            out.value
        );
        assert_eq!(out.extra["risk_state"], 3.0);
        assert!((out.extra["dist_atr"] - 3.0).abs() < 1e-6);
        assert!((out.extra["multiplier"] - 3.0).abs() < 1e-9);
        assert_eq!(out.state.as_deref(), Some("long_healthy"));
    }

    #[test]
    fn adaptive_multiplier_engages_on_volatility_spike_and_flags_bear_trap() {
        // length=5, atr_mult=3.0, high/low extremes, adaptive on, no body filter. 105 bars of
        // flat/stable range fill the 100-SMA ATR baseline, then one wide-range bar spikes raw ATR
        // well above it (vol_ratio > 1.3) while also wicking through the long stop without
        // closing through it (bear trap).
        let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, true, 0.0, 0.35, 0.75);
        let mut bars: Vec<Bar> = (0..105)
            .map(|i| Bar::new(i as i64 * 60, 100.0, 101.0, 99.0, 100.0, 100.0))
            .collect();
        bars.push(Bar::new(105 * 60, 100.0, 250.0, 50.0, 150.0, 100.0));

        let mut last = None;
        for bar in &bars {
            if let Some(out) = engine.on_bar(bar) {
                last = Some(out);
            }
        }
        let out = last.unwrap();
        assert!(
            (out.extra["multiplier"] - 3.6).abs() < 1e-9,
            "multiplier: {}",
            out.extra["multiplier"]
        );
        assert!(
            (out.value - 100.240_000_000_000_01).abs() < 1e-6,
            "active_stop: {}",
            out.value
        );
        let alerts = engine.alerts();
        assert!(
            alerts.iter().any(|a| a.kind == "bull_bear_trap"),
            "expected a bear-trap alert, got {alerts:?}"
        );
    }

    #[test]
    fn body_filter_downgrades_a_structural_flip_to_a_weak_flip() {
        // length=5, atr_mult=3.0, high/low extremes, no adaptive, Pine-default body filter 0.8.
        // After 6 warmup/trend bars establish long_stop, a bar closes below long_stop but with a
        // tiny body relative to ATR — the direction must NOT flip, and a weak-short alert (not a
        // sell-signal) fires instead.
        let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, false, false, 0.8, 0.35, 0.75);
        let mut bars = trending_up_bars(6);
        bars.push(Bar::new(6 * 60, 94.6, 96.0, 94.0, 94.5, 100.0));

        let mut last = None;
        for bar in &bars {
            if let Some(out) = engine.on_bar(bar) {
                last = Some(out);
            }
        }
        let out = last.unwrap();
        assert!(
            (out.value - 95.0).abs() < 1e-9,
            "active_stop must stay at the long stop (no flip): {}",
            out.value
        );
        assert_eq!(
            out.extra["risk_state"], 1.0,
            "close is within danger distance of the stop"
        );
        assert_eq!(out.state.as_deref(), Some("long_danger"));
        let alerts = engine.alerts();
        assert!(
            alerts.iter().any(|a| a.kind == "bear_weak_flip"),
            "expected a weak-short-flip alert, got {alerts:?}"
        );
        assert!(
            !alerts.iter().any(|a| a.kind == "bear_flip"),
            "must not be a real flip: {alerts:?}"
        );
    }

    #[test]
    fn test_warmup_returns_none_then_emits() {
        let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, true, false, 0.8, 0.35, 0.75);
        let bars = trending_up_bars(10);
        let mut outputs = Vec::new();
        for bar in &bars {
            outputs.push(engine.on_bar(bar));
        }
        assert!(outputs[..4].iter().all(|o| o.is_none()));
        assert!(outputs[4..].iter().all(|o| o.is_some()));
    }

    #[test]
    fn test_reset_clears_state() {
        let mut engine = ChandelierFlipRadarEngine::new(5, 3.0, true, false, 0.8, 0.35, 0.75);
        for bar in trending_up_bars(6) {
            engine.on_bar(&bar);
        }
        engine.reset();
        assert_eq!(engine.on_bar(&trending_up_bars(1)[0]), None);
    }
}
