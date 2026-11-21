//! Trailing lines: Range Filter, MOST, OTT, HalfTrend and the SSL channel.
//!
//! Every engine is written from its published formula; the conventions a formula leaves open
//! (start values, the bar of the first output, alerts) are stated on each type. Bars are counted
//! from 0 in the formulas, `warmup_period` is the 1-based number of the first bar with a value, and
//! every state runs from the first bar whether or not a value is published yet. Directions start
//! undecided (0) instead of in one direction, so long and short are treated alike.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::definition::{IndicatorDefinition, IndicatorFacts, ParamSpec, Verification};
use super::momentum_oscillators::{push_capped, window_mean, Atr, SeededEma, StateChange};
use super::smoothing::{crossed_over, crossed_under};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

fn extras(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
    pairs.iter().map(|(k, v)| (k.to_string(), *v)).collect()
}

// ---------------------------------------------------------------------------------------------
// Range Filter
// ---------------------------------------------------------------------------------------------

/// Range Filter over the closing price: a line that follows the close only when it moves out of
/// a band of `mult` times the smoothed absolute close-to-close change.
///
/// The band width is `r = mult EMA(2 period - 1)(EMA(period)(|x_i - x_{i-1}|))`, both EMAs
/// seeded on bar 1. The filter starts at the first close; a close above it pulls it up to
/// `x - r` (never down), any other close pulls it down to `x + r` (never up). The direction is the
/// sign of the filter's last move beyond a `1e-9` relative tolerance, so a move within rounding
/// noise is no change; it starts at 0. First output on bar 1; alerts `bull` and `bear` when the
/// direction turns to +1 and -1.
#[derive(Debug, Clone)]
pub struct RangeFilterEngine {
    period: usize,
    mult: f64,
    prev: Option<f64>,
    filter: f64,
    s1: SeededEma,
    s2: SeededEma,
    direction: i8,
    change: StateChange,
}

impl RangeFilterEngine {
    pub fn new(period: usize, mult: f64) -> Self {
        Self {
            period,
            mult,
            prev: None,
            filter: 0.0,
            s1: SeededEma::new(period),
            s2: SeededEma::new(2 * period - 1),
            direction: 0,
            change: StateChange::default(),
        }
    }
}

