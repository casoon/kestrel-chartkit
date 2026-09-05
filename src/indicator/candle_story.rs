use std::collections::HashMap;

use crate::indicator::smoothing::Rma;
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Candle Story Engine — normalised single- and multi-candle classification.
///
/// # Why a set and not a code
///
/// The engine reports **every** pattern it recognises on a bar, each as its own `extra` flag.
/// A single `pattern_type` scalar cannot express what a bar actually is: a bar can be a marubozu
/// *and* engulf its predecessor, and with one slot the later check silently erases the earlier
/// one. Which pattern survived then depended on the order of the `if` branches, which is not a
/// property of the market.
///
/// # Why the metrics come out too
///
/// `body_ratio`, the two wick ratios and `range_atr` are the normalised form every candle
/// definition is stated in. Emitting them makes the classification auditable — a caller can see
/// *why* a bar was or was not a pinbar instead of trusting the flag — and it makes the thresholds
/// meaningful, because a threshold on a ratio transfers between instruments and one on a price
/// distance does not.
///
/// # Why the trend is an input
///
/// Hammer and hanging man are the same geometry. So are inverted hammer and shooting star. What
/// separates them is the move that came before, and that is not in the candle. The engine
/// therefore reports the geometry (`hammer_shape`, `inverted_hammer_shape`) separately from the
/// named readings (`hammer`, `hanging_man`, …), and derives the named ones from an explicit
/// `trend_context` measured over `trend_lookback` bars. Naming a shape without that context would
/// assert something the bar does not contain.
pub struct CandleStoryEngine {
    config: CandleStoryConfig,
    window: Vec<Bar>,
    closes: Vec<f64>,
    atr: Rma,
    atr_value: Option<f64>,
    prev_close: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

/// Thresholds of the classification. Every one of them is a decision, not a constant of nature.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleStoryConfig {
    /// Minimum wick share of the range for a pinbar.
    pub pin_wick_min: f64,
    /// How far towards the opposite end the close must sit for a pinbar (0…1).
    pub pin_close_pos: f64,
    /// Minimum body share of the range for a marubozu.
    pub marubozu_body_min: f64,
    /// Maximum body share of the range for a doji.
    pub doji_body_max: f64,
    /// Maximum body share of the range for a spinning top.
    pub spinning_top_body_max: f64,
    /// Wick length as a multiple of the **body** for hammer and inverted hammer.
    ///
    /// Deliberately a different denominator than `pin_wick_min`, which measures against the
    /// range. The two definitions circulate under the same names and select different bars; the
    /// engine keeps them apart instead of picking one.
    pub hammer_wick_body_min: f64,
    /// Maximum opposite wick, as a multiple of the body, for hammer and inverted hammer.
    pub hammer_opposite_max: f64,
    /// Relative tolerance for two highs or lows counting as equal (tweezer).
    pub tweezer_tolerance: f64,
    /// Minimum range relative to ATR before a shape is reported at all.
    pub min_range_atr: f64,
    /// ATR length backing `range_atr`.
    pub atr_len: usize,
    /// Bars used to determine the prior move.
    pub trend_lookback: usize,
    /// How far, in ATR, the close must have moved over the lookback to count as a trend.
    pub trend_min_atr: f64,
}

impl Default for CandleStoryConfig {
    fn default() -> Self {
        Self {
            pin_wick_min: 0.55,
            pin_close_pos: 0.65,
            marubozu_body_min: 0.82,
            doji_body_max: 0.08,
            spinning_top_body_max: 0.3,
            hammer_wick_body_min: 2.0,
            hammer_opposite_max: 0.5,
            tweezer_tolerance: 0.0015,
            min_range_atr: 0.5,
            atr_len: 14,
            trend_lookback: 10,
            trend_min_atr: 1.0,
        }
    }
}

/// The normalised description of one bar — the form every candle definition is stated in.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Metrics {
    body: f64,
    range: f64,
    upper: f64,
    lower: f64,
    body_ratio: f64,
    upper_ratio: f64,
    lower_ratio: f64,
    close_position: f64,
    bullish: bool,
}

