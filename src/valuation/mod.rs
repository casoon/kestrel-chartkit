//! A shared valuation context: when we are valuing, on which market data, and with which curves.
//!
//! A single constant interest rate is enough to discount one cashflow, and that is what
//! [`crate::finance::discount_factor`] does. It is not a market: real term structures differ by
//! maturity, differ again between discounting and projecting forward rates, and differ per
//! currency. This module holds those pieces together so a valuation is reproducible rather than
//! assembled from whatever constants a caller had at hand.
//!
//! (Dieser Modul-Doc-Kommentar verwendet voll qualifizierte `crate::`-Pfade: rustdoc löst
//! Intra-Doc-Links hier gegen den Crate-Wurzel-Scope auf, weil `pub mod valuation;` in `lib.rs`
//! einen eigenen `///`-Kommentar trägt, der mit diesem zu einem Block verschmilzt — dieselbe
//! Lage wie in `applicability.rs`.)
//!
//! Three rules run through it:
//!
//! * **Missing data is an error, never a default.** A currency without a curve does not silently
//!   become a zero rate; asking for it fails.
//! * **Discounting and projecting are different roles**, so [`DiscountCurve`] and [`ForwardCurve`]
//!   are different types even though they share the same term structure representation. Passing
//!   one where the other belongs will not compile.
//! * **A number carries its inputs.** Every valuation returns a [`Valued`] with the
//!   [`ValuationStamp`] it was produced under, so a result found later can be traced back to the
//!   data snapshot that produced it.
//!
//! [`crate::valuation::portfolio`] builds on this: positions valued by model, revalued under market scenarios,
//! and aggregated in account currency.
//!
//! [`crate::valuation::bootstrap`] builds a curve from quoted instruments instead of from given zero rates,
//! and [`crate::valuation::volatility`] holds volatility over strike and maturity.

pub mod bootstrap;
pub mod portfolio;
pub mod volatility;

use std::collections::HashMap;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::contract::{Currency, FxRate};
use crate::finance::{year_fraction, Date, DayCountConvention, FinanceError, FixedRateBond};
use crate::option::{black_scholes_merton, BlackScholesInputs, OptionError, OptionType};

/// Why a valuation in a [`ValuationContext`] could not be produced.
///
/// Distinct from [`crate::contract::ValuationError`], which is about a single contract's own
/// arithmetic; this one is about the market data a valuation runs on.
#[derive(Debug, Clone, PartialEq)]
pub enum ValuationContextError {
    /// No discount curve for this currency. The context does not invent one.
    MissingDiscountCurve(Currency),
    /// No forward curve for this currency.
    MissingForwardCurve(Currency),
    /// No volatility surface for this underlying.
    MissingVolatilitySurface(String),
    /// No exchange rate connecting these two currencies.
    MissingFxRate { from: Currency, to: Currency },
    /// A curve was asked for a date before its reference date, or for a non-finite time.
    TimeOutsideCurve,
    /// The curve's nodes do not describe a term structure.
    InvalidCurve(&'static str),
    /// The underlying instrument or its schedule is invalid.
    Instrument(FinanceError),
    /// The option inputs are invalid.
    Option(OptionError),
    /// A scenario's shocks are not usable.
    InvalidScenario(&'static str),
    /// The instrument's exercise style has no pricing engine in this crate.
    UnsupportedExercise(crate::option::OptionStyle),
    /// The requested strike or maturity lies further outside the quoted volatility grid than its
    /// declared validity allows.
    OutsideSurfaceValidity,
}

impl fmt::Display for ValuationContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingDiscountCurve(currency) => {
                write!(f, "no discount curve for {currency}")
            }
            Self::MissingForwardCurve(currency) => {
                write!(f, "no forward curve for {currency}")
            }
            Self::MissingVolatilitySurface(symbol) => {
                write!(f, "no volatility surface for {symbol}")
            }
            Self::MissingFxRate { from, to } => {
                write!(f, "no fx rate from {from} to {to}")
            }
            Self::TimeOutsideCurve => f.write_str("date lies before the curve's reference date"),
            Self::InvalidCurve(reason) => write!(f, "invalid curve: {reason}"),
            Self::Instrument(err) => write!(f, "invalid instrument: {err}"),
            Self::Option(err) => write!(f, "invalid option inputs: {err:?}"),
            Self::InvalidScenario(reason) => write!(f, "invalid scenario: {reason}"),
            Self::OutsideSurfaceValidity => {
                f.write_str("strike or maturity lies outside the volatility surface's validity")
            }
            Self::UnsupportedExercise(style) => write!(
                f,
                "no pricing engine for {style:?} exercise; only European options are valued here"
            ),
        }
    }
}

