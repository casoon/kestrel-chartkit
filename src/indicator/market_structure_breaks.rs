use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendState {
    Uptrend,
    Downtrend,
    Neutral,
}

/// Market Structure Break (BOS / ChOCH) Engine.
/// Detects Break of Structure (BOS) for trend continuation and Change of Character (ChOCH) for trend reversals.
pub struct MarketStructureBreaksEngine {
    lookback: usize,
    bars: Vec<Bar>,
    swing_high: Option<f64>,
    swing_low: Option<f64>,
    trend: TrendState,
    alerts: Vec<IndicatorAlert>,
}

impl MarketStructureBreaksEngine {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            bars: Vec::new(),
            swing_high: None,
            swing_low: None,
            trend: TrendState::Neutral,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for MarketStructureBreaksEngine {
    fn name(&self) -> &str {
        "market_structure_breaks"
    }

    fn warmup_period(&self) -> usize {
        self.lookback * 2 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.swing_high = None;
        self.swing_low = None;
        self.trend = TrendState::Neutral;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push(bar.clone());
        let max_history = self.lookback * 4 + 1;
        if self.bars.len() > max_history {
            self.bars.remove(0);
        }

        self.alerts.clear();

        let req_len = self.lookback * 2 + 1;
        if self.bars.len() < req_len {
            return None;
        }

        let cand_idx = self.bars.len() - 1 - self.lookback;
        let cand_high = self.bars[cand_idx].high;
        let cand_low = self.bars[cand_idx].low;

        let mut is_high = true;
        let mut is_low = true;

        for i in (cand_idx - self.lookback)..=cand_idx + self.lookback {
            if i == cand_idx {
                continue;
            }
            if self.bars[i].high >= cand_high {
                is_high = false;
            }
            if self.bars[i].low <= cand_low {
                is_low = false;
            }
        }

        if is_high {
            self.swing_high = Some(cand_high);
        }
        if is_low {
            self.swing_low = Some(cand_low);
        }

        let mut signal_val = 0.0f64;

        if let Some(sh) = self.swing_high {
            if bar.close > sh {
                if self.trend == TrendState::Uptrend {
                    self.alerts.push(IndicatorAlert::new(
                        "bullish_bos",
                        format!("Bullish Break of Structure (BOS) above ${:.2}", sh),
                        0.90,
                    ));
                    signal_val = 1.0;
                } else if self.trend == TrendState::Downtrend || self.trend == TrendState::Neutral {
                    self.trend = TrendState::Uptrend;
                    self.alerts.push(IndicatorAlert::new(
                        "bullish_choch",
                        format!(
                            "Bullish Change of Character (ChOCH Reversal) above ${:.2}",
                            sh
                        ),
                        0.95,
                    ));
                    signal_val = 2.0;
                }
                self.swing_high = None;
            }
        }

        if let Some(sl) = self.swing_low {
            if bar.close < sl {
                if self.trend == TrendState::Downtrend {
                    self.alerts.push(IndicatorAlert::new(
                        "bearish_bos",
                        format!("Bearish Break of Structure (BOS) below ${:.2}", sl),
                        0.90,
                    ));
                    signal_val = -1.0;
                } else if self.trend == TrendState::Uptrend || self.trend == TrendState::Neutral {
                    self.trend = TrendState::Downtrend;
                    self.alerts.push(IndicatorAlert::new(
                        "bearish_choch",
                        format!(
                            "Bearish Change of Character (ChOCH Reversal) below ${:.2}",
                            sl
                        ),
                        0.95,
                    ));
                    signal_val = -2.0;
                }
                self.swing_low = None;
            }
        }

        Some(IndicatorOutput::new(signal_val))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_market_structure_breaks(params: &HashMap<String, f64>) -> MarketStructureBreaksEngine {
    let lookback = params.get("lookback").copied().unwrap_or(5.0) as usize;
    MarketStructureBreaksEngine::new(lookback)
}
