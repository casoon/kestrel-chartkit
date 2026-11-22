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
///
/// One deliberate variant stays outside this kernel: `price_average_relation` reads the path as
/// `SMA(|close - close_prev|, n) * n` from its own row history, clamps the ratio to `0..=1` and
/// gives a zero path the value `0`. It agrees with this ratio mathematically, not bit for bit, and
/// its published values are pinned, so it keeps its own form.
pub(crate) fn efficiency_ratio(closes: impl Iterator<Item = f64>) -> Option<f64> {
    let (net, path) = path_and_net(closes)?;
    (path > 0.0).then(|| net / path)
}

/// Centred sums of two equally long samples: the covariance sum and both variance sums
/// (`sum (x - mean_x)(y - mean_y)`, `sum (x - mean_x)^2`, `sum (y - mean_y)^2`), accumulated in
/// sample order. The Pearson correlation is `cov / sqrt(var_x * var_y)`; what counts as "not
/// defined" (flat series, non-finite denominator) and whether the result is clamped is the
/// caller's policy.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PearsonSums {
    pub(crate) cov: f64,
    pub(crate) var_x: f64,
    pub(crate) var_y: f64,
}

impl PearsonSums {
    /// `sqrt(var_x * var_y)`.
    pub(crate) fn denominator(&self) -> f64 {
        (self.var_x * self.var_y).sqrt()
    }
}

pub(crate) fn pearson_sums(x: &[f64], y: &[f64]) -> PearsonSums {
    let n = x.len() as f64;
    let mean_x = x.iter().sum::<f64>() / n;
    let mean_y = y.iter().sum::<f64>() / n;
    let mut sums = PearsonSums {
        cov: 0.0,
        var_x: 0.0,
        var_y: 0.0,
    };
    for (xi, yi) in x.iter().zip(y) {
        let dx = xi - mean_x;
        let dy = yi - mean_y;
        sums.cov += dx * dy;
        sums.var_x += dx * dx;
        sums.var_y += dy * dy;
    }
    sums
}

/// Pearson correlation of two equally long samples, clamped to `-1..=1`; `None` when one of them
/// does not vary.
pub(crate) fn pearson(x: &[f64], y: &[f64]) -> Option<f64> {
    let sums = pearson_sums(x, y);
    let denom = sums.denominator();
    if denom <= 0.0 {
        return None; // a flat series has no defined correlation
    }
    Some((sums.cov / denom).clamp(-1.0, 1.0))
}

/// How values equal to the ranked one count in [`rank_weight`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RankTies {
    /// Values `<=` the ranked one count fully (the percentile rank "at or below").
    AtOrBelow,
    /// Equal values count half (mid-rank), so a value and its negation rank symmetrically.
    Half,
}

/// Weighted number of `values` that lie below `current`, with ties counted by `ties`. Callers
/// divide by their own window size and scale to a fraction or a percentage; only the counting is
/// shared. Values that are NaN are never counted.
pub(crate) fn rank_weight(values: impl Iterator<Item = f64>, current: f64, ties: RankTies) -> f64 {
    let mut less = 0usize;
    let mut equal = 0usize;
    for v in values {
        if v < current {
            less += 1;
        } else if v == current {
            equal += 1;
        }
    }
    match ties {
        RankTies::AtOrBelow => (less + equal) as f64,
        RankTies::Half => less as f64 + 0.5 * equal as f64,
    }
}

/// Strength index from an averaged up move and an averaged down move: `100 - 100 / (1 + up / down)`,
/// `100` without down moves and `0` without up moves. `no_move` is the value when neither side
/// moved (`50` for RSI and MFI as published, `100` for the variants that let the "no down moves"
/// rule decide). RSI feeds it the average gain and loss, MFI the positive and negative money flow.
pub(crate) fn up_down_index(up: f64, down: f64, no_move: f64) -> f64 {
    if up == 0.0 && down == 0.0 {
        no_move
    } else if down == 0.0 {
        100.0
    } else if up == 0.0 {
        0.0
    } else {
        100.0 - 100.0 / (1.0 + up / down)
    }
}

