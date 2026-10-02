//! How many bars until an indicator's output no longer depends on where its input series began.
//!
//! [`Indicator::warmup_period`](super::Indicator::warmup_period) says when the *first* value
//! arrives. Recursive smoothing (Wilder/EMA state, running extremes, accumulated zones) keeps the
//! imprint of the starting point much longer: an RSI's 100-bar context line still differs by
//! several percent 200 bars after its first value, depending only on how much history preceded
//! it. Two consumers that start the same indicator at different points — a live process seeded
//! with a short history and a backtest warmed up with a longer one — then see different values on
//! the same bar.
//!
//! [`settle_bars`] answers the question empirically instead of per-indicator formulas: it runs the
//! indicator twice over each of several fixed reference series ([`REFERENCE_SHAPES`]), once from the
//! beginning and once starting [`LATE_STARTS`] bars later, and reports the latest bar (counted from the late start) after which every
//! numeric output of both runs agrees within [`RELATIVE_TOLERANCE`]. The measurement follows the
//! actual parameters and implementation, so it cannot drift from the code. It is a measurement on
//! one reference path, not a proof; the result carries a safety margin
//! ([`MARGIN_FACTOR`], [`MARGIN_BARS`]), and the test suite checks it against a second,
//! differently shaped series.
//!
//! `None` means the outputs never converge within the reference series, or only so late (beyond
//! half of it) that the number would not carry over to other paths: cumulative indicators
//! (OBV, PVT, CVD, NVI, accumulation/distribution), anchored ones (MIDAS) and counters over the
//! whole history. Their absolute level is defined only relative to a chosen start.

use std::collections::HashMap;

use crate::Bar;

use super::registry;

/// Relative difference (against the larger magnitude) below which two outputs count as equal.
pub const RELATIVE_TOLERANCE: f64 = 1e-3;

/// Length of the reference series.
const SERIES_LEN: usize = 4000;

/// The measured settle point is multiplied by this factor …
pub const MARGIN_FACTOR: f64 = 1.5;

/// … and this many bars are added.
pub const MARGIN_BARS: usize = 10;

/// Bars from the start of a series after which `name` (built with `params`) no longer depends on
/// where the series began, measured on bars spaced `bar_seconds` apart.
///
/// `None` if the indicator cannot be built from `params` or never converges within the reference
/// series. Always at least the indicator's `warmup_period`.
pub fn settle_bars(name: &str, params: &HashMap<String, f64>, bar_seconds: i64) -> Option<usize> {
    registry::build(name, params)?;
    settle_with(
        || registry::build(name, params).expect("built above"),
        bar_seconds,
        Criterion::Outputs,
    )
}

/// Like [`settle_bars`], but asks only whether the two runs fire the same alert kinds.
///
/// What a rule set trades on is the alerts, and they are coarser than the fourth decimal of an
/// output: the RSI's slow context line needs about five times longer to agree numerically than
/// the alerts derived from it. This is the measure for warming up a backtest window or seeding a
/// live process; [`settle_bars`] is the one for comparing stored values.
pub fn alert_settle_bars(
    name: &str,
    params: &HashMap<String, f64>,
    bar_seconds: i64,
) -> Option<usize> {
    registry::build(name, params)?;
    settle_with(
        || registry::build(name, params).expect("built above"),
        bar_seconds,
        Criterion::Alerts,
    )
}

/// What two runs must agree on to count as settled.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Criterion {
    /// Every numeric output within [`RELATIVE_TOLERANCE`], and the same `state`.
    Outputs,
    /// The same set of alert kinds on every bar.
    Alerts,
}

/// Both settle measures of one indicator.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Settle {
    /// See [`settle_bars`].
    pub outputs: Option<usize>,
    /// See [`alert_settle_bars`].
    pub alerts: Option<usize>,
}

impl Settle {
    pub fn get(&self, criterion: Criterion) -> Option<usize> {
        match criterion {
            Criterion::Outputs => self.outputs,
            Criterion::Alerts => self.alerts,
        }
    }
}

