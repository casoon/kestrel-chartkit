//! Price gaps between consecutive bars: detection, size in ATR, the filled fraction of every open
//! gap, and closing as an event.
//!
//! A gap is a price range between two consecutive bars in which nothing traded: the later bar
//! opens beyond the earlier one. Which gaps a series shows is a property of the series first
//! (session cut, bar period, contract rolls, price adjustment) and of the market second;
//! [`crate::applicability`] names the series classes on which the result is limited.
//!
//! The classical kinds of gap (common, breakaway, runaway, exhaustion) are assigned in hindsight
//! from what follows a gap. This detector does not name them: it reports the gap, its size and
//! whether and when it was closed.
//!
//! # The calculation
//!
//! All comparisons are causal: a bar only reads itself and earlier bars.
//!
//! ```text
//! atr_prev = Wilder average of the true range over atr_len bars, as of the previous bar
//!
//! reference = range:   upper = high[i-1]     lower = low[i-1]
//! reference = close:   upper = close[i-1]    lower = close[i-1]
//!
//! gap up   on bar i:   open[i] > upper       bottom = upper      top = open[i]
//! gap down on bar i:   open[i] < lower       bottom = open[i]    top = lower
//!
//! size     = top - bottom
//! size_atr = size / atr_prev
//! the gap counts when size >= min_atr * atr_prev
//!
//! every gap that counts is followed on its own bar and on the max_bars bars after it:
//!     gap up:    closed on the first bar with low <= bottom
//!                fill = (top - lowest low since the gap bar) / size
//!     gap down:  closed on the first bar with high >= top
//!                fill = (highest high since the gap bar - bottom) / size
//!     fill is limited to 0..1 and is 1 for a closed gap
//! a gap that is still open after max_bars bars is dropped
//! ```
//!
//! The lows and highs "since the gap bar" include the gap bar itself, so a gap can be closed on
//! the bar that forms it (0 bars). The first `atr_len` bars only feed the ATR; the first output
//! comes with bar `atr_len + 1`, the first bar whose predecessor has an ATR.
//!
//! # Conventions
//!
//! Decisions of this crate where usage differs or the literature names no number:
//!
//! - **Reference.** Both definitions are in use: the open against the previous bar's high and low
//!   (the empty space visible on a bar chart, default) and the open against the previous close.
//!   They yield different sets of gaps and different edges to reach. A third, stricter reading
//!   asks for the whole bar beyond the previous one (`low[i] > high[i-1]`); those are exactly the
//!   gaps of `reference = range` that are not closed on their own bar.
//! - **ATR of the previous bar.** That value is known at the open. The gap bar's own true range
//!   contains the distance to the previous close and with it the gap, so its ATR would measure the
//!   gap against itself.
//! - **`min_atr = 0`.** No minimum size is established; the default counts every gap. With
//!   `reference = close` and no minimum nearly every bar forms a gap.
//! - **`max_bars = 100`.** "Closed" is only a testable statement with a deadline, and no deadline
//!   is established. The default keeps a gap on a chart of usual length until it is closed and
//!   bounds both the memory and the dependence on the start of the series: after `max_bars` bars
//!   the output no longer depends on gaps formed before the first bar.
//! - **While the ATR is zero** (no bar so far had a range) a gap has no size in ATR: it counts for
//!   every `min_atr`, and `size_atr` is absent.
//!
//! # Output
//!
//! `value` is the gap formed on this bar: 1 up, -1 down, 0 none. `extra`:
//!
//! | key | present | meaning |
//! |---|---|---|
//! | `atr_prev` | always | the ATR as of the previous bar, in price units |
//! | `gap_top`, `gap_bottom` | on a gap bar | the edges of the gap formed on this bar |
//! | `size` | on a gap bar | `gap_top - gap_bottom` |
//! | `size_atr` | on a gap bar while `atr_prev > 0` | `size / atr_prev` |
//! | `open_count` | always | gaps still open after this bar |
//! | `open_<n>_direction`, `open_<n>_top`, `open_<n>_bottom`, `open_<n>_fill`, `open_<n>_bars` | for `n = 1..=open_count`, the youngest gap first | direction (1 up, -1 down), edges, filled fraction (`0 <= fill < 1`) and bars since the gap bar (0 on the gap bar) |
//! | `closed_count` | always | gaps closed on this bar |
//! | `closed_<n>_bars` | for `n = 1..=closed_count`, the youngest gap first | bars from the gap bar to this bar (0: closed on the bar that formed it) |
//!
//! At most one gap forms per bar, so the bars since its gap bar identify a gap. Alerts: `gap_up`
//! and `gap_down` on the bar a gap forms, `gap_closed` on a bar that closes at least one.
//! [`GapEngine::open_gaps`] and [`GapEngine::closed_gaps`] return the same gaps as [`Gap`] values.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// What the open of a bar is compared with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GapReference {
    /// The previous bar's high and low: the open lies outside the previous bar.
    Range,
    /// The previous bar's close.
    Close,
}

