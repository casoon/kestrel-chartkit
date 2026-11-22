//! Trendlines: a support line through confirmed ZigZag lows and a resistance line through
//! confirmed ZigZag highs, each with its channel, its touches and its break.
//!
//! [`super::chart_patterns::ChartPatternDetector::scan`] fits the same two-point line over a
//! finished node list; this is the streaming form, which also says what happens when a line is
//! pierced or broken, and reports what a chart draws in the output.
//!
//! # Calculation
//!
//! The nodes are those of [`AdvancedZigZagEngine`] with the same `depth`, `backstep`, deviation
//! and `atr_len`. Only confirmed nodes count; a node becomes known on the bar that confirms it,
//! which is later than the bar it lies on. The ATR is the Wilder average of the true range over
//! `atr_len` bars, and `tolerance = tolerance_atr · ATR` of the current bar. Nodes confirmed
//! before the ATR exists are not used.
//!
//! For the support line (the resistance line mirrors it: highs for lows, above for below):
//!
//! ```text
//! segment = the confirmed nodes since the last break of this line, beginning with a low
//!
//! on the bar that confirms a low c, with at least one low before it in the segment:
//!     a = the oldest low of the segment for which no low k between a and c lies more than
//!         the tolerance below the line through a and c:   k.price >= line(k.time) - tolerance
//!     the nodes before a leave the segment
//!     line(time)   = a.price + (c.price - a.price) / (c.time - a.time) · (time - a.time)
//!     nodes          = the lows from a to c
//!     touching nodes = those of them with |k.price - line(k.time)| <= tolerance    (a and c included)
//!     node_touches   = their number
//!     touches        = 0
//!
//! on the bar that confirms a low or a high, while a line exists:
//!     width   = the largest high.price - line(high.time) over the highs after a
//!     channel = line + width, if width > 0
//!     channel touching nodes = the highs after a with
//!                              |k.price - (line(k.time) + width)| <= tolerance
//!                              (the high that gives the width included)
//!
//! on every bar from the one that formed the line:
//!     value = line(time of the bar)
//!     touch:  |low - value| <= tolerance          -> touches + 1
//!     channel touch:  |high - (value + width)| <= tolerance
//!     break:  close < value                       -> the line ends with this bar, the segment is emptied
//! ```
//!
//! The line through the oldest and the newest low is the rule of
//! [`super::chart_patterns::ChartPatternDetector`]; where that gives no line because a low in
//! between pierces it, the anchor moves forward here until none does. Two lows always give a line.
//! Every newly confirmed low is the newest anchor, so the line is fixed anew with each of them and
//! `touches` counts the bars since. A touch says that the bar's low came within the tolerance of
//! the line, from either side; whether the bar closed beyond it is the break, tested on its own.
//!
//! The parallel is laid through one node by construction. The channel touching nodes say how many
//! nodes of the other kind lie on it: a channel with two touching nodes on the line and two on
//! the parallel has two touches on each boundary. The tolerance of both lists is that of the bar
//! that computed them: the bar that fixed the line for the line's nodes, the bar of the latest
//! confirmation for the parallel's.
//!
//! The line is straight in time: like [`super::chart_patterns::TrendLine`] it is computed from
//! timestamps. On a series with gaps in time it is therefore not straight over the bar count.
//!
//! The value of the line at a bar is interpolated and so rounded in floating point. Every
//! comparison above therefore treats a price within [`crate::indicator::MARK_TOLERANCE`] ATR of
//! a mark as on it: a node or a low exactly `tolerance` from the line touches it and does not
//! pierce it, a close on the line is no break, and a high on a support line gives no channel.
//!
//! `tolerance_atr` has no source in the literature, which describes a touch by eye; the default
//! of 0.25 is this crate's convention, the same as the default of `zone_rejection`.
//!
//! # Output
//!
//! `value`: the number of lines in this output (0 to 2). `extra`:
//!
//! | key | present | meaning |
//! |---|---|---|
//! | `atr` | always | the ATR of this bar, in price units |
//! | `tolerance` | always | `tolerance_atr · atr` |
//! | `support_value` | while the support line exists, its break bar included | the line at this bar |
//! | `support_start_price`, `support_end_price` | as above | the prices of the anchors `a` and `c` |
//! | `support_start_bars_back`, `support_end_bars_back` | as above | how many bars back the anchors lie |
//! | `support_nodes` | as above | the lows from `a` to `c` |
//! | `support_node_touches` | as above | those within the tolerance of the line, at least 2 |
//! | `support_node_touch_<i>_bars_back`, `support_node_touch_<i>_price` | as above | touching node `i` (from 1, oldest first): how many bars back it lies and its price |
//! | `support_touches` | as above | bars since the line was fixed whose low came within the tolerance |
//! | `support_touch` | as above | 1 if this bar is such a bar, else 0 |
//! | `support_formed` | as above | 1 on the bar that fixed the line (a new line or a new anchor), else 0 |
//! | `support_break` | as above | 1 on the bar whose close lies below the line, else 0 |
//! | `support_channel_value`, `support_channel_width` | while the line has a channel | the parallel at this bar and its distance from the line |
//! | `support_channel_node_touches` | as above | the highs after `a` within the tolerance of the parallel, at least 1 |
//! | `support_channel_node_touch_<i>_bars_back`, `support_channel_node_touch_<i>_price` | as above | channel touching node `i` (from 1, oldest first) |
//! | `support_channel_touch` | as above | 1 if this bar's high came within the tolerance of the parallel, else 0 |
//!
//! The same keys with `resistance_` for the line through the highs; its channel lies below it,
//! `resistance_channel_value = resistance_value - resistance_channel_width`, its channel touching
//! nodes are lows and `resistance_channel_touch` reads the bar's low. A line that breaks is in the
//! output of its break bar and gone from the next one.
//!
//! Each list names at most [`TRENDLINE_LISTED_NODES`] nodes, so the output does not grow with the
//! length of a line: where more nodes touch, the newest are listed, the oldest of those as node 1,
//! and `node_touches` / `channel_node_touches` still count all of them. The oldest touching node
//! of the line is its anchor `a` and stays in `support_start_*`. The lists change only on a bar
//! that confirms a node; between those bars only the `bars_back` values grow.
//!
//! Alerts: `bear_support_line_break` on a close below the support line,
//! `bull_resistance_line_break` on a close above the resistance line (strength 0.6).
//!
//! First output with the first ATR, on bar `atr_len`. A series and its mirror image (every price
//! negated, highs and lows exchanged) give the mirrored output: support for resistance, every
//! price negated.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::chart_patterns::TrendLine;
use super::kernels::{true_range, Marks};
use super::smoothing::Rma;
use super::zigzag_advanced::{AdvancedZigZagEngine, ZigZagDeviationMode, ZigZagNode};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// A confirmed ZigZag node with the index of the bar it lies on.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrendlineNode {
    pub timestamp: i64,
    pub price: f64,
    pub is_high: bool,
    /// 0-based count of the bar since the engine started (or was reset).
    pub bar: usize,
}

