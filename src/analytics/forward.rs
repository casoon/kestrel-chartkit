//! Forward-path statistics: what happened *after* comparable situations.
//!
//! Three questions a position judgement asks of history, each answered from observed paths rather
//! than from an assumed distribution:
//!
//! - **How much longer does a phase run, given its age?** [`conditional_end_probability`] —
//!   Kaplan–Meier over observed phase lengths, still-running phases counted as censored. The
//!   average length of finished phases does not answer it: a phase that has outlasted the average
//!   need not be about to end.
//! - **Which of two levels is touched first?** [`first_passage`] / [`summarize_passages`] — the
//!   path between start and horizon matters, not only where it ends: a close-to-close band says
//!   nothing about being stopped out on the way.
//! - **Where does the price stand after `h` bars, and how far does it travel before?**
//!   [`forward_move`], [`empirical_band`] and [`band_coverage`] — empirical quantile bands and the
//!   check whether an announced 80 % band later contains about 80 % of outcomes.
//!
//! All functions are deterministic and look only forward from the start index they are given;
//! choosing *which* starts are comparable is the caller's job.

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::stats::{rolling_quantile, wilson_interval, ProportionInterval};
use crate::Bar;

/// One observed phase: its length in bars and whether it has ended. A phase that is still
/// running when the data stops is right-censored (`ended = false`): it is known to have lasted
/// at least `bars`, not exactly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PhaseDuration {
    pub bars: usize,
    pub ended: bool,
}

/// Probability that a phase which has lasted `age` bars ends within the next `horizon` bars.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ConditionalEnd {
    pub probability: f64,
    /// Phases still running at `age` — the sample the estimate rests on.
    pub at_risk: usize,
    /// Greenwood standard error of `probability`.
    pub std_error: f64,
}

/// Kaplan–Meier estimate of `P(end in (age, age + horizon] | lasted past age)`.
///
/// With event lengths `t_j` (distinct lengths at which at least one phase ended), `d_j` phases
/// ending at `t_j` and `n_j` phases of length `≥ t_j`, the conditional survival is
/// `Π (1 − d_j/n_j)` over `age < t_j ≤ age + horizon`, and the probability is one minus that.
/// The Greenwood variance `S² · Σ d_j / (n_j (n_j − d_j))` over the same range gives the standard
/// error; a term with `n_j = d_j` (everyone at risk ends) is left out, since the variance is not
/// defined there and the estimate is exactly 1.
///
/// `None` if no phase lasted past `age`.
pub fn conditional_end_probability(
    durations: &[PhaseDuration],
    age: usize,
    horizon: usize,
) -> Option<ConditionalEnd> {
    let at_risk = durations.iter().filter(|d| d.bars > age).count();
    if at_risk == 0 {
        return None;
    }
    let mut event_lengths: Vec<usize> = durations
        .iter()
        .filter(|d| d.ended && d.bars > age && d.bars <= age + horizon)
        .map(|d| d.bars)
        .collect();
    event_lengths.sort_unstable();
    event_lengths.dedup();

    let mut survival = 1.0;
    let mut greenwood = 0.0;
    for t in event_lengths {
        let n = durations.iter().filter(|d| d.bars >= t).count() as f64;
        let d = durations.iter().filter(|x| x.ended && x.bars == t).count() as f64;
        survival *= 1.0 - d / n;
        if n > d {
            greenwood += d / (n * (n - d));
        }
    }
    Some(ConditionalEnd {
        probability: 1.0 - survival,
        at_risk,
        std_error: survival * greenwood.sqrt(),
    })
}

/// Which level a path touched first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub enum Passage {
    /// The upper level, on the given bar after the start (1 = the next bar).
    Upper {
        bars: usize,
    },
    Lower {
        bars: usize,
    },
    /// Neither within the horizon.
    Neither,
}

/// What to assume when one bar touches both levels. OHLC does not record the order; a backtest
/// that wants to be conservative assumes the level against the position came first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tie {
    LowerFirst,
    UpperFirst,
}

/// Scans `bars[start + 1 ..= start + horizon]` for the first bar whose high reaches `upper` or
/// whose low reaches `lower` (a gap beyond a level counts on the bar it happens). `None` if the
/// data ends before the horizon — an incomplete path would bias toward `Neither`.
pub fn first_passage(
    bars: &[Bar],
    start: usize,
    upper: f64,
    lower: f64,
    horizon: usize,
    tie: Tie,
) -> Option<Passage> {
    if start + horizon >= bars.len() {
        return None;
    }
    for (k, bar) in bars[start + 1..=start + horizon].iter().enumerate() {
        let hit_upper = bar.high >= upper;
        let hit_lower = bar.low <= lower;
        let bars_after = k + 1;
        match (hit_upper, hit_lower) {
            (true, true) => {
                return Some(match tie {
                    Tie::LowerFirst => Passage::Lower { bars: bars_after },
                    Tie::UpperFirst => Passage::Upper { bars: bars_after },
                })
            }
            (true, false) => return Some(Passage::Upper { bars: bars_after }),
            (false, true) => return Some(Passage::Lower { bars: bars_after }),
            (false, false) => {}
        }
    }
    Some(Passage::Neither)
}

