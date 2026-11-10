//! The point-and-figure chart as a streaming indicator: every bar is fed to a
//! [`PnfChart`] and reports the column the chart is in.
//!
//! A point-and-figure chart has no time axis, so the two views are offered side by side. The
//! chart itself, the columns with their boxes, is the typed structure [`PnfChart`], read through
//! [`PointAndFigureEngine::chart`] or built directly with [`crate::point_and_figure()`]; the
//! horizontal count works on it. The bar output states where the chart stands at each bar: the
//! number of the current column, its direction and its boxes. Because a column only ever grows
//! in its own direction, the last bar that reports a column number carries that column's final
//! extent, so the whole chart can be rebuilt from the bar outputs alone.
//!
//! The construction and its conventions are described in [`mod@crate::point_and_figure`].
//! `method = close` feeds the close of every bar ([`PnfChart::push`]), `method = high_low` its
//! high and low ([`PnfChart::push_high_low`]).
//!
//! # Output
//!
//! `value` is the direction of the current column: 1 for a column of X, -1 for a column of O, 0
//! while no column exists. `extra`:
//!
//! | key | present | meaning |
//! |---|---|---|
//! | `column` | always | number of the current column, counted from 1 since the start of the series; 0 before the first |
//! | `top`, `bottom` | while a column exists | the grid lines of its highest and lowest filled box, in price units |
//! | `boxes` | while a column exists | number of its filled boxes |
//!
//! Alerts: `pnf_new_x_column` and `pnf_new_o_column` on the bar that opens a column. `column`
//! counts from the first bar and therefore depends on where the series starts.

use std::collections::HashMap;

use crate::model::Bar;
use crate::point_and_figure::{PnfChange, PnfChart, PnfDirection};

use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Which prices of a bar build the chart.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PnfMethod {
    /// The close of every bar.
    Close,
    /// High and low of every bar, read in the direction of the current column first.
    HighLow,
}

/// Streaming point-and-figure chart over bars; see the module documentation.
#[derive(Debug, Clone)]
pub struct PointAndFigureEngine {
    chart: PnfChart,
    method: PnfMethod,
    alerts: Vec<IndicatorAlert>,
}

impl PointAndFigureEngine {
    /// # Panics
    ///
    /// If `box_size` is not a positive, finite number ([`PnfChart::new`]).
    pub fn new(box_size: f64, reversal: usize, method: PnfMethod) -> Self {
        Self {
            chart: PnfChart::new(box_size, reversal),
            method,
            alerts: Vec::new(),
        }
    }

    /// The chart built so far.
    pub fn chart(&self) -> &PnfChart {
        &self.chart
    }

    pub fn method(&self) -> PnfMethod {
        self.method
    }
}

impl Indicator for PointAndFigureEngine {
    fn name(&self) -> &str {
        "point_and_figure"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.chart.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let change = match self.method {
            PnfMethod::Close => self.chart.push(bar.close),
            PnfMethod::HighLow => self.chart.push_high_low(bar.high, bar.low),
        };
        let columns = self.chart.columns();
        let mut extra = HashMap::new();
        extra.insert("column".to_string(), columns.len() as f64);
        let Some(current) = columns.last() else {
            return Some(IndicatorOutput::with_extra(0.0, extra));
        };
        extra.insert("top".to_string(), current.top);
        extra.insert("bottom".to_string(), current.bottom);
        extra.insert("boxes".to_string(), current.boxes() as f64);
        if change == PnfChange::NewColumn {
            let kind = match current.direction {
                PnfDirection::Up => "pnf_new_x_column",
                PnfDirection::Down => "pnf_new_o_column",
            };
            self.alerts.push(IndicatorAlert::new(
                kind,
                format!(
                    "Point and figure: column {} of {} from {:.4} to {:.4}",
                    columns.len(),
                    current.direction.symbol(),
                    current.bottom,
                    current.top
                ),
                1.0,
            ));
        }
        Some(IndicatorOutput::with_extra(current.direction.code(), extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const POINT_AND_FIGURE_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "point_and_figure",
        aliases: &["pnf"],
        description: "Point-and-figure chart: columns of X and O from a box size and a reversal in boxes; the current column with its number, direction and boxes on every bar",
        params: &[
            super::definition::ParamSpec::number("box_size", 1.0, 1e-9, 1e9)
                .meaning("price distance of one box; to be set to the price level of the series"),
            super::definition::ParamSpec::count("reversal", 3, 1, 1000)
                .meaning("boxes the price must turn by to open a new column"),
            super::definition::ParamSpec::choice("method", "close", &["close", "high_low"], true)
                .meaning("prices that build the chart: the close, or high and low read in the direction of the column first"),
        ],
        checks: &[],
        build: |p| {
            Ok(Box::new(PointAndFigureEngine::new(
                p.number("box_size"),
                p.count("reversal"),
                match p.choice("method") {
                    "high_low" => PnfMethod::HighLow,
                    _ => PnfMethod::Close,
                },
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(high: f64, low: f64, close: f64) -> Bar {
        Bar::new(0, close, high, low, close, 0.0)
    }

    #[test]
    fn test_reports_the_current_column_on_every_bar() {
        let mut engine = PointAndFigureEngine::new(1.0, 3, PnfMethod::Close);
        let first = engine.on_bar(&bar(101.0, 99.0, 100.0)).unwrap();
        assert_eq!(first.value, 0.0);
        assert_eq!(first.extra.len(), 1);
        assert_eq!(first.extra["column"], 0.0);

        let rise = engine.on_bar(&bar(104.0, 100.0, 103.5)).unwrap();
        assert_eq!(rise.value, 1.0);
        assert_eq!(rise.extra["column"], 1.0);
        assert_eq!((rise.extra["bottom"], rise.extra["top"]), (100.0, 103.0));
        assert_eq!(rise.extra["boxes"], 4.0);
        assert_eq!(engine.alerts()[0].kind, "pnf_new_x_column");

        let quiet = engine.on_bar(&bar(104.0, 101.0, 102.0)).unwrap();
        assert_eq!(quiet.extra, rise.extra);
        assert!(engine.alerts().is_empty());

        let turn = engine.on_bar(&bar(102.0, 99.0, 100.0)).unwrap();
        assert_eq!(turn.value, -1.0);
        assert_eq!(turn.extra["column"], 2.0);
        assert_eq!((turn.extra["bottom"], turn.extra["top"]), (100.0, 102.0));
        assert_eq!(engine.alerts()[0].kind, "pnf_new_o_column");
        assert_eq!(engine.chart().columns().len(), 2);

        engine.reset();
        assert!(engine.chart().columns().is_empty());
        assert_eq!(engine.on_bar(&bar(101.0, 99.0, 100.0)), Some(first));
    }

    #[test]
    fn test_high_low_method_reads_highs_and_lows() {
        let mut engine = PointAndFigureEngine::new(1.0, 3, PnfMethod::HighLow);
        engine.on_bar(&bar(100.5, 99.5, 100.0));
        // The close stays inside the anchor box; the high leaves it.
        let out = engine.on_bar(&bar(103.2, 100.0, 100.4)).unwrap();
        assert_eq!(out.value, 1.0);
        assert_eq!(out.extra["top"], 103.0);
    }
}
