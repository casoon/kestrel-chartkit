//! Positions valued by model, revalued under market scenarios, aggregated in account currency.
//!
//! [`crate::portfolio::evaluate_portfolio`] answers a different question: it takes a price per
//! position and works out exposure and stop risk from it. That is the right tool when a price is
//! all there is. It cannot revalue an option after a volatility move or a bond after a rate move,
//! because a percentage shock to a price is not a repricing of a non-linear instrument.
//!
//! Here a position says *what it is*, and the value follows from a [`ValuationContext`]. A
//! scenario shocks the market data, not the results, and everything is then computed again from
//! scratch — which is also where the sensitivities come from: each one is the portfolio revalued
//! under a defined, named move. They are therefore consistent with the valuation by construction,
//! summable in account currency across instrument types, and cannot drift apart from the prices
//! they belong to.

use crate::contract::Currency;
use crate::finance::{Date, FixedRateBond};
use crate::option::{OptionStyle, OptionType};
use crate::portfolio::PositionSide;

use super::{ValuationContext, ValuationContextError, ValuationStamp, Valued};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// What a position holds, in enough detail to value it.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ValuedInstrument {
    /// Valued straight from its price: `price * multiplier` per unit. Equities, futures, CFDs.
    Linear {
        price: f64,
        /// Contract point multiplier, `1.0` for a plain share.
        multiplier: f64,
    },
    /// A European option on a spot underlying, valued with Black-Scholes-Merton.
    ///
    /// `style` is carried so an American contract can be *refused* rather than quietly valued as
    /// if early exercise were worthless. There is no American engine in this crate, and pricing
    /// one with a European formula would understate it without saying so.
    EuropeanOption {
        option_type: OptionType,
        style: OptionStyle,
        spot: f64,
        strike: f64,
        expiry: Date,
        volatility: f64,
        dividend_yield: f64,
        /// Underlying units per contract.
        contract_size: f64,
    },
    /// A fixed-rate bond, valued by discounting its remaining cashflows on the currency's curve.
    ///
    /// The value is the dirty price for one bond of the schedule's face value.
    Bond { bond: Box<FixedRateBond> },
}

/// A position to be valued: what it is, how much of it, and in which currency it trades.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ValuationPosition {
    pub symbol: String,
    pub currency: Currency,
    pub side: PositionSide,
    /// Absolute size; direction comes from `side`.
    pub quantity: f64,
    pub instrument: ValuedInstrument,
}

impl ValuationPosition {
    pub fn new(
        symbol: impl Into<String>,
        currency: Currency,
        side: PositionSide,
        quantity: f64,
        instrument: ValuedInstrument,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            currency,
            side,
            quantity,
            instrument,
        }
    }

    fn signed_quantity(&self) -> f64 {
        match self.side {
            PositionSide::Long => self.quantity,
            PositionSide::Short => -self.quantity,
        }
    }
}

/// Which model produced a position's value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ValuationModel {
    /// Price times multiplier.
    Linear,
    /// Black-Scholes-Merton with the zero rate to expiry.
    BlackScholesMerton,
    /// Cashflows discounted on the currency's curve.
    DiscountedCashflows,
}

/// Shocks applied to the market data before revaluing. Every field is neutral at its zero value,
/// and the unit is part of the field, not of the caller's memory.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarketScenario {
    /// Parallel shift of every zero rate, in absolute rate units: `0.0001` is one basis point.
    ///
    /// Absolute rather than relative, because a relative shift does nothing at a zero rate and
    /// points the wrong way at a negative one.
    pub rate_shift: f64,
    /// Relative move of every underlying spot price: `-0.10` is minus ten percent.
    pub underlying_shock_pct: f64,
    /// Absolute shift of every volatility, in volatility points: `0.05` moves 20% vol to 25%.
    /// A shifted volatility is floored at zero.
    pub volatility_shift: f64,
    /// Relative move of every foreign currency against the account currency: `-0.05` makes
    /// foreign holdings worth five percent less in account currency.
    pub fx_shock_pct: f64,
}

impl Default for MarketScenario {
    fn default() -> Self {
        Self::neutral()
    }
}

impl MarketScenario {
    /// No shocks. Revaluing under it must reproduce the base valuation exactly.
    pub fn neutral() -> Self {
        Self {
            rate_shift: 0.0,
            underlying_shock_pct: 0.0,
            volatility_shift: 0.0,
            fx_shock_pct: 0.0,
        }
    }

    pub fn with_rate_shift(mut self, shift: f64) -> Self {
        self.rate_shift = shift;
        self
    }

    pub fn with_underlying_shock(mut self, pct: f64) -> Self {
        self.underlying_shock_pct = pct;
        self
    }

    pub fn with_volatility_shift(mut self, shift: f64) -> Self {
        self.volatility_shift = shift;
        self
    }

    pub fn with_fx_shock(mut self, pct: f64) -> Self {
        self.fx_shock_pct = pct;
        self
    }

