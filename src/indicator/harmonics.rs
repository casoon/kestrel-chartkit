//! Harmonic XABCD patterns: ratio checks over a swing sequence, a zone instead of a line, and a
//! three-state lifecycle.
//!
//! Built on [`super::zigzag_advanced::ZigZagNode`] sequences like [`super::chart_patterns`], and
//! for the same reason: the swing definition is the input, not something to re-derive per module.
//!
//! # The patterns do not differ in shape
//!
//! Gartley, Bat, Butterfly, Crab, Cypher and Shark are all five-point zigzags. What separates them
//! is three numbers. This module is therefore a table plus a tolerance, and not six detectors.
//!
//! # Why the PRZ is a zone
//!
//! "D sits at 88.6 %, therefore long" is the misreading. What is looked for is the place where
//! several *independent* projections coincide: the XA retracement, the BC extension, and the
//! AB=CD projection. When they agree the zone is narrow and the statement sharp; when they
//! disagree the zone is wide — and that width is information, not a defect of the drawing. A
//! single Fibonacci line hides exactly that and looks more precise than it is.
//!
//! # Why the tolerance is the central decision
//!
//! A `b` ratio of 0.60 is not 0.618. Whether it counts as a Gartley depends on an interval
//! somebody has to choose, and a statement about harmonic patterns without a stated tolerance says
//! nothing. [`HarmonicConfig::ratio_tolerance`] is that number, and it is deliberately a
//! parameter with a documented default rather than a constant buried in a comparison.

use super::zigzag_advanced::ZigZagNode;
use crate::model::Bar;

/// An inclusive ratio interval.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RatioRange {
    pub min: f64,
    pub max: f64,
}

impl RatioRange {
    pub const fn new(min: f64, max: f64) -> Self {
        Self { min, max }
    }

    /// Whether `value` lies in the interval, widened by `tolerance` on both sides.
    pub fn contains(&self, value: f64, tolerance: f64) -> bool {
        value >= self.min - tolerance && value <= self.max + tolerance
    }

    fn center(&self) -> f64 {
        (self.min + self.max) / 2.0
    }
}

/// One row of the ratio table — the entire difference between two harmonic patterns.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HarmonicDefinition {
    pub name: &'static str,
    /// How far B retraces XA.
    pub b: RatioRange,
    /// How far C retraces AB.
    pub c: RatioRange,
    /// Where D sits relative to XA. Above 1.0 the pattern extends past X.
    pub d: RatioRange,
    /// How far CD extends BC.
    pub cd: RatioRange,
}

/// The table the documentation prints, as data.
///
/// Kept here rather than in a consumer so that a figure and a chapter cannot drift apart: both
/// read the same rows.
pub const HARMONIC_TABLE: &[HarmonicDefinition] = &[
    HarmonicDefinition {
        name: "gartley",
        b: RatioRange::new(0.586, 0.65),
        c: RatioRange::new(0.382, 0.886),
        d: RatioRange::new(0.75, 0.82),
        cd: RatioRange::new(1.13, 1.618),
    },
    HarmonicDefinition {
        name: "bat",
        b: RatioRange::new(0.382, 0.5),
        c: RatioRange::new(0.382, 0.886),
        d: RatioRange::new(0.85, 0.92),
        cd: RatioRange::new(1.618, 2.618),
    },
    HarmonicDefinition {
        name: "butterfly",
        b: RatioRange::new(0.75, 0.82),
        c: RatioRange::new(0.382, 0.886),
        d: RatioRange::new(1.13, 1.618),
        cd: RatioRange::new(1.618, 2.618),
    },
    HarmonicDefinition {
        name: "crab",
        b: RatioRange::new(0.382, 0.618),
        c: RatioRange::new(0.382, 0.886),
        d: RatioRange::new(1.5, 1.7),
        cd: RatioRange::new(2.24, 3.618),
    },
    HarmonicDefinition {
        name: "deep_crab",
        b: RatioRange::new(0.85, 0.92),
        c: RatioRange::new(0.382, 0.886),
        d: RatioRange::new(1.5, 1.7),
        cd: RatioRange::new(2.24, 3.618),
    },
    HarmonicDefinition {
        name: "shark",
        b: RatioRange::new(0.382, 0.618),
        c: RatioRange::new(1.13, 1.618),
        d: RatioRange::new(0.85, 1.13),
        cd: RatioRange::new(1.618, 2.24),
    },
];

