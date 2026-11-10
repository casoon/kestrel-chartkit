//! European vanilla option pricing, analytical Greeks, and implied volatility solver.
//!
//! Implements Black-Scholes-Merton (BSM) for spot/equity with continuous dividend yield,
//! Black-76 for futures/forwards, analytical Greeks, put-call parity verification,
//! and robust root-finding for implied volatility.
//!
//! The result of a position held to expiry needs no model: [`option_expiry_result`],
//! [`option_expiry_profile`] and [`option_expiry_series`] give it for a call or put, bought or
//! sold, with the premium and the contract size.

use std::f64::consts::PI;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::portfolio::PositionSide;

/// Type of option contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum OptionType {
    Call,
    Put,
}

/// Exercise style of the option.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum OptionStyle {
    #[default]
    European,
    American,
}

/// Core inputs required for Black-Scholes-Merton pricing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BlackScholesInputs {
    /// Underlying spot price (must be positive).
    pub spot: f64,
    /// Option strike price (must be positive).
    pub strike: f64,
    /// Time to expiration in years (must be non-negative).
    pub time_to_expiry_years: f64,
    /// Continuous risk-free interest rate (e.g. 0.05 for 5%).
    pub risk_free_rate: f64,
    /// Continuous dividend yield or foreign interest rate (e.g. 0.02 for 2%).
    pub dividend_yield: f64,
    /// Annualized volatility (must be non-negative, e.g. 0.20 for 20%).
    pub volatility: f64,
}

impl BlackScholesInputs {
    /// Validates the input parameters.
    pub fn validate(&self) -> Result<(), OptionError> {
        if !self.spot.is_finite() || self.spot <= 0.0 {
            return Err(OptionError::InvalidInput(
                "spot price must be positive and finite",
            ));
        }
        if !self.strike.is_finite() || self.strike <= 0.0 {
            return Err(OptionError::InvalidInput(
                "strike price must be positive and finite",
            ));
        }
        if !self.time_to_expiry_years.is_finite() || self.time_to_expiry_years < 0.0 {
            return Err(OptionError::InvalidInput(
                "time to expiry must be non-negative and finite",
            ));
        }
        if !self.risk_free_rate.is_finite() {
            return Err(OptionError::InvalidInput("risk-free rate must be finite"));
        }
        if !self.dividend_yield.is_finite() {
            return Err(OptionError::InvalidInput("dividend yield must be finite"));
        }
        if !self.volatility.is_finite() || self.volatility < 0.0 {
            return Err(OptionError::InvalidInput(
                "volatility must be non-negative and finite",
            ));
        }
        Ok(())
    }
}

/// First- and second-order price sensitivities (Greeks).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionGreeks {
    /// Delta ($\partial V / \partial S$): Sensitivity to underlying spot price.
    pub delta: f64,
    /// Gamma ($\partial^2 V / \partial S^2$): Rate of change of Delta.
    pub gamma: f64,
    /// Vega ($\partial V / \partial \sigma$): Sensitivity to a 1.0 (100 percentage point) change in volatility.
    pub vega: f64,
    /// Theta ($\partial V / \partial t$): Annualized rate of time decay (negative for standard long options).
    pub theta_annual: f64,
    /// Theta per calendar day (`theta_annual / 365.0`).
    pub theta_daily: f64,
    /// Rho ($\partial V / \partial r$): Sensitivity to a 1.0 (100 percentage point) change in risk-free rate.
    pub rho: f64,
}

/// Complete evaluated outcome of an option pricing computation.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionPricingResult {
    /// Theoretical option price.
    pub price: f64,
    /// Immediate intrinsic exercise value ($\max(S - K, 0)$ for Call, $\max(K - S, 0)$ for Put).
    pub intrinsic_value: f64,
    /// Remaining time value (`price - intrinsic_value`).
    pub time_value: f64,
    /// Evaluated sensitivities (Greeks).
    pub greeks: OptionGreeks,
}

