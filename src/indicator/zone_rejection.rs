//! Failed breakout at a confirmed zone ("zone rejection").
//!
//! A wick pierces a confirmed support (resistance) zone and the bar closes back above (below)
//! it. Four WavesUnchained scripts arrive at this pattern independently — Wyckoff's spring/UTAD,
//! the Candle Story trap, Structure Break Risk's swing failure, the Chandelier trap — and it is
//! the one setup that carries its own invalidation: beyond the wick tip the idea is simply
//! wrong. Kestrel plan/55 point 3.
//!
//! **Zones** are built causally from confirmed pivots (`pivot_len` bars on each side; a pivot is
//! known `pivot_len` bars after it formed). A pivot high joins an intact resistance zone whose
//! band, widened by `tolerance_atr` ATR, contains it — otherwise it opens a new one; pivot lows
//! build supports the same way. A zone is **confirmed** once `min_touches` pivots formed it, and
//! **broken** — and dropped — on a close more than 0.2 ATR beyond its outer edge (the threshold
//! [`crate::structure::ZoneRegistry`] uses). A zone does not grow wider than `max_width_atr` ATR;
//! a pivot outside that opens a new zone.
//!
//! **Event**, judged against the zones as they stood before the current bar:
//! - `bull_failed_breakout`: the low is below a confirmed support's bottom, the close above it.
//! - `bear_failed_breakout`: the high is above a confirmed resistance's top, the close below it.
//!
//! An outside bar that does both reports neither. `extra["wick"]` carries the wick tip of the
//! event bar (the low for bull, the high for bear) — the natural stop level — and is present
//! only on event bars. `value` is `+1`, `-1` or `0`.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Close beyond the outer edge, in ATR, that breaks a zone — as in `ZoneRegistry`.
const BREAK_ATR: f64 = 0.2;

#[derive(Debug, Clone, Copy, PartialEq)]
struct Zone {
    support: bool,
    bottom: f64,
    top: f64,
    touches: u32,
}

#[derive(Debug, Clone)]
pub struct ZoneRejection {
    pivot_len: usize,
    tolerance_atr: f64,
    max_width_atr: f64,
    min_touches: u32,
    max_zones: usize,
    atr_len: usize,

    window: VecDeque<Bar>,
    atr: Rma,
    atr_value: Option<f64>,
    prev_close: Option<f64>,
    zones: Vec<Zone>,

    event: i8,
    wick: f64,
    strength: f64,
}

impl ZoneRejection {
    pub fn new(
        pivot_len: usize,
        atr_len: usize,
        tolerance_atr: f64,
        max_width_atr: f64,
        min_touches: u32,
    ) -> Self {
        let pivot_len = pivot_len.max(1);
        Self {
            pivot_len,
            tolerance_atr: tolerance_atr.max(0.0),
            max_width_atr: max_width_atr.max(0.0),
            min_touches: min_touches.max(1),
            max_zones: 40,
            atr_len: atr_len.max(1),
            window: VecDeque::with_capacity(2 * pivot_len + 1),
            atr: Rma::new(atr_len.max(1)),
            atr_value: None,
            prev_close: None,
            zones: Vec::new(),
            event: 0,
            wick: 0.0,
            strength: 0.0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 14, 0.25, 2.0, 2)
    }

    /// Adds a confirmed pivot to a matching intact zone of its kind, or opens a new one.
    fn add_pivot(&mut self, price: f64, support: bool, atr: f64) {
        let tolerance = self.tolerance_atr * atr;
        let max_width = self.max_width_atr * atr;
        let matching = self.zones.iter_mut().find(|z| {
            z.support == support
                && price >= z.bottom - tolerance
                && price <= z.top + tolerance
                && z.top.max(price) - z.bottom.min(price) <= max_width
        });
        match matching {
            Some(zone) => {
                zone.bottom = zone.bottom.min(price);
                zone.top = zone.top.max(price);
                zone.touches += 1;
            }
            None => {
                if self.zones.len() == self.max_zones {
                    self.zones.remove(0);
                }
                self.zones.push(Zone {
                    support,
                    bottom: price,
                    top: price,
                    touches: 1,
                });
            }
        }
    }
}