/// Where a candidate stands. The distinction the whole module exists for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HarmonicState {
    /// X, A, B and C are in place; D is a projection and nothing has happened yet.
    ///
    /// The useful state: it says where to look *before* the fact. It is also the one most easily
    /// mistaken for the others, because a drawn zone looks like a finding.
    Candidate,
    /// Price has reached the zone and the ratios hold.
    Complete,
    /// A reversal out of the zone has occurred.
    Confirmed,
    /// Price ran through the zone without turning.
    Invalidated,
}

/// The zone where the independent projections coincide.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Prz {
    pub low: f64,
    pub high: f64,
}

impl Prz {
    pub fn contains(&self, price: f64) -> bool {
        price >= self.low && price <= self.high
    }

    pub fn width(&self) -> f64 {
        self.high - self.low
    }
}

/// A recognised or projected XABCD structure.
#[derive(Debug, Clone, PartialEq)]
pub struct HarmonicCandidate {
    pub name: &'static str,
    /// True when the structure resolves upward at D — X high, A low, …
    pub bullish: bool,
    pub x: f64,
    pub a: f64,
    pub b: f64,
    pub c: f64,
    /// `None` while the structure is a candidate.
    pub d: Option<f64>,
    pub prz: Prz,
    pub state: HarmonicState,
    /// Measured ratios, in the order b, c, d, cd. `d` and `cd` are `None` for a candidate.
    pub ratios: (f64, f64, Option<f64>, Option<f64>),
}

impl HarmonicCandidate {
    /// Advances the lifecycle with a subsequent bar.
    ///
    /// A candidate becomes complete when price trades into the zone, and confirmed when it closes
    /// back out of it in the pattern's direction. Running through the zone invalidates — and that
    /// is the same condition read from the other side, which is why it needs no separate rule.
    pub fn update_state(&mut self, bar: &Bar) -> HarmonicState {
        match self.state {
            HarmonicState::Confirmed | HarmonicState::Invalidated => self.state,
            HarmonicState::Candidate => {
                let reached = if self.bullish {
                    bar.low <= self.prz.high
                } else {
                    bar.high >= self.prz.low
                };
                if reached {
                    self.d = Some(if self.bullish { bar.low } else { bar.high });
                    self.state = HarmonicState::Complete;
                }
                if self.ran_through(bar) {
                    self.state = HarmonicState::Invalidated;
                }
                self.state
            }
            HarmonicState::Complete => {
                if self.ran_through(bar) {
                    self.state = HarmonicState::Invalidated;
                } else {
                    let turned = if self.bullish {
                        bar.close > self.prz.high
                    } else {
                        bar.close < self.prz.low
                    };
                    if turned {
                        self.state = HarmonicState::Confirmed;
                    }
                }
                self.state
            }
        }
    }

    fn ran_through(&self, bar: &Bar) -> bool {
        if self.bullish {
            bar.close < self.prz.low
        } else {
            bar.close > self.prz.high
        }
    }
}

/// Configuration of the detector.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HarmonicConfig {
    /// Absolute widening of every ratio interval. The number that decides how many patterns exist.
    pub ratio_tolerance: f64,
    /// Minimum PRZ half-width as a share of XA, so a zone is never a line by accident.
    pub min_prz_share: f64,
}

impl Default for HarmonicConfig {
    fn default() -> Self {
        Self {
            ratio_tolerance: 0.03,
            min_prz_share: 0.005,
        }
    }
}

/// Scans swing sequences for XABCD structures.
pub struct HarmonicDetector {
    pub config: HarmonicConfig,
}

impl Default for HarmonicDetector {
    fn default() -> Self {
        Self::new(HarmonicConfig::default())
    }
}

impl HarmonicDetector {
    pub fn new(config: HarmonicConfig) -> Self {
        Self { config }
    }

