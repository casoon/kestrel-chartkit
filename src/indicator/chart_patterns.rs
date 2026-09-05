//! Chart pattern framework: swing-based trendline fits, tolerance-checked pattern rules,
//! confidence ranking, overlap eviction, and a lifecycle state — built on
//! [`super::zigzag_advanced::ZigZagNode`] sequences (e.g. from
//! [`super::zigzag_advanced::AdvancedZigZagEngine::nodes`]) rather than re-detecting swings.
//!
//! Covers Triangle, Rising/Falling Wedge, the 1-2-3 Reversal, the Wolfe Wave, the classic
//! reversal family (double and triple top/bottom, head and shoulders and its inverse), and
//! auto-fitted trendlines. Each detector is a fixed, documented geometric rule over swing points —
//! a deterministic approximation of how these patterns are described in TA literature, not a claim
//! that every instance found is a "real" tradable pattern.
//!
//! # Complete is not confirmed
//!
//! Two roughly equal highs are not a double top. They become one when the trough between them
//! breaks. The reversal detectors therefore emit their candidates as [`PatternState::Forming`]
//! even though every defining node is already in place, and only [`PatternState::Confirmed`] once
//! price closes through the neckline. That distinction is the whole point of the family: the
//! shape is visible long before it means anything, and a detector that reported the shape as the
//! result would encode exactly the misreading these patterns are famous for.

use crate::model::Bar;

use super::zigzag_advanced::ZigZagNode;

/// A two-point price line, usable to project a value at any timestamp.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TrendLine {
    pub start: (i64, f64),
    pub end: (i64, f64),
}

impl TrendLine {
    pub fn from_nodes(a: &ZigZagNode, b: &ZigZagNode) -> Self {
        Self {
            start: (a.timestamp, a.price),
            end: (b.timestamp, b.price),
        }
    }

    pub fn slope(&self) -> f64 {
        let dt = (self.end.0 - self.start.0) as f64;
        if dt == 0.0 {
            return 0.0;
        }
        (self.end.1 - self.start.1) / dt
    }

    pub fn value_at(&self, timestamp: i64) -> f64 {
        self.start.1 + self.slope() * (timestamp - self.start.0) as f64
    }
}

/// Lifecycle of a detected pattern candidate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PatternState {
    /// Still within its defining swing points; boundary not yet broken.
    Forming,
    /// Price broke out through a boundary in the pattern's implied direction.
    Confirmed,
    /// Price violated the pattern's structure without a valid breakout (e.g. closed back through
    /// the opposite boundary first).
    Invalidated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChartPatternKind {
    Triangle,
    RisingWedge,
    FallingWedge,
    ReversalOneTwoThree,
    WolfeWave,
    AutoTrendline,
    DoubleTop,
    DoubleBottom,
    TripleTop,
    TripleBottom,
    HeadAndShoulders,
    InverseHeadAndShoulders,
}

impl ChartPatternKind {
    /// Whether this kind resolves downward — a top rather than a bottom.
    ///
    /// Only meaningful for the reversal family; the others carry their direction in their lines.
    fn is_top(self) -> bool {
        matches!(
            self,
            ChartPatternKind::DoubleTop
                | ChartPatternKind::TripleTop
                | ChartPatternKind::HeadAndShoulders
        )
    }