    pub fn validate(&self) -> Result<(), ValuationContextError> {
        let finite = self.rate_shift.is_finite()
            && self.underlying_shock_pct.is_finite()
            && self.volatility_shift.is_finite()
            && self.fx_shock_pct.is_finite();
        if !finite {
            return Err(ValuationContextError::InvalidScenario(
                "every shock must be finite",
            ));
        }
        if self.underlying_shock_pct < -1.0 {
            return Err(ValuationContextError::InvalidScenario(
                "underlying_shock_pct must be >= -1.0",
            ));
        }
        if self.fx_shock_pct < -1.0 {
            return Err(ValuationContextError::InvalidScenario(
                "fx_shock_pct must be >= -1.0",
            ));
        }
        Ok(())
    }
}

/// One position's value under one scenario.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PositionValuation {
    pub symbol: String,
    pub currency: Currency,
    /// Positive for long, negative for short.
    pub signed_quantity: f64,
    /// Value of one unit, in the position's own currency.
    pub unit_value: f64,
    /// `unit_value * signed_quantity`, in the position's own currency.
    pub value: f64,
    /// The same value in account currency.
    pub value_account: f64,
    pub model: ValuationModel,
}

/// A portfolio's value under one scenario.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PortfolioValuation {
    pub positions: Vec<PositionValuation>,
    pub account_currency: Currency,
    pub total_value_account: f64,
}

/// Base value, scenario value, and the difference — plus the sensitivities, each of which is
/// itself a revaluation under a named move.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PortfolioScenarioResult {
    pub base: PortfolioValuation,
    pub stressed: PortfolioValuation,
    /// `stressed - base`, in account currency.
    pub pnl_account: f64,
    pub sensitivities: PortfolioSensitivities,
}

/// Value changes under one defined move each, all in account currency and all measured by
/// revaluing rather than by a closed-form derivative.
///
/// They are differences, not derivatives: a large move is not the small move scaled up, and for a
/// non-linear position the two differ — which is the honest answer, since that difference is what
/// convexity is.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PortfolioSensitivities {
    /// Change if every underlying rises one percent.
    pub underlying_up_1pct: f64,
    /// Change if every volatility rises one point (e.g. 20% to 21%).
    pub volatility_up_1pt: f64,
    /// Change if every zero rate rises one basis point.
    pub rate_up_1bp: f64,
    /// Change if every foreign currency gains one percent against the account currency.
    pub fx_up_1pct: f64,
}

impl ValuationContext {
    /// Values one position under a scenario.
    pub fn value_position(
        &self,
        position: &ValuationPosition,
        account_currency: &Currency,
        scenario: &MarketScenario,
    ) -> Result<PositionValuation, ValuationContextError> {
        scenario.validate()?;

        let (unit_value, model) = match &position.instrument {
            ValuedInstrument::Linear { price, multiplier } => {
                let shocked = price * (1.0 + scenario.underlying_shock_pct);
                (shocked * multiplier, ValuationModel::Linear)
            }
            ValuedInstrument::EuropeanOption {
                option_type,
                style,
                spot,
                strike,
                expiry,
                volatility,
                dividend_yield,
                contract_size,
            } => {
                if *style != OptionStyle::European {
                    return Err(ValuationContextError::UnsupportedExercise(*style));
                }
                let priced = self.price_european_option_shifted(
                    &position.currency,
                    *option_type,
                    spot * (1.0 + scenario.underlying_shock_pct),
                    *strike,
                    *expiry,
                    (volatility + scenario.volatility_shift).max(0.0),
                    *dividend_yield,
                    scenario.rate_shift,
                )?;
                (
                    priced.value.price * contract_size,
                    ValuationModel::BlackScholesMerton,
                )
            }
            ValuedInstrument::Bond { bond } => {
                let priced =
                    self.price_bond_shifted(bond, &position.currency, scenario.rate_shift)?;
                (
                    priced.value.dirty_price,
                    ValuationModel::DiscountedCashflows,
                )
            }
        };

        let signed_quantity = position.signed_quantity();
        let value = unit_value * signed_quantity;
        let converted = self.convert(value, &position.currency, account_currency)?;
        let value_account = if position.currency == *account_currency {
            converted
        } else {
            converted * (1.0 + scenario.fx_shock_pct)
        };

        Ok(PositionValuation {
            symbol: position.symbol.clone(),
            currency: position.currency.clone(),
            signed_quantity,
            unit_value,
            value,
            value_account,
            model,
        })
    }

    /// Values every position under one scenario and sums them in account currency.
    pub fn value_portfolio(
        &self,
        positions: &[ValuationPosition],
        account_currency: &Currency,
        scenario: &MarketScenario,
    ) -> Result<Valued<PortfolioValuation>, ValuationContextError> {
        let mut valued = Vec::with_capacity(positions.len());
        let mut total = 0.0;
        for position in positions {
            let one = self.value_position(position, account_currency, scenario)?;
            total += one.value_account;
            valued.push(one);
        }

        Ok(Valued {
            value: PortfolioValuation {
                positions: valued,
                account_currency: account_currency.clone(),
                total_value_account: total,
            },
            stamp: self.stamp(),
        })
    }

