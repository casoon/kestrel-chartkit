use crate::model::{Bar, BarQuality};
use std::collections::HashMap;

/// Synchronized bar pair for multi-series computations (e.g. Asset vs Benchmark).
#[derive(Debug, Clone, PartialEq)]
pub struct SynchronizedBarPair {
    pub timestamp: i64,
    pub primary: Bar,
    pub benchmark: Bar,
    /// Original timestamp of the benchmark sample, retained when forward-filled.
    pub benchmark_source_timestamp: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MissingSeriesPolicy {
    Skip,
    #[default]
    ForwardFill,
}

/// Synchronizer for aligning multiple price series with different timestamps or missing bars.
#[derive(Debug, Clone, Default)]
pub struct MultiSeriesSync {
    last_benchmark: Option<Bar>,
    missing_policy: MissingSeriesPolicy,
}

impl MultiSeriesSync {
    pub fn new() -> Self {
        Self {
            last_benchmark: None,
            missing_policy: MissingSeriesPolicy::ForwardFill,
        }
    }

    pub fn with_policy(missing_policy: MissingSeriesPolicy) -> Self {
        Self {
            last_benchmark: None,
            missing_policy,
        }
    }

    pub fn reset(&mut self) {
        self.last_benchmark = None;
    }

    /// Aligns incoming primary asset bars with benchmark bars.
    /// A missing benchmark is either skipped or forward-filled according to the configured policy.
    pub fn align_step(
        &mut self,
        primary: &Bar,
        benchmark: Option<&Bar>,
    ) -> Option<SynchronizedBarPair> {
        let bench_bar = match benchmark {
            Some(b) => {
                self.last_benchmark = Some(b.clone());
                b.clone()
            }
            None if self.missing_policy == MissingSeriesPolicy::ForwardFill => {
                self.last_benchmark.clone()?
            }
            None => return None,
        };

        let benchmark_source_timestamp = bench_bar.timestamp;

        Some(SynchronizedBarPair {
            timestamp: primary.timestamp,
            primary: primary.clone(),
            benchmark: bench_bar,
            benchmark_source_timestamp,
        })
    }

    /// Batch aligns two full series of bars by timestamp.
    pub fn align_series(
        primary_series: &[Bar],
        benchmark_series: &[Bar],
    ) -> Vec<SynchronizedBarPair> {
        let mut bench_map: HashMap<i64, Bar> = HashMap::new();
        for b in benchmark_series {
            bench_map.insert(b.timestamp, b.clone());
        }

        let mut sync = MultiSeriesSync::new();
        let mut result = Vec::with_capacity(primary_series.len());

        for p_bar in primary_series {
            let b_bar = bench_map.get(&p_bar.timestamp);
            if let Some(pair) = sync.align_step(p_bar, b_bar) {
                result.push(pair);
            }
        }

        result
    }
}

/// One series' bar within an [`AlignedStep`], tagged with explicit [`BarQuality`] (observed vs.
/// forward-filled, and whether a gap preceded it).
#[derive(Debug, Clone, PartialEq)]
pub struct AlignedSeriesBar {
    pub bar: Bar,
    pub quality: BarQuality,
}

/// One synchronized alignment step across an arbitrary number of named series.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct AlignedStep {
    pub timestamp: i64,
    /// Present entries are keyed by series ID. A series absent from this map means it was
    /// missing this step and either had no prior bar to forward-fill from, or its last bar aged
    /// out under [`MultiSeriesAligner::with_max_forward_fill_age`] — i.e. absence itself is the
    /// explicit gap signal.
    pub series: HashMap<String, AlignedSeriesBar>,
}

/// Generalizes [`MultiSeriesSync`] from exactly two series (primary/benchmark) to an arbitrary,
/// named set: N-way synchronization, an optional maximum forward-fill staleness age, and explicit
/// per-series [`BarQuality`] (observed vs. forward-filled) instead of a single implicit benchmark
/// slot.
#[derive(Debug, Clone)]
pub struct MultiSeriesAligner {
    series_ids: Vec<String>,
    last_seen: HashMap<String, (Bar, i64)>,
    missing_policy: MissingSeriesPolicy,
    max_forward_fill_age_seconds: Option<i64>,
}

impl MultiSeriesAligner {
    pub fn new(series_ids: impl IntoIterator<Item = impl Into<String>>) -> Self {
        Self {
            series_ids: series_ids.into_iter().map(Into::into).collect(),
            last_seen: HashMap::new(),
            missing_policy: MissingSeriesPolicy::ForwardFill,
            max_forward_fill_age_seconds: None,
        }
    }

    pub fn with_policy(mut self, policy: MissingSeriesPolicy) -> Self {
        self.missing_policy = policy;
        self
    }

    /// Caps how old (in seconds) a forward-filled bar may be before it is dropped instead of
    /// reused, so a series that has gone permanently silent does not keep forward-filling
    /// forever.
    pub fn with_max_forward_fill_age(mut self, seconds: i64) -> Self {
        self.max_forward_fill_age_seconds = Some(seconds);
        self
    }

    pub fn reset(&mut self) {
        self.last_seen.clear();
    }

