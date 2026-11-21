//! Price location: where the close stands relative to its own recent history, bar by bar.
//!
//! [`PriceLocationEngine`] is pushed one bar at a time and reports, for that bar:
//!
//! | field | definition |
//! |---|---|
//! | `atr` | Wilder ATR over `atr_len`: true range `TR = max(H − L, \|H − C₋₁\|, \|L − C₋₁\|)` (`H − L` on the first bar); the first value is the plain mean of the first `atr_len` true ranges, then `ATR = (ATR₋₁ · (n − 1) + TR) / n`. Price units. |
//! | `ema` | EMA over `ema_len`, `α = 2 / (len + 1)`, seeded with the first close. |
//! | `stretch` | `(close − ema) / atr` once at least `ema_len` bars were seen and `atr > 0`. |
//! | `range_position` | close inside the high/low range of the last `range_len` bars (0 = low, 1 = high), 0.5 when that range is flat; from the `range_len`-th bar. |
//! | `sma` | simple mean of the last `sma_len` closes, from the `sma_len`-th bar. |
//! | `sma_prior` | `sma` exactly `slope_bars` bars earlier (`None` while either is missing). |
//! | `vol_tercile` | rank of `atr / close` among the finite values of the last `vol_window` bars (the current one included): `share = #{x < current} / count`, tercile `min(⌊3 · share⌋, 2)`; from the `vol_warmup`-th bar. |
//!
//! The catalog indicator `atr` is the ATR on its own. The update `(ATR₋₁ · (n − 1) + TR) / n` is written exactly so (not as `α · TR + (1 − α) ·
//! ATR₋₁`): the two agree mathematically, not bit for bit, and downstream thresholds compare
//! these values.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/price_location.py`.

use std::collections::VecDeque;

use crate::indicator::atr::TrueRangeSmoothing;
use crate::indicator::definition::{IndicatorDefinition, ParamSpec};
use crate::indicator::kernels::{true_range_hl, AtrKernel};
use crate::indicator::smoothing::{Ema, Sma};
use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;

/// True range of a bar; `prev_close = None` on the first bar gives `high − low`.
pub fn true_range(high: f64, low: f64, prev_close: Option<f64>) -> f64 {
    true_range_hl(high, low, prev_close)
}

/// Wilder ATR, pushed bar by bar (see the module table).
#[deprecated(
    note = "use the catalog indicator `atr` (percent of close; `extra[\"raw\"]` is this price distance)"
)]
#[derive(Debug, Clone)]
pub struct WilderAtr(AtrKernel);

#[allow(deprecated)]
impl WilderAtr {
    pub fn new(len: usize) -> Self {
        Self(AtrKernel::new(TrueRangeSmoothing::Rma, len.max(1)))
    }

    /// The ATR after this bar, `None` before the `len`-th bar.
    pub fn push(&mut self, high: f64, low: f64, close: f64) -> Option<f64> {
        self.0.push_hlc(high, low, close)
    }
}

/// Wilder ATR for every bar of `bars` (`None` before the `len`-th bar).
#[deprecated(
    note = "use the catalog indicator `atr` (percent of close; `extra[\"raw\"]` is this price distance)"
)]
pub fn wilder_atr_series(bars: &[Bar], len: usize) -> Vec<Option<f64>> {
    let mut atr = AtrKernel::new(TrueRangeSmoothing::Rma, len.max(1));
    bars.iter()
        .map(|b| atr.push_hlc(b.high, b.low, b.close))
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
    atr: AtrKernel,
    ema: Ema,
    hl: VecDeque<(f64, f64)>,
    sma: Sma,
    smas: VecDeque<Option<f64>>,
    rel: VecDeque<Option<f64>>,
}

impl PriceLocationEngine {
    pub fn new(cfg: PriceLocationConfig) -> Self {
        Self {
            atr: AtrKernel::new(TrueRangeSmoothing::Rma, cfg.atr_len.max(1)),
            cfg,
            seen: 0,
            ema: Ema::new(cfg.ema_len),
            hl: VecDeque::new(),
            sma: Sma::new(cfg.sma_len),
            smas: VecDeque::new(),
            rel: VecDeque::new(),
        }
    }

