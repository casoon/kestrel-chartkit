//! Indicators over two bar series, built by name.
//!
//! [`Indicator::on_bar`] sees one bar of one series. A relative strength or a return correlation
//! compares that series with a second one, the *reference* (a benchmark, a sector, another
//! instrument). Such an indicator implements [`PairIndicator`] and is listed in [`pair_catalog`],
//! not in [`catalog`](super::registry::catalog): every entry of `catalog()` builds with
//! `build_checked` and runs on one series, and that stays so.
//!
//! The path mirrors the one-series path step by step:
//!
//! | One series | Two series |
//! |---|---|
//! | [`catalog`](super::registry::catalog) | [`pair_catalog`] |
//! | [`build_checked`](super::registry::build_checked) | [`build_pair_checked`] |
//! | [`Indicator::on_bar`] | [`PairIndicator::on_bars`] |
//! | [`run_batch`](crate::runner::run_batch) | [`run_pair_batch`] |
//!
//! [`input_series`] says for any name which of the two paths builds it. Parameters are declared
//! and validated the same way ([`ParamSpec`], one `f64` map), and the result is the same
//! [`IndicatorOutput`], so a consumer needs no case per indicator: only the second bar series is
//! new. [`WithReference`] binds a reference series to a pair indicator and is an [`Indicator`]
//! again, for code that holds a `Box<dyn Indicator>` and feeds it bar by bar.
//!
//! # Alignment
//!
//! The two series are aligned on their timestamps, the rule of
//! [`align_closes`](crate::cross_asset::align_closes): only timestamps both series hold count.
//!
//! - A bar of the first series whose timestamp the reference lacks is **skipped**: the indicator
//!   does not see it, its state does not change, and the output for that bar is `None`. Nothing
//!   is carried forward to fill the gap, because a filled-in close would be a price of another
//!   point in time.
//! - A bar of the reference whose timestamp the first series lacks is **ignored**.
//! - Where the reference holds a timestamp twice, the later bar counts.
//! - Windows and warmup therefore count *paired* bars, not the bars of either series.
//!
//! The timestamp is an opaque key: both series must use the same unit and the same bar boundary.
//! Whether equal timestamps mean the same span of trading is a property of the two series, see
//! [`check_pair_applicability`](crate::applicability::check_pair_applicability).

use std::collections::{BTreeMap, HashMap};

use super::definition::{definition, f64_defaults, resolve_f64_map};
use super::definition::{Check, ParamSpec, ResolvedParams};
use super::registry::{IndicatorCatalogEntry, OutputRange, RegistryError};
use super::unit::IndicatorUnit;
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;
use crate::runner::TimestampedOutput;

/// An indicator over two bar series: the series itself and a reference it is compared with.
///
/// The counterpart of [`Indicator`] with one more bar per step. Callers pass two bars with the
/// same timestamp; [`run_pair_batch`] and [`WithReference`] do the pairing.
pub trait PairIndicator: Send + Sync {
    fn name(&self) -> &str;
    /// The number of paired bars up to and including the first output.
    fn warmup_period(&self) -> usize {
        0
    }
    fn reset(&mut self);
    /// One step: `bar` of the series and `reference`, the bar of the reference series with the
    /// same timestamp.
    fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput>;
    /// Alerts raised by the last [`PairIndicator::on_bars`] call.
    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

impl<T: PairIndicator + ?Sized> PairIndicator for Box<T> {
    fn name(&self) -> &str {
        (**self).name()
    }
    fn warmup_period(&self) -> usize {
        (**self).warmup_period()
    }
    fn reset(&mut self) {
        (**self).reset()
    }
    fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput> {
        (**self).on_bars(bar, reference)
    }
    fn alerts(&self) -> Vec<IndicatorAlert> {
        (**self).alerts()
    }
}

/// Everything the registry knows about one two-series indicator: the counterpart of
/// [`IndicatorDefinition`](super::definition::IndicatorDefinition), with the unit and range of the
/// output declared here instead of in the by-name tables of the one-series registry.
pub struct PairIndicatorDefinition {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub params: &'static [ParamSpec],
    pub checks: &'static [Check],
    /// What [`output_unit`](super::unit::output_unit) answers for this name.
    pub unit: IndicatorUnit,
    /// What [`output_range`](super::registry::output_range) answers for this name.
    pub output_range: OutputRange,
    pub build: fn(&ResolvedParams) -> Result<Box<dyn PairIndicator>, RegistryError>,
}

impl PairIndicatorDefinition {
    /// The catalog entry: name, description and parameter defaults.
    pub fn catalog_entry(&self) -> IndicatorCatalogEntry {
        IndicatorCatalogEntry {
            name: self.name,
            description: self.description,
            default_params: f64_defaults(self.params),
        }
    }
}

/// Every two-series indicator. Each definition lives beside its implementation.
const DEFINITIONS: &[&PairIndicatorDefinition] = &[
    &super::correlation::CORRELATION_DEFINITION,
    &super::relative_strength::RELATIVE_STRENGTH_DEFINITION,
];

/// Every two-series indicator definition.
pub fn pair_definitions() -> impl Iterator<Item = &'static PairIndicatorDefinition> {
    DEFINITIONS.iter().copied()
}

