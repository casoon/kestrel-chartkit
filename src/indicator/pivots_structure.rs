use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Market Structure & Pivot Points Engine.
/// Detects pivot highs/lows (HH/HL/LH/LL) and computes bounded structure score (-100..+100).
pub struct PivotStructureEngine {
    left_bars: usize,
    right_bars: usize,
    score_window: usize,
    bars: Vec<Bar>,
    pivot_scores: Vec<f64>,
    last_high: Option<f64>,
    last_low: Option<f64>,
    prev_high: Option<f64>,
    prev_low: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl PivotStructureEngine {
    pub fn new(left_bars: usize, right_bars: usize, score_window: usize) -> Self {
        Self {
            left_bars,
            right_bars,
            score_window,
            bars: Vec::new(),
            pivot_scores: Vec::new(),
            last_high: None,
            last_low: None,
            prev_high: None,
            prev_low: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for PivotStructureEngine {
    fn name(&self) -> &str {
        "pivots_structure"
    }

    fn warmup_period(&self) -> usize {
        self.left_bars + self.right_bars + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.pivot_scores.clear();
        self.last_high = None;
        self.last_low = None;
        self.prev_high = None;
        self.prev_low = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push(bar.clone());
        let max_history = (self.left_bars + self.right_bars + 1) * 4;
        if self.bars.len() > max_history {
            self.bars.remove(0);
        }

        self.alerts.clear();

        let req_len = self.left_bars + self.right_bars + 1;
        if self.bars.len() < req_len {
            return None;
        }

        // Pivot index candidate is `self.bars.len() - 1 - self.right_bars`
        let candidate_idx = self.bars.len() - 1 - self.right_bars;
        let cand_high = self.bars[candidate_idx].high;
        let cand_low = self.bars[candidate_idx].low;

        let mut is_pivot_high = true;
        let mut is_pivot_low = true;

        for i in (candidate_idx - self.left_bars)..=candidate_idx + self.right_bars {
            if i == candidate_idx {
                continue;
            }
            if self.bars[i].high >= cand_high {
                is_pivot_high = false;
            }
            if self.bars[i].low <= cand_low {
                is_pivot_low = false;
            }
        }

        let mut cur_score = 0.0;
        let mut found_pivot = false;

        if is_pivot_high {
            self.prev_high = self.last_high;
            self.last_high = Some(cand_high);
            if let Some(prev) = self.prev_high {
                cur_score += if cand_high > prev { 2.0 } else { -1.0 };
                found_pivot = true;
            }
        }

        if is_pivot_low {
            self.prev_low = self.last_low;
            self.last_low = Some(cand_low);
            if let Some(prev) = self.prev_low {
                cur_score += if cand_low > prev { 1.0 } else { -2.0 };
                found_pivot = true;
            }
        }

        if found_pivot {
            self.pivot_scores.push(cur_score);
            if self.pivot_scores.len() > self.score_window {
                self.pivot_scores.remove(0);
            }
        }

        let score = if !self.pivot_scores.is_empty() {
            let sum: f64 = self.pivot_scores.iter().sum();
            let max_possible = (self.score_window as f64) * 2.0;
            let raw = (sum / max_possible) * 100.0;
            raw.clamp(-100.0, 100.0)
        } else {
            0.0
        };

        if score >= 50.0 {
            self.alerts.push(IndicatorAlert::new(
                "structure_bullish_bias",
                format!("Strong Bullish Market Structure Bias (+{:.0} Score)", score),
                0.85,
            ));
        } else if score <= -50.0 {
            self.alerts.push(IndicatorAlert::new(
                "structure_bearish_bias",
                format!("Strong Bearish Market Structure Bias ({:.0} Score)", score),
                0.85,
            ));
        }

        Some(IndicatorOutput::new(score))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_pivots_structure(params: &HashMap<String, f64>) -> PivotStructureEngine {
    let left_bars = params.get("left_bars").copied().unwrap_or(5.0) as usize;
    let right_bars = params.get("right_bars").copied().unwrap_or(5.0) as usize;
    let score_window = params.get("score_window").copied().unwrap_or(10.0) as usize;
    PivotStructureEngine::new(left_bars, right_bars, score_window)
}