/// The measurement behind [`settle_bars`] and [`alert_settle_bars`] for an indicator built by
/// `build` — for consumers that construct indicators themselves rather than through
/// [`registry::build`]. `build` must return a fresh instance with identical parameters each call.
pub fn settle_with<F>(build: F, bar_seconds: i64, criterion: Criterion) -> Option<usize>
where
    F: Fn() -> Box<dyn super::Indicator>,
{
    settle_both_with(build, bar_seconds).get(criterion)
}

/// Both measures from the same runs — half the work of asking twice.
pub fn settle_both_with<F>(build: F, bar_seconds: i64) -> Settle
where
    F: Fn() -> Box<dyn super::Indicator>,
{
    let warmup = build().warmup_period();
    let mut outputs = Some(0);
    let mut alerts = Some(0);
    for shape in REFERENCE_SHAPES {
        let bars = shape.series(bar_seconds);
        let early = run(&mut build(), &bars);
        for late_start in LATE_STARTS {
            let (o, a) = compare(&early, &bars, late_start, &build);
            outputs = outputs.zip(o).map(|(x, y)| x.max(y));
            alerts = alerts.zip(a).map(|(x, y)| x.max(y));
        }
    }
    let finish = |measured: Option<usize>| {
        let with_margin = (measured? as f64 * MARGIN_FACTOR).ceil() as usize + MARGIN_BARS;
        // A settle point beyond half the reference series is not a reliable measurement: on
        // another path the imprint lasts longer still (zone counters, trailing states that reset
        // only on a flip). Treat it like a cumulative indicator.
        (with_margin <= SERIES_LEN / 2).then_some(with_margin.max(warmup))
    };
    Settle {
        outputs: finish(outputs),
        alerts: finish(alerts),
    }
}

/// One reference path: seed, drift per bar, length of each drift phase, noise scale.
#[derive(Debug, Clone, Copy)]
pub struct Shape {
    pub seed: u64,
    pub drift: f64,
    pub phase_len: usize,
    pub volatility: f64,
}

impl Shape {
    pub fn series(&self, bar_seconds: i64) -> Vec<Bar> {
        reference_series(
            bar_seconds,
            self.seed,
            self.drift,
            self.phase_len,
            self.volatility,
        )
    }
}

/// The paths [`settle_bars`] measures on — moderate trends, long strong trends, choppy noise and
/// a mixed case. How long a start leaves its imprint depends on the path (structure detectors
/// settle once the first pivot after the start is shared), so one path underestimates.
pub const REFERENCE_SHAPES: [Shape; 4] = [
    Shape {
        seed: 0x9e37_79b9_7f4a_7c15,
        drift: 0.08,
        phase_len: 90,
        volatility: 1.0,
    },
    Shape {
        seed: 0xd1b5_4a32_d192_ed03,
        drift: 0.25,
        phase_len: 200,
        volatility: 0.6,
    },
    Shape {
        seed: 0x8cb9_2ba7_2f3d_8dd7,
        drift: 0.02,
        phase_len: 40,
        volatility: 2.0,
    },
    Shape {
        seed: 0xbf58_476d_1ce4_e5b9,
        drift: 0.12,
        phase_len: 120,
        volatility: 1.4,
    },
];

/// Second-run start offsets, in bars.
pub const LATE_STARTS: [usize; 2] = [300, 1000];

/// Used by [`settle_bars`], [`alert_settle_bars`] and by the test that checks its result on a second series.
///
/// Bars (counted from the late start) up to and including the last one on which the two runs
/// disagree; `0` if they never disagree. `None` if they still disagree in the final fifth of the
/// series, i.e. the outputs do not converge.
pub fn measure(
    name: &str,
    params: &HashMap<String, f64>,
    bars: &[Bar],
    late_start: usize,
    criterion: Criterion,
) -> Option<usize> {
    registry::build(name, params)?;
    measure_with(
        &|| registry::build(name, params).expect("built above"),
        bars,
        late_start,
        criterion,
    )
}

/// [`measure`] for an indicator built by `build`.
pub fn measure_with<F>(
    build: &F,
    bars: &[Bar],
    late_start: usize,
    criterion: Criterion,
) -> Option<usize>
where
    F: Fn() -> Box<dyn super::Indicator>,
{
    let early = run(&mut build(), bars);
    let (outputs, alerts) = compare(&early, bars, late_start, build);
    match criterion {
        Criterion::Outputs => outputs,
        Criterion::Alerts => alerts,
    }
}

