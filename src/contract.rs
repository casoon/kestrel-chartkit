//! Provider-neutral instrument contract specifications, currency definitions, and valuation models.
//!
//! Provides explicit contract units (multiplier, lot size, currencies) and FX conversion
//! so position sizing, notionals, and P&L can be computed consistently in an account currency.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// An ISO currency code or currency symbol identifier (e.g. "EUR", "USD", "GBP", "JPY").
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Currency(String);

impl Currency {
    /// Creates a new normalized currency identifier (trimmed, uppercase).
    pub fn new(code: impl AsRef<str>) -> Self {
        Self(code.as_ref().trim().to_uppercase())
    }

    pub fn eur() -> Self {
        Self("EUR".to_string())
    }

    pub fn usd() -> Self {
        Self("USD".to_string())
    }

    pub fn gbp() -> Self {
        Self("GBP".to_string())
    }

    pub fn chf() -> Self {
        Self("CHF".to_string())
    }

    pub fn jpy() -> Self {
        Self("JPY".to_string())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Currency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl From<&str> for Currency {
    fn from(s: &str) -> Self {
        Self::new(s)
    }
}

impl From<String> for Currency {
    fn from(s: String) -> Self {
        Self::new(s)
    }
}

/// Category of traded financial contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum InstrumentType {
    /// Standard stock / equity share. Multiplier is typically 1.0.
    Equity,
    /// Linear commodity, index, or rate future with a fixed point multiplier.
    LinearFuture,
    /// Spot or forward Foreign Exchange pair.
    Forex,
    /// Linear Contract For Difference.
    Cfd,
    /// Spot Cryptocurrency.
    CryptoSpot,
    /// Derivative financial option contract.
    Option,
}

impl InstrumentType {
    /// Returns true if this instrument uses standard linear valuation (`price * quantity * multiplier`).
    ///
    /// Inverse, quanto, or non-linear option contracts are not linear.
    pub fn is_linear(&self) -> bool {
        match self {
            Self::Equity | Self::LinearFuture | Self::Forex | Self::Cfd | Self::CryptoSpot => true,
            Self::Option => false,
        }
    }
}

/// Detailed operative contract specifications complementing [`crate::model::InstrumentMeta`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ContractSpec {
    /// Currency in which the instrument's market price is quoted (e.g. USD for AAPL, EUR for DAX).
    pub price_currency: Currency,
    /// Currency in which margin and cash settlements occur.
    pub settlement_currency: Currency,
    /// Contract point multiplier (e.g. 1.0 for equities, 25.0 for DAX future, 100.0 for SPX).
    pub multiplier: f64,
    /// Minimum increment of order quantity (lot step size, e.g. 1.0 for futures, 0.0001 for crypto).
    pub quantity_step: f64,
    /// Minimum allowed order quantity. Must be >= `quantity_step`.
    pub min_quantity: f64,
    /// The structural category of the instrument.
    pub instrument_type: InstrumentType,
}

impl Default for ContractSpec {
    fn default() -> Self {
        Self {
            price_currency: Currency::usd(),
            settlement_currency: Currency::usd(),
            multiplier: 1.0,
            quantity_step: 1.0,
            min_quantity: 1.0,
            instrument_type: InstrumentType::Equity,
        }
    }
}

/// Errors when validating a [`ContractSpec`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractSpecError {
    NonPositiveMultiplier,
    NonFiniteMultiplier,
    NonPositiveQuantityStep,
    NonFiniteQuantityStep,
    InvalidMinQuantity,
    EmptyPriceCurrency,
    EmptySettlementCurrency,
}

impl fmt::Display for ContractSpecError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NonPositiveMultiplier => f.write_str("multiplier must be > 0"),
            Self::NonFiniteMultiplier => f.write_str("multiplier must be finite"),
            Self::NonPositiveQuantityStep => f.write_str("quantity_step must be > 0"),
            Self::NonFiniteQuantityStep => f.write_str("quantity_step must be finite"),
            Self::InvalidMinQuantity => {
                f.write_str("min_quantity must be finite and >= quantity_step")
            }
            Self::EmptyPriceCurrency => f.write_str("price_currency must not be empty"),
            Self::EmptySettlementCurrency => f.write_str("settlement_currency must not be empty"),
        }
    }
}

