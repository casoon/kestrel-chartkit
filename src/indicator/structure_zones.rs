//! Structure Zones: structural events on micro (3/3) and major (7/7) swings, automatic
//! support/resistance zones with a lifecycle, and an assessment of the nearest support and
//! resistance zone — reach, hold and break likelihood.
//!
//! **Zones.** Created from: each major swing high/low (resistance/support, width 0.3 ATR around
//! the pivot, quality 2); a spring (between the low and the swept swing low; major springs take
//! precedence) and an upthrust mirrored, quality 3 for wick ≥ 55 % with range ≥ 0.8 ATR (micro: 2
//! for wick ≥ 35 %, else 1; major: else 2), +1 with absorption (real volume, effort-vs-result
//! falling three bars, relative volume > 1.2); a major BOS flips the broken swing into a zone of
//! the other role; a climax candle (range ≥ 1.8 ATR, volume ≥ 1.8 × average when real) spans its
//! bar, quality 3 with relative volume ≥ 2.7. Zones expire after 200 bars. From the bar after
//! creation: a new visit (the range enters the zone) counts a touch and makes a fresh zone
//! tested; within a visit, the first close beyond the midpoint in the zone's direction with a
//! candle in that direction is a bounce (confirmed; ≥ 0.5 ATR from the far edge good, else weak);
//! a close past the midpoint against it is a deep penetration; a close beyond the far edge
//! (previous close not) breaks it; a broken zone that price re-enters closing on the other side
//! of the midpoint flips (role reversed).
//!
//! **Assessment** of the nearest support (midpoint below the close) and resistance (above) among
//! the unbroken zones within 3 ATR: structural strength (type, quality, status, age), historical
//! respect (bounces, penetrations, touches), approach quality (range contraction, drying volume,
//! growing wick, RSI/MFI extremes against momentum and a major BOS through it), context (range vs.
//! trend, low efficiency), reach (distance and the last two closes). Hold = 0.30 structural +
//! 0.25 respect + 0.30 approach + 0.15 context (approach and context mapped from −100..100 to
//! 0..100), plus 5 with an unfilled fair value gap nearby (gap ≥ 0.3 ATR, at most 20 bars old), plus
//! 0.1 × the score of a nearby unmitigated order block (last opposite candle within 10 bars before
//! a BOS; 25, +30 for a wide BOS bar, +20 with an overlapping gap, +15 with relative volume ≥ 1.3)
//! and plus 7 near the point of control (else 3 inside the value area) of the volume profile of
//! the last major swing leg (15 rows, real volume only). Break = 100 − hold with penalties.
//! State: 0 stable, 1 hold possible, 2 weakening, 3 under pressure, 4 reaction likely, 5 break
//! likely.
//!
//! `value`: the number of tracked zones. `extra` (each while it exists): `support_mid`,
//! `support_upper`, `support_lower`, `support_type` (1/2 swing, 3 spring, 4 upthrust, 5/6 BOS
//! flip, 7/8 climax), `support_status` (0 fresh, 1 tested, 2 confirmed, 4 flipped),
//! `support_distance_atr`, `support_reach`, `support_hold`, `support_break`, `support_structural`,
//! `support_respect`, `support_approach`, `support_context`, `support_state`, the same for
//! `resistance_`; always `micro_trend`, `major_trend` (BOS-driven swing trend), `clx_state`
//! (0..5), `clx_direction`, `event_code` (bit mask of this bar's events: 1 swing high, 2
//! swing low, 4/8 micro BOS up/down, 16/32 major BOS, 64/128 spring/upthrust, 256/512 major,
//! 1024 climax, 2048 test, 4096/8192 rejection, 16384/32768 EQH/EQL, 65536/131072 CHoCH); `poc`,
//! `va_high`, `va_low` once a profile exists.
//!
//! Alerts on entry (prefix = expected move, warnings without one): `support_relevant` /
//! `resistance_relevant` (zone within 1 ATR, reach ≥ 60), `bull_support_hold` /
//! `bear_resistance_hold` (hold ≥ 60, break < 45), `bear_support_break` / `bull_resistance_break`
//! (break ≥ 60, hold < 45). The original re-fires them on every bar the condition holds.
//!
//! Conventions the original leaves open (also in the golden fixture): EMAs seeded with the first
//! close; ATR = Wilder average of the true range (first bar high - low), SMA-seeded; RSI = Wilder
//! averages of the close changes, SMA-seeded; MFI over the last 14 changes of hlc3 (50 without
//! flow, 100 without negative flow); pivots strict on both sides; real volume = cumulative volume
//! so far positive. Swings, BOS, swing trends, the volume profile and the effort-vs-result series
//! run from the first bar; zones, gaps, order blocks and the climax sequence start with the first
//! ATR (Pine compares against `na` before); output once RSI — with real volume also MFI and the
//! volume average — exists. As in the original the climax sequence advances several stages in one
//! bar when they are met, and the profile's high/low include the confirming bar.
//!
//! Golden reference: `reference/kestrel_reference/fixtures/structure_zones.py`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::{Ema, Rma, Sma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

