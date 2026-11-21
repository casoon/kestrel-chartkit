//! Chart patterns and harmonic patterns as per-bar events (catalog entries `chart_patterns` and
//! `harmonic_patterns`).
//!
//! [`ChartPatternDetector`] and [`HarmonicDetector`] scan a finished list of ZigZag nodes. Here
//! the nodes arrive the way a live feed delivers them: an [`AdvancedZigZagEngine`] runs over the
//! bars, and each time it confirms a node (the swing is fixed once price has reversed past the
//! deviation), the detector looks at the newest confirmed nodes. The running, unconfirmed leg is
//! never an input, so what has been reported never changes later.
//!
//! # One event per pattern instance
//!
//! A pattern instance is a pattern kind and its anchor nodes. It is found once: on the bar that
//! confirms its last anchor node, the only time a window of nodes ends there. From then on the
//! detector's own lifecycle rule ([`ChartPatternCandidate::update_state`] and
//! [`HarmonicCandidate::update_state`]) follows it bar by bar, and the bar on which that rule
//! reports `Confirmed` raises the alert, once. The bars after the last anchor node up to the bar
//! that confirms it are replayed through the rule first, so a close through the neckline during
//! the swing that confirmed the node counts: the alert is raised on the confirming bar, the
//! earliest bar a live feed can know it, and the levels in `extra` are those of the bar on which
//! the rule held. An instance that the rule invalidates raises nothing.
//!
//! The ranking of the batch scan does not take part: overlapping instances of one kind (a triangle
//! over nodes 1–4 and another over 2–5) are followed independently and each that confirms raises
//! its own alert. An instance that has neither confirmed nor been invalidated when `pending_swings`
//! further nodes have been confirmed behind its last anchor expires without an alert.
//!
//! # Output
//!
//! `value` is the direction of the most recently confirmed instance (`1` bullish, `-1` bearish,
//! `0` while none has confirmed); the last direction is carried forward on every later bar until
//! the next instance confirms. `extra` has `active` (instances followed after the bar) and
//! `confirmed` (instances confirmed on this bar). On a bar that confirms instances one alert per
//! instance is raised, in the order the instances were found, and the further `extra` keys
//! describe the first of them:
//!
//! | Key | Meaning |
//! |---|---|
//! | `pattern` | Kind code (below). |
//! | `level` | The level the close went through: the broken boundary or neckline (chart patterns; the 1-2-3's third node). |
//! | `invalidation` | The level beyond which the rule gives the instance up: the pattern's extreme (reversal family), the second node (1-2-3), point 5 (Wolfe wave), the opposite channel line (flag), the handle's extreme (cup and handle and its inverse); for harmonics the far edge of the PRZ. |
//! | `target` | Wolfe wave only: the line through points 1 and 4. |
//! | `fit` | Chart patterns: the detector's `0..1` geometric fit. |
//! | `prz_low`, `prz_high`, `d` | Harmonics: the PRZ and the price of point D. |
//!
//! Kind codes of `chart_patterns`: 1 triangle, 2 ascending triangle, 3 descending triangle,
//! 4 broadening, 5 rising wedge, 6 falling wedge, 7 one-two-three reversal, 8 Wolfe wave,
//! 9 double top, 10 double bottom, 11 triple top, 12 triple bottom, 13 head and shoulders,
//! 14 inverse head and shoulders, 15 flag, 16 cup and handle, 17 inverse cup and handle. The
//! auto-fitted trendlines are the `trendlines` entry's.
//! Kind codes of `harmonic_patterns`: the 1-based row of [`HARMONIC_TABLE`], 1 gartley, 2 bat,
//! 3 butterfly, 4 crab, 5 deep crab, 6 shark.
//!
//! Alerts are `bull_<kind>` and `bear_<kind>` with the kind spelled as in the code table
//! (`bear_double_top`, `bull_inverse_head_and_shoulders`, `bull_gartley`). For the triangle
//! family, the wedges and the broadening formation the direction is the side the close broke out
//! on: bullish only above both boundaries, bearish only below both (past the apex, where the lines
//! have crossed, a close between them is no breakout); for the 1-2-3 and the Wolfe wave it follows the
//! swing structure, for the reversal family the pattern (top: bearish), for the flag the pole's
//! direction, for cup and handle bullish and for its inverse bearish, and for harmonics the pattern's resolution at D.
//!
//! # Levels and zones
//!
//! The level tolerance of `chart_patterns` (`tolerance_pct`) is a percentage of a node's price,
//! so the allowed distance grows with the price level and the patterns are not shift invariant:
//! adding a constant to every price changes which swings count as one level. The cup and handle
//! rule compares durations in bars, so gaps in the timestamps do not change a verdict.
//!
//! `harmonic_patterns` measures D against XA for every row but the shark, which measures it as a
//! retracement of XC. The reversal zone always contains the projection of the row's D interval,
//! widened by `ratio_tolerance`, so a D that passes the ratio test never lies outside its own
//! zone; the BC and AB=CD projections widen it where they fall outside.
//!
//! The ZigZag parameters are those of `zigzag_advanced`, with the same defaults.