impl std::error::Error for ValuationContextError {}

impl From<FinanceError> for ValuationContextError {
    fn from(err: FinanceError) -> Self {
        Self::Instrument(err)
    }
}

impl From<OptionError> for ValuationContextError {
    fn from(err: OptionError) -> Self {
        Self::Option(err)
    }
}

/// A term structure of continuously compounded zero rates.
///
/// Between nodes the zero rate is linear in time; outside them it is held flat at the first
/// respectively last node's rate. Flat extrapolation is a convention, not a derivation — it is
/// stated here rather than left implicit, because the alternative (continuing the slope) produces
/// nonsensical discount factors a few years past the last node.
///
/// Continuously compounded zero rates are the representation because they stay well behaved when
/// rates are negative: the discount factor `exp(-z * t)` is positive for every real `z`, and a
/// flat curve is exactly the single-node case, so the constant-rate results this crate produced
/// before remain reproducible.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct YieldCurve {
    reference: Date,
    day_count: DayCountConvention,
    /// `(time in years from the reference date, continuously compounded zero rate)`, ascending.
    nodes: Vec<(f64, f64)>,
}

impl YieldCurve {
    /// A curve with one rate for every maturity — the compatible special case.
    pub fn flat(reference: Date, rate: f64, day_count: DayCountConvention) -> Self {
        Self {
            reference,
            day_count,
            nodes: vec![(0.0, rate)],
        }
    }