/// Every parameter of the calculation; see the module documentation for the reasons behind the
/// defaults.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GapParams {
    /// What the open is compared with. Default [`GapReference::Range`].
    pub reference: GapReference,
    /// Bars of the Wilder ATR the gap size is measured in. Default 14, Wilder's own length.
    pub atr_len: usize,
    /// Smallest gap that counts, in ATR of the previous bar. Default 0: every gap counts.
    pub min_atr: f64,
    /// Bars after its own on which a gap is followed. Default 100.
    pub max_bars: usize,
}

impl Default for GapParams {
    fn default() -> Self {
        Self {
            reference: GapReference::Range,
            atr_len: 14,
            min_atr: 0.0,
            max_bars: 100,
        }
    }
}

/// Direction of a gap: the open above (`Up`) or below (`Down`) the reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum GapDirection {
    Up,
    Down,
}

impl GapDirection {
    /// `1.0` for up, `-1.0` for down: the code used in `value` and `open_<n>_direction`.
    pub fn code(self) -> f64 {
        match self {
            GapDirection::Up => 1.0,
            GapDirection::Down => -1.0,
        }
    }
}

/// One gap and how far it has been filled.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Gap {
    pub direction: GapDirection,
    /// Upper edge: the open of the gap bar (gap up) or the reference (gap down).
    pub top: f64,
    /// Lower edge: the reference (gap up) or the open of the gap bar (gap down).
    pub bottom: f64,
    /// `size / atr_prev`; `None` while the ATR of the previous bar was zero.
    pub size_atr: Option<f64>,
    /// The bar that formed the gap, counted from 1 since the engine started or was reset.
    pub bar: usize,
    /// Filled fraction, `0..=1`; `1` exactly when the gap is closed.
    pub fill: f64,
    /// Bars from the gap bar to the bar that closed it (0: the gap bar itself); `None` while open.
    pub closed_after: Option<usize>,
}

impl Gap {
    /// `top - bottom`, in price units.
    pub fn size(&self) -> f64 {
        self.top - self.bottom
    }
}

/// High, low and close of the previous bar.
#[derive(Debug, Clone, Copy)]
struct Previous {
    high: f64,
    low: f64,
    close: f64,
}

/// Streaming gap detector; the calculation is described in the module documentation.
#[derive(Debug, Clone)]
pub struct GapEngine {
    params: GapParams,
    atr: Rma,
    /// The ATR after the last bar: `atr_prev` of the next one.
    atr_value: Option<f64>,
    previous: Option<Previous>,
    index: usize,
    open: Vec<Gap>,
    closed: Vec<Gap>,
    alerts: Vec<IndicatorAlert>,
}