/// How many touching nodes of a line, and of its channel parallel, the output lists at most (the
/// newest ones). [`Trendline::touching_nodes`] and [`Trendline::channel_touching_nodes`] hold all.
pub const TRENDLINE_LISTED_NODES: usize = 8;

/// One line as the engine holds it after a bar.
#[derive(Debug, Clone, PartialEq)]
pub struct Trendline {
    /// `true` for the line through the lows, `false` for the line through the highs.
    pub is_support: bool,
    /// The oldest anchor `a`.
    pub start: TrendlineNode,
    /// The newest anchor `c`.
    pub end: TrendlineNode,
    /// The nodes of the line's kind from `start` to `end`.
    pub nodes: usize,
    /// Those of them within the tolerance of the line, the anchors included.
    pub node_touches: usize,
    /// The nodes counted in `node_touches`, oldest first: `start` is the first, `end` the last.
    pub touching_nodes: Vec<TrendlineNode>,
    /// Bars since the line was fixed whose low (support) or high (resistance) came within the
    /// tolerance.
    pub touches: usize,
    /// Distance of the channel parallel from the line, if a node of the other kind lies on the
    /// far side of the line.
    pub channel_width: Option<f64>,
    /// The nodes of the other kind after `start` within the tolerance of the channel parallel,
    /// oldest first, the node that gives the width included. Empty without a channel.
    pub channel_touching_nodes: Vec<TrendlineNode>,
}

