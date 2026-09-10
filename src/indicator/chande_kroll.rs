use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::{ExtremeWindow, Rma};
use super::{Indicator, IndicatorOutput};

/// Chande Kroll Stop: two stop levels built from window extremes and a smoothed true range, then
/// passed through a second extreme window.
///
/// First stage, over `atr_len` bars:
///
/// ```text
/// preliminary_long  = highest(high, atr_len) - mult * ATR(atr_len)
/// preliminary_short = lowest(low,  atr_len) + mult * ATR(atr_len)
/// ```
///
/// Second stage, over `stop_len` values of those series:
///
/// ```text
/// stop_long  = highest(preliminary_long,  stop_len)
/// stop_short = lowest(preliminary_short, stop_len)
/// ```
///
/// The names follow the economic role: `stop_long` is the line *below* price, where a long
/// position would be given up, and `stop_short` the line *above* price for a short one. Other
/// implementations pair the names with the opposite extremes; the mapping here is the one pinned
/// by this crate's reference fixture, and no parity with any other implementation is claimed.
///
/// The true range is smoothed the Wilder way ([`Rma`]), the same convention this crate's ATR and
/// Chandelier Exit use. A simple moving average over the true range is a different default found
/// elsewhere and would produce different levels under the same name.
///
/// This is not a Chandelier Exit: that one ratchets a single stop and flips direction. Here both
/// lines are published as they come out of the formula. They can cross — in a narrow range the
/// two stops can end up on the wrong side of each other — and that is left visible: sorting,
/// ratcheting or clamping them would add a position decision this indicator does not make.
///
/// Per-bar outputs, all in the price units of the series:
/// - `value` and `extra["stop_long"]`: the long stop.
/// - `extra["stop_short"]`: the short stop.
///
/// First output: with bar `atr_len + stop_len - 1` — the first stage has its first value with bar
/// `atr_len` (the Wilder seed takes `atr_len` true ranges, the first bar's being its
/// `high - low`), and the second stage needs `stop_len` of those values. [`Indicator::reset`]
/// clears both stages, so the next series starts deterministically.
#[derive(Debug, Clone)]
pub struct ChandeKrollStop {
    atr_len: usize,
    stop_len: usize,
    mult: f64,
    prev_close: Option<f64>,
    tr_rma: Rma,
    high_window: ExtremeWindow,
    low_window: ExtremeWindow,
    long_window: ExtremeWindow,
    short_window: ExtremeWindow,
}

impl ChandeKrollStop {
    pub fn new(atr_len: usize, stop_len: usize, mult: f64) -> Self {
        let atr_len = atr_len.max(1);
        let stop_len = stop_len.max(1);
        Self {
            atr_len,
            stop_len,
            mult,
            prev_close: None,
            tr_rma: Rma::new(atr_len),
            high_window: ExtremeWindow::new(atr_len),
            low_window: ExtremeWindow::new(atr_len),
            long_window: ExtremeWindow::new(stop_len),
            short_window: ExtremeWindow::new(stop_len),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 9, 3.0)
    }
}

impl Indicator for ChandeKrollStop {
    fn name(&self) -> &str {
        "chande_kroll"
    }

    fn warmup_period(&self) -> usize {
        self.atr_len + self.stop_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let tr = match self.prev_close {
            None => bar.high - bar.low,
            Some(prev_close) => (bar.high - bar.low)
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs()),
        };
        self.prev_close = Some(bar.close);

        let highest_high = self.high_window.push(bar.high).map(|(_, high)| high);
        let lowest_low = self.low_window.push(bar.low).map(|(low, _)| low);
        let atr = self.tr_rma.update(tr);

        let (Some(highest_high), Some(lowest_low), Some(atr)) = (highest_high, lowest_low, atr)
        else {
            return None;
        };

        let preliminary_long = highest_high - self.mult * atr;
        let preliminary_short = lowest_low + self.mult * atr;

        let stop_long = self
            .long_window
            .push(preliminary_long)
            .map(|(_, high)| high);
        let stop_short = self
            .short_window
            .push(preliminary_short)
            .map(|(low, _)| low);

        let (Some(stop_long), Some(stop_short)) = (stop_long, stop_short) else {
            return None;
        };

        let extra = HashMap::from([
            ("stop_long".to_string(), stop_long),
            ("stop_short".to_string(), stop_short),
        ]);
        Some(IndicatorOutput::with_extra(stop_long, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.tr_rma.reset();
        self.high_window.reset();
        self.low_window.reset();
        self.long_window.reset();
        self.short_window.reset();
    }
}