    /// The reversal family, whose members share one lifecycle rule: break the neckline.
    fn is_reversal_family(self) -> bool {
        matches!(
            self,
            ChartPatternKind::DoubleTop
                | ChartPatternKind::DoubleBottom
                | ChartPatternKind::TripleTop
                | ChartPatternKind::TripleBottom
                | ChartPatternKind::HeadAndShoulders
                | ChartPatternKind::InverseHeadAndShoulders
        )
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChartPatternCandidate {
    pub kind: ChartPatternKind,
    pub nodes: Vec<ZigZagNode>,
    pub upper_line: Option<TrendLine>,
    pub lower_line: Option<TrendLine>,
    pub state: PatternState,
    /// Heuristic `0.0..=1.0` ranking used for overlap eviction: higher generally means a cleaner
    /// geometric fit (more converging, more parallel, etc., depending on `kind`).
    pub confidence: f64,
}

impl ChartPatternCandidate {
    fn formed_at(&self) -> i64 {
        self.nodes.last().map(|n| n.timestamp).unwrap_or(0)
    }

    fn node_range(&self) -> (i64, i64) {
        let start = self.nodes.first().map(|n| n.timestamp).unwrap_or(0);
        let end = self.formed_at();
        (start, end)
    }

    /// Advances this candidate's lifecycle given a subsequent bar, and returns the (possibly
    /// unchanged) resulting state. A no-op once already `Confirmed`/`Invalidated` — those are
    /// terminal.
    pub fn update_state(&mut self, bar: &Bar) -> PatternState {
        if self.state != PatternState::Forming {
            return self.state;
        }

        if self.kind.is_reversal_family() {
            self.state = self.update_reversal_state(bar);
            return self.state;
        }

        self.state = match self.kind {
            ChartPatternKind::Triangle
            | ChartPatternKind::RisingWedge
            | ChartPatternKind::FallingWedge => match (&self.upper_line, &self.lower_line) {
                (Some(upper), Some(lower)) => {
                    if bar.close > upper.value_at(bar.timestamp)
                        || bar.close < lower.value_at(bar.timestamp)
                    {
                        PatternState::Confirmed
                    } else {
                        PatternState::Forming
                    }
                }
                _ => PatternState::Forming,
            },
            ChartPatternKind::AutoTrendline => match self.upper_line.or(self.lower_line) {
                Some(line) => {
                    let is_resistance = self.upper_line.is_some();
                    let broke = if is_resistance {
                        bar.close > line.value_at(bar.timestamp)
                    } else {
                        bar.close < line.value_at(bar.timestamp)
                    };
                    if broke {
                        PatternState::Confirmed
                    } else {
                        PatternState::Forming
                    }
                }
                None => PatternState::Forming,
            },
            ChartPatternKind::ReversalOneTwoThree => {
                let (n2, n3) = (&self.nodes[1], &self.nodes[2]);
                let bearish = n2.is_high;
                if bearish {
                    if bar.close < n3.price {
                        PatternState::Confirmed
                    } else if bar.close > n2.price {
                        PatternState::Invalidated
                    } else {
                        PatternState::Forming
                    }
                } else if bar.close > n3.price {
                    PatternState::Confirmed
                } else if bar.close < n2.price {
                    PatternState::Invalidated
                } else {
                    PatternState::Forming
                }
            }
            ChartPatternKind::DoubleTop
            | ChartPatternKind::DoubleBottom
            | ChartPatternKind::TripleTop
            | ChartPatternKind::TripleBottom
            | ChartPatternKind::HeadAndShoulders
            | ChartPatternKind::InverseHeadAndShoulders => unreachable!("handled above"),
            ChartPatternKind::WolfeWave => {
                let n5 = self.nodes[4];
                let target_line = TrendLine::from_nodes(&self.nodes[0], &self.nodes[3]); // line 1-4 projects the target
                let target = target_line.value_at(bar.timestamp);
                let reverting_toward_target = if n5.is_high {
                    bar.close < n5.price && bar.close >= target.min(n5.price)
                } else {
                    bar.close > n5.price && bar.close <= target.max(n5.price)
                };
                let continuing_past_five = if n5.is_high {
                    bar.close > n5.price
                } else {
                    bar.close < n5.price
                };
                if reverting_toward_target {
                    PatternState::Confirmed
                } else if continuing_past_five {
                    PatternState::Invalidated
                } else {
                    PatternState::Forming
                }
            }
        };

        self.state
    }
}

impl ChartPatternCandidate {
    /// The reversal family shares one rule: the neckline decides.
    ///
    /// Confirmed on a close through the neckline in the pattern's direction; invalidated on a
    /// close past the pattern's own extreme, which is where the structure it describes stops
    /// existing. In between it stays `Forming` — complete, and saying nothing yet.
    fn update_reversal_state(&self, bar: &Bar) -> PatternState {
        let top = self.kind.is_top();
        let Some(neckline) = (if top {
            self.lower_line
        } else {
            self.upper_line
        }) else {
            return PatternState::Forming;
        };
        let level = neckline.value_at(bar.timestamp);

        let extreme = if top {
            self.nodes
                .iter()
                .map(|n| n.price)
                .fold(f64::NEG_INFINITY, f64::max)
        } else {
            self.nodes
                .iter()
                .map(|n| n.price)
                .fold(f64::INFINITY, f64::min)
        };

        if top {
            if bar.close < level {
                PatternState::Confirmed
            } else if bar.close > extreme {
                PatternState::Invalidated
            } else {
                PatternState::Forming
            }
        } else if bar.close > level {
            PatternState::Confirmed
        } else if bar.close < extreme {
            PatternState::Invalidated
        } else {
            PatternState::Forming
        }
    }
}

/// Scans a swing-node sequence for pattern candidates and evicts overlapping lower-confidence
/// ones, so the result is a ranked, non-redundant set rather than every geometrically-possible
/// match.
pub struct ChartPatternDetector {
    pub tolerance_pct: f64,
}

impl ChartPatternDetector {
    pub fn new(tolerance_pct: f64) -> Self {
        Self {
            tolerance_pct: tolerance_pct.max(0.001),
        }
    }

    /// Detects all supported pattern kinds over `nodes`, then evicts overlapping candidates
    /// (sharing any swing node), keeping the highest-confidence one per overlapping cluster.
    pub fn scan(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut candidates = Vec::new();
        candidates.extend(self.scan_triangles_and_wedges(nodes));
        candidates.extend(self.scan_reversal_one_two_three(nodes));
        candidates.extend(self.scan_wolfe_waves(nodes));
        candidates.extend(self.scan_double_extremes(nodes));
        candidates.extend(self.scan_triple_extremes(nodes));
        candidates.extend(self.scan_head_and_shoulders(nodes));
        if let Some(trendline) = self.auto_trendline(nodes, true) {
            candidates.push(trendline);
        }
        if let Some(trendline) = self.auto_trendline(nodes, false) {
            candidates.push(trendline);
        }
        self.evict_overlaps(candidates)
    }

    /// Evicts overlapping candidates *within the same pattern kind* only (sliding-window
    /// detection naturally produces redundant near-duplicates of one kind over the same swing
    /// points). Different kinds describe different information and are allowed to coexist over
    /// the same nodes — e.g. a 1-2-3 reversal and an unrelated 2-point auto-trendline spanning the
    /// same three nodes are not "competing" for the same signal.
    fn evict_overlaps(&self, candidates: Vec<ChartPatternCandidate>) -> Vec<ChartPatternCandidate> {
        let mut by_kind: Vec<(ChartPatternKind, Vec<ChartPatternCandidate>)> = Vec::new();
        for candidate in candidates {
            match by_kind.iter_mut().find(|(k, _)| *k == candidate.kind) {
                Some((_, group)) => group.push(candidate),
                None => by_kind.push((candidate.kind, vec![candidate])),
            }
        }

        let mut kept: Vec<ChartPatternCandidate> = Vec::new();
        for (_, mut group) in by_kind {
            group.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
            'outer: for candidate in group {
                let (c_start, c_end) = candidate.node_range();
                for existing in &kept {
                    if existing.kind != candidate.kind {
                        continue;
                    }
                    let (e_start, e_end) = existing.node_range();
                    let overlaps = c_start <= e_end && e_start <= c_end;
                    if overlaps {
                        continue 'outer;
                    }
                }
                kept.push(candidate);
            }
        }
        kept.sort_by_key(|c| c.formed_at());
        kept
    }

    /// Triangles/wedges need 4 alternating swing nodes (H,L,H,L or L,H,L,H): an upper line through
    /// the two highs, a lower line through the two lows.
    fn scan_triangles_and_wedges(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(4) {
            let alternating = window.windows(2).all(|p| p[0].is_high != p[1].is_high);
            if !alternating {
                continue;
            }
            let highs: Vec<&ZigZagNode> = window.iter().filter(|n| n.is_high).collect();
            let lows: Vec<&ZigZagNode> = window.iter().filter(|n| !n.is_high).collect();
            if highs.len() != 2 || lows.len() != 2 {
                continue;
            }

            let upper = TrendLine::from_nodes(highs[0], highs[1]);
            let lower = TrendLine::from_nodes(lows[0], lows[1]);

            let (start_ts, end_ts) = (
                window.first().unwrap().timestamp,
                window.last().unwrap().timestamp,
            );
            let gap_start = upper.value_at(start_ts) - lower.value_at(start_ts);
            let gap_end = upper.value_at(end_ts) - lower.value_at(end_ts);
            if gap_start <= 0.0 || gap_end <= 0.0 || gap_end >= gap_start {
                continue; // must be converging
            }

            let convergence = 1.0 - (gap_end / gap_start);
            let flat_tol = self.tolerance_pct / 100.0;
            let upper_flat = upper.slope().abs() / gap_start.max(1e-9) < flat_tol;
            let lower_flat = lower.slope().abs() / gap_start.max(1e-9) < flat_tol;

            let kind = if upper.slope() > 0.0 && lower.slope() > 0.0 {
                ChartPatternKind::RisingWedge
            } else if upper.slope() < 0.0 && lower.slope() < 0.0 {
                ChartPatternKind::FallingWedge
            } else if (upper.slope() <= 0.0 || upper_flat) && (lower.slope() >= 0.0 || lower_flat) {
                ChartPatternKind::Triangle
            } else {
                continue;
            };

            out.push(ChartPatternCandidate {
                kind,
                nodes: window.to_vec(),
                upper_line: Some(upper),
                lower_line: Some(lower),
                state: PatternState::Forming,
                confidence: convergence.clamp(0.0, 1.0),
            });
        }
        out
    }

    /// A 1-2-3 reversal: three consecutive nodes where the middle one is a failed extreme (did
    /// not extend the trend) and the third breaks past the first's level in the opposite
    /// direction — e.g. bearish: low(1) < high(2) fails to make a new high vs. the prior trend,
    /// then low(3) < low(1).
    fn scan_reversal_one_two_three(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(3) {
            let (n1, n2, n3) = (&window[0], &window[1], &window[2]);
            if n1.is_high == n2.is_high || n2.is_high == n3.is_high {
                continue;
            }

            let bearish = !n1.is_high && n2.is_high && !n3.is_high && n3.price < n1.price;
            let bullish = n1.is_high && !n2.is_high && n3.is_high && n3.price > n1.price;
            if !bearish && !bullish {
                continue;
            }

            let magnitude = (n3.price - n1.price).abs() / n1.price.abs().max(1e-9);
            out.push(ChartPatternCandidate {
                kind: ChartPatternKind::ReversalOneTwoThree,
                nodes: window.to_vec(),
                upper_line: None,
                lower_line: None,
                state: PatternState::Forming,
                confidence: magnitude.min(1.0),
            });
        }
        out
    }

    /// How close two prices have to be to count as "the same level" here.
    ///
    /// `tolerance_pct` is a percentage, as everywhere else in this module. It is the single number
    /// that decides how many of these patterns exist: at two percent one finds few double tops, at
    /// eight percent many. A statement about their frequency that omits it says nothing.
    fn within_tolerance(&self, a: f64, b: f64, scale: f64) -> bool {
        (a - b).abs() <= self.tolerance_pct / 100.0 * scale.abs().max(1e-9)
    }

    /// How well two prices match, as `0.0..=1.0` — the confidence of the equal-level family.
    fn level_match(&self, a: f64, b: f64, scale: f64) -> f64 {
        let allowed = self.tolerance_pct / 100.0 * scale.abs().max(1e-9);
        if allowed <= 0.0 {
            return 0.0;
        }
        (1.0 - (a - b).abs() / allowed).clamp(0.0, 1.0)
    }

    /// Double top and bottom: two extremes at roughly the same level with one counter-swing
    /// between them.
    ///
    /// The counter-swing has to be more than the level tolerance away, otherwise three points of
    /// noise on one level would qualify. The neckline is that middle node, held horizontally —
    /// its break is what turns two equal highs into a double top.
    fn scan_double_extremes(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(3) {
            let (n1, n2, n3) = (&window[0], &window[1], &window[2]);
            if n1.is_high != n3.is_high || n1.is_high == n2.is_high {
                continue;
            }
            if !self.within_tolerance(n1.price, n3.price, n1.price) {
                continue;
            }
            if self.within_tolerance(n1.price, n2.price, n1.price) {
                continue;
            }

            let neckline = TrendLine::from_nodes(n2, n2);
            let (kind, upper_line, lower_line) = if n1.is_high {
                (ChartPatternKind::DoubleTop, None, Some(neckline))
            } else {
                (ChartPatternKind::DoubleBottom, Some(neckline), None)
            };

            out.push(ChartPatternCandidate {
                kind,
                nodes: window.to_vec(),
                upper_line,
                lower_line,
                state: PatternState::Forming,
                confidence: self.level_match(n1.price, n3.price, n1.price),
            });
        }
        out
    }

    /// Triple top and bottom: three extremes on one level, two counter-swings between them.
    ///
    /// The neckline is the *further* of the two counter-swings — the lower trough for a top. The
    /// nearer one breaking is not yet the pattern; taking the conservative level keeps
    /// `Confirmed` meaning the same thing it means for the double.
    fn scan_triple_extremes(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(5) {
            let alternating = window.windows(2).all(|p| p[0].is_high != p[1].is_high);
            if !alternating {
                continue;
            }
            let (n1, n3, n5) = (&window[0], &window[2], &window[4]);
            if !self.within_tolerance(n1.price, n3.price, n1.price)
                || !self.within_tolerance(n1.price, n5.price, n1.price)
            {
                continue;
            }
            let (n2, n4) = (&window[1], &window[3]);
            if self.within_tolerance(n1.price, n2.price, n1.price) {
                continue;
            }

            let conservative = if n1.is_high {
                if n2.price <= n4.price {
                    n2
                } else {
                    n4
                }
            } else if n2.price >= n4.price {
                n2
            } else {
                n4
            };
            let neckline = TrendLine::from_nodes(conservative, conservative);
            let (kind, upper_line, lower_line) = if n1.is_high {
                (ChartPatternKind::TripleTop, None, Some(neckline))
            } else {
                (ChartPatternKind::TripleBottom, Some(neckline), None)
            };

            let fit = self.level_match(n1.price, n3.price, n1.price)
                * self.level_match(n1.price, n5.price, n1.price);
            out.push(ChartPatternCandidate {
                kind,
                nodes: window.to_vec(),
                upper_line,
                lower_line,
                state: PatternState::Forming,
                confidence: fit,
            });
        }
        out
    }

    /// Head and shoulders and its inverse: five alternating nodes whose middle extreme overshoots
    /// both its neighbours of the same type, which sit at roughly one level.
    ///
    /// The neckline is the line through the two counter-swings and is deliberately *not* forced
    /// horizontal — a sloping neckline is the common case, and flattening it would move the
    /// confirmation level.
    ///
    /// Note what is not required: that the right shoulder be "well formed". It bears no weight.
    /// The pattern completes there and is confirmed only at the neckline.
    fn scan_head_and_shoulders(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(5) {
            let alternating = window.windows(2).all(|p| p[0].is_high != p[1].is_high);
            if !alternating {
                continue;
            }
            let (n1, n2, n3, n4, n5) = (&window[0], &window[1], &window[2], &window[3], &window[4]);

            let head_overshoots = if n1.is_high {
                n3.price > n1.price && n3.price > n5.price
            } else {
                n3.price < n1.price && n3.price < n5.price
            };
            if !head_overshoots {
                continue;
            }
            if !self.within_tolerance(n1.price, n5.price, n3.price) {
                continue;
            }
            if !self.within_tolerance(n2.price, n4.price, n3.price) {
                continue;
            }

            let neckline = TrendLine::from_nodes(n2, n4);
            let (kind, upper_line, lower_line) = if n1.is_high {
                (ChartPatternKind::HeadAndShoulders, None, Some(neckline))
            } else {
                (
                    ChartPatternKind::InverseHeadAndShoulders,
                    Some(neckline),
                    None,
                )
            };

            let shoulders = self.level_match(n1.price, n5.price, n3.price);
            let necks = self.level_match(n2.price, n4.price, n3.price);
            out.push(ChartPatternCandidate {
                kind,
                nodes: window.to_vec(),
                upper_line,
                lower_line,
                state: PatternState::Forming,
                confidence: shoulders * necks,
            });
        }
        out
    }

    /// A Wolfe Wave: 5 alternating points where line(1-3) and line(2-4) are roughly parallel and
    /// point 5 pierces the line(1-3) extension.
    fn scan_wolfe_waves(&self, nodes: &[ZigZagNode]) -> Vec<ChartPatternCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(5) {
            let alternating = window.windows(2).all(|p| p[0].is_high != p[1].is_high);
            if !alternating {
                continue;
            }
            let (n1, n2, n3, n4, n5) = (&window[0], &window[1], &window[2], &window[3], &window[4]);

            let line13 = TrendLine::from_nodes(n1, n3);
            let line24 = TrendLine::from_nodes(n2, n4);

            let scale = (n1.price.abs() + n3.price.abs()).max(1e-9);
            let slope_diff = (line13.slope() - line24.slope()).abs() / scale;
            let parallel_tol = self.tolerance_pct / 100.0 * 5.0;
            if slope_diff > parallel_tol {
                continue;
            }

            let projected13_at5 = line13.value_at(n5.timestamp);
            let pierces = if n1.is_high {
                // 1,3,5 are lows (bullish Wolfe): point 5 must undercut the 1-3 extension.
                !n5.is_high && n5.price < projected13_at5
            } else {
                n5.is_high && n5.price > projected13_at5
            };
            if !pierces {
                continue;
            }

            let confidence = (1.0 - slope_diff / parallel_tol.max(1e-9)).clamp(0.0, 1.0);
            out.push(ChartPatternCandidate {
                kind: ChartPatternKind::WolfeWave,
                nodes: window.to_vec(),
                upper_line: Some(if n1.is_high { line24 } else { line13 }),
                lower_line: Some(if n1.is_high { line13 } else { line24 }),
                state: PatternState::Forming,
                confidence,
            });
        }
        out
    }