    /// Aligns one step at `timestamp`. `incoming` holds bars for whichever configured series
    /// delivered one for exactly this timestamp; missing series are forward-filled (subject to
    /// [`MultiSeriesAligner::with_max_forward_fill_age`]) or skipped per the configured
    /// [`MissingSeriesPolicy`].
    pub fn align_step(&mut self, timestamp: i64, incoming: &HashMap<String, Bar>) -> AlignedStep {
        let mut series = HashMap::with_capacity(self.series_ids.len());

        for id in &self.series_ids {
            if let Some(bar) = incoming.get(id) {
                self.last_seen.insert(id.clone(), (bar.clone(), timestamp));
                series.insert(
                    id.clone(),
                    AlignedSeriesBar {
                        bar: bar.clone(),
                        quality: BarQuality::observed(),
                    },
                );
                continue;
            }

            if self.missing_policy != MissingSeriesPolicy::ForwardFill {
                continue;
            }

            let Some((last_bar, last_ts)) = self.last_seen.get(id) else {
                continue;
            };
            let age = timestamp - last_ts;
            let within_age = self
                .max_forward_fill_age_seconds
                .map(|max| age <= max)
                .unwrap_or(true);
            if !within_age {
                continue;
            }

            let mut filled = last_bar.clone();
            filled.timestamp = timestamp;
            series.insert(
                id.clone(),
                AlignedSeriesBar {
                    bar: filled,
                    quality: BarQuality {
                        volume_available: false,
                        is_synthetic: false,
                        is_forward_filled: true,
                        has_gap: age > 0,
                    },
                },
            );
        }

        AlignedStep { timestamp, series }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_multi_series_sync_forward_fill() {
        let mut sync = MultiSeriesSync::new();
        let b1 = Bar::new(1000, 10.0, 11.0, 9.0, 10.5, 500.0);
        let p1 = Bar::new(1000, 100.0, 105.0, 95.0, 102.0, 1000.0);
        let p2 = Bar::new(2000, 102.0, 106.0, 101.0, 105.0, 1000.0);

        let pair1 = sync.align_step(&p1, Some(&b1)).unwrap();
        assert_eq!(pair1.benchmark.timestamp, 1000);

        // p2 arrives, benchmark is missing for 2000 -> forward fills b1
        let pair2 = sync.align_step(&p2, None).unwrap();
        assert_eq!(pair2.benchmark.timestamp, 1000);
        assert_eq!(pair2.benchmark_source_timestamp, 1000);
        assert_eq!(pair2.benchmark.close, 10.5);
    }

    #[test]
    fn never_substitutes_the_primary_for_a_missing_benchmark() {
        let primary = Bar::new(1_000, 100.0, 105.0, 95.0, 102.0, 1_000.0);
        assert!(MultiSeriesSync::new().align_step(&primary, None).is_none());
        assert!(MultiSeriesSync::with_policy(MissingSeriesPolicy::Skip)
            .align_step(&primary, None)
            .is_none());
    }

    #[test]
    fn test_multi_series_aligner_aligns_more_than_two_series() {
        let mut aligner =
            MultiSeriesAligner::new(["a", "b", "c"]).with_policy(MissingSeriesPolicy::ForwardFill);

        let mut incoming = HashMap::new();
        incoming.insert("a".to_string(), Bar::new(0, 1.0, 2.0, 0.5, 1.5, 10.0));
        incoming.insert("b".to_string(), Bar::new(0, 2.0, 3.0, 1.5, 2.5, 10.0));
        incoming.insert("c".to_string(), Bar::new(0, 3.0, 4.0, 2.5, 3.5, 10.0));
        let step0 = aligner.align_step(0, &incoming);
        assert_eq!(step0.series.len(), 3);
        for bar in step0.series.values() {
            assert_eq!(bar.quality, BarQuality::observed());
        }

        // Only "a" delivers at t=60; "b" and "c" forward-fill with explicit quality flags.
        let mut incoming = HashMap::new();
        incoming.insert("a".to_string(), Bar::new(60, 1.1, 2.1, 0.6, 1.6, 10.0));
        let step1 = aligner.align_step(60, &incoming);
        assert_eq!(step1.series.len(), 3);
        assert_eq!(step1.series["a"].quality, BarQuality::observed());
        assert!(step1.series["b"].quality.is_forward_filled);
        assert!(step1.series["b"].quality.has_gap);
        assert_eq!(step1.series["b"].bar.close, 2.5);
        assert_eq!(step1.series["b"].bar.timestamp, 60);
    }

    #[test]
    fn test_multi_series_aligner_drops_stale_forward_fills() {
        let mut aligner = MultiSeriesAligner::new(["a", "b"])
            .with_policy(MissingSeriesPolicy::ForwardFill)
            .with_max_forward_fill_age(90);

        let mut incoming = HashMap::new();
        incoming.insert("a".to_string(), Bar::new(0, 1.0, 2.0, 0.5, 1.5, 10.0));
        incoming.insert("b".to_string(), Bar::new(0, 2.0, 3.0, 1.5, 2.5, 10.0));
        aligner.align_step(0, &incoming);

        // "b" stays silent past the 90s max forward-fill age -> dropped, not stale-filled.
        let step = aligner.align_step(
            200,
            &HashMap::from([("a".to_string(), Bar::new(200, 1.2, 2.2, 0.7, 1.7, 10.0))]),
        );
        assert!(step.series.contains_key("a"));
        assert!(
            !step.series.contains_key("b"),
            "stale forward-fill beyond max age must be omitted, not silently reused"
        );
    }

    #[test]
    fn test_multi_series_aligner_skip_policy_never_forward_fills() {
        let mut aligner =
            MultiSeriesAligner::new(["a", "b"]).with_policy(MissingSeriesPolicy::Skip);
        aligner.align_step(
            0,
            &HashMap::from([
                ("a".to_string(), Bar::new(0, 1.0, 2.0, 0.5, 1.5, 10.0)),
                ("b".to_string(), Bar::new(0, 2.0, 3.0, 1.5, 2.5, 10.0)),
            ]),
        );

        let step = aligner.align_step(
            60,
            &HashMap::from([("a".to_string(), Bar::new(60, 1.1, 2.1, 0.6, 1.6, 10.0))]),
        );
        assert!(step.series.contains_key("a"));
        assert!(!step.series.contains_key("b"));
    }
}
