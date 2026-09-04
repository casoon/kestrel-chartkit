use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Supported resolution timeframes for market bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Resolution {
    M1,
    M5,
    M15,
    M30,
    H1,
    H4,
    D1,
    W1,
}

impl Resolution {
    pub fn as_str(&self) -> &'static str {
        match self {
            Resolution::M1 => "1m",
            Resolution::M5 => "5m",
            Resolution::M15 => "15m",
            Resolution::M30 => "30m",
            Resolution::H1 => "1h",
            Resolution::H4 => "4h",
            Resolution::D1 => "1d",
            Resolution::W1 => "1w",
        }
    }
}

impl fmt::Display for Resolution {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Selectable price/volume data source for indicators and series calculations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum Source {
    #[default]
    Close,
    Open,
    High,
    Low,
    Hl2,
    Hlc3,
    Ohlc4,
    Volume,
    TypicalPrice,
}

impl Source {
    /// Extracts the scalar price/volume value from an OHLCV bar according to the chosen source.
    pub fn extract(&self, bar: &Bar) -> f64 {
        match self {
            Source::Close => bar.close,
            Source::Open => bar.open,
            Source::High => bar.high,
            Source::Low => bar.low,
            Source::Hl2 => (bar.high + bar.low) / 2.0,
            Source::Hlc3 => (bar.high + bar.low + bar.close) / 3.0,
            Source::Ohlc4 => (bar.open + bar.high + bar.low + bar.close) / 4.0,
            Source::Volume => bar.volume,
            Source::TypicalPrice => bar.typical_price(),
        }
    }
}

/// Provider-neutral instrument metadata.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct InstrumentMeta {
    pub symbol: String,
    pub tick_size: f64,
    pub price_precision: usize,
    pub timezone: String,
}

impl Default for InstrumentMeta {
    fn default() -> Self {
        Self {
            symbol: "GENERIC".to_string(),
            tick_size: 0.01,
            price_precision: 2,
            timezone: "UTC".to_string(),
        }
    }
}

/// Reason why [`InstrumentMeta`] fails the operative validity contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InstrumentMetaError {
    NonPositiveTickSize,
    NonFiniteTickSize,
    ExcessivePricePrecision,
    EmptySymbol,
    EmptyTimezone,
}

impl fmt::Display for InstrumentMetaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NonPositiveTickSize => "tick_size must be greater than zero",
            Self::NonFiniteTickSize => "tick_size must be finite",
            Self::ExcessivePricePrecision => "price_precision must be <= 12",
            Self::EmptySymbol => "symbol must not be empty",
            Self::EmptyTimezone => "timezone must not be empty",
        };
        f.write_str(message)
    }
}

impl std::error::Error for InstrumentMetaError {}

impl InstrumentMeta {
    /// Validates the operative contract required to use this metadata for rounding, risk and
    /// session calculations: a finite positive tick size, a sane price precision, and non-empty
    /// symbol/timezone identifiers.
    pub fn validate(&self) -> Result<(), InstrumentMetaError> {
        if !self.tick_size.is_finite() {
            return Err(InstrumentMetaError::NonFiniteTickSize);
        }
        if self.tick_size <= 0.0 {
            return Err(InstrumentMetaError::NonPositiveTickSize);
        }
        if self.price_precision > 12 {
            return Err(InstrumentMetaError::ExcessivePricePrecision);
        }
        if self.symbol.trim().is_empty() {
            return Err(InstrumentMetaError::EmptySymbol);
        }
        if self.timezone.trim().is_empty() {
            return Err(InstrumentMetaError::EmptyTimezone);
        }
        Ok(())
    }

    /// Rounds `price` to the nearest multiple of [`InstrumentMeta::tick_size`].
    ///
    /// Returns `price` unchanged if `tick_size` is non-finite or non-positive, so this method is
    /// safe to call on unvalidated metadata (see [`InstrumentMeta::validate`] to reject that case
    /// explicitly at ingestion boundaries).
    pub fn round_to_tick(&self, price: f64) -> f64 {
        if !self.tick_size.is_finite() || self.tick_size <= 0.0 || !price.is_finite() {
            return price;
        }
        (price / self.tick_size).round() * self.tick_size
    }
}

/// Explicit availability/quality metadata for an OHLCV bar, replacing implicit `f64`
/// conventions (e.g. `volume == 0.0` meaning "unknown" versus "genuinely zero").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BarQuality {
    /// False when the feed cannot report volume for this bar (as opposed to a true zero-volume
    /// bar). Consumers should fall back to equal-weight/price-based heuristics when false.
    pub volume_available: bool,
    /// True when the bar was synthesized (e.g. holiday padding, session stitching) rather than
    /// observed directly from the feed.
    pub is_synthetic: bool,
    /// True when the bar's price/volume was forward-filled from a prior bar rather than observed.
    pub is_forward_filled: bool,
    /// True when a time gap precedes this bar (missing bar(s) between it and the prior bar).
    pub has_gap: bool,
}

