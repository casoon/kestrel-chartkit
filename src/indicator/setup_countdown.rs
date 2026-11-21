//! Setup (9) and countdown (13) exhaustion count after DeMark (1994).
//!
//! The setup counts consecutive closes against the close four bars earlier, starting on a price
//! flip. A completed setup of nine starts a countdown of thirteen qualifying closes against the low
//! (buy) or high (sell) two bars earlier; the thirteenth additionally has to stay on the right side
//! of the close of the eighth.

use std::collections::{HashMap, VecDeque};

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Side {
    None,
    Buy,
    Sell,
}

/// Setup and countdown counter. Publishes from the sixth bar on: `value` is the setup count (positive
/// for a sell setup, negative for a buy setup, `0` for none) and the extra `countdown` the countdown
/// count with the same sign (`+-13` on the completion bar). Alerts `buy_setup`, `sell_setup`,
/// `buy_countdown` and `sell_countdown` fire on the bar that completes the count.
/// [`Indicator::reset`] clears all state.
pub struct SetupCountdownEngine {
    /// The last six bars as `(high, low, close)`.
    window: VecDeque<(f64, f64, f64)>,
    setup_dir: Side,
    setup_count: u32,
    cd_dir: Side,
    cd_count: u32,
    cd_c8: f64,
    alerts: Vec<IndicatorAlert>,
}