impl Metrics {
    fn of(bar: &Bar) -> Option<Self> {
        let range = bar.high - bar.low;
        if range <= 0.0 || !range.is_finite() {
            return None;
        }
        let body = (bar.close - bar.open).abs();
        let upper = bar.high - bar.close.max(bar.open);
        let lower = bar.close.min(bar.open) - bar.low;
        Some(Self {
            body,
            range,
            upper,
            lower,
            body_ratio: body / range,
            upper_ratio: upper / range,
            lower_ratio: lower / range,
            close_position: (bar.close - bar.low) / range,
            bullish: bar.close >= bar.open,
        })
    }
}

impl CandleStoryEngine {
    pub fn new() -> Self {
        Self::with_config(CandleStoryConfig::default())
    }

    pub fn with_config(config: CandleStoryConfig) -> Self {
        Self {
            atr: Rma::new(config.atr_len),
            config,
            window: Vec::with_capacity(5),
            closes: Vec::new(),
            atr_value: None,
            prev_close: None,
            alerts: Vec::new(),
        }
    }

    /// The prior move, in ATR units: negative after a decline, positive after an advance.
    ///
    /// `None` until enough bars have accumulated. A named reading that depends on the trend is
    /// withheld while it is `None` rather than defaulting to one of the two readings.
    fn trend_context(&self) -> Option<f64> {
        let atr = self.atr_value?;
        if atr <= 0.0 || self.closes.len() <= self.config.trend_lookback {
            return None;
        }
        let now = *self.closes.last()?;
        let then = self.closes[self.closes.len() - 1 - self.config.trend_lookback];
        Some((now - then) / atr)
    }
}

impl Default for CandleStoryEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// Records a pattern: sets its flag and emits the matching alert.
fn mark(
    found: &mut HashMap<String, f64>,
    alerts: &mut Vec<IndicatorAlert>,
    kind: &str,
    message: impl Into<String>,
    strength: f64,
) {
    found.insert(kind.to_string(), 1.0);
    alerts.push(IndicatorAlert::new(kind, message, strength));
}