    /// Every complete five-point structure whose ratios match a row of the table.
    pub fn scan(&self, nodes: &[ZigZagNode]) -> Vec<HarmonicCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(5) {
            if !window.windows(2).all(|p| p[0].is_high != p[1].is_high) {
                continue;
            }
            let (x, a, b, c, d) = (
                window[0].price,
                window[1].price,
                window[2].price,
                window[3].price,
                window[4].price,
            );
            let Some((b_r, c_r, d_r, cd_r)) = measure(x, a, b, c, Some(d)) else {
                continue;
            };
            let (Some(d_r), Some(cd_r)) = (d_r, cd_r) else {
                continue;
            };

            for def in HARMONIC_TABLE {
                let t = self.config.ratio_tolerance;
                if def.b.contains(b_r, t)
                    && def.c.contains(c_r, t)
                    && def.d.contains(d_r, t)
                    && def.cd.contains(cd_r, t)
                {
                    let prz = self.prz(def, x, a, b, c);
                    out.push(HarmonicCandidate {
                        name: def.name,
                        bullish: !window[0].is_high,
                        x,
                        a,
                        b,
                        c,
                        d: Some(d),
                        prz,
                        state: HarmonicState::Complete,
                        ratios: (b_r, c_r, Some(d_r), Some(cd_r)),
                    });
                }
            }
        }
        out
    }

    /// Structures still missing their D — the state that says where to look.
    pub fn scan_candidates(&self, nodes: &[ZigZagNode]) -> Vec<HarmonicCandidate> {
        let mut out = Vec::new();
        for window in nodes.windows(4) {
            if !window.windows(2).all(|p| p[0].is_high != p[1].is_high) {
                continue;
            }
            let (x, a, b, c) = (
                window[0].price,
                window[1].price,
                window[2].price,
                window[3].price,
            );
            let Some((b_r, c_r, _, _)) = measure(x, a, b, c, None) else {
                continue;
            };

            for def in HARMONIC_TABLE {
                let t = self.config.ratio_tolerance;
                if def.b.contains(b_r, t) && def.c.contains(c_r, t) {
                    out.push(HarmonicCandidate {
                        name: def.name,
                        bullish: !window[0].is_high,
                        x,
                        a,
                        b,
                        c,
                        d: None,
                        prz: self.prz(def, x, a, b, c),
                        state: HarmonicState::Candidate,
                        ratios: (b_r, c_r, None, None),
                    });
                }
            }
        }
        out
    }

    /// The zone spanned by three independent projections of D.
    ///
    /// Their spread *is* the zone. When they agree it is narrow and the statement sharp; when they
    /// disagree it is wide, and that is the information a single line would suppress.
    fn prz(&self, def: &HarmonicDefinition, x: f64, a: f64, b: f64, c: f64) -> Prz {
        let xa = a - x;
        let bc = c - b;
        let ab = b - a;

        let from_xa = a - def.d.center() * xa;
        let from_bc = c + def.cd.center() * (-bc);
        let from_abcd = c + ab;

        let mut low = from_xa.min(from_bc).min(from_abcd);
        let mut high = from_xa.max(from_bc).max(from_abcd);

        // A zone that collapses to a line would be read as a price, which is the misreading this
        // whole type exists to prevent.
        let floor = self.config.min_prz_share * xa.abs();
        if high - low < floor {
            let mid = (high + low) / 2.0;
            low = mid - floor / 2.0;
            high = mid + floor / 2.0;
        }
        Prz { low, high }
    }
}