use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::chart_patterns::{
    ChartPatternCandidate, ChartPatternDetector, ChartPatternKind, PatternState, TrendLine,
};
use super::definition::{IndicatorDefinition, ParamSpec, ResolvedParams};
use super::harmonics::{
    HarmonicCandidate, HarmonicConfig, HarmonicDetector, HarmonicState, HARMONIC_TABLE,
};
use super::zigzag_advanced::{AdvancedZigZagEngine, ZigZagDeviationMode, ZigZagNode};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Nodes the detectors look at when a node confirms: the longest pattern has five anchors.
const WINDOW: usize = 5;

/// A ZigZag over the bars, reporting each node as it confirms, with the bars since the swing
/// that is still running.
struct SwingFeed {
    zigzag: AdvancedZigZagEngine,
    depth: usize,
    bars: VecDeque<Bar>,
    seen: usize,
    /// Bars fed so far.
    fed: usize,
    /// Bar index of each of the newest [`WINDOW`] confirmed nodes.
    node_bars: VecDeque<i64>,
}

/// A node that has just confirmed: the newest [`WINDOW`] confirmed nodes ending at it.
struct Confirmed {
    tail: Vec<ZigZagNode>,
    /// The bar index of each node of `tail`.
    clock: Vec<i64>,
}

impl SwingFeed {
    fn new(depth: usize, backstep: usize, deviation: ZigZagDeviationMode, atr_len: usize) -> Self {
        Self {
            zigzag: AdvancedZigZagEngine::new(depth, backstep, deviation, atr_len),
            depth,
            bars: VecDeque::new(),
            seen: 0,
            fed: 0,
            node_bars: VecDeque::new(),
        }
    }

    /// Feeds one bar. `None` before the ZigZag has its first full window of `2 · depth + 1` bars.
    fn push(&mut self, bar: &Bar) -> Option<Vec<Confirmed>> {
        self.zigzag.on_bar(bar);
        self.fed += 1;
        let ready = self.fed > 2 * self.depth;
        self.bars.push_back(bar.clone());
        let first_bar = self.fed - self.bars.len();
        let nodes = self.zigzag.nodes();
        let confirmed = nodes.iter().take_while(|n| n.confirmed).count();
        let new: Vec<Confirmed> = (self.seen..confirmed)
            .map(|index| {
                // A node is confirmed while its bar is still kept (see `keep_from` below).
                let at = self
                    .bars
                    .iter()
                    .position(|b| b.timestamp == nodes[index].timestamp)
                    .expect("the bar of a node that has just confirmed is still kept");
                self.node_bars.push_back((first_bar + at) as i64);
                if self.node_bars.len() > WINDOW {
                    self.node_bars.pop_front();
                }
                Confirmed {
                    tail: nodes[(index + 1).saturating_sub(WINDOW)..=index].to_vec(),
                    clock: self.node_bars.iter().copied().collect(),
                }
            })
            .collect();
        self.seen = confirmed;

        // The next node to confirm is the running one, and its replay needs the bars after it.
        let keep_from = match (confirmed.checked_sub(1), nodes.get(confirmed)) {
            (Some(last), Some(running)) => Some(nodes[last].timestamp.min(running.timestamp)),
            (Some(last), None) => Some(nodes[last].timestamp),
            (None, Some(running)) => Some(running.timestamp),
            (None, None) => None,
        };
        match keep_from {
            Some(from) => {
                while self.bars.front().is_some_and(|b| b.timestamp < from) {
                    self.bars.pop_front();
                }
            }
            None => {
                while self.bars.len() > self.depth + 1 {
                    self.bars.pop_front();
                }
            }
        }
        ready.then_some(new)
    }