/// Errors originating from option pricing or volatility inversion.
#[derive(Debug, Clone, PartialEq)]
pub enum OptionError {
    InvalidInput(&'static str),
    PriceBelowIntrinsic,
    PriceAboveBoundary,
    SolverMaxIterationsExceeded,
    SolverFailedToConverge,
}

impl fmt::Display for OptionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid option input: {msg}"),
            Self::PriceBelowIntrinsic => write!(
                f,
                "option price violates lower arbitrage bound (below intrinsic value)"
            ),
            Self::PriceAboveBoundary => write!(f, "option price violates upper arbitrage bound"),
            Self::SolverMaxIterationsExceeded => {
                write!(f, "implied volatility solver exceeded maximum iterations")
            }
            Self::SolverFailedToConverge => {
                write!(f, "implied volatility solver failed to converge")
            }
        }
    }
}

impl std::error::Error for OptionError {}

/// Evaluates a polynomial in Horner form: `lead` is the highest-order coefficient, `rest` the
/// remaining ones in descending order.
fn horner(x: f64, lead: f64, rest: &[f64]) -> f64 {
    rest.iter()
        .fold(lead, |acc, coefficient| acc * x + coefficient)
}

/// Standard normal cumulative distribution function $\Phi(x)$, evaluated with Hart's rational
/// approximation.
///
/// Every option price, delta, theta and rho in this module passes through here, so this
/// function's accuracy is their ceiling. Hart's form holds close to double precision across the
/// body and both tails — `tests/golden_reference_option_diff.rs` pins it against independently
/// generated reference values. The earlier Abramowitz & Stegun 7.1.26 approximation used here was accurate
/// to about $1.5 \times 10^{-7}$ absolute, which showed up as errors of that order times the
/// price level in every quantity derived from it.
///
/// Beyond $|x| = 37$ the result is 0 or 1 in double precision, and is returned as such.
pub fn normal_cdf(x: f64) -> f64 {
    let abs_x = x.abs();
    if abs_x > 37.0 {
        return if x > 0.0 { 1.0 } else { 0.0 };
    }

    let exponential = (-0.5 * abs_x * abs_x).exp();
    let upper_tail = if abs_x < 7.071_067_811_865_475 {
        // Rational approximation for the body, both polynomials in Horner form.
        let numerator = horner(
            abs_x,
            3.526_249_659_989_109e-2,
            &[
                0.700_383_064_443_688,
                6.373_962_203_531_65,
                33.912_866_078_383,
                112.079_291_497_871,
                221.213_596_169_931,
                220.206_867_912_376,
            ],
        );
        let denominator = horner(
            abs_x,
            8.838_834_764_831_844e-2,
            &[
                1.755_667_163_182_64,
                16.064_177_579_207,
                86.780_732_202_946_1,
                296.564_248_779_674,
                637.333_633_378_831,
                793.826_512_519_948,
                440.413_735_824_752,
            ],
        );
        exponential * numerator / denominator
    } else {
        // Continued fraction for the far tail, where the quotient above loses its digits.
        let mut fraction = abs_x + 0.65;
        for term in [4.0, 3.0, 2.0, 1.0] {
            fraction = abs_x + term / fraction;
        }
        exponential / (fraction * 2.506_628_274_631_000_5)
    };

    if x > 0.0 {
        1.0 - upper_tail
    } else {
        upper_tail
    }
}

/// Standard normal probability density function $\phi(x) = \frac{1}{\sqrt{2\pi}} e^{-x^2 / 2}$.
pub fn normal_pdf(x: f64) -> f64 {
    (1.0 / (2.0 * PI).sqrt()) * (-0.5 * x * x).exp()
}

