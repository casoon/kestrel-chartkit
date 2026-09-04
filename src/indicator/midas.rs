//! MIDAS curves: a launch-anchored cumulative volume-weighted curve (the same accumulation as
//! [`super::anchored_vwap::AnchoredVwapEngine`]), plus Levine's Topfinder/Bottomfinder projection
//! — the part a plain anchored VWAP does not cover. After price sets a new extreme (high for a
//! Topfinder, low for a Bottomfinder) since launch, the projection curve estimates where price
//! will meet resistance/support going forward using the classic square-root volume-decay formula:
//!
//! ```text
//! midas(t)      = cum(price * volume) / cum(volume)              since launch
//! projection(t) = extreme - (extreme - midas(t_extreme)) * sqrt(cum_v(t_extreme) / cum_v(t))
//! ```
//!
//! `projection` converges back toward `extreme` as cumulative volume grows, modeling the curve's
//! resistance/support decay — the qualitative behavior Topfinder/Bottomfinder curves are known
//! for, not a parameter variant of a rolling VWAP.

use std::collections::HashMap;

use crate::model::{Bar, Source};

use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Lifecycle state of a MIDAS Topfinder/Bottomfinder projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidasState {
    /// No extreme (high for Topfinder, low for Bottomfinder) has been set since launch yet;
    /// only the base MIDAS curve is meaningful.
    Launch,
    /// An extreme has been set and the projection curve is actively converging toward it.
    Projecting,
    /// `maturity_bars` have passed since the extreme with no new one set: the projection is
    /// considered to have played out.
    Exhausted,
}

/// Which extreme this engine tracks: highs (Topfinder, projecting resistance after an up-move) or
/// lows (Bottomfinder, projecting support after a down-move).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MidasMode {
    Topfinder,
    Bottomfinder,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MidasOutput {
    pub curve: f64,
    pub projection: Option<f64>,
    pub state: MidasState,
}

pub struct MidasCurveEngine {
    mode: MidasMode,
    source: Source,
    maturity_bars: u32,
    cum_pv: f64,
    cum_v: f64,
    extreme_price: Option<f64>,
    extreme_cum_v: f64,
    extreme_curve_value: f64,
    bars_since_extreme: u32,
    alerts: Vec<IndicatorAlert>,
}

