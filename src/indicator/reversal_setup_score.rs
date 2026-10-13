//! Reversal Setup Score: two-layer reversal scoring — a setup score (momentum, overextension,
//! candle, volume) and a confirmation score (spring/upthrust, BOS/CHoCH, fake breaks, climax
//! sequence, premium/discount) — with event decay, follow-through, conflicts and a status per
//! side.
//!
//! Default momentum mix "A: RSI+MFI+MACD" (the Stoch-RSI terms of mixes B/C are not ported).
//!
//! **Setup (bull; bear mirrored).** RSI below 35 (+1), RSI below 45 and rising (+0.5), MFI below
//! 30 (+1), MFI rising from below 40 (+0.5) — without real volume the two MFI terms are taken
//! by RSI below 35 / RSI below 45 and rising; MACD histogram rising from below 0 (+0.5); close
//! more than 1.5 ATR below EMA 20 (+1) and more than 2.25 ATR (+0.5); low at or below the lower
//! Bollinger band (+1); lower wick ≥ 40 % of the range (+1); bar efficiency (|Δclose| / range)
//! EMA 5 falling for 3 bars with the close below EMA 20 (+1.5); relative volume below 0.6
//! (-0.5); shock bar — range > 2.5 ATR and relative volume > 2 (-2). Floored at 0, × 1.2 in a
//! range (efficiency ratio 10 ≤ 0.35), × 0.8 in a trend (EMA 20 against EMA 50 with ER > 0.35).
//!
//! **Confirmation (bull).** Spring at the last micro (3/3) or major (7/7) swing low — wick ≥ 35 %,
//! range ≥ 0.5 ATR, close back above — scored base 1.5 (micro) / 3 (major) +1.5 with 3+ quality
//! factors, +0.5 with 1–2 (wick ≥ 55 %, range ≥ 0.8 ATR, relative volume ≥ 1.3, sweep ≥ 0.15
//! ATR); BOS above the last micro/major swing high, base 0.5 / 2, +1.5 with 2+ factors, +0.5 with
//! 1 (break ≥ 0.1 ATR, ≥ 0.3 ATR, relative volume ≥ 1.3), CHoCH +0.75 / +1.5; fake breakdown
//! without spring +0.5, major fake breakdown +1; climax candle (range ≥ 1.8 ATR, volume ≥ 1.8 ×
//! average when real) below EMA 20 +1; climax sequence test valid +1.5, confirmed +2.5; discount
//! zone of the major swing range (lowest quarter) +1, premium zone -0.5. Floored at 0, × 1.3 in a
//! trend, × 1.2 / 0.8 for high / low Bollinger-width regimes (width against its 50-bar average
//! above 1.3 / below 0.7).
//!
//! **Climax sequence.** A climax candle starts it (selling climax = bearish candle → bull
//! sequence); then reacting (close beyond the climax close), test pending (back within 1.5 ATR
//! of it, at least 2 bars later), test valid (range ≤ 0.6 × climax range, volume ≤ 0.7 × climax
//! volume when real, close within 1.5 ATR), confirmed (BOS in the sequence direction); it lapses
//! 20 bars after the climax.
//!
//! **Events.** A spring, BOS or newly confirmed climax sequence is a bull event: its close is the
//! trigger, its confirmation is latched and decays (full for 3 bars, then linearly to 20 % at 6,
//! 0 after). Follow-through within 1–4 bars: higher close, close above the trigger, no bear BOS.
//! Failed: 6 bars without follow-through, or the close back below the trigger within 12 bars
//! after a follow-through. Status: `failed`, `candidate` (setup ≥ 3, decayed confirmation ≥ 2,
//! follow-through), `pending` (without follow-through), `setup` (setup ≥ 5), `alert`
//! (confirmation without setup), `watch` (setup ≥ 3), `none` — `conflicted` instead of
//! candidate/pending/setup when the other side is confirmed too or the setup runs against a
//! strong trend (ER > 0.45). Quality `0..=100` from 50: ±15 efficiency falling at the level/EMA
//! side, +10 wick, +15 absorption (relative volume ≥ 1.5 with range < 0.7 ATR), -20 shock bar,
//! -15 momentum without confirmation, -10 event decayed below 0.3, +10 CHoCH, ±5 zone.
//!
//! `value`: bull score `1.0 · setup + 1.3 · confirmation`; `secondary`: bear score. `extra`:
//! `bull_setup`, `bull_conf`, `bull_conf_decayed`, `bull_quality`, `bull_status` (0 none,
//! 1 watch, 2 setup, 3 alert, 4 pending, 5 candidate, 6 conflicted, 7 failed), the same for
//! `bear_`, `conflict_count`, `clx_state` (0..5), `clx_direction`, `price_zone` (+1 premium, -1
//! discount), `efficiency_ratio`, `rsi`. `state`: `<bull status>|<bear status>`.
//!
//! Alerts (closed bars; prefix = expected move, warnings without one): `bull_review` /
//! `bear_review` (setup ≥ 4, decayed confirmation ≥ 1.5, fewer than two conflicts, event within
//! 2 bars), `bull_action` / `bear_action` (setup ≥ 5, decayed confirmation ≥ 3, follow-through,
//! no opposed/trend conflict, close beyond the trigger, quality ≥ 50), `bull_candidate` /
//! `bear_candidate` (status becomes candidate), `bull_spring`, `bear_upthrust`, `bull_major_bos`,
//! `bear_major_bos`, `bull_choch`, `bear_choch`, `bear_failed_bull_reversal` /
//! `bull_failed_bear_reversal` (a failed side while the other has a setup — a trap hint),
//! `long_signal_invalidated` / `short_signal_invalidated` (a former candidate failed), `conflict`
//! (two or more conflicts, on entry). Candidate and conflict fire on entry rather
//! than on every bar.
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs (also inside MACD
//! 12/26/9) seeded with the first value; ATR = Wilder average of the true range (first bar high -
//! low), SMA-seeded; RSI = Wilder averages of the close changes, SMA-seeded; MFI over the last 14
//! changes of hlc3 (50 without flow, 100 without negative flow); population standard deviation;
//! `ta.falling` = below each of the previous 3 values; pivots strict on both sides; real volume =
//! cumulative volume so far positive. Swings, BOS and the micro/major swing trends run from the
//! first bar; scores, the climax sequence, events and output start once every input exists (bar
//! 68 with the defaults: the 50-bar Bollinger-width average).
//!
//! Golden reference: `reference/kestrel_reference/fixtures/reversal_setup_score.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

const STATUS: [&str; 8] = [
    "none",
    "watch",
    "setup",
    "alert",
    "pending",
    "candidate",
    "conflicted",
    "failed",
];

#[derive(Debug, Clone, Copy)]
struct Row {
    high: f64,
    low: f64,
    close: f64,
    eff_ema: Option<f64>,
}

/// One side's event tracking.
#[derive(Debug, Clone, Copy, Default)]
struct Side {
    trigger: Option<f64>,
    event_bar: Option<usize>,
    event_conf: f64,
    ft: bool,
    failed: bool,
    was_candidate: bool,
    prev_failed: bool,
    prev_trap: bool,
    prev_status: usize,
}

/// Streaming Reversal Setup Score; see the module documentation.
pub struct ReversalSetupScore {
    ema20: Ema,
    ema50: Ema,
    macd_fast: Ema,
    macd_slow: Ema,
    macd_signal: Ema,
    eff_ema: Ema,
    atr: Rma,
    gain: Rma,
    loss: Rma,
    bb_basis: Sma,
    bb_width_avg: Sma,
    avg_vol: Sma,
    rows: VecDeque<Row>,
    flows: VecDeque<(f64, f64)>,
    prev_hlc3: Option<f64>,
    prev_rsi: Option<f64>,
    prev_mfi: Option<f64>,
    prev_hist: Option<f64>,
    cum_volume: f64,
    bars: usize,
    micro: [Option<f64>; 2],
    major: [Option<f64>; 2],
    micro_trend: i8,
    major_trend: i8,
    clx_state: u8,
    clx_dir: i8,
    clx: (f64, f64, f64, f64, f64, usize),
    prev_clx_confirmed: bool,
    bull: Side,
    bear: Side,
    prev_conflict: bool,
    alerts: Vec<IndicatorAlert>,
}

impl Default for ReversalSetupScore {
    fn default() -> Self {
        Self::new()
    }
}

fn spring_score(wick: f64, catr: f64, rv: f64, sweep: f64, major: bool) -> f64 {
    let factors = u8::from(wick >= 0.55)
        + u8::from(catr >= 0.8)
        + u8::from(rv >= 1.3)
        + u8::from(sweep >= 0.15);
    let base = if major { 3.0 } else { 1.5 };
    if factors >= 3 {
        base + 1.5
    } else if factors >= 1 {
        base + 0.5
    } else {
        base
    }
}

fn bos_score(sweep: f64, rv: f64, major: bool) -> f64 {
    let factors = u8::from(sweep >= 0.1) + u8::from(sweep >= 0.3) + u8::from(rv >= 1.3);
    let base = if major { 2.0 } else { 0.5 };
    if factors >= 2 {
        base + 1.5
    } else if factors >= 1 {
        base + 0.5
    } else {
        base
    }
}

fn decay(age: usize) -> f64 {
    if age <= 3 {
        1.0
    } else if age <= 6 {
        1.0 - 0.8 * (age - 3) as f64 / 3.0
    } else {
        0.0
    }
}

impl ReversalSetupScore {
    pub fn new() -> Self {
        Self {
            ema20: Ema::new(20),
            ema50: Ema::new(50),
            macd_fast: Ema::new(12),
            macd_slow: Ema::new(26),
            macd_signal: Ema::new(9),
            eff_ema: Ema::new(5),
            atr: Rma::new(14),
            gain: Rma::new(14),
            loss: Rma::new(14),
            bb_basis: Sma::new(20),
            bb_width_avg: Sma::new(50),
            avg_vol: Sma::new(20),
            rows: VecDeque::with_capacity(24),
            flows: VecDeque::with_capacity(15),
            prev_hlc3: None,
            prev_rsi: None,
            prev_mfi: None,
            prev_hist: None,
            cum_volume: 0.0,
            bars: 0,
            micro: [None; 2],
            major: [None; 2],
            micro_trend: 0,
            major_trend: 0,
            clx_state: 0,
            clx_dir: 0,
            clx: (0.0, 0.0, 0.0, 0.0, 0.0, 0),
            prev_clx_confirmed: false,
            bull: Side::default(),
            bear: Side::default(),
            prev_conflict: false,
            alerts: Vec::new(),
        }
    }

    fn at(&self, k: usize) -> Option<&Row> {
        self.rows.len().checked_sub(k + 1).map(|i| &self.rows[i])
    }

    fn pivot(&self, n: usize, low_side: bool) -> Option<f64> {
        let centre = self.at(n)?;
        self.at(2 * n)?;
        let v = if low_side { centre.low } else { centre.high };
        for k in (0..n).chain(n + 1..=2 * n) {
            let b = self.at(k)?;
            let ok = if low_side { v < b.low } else { v > b.high };
            if !ok {
                return None;
            }
        }
        Some(v)
    }
}

impl Indicator for ReversalSetupScore {
    fn name(&self) -> &str {
        "reversal_setup_score"
    }

    fn warmup_period(&self) -> usize {
        68
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let t = self.bars;
        self.bars += 1;
        let prev = self.rows.back().copied();
        let prev_close = prev.map(|p| p.close);

        // Series.
        let ema20 = self.ema20.update(bar.close).expect("first-sample seed");
        let ema50 = self.ema50.update(bar.close).expect("first-sample seed");
        let tr = match prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let rsi = prev_close.and_then(|pc| {
            let ch = bar.close - pc;
            let g = self.gain.update(ch.max(0.0));
            let l = self.loss.update((-ch).max(0.0));
            match (g, l) {
                (Some(g), Some(l)) => Some(if g == 0.0 && l == 0.0 {
                    50.0
                } else if l == 0.0 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + g / l)
                }),
                _ => None,
            }
        });
        let macd =
            self.macd_fast.update(bar.close).unwrap() - self.macd_slow.update(bar.close).unwrap();
        let hist = macd - self.macd_signal.update(macd).unwrap();
        self.cum_volume += bar.volume;
        let real = self.cum_volume > 0.0;
        let hlc3 = (bar.high + bar.low + bar.close) / 3.0;
        let mfi = self.prev_hlc3.and_then(|p| {
            let raw = hlc3 * bar.volume;
            let flow = if hlc3 > p {
                (raw, 0.0)
            } else if hlc3 < p {
                (0.0, raw)
            } else {
                (0.0, 0.0)
            };
            self.flows.push_back(flow);
            if self.flows.len() > 14 {
                self.flows.pop_front();
            }
            (self.flows.len() == 14).then(|| {
                let (pos, neg) = self
                    .flows
                    .iter()
                    .fold((0.0, 0.0), |a, f| (a.0 + f.0, a.1 + f.1));
                if pos == 0.0 && neg == 0.0 {
                    50.0
                } else if neg == 0.0 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + pos / neg)
                }
            })
        });
        self.prev_hlc3 = Some(hlc3);
        let avg_vol = self.avg_vol.update(bar.volume);
        let rel_vol = match avg_vol {
            Some(a) if real && a > 0.0 => bar.volume / a,
            _ => 1.0,
        };
        let range = bar.high - bar.low;
        let eff = match prev_close {
            Some(pc) if range > 0.0 => (bar.close - pc).abs() / range,
            _ => 0.0,
        };
        let eff_ema = prev_close.map(|_| self.eff_ema.update(eff).unwrap());
        self.rows.push_back(Row {
            high: bar.high,
            low: bar.low,
            close: bar.close,
            eff_ema,
        });
        if self.rows.len() > 22 {
            self.rows.pop_front();
        }
        let bb_basis = self.bb_basis.update(bar.close);
        let bb_dev = bb_basis.map(|b| {
            let var = (0..20)
                .map(|k| (self.at(k).unwrap().close - b).powi(2))
                .sum::<f64>()
                / 20.0;
            var.sqrt() * 2.0
        });
        let bb_width_avg = bb_dev.and_then(|d| self.bb_width_avg.update(2.0 * d));

        // Swings, BOS and CHoCH from the first bar.
        if let Some(v) = self.pivot(3, false) {
            self.micro[0] = Some(v);
        }
        if let Some(v) = self.pivot(3, true) {
            self.micro[1] = Some(v);
        }
        if let Some(v) = self.pivot(7, false) {
            self.major[0] = Some(v);
        }
        if let Some(v) = self.pivot(7, true) {
            self.major[1] = Some(v);
        }
        let [sh, sl] = self.micro;
        let [msh, msl] = self.major;
        let up_break = |lvl: Option<f64>| matches!((lvl, prev_close), (Some(l), Some(pc)) if bar.close > l && pc <= l);
        let dn_break = |lvl: Option<f64>| matches!((lvl, prev_close), (Some(l), Some(pc)) if bar.close < l && pc >= l);
        let micro_bull_bos = up_break(sh);
        let micro_bear_bos = dn_break(sl);
        let major_bull_bos = up_break(msh);
        let major_bear_bos = dn_break(msl);
        let micro_bull_choch = micro_bull_bos && self.micro_trend == -1;
        let micro_bear_choch = micro_bear_bos && self.micro_trend == 1;
        let major_bull_choch = major_bull_bos && self.major_trend == -1;
        let major_bear_choch = major_bear_bos && self.major_trend == 1;
        if micro_bull_bos {
            self.micro_trend = 1;
        }
        if micro_bear_bos {
            self.micro_trend = -1;
        }
        if major_bull_bos {
            self.major_trend = 1;
        }
        if major_bear_bos {
            self.major_trend = -1;
        }

        let prev_rsi = std::mem::replace(&mut self.prev_rsi, rsi);
        let prev_mfi = std::mem::replace(&mut self.prev_mfi, mfi);
        let prev_hist = self.prev_hist.replace(hist);
        let (
            Some(atr),
            Some(rsi),
            Some(prev_rsi),
            Some(prev_hist),
            Some(bb_basis),
            Some(bb_dev),
            Some(bb_width_avg),
            Some(avg_vol),
        ) = (
            atr,
            rsi,
            prev_rsi,
            prev_hist,
            bb_basis,
            bb_dev,
            bb_width_avg,
            avg_vol,
        )
        else {
            return None;
        };
        let mfi_now = if real { Some(mfi?) } else { None };
        let e0 = eff_ema?;
        let falling =
            (1..=3).all(|k| matches!(self.at(k).and_then(|r| r.eff_ema), Some(p) if e0 < p));
        let close10 = self.at(10)?.close;

        let c = bar.close;
        let p2e = if atr > 0.0 { (c - ema20) / atr } else { 0.0 };
        let upper = bar.high - bar.open.max(c);
        let lower = bar.open.min(c) - bar.low;
        let uwr = if range > 0.0 { upper / range } else { 0.0 };
        let lwr = if range > 0.0 { lower / range } else { 0.0 };
        let catr = if atr > 0.0 { range / atr } else { 0.0 };
        let shock = catr > 2.5 && rel_vol > 2.0;
        let bb_upper = bb_basis + bb_dev;
        let bb_lower = bb_basis - bb_dev;
        let width_ratio = if bb_width_avg > 0.0 {
            (bb_upper - bb_lower) / bb_width_avg
        } else {
            1.0
        };
        let path: f64 = (0..10)
            .map(|k| (self.at(k).unwrap().close - self.at(k + 1).unwrap().close).abs())
            .sum();
        let er = if path > 0.0 {
            (c - close10).abs() / path
        } else {
            0.0
        };
        let trend_up = ema20 > ema50 && er > 0.35;
        let trend_dn = ema20 < ema50 && er > 0.35;
        let is_range = er <= 0.35;
        let setup_mult = if is_range {
            1.2
        } else if trend_up || trend_dn {
            0.8
        } else {
            1.0
        };
        let conf_mult = if trend_up || trend_dn { 1.3 } else { 1.0 };
        let vol_bonus = if width_ratio > 1.3 {
            1.2
        } else if width_ratio < 0.7 {
            0.8
        } else {
            1.0
        };
        let (hh, ll) = (0..20).fold((f64::NEG_INFINITY, f64::INFINITY), |(h, l), k| {
            let r = self.at(k).unwrap();
            (h.max(r.high), l.min(r.low))
        });
        let span = hh - ll;
        let near_high = (if span > 0.0 { (hh - c) / span } else { 0.5 }) < 0.15;
        let near_low = (if span > 0.0 { (c - ll) / span } else { 0.5 }) < 0.15;
        let zone = match (msh, msl) {
            (Some(h), Some(l)) if h - l > 0.0 => {
                let r = h - l;
                if c >= l + r * 0.75 {
                    1
                } else if c <= l + r * 0.25 {
                    -1
                } else {
                    0
                }
            }
            _ => 0,
        };
        let fake_up = matches!(sh, Some(h) if bar.high > h && c <= h);
        let fake_dn = matches!(sl, Some(l) if bar.low < l && c >= l);
        let major_fake_up = matches!(msh, Some(h) if bar.high > h && c <= h);
        let major_fake_dn = matches!(msl, Some(l) if bar.low < l && c >= l);
        let spring = fake_dn && lwr >= 0.35 && catr >= 0.5;
        let upthrust = fake_up && uwr >= 0.35 && catr >= 0.5;
        let major_spring = major_fake_dn && lwr >= 0.35 && catr >= 0.5;
        let major_upthrust = major_fake_up && uwr >= 0.35 && catr >= 0.5;

        // Climax sequence.
        let climax = catr >= 1.8 && (!real || (avg_vol > 0.0 && bar.volume >= 1.8 * avg_vol));
        if climax {
            self.clx_state = 1;
            self.clx_dir = if c < bar.open {
                1
            } else if c > bar.open {
                -1
            } else {
                0
            };
            self.clx = (c, bar.high, bar.low, range, bar.volume, t);
        }
        let (clx_close, clx_high, clx_low, clx_range, clx_vol, clx_bar) = self.clx;
        if self.clx_state >= 1 && t - clx_bar <= 20 {
            match self.clx_state {
                1 if (self.clx_dir == 1 && c > clx_close)
                    || (self.clx_dir == -1 && c < clx_close) =>
                {
                    self.clx_state = 2;
                }
                2 => {
                    let back = if atr > 0.0 && self.clx_dir == 1 {
                        c < clx_close + 1.5 * atr && c > clx_low - 0.5 * atr
                    } else if atr > 0.0 && self.clx_dir == -1 {
                        c > clx_close - 1.5 * atr && c < clx_high + 0.5 * atr
                    } else {
                        false
                    };
                    if back && t - clx_bar >= 2 {
                        self.clx_state = 3;
                    }
                }
                3 => {
                    let test = range <= clx_range * 0.6
                        && range > 0.0
                        && (!real || bar.volume <= clx_vol * 0.7);
                    let in_zone = atr > 0.0 && (c - clx_close).abs() <= 1.5 * atr;
                    if test && in_zone {
                        self.clx_state = 4;
                    }
                }
                4 if (self.clx_dir == 1 && (micro_bull_bos || major_bull_bos))
                    || (self.clx_dir == -1 && (micro_bear_bos || major_bear_bos)) =>
                {
                    self.clx_state = 5;
                }
                _ => {}
            }
        }
        if self.clx_state >= 1 && t - clx_bar > 20 {
            self.clx_state = 0;
        }
        let test_valid = self.clx_state >= 4;
        let confirmed = self.clx_state == 5;
        let just_confirmed = confirmed && !self.prev_clx_confirmed;
        self.prev_clx_confirmed = confirmed;
        let dir = self.clx_dir;

        // Setup scores.
        let mut bull_setup = 0.0;
        let mut bear_setup = 0.0;
        if rsi < 35.0 {
            bull_setup += 1.0;
        }
        if rsi < 45.0 && rsi > prev_rsi {
            bull_setup += 0.5;
        }
        if rsi > 65.0 {
            bear_setup += 1.0;
        }
        if rsi > 55.0 && rsi < prev_rsi {
            bear_setup += 0.5;
        }
        match (mfi_now, prev_mfi) {
            (Some(m), pm) => {
                if m < 30.0 {
                    bull_setup += 1.0;
                }
                if matches!(pm, Some(p) if m > p && p < 40.0) {
                    bull_setup += 0.5;
                }
                if m > 70.0 {
                    bear_setup += 1.0;
                }
                if matches!(pm, Some(p) if m < p && p > 60.0) {
                    bear_setup += 0.5;
                }
            }
            (None, _) => {
                if rsi < 35.0 {
                    bull_setup += 1.0;
                }
                if rsi < 45.0 && rsi > prev_rsi {
                    bull_setup += 0.5;
                }
                if rsi > 65.0 {
                    bear_setup += 1.0;
                }
                if rsi > 55.0 && rsi < prev_rsi {
                    bear_setup += 0.5;
                }
            }
        }
        if hist > prev_hist && prev_hist < 0.0 {
            bull_setup += 0.5;
        }
        if hist < prev_hist && prev_hist > 0.0 {
            bear_setup += 0.5;
        }
        if p2e < -1.5 {
            bull_setup += 1.0;
        }
        if p2e < -2.25 {
            bull_setup += 0.5;
        }
        if p2e > 1.5 {
            bear_setup += 1.0;
        }
        if p2e > 2.25 {
            bear_setup += 0.5;
        }
        if bar.low <= bb_lower {
            bull_setup += 1.0;
        }
        if bar.high >= bb_upper {
            bear_setup += 1.0;
        }
        if lwr >= 0.4 {
            bull_setup += 1.0;
        }
        if uwr >= 0.4 {
            bear_setup += 1.0;
        }
        if falling && c < ema20 {
            bull_setup += 1.5;
        }
        if falling && c > ema20 {
            bear_setup += 1.5;
        }
        if rel_vol < 0.6 {
            bull_setup -= 0.5;
            bear_setup -= 0.5;
        }
        if shock {
            bull_setup -= 2.0;
            bear_setup -= 2.0;
        }
        let bull_setup: f64 = f64::max(0.0, bull_setup) * setup_mult;
        let bear_setup: f64 = f64::max(0.0, bear_setup) * setup_mult;

        // Confirmation scores.
        let mut bull_conf = 0.0;
        if major_spring {
            let sweep = msl.map_or(0.0, |l| if atr > 0.0 { (l - bar.low) / atr } else { 0.0 });
            bull_conf += spring_score(lwr, catr, rel_vol, sweep, true);
        } else if spring {
            let sweep = sl.map_or(0.0, |l| if atr > 0.0 { (l - bar.low) / atr } else { 0.0 });
            bull_conf += spring_score(lwr, catr, rel_vol, sweep, false);
        }
        if major_bull_bos {
            let sw = msh.map_or(0.0, |h| if atr > 0.0 { (c - h) / atr } else { 0.0 });
            bull_conf += bos_score(sw, rel_vol, true);
            if major_bull_choch {
                bull_conf += 1.5;
            }
        } else if micro_bull_bos {
            let sw = sh.map_or(0.0, |h| if atr > 0.0 { (c - h) / atr } else { 0.0 });
            bull_conf += bos_score(sw, rel_vol, false);
            if micro_bull_choch {
                bull_conf += 0.75;
            }
        }
        if fake_dn && !spring && !major_spring {
            bull_conf += 0.5;
        }
        if major_fake_dn && !major_spring {
            bull_conf += 1.0;
        }
        if climax && c < ema20 {
            bull_conf += 1.0;
        }
        if test_valid && dir == 1 {
            bull_conf += 1.5;
        }
        if confirmed && dir == 1 {
            bull_conf += 2.5;
        }
        if zone == -1 {
            bull_conf += 1.0;
        }
        if zone == 1 {
            bull_conf -= 0.5;
        }
        let bull_conf = f64::max(0.0, bull_conf) * conf_mult * vol_bonus;

        let mut bear_conf = 0.0;
        if major_upthrust {
            let sweep = msh.map_or(0.0, |h| if atr > 0.0 { (bar.high - h) / atr } else { 0.0 });
            bear_conf += spring_score(uwr, catr, rel_vol, sweep, true);
        } else if upthrust {
            let sweep = sh.map_or(0.0, |h| if atr > 0.0 { (bar.high - h) / atr } else { 0.0 });
            bear_conf += spring_score(uwr, catr, rel_vol, sweep, false);
        }
        if major_bear_bos {
            let sw = msl.map_or(0.0, |l| if atr > 0.0 { (l - c) / atr } else { 0.0 });
            bear_conf += bos_score(sw, rel_vol, true);
            if major_bear_choch {
                bear_conf += 1.5;
            }
        } else if micro_bear_bos {
            let sw = sl.map_or(0.0, |l| if atr > 0.0 { (l - c) / atr } else { 0.0 });
            bear_conf += bos_score(sw, rel_vol, false);
            if micro_bear_choch {
                bear_conf += 0.75;
            }
        }
        if fake_up && !upthrust && !major_upthrust {
            bear_conf += 0.5;
        }
        if major_fake_up && !major_upthrust {
            bear_conf += 1.0;
        }
        if climax && c > ema20 {
            bear_conf += 1.0;
        }
        if test_valid && dir == -1 {
            bear_conf += 1.5;
        }
        if confirmed && dir == -1 {
            bear_conf += 2.5;
        }
        if zone == 1 {
            bear_conf += 1.0;
        }
        if zone == -1 {
            bear_conf -= 0.5;
        }
        let bear_conf = f64::max(0.0, bear_conf) * conf_mult * vol_bonus;

        // Events, decay, follow-through, failure.
        let new_bull = spring
            || major_spring
            || micro_bull_bos
            || major_bull_bos
            || (just_confirmed && dir == 1);
        let new_bear = upthrust
            || major_upthrust
            || micro_bear_bos
            || major_bear_bos
            || (just_confirmed && dir == -1);
        let any_bull_bos = micro_bull_bos || major_bull_bos;
        let any_bear_bos = micro_bear_bos || major_bear_bos;
        struct Eval {
            age: usize,
            decay: f64,
            conf_decayed: f64,
        }
        let track = |s: &mut Side, new: bool, conf: f64, up: bool, counter_bos: bool| -> Eval {
            if new {
                s.trigger = Some(c);
                s.event_bar = Some(t);
                s.event_conf = conf;
                s.ft = false;
                s.failed = false;
                s.was_candidate = false;
            }
            let age = s.event_bar.map_or(999, |b| t - b);
            let d = decay(age);
            let conf_decayed = conf.max(s.event_conf * d);
            let active = (1..=4).contains(&age) && s.trigger.is_some();
            let trig = s.trigger.unwrap_or(0.0);
            let pc = prev_close.unwrap();
            let follow = active
                && if up {
                    c > pc && c > trig
                } else {
                    c < pc && c < trig
                }
                && !counter_bos;
            if follow {
                s.ft = true;
            }
            let expired = age > 6 && !s.ft && s.trigger.is_some();
            let lost = s.trigger.is_some()
                && (1..=12).contains(&age)
                && if up { c < trig } else { c > trig }
                && s.ft;
            if expired || lost {
                s.failed = true;
            }
            Eval {
                age,
                decay: d,
                conf_decayed,
            }
        };
        let mut bull = self.bull;
        let mut bear = self.bear;
        let be = track(&mut bull, new_bull, bull_conf, true, any_bear_bos);
        let se = track(&mut bear, new_bear, bear_conf, false, any_bull_bos);

        let bull_ready = bull_setup >= 3.0;
        let bull_strong = bull_setup >= 5.0;
        let bull_confirmed = be.conf_decayed >= 2.0;
        let bear_ready = bear_setup >= 3.0;
        let bear_strong = bear_setup >= 5.0;
        let bear_confirmed = se.conf_decayed >= 2.0;
        let setup_conflict = bull_ready && bear_ready;
        let conf_conflict = bull_confirmed && bear_confirmed;
        let bull_trend_conflict = bull_ready && trend_dn && er > 0.45;
        let bear_trend_conflict = bear_ready && trend_up && er > 0.45;
        let bull_opposed = bull_ready && bull_confirmed && bear_confirmed;
        let bear_opposed = bear_ready && bear_confirmed && bull_confirmed;
        let is_conflict = setup_conflict
            || conf_conflict
            || bull_trend_conflict
            || bear_trend_conflict
            || bull_opposed
            || bear_opposed;
        let conflict_count = [
            setup_conflict,
            conf_conflict,
            bull_trend_conflict,
            bear_trend_conflict,
        ]
        .iter()
        .filter(|b| **b)
        .count();
        let status = |s: &Side,
                      ready: bool,
                      strong: bool,
                      confirmed: bool,
                      opposed: bool,
                      trend_conflict: bool|
         -> usize {
            if s.failed {
                7
            } else if ready && confirmed && s.ft {
                if opposed || trend_conflict {
                    6
                } else {
                    5
                }
            } else if ready && confirmed {
                if opposed || trend_conflict {
                    6
                } else {
                    4
                }
            } else if strong {
                if trend_conflict {
                    6
                } else {
                    2
                }
            } else if confirmed && !ready {
                3
            } else if ready {
                1
            } else {
                0
            }
        };
        let bull_status = status(
            &bull,
            bull_ready,
            bull_strong,
            bull_confirmed,
            bull_opposed,
            bull_trend_conflict,
        );
        let bear_status = status(
            &bear,
            bear_ready,
            bear_strong,
            bear_confirmed,
            bear_opposed,
            bear_trend_conflict,
        );
        if bull_status == 5 {
            bull.was_candidate = true;
        }
        if bear_status == 5 {
            bear.was_candidate = true;
        }
        let bull_score = bull_setup + 1.3 * bull_conf;
        let bear_score = bear_setup + 1.3 * bear_conf;
        let absorption = real && rel_vol >= 1.5 && catr < 0.7;
        let quality = |up: bool, setup: f64, conf: f64, e: &Eval, choch: bool| {
            let mut q: f64 = 50.0;
            let (at_level, ema_side, wick) = if up {
                (near_low, c < ema20, lwr)
            } else {
                (near_high, c > ema20, uwr)
            };
            if falling && (at_level || ema_side) {
                q += 15.0;
            }
            if wick > 0.4 {
                q += 10.0;
            }
            if absorption {
                q += 15.0;
            }
            if shock {
                q -= 20.0;
            }
            if setup >= 5.0 && conf < 2.0 {
                q -= 15.0;
            }
            if e.decay < 0.3 && e.age < 999 {
                q -= 10.0;
            }
            if choch {
                q += 10.0;
            }
            let good = if up { -1 } else { 1 };
            if zone == good {
                q += 5.0;
            }
            if zone == -good {
                q -= 5.0;
            }
            q.clamp(0.0, 100.0)
        };
        let bull_q = quality(
            true,
            bull_setup,
            bull_conf,
            &be,
            micro_bull_choch || major_bull_choch,
        );
        let bear_q = quality(
            false,
            bear_setup,
            bear_conf,
            &se,
            micro_bear_choch || major_bear_choch,
        );

        // Alerts.
        let mut fire = |kind: &str, strength: f64| {
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!(
                    "Reversal Setup Score: {kind} (bull {bull_score:.1}, bear {bear_score:.1})"
                ),
                strength,
            ));
        };
        if bull_setup >= 4.0
            && be.conf_decayed >= 1.5
            && conflict_count < 2
            && (new_bull || be.age <= 2)
        {
            fire("bull_review", 0.5);
        }
        if bear_setup >= 4.0
            && se.conf_decayed >= 1.5
            && conflict_count < 2
            && (new_bear || se.age <= 2)
        {
            fire("bear_review", 0.5);
        }
        if bull_setup >= 5.0
            && be.conf_decayed >= 3.0
            && bull.ft
            && !bull_opposed
            && !bull_trend_conflict
            && c > bull.trigger.unwrap_or(0.0)
            && bull_q >= 50.0
        {
            fire("bull_action", bull_q / 100.0);
        }
        if bear_setup >= 5.0
            && se.conf_decayed >= 3.0
            && bear.ft
            && !bear_opposed
            && !bear_trend_conflict
            && c < bear.trigger.unwrap_or(0.0)
            && bear_q >= 50.0
        {
            fire("bear_action", bear_q / 100.0);
        }
        if bull.failed && !bull.prev_failed && bull.was_candidate {
            fire("long_signal_invalidated", 0.6);
        }
        if bear.failed && !bear.prev_failed && bear.was_candidate {
            fire("short_signal_invalidated", 0.6);
        }
        if bull_status == 5 && bull.prev_status != 5 {
            fire("bull_candidate", bull_q / 100.0);
        }
        if bear_status == 5 && bear.prev_status != 5 {
            fire("bear_candidate", bear_q / 100.0);
        }
        let conflict_alert = is_conflict && conflict_count >= 2;
        if conflict_alert && !self.prev_conflict {
            fire("conflict", 0.4);
        }
        if spring || major_spring {
            fire("bull_spring", 0.5);
        }
        if upthrust || major_upthrust {
            fire("bear_upthrust", 0.5);
        }
        if major_bull_bos {
            fire("bull_major_bos", 0.6);
        }
        if major_bear_bos {
            fire("bear_major_bos", 0.6);
        }
        let bull_trap = bull.failed && bear_setup > 0.0;
        let bear_trap = bear.failed && bull_setup > 0.0;
        if bull_trap && !bull.prev_trap {
            fire("bear_failed_bull_reversal", 0.5);
        }
        if bear_trap && !bear.prev_trap {
            fire("bull_failed_bear_reversal", 0.5);
        }
        if micro_bull_choch || major_bull_choch {
            fire("bull_choch", 0.5);
        }
        if micro_bear_choch || major_bear_choch {
            fire("bear_choch", 0.5);
        }
        self.prev_conflict = conflict_alert;
        bull.prev_failed = bull.failed;
        bear.prev_failed = bear.failed;
        bull.prev_trap = bull_trap;
        bear.prev_trap = bear_trap;
        bull.prev_status = bull_status;
        bear.prev_status = bear_status;
        self.bull = bull;
        self.bear = bear;

        let mut extra = HashMap::new();
        for (k, v) in [
            ("bull_setup", bull_setup),
            ("bull_conf", bull_conf),
            ("bull_conf_decayed", be.conf_decayed),
            ("bull_quality", bull_q),
            ("bull_status", bull_status as f64),
            ("bear_setup", bear_setup),
            ("bear_conf", bear_conf),
            ("bear_conf_decayed", se.conf_decayed),
            ("bear_quality", bear_q),
            ("bear_status", bear_status as f64),
            ("conflict_count", conflict_count as f64),
            ("clx_state", self.clx_state as f64),
            ("clx_direction", self.clx_dir as f64),
            ("price_zone", zone as f64),
            ("efficiency_ratio", er),
            ("rsi", rsi),
        ] {
            extra.insert(k.to_string(), v);
        }
        let state = format!("{}|{}", STATUS[bull_status], STATUS[bear_status]);
        Some(
            IndicatorOutput::with_extra(bull_score, extra)
                .with_secondary(bear_score)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
