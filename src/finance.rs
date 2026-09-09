//! Financial day-count conventions, cashflow discounting, and bond valuation.
//!
//! Provides standard day-count year fraction calculators (Actual/360, Actual/365Fixed,
//! 30/360 Bond Basis, Actual/Actual ISDA), cashflow discounting, bond pricing (clean/dirty,
//! accrued interest), Macaulay/Modified Duration, DV01, and Yield to Maturity (YTM) inversion.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A simple calendar date (year, month, day) in the Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Creates a new date, validating month (1..=12) and day (1..=days_in_month).
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day < 1 {
            return None;
        }
        let days = Self::days_in_month(year, month);
        if day > days {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
    }

    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if Self::is_leap_year(year) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }

    /// Converts the date into an ordinal day count (days since 0001-01-01).
    pub fn to_day_number(&self) -> i64 {
        let mut y = self.year as i64;
        let mut m = self.month as i64;
        if m <= 2 {
            y -= 1;
            m += 12;
        }
        // Gregorian calendar formula
        (365 * y) + (y / 4) - (y / 100) + (y / 400) + ((153 * (m + 1)) / 5) + self.day as i64 - 428
    }

    /// Returns the number of actual elapsed calendar days from `self` to `other` (`other - self`).
    pub fn days_until(&self, other: &Date) -> i64 {
        other.to_day_number() - self.to_day_number()
    }
}

/// Financial day-count convention determining the fraction of a year between two dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum DayCountConvention {
    /// Money market convention: Actual calendar days divided by 360.
    #[default]
    Actual360,
    /// Fixed year convention: Actual calendar days divided by 365.
    Actual365Fixed,
    /// US 30/360 Bond Basis (ISMA-30/360 / BMA). Assumes 30 days per month.
    Thirty360,
    /// Actual/Actual ISDA: Splits leap years and normal years proportionally.
    ActualActualISDA,
}

/// Computes the year fraction between two dates according to the chosen day-count convention.
pub fn year_fraction(d1: Date, d2: Date, convention: DayCountConvention) -> f64 {
    if d1 == d2 {
        return 0.0;
    }
    let (start, end, sign) = if d1 <= d2 {
        (d1, d2, 1.0)
    } else {
        (d2, d1, -1.0)
    };

    let fraction = match convention {
        DayCountConvention::Actual360 => start.days_until(&end) as f64 / 360.0,
        DayCountConvention::Actual365Fixed => start.days_until(&end) as f64 / 365.0,
        DayCountConvention::Thirty360 => {
            let mut d1_day = start.day;
            let mut d2_day = end.day;
            if d1_day == 31 {
                d1_day = 30;
            }
            if d2_day == 31 && d1_day >= 30 {
                d2_day = 30;
            }
            let days_360 = (end.year as i64 - start.year as i64) * 360
                + (end.month as i64 - start.month as i64) * 30
                + (d2_day as i64 - d1_day as i64);
            days_360 as f64 / 360.0
        }
        DayCountConvention::ActualActualISDA => {
            if start.year == end.year {
                let year_days = if Date::is_leap_year(start.year) {
                    366.0
                } else {
                    365.0
                };
                start.days_until(&end) as f64 / year_days
            } else {
                let end_of_first_year = Date::new(start.year, 12, 31).unwrap();
                let start_of_last_year = Date::new(end.year, 1, 1).unwrap();

                let first_year_days = if Date::is_leap_year(start.year) {
                    366.0
                } else {
                    365.0
                };
                let last_year_days = if Date::is_leap_year(end.year) {
                    366.0
                } else {
                    365.0
                };

                let days1 = start.days_until(&end_of_first_year) + 1;
                let days2 = start_of_last_year.days_until(&end);

                let middle_years = (end.year - start.year - 1).max(0) as f64;

                (days1 as f64 / first_year_days) + middle_years + (days2 as f64 / last_year_days)
            }
        }
    };

    sign * fraction
}

