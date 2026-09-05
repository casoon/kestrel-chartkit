//! Money Flow Profile: a rolling-window Volume-by-Price profile that bins by **dollar volume**
//! (`volume × row-mid-price`) instead of raw volume, plus an aggregate bull%/bear% flow-bias
//! scalar with debounced (50%-crossing) bias-flip alerts and distance-scaled value-area breakout
//! alerts.
//!
//! Chartkit's existing [`super::volume_profile`]/[`super::volume_profile_extended`]/
//! [`super::volume_profile_persistent`] family bins by raw `bar.volume` and is, taken together,
//! more feature-rich overall (HVN/LVN zones, absorption, per-bin delta). What none of them do is
//! dollar-volume weighting — for a wide-range instrument this genuinely shifts where POC/VAH/VAL
//! land, since a bar with modest volume at a high price level can outweigh a bar with heavy
//! volume at a low price level once weighted by price. Kept as a separate engine rather than a
//! mode flag on the existing family, matching the precedent of that family already being three
//! separate engines rather than one engine with every mode as a flag.
//!
//! This is a **windowed recompute**, not an incremental running average: row boundaries are
//! re-derived from the window's current high/low every bar (there is no meaningful "add one, drop
//! one" incremental step once bin edges themselves move), same as
//! [`super::volume_profile::VolumeProfileEngine`].
//!
//! Ported from the sibling `kestrel` repo's `crates/core/src/indicators/money_flow_profile.rs`
//! (itself a scoped port of `indicators/money_flow/money_flow_delta_profile/
//! money_flow_delta_profile.pine` v2.0 — the numeric substance only: POC, Value Area High/Low,
//! Delta POC and the overall bull% flow bias; drawing, HVN/LVN zone overlays, absorption, intrabar
//! delta and non-default Money-Flow/price modes are explicitly not part of this port, same
//! restriction as other scoped ports in this crate). See
//! `kestrel/plan/kestrel-chartkit-migration.md` for the comparison that identified this as worth
//! porting.
//!
//! Bars with `volume <= 0.0` are treated as `1.0` (synthetic volume) rather than dropped — this
//! is the Pine original's default (`useSynVol = true`) and matters for feeds that report
//! `volume = 0`/unavailable.

use std::collections::VecDeque;

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

use std::collections::HashMap;

/// Money Flow Profile engine — see the module doc comment for how this relates to the
/// [`super::volume_profile`] family.
pub struct MoneyFlowProfileEngine {
    lookback: usize,
    rows: usize,
    va_pct: f64,

    window: VecDeque<Bar>,
    prev_close: Option<f64>,
    prev_vah: Option<f64>,
    prev_val: Option<f64>,
    prev_bull_pct: Option<f64>,

    alerts: Vec<IndicatorAlert>,
}

impl MoneyFlowProfileEngine {
    /// `lookback` bars form the rolling window; `rows` bins the window's high/low range;
    /// `va_pct` is the fraction of total flow the value area expands to capture from POC
    /// outward (Pine default `0.70`).
    pub fn new(lookback: usize, rows: usize, va_pct: f64) -> Self {
        let lookback = lookback.max(1);
        let rows = rows.max(1);
        Self {
            lookback,
            rows,
            va_pct,
            window: VecDeque::with_capacity(lookback),
            prev_close: None,
            prev_vah: None,
            prev_val: None,
            prev_bull_pct: None,
            alerts: Vec::new(),
        }
    }

    /// Pine defaults: `lookback=200`, `rows=25`, `va_pct=0.70`.
    pub fn with_defaults() -> Self {
        Self::new(200, 25, 0.70)
    }
}

impl Indicator for MoneyFlowProfileEngine {
    fn name(&self) -> &str {
        "money_flow_profile"
    }

    fn warmup_period(&self) -> usize {
        // Technical minimum for a non-degenerate high/low range, same as the Pine original
        // (which computes over whatever history exists rather than waiting for the full
        // lookback) — the profile only becomes operationally meaningful once the window has
        // accumulated close to `lookback` bars.
        2
    }

    fn reset(&mut self) {
        self.window.clear();
        self.prev_close = None;
        self.prev_vah = None;
        self.prev_val = None;
        self.prev_bull_pct = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        if self.window.len() == self.lookback {
            self.window.pop_front();
        }
        self.window.push_back(bar.clone());
        if self.window.len() < 2 {
            return None;
        }

        let p_lo = self
            .window
            .iter()
            .map(|b| b.low)
            .fold(f64::INFINITY, f64::min);
        let p_hi = self
            .window
            .iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max);
        if p_hi <= p_lo {
            return None;
        }
        let p_step = (p_hi - p_lo) / self.rows as f64;

        let mut total_flow = vec![0.0_f64; self.rows];
        let mut bull_flow = vec![0.0_f64; self.rows];