const ZONE_WIDTH_ATR: f64 = 0.3;
const ZONE_EXPIRE: usize = 200;
const MAX_ASSESS_ATR: f64 = 3.0;
const VP_ROWS: usize = 15;
const HISTORY: usize = 220;

#[derive(Debug, Clone, Copy)]
struct Row {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
    lwr: f64,
    uwr: f64,
    evr_norm: f64,
}

#[derive(Debug, Clone)]
struct Zone {
    kind: u8,
    upper: f64,
    lower: f64,
    mid: f64,
    dir: i8,
    quality: u8,
    created: usize,
    status: u8,
    touches: u32,
    good: u32,
    weak: u32,
    max_bounce: f64,
    last_bounce: f64,
    deep: u32,
    visit_bounced: bool,
}

#[derive(Debug, Clone, Copy)]
struct Gap {
    upper: f64,
    lower: f64,
    dir: i8,
    created: usize,
}

#[derive(Debug, Clone, Copy)]
struct Block {
    upper: f64,
    lower: f64,
    mitigated: bool,
    score: f64,
}

/// Streaming Structure Zones; see the module documentation.
pub struct StructureZones {
    atr: Rma,
    ema20: Ema,
    ema50: Ema,
    gain: Rma,
    loss: Rma,
    avg_vol: Sma,
    evr_avg: Sma,
    approach_avg: Sma,
    rows: VecDeque<Row>,
    flows: VecDeque<(f64, f64)>,
    prev_hlc3: Option<f64>,
    cum_volume: f64,
    bars: usize,
    swing_high: Option<f64>,
    swing_low: Option<f64>,
    major_high: Option<f64>,
    major_low: Option<f64>,
    prev_swing_high: Option<f64>,
    prev_swing_low: Option<f64>,
    prev_major_sh: Option<(f64, usize)>,
    prev_major_sl: Option<(f64, usize)>,
    profile: Option<(f64, f64, f64)>,
    micro_trend: i8,
    major_trend: i8,
    clx_state: u8,
    clx_dir: i8,
    clx: (f64, f64, f64, f64, f64, usize),
    clx_test_bar: Option<usize>,
    bars_since_test: usize,
    gaps: Vec<Gap>,
    bull_blocks: Vec<Block>,
    bear_blocks: Vec<Block>,
    zones: Vec<Zone>,
    prev_flags: [bool; 6],
    alerts: Vec<IndicatorAlert>,
}

impl Default for StructureZones {
    fn default() -> Self {
        Self::new()
    }
}

fn clamp(v: f64, lo: f64, hi: f64) -> f64 {
    v.max(lo).min(hi)
}