/// Evaluates a European vanilla option using the Black-Scholes-Merton formula with dividend yield.
pub fn black_scholes_merton(
    option_type: OptionType,
    inputs: &BlackScholesInputs,
) -> Result<OptionPricingResult, OptionError> {
    inputs.validate()?;

    let s = inputs.spot;
    let k = inputs.strike;
    let t = inputs.time_to_expiry_years;
    let r = inputs.risk_free_rate;
    let q = inputs.dividend_yield;
    let sigma = inputs.volatility;

    let intrinsic = match option_type {
        OptionType::Call => (s - k).max(0.0),
        OptionType::Put => (k - s).max(0.0),
    };

    // Boundary case: at expiration (T = 0) the option is worth its intrinsic value.
    if t <= 1e-12 {
        let delta = match option_type {
            OptionType::Call => {
                if s > k {
                    1.0
                } else if (s - k).abs() < 1e-12 {
                    0.5
                } else {
                    0.0
                }
            }
            OptionType::Put => {
                if s < k {
                    -1.0
                } else if (s - k).abs() < 1e-12 {
                    -0.5
                } else {
                    0.0
                }
            }
        };
        return Ok(OptionPricingResult {
            price: intrinsic,
            intrinsic_value: intrinsic,
            time_value: 0.0,
            greeks: OptionGreeks {
                delta,
                gamma: 0.0,
                vega: 0.0,
                theta_annual: 0.0,
                theta_daily: 0.0,
                rho: 0.0,
            },
        });
    }

    // Boundary case: zero volatility before expiry (T > 0, sigma = 0). The terminal price is the
    // deterministic forward, so the value is the discounted forward payoff
    // `max(S e^{-qT} - K e^{-rT}, 0)` (put: `max(K e^{-rT} - S e^{-qT}, 0)`), the limit of the
    // Black formula at zero standard deviation — not the spot intrinsic value. Greeks are the derivatives of
    // that deterministic value: delta `±e^{-qT}` while in the money on a forward basis (half of it
    // exactly at the money-forward), gamma and vega 0, theta and rho from the BSM formulas with
    // `N(d1) = N(d2)` replaced by that in-the-money indicator.
    if sigma <= 1e-12 {
        let df_q = (-q * t).exp();
        let df_r = (-r * t).exp();
        let fwd_term = s * df_q;
        let strike_term = k * df_r;
        let call_itm = if (fwd_term - strike_term).abs() <= 1e-12 * fwd_term.max(strike_term) {
            0.5
        } else if fwd_term > strike_term {
            1.0
        } else {
            0.0
        };
        let (price, delta, theta_annual, rho) = match option_type {
            OptionType::Call => (
                (fwd_term - strike_term).max(0.0),
                df_q * call_itm,
                (q * fwd_term - r * strike_term) * call_itm,
                strike_term * t * call_itm,
            ),
            OptionType::Put => {
                let put_itm = 1.0 - call_itm;
                (
                    (strike_term - fwd_term).max(0.0),
                    -df_q * put_itm,
                    (r * strike_term - q * fwd_term) * put_itm,
                    -strike_term * t * put_itm,
                )
            }
        };
        return Ok(OptionPricingResult {
            price,
            intrinsic_value: intrinsic,
            time_value: (price - intrinsic).max(0.0),
            greeks: OptionGreeks {
                delta,
                gamma: 0.0,
                vega: 0.0,
                theta_annual,
                theta_daily: theta_annual / 365.0,
                rho,
            },
        });
    }

    let sqrt_t = t.sqrt();
    let d1 = ((s / k).ln() + (r - q + 0.5 * sigma * sigma) * t) / (sigma * sqrt_t);
    let d2 = d1 - sigma * sqrt_t;

    let df_q = (-q * t).exp();
    let df_r = (-r * t).exp();

    let pdf_d1 = normal_pdf(d1);

    let price = match option_type {
        OptionType::Call => s * df_q * normal_cdf(d1) - k * df_r * normal_cdf(d2),
        OptionType::Put => k * df_r * normal_cdf(-d2) - s * df_q * normal_cdf(-d1),
    };

    // Greeks
    let delta = match option_type {
        OptionType::Call => df_q * normal_cdf(d1),
        OptionType::Put => df_q * (normal_cdf(d1) - 1.0),
    };

    let gamma = (df_q * pdf_d1) / (s * sigma * sqrt_t);
    let vega = s * df_q * sqrt_t * pdf_d1;

    let theta_common = -(s * df_q * pdf_d1 * sigma) / (2.0 * sqrt_t);
    let theta_annual = match option_type {
        OptionType::Call => {
            theta_common - r * k * df_r * normal_cdf(d2) + q * s * df_q * normal_cdf(d1)
        }
        OptionType::Put => {
            theta_common + r * k * df_r * normal_cdf(-d2) - q * s * df_q * normal_cdf(-d1)
        }
    };
    let theta_daily = theta_annual / 365.0;

    let rho = match option_type {
        OptionType::Call => k * t * df_r * normal_cdf(d2),
        OptionType::Put => -k * t * df_r * normal_cdf(-d2),
    };

    let time_value = (price - intrinsic).max(0.0);

    Ok(OptionPricingResult {
        price,
        intrinsic_value: intrinsic,
        time_value,
        greeks: OptionGreeks {
            delta,
            gamma,
            vega,
            theta_annual,
            theta_daily,
            rho,
        },
    })
}