impl Indicator for ZoneRejection {
    fn name(&self) -> &str {
        "zone_rejection"
    }

    /// The first output comes with the ATR; pivots need their own window too.
    fn warmup_period(&self) -> usize {
        (2 * self.pivot_len + 1).max(self.atr_len)
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.event = 0;
        self.strength = 0.0;

        let true_range = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        self.prev_close = Some(bar.close);
        let atr_now = self.atr.update(true_range);

        // The event is judged against the zones as they stood before this bar, with the ATR
        // known before it.
        if let Some(atr) = self.atr_value {
            let confirmed = |z: &&Zone| z.touches >= self.min_touches;
            let bull = self
                .zones
                .iter()
                .filter(confirmed)
                .filter(|z| z.support && bar.low < z.bottom && bar.close > z.bottom)
                .max_by_key(|z| z.touches);
            let bear = self
                .zones
                .iter()
                .filter(confirmed)
                .filter(|z| !z.support && bar.high > z.top && bar.close < z.top)
                .max_by_key(|z| z.touches);
            let touch_strength = |z: &Zone| (f64::from(z.touches) / 5.0).min(1.0);
            match (bull, bear) {
                (Some(z), None) => {
                    self.event = 1;
                    self.wick = bar.low;
                    self.strength = touch_strength(z);
                }
                (None, Some(z)) => {
                    self.event = -1;
                    self.wick = bar.high;
                    self.strength = touch_strength(z);
                }
                _ => {}
            }

            // Breaks: a close far enough beyond the outer edge ends the zone.
            let limit = BREAK_ATR * atr;
            self.zones.retain(|z| {
                if z.support {
                    bar.close >= z.bottom - limit
                } else {
                    bar.close <= z.top + limit
                }
            });
        }

        // Pivots confirm `pivot_len` bars late and only then enter the zones.
        self.window.push_back(bar.clone());
        if self.window.len() > 2 * self.pivot_len + 1 {
            self.window.pop_front();
        }
        if let (Some(atr), true) = (self.atr_value, self.window.len() == 2 * self.pivot_len + 1) {
            let mid = self.pivot_len;
            let pivot = self.window[mid].clone();
            let is_high = self
                .window
                .iter()
                .enumerate()
                .all(|(i, b)| i == mid || b.high < pivot.high);
            let is_low = self
                .window
                .iter()
                .enumerate()
                .all(|(i, b)| i == mid || b.low > pivot.low);
            if is_high {
                self.add_pivot(pivot.high, false, atr);
            }
            if is_low {
                self.add_pivot(pivot.low, true, atr);
            }
        }
        self.atr_value = atr_now.or(self.atr_value);

        self.atr_value?;
        let mut extra = HashMap::new();
        if self.event != 0 {
            extra.insert("wick".to_string(), self.wick);
        }
        extra.insert(
            "zones".to_string(),
            self.zones
                .iter()
                .filter(|z| z.touches >= self.min_touches)
                .count() as f64,
        );
        Some(IndicatorOutput::with_extra(f64::from(self.event), extra))
    }

