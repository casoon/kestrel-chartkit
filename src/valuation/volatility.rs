//! Volatility over strike and maturity.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::finance::{year_fraction, Date, DayCountConvention};

use super::ValuationContextError;

/// A volatility surface given as a grid of quoted points.
///
/// Maturities run along one axis and strikes along the other, and every cell holds the volatility
/// quoted there. Between the grid lines the surface is bilinear: linear in maturity, linear in
/// strike, in that order. Outside them it is held flat at the edge value — the same convention
/// the yield curve uses, stated here for the same reason: it is an assumption, and an assumption
/// that is written down can be argued with.
///
/// The grid is deliberately the whole model. A parametric fit — a smile function with its own
/// coefficients — would produce a smooth surface everywhere, including where nothing was quoted,
/// and its shape would come from the parametrisation rather than from the market. Interpolating
/// what was actually quoted keeps the two apart.
///
/// `validity` bounds how far a caller may ask beyond the quoted range before the surface refuses
/// to answer. Flat extrapolation one strike beyond the grid is a mild assumption; ten strikes
/// beyond it is a fabrication, and the difference should not be silent.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VolatilitySurface {
    reference: Date,
    day_count: DayCountConvention,
    /// Ascending maturities in years from the reference date.
    maturities: Vec<f64>,
    /// Ascending strikes.
    strikes: Vec<f64>,
    /// `volatilities[maturity index][strike index]`.
    volatilities: Vec<Vec<f64>>,
    validity: SurfaceValidity,
}

/// How far outside the quoted grid the surface still answers.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SurfaceValidity {
    /// Multiples of the quoted strike range that may be exceeded on either side. `0.0` refuses
    /// any strike outside the grid, `f64::INFINITY` never refuses.
    pub strike_tolerance: f64,
    /// Years beyond the last quoted maturity that may still be asked for.
    pub maturity_tolerance_years: f64,
}

impl Default for SurfaceValidity {
    fn default() -> Self {
        // A tenth of the quoted strike width and a quarter year: close enough to the grid that
        // holding the edge value flat is a defensible reading of the quotes.
        Self {
            strike_tolerance: 0.1,
            maturity_tolerance_years: 0.25,
        }
    }
}

impl VolatilitySurface {
    /// Builds a surface from a grid.
    ///
    /// `maturities` are years from `reference`, `strikes` are prices, and `volatilities` is one
    /// row per maturity. Both axes must be finite, strictly ascending and non-empty, every
    /// volatility finite and non-negative, and the grid rectangular.
    pub fn new(
        reference: Date,
        maturities: Vec<f64>,
        strikes: Vec<f64>,
        volatilities: Vec<Vec<f64>>,
        day_count: DayCountConvention,
        validity: SurfaceValidity,
    ) -> Result<Self, ValuationContextError> {
        if maturities.is_empty() || strikes.is_empty() {
            return Err(ValuationContextError::InvalidCurve(
                "a surface needs at least one maturity and one strike",
            ));
        }
        if ascending_violation(&maturities) || ascending_violation(&strikes) {
            return Err(ValuationContextError::InvalidCurve(
                "maturities and strikes must be finite and strictly ascending",
            ));
        }
        if maturities.iter().any(|t| *t < 0.0) || strikes.iter().any(|k| *k <= 0.0) {
            return Err(ValuationContextError::InvalidCurve(
                "maturities must be non-negative and strikes positive",
            ));
        }
        if volatilities.len() != maturities.len()
            || volatilities.iter().any(|row| row.len() != strikes.len())
        {
            return Err(ValuationContextError::InvalidCurve(
                "the volatility grid must be rectangular and match both axes",
            ));
        }
        if volatilities
            .iter()
            .any(|row| row.iter().any(|v| !v.is_finite() || *v < 0.0))
        {
            return Err(ValuationContextError::InvalidCurve(
                "volatilities must be finite and non-negative",
            ));
        }

        Ok(Self {
            reference,
            day_count,
            maturities,
            strikes,
            volatilities,
            validity,
        })
    }

    /// A surface with one volatility everywhere — the compatible special case, so a caller who
    /// only has one number keeps working.
    pub fn flat(
        reference: Date,
        volatility: f64,
        day_count: DayCountConvention,
    ) -> Result<Self, ValuationContextError> {
        Self::new(
            reference,
            vec![0.0],
            vec![1.0],
            vec![vec![volatility]],
            day_count,
            SurfaceValidity {
                strike_tolerance: f64::INFINITY,
                maturity_tolerance_years: f64::INFINITY,
            },
        )
    }

    pub fn reference_date(&self) -> Date {
        self.reference
    }

    /// Volatility for a maturity in years and a strike.
    pub fn volatility_at(&self, time: f64, strike: f64) -> Result<f64, ValuationContextError> {
        if !time.is_finite() || time < 0.0 || !strike.is_finite() || strike <= 0.0 {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        self.check_validity(time, strike)?;

        let (row_low, row_high, row_weight) = bracket(&self.maturities, time);
        let (col_low, col_high, col_weight) = bracket(&self.strikes, strike);

        let interpolate = |row: usize| {
            let low = self.volatilities[row][col_low];
            let high = self.volatilities[row][col_high];
            low + col_weight * (high - low)
        };
        let low = interpolate(row_low);
        let high = interpolate(row_high);
        Ok(low + row_weight * (high - low))
    }

    /// Volatility for an expiry date and a strike.
    pub fn volatility(&self, expiry: Date, strike: f64) -> Result<f64, ValuationContextError> {
        if expiry < self.reference {
            return Err(ValuationContextError::TimeOutsideCurve);
        }
        self.volatility_at(
            year_fraction(self.reference, expiry, self.day_count),
            strike,
        )
    }

    fn check_validity(&self, time: f64, strike: f64) -> Result<(), ValuationContextError> {
        let last_maturity = self.maturities[self.maturities.len() - 1];
        if time > last_maturity + self.validity.maturity_tolerance_years {
            return Err(ValuationContextError::OutsideSurfaceValidity);
        }

        let (first_strike, last_strike) = (self.strikes[0], self.strikes[self.strikes.len() - 1]);
        let width = (last_strike - first_strike).max(f64::MIN_POSITIVE);
        let allowance = width * self.validity.strike_tolerance;
        if strike < first_strike - allowance || strike > last_strike + allowance {
            return Err(ValuationContextError::OutsideSurfaceValidity);
        }
        Ok(())
    }
}

fn ascending_violation(values: &[f64]) -> bool {
    values.iter().any(|v| !v.is_finite()) || values.windows(2).any(|w| w[0] >= w[1])
}

/// Neighbouring indices and the weight between them; outside the axis both indices are the edge,
/// which is what holds the value flat.
fn bracket(axis: &[f64], value: f64) -> (usize, usize, f64) {
    if value <= axis[0] {
        return (0, 0, 0.0);
    }
    let last = axis.len() - 1;
    if value >= axis[last] {
        return (last, last, 0.0);
    }
    let upper = axis.partition_point(|entry| *entry <= value).max(1);
    let lower = upper - 1;
    let weight = (value - axis[lower]) / (axis[upper] - axis[lower]);
    (lower, upper, weight)
}
