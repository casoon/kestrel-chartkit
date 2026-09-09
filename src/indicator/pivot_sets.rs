use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::timeframe::{Timeframe, TimeframeError};
use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Type of pivot calculation set.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum PivotSetType {
    #[default]
    Classic,
    Fibonacci,
    Camarilla,
    Woodie,
    DeMark,
    Cpr, // Central Pivot Range
}

/// Multi-pivot sets engine calculating classic, fibonacci, camarilla, woodie, demark and CPR levels.
#[derive(Debug, Clone)]
pub struct PivotSetsEngine {
    pivot_type: PivotSetType,
    period_high: f64,
    period_low: f64,
    period_close: f64,
    period_open: f64,
    curr_period_high: f64,
    curr_period_low: f64,
    curr_period_open: f64,
    curr_period_close: f64,
    period_timeframe: Timeframe,
    utc_offset_seconds: i32,
    current_period_start: Option<i64>,
}

impl PivotSetsEngine {
    pub fn new(pivot_type: PivotSetType) -> Self {
        Self {
            pivot_type,
            period_high: 0.0,
            period_low: 0.0,
            period_close: 0.0,
            period_open: 0.0,
            curr_period_high: 0.0,
            curr_period_low: f64::MAX,
            curr_period_open: 0.0,
            curr_period_close: 0.0,
            period_timeframe: Timeframe::Day(1),
            utc_offset_seconds: 0,
            current_period_start: None,
        }
    }

    pub fn with_timeframe(
        pivot_type: PivotSetType,
        period_timeframe: Timeframe,
    ) -> Result<Self, TimeframeError> {
        let mut engine = Self::new(pivot_type);
        engine.period_timeframe = period_timeframe.validate()?;
        Ok(engine)
    }

    pub fn with_utc_offset(mut self, utc_offset_seconds: i32) -> Self {
        self.utc_offset_seconds = utc_offset_seconds;
        self
    }

    pub fn with_defaults() -> Self {
        Self::new(PivotSetType::Classic)
    }
}

