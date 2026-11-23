//! Instrument state heatmap: ten normalised state measures per bar, in five functional groups
//! of two complementary measures each.
//!
//! Every track is a **description of the state of the window that is over**, not a signal. Each
//! value lies in `[-1, +1]`: `+1` is the favourable pole of the measure (order, tailwind,
//! expansion), `0` is neutral, `-1` the adverse pole (noise, headwind, exhaustion). `NaN` means
//! "not evaluated": the warm-up is not over, or the input the measure needs (real volume) is
//! absent. Nothing is substituted for a missing input.
//!
//! | Group | Field | Raw measure | `+1` | `-1` |
//! |---|---|---|---|---|
//! | direction | `dir_hma` | relative one-bar change of `HMA(220)` | rising | falling |
//! | direction | `dir_kas` | `(close - KAS) / ATR(14)` of the adaptive smoother | above | below |
//! | order | `ord_adx` | `ADX(14, 14)` | strong trend | no trend |
//! | order | `ord_fdi` | fractal dimension (halves, 30) | straight path | noise |
//! | volatility | `vol_docht` | wick share of the bar range, mean of 14 | small wicks | large wicks |
//! | volatility | `vol_squeeze` | Bollinger width / Keltner width (20, 2, 1.5) | expanded | compressed |
//! | flow | `flow_cmf` | `CMF(20)` | accumulation | distribution |
//! | flow | `flow_rvat` | volume / mean volume of the 20 bars before | busy | quiet |
//! | momentum | `mom_schub` | `(close - 2 close[5 ago] + close[10 ago]) / ATR(14)` | accelerating up | accelerating down |
//! | momentum | `mom_dehnung` | `abs((close - SMA50) / stdev50)` | near the mean | stretched |
//!
//! # Normalisation
//!
//! No fixed thresholds. Each raw measure is ranked against its own last
//! [`HEATMAP_WINDOW`](crate::heatmap::HEATMAP_WINDOW) values (the current one included; ties count half), so the scale follows
//! the instrument's own volatility. Unipolar measures map the percentile `p` to `2p - 1`
//! (inverted where the high raw value is the adverse pole). Signed measures keep their sign and
//! use the percentile of the absolute value: `sign(x) * p(|x|)`, so a raw zero stays neutral.
//! A track needs at least [`HEATMAP_MIN_RANKED`](crate::heatmap::HEATMAP_MIN_RANKED) ranked values before it reports.
//!
//! Only bars up to and including the row's bar are used.

use crate::indicator::adx::Adx;
use crate::indicator::fractal_dimension::FractalDimensionEngine;
use crate::indicator::kas::Kas;
use crate::indicator::moving_averages::HmaEngine;
use crate::indicator::volume_indicators::CmfEngine;
use crate::indicator::Indicator;
use crate::model::Bar;
use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Length of the rolling percentile window.
pub const HEATMAP_WINDOW: usize = 200;
/// Minimum number of finite values in the window before a track reports.
pub const HEATMAP_MIN_RANKED: usize = 50;

const HMA_PERIOD: usize = 220;
const ATR_PERIOD: usize = 14;
const WICK_PERIOD: usize = 14;
const SQUEEZE_PERIOD: usize = 20;
const BB_MULT: f64 = 2.0;
const KC_MULT: f64 = 1.5;
const CMF_PERIOD: usize = 20;
const RVAT_PERIOD: usize = 20;
const STRETCH_PERIOD: usize = 50;
const THRUST_PERIOD: usize = 10;

/// The ten state values of one bar, each in `[-1, +1]` or `NaN` (not evaluated).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct InstrumentHeatmapRow {
    pub ts: i64,
    pub dir_hma: f32,
    pub dir_kas: f32,
    pub ord_adx: f32,
    pub ord_fdi: f32,
    pub vol_docht: f32,
    pub vol_squeeze: f32,
    pub flow_cmf: f32,
    pub flow_rvat: f32,
    pub mom_schub: f32,
    pub mom_dehnung: f32,
}

impl InstrumentHeatmapRow {
    /// The ten values in field order (`dir_hma` first).
    pub fn values(&self) -> [f32; 10] {
        [
            self.dir_hma,
            self.dir_kas,
            self.ord_adx,
            self.ord_fdi,
            self.vol_docht,
            self.vol_squeeze,
            self.flow_cmf,
            self.flow_rvat,
            self.mom_schub,
            self.mom_dehnung,
        ]
    }
}

