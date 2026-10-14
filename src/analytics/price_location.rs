//! Price location: where the close stands relative to its own recent history, bar by bar.
//!
//! [`PriceLocationEngine`] is pushed one bar at a time and reports, for that bar:
//!
//! | field | definition |
//! |---|---|
//! | `atr` | Wilder ATR over `atr_len`: true range `TR = max(H − L, |H − C₋₁|, |L − C₋₁|)` (`H − L` on the first bar); the first value is the plain mean of the first `atr_len` true ranges, then `ATR = (ATR₋₁ · (n − 1) + TR) / n`. Price units. |
//! | `ema` | EMA over `ema_len`, `α = 2 / (len + 1)`, seeded with the first close. |
//! | `stretch` | `(close − ema) / atr` once at least `ema_len` bars were seen and `atr > 0`. |
//! | `range_position` | close inside the high/low range of the last `range_len` bars (0 = low, 1 = high), 0.5 when that range is flat; from the `range_len`-th bar. |
//! | `sma` | simple mean of the last `sma_len` closes, from the `sma_len`-th bar. |
//! | `sma_prior` | `sma` exactly `slope_bars` bars earlier (`None` while either is missing). |
//! | `vol_tercile` | rank of `atr / close` among the finite values of the last `vol_window` bars (the current one included): `share = #{x < current} / count`, tercile `min(⌊3 · share⌋, 2)`; from the `vol_warmup`-th bar. |
//!
//! [`WilderAtr`] is the ATR part on its own, [`wilder_atr_series`] the ATR over a whole series.
//! The update `(ATR₋₁ · (n − 1) + TR) / n` is written exactly so (not as `α · TR + (1 − α) ·
//! ATR₋₁`): the two agree mathematically, not bit for bit, and downstream thresholds compare
//! these values.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/price_location.py`.

use std::collections::VecDeque;

use crate::model::Bar;

/// True range of a bar; `prev_close = None` on the first bar gives `high − low`.
pub fn true_range(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
    match prev_close {
        Some(pc) => (high - low).max((high - pc).abs()).max((low - pc).abs()),
        None => high - low,
    }
}

/// Wilder ATR, pushed bar by bar (see the module table).
#[derive(Debug, Clone)]
pub struct WilderAtr {
    len: usize,
    seen: usize,
    sum: f64,
    value: Option<f64>,
    prev_close: Option<f64>,
}

impl WilderAtr {
    pub fn new(len: usize) -> Self {
        Self {
            len: len.max(1),
            seen: 0,
            sum: 0.0,
            value: None,
            prev_close: None,
        }
    }

    /// The ATR after this bar, `None` before the `len`-th bar.
    pub fn push(&mut self, high: f64, low: f64, close: f64) -> Option<f64> {
        let tr = true_range(high, low, self.prev_close);
        let n = self.len as f64;
        self.value = match self.value {
            Some(a) => Some((a * (n - 1.0) + tr) / n),
            None => {
                self.sum += tr;
                (self.seen + 1 == self.len).then_some(self.sum / n)
            }
        };
        self.prev_close = Some(close);
        self.seen += 1;
        self.value
    }
}

/// Wilder ATR for every bar of `bars` (`None` before the `len`-th bar).
pub fn wilder_atr_series(bars: &[Bar], len: usize) -> Vec<Option<f64>> {
    let mut atr = WilderAtr::new(len);
    bars.iter()
        .map(|b| atr.push(b.high, b.low, b.close))
        .collect()
}

/// Window lengths of [`PriceLocationEngine`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PriceLocationConfig {
    pub atr_len: usize,
    pub ema_len: usize,
    pub range_len: usize,
    pub sma_len: usize,
    pub slope_bars: usize,
    pub vol_window: usize,
    /// First bar (count) with a `vol_tercile`.
    pub vol_warmup: usize,
}

impl Default for PriceLocationConfig {
    fn default() -> Self {
        Self {
            atr_len: 14,
            ema_len: 20,
            range_len: 50,
            sma_len: 200,
            slope_bars: 20,
            vol_window: 100,
            vol_warmup: 200,
        }
    }
}

/// One bar's reading (see the module table).
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct PriceLocation {
    pub atr: Option<f64>,
    pub ema: Option<f64>,
    pub stretch: Option<f64>,
    pub range_position: Option<f64>,
    pub sma: Option<f64>,
    pub sma_prior: Option<f64>,
    pub vol_tercile: Option<u8>,
}