    /// Values the portfolio as it stands and under `scenario`, and measures the sensitivities by
    /// revaluing under one defined move each.
    pub fn stress_portfolio(
        &self,
        positions: &[ValuationPosition],
        account_currency: &Currency,
        scenario: &MarketScenario,
    ) -> Result<Valued<PortfolioScenarioResult>, ValuationContextError> {
        let base = self.value_portfolio(positions, account_currency, &MarketScenario::neutral())?;
        let stressed = self.value_portfolio(positions, account_currency, scenario)?;

        let base_total = base.value.total_value_account;
        let measure = |scenario: MarketScenario| -> Result<f64, ValuationContextError> {
            Ok(self
                .value_portfolio(positions, account_currency, &scenario)?
                .value
                .total_value_account
                - base_total)
        };

        let sensitivities = PortfolioSensitivities {
            underlying_up_1pct: measure(MarketScenario::neutral().with_underlying_shock(0.01))?,
            volatility_up_1pt: measure(MarketScenario::neutral().with_volatility_shift(0.01))?,
            rate_up_1bp: measure(MarketScenario::neutral().with_rate_shift(0.0001))?,
            fx_up_1pct: measure(MarketScenario::neutral().with_fx_shock(0.01))?,
        };

        Ok(Valued {
            value: PortfolioScenarioResult {
                pnl_account: stressed.value.total_value_account - base_total,
                base: base.value,
                stressed: stressed.value,
                sensitivities,
            },
            stamp: self.stamp(),
        })
    }
}

/// One named sensitivity, with the move it measures.
///
/// The kind is part of the result rather than a string a consumer invents: a report that labels
/// "rate" without saying *how much* rate has labelled nothing, and a reader who assumes basis
/// points where percent was meant is off by four orders of magnitude.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum SensitivityKind {
    UnderlyingUpOnePercent,
    VolatilityUpOnePoint,
    RateUpOneBasisPoint,
    FxUpOnePercent,
}

impl SensitivityKind {
    /// The move this sensitivity is the answer to, ready to put next to the number.
    pub fn described_move(&self) -> &'static str {
        match self {
            Self::UnderlyingUpOnePercent => "every underlying +1%",
            Self::VolatilityUpOnePoint => "every volatility +1 point",
            Self::RateUpOneBasisPoint => "every zero rate +1 bp",
            Self::FxUpOnePercent => "every foreign currency +1% against the account currency",
        }
    }
}

impl PortfolioSensitivities {
    /// The four sensitivities with their kinds, for a consumer that renders them as a list.
    pub fn entries(&self) -> [(SensitivityKind, f64); 4] {
        [
            (
                SensitivityKind::UnderlyingUpOnePercent,
                self.underlying_up_1pct,
            ),
            (
                SensitivityKind::VolatilityUpOnePoint,
                self.volatility_up_1pt,
            ),
            (SensitivityKind::RateUpOneBasisPoint, self.rate_up_1bp),
            (SensitivityKind::FxUpOnePercent, self.fx_up_1pct),
        ]
    }
}

/// What a consumer carries onward from a valuation.
///
/// The pieces exist separately — [`Valued`], [`PortfolioScenarioResult`], [`ValuationModel`] —
/// and a report could assemble them itself. This type is the assembled form, so that every
/// consumer assembles it the same way and none of them re-derives what a number means.
///
/// What the contract says, and what a consumer must not do with it:
///
/// * Every figure is in `account_currency`, at the [`ValuationStamp`] given. A number without its
///   stamp cannot be traced back to the data it came from, so they travel together.
/// * `positions` carries each position's [`ValuationModel`]. A display that puts a linearly
///   valued position next to a model-valued one without distinction blurs exactly what the
///   valuation was for.
/// * `sensitivities` are *differences* under the named moves, not derivatives. Scaling one up to
///   a larger move is not permitted: for a non-linear position the two disagree, and that
///   disagreement is the convexity the number was supposed to expose.
/// * A valuation this crate does not support comes back as an error, never as a zero. A consumer
///   that renders a missing value as `0.00` has reported a position worth nothing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PortfolioReport {
    pub stamp: ValuationStamp,
    pub account_currency: Currency,
    /// Value before the scenario.
    pub base_value_account: f64,
    /// Value under the scenario.
    pub scenario_value_account: f64,
    /// `scenario_value_account - base_value_account`.
    pub pnl_account: f64,
    /// Position level of the base valuation, each with the model that produced it.
    pub positions: Vec<PositionValuation>,
    pub sensitivities: Vec<(SensitivityKind, f64)>,
}

impl PortfolioReport {
    /// Flattens a scenario result into the consumer-facing form.
    pub fn from_scenario(result: &Valued<PortfolioScenarioResult>) -> Self {
        Self {
            stamp: result.stamp.clone(),
            account_currency: result.value.base.account_currency.clone(),
            base_value_account: result.value.base.total_value_account,
            scenario_value_account: result.value.stressed.total_value_account,
            pnl_account: result.value.pnl_account,
            positions: result.value.base.positions.clone(),
            sensitivities: result.value.sensitivities.entries().to_vec(),
        }
    }
}
