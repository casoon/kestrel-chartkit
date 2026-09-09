//! Bar-series transformations: alternative candles derived from observed ones.
//!
//! What separates these from indicators is what they produce. An indicator answers with a number
//! about a bar; a transformation answers with another bar. The prices in that bar were never
//! traded, which is the whole reason this lives in its own type rather than in [`crate::Bar`]:
//! a synthetic price must not reach fill, slippage or risk arithmetic by accident.

use crate::model::{Bar, Provenance, SeriesIdentity};

/// One Heikin-Ashi candle together with the observed bar it was derived from.
///
/// The OHLC values here are computed, not traded. `volume` is the *source bar's* volume, passed
/// through unchanged — Heikin-Ashi averages prices, it does not redistribute turnover.
///
/// The link back to `source` is kept so that anything needing a real price (a fill, a stop
/// distance, a risk figure) can reach it without a second lookup, and so a renderer can draw the
/// transformed candle against the bar it belongs to.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HeikinAshiBar {
    /// Timestamp of the source bar, unchanged: one input bar yields exactly one output candle.
    pub timestamp: i64,
    /// `(previous ha_open + previous ha_close) / 2`; on the first bar of a series `(open + close) / 2`.
    pub open: f64,
    /// `max(source high, ha_open, ha_close)`.
    pub high: f64,
    /// `min(source low, ha_open, ha_close)`.
    pub low: f64,
    /// `(open + high + low + close) / 4` of the source bar.
    pub close: f64,
    /// The source bar's volume, unchanged.
    pub volume: f64,
    /// The observed bar this candle was derived from.
    pub source: Bar,
}

impl HeikinAshiBar {
    /// Converts to a plain [`Bar`], for the deliberate case of running an indicator over the
    /// transformed series.
    ///
    /// This drops the distinction between computed and observed prices — the resulting bar looks
    /// like any other. Pair it with [`HeikinAshiBar::synthetic_identity`] so the series it
    /// belongs to still says where its prices came from, and keep fill, slippage and risk
    /// arithmetic on [`HeikinAshiBar::source`].
    pub fn to_bar(&self) -> Bar {
        Bar::new(
            self.timestamp,
            self.open,
            self.high,
            self.low,
            self.close,
            self.volume,
        )
    }

    /// The identity a transformed series carries: `base` with its provenance set to
    /// [`Provenance::Synthetic`], since the prices are derived rather than observed.
    pub fn synthetic_identity(base: &SeriesIdentity) -> SeriesIdentity {
        let mut identity = base.clone();
        identity.provenance = Provenance::Synthetic;
        identity
    }
}

/// Streaming Heikin-Ashi transformation.
///
/// From the second candle on:
///
/// ```text
/// ha_close = (open + high + low + close) / 4
/// ha_open  = (previous ha_open + previous ha_close) / 2
/// ha_high  = max(high, ha_open, ha_close)
/// ha_low   = min(low,  ha_open, ha_close)
/// ```
///
/// The first candle has no predecessor to average, so its `ha_open` is `(open + close) / 2` of
/// that bar. That is a decision, not a derivation: other implementations seed with `(O+H+L+C)/4`
/// instead and produce a different opening candle, after which both converge.
///
/// The recursion is causal — a candle is final when its bar is — and it carries state, so a
/// series switch must go through [`HeikinAshi::reset`] rather than continuing across the
/// boundary; see [`SeriesIdentity`] for what makes two series different.
#[derive(Debug, Clone, Default)]
pub struct HeikinAshi {
    prev: Option<(f64, f64)>,
}

impl HeikinAshi {
    pub fn new() -> Self {
        Self::default()
    }

    /// Transforms one bar. Every input bar yields exactly one candle — there is no warmup.
    pub fn update(&mut self, bar: &Bar) -> HeikinAshiBar {
        let close = (bar.open + bar.high + bar.low + bar.close) / 4.0;
        let open = match self.prev {
            None => (bar.open + bar.close) / 2.0,
            Some((prev_open, prev_close)) => (prev_open + prev_close) / 2.0,
        };
        self.prev = Some((open, close));

        HeikinAshiBar {
            timestamp: bar.timestamp,
            open,
            high: bar.high.max(open).max(close),
            low: bar.low.min(open).min(close),
            close,
            volume: bar.volume,
            source: bar.clone(),
        }
    }

    pub fn reset(&mut self) {
        self.prev = None;
    }
}