/// Choppiness Index from the summed true ranges and the overall high-low range of the same
/// `len` bars: `100 * log10(sum_tr / range) / log10(len)`. High values mean the bars overlap
/// (chop), low values that they line up (trend). `clamped` limits the result to `0..=100`, as the
/// indicator publishes it; the regime read-out takes the raw value. Choosing a floor for a
/// zero range, or declining to rank one, stays with the caller.
pub(crate) fn choppiness(sum_tr: f64, range: f64, len: usize, clamped: bool) -> f64 {
    let chop = 100.0 * (sum_tr / range).log10() / (len as f64).log10();
    if clamped {
        chop.clamp(0.0, 100.0)
    } else {
        chop
    }
}

/// Close location value of a bar: `((close - low) - (high - close)) / (high - low)`, `-1` with the
/// close at the low, `+1` at the high. `None` for a bar whose range is not above `min_range`
/// (`0.0` for "no range at all", `1e-8` for the volume indicators, which then read the bar as
/// `0`); the caller decides what a bar without a location counts as. Not clamped.
///
/// The share form `(close - low) / (high - low)` of the volume-split indicators is the same
/// quantity on `0..=1`, `(clv + 1) / 2`; it is written out there because the two agree
/// mathematically, not bit for bit, and their published values are pinned.
pub(crate) fn close_location_value(high: f64, low: f64, close: f64, min_range: f64) -> Option<f64> {
    let range = high - low;
    (range > min_range).then(|| ((close - low) - (high - close)) / range)
}

/// What the inverse VIX Fix divides by.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum InverseBase {
    /// The bar's high: the exact mirror of the VIX Fix in log price (`p' = C / p` turns `inverse`
    /// into the VIX Fix of the original series). The `vix_fix` indicator.
    High,
    /// The lowest close of the window: the plain percentage rise of the high above it, the form
    /// the fear-gauge read-out publishes (not the mirror; it reads larger than `High`).
    LowestClose,
}

/// The Williams VIX Fix pair of one bar: `(wvf, inverse)` with
/// `wvf = 100 * (highest_close - low) / highest_close` and
/// `inverse = 100 * (high - lowest_close) / base`, `base` as `inverse_base` says. The highest and
/// lowest close are taken over the same lookback window. A non-positive divisor reads `0`.
pub(crate) fn vix_fix_pair(
    high: f64,
    low: f64,
    highest_close: f64,
    lowest_close: f64,
    inverse_base: InverseBase,
) -> (f64, f64) {
    let wvf = if highest_close > 0.0 {
        ((highest_close - low) / highest_close) * 100.0
    } else {
        0.0
    };
    let base = match inverse_base {
        InverseBase::High => high,
        InverseBase::LowestClose => lowest_close,
    };
    let inverse = if base > 0.0 {
        ((high - lowest_close) / base) * 100.0
    } else {
        0.0
    };
    (wvf, inverse)
}

/// Upper spike band of a window of values: `mean + mult * sd`, `sd` the population deviation of
/// `window` around `mean` (the caller's mean of that same window, rolling or direct).
pub(crate) fn spike_band(window: &[f64], mean: f64, mult: f64) -> f64 {
    let variance = window.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / window.len() as f64;
    mean + mult * variance.sqrt()
}

/// Floor-trader pivot of a period from its high `h`, low `l` and close `c`:
/// `p = (h + l + c) / 3`, `r1 = 2p - l`, `s1 = 2p - h`, `r2 = p + (h - l)`, `s2 = p - (h - l)`,
/// `r3 = h + 2 (p - l)`, `s3 = l - 2 (h - p)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct FloorPivots {
    pub p: f64,
    pub r1: f64,
    pub s1: f64,
    pub r2: f64,
    pub s2: f64,
    pub r3: f64,
    pub s3: f64,
}

pub(crate) fn floor_pivots(h: f64, l: f64, c: f64) -> FloorPivots {
    let p = (h + l + c) / 3.0;
    FloorPivots {
        p,
        r1: 2.0 * p - l,
        s1: 2.0 * p - h,
        r2: p + (h - l),
        s2: p - (h - l),
        r3: h + 2.0 * (p - l),
        s3: l - 2.0 * (h - p),
    }
}