impl Default for SetupCountdownEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl SetupCountdownEngine {
    pub fn new() -> Self {
        Self {
            window: VecDeque::new(),
            setup_dir: Side::None,
            setup_count: 0,
            cd_dir: Side::None,
            cd_count: 0,
            cd_c8: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for SetupCountdownEngine {
    fn name(&self) -> &str {
        "setup_countdown"
    }

    fn warmup_period(&self) -> usize {
        6
    }

    fn reset(&mut self) {
        *self = Self::new();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.window.push_back((bar.high, bar.low, bar.close));
        if self.window.len() > 6 {
            self.window.pop_front();
        }
        if self.window.len() < 6 {
            return None;
        }
        // Indices: 0 = bar i-5, 1 = i-4, 3 = i-2, 4 = i-1, 5 = i.
        let (high, low, close) = self.window[5];
        let c_back4 = self.window[1].2;
        let c_prev = self.window[4].2;
        let c_back5 = self.window[0].2;

        if close < c_back4 {
            if self.setup_dir == Side::Buy {
                self.setup_count += 1;
            } else if c_prev > c_back5 {
                self.setup_dir = Side::Buy;
                self.setup_count = 1;
            } else {
                self.setup_dir = Side::None;
                self.setup_count = 0;
            }
        } else if close > c_back4 {
            if self.setup_dir == Side::Sell {
                self.setup_count += 1;
            } else if c_prev < c_back5 {
                self.setup_dir = Side::Sell;
                self.setup_count = 1;
            } else {
                self.setup_dir = Side::None;
                self.setup_count = 0;
            }
        } else {
            self.setup_dir = Side::None;
            self.setup_count = 0;
        }

        let completed = if self.setup_count == 9 {
            self.setup_dir
        } else {
            Side::None
        };
        match completed {
            Side::Buy => {
                self.alerts.push(IndicatorAlert::new(
                    "buy_setup",
                    "buy setup of nine completed",
                    1.0,
                ));
            }
            Side::Sell => {
                self.alerts.push(IndicatorAlert::new(
                    "sell_setup",
                    "sell setup of nine completed",
                    1.0,
                ));
            }
            Side::None => {}
        }
        if completed != Side::None && self.cd_dir != completed {
            // A completion of the other side cancels the running countdown; either way a new one
            // starts for the completed side.
            self.cd_dir = completed;
            self.cd_count = 0;
        }

        let low_two_back = self.window[3].1;
        let high_two_back = self.window[3].0;
        match self.cd_dir {
            Side::Buy => {
                if close <= low_two_back {
                    if self.cd_count < 12 {
                        self.cd_count += 1;
                        if self.cd_count == 8 {
                            self.cd_c8 = close;
                        }
                    } else if self.cd_count == 12 && low <= self.cd_c8 {
                        self.cd_count = 13;
                        self.alerts.push(IndicatorAlert::new(
                            "buy_countdown",
                            "buy countdown of thirteen completed",
                            1.0,
                        ));
                    }
                }
            }
            Side::Sell => {
                if close >= high_two_back {
                    if self.cd_count < 12 {
                        self.cd_count += 1;
                        if self.cd_count == 8 {
                            self.cd_c8 = close;
                        }
                    } else if self.cd_count == 12 && high >= self.cd_c8 {
                        self.cd_count = 13;
                        self.alerts.push(IndicatorAlert::new(
                            "sell_countdown",
                            "sell countdown of thirteen completed",
                            1.0,
                        ));
                    }
                }
            }
            Side::None => {}
        }

        let value = match self.setup_dir {
            Side::Buy => -(self.setup_count as f64),
            Side::Sell => self.setup_count as f64,
            Side::None => 0.0,
        };
        let countdown = match self.cd_dir {
            Side::Buy => -(self.cd_count as f64),
            Side::Sell => self.cd_count as f64,
            Side::None => 0.0,
        };
        if self.cd_count == 13 {
            self.cd_dir = Side::None;
            self.cd_count = 0;
        }
        Some(IndicatorOutput::with_extra(
            value,
            HashMap::from([("countdown".to_string(), countdown)]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const SETUP_COUNTDOWN_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "setup_countdown",
        aliases: &[],
        description: "Setup (9) and countdown (13) exhaustion count after DeMark (1994), the core rules only: consecutive closes against the close four bars earlier from a price flip, then thirteen closes against the low or high two bars earlier, the 13th deferred until its low or high passes the close of the 8th count; a completed setup of the other side cancels a running countdown; equal closes break a setup; the later cancellation and recycle refinements are not implemented; value is the signed setup count, extra countdown the signed countdown count",
        params: &[],
        checks: &[],
        facts: super::definition::IndicatorFacts::unitless(),
        verification: super::definition::Verification::Verified,
        build: |_| Ok(Box::new(SetupCountdownEngine::new())),
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, c: f64) -> Bar {
        Bar::new(t, c, c + 0.5, c - 0.5, c, 100.0)
    }

    fn run(closes: &[f64]) -> Vec<Option<IndicatorOutput>> {
        let mut engine = SetupCountdownEngine::new();
        closes
            .iter()
            .enumerate()
            .map(|(i, c)| engine.on_bar(&bar(i as i64, *c)))
            .collect()
    }

    fn at(out: &[Option<IndicatorOutput>], i: usize) -> (f64, f64) {
        let o = out[i].as_ref().unwrap();
        (o.value, o.extra["countdown"])
    }

    /// Five rising closes, then a fall of one per bar: bar 5 flips (setup 1), bar 13 completes the
    /// setup of nine, the countdown then counts to twelve on bar 24, a rebound defers the
    /// thirteenth until bar 28.
    fn buy_series() -> Vec<f64> {
        let mut c = vec![110.0, 111.0, 112.0, 113.0, 114.0];
        c.extend((0..9).map(|k| 109.0 - k as f64)); // bars 5..13: 109 .. 101
        c.extend((0..11).map(|k| 100.0 - k as f64)); // bars 14..24: 100 .. 90
        c.extend([100.0, 100.0, 99.5]); // bars 25..27: rebound, thirteenth deferred
        c.extend([94.0, 94.0]); // bar 28: thirteenth, bar 29: countdown ended
        c
    }

    #[test]
    fn warmup_is_six_bars() {
        let out = run(&buy_series());
        assert!(out[..5].iter().all(Option::is_none));
        assert!(out[5].is_some());
    }

    #[test]
    fn buy_setup_with_flip_counts_to_nine_and_runs_past_it() {
        let out = run(&buy_series());
        for (k, i) in (5..=13).enumerate() {
            assert_eq!(at(&out, i).0, -(k as f64 + 1.0), "bar {i}");
        }
        assert_eq!(at(&out, 14).0, -10.0);
    }

    #[test]
    fn buy_countdown_defers_the_thirteenth_and_ends_after_it() {
        let mut engine = SetupCountdownEngine::new();
        let mut alerts = Vec::new();
        let mut out = Vec::new();
        for (i, c) in buy_series().iter().enumerate() {
            let b = if i == 28 {
                Bar::new(i as i64, *c, *c + 1.0, *c - 0.5, *c, 100.0)
            } else {
                bar(i as i64, *c)
            };
            out.push(engine.on_bar(&b));
            alerts.push(
                engine
                    .alerts()
                    .into_iter()
                    .map(|a| a.kind)
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(alerts[13], vec!["buy_setup"]);
        assert_eq!(at(&out, 13).1, -1.0);
        assert_eq!(at(&out, 20).1, -8.0);
        assert_eq!(at(&out, 24).1, -12.0);
        // Bar 25 and 26 do not qualify; bar 27 qualifies but its low is above the close of bar 20.
        for (i, a) in alerts.iter().enumerate().take(28).skip(25) {
            assert_eq!(at(&out, i).1, -12.0, "bar {i}");
            assert!(a.is_empty(), "bar {i}");
        }
        assert_eq!(at(&out, 28).1, -13.0);
        assert_eq!(alerts[28], vec!["buy_countdown"]);
        assert_eq!(at(&out, 29).1, 0.0);
        assert_eq!(
            alerts.iter().filter(|a| !a.is_empty()).count(),
            2,
            "no other alert"
        );
    }

    #[test]
    fn equal_close_breaks_a_setup_and_no_flip_means_no_restart() {
        // Bars 5..8 count 1..4; bar 9 closes where bar 5 did (equality): setup gone. Bar 10 has the
        // buy relation again, but the previous bar had neither relation, so no flip: still none.
        let out = run(&[
            110.0, 111.0, 112.0, 113.0, 114.0, 109.0, 108.0, 107.0, 106.0, 109.0, 105.0,
        ]);
        assert_eq!(at(&out, 8).0, -4.0);
        assert_eq!(at(&out, 9).0, 0.0);
        assert_eq!(at(&out, 10).0, 0.0);
    }

    #[test]
    fn a_completed_sell_setup_cancels_the_buy_countdown_and_starts_its_own() {
        let mut c = vec![110.0, 111.0, 112.0, 113.0, 114.0];
        c.extend((0..9).map(|k| 109.0 - k as f64)); // buy setup completes on bar 13
        c.extend((0..9).map(|k| 110.0 + k as f64)); // bars 14..22: sell setup 1..9
        let out = run(&c);
        assert_eq!(at(&out, 13), (-9.0, -1.0));
        assert_eq!(at(&out, 14), (1.0, -1.0));
        assert_eq!(at(&out, 21), (8.0, -1.0));
        assert_eq!(at(&out, 22), (9.0, 1.0));
    }

    #[test]
    fn the_sell_countdown_mirrors_the_buy_countdown() {
        let mut engine = SetupCountdownEngine::new();
        let mut kinds = Vec::new();
        let mut out = Vec::new();
        for (i, c) in buy_series().iter().enumerate() {
            // Reflect about 110: close 220 - c, high 220 - low, low 220 - high.
            let (high, low) = if i == 28 {
                (c + 1.0, c - 0.5)
            } else {
                (c + 0.5, c - 0.5)
            };
            out.push(engine.on_bar(&Bar::new(
                i as i64,
                220.0 - c,
                220.0 - low,
                220.0 - high,
                220.0 - c,
                100.0,
            )));
            kinds.push(
                engine
                    .alerts()
                    .into_iter()
                    .map(|a| a.kind)
                    .collect::<Vec<_>>(),
            );
        }
        assert_eq!(at(&out, 13), (9.0, 1.0));
        assert_eq!(kinds[13], vec!["sell_setup"]);
        assert_eq!(at(&out, 27).1, 12.0);
        assert_eq!(at(&out, 28).1, 13.0);
        assert_eq!(kinds[28], vec!["sell_countdown"]);
        assert_eq!(at(&out, 29).1, 0.0);
    }
}