/// The definition of the two-series indicator `name` (canonical name or alias,
/// case-insensitive).
pub fn pair_definition(name: &str) -> Option<&'static PairIndicatorDefinition> {
    let lower = name.to_lowercase();
    pair_definitions().find(|d| d.name == lower || d.aliases.contains(&lower.as_str()))
}

/// Every indicator that needs two bar series, each buildable with [`build_pair_checked`]. None of
/// these names is in [`catalog`](super::registry::catalog), and none of those is in here.
pub fn pair_catalog() -> Vec<IndicatorCatalogEntry> {
    pair_definitions().map(|d| d.catalog_entry()).collect()
}

/// How many bar series the indicator `name` reads: `2` for a name of [`pair_catalog`] (build it
/// with [`build_pair_checked`]), `1` for a name of [`catalog`](super::registry::catalog) (build
/// it with [`build_checked`](super::registry::build_checked)), `None` for an unknown name.
/// Aliases count like the name they stand for.
pub fn input_series(name: &str) -> Option<usize> {
    if pair_definition(name).is_some() {
        Some(2)
    } else {
        definition(name).map(|_| 1)
    }
}

impl IndicatorCatalogEntry {
    /// How many bar series this entry reads, see [`input_series`].
    pub fn input_series(&self) -> usize {
        input_series(self.name).unwrap_or(1)
    }
}

/// Builds a two-series indicator from its [`pair_catalog`] name (or an alias) and an `f64`
/// parameter map, validated like [`build_checked`](super::registry::build_checked): unknown keys,
/// whole numbers, ranges and relations are errors, missing parameters take their defaults. A
/// one-series name is a [`RegistryError::UnknownIndicator`] here, as a two-series name is there.
pub fn build_pair_checked(
    name: &str,
    params: &HashMap<String, f64>,
) -> Result<Box<dyn PairIndicator>, RegistryError> {
    let definition =
        pair_definition(name).ok_or_else(|| RegistryError::UnknownIndicator(name.to_string()))?;
    (definition.build)(&resolve_f64_map(
        definition.params,
        definition.checks,
        name,
        params,
    )?)
}

/// The reference series by timestamp. Where it holds a timestamp twice, the later bar counts
/// (as in [`align_closes`](crate::cross_asset::align_closes)).
fn by_timestamp(reference: &[Bar]) -> BTreeMap<i64, Bar> {
    reference
        .iter()
        .map(|bar| (bar.timestamp, bar.clone()))
        .collect()
}

