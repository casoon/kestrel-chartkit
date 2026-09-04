//! Chandelier Exit: an ATR-trailing stop that flips direction when price closes beyond the
//! opposite stop, ratcheting only in the favorable direction while a position is held. A
//! reusable engine instead of the ad-hoc, per-script duplication of this formula across the Pine
//! catalog.
//!
//! ```text
//! long_stop  = highest(high, length) - mult * atr(length)
//! short_stop = lowest(low, length)   + mult * atr(length)
//! // stops only ever tighten (never loosen) while the position direction is unchanged
//! long_stop  := close[1] > long_stop[1]  ? max(long_stop,  long_stop[1])  : long_stop
//! short_stop := close[1] < short_stop[1] ? min(short_stop, short_stop[1]) : short_stop
//! direction flips to long when close > short_stop[1], to short when close < long_stop[1]
//! ```

use std::collections::HashMap;

use crate::model::Bar;
use crate::series::Series;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Reusable Chandelier Exit engine. `length` sizes both the highest-high/lowest-low lookback and
/// the internal ATR; `atr_mult` scales the ATR offset from those extremes.
#[derive(Debug, Clone)]
pub struct ChandelierExitEngine {
    length: usize,
    atr_mult: f64,
    tr_rma: Rma,
    prev_close: Option<f64>,
    highs: Series<f64>,
    lows: Series<f64>,
    long_stop_prev: Option<f64>,
    short_stop_prev: Option<f64>,
    direction: i8,
    alerts: Vec<IndicatorAlert>,
}

impl ChandelierExitEngine {
    pub fn new(length: usize, atr_mult: f64) -> Self {
        let length = length.max(1);
        Self {
            length,
            atr_mult,
            tr_rma: Rma::new(length),
            prev_close: None,
            highs: Series::new(length),
            lows: Series::new(length),
            long_stop_prev: None,
            short_stop_prev: None,
            direction: 1,
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(22, 3.0)
    }
}

impl Indicator for ChandelierExitEngine {
    fn name(&self) -> &str {
        "chandelier_exit"
    }

    fn warmup_period(&self) -> usize {
        self.length
    }

    fn reset(&mut self) {
        self.tr_rma.reset();
        self.prev_close = None;
        self.highs.reset();
        self.lows.reset();
        self.long_stop_prev = None;
        self.short_stop_prev = None;
        self.direction = 1;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        let tr = match self.prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };

        self.highs.push(bar.high);
        self.lows.push(bar.low);
        let atr = self.tr_rma.update(tr);

        let (highest_high, lowest_low, atr) = match (
            self.highs.highest(self.length),
            self.lows.lowest(self.length),
            atr,
        ) {
            (Some(hh), Some(ll), Some(a)) => (hh, ll, a),
            _ => {
                self.prev_close = Some(bar.close);
                return None;
            }
        };

        let raw_long_stop = highest_high - self.atr_mult * atr;
        let raw_short_stop = lowest_low + self.atr_mult * atr;

        let long_stop_prev = self.long_stop_prev.unwrap_or(raw_long_stop);
        let short_stop_prev = self.short_stop_prev.unwrap_or(raw_short_stop);

        let long_stop = match self.prev_close {
            Some(pc) if pc > long_stop_prev => raw_long_stop.max(long_stop_prev),
            _ => raw_long_stop,
        };
        let short_stop = match self.prev_close {
            Some(pc) if pc < short_stop_prev => raw_short_stop.min(short_stop_prev),
            _ => raw_short_stop,
        };

        let mut direction = self.direction;
        if bar.close > short_stop_prev {
            direction = 1;
        } else if bar.close < long_stop_prev {
            direction = -1;
        }

        if direction != self.direction {
            let (kind, note) = if direction == 1 {
                ("chandelier_flip_long", "Chandelier Exit flipped long")
            } else {
                ("chandelier_flip_short", "Chandelier Exit flipped short")
            };
            self.alerts.push(IndicatorAlert::new(kind, note, 1.0));
        }

        self.direction = direction;
        self.long_stop_prev = Some(long_stop);
        self.short_stop_prev = Some(short_stop);
        self.prev_close = Some(bar.close);

        let (stop_value, opposite_stop, state) = if direction == 1 {
            (long_stop, short_stop, "long")
        } else {
            (short_stop, long_stop, "short")
        };

        let mut extra = HashMap::new();
        extra.insert("long_stop".to_string(), long_stop);
        extra.insert("short_stop".to_string(), short_stop);

        Some(
            IndicatorOutput::with_extra(stop_value, extra)
                .with_secondary(opposite_stop)
                .with_state(state),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trending_up_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64 * 2.0;
                Bar::new(
                    i as i64 * 60,
                    base,
                    base + 3.0,
                    base - 3.0,
                    base + 1.0,
                    100.0,
                )
            })
            .collect()
    }

    #[test]
    fn test_warmup_returns_none_then_emits() {
        let mut engine = ChandelierExitEngine::new(5, 3.0);
        let bars = trending_up_bars(10);
        let mut outputs = Vec::new();
        for bar in &bars {
            outputs.push(engine.on_bar(bar));
        }
        assert!(outputs[..4].iter().all(|o| o.is_none()));
        assert!(outputs[4..].iter().all(|o| o.is_some()));
    }

    #[test]
    fn test_stop_only_moves_favorably_while_trending() {
        let mut engine = ChandelierExitEngine::new(5, 3.0);
        let bars = trending_up_bars(20);
        let mut long_stops = Vec::new();
        for bar in &bars {
            if let Some(out) = engine.on_bar(bar) {
                if out.state.as_deref() == Some("long") {
                    long_stops.push(out.value);
                }
            }
        }
        assert!(long_stops.len() > 2);
        for pair in long_stops.windows(2) {
            assert!(
                pair[1] >= pair[0] - 1e-9,
                "long stop must never loosen while price keeps making new highs: {:?}",
                pair
            );
        }
    }

    #[test]
    fn test_direction_flips_on_stop_breach() {
        let mut engine = ChandelierExitEngine::new(3, 1.0);
        let mut bars = trending_up_bars(6);
        // Sharp reversal candle far below the recent range breaches the long stop.
        bars.push(Bar::new(600, 90.0, 91.0, 60.0, 61.0, 100.0));

        let mut flipped_short = false;
        for bar in &bars {
            if let Some(out) = engine.on_bar(bar) {
                if out.state.as_deref() == Some("short") {
                    flipped_short = true;
                }
            }
        }
        assert!(flipped_short, "sharp reversal must flip direction to short");
    }

    #[test]
    fn test_reset_clears_state() {
        let mut engine = ChandelierExitEngine::new(3, 2.0);
        for bar in trending_up_bars(6) {
            engine.on_bar(&bar);
        }
        engine.reset();
        assert_eq!(engine.on_bar(&trending_up_bars(1)[0]), None);
    }
}