/// Track names in field order, matching [`InstrumentHeatmapRow::values`].
pub const HEATMAP_TRACKS: [&str; 10] = [
    "dir_hma",
    "dir_kas",
    "ord_adx",
    "ord_fdi",
    "vol_docht",
    "vol_squeeze",
    "flow_cmf",
    "flow_rvat",
    "mom_schub",
    "mom_dehnung",
];

/// Rolling percentile ranker over the last [`HEATMAP_WINDOW`] raw values.
struct Ranker {
    window: VecDeque<f64>,
}

impl Ranker {
    fn new() -> Self {
        Self {
            window: VecDeque::with_capacity(HEATMAP_WINDOW),
        }
    }

    /// Pushes `x` (NaN allowed, ranks to NaN) and returns its percentile in `(0, 1)` against
    /// the window including itself, or `None` while fewer than [`HEATMAP_MIN_RANKED`] finite
    /// values are present or `x` is not finite.
    fn push(&mut self, x: f64) -> Option<f64> {
        if self.window.len() == HEATMAP_WINDOW {
            self.window.pop_front();
        }
        self.window.push_back(x);
        if !x.is_finite() {
            return None;
        }
        let (mut n, mut less, mut equal) = (0usize, 0usize, 0usize);
        for &v in self.window.iter().filter(|v| v.is_finite()) {
            n += 1;
            if v < x {
                less += 1;
            } else if v == x {
                equal += 1;
            }
        }
        if n < HEATMAP_MIN_RANKED {
            return None;
        }
        Some((less as f64 + 0.5 * equal as f64) / n as f64)
    }
}

/// Unipolar track: percentile mapped to `[-1, 1]`, optionally inverted.
struct Unipolar {
    ranker: Ranker,
    invert: bool,
}

impl Unipolar {
    fn new(invert: bool) -> Self {
        Self {
            ranker: Ranker::new(),
            invert,
        }
    }

    fn push(&mut self, x: f64) -> f32 {
        match self.ranker.push(x) {
            Some(p) => {
                let v = 2.0 * p - 1.0;
                (if self.invert { -v } else { v }) as f32
            }
            None => f32::NAN,
        }
    }
}

/// Signed track: `sign(x) * percentile(|x|)`.
struct Signed {
    ranker: Ranker,
}

impl Signed {
    fn new() -> Self {
        Self {
            ranker: Ranker::new(),
        }
    }

    fn push(&mut self, x: f64) -> f32 {
        match self.ranker.push(x.abs()) {
            Some(p) => (if x < 0.0 { -p } else { p }) as f32,
            None => f32::NAN,
        }
    }
}

fn true_range(bar: &Bar, prev_close: Option<f64>) -> f64 {
    let hl = bar.high - bar.low;
    match prev_close {
        Some(pc) => hl.max((bar.high - pc).abs()).max((bar.low - pc).abs()),
        None => hl,
    }
}

fn mean(w: &VecDeque<f64>) -> f64 {
    w.iter().sum::<f64>() / w.len() as f64
}

fn push_capped(w: &mut VecDeque<f64>, x: f64, cap: usize) {
    if w.len() == cap {
        w.pop_front();
    }
    w.push_back(x);
}

fn std_dev(w: &VecDeque<f64>) -> f64 {
    let m = mean(w);
    (w.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / w.len() as f64).sqrt()
}