    pub fn config(&self) -> &PriceLocationConfig {
        &self.cfg
    }

    pub fn push(&mut self, high: f64, low: f64, close: f64) -> PriceLocation {
        let c = self.cfg;
        let atr = self.atr.push_hlc(high, low, close);
        let ema = self.ema.update(close).unwrap();
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
        let sma = self.sma.update(close);
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

/// The catalog entry `price_location`: [`PriceLocationEngine`] as an [`Indicator`]. `value` is the
/// stretch (`(close - ema) / atr`, in ATRs); `extra` carries `atr`, `ema`, `range_position`,
/// `sma`, `sma_prior` and `vol_tercile` (0, 1 or 2) on the bars where each exists. There is no
/// output while the stretch is undefined (before the `max(atr_len, ema_len)`-th bar, and while the
/// ATR is zero). [`Indicator::reset`] starts over.
#[derive(Debug, Clone)]
pub struct PriceLocationIndicator {
    engine: PriceLocationEngine,
}

impl PriceLocationIndicator {
    pub fn new(cfg: PriceLocationConfig) -> Self {
        Self {
            engine: PriceLocationEngine::new(cfg),
        }
    }
}

impl Indicator for PriceLocationIndicator {
    fn name(&self) -> &str {
        "kestrel_price_location"
    }

    fn warmup_period(&self) -> usize {
        let cfg = self.engine.config();
        cfg.atr_len.max(cfg.ema_len)
    }

    fn reset(&mut self) {
        self.engine = PriceLocationEngine::new(*self.engine.config());
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let reading = self.engine.push(bar.high, bar.low, bar.close);
        let mut output = IndicatorOutput::new(reading.stretch?);
        for (key, value) in [
            ("atr", reading.atr),
            ("ema", reading.ema),
            ("range_position", reading.range_position),
            ("sma", reading.sma),
            ("sma_prior", reading.sma_prior),
            ("vol_tercile", reading.vol_tercile.map(f64::from)),
        ] {
            if let Some(value) = value {
                output.extra.insert(key.to_string(), value);
            }
        }
        Some(output)
    }
}

/// Registry definition of `kestrel_price_location`; `price_location` is its alias.
pub const PRICE_LOCATION_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "kestrel_price_location",
    aliases: &["price_location"],
    description: "Kestrel price location: where the close stands against its own recent history: the stretch (close - EMA) / ATR as value, with atr, ema, range_position (0 low .. 1 high of the range), sma, sma_prior and vol_tercile (0..2) as extras",
    params: &[
        ParamSpec::count("atr_len", 14, 1, 10000).meaning("Wilder ATR length"),
        ParamSpec::count("ema_len", 20, 1, 10000).meaning("EMA length the stretch is measured from"),
        ParamSpec::count("range_len", 50, 1, 10000)
            .meaning("bars of the high-low range the close position is taken in"),
        ParamSpec::count("sma_len", 200, 1, 10000).meaning("simple average length"),
        ParamSpec::count("slope_bars", 20, 1, 10000)
            .meaning("bars back for the earlier value of the simple average"),
        ParamSpec::count("vol_window", 100, 1, 10000)
            .meaning("bars of ATR / close the volatility tercile ranks against"),
        ParamSpec::count("vol_warmup", 200, 1, 10000)
            .meaning("first bar that reports a volatility tercile"),
    ],
    checks: &[],
    facts: crate::indicator::definition::IndicatorFacts::unitless().centered(0.0).range_dependent(),
    verification: crate::indicator::definition::Verification::Verified,
    build: |p| {
        Ok(Box::new(PriceLocationIndicator::new(PriceLocationConfig {
            atr_len: p.count("atr_len"),
            ema_len: p.count("ema_len"),
            range_len: p.count("range_len"),
            sma_len: p.count("sma_len"),
            slope_bars: p.count("slope_bars"),
            vol_window: p.count("vol_window"),
            vol_warmup: p.count("vol_warmup"),
        })))
    },
};

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
