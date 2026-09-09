use super::rsi::Rsi;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::stats::percent_rank;
use std::collections::VecDeque;

/// Connors RSI Engine.
/// Connors RSI = (RSI(close, 3) + RSI(Streak, 2) + PercentRank(ROC(1), 100)) / 3
#[derive(Debug, Clone)]
pub struct ConnorsRsiEngine {
    rsi_close: Rsi,
    rsi_streak: Rsi,
    prev_close: Option<f64>,
    current_streak: f64,
    roc_history: VecDeque<f64>,
}

impl ConnorsRsiEngine {
    pub fn new(rsi_len: usize, streak_len: usize, rank_len: usize) -> Self {
        Self {
            rsi_close: Rsi::with_period(rsi_len),
            rsi_streak: Rsi::with_period(streak_len),
            prev_close: None,
            current_streak: 0.0,
            roc_history: VecDeque::with_capacity(rank_len),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(3, 2, 100)
    }
}

impl Indicator for ConnorsRsiEngine {
    fn name(&self) -> &str {
        "connors_rsi"
    }

    fn warmup_period(&self) -> usize {
        100
    }

    fn reset(&mut self) {
        self.rsi_close.reset();
        self.rsi_streak.reset();
        self.prev_close = None;
        self.current_streak = 0.0;
        self.roc_history.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let rsi_c = self.rsi_close.on_bar(bar)?.value;

        if let Some(prev) = self.prev_close {
            if bar.close > prev {
                self.current_streak = if self.current_streak >= 0.0 {
                    self.current_streak + 1.0
                } else {
                    1.0
                };
            } else if bar.close < prev {
                self.current_streak = if self.current_streak <= 0.0 {
                    self.current_streak - 1.0
                } else {
                    -1.0
                };
            } else {
                self.current_streak = 0.0;
            }

            let roc = if prev > 0.0 {
                (bar.close - prev) / prev
            } else {
                0.0
            };
            self.roc_history.push_back(roc);
            if self.roc_history.len() > 100 {
                self.roc_history.pop_front();
            }
        }
        self.prev_close = Some(bar.close);

        // Dummy bar wrapping streak as price
        let streak_bar = Bar::new(
            bar.timestamp,
            self.current_streak,
            self.current_streak,
            self.current_streak,
            self.current_streak,
            1.0,
        );
        let rsi_s = self.rsi_streak.on_bar(&streak_bar)?.value;

        let last_roc = self.roc_history.back().copied().unwrap_or(0.0);
        let slice: Vec<f64> = self.roc_history.iter().copied().collect();
        let prank = percent_rank(&slice, last_roc);

        let crsi = (rsi_c + rsi_s + prank) / 3.0;
        Some(IndicatorOutput::new(crsi.clamp(0.0, 100.0)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connors_rsi() {
        let mut crsi = ConnorsRsiEngine::with_defaults();
        let mut out = None;
        for i in 0..120 {
            let b = Bar::new(i, 100.0, 105.0, 95.0, 100.0 + (i % 5) as f64, 1000.0);
            out = crsi.on_bar(&b);
        }
        assert!(out.is_some());
        let val = out.unwrap().value;
        assert!((0.0..=100.0).contains(&val));
    }
}