impl std::error::Error for ContractSpecError {}

impl ContractSpec {
    /// Validates the contractual consistency of the specification.
    pub fn validate(&self) -> Result<(), ContractSpecError> {
        if !self.multiplier.is_finite() {
            return Err(ContractSpecError::NonFiniteMultiplier);
        }
        if self.multiplier <= 0.0 {
            return Err(ContractSpecError::NonPositiveMultiplier);
        }
        if !self.quantity_step.is_finite() {
            return Err(ContractSpecError::NonFiniteQuantityStep);
        }
        if self.quantity_step <= 0.0 {
            return Err(ContractSpecError::NonPositiveQuantityStep);
        }
        if !self.min_quantity.is_finite() || self.min_quantity < self.quantity_step {
            return Err(ContractSpecError::InvalidMinQuantity);
        }
        if self.price_currency.as_str().is_empty() {
            return Err(ContractSpecError::EmptyPriceCurrency);
        }
        if self.settlement_currency.as_str().is_empty() {
            return Err(ContractSpecError::EmptySettlementCurrency);
        }
        Ok(())
    }

    /// Rounds `quantity` strictly downward to the nearest integer multiple of `quantity_step`.
    ///
    /// If the resulting quantity is strictly less than `min_quantity`, returns `0.0`
    /// (no trade generated below minimum threshold).
    pub fn round_quantity_down(&self, quantity: f64) -> f64 {
        if !quantity.is_finite() || quantity <= 0.0 || self.quantity_step <= 0.0 {
            return 0.0;
        }
        let steps = (quantity / self.quantity_step + 1e-12).floor();
        let rounded = steps * self.quantity_step;
        // Use a small epsilon to avoid floating point precision edge cases (e.g. 0.99999999999 < 1.0)
        if rounded + 1e-12 < self.min_quantity {
            0.0
        } else {
            rounded
        }
    }
}

/// An FX quote between two currencies: `base_currency / quote_currency = rate`.
///
/// Example: `EUR / USD = 1.08` means 1 EUR = 1.08 USD.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FxRate {
    pub base: Currency,
    pub quote: Currency,
    pub rate: f64,
    /// Epoch timestamp of the quote in seconds or milliseconds (caller-defined convention).
    pub timestamp: i64,
}

impl FxRate {
    pub fn new(base: impl Into<Currency>, quote: impl Into<Currency>, rate: f64) -> Self {
        Self {
            base: base.into(),
            quote: quote.into(),
            rate,
            timestamp: 0,
        }
    }

    /// Converts an `amount` from `from` currency to `to` currency using this FX rate.
    ///
    /// If `from == to`, returns `Ok(amount)` without needing a valid FX quote.
    /// If missing or inverted, computes the direct or reciprocal rate.
    pub fn convert(
        &self,
        amount: f64,
        from: &Currency,
        to: &Currency,
    ) -> Result<f64, FxConversionError> {
        if from == to {
            return Ok(amount);
        }
        if !self.rate.is_finite() || self.rate <= 0.0 {
            return Err(FxConversionError::InvalidRate(self.rate));
        }

        if from == &self.base && to == &self.quote {
            // e.g. amount in EUR -> USD: amount * rate
            Ok(amount * self.rate)
        } else if from == &self.quote && to == &self.base {
            // e.g. amount in USD -> EUR: amount / rate
            Ok(amount / self.rate)
        } else {
            Err(FxConversionError::MissingPair {
                from: from.clone(),
                to: to.clone(),
            })
        }
    }
}

/// Errors during foreign exchange currency conversion.
#[derive(Debug, Clone, PartialEq)]
pub enum FxConversionError {
    InvalidRate(f64),
    MissingPair { from: Currency, to: Currency },
    StaleRate { age_seconds: i64, max_age: i64 },
}

