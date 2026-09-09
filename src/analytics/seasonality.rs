#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::finance::Date;
use crate::model::Bar;

/// One calendar month's return, measured close to close across the month boundary.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MonthlyReturn {
    pub year: i32,
    /// 1 through 12.
    pub month: u32,
    /// Percentage change from the previous month's last close to this month's last close.
    pub return_pct: f64,
    /// Whether this month's last observed bar is known to be followed by another month.
    ///
    /// The final month of a series is `false`: its last bar may or may not be the month's last
    /// trading day, and this cannot be told from the data alone. Such a month is excluded from
    /// the statistics rather than counted as if it had closed.
    pub complete: bool,
}

/// What the completed observations of one calendar month say.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MonthStatistics {
    /// 1 through 12.
    pub month: u32,
    /// How many completed years contributed. Published because a mean over three years and one
    /// over thirty are not the same claim.
    pub samples: usize,
    pub mean_return_pct: f64,
    /// Sample standard deviation over the years, `None` with fewer than two of them — a spread
    /// needs at least two observations, and zero would suggest certainty.
    pub stdev_return_pct: Option<f64>,
    /// Share of contributing years in which the month closed higher, in `0..=1`.
    ///
    /// A frequency, not a calibrated probability: it says what happened in these `samples` years,
    /// not what is likely to happen next.
    pub positive_share: f64,
}

/// Monthly seasonality of a price series: what each calendar month did, historically.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SeasonalityReport {
    /// Every month found in the series, oldest first, including the incomplete final one.
    pub monthly_returns: Vec<MonthlyReturn>,
    /// One entry per calendar month that has at least one completed observation, ascending by
    /// month number.
    pub months: Vec<MonthStatistics>,
    /// Timestamp of the last bar the report was computed from — the data cut-off it belongs to.
    pub as_of: i64,
    /// How many completed monthly returns went into the statistics.
    pub completed_months: usize,
}

/// Computes the monthly seasonality of a bar series.
///
/// A month's return runs from the last close of the previous month to the last close of that
/// month. The first month of a series therefore has no return — there is nothing before it to
/// measure against — and the last month is marked incomplete, because whether its final bar is
/// the month's final bar cannot be known from the series.
///
/// This is descriptive statistics and stops there. There is no projection, no expected return for
/// a coming month, and `positive_share` is a frequency over the years present, not a probability
/// that the next one will be positive. Turning a frequency into a forecast needs assumptions this
/// function is not in a position to make.
///
/// Bars must be in ascending time order; a price adjustment convention (raw, back-adjusted,
/// total return) changes the answer and belongs to the caller's series, not to this computation —
/// see [`crate::model::SeriesIdentity`].
pub fn monthly_seasonality(bars: &[Bar]) -> SeasonalityReport {
    let mut monthly_returns = Vec::new();
    let as_of = bars.last().map(|bar| bar.timestamp).unwrap_or(0);

    // Last close of each calendar month, in order of appearance.
    let mut month_closes: Vec<((i32, u32), f64)> = Vec::new();
    for bar in bars {
        if !bar.close.is_finite() || bar.close <= 0.0 {
            continue;
        }
        let date = date_of(bar.timestamp);
        let key = (date.year, date.month);
        match month_closes.last_mut() {
            Some((last_key, close)) if *last_key == key => *close = bar.close,
            _ => month_closes.push((key, bar.close)),
        }
    }

    for index in 1..month_closes.len() {
        let ((year, month), close) = month_closes[index];
        let previous_close = month_closes[index - 1].1;
        if previous_close <= 0.0 {
            continue;
        }
        monthly_returns.push(MonthlyReturn {
            year,
            month,
            return_pct: 100.0 * (close / previous_close - 1.0),
            complete: index + 1 < month_closes.len(),
        });
    }

    let mut months = Vec::new();
    for month in 1..=12u32 {
        let returns: Vec<f64> = monthly_returns
            .iter()
            .filter(|entry| entry.month == month && entry.complete)
            .map(|entry| entry.return_pct)
            .collect();
        if returns.is_empty() {
            continue;
        }

        let samples = returns.len();
        let mean = returns.iter().sum::<f64>() / samples as f64;
        let stdev = (samples >= 2).then(|| {
            let variance = returns
                .iter()
                .map(|value| {
                    let diff = value - mean;
                    diff * diff
                })
                .sum::<f64>()
                / (samples - 1) as f64;
            variance.sqrt()
        });
        let positive = returns.iter().filter(|value| **value > 0.0).count();

        months.push(MonthStatistics {
            month,
            samples,
            mean_return_pct: mean,
            stdev_return_pct: stdev,
            positive_share: positive as f64 / samples as f64,
        });
    }

    let completed_months = monthly_returns
        .iter()
        .filter(|entry| entry.complete)
        .count();
    SeasonalityReport {
        monthly_returns,
        months,
        as_of,
        completed_months,
    }
}

/// Calendar date of a Unix timestamp, in UTC.
fn date_of(timestamp: i64) -> Date {
    let epoch = Date::new(1970, 1, 1).expect("the epoch is a valid date");
    epoch.add_days(timestamp.div_euclid(86_400))
}
