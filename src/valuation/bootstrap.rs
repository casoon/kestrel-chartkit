//! Building a curve from quoted market instruments.
//!
//! [`YieldCurve::from_zero_rates`] takes a term structure that someone has already worked out.
//! This module works it out: given instruments that trade, it finds the zero rates that reprice
//! them. Two instrument types are supported and named, because an unnamed one would be a silent
//! approximation:
//!
//! * a directly quoted zero rate, and
//! * a par swap — a fixed rate paid `frequency` times a year that makes the swap worth nothing
//!   today.
//!
//! Everything else — futures, FRAs, tenor basis, cross-currency — is *not* supported and is
//! rejected rather than fitted with something that looks similar.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::finance::{year_fraction, CouponSchedule, Date, DayCountConvention};

use super::{ValuationContextError, YieldCurve};

/// A quoted instrument a curve is calibrated to.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum CalibrationInstrument {
    /// A continuously compounded zero rate quoted directly for `maturity`.
    ZeroRate { maturity: Date, rate: f64 },
    /// A par swap: `rate` paid `frequency` times a year until `maturity`, worth zero today.
    ///
    /// The par condition used is `rate * annuity = 1 - discount(maturity)`, with the annuity
    /// summed over the swap's own payment schedule. That is the textbook single-curve
    /// relationship: it assumes discounting and projection happen on the same curve, which is the
    /// case this crate models. A market that discounts on a separate collateral curve needs a
    /// second curve, and pretending otherwise here would misprice by exactly that difference.
    ParSwap {
        maturity: Date,
        rate: f64,
        frequency: u32,
    },
}

impl CalibrationInstrument {
    pub fn maturity(&self) -> Date {
        match self {
            Self::ZeroRate { maturity, .. } | Self::ParSwap { maturity, .. } => *maturity,
        }
    }
}

impl YieldCurve {
    /// Builds a curve that reprices every given instrument.
    ///
    /// Instruments are taken in order of maturity, each fixing one node. A zero rate fixes its
    /// node directly. A par swap is solved for: the zero rate at its maturity is chosen so that
    /// the curve — including the interpolation between the already fixed nodes and this one —
    /// values the swap at par. Solving against the interpolation actually used is what makes the
    /// result self-consistent; fixing nodes by a closed formula and interpolating afterwards
    /// would leave the intermediate payment dates mispriced.
    ///
    /// Fails when instruments share a maturity, when one matures at or before the reference date,
    /// or when a swap has no solution in the searched range of `-50%` to `+100%`.
    ///
    /// The result is a plain [`YieldCurve`]: once built, nothing distinguishes it from one given
    /// by hand, and the same flat extrapolation applies beyond the last instrument.
    pub fn bootstrap(
        reference: Date,
        instruments: &[CalibrationInstrument],
        day_count: DayCountConvention,
    ) -> Result<Self, ValuationContextError> {
        if instruments.is_empty() {
            return Err(ValuationContextError::InvalidCurve(
                "bootstrapping needs at least one instrument",
            ));
        }

        let mut sorted = instruments.to_vec();
        sorted.sort_by_key(|instrument| instrument.maturity());
        if sorted
            .windows(2)
            .any(|w| w[0].maturity() == w[1].maturity())
        {
            return Err(ValuationContextError::InvalidCurve(
                "two instruments share a maturity",
            ));
        }
        if sorted[0].maturity() <= reference {
            return Err(ValuationContextError::InvalidCurve(
                "instruments must mature after the reference date",
            ));
        }

        let mut nodes: Vec<(f64, f64)> = Vec::with_capacity(sorted.len());
        for instrument in &sorted {
            let time = year_fraction(reference, instrument.maturity(), day_count);
            match instrument {
                CalibrationInstrument::ZeroRate { rate, .. } => {
                    if !rate.is_finite() {
                        return Err(ValuationContextError::InvalidCurve(
                            "a quoted zero rate must be finite",
                        ));
                    }
                    nodes.push((time, *rate));
                }
                CalibrationInstrument::ParSwap {
                    maturity,
                    rate,
                    frequency,
                } => {
                    let rate = solve_par_swap_node(
                        reference, *maturity, *rate, *frequency, day_count, &nodes,
                    )?;
                    nodes.push((time, rate));
                }
            }
        }

        Self::from_zero_rates(reference, nodes, day_count)
    }