impl MidasCurveEngine {
    pub fn new(mode: MidasMode, source: Source, maturity_bars: u32) -> Self {
        Self {
            mode,
            source,
            maturity_bars: maturity_bars.max(1),
            cum_pv: 0.0,
            cum_v: 0.0,
            extreme_price: None,
            extreme_cum_v: 0.0,
            extreme_curve_value: 0.0,
            bars_since_extreme: 0,
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults(mode: MidasMode) -> Self {
        Self::new(mode, Source::Hlc3, 20)
    }

    fn is_new_extreme(&self, bar: &Bar) -> bool {
        match (self.mode, self.extreme_price) {
            (MidasMode::Topfinder, None) => true,
            (MidasMode::Topfinder, Some(extreme)) => bar.high > extreme,
            (MidasMode::Bottomfinder, None) => true,
            (MidasMode::Bottomfinder, Some(extreme)) => bar.low < extreme,
        }
    }
}

impl Indicator for MidasCurveEngine {
    fn name(&self) -> &str {
        // Fixed regardless of `mode`, matching the established convention for other
        // enum-configured indicators (e.g. `AnchoredVwapEngine`, `PivotSetsEngine`): the mode is
        // exposed through output data (`IndicatorOutput::state`), not the indicator's identity.
        "midas"
    }

    fn reset(&mut self) {
        self.cum_pv = 0.0;
        self.cum_v = 0.0;
        self.extreme_price = None;
        self.extreme_cum_v = 0.0;
        self.extreme_curve_value = 0.0;
        self.bars_since_extreme = 0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let price = self.source.extract(bar);
        self.cum_pv += price * bar.volume;
        self.cum_v += bar.volume;
        if self.cum_v <= 0.0 {
            return None;
        }
        let curve = self.cum_pv / self.cum_v;

        // A pullback (at least one bar since the running extreme was last set) had already begun
        // before this bar iff we were in Projecting/Exhausted territory, i.e. `bars_since_extreme
        // > 0`. Only a *new* extreme arriving after such a pullback genuinely invalidates an
        // in-progress projection; a new extreme while still climbing every bar (Launch) does not.
        let had_pullback = self.bars_since_extreme > 0;

        if self.is_new_extreme(bar) {
            let extreme_price = match self.mode {
                MidasMode::Topfinder => bar.high,
                MidasMode::Bottomfinder => bar.low,
            };
            self.extreme_price = Some(extreme_price);
            self.extreme_cum_v = self.cum_v;
            self.extreme_curve_value = curve;
            self.bars_since_extreme = 0;
            if had_pullback {
                self.alerts.push(IndicatorAlert::new(
                    "midas_extreme_reset",
                    "MIDAS projection restarted from a new extreme",
                    0.6,
                ));
            }
        } else if self.extreme_price.is_some() {
            self.bars_since_extreme += 1;
        }

        // `bars_since_extreme == 0` means either no extreme exists yet, or a new one was just set
        // this very bar (still climbing) — both are "Launch": the projection only begins once a
        // bar has passed *without* extending the extreme.
        let (projection, state) = match self.extreme_price {
            Some(extreme) if self.bars_since_extreme > 0 => {
                let decay = if self.cum_v > 0.0 {
                    (self.extreme_cum_v / self.cum_v).sqrt()
                } else {
                    1.0
                };
                let projected = extreme - (extreme - self.extreme_curve_value) * decay;
                let state = if self.bars_since_extreme >= self.maturity_bars {
                    MidasState::Exhausted
                } else {
                    MidasState::Projecting
                };
                (Some(projected), state)
            }
            _ => (None, MidasState::Launch),
        };

        if state == MidasState::Exhausted && self.bars_since_extreme == self.maturity_bars {
            self.alerts.push(IndicatorAlert::new(
                "midas_exhausted",
                "MIDAS projection reached maturity without a new extreme",
                0.5,
            ));
        }

        let mut extra = HashMap::new();
        if let Some(p) = projection {
            extra.insert("projection".to_string(), p);
        }
        extra.insert(
            "bars_since_extreme".to_string(),
            self.bars_since_extreme as f64,
        );

        let state_label = match state {
            MidasState::Launch => "launch",
            MidasState::Projecting => "projecting",
            MidasState::Exhausted => "exhausted",
        };

        Some(
            IndicatorOutput::with_extra(curve, extra)
                .with_secondary(projection.unwrap_or(curve))
                .with_state(state_label),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn up_move_bars() -> Vec<Bar> {
        // Rally to a peak at bar 4, then pull back.
        vec![
            Bar::new(0, 100.0, 101.0, 99.0, 100.5, 100.0),
            Bar::new(60, 100.5, 103.0, 100.0, 102.5, 120.0),
            Bar::new(120, 102.5, 106.0, 102.0, 105.5, 150.0),
            Bar::new(180, 105.5, 110.0, 105.0, 109.0, 200.0),
            Bar::new(240, 109.0, 115.0, 108.5, 113.0, 250.0), // peak: high = 115.0
            Bar::new(300, 113.0, 114.0, 108.0, 109.0, 180.0),
            Bar::new(360, 109.0, 111.0, 105.0, 106.0, 160.0),
        ]
    }

    #[test]
    fn test_curve_matches_manual_vwap_accumulation() {
        let mut engine = MidasCurveEngine::new(MidasMode::Topfinder, Source::Close, 20);
        let bars = [
            Bar::new(0, 100.0, 101.0, 99.0, 100.0, 10.0),
            Bar::new(60, 101.0, 102.0, 100.0, 102.0, 20.0),
        ];
        let mut last = None;
        for bar in &bars {
            last = engine.on_bar(bar);
        }
        let expected = (100.0 * 10.0 + 102.0 * 20.0) / 30.0;
        assert!((last.unwrap().value - expected).abs() < 1e-9);
    }

    #[test]
    fn test_topfinder_projection_converges_toward_extreme() {
        let mut engine = MidasCurveEngine::new(MidasMode::Topfinder, Source::Hlc3, 20);
        let mut projections = Vec::new();
        for bar in up_move_bars() {
            if let Some(out) = engine.on_bar(&bar) {
                if out.state.as_deref() == Some("projecting") {
                    projections.push(out.extra["projection"]);
                }
            }
        }
        assert!(projections.len() >= 2);
        // As cumulative volume grows past the extreme, sqrt(extreme_v / v) shrinks toward 0, so
        // successive projections must move monotonically closer to the 115.0 extreme.
        for pair in projections.windows(2) {
            let dist_a = (115.0f64 - pair[0]).abs();
            let dist_b = (115.0f64 - pair[1]).abs();
            assert!(dist_b <= dist_a + 1e-9);
        }
    }

    #[test]
    fn test_state_transitions_launch_projecting_exhausted() {
        let mut engine = MidasCurveEngine::new(MidasMode::Topfinder, Source::Hlc3, 2);
        let bars = up_move_bars();

        // The first bar always sets the running extreme (still "climbing"), so it is Launch.
        assert_eq!(
            engine.on_bar(&bars[0]).unwrap().state.as_deref(),
            Some("launch")
        );

        for bar in &bars[1..5] {
            engine.on_bar(bar);
        }
        // Bar 5 and 6 are two bars past the peak at bar 4 with maturity_bars = 2.
        let out5 = engine.on_bar(&bars[5]).unwrap();
        assert_eq!(out5.state.as_deref(), Some("projecting"));
        let out6 = engine.on_bar(&bars[6]).unwrap();
        assert_eq!(out6.state.as_deref(), Some("exhausted"));
    }

    #[test]
    fn test_bottomfinder_tracks_lows_not_highs() {
        let mut engine = MidasCurveEngine::new(MidasMode::Bottomfinder, Source::Hlc3, 20);
        let down_bars: Vec<Bar> = up_move_bars()
            .into_iter()
            .map(|b| {
                Bar::new(
                    b.timestamp,
                    220.0 - b.close,
                    220.0 - b.low,
                    220.0 - b.high,
                    220.0 - b.open,
                    b.volume,
                )
            })
            .collect();
        let mut saw_projecting = false;
        for bar in &down_bars {
            if let Some(out) = engine.on_bar(bar) {
                if out.state.as_deref() == Some("projecting") {
                    saw_projecting = true;
                }
            }
        }
        assert!(
            saw_projecting,
            "a clear down-move must eventually start a Bottomfinder projection"
        );
    }
}