/// The four ratios, measured. `None` when a leg has zero length and the ratio is undefined.
fn measure(
    x: f64,
    a: f64,
    b: f64,
    c: f64,
    d: Option<f64>,
) -> Option<(f64, f64, Option<f64>, Option<f64>)> {
    let xa = (a - x).abs();
    let ab = (b - a).abs();
    let bc = (c - b).abs();
    if xa <= f64::EPSILON || ab <= f64::EPSILON || bc <= f64::EPSILON {
        return None;
    }
    let b_ratio = ab / xa;
    let c_ratio = bc / ab;
    let (d_ratio, cd_ratio) = match d {
        Some(d) => (Some((a - d).abs() / xa), Some((d - c).abs() / bc)),
        None => (None, None),
    };
    Some((b_ratio, c_ratio, d_ratio, cd_ratio))
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

    /// X = 100, A = 120, Gartley ratios: B = 107.64, C = 113.82, D = 104.28.
    ///
    /// Hand-calculated from the table this module prints, which is the point — chapter and
    /// detector read the same numbers.
    fn gartley_nodes() -> Vec<ZigZagNode> {
        vec![
            node(0, 100.0, false),
            node(600, 120.0, true),
            node(1200, 107.64, false),
            node(1800, 113.82, true),
            node(2400, 104.28, false),
        ]
    }

    #[test]
    fn measures_the_ratios_of_the_table() {
        let (b, c, d, cd) = measure(100.0, 120.0, 107.64, 113.82, Some(104.28)).unwrap();
        assert!((b - 0.618).abs() < 1e-9, "b = {b}");
        assert!((c - 0.5).abs() < 1e-9, "c = {c}");
        assert!((d.unwrap() - 0.786).abs() < 1e-9);
        // CD = |104.28 - 113.82| = 9.54, BC = 6.18 → 1.5436…
        assert!((cd.unwrap() - 9.54 / 6.18).abs() < 1e-9);
    }

    #[test]
    fn recognises_a_gartley() {
        let found = HarmonicDetector::default().scan(&gartley_nodes());
        assert!(found.iter().any(|h| h.name == "gartley"), "{found:?}");
        let g = found.iter().find(|h| h.name == "gartley").unwrap();
        assert!(g.bullish, "X is a low, so D resolves upward");
        assert_eq!(g.state, HarmonicState::Complete);
    }

    #[test]
    fn the_tolerance_decides_whether_it_exists() {
        // 0.60 instead of 0.618 — B at 108.0. Inside a three-point tolerance, outside a tight one.
        let mut nodes = gartley_nodes();
        nodes[2].price = 108.0;

        let weit = HarmonicDetector::default();
        assert!(weit.scan(&nodes).iter().any(|h| h.name == "gartley"));

        let eng = HarmonicDetector::new(HarmonicConfig {
            ratio_tolerance: 0.0,
            ..HarmonicConfig::default()
        });
        assert!(!eng.scan(&nodes).iter().any(|h| h.name == "gartley"));
    }

    #[test]
    fn a_candidate_has_no_d_but_a_zone() {
        let nodes = &gartley_nodes()[..4];
        let candidates = HarmonicDetector::default().scan_candidates(nodes);
        let g = candidates
            .iter()
            .find(|h| h.name == "gartley")
            .expect("candidate");
        assert_eq!(g.state, HarmonicState::Candidate);
        assert!(g.d.is_none());
        assert!(g.prz.width() > 0.0, "a zone, never a line");
        assert!(
            g.prz.contains(104.28),
            "the actual D lies inside: {:?}",
            g.prz
        );
    }

    #[test]
    fn the_zone_is_never_a_line() {
        // Degenerate case: all three projections coincide. The floor keeps it a zone.
        let d = HarmonicDetector::default();
        let prz = d.prz(&HARMONIC_TABLE[0], 100.0, 120.0, 107.64, 113.82);
        assert!(prz.width() >= 0.005 * 20.0);
    }

    #[test]
    fn a_candidate_completes_and_confirms() {
        let mut c = HarmonicDetector::default()
            .scan_candidates(&gartley_nodes()[..4])
            .into_iter()
            .find(|h| h.name == "gartley")
            .expect("candidate");

        // Above the zone: nothing has happened.
        assert_eq!(
            c.update_state(&Bar::new(3000, 110.0, 111.0, 109.0, 110.0, 1.0)),
            HarmonicState::Candidate
        );
        // Into the zone.
        let mitte = (c.prz.low + c.prz.high) / 2.0;
        assert_eq!(
            c.update_state(&Bar::new(3600, 106.0, 106.5, mitte, mitte + 0.2, 1.0)),
            HarmonicState::Complete
        );
        // Back out of it upward.
        assert_eq!(
            c.update_state(&Bar::new(4200, 106.0, 112.0, 105.8, 111.0, 1.0)),
            HarmonicState::Confirmed
        );
    }

    #[test]
    fn running_through_the_zone_invalidates() {
        let mut c = HarmonicDetector::default()
            .scan_candidates(&gartley_nodes()[..4])
            .into_iter()
            .find(|h| h.name == "gartley")
            .expect("candidate");
        assert_eq!(
            c.update_state(&Bar::new(3600, 106.0, 106.5, 90.0, 91.0, 1.0)),
            HarmonicState::Invalidated
        );
        // Terminal: a later recovery does not resurrect it.
        assert_eq!(
            c.update_state(&Bar::new(4200, 91.0, 120.0, 91.0, 119.0, 1.0)),
            HarmonicState::Invalidated
        );
    }

    #[test]
    fn a_butterfly_extends_past_x_and_a_gartley_does_not() {
        let gartley = HARMONIC_TABLE.iter().find(|d| d.name == "gartley").unwrap();
        let butterfly = HARMONIC_TABLE
            .iter()
            .find(|d| d.name == "butterfly")
            .unwrap();
        assert!(gartley.d.max < 1.0, "retracement stays inside XA");
        assert!(butterfly.d.min > 1.0, "extension runs past X");
    }
}