    /// A curve through the given `(time, zero rate)` nodes.
    ///
    /// Times are years from `reference` and must be finite, non-negative and strictly ascending;
    /// rates must be finite. At least one node is required.
    pub fn from_zero_rates(
        reference: Date,
        nodes: Vec<(f64, f64)>,
        day_count: DayCountConvention,
    ) -> Result<Self, ValuationContextError> {
        if nodes.is_empty() {
            return Err(ValuationContextError::InvalidCurve(
                "a curve needs at least one node",
            ));
        }
        if nodes
            .iter()
            .any(|(t, r)| !t.is_finite() || *t < 0.0 || !r.is_finite())
        {
            return Err(ValuationContextError::InvalidCurve(
                "node times must be finite and non-negative, rates finite",
            ));
        }
        if nodes.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(ValuationContextError::InvalidCurve(
                "node times must be strictly ascending",
            ));
        }
        Ok(Self {
            reference,
            day_count,
            nodes,
        })
    }

    pub fn reference_date(&self) -> Date {
        self.reference
    }

    pub fn day_count(&self) -> DayCountConvention {
        self.day_count
    }

    pub fn nodes(&self) -> &[(f64, f64)] {
        &self.nodes
    }

    /// Years from the reference date to `date`, under the curve's own day count.
    pub fn time_to(&self, date: Date) -> Result<f64, ValuationContextError> {
        if date < self.reference {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        Ok(year_fraction(self.reference, date, self.day_count))
    }

    /// Continuously compounded zero rate for maturity `t`, in years.
    pub fn zero_rate(&self, t: f64) -> Result<f64, ValuationContextError> {
        if !t.is_finite() || t < 0.0 {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        let first = self.nodes[0];
        if t <= first.0 {
            return Ok(first.1);
        }
        let last = self.nodes[self.nodes.len() - 1];
        if t >= last.0 {
            return Ok(last.1);
        }
        let index = self
            .nodes
            .partition_point(|(node_t, _)| *node_t <= t)
            .max(1);
        let (t0, r0) = self.nodes[index - 1];
        let (t1, r1) = self.nodes[index];
        let weight = (t - t0) / (t1 - t0);
        Ok(r0 + weight * (r1 - r0))
    }

    /// Discount factor for maturity `t`, in years: `exp(-z(t) * t)`.
    pub fn discount_factor_at(&self, t: f64) -> Result<f64, ValuationContextError> {
        Ok((-self.zero_rate(t)? * t).exp())
    }

    /// Discount factor for a calendar date.
    pub fn discount_factor(&self, date: Date) -> Result<f64, ValuationContextError> {
        self.discount_factor_at(self.time_to(date)?)
    }

    /// The same curve with every zero rate shifted by `delta`, in absolute rate units — a
    /// parallel shift. `0.0001` is one basis point.
    ///
    /// Parallel is the only shape offered here: a twist or a steepening needs a statement about
    /// which part of the curve moves how, and that belongs to whoever has that view.
    pub fn shifted(&self, delta: f64) -> Self {
        Self {
            reference: self.reference,
            day_count: self.day_count,
            nodes: self
                .nodes
                .iter()
                .map(|(t, rate)| (*t, rate + delta))
                .collect(),
        }
    }

    /// Continuously compounded forward rate covering `t1..t2`, from the same term structure:
    /// `(z(t2) * t2 - z(t1) * t1) / (t2 - t1)`.
    pub fn forward_rate(&self, t1: f64, t2: f64) -> Result<f64, ValuationContextError> {
        if !t2.is_finite() || t2 <= t1 {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        let (z1, z2) = (self.zero_rate(t1)?, self.zero_rate(t2)?);
        Ok((z2 * t2 - z1 * t1) / (t2 - t1))
    }
}

/// The curve money is discounted on.
///
/// A separate type from [`ForwardCurve`] on purpose: the two answer different questions and are
/// not interchangeable, even when a market happens to use the same numbers for both.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DiscountCurve(YieldCurve);

impl DiscountCurve {
    pub fn new(curve: YieldCurve) -> Self {
        Self(curve)
    }

    pub fn curve(&self) -> &YieldCurve {
        &self.0
    }

    pub fn discount_factor(&self, date: Date) -> Result<f64, ValuationContextError> {
        self.0.discount_factor(date)
    }
}

/// The curve future rates are projected from.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ForwardCurve(YieldCurve);

impl ForwardCurve {
    pub fn new(curve: YieldCurve) -> Self {
        Self(curve)
    }

    pub fn curve(&self) -> &YieldCurve {
        &self.0
    }

    pub fn forward_rate(&self, t1: f64, t2: f64) -> Result<f64, ValuationContextError> {
        self.0.forward_rate(t1, t2)
    }
}

/// Which inputs a valuation was produced from.
///
/// Carried out with every result so a number found in a report months later can be traced to the
/// data it came from. `as_of` is the market data snapshot, which is not the same as the valuation
/// date: revaluing an old date on today's data and on that day's data are different runs, and the
/// stamp is what tells them apart.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValuationStamp {
    /// The date being valued.
    pub valuation_date: Date,
    /// Unix timestamp of the market data snapshot the context was built from.
    pub as_of: i64,
    /// Caller-defined identifier of that input set — a snapshot id, a dataset version, a commit.
    pub data_version: String,
}

/// A value together with the inputs it was produced from.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Valued<T> {
    pub value: T,
    pub stamp: ValuationStamp,
}

impl<T> Valued<T> {
    pub fn into_inner(self) -> T {
        self.value
    }
}

/// A bond valued against a curve rather than against a single yield.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BondCurveValuation {
    /// Present value of every outstanding cashflow.
    pub dirty_price: f64,
    /// `dirty_price` minus accrued interest.
    pub clean_price: f64,
    pub accrued_interest: f64,
}

/// When we are valuing, on which market data, and with which curves.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValuationContext {
    valuation_date: Date,
    as_of: i64,
    data_version: String,
    discount_curves: HashMap<Currency, DiscountCurve>,
    forward_curves: HashMap<Currency, ForwardCurve>,
    volatility_surfaces: HashMap<String, volatility::VolatilitySurface>,
    fx_rates: Vec<FxRate>,
}

