use super::rsi::Rsi;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use std::collections::VecDeque;

/// Connors RSI: the mean of a price RSI, a streak RSI and a percentile rank of the one-bar return.
///
/// - `rsi_close`: the raw Wilder RSI over `rsi_len` changes of the close.
/// - `rsi_streak`: the raw Wilder RSI over `streak_len` changes of the up/down streak. The streak
///   is `+n` after `n` consecutive higher closes, `-n` after `n` lower ones and `0` on an unchanged
///   close; it exists from the second bar on.
/// - `rank`: the percentage of the `rank_len` one-bar returns *before* the current one that lie
///   below it, `(close - prev) / prev`. **Deliberate deviation:** a return equal to the current one
///   counts half. The original counts only strictly smaller returns, Pine's `ta.percentrank`
///   counts smaller or equal ones; the half-weight keeps the rank symmetric, so a series without
///   movement ranks at `50`.
///
/// Mirroring: the rank part is exactly mirrored (`100 - rank`) by the reciprocal price `1 / close`,
/// which reverses the order of the simple returns. RSI and streak mirror under a price reflection
/// (`2m - close`), not under the reciprocal, so the whole `CRSI` is not exactly mirrored by either.
///
/// `CRSI = (rsi_close + rsi_streak + rank) / 3`, in `0..=100`.
///
/// First output: once all three parts exist, i.e. with bar
/// `max(rsi_len + 1, streak_len + 2, rank_len + 2)`. [`Indicator::reset`] clears all state.
#[derive(Debug, Clone)]
pub struct ConnorsRsiEngine {
    rsi_close: Rsi,
    rsi_streak: Rsi,
    rank_len: usize,
    prev_close: Option<f64>,
    current_streak: f64,
    roc_history: VecDeque<f64>,
}

/// An RSI publishing its raw value: the line average over one sample is the identity.
fn raw_rsi(len: usize) -> Rsi {
    Rsi::new(len, 1, 3, 50.0, 70.0, 30.0, 5, true, len, 4, 10.0)
}

impl ConnorsRsiEngine {
    pub fn new(rsi_len: usize, streak_len: usize, rank_len: usize) -> Self {
        let rank_len = rank_len.max(1);
        Self {
            rsi_close: raw_rsi(rsi_len),
            rsi_streak: raw_rsi(streak_len),
            rank_len,
            prev_close: None,
            current_streak: 0.0,
            roc_history: VecDeque::with_capacity(rank_len + 1),
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
        // The streak starts on the second bar, the rank needs `rank_len` returns before the current.
        self.rsi_close
            .warmup_period()
            .max(self.rsi_streak.warmup_period() + 1)
            .max(self.rank_len + 2)
    }

    fn reset(&mut self) {
        self.rsi_close.reset();
        self.rsi_streak.reset();
        self.prev_close = None;
        self.current_streak = 0.0;
        self.roc_history.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let rsi_c = self.rsi_close.on_bar(bar).map(|o| o.value);

        let prev = self.prev_close.replace(bar.close)?;
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
        let streak_bar = Bar::new(
            bar.timestamp,
            self.current_streak,
            self.current_streak,
            self.current_streak,
            self.current_streak,
            1.0,
        );
        let rsi_s = self.rsi_streak.on_bar(&streak_bar).map(|o| o.value);

        let roc = if prev > 0.0 {
            (bar.close - prev) / prev
        } else {
            0.0
        };
        let rank = (self.roc_history.len() == self.rank_len).then(|| {
            let below = self.roc_history.iter().fold(0.0, |acc, &r| {
                acc + if r < roc {
                    1.0
                } else if r == roc {
                    0.5
                } else {
                    0.0
                }
            });
            100.0 * below / self.rank_len as f64
        });
        self.roc_history.push_back(roc);
        if self.roc_history.len() > self.rank_len {
            self.roc_history.pop_front();
        }

        let crsi = (rsi_c? + rsi_s? + rank?) / 3.0;
        Some(IndicatorOutput::new(crsi.clamp(0.0, 100.0)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

pub const CONNORS_RSI_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "connors_rsi",
        aliases: &[],
        description: "Connors RSI",
        params: &[
            super::definition::ParamSpec::count("rsi_len", 3, 1, 10000)
                .meaning("bars in the price RSI"),
            super::definition::ParamSpec::count("streak_len", 2, 1, 10000)
                .meaning("bars in the streak RSI"),
            super::definition::ParamSpec::count("rank_len", 100, 1, 10000).meaning(
                "earlier one-bar returns the current one is ranked against; ties count half",
            ),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(ConnorsRsiEngine::new(
                p.count("rsi_len"),
                p.count("streak_len"),
                p.count("rank_len"),
            )))
        },
    };

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

    fn closes(engine: &mut ConnorsRsiEngine, closes: &[f64]) -> Vec<Option<f64>> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| {
                engine
                    .on_bar(&Bar::new(i as i64, c, c, c, c, 1.0))
                    .map(|o| o.value)
            })
            .collect()
    }

    /// Asymmetric: uneven up and down steps, so the rank window decides the rank.
    const UNEVEN: [f64; 16] = [
        100.0, 103.0, 102.5, 106.0, 105.0, 104.8, 109.0, 108.0, 108.4, 112.0, 111.0, 110.5, 110.9,
        116.0, 115.0, 115.2,
    ];

    #[test]
    fn rank_len_sets_the_rank_window() {
        let short = closes(&mut ConnorsRsiEngine::new(2, 2, 3), &UNEVEN);
        let long = closes(&mut ConnorsRsiEngine::new(2, 2, 8), &UNEVEN);
        assert_ne!(short.last(), long.last());
    }

    #[test]
    fn first_output_waits_for_a_full_rank_window() {
        let mut engine = ConnorsRsiEngine::new(2, 2, 8);
        assert_eq!(engine.warmup_period(), 10);
        let out = closes(&mut engine, &UNEVEN);
        assert_eq!(out.iter().position(Option::is_some), Some(9));
    }

    /// Nothing moves: price RSI and streak RSI are 50, and every return ties with the current one,
    /// so the rank sits in the middle as well.
    #[test]
    fn unchanged_closes_give_the_midpoint() {
        let out = closes(&mut ConnorsRsiEngine::new(3, 2, 5), &[50.0; 12]);
        assert_eq!(out.last(), Some(&Some(50.0)));
    }
}