/// Resets `indicator`, then feeds it `bars` in order, each paired with the bar of `reference`
/// that has the same timestamp. One [`TimestampedOutput`] per bar of `bars`, as
/// [`run_batch`](crate::runner::run_batch) gives: `None` during warmup and for every bar whose
/// timestamp `reference` lacks (that bar is skipped, see the [module documentation](self)).
pub fn run_pair_batch<P: PairIndicator + ?Sized>(
    indicator: &mut P,
    bars: &[Bar],
    reference: &[Bar],
) -> Vec<TimestampedOutput> {
    indicator.reset();
    let reference = by_timestamp(reference);
    bars.iter()
        .map(|bar| TimestampedOutput {
            timestamp: bar.timestamp,
            output: reference
                .get(&bar.timestamp)
                .and_then(|partner| indicator.on_bars(bar, partner)),
        })
        .collect()
}

/// A [`PairIndicator`] bound to its reference series: an [`Indicator`] over the first series.
///
/// [`Indicator::on_bar`] looks up the reference bar with the bar's timestamp and passes both on;
/// a bar without such a partner is skipped and gives `None` (the alignment rule of the
/// [module documentation](self)). With it, everything written against `Indicator` —
/// [`run_batch`](crate::runner::run_batch), a boxed indicator fed bar by bar — runs a two-series
/// indicator unchanged. [`Indicator::reset`] resets the calculation and keeps the reference.
pub struct WithReference<P: PairIndicator> {
    pair: P,
    reference: BTreeMap<i64, Bar>,
    /// Whether the last bar had a partner; without one the pair's alerts are not current.
    paired: bool,
}

impl<P: PairIndicator> WithReference<P> {
    pub fn new(pair: P, reference: &[Bar]) -> Self {
        Self {
            pair,
            reference: by_timestamp(reference),
            paired: false,
        }
    }

    /// Adds a bar to the reference series, for a reference that grows while the indicator runs.
    /// It replaces a bar with the same timestamp. The reference bar of a timestamp has to be
    /// present before the bar of the first series with that timestamp is fed.
    pub fn push_reference(&mut self, bar: Bar) {
        self.reference.insert(bar.timestamp, bar);
    }
}

impl<P: PairIndicator> Indicator for WithReference<P> {
    fn name(&self) -> &str {
        self.pair.name()
    }

    fn warmup_period(&self) -> usize {
        self.pair.warmup_period()
    }