impl ValuationContext {
    /// An empty context. Curves and rates are added explicitly; nothing is assumed.
    pub fn new(valuation_date: Date, as_of: i64, data_version: impl Into<String>) -> Self {
        Self {
            valuation_date,
            as_of,
            data_version: data_version.into(),
            discount_curves: HashMap::new(),
            forward_curves: HashMap::new(),
            volatility_surfaces: HashMap::new(),
            fx_rates: Vec::new(),
        }
    }

    pub fn with_discount_curve(mut self, currency: Currency, curve: DiscountCurve) -> Self {
        self.discount_curves.insert(currency, curve);
        self
    }

    pub fn with_forward_curve(mut self, currency: Currency, curve: ForwardCurve) -> Self {
        self.forward_curves.insert(currency, curve);
        self
    }

    /// Adds a volatility surface for one underlying.
    ///
    /// Keyed by the underlying's symbol rather than by currency: two instruments quoted in the
    /// same currency have their own smiles, and sharing one between them would be a statement
    /// about the market that nobody made.
    pub fn with_volatility_surface(
        mut self,
        symbol: impl Into<String>,
        surface: volatility::VolatilitySurface,
    ) -> Self {
        self.volatility_surfaces.insert(symbol.into(), surface);
        self
    }

    pub fn volatility_surface(
        &self,
        symbol: &str,
    ) -> Result<&volatility::VolatilitySurface, ValuationContextError> {
        self.volatility_surfaces
            .get(symbol)
            .ok_or_else(|| ValuationContextError::MissingVolatilitySurface(symbol.to_string()))
    }

    pub fn with_fx_rate(mut self, rate: FxRate) -> Self {
        self.fx_rates.push(rate);
        self
    }

    pub fn valuation_date(&self) -> Date {
        self.valuation_date
    }

    /// The inputs this context stands for.
    pub fn stamp(&self) -> ValuationStamp {
        ValuationStamp {
            valuation_date: self.valuation_date,
            as_of: self.as_of,
            data_version: self.data_version.clone(),
        }
    }

    fn valued<T>(&self, value: T) -> Valued<T> {
        Valued {
            value,
            stamp: self.stamp(),
        }
    }

    pub fn discount_curve(
        &self,
        currency: &Currency,
    ) -> Result<&DiscountCurve, ValuationContextError> {
        self.discount_curves
            .get(currency)
            .ok_or_else(|| ValuationContextError::MissingDiscountCurve(currency.clone()))
    }

    pub fn forward_curve(
        &self,
        currency: &Currency,
    ) -> Result<&ForwardCurve, ValuationContextError> {
        self.forward_curves
            .get(currency)
            .ok_or_else(|| ValuationContextError::MissingForwardCurve(currency.clone()))
    }

    /// Converts an amount between currencies using the rates this context holds.
    ///
    /// Direct and inverse quotations both count; no cross rates are constructed through a third
    /// currency, because which currency to route through is a decision this crate has no basis
    /// for making.
    pub fn convert(
        &self,
        amount: f64,
        from: &Currency,
        to: &Currency,
    ) -> Result<f64, ValuationContextError> {
        if from == to {
            return Ok(amount);
        }
        self.fx_rates
            .iter()
            .find_map(|rate| rate.convert(amount, from, to).ok())
            .ok_or_else(|| ValuationContextError::MissingFxRate {
                from: from.clone(),
                to: to.clone(),
            })
    }

    /// Present value of a bond's outstanding cashflows on the currency's discount curve.
    ///
    /// This is the curve-based counterpart to [`FixedRateBond::price`], which discounts at a
    /// single yield. Accrued interest is the same figure in both — it comes from the schedule,
    /// not from the discounting.
    pub fn price_bond(
        &self,
        bond: &FixedRateBond,
        currency: &Currency,
    ) -> Result<Valued<BondCurveValuation>, ValuationContextError> {
        self.price_bond_shifted(bond, currency, 0.0)
    }

