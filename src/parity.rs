//! Pine-parity fixture harness: a standardized way to compare a Rust indicator's output series
//! against confirmed Pine reference values, with timestamp alignment, automatic warmup handling,
//! per-row or default tolerances, explicit missing-value rows, and an MTF-boundary-aware
//! comparison mode — reusable across the whole porting scope instead of the hand-rolled
//! per-indicator fixture parsing the existing golden tests use.

use std::collections::HashMap;

use crate::indicator::IndicatorOutput;
use crate::runner::TimestampedOutput;
use crate::timeframe::Timeframe;

/// One expected reference row: timestamp, expected value, and an optional per-row tolerance
/// overriding [`ParityFixture::default_tolerance`]. `expected.is_nan()` marks an explicitly
/// missing/skip row (a bar with no confirmed Pine reference, e.g. inside its own warmup).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParityFixtureRow {
    pub timestamp: i64,
    pub expected: f64,
    pub tolerance: Option<f64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParityFixture {
    pub rows: Vec<ParityFixtureRow>,
    pub default_tolerance: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParityFixtureError {
    pub line_number: usize,
    pub line: String,
    pub reason: String,
}

impl ParityFixture {
    /// Parses `text`: one row per non-empty, non-`#`-comment line, `timestamp,expected` or
    /// `timestamp,expected,tolerance` (CSV, whitespace-trimmed). `expected` may be `nan`/`NaN` to
    /// mark an explicit missing-value row.
    pub fn parse(text: &str, default_tolerance: f64) -> Result<Self, ParityFixtureError> {
        let mut rows = Vec::new();
        for (i, raw_line) in text.lines().enumerate() {
            let line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split(',').map(str::trim).collect();
            if parts.len() < 2 || parts.len() > 3 {
                return Err(ParityFixtureError {
                    line_number: i + 1,
                    line: raw_line.to_string(),
                    reason: "expected 'timestamp,expected[,tolerance]'".to_string(),
                });
            }
            let timestamp: i64 = parts[0].parse().map_err(|_| ParityFixtureError {
                line_number: i + 1,
                line: raw_line.to_string(),
                reason: "invalid timestamp".to_string(),
            })?;
            let expected: f64 = parts[1].parse().map_err(|_| ParityFixtureError {
                line_number: i + 1,
                line: raw_line.to_string(),
                reason: "invalid expected value".to_string(),
            })?;
            let tolerance = match parts.get(2) {
                Some(s) => Some(s.parse().map_err(|_| ParityFixtureError {
                    line_number: i + 1,
                    line: raw_line.to_string(),
                    reason: "invalid tolerance".to_string(),
                })?),
                None => None,
            };
            rows.push(ParityFixtureRow {
                timestamp,
                expected,
                tolerance,
            });
        }
        Ok(Self {
            rows,
            default_tolerance,
        })
    }
}

/// Outcome of comparing one fixture row against the actual output series.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParityOutcome {
    Match {
        timestamp: i64,
        actual: f64,
        expected: f64,
    },
    Mismatch {
        timestamp: i64,
        actual: f64,
        expected: f64,
        diff: f64,
        tolerance: f64,
    },
    /// The fixture expects a confirmed value at this timestamp but the actual series has no bar
    /// there, or the indicator was still in warmup (`None`) — a genuine parity gap.
    MissingActual { timestamp: i64 },
    /// The fixture explicitly marks this timestamp as having no reference value (`expected` was
    /// `NaN`); not compared.
    SkippedMissingExpected { timestamp: i64 },
    /// The fixture expects a value at a timestamp that is not a confirmed higher-timeframe bucket
    /// boundary under the configured MTF comparison mode; not compared.
    SkippedNotConfirmedBoundary { timestamp: i64 },
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParityReport {
    pub outcomes: Vec<ParityOutcome>,
}

impl ParityReport {
    pub fn all_passed(&self) -> bool {
        !self.outcomes.iter().any(|o| {
            matches!(
                o,
                ParityOutcome::Mismatch { .. } | ParityOutcome::MissingActual { .. }
            )
        })
    }

    pub fn mismatches(&self) -> Vec<&ParityOutcome> {
        self.outcomes
            .iter()
            .filter(|o| {
                matches!(
                    o,
                    ParityOutcome::Mismatch { .. } | ParityOutcome::MissingActual { .. }
                )
            })
            .collect()
    }

    pub fn matched_count(&self) -> usize {
        self.outcomes
            .iter()
            .filter(|o| matches!(o, ParityOutcome::Match { .. }))
            .count()
    }
}

/// Compares `actual` (e.g. from [`crate::runner::run_batch`]) against `fixture`, aligning by
/// timestamp. `value_selector` extracts the scalar to compare from each bar's
/// [`IndicatorOutput`] (e.g. `|o| o.value`).
pub fn compare_series(
    actual: &[TimestampedOutput],
    fixture: &ParityFixture,
    value_selector: impl Fn(&IndicatorOutput) -> f64,
) -> ParityReport {
    compare_series_filtered(actual, fixture, value_selector, |_| true)
}

/// Like [`compare_series`], but only compares fixture rows whose timestamp is itself a `target_tf`
/// bucket boundary (via `Timeframe::bucket_start`) — the MTF-boundary-aware mode, for
/// validating a port's higher-timeframe output only at points where Pine's confirmed HTF value is
/// actually available, not mid-bucket.
pub fn compare_series_at_timeframe_boundaries(
    actual: &[TimestampedOutput],
    fixture: &ParityFixture,
    value_selector: impl Fn(&IndicatorOutput) -> f64,
    target_tf: Timeframe,
    utc_offset_seconds: i32,
) -> ParityReport {
    compare_series_filtered(actual, fixture, value_selector, |timestamp| {
        target_tf.bucket_start(timestamp, utc_offset_seconds) == timestamp
    })
}

fn compare_series_filtered(
    actual: &[TimestampedOutput],
    fixture: &ParityFixture,
    value_selector: impl Fn(&IndicatorOutput) -> f64,
    boundary_filter: impl Fn(i64) -> bool,
) -> ParityReport {
    let actual_map: HashMap<i64, Option<f64>> = actual
        .iter()
        .map(|entry| (entry.timestamp, entry.output.as_ref().map(&value_selector)))
        .collect();

    let outcomes = fixture
        .rows
        .iter()
        .map(|row| {
            if row.expected.is_nan() {
                return ParityOutcome::SkippedMissingExpected {
                    timestamp: row.timestamp,
                };
            }
            if !boundary_filter(row.timestamp) {
                return ParityOutcome::SkippedNotConfirmedBoundary {
                    timestamp: row.timestamp,
                };
            }
            match actual_map.get(&row.timestamp) {
                Some(Some(actual)) => {
                    let tolerance = row.tolerance.unwrap_or(fixture.default_tolerance);
                    let diff = (actual - row.expected).abs();
                    if diff <= tolerance {
                        ParityOutcome::Match {
                            timestamp: row.timestamp,
                            actual: *actual,
                            expected: row.expected,
                        }
                    } else {
                        ParityOutcome::Mismatch {
                            timestamp: row.timestamp,
                            actual: *actual,
                            expected: row.expected,
                            diff,
                            tolerance,
                        }
                    }
                }
                _ => ParityOutcome::MissingActual {
                    timestamp: row.timestamp,
                },
            }
        })
        .collect();

    ParityReport { outcomes }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn output_series(pairs: &[(i64, Option<f64>)]) -> Vec<TimestampedOutput> {
        pairs
            .iter()
            .map(|&(ts, v)| TimestampedOutput {
                timestamp: ts,
                output: v.map(IndicatorOutput::new),
            })
            .collect()
    }

    #[test]
    fn test_parses_fixture_with_optional_tolerance_and_comments() {
        let text = "# comment\n0,1.5\n60,2.0,0.01\n\n120,nan\n";
        let fixture = ParityFixture::parse(text, 0.001).unwrap();
        assert_eq!(fixture.rows.len(), 3);
        assert_eq!(fixture.rows[0].tolerance, None);
        assert_eq!(fixture.rows[1].tolerance, Some(0.01));
        assert!(fixture.rows[2].expected.is_nan());
    }

    #[test]
    fn test_parse_rejects_malformed_row() {
        let err = ParityFixture::parse("not,a,valid,row,here", 0.001).unwrap_err();
        assert_eq!(err.line_number, 1);
    }

    #[test]
    fn test_match_within_tolerance() {
        let actual = output_series(&[(0, Some(1.4995))]);
        let fixture = ParityFixture::parse("0,1.5", 0.001).unwrap();
        let report = compare_series(&actual, &fixture, |o| o.value);
        assert!(report.all_passed());
        assert_eq!(report.matched_count(), 1);
    }

    #[test]
    fn test_mismatch_beyond_tolerance() {
        let actual = output_series(&[(0, Some(2.0))]);
        let fixture = ParityFixture::parse("0,1.5,0.01", 0.001).unwrap();
        let report = compare_series(&actual, &fixture, |o| o.value);
        assert!(!report.all_passed());
        assert_eq!(report.mismatches().len(), 1);
    }

    #[test]
    fn test_missing_actual_during_warmup_is_a_gap_not_silently_skipped() {
        let actual = output_series(&[(0, None)]);
        let fixture = ParityFixture::parse("0,1.5", 0.001).unwrap();
        let report = compare_series(&actual, &fixture, |o| o.value);
        assert!(!report.all_passed());
        assert!(matches!(
            report.outcomes[0],
            ParityOutcome::MissingActual { .. }
        ));
    }

    #[test]
    fn test_explicit_missing_expected_is_skipped_not_a_failure() {
        let actual = output_series(&[(0, None)]);
        let fixture = ParityFixture::parse("0,nan", 0.001).unwrap();
        let report = compare_series(&actual, &fixture, |o| o.value);
        assert!(report.all_passed());
        assert!(matches!(
            report.outcomes[0],
            ParityOutcome::SkippedMissingExpected { .. }
        ));
    }

    #[test]
    fn test_mtf_boundary_mode_skips_non_boundary_rows() {
        let actual = output_series(&[(30, Some(1.0)), (300, Some(2.0))]);
        // 30 is not a 5-minute (300s) bucket boundary; 300 is.
        let fixture = ParityFixture::parse("30,1.0\n300,2.0", 0.001).unwrap();
        let report = compare_series_at_timeframe_boundaries(
            &actual,
            &fixture,
            |o| o.value,
            Timeframe::Minute(5),
            0,
        );
        assert!(matches!(
            report.outcomes[0],
            ParityOutcome::SkippedNotConfirmedBoundary { .. }
        ));
        assert!(matches!(report.outcomes[1], ParityOutcome::Match { .. }));
    }
}