impl Indicator for CandleStoryEngine {
    fn name(&self) -> &str {
        "candle_story"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.window.clear();
        self.closes.clear();
        self.atr = Rma::new(self.config.atr_len);
        self.atr_value = None;
        self.prev_close = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push(bar.clone());
        if self.window.len() > 5 {
            self.window.remove(0);
        }
        self.closes.push(bar.close);
        if self.closes.len() > self.config.trend_lookback + 2 {
            self.closes.remove(0);
        }

        let true_range = match self.prev_close {
            Some(prev) => (bar.high - bar.low)
                .max((bar.high - prev).abs())
                .max((bar.low - prev).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        self.atr_value = self.atr.update(true_range);

        self.alerts.clear();

        let Some(m) = Metrics::of(bar) else {
            return Some(IndicatorOutput::new(0.0));
        };

        let pressure = (m.close_position - 0.5) * 200.0;
        let range_atr = self.atr_value.filter(|a| *a > 0.0).map(|a| m.range / a);
        let cfg = self.config;

        let mut found: HashMap<String, f64> = HashMap::new();
        let mut alerts = Vec::new();

        // A bar below the size floor is classified as nothing: without it every micro-bar with an
        // accidental wick distribution becomes a hammer, and there are a great many of those.
        let big_enough = range_atr.is_none_or(|r| r >= cfg.min_range_atr);

        if big_enough {
            self.classify_single(&m, &mut found, &mut alerts);
            let trend = self.trend_context();
            self.name_by_trend(&m, trend, &mut found, &mut alerts);
        }
        if self.window.len() >= 2 {
            let prev = self.window[self.window.len() - 2].clone();
            self.classify_pair(&m, &prev, bar, &mut found, &mut alerts);
        }
        if self.window.len() >= 3 {
            self.classify_triple(&mut found, &mut alerts);
        }

        self.alerts = alerts;

        let mut extra: HashMap<String, f64> = found;
        let count = extra.len() as f64;
        extra.insert("pattern_count".to_string(), count);
        extra.insert("pressure".to_string(), pressure);
        extra.insert("body_ratio".to_string(), m.body_ratio);
        extra.insert("upper_wick_ratio".to_string(), m.upper_ratio);
        extra.insert("lower_wick_ratio".to_string(), m.lower_ratio);
        extra.insert("close_position".to_string(), m.close_position);
        if let Some(r) = range_atr {
            extra.insert("range_atr".to_string(), r);
        }
        if let Some(t) = self.trend_context() {
            extra.insert("trend_context".to_string(), t);
        }

        Some(IndicatorOutput::with_extra(pressure, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

impl CandleStoryEngine {
    /// Shapes that need one bar only.
    fn classify_single(
        &self,
        m: &Metrics,
        found: &mut HashMap<String, f64>,
        alerts: &mut Vec<IndicatorAlert>,
    ) {
        let cfg = self.config;

        if m.body_ratio <= cfg.doji_body_max {
            mark(
                found,
                alerts,
                "doji",
                "Doji — open and close nearly equal",
                0.6,
            );
            if m.lower_ratio >= cfg.pin_wick_min {
                mark(found, alerts, "dragonfly_doji", "Dragonfly doji", 0.7);
            }
            if m.upper_ratio >= cfg.pin_wick_min {
                mark(found, alerts, "gravestone_doji", "Gravestone doji", 0.7);
            }
            if m.lower_ratio >= 0.3 && m.upper_ratio >= 0.3 {
                mark(found, alerts, "long_legged_doji", "Long-legged doji", 0.6);
            }
        } else if m.body_ratio <= cfg.spinning_top_body_max {
            mark(
                found,
                alerts,
                "spinning_top",
                "Spinning top — much movement, little net result",
                0.5,
            );
        }

        // Pinbar: wick against the *range*.
        if m.lower_ratio >= cfg.pin_wick_min && m.close_position >= cfg.pin_close_pos {
            mark(
                found,
                alerts,
                "bullish_pinbar",
                format!(
                    "Bullish pinbar (lower wick {:.0}% of range)",
                    m.lower_ratio * 100.0
                ),
                0.9,
            );
        }
        if m.upper_ratio >= cfg.pin_wick_min && m.close_position <= 1.0 - cfg.pin_close_pos {
            mark(
                found,
                alerts,
                "bearish_pinbar",
                format!(
                    "Bearish pinbar (upper wick {:.0}% of range)",
                    m.upper_ratio * 100.0
                ),
                0.9,
            );
        }

        // Hammer geometry: wick against the *body*. A different denominator, a different set.
        if m.body > 0.0 {
            if m.lower >= cfg.hammer_wick_body_min * m.body
                && m.upper <= cfg.hammer_opposite_max * m.body
            {
                mark(
                    found,
                    alerts,
                    "hammer_shape",
                    "Long lower wick, small body at the top",
                    0.6,
                );
            }
            if m.upper >= cfg.hammer_wick_body_min * m.body
                && m.lower <= cfg.hammer_opposite_max * m.body
            {
                mark(
                    found,
                    alerts,
                    "inverted_hammer_shape",
                    "Long upper wick, small body at the bottom",
                    0.6,
                );
            }
        }

        if m.body_ratio >= cfg.marubozu_body_min {
            let kind = if m.bullish {
                "bullish_marubozu"
            } else {
                "bearish_marubozu"
            };
            mark(
                found,
                alerts,
                kind,
                format!("Marubozu ({:.0}% body dominance)", m.body_ratio * 100.0),
                0.8,
            );
        }
    }

    /// The readings that only exist together with a prior move.
    ///
    /// Hammer and hanging man are the same shape. Reporting one of them without the trend would
    /// be a claim the bar does not support, so both are withheld while the context is unknown.
    fn name_by_trend(
        &self,
        _m: &Metrics,
        trend: Option<f64>,
        found: &mut HashMap<String, f64>,
        alerts: &mut Vec<IndicatorAlert>,
    ) {
        let Some(trend) = trend else { return };
        let min = self.config.trend_min_atr;
        let downtrend = trend <= -min;
        let uptrend = trend >= min;

        if found.contains_key("hammer_shape") {
            if downtrend {
                mark(
                    found,
                    alerts,
                    "hammer",
                    "Hammer — same shape, after a decline",
                    0.75,
                );
            } else if uptrend {
                mark(
                    found,
                    alerts,
                    "hanging_man",
                    "Hanging man — same shape, after an advance",
                    0.75,
                );
            }
        }
        if found.contains_key("inverted_hammer_shape") {
            if downtrend {
                mark(
                    found,
                    alerts,
                    "inverted_hammer",
                    "Inverted hammer — after a decline",
                    0.7,
                );
            } else if uptrend {
                mark(
                    found,
                    alerts,
                    "shooting_star",
                    "Shooting star — after an advance",
                    0.75,
                );
            }
        }
    }

    /// Two-bar patterns.
    fn classify_pair(
        &self,
        m: &Metrics,
        prev: &Bar,
        bar: &Bar,
        found: &mut HashMap<String, f64>,
        alerts: &mut Vec<IndicatorAlert>,
    ) {
        let cfg = self.config;
        let prev_body = (prev.close - prev.open).abs();
        let prev_bearish = prev.close < prev.open;
        let prev_bullish = prev.close > prev.open;

        // Body against body — the definition that circulates most widely. The range variant
        // additionally requires the outer bar to exceed both extremes and selects far fewer bars;
        // it is reported separately rather than folded in.
        let engulfs = m.body > prev_body;
        if m.bullish && prev_bearish && engulfs && bar.close > prev.open && bar.open <= prev.close {
            mark(
                found,
                alerts,
                "bullish_engulfing",
                "Bullish engulfing (body over body)",
                0.85,
            );
            if bar.high >= prev.high && bar.low <= prev.low {
                mark(
                    found,
                    alerts,
                    "bullish_engulfing_range",
                    "Bullish engulfing (range over range)",
                    0.9,
                );
            }
        } else if !m.bullish
            && prev_bullish
            && engulfs
            && bar.close < prev.open
            && bar.open >= prev.close
        {
            mark(
                found,
                alerts,
                "bearish_engulfing",
                "Bearish engulfing (body over body)",
                0.85,
            );
            if bar.high >= prev.high && bar.low <= prev.low {
                mark(
                    found,
                    alerts,
                    "bearish_engulfing_range",
                    "Bearish engulfing (range over range)",
                    0.9,
                );
            }
        }

        // Harami — the inverse containment: this body sits inside the previous one.
        let inside = bar.open.max(bar.close) <= prev.open.max(prev.close)
            && bar.open.min(bar.close) >= prev.open.min(prev.close);
        if inside && m.body < prev_body {
            if prev_bearish {
                mark(
                    found,
                    alerts,
                    "bullish_harami",
                    "Bullish harami — inside the previous body",
                    0.7,
                );
            } else if prev_bullish {
                mark(
                    found,
                    alerts,
                    "bearish_harami",
                    "Bearish harami — inside the previous body",
                    0.7,
                );
            }
            if found.contains_key("doji") {
                mark(
                    found,
                    alerts,
                    "harami_cross",
                    "Harami cross — the inside bar is a doji",
                    0.75,
                );
            }
        }

        if prev.low.abs() > 0.0 && prev.high.abs() > 0.0 {
            let high_diff = (bar.high - prev.high).abs() / prev.high.abs();
            let low_diff = (bar.low - prev.low).abs() / prev.low.abs();
            if low_diff < cfg.tweezer_tolerance && m.bullish && prev_bearish {
                mark(
                    found,
                    alerts,
                    "bullish_tweezer",
                    "Tweezer bottom — two equal lows",
                    0.8,
                );
            } else if high_diff < cfg.tweezer_tolerance && !m.bullish && prev_bullish {
                mark(
                    found,
                    alerts,
                    "bearish_tweezer",
                    "Tweezer top — two equal highs",
                    0.8,
                );
            }
        }
    }

    /// Three-bar patterns: morning and evening star.
    fn classify_triple(&self, found: &mut HashMap<String, f64>, alerts: &mut Vec<IndicatorAlert>) {
        let n = self.window.len();
        let (first, middle, last) = (
            &self.window[n - 3],
            &self.window[n - 2],
            &self.window[n - 1],
        );
        let (Some(f), Some(mid), Some(l)) =
            (Metrics::of(first), Metrics::of(middle), Metrics::of(last))
        else {
            return;
        };

        // The middle bar has to be small; that is what makes it a star rather than a continuation.
        if mid.body_ratio > self.config.spinning_top_body_max {
            return;
        }
        let first_mid = (first.open + first.close) / 2.0;

        if !f.bullish && l.bullish && last.close > first_mid && middle.close < first.close {
            mark(
                found,
                alerts,
                "morning_star",
                "Morning star — decline, pause, recovery past the midpoint",
                0.8,
            );
        }
        if f.bullish && !l.bullish && last.close < first_mid && middle.close > first.close {
            mark(
                found,
                alerts,
                "evening_star",
                "Evening star — advance, pause, decline past the midpoint",
                0.8,
            );
        }
    }
}

pub fn build_candle_story(_params: &HashMap<String, f64>) -> CandleStoryEngine {
    CandleStoryEngine::new()
}