    /// [`ValuationContext::price_bond`] with every zero rate shifted in parallel by `rate_shift`
    /// (absolute rate units). The scenario machinery in [`portfolio`] uses this; a shift of zero
    /// is the unshocked case.
    pub fn price_bond_shifted(
        &self,
        bond: &FixedRateBond,
        currency: &Currency,
        rate_shift: f64,
    ) -> Result<Valued<BondCurveValuation>, ValuationContextError> {
        let curve = self.discount_curve(currency)?.curve().shifted(rate_shift);
        let flows = bond.cashflows(self.valuation_date);
        if flows.is_empty() {
            return Err(ValuationContextError::Instrument(
                FinanceError::InvalidInput("no cashflows remain after the valuation date"),
            ));
        }

        let mut dirty_price = 0.0;
        for flow in &flows {
            dirty_price += flow.amount * curve.discount_factor(flow.date)?;
        }
        let accrued_interest = bond.accrued_interest(self.valuation_date);

        Ok(self.valued(BondCurveValuation {
            dirty_price,
            clean_price: dirty_price - accrued_interest,
            accrued_interest,
        }))
    }

    /// Prices a European option, taking the interest rate from the currency's discount curve.
    ///
    /// The rate used is the zero rate to expiry. For a European option that is not an
    /// approximation: only the discount factor to expiry and the forward enter the formula, and
    /// both follow from that single rate.
    ///
    /// Volatility and dividend yield are passed in rather than read from the context: a
    /// volatility surface over strike and maturity is not modelled yet, and inventing a flat one
    /// here would hide that.
    #[allow(clippy::too_many_arguments)]
    pub fn price_european_option(
        &self,
        currency: &Currency,
        option_type: OptionType,
        spot: f64,
        strike: f64,
        expiry: Date,
        volatility: f64,
        dividend_yield: f64,
    ) -> Result<Valued<crate::option::OptionPricingResult>, ValuationContextError> {
        self.price_european_option_shifted(
            currency,
            option_type,
            spot,
            strike,
            expiry,
            volatility,
            dividend_yield,
            0.0,
        )
    }

    /// Prices a European option taking *both* the rate and the volatility from the context: the
    /// rate from the currency's discount curve, the volatility from the underlying's surface at
    /// this option's own strike and expiry.
    ///
    /// The surface is what makes this different from
    /// [`ValuationContext::price_european_option`], which takes a volatility from the caller. A
    /// strike or expiry outside the surface's declared validity fails here rather than being
    /// answered with the nearest quoted value.
    #[allow(clippy::too_many_arguments)]
    pub fn price_european_option_on_surface(
        &self,
        symbol: &str,
        currency: &Currency,
        option_type: OptionType,
        spot: f64,
        strike: f64,
        expiry: Date,
        dividend_yield: f64,
    ) -> Result<Valued<crate::option::OptionPricingResult>, ValuationContextError> {
        let volatility = self
            .volatility_surface(symbol)?
            .volatility(expiry, strike)?;
        self.price_european_option(
            currency,
            option_type,
            spot,
            strike,
            expiry,
            volatility,
            dividend_yield,
        )
    }

    /// [`ValuationContext::price_european_option`] with the curve shifted in parallel by
    /// `rate_shift` (absolute rate units).
    #[allow(clippy::too_many_arguments)]
    pub fn price_european_option_shifted(
        &self,
        currency: &Currency,
        option_type: OptionType,
        spot: f64,
        strike: f64,
        expiry: Date,
        volatility: f64,
        dividend_yield: f64,
        rate_shift: f64,
    ) -> Result<Valued<crate::option::OptionPricingResult>, ValuationContextError> {
        let curve = self.discount_curve(currency)?.curve().shifted(rate_shift);
        if expiry < self.valuation_date {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        let time_to_expiry_years = year_fraction(self.valuation_date, expiry, curve.day_count());
        let risk_free_rate = curve.zero_rate(time_to_expiry_years)?;

        let priced = black_scholes_merton(
            option_type,
            &BlackScholesInputs {
                spot,
                strike,
                time_to_expiry_years,
                risk_free_rate,
                dividend_yield,
                volatility,
            },
        )?;
        Ok(self.valued(priced))
    }
}