impl BarQuality {
    /// Quality flags for a directly observed, complete bar: volume available, no synthetic or
    /// forward-filled data, no gap.
    pub fn observed() -> Self {
        Self {
            volume_available: true,
            is_synthetic: false,
            is_forward_filled: false,
            has_gap: false,
        }
    }
}

/// An OHLCV [`Bar`] paired with explicit [`BarQuality`] metadata.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct QualifiedBar {
    pub bar: Bar,
    pub quality: BarQuality,
}

impl QualifiedBar {
    pub fn new(bar: Bar, quality: BarQuality) -> Self {
        Self { bar, quality }
    }

    /// Wraps a bar with [`BarQuality::observed`] flags.
    pub fn observed(bar: Bar) -> Self {
        Self {
            bar,
            quality: BarQuality::observed(),
        }
    }
}

/// Generic OHLCV Bar data point.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Bar {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// Reason why an OHLCV bar violates the public input contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BarValidationError {
    NonFiniteValue,
    NonPositivePrice,
    NegativeVolume,
    InvalidPriceRange,
}

impl fmt::Display for BarValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::NonFiniteValue => "OHLCV values must be finite",
            Self::NonPositivePrice => "OHLC prices must be greater than zero",
            Self::NegativeVolume => "volume must be non-negative",
            Self::InvalidPriceRange => "low/high must contain open and close",
        };
        f.write_str(message)
    }
}

impl std::error::Error for BarValidationError {}