impl fmt::Display for FxConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRate(r) => write!(f, "invalid non-positive FX rate: {r}"),
            Self::MissingPair { from, to } => {
                write!(f, "no FX rate available to convert from {from} to {to}")
            }
            Self::StaleRate {
                age_seconds,
                max_age,
            } => write!(
                f,
                "FX rate is stale: age {age_seconds}s exceeds max {max_age}s"
            ),
        }
    }
}

impl std::error::Error for FxConversionError {}

/// Errors during contract valuation and sizing calculations.
#[derive(Debug, Clone, PartialEq)]
pub enum ValuationError {
    InvalidContract(ContractSpecError),
    FxUnavailable(FxConversionError),
    NonPositivePrice(f64),
    NonFiniteInput(&'static str),
}

impl fmt::Display for ValuationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidContract(e) => write!(f, "invalid contract specification: {e}"),
            Self::FxUnavailable(e) => write!(f, "currency conversion failed: {e}"),
            Self::NonPositivePrice(p) => write!(f, "price must be positive: {p}"),
            Self::NonFiniteInput(field) => write!(f, "input {field} must be finite"),
        }
    }
}

impl std::error::Error for ValuationError {}

impl From<ContractSpecError> for ValuationError {
    fn from(e: ContractSpecError) -> Self {
        Self::InvalidContract(e)
    }
}

impl From<FxConversionError> for ValuationError {
    fn from(e: FxConversionError) -> Self {
        Self::FxUnavailable(e)
    }
}

/// Computes the total notional value of a position in account currency:
/// `quantity * price * multiplier * fx_rate`.
///
/// `fx_rate` converts from `spec.price_currency` to the account currency.
/// If `spec.price_currency == account_currency`, pass `Some(1.0)` or use [`FxRate::convert`].
/// If FX is required but unavailable, passing `None` returns [`ValuationError::FxUnavailable`].
pub fn notional_value(
    price: f64,
    quantity: f64,
    spec: &ContractSpec,
    fx_to_account: Option<f64>,
) -> Result<f64, ValuationError> {
    spec.validate()?;
    if !price.is_finite() || price <= 0.0 {
        return Err(ValuationError::NonPositivePrice(price));
    }
    if !quantity.is_finite() || quantity < 0.0 {
        return Err(ValuationError::NonFiniteInput("quantity"));
    }
    let fx = match fx_to_account {
        Some(rate) if rate.is_finite() && rate > 0.0 => rate,
        Some(invalid) => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::InvalidRate(invalid),
            ))
        }
        None => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::MissingPair {
                    from: spec.price_currency.clone(),
                    to: spec.settlement_currency.clone(),
                },
            ))
        }
    };
    Ok(quantity * price * spec.multiplier * fx)
}

/// Computes the stop risk amount in account currency:
/// `quantity * |entry - stop| * multiplier * fx_rate`.
pub fn stop_risk_amount(
    entry: f64,
    stop: f64,
    quantity: f64,
    spec: &ContractSpec,
    fx_to_account: Option<f64>,
) -> Result<f64, ValuationError> {
    spec.validate()?;
    if !entry.is_finite() || entry <= 0.0 {
        return Err(ValuationError::NonPositivePrice(entry));
    }
    if !stop.is_finite() || stop <= 0.0 {
        return Err(ValuationError::NonPositivePrice(stop));
    }
    if !quantity.is_finite() || quantity < 0.0 {
        return Err(ValuationError::NonFiniteInput("quantity"));
    }
    let fx = match fx_to_account {
        Some(rate) if rate.is_finite() && rate > 0.0 => rate,
        Some(invalid) => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::InvalidRate(invalid),
            ))
        }
        None => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::MissingPair {
                    from: spec.price_currency.clone(),
                    to: spec.settlement_currency.clone(),
                },
            ))
        }
    };
    let price_diff = (entry - stop).abs();
    Ok(quantity * price_diff * spec.multiplier * fx)
}

