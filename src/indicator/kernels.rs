//! Shared formula kernels: each of these formulas exists once, here, and the catalog indicators,
//! composites and the read-outs in [`crate::analytics`] all call it.

use crate::model::Bar;

use super::atr::TrueRangeSmoothing;
use super::smoothing::{Ema, Rma, Sma, Wma};

/// True range from the bar's high and low and the previous close:
/// `max(high - low, |high - prev_close|, |low - prev_close|)`. Without a previous close (the first
/// bar) both gap terms are absent and the result is `high - low`.
pub(crate) fn true_range_hl(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
    match prev_close {
        Some(pc) => (high - low).max((high - pc).abs()).max((low - pc).abs()),
        None => high - low,
    }
}

/// [`true_range_hl`] of a bar.
pub(crate) fn true_range(bar: &Bar, prev_close: Option<f64>) -> f64 {
    true_range_hl(bar.high, bar.low, prev_close)
}

/// Net move and path length of a run of closes, in the order they are given (oldest first or
/// newest first): `(|last - first|, sum |c_i - c_{i-1}|)`. The path sums in iteration order, so a
/// caller keeps its own rounding by choosing the direction. `None` for fewer than two closes.
pub(crate) fn path_and_net(closes: impl Iterator<Item = f64>) -> Option<(f64, f64)> {
    let mut first = 0.0;
    let mut prev = 0.0;
    let mut path = 0.0;
    let mut count = 0usize;
    for c in closes {
        if count == 0 {
            first = c;
        } else {
            path += (c - prev).abs();
        }
        prev = c;
        count += 1;
    }
    (count >= 2).then(|| ((prev - first).abs(), path))
}

/// Kaufman's efficiency ratio of a run of closes: `|net move| / path length`, `1` for a straight
/// run and near `0` for much travel without progress. `None` when the path is not positive (nothing
/// moved) or fewer than two closes are given; each caller chooses its own value for that case.
pub(crate) fn efficiency_ratio(closes: impl Iterator<Item = f64>) -> Option<f64> {
    let (net, path) = path_and_net(closes)?;
    (path > 0.0).then(|| net / path)
}

/// The true-range average in whichever form was selected. Every mode publishes from the `N`-th
/// true range on, so switching the method moves the values, never the first output.
#[derive(Debug, Clone)]
pub(crate) enum TrSmoother {
    Rma(Rma),
    Sma(Sma),
    Ema { ema: Ema, len: usize, seen: usize },
    Wma(Wma),
}

impl TrSmoother {
    pub(crate) fn new(method: TrueRangeSmoothing, len: usize) -> Self {
        match method {
            TrueRangeSmoothing::Rma => Self::Rma(Rma::new(len)),
            TrueRangeSmoothing::Sma => Self::Sma(Sma::new(len)),
            TrueRangeSmoothing::Ema => Self::Ema {
                ema: Ema::new(len),
                len,
                seen: 0,
            },
            TrueRangeSmoothing::Wma => Self::Wma(Wma::new(len)),
        }
    }

    pub(crate) fn update(&mut self, tr: f64) -> Option<f64> {
        match self {
            Self::Rma(rma) => rma.update(tr),
            Self::Sma(sma) => sma.update(tr),
            Self::Ema { ema, len, seen } => {
                let value = ema.update(tr)?;
                *seen += 1;
                (*seen >= *len).then_some(value)
            }
            Self::Wma(wma) => wma.update(tr),
        }
    }

    pub(crate) fn reset(&mut self) {
        match self {
            Self::Rma(rma) => rma.reset(),
            Self::Sma(sma) => sma.reset(),
            Self::Ema { ema, seen, .. } => {
                ema.reset();
                *seen = 0;
            }
            Self::Wma(wma) => wma.reset(),
        }
    }
}

/// Average true range, pushed bar by bar: the [`true_range`] kernel averaged with the chosen
/// [`TrueRangeSmoothing`] (Wilder's by default). Publishes from the `len`-th true range on.
#[derive(Debug, Clone)]
pub(crate) struct AtrKernel {
    prev_close: Option<f64>,
    average: TrSmoother,
}

impl AtrKernel {
    pub(crate) fn new(method: TrueRangeSmoothing, len: usize) -> Self {
        Self {
            prev_close: None,
            average: TrSmoother::new(method, len),
        }
    }

    /// The ATR after this bar, `None` until `len` true ranges have been seen.
    pub(crate) fn push_hlc(&mut self, high: f64, low: f64, close: f64) -> Option<f64> {
        let tr = true_range_hl(high, low, self.prev_close);
        self.prev_close = Some(close);
        self.average.update(tr)
    }

    pub(crate) fn push(&mut self, bar: &Bar) -> Option<f64> {
        self.push_hlc(bar.high, bar.low, bar.close)
    }

    pub(crate) fn reset(&mut self) {
        self.prev_close = None;
        self.average.reset();
    }
}
