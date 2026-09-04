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
}

#[derive(Debug, Clone, PartialEq)]
pub struct CorrectionValidation {
    pub variant: CorrectionVariant,
    pub valid: bool,
    pub violations: Vec<RuleViolation>,
    pub pullback_quality: f64,
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
    if nodes.len() != 6 {
        return None;
    }
    if nodes.windows(2).any(|p| p[0].is_high == p[1].is_high) {
        return None;
    }

    let bullish = nodes[1].price > nodes[0].price;
    let (w0, w1, w2, w3, w4, w5) = (
        nodes[0].price,
        nodes[1].price,
        nodes[2].price,
        nodes[3].price,
        nodes[4].price,
        nodes[5].price,
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
    })
}

/// Validates a 4-node correction labeled `[0, A, B, C]`, classifying it as a Zigzag (B retraces
/// less than 100% of A), Flat (B retraces close to 100% of A, C similar length to A), or Expanded
/// Flat (B exceeds the start of the move that preceded A). Returns `None` if `nodes` does not
/// have exactly 4 alternating entries.
pub fn validate_correction(nodes: &[ZigZagNode]) -> Option<CorrectionValidation> {
    if nodes.len() != 4 {
        return None;
    }
    if nodes.windows(2).any(|p| p[0].is_high == p[1].is_high) {
        return None;
    }

    let bearish_correction = nodes[1].price > nodes[0].price; // 0->A moves down within an uptrend correction, etc.; use magnitude only
    let _ = bearish_correction;

    let (n0, a, b, c) = (
        nodes[0].price,
        nodes[1].price,
        nodes[2].price,
        nodes[3].price,
    );
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
}