impl GapEngine {
    pub fn new(params: GapParams) -> Self {
        let params = GapParams {
            atr_len: params.atr_len.max(1),
            ..params
        };
        Self {
            params,
            atr: Rma::new(params.atr_len),
            atr_value: None,
            previous: None,
            index: 0,
            open: Vec::new(),
            closed: Vec::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(GapParams::default())
    }

    pub fn params(&self) -> GapParams {
        self.params
    }

    /// The gaps still open after the last bar, oldest first.
    pub fn open_gaps(&self) -> &[Gap] {
        &self.open
    }

    /// The gaps closed on the last bar, oldest first.
    pub fn closed_gaps(&self) -> &[Gap] {
        &self.closed
    }

    /// The gap bar `bar` forms against `previous`, if any: direction, top and bottom.
    fn formed(&self, bar: &Bar, previous: Previous) -> Option<(GapDirection, f64, f64)> {
        let (upper, lower) = match self.params.reference {
            GapReference::Range => (previous.high, previous.low),
            GapReference::Close => (previous.close, previous.close),
        };
        if bar.open > upper {
            Some((GapDirection::Up, bar.open, upper))
        } else if bar.open < lower {
            Some((GapDirection::Down, lower, bar.open))
        } else {
            None
        }
    }
}

impl Indicator for GapEngine {
    fn name(&self) -> &str {
        "gap"
    }

    /// The first bar whose predecessor has an ATR.
    fn warmup_period(&self) -> usize {
        self.params.atr_len + 1
    }

    fn reset(&mut self) {
        self.atr.reset();
        self.atr_value = None;
        self.previous = None;
        self.index = 0;
        self.open.clear();
        self.closed.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.closed.clear();
        self.index += 1;
        let index = self.index;

        let previous = self.previous.replace(Previous {
            high: bar.high,
            low: bar.low,
            close: bar.close,
        });
        let true_range = match previous {
            Some(p) => (bar.high - bar.low)
                .max((bar.high - p.close).abs())
                .max((bar.low - p.close).abs()),
            None => bar.high - bar.low,
        };
        let atr_prev = self.atr_value;
        self.atr_value = self.atr.update(true_range);
        let (previous, atr_prev) = (previous?, atr_prev?);

        let max_bars = self.params.max_bars;
        self.open.retain(|gap| index - gap.bar <= max_bars);

        let formed = self
            .formed(bar, previous)
            .filter(|(_, top, bottom)| top - bottom >= self.params.min_atr * atr_prev);
        if let Some((direction, top, bottom)) = formed {
            self.open.push(Gap {
                direction,
                top,
                bottom,
                size_atr: (atr_prev > 0.0).then(|| (top - bottom) / atr_prev),
                bar: index,
                fill: 0.0,
                closed_after: None,
            });
            let (kind, word) = match direction {
                GapDirection::Up => ("gap_up", "up"),
                GapDirection::Down => ("gap_down", "down"),
            };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!("Gap {word}: {bottom:.4} to {top:.4}"),
                1.0,
            ));
        }

        for gap in &mut self.open {
            let (reached, depth) = match gap.direction {
                GapDirection::Up => (bar.low <= gap.bottom, gap.top - bar.low),
                GapDirection::Down => (bar.high >= gap.top, bar.high - gap.bottom),
            };
            if reached {
                gap.fill = 1.0;
                gap.closed_after = Some(index - gap.bar);
            } else {
                gap.fill = gap.fill.max((depth / gap.size()).clamp(0.0, 1.0));
            }
        }
        let (closed, open) = std::mem::take(&mut self.open)
            .into_iter()
            .partition(|gap| gap.closed_after.is_some());
        self.closed = closed;
        self.open = open;
        if !self.closed.is_empty() {
            self.alerts.push(IndicatorAlert::new(
                "gap_closed",
                format!("Gap closed: {} on this bar", self.closed.len()),
                1.0,
            ));
        }

