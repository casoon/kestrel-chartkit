//! European vanilla option pricing, analytical Greeks, and implied volatility solver.
//!
//! Implements Black-Scholes-Merton (BSM) for spot/equity with continuous dividend yield,
//! Black-76 for futures/forwards, analytical Greeks, put-call parity verification,
//! and robust root-finding for implied volatility.

use std::f64::consts::PI;
use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

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

/// Standard normal cumulative distribution function $\Phi(x)$ using Abramowitz & Stegun (1964) formula 7.1.26.
/// Maximum absolute approximation error is less than $1.5 \times 10^{-7}$.
pub fn normal_cdf(x: f64) -> f64 {
    if x < -38.0 {
        return 0.0;
    }
    if x > 38.0 {
        return 1.0;
    }

    let p = 0.3275911;
    let a1 = 0.254829592;
    let a2 = -0.284496736;
    let a3 = 1.421413741;
    let a4 = -1.453152027;
    let a5 = 1.061405429;

    let sign = if x < 0.0 { -1.0 } else { 1.0 };
    let abs_x = x.abs() / std::f64::consts::SQRT_2;

    let t = 1.0 / (1.0 + p * abs_x);
    let poly = (((a5 * t + a4) * t + a3) * t + a2) * t + a1;
    let erf = 1.0 - poly * t * (-abs_x * abs_x).exp();

    0.5 * (1.0 + sign * erf)
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

    // Boundary case: At expiration (T = 0) or zero volatility
    if t <= 1e-12 || sigma <= 1e-12 {
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
    black_scholes_merton(option_type, &inputs)
}

/// Inverts the Black-Scholes-Merton formula to find the implied volatility from a market price.
///
/// Uses bounded Brent-Newton hybrid iteration. Returns volatility as an annualized fraction (e.g. 0.20 for 20%).
pub fn implied_volatility(
    option_type: OptionType,
    market_price: f64,
    spot: f64,
    strike: f64,
    time_to_expiry_years: f64,
    risk_free_rate: f64,
    dividend_yield: f64,
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
    let price_low = black_scholes_merton(option_type, &inputs)?.price;
    if (price_low - market_price).abs() < 1e-7 {
        return Ok(vol_low);
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
    let max_iter = 100;
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

    Ok(current_vol)
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