/// Compounding frequency convention for interest rates and discounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Compounding {
    /// Continuous compounding: $D(\tau) = e^{-r \tau}$.
    #[default]
    Continuous,
    /// Annual compounding: $D(\tau) = (1 + r)^{-\tau}$.
    Annual,
    /// Periodic compounding $m$ times per year: $D(\tau) = (1 + r/m)^{-m \tau}$.
    Periodic(u32),
}

/// Computes the discount factor $D(\tau)$ for a given interest rate, time fraction $\tau$, and compounding method.
///
/// Supports negative interest rates; the discount factor remains strictly positive.
pub fn discount_factor(rate: f64, tau: f64, compounding: Compounding) -> f64 {
    if !rate.is_finite() || !tau.is_finite() || tau < 0.0 {
        return 0.0;
    }
    match compounding {
        Compounding::Continuous => (-rate * tau).exp(),
        Compounding::Annual => {
            if rate <= -1.0 {
                0.0
            } else {
                (1.0 + rate).powf(-tau)
            }
        }
        Compounding::Periodic(m) => {
            let m_f = m.max(1) as f64;
            let base = 1.0 + rate / m_f;
            if base <= 0.0 {
                0.0
            } else {
                base.powf(-m_f * tau)
            }
        }
    }
}

/// Result of evaluating a fixed-coupon bond.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BondPricingResult {
    /// Dirty (full) price including accrued interest.
    pub dirty_price: f64,
    /// Clean price excluding accrued interest (`dirty_price - accrued_interest`).
    pub clean_price: f64,
    /// Accrued interest earned since the last coupon date.
    pub accrued_interest: f64,
    /// Macaulay duration in years: weighted average maturity of cashflows.
    pub macaulay_duration: f64,
    /// Modified duration: percentage price change per 100 bp change in yield.
    pub modified_duration: f64,
    /// Dollar value of a 1 basis point (0.01% = 0.0001) yield decrease: `dirty_price * modified_duration * 0.0001`.
    pub dv01: f64,
}

/// Errors originating from finance or bond pricing calculations.
#[derive(Debug, Clone, PartialEq)]
pub enum FinanceError {
    InvalidInput(&'static str),
    SolverFailedToConverge,
}

impl fmt::Display for FinanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid finance input: {msg}"),
            Self::SolverFailedToConverge => {
                write!(f, "yield to maturity solver failed to converge")
            }
        }
    }
}

impl std::error::Error for FinanceError {}