        let mut extra = HashMap::new();
        extra.insert("atr_prev".to_string(), atr_prev);
        if let Some((_, top, bottom)) = formed {
            extra.insert("gap_top".to_string(), top);
            extra.insert("gap_bottom".to_string(), bottom);
            extra.insert("size".to_string(), top - bottom);
            if atr_prev > 0.0 {
                extra.insert("size_atr".to_string(), (top - bottom) / atr_prev);
            }
        }
        extra.insert("open_count".to_string(), self.open.len() as f64);
        for (n, gap) in self.open.iter().rev().enumerate() {
            let n = n + 1;
            extra.insert(format!("open_{n}_direction"), gap.direction.code());
            extra.insert(format!("open_{n}_top"), gap.top);
            extra.insert(format!("open_{n}_bottom"), gap.bottom);
            extra.insert(format!("open_{n}_fill"), gap.fill);
            extra.insert(format!("open_{n}_bars"), (index - gap.bar) as f64);
        }
        extra.insert("closed_count".to_string(), self.closed.len() as f64);
        for (n, gap) in self.closed.iter().rev().enumerate() {
            extra.insert(format!("closed_{}_bars", n + 1), (index - gap.bar) as f64);
        }
        let value = formed.map_or(0.0, |(direction, _, _)| direction.code());
        Some(IndicatorOutput::with_extra(value, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const GAP_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "gap",
        aliases: &[],
        description: "Price gap between consecutive bars: the open beyond the previous bar's range or close, size in ATR of the previous bar, every open gap with its filled fraction, closing as an event",
        params: &[
            super::definition::ParamSpec::choice("reference", "range", &["range", "close"], true)
                .meaning("what the open is compared with: the previous bar's high and low, or its close"),
            super::definition::ParamSpec::count("atr_len", 14, 1, 10000)
                .meaning("bars of the Wilder ATR the gap size is measured in"),
            super::definition::ParamSpec::number("min_atr", 0.0, 0.0, 1000.0)
                .meaning("smallest gap that counts, in ATR of the previous bar; 0 counts every gap"),
            super::definition::ParamSpec::count("max_bars", 100, 1, 10000)
                .meaning("bars after its own on which a gap is followed; a gap still open then is dropped"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(GapEngine::new(GapParams {
                reference: match p.choice("reference") {
                    "close" => GapReference::Close,
                    _ => GapReference::Range,
                },
                atr_len: p.count("atr_len"),
                min_atr: p.number("min_atr"),
                max_bars: p.count("max_bars"),
            })))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(0, open, high, low, close, 0.0)
    }

    /// Bars of range 2 around 100 (ATR 2 from the first bar on), then the gap bars given.
    fn run(params: GapParams, tail: &[Bar]) -> (GapEngine, Vec<IndicatorOutput>) {
        let mut engine = GapEngine::new(params);
        for _ in 0..params.atr_len {
            assert!(engine.on_bar(&bar(100.0, 101.0, 99.0, 100.0)).is_none());
        }
        let outputs = tail
            .iter()
            .map(|b| engine.on_bar(b).expect("output after the warmup"))
            .collect();
        (engine, outputs)
    }

    #[test]
    fn test_gap_up_is_measured_in_the_atr_of_the_previous_bar() {
        let (engine, out) = run(GapParams::default(), &[bar(104.0, 105.0, 103.5, 104.5)]);
        assert_eq!(out[0].value, 1.0);
        assert_eq!(out[0].extra["atr_prev"], 2.0);
        assert_eq!(out[0].extra["gap_bottom"], 101.0);
        assert_eq!(out[0].extra["gap_top"], 104.0);
        assert_eq!(out[0].extra["size"], 3.0);
        assert_eq!(out[0].extra["size_atr"], 1.5);
        assert_eq!(out[0].extra["open_count"], 1.0);
        assert!((out[0].extra["open_1_fill"] - 0.5 / 3.0).abs() < 1e-12);
        assert_eq!(out[0].extra["open_1_bars"], 0.0);
        assert_eq!(out[0].extra["closed_count"], 0.0);
        let gap = engine.open_gaps()[0];
        assert_eq!((gap.direction, gap.bar), (GapDirection::Up, 15));
        assert_eq!(gap.size_atr, Some(1.5));
        assert_eq!(engine.alerts()[0].kind, "gap_up");
    }

    #[test]
    fn test_gap_closes_when_the_edge_is_reached_not_before() {
        let (engine, out) = run(
            GapParams::default(),
            &[
                bar(104.0, 105.0, 103.5, 104.5),
                bar(104.0, 104.5, 101.5, 102.0),
                bar(102.0, 102.5, 101.0, 101.5),
            ],
        );
        assert_eq!(out[1].extra["open_count"], 1.0);
        assert!((out[1].extra["open_1_fill"] - 2.5 / 3.0).abs() < 1e-12);
        assert_eq!(out[2].extra["open_count"], 0.0);
        assert_eq!(out[2].extra["closed_count"], 1.0);
        assert_eq!(out[2].extra["closed_1_bars"], 2.0);
        assert!(engine.open_gaps().is_empty());
        let closed = engine.closed_gaps()[0];
        assert_eq!((closed.fill, closed.closed_after), (1.0, Some(2)));
        assert_eq!(engine.alerts()[0].kind, "gap_closed");
    }

    #[test]
    fn test_gap_closed_on_its_own_bar_is_reported_with_zero_bars() {
        let (engine, out) = run(GapParams::default(), &[bar(102.0, 102.5, 100.5, 101.0)]);
        assert_eq!(out[0].value, 1.0);
        assert_eq!(out[0].extra["open_count"], 0.0);
        assert_eq!(out[0].extra["closed_1_bars"], 0.0);
        let kinds: Vec<String> = engine.alerts().into_iter().map(|a| a.kind).collect();
        assert_eq!(kinds, ["gap_up", "gap_closed"]);
    }

    #[test]
    fn test_reference_close_moves_the_edge_to_the_previous_close() {
        let params = GapParams {
            reference: GapReference::Close,
            ..GapParams::default()
        };
        // Inside the previous range (99 to 101), so no gap against the range.
        let inside = [bar(100.5, 101.0, 100.2, 100.8)];
        assert_eq!(run(GapParams::default(), &inside).1[0].value, 0.0);
        let out = run(params, &inside).1;
        assert_eq!(out[0].value, 1.0);
        assert_eq!(out[0].extra["gap_bottom"], 100.0);
        assert_eq!(out[0].extra["gap_top"], 100.5);
    }

    #[test]
    fn test_minimum_size_is_inclusive() {
        let gap_bar = [bar(104.0, 105.0, 103.5, 104.5)];
        let at = GapParams {
            min_atr: 1.5,
            ..GapParams::default()
        };
        assert_eq!(run(at, &gap_bar).1[0].value, 1.0);
        let above = GapParams {
            min_atr: 1.6,
            ..GapParams::default()
        };
        let out = &run(above, &gap_bar).1[0];
        assert_eq!(out.value, 0.0);
        assert!(!out.extra.contains_key("gap_top"));
    }

    #[test]
    fn test_gap_is_dropped_after_max_bars_without_an_event() {
        let params = GapParams {
            max_bars: 2,
            ..GapParams::default()
        };
        let quiet = bar(104.5, 105.0, 104.0, 104.5);
        let (engine, out) = run(
            params,
            &[
                bar(104.0, 105.0, 103.5, 104.5),
                quiet.clone(),
                quiet.clone(),
                quiet,
            ],
        );
        assert_eq!(out[2].extra["open_count"], 1.0);
        assert_eq!(out[2].extra["open_1_bars"], 2.0);
        assert_eq!(out[3].extra["open_count"], 0.0);
        assert_eq!(out[3].extra["closed_count"], 0.0);
        assert!(engine.alerts().is_empty());
    }

    #[test]
    fn test_without_a_range_so_far_a_gap_has_no_size_in_atr() {
        let mut engine = GapEngine::new(GapParams {
            min_atr: 5.0,
            ..GapParams::default()
        });
        for _ in 0..14 {
            engine.on_bar(&bar(100.0, 100.0, 100.0, 100.0));
        }
        let out = engine.on_bar(&bar(100.5, 100.5, 100.5, 100.5)).unwrap();
        assert_eq!(out.value, 1.0);
        assert_eq!(out.extra["atr_prev"], 0.0);
        assert!(!out.extra.contains_key("size_atr"));
        assert_eq!(engine.open_gaps()[0].size_atr, None);
    }
}