    /// Auto-fits the best trendline through same-type swing points: the oldest and newest node of
    /// that type, valid only if no intermediate node of the same type violates it (a resistance
    /// line no high pierces, or a support line no low pierces).
    fn auto_trendline(
        &self,
        nodes: &[ZigZagNode],
        for_highs: bool,
    ) -> Option<ChartPatternCandidate> {
        let same_type: Vec<&ZigZagNode> = nodes.iter().filter(|n| n.is_high == for_highs).collect();
        if same_type.len() < 2 {
            return None;
        }
        let first = *same_type.first().unwrap();
        let last = *same_type.last().unwrap();
        let line = TrendLine::from_nodes(first, last);

        let violated = same_type.iter().any(|n| {
            let projected = line.value_at(n.timestamp);
            if for_highs {
                n.price > projected * (1.0 + self.tolerance_pct / 100.0)
            } else {
                n.price < projected * (1.0 - self.tolerance_pct / 100.0)
            }
        });
        if violated {
            return None;
        }

        let touches = same_type.len();
        let confidence = ((touches as f64 - 2.0) / 4.0 + 0.5).clamp(0.0, 1.0);

        Some(ChartPatternCandidate {
            kind: ChartPatternKind::AutoTrendline,
            nodes: same_type.into_iter().copied().collect(),
            upper_line: for_highs.then_some(line),
            lower_line: (!for_highs).then_some(line),
            state: PatternState::Forming,
            confidence,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn node(ts: i64, price: f64, is_high: bool) -> ZigZagNode {
        ZigZagNode {
            timestamp: ts,
            price,
            is_high,
            confirmed: true,
        }
    }

    #[test]
    fn test_trendline_value_at_interpolates() {
        let a = node(0, 100.0, true);
        let b = node(100, 200.0, true);
        let line = TrendLine::from_nodes(&a, &b);
        assert!((line.value_at(50) - 150.0).abs() < 1e-9);
    }

    #[test]
    fn test_detects_converging_triangle() {
        let nodes = vec![
            node(0, 110.0, true),
            node(10, 90.0, false),
            node(20, 105.0, true),
            node(30, 95.0, false),
        ];
        let detector = ChartPatternDetector::new(50.0);
        let candidates = detector.scan(&nodes);
        assert!(candidates
            .iter()
            .any(|c| c.kind == ChartPatternKind::Triangle));
    }

    #[test]
    fn test_detects_bearish_one_two_three_reversal() {
        let nodes = vec![
            node(0, 100.0, false),
            node(10, 110.0, true),
            node(20, 95.0, false),
        ];
        let detector = ChartPatternDetector::new(1.0);
        let candidates = detector.scan(&nodes);
        assert!(candidates
            .iter()
            .any(|c| c.kind == ChartPatternKind::ReversalOneTwoThree));
    }

    #[test]
    fn test_auto_trendline_rejects_violated_support() {
        // Three lows: a line from the first (100) to the last (95) projects ~97.5 at t=10, but
        // the middle low dips to 90 -- well below that line, violating it as a support trendline.
        let nodes = vec![
            node(0, 100.0, false),
            node(5, 105.0, true),
            node(10, 90.0, false),
            node(15, 102.0, true),
            node(20, 95.0, false),
        ];
        let detector = ChartPatternDetector::new(0.1);
        let candidates = detector.scan(&nodes);
        assert!(!candidates.iter().any(
            |c| c.kind == ChartPatternKind::AutoTrendline && c.nodes.iter().all(|n| !n.is_high)
        ));
    }

    #[test]
    fn test_evict_overlaps_keeps_only_highest_confidence_within_same_kind() {
        // A longer alternating sequence so multiple overlapping 4-node triangle/wedge windows
        // are genuinely detected and compete against each other for eviction.
        let nodes = vec![
            node(0, 130.0, true),
            node(10, 70.0, false),
            node(20, 120.0, true),
            node(30, 80.0, false),
            node(40, 110.0, true),
            node(50, 90.0, false),
        ];
        let detector = ChartPatternDetector::new(50.0);
        let candidates = detector.scan(&nodes);

        // Within any single kind, no two surviving candidates may share a node timestamp range.
        for kind in [
            ChartPatternKind::Triangle,
            ChartPatternKind::RisingWedge,
            ChartPatternKind::FallingWedge,
        ] {
            let same_kind: Vec<&ChartPatternCandidate> =
                candidates.iter().filter(|c| c.kind == kind).collect();
            for (i, a) in same_kind.iter().enumerate() {
                for b in same_kind.iter().skip(i + 1) {
                    let (a_start, a_end) = a.node_range();
                    let (b_start, b_end) = b.node_range();
                    assert!(
                        a_end < b_start || b_end < a_start,
                        "overlapping candidates of the same kind must have been evicted"
                    );
                }
            }
        }

        // Different kinds are allowed to overlap (e.g. an auto-trendline and a triangle sharing
        // nodes describe different information), so the result set is non-empty and mixed.
        assert!(!candidates.is_empty());
    }

    #[test]
    fn test_triangle_confirms_on_breakout() {
        let nodes = vec![
            node(0, 110.0, true),
            node(10, 90.0, false),
            node(20, 105.0, true),
            node(30, 95.0, false),
        ];
        let detector = ChartPatternDetector::new(50.0);
        let mut candidates = detector.scan(&nodes);
        let triangle = candidates
            .iter_mut()
            .find(|c| c.kind == ChartPatternKind::Triangle)
            .unwrap();

        // Still inside both lines: stays Forming.
        let inside = Bar::new(35, 100.0, 100.5, 99.5, 100.0, 1.0);
        assert_eq!(triangle.update_state(&inside), PatternState::Forming);

        // Breaks decisively above the upper line.
        let breakout = Bar::new(40, 130.0, 130.5, 129.5, 130.0, 1.0);
        assert_eq!(triangle.update_state(&breakout), PatternState::Confirmed);

        // Terminal: a later bar cannot change a Confirmed pattern back to Forming.
        let after = Bar::new(50, 50.0, 50.5, 49.5, 50.0, 1.0);
        assert_eq!(triangle.update_state(&after), PatternState::Confirmed);
    }

    #[test]
    fn test_reversal_one_two_three_confirms_and_invalidates() {
        let confirm_nodes = vec![
            node(0, 100.0, false),
            node(10, 110.0, true),
            node(20, 95.0, false),
        ];
        let mut confirm_candidate = ChartPatternDetector::new(1.0)
            .scan(&confirm_nodes)
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::ReversalOneTwoThree)
            .unwrap();
        let breaks_below_n3 = Bar::new(30, 90.0, 90.5, 89.5, 90.0, 1.0);
        assert_eq!(
            confirm_candidate.update_state(&breaks_below_n3),
            PatternState::Confirmed
        );

        let mut invalidate_candidate = ChartPatternDetector::new(1.0)
            .scan(&confirm_nodes)
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::ReversalOneTwoThree)
            .unwrap();
        let reclaims_above_n2 = Bar::new(30, 115.0, 115.5, 114.5, 115.0, 1.0);
        assert_eq!(
            invalidate_candidate.update_state(&reclaims_above_n2),
            PatternState::Invalidated
        );
    }

    // -----------------------------------------------------------------------
    // Die Umkehrfamilie
    // -----------------------------------------------------------------------

    /// Zwei annähernd gleich hohe Hochs, ein Zwischentief.
    fn doppeltop_nodes() -> Vec<ZigZagNode> {
        vec![
            node(0, 118.0, true),
            node(1200, 108.0, false),
            node(2400, 117.4, true),
        ]
    }

    #[test]
    fn test_double_top_is_forming_until_the_neckline_breaks() {
        // Der Kernsatz der Familie: Die Formation ist vollständig und sagt trotzdem nichts.
        let detector = ChartPatternDetector::new(2.0);
        let mut candidate = detector
            .scan(&doppeltop_nodes())
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::DoubleTop)
            .expect("double top detected");
        assert_eq!(candidate.state, PatternState::Forming);

        // Über dem Zwischentief bleibt es beim Zustand.
        assert_eq!(
            candidate.update_state(&Bar::new(3000, 112.0, 113.0, 111.0, 112.0, 1.0)),
            PatternState::Forming
        );
        // Erst der Schluss darunter bestätigt.
        assert_eq!(
            candidate.update_state(&Bar::new(3600, 109.0, 109.5, 107.0, 107.5, 1.0)),
            PatternState::Confirmed
        );
    }