impl StructureZones {
    pub fn new() -> Self {
        Self {
            atr: Rma::new(14),
            ema20: Ema::new(20),
            ema50: Ema::new(50),
            gain: Rma::new(14),
            loss: Rma::new(14),
            avg_vol: Sma::new(20),
            evr_avg: Sma::new(14),
            approach_avg: Sma::new(5),
            rows: VecDeque::with_capacity(HISTORY + 1),
            flows: VecDeque::with_capacity(15),
            prev_hlc3: None,
            cum_volume: 0.0,
            bars: 0,
            swing_high: None,
            swing_low: None,
            major_high: None,
            major_low: None,
            prev_swing_high: None,
            prev_swing_low: None,
            prev_major_sh: None,
            prev_major_sl: None,
            profile: None,
            micro_trend: 0,
            major_trend: 0,
            clx_state: 0,
            clx_dir: 0,
            clx: (0.0, 0.0, 0.0, 0.0, 0.0, 0),
            clx_test_bar: None,
            bars_since_test: 999,
            gaps: Vec::new(),
            bull_blocks: Vec::new(),
            bear_blocks: Vec::new(),
            zones: Vec::new(),
            prev_flags: [false; 6],
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

    /// Volume profile of the leg from `start` to `end` (absolute bars): (POC, VA high, VA low).
    fn leg_profile(&self, t: usize, start: usize, end: usize) -> Option<(f64, f64, f64)> {
        let leg_len = end - start;
        let cur = self.at(0)?;
        let (mut hi, mut lo) = (cur.high, cur.low);
        let span = leg_len.min(200);
        for bk in 0..=span {
            let r = self.at(t - end + bk)?;
            hi = hi.max(r.high);
            lo = lo.min(r.low);
        }
        let step = if hi > lo {
            (hi - lo) / VP_ROWS as f64
        } else {
            0.0
        };
        if step <= 0.0 {
            return None;
        }
        let mut bins = [0.0f64; VP_ROWS];
        for bk in 0..=span {
            let r = self.at(t - end + bk)?;
            if r.volume > 0.0 {
                for (row, bin) in bins.iter_mut().enumerate() {
                    let row_lo = lo + row as f64 * step;
                    let row_hi = row_lo + step;
                    if r.high >= row_lo && r.low < row_hi {
                        let overlap = r.high.min(row_hi) - r.low.max(row_lo);
                        let rng = r.high - r.low;
                        let portion = if rng > 0.0 {
                            overlap / rng
                        } else {
                            1.0 / VP_ROWS as f64
                        };
                        *bin += r.volume * portion;
                    }
                }
            }
        }
        let (mut best, mut best_vol) = (0usize, 0.0);
        for (i, b) in bins.iter().enumerate() {
            if *b > best_vol {
                best_vol = *b;
                best = i;
            }
        }
        let total: f64 = bins.iter().sum();
        let (mut va_vol, mut va_lo, mut va_hi) = (best_vol, best, best);
        for _ in 0..VP_ROWS {
            if va_vol >= total * 0.70 {
                break;
            }
            let below = if va_lo > 0 { bins[va_lo - 1] } else { 0.0 };
            let above = if va_hi < VP_ROWS - 1 {
                bins[va_hi + 1]
            } else {
                0.0
            };
            if below >= above && va_lo > 0 {
                va_lo -= 1;
                va_vol += below;
            } else if va_hi < VP_ROWS - 1 {
                va_hi += 1;
                va_vol += above;
            } else if va_lo > 0 {
                va_lo -= 1;
                va_vol += below;
            } else {
                break;
            }
        }
        Some((
            lo + (best as f64 + 0.5) * step,
            lo + (va_hi + 1) as f64 * step,
            lo + va_lo as f64 * step,
        ))
    }

    fn add_zone(&mut self, t: usize, kind: u8, upper: f64, lower: f64, dir: i8, quality: u8) {
        self.zones.push(Zone {
            kind,
            upper,
            lower,
            mid: (upper + lower) / 2.0,
            dir,
            quality,
            created: t,
            status: 0,
            touches: 0,
            good: 0,
            weak: 0,
            max_bounce: 0.0,
            last_bounce: 0.0,
            deep: 0,
            visit_bounced: false,
        });
    }
}

impl Indicator for StructureZones {
    fn name(&self) -> &str {
        "structure_zones"
    }

    /// The earliest possible first output: without volume it starts with ATR (14) and RSI (14,
    /// from the second bar) on bar 15. With real volume it also waits for the MFI and the 20-bar
    /// volume average: first output on bar 20.
    fn warmup_period(&self) -> usize {
        15
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
        let (o, h, l, c, v) = (bar.open, bar.high, bar.low, bar.close, bar.volume);

        let tr = match prev_close {
            Some(pc) => (h - l).max((h - pc).abs()).max((l - pc).abs()),
            None => h - l,
        };
        let atr = self.atr.update(tr);
        self.cum_volume += v;
        let real = self.cum_volume > 0.0;
        let avg_vol = self.avg_vol.update(v);
        let rel_vol = match avg_vol {
            Some(a) if real && a > 0.0 => v / a,
            _ => 1.0,
        };
        let range = h - l;
        let body_ratio = if range > 0.0 {
            (c - o).abs() / range
        } else {
            0.0
        };
        let uwr = if range > 0.0 {
            (h - o.max(c)) / range
        } else {
            0.0
        };
        let lwr = if range > 0.0 {
            (o.min(c) - l) / range
        } else {
            0.0
        };
        let catr = match atr {
            Some(a) if a > 0.0 => range / a,
            _ => 0.0,
        };
        let ema20 = self.ema20.update(c).unwrap();
        let ema50 = self.ema50.update(c).unwrap();
        let rsi = prev_close.and_then(|pc| {
            let ch = c - pc;
            let g = self.gain.update(ch.max(0.0));
            let lo = self.loss.update((-ch).max(0.0));
            match (g, lo) {
                (Some(g), Some(lo)) => Some(if g == 0.0 && lo == 0.0 {
                    50.0
                } else if lo == 0.0 {
                    100.0
                } else {
                    100.0 - 100.0 / (1.0 + g / lo)
                }),
                _ => None,
            }
        });
        let hlc3 = (h + l + c) / 3.0;
        let mfi = self.prev_hlc3.and_then(|p| {
            let raw = hlc3 * v;
            self.flows.push_back(if hlc3 > p {
                (raw, 0.0)
            } else if hlc3 < p {
                (0.0, raw)
            } else {
                (0.0, 0.0)
            });
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
        let evr = if range > 0.0 { v / range } else { 0.0 };
        let evr_norm = match self.evr_avg.update(evr) {
            Some(a) if a > 0.0 => evr / a,
            _ => 1.0,
        };
        let approach_avg = self.approach_avg.update(range);
        self.rows.push_back(Row {
            open: o,
            high: h,
            low: l,
            close: c,
            volume: v,
            lwr,
            uwr,
            evr_norm,
        });
        if self.rows.len() > HISTORY {
            self.rows.pop_front();
        }
        let nz_evr = |k: usize| self.at(k).map_or(0.0, |r| r.evr_norm);
        let evr_falling = nz_evr(0) < nz_evr(1) && nz_evr(1) < nz_evr(2) && nz_evr(2) < nz_evr(3);
        let absorption = real && evr_falling && rel_vol > 1.2;
        let er = if t >= 10 {
            let path: f64 = (0..10)
                .map(|k| (self.at(k).unwrap().close - self.at(k + 1).unwrap().close).abs())
                .sum();
            let net = (c - self.at(10).unwrap().close).abs();
            if path > 0.0 {
                net / path
            } else {
                0.0
            }
        } else {
            0.0
        };
        let trend_up = ema20 > ema50 && er > 0.35;
        let trend_dn = ema20 < ema50 && er > 0.35;
        let is_range = er <= 0.35;

        // Swings, EQH/EQL, volume profile, BOS and swing trends.
        let ph = self.pivot(3, false);
        let pl = self.pivot(3, true);
        if let Some(p) = ph {
            self.swing_high = Some(p);
        }
        if let Some(p) = pl {
            self.swing_low = Some(p);
        }
        let mph = self.pivot(7, false);
        let mpl = self.pivot(7, true);
        if let Some(p) = mph {
            self.major_high = Some(p);
        }
        if let Some(p) = mpl {
            self.major_low = Some(p);
        }
        let mut eqh = None;
        let mut eql = None;
        if let Some(p) = ph {
            if let (Some(prev), Some(a)) = (self.prev_swing_high, atr) {
                if a > 0.0 && (p - prev).abs() < 0.1 * a {
                    eqh = Some((p + prev) / 2.0);
                }
            }
            self.prev_swing_high = Some(p);
        }
        if let Some(p) = pl {
            if let (Some(prev), Some(a)) = (self.prev_swing_low, atr) {
                if a > 0.0 && (p - prev).abs() < 0.1 * a {
                    eql = Some((p + prev) / 2.0);
                }
            }
            self.prev_swing_low = Some(p);
        }
        let new_msh = mph.filter(|p| self.prev_major_sh.map(|x| x.0) != Some(*p));
        let new_msl = mpl.filter(|p| self.prev_major_sl.map(|x| x.0) != Some(*p));
        if let Some(p) = new_msh {
            self.prev_major_sh = Some((p, t - 7));
        }
        if let Some(p) = new_msl {
            self.prev_major_sl = Some((p, t - 7));
        }
        if (new_msh.is_some() || new_msl.is_some()) && real {
            let leg = if new_msh.is_some() {
                self.prev_major_sl.map(|x| x.1)
            } else {
                self.prev_major_sh.map(|x| x.1)
            };
            let end = t - 7;
            if let Some(start) = leg {
                if end > start {
                    if let Some(p) = self.leg_profile(t, start, end) {
                        self.profile = Some(p);
                    }
                }
            }
        }
        let up_break =
            |lvl: Option<f64>| matches!((lvl, prev_close), (Some(x), Some(pc)) if c > x && pc <= x);
        let dn_break =
            |lvl: Option<f64>| matches!((lvl, prev_close), (Some(x), Some(pc)) if c < x && pc >= x);
        let micro_bull = up_break(self.swing_high);
        let micro_bear = dn_break(self.swing_low);
        let major_bull = up_break(self.major_high);
        let major_bear = dn_break(self.major_low);
        let bull_choch =
            (micro_bull && self.micro_trend == -1) || (major_bull && self.major_trend == -1);
        let bear_choch =
            (micro_bear && self.micro_trend == 1) || (major_bear && self.major_trend == 1);
        if micro_bull {
            self.micro_trend = 1;
        }
        if micro_bear {
            self.micro_trend = -1;
        }
        if major_bull {
            self.major_trend = 1;
        }
        if major_bear {
            self.major_trend = -1;
        }
        let (sh, sl, msh, msl) = (
            self.swing_high,
            self.swing_low,
            self.major_high,
            self.major_low,
        );
        let fake_up = matches!(sh, Some(x) if h > x && c <= x);
        let fake_dn = matches!(sl, Some(x) if l < x && c >= x);
        let mfake_up = matches!(msh, Some(x) if h > x && c <= x);
        let mfake_dn = matches!(msl, Some(x) if l < x && c >= x);
        let spring = fake_dn && lwr >= 0.35 && catr >= 0.5;
        let upthrust = fake_up && uwr >= 0.35 && catr >= 0.5;
        let mspring = mfake_dn && lwr >= 0.35 && catr >= 0.5;
        let mupthrust = mfake_up && uwr >= 0.35 && catr >= 0.5;
        let bull_rej = lwr >= 0.5 && body_ratio <= 0.3 && c > o;
        let bear_rej = uwr >= 0.5 && body_ratio <= 0.3 && c < o;
        let climax =
            catr >= 1.8 && (!real || matches!(avg_vol, Some(a) if a > 0.0 && v >= 1.8 * a));

        self.bars_since_test += 1;
        let mut test_candle = false;
        if let Some(a) = atr {
            // Climax sequence: stages may advance in the same bar.
            if climax {
                self.clx_state = 1;
                self.clx_dir = if c < o {
                    1
                } else if c > o {
                    -1
                } else {
                    0
                };
                self.clx = (c, h, l, range, v, t);
                self.clx_test_bar = None;
            }
            let (cc, ch, cl, cr, cv, cb) = self.clx;
            let active = self.clx_state >= 1 && t - cb <= 20;
            if active
                && self.clx_state == 1
                && ((self.clx_dir == 1 && c > cc) || (self.clx_dir == -1 && c < cc))
            {
                self.clx_state = 2;
            }
            if active && self.clx_state == 2 {
                let back = if self.clx_dir == 1 && a > 0.0 {
                    c < cc + 1.5 * a && c > cl - 0.5 * a
                } else if self.clx_dir == -1 && a > 0.0 {
                    c > cc - 1.5 * a && c < ch + 0.5 * a
                } else {
                    false
                };
                if back && t - cb >= 2 {
                    self.clx_state = 3;
                }
            }
            if active && self.clx_state == 3 {
                let test = range <= cr * 0.6 && range > 0.0 && (!real || v <= cv * 0.7);
                let in_zone = a > 0.0 && (c - cc).abs() <= 1.5 * a;
                if test && in_zone {
                    self.clx_state = 4;
                    self.clx_test_bar = Some(t);
                }
            }
            if active
                && self.clx_state == 4
                && ((self.clx_dir == 1 && (micro_bull || major_bull))
                    || (self.clx_dir == -1 && (micro_bear || major_bear)))
            {
                self.clx_state = 5;
            }
            test_candle =
                self.clx_state == 4 && self.clx_test_bar == Some(t) && self.bars_since_test >= 3;
            if test_candle {
                self.bars_since_test = 0;
            }

            // Fair value gaps.
            let min_gap = a * 0.3;
            if let Some(r2) = self.at(2).copied() {
                if l > r2.high && l - r2.high >= min_gap {
                    self.gaps.push(Gap {
                        upper: l,
                        lower: r2.high,
                        dir: 1,
                        created: t,
                    });
                    if self.gaps.len() > 10 {
                        self.gaps.remove(0);
                    }
                }
                if h < r2.low && r2.low - h >= min_gap {
                    self.gaps.push(Gap {
                        upper: r2.low,
                        lower: h,
                        dir: -1,
                        created: t,
                    });
                    if self.gaps.len() > 10 {
                        self.gaps.remove(0);
                    }
                }
            }
        }
        let mut gap_near = (false, false);
        if let Some(a) = atr {
            let mut i = self.gaps.len();
            while i > 0 {
                i -= 1;
                let g = self.gaps[i];
                let filled = (g.dir == 1 && l <= g.lower) || (g.dir == -1 && h >= g.upper);
                if t - g.created > 20 || filled {
                    self.gaps.remove(i);
                    continue;
                }
                if g.dir == 1 && c > g.lower && c < g.upper + a {
                    gap_near.0 = true;
                }
                if g.dir == -1 && c < g.upper && c > g.lower - a {
                    gap_near.1 = true;
                }
            }

            // Order blocks at a BOS.
            for (bull, bos) in [
                (true, micro_bull || major_bull),
                (false, micro_bear || major_bear),
            ] {
                if !bos {
                    continue;
                }
                let found = (1..=10).find(|&k| {
                    self.at(k).is_some_and(|r| {
                        if bull {
                            r.close < r.open
                        } else {
                            r.close > r.open
                        }
                    })
                });
                if let Some(k) = found {
                    let r = *self.at(k).unwrap();
                    let overlap = self.gaps.iter().any(|g| {
                        g.dir == if bull { 1 } else { -1 } && g.upper >= r.low && g.lower <= r.high
                    });
                    let score = if catr >= 1.5 { 30.0 } else { 0.0 }
                        + 25.0
                        + if overlap { 20.0 } else { 0.0 }
                        + if rel_vol >= 1.3 { 15.0 } else { 0.0 };
                    let list = if bull {
                        &mut self.bull_blocks
                    } else {
                        &mut self.bear_blocks
                    };
                    list.push(Block {
                        upper: r.high,
                        lower: r.low,
                        mitigated: false,
                        score,
                    });
                    if list.len() > 5 {
                        list.remove(0);
                    }
                }
            }
        }
        let mut block_near = [(false, 0.0f64); 2];
        if let Some(a) = atr {
            for (side, list) in [&mut self.bull_blocks, &mut self.bear_blocks]
                .into_iter()
                .enumerate()
            {
                for b in list.iter_mut().rev() {
                    if b.mitigated {
                        continue;
                    }
                    if l <= b.upper && h >= b.lower {
                        b.mitigated = true;
                        continue;
                    }
                    let dist = if a > 0.0 {
                        (c - (b.upper + b.lower) / 2.0).abs() / a
                    } else {
                        999.0
                    };
                    if dist < 2.0 {
                        block_near[side].0 = true;
                        block_near[side].1 = block_near[side].1.max(b.score);
                    }
                }
            }

            // Zone creation.
            let half = a * ZONE_WIDTH_ATR / 2.0;
            if let Some(p) = mph {
                self.add_zone(t, 2, p + half, p - half, -1, 2);
            }
            if let Some(p) = mpl {
                self.add_zone(t, 1, p + half, p - half, 1, 2);
            }
            let abs_spring = absorption && (spring || mspring);
            let abs_ut = absorption && (upthrust || mupthrust);
            if let (true, Some(s)) = (mspring, msl) {
                let q = if lwr >= 0.55 && catr >= 0.8 { 3 } else { 2 };
                let q = if abs_spring { (q + 1).min(3) } else { q };
                self.add_zone(t, 3, l.max(s), l.min(s), 1, q);
            } else if let (true, Some(s)) = (spring, sl) {
                let q = if lwr >= 0.55 && catr >= 0.8 {
                    3
                } else if lwr >= 0.35 {
                    2
                } else {
                    1
                };
                let q = if abs_spring { (q + 1).min(3) } else { q };
                self.add_zone(t, 3, l.max(s), l.min(s), 1, q);
            }
            if let (true, Some(s)) = (mupthrust, msh) {
                let q = if uwr >= 0.55 && catr >= 0.8 { 3 } else { 2 };
                let q = if abs_ut { (q + 1).min(3) } else { q };
                self.add_zone(t, 4, h.max(s), h.min(s), -1, q);
            } else if let (true, Some(s)) = (upthrust, sh) {
                let q = if uwr >= 0.55 && catr >= 0.8 {
                    3
                } else if uwr >= 0.35 {
                    2
                } else {
                    1
                };
                let q = if abs_ut { (q + 1).min(3) } else { q };
                self.add_zone(t, 4, h.max(s), h.min(s), -1, q);
            }
            if major_bull {
                let s = msh.unwrap();
                self.add_zone(t, 5, s + half, s - half, 1, 2);
            }
            if major_bear {
                let s = msl.unwrap();
                self.add_zone(t, 6, s + half, s - half, -1, 2);
            }
            if climax {
                let dir = if c < o { 1 } else { -1 };
                let q = if rel_vol >= 1.8 * 1.5 { 3 } else { 2 };
                self.add_zone(t, if dir == 1 { 7 } else { 8 }, h, l, dir, q);
            }

            // Zone lifecycle.
            let (ph1, pl1) = (prev.map(|p| p.high), prev.map(|p| p.low));
            let mut i = self.zones.len();
            while i > 0 {
                i -= 1;
                let age = t - self.zones[i].created;
                if age > ZONE_EXPIRE {
                    self.zones.remove(i);
                    continue;
                }
                if age == 0 {
                    continue;
                }
                let z = &mut self.zones[i];
                let in_zone = h >= z.lower && l <= z.upper;
                let above = c > z.upper;
                let below = c < z.lower;
                let prev_in = h_in(ph1, pl1, z.lower, z.upper);
                if in_zone && !prev_in {
                    z.touches += 1;
                    z.visit_bounced = false;
                    if z.status == 0 {
                        z.status = 1;
                    }
                }
                let can_bounce = (z.status == 1 || z.status == 2) && in_zone && !z.visit_bounced;
                if can_bounce && z.dir == 1 && c > z.mid && c > o {
                    z.status = 2;
                    z.visit_bounced = true;
                    let bounce = if a > 0.0 { (c - z.lower) / a } else { 0.0 };
                    z.last_bounce = bounce;
                    z.max_bounce = z.max_bounce.max(bounce);
                    if bounce >= 0.5 {
                        z.good += 1;
                    } else {
                        z.weak += 1;
                    }
                }
                if can_bounce && z.dir == -1 && c < z.mid && c < o {
                    z.status = 2;
                    z.visit_bounced = true;
                    let bounce = if a > 0.0 { (z.upper - c) / a } else { 0.0 };
                    z.last_bounce = bounce;
                    z.max_bounce = z.max_bounce.max(bounce);
                    if bounce >= 0.5 {
                        z.good += 1;
                    } else {
                        z.weak += 1;
                    }
                }
                if (z.dir == 1 && in_zone && c < z.mid) || (z.dir == -1 && in_zone && c > z.mid) {
                    z.deep += 1;
                }
                let pc = prev_close.unwrap();
                let mut broke = false;
                if z.dir == 1 && below && pc >= z.lower {
                    z.status = 3;
                    broke = true;
                }
                if z.dir == -1 && above && pc <= z.upper {
                    z.status = 3;
                    broke = true;
                }
                if z.status == 3 && !broke && z.dir == 1 && in_zone && c < z.mid {
                    z.status = 4;
                    z.dir = -1;
                } else if z.status == 3 && !broke && z.dir == -1 && in_zone && c > z.mid {
                    z.status = 4;
                    z.dir = 1;
                }
            }
        }

        let mut code = 0u32;
        for (hit, bit) in [
            (ph.is_some(), 1),
            (pl.is_some(), 2),
            (micro_bull, 4),
            (micro_bear, 8),
            (major_bull, 16),
            (major_bear, 32),
            (spring, 64),
            (upthrust, 128),
            (mspring, 256),
            (mupthrust, 512),
            (climax, 1024),
            (test_candle, 2048),
            (bull_rej, 4096),
            (bear_rej, 8192),
            (eqh.is_some(), 16384),
            (eql.is_some(), 32768),
            (bull_choch, 65536),
            (bear_choch, 131072),
        ] {
            if hit {
                code += bit;
            }
        }

        // Output gate.
        let (Some(a), Some(rsi), Some(approach_avg)) = (atr, rsi, approach_avg) else {
            return None;
        };
        let (mfi, avg_vol) = if real {
            (Some(mfi?), Some(avg_vol?))
        } else {
            (None, None)
        };

        // Assessment of the nearest support and resistance.
        let mut near: [Option<(usize, f64)>; 2] = [None, None];
        for (i, z) in self.zones.iter().enumerate() {
            if z.status == 3 {
                continue;
            }
            let dist = if a > 0.0 {
                (c - z.mid).abs() / a
            } else {
                999.0
            };
            if dist > MAX_ASSESS_ATR {
                continue;
            }
            if z.dir == 1 && z.mid < c && near[0].is_none_or(|n| dist < n.1) {
                near[0] = Some((i, dist));
            }
            if z.dir == -1 && z.mid > c && near[1].is_none_or(|n| dist < n.1) {
                near[1] = Some((i, dist));
            }
        }
        let prev_lwr = prev.map_or(0.0, |p| p.lwr);
        let prev_uwr = prev.map_or(0.0, |p| p.uwr);
        let c1 = self.at(1).map(|r| r.close);
        let c2 = self.at(2).map(|r| r.close);
        let mut extra = HashMap::new();
        let mut flags = [false; 6];
        for (side, name) in ["support", "resistance"].iter().enumerate() {
            let Some((idx, dist)) = near[side] else {
                continue;
            };
            let z = &self.zones[idx];
            let up = z.dir == 1;
            let age = t - z.created;
            let mut s: f64 = match z.kind {
                3 | 4 => 30.0,
                5..=8 => 25.0,
                _ => 20.0,
            };
            s += match z.quality {
                3 => 20.0,
                2 => 10.0,
                _ => 4.0,
            };
            s += match z.status {
                2 => 20.0,
                1 => 10.0,
                0 => 6.0,
                _ => -25.0,
            };
            s += if age < 30 {
                10.0
            } else if age < 80 {
                5.0
            } else if age > 150 {
                -5.0
            } else {
                0.0
            };
            let structural = clamp(s, 0.0, 100.0);
            let mut r: f64 = 30.0;
            r += (z.good as f64 * 12.0).min(30.0);
            r += if z.max_bounce >= 1.0 {
                15.0
            } else if z.max_bounce >= 0.5 {
                8.0
            } else {
                0.0
            };
            r -= (z.deep as f64 * 5.0).min(20.0);
            if z.weak > z.good {
                r -= 10.0;
            }
            if z.touches > 3 {
                r -= (z.touches - 3) as f64 * 5.0;
            }
            if z.good >= 2 && z.last_bounce < z.max_bounce * 0.5 {
                r -= 10.0;
            }
            let respect = clamp(r, 0.0, 100.0);
            let mut aq: f64 = 0.0;
            let contracting = range < approach_avg * 0.8;
            let expanding = range > approach_avg * 1.2;
            let vol_low = matches!(avg_vol, Some(av) if v < av * 0.9);
            let vol_high = matches!(avg_vol, Some(av) if v > av * 1.3);
            if up {
                aq += if contracting { 15.0 } else { 0.0 };
                aq += if vol_low && matches!(c1, Some(x) if c < x) {
                    10.0
                } else {
                    0.0
                };
                aq += if lwr > 0.3 && lwr > prev_lwr {
                    10.0
                } else {
                    0.0
                };
                aq += if rsi < 35.0 { 10.0 } else { 0.0 };
                aq += if matches!(mfi, Some(m) if m < 30.0) {
                    8.0
                } else {
                    0.0
                };
                aq -= if expanding && c < o { 15.0 } else { 0.0 };
                aq -= if vol_high && c < o { 12.0 } else { 0.0 };
                aq -= if major_bear { 18.0 } else { 0.0 };
            } else {
                aq += if contracting { 15.0 } else { 0.0 };
                aq += if vol_low && matches!(c1, Some(x) if c > x) {
                    10.0
                } else {
                    0.0
                };
                aq += if uwr > 0.3 && uwr > prev_uwr {
                    10.0
                } else {
                    0.0
                };
                aq += if rsi > 65.0 { 10.0 } else { 0.0 };
                aq += if matches!(mfi, Some(m) if m > 70.0) {
                    8.0
                } else {
                    0.0
                };
                aq -= if expanding && c > o { 15.0 } else { 0.0 };
                aq -= if vol_high && c > o { 12.0 } else { 0.0 };
                aq -= if major_bull { 18.0 } else { 0.0 };
            }
            let approach = clamp(aq, -100.0, 100.0);
            let mut ca: f64 = if is_range {
                15.0
            } else if (up && trend_dn) || (!up && trend_up) {
                -20.0
            } else {
                5.0
            };
            if er < 0.25 {
                ca += 10.0;
            }
            let context = clamp(ca, -100.0, 100.0);
            let mut reach: f64 = if dist <= 0.5 {
                75.0
            } else if dist <= 1.0 {
                60.0
            } else if dist <= 2.0 {
                40.0
            } else {
                20.0
            };
            if let (Some(x1), Some(x2)) = (c1, c2) {
                let falling = c < x1 && x1 < x2;
                let rising = c > x1 && x1 > x2;
                if (up && falling) || (!up && rising) {
                    reach += 15.0;
                }
                if (up && rising) || (!up && falling) {
                    reach -= 15.0;
                }
            }
            let reach = clamp(reach, 0.0, 100.0);
            let mut hold = clamp(
                0.30 * structural
                    + 0.25 * respect
                    + 0.30 * (approach + 100.0) / 2.0
                    + 0.15 * (context + 100.0) / 2.0,
                0.0,
                100.0,
            );
            let (g_near, (b_near, b_score)) = if side == 0 {
                (gap_near.0, block_near[0])
            } else {
                (gap_near.1, block_near[1])
            };
            if g_near {
                hold = (hold + 5.0).min(100.0);
            }
            if b_near {
                hold = (hold + b_score * 0.1).min(100.0);
            }
            match self.profile {
                Some((poc, _, _)) if a > 0.0 && (z.mid - poc).abs() / a < 1.0 => {
                    hold = (hold + 7.0).min(100.0);
                }
                Some((_, va_hi, va_lo)) if z.mid >= va_lo && z.mid <= va_hi => {
                    hold = (hold + 3.0).min(100.0);
                }
                _ => {}
            }
            let mut brk = 100.0 - hold;
            if approach < -30.0 {
                brk += 10.0;
            }
            if z.touches > 3 {
                brk += z.touches as f64 * 3.0;
            }
            if z.weak > z.good {
                brk += 10.0;
            }
            if z.good >= 2 && z.last_bounce < z.max_bounce * 0.4 {
                brk += 12.0;
            }
            let brk = clamp(brk, 0.0, 100.0);
            let state = if brk >= 65.0 && approach < -20.0 {
                5.0
            } else if hold >= 65.0 {
                4.0
            } else if hold >= 50.0 && brk >= 50.0 {
                3.0
            } else if z.touches > 3 && z.last_bounce < z.max_bounce * 0.5 {
                2.0
            } else if hold >= 50.0 {
                1.0
            } else {
                0.0
            };
            for (k, val) in [
                ("mid", z.mid),
                ("upper", z.upper),
                ("lower", z.lower),
                ("type", z.kind as f64),
                ("status", z.status as f64),
                ("distance_atr", dist),
                ("reach", reach),
                ("hold", hold),
                ("break", brk),
                ("structural", structural),
                ("respect", respect),
                ("approach", approach),
                ("context", context),
                ("state", state),
            ] {
                extra.insert(format!("{name}_{k}"), val);
            }
            let close_by = dist <= 1.0;
            flags[side] = close_by && reach >= 60.0;
            flags[2 + side] = close_by && hold >= 60.0 && brk < 45.0;
            flags[4 + side] = close_by && brk >= 60.0 && hold < 45.0;
        }
        let kinds = [
            "support_relevant",
            "resistance_relevant",
            "bull_support_hold",
            "bear_resistance_hold",
            "bear_support_break",
            "bull_resistance_break",
        ];
        for (i, kind) in kinds.iter().enumerate() {
            if flags[i] && !self.prev_flags[i] {
                self.alerts.push(IndicatorAlert::new(
                    *kind,
                    format!("Structure Zones: {kind}"),
                    0.5,
                ));
            }
        }
        self.prev_flags = flags;

        extra.insert("micro_trend".to_string(), self.micro_trend as f64);
        extra.insert("major_trend".to_string(), self.major_trend as f64);
        extra.insert("clx_state".to_string(), self.clx_state as f64);
        extra.insert("clx_direction".to_string(), self.clx_dir as f64);
        extra.insert("event_code".to_string(), code as f64);
        if let Some((poc, va_hi, va_lo)) = self.profile {
            extra.insert("poc".to_string(), poc);
            extra.insert("va_high".to_string(), va_hi);
            extra.insert("va_low".to_string(), va_lo);
        }
        Some(IndicatorOutput::with_extra(self.zones.len() as f64, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Whether the previous bar's range overlapped `lower..=upper`.
fn h_in(high: Option<f64>, low: Option<f64>, lower: f64, upper: f64) -> bool {
    matches!((high, low), (Some(h), Some(l)) if h >= lower && l <= upper)
}

pub const STRUCTURE_ZONES_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "structure_zones",
        aliases: &[],
        description: "Structure Zones (default parameters): micro 3/3 and major 7/7 swing events (BOS/CHoCH, spring/upthrust, fake breaks, climax-test sequence, rejections, EQH/EQL as event_code), automatic support/resistance zones (major swings, springs/upthrusts, BOS flips, climax bars) with lifecycle fresh/tested/confirmed/broken/flipped, and an assessment of the nearest support and resistance within 3 ATR (reach, hold, break 0..100 from structure, respect, approach, context, plus nearby fair value gap, order block and volume-profile POC/value area); value = number of tracked zones; extra: support_*/resistance_* (mid, upper, lower, type, status, distance_atr, reach, hold, break, structural, respect, approach, context, state), micro_trend, major_trend, clx_state, clx_direction, event_code, poc, va_high, va_low; alerts support/resistance_relevant, bull_support_hold, bear_resistance_hold, bear_support_break, bull_resistance_break",
        params: &[

        ],
        checks: &[],
        build: |_p| {
            Ok(Box::new(StructureZones::new()))
        },
    };