/// The two published ways to estimate a fractal dimension from the bars of a window of `n`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum FractalMethod {
    /// Ehlers' halving estimate (the one behind FRAMA): with `half = max(n / 2, 2)`, the high-low
    /// range of the newest `half` bars (`r1`), of the `half` bars before them (`r2`) and of the
    /// whole window of `n` bars (`r3`), each divided by its bar count;
    /// `D = (ln(r1 + r2) - ln(r3)) / ln 2`, clamped to `1..=2`. Reads `n` bars.
    Halves,
    /// Path over range: `D = 1 + ln(path / range) / ln n`, the path being the summed absolute
    /// close-to-close moves of `n` steps and the range the high-low range of the same `n + 1`
    /// bars (the length of the path a straight line would not need, in the manner of Katz's
    /// estimate). Not clamped. Reads `n + 1` bars.
    Path,
}

impl FractalMethod {
    /// Bars a window of `n` has to hold.
    pub(crate) fn bars_needed(self, n: usize) -> usize {
        match self {
            Self::Halves => n.max(2 * (n / 2).max(2)),
            Self::Path => n + 1,
        }
    }
}

/// Fractal dimension of the newest bars (`bar_at(k)` is the `(high, low, close)` of the bar `k`
/// bars back, `0` the newest), in the measure `method` names. `1.5` when the range or the path is
/// not positive; `None` when the window is not yet complete.
pub(crate) fn fractal_dimension(
    method: FractalMethod,
    n: usize,
    bar_at: impl Fn(usize) -> Option<(f64, f64, f64)>,
) -> Option<f64> {
    let range = |from: usize, count: usize| -> Option<(f64, f64)> {
        let mut hi = f64::NEG_INFINITY;
        let mut lo = f64::INFINITY;
        for k in from..from + count {
            let (high, low, _) = bar_at(k)?;
            hi = hi.max(high);
            lo = lo.min(low);
        }
        Some((hi, lo))
    };
    match method {
        FractalMethod::Halves => {
            let half = (n / 2).max(2);
            let (h1, l1) = range(0, half)?;
            let (h2, l2) = range(half, half)?;
            let (h3, l3) = range(0, n)?;
            let r1 = (h1 - l1) / half as f64;
            let r2 = (h2 - l2) / half as f64;
            let r3 = (h3 - l3) / n as f64;
            let d = if r1 + r2 > 0.0 && r3 > 0.0 {
                ((r1 + r2).ln() - r3.ln()) / 2f64.ln()
            } else {
                1.5
            };
            Some(d.clamp(1.0, 2.0))
        }
        FractalMethod::Path => {
            let (high, low) = range(0, n + 1)?;
            let range_hl = high - low;
            let mut path = 0.0;
            for k in (1..=n).rev() {
                path += (bar_at(k - 1)?.2 - bar_at(k)?.2).abs();
            }
            Some(if range_hl > 0.0 && path > 0.0 {
                (path / range_hl).ln() / (n as f64).ln() + 1.0
            } else {
                1.5
            })
        }
    }
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

/// How a bar qualifies as a swing pivot: `left` bars before it and `right` bars after it must all
/// lie beyond it. `*_strict` decides what a tie means on that side: strict, an equal extreme
/// disqualifies the bar; non-strict, an equal extreme is tolerated. Every pivot detector in the
/// crate states its rule through this type and runs it through [`PivotRule::holds`] (one window)
/// or [`pivot_flags`] (a whole series); a pivot is known `right` bars after the bar itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PivotRule {
    pub(crate) left: usize,
    pub(crate) right: usize,
    pub(crate) left_strict: bool,
    pub(crate) right_strict: bool,
}

impl PivotRule {
    pub(crate) const fn new(
        left: usize,
        right: usize,
        left_strict: bool,
        right_strict: bool,
    ) -> Self {
        Self {
            left,
            right,
            left_strict,
            right_strict,
        }
    }

    /// `width` bars on each side, the same tie rule on both.
    pub(crate) const fn symmetric(width: usize, strict: bool) -> Self {
        Self::new(width, width, strict, strict)
    }

    /// Bars a pivot needs: the left ones, the pivot and the right ones.
    pub(crate) const fn window(&self) -> usize {
        self.left + self.right + 1
    }

    /// Whether the bar at position `left` of `values` is a pivot. `values` is one value per bar of
    /// the window in time order, oldest first, exactly [`PivotRule::window`] long: the highs for a
    /// pivot high (`low_side` false), the lows for a pivot low. A shorter or longer run is no
    /// pivot. A neighbour that is not a number never lets a side qualify.
    pub(crate) fn holds(&self, low_side: bool, values: impl Iterator<Item = f64> + Clone) -> bool {
        let Some(centre) = values.clone().nth(self.left) else {
            return false;
        };
        let mut seen = 0usize;
        let beyond = values.enumerate().all(|(i, v)| {
            seen = i + 1;
            if i == self.left {
                return true;
            }
            let strict = if i < self.left {
                self.left_strict
            } else {
                self.right_strict
            };
            match (low_side, strict) {
                (true, true) => centre < v,
                (true, false) => centre <= v,
                (false, true) => centre > v,
                (false, false) => centre >= v,
            }
        });
        beyond && seen == self.window()
    }
}