    fn reset(&mut self) {
        self.pair.reset();
        self.paired = false;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let partner = self.reference.get(&bar.timestamp);
        self.paired = partner.is_some();
        self.pair.on_bars(bar, partner?)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        if self.paired {
            self.pair.alerts()
        } else {
            Vec::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cross_asset::{align_closes, CloseSample};
    use crate::indicator::registry::{build_checked, catalog, output_range};
    use crate::indicator::unit::output_unit;
    use crate::runner::run_batch;

    fn bar(timestamp: i64, close: f64) -> Bar {
        Bar::new(timestamp, close, close, close, close, 0.0)
    }

    /// Records the timestamps it is fed and outputs the sum of the two closes.
    #[derive(Default)]
    struct Recorder {
        seen: Vec<(i64, i64)>,
    }

    impl PairIndicator for Recorder {
        fn name(&self) -> &str {
            "recorder"
        }
        fn reset(&mut self) {
            self.seen.clear();
        }
        fn on_bars(&mut self, bar: &Bar, reference: &Bar) -> Option<IndicatorOutput> {
            self.seen.push((bar.timestamp, reference.timestamp));
            Some(IndicatorOutput::new(bar.close + reference.close))
        }
    }

    #[test]
    fn test_catalogs_are_disjoint_and_input_series_tells_them_apart() {
        let single: Vec<&str> = catalog().iter().map(|e| e.name).collect();
        let pairs = pair_catalog();
        assert_eq!(
            pairs.iter().map(|e| e.name).collect::<Vec<_>>(),
            ["correlation", "relative_strength"]
        );
        for entry in &pairs {
            assert!(!single.contains(&entry.name), "{}", entry.name);
            assert_eq!(input_series(entry.name), Some(2));
            assert_eq!(entry.input_series(), 2);
            assert!(matches!(
                build_checked(entry.name, &entry.default_params),
                Err(RegistryError::UnknownIndicator(_))
            ));
            assert!(build_pair_checked(entry.name, &entry.default_params).is_ok());
        }
        for entry in catalog() {
            assert_eq!(input_series(entry.name), Some(1), "{}", entry.name);
            assert_eq!(entry.input_series(), 1);
            assert!(matches!(
                build_pair_checked(entry.name, &HashMap::new()),
                Err(RegistryError::UnknownIndicator(_))
            ));
        }
        assert_eq!(input_series("no_such_indicator"), None);
    }

    #[test]
    fn test_pair_names_and_aliases_do_not_collide_with_the_one_series_registry() {
        for pair in pair_definitions() {
            for name in std::iter::once(&pair.name).chain(pair.aliases) {
                assert!(definition(name).is_none(), "'{name}' is taken");
                assert_eq!(
                    pair_definition(&name.to_uppercase()).unwrap().name,
                    pair.name
                );
                assert_eq!(input_series(name), Some(2));
            }
            let built = (pair.build)(
                &resolve_f64_map(pair.params, pair.checks, pair.name, &HashMap::new()).unwrap(),
            )
            .unwrap();
            assert_eq!(built.name(), pair.name);
        }
    }

    #[test]
    fn test_output_unit_and_range_come_from_the_pair_definition() {
        assert_eq!(output_unit("correlation"), IndicatorUnit::Ratio);
        assert_eq!(
            output_range("correlation"),
            OutputRange::Bounded {
                min: -1.0,
                max: 1.0
            }
        );
        assert_eq!(output_unit("Relative_Strength"), IndicatorUnit::Ratio);
        assert_eq!(
            output_range("relative_strength"),
            OutputRange::Centered { center: 0.0 }
        );
    }

    #[test]
    fn test_build_pair_checked_validates_like_build_checked() {
        let ok = HashMap::from([("len".to_string(), 5.0)]);
        assert!(build_pair_checked("correlation", &ok).is_ok());
        assert!(build_pair_checked("CORRELATION", &ok).is_ok());
        for (name, key, value) in [
            ("correlation", "len", 2.0),
            ("correlation", "len", 5.5),
            ("correlation", "len", f64::NAN),
            ("relative_strength", "len", 0.0),
            ("relative_strength", "len", 10_001.0),
        ] {
            assert!(
                matches!(
                    build_pair_checked(name, &HashMap::from([(key.to_string(), value)])),
                    Err(RegistryError::InvalidParameter { .. })
                ),
                "{name} {key}={value}"
            );
        }
        assert_eq!(
            build_pair_checked("correlation", &HashMap::from([("period".to_string(), 5.0)])).err(),
            Some(RegistryError::UnknownParameter {
                parameter: "period".to_string(),
                indicator: "correlation".to_string(),
            })
        );
    }

    /// The pairing is the timestamp intersection of `align_closes`, in the order of the first
    /// series: a bar without a partner is skipped, a reference bar without one is ignored, and a
    /// timestamp the reference holds twice takes its later bar.
    #[test]
    fn test_run_pair_batch_pairs_on_shared_timestamps() {
        let bars: Vec<Bar> = [0, 1, 2, 3, 4, 5]
            .iter()
            .map(|&t| bar(t, 100.0 + t as f64))
            .collect();
        let reference = vec![
            bar(-1, 9.0),
            bar(0, 10.0),
            bar(2, 12.0),
            bar(3, 13.0),
            bar(3, 13.5),
            bar(5, 15.0),
            bar(7, 17.0),
        ];
        let mut recorder = Recorder::default();
        let out = run_pair_batch(&mut recorder, &bars, &reference);

        assert_eq!(recorder.seen, [(0, 0), (2, 2), (3, 3), (5, 5)]);
        let values: Vec<Option<f64>> = out
            .iter()
            .map(|e| e.output.as_ref().map(|o| o.value))
            .collect();
        assert_eq!(
            values,
            [
                Some(110.0),
                None,
                Some(114.0),
                Some(116.5),
                None,
                Some(120.0)
            ]
        );
        assert_eq!(
            out.iter().map(|e| e.timestamp).collect::<Vec<_>>(),
            [0, 1, 2, 3, 4, 5]
        );

        let samples = |series: &[Bar]| {
            series
                .iter()
                .map(|b| CloseSample {
                    timestamp: b.timestamp,
                    close: b.close,
                })
                .collect::<Vec<_>>()
        };
        let aligned = align_closes(&[
            ("a".to_string(), samples(&bars)),
            ("b".to_string(), samples(&reference)),
        ]);
        let fed: Vec<i64> = recorder.seen.iter().map(|(t, _)| *t).collect();
        assert_eq!(fed, aligned.timestamps);
        assert_eq!(aligned.closes[1].1, [10.0, 12.0, 13.5, 15.0]);
    }

    #[test]
    fn test_run_pair_batch_resets_first_and_handles_empty_series() {
        let bars = vec![bar(0, 1.0), bar(1, 2.0)];
        let mut recorder = Recorder::default();
        run_pair_batch(&mut recorder, &bars, &bars);
        let again = run_pair_batch(&mut recorder, &bars, &bars);
        assert_eq!(recorder.seen.len(), 2);
        assert_eq!(again.len(), 2);

        assert!(run_pair_batch(&mut recorder, &[], &bars).is_empty());
        let unpaired = run_pair_batch(&mut recorder, &bars, &[]);
        assert!(unpaired.iter().all(|e| e.output.is_none()));
        assert_eq!(unpaired.len(), 2);
    }

    /// Bound to its reference, a pair indicator is an `Indicator`: `run_batch` gives what
    /// `run_pair_batch` gives.
    #[test]
    fn test_with_reference_runs_through_run_batch_like_run_pair_batch() {
        let bars: Vec<Bar> = (0..40)
            .map(|t| bar(t, 100.0 + (t as f64 * 0.7).sin() * 5.0 + t as f64 * 0.3))
            .collect();
        let reference: Vec<Bar> = (0..40)
            .filter(|t| t % 7 != 3)
            .map(|t| bar(t, 50.0 + (t as f64 * 0.4).cos() * 3.0))
            .collect();
        for entry in pair_catalog() {
            let params = HashMap::from([("len".to_string(), 5.0)]);
            let mut pair = build_pair_checked(entry.name, &params).unwrap();
            let direct = run_pair_batch(pair.as_mut(), &bars, &reference);
            assert!(direct.iter().any(|e| e.output.is_some()), "{}", entry.name);

            let mut bound: Box<dyn Indicator> = Box::new(WithReference::new(
                build_pair_checked(entry.name, &params).unwrap(),
                &reference,
            ));
            assert_eq!(bound.name(), entry.name);
            assert_eq!(bound.warmup_period(), pair.warmup_period());
            assert_eq!(run_batch(bound.as_mut(), &bars), direct, "{}", entry.name);
        }
    }

    #[test]
    fn test_with_reference_takes_reference_bars_while_running() {
        let mut bound = WithReference::new(Recorder::default(), &[bar(0, 1.0)]);
        assert_eq!(bound.on_bar(&bar(0, 2.0)).map(|o| o.value), Some(3.0));
        assert!(bound.on_bar(&bar(1, 2.0)).is_none());
        bound.push_reference(bar(2, 5.0));
        bound.push_reference(bar(2, 6.0));
        assert_eq!(bound.on_bar(&bar(2, 2.0)).map(|o| o.value), Some(8.0));
    }

    /// An alert belongs to the bar that raised it: after a bar without a partner there is none.
    #[test]
    fn test_with_reference_reports_no_alerts_for_a_skipped_bar() {
        let params = HashMap::from([("len".to_string(), 1.0)]);
        let reference = vec![bar(0, 100.0), bar(1, 100.0)];
        let mut bound = WithReference::new(
            build_pair_checked("relative_strength", &params).unwrap(),
            &reference,
        );
        bound.on_bar(&bar(0, 100.0));
        assert!(bound.on_bar(&bar(1, 110.0)).is_some());
        assert_eq!(bound.alerts().len(), 1);
        assert!(bound.on_bar(&bar(2, 110.0)).is_none());
        assert!(bound.alerts().is_empty());
    }
}
