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
    /// Minimum body share of the range for a belt hold.
    ///
    /// Lower than `marubozu_body_min` on purpose: a belt hold is defined by *where the bar opened*,
    /// not by the absence of both wicks. Requiring the marubozu threshold here would make the
    /// belt hold a subset of the marubozu and the flag redundant.
    pub belt_hold_body_min: f64,
    /// Maximum wick on the *opening* side, as a share of the range, for a belt hold.
    pub belt_hold_open_wick_max: f64,
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
            belt_hold_body_min: 0.6,
            belt_hold_open_wick_max: 0.03,
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

/// Whether two prices are (almost) equal, relative to their own scale.
///
/// Several two- and three-bar patterns are defined by two closes or two opens "matching" rather
/// than a shape — counterattack lines, separating lines, matching low, stick sandwich,
/// deliberation. One tolerance (`tweezer_tolerance`) already exists for exactly this question on
/// highs and lows; this reuses it instead of adding a second, differently named threshold for the
/// same idea applied to closes and opens.
fn nahezu_gleich(a: f64, b: f64, tolerance: f64) -> bool {
    let scale = a.abs().max(b.abs());
    scale > 0.0 && (a - b).abs() / scale < tolerance
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
        if self.window.len() >= 4 {
            self.classify_four(&mut found, &mut alerts);
        }
        if self.window.len() >= 5 {
            self.classify_five(&mut found, &mut alerts);
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

        // Belt hold geometry: the bar opens at its own extreme and runs from there. Only the
        // *opening* side has to be free of a wick — the closing side may leave one, which is what
        // separates "opened at the low and never looked back" from a marubozu, where the period
        // was decided at both ends.
        if m.body_ratio >= cfg.belt_hold_body_min {
            if m.bullish && m.lower_ratio <= cfg.belt_hold_open_wick_max {
                mark(
                    found,
                    alerts,
                    "bullish_belt_hold_shape",
                    "Opened at the low, no lower wick",
                    0.55,
                );
            }
            if !m.bullish && m.upper_ratio <= cfg.belt_hold_open_wick_max {
                mark(
                    found,
                    alerts,
                    "bearish_belt_hold_shape",
                    "Opened at the high, no upper wick",
                    0.55,
                );
            }
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
        // A belt hold is the shape *plus* the move it opposes. A bullish bar that opens at its low
        // in an advance is a continuation bar, not a belt hold — the classical reading names the
        // opening against a prevailing move, so the shape alone stays unnamed.
        if found.contains_key("bullish_belt_hold_shape") && downtrend {
            mark(
                found,
                alerts,
                "bullish_belt_hold",
                "Bullish belt hold — opened at the low, after a decline",
                0.7,
            );
        }
        if found.contains_key("bearish_belt_hold_shape") && uptrend {
            mark(
                found,
                alerts,
                "bearish_belt_hold",
                "Bearish belt hold — opened at the high, after an advance",
                0.7,
            );
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
            // Homing pigeon: the same containment as a harami, but both candles share a colour
            // instead of the inner one being read as a pause. Reported alongside `bearish_harami`,
            // not instead of it — a bar can be both at once.
            if prev_bearish && !m.bullish {
                mark(
                    found,
                    alerts,
                    "homing_pigeon",
                    "Homing pigeon — a harami where both candles are bearish",
                    0.7,
                );
            }
        }

        // Kicking: two marubozu of opposite colour, with the second gapping past the first's own
        // extreme — not just past its body, which every marubozu already does by definition.
        if let Some(pm) = Metrics::of(prev) {
            if pm.body_ratio >= cfg.marubozu_body_min && m.body_ratio >= cfg.marubozu_body_min {
                let kicking = (prev_bullish && !m.bullish && bar.high < prev.low)
                    || (prev_bearish && m.bullish && bar.low > prev.high);
                if kicking {
                    mark(
                        found,
                        alerts,
                        "kicking",
                        "Kicking — two marubozu, a gap between them",
                        0.85,
                    );
                }
            }
        }

        // Counterattack lines / separating lines: opposite colours, one price matching almost
        // exactly — the close for counterattack, the open for separating lines. Reusing the
        // tweezer tolerance rather than a new threshold for the same "almost equal" question.
        if prev_bullish != m.bullish {
            if nahezu_gleich(bar.close, prev.close, cfg.tweezer_tolerance) {
                mark(
                    found,
                    alerts,
                    "counterattack_lines",
                    "Counterattack lines — matching closes, opposite colour",
                    0.7,
                );
            }
            if nahezu_gleich(bar.open, prev.open, cfg.tweezer_tolerance) {
                mark(
                    found,
                    alerts,
                    "separating_lines",
                    "Separating lines — matching opens, opposite colour",
                    0.7,
                );
            }
        }

        // Matching low: two bearish candles that close at (almost) the same price.
        if prev_bearish && !m.bullish && nahezu_gleich(bar.close, prev.close, cfg.tweezer_tolerance)
        {
            mark(
                found,
                alerts,
                "matching_low",
                "Matching low — two bearish candles, the same close",
                0.7,
            );
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

    /// Three-bar patterns: morning/evening star, advance block, abandoned baby.
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

        // Morning/evening star: the middle bar has to be small; that is what makes it a pause
        // rather than a continuation.
        if mid.body_ratio <= self.config.spinning_top_body_max {
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

        // Advance block: three bullish candles still climbing, but each with a smaller body and
        // a longer upper wick than the one before — an uptrend running out of conviction rather
        // than reversing outright.
        if f.bullish
            && mid.bullish
            && l.bullish
            && mid.body < f.body
            && l.body < mid.body
            && mid.upper > f.upper
            && l.upper > mid.upper
        {
            mark(
                found,
                alerts,
                "advance_block",
                "Advance block — shrinking bodies, growing upper wicks",
                0.6,
            );
        }

        // Abandoned baby: a candle in trend direction, a gap, a doji isolated by gaps on both
        // sides, and a candle against the trend on the far side of the second gap. Checked
        // against the bars' actual ranges (not the doji's ratios alone) because the isolation —
        // not the doji shape — is what makes this an abandoned baby rather than a star.
        if mid.body_ratio <= self.config.doji_body_max {
            let island_above = middle.low > first.high && middle.low > last.high;
            let island_below = middle.high < first.low && middle.high < last.low;
            if (island_above && f.bullish && !l.bullish)
                || (island_below && !f.bullish && l.bullish)
            {
                mark(
                    found,
                    alerts,
                    "abandoned_baby",
                    "Abandoned baby — a doji isolated by gaps on both sides",
                    0.85,
                );
            }
        }

        // Tri-star: the same isolation as an abandoned baby, but all three bars are dojis rather
        // than trend candles either side — there is no trend reading to derive from the shape.
        if f.body_ratio <= self.config.doji_body_max
            && mid.body_ratio <= self.config.doji_body_max
            && l.body_ratio <= self.config.doji_body_max
            && ((middle.low > first.high && middle.low > last.high)
                || (middle.high < first.low && middle.high < last.low))
        {
            mark(
                found,
                alerts,
                "tri_star",
                "Tri-star — three dojis, the middle one isolated by gaps",
                0.6,
            );
        }

        // Tasuki gap: two candles in trend direction with a gap between them, then a counter
        // candle that opens inside the second and closes back into the gap without filling it.
        if f.bullish && mid.bullish && middle.low > first.high {
            if !l.bullish && last.close > first.high && last.close < middle.low {
                mark(
                    found,
                    alerts,
                    "tasuki_gap",
                    "Tasuki gap — an upside gap, the counter candle stays inside it",
                    0.6,
                );
            }
        } else if !f.bullish
            && !mid.bullish
            && middle.high < first.low
            && l.bullish
            && last.close < first.low
            && last.close > middle.high
        {
            mark(
                found,
                alerts,
                "tasuki_gap",
                "Tasuki gap — a downside gap, the counter candle stays inside it",
                0.6,
            );
        }

        // Upside gap two crows: a bullish candle, a gap up, then a bearish candle whose body a
        // second bearish candle encloses — without the close falling back through the gap.
        if f.bullish
            && !mid.bullish
            && !l.bullish
            && middle.low > first.high
            && last.open > middle.open
            && last.close < middle.close
            && last.close > first.close
        {
            mark(
                found,
                alerts,
                "upside_gap_two_crows",
                "Upside gap two crows — the gap survives both bearish candles",
                0.7,
            );
        }

        // Three stars in the south: three bearish candles, each with a smaller range and a
        // higher low than the one before — a decline running out of room, told through the lows
        // rather than the bodies (that is Advance Block's mirror).
        if !f.bullish
            && !mid.bullish
            && !l.bullish
            && mid.range < f.range
            && l.range < mid.range
            && middle.low > first.low
            && last.low > middle.low
        {
            mark(
                found,
                alerts,
                "three_stars_in_the_south",
                "Three stars in the south — shrinking ranges, rising lows",
                0.6,
            );
        }

        // Deliberation: two large bullish candles, then a small third that opens right at the
        // second's close — a stall, not a reversal on its own.
        if f.bullish
            && mid.bullish
            && l.bullish
            && f.body_ratio > self.config.spinning_top_body_max
            && mid.body_ratio > self.config.spinning_top_body_max
            && l.body_ratio <= self.config.spinning_top_body_max
            && nahezu_gleich(last.open, middle.close, self.config.tweezer_tolerance)
        {
            mark(
                found,
                alerts,
                "deliberation",
                "Deliberation — a small third candle opening at the second's close",
                0.6,
            );
        }

        // Stick sandwich: bearish, bullish, bearish — the two outer candles closing at
        // (almost) the same price.
        if !f.bullish
            && mid.bullish
            && !l.bullish
            && nahezu_gleich(last.close, first.close, self.config.tweezer_tolerance)
        {
            mark(
                found,
                alerts,
                "stick_sandwich",
                "Stick sandwich — matching closes either side of one counter candle",
                0.7,
            );
        }

        // Unique three river bottom: a large bearish candle, then a bearish candle with a long
        // lower wick that makes a new low, then a small bullish candle that holds above it.
        if !f.bullish
            && !mid.bullish
            && l.bullish
            && middle.low < first.low
            && mid.lower_ratio >= self.config.pin_wick_min
            && l.body_ratio <= self.config.spinning_top_body_max
            && last.low > middle.low
        {
            mark(
                found,
                alerts,
                "unique_three_river_bottom",
                "Unique three river bottom — a long lower wick at a new low, then a small hold",
                0.65,
            );
        }
    }

    /// Four-bar patterns: three-line strike, concealing baby swallow.
    fn classify_four(&self, found: &mut HashMap<String, f64>, alerts: &mut Vec<IndicatorAlert>) {
        let n = self.window.len();
        let (b0, b1, b2, b3) = (
            &self.window[n - 4],
            &self.window[n - 3],
            &self.window[n - 2],
            &self.window[n - 1],
        );
        let (Some(m0), Some(m1), Some(m2), Some(m3)) = (
            Metrics::of(b0),
            Metrics::of(b1),
            Metrics::of(b2),
            Metrics::of(b3),
        ) else {
            return;
        };
        let cfg = self.config;

        // Three-line strike: three same-direction candles each making further progress, then a
        // single counter candle whose range covers all three — an engulfing over three bars
        // instead of one.
        let three_line_strike = if m0.bullish && m1.bullish && m2.bullish {
            b1.close > b0.close
                && b2.close > b1.close
                && !m3.bullish
                && b3.high >= b0.high.max(b1.high).max(b2.high)
                && b3.low <= b0.low.min(b1.low).min(b2.low)
        } else if !m0.bullish && !m1.bullish && !m2.bullish {
            b1.close < b0.close
                && b2.close < b1.close
                && m3.bullish
                && b3.high >= b0.high.max(b1.high).max(b2.high)
                && b3.low <= b0.low.min(b1.low).min(b2.low)
        } else {
            false
        };
        if three_line_strike {
            mark(
                found,
                alerts,
                "three_line_strike",
                "Three-line strike — a single candle engulfing three",
                0.75,
            );
        }

        // Concealing baby swallow: two bearish marubozu, then a bearish candle whose upper wick
        // pokes into the second's body, then a candle that fully encloses the third's range.
        if !m0.bullish
            && !m1.bullish
            && !m2.bullish
            && !m3.bullish
            && m0.body_ratio >= cfg.marubozu_body_min
            && m1.body_ratio >= cfg.marubozu_body_min
            && b2.high > b1.close
            && b2.high < b1.open
            && b3.high >= b2.high
            && b3.low <= b2.low
        {
            mark(
                found,
                alerts,
                "concealing_baby_swallow",
                "Concealing baby swallow — an upper wick into the previous body, then fully enclosed",
                0.6,
            );
        }
    }

    /// Five-bar patterns: breakaway.
    ///
    /// A gap in trend direction, three small continuation candles, then a large counter candle
    /// that closes back into the gap. `window` already caps at five bars — the same buffer the
    /// pinbar/engulfing checks use — so no extra state is needed to reach back this far.
    fn classify_five(&self, found: &mut HashMap<String, f64>, alerts: &mut Vec<IndicatorAlert>) {
        let n = self.window.len();
        let (b0, b1, b2, b3, b4) = (
            &self.window[n - 5],
            &self.window[n - 4],
            &self.window[n - 3],
            &self.window[n - 2],
            &self.window[n - 1],
        );
        let (Some(m0), Some(m1), Some(m2), Some(m3), Some(m4)) = (
            Metrics::of(b0),
            Metrics::of(b1),
            Metrics::of(b2),
            Metrics::of(b3),
            Metrics::of(b4),
        ) else {
            return;
        };

        let inner_small = m1.body < m0.body && m2.body < m0.body && m3.body < m0.body;
        let counter_large = m4.body > m1.body && m4.body > m2.body && m4.body > m3.body;

        if inner_small && counter_large {
            let breakaway = if m0.bullish {
                // Gap up in trend direction, three small bullish continuation candles, then a
                // large bearish candle closing back between b0's high and b1's low — the gap it
                // left open.
                let gap = b1.low > b0.high;
                let continuation = m1.bullish && m2.bullish && m3.bullish;
                let closes_into_gap = !m4.bullish && b4.close > b0.high && b4.close < b1.low;
                gap && continuation && closes_into_gap
            } else {
                let gap = b1.high < b0.low;
                let continuation = !m1.bullish && !m2.bullish && !m3.bullish;
                let closes_into_gap = m4.bullish && b4.close < b0.low && b4.close > b1.high;
                gap && continuation && closes_into_gap
            };
            if breakaway {
                mark(
                    found,
                    alerts,
                    "breakaway",
                    "Breakaway — gap, three small continuation candles, a large counter candle closing into the gap",
                    0.7,
                );
            }
        }

        // Rising/falling three methods and mat hold share a shape — a large candle, three small
        // ones sitting inside its range, then a candle resuming the original direction — and
        // differ only in how far the last candle has to travel. Rising/falling need it to close
        // past the first candle's own extreme; mat hold only needs it large and same-direction,
        // which is why a completed rising/falling three methods also reads as a mat hold.
        let contained = [b1, b2, b3]
            .iter()
            .all(|b| b.high <= b0.high && b.low >= b0.low);

        if contained && inner_small {
            if m0.bullish && m4.bullish && b4.close > b0.high {
                mark(
                    found,
                    alerts,
                    "rising_three_methods",
                    "Rising three methods — three small candles inside a large one, then a close beyond its high",
                    0.7,
                );
            }
            if !m0.bullish && !m4.bullish && b4.close < b0.low {
                mark(
                    found,
                    alerts,
                    "falling_three_methods",
                    "Falling three methods — three small candles inside a large one, then a close beyond its low",
                    0.7,
                );
            }
            if counter_large && m0.bullish == m4.bullish {
                mark(
                    found,
                    alerts,
                    "mat_hold",
                    "Mat hold — three small candles inside a large one, then a large candle resuming its direction",
                    0.6,
                );
            }
        }

        // Ladder bottom: three bearish candles with progressively lower closes, then a bearish
        // candle with a long upper wick, then a bullish candle opening above it — the long wick is
        // what separates this from a plain four-bar decline.
        if !m0.bullish
            && !m1.bullish
            && !m2.bullish
            && !m3.bullish
            && b1.close < b0.close
            && b2.close < b1.close
            && m3.upper_ratio >= self.config.pin_wick_min
            && m4.bullish
            && b4.open > b3.high
        {
            mark(
                found,
                alerts,
                "ladder_bottom",
                "Ladder bottom — falling closes, a long upper wick, then a bullish gap open",
                0.65,
            );
        }
    }
}

pub fn build_candle_story(_params: &HashMap<String, f64>) -> CandleStoryEngine {
    CandleStoryEngine::new()
}