    #[test]
    fn test_double_top_invalidates_above_its_own_extreme() {
        let detector = ChartPatternDetector::new(2.0);
        let mut candidate = detector
            .scan(&doppeltop_nodes())
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::DoubleTop)
            .expect("double top detected");
        assert_eq!(
            candidate.update_state(&Bar::new(3000, 118.0, 120.0, 117.0, 119.0, 1.0)),
            PatternState::Invalidated
        );
    }

    #[test]
    fn test_double_top_needs_the_two_highs_to_match() {
        // Bei enger Toleranz sind 118 und 117,4 nicht mehr dasselbe Niveau — dieselbe Pivotfolge,
        // ein anderes Ergebnis. Genau das ist der Ermessensspielraum.
        let eng = ChartPatternDetector::new(0.1);
        assert!(!eng
            .scan(&doppeltop_nodes())
            .iter()
            .any(|c| c.kind == ChartPatternKind::DoubleTop));
    }

    #[test]
    fn test_double_bottom_mirrors() {
        let nodes = vec![
            node(0, 90.0, false),
            node(1200, 100.0, true),
            node(2400, 90.5, false),
        ];
        let mut candidate = ChartPatternDetector::new(2.0)
            .scan(&nodes)
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::DoubleBottom)
            .expect("double bottom detected");
        assert_eq!(
            candidate.update_state(&Bar::new(3000, 100.5, 102.0, 100.0, 101.5, 1.0)),
            PatternState::Confirmed
        );
    }

    #[test]
    fn test_triple_top_uses_the_lower_trough_as_neckline() {
        // Das nähere Zwischentief zu nehmen wäre großzügiger — und „bestätigt" hieße dann bei
        // Dreifach etwas anderes als bei Doppel.
        let nodes = vec![
            node(0, 120.0, true),
            node(600, 112.0, false),
            node(1200, 119.5, true),
            node(1800, 108.0, false),
            node(2400, 120.4, true),
        ];
        let mut candidate = ChartPatternDetector::new(2.0)
            .scan(&nodes)
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::TripleTop)
            .expect("triple top detected");

        // Unter dem höheren, aber über dem tieferen Zwischentief: noch nicht bestätigt.
        assert_eq!(
            candidate.update_state(&Bar::new(3000, 111.0, 111.5, 110.0, 110.0, 1.0)),
            PatternState::Forming
        );
        assert_eq!(
            candidate.update_state(&Bar::new(3600, 109.0, 109.2, 107.0, 107.4, 1.0)),
            PatternState::Confirmed
        );
    }

    /// Fünf Pivots: Hoch, Tief, höheres Hoch, Tief, ähnlich hohes Hoch.
    fn sks_nodes() -> Vec<ZigZagNode> {
        vec![
            node(0, 112.0, true),
            node(600, 104.0, false),
            node(1200, 124.0, true),
            node(1800, 103.4, false),
            node(2400, 111.6, true),
        ]
    }

    #[test]
    fn test_head_and_shoulders_confirms_on_the_sloping_neckline() {
        let mut candidate = ChartPatternDetector::new(2.0)
            .scan(&sks_nodes())
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::HeadAndShoulders)
            .expect("head and shoulders detected");
        assert_eq!(
            candidate.state,
            PatternState::Forming,
            "die rechte Schulter bestätigt nichts"
        );

        // Die Nackenlinie fällt von 104 bei t=600 auf 103,4 bei t=1800: −0,0005 je Zeiteinheit.
        // Bei t=3000 liegt sie damit bei 102,8.
        let neckline = candidate.lower_line.expect("neckline");
        assert!((neckline.value_at(3000) - 102.8).abs() < 1e-9);

        assert_eq!(
            candidate.update_state(&Bar::new(3000, 103.5, 103.6, 103.0, 103.2, 1.0)),
            PatternState::Forming,
            "über der Linie, obwohl unter dem tieferen Zwischentief"
        );
        assert_eq!(
            candidate.update_state(&Bar::new(3600, 103.0, 103.1, 101.0, 101.5, 1.0)),
            PatternState::Confirmed
        );
    }

    #[test]
    fn test_head_and_shoulders_needs_a_head() {
        // Ohne überragendes mittleres Hoch bleibt es eine Folge von drei Hochs.
        let nodes = vec![
            node(0, 112.0, true),
            node(600, 104.0, false),
            node(1200, 111.0, true),
            node(1800, 103.4, false),
            node(2400, 111.6, true),
        ];
        assert!(!ChartPatternDetector::new(2.0)
            .scan(&nodes)
            .iter()
            .any(|c| c.kind == ChartPatternKind::HeadAndShoulders));
    }

    #[test]
    fn test_inverse_head_and_shoulders_mirrors() {
        let nodes = vec![
            node(0, 98.0, false),
            node(600, 106.0, true),
            node(1200, 86.0, false),
            node(1800, 106.6, true),
            node(2400, 98.4, false),
        ];
        let mut candidate = ChartPatternDetector::new(2.0)
            .scan(&nodes)
            .into_iter()
            .find(|c| c.kind == ChartPatternKind::InverseHeadAndShoulders)
            .expect("inverse head and shoulders detected");
        assert_eq!(
            candidate.update_state(&Bar::new(3000, 106.0, 108.0, 105.5, 107.5, 1.0)),
            PatternState::Confirmed
        );
    }
}