/// Counts and shares of a set of passages.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct PassageSummary {
    pub n: usize,
    pub upper: usize,
    pub lower: usize,
    pub neither: usize,
    /// Share upper first, with a Wilson interval at the given `z`.
    pub p_upper: ProportionInterval,
    pub p_lower: ProportionInterval,
    /// Median bars to the level, over the paths that reached it.
    pub median_bars_upper: Option<f64>,
    pub median_bars_lower: Option<f64>,
}

/// Summary of `passages`; `None` if there are none or `z` is not positive.
pub fn summarize_passages(passages: &[Passage], z: f64) -> Option<PassageSummary> {
    let n = passages.len();
    let upper_bars: Vec<f64> = passages
        .iter()
        .filter_map(|p| match p {
            Passage::Upper { bars } => Some(*bars as f64),
            _ => None,
        })
        .collect();
    let lower_bars: Vec<f64> = passages
        .iter()
        .filter_map(|p| match p {
            Passage::Lower { bars } => Some(*bars as f64),
            _ => None,
        })
        .collect();
    let median = |v: &[f64]| (!v.is_empty()).then(|| rolling_quantile(v, 0.5));
    Some(PassageSummary {
        n,
        upper: upper_bars.len(),
        lower: lower_bars.len(),
        neither: n - upper_bars.len() - lower_bars.len(),
        p_upper: wilson_interval(upper_bars.len(), n, z)?,
        p_lower: wilson_interval(lower_bars.len(), n, z)?,
        median_bars_upper: median(&upper_bars),
        median_bars_lower: median(&lower_bars),
    })
}

/// The path from the close of `bars[start]` over the next `horizon` bars, in price units.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct ForwardMove {
    /// Close at the horizon minus the start close.
    pub close_change: f64,
    /// Highest high minus the start close (≥ 0 unless every high is below the start).
    pub max_up: f64,
    /// Start close minus the lowest low.
    pub max_down: f64,
}

/// `None` if the data ends before the horizon.
pub fn forward_move(bars: &[Bar], start: usize, horizon: usize) -> Option<ForwardMove> {
    if horizon == 0 || start + horizon >= bars.len() {
        return None;
    }
    let base = bars[start].close;
    let window = &bars[start + 1..=start + horizon];
    let high = window
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let low = window.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
    Some(ForwardMove {
        close_change: window.last()?.close - base,
        max_up: high - base,
        max_down: base - low,
    })
}

/// A central empirical band.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Band {
    pub lower: f64,
    pub upper: f64,
    /// The share of outcomes the band is meant to contain.
    pub coverage: f64,
}

/// The `(1 − coverage)/2` and `(1 + coverage)/2` quantiles of `samples` (linear interpolation
/// between order statistics, as in [`rolling_quantile`]). `None` for fewer than two finite
/// samples or a coverage outside `(0, 1)`.
pub fn empirical_band(samples: &[f64], coverage: f64) -> Option<Band> {
    let finite = samples.iter().filter(|v| v.is_finite()).count();
    if finite < 2 || !(coverage > 0.0 && coverage < 1.0) {
        return None;
    }
    Some(Band {
        lower: rolling_quantile(samples, (1.0 - coverage) / 2.0),
        upper: rolling_quantile(samples, (1.0 + coverage) / 2.0),
        coverage,
    })
}

/// Share of `(band, outcome)` pairs whose outcome lies within its band (bounds included) — the
/// check that an announced 80 % band contains about 80 % of what happened. `None` if empty.
pub fn band_coverage(pairs: &[(Band, f64)]) -> Option<f64> {
    if pairs.is_empty() {
        return None;
    }
    let inside = pairs
        .iter()
        .filter(|(band, outcome)| *outcome >= band.lower && *outcome <= band.upper)
        .count();
    Some(inside as f64 / pairs.len() as f64)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: close,
            high,
            low,
            close,
            volume: 0.0,
        }
    }

    #[test]
    fn censored_phases_count_as_at_risk_but_never_end() {
        let d = [
            PhaseDuration {
                bars: 10,
                ended: false,
            },
            PhaseDuration {
                bars: 10,
                ended: false,
            },
        ];
        let e = conditional_end_probability(&d, 5, 3).unwrap();
        assert_eq!(e.probability, 0.0);
        assert_eq!(e.at_risk, 2);
        assert_eq!(conditional_end_probability(&d, 10, 3), None);
    }

    #[test]
    fn a_tie_follows_the_rule() {
        let bars = vec![bar(100.0, 100.0, 100.0), bar(106.0, 94.0, 100.0)];
        assert_eq!(
            first_passage(&bars, 0, 105.0, 95.0, 1, Tie::LowerFirst),
            Some(Passage::Lower { bars: 1 })
        );
        assert_eq!(
            first_passage(&bars, 0, 105.0, 95.0, 1, Tie::UpperFirst),
            Some(Passage::Upper { bars: 1 })
        );
        // Not enough data for the horizon: no verdict rather than `Neither`.
        assert_eq!(
            first_passage(&bars, 0, 105.0, 95.0, 2, Tie::LowerFirst),
            None
        );
    }

    #[test]
    fn band_bounds_are_inclusive() {
        let band = Band {
            lower: -1.0,
            upper: 1.0,
            coverage: 0.8,
        };
        assert_eq!(
            band_coverage(&[(band, -1.0), (band, 1.0), (band, 1.5)]),
            Some(2.0 / 3.0)
        );
        assert_eq!(band_coverage(&[]), None);
        assert_eq!(empirical_band(&[1.0], 0.8), None);
        assert_eq!(empirical_band(&[1.0, 2.0], 1.0), None);
    }
}