    /// The fixed rate that would make a swap of this maturity and frequency worth nothing on this
    /// curve — the curve's own view of where that swap trades.
    ///
    /// Repricing the instruments a curve was built from is how a bootstrap is checked, so this is
    /// deliberately public rather than hidden in a test.
    pub fn par_swap_rate(
        &self,
        maturity: Date,
        frequency: u32,
    ) -> Result<f64, ValuationContextError> {
        let (annuity, final_discount) = swap_legs(self, maturity, frequency)?;
        if annuity <= 0.0 {
            return Err(ValuationContextError::InvalidCurve(
                "swap annuity is not positive",
            ));
        }
        Ok((1.0 - final_discount) / annuity)
    }
}

/// Annuity (sum of accrual fraction times discount factor) and the discount factor at maturity.
fn swap_legs(
    curve: &YieldCurve,
    maturity: Date,
    frequency: u32,
) -> Result<(f64, f64), ValuationContextError> {
    let schedule = CouponSchedule::covering(curve.reference_date(), maturity, frequency)
        .map_err(ValuationContextError::Instrument)?;

    let mut annuity = 0.0;
    for index in 0..schedule.period_count() {
        let (start, end) = schedule
            .period(index)
            .expect("index below period_count is valid");
        let accrual = year_fraction(start, end, curve.day_count());
        annuity += accrual * curve.discount_factor(end)?;
    }
    Ok((annuity, curve.discount_factor(maturity)?))
}

/// Finds the zero rate at the swap's maturity that prices it at par.
///
/// Bisection rather than a Newton step: the objective is monotone in the rate over the searched
/// range, so bisection cannot run away, and a curve build that silently converges to the wrong
/// root would be worse than one that fails.
fn solve_par_swap_node(
    reference: Date,
    maturity: Date,
    quoted_rate: f64,
    frequency: u32,
    day_count: DayCountConvention,
    fixed_nodes: &[(f64, f64)],
) -> Result<f64, ValuationContextError> {
    if !quoted_rate.is_finite() {
        return Err(ValuationContextError::InvalidCurve(
            "a quoted swap rate must be finite",
        ));
    }

    let time = year_fraction(reference, maturity, day_count);
    let mismatch = |candidate: f64| -> Result<f64, ValuationContextError> {
        let mut nodes = fixed_nodes.to_vec();
        nodes.push((time, candidate));
        let curve = YieldCurve::from_zero_rates(reference, nodes, day_count)?;
        let (annuity, final_discount) = swap_legs(&curve, maturity, frequency)?;
        // Par condition: rate * annuity = 1 - discount(maturity).
        Ok(quoted_rate * annuity - (1.0 - final_discount))
    };

    let (mut low, mut high) = (-0.5, 1.0);
    let (low_value, high_value) = (mismatch(low)?, mismatch(high)?);
    if low_value.signum() == high_value.signum() {
        return Err(ValuationContextError::InvalidCurve(
            "no zero rate between -50% and +100% prices this swap at par",
        ));
    }

    // 100 halvings take the 1.5-wide bracket far below double precision; the loop is bounded so a
    // pathological objective cannot hang the build.
    for _ in 0..100 {
        let middle = 0.5 * (low + high);
        let value = mismatch(middle)?;
        if value == 0.0 {
            return Ok(middle);
        }
        if value.signum() == low_value.signum() {
            low = middle;
        } else {
            high = middle;
        }
    }
    Ok(0.5 * (low + high))
}