/// Pivot flags for every bar of `bars` as `(lows, highs)`. Bar `j` is a pivot low if its low
/// satisfies `rule` against `bars[j - left ..= j + right]` (highs likewise); the first `left` and
/// last `right` bars are never pivots.
pub(crate) fn pivot_flags(bars: &[Bar], rule: PivotRule) -> (Vec<bool>, Vec<bool>) {
    let n = bars.len();
    let mut lows = vec![false; n];
    let mut highs = vec![false; n];
    for j in rule.left..n.saturating_sub(rule.right) {
        let window = &bars[j - rule.left..=j + rule.right];
        lows[j] = rule.holds(true, window.iter().map(|b| b.low));
        highs[j] = rule.holds(false, window.iter().map(|b| b.high));
    }
    (lows, highs)
}

/// How close to a mark, in ATR, a price counts as on the mark.
///
/// Levels, zones and trendlines compare a price with a mark that is itself computed: the mean of
/// a level's pivots, a level plus or minus a multiple of the ATR, the value of a line between two
/// nodes. In binary floating point such a mark differs from its decimal value by a few units in
/// the last place, and so does a difference of two prices: with pivots at 100.00 and 100.20 it
/// is 0.20000000000000284, not 0.2. Left alone, that noise decides every comparison that falls
/// exactly on its bound, and it decides it one way for a series and the other way for a series
/// shifted by a cent.
///
/// The convention: a price within `MARK_TOLERANCE · ATR` of a mark is on it, and a distance
/// within `MARK_TOLERANCE · ATR` of a tolerance is on the tolerance. An inclusive bound (at or
/// beyond the mark, within the tolerance) is then met and a strict one (beyond the mark, less
/// than the tolerance) is not. With an ATR of 2 the margin is `2e-9`, far below any tick and far
/// above the rounding of a price near 100 (`1.4e-14`); it holds as long as the ATR is more than
/// about a millionth of the price. The margin is in ATR because every bound it guards is: it
/// scales with the series and is the same for a series and its mirror image (every price
/// negated), so both give mirrored results.
///
/// `kestrel_equal_level_liquidity`, `trendlines` and `kestrel_structure_zones` apply it to the
/// comparisons named in their documentation. It is the counterpart of
/// [`crate::point_and_figure::GRID_TOLERANCE`], which does the same for grid lines in boxes.
/// Comparisons of two prices of the series with each other (a pivot against its neighbours, a
/// close against the previous low) need no margin: equal decimals are equal numbers.
pub const MARK_TOLERANCE: f64 = 1e-9;

/// Comparisons of a price with a computed mark under [`MARK_TOLERANCE`], for one bar's ATR.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Marks {
    margin: f64,
}

impl Marks {
    /// The comparisons for a bar whose ATR is `atr`. An ATR of 0 leaves them exact.
    pub(crate) fn new(atr: f64) -> Self {
        Self {
            margin: MARK_TOLERANCE * atr,
        }
    }

    /// `price` is at or above `mark`: on it or beyond it.
    pub(crate) fn at_or_above(self, price: f64, mark: f64) -> bool {
        price >= mark - self.margin
    }

    /// `price` is at or below `mark`: on it or beyond it.
    pub(crate) fn at_or_below(self, price: f64, mark: f64) -> bool {
        price <= mark + self.margin
    }

    /// `price` is above `mark` and not on it.
    pub(crate) fn above(self, price: f64, mark: f64) -> bool {
        price > mark + self.margin
    }

    /// `price` is below `mark` and not on it.
    pub(crate) fn below(self, price: f64, mark: f64) -> bool {
        price < mark - self.margin
    }

    /// `distance` is at most `tolerance`: a distance on the tolerance is within it.
    pub(crate) fn within(self, distance: f64, tolerance: f64) -> bool {
        self.at_or_below(distance, tolerance)
    }
}

#[cfg(test)]
mod mark_tests {
    use super::*;