impl Bar {
    /// Creates a bar without validation.
    ///
    /// Use [`Bar::try_new`] at data-ingestion boundaries. This unchecked constructor is retained
    /// for trusted feeds and compatibility with existing consumers.
    pub fn new(timestamp: i64, open: f64, high: f64, low: f64, close: f64, volume: f64) -> Self {
        Self {
            timestamp,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    /// Creates a bar after validating the OHLCV input contract.
    pub fn try_new(
        timestamp: i64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
    ) -> Result<Self, BarValidationError> {
        let bar = Self::new(timestamp, open, high, low, close, volume);
        bar.validate()?;
        Ok(bar)
    }

    pub fn typical_price(&self) -> f64 {
        (self.high + self.low + self.close) / 3.0
    }

    /// Verifies that OHLCV prices and volumes satisfy mathematical and physical domain requirements:
    /// - All price values are finite and strictly positive (> 0.0)
    /// - Volume is finite and non-negative (>= 0.0)
    /// - Structural inequality holds: `low <= min(open, close)` and `high >= max(open, close)`
    pub fn validate(&self) -> Result<(), BarValidationError> {
        if !self.open.is_finite()
            || !self.high.is_finite()
            || !self.low.is_finite()
            || !self.close.is_finite()
            || !self.volume.is_finite()
        {
            return Err(BarValidationError::NonFiniteValue);
        }

        if self.open <= 0.0 || self.high <= 0.0 || self.low <= 0.0 || self.close <= 0.0 {
            return Err(BarValidationError::NonPositivePrice);
        }
        if self.volume < 0.0 {
            return Err(BarValidationError::NegativeVolume);
        }

        let min_oc = self.open.min(self.close);
        let max_oc = self.open.max(self.close);

        if self.low > min_oc || self.high < max_oc || self.low > self.high {
            return Err(BarValidationError::InvalidPriceRange);
        }

        Ok(())
    }

    /// Returns whether the bar satisfies [`Bar::validate`].
    pub fn is_valid(&self) -> bool {
        self.validate().is_ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bar_validation_contract() {
        let valid = Bar::new(1000, 100.0, 105.0, 95.0, 104.0, 1000.0);
        assert!(valid.is_valid());
        assert_eq!(
            Bar::try_new(1000, 100.0, 105.0, 95.0, 104.0, 1000.0),
            Ok(valid)
        );

        // Negative price
        let neg_price = Bar::new(1000, -100.0, 105.0, 95.0, 104.0, 1000.0);
        assert!(!neg_price.is_valid());

        // Negative volume
        let neg_vol = Bar::new(1000, 100.0, 105.0, 95.0, 104.0, -10.0);
        assert!(!neg_vol.is_valid());

        // High lower than open/close
        let bad_high = Bar::new(1000, 100.0, 90.0, 80.0, 95.0, 1000.0);
        assert!(!bad_high.is_valid());

        // NaN price
        let nan_price = Bar::new(1000, f64::NAN, 105.0, 95.0, 104.0, 1000.0);
        assert!(!nan_price.is_valid());
        assert_eq!(
            nan_price.validate(),
            Err(BarValidationError::NonFiniteValue)
        );
    }

    #[test]
    fn test_instrument_meta_validate() {
        assert_eq!(InstrumentMeta::default().validate(), Ok(()));

        let bad_tick = InstrumentMeta {
            tick_size: 0.0,
            ..InstrumentMeta::default()
        };
        assert_eq!(
            bad_tick.validate(),
            Err(InstrumentMetaError::NonPositiveTickSize)
        );

        let bad_precision = InstrumentMeta {
            price_precision: 13,
            ..InstrumentMeta::default()
        };
        assert_eq!(
            bad_precision.validate(),
            Err(InstrumentMetaError::ExcessivePricePrecision)
        );

        let empty_symbol = InstrumentMeta {
            symbol: "".to_string(),
            ..InstrumentMeta::default()
        };
        assert_eq!(
            empty_symbol.validate(),
            Err(InstrumentMetaError::EmptySymbol)
        );
    }

    #[test]
    fn test_instrument_meta_round_to_tick() {
        let meta = InstrumentMeta {
            tick_size: 0.25,
            ..InstrumentMeta::default()
        };
        assert_eq!(meta.round_to_tick(100.10), 100.0);
        assert_eq!(meta.round_to_tick(100.13), 100.25);
        assert_eq!(meta.round_to_tick(100.125), 100.25);

        let invalid_tick = InstrumentMeta {
            tick_size: 0.0,
            ..InstrumentMeta::default()
        };
        // Falls back to the unrounded price rather than dividing by zero.
        assert_eq!(invalid_tick.round_to_tick(100.10), 100.10);
    }

    #[test]
    fn test_bar_quality_defaults() {
        let default_quality = BarQuality::default();
        assert!(!default_quality.volume_available);
        assert!(!default_quality.is_synthetic);

        let observed = BarQuality::observed();
        assert!(observed.volume_available);
        assert!(!observed.is_synthetic);
        assert!(!observed.is_forward_filled);
        assert!(!observed.has_gap);

        let bar = Bar::new(1000, 100.0, 105.0, 95.0, 104.0, 1000.0);
        let qualified = QualifiedBar::observed(bar.clone());
        assert_eq!(qualified.bar, bar);
        assert_eq!(qualified.quality, BarQuality::observed());
    }
}

/// Classification of current market regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum MarketRegime {
    BullishExpansion,
    BearishExpansion,
    #[default]
    Consolidation,
    Transition,
}

impl fmt::Display for MarketRegime {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MarketRegime::BullishExpansion => write!(f, "Bullish Expansion"),
            MarketRegime::BearishExpansion => write!(f, "Bearish Expansion"),
            MarketRegime::Consolidation => write!(f, "Consolidation / Range"),
            MarketRegime::Transition => write!(f, "Regime Transition"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ZoneKind {
    Support,
    Resistance,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SupportResistanceZone {
    pub kind: ZoneKind,
    pub price: f64,
    pub price_top: f64,
    pub price_bottom: f64,
    pub strength: f64, // 0.0 ..= 1.0
    pub distance_pct: f64,
    pub touches: u32,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct RiskPlan {
    pub entry: f64,
    pub stop_loss: f64,
    pub target_1: f64,
    pub target_2: f64,
    pub risk_reward_ratio: f64,
}

impl RiskPlan {
    /// Rounds `entry`/`stop_loss`/`target_1`/`target_2` to `instrument`'s tick size and
    /// recomputes `risk_reward_ratio` from the rounded prices, so a plan built from raw
    /// ATR-derived math becomes tradable at the instrument's actual price granularity.
    pub fn rounded_to(&self, instrument: &InstrumentMeta) -> Self {
        let entry = instrument.round_to_tick(self.entry);
        let stop_loss = instrument.round_to_tick(self.stop_loss);
        let target_1 = instrument.round_to_tick(self.target_1);
        let target_2 = instrument.round_to_tick(self.target_2);

        let risk = (entry - stop_loss).abs();
        let reward = (target_2 - entry).abs();
        let risk_reward_ratio = if risk > 0.0 {
            reward / risk
        } else {
            self.risk_reward_ratio
        };

        Self {
            entry,
            stop_loss,
            target_1,
            target_2,
            risk_reward_ratio,
        }
    }
}

#[cfg(test)]
mod risk_plan_tests {
    use super::*;

    #[test]
    fn test_risk_plan_rounded_to_tick() {
        let plan = RiskPlan {
            entry: 100.13,
            stop_loss: 98.77,
            target_1: 101.5,
            target_2: 103.02,
            risk_reward_ratio: 2.11,
        };
        let instrument = InstrumentMeta {
            tick_size: 0.25,
            ..InstrumentMeta::default()
        };
        let rounded = plan.rounded_to(&instrument);

        assert_eq!(rounded.entry, 100.25);
        assert_eq!(rounded.stop_loss, 98.75);
        assert_eq!(rounded.target_1, 101.5);
        assert_eq!(rounded.target_2, 103.0);

        let expected_rrr = (103.0f64 - 100.25).abs() / (100.25f64 - 98.75).abs();
        assert!((rounded.risk_reward_ratio - expected_rrr).abs() < 1e-9);
    }
}
