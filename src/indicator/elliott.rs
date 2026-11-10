//! Elliott Wave and Fibonacci pattern validation: rule-checked impulses, diagonals, corrections
//! (Zigzag/Flat variants), triangles and combinations, C-setup projection, pullback quality scoring,
//! and a reaction-memory tracker — built on [`super::zigzag_advanced::ZigZagNode`] sequences,
//! reusing [`super::price_levels::swing_fibonacci_levels`] for level projection rather than
//! duplicating the ratio table.
//!
//! Wave counting is inherently interpretive; this validates a *given* labeling against Elliott's
//! documented structural rules (not heuristics about which count is "right") and scores how
//! Fibonacci-clean the retracements are — a rule checker and quality scorer, not a wave counter
//! that discovers labelings on its own.
//!
//! # The pivot sequence every validator requires
//!
//! A labeling is a sequence of pivots, and the validators only evaluate what is one:
//!
//! - the number of nodes the structure has (6 for an impulse, a diagonal or a triangle, 4 for a
//!   correction, 8 or 12 for a combination),
//! - highs and lows alternate,
//! - timestamps do not decrease (equal timestamps are accepted: one bar can carry both a pivot high
//!   and a pivot low),
//! - every high lies above the node before it and every low below it, so every leg has a length
//!   and the sequence a direction ([`WaveDirection`]).
//!
//! A sequence that fails one of these is not a wrong count but no count at all. The `check_*`
//! functions return a [`SequenceError`] naming the node; the `validate_*` functions return `None`.
//! One placement is left to a rule instead: the end of wave C of a correction, whose misplacement
//! has always been reported as the violation `wave_c_must_extend_past_b`.
//!
//! # Rules, guidelines and this crate's conventions
//!
//! A **rule** makes a labeling invalid: it appears in `violations` and clears `valid`. A
//! **guideline** describes what a structure usually looks like: departing from it is listed as a
//! [`GuidelineDeviation`] (in `guidelines` of a correction) and leaves `valid` untouched. Only
//! what the literature states as a rule is reported as a violation. The reference is the "Summary
//! of Rules and Guidelines for Waves" of later editions of Frost and Prechter's *Elliott Wave
//! Principle*; it was consulted in reproductions, not in the book itself. Each `check_*` function
//! lists the rules it checks. Whatever needs the subdivision of a wave or its place in a larger
//! count cannot be checked, since neither is in the pivots of one degree.
//!
//! Where that summary leaves a choice open, the choice is **this crate's convention** and is
//! named as such on the item it belongs to: how a tie is counted, where four points cannot tell a
//! zigzag from a flat, which triangle a mixed sequence is measured against, and the tolerance for
//! "the same level". The thresholds of these checks are public constants whose comments state
//! their origin.
//!
//! A sequence and its mirror image (highs for lows, every price negated) give the same result
//! with the opposite [`WaveDirection`], to the last bit: each validator evaluates a falling
//! sequence as the rising one it mirrors.

use std::fmt;

use crate::model::SeriesCapabilities;
use crate::stats::rolling_median;

use super::price_levels::{swing_fibonacci_levels, PriceLevel};
use super::zigzag_advanced::ZigZagNode;

/// A rule violation found while validating a labeling. `rule` is a stable identifier, `detail`
/// the sentence that goes with it.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleViolation {
    pub rule: String,
    pub detail: String,
}

/// A guideline a labeling departs from. A guideline describes what a structure usually looks
/// like; departing from it never makes a labeling invalid, so deviations are listed apart from
/// [`RuleViolation`]s and do not enter `valid`. `guideline` is a stable identifier, `detail` the
/// sentence that goes with it.
#[derive(Debug, Clone, PartialEq)]
pub struct GuidelineDeviation {
    pub guideline: String,
    pub detail: String,
}

fn violation(rule: &str, detail: &str) -> RuleViolation {
    RuleViolation {
        rule: rule.to_string(),
        detail: detail.to_string(),
    }
}

fn deviation(guideline: &str, detail: impl Into<String>) -> GuidelineDeviation {
    GuidelineDeviation {
        guideline: guideline.to_string(),
        detail: detail.into(),
    }
}

/// The direction of a labeled sequence: the direction of its first leg, from node 0 to node 1.
/// For a correction that is the direction of wave A, for a triangle of wave a.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveDirection {
    /// Node 0 is a low and the first leg rises.
    Up,
    /// Node 0 is a high and the first leg falls.
    Down,
}

/// Why a node sequence is not a pivot sequence a validator can evaluate (see the module
/// documentation). `index` is the position in the slice of the node that breaks the condition;
/// it is always at least 1, since each condition compares a node with the one before it.
///
/// The conditions are tested in the order of the variants, each over the whole sequence, and the
/// first node that breaks the first failing condition is reported.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SequenceError {
    /// The slice does not have a node count the structure accepts.
    NodeCount {
        found: usize,
        expected: &'static [usize],
    },
    /// Node `index` is of the same kind (high or low) as the node before it.
    NotAlternating { index: usize },
    /// Node `index` has an earlier timestamp than the node before it.
    TimeOrder { index: usize },
    /// Node `index` does not lie beyond the node before it on its own side: a high that is not
    /// above it or a low that is not below it. An equal price counts, as does a price that is not
    /// a number.
    PivotPlacement { index: usize },
}

impl SequenceError {
    /// The node the error names, or `None` for a wrong node count.
    pub fn index(&self) -> Option<usize> {
        match *self {
            SequenceError::NodeCount { .. } => None,
            SequenceError::NotAlternating { index }
            | SequenceError::TimeOrder { index }
            | SequenceError::PivotPlacement { index } => Some(index),
        }
    }
}

impl fmt::Display for SequenceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {
            SequenceError::NodeCount { found, expected } => {
                let counts: Vec<String> = expected.iter().map(|n| n.to_string()).collect();
                write!(f, "expected {} nodes, found {found}", counts.join(" or "))
            }
            SequenceError::NotAlternating { index } => write!(
                f,
                "nodes {} and {index} are of the same kind; highs and lows must alternate",
                index - 1
            ),
            SequenceError::TimeOrder { index } => write!(
                f,
                "node {index} is earlier than node {}; timestamps must not decrease",
                index - 1
            ),
            SequenceError::PivotPlacement { index } => write!(
                f,
                "node {index} does not lie beyond node {}: a high must be above the node before \
                 it, a low below",
                index - 1
            ),
        }
    }
}

impl std::error::Error for SequenceError {}

/// Tests the pivot-sequence conditions of the module documentation and returns the direction of
/// the first leg. `placed_by_rule` lists the nodes whose placement a rule of the structure reports
/// instead (the end of wave C of a correction).
fn pivot_sequence(
    nodes: &[ZigZagNode],
    expected: &'static [usize],
    placed_by_rule: &[usize],
) -> Result<WaveDirection, SequenceError> {
    if !expected.contains(&nodes.len()) {
        return Err(SequenceError::NodeCount {
            found: nodes.len(),
            expected,
        });
    }
    let later = 1..nodes.len();
    if let Some(index) = later
        .clone()
        .find(|&i| nodes[i].is_high == nodes[i - 1].is_high)
    {
        return Err(SequenceError::NotAlternating { index });
    }
    if let Some(index) = later
        .clone()
        .find(|&i| nodes[i].timestamp < nodes[i - 1].timestamp)
    {
        return Err(SequenceError::TimeOrder { index });
    }
    let placed = |i: usize| {
        if nodes[i].is_high {
            nodes[i].price > nodes[i - 1].price
        } else {
            nodes[i].price < nodes[i - 1].price
        }
    };
    if let Some(index) = later
        .clone()
        .find(|&i| !placed_by_rule.contains(&i) && !placed(i))
    {
        return Err(SequenceError::PivotPlacement { index });
    }
    Ok(if nodes[0].is_high {
        WaveDirection::Down
    } else {
        WaveDirection::Up
    })
}

/// The prices of a checked sequence, oriented so that the first leg rises: for a falling sequence
/// every price is negated. The rules are then written once, for the rising case. Negation is
/// exact, so a sequence and its mirror image produce the same numbers.
fn oriented<const N: usize>(nodes: &[ZigZagNode], direction: WaveDirection) -> [f64; N] {
    let sign = match direction {
        WaveDirection::Up => 1.0,
        WaveDirection::Down => -1.0,
    };
    std::array::from_fn(|i| sign * nodes[i].price)
}