    fn reset(&mut self) {
        self.window.clear();
        self.atr.reset();
        self.atr_value = None;
        self.prev_close = None;
        self.zones.clear();
        self.event = 0;
        self.wick = 0.0;
        self.strength = 0.0;
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        match self.event {
            1 => vec![IndicatorAlert::new(
                "bull_failed_breakout",
                "ZONE · FAILED BREAKOUT BELOW SUPPORT",
                self.strength,
            )],
            -1 => vec![IndicatorAlert::new(
                "bear_failed_breakout",
                "ZONE · FAILED BREAKOUT ABOVE RESISTANCE",
                self.strength,
            )],
            _ => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(ts: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(ts, open, high, low, close, 0.0)
    }

    /// A range between ~100 and ~110 with two clear lows near 100.2/100.4 and highs near
    /// 109.6/109.9 — deliberately not symmetric around anything.
    fn range() -> Vec<Bar> {
        let closes = [
            105.0, 106.0, 107.5, 108.8, 109.6, 108.7, 107.1, 105.3, 103.0, 101.6, 100.2, 101.4,
            103.2, 105.1, 106.9, 108.4, 109.9, 108.9, 107.2, 105.0, 102.9, 101.5, 100.4, 101.7,
            103.6, 105.4, 106.8, 107.9, 107.0, 105.8,
        ];
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| bar(i as i64, c - 0.1, c + 0.3, c - 0.3, c))
            .collect()
    }

    fn feed(ind: &mut ZoneRejection, bars: &[Bar]) {
        for b in bars {
            ind.on_bar(b);
        }
    }

    #[test]
    fn wick_below_confirmed_support_closing_back_is_bull_failed_breakout() {
        let mut ind = ZoneRejection::new(3, 5, 0.5, 2.0, 2);
        feed(&mut ind, &range());
        // Down to the support, a wick through it, a close back above.
        feed(&mut ind, &[bar(30, 104.0, 104.2, 102.2, 102.4)]);
        feed(&mut ind, &[bar(31, 102.3, 102.6, 101.0, 101.2)]);
        let out = ind
            .on_bar(&bar(32, 101.1, 101.5, 99.3, 100.9))
            .expect("warm");
        assert_eq!(out.value, 1.0);
        assert_eq!(out.extra.get("wick"), Some(&99.3));
        let alerts = ind.alerts();
        assert_eq!(alerts.len(), 1);
        assert_eq!(alerts[0].kind, "bull_failed_breakout");
    }

    #[test]
    fn close_below_support_is_a_break_not_a_failed_breakout() {
        let mut ind = ZoneRejection::new(3, 5, 0.5, 2.0, 2);
        feed(&mut ind, &range());
        feed(&mut ind, &[bar(30, 104.0, 104.2, 102.2, 102.4)]);
        feed(&mut ind, &[bar(31, 102.3, 102.6, 101.0, 101.2)]);
        let out = ind.on_bar(&bar(32, 101.1, 101.3, 98.0, 98.4)).unwrap();
        assert_eq!(out.value, 0.0);
        assert!(ind.alerts().is_empty());
        // The support is gone: the same pattern again is no longer an event.
        let again = ind.on_bar(&bar(33, 99.0, 100.8, 97.5, 99.9)).unwrap();
        assert_eq!(again.value, 0.0);
    }

    #[test]
    fn wick_above_confirmed_resistance_closing_back_is_bear_failed_breakout() {
        let mut ind = ZoneRejection::new(3, 5, 0.5, 2.0, 2);
        feed(&mut ind, &range());
        feed(&mut ind, &[bar(30, 106.0, 108.6, 105.9, 108.4)]);
        let out = ind.on_bar(&bar(31, 108.5, 111.2, 108.2, 109.1)).unwrap();
        assert_eq!(out.value, -1.0);
        assert_eq!(out.extra.get("wick"), Some(&111.2));
        assert_eq!(ind.alerts()[0].kind, "bear_failed_breakout");
    }

    #[test]
    fn a_single_pivot_is_not_a_confirmed_zone() {
        let mut ind = ZoneRejection::new(3, 5, 0.5, 2.0, 3);
        feed(&mut ind, &range());
        feed(&mut ind, &[bar(30, 104.0, 104.2, 102.2, 102.4)]);
        feed(&mut ind, &[bar(31, 102.3, 102.6, 101.0, 101.2)]);
        // Two pivot lows only — three are required here.
        let out = ind.on_bar(&bar(32, 101.1, 101.5, 99.3, 100.9)).unwrap();
        assert_eq!(out.value, 0.0);
    }

    #[test]
    fn no_wick_output_without_event() {
        let mut ind = ZoneRejection::new(3, 5, 0.5, 2.0, 2);
        let mut last = None;
        for b in range() {
            last = ind.on_bar(&b);
        }
        let out = last.expect("warm");
        assert!(!out.extra.contains_key("wick"));
        assert!(out.extra.get("zones").copied().unwrap_or(0.0) >= 1.0);
    }
}