/// Computes the realized or unrealized P&L in account currency:
/// `(exit - entry) * direction * quantity * multiplier * fx_rate`.
pub fn contract_pnl(
    entry: f64,
    exit: f64,
    quantity: f64,
    is_long: bool,
    spec: &ContractSpec,
    fx_to_account: Option<f64>,
) -> Result<f64, ValuationError> {
    spec.validate()?;
    if !entry.is_finite() || entry <= 0.0 {
        return Err(ValuationError::NonPositivePrice(entry));
    }
    if !exit.is_finite() || exit <= 0.0 {
        return Err(ValuationError::NonPositivePrice(exit));
    }
    if !quantity.is_finite() || quantity < 0.0 {
        return Err(ValuationError::NonFiniteInput("quantity"));
    }
    let fx = match fx_to_account {
        Some(rate) if rate.is_finite() && rate > 0.0 => rate,
        Some(invalid) => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::InvalidRate(invalid),
            ))
        }
        None => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::MissingPair {
                    from: spec.price_currency.clone(),
                    to: spec.settlement_currency.clone(),
                },
            ))
        }
    };
    let diff = if is_long { exit - entry } else { entry - exit };
    Ok(diff * quantity * spec.multiplier * fx)
}

/// Computes the single-tick value in account currency:
/// `tick_size * multiplier * fx_rate`.
pub fn contract_tick_value(
    tick_size: f64,
    spec: &ContractSpec,
    fx_to_account: Option<f64>,
) -> Result<f64, ValuationError> {
    spec.validate()?;
    if !tick_size.is_finite() || tick_size <= 0.0 {
        return Err(ValuationError::NonPositivePrice(tick_size));
    }
    let fx = match fx_to_account {
        Some(rate) if rate.is_finite() && rate > 0.0 => rate,
        Some(invalid) => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::InvalidRate(invalid),
            ))
        }
        None => {
            return Err(ValuationError::FxUnavailable(
                FxConversionError::MissingPair {
                    from: spec.price_currency.clone(),
                    to: spec.settlement_currency.clone(),
                },
            ))
        }
    };
    Ok(tick_size * spec.multiplier * fx)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contract_spec_validation() {
        let default_spec = ContractSpec::default();
        assert_eq!(default_spec.validate(), Ok(()));

        let bad_multiplier = ContractSpec {
            multiplier: 0.0,
            ..ContractSpec::default()
        };
        assert_eq!(
            bad_multiplier.validate(),
            Err(ContractSpecError::NonPositiveMultiplier)
        );

        let bad_step = ContractSpec {
            quantity_step: -1.0,
            ..ContractSpec::default()
        };
        assert_eq!(
            bad_step.validate(),
            Err(ContractSpecError::NonPositiveQuantityStep)
        );

        let bad_min = ContractSpec {
            quantity_step: 10.0,
            min_quantity: 5.0,
            ..ContractSpec::default()
        };
        assert_eq!(
            bad_min.validate(),
            Err(ContractSpecError::InvalidMinQuantity)
        );
    }

    #[test]
    fn test_quantity_down_rounding() {
        let spec = ContractSpec {
            quantity_step: 0.5,
            min_quantity: 1.0,
            ..ContractSpec::default()
        };
        assert_eq!(spec.round_quantity_down(2.8), 2.5);
        assert_eq!(spec.round_quantity_down(1.0), 1.0);
        // Strictly below min_quantity -> 0.0 (no trade)
        assert_eq!(spec.round_quantity_down(0.9), 0.0);
        assert_eq!(spec.round_quantity_down(0.5), 0.0);
    }

    #[test]
    fn test_fx_conversion() {
        let fx = FxRate::new("EUR", "USD", 1.08);

        // Same currency -> identity without rate check
        assert_eq!(
            fx.convert(100.0, &Currency::eur(), &Currency::eur())
                .unwrap(),
            100.0
        );

        // Direct EUR -> USD
        let usd = fx
            .convert(100.0, &Currency::eur(), &Currency::usd())
            .unwrap();
        assert!((usd - 108.0).abs() < 1e-9);

        // Reciprocal USD -> EUR
        let eur = fx
            .convert(108.0, &Currency::usd(), &Currency::eur())
            .unwrap();
        assert!((eur - 100.0).abs() < 1e-9);

        // Missing pair
        let err = fx.convert(100.0, &Currency::gbp(), &Currency::usd());
        assert!(matches!(err, Err(FxConversionError::MissingPair { .. })));
    }
}
