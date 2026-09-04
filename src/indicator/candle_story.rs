use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Candle Story Engine (Price Action & Pattern Recognition).
/// Identifies Pinbars/Kangaroo Tails, Engulfing Bars, Tweezers, and Marubozu candles.
pub struct CandleStoryEngine {
    window: Vec<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl CandleStoryEngine {
    pub fn new() -> Self {
        Self {
            window: Vec::with_capacity(5),
            alerts: Vec::new(),
        }
    }
}

impl Default for CandleStoryEngine {
    fn default() -> Self {
        Self::new()
    }
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
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push(bar.clone());
        if self.window.len() > 5 {
            self.window.remove(0);
        }

        self.alerts.clear();

        let range = bar.high - bar.low;
        if range <= 0.0 {
            return Some(IndicatorOutput::new(0.0));
        }

        let body = (bar.close - bar.open).abs();
        let upper_wick = bar.high - bar.close.max(bar.open);
        let lower_wick = bar.close.min(bar.open) - bar.low;
        let is_bullish = bar.close >= bar.open;

        // Calculate Candle Pressure (-100..+100)
        let close_pos = (bar.close - bar.low) / range; // 0.0 to 1.0
        let pressure = (close_pos - 0.5) * 200.0;

        let mut pattern_type = 0.0f64;

        // 1. Kangaroo Tail / Pinbar Reversal
        if lower_wick >= 0.55 * range && close_pos >= 0.65 {
            pattern_type = 1.0; // Bullish Kangaroo
            self.alerts.push(IndicatorAlert::new(
                "bullish_kangaroo_tail",
                format!(
                    "Bullish Kangaroo Tail / Pinbar Reversal (Lower Wick {:.0}%)",
                    (lower_wick / range) * 100.0
                ),
                0.9,
            ));
        } else if upper_wick >= 0.55 * range && close_pos <= 0.35 {
            pattern_type = 2.0; // Bearish Kangaroo
            self.alerts.push(IndicatorAlert::new(
                "bearish_kangaroo_tail",
                format!(
                    "Bearish Kangaroo Tail / Pinbar Reversal (Upper Wick {:.0}%)",
                    (upper_wick / range) * 100.0
                ),
                0.9,
            ));
        }

        // 2. Engulfing Bar (requires at least 2 bars)
        if self.window.len() >= 2 {
            let prev = &self.window[self.window.len() - 2];
            let prev_body = (prev.close - prev.open).abs();
            let prev_bearish = prev.close < prev.open;
            let prev_bullish = prev.close > prev.open;

            if is_bullish && prev_bearish && body > prev_body && bar.close > prev.open {
                pattern_type = 3.0; // Bullish Engulfing
                self.alerts.push(IndicatorAlert::new(
                    "bullish_engulfing",
                    "Bullish Engulfing Pattern (Strong Momentum Shift)",
                    0.85,
                ));
            } else if !is_bullish && prev_bullish && body > prev_body && bar.close < prev.open {
                pattern_type = 4.0; // Bearish Engulfing
                self.alerts.push(IndicatorAlert::new(
                    "bearish_engulfing",
                    "Bearish Engulfing Pattern (Strong Seller Dominance)",
                    0.85,
                ));
            }

            // 3. Tweezer Top / Bottom
            let high_diff = (bar.high - prev.high).abs() / bar.high;
            let low_diff = (bar.low - prev.low).abs() / bar.low;

            if low_diff < 0.0015 && is_bullish && prev_bearish {
                pattern_type = 5.0; // Bullish Tweezer
                self.alerts.push(IndicatorAlert::new(
                    "bullish_tweezer",
                    "Bullish Tweezer Bottom (Double Support Reversal)",
                    0.8,
                ));
            } else if high_diff < 0.0015 && !is_bullish && prev_bullish {
                pattern_type = 6.0; // Bearish Tweezer
                self.alerts.push(IndicatorAlert::new(
                    "bearish_tweezer",
                    "Bearish Tweezer Top (Double Resistance Reversal)",
                    0.8,
                ));
            }
        }

        // 4. Marubozu (Extreme Institutional Dominance)
        if body / range >= 0.82 {
            if is_bullish {
                pattern_type = 7.0;
                self.alerts.push(IndicatorAlert::new(
                    "bullish_marubozu",
                    format!(
                        "Bullish Marubozu ({:.0}% Body Dominance)",
                        (body / range) * 100.0
                    ),
                    0.8,
                ));
            } else {
                pattern_type = 8.0;
                self.alerts.push(IndicatorAlert::new(
                    "bearish_marubozu",
                    format!(
                        "Bearish Marubozu ({:.0}% Body Dominance)",
                        (body / range) * 100.0
                    ),
                    0.8,
                ));
            }
        }

        let mut extra = HashMap::new();
        extra.insert("pattern_type".to_string(), pattern_type);
        extra.insert("pressure".to_string(), pressure);
        extra.insert("body_ratio".to_string(), body / range);

        Some(IndicatorOutput::with_extra(pressure, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_candle_story(_params: &HashMap<String, f64>) -> CandleStoryEngine {
    CandleStoryEngine::new()
}
