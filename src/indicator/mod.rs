pub mod adx;
pub mod alligator;
pub mod anchored_vwap;
pub mod atr;
pub mod bollinger;
pub mod bop;
pub mod bos_choch;
pub mod buy_sell_pressure;
pub mod candle_story;
pub mod cci;
pub mod chaikin_osc;
pub mod chandelier_exit;
pub mod chandelier_flip_radar;
pub mod chart_patterns;
pub mod choppiness;
pub mod connors_rsi;
pub mod coppock;
pub mod divergence;
pub mod dpo;
pub mod efficiency;
pub mod elliott;
pub mod envelope;
pub mod eom;
pub mod fisher_transform;
pub mod harmonics;
pub mod kst;
pub mod liquidity_fvg;
pub mod liquidity_sweeps;
pub mod lsma;
pub mod macd;
pub mod market_structure_breaks;
pub mod mass_index;
pub mod mcginley;
pub mod mfi;
pub mod midas;
pub mod momentum_indicators;
pub mod money_flow_profile;
pub mod moving_averages;
pub mod multi_factor;
pub mod nvi_pvi;
pub mod order_block;
pub mod params;
pub mod pivot_sets;
pub mod pivots_structure;
pub mod price_levels;
pub mod registry;
pub mod relative_strength;
pub mod rsi;
pub mod rvi;
pub mod smart_money_structure;
pub mod smoothing;
pub mod source_mapped;
pub mod stoch_rsi;
pub mod swing_structure;
pub mod tema;
pub mod trend_quality;
pub mod trend_relationship;
pub mod trend_structural;
pub mod tsi;
pub mod vix_fix;
pub mod volatility_indicators;
pub mod volatility_regime;
pub mod volume_flow;
pub mod volume_flow_hires;
pub mod volume_indicators;
pub mod volume_profile;
pub mod volume_profile_extended;
pub mod volume_profile_persistent;
pub mod vortex;
pub mod vwap;
pub mod wavetrend;
pub mod williams_r;
pub mod wyckoff;
pub mod zigzag;
pub mod zigzag_advanced;
pub mod zscore;

use std::collections::HashMap;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::artifact::Artifact;
use crate::model::{Bar, BarValidationError};

/// Output struct returned by an indicator on each processed Bar.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IndicatorOutput {
    pub value: f64,
    pub secondary: Option<f64>,
    pub signal: Option<f64>,
    pub extra: HashMap<String, f64>,
    /// Machine-readable state label (e.g. "trending", "range", "spring"), replacing indicator-
    /// local string keys stuffed into `extra`.
    pub state: Option<String>,
    /// Human-readable explanation for the current `state`/`value`.
    pub reason: Option<String>,
    /// Typed result artifacts (pivots, zones, profiles, scenario progress) emitted this bar.
    pub artifacts: Vec<Artifact>,
}

impl IndicatorOutput {
    pub fn new(value: f64) -> Self {
        Self {
            value,
            secondary: None,
            signal: None,
            extra: HashMap::new(),
            state: None,
            reason: None,
            artifacts: Vec::new(),
        }
    }

    pub fn with_secondary(mut self, secondary: f64) -> Self {
        self.secondary = Some(secondary);
        self
    }

    pub fn with_signal(mut self, signal: f64) -> Self {
        self.signal = Some(signal);
        self
    }

    pub fn with_extra(value: f64, extra: HashMap<String, f64>) -> Self {
        Self {
            value,
            secondary: None,
            signal: None,
            extra,
            state: None,
            reason: None,
            artifacts: Vec::new(),
        }
    }

    pub fn with_state(mut self, state: impl Into<String>) -> Self {
        self.state = Some(state.into());
        self
    }

    pub fn with_reason(mut self, reason: impl Into<String>) -> Self {
        self.reason = Some(reason.into());
        self
    }

    pub fn with_artifact(mut self, artifact: impl Into<Artifact>) -> Self {
        self.artifacts.push(artifact.into());
        self
    }
}

/// Alert emitted by an indicator when a critical threshold or cross occurs.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IndicatorAlert {
    pub kind: String,
    pub note: String,
    pub strength: f64,
}

impl IndicatorAlert {
    pub fn new(kind: impl Into<String>, note: impl Into<String>, strength: f64) -> Self {
        Self {
            kind: kind.into(),
            note: note.into(),
            strength,
        }
    }
}

impl fmt::Display for IndicatorAlert {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.note, self.kind)
    }
}

/// Core trait implemented by all technical indicators.
pub trait Indicator: Send + Sync {
    fn name(&self) -> &str;
    fn warmup_period(&self) -> usize {
        0
    }
    fn reset(&mut self);
    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput>;
    /// Validates a bar before forwarding it to [`Indicator::on_bar`].
    fn on_checked_bar(&mut self, bar: &Bar) -> Result<Option<IndicatorOutput>, BarValidationError> {
        bar.validate()?;
        Ok(self.on_bar(bar))
    }
    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Lets a boxed indicator (as returned by [`registry::build`]/[`registry::build_typed`]) be used
/// anywhere a concrete `Indicator` is expected, e.g. as the generic parameter of
/// [`source_mapped::SourceMapped`] or [`crate::graph::Leaf`].
impl<T: Indicator + ?Sized> Indicator for Box<T> {
    fn name(&self) -> &str {
        (**self).name()
    }
    fn warmup_period(&self) -> usize {
        (**self).warmup_period()
    }
    fn reset(&mut self) {
        (**self).reset()
    }
    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        (**self).on_bar(bar)
    }
    fn on_checked_bar(&mut self, bar: &Bar) -> Result<Option<IndicatorOutput>, BarValidationError> {
        (**self).on_checked_bar(bar)
    }
    fn alerts(&self) -> Vec<IndicatorAlert> {
        (**self).alerts()
    }
}