impl Trendline {
    /// The line through the two anchors.
    pub fn line(&self) -> TrendLine {
        TrendLine {
            start: (self.start.timestamp, self.start.price),
            end: (self.end.timestamp, self.end.price),
        }
    }

    /// The channel parallel at `timestamp`: above a support line, below a resistance line.
    pub fn channel_value_at(&self, timestamp: i64) -> Option<f64> {
        let value = self.line().value_at(timestamp);
        self.channel_width.map(|width| {
            if self.is_support {
                value + width
            } else {
                value - width
            }
        })
    }
}

/// One side: the nodes since the last break and the line through those of its kind.
#[derive(Debug, Clone)]
struct Side {
    is_support: bool,
    /// Confirmed nodes since the last break, beginning with a node of this side's kind.
    segment: Vec<TrendlineNode>,
    line: Option<Trendline>,
}

impl Side {
    fn new(is_support: bool) -> Self {
        Self {
            is_support,
            segment: Vec::new(),
            line: None,
        }
    }

    /// How far `price` lies on the far side of `value`: above a support line, below a
    /// resistance line. Negative on the near side.
    fn beyond(&self, price: f64, value: f64) -> f64 {
        if self.is_support {
            price - value
        } else {
            value - price
        }
    }

    /// Takes a newly confirmed node. Returns whether it fixed the line anew.
    fn add(&mut self, node: TrendlineNode, tolerance: f64, marks: Marks) -> bool {
        let own_kind = node.is_high != self.is_support;
        if !own_kind {
            if !self.segment.is_empty() {
                self.segment.push(node);
                self.update_channel(tolerance, marks);
            }
            return false;
        }
        self.segment.push(node);
        let own: Vec<usize> = (0..self.segment.len())
            .filter(|&i| self.segment[i].is_high == node.is_high)
            .collect();
        if own.len() < 2 {
            return false;
        }
        let through = |a: &TrendlineNode| TrendLine {
            start: (a.timestamp, a.price),
            end: (node.timestamp, node.price),
        };
        // The oldest anchor whose line no node of the kind in between pierces. The last
        // candidate has none in between, so one is always found.
        let between = &own[..own.len() - 1];
        let first = (0..between.len())
            .find(|&i| {
                let line = through(&self.segment[between[i]]);
                between[i + 1..].iter().all(|&k| {
                    let k = &self.segment[k];
                    marks.at_or_above(self.beyond(k.price, line.value_at(k.timestamp)), -tolerance)
                })
            })
            .expect("the newest two nodes of the kind have none between them");
        self.segment.drain(..between[first]);
        let start = self.segment[0];
        let line = through(&start);
        let kind: Vec<&TrendlineNode> = self
            .segment
            .iter()
            .filter(|k| k.is_high == node.is_high)
            .collect();
        let touching_nodes: Vec<TrendlineNode> = kind
            .iter()
            .filter(|k| marks.within((k.price - line.value_at(k.timestamp)).abs(), tolerance))
            .map(|k| **k)
            .collect();
        self.line = Some(Trendline {
            is_support: self.is_support,
            start,
            end: node,
            nodes: kind.len(),
            node_touches: touching_nodes.len(),
            touching_nodes,
            touches: 0,
            channel_width: None,
            channel_touching_nodes: Vec::new(),
        });
        self.update_channel(tolerance, marks);
        true
    }