/// Prices a standard fixed-rate bond with regular coupon payments.
///
/// Returns clean price, dirty price, accrued interest, Macaulay/Modified duration, and DV01.
///
/// # Model and its limits
///
/// This function works from a *derived* time grid, not from an actual payment schedule: the
/// number of remaining coupons is the remaining year fraction times the frequency, rounded, and
/// each payment is discounted at `i / frequency` years. There is no calendar, no business-day
/// rule, no month-end convention and no stub period.
///
/// What that costs is measured against reference values in
/// `tests/golden_reference_bond_diff.rs`:
///
/// * **Settlement on a coupon date.** Dirty price within `1e-4` relative, both durations within
///   `1.4e-3` relative of a full-schedule valuation. The residual comes from the grid: real
///   coupon dates are not exactly `i / frequency` years apart under Actual/365Fixed.
/// * **Accrued interest and clean price are not reliable.** Without a schedule this function
///   cannot tell that a settlement date *is* a coupon date — leap years alone keep the remaining
///   year fraction from being a clean multiple of the period length. The accrued interest can
///   come out nearly a full coupon too high, and the clean price correspondingly too low. Use
///   `dirty_price` and treat the split as unsupported.
/// * **Settlement between coupon dates.** The rounded coupon count drops the fractional first
///   period, and the dirty price deviates by percent, not basis points.
///
/// Real payment schedules with calendars, business-day adjustment and stub periods are planned
/// separately; until then these are the boundaries of what this function claims.
pub fn price_bond(
    face_value: f64,
    coupon_rate: f64,
    frequency: u32,
    settlement: Date,
    maturity: Date,
    ytm: f64,
    convention: DayCountConvention,
) -> Result<BondPricingResult, FinanceError> {
    if !face_value.is_finite() || face_value <= 0.0 {
        return Err(FinanceError::InvalidInput("face_value must be positive"));
    }
    if !coupon_rate.is_finite() || coupon_rate < 0.0 {
        return Err(FinanceError::InvalidInput(
            "coupon_rate must be non-negative",
        ));
    }
    if frequency == 0 {
        return Err(FinanceError::InvalidInput("frequency must be >= 1"));
    }
    if settlement >= maturity {
        return Err(FinanceError::InvalidInput(
            "settlement must precede maturity",
        ));
    }
    if !ytm.is_finite() {
        return Err(FinanceError::InvalidInput("ytm must be finite"));
    }

    let m = frequency as f64;
    let coupon_payment = face_value * coupon_rate / m;
    let tau_to_maturity = year_fraction(settlement, maturity, convention);

    // Number of remaining coupon periods
    let approx_periods = (tau_to_maturity * m).round().max(1.0) as usize;

    let mut dirty_price = 0.0f64;
    let mut weighted_pv_sum = 0.0f64;

    for i in 1..=approx_periods {
        let period_idx = i as f64;
        let tau_cf = (period_idx / m).min(tau_to_maturity);
        let df = discount_factor(ytm, tau_cf, Compounding::Periodic(frequency));

        let cf = if i == approx_periods {
            coupon_payment + face_value
        } else {
            coupon_payment
        };

        let pv = cf * df;
        dirty_price += pv;
        weighted_pv_sum += tau_cf * pv;
    }

    let macaulay_duration = if dirty_price > 0.0 {
        weighted_pv_sum / dirty_price
    } else {
        0.0
    };

    let modified_duration = macaulay_duration / (1.0 + ytm / m);
    let dv01 = dirty_price * modified_duration * 0.0001;

    // Approximate accrued interest if settlement is between coupon dates
    let coupon_period_fraction = 1.0 / m;
    let time_since_last = (coupon_period_fraction - (tau_to_maturity % coupon_period_fraction))
        .abs()
        % coupon_period_fraction;
    let accrued_interest = coupon_payment * (time_since_last / coupon_period_fraction);
    let clean_price = dirty_price - accrued_interest;

    Ok(BondPricingResult {
        dirty_price,
        clean_price,
        accrued_interest,
        macaulay_duration,
        modified_duration,
        dv01,
    })
}

/// Solves for the Yield to Maturity (YTM) given a clean bond market price.
pub fn yield_to_maturity(
    clean_price: f64,
    face_value: f64,
    coupon_rate: f64,
    frequency: u32,
    settlement: Date,
    maturity: Date,
    convention: DayCountConvention,
) -> Result<f64, FinanceError> {
    if !clean_price.is_finite() || clean_price <= 0.0 {
        return Err(FinanceError::InvalidInput("clean_price must be positive"));
    }

    let mut ytm = coupon_rate.max(0.01);
    let max_iter = 100;
    let tol = 1e-8;

    for _ in 0..max_iter {
        let res = price_bond(
            face_value,
            coupon_rate,
            frequency,
            settlement,
            maturity,
            ytm,
            convention,
        )?;
        let diff = res.clean_price - clean_price;

        if diff.abs() < tol {
            return Ok(ytm);
        }

        // Derivative dPrice / dYTM = -dirty_price * modified_duration
        let derivative = -res.dirty_price * res.modified_duration;
        if derivative.abs() < 1e-12 {
            return Err(FinanceError::SolverFailedToConverge);
        }

        let step = diff / derivative;
        ytm -= step;

        if ytm < -0.50 {
            ytm = -0.50;
        }
    }

    Ok(ytm)
}