    /// The bars after `timestamp`, up to and including the current one.
    fn bars_after(&self, timestamp: i64) -> Vec<Bar> {
        self.bars
            .iter()
            .filter(|b| b.timestamp > timestamp)
            .cloned()
            .collect()
    }

    fn reset(&mut self) {
        self.zigzag.reset();
        self.bars.clear();
        self.seen = 0;
        self.fed = 0;
        self.node_bars.clear();
    }
}

/// One instance that confirmed on the current bar.
struct Event {
    alert: IndicatorAlert,
    direction: f64,
    extra: Vec<(&'static str, f64)>,
}

fn alert(bullish: bool, base: &str) -> IndicatorAlert {
    IndicatorAlert::new(
        format!("{}_{base}", if bullish { "bull" } else { "bear" }),
        format!("{} · CONFIRMED", base.replace('_', " ").to_uppercase()),
        1.0,
    )
}

fn output(value: f64, active: usize, events: &[Event]) -> (IndicatorOutput, Vec<IndicatorAlert>) {
    let mut extra = HashMap::new();
    extra.insert("active".to_string(), active as f64);
    extra.insert("confirmed".to_string(), events.len() as f64);
    if let Some(first) = events.first() {
        for (key, v) in &first.extra {
            extra.insert((*key).to_string(), *v);
        }
    }
    (
        IndicatorOutput::with_extra(value, extra),
        events.iter().map(|e| e.alert.clone()).collect(),
    )
}

// --- chart_patterns --------------------------------------------------------------------------

fn chart_code(kind: ChartPatternKind) -> f64 {
    match kind {
        ChartPatternKind::Triangle => 1.0,
        ChartPatternKind::AscendingTriangle => 2.0,
        ChartPatternKind::DescendingTriangle => 3.0,
        ChartPatternKind::Broadening => 4.0,
        ChartPatternKind::RisingWedge => 5.0,
        ChartPatternKind::FallingWedge => 6.0,
        ChartPatternKind::ReversalOneTwoThree => 7.0,
        ChartPatternKind::WolfeWave => 8.0,
        ChartPatternKind::DoubleTop => 9.0,
        ChartPatternKind::DoubleBottom => 10.0,
        ChartPatternKind::TripleTop => 11.0,
        ChartPatternKind::TripleBottom => 12.0,
        ChartPatternKind::HeadAndShoulders => 13.0,
        ChartPatternKind::InverseHeadAndShoulders => 14.0,
        ChartPatternKind::Flag => 15.0,
        ChartPatternKind::CupAndHandle => 16.0,
        ChartPatternKind::InverseCupAndHandle => 17.0,
        ChartPatternKind::AutoTrendline => unreachable!("the trendlines entry owns the lines"),
    }
}

fn chart_base(kind: ChartPatternKind) -> &'static str {
    match kind {
        ChartPatternKind::Triangle => "triangle",
        ChartPatternKind::AscendingTriangle => "ascending_triangle",
        ChartPatternKind::DescendingTriangle => "descending_triangle",
        ChartPatternKind::Broadening => "broadening",
        ChartPatternKind::RisingWedge => "rising_wedge",
        ChartPatternKind::FallingWedge => "falling_wedge",
        ChartPatternKind::ReversalOneTwoThree => "one_two_three",
        ChartPatternKind::WolfeWave => "wolfe_wave",
        ChartPatternKind::DoubleTop => "double_top",
        ChartPatternKind::DoubleBottom => "double_bottom",
        ChartPatternKind::TripleTop => "triple_top",
        ChartPatternKind::TripleBottom => "triple_bottom",
        ChartPatternKind::HeadAndShoulders => "head_and_shoulders",
        ChartPatternKind::InverseHeadAndShoulders => "inverse_head_and_shoulders",
        ChartPatternKind::Flag => "flag",
        ChartPatternKind::CupAndHandle => "cup_and_handle",
        ChartPatternKind::InverseCupAndHandle => "inverse_cup_and_handle",
        ChartPatternKind::AutoTrendline => unreachable!("the trendlines entry owns the lines"),
    }
}