    /// The parallel through the node of the other kind that lies furthest beyond the line, and
    /// the nodes of that kind within the tolerance of it.
    fn update_channel(&mut self, tolerance: f64, marks: Marks) {
        let Some(current) = &self.line else {
            return;
        };
        let line = current.line();
        let kind_is_high = !self.is_support;
        let others: Vec<(TrendlineNode, f64)> = self
            .segment
            .iter()
            .filter(|k| k.is_high != kind_is_high)
            .map(|k| (*k, self.beyond(k.price, line.value_at(k.timestamp))))
            .collect();
        let width = others
            .iter()
            .map(|(_, beyond)| *beyond)
            .fold(f64::NEG_INFINITY, f64::max);
        let channel_width = marks.above(width, 0.0).then_some(width);
        let channel_touching_nodes = match channel_width {
            Some(width) => others
                .iter()
                .filter(|(_, beyond)| marks.within((beyond - width).abs(), tolerance))
                .map(|(k, _)| *k)
                .collect(),
            None => Vec::new(),
        };
        if let Some(current) = &mut self.line {
            current.channel_width = channel_width;
            current.channel_touching_nodes = channel_touching_nodes;
        }
    }
}

/// Streaming trendlines; see the module documentation.
pub struct TrendlineEngine {
    zigzag: AdvancedZigZagEngine,
    depth: usize,
    tolerance_atr: f64,
    atr: Rma,
    atr_len: usize,
    prev_close: Option<f64>,
    bar_index: usize,
    /// Timestamps of the last `depth + 1` bars: the front one is the ZigZag's middle bar.
    recent: VecDeque<i64>,
    /// Bar index of the ZigZag's running node.
    running_bar: usize,
    support: Side,
    resistance: Side,
    alerts: Vec<IndicatorAlert>,
}

impl TrendlineEngine {
    pub fn new(
        depth: usize,
        backstep: usize,
        deviation: ZigZagDeviationMode,
        atr_len: usize,
        tolerance_atr: f64,
    ) -> Self {
        let depth = depth.max(1);
        let atr_len = atr_len.max(1);
        Self {
            zigzag: AdvancedZigZagEngine::new(depth, backstep, deviation, atr_len),
            depth,
            tolerance_atr: tolerance_atr.max(0.0),
            atr: Rma::new(atr_len),
            atr_len,
            prev_close: None,
            bar_index: 0,
            recent: VecDeque::with_capacity(depth + 2),
            running_bar: 0,
            support: Side::new(true),
            resistance: Side::new(false),
            alerts: Vec::new(),
        }
    }

    /// The ZigZag defaults (depth 3, backstep 2, 1 %, ATR 14) with a tolerance of 0.25 ATR.
    pub fn with_defaults() -> Self {
        Self::new(3, 2, ZigZagDeviationMode::Percent(1.0), 14, 0.25)
    }

    /// The line through the lows as of the last bar. A line broken on that bar is gone.
    pub fn support(&self) -> Option<&Trendline> {
        self.support.line.as_ref()
    }

    /// The line through the highs as of the last bar. A line broken on that bar is gone.
    pub fn resistance(&self) -> Option<&Trendline> {
        self.resistance.line.as_ref()
    }

    /// The nodes of the underlying ZigZag, the running one included.
    pub fn nodes(&self) -> &[ZigZagNode] {
        self.zigzag.nodes()
    }
}

impl Indicator for TrendlineEngine {
    fn name(&self) -> &str {
        "trendlines"
    }

    fn warmup_period(&self) -> usize {
        self.atr_len
    }