/// Evaluates a European option on a forward or futures contract using Black's 1976 model.
///
/// Greeks are taken with the forward held fixed: delta/gamma are with respect to the forward,
/// and rho is `-T · price` (only the discount factor `e^{-rT}` depends on `r`). The
/// zero-volatility case (`T > 0`, `sigma = 0`) values the discounted payoff
/// `e^{-rT} max(F - K, 0)` (put analog), see [`black_scholes_merton`].
pub fn black_76(
    option_type: OptionType,
    forward: f64,
    strike: f64,
    time_to_expiry_years: f64,
    risk_free_rate: f64,
    volatility: f64,
) -> Result<OptionPricingResult, OptionError> {
    if forward <= 0.0 {
        return Err(OptionError::InvalidInput(
            "forward must be positive and finite",
        ));
    }
    // In Black-76, Spot = Forward and dividend yield q = risk_free_rate r so df_q = df_r.
    let inputs = BlackScholesInputs {
        spot: forward,
        strike,
        time_to_expiry_years,
        risk_free_rate,
        dividend_yield: risk_free_rate,
        volatility,
    };
    // The BSM mapping (q = r) gets price, delta, gamma, vega and theta right for a fixed
    // forward, but its rho differentiates only the strike discount and treats the forward as
    // carried by r. With F fixed, the whole price is discounted by e^{-rT}, so rho = -T * price.
    let mut result = black_scholes_merton(option_type, &inputs)?;
    result.greeks.rho = -time_to_expiry_years * result.price;
    Ok(result)
}

/// Inverts the Black-Scholes-Merton formula to find the implied volatility from a market price.
///
/// Uses bounded Brent-Newton hybrid iteration. Returns volatility as an annualized fraction (e.g. 0.20 for 20%).
///
/// The search domain is `sigma > 0`. The lower bracket starts at `1e-4` and is lowered (down to
/// `1e-10`) when the market price lies below the price at that volatility. A result is returned
/// only when the price matches to `1e-8` or the volatility bracket has shrunk below `1e-10`;
/// otherwise the solver fails with [`OptionError::SolverMaxIterationsExceeded`] (iteration limit)
/// or [`OptionError::SolverFailedToConverge`] (no bracket in the domain) — never with the last
/// iterate.
pub fn implied_volatility(
    option_type: OptionType,
    market_price: f64,
    spot: f64,
    strike: f64,
    time_to_expiry_years: f64,
    risk_free_rate: f64,
    dividend_yield: f64,
) -> Result<f64, OptionError> {
    implied_volatility_with_limit(
        option_type,
        market_price,
        spot,
        strike,
        time_to_expiry_years,
        risk_free_rate,
        dividend_yield,
        100,
    )
}

