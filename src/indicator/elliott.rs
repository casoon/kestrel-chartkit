//! Elliott Wave and Fibonacci pattern validation: rule-checked impulses and corrections (Zigzag/
//! Flat variants), C-setup projection, pullback quality scoring, and a reaction-memory tracker —
//! built on [`super::zigzag_advanced::ZigZagNode`] sequences, reusing
//! [`super::price_levels::swing_fibonacci_levels`] for level projection rather than duplicating
//! the ratio table.
//!
//! Wave counting is inherently interpretive; this validates a *given* labeling against Elliott's
//! documented structural rules (not heuristics about which count is "right") and scores how
//! Fibonacci-clean the retracements are — a rule checker and quality scorer, not a wave counter
//! that discovers labelings on its own.

use crate::model::SeriesCapabilities;
use crate::stats::rolling_median;

use super::price_levels::{swing_fibonacci_levels, PriceLevel};
use super::zigzag_advanced::ZigZagNode;

/// A rule violation found while validating an impulse or correction.
#[derive(Debug, Clone, PartialEq)]
pub struct RuleViolation {
    pub rule: String,
    pub detail: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorrectionVariant {
    Zigzag,
    Flat,
    ExpandedFlat,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ImpulseValidation {
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
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
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

/// Validates a 6-node bullish-or-bearish impulse labeled `[0, 1, 2, 3, 4, 5]` against Elliott's
/// three cardinal rules: wave 2 never retraces beyond the start of wave 1, wave 3 is never the
/// shortest of waves 1/3/5, and wave 4 never enters wave 1's price territory. Returns `None` if
/// `nodes` does not have exactly 6 alternating entries.
pub fn validate_impulse(nodes: &[ZigZagNode]) -> Option<ImpulseValidation> {
    let [node0, node1, node2, node3, node4, node5] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let bullish = node1.price > node0.price;
    let (w0, w1, w2, w3, w4, w5) = (
        node0.price,
        node1.price,
        node2.price,
        node3.price,
        node4.price,
        node5.price,
    );

    let mut violations = Vec::new();

    let wave2_ok = if bullish { w2 > w0 } else { w2 < w0 };
    if !wave2_ok {
        violations.push(RuleViolation {
            rule: "wave2_no_full_retrace".to_string(),
            detail: "Wave 2 retraced beyond the start of wave 1".to_string(),
        });
    }

    let len1 = (w1 - w0).abs();
    let len3 = (w3 - w2).abs();
    let len5 = (w5 - w4).abs();
    if len3 < len1 && len3 < len5 {
        violations.push(RuleViolation {
            rule: "wave3_not_shortest".to_string(),
            detail: "Wave 3 is the shortest of waves 1, 3, and 5".to_string(),
        });
    }

    let wave4_ok = if bullish { w4 > w1 } else { w4 < w1 };
    if !wave4_ok {
        violations.push(RuleViolation {
            rule: "wave4_no_overlap".to_string(),
            detail: "Wave 4 entered wave 1's price territory".to_string(),
        });
    }

    let retrace2 = if len1 > 0.0 {
        (w0 - w2).abs() / len1
    } else {
        f64::INFINITY
    };
    let len34 = (w3 - w2).abs();
    let retrace4 = if len34 > 0.0 {
        (w3 - w4).abs() / len34
    } else {
        f64::INFINITY
    };
    let pullback_quality = if retrace2.is_finite() && retrace4.is_finite() {
        let d2 = nearest_fib_distance(retrace2);
        let d4 = nearest_fib_distance(retrace4);
        (1.0 - (d2 + d4)).clamp(0.0, 1.0)
    } else {
        0.0
    };

    Some(ImpulseValidation {
        valid: violations.is_empty(),
        violations,
        pullback_quality,
        series_capabilities: None,
    })
}

/// Validates a 4-node correction labeled `[0, A, B, C]`, classifying it as a Zigzag (B retraces
/// less than 100% of A), Flat (B retraces close to 100% of A, C similar length to A), or Expanded
/// Flat (B exceeds the start of the move that preceded A). Returns `None` if `nodes` does not
/// have exactly 4 alternating entries.
pub fn validate_correction(nodes: &[ZigZagNode]) -> Option<CorrectionValidation> {
    let [node0, node1, node2, node3] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let bearish_correction = node1.price > node0.price; // 0->A moves down within an uptrend correction, etc.; use magnitude only
    let _ = bearish_correction;

    let (n0, a, b, c) = (node0.price, node1.price, node2.price, node3.price);
    let leg_a = (a - n0).abs();
    let leg_b_retrace = if leg_a > 0.0 {
        (b - a).abs() / leg_a
    } else {
        f64::INFINITY
    };
    let leg_c = (c - b).abs();
    let c_vs_a = if leg_a > 0.0 {
        leg_c / leg_a
    } else {
        f64::INFINITY
    };

    let variant = if leg_b_retrace >= 1.0 {
        CorrectionVariant::ExpandedFlat
    } else if leg_b_retrace >= 0.90 {
        CorrectionVariant::Flat
    } else {
        CorrectionVariant::Zigzag
    };

    let mut violations = Vec::new();
    // C must continue past B in the same direction as A (a genuine 3-wave correction, not a
    // reversal back through the start).
    let a_dir_down = a < n0;
    let c_continues = if a_dir_down { c < b } else { c > b };
    if !c_continues {
        violations.push(RuleViolation {
            rule: "wave_c_must_extend_past_b".to_string(),
            detail: "Wave C did not continue past wave B in wave A's direction".to_string(),
        });
    }

    if variant == CorrectionVariant::Zigzag && leg_b_retrace > 0.786 {
        violations.push(RuleViolation {
            rule: "zigzag_b_retrace_bound".to_string(),
            detail: "Wave B retraced more than a Zigzag's typical bound (78.6%) without qualifying as a Flat".to_string(),
        });
    }

    let quality_ref = match variant {
        CorrectionVariant::Zigzag => nearest_fib_distance(leg_b_retrace.min(1.0)),
        CorrectionVariant::Flat | CorrectionVariant::ExpandedFlat => {
            (1.0 - c_vs_a.min(2.0) / 1.0).abs().min(1.0)
        }
    };
    let pullback_quality = (1.0 - quality_ref).clamp(0.0, 1.0);

    Some(CorrectionValidation {
        variant,
        valid: violations.is_empty(),
        violations,
        pullback_quality,
        series_capabilities: None,
    })
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

/// Validates a 6-node diagonal labeled `[0, 1, 2, 3, 4, 5]` — same shape as
/// [`validate_impulse`], with rule 3 relaxed (wave 4 may enter wave 1's territory) and rule 2
/// retained (wave 3 is never the shortest). Returns `None` if `nodes` does not have exactly 6
/// alternating entries.
pub fn validate_diagonal(nodes: &[ZigZagNode], kind: DiagonalKind) -> Option<DiagonalValidation> {
    let [node0, node1, node2, node3, node4, node5] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let bullish = node1.price > node0.price;
    let (w0, w1, w2, w3, w4, w5) = (
        node0.price,
        node1.price,
        node2.price,
        node3.price,
        node4.price,
        node5.price,
    );

    let mut violations = Vec::new();

    let wave2_ok = if bullish { w2 > w0 } else { w2 < w0 };
    if !wave2_ok {
        violations.push(RuleViolation {
            rule: "wave2_no_full_retrace".to_string(),
            detail: "Wave 2 retraced beyond the start of wave 1".to_string(),
        });
    }

    let len1 = (w1 - w0).abs();
    let len2 = (w2 - w1).abs();
    let len3 = (w3 - w2).abs();
    let len4 = (w4 - w3).abs();
    let len5 = (w5 - w4).abs();
    if len3 < len1 && len3 < len5 {
        violations.push(RuleViolation {
            rule: "wave3_not_shortest".to_string(),
            detail: "Wave 3 is the shortest of waves 1, 3, and 5".to_string(),
        });
    }

    let wave4_overlaps_wave1 = if bullish { w4 <= w1 } else { w4 >= w1 };

    let variant = if len5 < len3 && len3 < len1 && len4 < len2 {
        DiagonalVariant::Contracting
    } else {
        DiagonalVariant::Expanding
    };

    Some(DiagonalValidation {
        kind,
        variant,
        valid: violations.is_empty(),
        violations,
        wave4_overlaps_wave1,
        series_capabilities: None,
    })
}

/// How the two boundary lines of a 5-leg (`a-b-c-d-e`) triangle relate. Contracting and expanding
/// mirror [`DiagonalVariant`]; `RunningOrBarrier` covers the case where leg `d` runs past the end
/// of leg `b` instead of staying inside it — the two variants share a geometry check and are kept
/// as one classification rather than split, since distinguishing them further needs the trend
/// context the six pivots alone don't carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriangleVariant {
    Contracting,
    Expanding,
    RunningOrBarrier,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TriangleValidation {
    pub variant: TriangleVariant,
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

/// Classifies a 6-node, 5-leg sequence labeled `[0, a, b, c, d, e]` as a contracting, expanding, or
/// running/barrier triangle. Triangles carry no hard rules of their own in Elliott's own writing —
/// only the classification varies — so unlike [`validate_diagonal`] this returns no violations.
/// Returns `None` if `nodes` does not have exactly 6 alternating entries.
pub fn validate_triangle(nodes: &[ZigZagNode]) -> Option<TriangleValidation> {
    let [node0, node1, node2, node3, node4, node5] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    // Classified from the two boundary lines, not from leg lengths. The boundaries are what the
    // names describe: a contracting triangle's converge, an expanding triangle's diverge. Leg
    // lengths follow from that but do not determine it — a sequence can shrink leg over leg and
    // still have both boundaries running the same way, which is the barrier/running case.
    //
    // `node0` is the point the triangle starts from and belongs to neither boundary; the five legs
    // it opens are `a`-`e`. For a sequence starting at a low, `node1`/`node3`/`node5` are the
    // highs and `node2`/`node4` the lows; starting at a high it is the other way round. Both
    // orientations reduce to the same two questions, so they are asked once, on the extremes.
    let von_tief = node1.price > node0.price;
    let (erstes_extrem, zweites_extrem, drittes_extrem) = (node1.price, node3.price, node5.price);
    let (erste_gegenseite, zweite_gegenseite) = (node2.price, node4.price);

    let obere_faellt = if von_tief {
        zweites_extrem < erstes_extrem && drittes_extrem < zweites_extrem
    } else {
        zweite_gegenseite < erste_gegenseite
    };
    let untere_steigt = if von_tief {
        zweite_gegenseite > erste_gegenseite
    } else {
        zweites_extrem > erstes_extrem && drittes_extrem > zweites_extrem
    };

    let obere_steigt = if von_tief {
        zweites_extrem > erstes_extrem && drittes_extrem > zweites_extrem
    } else {
        zweite_gegenseite > erste_gegenseite
    };
    let untere_faellt = if von_tief {
        zweite_gegenseite < erste_gegenseite
    } else {
        zweites_extrem < erstes_extrem && drittes_extrem < zweites_extrem
    };

    let variant = if obere_faellt && untere_steigt {
        TriangleVariant::Contracting
    } else if obere_steigt && untere_faellt {
        TriangleVariant::Expanding
    } else {
        TriangleVariant::RunningOrBarrier
    };

    Some(TriangleValidation {
        variant,
        series_capabilities: None,
    })
}

/// A W-X-Y or W-X-Y-X-Z combination correction: two or three simple corrections
/// ([`validate_correction`]) joined by connecting "X" waves.
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
/// correction of its own — consistent with how [`validate_impulse`] and [`validate_diagonal`]
/// treat each numbered wave as atomic rather than recursing into its sub-degree. Returns `None` if
/// `nodes` is not exactly 8 or 12 nodes, or not alternating.
pub fn validate_combination(nodes: &[ZigZagNode]) -> Option<CombinationValidation> {
    if nodes.len() != 8 && nodes.len() != 12 {
        return None;
    }
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let variant = if nodes.len() == 8 {
        CombinationVariant::DoubleThree
    } else {
        CombinationVariant::TripleThree
    };

    // Each correction segment is 4 nodes: `[start, a, b, c]`. The connecting X leg is the single
    // move from one segment's `c` to the next segment's `start` — those are adjacent nodes in
    // `nodes` (e.g. `Wc` at index 3, `X` at index 4), not a shared one, and the next segment's
    // `start` (`X`) is where the following correction begins. So segments simply tile `nodes` in
    // non-overlapping groups of 4: `[0..4]`, `[4..8]`, ...
    let segment_count = nodes.len() / 4;
    let mut segments = Vec::with_capacity(segment_count);
    for i in 0..segment_count {
        let start = i * 4;
        let segment = &nodes[start..start + 4];
        let validation = validate_correction(segment)?;
        segments.push(validation);
    }

    let valid = segments.iter().all(|s| s.valid);

    Some(CombinationValidation {
        variant,
        segments,
        valid,
        series_capabilities: None,
    })
}

/// Which of waves 1, 3, or 5 is "extended" — long enough relative to the other two that it reads
/// as the dominant wave of the impulse. Elliott's guideline: extended when its length is at least
/// 1.618 times the longer of the other two. Returns `None` if no wave qualifies, or if `nodes` is
/// not exactly 6 alternating entries.
pub fn identify_extended_wave(nodes: &[ZigZagNode]) -> Option<u8> {
    let [node0, node1, node2, node3, node4, node5] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let len1 = (node1.price - node0.price).abs();
    let len3 = (node3.price - node2.price).abs();
    let len5 = (node5.price - node4.price).abs();

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
/// cardinal rules ([`validate_impulse`]) may still all hold — this is a guideline violation, not a
/// rule violation. Returns `None` if `nodes` is not exactly 6 alternating entries.
pub fn is_truncated_fifth(nodes: &[ZigZagNode]) -> Option<bool> {
    let [node0, node1, _node2, node3, _node4, node5] = nodes else {
        return None;
    };
    if nodes.windows(2).any(|p| match p {
        [a, b] => a.is_high == b.is_high,
        _ => false,
    }) {
        return None;
    }

    let bullish = node1.price > node0.price;
    Some(if bullish {
        node5.price <= node3.price
    } else {
        node5.price >= node3.price
    })
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
    fn test_triangle_classifies_contracting_expanding_and_running() {
        // Converging boundaries: the highs fall (130 > 125 > 122) while the lows rise (110 < 115).
        let contracting = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 115.0, false),
            node(5, 122.0, true),
        ];
        assert_eq!(
            validate_triangle(&contracting).unwrap().variant,
            TriangleVariant::Contracting
        );

        // Diverging boundaries: the highs rise (110 < 120 < 140) while the lows fall (95 > 85).
        let expanding = vec![
            node(0, 100.0, false),
            node(1, 110.0, true),
            node(2, 95.0, false),
            node(3, 120.0, true),
            node(4, 85.0, false),
            node(5, 140.0, true),
        ];
        assert_eq!(
            validate_triangle(&expanding).unwrap().variant,
            TriangleVariant::Expanding
        );

        // Both boundaries fall: the highs (130 > 125 > 120) and the lows (110 > 108) run the same
        // way. Neither converging nor diverging — the running/barrier case.
        let running = vec![
            node(0, 100.0, false),
            node(1, 130.0, true),
            node(2, 110.0, false),
            node(3, 125.0, true),
            node(4, 108.0, false),
            node(5, 120.0, true),
        ];
        assert_eq!(
            validate_triangle(&running).unwrap().variant,
            TriangleVariant::RunningOrBarrier
        );

        // The same three cases mirrored: a triangle that starts from a high. The classification
        // must not depend on which side the sequence opens with.
        let contracting_von_hoch = vec![
            node(0, 140.0, true),
            node(1, 110.0, false),
            node(2, 130.0, true),
            node(3, 115.0, false),
            node(4, 125.0, true),
            node(5, 118.0, false),
        ];
        assert_eq!(
            validate_triangle(&contracting_von_hoch).unwrap().variant,
            TriangleVariant::Contracting
        );
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
}