    fn reset(&mut self) {
        self.zigzag.reset();
        self.atr.reset();
        self.prev_close = None;
        self.bar_index = 0;
        self.recent.clear();
        self.running_bar = 0;
        self.support = Side::new(true);
        self.resistance = Side::new(false);
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let t = self.bar_index;
        self.bar_index += 1;

        let tr = true_range(bar, self.prev_close);
        self.prev_close = Some(bar.close);
        let atr = self.atr.update(tr);

        self.recent.push_back(bar.timestamp);
        if self.recent.len() > self.depth + 1 {
            self.recent.pop_front();
        }
        // The ZigZag's middle bar: a node set or moved on this bar lies there.
        let middle =
            (self.recent.len() == self.depth + 1).then(|| (self.recent[0], t - self.depth));

        // Every node but the newest is confirmed.
        let confirmed_before = self.zigzag.nodes().len().saturating_sub(1);
        self.zigzag.on_bar(bar);
        let nodes = self.zigzag.nodes();
        let bar_of = |node: &ZigZagNode, running_bar: usize| match middle {
            Some((timestamp, index)) if node.timestamp == timestamp => index,
            _ => running_bar,
        };
        let newly: Vec<TrendlineNode> = nodes[confirmed_before..nodes.len().saturating_sub(1)]
            .iter()
            .map(|n| TrendlineNode {
                timestamp: n.timestamp,
                price: n.price,
                is_high: n.is_high,
                bar: bar_of(n, self.running_bar),
            })
            .collect();
        if let Some(running) = nodes.last() {
            self.running_bar = bar_of(running, self.running_bar);
        }

        let atr = atr?;
        let tolerance = self.tolerance_atr * atr;
        let marks = Marks::new(atr);

        let mut extra = HashMap::new();
        extra.insert("atr".to_string(), atr);
        extra.insert("tolerance".to_string(), tolerance);
        let mut lines = 0;
        for side in [&mut self.support, &mut self.resistance] {
            let mut formed = false;
            for node in &newly {
                formed |= side.add(*node, tolerance, marks);
            }
            let Some(current) = side.line.as_mut() else {
                continue;
            };
            let value = current.line().value_at(bar.timestamp);
            let (extreme, broke) = if side.is_support {
                (bar.low, marks.below(bar.close, value))
            } else {
                (bar.high, marks.above(bar.close, value))
            };
            let touch = marks.within((extreme - value).abs(), tolerance);
            if touch {
                current.touches += 1;
            }

            let name = if side.is_support {
                "support"
            } else {
                "resistance"
            };
            let flag = |on: bool| if on { 1.0 } else { 0.0 };
            for (key, number) in [
                ("value", value),
                ("start_price", current.start.price),
                ("start_bars_back", (t - current.start.bar) as f64),
                ("end_price", current.end.price),
                ("end_bars_back", (t - current.end.bar) as f64),
                ("nodes", current.nodes as f64),
                ("node_touches", current.node_touches as f64),
                ("touches", current.touches as f64),
                ("touch", flag(touch)),
                ("formed", flag(formed)),
                ("break", flag(broke)),
            ] {
                extra.insert(format!("{name}_{key}"), number);
            }
            let mut list = |prefix: &str, nodes: &[TrendlineNode]| {
                let listed = &nodes[nodes.len().saturating_sub(TRENDLINE_LISTED_NODES)..];
                for (i, node) in listed.iter().enumerate() {
                    let key = format!("{name}_{prefix}_{}", i + 1);
                    extra.insert(format!("{key}_bars_back"), (t - node.bar) as f64);
                    extra.insert(format!("{key}_price"), node.price);
                }
            };
            list("node_touch", &current.touching_nodes);
            if let Some(width) = current.channel_width {
                list("channel_node_touch", &current.channel_touching_nodes);
                // The parallel lies on the far side: the bar reaches it with its other extreme.
                let (parallel, far_extreme) = if side.is_support {
                    (value + width, bar.high)
                } else {
                    (value - width, bar.low)
                };
                let channel_touch = marks.within((far_extreme - parallel).abs(), tolerance);
                extra.insert(format!("{name}_channel_value"), parallel);
                extra.insert(format!("{name}_channel_width"), width);
                extra.insert(
                    format!("{name}_channel_node_touches"),
                    current.channel_touching_nodes.len() as f64,
                );
                extra.insert(format!("{name}_channel_touch"), flag(channel_touch));
            }
            lines += 1;

            if broke {
                self.alerts.push(IndicatorAlert::new(
                    if side.is_support {
                        "bear_support_line_break"
                    } else {
                        "bull_resistance_line_break"
                    },
                    if side.is_support {
                        "Close below the support line"
                    } else {
                        "Close above the resistance line"
                    },
                    0.6,
                ));
                side.line = None;
                side.segment.clear();
            }
        }
        Some(IndicatorOutput::with_extra(lines as f64, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Registry definition of `trendlines`. The first five parameters are those of
/// `zigzag_advanced` and name the same nodes.
pub const TRENDLINES_DEFINITION: super::definition::IndicatorDefinition = super::definition::IndicatorDefinition {
    name: "trendlines",
    aliases: &[],
    description: "Trendlines through confirmed zigzag_advanced nodes: a support line through the lows and a resistance line through the highs (oldest anchor no node in between pierces by more than tolerance_atr ATR, newest node), each with its channel parallel, node and bar touches and its break on a close beyond it; value = lines in the output; extra: atr, tolerance, support_*/resistance_* (value, start_price, start_bars_back, end_price, end_bars_back, nodes, node_touches, touches, touch, formed, break, channel_value, channel_width), the touching nodes as node_touch_<i>_bars_back / node_touch_<i>_price, and for the channel parallel channel_node_touches (nodes of the other kind within the tolerance of it), channel_node_touch_<i>_bars_back / _price and channel_touch (this bar's far extreme within the tolerance of it); each list names the newest 8 at most, oldest first; alerts bear_support_line_break, bull_resistance_line_break",
    params: &[
        super::definition::ParamSpec::count("depth", 3, 1, 10000).meaning("bars on each side of a pivot"),
        super::definition::ParamSpec::count("backstep", 2, 0, 10000)
            .meaning("bars within which a newer pivot replaces an older one"),
        super::definition::ParamSpec::choice("deviation_mode", "percent", &["percent", "atr_multiple"], true)
            .meaning("unit of the deviation: percent of price or multiples of the ATR"),
        super::definition::ParamSpec::number("deviation_pct", 1.0, 0.001, 1000.0)
            .range_by(
                "deviation_mode",
                &[("percent", 0.001, 100.0), ("atr_multiple", 0.001, 1000.0)],
            )
            .meaning("minimum move for a new leg, in the unit of deviation_mode"),
        super::definition::ParamSpec::count("atr_len", 14, 1, 10000).meaning("bars in the ATR"),
        super::definition::ParamSpec::number("tolerance_atr", 0.25, 0.0, 100.0)
            .meaning("distance from a line, in ATR, within which a node or a bar touches it and beyond which a node pierces it"),
    ],
    checks: &[],
    facts: super::definition::IndicatorFacts::unitless()
        .session_sensitive()
        .roll_sensitive()
        .adjustment_sensitive()
        .range_dependent(),
    verification: super::definition::Verification::Verified,
    build: |p| {
        let value = p.number("deviation_pct");
        let deviation = match p.choice("deviation_mode") {
            "atr_multiple" => ZigZagDeviationMode::AtrMultiple(value),
            _ => ZigZagDeviationMode::Percent(value),
        };
        Ok(Box::new(TrendlineEngine::new(
            p.count("depth"),
            p.count("backstep"),
            deviation,
            p.count("atr_len"),
            p.number("tolerance_atr"),
        )))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Lows at 100 (bar 5), 103.5 (bar 15) and 108 (bar 25), highs at 108, 112 and 116 in
    /// between, then a decline. High and low lie 0.5 beyond the close (or at the open).
    fn bars() -> Vec<Bar> {
        let closes = [
            104.0, 103.3, 102.6, 101.9, 101.2, 100.5, 102.0, 103.5, 105.0, 106.4, 107.5, 106.6,
            105.7, 104.9, 104.4, 104.0, 105.6, 107.2, 108.9, 110.4, 111.5, 110.7, 109.9, 109.2,
            108.8, 108.5, 110.0, 111.6, 113.1, 114.5, 115.5, 115.0, 114.0, 113.1, 112.3, 111.4,
            110.8, 110.3, 110.6, 110.2,
        ];
        let mut previous = closes[0];
        closes
            .iter()
            .enumerate()
            .map(|(i, &close)| {
                let open = previous;
                previous = close;
                Bar::new(
                    i as i64 * 60,
                    open,
                    (close + 0.5).max(open),
                    (close - 0.5).min(open),
                    close,
                    1000.0,
                )
            })
            .collect()
    }

    fn run(tolerance_atr: f64) -> (TrendlineEngine, Vec<Option<IndicatorOutput>>, Vec<usize>) {
        let mut engine =
            TrendlineEngine::new(3, 2, ZigZagDeviationMode::Percent(1.0), 14, tolerance_atr);
        let mut breaks = Vec::new();
        let outputs = bars()
            .iter()
            .enumerate()
            .map(|(i, bar)| {
                let out = engine.on_bar(bar);
                if engine
                    .alerts()
                    .iter()
                    .any(|a| a.kind == "bear_support_line_break")
                {
                    breaks.push(i);
                }
                out
            })
            .collect();
        (engine, outputs, breaks)
    }

    #[test]
    fn test_line_through_the_oldest_and_the_newest_low() {
        // Tolerance 0.5 ATR, about 0.76: the middle low, 0.5 below the line, does not pierce it.
        let (_, outputs, breaks) = run(0.5);
        assert!(outputs[12].is_none() && outputs[13].is_some(), "first ATR");

        // Bar 23 confirms the low of bar 15: the line 100 -> 103.5, 0.35 a bar.
        let formed = outputs[23].as_ref().unwrap();
        assert_eq!(formed.extra["support_formed"], 1.0);
        assert!((formed.extra["support_value"] - 106.3).abs() < 1e-9);
        assert!(!outputs[22]
            .as_ref()
            .unwrap()
            .extra
            .contains_key("support_value"));

        // Bar 33 confirms the low of bar 25: the line 100 -> 108, 0.4 a bar, three nodes on it.
        let out = outputs[33].as_ref().unwrap();
        assert_eq!(out.extra["support_formed"], 1.0);
        assert_eq!(out.extra["support_start_price"], 100.0);
        assert_eq!(out.extra["support_start_bars_back"], 28.0);
        assert_eq!(out.extra["support_end_price"], 108.0);
        assert_eq!(out.extra["support_end_bars_back"], 8.0);
        assert_eq!(out.extra["support_nodes"], 3.0);
        assert_eq!(out.extra["support_node_touches"], 3.0);
        assert!((out.extra["support_value"] - 111.2).abs() < 1e-9);
        // The channel runs through the high furthest above the line: 116 at bar 30 against 110.
        assert!((out.extra["support_channel_width"] - 6.0).abs() < 1e-9);
        assert!((out.extra["support_channel_value"] - 117.2).abs() < 1e-9);

        // Bar 35: the close 111.4 lies below the line at 112.0.
        assert_eq!(breaks, [35]);
        let broken = outputs[35].as_ref().unwrap();
        assert_eq!(broken.extra["support_break"], 1.0);
        assert!((broken.extra["support_value"] - 112.0).abs() < 1e-9);
        assert_eq!(outputs[34].as_ref().unwrap().extra["support_break"], 0.0);
        assert!(!outputs[36]
            .as_ref()
            .unwrap()
            .extra
            .contains_key("support_value"));
    }

    #[test]
    fn test_a_pierced_line_moves_its_anchor() {
        // Tolerance 0.25 ATR, about 0.38: the middle low pierces the line 100 -> 108 by 0.5, so
        // the line starts at that low instead.
        let (engine, outputs, breaks) = run(0.25);
        let out = outputs[33].as_ref().unwrap();
        assert_eq!(out.extra["support_start_price"], 103.5);
        assert_eq!(out.extra["support_nodes"], 2.0);
        assert!((out.extra["support_value"] - 111.6).abs() < 1e-9);
        // Bar 34: the low 111.8 comes within the tolerance of the line at 112.05.
        let touched = outputs[34].as_ref().unwrap();
        assert_eq!(touched.extra["support_touch"], 1.0);
        assert_eq!(touched.extra["support_touches"], 1.0);
        assert_eq!(breaks, [35]);
        assert!(engine.support().is_none(), "the broken line is gone");

        // The highs 108, 112, 116 are known as a line once the third is confirmed; until then
        // the resistance line runs through the first two.
        let resistance = engine.resistance().expect("line through the highs");
        assert!(!resistance.is_support);
        assert_eq!(
            (resistance.start.price, resistance.end.price),
            (108.0, 112.0)
        );
        assert_eq!((resistance.start.bar, resistance.end.bar), (10, 20));
        assert!((resistance.line().value_at(30 * 60) - 116.0).abs() < 1e-9);
    }

    #[test]
    fn test_the_line_names_its_touching_nodes_and_those_of_the_parallel() {
        // Tolerance 0.5 ATR: after bar 33 the line runs 100 -> 108 through three lows, and the
        // confirmed highs 108 and 112 both lie 6.0 above it, on the parallel (the high 116 of
        // bar 30 is not confirmed yet).
        let mut engine = TrendlineEngine::new(3, 2, ZigZagDeviationMode::Percent(1.0), 14, 0.5);
        let outputs: Vec<_> = bars()[..=33].iter().map(|bar| engine.on_bar(bar)).collect();
        let line = engine.support().expect("the line through the lows");
        let at = |nodes: &[TrendlineNode]| -> Vec<(usize, f64)> {
            nodes.iter().map(|node| (node.bar, node.price)).collect()
        };
        assert_eq!(
            at(&line.touching_nodes),
            [(5, 100.0), (15, 103.5), (25, 108.0)]
        );
        assert_eq!(line.node_touches, line.touching_nodes.len());
        assert_eq!(line.touching_nodes.first(), Some(&line.start));
        assert_eq!(line.touching_nodes.last(), Some(&line.end));
        assert!(line.touching_nodes.iter().all(|node| !node.is_high));
        assert_eq!(at(&line.channel_touching_nodes), [(10, 108.0), (20, 112.0)]);
        assert!(line.channel_touching_nodes.iter().all(|node| node.is_high));

        let out = outputs[33].as_ref().unwrap();
        for (i, (bars_back, price)) in [(28.0, 100.0), (18.0, 103.5), (8.0, 108.0)]
            .into_iter()
            .enumerate()
        {
            let key = format!("support_node_touch_{}", i + 1);
            assert_eq!(out.extra[&format!("{key}_bars_back")], bars_back);
            assert_eq!(out.extra[&format!("{key}_price")], price);
        }
        assert!(!out.extra.contains_key("support_node_touch_4_price"));
        assert_eq!(out.extra["support_channel_node_touches"], 2.0);
        assert_eq!(out.extra["support_channel_node_touch_1_bars_back"], 23.0);
        assert_eq!(out.extra["support_channel_node_touch_1_price"], 108.0);
        assert_eq!(out.extra["support_channel_node_touch_2_bars_back"], 13.0);
        assert_eq!(out.extra["support_channel_node_touch_2_price"], 112.0);
        assert!(!out.extra.contains_key("support_channel_node_touch_3_price"));
        // The high of bar 33 (113.6) is far below the parallel at 117.2.
        assert_eq!(out.extra["support_channel_touch"], 0.0);

        // With the narrower tolerance the line starts at the middle low (bar 15): two touching
        // nodes, and the high 108 of bar 10 lies before the anchor. The one confirmed high after
        // it, 112 at bar 20, gives the width: 6.25 above the line 103.5 -> 108 (105.75 there).
        let mut narrow = TrendlineEngine::new(3, 2, ZigZagDeviationMode::Percent(1.0), 14, 0.25);
        for bar in &bars()[..=33] {
            narrow.on_bar(bar);
        }
        let line = narrow.support().expect("the line through the lows");
        assert_eq!(at(&line.touching_nodes), [(15, 103.5), (25, 108.0)]);
        assert_eq!(at(&line.channel_touching_nodes), [(20, 112.0)]);
        assert!((line.channel_width.unwrap() - 6.25).abs() < 1e-9);
    }

    #[test]
    fn test_nothing_before_the_atr_and_reset_restarts() {
        let (mut engine, outputs, _) = run(0.25);
        engine.reset();
        let again: Vec<_> = bars().iter().map(|bar| engine.on_bar(bar)).collect();
        assert_eq!(outputs, again);
        assert_eq!(engine.warmup_period(), 14);
    }
}
