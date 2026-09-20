use super::bollinger::BollingerBands;
use super::volatility_indicators::KeltnerChannelEngine;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Volatility Regime Classification State.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum VolatilityState {
    Squeeze,
    #[default]
    Normal,
    Expansion,
}

/// Volatility regime: Bollinger Bands against a Keltner Channel.
///
/// Bollinger Bands over `period` closes at `bb_mult` population standard deviations
/// ([`BollingerBands`]); a Keltner Channel with an EMA(`period`) basis and bands at `kc_mult`
/// times the plain mean of the last 10 true ranges ([`KeltnerChannelEngine`]`(period, 10,
/// kc_mult)`). Then
///
/// - squeeze (`value = -1`): both Bollinger bands inside the channel, `bb_upper <= kc_upper`
///   and `bb_lower >= kc_lower`;
/// - otherwise expansion (`value = 1`): `bb_width > 1.3 · kc_width`;
/// - otherwise normal (`value = 0`).
///
/// Widths are `upper - lower`, the channel's at least `1e-8`; both are in `extra` (`bb_width`,
/// `kc_width`), with `extra["squeeze"]` as 1 or 0. The two middles differ (SMA against EMA), so on
/// a drifting series the bands can leave the channel on one side without the width test firing.
/// First output once both have one: with bar `max(period, 10)`.
///
/// Alert kinds: `volatility_squeeze` and `volatility_expansion`. Up to and including 0.15.0 both
/// states shared the kind `volatility`, which made the detector unusable as an alert rule — a
/// rule could only say "the regime changed", never to what. Consumers matching on the old name
/// see no alert from this indicator any more; the state is unchanged in `value` and
/// `extra["squeeze"]`.
#[derive(Debug, Clone)]
pub struct VolatilityRegimeDetector {
    period: usize,
    bb: BollingerBands,
    keltner: KeltnerChannelEngine,
    state: VolatilityState,
}

impl VolatilityRegimeDetector {
    pub fn new(period: usize, bb_mult: f64, kc_mult: f64) -> Self {
        Self {
            period: period.max(1),
            bb: BollingerBands::new(period, bb_mult),
            keltner: KeltnerChannelEngine::new(period, 10, kc_mult),
            state: VolatilityState::Normal,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 2.0, 1.5)
    }

    pub fn state(&self) -> VolatilityState {
        self.state
    }
}

impl Indicator for VolatilityRegimeDetector {
    fn name(&self) -> &str {
        "volatility_regime"
    }

    fn warmup_period(&self) -> usize {
        self.period.max(10)
    }

    fn reset(&mut self) {
        self.bb.reset();
        self.keltner.reset();
        self.state = VolatilityState::Normal;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let bb_out = self.bb.on_bar(bar);
        let kc_out = self.keltner.on_bar(bar);
        let (Some(bb_out), Some(kc_out)) = (bb_out, kc_out) else {
            return None;
        };

        let bb_upper = bb_out.extra.get("upper").copied().unwrap_or(bb_out.value);
        let bb_lower = bb_out.extra.get("lower").copied().unwrap_or(bb_out.value);

        let kc_upper = kc_out.extra.get("upper").copied().unwrap_or(bb_upper);
        let kc_lower = kc_out.extra.get("lower").copied().unwrap_or(bb_lower);

        // Squeeze when Bollinger Bands are completely inside Keltner Channel
        let is_squeeze = bb_upper <= kc_upper && bb_lower >= kc_lower;
        // Expansion when the Bollinger width exceeds 1.3x the Keltner width
        let bb_width = bb_upper - bb_lower;
        let kc_width = (kc_upper - kc_lower).max(1e-8);
        let is_expansion = bb_width > kc_width * 1.3;

        self.state = if is_squeeze {
            VolatilityState::Squeeze
        } else if is_expansion {
            VolatilityState::Expansion
        } else {
            VolatilityState::Normal
        };

        let state_code = match self.state {
            VolatilityState::Squeeze => -1.0,
            VolatilityState::Normal => 0.0,
            VolatilityState::Expansion => 1.0,
        };

        let mut extra = HashMap::new();
        extra.insert("bb_width".to_string(), bb_width);
        extra.insert("kc_width".to_string(), kc_width);
        extra.insert("squeeze".to_string(), if is_squeeze { 1.0 } else { 0.0 });

        Some(IndicatorOutput::with_extra(state_code, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut alerts = Vec::new();
        if self.state == VolatilityState::Squeeze {
            alerts.push(IndicatorAlert::new(
                "volatility_squeeze",
                "Bollinger Squeeze in Effect",
                0.7,
            ));
        } else if self.state == VolatilityState::Expansion {
            alerts.push(IndicatorAlert::new(
                "volatility_expansion",
                "Volatility Expansion Triggered",
                0.8,
            ));
        }
        alerts
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_regime() {
        let mut vr = VolatilityRegimeDetector::with_defaults();
        let mut out = None;
        for i in 0..30 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + (i % 2) as f64, 1000.0);
            out = vr.on_bar(&b);
        }
        assert!(out.is_some());
    }

    /// Squeeze and expansion must be distinguishable from the alert alone.
    ///
    /// Both carried the kind `volatility` up to 0.15.0, so a rule could match "this indicator
    /// fired" but not which of its two opposite states — the single reason the detector was
    /// unusable as an alert rule downstream. Asserting the two kinds are *different* would pass
    /// again the moment someone renamed one of them to the other's name, so the test names both.
    #[test]
    fn squeeze_and_expansion_carry_their_own_alert_kind() {
        // Wide bars, near-constant close: the Bollinger bands collapse inside the channel.
        let mut vr = VolatilityRegimeDetector::with_defaults();
        for i in 0..30 {
            vr.on_bar(&Bar::new(
                i,
                100.0,
                105.0,
                95.0,
                100.0 + (i % 2) as f64,
                1000.0,
            ));
        }
        assert_eq!(vr.state(), VolatilityState::Squeeze);
        let kinds: Vec<String> = vr.alerts().iter().map(|a| a.kind.clone()).collect();
        assert_eq!(kinds, vec!["volatility_squeeze".to_string()]);

        // Narrow bars on a steady ramp: the window's spread dwarfs the true range.
        let mut vr = VolatilityRegimeDetector::with_defaults();
        for i in 0..30 {
            let close = 100.0 + i as f64;
            vr.on_bar(&Bar::new(i, close, close + 0.1, close - 0.1, close, 1000.0));
        }
        assert_eq!(vr.state(), VolatilityState::Expansion);
        let kinds: Vec<String> = vr.alerts().iter().map(|a| a.kind.clone()).collect();
        assert_eq!(kinds, vec!["volatility_expansion".to_string()]);
    }
}
