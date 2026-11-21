//! Confirmed fractal highs and lows after Williams (1995), published as levels.

use std::collections::{HashMap, VecDeque};

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Fractal levels. A bar `j` is an up (down) fractal when its high (low) is strictly above
/// (below) the highs (lows) of the `span` bars on each side; it is confirmed `span` bars later, on
/// bar `j + span`, never earlier. `value` is the high of the latest confirmed up fractal, the extra
/// `down_level` the low of the latest confirmed down fractal; nothing is published until both exist.
/// Alerts `up_fractal` and `down_fractal` fire on the confirming bar, only on published bars.
/// [`Indicator::reset`] clears all state.
pub struct WilliamsFractalsEngine {
    span: usize,
    /// The last `2 * span + 1` bars as `(high, low)`.
    window: VecDeque<(f64, f64)>,
    up_level: Option<f64>,
    down_level: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl WilliamsFractalsEngine {
    pub fn new(span: usize) -> Self {
        Self {
            span: span.max(1),
            window: VecDeque::new(),
            up_level: None,
            down_level: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for WilliamsFractalsEngine {
    fn name(&self) -> &str {
        "williams_fractals"
    }

    /// The earliest possible first output: the first confirmation, with bar `2 * span + 1`.
    fn warmup_period(&self) -> usize {
        2 * self.span + 1
    }

    fn reset(&mut self) {
        self.window.clear();
        self.up_level = None;
        self.down_level = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let size = 2 * self.span + 1;
        self.window.push_back((bar.high, bar.low));
        if self.window.len() > size {
            self.window.pop_front();
        }
        if self.window.len() < size {
            return None;
        }
        let (high, low) = self.window[self.span];
        let mut up = false;
        let mut down = false;
        let (mut up_ok, mut down_ok) = (true, true);
        for (k, (h, l)) in self.window.iter().enumerate() {
            if k == self.span {
                continue;
            }
            up_ok &= high > *h;
            down_ok &= low < *l;
        }
        if up_ok {
            self.up_level = Some(high);
            up = true;
        }
        if down_ok {
            self.down_level = Some(low);
            down = true;
        }
        let (Some(up_level), Some(down_level)) = (self.up_level, self.down_level) else {
            return None;
        };
        if up {
            self.alerts.push(IndicatorAlert::new(
                "up_fractal",
                "up fractal confirmed",
                1.0,
            ));
        }
        if down {
            self.alerts.push(IndicatorAlert::new(
                "down_fractal",
                "down fractal confirmed",
                1.0,
            ));
        }
        Some(IndicatorOutput::with_extra(
            up_level,
            HashMap::from([("down_level".to_string(), down_level)]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const WILLIAMS_FRACTALS_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "williams_fractals",
        aliases: &[],
        description: "Confirmed fractal highs and lows after Williams (1995): a bar whose high (low) is strictly above (below) those of span bars on each side, confirmed span bars later; value is the latest up-fractal high, extra down_level the latest down-fractal low, published once both exist",
        params: &[
            super::definition::ParamSpec::count("span", 2, 1, 1000)
                .meaning("bars on each side of a fractal (2 is the five-bar pattern)"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price().range_dependent(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| Ok(Box::new(WilliamsFractalsEngine::new(p.count("span")))),
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, h: f64, l: f64) -> Bar {
        Bar::new(t, l, h, l, h, 100.0)
    }

    type Step = (Option<(f64, f64)>, Vec<String>);

    fn run(span: usize, hl: &[(f64, f64)]) -> Vec<Step> {
        let mut engine = WilliamsFractalsEngine::new(span);
        hl.iter()
            .enumerate()
            .map(|(i, (h, l))| {
                let o = engine
                    .on_bar(&bar(i as i64, *h, *l))
                    .map(|o| (o.value, o.extra["down_level"]));
                let a = engine.alerts().into_iter().map(|a| a.kind).collect();
                (o, a)
            })
            .collect()
    }

    #[test]
    fn five_bar_fractals_are_confirmed_two_bars_late_and_published_with_both_levels() {
        // Down fractal at bar 2 (low 8, confirmed on bar 4); up fractal at bar 5 (high 20,
        // confirmed on bar 7). The up fractal's confirmation is the first bar with both levels.
        let hl = [
            (12.0, 10.0),
            (12.0, 9.0),
            (11.0, 8.0),
            (13.0, 9.5),
            (14.0, 10.0),
            (20.0, 15.0),
            (18.0, 14.0),
            (17.0, 13.0),
            (16.0, 12.0),
        ];
        let out = run(2, &hl);
        assert!(out[..7].iter().all(|(o, _)| o.is_none()));
        assert_eq!(out[7].0, Some((20.0, 8.0)));
        assert_eq!(out[7].1, vec!["up_fractal"]);
        assert_eq!(out[8].0, Some((20.0, 8.0)));
        assert!(out[8].1.is_empty());
    }

    #[test]
    fn equal_highs_do_not_make_a_fractal() {
        // Bar 2 ties the high of bar 3: strict comparison, no up fractal.
        let hl = [
            (10.0, 5.0),
            (11.0, 5.0),
            (12.0, 5.0),
            (12.0, 5.0),
            (10.0, 5.0),
            (9.0, 5.0),
        ];
        let out = run(2, &hl);
        assert!(out.iter().all(|(o, a)| o.is_none() && a.is_empty()));
    }

    #[test]
    fn a_later_fractal_replaces_the_level_and_both_kinds_can_confirm_together() {
        // Span 1. Bar 1 is an outside bar: high and low both beat their neighbours.
        let hl = [
            (10.0, 8.0),
            (12.0, 6.0),
            (9.0, 7.0),
            (11.0, 9.0),
            (10.0, 8.0),
        ];
        let out = run(1, &hl);
        assert_eq!(out[2].0, Some((12.0, 6.0)));
        assert_eq!(out[2].1, vec!["up_fractal", "down_fractal"]);
        // Bar 3 is a new up fractal (11 > 9 and 11 > 10): level moves, down level stays.
        assert_eq!(out[4].0, Some((11.0, 6.0)));
        assert_eq!(out[4].1, vec!["up_fractal"]);
    }
}