#[allow(clippy::too_many_arguments)]
fn implied_volatility_with_limit(
    option_type: OptionType,
    market_price: f64,
    spot: f64,
    strike: f64,
    time_to_expiry_years: f64,
    risk_free_rate: f64,
    dividend_yield: f64,
    max_iter: usize,
) -> Result<f64, OptionError> {
    if !market_price.is_finite() || market_price <= 0.0 {
        return Err(OptionError::InvalidInput(
            "market price must be positive and finite",
        ));
    }
    if time_to_expiry_years <= 1e-12 {
        return Err(OptionError::InvalidInput(
            "cannot solve IV for expired option (T=0)",
        ));
    }

    let df_q = (-dividend_yield * time_to_expiry_years).exp();
    let df_r = (-risk_free_rate * time_to_expiry_years).exp();

    // Arbitrage bounds check
    let (lower_bound, upper_bound) = match option_type {
        OptionType::Call => ((spot * df_q - strike * df_r).max(0.0), spot * df_q),
        OptionType::Put => ((strike * df_r - spot * df_q).max(0.0), strike * df_r),
    };

    if market_price < lower_bound - 1e-7 {
        return Err(OptionError::PriceBelowIntrinsic);
    }
    if market_price > upper_bound + 1e-7 {
        return Err(OptionError::PriceAboveBoundary);
    }

    // Base template
    let mut inputs = BlackScholesInputs {
        spot,
        strike,
        time_to_expiry_years,
        risk_free_rate,
        dividend_yield,
        volatility: 0.20,
    };

    // Bracket search: find lower and upper vol bounds
    let mut vol_low = 1e-4f64;
    let mut vol_high = 5.0f64;

    inputs.volatility = vol_low;
    let mut price_low = black_scholes_merton(option_type, &inputs)?.price;
    while price_low > market_price && vol_low > 1e-10 {
        vol_low *= 0.1;
        inputs.volatility = vol_low;
        price_low = black_scholes_merton(option_type, &inputs)?.price;
    }
    if (price_low - market_price).abs() < 1e-7 {
        return Ok(vol_low);
    }
    if price_low > market_price {
        return Err(OptionError::SolverFailedToConverge);
    }

    inputs.volatility = vol_high;
    let mut price_high = black_scholes_merton(option_type, &inputs)?.price;
    while price_high < market_price && vol_high < 20.0 {
        vol_high *= 2.0;
        inputs.volatility = vol_high;
        price_high = black_scholes_merton(option_type, &inputs)?.price;
    }

    if price_high < market_price {
        return Err(OptionError::PriceAboveBoundary);
    }

    // Hybrid Newton-Raphson / Bisection
    let mut current_vol = 0.5 * (vol_low + vol_high);
    let tol = 1e-8;

    for _ in 0..max_iter {
        inputs.volatility = current_vol;
        let res = black_scholes_merton(option_type, &inputs)?;
        let diff = res.price - market_price;

        if diff.abs() < tol {
            return Ok(current_vol);
        }

        // Update bracket
        if diff > 0.0 {
            vol_high = current_vol;
        } else {
            vol_low = current_vol;
        }

        let vega = res.greeks.vega;
        let mut step_accepted = false;

        if vega > 1e-12 {
            let next_newton = current_vol - diff / vega;
            if next_newton > vol_low && next_newton < vol_high {
                current_vol = next_newton;
                step_accepted = true;
            }
        }

        if !step_accepted {
            // Fallback to bisection
            current_vol = 0.5 * (vol_low + vol_high);
        }

        if (vol_high - vol_low) < 1e-10 {
            return Ok(current_vol);
        }
    }

    Err(OptionError::SolverMaxIterationsExceeded)
}

/// Verifies European put-call parity and returns the pricing discrepancy.
///
/// Parity equation: $C - P = S e^{-q T} - K e^{-r T}$.
/// Returns $(C - P) - (S e^{-q T} - K e^{-r T})$.
/// A value close to zero (e.g. $|diff| < 10^{-6}$) indicates exact put-call parity.
pub fn verify_put_call_parity(
    call_price: f64,
    put_price: f64,
    spot: f64,
    strike: f64,
    time_to_expiry_years: f64,
    risk_free_rate: f64,
    dividend_yield: f64,
) -> f64 {
    let forward_term = spot * (-dividend_yield * time_to_expiry_years).exp();
    let discount_strike = strike * (-risk_free_rate * time_to_expiry_years).exp();
    (call_price - put_price) - (forward_term - discount_strike)
}