        for b in &self.window {
            let (h, l, c) = (b.high, b.low, b.close);
            if h <= l {
                continue;
            }
            let v = if b.volume > 0.0 { b.volume } else { 1.0 };
            let buy_ratio = ((c - l) / (h - l)).clamp(0.0, 1.0);

            for r in 0..self.rows {
                let row_lo = p_lo + r as f64 * p_step;
                let row_hi = row_lo + p_step;
                if h < row_lo || l >= row_hi {
                    continue;
                }
                let overlap = if l >= row_lo && h > row_hi {
                    (row_hi - l) / (h - l)
                } else if h <= row_hi && l < row_lo {
                    (h - row_lo) / (h - l)
                } else if l >= row_lo && h <= row_hi {
                    1.0
                } else {
                    p_step / (h - l)
                };

                let mf_price = p_lo + (r as f64 + 0.5) * p_step; // Row Mid (default)
                let flow = v * overlap * mf_price; // Money Flow source (default): dollar volume.
                total_flow[r] += flow;
                bull_flow[r] += flow * buy_ratio;
            }
        }

        let tot_max = total_flow.iter().cloned().fold(0.0_f64, f64::max);
        if tot_max <= 0.0 {
            return None;
        }
        let tot_sum: f64 = total_flow.iter().sum();
        let poc_idx = total_flow
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap())
            .map(|(i, _)| i)
            .unwrap();
        let poc_price = p_lo + (poc_idx as f64 + 0.5) * p_step;

        let mut delta_poc_idx = 0usize;
        let mut delta_poc_abs_max = 0.0_f64;
        for r in 0..self.rows {
            let d = (2.0 * bull_flow[r] - total_flow[r]).abs();
            if d > delta_poc_abs_max {
                delta_poc_abs_max = d;
                delta_poc_idx = r;
            }
        }
        let delta_poc_price = p_lo + (delta_poc_idx as f64 + 0.5) * p_step;

        // Value Area: expand from POC toward the highest adjacent row until `va_pct` of total
        // flow is captured.
        let mut va_lo = poc_idx;
        let mut va_hi = poc_idx;
        let mut va_acc = total_flow[poc_idx];
        let va_tgt = tot_sum * self.va_pct;
        while va_acc < va_tgt {
            let add_lo = if va_lo > 0 {
                total_flow[va_lo - 1]
            } else {
                -1.0
            };
            let add_hi = if va_hi < self.rows - 1 {
                total_flow[va_hi + 1]
            } else {
                -1.0
            };
            if add_lo < 0.0 && add_hi < 0.0 {
                break;
            }
            if add_lo >= add_hi {
                va_lo -= 1;
                va_acc += add_lo;
            } else {
                va_hi += 1;
                va_acc += add_hi;
            }
        }
        let vah_price = p_lo + (va_hi + 1) as f64 * p_step;
        let val_price = p_lo + va_lo as f64 * p_step;

        let bull_pct = bull_flow.iter().sum::<f64>() / tot_sum * 100.0;

        let mut vah_breakout = false;
        let mut val_breakdown = false;
        let mut bull_bias = false;
        let mut bear_bias = false;
        let mut vah_breakout_strength = 0.0;
        let mut val_breakdown_strength = 0.0;

        if let (Some(prev_close), Some(prev_vah), Some(prev_val), Some(prev_bull_pct)) = (
            self.prev_close,
            self.prev_vah,
            self.prev_val,
            self.prev_bull_pct,
        ) {
            vah_breakout = crossed_over(prev_close, prev_vah, bar.close, vah_price);
            val_breakdown = crossed_under(prev_close, prev_val, bar.close, val_price);
            bull_bias = crossed_over(prev_bull_pct, 50.0, bull_pct, 50.0);
            bear_bias = crossed_under(prev_bull_pct, 50.0, bull_pct, 50.0);

            let va_width = vah_price - val_price;
            vah_breakout_strength = if va_width > 0.0 {
                ((bar.close - vah_price) / va_width).clamp(0.0, 1.0)
            } else {
                1.0
            };
            val_breakdown_strength = if va_width > 0.0 {
                ((val_price - bar.close) / va_width).clamp(0.0, 1.0)
            } else {
                1.0
            };
        }

        if vah_breakout {
            self.alerts.push(IndicatorAlert::new(
                "vah_breakout",
                "Money Flow Profile: close crossed above the Value Area High",
                vah_breakout_strength,
            ));
        }
        if val_breakdown {
            self.alerts.push(IndicatorAlert::new(
                "val_breakdown",
                "Money Flow Profile: close crossed below the Value Area Low",
                val_breakdown_strength,
            ));
        }
        if bull_bias {
            self.alerts.push(IndicatorAlert::new(
                "bull_bias",
                "Money Flow Profile: flow bias turned bullish",
                1.0,
            ));
        }
        if bear_bias {
            self.alerts.push(IndicatorAlert::new(
                "bear_bias",
                "Money Flow Profile: flow bias turned bearish",
                1.0,
            ));
        }

        self.prev_close = Some(bar.close);
        self.prev_vah = Some(vah_price);
        self.prev_val = Some(val_price);
        self.prev_bull_pct = Some(bull_pct);

        let mut extra = HashMap::new();
        extra.insert("vah".to_string(), vah_price);
        extra.insert("val".to_string(), val_price);
        extra.insert("delta_poc".to_string(), delta_poc_price);
        extra.insert("bull_pct".to_string(), bull_pct);

        Some(IndicatorOutput::with_extra(poc_price, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Reference values below were independently derived (Python re-implementation of this same
    /// documented formula, not by running this Rust code) — see
    /// `kestrel/plan/kestrel-chartkit-migration.md`. The two bars are deliberately shaped so raw
    /// volume and dollar volume disagree on which bin dominates: bar A has 3x bar B's volume
    /// (1000 vs 300) but sits at ~1/5th bar B's price (~10 vs ~50) — a raw-volume-weighted
    /// profile (like `VolumeProfileEngine`) would put POC in bar A's bin; dollar-volume weighting
    /// puts it in bar B's bin instead (300 * ~48 > 1000 * ~12), which is exactly the behavior
    /// this engine exists to provide.
    #[test]
    fn dollar_volume_weighting_moves_poc_to_the_higher_priced_lower_volume_bar() {
        let mut mfp = MoneyFlowProfileEngine::new(2, 10, 0.70);
        let bars = [
            Bar::new(1, 10.0, 10.1, 9.9, 10.0, 1000.0),
            Bar::new(2, 49.9, 50.0, 49.8, 49.9, 300.0),
        ];

        assert!(
            mfp.on_bar(&bars[0]).is_none(),
            "single bar is below the technical minimum window"
        );
        let out = mfp
            .on_bar(&bars[1])
            .expect("two-bar window must produce a profile");

        let tol = 1e-6;
        assert!((out.value - 47.995).abs() < tol, "POC: {}", out.value);
        assert!(
            (out.extra["vah"] - 50.0).abs() < tol,
            "VAH: {}",
            out.extra["vah"]
        );
        assert!(
            (out.extra["val"] - 9.9).abs() < tol,
            "VAL: {}",
            out.extra["val"]
        );
        assert!(
            (out.extra["delta_poc"] - 11.905).abs() < tol,
            "Delta POC: {}",
            out.extra["delta_poc"]
        );
        assert!(
            (out.extra["bull_pct"] - 50.0).abs() < tol,
            "bull_pct: {}",
            out.extra["bull_pct"]
        );
    }

    #[test]
    fn bull_bias_alert_fires_when_flow_bias_crosses_above_fifty_percent() {
        let mut mfp = MoneyFlowProfileEngine::new(3, 10, 0.70);
        let bars = [
            Bar::new(1, 10.0, 10.1, 9.9, 10.0, 1000.0),
            Bar::new(2, 49.9, 50.0, 49.8, 49.9, 300.0),
            Bar::new(3, 58.0, 60.0, 55.0, 58.0, 200.0),
        ];

        mfp.on_bar(&bars[0]);
        let bar2_out = mfp.on_bar(&bars[1]).unwrap();
        assert!(
            mfp.alerts().is_empty(),
            "no prior bar to cross from yet on the first emitted output"
        );
        assert!((bar2_out.extra["bull_pct"] - 50.0).abs() < 1e-6);

        let bar3_out = mfp.on_bar(&bars[2]).unwrap();
        let tol = 1e-6;
        assert!(
            (bar3_out.value - 47.474_999_999_999_994).abs() < tol,
            "POC: {}",
            bar3_out.value
        );
        assert!(
            (bar3_out.extra["bull_pct"] - 53.002_600_739_487_37).abs() < tol,
            "bull_pct: {}",
            bar3_out.extra["bull_pct"]
        );
        let alerts = mfp.alerts();
        assert!(
            alerts.iter().any(|a| a.kind == "bull_bias"),
            "bull_pct crossed 50% from below (50.0 -> 53.00): {alerts:?}"
        );
        assert!(
            !alerts.iter().any(|a| a.kind == "bear_bias"),
            "must not also fire the opposite bias: {alerts:?}"
        );
    }

    #[test]
    fn synthetic_volume_substitutes_for_non_positive_volume() {
        let mut mfp = MoneyFlowProfileEngine::new(2, 5, 0.70);
        let bars = [
            Bar::new(1, 100.0, 101.0, 99.0, 100.0, 0.0),
            Bar::new(2, 101.0, 102.0, 100.0, 101.0, 0.0),
        ];
        // Both bars report zero volume; the engine must still produce a profile (treating volume
        // as 1.0) instead of degenerating to an all-zero, POC-less window.
        mfp.on_bar(&bars[0]);
        let out = mfp.on_bar(&bars[1]);
        assert!(
            out.is_some(),
            "zero-volume bars must fall back to synthetic volume, not None"
        );
    }

    #[test]
    fn reset_clears_window_and_cross_state() {
        let mut mfp = MoneyFlowProfileEngine::new(2, 10, 0.70);
        mfp.on_bar(&Bar::new(1, 10.0, 10.1, 9.9, 10.0, 1000.0));
        mfp.on_bar(&Bar::new(2, 49.9, 50.0, 49.8, 49.9, 300.0));
        mfp.reset();
        assert!(mfp
            .on_bar(&Bar::new(1, 10.0, 10.1, 9.9, 10.0, 1000.0))
            .is_none());
    }
}