    #[test]
    fn a_distance_on_the_tolerance_in_decimal_notation_is_within_it() {
        // 0.10 x 2.0 is 0.2; 100.2 - 100.0 is 0.20000000000000284 and 100.3 - 100.1 likewise,
        // while 100.1 - 99.9 is 0.19999999999998863.
        let marks = Marks::new(2.0);
        let tolerance = 0.10 * 2.0;
        assert!(100.2_f64 - 100.0 > tolerance && 100.1_f64 - 99.9 < tolerance);
        for (a, b) in [(100.0_f64, 100.2), (100.1, 100.3), (99.9, 100.1)] {
            assert!(marks.within((a - b).abs(), tolerance), "{a} {b}");
            assert!(!marks.below((a - b).abs(), tolerance), "{a} {b}");
            assert!(marks.within((b - a).abs(), tolerance), "{a} {b}");
        }
        assert!(marks.within(100.19_f64 - 100.0, tolerance));
        assert!(marks.below(100.19_f64 - 100.0, tolerance));
        assert!(!marks.within(100.21_f64 - 100.0, tolerance));
        assert!(!marks.within(0.2 + 1e-8, tolerance), "five margins beyond");
    }

    #[test]
    fn a_price_on_the_mark_is_at_it_and_not_beyond_it() {
        let marks = Marks::new(2.0);
        // 100.1 + 0.1 is 100.19999999999999, below the price 100.2.
        let mark = 100.1_f64 + 0.1;
        assert!(mark < 100.2);
        assert!(marks.at_or_above(100.2, mark) && marks.at_or_below(100.2, mark));
        assert!(!marks.above(100.2, mark) && !marks.below(100.2, mark));
        assert!(marks.above(100.21, mark) && !marks.at_or_below(100.21, mark));
        assert!(marks.below(100.19, mark) && !marks.at_or_above(100.19, mark));
    }

    #[test]
    fn the_mirror_image_decides_the_same_and_an_atr_of_zero_is_exact() {
        let marks = Marks::new(0.5);
        for (price, mark) in [
            (100.2, 100.1 + 0.1),
            (99.9, 100.1 - 0.2),
            (100.0, 100.0 + 1e-9),
        ] {
            assert_eq!(
                marks.at_or_above(price, mark),
                marks.at_or_below(-price, -mark)
            );
            assert_eq!(marks.above(price, mark), marks.below(-price, -mark));
        }
        let exact = Marks::new(0.0);
        assert!(!exact.at_or_above(100.2, 100.2 + 1e-13));
        assert!(exact.above(100.2 + 1e-13, 100.2));
    }
}

#[cfg(test)]
mod pivot_tests {
    use super::*;

    #[test]
    fn ties_follow_the_strictness_of_their_side() {
        let highs = [1.0, 3.0, 3.0, 1.0];
        let tie_right = |rule: PivotRule| rule.holds(false, highs.iter().copied().take(3));
        // Window of the bar at index 1: left 1, right 1; the tie sits on its right.
        assert!(tie_right(PivotRule::new(1, 1, true, false)));
        assert!(!tie_right(PivotRule::new(1, 1, true, true)));
        // Window of the bar at index 2: the tie sits on its left.
        let tie_left = |rule: PivotRule| rule.holds(false, highs.iter().copied().skip(1));
        assert!(tie_left(PivotRule::new(1, 1, false, true)));
        assert!(!tie_left(PivotRule::new(1, 1, true, true)));
    }

    #[test]
    fn low_side_mirrors_high_side() {
        let lows = [3.0, 1.0, 2.0];
        let negated: Vec<f64> = lows.iter().map(|v| -v).collect();
        for strict in [false, true] {
            let rule = PivotRule::symmetric(1, strict);
            assert!(rule.holds(true, lows.iter().copied()));
            assert!(rule.holds(false, negated.iter().copied()));
        }
    }

    #[test]
    fn a_window_of_the_wrong_length_is_no_pivot() {
        let rule = PivotRule::symmetric(2, false);
        assert!(!rule.holds(false, [1.0, 2.0, 5.0, 2.0].into_iter()));
        assert!(!rule.holds(false, [1.0, 2.0, 5.0, 2.0, 1.0, 0.0].into_iter()));
        assert!(rule.holds(false, [1.0, 2.0, 5.0, 2.0, 1.0].into_iter()));
    }
}