/// One option position held to expiry: a call or put, bought or sold, and the premium it changed
/// hands at.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionExpiryPosition {
    pub option_type: OptionType,
    /// [`PositionSide::Long`] bought the option and paid the premium, [`PositionSide::Short`]
    /// sold it and received the premium.
    pub side: PositionSide,
    /// Strike price (must be positive).
    pub strike: f64,
    /// Premium per unit of the underlying (must be non-negative; for a put at most the strike,
    /// the most a put can pay). Always the positive amount, for both sides.
    pub premium: f64,
    /// Units of the underlying per contract (must be positive). With `1.0` every amount is per
    /// unit of the underlying.
    pub contract_size: f64,
}

impl OptionExpiryPosition {
    /// Validates the position.
    pub fn validate(&self) -> Result<(), OptionError> {
        if !self.strike.is_finite() || self.strike <= 0.0 {
            return Err(OptionError::InvalidInput(
                "strike price must be positive and finite",
            ));
        }
        if !self.premium.is_finite() || self.premium < 0.0 {
            return Err(OptionError::InvalidInput(
                "premium must be non-negative and finite",
            ));
        }
        if !self.contract_size.is_finite() || self.contract_size <= 0.0 {
            return Err(OptionError::InvalidInput(
                "contract size must be positive and finite",
            ));
        }
        if self.option_type == OptionType::Put && self.premium > self.strike {
            return Err(OptionError::PriceAboveBoundary);
        }
        Ok(())
    }
}

/// A largest profit or largest loss, which a sold or bought call does not have.
///
/// `Unlimited` is a variant of its own so that no infinite number stands in for it: a value
/// meant for an axis or a JSON document is always finite.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum PayoffBound {
    /// The amount, never negative.
    Limited(f64),
    /// Grows without bound as the underlying rises.
    Unlimited,
}

impl PayoffBound {
    /// The amount, `None` for [`PayoffBound::Unlimited`].
    pub fn limited(self) -> Option<f64> {
        match self {
            Self::Limited(amount) => Some(amount),
            Self::Unlimited => None,
        }
    }
}

/// The key figures of an [`OptionExpiryPosition`], see [`option_expiry_profile`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionExpiryProfile {
    /// The price of the underlying at expiry at which the result is zero: `strike + premium`
    /// for a call, `strike - premium` for a put, the same for both sides.
    pub break_even: f64,
    /// The largest result over all prices of the underlying from `0` up.
    pub max_profit: PayoffBound,
    /// The largest loss as a positive amount: the lowest result is `-max_loss`.
    pub max_loss: PayoffBound,
}

/// One point of [`option_expiry_series`].
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OptionExpiryPoint {
    /// Price of the underlying at expiry.
    pub underlying: f64,
    /// Result of the position at that price, see [`option_expiry_result`].
    pub result: f64,
}

/// The result of `position` when the underlying stands at `underlying` at expiry.
///
/// At expiry an option is worth its intrinsic value, `max(S - K, 0)` for a call and
/// `max(K - S, 0)` for a put — what [`black_scholes_merton`] returns with no time left. With the
/// premium `P` and the contract size `c`:
///
/// - bought: `(intrinsic - P) * c`
/// - sold: `(P - intrinsic) * c`
///
/// Positive is a profit of the position's holder, negative a loss; the two sides of one trade add
/// up to zero. The premium counts at its amount: no interest between trade and expiry, no fees.
///
/// `underlying` must be finite and not negative.
pub fn option_expiry_result(
    position: &OptionExpiryPosition,
    underlying: f64,
) -> Result<f64, OptionError> {
    position.validate()?;
    if !underlying.is_finite() || underlying < 0.0 {
        return Err(OptionError::InvalidInput(
            "underlying price at expiry must be non-negative and finite",
        ));
    }
    Ok(expiry_result(position, underlying))
}

/// [`option_expiry_result`] for a validated position and price.
fn expiry_result(position: &OptionExpiryPosition, underlying: f64) -> f64 {
    let intrinsic = match position.option_type {
        OptionType::Call => (underlying - position.strike).max(0.0),
        OptionType::Put => (position.strike - underlying).max(0.0),
    };
    // Written out per side so that a result of zero is `0.0` for both, never `-0.0`.
    let per_unit = match position.side {
        PositionSide::Long => intrinsic - position.premium,
        PositionSide::Short => position.premium - intrinsic,
    };
    per_unit * position.contract_size
}