impl Indicator for PivotSetsEngine {
    fn name(&self) -> &str {
        "pivot_sets"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.period_high = 0.0;
        self.period_low = 0.0;
        self.period_close = 0.0;
        self.period_open = 0.0;
        self.curr_period_high = 0.0;
        self.curr_period_low = f64::MAX;
        self.curr_period_open = 0.0;
        self.curr_period_close = 0.0;
        self.current_period_start = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let period_start = self
            .period_timeframe
            .bucket_start(bar.timestamp, self.utc_offset_seconds);
        let is_new_period = self
            .current_period_start
            .is_some_and(|previous| previous != period_start);
        self.current_period_start = Some(period_start);

        if is_new_period {
            self.period_high = self.curr_period_high;
            self.period_low = self.curr_period_low;
            self.period_close = self.curr_period_close;
            self.period_open = self.curr_period_open;

            self.curr_period_high = bar.high;
            self.curr_period_low = bar.low;
            self.curr_period_open = bar.open;
            self.curr_period_close = bar.close;
        } else {
            if self.curr_period_open == 0.0 {
                self.curr_period_open = bar.open;
            }
            self.curr_period_high = self.curr_period_high.max(bar.high);
            self.curr_period_low = self.curr_period_low.min(bar.low);
            self.curr_period_close = bar.close;
        }

        let (h, l, c, o) = if self.period_high > 0.0 {
            (
                self.period_high,
                self.period_low,
                self.period_close,
                self.period_open,
            )
        } else {
            (
                self.curr_period_high,
                self.curr_period_low,
                bar.close,
                self.curr_period_open,
            )
        };

        let mut extra = HashMap::new();
        let p: f64;

        match self.pivot_type {
            PivotSetType::Classic => {
                p = (h + l + c) / 3.0;
                extra.insert("p".to_string(), p);
                extra.insert("r1".to_string(), 2.0 * p - l);
                extra.insert("s1".to_string(), 2.0 * p - h);
                extra.insert("r2".to_string(), p + (h - l));
                extra.insert("s2".to_string(), p - (h - l));
                extra.insert("r3".to_string(), h + 2.0 * (p - l));
                extra.insert("s3".to_string(), l - 2.0 * (h - p));
            }
            PivotSetType::Fibonacci => {
                p = (h + l + c) / 3.0;
                let range = h - l;
                extra.insert("p".to_string(), p);
                extra.insert("r1".to_string(), p + 0.382 * range);
                extra.insert("s1".to_string(), p - 0.382 * range);
                extra.insert("r2".to_string(), p + 0.618 * range);
                extra.insert("s2".to_string(), p - 0.618 * range);
                extra.insert("r3".to_string(), p + 1.000 * range);
                extra.insert("s3".to_string(), p - 1.000 * range);
            }
            PivotSetType::Camarilla => {
                p = (h + l + c) / 3.0;
                let range = h - l;
                extra.insert("p".to_string(), p);
                extra.insert("r1".to_string(), c + range * 1.1 / 12.0);
                extra.insert("s1".to_string(), c - range * 1.1 / 12.0);
                extra.insert("r2".to_string(), c + range * 1.1 / 6.0);
                extra.insert("s2".to_string(), c - range * 1.1 / 6.0);
                extra.insert("r3".to_string(), c + range * 1.1 / 4.0);
                extra.insert("s3".to_string(), c - range * 1.1 / 4.0);
                extra.insert("r4".to_string(), c + range * 1.1 / 2.0);
                extra.insert("s4".to_string(), c - range * 1.1 / 2.0);
            }
            PivotSetType::Woodie => {
                p = (h + l + 2.0 * c) / 4.0;
                extra.insert("p".to_string(), p);
                extra.insert("r1".to_string(), 2.0 * p - l);
                extra.insert("s1".to_string(), 2.0 * p - h);
                extra.insert("r2".to_string(), p + (h - l));
                extra.insert("s2".to_string(), p - (h - l));
            }
            PivotSetType::DeMark => {
                let x = if c < o {
                    h + 2.0 * l + c
                } else if c > o {
                    2.0 * h + l + c
                } else {
                    h + l + 2.0 * c
                };
                p = x / 4.0;
                extra.insert("p".to_string(), p);
                extra.insert("r1".to_string(), x / 2.0 - l);
                extra.insert("s1".to_string(), x / 2.0 - h);
            }
            PivotSetType::Cpr => {
                p = (h + l + c) / 3.0;
                let bc = (h + l) / 2.0;
                let tc = (p - bc) + p;
                extra.insert("p".to_string(), p);
                extra.insert("tc".to_string(), tc.max(bc));
                extra.insert("bc".to_string(), tc.min(bc));
            }
        }

        Some(IndicatorOutput::with_extra(p, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pivot_sets_classic() {
        let mut ps = PivotSetsEngine::new(PivotSetType::Classic);
        let bar1 = Bar::new(0, 100.0, 110.0, 90.0, 105.0, 1000.0);
        let bar2 = Bar::new(86400, 105.0, 115.0, 100.0, 110.0, 1000.0);

        ps.on_bar(&bar1);
        let out = ps.on_bar(&bar2).unwrap();
        // Pivot P = (110 + 90 + 105) / 3 = 101.666...
        assert!((out.extra["p"] - 101.666_666_666_666_67).abs() < 1e-12);
        assert!(out.extra.contains_key("r1"));
        assert!(out.extra.contains_key("s1"));
    }

    #[test]
    fn period_close_comes_from_previous_period() {
        let mut ps = PivotSetsEngine::new(PivotSetType::Classic);
        ps.on_bar(&Bar::new(0, 100.0, 110.0, 90.0, 101.0, 1.0));
        ps.on_bar(&Bar::new(60, 101.0, 112.0, 91.0, 107.0, 1.0));
        let output = ps
            .on_bar(&Bar::new(86_400, 200.0, 210.0, 190.0, 205.0, 1.0))
            .unwrap();
        assert!((output.extra["p"] - (112.0 + 90.0 + 107.0) / 3.0).abs() < 1e-12);
    }
}