/// A flat's minimum retracement: wave B retraces at least 90 percent of wave A. Rule in Frost
/// and Prechter's summary of rules ("Flat"). Below it, four points can only be a zigzag.
pub const FLAT_B_MIN_RETRACE: f64 = 0.90;

/// The retracement of wave B above which a flat whose wave C ends beyond the end of wave A is
/// called an expanded flat. Definition in Frost and Prechter's summary of rules ("more than 105
/// percent"), hence compared strictly.
pub const EXPANDED_FLAT_B_MIN: f64 = 1.05;

/// The retracement of wave B a zigzag typically stays under. Guideline in Frost and Prechter's
/// summary ("typically retraces 38 to 79 percent"); writing 79 percent as the Fibonacci ratio
/// 0.786 is this crate's convention.
pub const ZIGZAG_B_TYPICAL_MAX: f64 = 0.786;

/// The retracement of wave B a flat usually stays under, and the default of
/// [`CorrectionOptions::flat_b_typical_max`]. Guideline in Frost and Prechter's summary ("usually
/// retraces between 100 and 138 percent"); writing 138 percent as 1.382 is this crate's
/// convention.
pub const FLAT_B_TYPICAL_MAX: f64 = 1.382;

/// In an expanding triangle, waves b, c and d each retrace at least this much of the wave before
/// them. Rule in Frost and Prechter's summary ("at least 100 percent").
pub const EXPANDING_TRIANGLE_RETRACE_MIN: f64 = 1.0;

/// In an expanding triangle, waves b, c and d each retrace at most this much of the wave before
/// them. Rule in Frost and Prechter's summary ("no more than 150 percent").
pub const EXPANDING_TRIANGLE_RETRACE_MAX: f64 = 1.5;

/// How far apart the ends of waves b and d may lie, as a share of the length of wave b, to count
/// as "essentially the same level" (barrier triangle), and the default of
/// [`TriangleOptions::barrier_tolerance`]. The literature gives no number; 5 percent of wave b is
/// this crate's convention.
pub const BARRIER_LEVEL_TOLERANCE: f64 = 0.05;

/// The form of a three-wave correction labeled `[0, A, B, C]`, read from where waves B and C end.
/// See [`check_correction`] for the boundaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionVariant {
    /// Wave B retraces less than [`FLAT_B_MIN_RETRACE`] of wave A, or wave A is asserted to
    /// subdivide into five waves.
    Zigzag,
    /// A flat that is neither expanded nor running: wave B ends near the start of wave A.
    Flat,
    /// A flat whose wave B retraces more than [`EXPANDED_FLAT_B_MIN`] of wave A and whose wave C
    /// ends beyond the end of wave A.
    ExpandedFlat,
    /// A flat whose wave B ends beyond the start of wave A and whose wave C does not end beyond
    /// the end of wave A.
    RunningFlat,
}

/// How many waves a wave subdivides into on the next lower degree. Not in the pivots of one
/// degree: where a validator takes it, it is an assertion of whoever made the count, like
/// [`DiagonalKind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WaveSubdivision {
    /// Five waves (an impulse or a diagonal).
    Five,
    /// Three waves (a correction).
    Three,
}

/// What [`check_correction`] takes besides the four nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CorrectionOptions {
    /// The asserted subdivision of wave A, which is what separates the two families in the
    /// literature: five waves in a zigzag (5-3-5), three in a flat (3-3-5). With `None` the family
    /// is read from the retracement of wave B alone, and each family's rule about wave B then
    /// holds by construction; with an assertion that rule can fail.
    pub wave_a: Option<WaveSubdivision>,
    /// The retracement of wave B above which a flat is reported as departing from the guideline
    /// `flat_b_retrace_bound`. Defaults to [`FLAT_B_TYPICAL_MAX`]. The literature gives a usual
    /// range and no upper limit, so exceeding it is never a violation.
    pub flat_b_typical_max: f64,
}