/// Break-even, largest profit and largest loss of `position` at expiry, over every price of the
/// underlying from `0` up. With strike `K`, premium `P` and contract size `c`:
///
/// | Position | Break-even | Largest profit | Largest loss |
/// |---|---|---|---|
/// | call bought | `K + P` | unlimited | `P * c` |
/// | call sold | `K + P` | `P * c` | unlimited |
/// | put bought | `K - P` | `(K - P) * c` | `P * c` |
/// | put sold | `K - P` | `P * c` | `(K - P) * c` |
///
/// The put's `(K - P) * c` is reached with the underlying at `0`. With a premium of `0` the
/// result is zero on the whole side on which the option expires worthless, and the break-even
/// is the strike, where that side ends.
pub fn option_expiry_profile(
    position: &OptionExpiryPosition,
) -> Result<OptionExpiryProfile, OptionError> {
    position.validate()?;
    let premium = position.premium * position.contract_size;
    let (break_even, bought_profit) = match position.option_type {
        OptionType::Call => (position.strike + position.premium, PayoffBound::Unlimited),
        OptionType::Put => (
            position.strike - position.premium,
            PayoffBound::Limited((position.strike - position.premium) * position.contract_size),
        ),
    };
    let (max_profit, max_loss) = match position.side {
        PositionSide::Long => (bought_profit, PayoffBound::Limited(premium)),
        PositionSide::Short => (PayoffBound::Limited(premium), bought_profit),
    };
    Ok(OptionExpiryProfile {
        break_even,
        max_profit,
        max_loss,
    })
}

/// The most points [`option_expiry_series`] returns.
pub const MAX_EXPIRY_SERIES_POINTS: usize = 10_001;