type Step = (Option<super::IndicatorOutput>, Vec<String>);

fn run(indicator: &mut Box<dyn super::Indicator>, bars: &[Bar]) -> Vec<Step> {
    bars.iter()
        .map(|bar| {
            let output = indicator.on_bar(bar);
            let mut kinds: Vec<String> = indicator.alerts().into_iter().map(|a| a.kind).collect();
            kinds.sort();
            (output, kinds)
        })
        .collect()
}

/// Last disagreement (outputs, alerts) of a run started at `late_start` against `early`; `None`
/// where the runs still disagree in the final fifth.
fn compare<F>(
    early: &[Step],
    bars: &[Bar],
    late_start: usize,
    build: &F,
) -> (Option<usize>, Option<usize>)
where
    F: Fn() -> Box<dyn super::Indicator>,
{
    let mut late_indicator = build();
    let late = run(&mut late_indicator, &bars[late_start..]);
    let late_warmup = late_indicator.warmup_period();
    let mut last_outputs: Option<usize> = None;
    let mut last_alerts: Option<usize> = None;
    for (j, (late_value, late_alerts)) in late.iter().enumerate() {
        let (early_value, early_alerts) = &early[late_start + j];
        let outputs_disagree = match (early_value, late_value) {
            (Some(a), Some(b)) => outputs_differ(a, b),
            // One run has a value, the other not: only a disagreement once the late run is past
            // its own warmup, otherwise it is the expected head start of the early run.
            (Some(_), None) | (None, Some(_)) => j >= late_warmup,
            (None, None) => false,
        };
        if outputs_disagree {
            last_outputs = Some(j + 1);
        }
        if early_alerts != late_alerts {
            last_alerts = Some(j + 1);
        }
    }
    let settled = |last: Option<usize>| match last {
        Some(j) if j > late.len() * 4 / 5 => None,
        Some(j) => Some(j),
        None => Some(0),
    };
    (settled(last_outputs), settled(last_alerts))
}

fn outputs_differ(a: &super::IndicatorOutput, b: &super::IndicatorOutput) -> bool {
    let mut pairs = vec![(a.value, b.value)];
    if let (Some(x), Some(y)) = (a.secondary, b.secondary) {
        pairs.push((x, y));
    }
    if let (Some(x), Some(y)) = (a.signal, b.signal) {
        pairs.push((x, y));
    }
    for (key, x) in &a.extra {
        if let Some(y) = b.extra.get(key) {
            pairs.push((*x, *y));
        }
    }
    pairs
        .into_iter()
        .any(|(x, y)| relative_difference(x, y) > RELATIVE_TOLERANCE)
        || a.state != b.state
}

fn relative_difference(x: f64, y: f64) -> f64 {
    if x.is_nan() && y.is_nan() {
        return 0.0;
    }
    (x - y).abs() / x.abs().max(y.abs()).max(1e-9)
}

/// Reference series for [`settle_bars`] and its tests.
///
/// Deterministic random walk with alternating drift phases (`phase_len` bars each), asymmetric
/// wicks and volume, noise scaled by `volatility`. `seed` must be non-zero.
pub fn reference_series(
    bar_seconds: i64,
    seed: u64,
    drift: f64,
    phase_len: usize,
    volatility: f64,
) -> Vec<Bar> {
    let mut state = seed;
    let mut noise = move || {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        (state % 10_000) as f64 / 10_000.0 - 0.5
    };
    let mut close: f64 = 100.0;
    (0..SERIES_LEN)
        .map(|i| {
            let open = close + noise() * 0.3;
            let phase_drift = if (i / phase_len).is_multiple_of(2) {
                drift
            } else {
                -drift
            };
            close = (open + noise() * 1.5 * volatility + phase_drift).max(5.0);
            let high = open.max(close) + (noise() + 0.5) * 0.9 * volatility;
            let low = open.min(close) - (noise() + 0.5) * 0.6 * volatility;
            Bar {
                timestamp: 1_700_000_000 + i as i64 * bar_seconds,
                open,
                high,
                low,
                close,
                volume: 1000.0 + (noise() + 0.5) * 900.0,
            }
        })
        .collect()
}