impl Default for CorrectionOptions {
    fn default() -> Self {
        Self {
            wave_a: None,
            flat_b_typical_max: FLAT_B_TYPICAL_MAX,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseValidation {
    /// Whether the impulse rises or falls: the direction of wave 1.
    pub direction: WaveDirection,
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
    /// How proportionally "clean" wave 2 and wave 4 are relative to common Fibonacci retracement
    /// ratios (0.382/0.5/0.618): `1.0` = both land close to a standard ratio, decaying with
    /// distance from the nearest one.
    pub pullback_quality: f64,
    /// Which series this validation was computed on, if the caller attached one via
    /// [`ImpulseValidation::with_capabilities`]. `None` by default: a validation is only
    /// meaningful for the series it was computed on (session cut, roll/adjustment, provenance),
    /// so a stored/exported result should carry this rather than be re-checked
    /// against a different series later without knowing it no longer applies.
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl ImpulseValidation {
    /// Tags this validation with the series it was computed on. See
    /// [`ImpulseValidation::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionValidation {
    pub variant: CorrectionVariant,
    /// The direction of wave A.
    pub direction: WaveDirection,
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
    /// Guidelines of the variant the correction departs from. They do not enter `valid`.
    pub guidelines: Vec<GuidelineDeviation>,
    pub pullback_quality: f64,
    /// See [`ImpulseValidation::series_capabilities`].
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl CorrectionValidation {
    /// Tags this validation with the series it was computed on. See
    /// [`ImpulseValidation::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

fn nearest_fib_distance(ratio: f64) -> f64 {
    const COMMON: [f64; 3] = [0.382, 0.5, 0.618];
    COMMON
        .iter()
        .map(|r| (r - ratio).abs())
        .fold(f64::INFINITY, f64::min)
}

/// Validates a 6-node impulse labeled `[0, 1, 2, 3, 4, 5]`, rising or falling, against three
/// rules of Frost and Prechter's summary. Written for a rising impulse; a falling one mirrors
/// every comparison:
///
/// - `wave2_no_full_retrace`: wave 2 never moves beyond the start of wave 1 (`P2 > P0`),
/// - `wave3_not_shortest`: wave 3 is never the shortest of waves 1, 3 and 5 (violated when it is
///   shorter than both),
/// - `wave4_no_overlap`: wave 4 never moves beyond the end of wave 1 (`P4 > P1`).
///
/// A wave 2 that ends exactly at the start of wave 1, or a wave 4 exactly at the end of wave 1,
/// violates its rule: counting the tie as a violation is this crate's convention for the impulse
/// and the diagonal, kept from earlier versions. It is the stricter reading; the corrections and
/// triangles, checked later, count a tie as not beyond. The summary's further rule that wave 3
/// always moves beyond the end of wave 1 follows from `wave4_no_overlap` on a pivot sequence. Its
/// rules about how the waves subdivide, and that waves 1, 3 and 5 are never all extended, are not
/// in the six pivots.
///
/// Returns a [`SequenceError`] if `nodes` is not a pivot sequence of six nodes (see the module
/// documentation).
pub fn check_impulse(nodes: &[ZigZagNode]) -> Result<ImpulseValidation, SequenceError> {
    let direction = pivot_sequence(nodes, &[6], &[])?;
    let [p0, p1, p2, p3, p4, p5] = oriented(nodes, direction);

    let mut violations = Vec::new();

    if p2 <= p0 {
        violations.push(violation(
            "wave2_no_full_retrace",
            "Wave 2 retraced beyond the start of wave 1",
        ));
    }

    let len1 = p1 - p0;
    let len3 = p3 - p2;
    let len5 = p5 - p4;
    if len3 < len1 && len3 < len5 {
        violations.push(violation(
            "wave3_not_shortest",
            "Wave 3 is the shortest of waves 1, 3, and 5",
        ));
    }

    if p4 <= p1 {
        violations.push(violation(
            "wave4_no_overlap",
            "Wave 4 entered wave 1's price territory",
        ));
    }

    let retrace2 = (p2 - p0).abs() / len1;
    let retrace4 = (p3 - p4) / len3;
    let pullback_quality = if retrace2.is_finite() && retrace4.is_finite() {
        let d2 = nearest_fib_distance(retrace2);
        let d4 = nearest_fib_distance(retrace4);
        (1.0 - (d2 + d4)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Ok(ImpulseValidation {
        direction,
        valid: violations.is_empty(),
        violations,
        pullback_quality,
        series_capabilities: None,
    })
}

/// [`check_impulse`] without the reason for a refusal: `None` if `nodes` is not a pivot sequence
/// of six nodes.
pub fn validate_impulse(nodes: &[ZigZagNode]) -> Option<ImpulseValidation> {
    check_impulse(nodes).ok()
}

/// Validates a 4-node correction labeled `[0, A, B, C]` and names its form.
///
/// With `r` the share of wave A that wave B retraces (`|B − A| / |A − 0|`):
///
/// | Wave B | Wave C | Variant |
/// |---|---|---|
/// | `r` < 0.90 | any | [`CorrectionVariant::Zigzag`] |
/// | `r` ≥ 0.90, not beyond the start of A | any | [`CorrectionVariant::Flat`] |
/// | beyond the start of A, `r` ≤ 1.05 | ends beyond the end of A | [`CorrectionVariant::Flat`] |
/// | `r` > 1.05 | ends beyond the end of A | [`CorrectionVariant::ExpandedFlat`] |
/// | beyond the start of A | does not end beyond the end of A | [`CorrectionVariant::RunningFlat`] |
///
/// Every sequence falls into exactly one row. "Beyond" is decided on the prices themselves and is
/// strict; wave B beyond the start of wave A is `r` > 1.00. The numbers 0.90
/// ([`FLAT_B_MIN_RETRACE`]) and 1.05 ([`EXPANDED_FLAT_B_MIN`]) and the two conditions on wave C
/// are the definitions of Frost and Prechter's summary. Two things are this crate's convention.
/// A tie is not beyond: a wave C that ends exactly at the end of wave A, or a wave B exactly at
/// the start of wave A. And from 0.90 on the form is a flat: the summary allows a zigzag's wave B
/// to reach the start of wave A and tells the two families apart by the subdivision of wave A,
/// which four points do not contain. [`CorrectionOptions::wave_a`] takes that subdivision as an
/// assertion: `Five` makes the correction a zigzag whatever `r` is, `Three` a flat whose variant
/// follows the four flat rows, also below 0.90.
///
/// Rules (in `violations`):
///
/// - `wave_c_must_extend_past_b`: wave C continues past the end of wave B in wave A's direction.
///   Without that there are no three waves.
/// - `zigzag_b_not_beyond_start_of_a`: in a zigzag, wave B never moves beyond the start of wave A.
///   Can only fail when wave A is asserted to be `Five`.
/// - `flat_b_min_retrace`: in a flat, wave B retraces at least 90 percent of wave A. Can only fail
///   when wave A is asserted to be `Three`.
///
/// Guidelines (in `guidelines`, never invalidating):
///
/// - `zigzag_b_retrace_bound`: a zigzag's wave B retraces at most [`ZIGZAG_B_TYPICAL_MAX`] of
///   wave A. Up to 0.39 this was reported as a violation.
/// - `zigzag_c_ends_beyond_a`: a zigzag's wave C ends beyond the end of wave A.
/// - `flat_c_ends_beyond_a`: the wave C of a [`CorrectionVariant::Flat`] ends beyond the end of
///   wave A. Not reported for a running flat, which is defined by the opposite.
/// - `flat_b_retrace_bound`: a flat's wave B retraces at most
///   [`CorrectionOptions::flat_b_typical_max`] of wave A, in every flat variant.
///
/// That four points are a correction at all is not something they can show: the first four
/// pivots of an impulse satisfy every rule of a zigzag, whose outline is the same. What separates
/// them is the count they stand in.
///
/// `pullback_quality` is, for a zigzag, one minus the distance of `min(r, 1)` to the nearest of
/// 0.382, 0.5 and 0.618, and for the flat variants one minus `|1 − min(c, 2)|` with `c` the
/// length of wave C in lengths of wave A, both limited to 0..1.
///
/// Returns a [`SequenceError`] if `nodes` is not a pivot sequence of four nodes (see the module
/// documentation); the placement of C is the rule above.
pub fn check_correction(
    nodes: &[ZigZagNode],
    options: &CorrectionOptions,
) -> Result<CorrectionValidation, SequenceError> {
    let direction = pivot_sequence(nodes, &[4], &[3])?;
    Ok(correction(nodes, direction, options))
}

/// The evaluation behind [`check_correction`], for four nodes already known to be a pivot
/// sequence up to the end of wave B.
fn correction(
    nodes: &[ZigZagNode],
    direction: WaveDirection,
    options: &CorrectionOptions,
) -> CorrectionValidation {
    let [start, a, b, c] = oriented(nodes, direction);
    let leg_a = a - start;
    let b_retrace = (a - b) / leg_a;
    let c_vs_a = (c - b).abs() / leg_a;
    let b_beyond_start_of_a = b < start;
    let c_continues = c > b;
    let c_beyond_a = c > a;

    let flat = match options.wave_a {
        Some(WaveSubdivision::Five) => false,
        Some(WaveSubdivision::Three) => true,
        None => b_retrace >= FLAT_B_MIN_RETRACE,
    };
    let variant = if !flat {
        CorrectionVariant::Zigzag
    } else if b_beyond_start_of_a && !c_beyond_a {
        CorrectionVariant::RunningFlat
    } else if b_retrace > EXPANDED_FLAT_B_MIN && c_beyond_a {
        CorrectionVariant::ExpandedFlat
    } else {
        CorrectionVariant::Flat
    };

    let mut violations = Vec::new();
    let mut guidelines = Vec::new();

    if !c_continues {
        violations.push(violation(
            "wave_c_must_extend_past_b",
            "Wave C did not continue past wave B in wave A's direction",
        ));
    }
    if flat {
        if b_retrace < FLAT_B_MIN_RETRACE {
            violations.push(violation(
                "flat_b_min_retrace",
                "Wave B retraced less than 90% of wave A",
            ));
        }
        if b_retrace > options.flat_b_typical_max {
            guidelines.push(deviation(
                "flat_b_retrace_bound",
                format!(
                    "Wave B retraced more than a Flat's typical bound ({:.1}%)",
                    options.flat_b_typical_max * 100.0
                ),
            ));
        }
        if variant == CorrectionVariant::Flat && !c_beyond_a {
            guidelines.push(deviation(
                "flat_c_ends_beyond_a",
                "Wave C did not end beyond the end of wave A",
            ));
        }
    } else {
        if b_beyond_start_of_a {
            violations.push(violation(
                "zigzag_b_not_beyond_start_of_a",
                "Wave B moved beyond the start of wave A",
            ));
        }
        if b_retrace > ZIGZAG_B_TYPICAL_MAX {
            guidelines.push(deviation(
                "zigzag_b_retrace_bound",
                "Wave B retraced more than a Zigzag's typical bound (78.6%) without qualifying as a Flat",
            ));
        }
        if !c_beyond_a {
            guidelines.push(deviation(
                "zigzag_c_ends_beyond_a",
                "Wave C did not end beyond the end of wave A",
            ));
        }
    }

    let quality_ref = if flat {
        (1.0 - c_vs_a.min(2.0)).abs().min(1.0)
    } else {
        nearest_fib_distance(b_retrace.min(1.0))
    };
    let pullback_quality = (1.0 - quality_ref).clamp(0.0, 1.0);

    CorrectionValidation {
        variant,
        direction,
        valid: violations.is_empty(),
        violations,
        guidelines,
        pullback_quality,
        series_capabilities: None,
    }
}

/// [`check_correction`] with the default [`CorrectionOptions`] and without the reason for a
/// refusal: `None` if `nodes` is not a pivot sequence of four nodes.
pub fn validate_correction(nodes: &[ZigZagNode]) -> Option<CorrectionValidation> {
    check_correction(nodes, &CorrectionOptions::default()).ok()
}

/// Projects Fibonacci "C-setup" target levels from a validated correction's A and B legs, reusing
/// [`swing_fibonacci_levels`] rather than a separate ratio table. `is_uptrend` matches that
/// function's convention: `true` if wave A ran low-to-high.
pub fn c_setup_levels(wave_a_start: f64, wave_a_end: f64, is_uptrend: bool) -> Vec<PriceLevel> {
    let (high, low) = if wave_a_end >= wave_a_start {
        (wave_a_end, wave_a_start)
    } else {
        (wave_a_start, wave_a_end)
    };
    swing_fibonacci_levels(high, low, is_uptrend)
}

/// Where in an impulse-shaped 6-node sequence a diagonal sits. Leading diagonals occupy wave 1 (or
/// A); ending diagonals occupy wave 5 (or C) — the distinction does not change the geometry check
/// below, only which rule relaxations apply in the surrounding theory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagonalKind {
    Leading,
    Ending,
}

/// Whether the two boundary lines (through waves 1-3-5 and 2-4) converge or diverge. Approximated
/// here from wave lengths rather than line intersection: a contracting diagonal's waves shrink
/// leg over leg, an expanding diagonal's waves grow leg over leg. This is the same simplification
/// [`super::chart_patterns`] makes for wedges — a converging/diverging channel read off the pivots
/// rather than a fitted line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagonalVariant {
    Contracting,
    Expanding,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DiagonalValidation {
    pub kind: DiagonalKind,
    pub variant: DiagonalVariant,
    /// Whether the diagonal rises or falls: the direction of wave 1.
    pub direction: WaveDirection,
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
    /// Whether wave 4 overlaps wave 1's price territory — expected and permitted in a diagonal,
    /// unlike a plain impulse. Carried through rather than discarded because it is the one
    /// property that distinguishes a diagonal from an impulse at the rule level.
    pub wave4_overlaps_wave1: bool,
    /// See [`ImpulseValidation::series_capabilities`].
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl DiagonalValidation {
    /// Tags this validation with the series it was computed on. See
    /// [`ImpulseValidation::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

/// Validates a 6-node diagonal labeled `[0, 1, 2, 3, 4, 5]` — same shape as [`check_impulse`],
/// with `wave4_no_overlap` relaxed (wave 4 may enter wave 1's territory) and the other two rules
/// retained. The variant is [`DiagonalVariant::Contracting`] when waves 1, 3, 5 and waves 2, 4
/// each shrink, and [`DiagonalVariant::Expanding`] for every other sequence.
///
/// Frost and Prechter's summary states further rules for diagonals that six pivots could decide
/// and that are not checked here: wave 3 moves beyond the end of wave 1, wave 4 never moves
/// beyond the end of wave 2 and ends within wave 1's territory, each wave is shorter (contracting)
/// or longer (expanding) than the wave two before it, and wave 5 of a leading or an expanding
/// diagonal ends beyond the end of wave 3.
///
/// Returns a [`SequenceError`] if `nodes` is not a pivot sequence of six nodes (see the module
/// documentation).
pub fn check_diagonal(
    nodes: &[ZigZagNode],
    kind: DiagonalKind,
) -> Result<DiagonalValidation, SequenceError> {
    let direction = pivot_sequence(nodes, &[6], &[])?;
    let [p0, p1, p2, p3, p4, p5] = oriented(nodes, direction);

    let mut violations = Vec::new();

    if p2 <= p0 {
        violations.push(violation(
            "wave2_no_full_retrace",
            "Wave 2 retraced beyond the start of wave 1",
        ));
    }

    let len1 = p1 - p0;
    let len2 = p1 - p2;
    let len3 = p3 - p2;
    let len4 = p3 - p4;
    let len5 = p5 - p4;
    if len3 < len1 && len3 < len5 {
        violations.push(violation(
            "wave3_not_shortest",
            "Wave 3 is the shortest of waves 1, 3, and 5",
        ));
    }

    let wave4_overlaps_wave1 = p4 <= p1;

    let variant = if len5 < len3 && len3 < len1 && len4 < len2 {
        DiagonalVariant::Contracting
    } else {
        DiagonalVariant::Expanding
    };

    Ok(DiagonalValidation {
        kind,
        variant,
        direction,
        valid: violations.is_empty(),
        violations,
        wave4_overlaps_wave1,
        series_capabilities: None,
    })
}

/// [`check_diagonal`] without the reason for a refusal: `None` if `nodes` is not a pivot sequence
/// of six nodes.
pub fn validate_diagonal(nodes: &[ZigZagNode], kind: DiagonalKind) -> Option<DiagonalValidation> {
    check_diagonal(nodes, kind).ok()
}

/// The kind of a 5-leg (`a-b-c-d-e`) triangle. The literature names three: contracting, barrier
/// and expanding. A running triangle is not a fourth kind but a contracting or barrier triangle
/// whose wave b ends beyond the start of wave a; that is [`TriangleValidation::running`]. See
/// [`check_triangle`] for how a sequence is assigned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriangleVariant {
    /// Waves c, d and e each stay within the end of the wave two before them.
    Contracting,
    /// A contracting triangle whose waves b and d end at essentially the same level.
    Barrier,
    /// Waves c, d and e each move beyond the end of the wave two before them.
    Expanding,
}

/// What [`check_triangle`] takes besides the six nodes.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TriangleOptions {
    /// How far apart the ends of waves b and d may lie, as a share of the length of wave b, for
    /// the triangle to be a [`TriangleVariant::Barrier`]. Defaults to
    /// [`BARRIER_LEVEL_TOLERANCE`]. With `0.0` only an exact tie is a barrier triangle.
    pub barrier_tolerance: f64,
}

impl Default for TriangleOptions {
    fn default() -> Self {
        Self {
            barrier_tolerance: BARRIER_LEVEL_TOLERANCE,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriangleValidation {
    pub variant: TriangleVariant,
    /// `true` for a contracting or barrier triangle whose wave b ends beyond the start of wave a:
    /// the literature's running triangle. Always `false` for an expanding triangle, where the
    /// retracement rule of wave b covers the same pivot.
    pub running: bool,
    /// The direction of wave a.
    pub direction: WaveDirection,
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
    /// See [`ImpulseValidation::series_capabilities`].
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl TriangleValidation {
    /// Tags this validation with the series it was computed on. See
    /// [`ImpulseValidation::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

/// Validates a 6-node, 5-leg triangle labeled `[0, a, b, c, d, e]` and names its kind.
///
/// Three comparisons carry it, each between the end of a wave and the end of the wave two before
/// it, which ran the same way: c against a, d against b, e against c. A wave is *beyond* when it
/// ends strictly further in its own direction. Frost and Prechter's summary defines a contracting
/// triangle by none of the three being beyond and an expanding triangle by all three.
///
/// - **Variant.** [`TriangleVariant::Expanding`] if at least two of the three waves are beyond.
///   Otherwise [`TriangleVariant::Barrier`] if the ends of b and d lie within
///   [`TriangleOptions::barrier_tolerance`] of each other (as a share of the length of wave b),
///   else [`TriangleVariant::Contracting`]. Assigning a mixed sequence to the kind it breaks fewer
///   comparisons of, and the tolerance for "essentially the same level", are this crate's
///   conventions; so is counting a tie as not beyond.
/// - **Running.** In a contracting or barrier triangle, [`TriangleValidation::running`] says
///   whether b ends beyond the start of a. Node 0 enters nothing else.
///
/// Rules (in `violations`), all from the summary:
///
/// - Contracting: `triangle_c_not_beyond_a`, `triangle_d_not_beyond_b`,
///   `triangle_e_not_beyond_c`: wave c never moves beyond the end of wave a, d never beyond the
///   end of b, e never beyond the end of c.
/// - Barrier: `triangle_c_not_beyond_a` and `triangle_e_not_beyond_c`. Wave d is held by the
///   level condition instead and may end on either side of b within the tolerance.
/// - Expanding: `expanding_triangle_c_beyond_a`, `expanding_triangle_d_beyond_b`,
///   `expanding_triangle_e_beyond_c`: each of c, d and e moves beyond the end of the wave two
///   before it; and `expanding_triangle_b_retrace`, `expanding_triangle_c_retrace`,
///   `expanding_triangle_d_retrace`: each of b, c and d retraces at least
///   [`EXPANDING_TRIANGLE_RETRACE_MIN`] and at most [`EXPANDING_TRIANGLE_RETRACE_MAX`] of the wave
///   before it.
///
/// What the summary further requires of a triangle is not in six pivots and is not checked: that
/// its waves subdivide into threes (3-3-3-3-3), and where it stands in the larger count.
///
/// Returns a [`SequenceError`] if `nodes` is not a pivot sequence of six nodes (see the module
/// documentation).
pub fn check_triangle(
    nodes: &[ZigZagNode],
    options: &TriangleOptions,
) -> Result<TriangleValidation, SequenceError> {
    let direction = pivot_sequence(nodes, &[6], &[])?;
    // Oriented so that wave a rises: a, c and e end at highs, b and d at lows.
    let [start, a, b, c, d, e] = oriented(nodes, direction);

    let c_beyond_a = c > a;
    let d_beyond_b = d < b;
    let e_beyond_c = e > c;
    let beyond = [c_beyond_a, d_beyond_b, e_beyond_c]
        .iter()
        .filter(|&&is_beyond| is_beyond)
        .count();

    let len_a = a - start;
    let len_b = a - b;
    let len_c = c - b;
    let len_d = c - d;

    let variant = if beyond >= 2 {
        TriangleVariant::Expanding
    } else if (d - b).abs() <= options.barrier_tolerance * len_b {
        TriangleVariant::Barrier
    } else {
        TriangleVariant::Contracting
    };
    let running = variant != TriangleVariant::Expanding && b < start;

    let mut violations = Vec::new();
    if variant == TriangleVariant::Expanding {
        if !c_beyond_a {
            violations.push(violation(
                "expanding_triangle_c_beyond_a",
                "Wave C did not move beyond the end of wave A",
            ));
        }
        if !d_beyond_b {
            violations.push(violation(
                "expanding_triangle_d_beyond_b",
                "Wave D did not move beyond the end of wave B",
            ));
        }
        if !e_beyond_c {
            violations.push(violation(
                "expanding_triangle_e_beyond_c",
                "Wave E did not move beyond the end of wave C",
            ));
        }
        let retraces = [
            (
                len_b / len_a,
                "expanding_triangle_b_retrace",
                "Wave B retraced less than 100% or more than 150% of wave A",
            ),
            (
                len_c / len_b,
                "expanding_triangle_c_retrace",
                "Wave C retraced less than 100% or more than 150% of wave B",
            ),
            (
                len_d / len_c,
                "expanding_triangle_d_retrace",
                "Wave D retraced less than 100% or more than 150% of wave C",
            ),
        ];
        for (retrace, rule, detail) in retraces {
            if !(EXPANDING_TRIANGLE_RETRACE_MIN..=EXPANDING_TRIANGLE_RETRACE_MAX).contains(&retrace)
            {
                violations.push(violation(rule, detail));
            }
        }
    } else {
        if c_beyond_a {
            violations.push(violation(
                "triangle_c_not_beyond_a",
                "Wave C moved beyond the end of wave A",
            ));
        }
        if d_beyond_b && variant == TriangleVariant::Contracting {
            violations.push(violation(
                "triangle_d_not_beyond_b",
                "Wave D moved beyond the end of wave B",
            ));
        }
        if e_beyond_c {
            violations.push(violation(
                "triangle_e_not_beyond_c",
                "Wave E moved beyond the end of wave C",
            ));
        }
    }

    Ok(TriangleValidation {
        variant,
        running,
        direction,
        valid: violations.is_empty(),
        violations,
        series_capabilities: None,
    })
}

/// [`check_triangle`] with the default [`TriangleOptions`] and without the reason for a refusal:
/// `None` if `nodes` is not a pivot sequence of six nodes.
pub fn validate_triangle(nodes: &[ZigZagNode]) -> Option<TriangleValidation> {
    check_triangle(nodes, &TriangleOptions::default()).ok()
}

/// A W-X-Y or W-X-Y-X-Z combination correction: two or three simple corrections
/// ([`check_correction`]) joined by connecting "X" waves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CombinationVariant {
    /// W-X-Y: a double three.
    DoubleThree,
    /// W-X-Y-X-Z: a triple three.
    TripleThree,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CombinationValidation {
    pub variant: CombinationVariant,
    /// One validated correction per W/Y(/Z) segment, in order.
    pub segments: Vec<CorrectionValidation>,
    /// `true` if every segment in `segments` is individually valid.
    pub valid: bool,
    /// See [`ImpulseValidation::series_capabilities`].
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl CombinationValidation {
    /// Tags this validation with the series it was computed on. See
    /// [`ImpulseValidation::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

/// Validates a combination correction from its full pivot sequence: 8 nodes for a W-X-Y double
/// three (`[0, Wa, Wb, Wc, X, Ya, Yb, Yc]`), 12 for a W-X-Y-X-Z triple three (append
/// `[X2, Za, Zb, Zc]`). Each connecting "X" wave is treated as a single atomic leg rather than a
/// correction of its own — consistent with how [`check_impulse`] and [`check_diagonal`] treat
/// each numbered wave as atomic rather than recursing into its sub-degree. Every segment is
/// evaluated as [`check_correction`] does, with the same `options`.
///
/// Returns a [`SequenceError`] if `nodes` is not a pivot sequence of 8 or 12 nodes (see the module
/// documentation). As in a single correction, the placement of each segment's wave C is left to
/// the rule `wave_c_must_extend_past_b`.
pub fn check_combination(
    nodes: &[ZigZagNode],
    options: &CorrectionOptions,
) -> Result<CombinationValidation, SequenceError> {
    let direction = pivot_sequence(nodes, &[8, 12], &[3, 7, 11])?;

    let variant = if nodes.len() == 8 {
        CombinationVariant::DoubleThree
    } else {
        CombinationVariant::TripleThree
    };

    // Each correction segment is 4 nodes: `[start, a, b, c]`. The connecting X leg is the single
    // move from one segment's `c` to the next segment's `start` — those are adjacent nodes in
    // `nodes` (e.g. `Wc` at index 3, `X` at index 4), not a shared one, and the next segment's
    // `start` (`X`) is where the following correction begins. So segments simply tile `nodes` in
    // non-overlapping groups of 4: `[0..4]`, `[4..8]`, ... Every segment starts at a node of the
    // same kind as node 0, so all of them run in the direction of the sequence.
    let segments: Vec<CorrectionValidation> = nodes
        .chunks(4)
        .map(|segment| correction(segment, direction, options))
        .collect();

    let valid = segments.iter().all(|s| s.valid);

    Ok(CombinationValidation {
        variant,
        segments,
        valid,
        series_capabilities: None,
    })
}

/// [`check_combination`] with the default [`CorrectionOptions`] and without the reason for a
/// refusal: `None` if `nodes` is not a pivot sequence of 8 or 12 nodes.
pub fn validate_combination(nodes: &[ZigZagNode]) -> Option<CombinationValidation> {
    check_combination(nodes, &CorrectionOptions::default()).ok()
}

/// Which of waves 1, 3, or 5 is "extended" — long enough relative to the other two that it reads
/// as the dominant wave of the impulse. Elliott's guideline: extended when its length is at least
/// 1.618 times the longer of the other two. Returns `None` if no wave qualifies, or if `nodes` is
/// not a pivot sequence of six nodes (see the module documentation).
pub fn identify_extended_wave(nodes: &[ZigZagNode]) -> Option<u8> {
    let direction = pivot_sequence(nodes, &[6], &[]).ok()?;
    let [p0, p1, p2, p3, p4, p5] = oriented(nodes, direction);

    let len1 = p1 - p0;
    let len3 = p3 - p2;
    let len5 = p5 - p4;

    const EXTENSION_RATIO: f64 = 1.618;

    if len1 >= EXTENSION_RATIO * len3.max(len5) {
        Some(1)
    } else if len3 >= EXTENSION_RATIO * len1.max(len5) {
        Some(3)
    } else if len5 >= EXTENSION_RATIO * len1.max(len3) {
        Some(5)
    } else {
        None
    }
}

/// A truncated (failed) fifth: wave 5 fails to move beyond wave 3's extreme, even though the three
/// cardinal rules ([`check_impulse`]) may still all hold — this is a guideline violation, not a
/// rule violation. Returns `None` if `nodes` is not a pivot sequence of six nodes (see the module
/// documentation).
pub fn is_truncated_fifth(nodes: &[ZigZagNode]) -> Option<bool> {
    let direction = pivot_sequence(nodes, &[6], &[]).ok()?;
    let [_, _, _, p3, _, p5] = oriented::<6>(nodes, direction);
    Some(p5 <= p3)
}

/// Empirically tracks how often (and by how much) price has historically reacted at each standard
/// Fibonacci ratio bucket, so future expectations can be calibrated from actual observed behavior
/// instead of textbook assumptions alone.
#[derive(Debug, Clone, Default)]
pub struct FibonacciReactionMemory {
    /// One bucket per ratio in [`super::price_levels::FIBONACCI_RATIOS`]: observed reaction
    /// magnitudes (in ATR units) recorded at that level.
    observations: Vec<(f64, Vec<f64>)>,
}

impl FibonacciReactionMemory {
    pub fn new() -> Self {
        let observations = super::price_levels::FIBONACCI_RATIOS
            .iter()
            .map(|&r| (r, Vec::new()))
            .collect();
        Self { observations }
    }

    /// Records a reaction magnitude (in ATR units) observed at the ratio nearest to `ratio`.
    pub fn record(&mut self, ratio: f64, reaction_magnitude_atr: f64) {
        if let Some((_, bucket)) = self
            .observations
            .iter_mut()
            .min_by(|(a, _), (b, _)| (a - ratio).abs().total_cmp(&(b - ratio).abs()))
        {
            bucket.push(reaction_magnitude_atr);
        }
    }

    /// Median observed reaction magnitude at the ratio nearest to `ratio`. `None` if that bucket
    /// has no observations yet.
    pub fn median_reaction(&self, ratio: f64) -> Option<f64> {
        self.observations
            .iter()
            .min_by(|(a, _), (b, _)| (a - ratio).abs().total_cmp(&(b - ratio).abs()))
            .filter(|(_, bucket)| !bucket.is_empty())
            .map(|(_, bucket)| rolling_median(bucket))
    }

    pub fn observation_count(&self, ratio: f64) -> usize {
        self.observations
            .iter()
            .min_by(|(a, _), (b, _)| (a - ratio).abs().total_cmp(&(b - ratio).abs()))
            .map(|(_, bucket)| bucket.len())
            .unwrap_or(0)
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
    fn test_valid_bullish_impulse_passes_all_rules() {
        let nodes = vec![
            node(0, 100.0, false), // 0
            node(1, 120.0, true),  // 1
            node(2, 110.0, false), // 2 (retraces 50% of wave1, doesn't undercut 0)
            node(3, 140.0, true),  // 3 (longest leg)
            node(4, 130.0, false), // 4 (stays above wave1 high=120)
            node(5, 150.0, true),  // 5
        ];
        let result = validate_impulse(&nodes).unwrap();
        assert!(result.valid, "violations: {:?}", result.violations);
        assert!(result.pullback_quality > 0.0);
    }

    #[test]
    fn test_impulse_rejects_wave2_full_retrace() {
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 95.0, false), // retraces beyond wave 1 start (100)
            node(3, 140.0, true),
            node(4, 130.0, false),
            node(5, 150.0, true),
        ];
        let result = validate_impulse(&nodes).unwrap();
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule == "wave2_no_full_retrace"));
    }

    #[test]
    fn test_impulse_rejects_wave4_overlap() {
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 110.0, false),
            node(3, 140.0, true),
            node(4, 115.0, false), // overlaps wave 1 territory (below 120)
            node(5, 150.0, true),
        ];
        let result = validate_impulse(&nodes).unwrap();
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule == "wave4_no_overlap"));
    }

    #[test]
    fn test_impulse_rejects_wave3_shortest() {
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 130.0, true), // wave1 = 30
            node(2, 120.0, false),
            node(3, 135.0, true), // wave3 = 15 (shortest)
            node(4, 125.0, false),
            node(5, 160.0, true), // wave5 = 35
        ];
        let result = validate_impulse(&nodes).unwrap();
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule == "wave3_not_shortest"));
    }

    #[test]
    fn test_validate_impulse_requires_exactly_six_alternating_nodes() {
        let too_few = vec![node(0, 100.0, false), node(1, 120.0, true)];
        assert!(validate_impulse(&too_few).is_none());

        let non_alternating = vec![
            node(0, 100.0, false),
            node(1, 120.0, false),
            node(2, 110.0, false),
            node(3, 140.0, true),
            node(4, 130.0, false),
            node(5, 150.0, true),
        ];
        assert!(validate_impulse(&non_alternating).is_none());
    }

    #[test]
    fn test_correction_classifies_zigzag_vs_flat() {
        let zigzag = vec![
            node(0, 150.0, true),
            node(1, 130.0, false), // A: -20
            node(2, 141.0, true),  // B retraces 55% of A -> zigzag
            node(3, 120.0, false), // C
        ];
        let result = validate_correction(&zigzag).unwrap();
        assert_eq!(result.variant, CorrectionVariant::Zigzag);

        let flat = vec![
            node(0, 150.0, true),
            node(1, 130.0, false), // A: -20
            node(2, 149.0, true),  // B retraces 95% of A -> flat
            node(3, 128.0, false), // C
        ];
        let result = validate_correction(&flat).unwrap();
        assert_eq!(result.variant, CorrectionVariant::Flat);
    }

    #[test]
    fn test_correction_rejects_c_not_extending_past_b() {
        // A runs down (150 -> 130), B retraces up to 141; a valid C must continue down past B
        // (below 141). Here C instead prints above B, violating the rule.
        let nodes = vec![
            node(0, 150.0, true),
            node(1, 130.0, false),
            node(2, 141.0, true),
            node(3, 145.0, false),
        ];
        let result = validate_correction(&nodes).unwrap();
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule == "wave_c_must_extend_past_b"));
    }

    #[test]
    fn test_c_setup_levels_delegate_to_swing_fibonacci() {
        let levels = c_setup_levels(100.0, 150.0, true);
        assert_eq!(
            levels.len(),
            super::super::price_levels::FIBONACCI_RATIOS.len()
        );
    }

    #[test]
    fn test_reaction_memory_buckets_by_nearest_ratio() {
        let mut memory = FibonacciReactionMemory::new();
        memory.record(0.62, 1.5);
        memory.record(0.615, 1.7);
        memory.record(0.235, 0.5);

        assert_eq!(memory.observation_count(0.618), 2);
        let median = memory.median_reaction(0.618).unwrap();
        assert!((median - 1.6).abs() < 0.2);
        assert_eq!(memory.observation_count(0.236), 1);
    }

    fn sample_capabilities() -> SeriesCapabilities {
        SeriesCapabilities {
            volume: crate::model::VolumeKind::RealTurnover,
            trade_direction: false,
            session: crate::model::SessionKind::Regular,
            continuity: crate::model::ContinuityKind::SingleContract,
            price_adjustment: crate::model::PriceAdjustment::Raw,
            provenance: crate::model::Provenance::Exchange,
            liquidity_tier: crate::model::LiquidityTier::Deep,
        }
    }

    #[test]
    fn test_impulse_validation_defaults_to_no_capabilities_and_can_be_tagged() {
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 110.0, false),
            node(3, 140.0, true),
            node(4, 130.0, false),
            node(5, 150.0, true),
        ];
        let result = validate_impulse(&nodes).unwrap();
        assert_eq!(result.series_capabilities, None);