/// [`option_expiry_result`] over a grid of prices of the underlying, for a payoff diagram.
///
/// The grid has `points` prices from `from` to `to`, both included and equally spaced:
/// `S_i = from + (to - from) * i / (points - 1)`. The result is linear on both sides of the
/// strike and bends there; the line through the points shows the bend exactly when the strike is
/// one of them (`from = 88`, `to = 112`, `points = 25` puts a point on every whole price).
///
/// Requires `0 <= from < to`, both finite, and `2 <= points <= `[`MAX_EXPIRY_SERIES_POINTS`].
pub fn option_expiry_series(
    position: &OptionExpiryPosition,
    from: f64,
    to: f64,
    points: usize,
) -> Result<Vec<OptionExpiryPoint>, OptionError> {
    position.validate()?;
    if !from.is_finite() || !to.is_finite() || from < 0.0 || from >= to {
        return Err(OptionError::InvalidInput(
            "price grid must satisfy 0 <= from < to, both finite",
        ));
    }
    if !(2..=MAX_EXPIRY_SERIES_POINTS).contains(&points) {
        return Err(OptionError::InvalidInput(
            "price grid must have between 2 and 10001 points",
        ));
    }
    let last = points - 1;
    Ok((0..points)
        .map(|i| {
            let underlying = if i == last {
                to
            } else {
                from + (to - from) * i as f64 / last as f64
            };
            OptionExpiryPoint {
                underlying,
                result: expiry_result(position, underlying),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(spot: f64, strike: f64, t: f64, r: f64, q: f64, vol: f64) -> BlackScholesInputs {
        BlackScholesInputs {
            spot,
            strike,
            time_to_expiry_years: t,
            risk_free_rate: r,
            dividend_yield: q,
            volatility: vol,
        }
    }

    #[test]
    fn zero_volatility_before_expiry_values_the_discounted_forward_payoff() {
        let call =
            black_scholes_merton(OptionType::Call, &inputs(100.0, 100.0, 1.0, 0.1, 0.0, 0.0))
                .unwrap();
        let expected = 100.0 - 100.0 * (-0.1f64).exp();
        assert!((call.price - expected).abs() < 1e-9, "{}", call.price);
        assert!((expected - 9.516_258_196_4).abs() < 1e-9);
        assert_eq!(call.greeks.delta, 1.0);
        assert_eq!(call.greeks.gamma, 0.0);
        assert_eq!(call.greeks.vega, 0.0);

        // Greeks match finite differences of the deterministic price.
        let h = 1e-5;
        let price_at = |r: f64, t: f64| {
            black_scholes_merton(OptionType::Call, &inputs(100.0, 100.0, t, r, 0.0, 0.0))
                .unwrap()
                .price
        };
        let rho_fd = (price_at(0.1 + h, 1.0) - price_at(0.1 - h, 1.0)) / (2.0 * h);
        assert!((call.greeks.rho - rho_fd).abs() < 1e-5);
        let theta_fd = -(price_at(0.1, 1.0 + h) - price_at(0.1, 1.0 - h)) / (2.0 * h);
        assert!((call.greeks.theta_annual - theta_fd).abs() < 1e-5);

        // The put is out of the money on a forward basis.
        let put = black_scholes_merton(OptionType::Put, &inputs(100.0, 100.0, 1.0, 0.1, 0.0, 0.0))
            .unwrap();
        assert_eq!(put.price, 0.0);
        assert_eq!(put.greeks.delta, 0.0);
        assert_eq!(put.greeks.rho, 0.0);

        // Black-76 inherits the branch: e^{-rT} max(F - K, 0).
        let b76 = black_76(OptionType::Put, 90.0, 100.0, 1.0, 0.05, 0.0).unwrap();
        assert!((b76.price - 10.0 * (-0.05f64).exp()).abs() < 1e-12);
        assert!((b76.greeks.delta + (-0.05f64).exp()).abs() < 1e-12);
    }

    #[test]
    fn implied_volatility_reports_iteration_limit_instead_of_the_last_iterate() {
        let price = black_scholes_merton(
            OptionType::Call,
            &inputs(100.0, 105.0, 0.5, 0.03, 0.0, 0.237),
        )
        .unwrap()
        .price;
        let args = (OptionType::Call, price, 100.0, 105.0, 0.5, 0.03, 0.0);
        assert_eq!(
            implied_volatility_with_limit(
                args.0, args.1, args.2, args.3, args.4, args.5, args.6, 1
            ),
            Err(OptionError::SolverMaxIterationsExceeded)
        );
        let solved =
            implied_volatility(args.0, args.1, args.2, args.3, args.4, args.5, args.6).unwrap();
        assert!((solved - 0.237).abs() < 1e-7, "{solved}");
    }

    #[test]
    fn implied_volatility_finds_volatilities_below_the_initial_bracket() {
        let target = 2e-5;
        let price = black_scholes_merton(
            OptionType::Call,
            &inputs(100.0, 100.0, 1.0, 0.0, 0.0, target),
        )
        .unwrap()
        .price;
        let solved =
            implied_volatility(OptionType::Call, price, 100.0, 100.0, 1.0, 0.0, 0.0).unwrap();
        assert!((solved - target).abs() < 1e-7, "{solved}");
    }

    #[test]
    fn black_76_rho_matches_finite_difference_with_fixed_forward() {
        let h = 1e-5;
        for option_type in [OptionType::Call, OptionType::Put] {
            for vol in [0.2, 0.0] {
                let price_at = |r: f64| black_76(option_type, 100.0, 95.0, 1.0, r, vol).unwrap();
                let fd = (price_at(0.05 + h).price - price_at(0.05 - h).price) / (2.0 * h);
                let rho = price_at(0.05).greeks.rho;
                assert!(
                    (rho - fd).abs() < 1e-5,
                    "{option_type:?} vol {vol}: {rho} vs {fd}"
                );
            }
        }
        let atm = black_76(OptionType::Call, 100.0, 100.0, 1.0, 0.05, 0.2).unwrap();
        let fd = (black_76(OptionType::Call, 100.0, 100.0, 1.0, 0.05 + h, 0.2)
            .unwrap()
            .price
            - black_76(OptionType::Call, 100.0, 100.0, 1.0, 0.05 - h, 0.2)
                .unwrap()
                .price)
            / (2.0 * h);
        assert!((atm.greeks.rho - fd).abs() < 1e-5);
        assert!((atm.greeks.rho + 7.577).abs() < 1e-3, "{}", atm.greeks.rho);
    }
}