/// The event of a candidate whose rule reported `Confirmed` on `bar`.
fn chart_event(c: &ChartPatternCandidate, bar: &Bar) -> Event {
    let ts = bar.timestamp;
    let mut extra: Vec<(&'static str, f64)> = vec![("pattern", chart_code(c.kind))];
    let bullish = match c.kind {
        ChartPatternKind::Triangle
        | ChartPatternKind::AscendingTriangle
        | ChartPatternKind::DescendingTriangle
        | ChartPatternKind::Broadening
        | ChartPatternKind::RisingWedge
        | ChartPatternKind::FallingWedge => {
            let upper = c.upper_line.expect("a boundary pattern has both lines");
            let lower = c.lower_line.expect("a boundary pattern has both lines");
            // Past the apex the lines have crossed: bullish only above both, bearish only below
            // both (the rule confirms nothing in between).
            let (u, l) = (upper.value_at(ts), lower.value_at(ts));
            let up = bar.close > u.max(l);
            extra.push(("level", if up { u.max(l) } else { u.min(l) }));
            up
        }
        ChartPatternKind::ReversalOneTwoThree => {
            let (n2, n3) = (&c.nodes[1], &c.nodes[2]);
            extra.push(("level", n3.price));
            extra.push(("invalidation", n2.price));
            !n2.is_high
        }
        ChartPatternKind::WolfeWave => {
            let n5 = &c.nodes[4];
            extra.push(("invalidation", n5.price));
            extra.push((
                "target",
                TrendLine::from_nodes(&c.nodes[0], &c.nodes[3]).value_at(ts),
            ));
            !n5.is_high
        }
        ChartPatternKind::Flag => {
            let upper = c.upper_line.expect("a flag has both channel lines");
            let lower = c.lower_line.expect("a flag has both channel lines");
            let bullish = !c.nodes[0].is_high;
            let (broken, opposite) = if bullish {
                (upper, lower)
            } else {
                (lower, upper)
            };
            extra.push(("level", broken.value_at(ts)));
            extra.push(("invalidation", opposite.value_at(ts)));
            bullish
        }
        ChartPatternKind::CupAndHandle | ChartPatternKind::InverseCupAndHandle => {
            extra.push(("level", c.nodes[2].price));
            extra.push(("invalidation", c.nodes[3].price));
            c.kind == ChartPatternKind::CupAndHandle
        }
        _ => {
            let top = c.kind.is_top();
            let neckline = if top { c.lower_line } else { c.upper_line }
                .expect("a reversal pattern has its neckline");
            let prices = c.nodes.iter().map(|n| n.price);
            let extreme = if top {
                prices.fold(f64::NEG_INFINITY, f64::max)
            } else {
                prices.fold(f64::INFINITY, f64::min)
            };
            extra.push(("level", neckline.value_at(ts)));
            extra.push(("invalidation", extreme));
            !top
        }
    };
    extra.push(("fit", c.confidence));
    Event {
        alert: alert(bullish, chart_base(c.kind)),
        direction: if bullish { 1.0 } else { -1.0 },
        extra,
    }
}

struct LiveChart {
    candidate: ChartPatternCandidate,
    last: usize,
}

/// The catalog entry `chart_patterns`: triangles (symmetrical, ascending, descending), broadening
/// formation, rising and falling wedge, 1-2-3 reversal, Wolfe wave, double and triple top and
/// bottom, head and shoulders with its inverse, flag, and cup and handle with its inverse, from [`ChartPatternDetector`] over the
/// ZigZag nodes as they confirm. See the module documentation for what one event is.
pub struct ChartPatternsEngine {
    feed: SwingFeed,
    detector: ChartPatternDetector,
    pending_swings: usize,
    live: Vec<LiveChart>,
    direction: f64,
    alerts: Vec<IndicatorAlert>,
}

impl ChartPatternsEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        depth: usize,
        backstep: usize,
        deviation: ZigZagDeviationMode,
        atr_len: usize,
        tolerance_pct: f64,
        flat_pct: f64,
        pending_swings: usize,
    ) -> Self {
        Self {
            feed: SwingFeed::new(depth, backstep, deviation, atr_len),
            detector: ChartPatternDetector::new(tolerance_pct).with_flat_pct(flat_pct),
            pending_swings,
            live: Vec::new(),
            direction: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for ChartPatternsEngine {
    fn name(&self) -> &str {
        "chart_patterns"
    }

    fn warmup_period(&self) -> usize {
        self.feed.zigzag.warmup_period()
    }

    fn reset(&mut self) {
        self.feed.reset();
        self.live.clear();
        self.direction = 0.0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let mut events = Vec::new();

        // Instances found on earlier bars follow this bar.
        self.live
            .retain_mut(|live| match live.candidate.update_state(bar) {
                PatternState::Forming => true,
                PatternState::Confirmed => {
                    events.push(chart_event(&live.candidate, bar));
                    false
                }
                PatternState::Invalidated => false,
            });

        let confirmed = self.feed.push(bar);
        let count = self.feed.seen;
        let first_new = count - confirmed.as_ref().map_or(0, Vec::len);
        for (offset, node) in confirmed.iter().flatten().enumerate() {
            let index = first_new + offset;
            let last = node.tail.last().expect("a confirmed node");
            for mut candidate in self.detector.scan_unranked_on(&node.tail, &node.clock) {
                if candidate.nodes.last().map(|n| n.timestamp) != Some(last.timestamp) {
                    continue;
                }
                let mut state = PatternState::Forming;
                let mut decided_on = None;
                for replayed in self.feed.bars_after(last.timestamp) {
                    state = candidate.update_state(&replayed);
                    if state != PatternState::Forming {
                        decided_on = Some(replayed);
                        break;
                    }
                }
                match (state, decided_on) {
                    (PatternState::Confirmed, Some(on)) => {
                        events.push(chart_event(&candidate, &on));
                    }
                    (PatternState::Forming, _) => self.live.push(LiveChart {
                        candidate,
                        last: index,
                    }),
                    _ => {}
                }
            }
        }
        let pending = self.pending_swings;
        self.live
            .retain(|live| count.saturating_sub(1 + live.last) < pending);
        confirmed?;
        if let Some(last) = events.last() {
            self.direction = last.direction;
        }
        let (out, alerts) = output(self.direction, self.live.len(), &events);
        self.alerts = alerts;
        Some(out)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

// --- harmonic_patterns -----------------------------------------------------------------------

fn harmonic_code(name: &str) -> f64 {
    HARMONIC_TABLE
        .iter()
        .position(|row| row.name == name)
        .expect("a candidate names a row of the table") as f64
        + 1.0
}

fn harmonic_event(c: &HarmonicCandidate) -> Event {
    let d = c.d.expect("a completed structure has its D");
    Event {
        alert: alert(c.bullish, c.name),
        direction: if c.bullish { 1.0 } else { -1.0 },
        extra: vec![
            ("pattern", harmonic_code(c.name)),
            ("prz_low", c.prz.low),
            ("prz_high", c.prz.high),
            ("d", d),
            (
                "invalidation",
                if c.bullish { c.prz.low } else { c.prz.high },
            ),
        ],
    }
}

struct LiveHarmonic {
    candidate: HarmonicCandidate,
    last: usize,
}

/// The catalog entry `harmonic_patterns`: the XABCD structures of [`HarmonicDetector`] (gartley,
/// bat, butterfly, crab, deep crab, shark) over the ZigZag nodes as they confirm. An instance is
/// found when its D node confirms; the detector's rule confirms it on the close out of the PRZ in
/// its direction and gives it up on a close through the zone. See the module documentation for
/// what one event is.
pub struct HarmonicPatternsEngine {
    feed: SwingFeed,
    detector: HarmonicDetector,
    pending_swings: usize,
    live: Vec<LiveHarmonic>,
    direction: f64,
    alerts: Vec<IndicatorAlert>,
}

impl HarmonicPatternsEngine {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        depth: usize,
        backstep: usize,
        deviation: ZigZagDeviationMode,
        atr_len: usize,
        ratio_tolerance: f64,
        min_prz_share: f64,
        pending_swings: usize,
    ) -> Self {
        Self {
            feed: SwingFeed::new(depth, backstep, deviation, atr_len),
            detector: HarmonicDetector::new(HarmonicConfig {
                ratio_tolerance,
                min_prz_share,
            }),
            pending_swings,
            live: Vec::new(),
            direction: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for HarmonicPatternsEngine {
    fn name(&self) -> &str {
        "harmonic_patterns"
    }

    fn warmup_period(&self) -> usize {
        self.feed.zigzag.warmup_period()
    }

    fn reset(&mut self) {
        self.feed.reset();
        self.live.clear();
        self.direction = 0.0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let mut events = Vec::new();

        self.live
            .retain_mut(|live| match live.candidate.update_state(bar) {
                HarmonicState::Confirmed => {
                    events.push(harmonic_event(&live.candidate));
                    false
                }
                HarmonicState::Invalidated => false,
                HarmonicState::Candidate | HarmonicState::Complete => true,
            });

        let confirmed = self.feed.push(bar);
        let count = self.feed.seen;
        let first_new = count - confirmed.as_ref().map_or(0, Vec::len);
        for (offset, node) in confirmed.iter().flatten().enumerate() {
            let index = first_new + offset;
            if node.tail.len() < WINDOW {
                continue;
            }
            let last = node.tail.last().expect("a confirmed node");
            for mut candidate in self.detector.scan(&node.tail) {
                let mut state = candidate.state;
                for replayed in self.feed.bars_after(last.timestamp) {
                    state = candidate.update_state(&replayed);
                    if matches!(state, HarmonicState::Confirmed | HarmonicState::Invalidated) {
                        break;
                    }
                }
                match state {
                    HarmonicState::Confirmed => events.push(harmonic_event(&candidate)),
                    HarmonicState::Invalidated => {}
                    HarmonicState::Candidate | HarmonicState::Complete => {
                        self.live.push(LiveHarmonic {
                            candidate,
                            last: index,
                        })
                    }
                }
            }
        }
        let pending = self.pending_swings;
        self.live
            .retain(|live| count.saturating_sub(1 + live.last) < pending);

        confirmed?;
        if let Some(last) = events.last() {
            self.direction = last.direction;
        }
        let (out, alerts) = output(self.direction, self.live.len(), &events);
        self.alerts = alerts;
        Some(out)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

// --- definitions -----------------------------------------------------------------------------

macro_rules! zigzag_params {
    ($($extra:expr),* $(,)?) => {
        &[
            ParamSpec::count("depth", 3, 1, 10000).meaning("bars on each side of a swing pivot"),
            ParamSpec::count("backstep", 2, 0, 10000)
                .meaning("bars between a confirmed swing and the next pivot that may confirm"),
            ParamSpec::choice("deviation_mode", "percent", &["percent", "atr_multiple"], true)
                .meaning("unit of the swing deviation: percent of price or multiples of the ATR"),
            ParamSpec::number("deviation_pct", 1.0, 0.001, 1000.0)
                .range_by(
                    "deviation_mode",
                    &[("percent", 0.001, 100.0), ("atr_multiple", 0.001, 1000.0)],
                )
                .meaning("minimum move that confirms a swing, in the unit of deviation_mode"),
            ParamSpec::count("atr_len", 14, 1, 10000).meaning("bars in the ATR of the ZigZag"),
            $($extra,)*
            ParamSpec::count("pending_swings", 2, 1, 100)
                .meaning("swings confirmed behind a pattern's last anchor after which an unconfirmed pattern is dropped"),
        ]
    };
}

fn deviation(p: &ResolvedParams) -> ZigZagDeviationMode {
    let value = p.number("deviation_pct");
    match p.choice("deviation_mode") {
        "atr_multiple" => ZigZagDeviationMode::AtrMultiple(value),
        _ => ZigZagDeviationMode::Percent(value),
    }
}

/// Registry definition of `chart_patterns`.
pub const CHART_PATTERNS_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "chart_patterns",
    aliases: &[],
    description: "Chart patterns from ZigZag swings as they confirm, one event per pattern instance on the bar its neckline or boundary is broken: triangles (symmetrical, ascending, descending), broadening, rising and falling wedge, 1-2-3 reversal, Wolfe wave, double and triple top and bottom, head and shoulders and its inverse, flag, cup and handle and its inverse. value is the direction of the latest confirmed pattern (1, -1, 0); alerts bull_<kind> and bear_<kind>; extras pattern (kind code 1-17), level, invalidation, target, fit, active, confirmed",
    params: zigzag_params![
        ParamSpec::number("tolerance_pct", 2.0, 0.001, 50.0)
            .meaning("percent of the swing price within which two swing prices count as one level"),
        ParamSpec::number("flat_pct", 1.5, 0.0, 100.0)
            .meaning("percent of the opening gap a boundary may move and still count as flat"),
    ],
    checks: &[],
    facts: crate::indicator::definition::IndicatorFacts::unitless()
        .session_sensitive()
        .roll_sensitive()
        .adjustment_sensitive()
        .range_dependent(),
    verification: crate::indicator::definition::Verification::Open(
        "no independent second implementation yet",
    ),
    build: |p| {
        Ok(Box::new(ChartPatternsEngine::new(
            p.count("depth"),
            p.count("backstep"),
            deviation(p),
            p.count("atr_len"),
            p.number("tolerance_pct"),
            p.number("flat_pct"),
            p.count("pending_swings"),
        )))
    },
};

/// Registry definition of `harmonic_patterns`.
pub const HARMONIC_PATTERNS_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "harmonic_patterns",
    aliases: &[],
    description: "Harmonic XABCD patterns (gartley, bat, butterfly, crab, deep crab, shark) from ZigZag swings as they confirm, one event per structure on the close out of its potential reversal zone. value is the direction of the latest confirmed pattern (1, -1, 0); alerts bull_<name> and bear_<name>; extras pattern (table row 1-6), prz_low, prz_high, d, invalidation, active, confirmed",
    params: zigzag_params![
        ParamSpec::number("ratio_tolerance", 0.03, 0.0, 0.5)
            .meaning("absolute widening of every ratio interval of the pattern table"),
        ParamSpec::number("min_prz_share", 0.005, 0.0, 1.0)
            .meaning("minimum width of the whole reversal zone as a share of the XA leg"),
    ],
    checks: &[],
    facts: crate::indicator::definition::IndicatorFacts::unitless()
        .session_sensitive()
        .roll_sensitive()
        .adjustment_sensitive()
        .range_dependent(),
    verification: crate::indicator::definition::Verification::Open(
        "no independent second implementation yet",
    ),
    build: |p| {
        Ok(Box::new(HarmonicPatternsEngine::new(
            p.count("depth"),
            p.count("backstep"),
            deviation(p),
            p.count("atr_len"),
            p.number("ratio_tolerance"),
            p.number("min_prz_share"),
            p.count("pending_swings"),
        )))
    },
};