        let tagged = result.with_capabilities(sample_capabilities());
        assert_eq!(tagged.series_capabilities, Some(sample_capabilities()));
    }

    #[test]
    fn test_correction_validation_defaults_to_no_capabilities_and_can_be_tagged() {
        let nodes = vec![
            node(0, 150.0, true),
            node(1, 130.0, false),
            node(2, 141.0, true),
            node(3, 100.0, false),
        ];
        let result = validate_correction(&nodes).unwrap();
        assert_eq!(result.series_capabilities, None);

        let tagged = result.with_capabilities(sample_capabilities());
        assert_eq!(tagged.series_capabilities, Some(sample_capabilities()));
    }

    #[test]
    fn test_diagonal_allows_wave4_overlap_that_would_fail_a_plain_impulse() {
        // Same fixture as test_impulse_rejects_wave4_overlap: wave 4 (115) enters wave 1's
        // territory (below the wave-1 high of 120), which rejects a plain impulse but is exactly
        // the relaxation a diagonal grants.
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 110.0, false),
            node(3, 140.0, true),
            node(4, 115.0, false),
            node(5, 150.0, true),
        ];
        assert!(validate_impulse(&nodes)
            .unwrap()
            .violations
            .iter()
            .any(|v| v.rule == "wave4_no_overlap"));

        let result = validate_diagonal(&nodes, DiagonalKind::Ending).unwrap();
        assert!(result.valid, "violations: {:?}", result.violations);
        assert!(result.wave4_overlaps_wave1);
    }

    #[test]
    fn test_diagonal_still_rejects_wave2_full_retrace() {
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 95.0, false),
            node(3, 140.0, true),
            node(4, 130.0, false),
            node(5, 150.0, true),
        ];
        let result = validate_diagonal(&nodes, DiagonalKind::Leading).unwrap();
        assert!(!result.valid);
        assert!(result
            .violations
            .iter()
            .any(|v| v.rule == "wave2_no_full_retrace"));
    }

    #[test]
    fn test_diagonal_classifies_contracting_vs_expanding() {
        let contracting = vec![
            node(0, 100.0, false),
            node(1, 120.0, true),  // len1 = 20
            node(2, 110.0, false), // len2 = 10
            node(3, 125.0, true),  // len3 = 15
            node(4, 116.0, false), // len4 = 9, overlaps wave 1 (<=120)
            node(5, 124.0, true),  // len5 = 8
        ];
        let result = validate_diagonal(&contracting, DiagonalKind::Ending).unwrap();
        assert!(result.valid, "violations: {:?}", result.violations);
        assert_eq!(result.variant, DiagonalVariant::Contracting);

        let expanding = vec![
            node(0, 100.0, false),
            node(1, 110.0, true),  // len1 = 10
            node(2, 102.0, false), // len2 = 8
            node(3, 125.0, true),  // len3 = 23
            node(4, 90.0, false),  // len4 = 35, overlaps wave 1 (<=110)
            node(5, 140.0, true),  // len5 = 50
        ];
        let result = validate_diagonal(&expanding, DiagonalKind::Leading).unwrap();
        assert!(result.valid, "violations: {:?}", result.violations);
        assert_eq!(result.variant, DiagonalVariant::Expanding);
    }

    #[test]
    fn test_triangle_names_the_three_kinds_and_the_running_case() {
        // No wave moves beyond the end of the wave two before it: highs 130 >= 125 >= 122, lows
        // 110 <= 115.
        let contracting = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 115.0, false),
            node(5, 122.0, true),
        ];
        let result = validate_triangle(&contracting).unwrap();
        assert_eq!(result.variant, TriangleVariant::Contracting);
        assert!(result.valid, "violations: {:?}", result.violations);
        assert!(!result.running);
        assert_eq!(result.direction, WaveDirection::Up);

        // Every wave moves beyond: highs 110 < 113 < 120, lows 98 > 94; b, c and d retrace 1.2,
        // 1.25 and 1.27 of the wave before them.
        let expanding = vec![
            node(0, 100.0, false),
            node(1, 110.0, true),
            node(2, 98.0, false),
            node(3, 113.0, true),
            node(4, 94.0, false),
            node(5, 120.0, true),
        ];
        let result = validate_triangle(&expanding).unwrap();
        assert_eq!(result.variant, TriangleVariant::Expanding);
        assert!(result.valid, "violations: {:?}", result.violations);

        // b and d end on the same level.
        let barrier = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 110.0, false),
            node(5, 120.0, true),
        ];
        let result = validate_triangle(&barrier).unwrap();
        assert_eq!(result.variant, TriangleVariant::Barrier);
        assert!(result.valid, "violations: {:?}", result.violations);

        // Starting from a high, with b (125) beyond the start of a (120): a running triangle.
        // Node 0 decides that and nothing else.
        let running = vec![
            node(0, 120.0, true),
            node(1, 100.0, false),
            node(2, 125.0, true),
            node(3, 105.0, false),
            node(4, 118.0, true),
            node(5, 108.0, false),
        ];
        let result = validate_triangle(&running).unwrap();
        assert_eq!(result.variant, TriangleVariant::Contracting);
        assert!(result.running);
        assert!(result.valid, "violations: {:?}", result.violations);
        assert_eq!(result.direction, WaveDirection::Down);
    }

    #[test]
    fn test_triangle_reports_the_wave_that_breaks_its_kind() {
        // Wave d (108) moves beyond the end of wave b (110) while c and e stay within: measured
        // as contracting, with the one rule it breaks. Up to 0.39 this was `RunningOrBarrier`.
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 108.0, false),
            node(5, 120.0, true),
        ];
        let result = validate_triangle(&nodes).unwrap();
        assert_eq!(result.variant, TriangleVariant::Contracting);
        assert!(!result.valid);
        let rules: Vec<&str> = result.violations.iter().map(|v| v.rule.as_str()).collect();
        assert_eq!(rules, ["triangle_d_not_beyond_b"]);
    }

    #[test]
    fn test_barrier_tolerance_is_a_share_of_wave_b() {
        // Wave b is 20 long; d ends 2 above the end of b.
        let nodes = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 112.0, false),
            node(5, 120.0, true),
        ];
        let variant = |barrier_tolerance: f64| {
            check_triangle(&nodes, &TriangleOptions { barrier_tolerance })
                .unwrap()
                .variant
        };
        assert_eq!(
            variant(BARRIER_LEVEL_TOLERANCE),
            TriangleVariant::Contracting
        );
        assert_eq!(variant(0.1), TriangleVariant::Barrier);
        assert_eq!(variant(0.0), TriangleVariant::Contracting);
    }

    #[test]
    fn test_validate_combination_classifies_wxy_from_two_valid_corrections() {
        let nodes = vec![
            node(0, 150.0, true),
            node(1, 130.0, false), // W: leg A
            node(2, 141.0, true),  // W: leg B, 55% retrace -> zigzag
            node(3, 120.0, false), // W: leg C, also the X wave's start
            node(4, 140.0, true),  // X wave's end, also Y's start
            node(5, 125.0, false), // Y: leg A
            node(6, 134.0, true),  // Y: leg B, 60% retrace -> zigzag
            node(7, 112.0, false), // Y: leg C
        ];
        let result = validate_combination(&nodes).unwrap();
        assert_eq!(result.variant, CombinationVariant::DoubleThree);
        assert_eq!(result.segments.len(), 2);
        assert!(result.valid, "segments: {:?}", result.segments);
        assert!(result
            .segments
            .iter()
            .all(|s| s.variant == CorrectionVariant::Zigzag));
    }

    #[test]
    fn test_validate_combination_rejects_wrong_node_count() {
        let too_few = vec![node(0, 100.0, false), node(1, 120.0, true)];
        assert!(validate_combination(&too_few).is_none());
    }

    #[test]
    fn test_identify_extended_wave() {
        let wave3_extended = vec![
            node(0, 100.0, false),
            node(1, 110.0, true), // len1 = 10
            node(2, 105.0, false),
            node(3, 145.0, true), // len3 = 40
            node(4, 135.0, false),
            node(5, 150.0, true), // len5 = 5
        ];
        assert_eq!(identify_extended_wave(&wave3_extended), Some(3));

        let no_extension = vec![
            node(0, 100.0, false),
            node(1, 120.0, true), // len1 = 20
            node(2, 110.0, false),
            node(3, 142.0, true), // len3 = 22
            node(4, 130.0, false),
            node(5, 148.0, true), // len5 = 18
        ];
        assert_eq!(identify_extended_wave(&no_extension), None);
    }

    #[test]
    fn test_is_truncated_fifth() {
        let truncated = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 115.0, false),
            node(3, 150.0, true),
            node(4, 135.0, false),
            node(5, 145.0, true), // fails to exceed wave 3's high (150)
        ];
        assert_eq!(is_truncated_fifth(&truncated), Some(true));

        let not_truncated = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 115.0, false),
            node(3, 150.0, true),
            node(4, 135.0, false),
            node(5, 160.0, true),
        ];
        assert_eq!(is_truncated_fifth(&not_truncated), Some(false));
    }

    fn impulse_nodes() -> Vec<ZigZagNode> {
        vec![
            node(0, 100.0, false),
            node(1, 120.0, true),
            node(2, 110.0, false),
            node(3, 140.0, true),
            node(4, 130.0, false),
            node(5, 150.0, true),
        ]
    }

    /// The same sequence upside down: highs for lows, every price negated.
    fn mirrored(nodes: &[ZigZagNode]) -> Vec<ZigZagNode> {
        nodes
            .iter()
            .map(|n| ZigZagNode {
                price: -n.price,
                is_high: !n.is_high,
                ..*n
            })
            .collect()
    }

    #[test]
    fn test_impulse_reports_its_direction() {
        let rising = impulse_nodes();
        assert_eq!(
            validate_impulse(&rising).unwrap().direction,
            WaveDirection::Up
        );
        assert_eq!(
            validate_impulse(&mirrored(&rising)).unwrap().direction,
            WaveDirection::Down
        );
    }

    #[test]
    fn test_falling_impulse_is_the_exact_mirror_of_the_rising_one() {
        // Every single rule broken once, and all of them kept: the mirrored sequence must give
        // the same verdict, the same violations and the same quality to the last bit.
        let sequences = [
            [100.0, 120.0, 110.0, 140.0, 130.0, 150.0],
            [100.0, 120.0, 95.0, 140.0, 130.0, 150.0],
            [100.0, 130.0, 120.0, 135.0, 131.0, 160.0],
            [100.0, 120.0, 110.0, 140.0, 115.0, 150.0],
            [100.0, 137.3, 118.9, 171.4, 149.2, 160.1],
        ];
        for prices in sequences {
            let rising: Vec<ZigZagNode> = prices
                .iter()
                .enumerate()
                .map(|(i, &p)| node(i as i64, p, i % 2 == 1))
                .collect();
            let up = check_impulse(&rising).unwrap();
            let down = check_impulse(&mirrored(&rising)).unwrap();
            assert_eq!(up.direction, WaveDirection::Up);
            assert_eq!(down.direction, WaveDirection::Down);
            assert_eq!(up.valid, down.valid, "{prices:?}");
            assert_eq!(up.violations, down.violations, "{prices:?}");
            assert_eq!(
                up.pullback_quality.to_bits(),
                down.pullback_quality.to_bits(),
                "{prices:?}"
            );
            assert_eq!(
                identify_extended_wave(&rising),
                identify_extended_wave(&mirrored(&rising))
            );
            assert_eq!(
                is_truncated_fifth(&rising),
                is_truncated_fifth(&mirrored(&rising))
            );
        }
    }

    #[test]
    fn test_sequence_errors_name_the_node() {
        let too_few = &impulse_nodes()[..5];
        assert_eq!(
            check_impulse(too_few),
            Err(SequenceError::NodeCount {
                found: 5,
                expected: &[6]
            })
        );

        let mut same_kind = impulse_nodes();
        same_kind[3].is_high = false;
        assert_eq!(
            check_impulse(&same_kind),
            Err(SequenceError::NotAlternating { index: 3 })
        );

        let mut out_of_order = impulse_nodes();
        out_of_order[4].timestamp = 2;
        assert_eq!(
            check_impulse(&out_of_order),
            Err(SequenceError::TimeOrder { index: 4 })
        );
        assert!(validate_impulse(&out_of_order).is_none());

        // A "low" above the high before it: wave 2 would not run against wave 1.
        let mut misplaced = impulse_nodes();
        misplaced[2].price = 125.0;
        let error = check_impulse(&misplaced).unwrap_err();
        assert_eq!(error, SequenceError::PivotPlacement { index: 2 });
        assert_eq!(error.index(), Some(2));
        assert_eq!(
            error.to_string(),
            "node 2 does not lie beyond node 1: a high must be above the node before it, a low \
             below"
        );
        assert!(validate_impulse(&misplaced).is_none());
        assert!(identify_extended_wave(&misplaced).is_none());
        assert!(is_truncated_fifth(&misplaced).is_none());

        // A leg without length has no direction.
        let mut flat_leg = impulse_nodes();
        flat_leg[1].price = 100.0;
        assert_eq!(
            check_impulse(&flat_leg),
            Err(SequenceError::PivotPlacement { index: 1 })
        );

        let mut not_a_number = impulse_nodes();
        not_a_number[3].price = f64::NAN;
        assert_eq!(
            check_impulse(&not_a_number),
            Err(SequenceError::PivotPlacement { index: 3 })
        );

        assert_eq!(
            check_combination(&impulse_nodes(), &CorrectionOptions::default()),
            Err(SequenceError::NodeCount {
                found: 6,
                expected: &[8, 12]
            })
        );
        assert_eq!(
            SequenceError::NodeCount {
                found: 6,
                expected: &[8, 12]
            }
            .to_string(),
            "expected 8 or 12 nodes, found 6"
        );
    }

    #[test]
    fn test_equal_timestamps_are_a_pivot_sequence() {
        // One bar can carry both a pivot high and a pivot low.
        let mut nodes = impulse_nodes();
        nodes[2].timestamp = nodes[1].timestamp;
        assert!(check_impulse(&nodes).unwrap().valid);
    }

    #[test]
    fn test_former_gap_between_zigzag_and_flat_is_a_valid_zigzag_with_a_guideline() {
        // B retraces 0.85 of A: above the zigzag's typical bound, below the flat's minimum.
        let nodes = vec![
            node(0, 100.0, true),
            node(1, 90.0, false),
            node(2, 98.5, true),
            node(3, 85.0, false),
        ];
        let result = validate_correction(&nodes).unwrap();
        assert_eq!(result.variant, CorrectionVariant::Zigzag);
        assert_eq!(result.direction, WaveDirection::Down);
        assert!(result.valid);
        assert!(result.violations.is_empty());
        assert_eq!(
            result.guidelines,
            vec![GuidelineDeviation {
                guideline: "zigzag_b_retrace_bound".to_string(),
                detail: "Wave B retraced more than a Zigzag's typical bound (78.6%) without \
                         qualifying as a Flat"
                    .to_string(),
            }]
        );
    }

    #[test]
    fn test_flat_variants_follow_where_b_and_c_end() {
        let correction = |b: f64, c: f64| {
            validate_correction(&[
                node(0, 140.0, true),
                node(1, 120.0, false),
                node(2, b, true),
                node(3, c, false),
            ])
            .unwrap()
        };
        // B on the start of A is not beyond it: a flat, not an expanded flat as up to 0.39.
        assert_eq!(correction(140.0, 115.0).variant, CorrectionVariant::Flat);
        // B at 1.10 of A, C beyond the end of A.
        assert_eq!(
            correction(142.0, 115.0).variant,
            CorrectionVariant::ExpandedFlat
        );
        // The same B, C short of the end of A: a running flat, valid.
        let running = correction(142.0, 125.0);
        assert_eq!(running.variant, CorrectionVariant::RunningFlat);
        assert!(running.valid);
        // B beyond the start of A but not past 1.05, C beyond the end of A: still a flat.
        assert_eq!(correction(140.5, 115.0).variant, CorrectionVariant::Flat);
    }

    #[test]
    fn test_asserted_subdivision_of_wave_a_lets_the_family_rule_fail() {
        let nodes = |b: f64| {
            vec![
                node(0, 140.0, true),
                node(1, 120.0, false),
                node(2, b, true),
                node(3, 110.0, false),
            ]
        };
        let with = |wave_a| CorrectionOptions {
            wave_a: Some(wave_a),
            ..CorrectionOptions::default()
        };
        let rules = |v: &CorrectionValidation| -> Vec<String> {
            v.violations.iter().map(|x| x.rule.clone()).collect()
        };

        // B beyond the start of A under a five-wave A: no zigzag.
        let zigzag = check_correction(&nodes(142.0), &with(WaveSubdivision::Five)).unwrap();
        assert_eq!(zigzag.variant, CorrectionVariant::Zigzag);
        assert_eq!(rules(&zigzag), ["zigzag_b_not_beyond_start_of_a"]);

        // B retracing 0.60 under a three-wave A: no flat.
        let flat = check_correction(&nodes(132.0), &with(WaveSubdivision::Three)).unwrap();
        assert_eq!(flat.variant, CorrectionVariant::Flat);
        assert_eq!(rules(&flat), ["flat_b_min_retrace"]);

        // Without the assertion both sequences are valid, each in the family B puts it in.
        assert!(validate_correction(&nodes(142.0)).unwrap().valid);
        assert!(validate_correction(&nodes(132.0)).unwrap().valid);
    }

    #[test]
    fn test_first_four_pivots_of_an_impulse_pass_as_a_zigzag() {
        // The documented limit: an outline of four points cannot show that it is a correction.
        let impulse = impulse_nodes();
        let result = validate_correction(&impulse[..4]).unwrap();
        assert_eq!(result.variant, CorrectionVariant::Zigzag);
        assert!(result.valid);
        assert!(result.guidelines.is_empty());
    }

    #[test]
    fn test_correction_refuses_a_b_that_does_not_run_against_a() {
        let nodes = vec![
            node(0, 150.0, true),
            node(1, 130.0, false),
            node(2, 120.0, true),
            node(3, 100.0, false),
        ];
        assert_eq!(
            check_correction(&nodes, &CorrectionOptions::default()),
            Err(SequenceError::PivotPlacement { index: 2 })
        );
        assert!(validate_correction(&nodes).is_none());
    }
}
