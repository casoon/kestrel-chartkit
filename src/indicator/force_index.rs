use std::collections::HashMap;

use crate::model::Bar;

use super::smoothing::Ema;
use super::{Indicator, IndicatorOutput};

/// Elder's Force Index: the bar-to-bar price change weighted by the volume that moved it.
///
/// Raw value: `(close_t - close_{t-1}) * volume_t`. There is no output on the first bar of a
/// series, because a change needs a previous close — a first bar is not a zero-force bar.
/// The main line is that raw series smoothed with an [`Ema`] of `ema_len`.
///
/// Unit: price change times volume, i.e. the series' price unit multiplied by whatever the
/// series counts as volume (traded turnover, contracts, or — on a tick-volume series — update
/// counts, which makes the magnitude meaningless even though the sign still reads). Values from
/// different instruments or volume kinds are not comparable; the sign and the zero crossing are.
/// See [`crate::applicability::data_requirements`], which declares real traded volume for this
/// indicator.
///
/// A bar with zero volume, or one that closes exactly where the previous bar closed, has a raw
/// force of 0. That zero enters the average as an ordinary observation: it pulls the smoothed
/// line towards zero, it does not set it to zero.
///
/// Per-bar outputs:
/// - `value`: the smoothed force index.
/// - `extra["raw"]`: the unsmoothed force of this bar, in the same unit.
///
/// First output: with the `ema_len`-th price change, i.e. after `ema_len + 1` bars. The internal
/// EMA runs from the first change onward — seeded with that first real observation, not with an
/// invented starting value — but its seed-dominated early values are not published.
/// [`Indicator::reset`] clears the previous close and the average, so the next series starts
/// deterministically; a series switch must go through it rather than continuing the average
/// across the boundary.
#[derive(Debug, Clone)]
pub struct ElderForceIndex {
    ema_len: usize,
    prev_close: Option<f64>,
    ema: Ema,
    changes_seen: usize,
}

impl ElderForceIndex {
    pub fn new(ema_len: usize) -> Self {
        Self {
            ema_len: ema_len.max(1),
            prev_close: None,
            ema: Ema::new(ema_len.max(1)),
            changes_seen: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(13)
    }
}

impl Indicator for ElderForceIndex {
    fn name(&self) -> &str {
        "efi"
    }

    fn warmup_period(&self) -> usize {
        self.ema_len + 1
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev_close = match self.prev_close {
            None => {
                self.prev_close = Some(bar.close);
                return None;
            }
            Some(p) => p,
        };
        self.prev_close = Some(bar.close);

        let raw = (bar.close - prev_close) * bar.volume;
        let line = self.ema.update(raw);
        self.changes_seen += 1;
        if self.changes_seen < self.ema_len {
            return None;
        }

        let mut extra = HashMap::new();
        extra.insert("raw".to_string(), raw);

        Some(IndicatorOutput::with_extra(line, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.ema.reset();
        self.changes_seen = 0;
    }
}