impl Indicator for RangeFilterEngine {
    fn name(&self) -> &str {
        "range_filter"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        *self = Self::new(self.period, self.mult);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let x = bar.close;
        let Some(prev) = self.prev.replace(x) else {
            self.filter = x;
            return None;
        };
        let r = self.mult * self.s2.update(self.s1.update((x - prev).abs()));
        let f_prev = self.filter;
        let f = if x > f_prev {
            f_prev.max(x - r)
        } else {
            f_prev.min(x + r)
        };
        self.filter = f;
        let tol = 1e-9 * f.abs().max(f_prev.abs()).max(1.0);
        if f > f_prev + tol {
            self.direction = 1;
        } else if f < f_prev - tol {
            self.direction = -1;
        }
        self.change.update(Some(self.direction));
        Some(IndicatorOutput::with_extra(
            f,
            extras(&[
                ("upper", f + r),
                ("lower", f - r),
                ("direction", f64::from(self.direction)),
            ]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.change.alerts("RANGE FILTER", "RANGE FILTER")
    }
}

// ---------------------------------------------------------------------------------------------
// MOST and OTT
// ---------------------------------------------------------------------------------------------

/// The percent trailing stop shared by MOST and OTT: a long stop `m (1 - p/100)` that only
/// ratchets up while `m` stays above it, a short stop `m (1 + p/100)` that only ratchets down while
/// `m` stays below it, and a direction that turns up when `m` rises above the previous short stop
/// and down when it falls below the previous long stop. The direction starts undecided (0).
#[derive(Debug, Clone)]
struct PercentStop {
    percent: f64,
    long: f64,
    short: f64,
    direction: i8,
    started: bool,
}

impl PercentStop {
    fn new(percent: f64) -> Self {
        Self {
            percent,
            long: 0.0,
            short: 0.0,
            direction: 0,
            started: false,
        }
    }

    fn update(&mut self, m: f64) {
        let fark = m * self.percent / 100.0;
        let (long_raw, short_raw) = (m - fark, m + fark);
        if !self.started {
            self.started = true;
            self.long = long_raw;
            self.short = short_raw;
            return;
        }
        let (long_prev, short_prev, d_prev) = (self.long, self.short, self.direction);
        self.long = if m > long_prev {
            long_raw.max(long_prev)
        } else {
            long_raw
        };
        self.short = if m < short_prev {
            short_raw.min(short_prev)
        } else {
            short_raw
        };
        self.direction = if d_prev != 1 && m > short_prev {
            1
        } else if d_prev != -1 && m < long_prev {
            -1
        } else {
            d_prev
        };
    }

    /// The stop on the side of the direction; `None` while undecided.
    fn stop(&self) -> Option<f64> {
        match self.direction {
            1 => Some(self.long),
            -1 => Some(self.short),
            _ => None,
        }
    }
}

/// MOST, a moving stop: a percent trailing stop (`percent` of the level) on the EMA(`length`) of
/// the close. The direction starts undecided instead of up and nothing is published until it is
/// decided; the warmup is declared as 2, the earliest bar it can be. Extras `ema` and
/// `direction`; alerts `bull` and `bear` when the direction turns to +1 and -1. The distances are
/// a percent of the level, so the entry is scale invariant but has no mirror property.
#[derive(Debug, Clone)]
pub struct MostEngine {
    length: usize,
    percent: f64,
    ema: SeededEma,
    stop: PercentStop,
    change: StateChange,
}

impl MostEngine {
    pub fn new(length: usize, percent: f64) -> Self {
        Self {
            length,
            percent,
            ema: SeededEma::new(length),
            stop: PercentStop::new(percent),
            change: StateChange::default(),
        }
    }
}

impl Indicator for MostEngine {
    fn name(&self) -> &str {
        "most"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        *self = Self::new(self.length, self.percent);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let m = self.ema.update(bar.close);
        self.stop.update(m);
        let Some(most) = self.stop.stop() else {
            self.change.update(None);
            return None;
        };
        self.change.update(Some(self.stop.direction));
        Some(IndicatorOutput::with_extra(
            most,
            extras(&[("ema", m), ("direction", f64::from(self.stop.direction))]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.change.alerts("MOST", "MOST")
    }
}

/// OTT, the optimized trend tracker: the MOST stop on a variable average of the close, shifted
/// by half the percent to the side the average lies on.
///
/// The variable average `V` starts at the first close and follows `V = a |k| C + (1 - a |k|)
/// V_prev` with `a = 2 / (length + 1)` and `k` the Chande momentum ratio of the last `cmo_len`
/// bars (the bars available during the first `cmo_len - 1`, bar 0 counting as no change). The
/// stops and the direction follow [`MostEngine`] with `V` in the place of the EMA; the line is
/// `stop (200 + percent) / 200` while `V` is above the stop and `stop (200 - percent) / 200`
/// otherwise, and is not shifted in time. Published from bar `cmo_len` once the direction is
/// decided (declared warmup `cmo_len + 1`); extras `var`, `stop`, `direction`. Alerts compare `V`
/// with the line two bars earlier: `bull` when `V` crosses above it, `bear` when it crosses below,
/// only while the four bars involved are published. No mirror property, scale invariant.
#[derive(Debug, Clone)]
pub struct OttEngine {
    length: usize,
    percent: f64,
    cmo_len: usize,
    count: usize,
    prev_close: f64,
    ups: VecDeque<f64>,
    downs: VecDeque<f64>,
    var: f64,
    stop: PercentStop,
    history: VecDeque<Option<(f64, f64)>>,
    cross_up: bool,
    cross_down: bool,
}

impl OttEngine {
    pub fn new(length: usize, percent: f64, cmo_len: usize) -> Self {
        Self {
            length,
            percent,
            cmo_len,
            count: 0,
            prev_close: 0.0,
            ups: VecDeque::with_capacity(cmo_len),
            downs: VecDeque::with_capacity(cmo_len),
            var: 0.0,
            stop: PercentStop::new(percent),
            history: VecDeque::with_capacity(4),
            cross_up: false,
            cross_down: false,
        }
    }
}

impl Indicator for OttEngine {
    fn name(&self) -> &str {
        "ott"
    }

    fn warmup_period(&self) -> usize {
        self.cmo_len + 1
    }

    fn reset(&mut self) {
        *self = Self::new(self.length, self.percent, self.cmo_len);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let c = bar.close;
        let i = self.count;
        self.count += 1;
        self.cross_up = false;
        self.cross_down = false;
        let (u, dn) = if i == 0 {
            (0.0, 0.0)
        } else {
            (
                (c - self.prev_close).max(0.0),
                (self.prev_close - c).max(0.0),
            )
        };
        self.prev_close = c;
        push_capped(&mut self.ups, u, self.cmo_len);
        push_capped(&mut self.downs, dn, self.cmo_len);
        let su: f64 = self.ups.iter().sum();
        let sd: f64 = self.downs.iter().sum();
        let k = if su + sd == 0.0 {
            0.0
        } else {
            (su - sd) / (su + sd)
        };
        if i == 0 {
            self.var = c;
        } else {
            let w = 2.0 / (self.length as f64 + 1.0) * k.abs();
            self.var = w * c + (1.0 - w) * self.var;
        }
        let v = self.var;
        self.stop.update(v);
        let published = match self.stop.stop() {
            Some(mt) if i >= self.cmo_len => Some(mt),
            _ => None,
        };
        let entry = published.map(|mt| {
            let ott = if v > mt {
                mt * (200.0 + self.percent) / 200.0
            } else {
                mt * (200.0 - self.percent) / 200.0
            };
            (v, ott)
        });
        if self.history.len() == 4 {
            self.history.pop_front();
        }
        self.history.push_back(entry);
        let (Some(mt), Some((_, ott))) = (published, entry) else {
            return None;
        };
        if let [Some((_, ott_3)), Some((_, ott_2)), Some((v_1, _)), Some((v_0, _))] =
            self.history.iter().copied().collect::<Vec<_>>()[..]
        {
            self.cross_up = crossed_over(v_1, ott_3, v_0, ott_2);
            self.cross_down = crossed_under(v_1, ott_3, v_0, ott_2);
        }
        Some(IndicatorOutput::with_extra(
            ott,
            extras(&[
                ("var", v),
                ("stop", mt),
                ("direction", f64::from(self.stop.direction)),
            ]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let mut out = Vec::new();
        if self.cross_up {
            out.push(IndicatorAlert::new(
                "bull",
                "OTT · AVERAGE CROSSES ABOVE LINE",
                1.0,
            ));
        }
        if self.cross_down {
            out.push(IndicatorAlert::new(
                "bear",
                "OTT · AVERAGE CROSSES BELOW LINE",
                1.0,
            ));
        }
        out
    }
}

// ---------------------------------------------------------------------------------------------
// HalfTrend
// ---------------------------------------------------------------------------------------------

/// HalfTrend from the highs, lows and closes.
///
/// Over the last `amplitude` bars: the highest high `hp`, lowest low `lp` and the mean high `hma`
/// and mean low `lma`. The state machine runs from bar `max(amplitude - 1, 1)`, starts undecided
/// and watches both sides: it turns down when `hma` is below the highest recorded low `maxLow`
/// and the close is below the previous low, and up when `lma` is above the lowest recorded high
/// `minHigh` and the close is above the previous high. The line then follows the trend,
/// ratcheting with `maxLow` (up) or `minHigh` (down); on a turn it continues from the other
/// side's line, and the first line is seeded with the extreme that decided it. `atr_high` and
/// `atr_low` lie `deviation * ATR(atr_len) / 2` above and below the line. Published once the
/// trend is decided from bar `atr_len - 1` on (declared warmup `atr_len`); alerts `bull` and
/// `bear` when the trend turns to +1 and -1.
#[derive(Debug, Clone)]
pub struct HalfTrendEngine {
    amplitude: usize,
    deviation: f64,
    atr_len: usize,
    count: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    prev: Option<(f64, f64)>,
    started: bool,
    max_low: f64,
    min_high: f64,
    trend: i8,
    up: f64,
    down: f64,
    atr: Atr,
    change: StateChange,
}

impl HalfTrendEngine {
    pub fn new(amplitude: usize, deviation: f64, atr_len: usize) -> Self {
        Self {
            amplitude,
            deviation,
            atr_len,
            count: 0,
            highs: VecDeque::with_capacity(amplitude),
            lows: VecDeque::with_capacity(amplitude),
            prev: None,
            started: false,
            max_low: 0.0,
            min_high: 0.0,
            trend: 0,
            up: 0.0,
            down: 0.0,
            atr: Atr::new(atr_len),
            change: StateChange::default(),
        }
    }
}

impl Indicator for HalfTrendEngine {
    fn name(&self) -> &str {
        "halftrend"
    }

    fn warmup_period(&self) -> usize {
        self.atr_len
    }

    fn reset(&mut self) {
        *self = Self::new(self.amplitude, self.deviation, self.atr_len);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let i = self.count;
        self.count += 1;
        let atr = self.atr.update(bar);
        push_capped(&mut self.highs, bar.high, self.amplitude);
        push_capped(&mut self.lows, bar.low, self.amplitude);
        let prev = self.prev.replace((bar.high, bar.low));
        let ready = i + 1 >= self.amplitude && i >= 1;
        let (Some((prev_high, prev_low)), true) = (prev, ready) else {
            self.change.update(None);
            return None;
        };
        if !self.started {
            self.started = true;
            self.max_low = prev_low;
            self.min_high = prev_high;
        }
        let hp = self.highs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
        let lp = self.lows.iter().copied().fold(f64::INFINITY, f64::min);
        let hma = window_mean(&self.highs);
        let lma = window_mean(&self.lows);
        let trend_prev = self.trend;
        let watch_down = trend_prev >= 0;
        let watch_up = trend_prev <= 0;
        if watch_down {
            self.max_low = lp.max(self.max_low);
        }
        if watch_up {
            self.min_high = hp.min(self.min_high);
        }
        if watch_down && hma < self.max_low && bar.close < prev_low {
            self.trend = -1;
            self.min_high = hp;
        } else if watch_up && lma > self.min_high && bar.close > prev_high {
            self.trend = 1;
            self.max_low = lp;
        }
        match self.trend {
            1 => {
                self.up = match trend_prev {
                    -1 => self.down,
                    0 => self.max_low,
                    _ => self.max_low.max(self.up),
                };
            }
            -1 => {
                self.down = match trend_prev {
                    1 => self.up,
                    0 => self.min_high,
                    _ => self.min_high.min(self.down),
                };
            }
            _ => {}
        }
        let published = self.trend != 0 && i + 1 >= self.atr_len;
        self.change.update(published.then_some(self.trend));
        if !published {
            return None;
        }
        let ht = if self.trend == 1 { self.up } else { self.down };
        let dev = self.deviation * atr.unwrap_or(f64::NAN) / 2.0;
        Some(IndicatorOutput::with_extra(
            ht,
            extras(&[
                ("trend", f64::from(self.trend)),
                ("atr_high", ht + dev),
                ("atr_low", ht - dev),
            ]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.change.alerts("HALFTREND", "HALFTREND")
    }
}

// ---------------------------------------------------------------------------------------------
// SSL channel
// ---------------------------------------------------------------------------------------------

/// The SSL channel from the highs, lows and closes: the SMA(`period`) of the highs and of the
/// lows, and `hlv`, which turns +1 when the close is above the high average, -1 when it is below
/// the low average and keeps its value in between. It starts undecided (0) and nothing is
/// published until it is decided. `value` is the upper line (the low average after a turn down,
/// else the high average), extra `ssl_down` the other one. First output on the first bar from
/// `period - 1` with a decided `hlv` (declared warmup `period`); alerts `bull` and `bear` when
/// `hlv` turns to +1 and -1.
#[derive(Debug, Clone)]
pub struct SslChannelEngine {
    period: usize,
    highs: VecDeque<f64>,
    lows: VecDeque<f64>,
    hlv: i8,
    change: StateChange,
}

impl SslChannelEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            highs: VecDeque::with_capacity(period),
            lows: VecDeque::with_capacity(period),
            hlv: 0,
            change: StateChange::default(),
        }
    }
}

impl Indicator for SslChannelEngine {
    fn name(&self) -> &str {
        "ssl_channel"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        *self = Self::new(self.period);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        push_capped(&mut self.highs, bar.high, self.period);
        push_capped(&mut self.lows, bar.low, self.period);
        if self.highs.len() < self.period {
            self.change.update(None);
            return None;
        }
        let sh = window_mean(&self.highs);
        let sl = window_mean(&self.lows);
        if bar.close > sh {
            self.hlv = 1;
        } else if bar.close < sl {
            self.hlv = -1;
        }
        if self.hlv == 0 {
            self.change.update(None);
            return None;
        }
        self.change.update(Some(self.hlv));
        let (up, down) = if self.hlv < 0 { (sl, sh) } else { (sh, sl) };
        Some(IndicatorOutput::with_extra(
            up,
            extras(&[("ssl_down", down), ("hlv", f64::from(self.hlv))]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.change.alerts("SSL", "SSL")
    }
}

// ---------------------------------------------------------------------------------------------
// Definitions
// ---------------------------------------------------------------------------------------------

/// Registry definition of `range_filter`.
pub const RANGE_FILTER_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "range_filter",
    aliases: &[],
    description: "Range Filter: a line that follows the close only when it leaves a band of `mult` times the double-smoothed absolute close-to-close change (extras `upper`, `lower`, `direction`). The direction is the sign of the line's last move beyond a relative 1e-9 and starts at 0. Published from bar 2.",
    params: &[
        ParamSpec::count("period", 100, 1, 10000).meaning("bars of the first smoothing of the change"),
        ParamSpec::number("mult", 3.0, 1e-9, 1000.0).meaning("band width in smoothed changes"),
    ],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: Verification::Verified,
    build: |p| Ok(Box::new(RangeFilterEngine::new(p.count("period"), p.number("mult")))),
};

/// Registry definition of `most`.
pub const MOST_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "most",
    aliases: &[],
    description: "MOST, a moving stop: a trailing stop whose distance is a percent of the level on the EMA of the close (extras `ema`, `direction`). The direction starts undecided instead of up and nothing is published until it is decided. The distances are a percent of the level, so prices scaled by a factor scale the line by it.",
    params: &[
        ParamSpec::count("length", 3, 1, 10000).meaning("bars of the EMA"),
        ParamSpec::number("percent", 2.0, 1e-9, 99.999999999).meaning("stop distance in percent of the level"),
    ],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: Verification::Verified,
    build: |p| Ok(Box::new(MostEngine::new(p.count("length"), p.number("percent")))),
};

/// Registry definition of `ott`.
pub const OTT_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "ott",
    aliases: &[],
    description: "Optimized Trend Tracker: a trailing stop whose distance is a percent of the level on a variable average of the close, shifted by half the percent to the side the average lies on (extras `var`, `stop`, `direction`). The average starts at the first close, the momentum sums use the bars available at the start and the direction starts undecided; the line is not shifted, and the alerts compare the average with the line two bars earlier.",
    params: &[
        ParamSpec::count("length", 2, 1, 10000).meaning("bars of the variable average"),
        ParamSpec::number("percent", 1.4, 1e-9, 99.999999999).meaning("stop distance in percent of the level"),
        ParamSpec::count("cmo_len", 9, 1, 10000).meaning("bars of the momentum ratio that scales the average"),
    ],
    checks: &[],
    facts: IndicatorFacts::price(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(OttEngine::new(
            p.count("length"),
            p.number("percent"),
            p.count("cmo_len"),
        )))
    },
};

/// Registry definition of `halftrend`.
pub const HALFTREND_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "halftrend",
    aliases: &[],
    description: "HalfTrend: a trend line from the extremes and averages of the last `amplitude` bars, with an ATR envelope (extras `trend`, `atr_high`, `atr_low`). It starts undecided and watches both sides instead of starting up; the first line is seeded with the extreme that decided it. Published once decided from bar atr_len.",
    params: &[
        ParamSpec::count("amplitude", 2, 1, 10000).meaning("bars of the extremes and averages"),
        ParamSpec::number("deviation", 2.0, 0.0, 100.0).meaning("envelope half-width in half ATRs"),
        ParamSpec::count("atr_len", 100, 1, 10000).meaning("bars of the ATR"),
    ],
    checks: &[],
    facts: IndicatorFacts::price().range_dependent(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(HalfTrendEngine::new(
            p.count("amplitude"),
            p.number("deviation"),
            p.count("atr_len"),
        )))
    },
};

/// Registry definition of `ssl_channel`.
pub const SSL_CHANNEL_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "ssl_channel",
    aliases: &[],
    description: "SSL channel: the SMA of the highs and of the lows, ordered by which side the close last broke out of (extras `ssl_down`, `hlv`). `hlv` starts undecided instead of up and nothing is published until it is decided. Published from bar period.",
    params: &[ParamSpec::count("period", 10, 1, 10000).meaning("bars of the averages")],
    checks: &[],
    facts: IndicatorFacts::price().range_dependent(),
    verification: Verification::Verified,
    build: |p| Ok(Box::new(SslChannelEngine::new(p.count("period")))),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(h: f64, l: f64, c: f64) -> Bar {
        Bar::new(0, c, h, l, c, 0.0)
    }

    fn close_bar(c: f64) -> Bar {
        bar(c, c, c)
    }

    #[test]
    fn range_filter_follows_only_outside_the_band() {
        let mut rf = RangeFilterEngine::new(1, 1.0);
        assert!(rf.on_bar(&close_bar(10.0)).is_none());
        // |a| = 2 -> r = 2, close 12 > f=10: max(10, 12 - 2) = 10, no move
        let o = rf.on_bar(&close_bar(12.0)).unwrap();
        assert_eq!(o.value, 10.0);
        assert_eq!(o.extra["upper"], 12.0);
        assert_eq!(o.extra["lower"], 8.0);
        assert_eq!(o.extra["direction"], 0.0);
        // |a| = 6 -> S = 6 (alpha 1), r = 6; close 18: max(10, 12) = 12 up
        let o = rf.on_bar(&close_bar(18.0)).unwrap();
        assert_eq!(o.value, 12.0);
        assert_eq!(o.extra["direction"], 1.0);
    }

    #[test]
    fn most_starts_undecided_and_turns_up_above_the_short_stop() {
        let mut most = MostEngine::new(1, 10.0);
        assert!(most.on_bar(&close_bar(100.0)).is_none());
        // ema = close; ss_0 = 110. 105 is below, still undecided.
        assert!(most.on_bar(&close_bar(105.0)).is_none());
        // ss_1 = min(115.5, 110) = 110 since 105 < 110; 120 > 110 -> up, stop = ls = max(108, ..)
        let o = most.on_bar(&close_bar(120.0)).unwrap();
        assert_eq!(o.extra["direction"], 1.0);
        assert!((o.value - 108.0).abs() < 1e-12);
    }

    #[test]
    fn most_is_scale_invariant() {
        let closes = [100.0, 104.0, 99.0, 107.0, 111.0, 95.0, 90.0, 99.0];
        let run = |k: f64| {
            let mut m = MostEngine::new(2, 2.0);
            closes
                .iter()
                .map(|&c| m.on_bar(&close_bar(c * k)))
                .collect::<Vec<_>>()
        };
        for (a, b) in run(1.0).iter().zip(run(3.0).iter()) {
            match (a, b) {
                (Some(a), Some(b)) => {
                    assert!((a.value * 3.0 - b.value).abs() < 1e-9);
                    assert_eq!(a.extra["direction"], b.extra["direction"]);
                }
                (None, None) => {}
                _ => panic!("published on one side only"),
            }
        }
    }

    #[test]
    fn ott_sums_use_the_bars_available_and_publish_from_cmo_len() {
        let mut ott = OttEngine::new(2, 1.4, 3);
        assert_eq!(ott.warmup_period(), 4);
        let closes = [10.0, 11.0, 12.0, 13.0, 14.0, 13.0, 15.0];
        let outs: Vec<_> = closes.iter().map(|&c| ott.on_bar(&close_bar(c))).collect();
        assert!(outs[..3].iter().all(Option::is_none));
    }

    #[test]
    fn ssl_channel_starts_undecided_and_orders_the_lines() {
        let mut ssl = SslChannelEngine::new(2);
        assert!(ssl.on_bar(&bar(11.0, 9.0, 10.0)).is_none());
        // sh = 11.5, sl = 9.5 ; close 10 is inside -> undecided
        assert!(ssl.on_bar(&bar(12.0, 10.0, 10.0)).is_none());
        // sh = 13, sl = 10.5 ; close 14 above -> +1
        let o = ssl.on_bar(&bar(14.0, 11.0, 14.0)).unwrap();
        assert_eq!(o.extra["hlv"], 1.0);
        assert_eq!(o.value, 13.0);
        assert_eq!(o.extra["ssl_down"], 10.5);
        // close below the low average: -1, lines swap
        let o = ssl.on_bar(&bar(12.0, 6.0, 6.0)).unwrap();
        assert_eq!(o.extra["hlv"], -1.0);
        assert_eq!(o.value, 8.5);
        assert_eq!(ssl.alerts()[0].kind, "bear");
    }

    #[test]
    fn halftrend_publishes_only_once_decided_and_after_the_atr() {
        let mut ht = HalfTrendEngine::new(2, 2.0, 3);
        let bars: Vec<Bar> = (0..12)
            .map(|i| {
                let c = 100.0 + (i as f64) * 2.0;
                bar(c + 1.0, c - 1.0, c)
            })
            .collect();
        let outs: Vec<_> = bars.iter().map(|b| ht.on_bar(b)).collect();
        assert!(outs[..2].iter().all(Option::is_none));
        let first = outs.iter().flatten().next().expect("a value");
        assert_eq!(first.extra["trend"], 1.0);
        assert!(first.extra["atr_high"] >= first.value);
    }
}