/// Computes the ten-track state heatmap for every bar, oldest first; one row per input bar.
///
/// Bars must be in time order. Tracks that need volume (`flow_cmf`, `flow_rvat`) are `NaN` for
/// every bar whose volume is not a positive finite number, and while their window holds too few
/// such bars.
pub fn evaluate_instrument_heatmap(bars: &[Bar]) -> Vec<InstrumentHeatmapRow> {
    let mut hma = HmaEngine::new(HMA_PERIOD);
    let mut kas = Kas::with_defaults();
    let mut adx = Adx::new(14, 14, 3, 20.0);
    let mut fdi = FractalDimensionEngine::new(30, false);
    let mut cmf = CmfEngine::new(CMF_PERIOD);

    let mut t_hma = Signed::new();
    let mut t_kas = Signed::new();
    let mut t_adx = Unipolar::new(false);
    let mut t_fdi = Unipolar::new(true);
    let mut t_docht = Unipolar::new(true);
    let mut t_squeeze = Unipolar::new(false);
    let mut t_cmf = Signed::new();
    let mut t_rvat = Unipolar::new(false);
    let mut t_schub = Signed::new();
    let mut t_dehnung = Unipolar::new(true);

    let mut prev_close: Option<f64> = None;
    let mut prev_hma: Option<f64> = None;
    let mut atr: Option<f64> = None;
    let mut wick: VecDeque<f64> = VecDeque::new();
    let mut closes_sq: VecDeque<f64> = VecDeque::new();
    let mut ranges_sq: VecDeque<f64> = VecDeque::new();
    let mut closes_st: VecDeque<f64> = VecDeque::new();
    let mut closes_sh: VecDeque<f64> = VecDeque::new();
    let mut volumes: VecDeque<f64> = VecDeque::new();

    let mut rows = Vec::with_capacity(bars.len());
    for bar in bars {
        let tr = true_range(bar, prev_close);
        atr = Some(match atr {
            None => tr,
            Some(a) => a + (tr - a) / ATR_PERIOD as f64,
        });
        prev_close = Some(bar.close);

        // direction: HMA relative slope
        let hma_now = hma.on_bar(bar).map(|o| o.value);
        let slope = match (hma_now, prev_hma) {
            (Some(now), Some(prev)) if prev > 0.0 => (now - prev) / prev,
            _ => f64::NAN,
        };
        if hma_now.is_some() {
            prev_hma = hma_now;
        }
        let dir_hma = t_hma.push(slope);

        // direction: close against the adaptive smoother, in ATR
        let kas_now = kas.update(bar.close);
        let kas_dist = match atr {
            Some(a) if a > 0.0 => (bar.close - kas_now) / a,
            _ => f64::NAN,
        };
        let dir_kas = t_kas.push(kas_dist);

        // order
        let adx_v = adx.on_bar(bar).map_or(f64::NAN, |o| o.value);
        let ord_adx = t_adx.push(adx_v);
        let fdi_v = fdi.on_bar(bar).map_or(f64::NAN, |o| o.value);
        let ord_fdi = t_fdi.push(fdi_v);

        // volatility: wick share of the range
        let range = bar.high - bar.low;
        let share = if range > 0.0 {
            1.0 - (bar.close - bar.open).abs() / range
        } else {
            0.0
        };
        push_capped(&mut wick, share, WICK_PERIOD);
        let wick_v = if wick.len() == WICK_PERIOD {
            mean(&wick)
        } else {
            f64::NAN
        };
        let vol_docht = t_docht.push(wick_v);

        // volatility: Bollinger width against Keltner width
        push_capped(&mut closes_sq, bar.close, SQUEEZE_PERIOD);
        push_capped(&mut ranges_sq, tr, SQUEEZE_PERIOD);
        let squeeze_v = if closes_sq.len() == SQUEEZE_PERIOD {
            let keltner = KC_MULT * mean(&ranges_sq);
            if keltner > 0.0 {
                BB_MULT * std_dev(&closes_sq) / keltner
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        };
        let vol_squeeze = t_squeeze.push(squeeze_v);

        // flow: only with real volume
        let has_volume = bar.volume.is_finite() && bar.volume > 0.0;
        let flow_bar = if has_volume {
            bar.clone()
        } else {
            Bar {
                volume: f64::NAN,
                ..bar.clone()
            }
        };
        let cmf_v = cmf.on_bar(&flow_bar).map_or(f64::NAN, |o| o.value);
        let flow_cmf = t_cmf.push(cmf_v);
        let rvat_v = if has_volume && volumes.len() == RVAT_PERIOD {
            let base = mean(&volumes);
            if base > 0.0 {
                bar.volume / base
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        };
        if has_volume {
            push_capped(&mut volumes, bar.volume, RVAT_PERIOD);
        }
        let flow_rvat = t_rvat.push(rvat_v);

        // momentum
        push_capped(&mut closes_sh, bar.close, THRUST_PERIOD + 1);
        let schub_v = match atr {
            Some(a) if a > 0.0 && closes_sh.len() == THRUST_PERIOD + 1 => {
                (bar.close - 2.0 * closes_sh[THRUST_PERIOD / 2] + closes_sh[0]) / a
            }
            _ => f64::NAN,
        };
        let mom_schub = t_schub.push(schub_v);
        push_capped(&mut closes_st, bar.close, STRETCH_PERIOD);
        let stretch_v = if closes_st.len() == STRETCH_PERIOD {
            let sd = std_dev(&closes_st);
            if sd > 0.0 {
                ((bar.close - mean(&closes_st)) / sd).abs()
            } else {
                f64::NAN
            }
        } else {
            f64::NAN
        };
        let mom_dehnung = t_dehnung.push(stretch_v);

        rows.push(InstrumentHeatmapRow {
            ts: bar.timestamp,
            dir_hma,
            dir_kas,
            ord_adx,
            ord_fdi,
            vol_docht,
            vol_squeeze,
            flow_cmf,
            flow_rvat,
            mom_schub,
            mom_dehnung,
        });
    }
    rows
}

/// Aggregated state of one bar: direction times order quality, with friction as diagnosis only.
///
/// A description of the closed window, not a signal. `ksi`, `dir`, `order_gate` and
/// `friction_score` are `NaN` when an input is not evaluated; `regime` is then `"NA"`.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct InstrumentStateIndex {
    pub ts: i64,
    /// `100 * dir * order_gate`, in `[-100, +100]`.
    pub ksi: f32,
    /// Direction vector in `[-1, +1]`.
    pub dir: f32,
    /// Order-quality gate in `[0.15, 1.0]`.
    pub order_gate: f32,
    /// Friction and stretch in `[0, 1]`; not part of `ksi`.
    pub friction_score: f32,
    /// `"EXP_BULL"`, `"DRIFT_BULL"`, `"CHOP"`, `"DRIFT_BEAR"`, `"EXP_BEAR"`, or `"NA"`.
    pub regime: &'static str,
    /// Stretch track in its lowest fifth.
    pub warn_overextended: bool,
    /// Wick track in its lowest fifth.
    pub warn_rejection: bool,
}

/// Collapses one heatmap row into the state index.
///
/// `dir` = `0.35 hma + 0.35 kas + 0.15 schub + 0.15 cmf`; without volume (`flow_cmf` is `NaN`)
/// `0.40 hma + 0.40 kas + 0.20 schub`. `order_gate` = `0.15 + 0.85 * (a + f) / 2` with `a`, `f`
/// the ADX and FDI tracks mapped to `[0, 1]`. Stretch and wick tracks only raise warning flags,
/// so a strong, stretched trend keeps its score.
pub fn compute_state_index(row: &InstrumentHeatmapRow) -> InstrumentStateIndex {
    let dir = if row.flow_cmf.is_nan() {
        0.40 * row.dir_hma + 0.40 * row.dir_kas + 0.20 * row.mom_schub
    } else {
        0.35 * row.dir_hma + 0.35 * row.dir_kas + 0.15 * row.mom_schub + 0.15 * row.flow_cmf
    };
    let a = (row.ord_adx + 1.0) / 2.0;
    let f = (row.ord_fdi + 1.0) / 2.0;
    let order_gate = 0.15 + 0.85 * (0.5 * a + 0.5 * f);
    let w = (row.vol_docht + 1.0) / 2.0;
    let s = (row.mom_dehnung + 1.0) / 2.0;
    let friction_score = 0.5 * w + 0.5 * s;
    let ksi = (100.0 * dir * order_gate).clamp(-100.0, 100.0);
    let regime = if ksi.is_nan() {
        "NA"
    } else if ksi >= 60.0 {
        "EXP_BULL"
    } else if ksi >= 20.0 {
        "DRIFT_BULL"
    } else if ksi <= -60.0 {
        "EXP_BEAR"
    } else if ksi <= -20.0 {
        "DRIFT_BEAR"
    } else {
        "CHOP"
    };
    InstrumentStateIndex {
        ts: row.ts,
        ksi,
        dir: dir.clamp(-1.0, 1.0),
        order_gate,
        friction_score,
        regime,
        warn_overextended: s < 0.20,
        warn_rejection: w < 0.20,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ranker_matches_hand_computed_percentiles() {
        let mut r = Ranker::new();
        // below the minimum count nothing is reported
        for x in 1..HEATMAP_MIN_RANKED {
            assert!(r.push(x as f64).is_none());
        }
        // 1..=50: the 50th value has 49 below and itself equal: (49 + 0.5) / 50 = 0.99
        assert_eq!(r.push(50.0), Some(0.99));
        // a tie with the maximum: 49 below, 2 equal, n = 51: (49 + 1) / 51
        let p = r.push(50.0).unwrap();
        assert!((p - 50.0 / 51.0).abs() < 1e-12);
        // NaN does not rank and does not count
        assert!(r.push(f64::NAN).is_none());
    }

    #[test]
    fn signed_track_keeps_sign_and_unipolar_inverts() {
        let mut s = Signed::new();
        let mut u = Unipolar::new(true);
        for x in 1..=HEATMAP_MIN_RANKED {
            s.push(x as f64);
            u.push(x as f64);
        }
        assert!(s.push(-100.0) < -0.9);
        assert!(u.push(100.0) < -0.9);
    }
}