/// Incremental [`PriceLocation`] per bar.
#[derive(Debug, Clone)]
pub struct PriceLocationEngine {
    cfg: PriceLocationConfig,
    seen: usize,
    atr: WilderAtr,
    ema: Option<f64>,
    hl: VecDeque<(f64, f64)>,
    closes: VecDeque<f64>,
    sma_sum: f64,
    smas: VecDeque<Option<f64>>,
    rel: VecDeque<Option<f64>>,
}

impl PriceLocationEngine {
    pub fn new(cfg: PriceLocationConfig) -> Self {
        Self {
            atr: WilderAtr::new(cfg.atr_len),
            cfg,
            seen: 0,
            ema: None,
            hl: VecDeque::new(),
            closes: VecDeque::new(),
            sma_sum: 0.0,
            smas: VecDeque::new(),
            rel: VecDeque::new(),
        }
    }

    pub fn config(&self) -> &PriceLocationConfig {
        &self.cfg
    }

    pub fn push(&mut self, high: f64, low: f64, close: f64) -> PriceLocation {
        let c = self.cfg;
        let atr = self.atr.push(high, low, close);
        let alpha = 2.0 / (c.ema_len as f64 + 1.0);
        let ema = match self.ema {
            Some(e) => e + alpha * (close - e),
            None => close,
        };
        self.ema = Some(ema);
        self.hl.push_back((high, low));
        if self.hl.len() > c.range_len {
            self.hl.pop_front();
        }
        let range_position = (self.hl.len() == c.range_len).then(|| {
            let hi = self.hl.iter().map(|x| x.0).fold(f64::MIN, f64::max);
            let lo = self.hl.iter().map(|x| x.1).fold(f64::MAX, f64::min);
            if hi > lo {
                (close - lo) / (hi - lo)
            } else {
                0.5
            }
        });
        let stretch = match (atr, self.seen + 1 >= c.ema_len) {
            (Some(a), true) if a > 0.0 => Some((close - ema) / a),
            _ => None,
        };
        self.closes.push_back(close);
        self.sma_sum += close;
        if self.closes.len() > c.sma_len {
            self.sma_sum -= self.closes.pop_front().unwrap_or(0.0);
        }
        let sma = (self.closes.len() == c.sma_len).then(|| self.sma_sum / c.sma_len as f64);
        self.smas.push_back(sma);
        if self.smas.len() > c.slope_bars + 1 {
            self.smas.pop_front();
        }
        let sma_prior = if self.smas.len() == c.slope_bars + 1 {
            self.smas[0]
        } else {
            None
        };
        let rel = atr.filter(|_| close > 0.0).map(|a| a / close);
        self.rel.push_back(rel);
        if self.rel.len() > c.vol_window {
            self.rel.pop_front();
        }
        let vol_tercile = (self.seen + 1 >= c.vol_warmup)
            .then_some(rel)
            .flatten()
            .map(|now| {
                let window: Vec<f64> = self.rel.iter().flatten().copied().collect();
                let share =
                    window.iter().filter(|x| **x < now).count() as f64 / window.len() as f64;
                (share * 3.0).min(2.0) as u8
            });
        self.seen += 1;
        PriceLocation {
            atr,
            ema: Some(ema),
            stretch,
            range_position,
            sma,
            sma_prior,
            vol_tercile,
        }
    }
}

/// [`PriceLocation`] for every bar of `bars`.
pub fn price_location_series(bars: &[Bar], cfg: PriceLocationConfig) -> Vec<PriceLocation> {
    let mut e = PriceLocationEngine::new(cfg);
    bars.iter()
        .map(|b| e.push(b.high, b.low, b.close))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn atr_starts_at_len_and_flat_range_is_centred() {
        let mut e = PriceLocationEngine::new(PriceLocationConfig {
            atr_len: 3,
            ema_len: 2,
            range_len: 2,
            sma_len: 2,
            slope_bars: 1,
            vol_window: 2,
            vol_warmup: 2,
        });
        let a = e.push(10.0, 10.0, 10.0);
        assert_eq!(a.atr, None);
        assert_eq!(a.range_position, None);
        let b = e.push(10.0, 10.0, 10.0);
        assert_eq!(b.range_position, Some(0.5));
        assert_eq!(b.atr, None);
        let c = e.push(11.0, 9.0, 10.0);
        assert_eq!(c.atr, Some(2.0 / 3.0));
    }
}
