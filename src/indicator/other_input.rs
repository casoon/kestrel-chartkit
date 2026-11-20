//! The entries of [`catalog`](super::registry::catalog) that do not read one bar of one series
//! per step (plan 141): the two-series indicators ([`super::pair`]), the candle transformation
//! Heikin-Ashi and the intrabar cumulative delta.
//!
//! They are defined like every other entry — name, aliases, description, [`ParamSpec`]s,
//! relations, one `build` — and differ in what `build` returns and in what a step reads. The
//! [`IndicatorInput`] of the entry says which builder builds it:
//!
//! | [`IndicatorInput`] | Builder | Built object |
//! |---|---|---|
//! | `Bars` | [`build_checked`](super::registry::build_checked) | [`Indicator`](super::Indicator) |
//! | `Pair` | [`build_pair_checked`](super::pair::build_pair_checked) | [`PairIndicator`] |
//! | `Transform` | [`build_transform_checked`] | [`BarTransform`] |
//! | `Intrabar` | [`build_intrabar_checked`] | [`IntrabarIndicator`] |
//!
//! Calling a builder with the name of another kind is a [`RegistryError::WrongInput`] naming the
//! right builder, never a panic and never a silent default. Parameters are validated exactly as
//! for `build_checked` (one `f64` map; unknown keys, ranges and relations are errors).
//!
//! The three kinds are one family in the documentation and in `INDICATOR_STATUS.md` ("Other
//! input kinds"): what unites them is the builder, not a topic — a correlation and a cumulative
//! delta share nothing else. A topical family would hide the one fact a consumer needs first,
//! which builder to call.

use std::collections::HashMap;

use super::cvd_intrabar::INTRABAR_CVD_DEFINITION;
use super::definition::{f64_defaults, resolve_f64_map, Check, ParamSpec, ResolvedParams};
use super::pair::{pair_definition, pair_definitions, PairIndicator};
use super::registry::{IndicatorCatalogEntry, IndicatorInput, OutputRange, RegistryError};
use super::unit::IndicatorUnit;
use crate::intrabar::IntrabarGroup;
use crate::model::Bar;
use crate::transform::{HeikinAshiBar, HEIKIN_ASHI_DEFINITION};

use super::cvd_intrabar::IntrabarDelta;

/// Ties the object an entry builds to its [`IndicatorInput`].
pub trait InputKind {
    const INPUT: IndicatorInput;
}

impl InputKind for dyn PairIndicator {
    const INPUT: IndicatorInput = IndicatorInput::Pair;
}

impl InputKind for dyn BarTransform {
    const INPUT: IndicatorInput = IndicatorInput::Transform;
}

impl InputKind for dyn IntrabarIndicator {
    const INPUT: IndicatorInput = IndicatorInput::Intrabar;
}

/// Everything the registry knows about one entry that is not a one-series indicator: the
/// counterpart of [`IndicatorDefinition`](super::definition::IndicatorDefinition) with the unit
/// and range of the primary output declared here (the by-name tables of the one-series path
/// cover `Bars` entries only), and a `build` returning the object of its kind `T`.
pub struct InputDefinition<T: ?Sized + 'static> {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub params: &'static [ParamSpec],
    pub checks: &'static [Check],
    /// What [`output_unit`](super::unit::output_unit) answers for this name.
    pub unit: IndicatorUnit,
    /// What [`output_range`](super::registry::output_range) answers for this name.
    pub output_range: OutputRange,
    pub build: fn(&ResolvedParams) -> Result<Box<T>, RegistryError>,
}

impl<T: ?Sized + InputKind + 'static> InputDefinition<T> {
    /// The catalog entry: name, description, parameter defaults and input kind.
    pub fn catalog_entry(&self) -> IndicatorCatalogEntry {
        IndicatorCatalogEntry {
            name: self.name,
            description: self.description,
            default_params: f64_defaults(self.params),
            input: T::INPUT,
        }
    }

    /// Builds from an `f64` parameter map, validated like
    /// [`build_checked`](super::registry::build_checked).
    pub fn build_from_f64(
        &self,
        requested: &str,
        params: &HashMap<String, f64>,
    ) -> Result<Box<T>, RegistryError> {
        (self.build)(&resolve_f64_map(
            self.params,
            self.checks,
            requested,
            params,
        )?)
    }
}

/// A candle transformation: one bar in, one derived candle out, with state across bars.
///
/// The output type is [`HeikinAshiBar`], the one transformation there is; a transformation with
/// another output would need its own kind.
pub trait BarTransform: Send + Sync {
    fn name(&self) -> &str;
    /// Transforms one bar. Every input bar yields exactly one candle.
    fn update(&mut self, bar: &Bar) -> HeikinAshiBar;
    fn reset(&mut self);
}

/// An indicator over the lower-timeframe bars of each parent bar, see
/// [`IntrabarGrouper`](crate::intrabar::IntrabarGrouper).
///
/// The output type is [`IntrabarDelta`], the one intrabar indicator there is.
pub trait IntrabarIndicator: Send + Sync {
    fn name(&self) -> &str;
    /// Books one parent bucket's children.
    fn on_group(&mut self, group: &IntrabarGroup) -> IntrabarDelta;
    /// Books a parent bar for which no children are available.
    fn on_missing_children(&mut self, parent: &Bar) -> IntrabarDelta;
    fn reset(&mut self);
}

/// Definition of a candle transformation, see [`BarTransform`].
pub type TransformDefinition = InputDefinition<dyn BarTransform>;
/// Definition of an intrabar indicator, see [`IntrabarIndicator`].
pub type IntrabarDefinition = InputDefinition<dyn IntrabarIndicator>;

const TRANSFORMS: &[&TransformDefinition] = &[&HEIKIN_ASHI_DEFINITION];
const INTRABARS: &[&IntrabarDefinition] = &[&INTRABAR_CVD_DEFINITION];

/// Every transformation definition.
pub fn transform_definitions() -> impl Iterator<Item = &'static TransformDefinition> {
    TRANSFORMS.iter().copied()
}

/// Every intrabar definition.
pub fn intrabar_definitions() -> impl Iterator<Item = &'static IntrabarDefinition> {
    INTRABARS.iter().copied()
}

/// The definition of the transformation `name` (canonical name or alias, case-insensitive).
pub fn transform_definition(name: &str) -> Option<&'static TransformDefinition> {
    let lower = name.to_lowercase();
    transform_definitions().find(|d| d.name == lower || d.aliases.contains(&lower.as_str()))
}

/// The definition of the intrabar indicator `name` (canonical name or alias, case-insensitive).
pub fn intrabar_definition(name: &str) -> Option<&'static IntrabarDefinition> {
    let lower = name.to_lowercase();
    intrabar_definitions().find(|d| d.name == lower || d.aliases.contains(&lower.as_str()))
}

/// The catalog entries of every kind but `Bars`, in the order pair, transform, intrabar.
pub(crate) fn other_entries() -> impl Iterator<Item = IndicatorCatalogEntry> {
    pair_definitions()
        .map(|d| d.catalog_entry())
        .chain(transform_definitions().map(|d| d.catalog_entry()))
        .chain(intrabar_definitions().map(|d| d.catalog_entry()))
}

/// What the catalog entry `name` (canonical name or alias, case-insensitive) reads, or `None`
/// for a name that is not in [`catalog`](super::registry::catalog).
pub fn input_of(name: &str) -> Option<IndicatorInput> {
    if super::definition::definition(name).is_some() {
        Some(IndicatorInput::Bars)
    } else if pair_definition(name).is_some() {
        Some(IndicatorInput::Pair)
    } else if transform_definition(name).is_some() {
        Some(IndicatorInput::Transform)
    } else if intrabar_definition(name).is_some() {
        Some(IndicatorInput::Intrabar)
    } else {
        None
    }
}

/// The error of a builder for `wanted` that does not know `name`: [`RegistryError::WrongInput`]
/// when the name belongs to another kind, [`RegistryError::UnknownIndicator`] otherwise.
pub(crate) fn wrong_input(name: &str, wanted: IndicatorInput) -> RegistryError {
    match input_of(name) {
        Some(input) if input != wanted => RegistryError::WrongInput {
            indicator: name.to_string(),
            input,
            builder: input.builder().to_string(),
        },
        _ => RegistryError::UnknownIndicator(name.to_string()),
    }
}

/// [`wrong_input`] for the one-series builders.
pub(crate) fn not_bars(name: &str) -> RegistryError {
    wrong_input(name, IndicatorInput::Bars)
}

/// The unit and range an entry of another kind declares in its definition.
pub(crate) fn declared_unit_range(name: &str) -> Option<(IndicatorUnit, OutputRange)> {
    pair_definition(name)
        .map(|d| (d.unit, d.output_range))
        .or_else(|| transform_definition(name).map(|d| (d.unit, d.output_range)))
        .or_else(|| intrabar_definition(name).map(|d| (d.unit, d.output_range)))
}

/// Builds a transformation from its catalog name (or an alias) and an `f64` parameter map,
/// validated like [`build_checked`](super::registry::build_checked).
pub fn build_transform_checked(
    name: &str,
    params: &HashMap<String, f64>,
) -> Result<Box<dyn BarTransform>, RegistryError> {
    transform_definition(name)
        .ok_or_else(|| wrong_input(name, IndicatorInput::Transform))?
        .build_from_f64(name, params)
}

/// Builds an intrabar indicator from its catalog name (or an alias) and an `f64` parameter map,
/// validated like [`build_checked`](super::registry::build_checked).
pub fn build_intrabar_checked(
    name: &str,
    params: &HashMap<String, f64>,
) -> Result<Box<dyn IntrabarIndicator>, RegistryError> {
    intrabar_definition(name)
        .ok_or_else(|| wrong_input(name, IndicatorInput::Intrabar))?
        .build_from_f64(name, params)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::cvd_intrabar::{DeltaAnchor, IntrabarCvd, UnchangedIntrabarPolicy};
    use crate::indicator::registry::{build_checked, catalog, catalog_of, output_range};
    use crate::indicator::unit::output_unit;
    use crate::transform::HeikinAshi;

    fn bar(timestamp: i64, open: f64, close: f64) -> Bar {
        Bar::new(
            timestamp,
            open,
            open.max(close) + 1.0,
            open.min(close) - 1.0,
            close,
            100.0,
        )
    }

    #[test]
    fn test_every_non_bars_entry_has_exactly_one_builder() {
        for entry in catalog().iter().filter(|e| e.input != IndicatorInput::Bars) {
            assert_eq!(input_of(entry.name), Some(entry.input), "{}", entry.name);
            let built = [
                build_pair_name(entry.name),
                build_transform_checked(entry.name, &entry.default_params).is_ok(),
                build_intrabar_checked(entry.name, &entry.default_params).is_ok(),
                build_checked(entry.name, &entry.default_params).is_ok(),
            ];
            assert_eq!(built.iter().filter(|b| **b).count(), 1, "{}", entry.name);
            let right = match entry.input {
                IndicatorInput::Pair => built[0],
                IndicatorInput::Transform => built[1],
                IndicatorInput::Intrabar => built[2],
                IndicatorInput::Bars => unreachable!(),
            };
            assert!(
                right,
                "{} not built by {}",
                entry.name,
                entry.input.builder()
            );
            assert!(declared_unit_range(entry.name).is_some());
            assert_eq!(
                output_unit(entry.name),
                declared_unit_range(entry.name).unwrap().0
            );
            assert_eq!(
                output_range(entry.name),
                declared_unit_range(entry.name).unwrap().1
            );
        }
        assert_eq!(catalog_of(IndicatorInput::Bars).len(), 149);
    }

    fn build_pair_name(name: &str) -> bool {
        super::super::pair::build_pair_checked(name, &HashMap::new()).is_ok()
    }

    #[test]
    fn test_wrong_builder_names_the_right_one() {
        for (name, wanted_builder) in [
            ("heikin_ashi", "build_transform_checked"),
            ("intrabar_cvd", "build_intrabar_checked"),
            ("correlation", "build_pair_checked"),
            ("rsi", "build_checked"),
        ] {
            let errors = [
                build_transform_checked(name, &HashMap::new()).err(),
                build_intrabar_checked(name, &HashMap::new()).err(),
                super::super::pair::build_pair_checked(name, &HashMap::new()).err(),
            ];
            let named: Vec<&RegistryError> = errors.iter().flatten().collect();
            assert!(
                named.iter().any(|e| matches!(e,
                    RegistryError::WrongInput { builder, .. } if builder == wanted_builder)),
                "{name}: {named:?}"
            );
        }
        assert!(matches!(
            build_transform_checked("nope", &HashMap::new()),
            Err(RegistryError::UnknownIndicator(_))
        ));
        assert!(matches!(
            build_intrabar_checked("nope", &HashMap::new()),
            Err(RegistryError::UnknownIndicator(_))
        ));
    }

    #[test]
    fn test_built_heikin_ashi_equals_the_engine() {
        let mut built = build_transform_checked("Heikin_Ashi", &HashMap::new()).unwrap();
        let mut engine = HeikinAshi::new();
        for i in 0..20 {
            let b = bar(i, 100.0 + i as f64, 101.0 + (i % 3) as f64);
            assert_eq!(built.update(&b), engine.update(&b));
        }
        assert!(matches!(
            build_transform_checked("heikin_ashi", &HashMap::from([("len".to_string(), 3.0)])),
            Err(RegistryError::UnknownParameter { .. })
        ));
    }

    #[test]
    fn test_built_intrabar_cvd_equals_the_engine_and_validates() {
        let params = HashMap::from([
            ("anchor".to_string(), 1.0),
            ("anchor_offset_seconds".to_string(), 3600.0),
            ("unchanged_policy".to_string(), 1.0),
        ]);
        let mut built = build_intrabar_checked("intrabar_cvd", &params).unwrap();
        let mut engine = IntrabarCvd::new(
            DeltaAnchor::Daily {
                start_offset_seconds: 3600,
            },
            UnchangedIntrabarPolicy::CarryPreviousDirection,
        );
        for i in 0..6 {
            let group = IntrabarGroup {
                parent_timestamp: i * 14_400,
                children: (0..4)
                    .map(|c| {
                        bar(
                            i * 14_400 + c * 3600,
                            100.0,
                            100.0 + ((c + i) % 3) as f64 - 1.0,
                        )
                    })
                    .collect(),
            };
            assert_eq!(built.on_group(&group), engine.on_group(&group));
        }
        assert_eq!(
            built.on_missing_children(&bar(99, 1.0, 1.0)),
            engine.on_missing_children(&bar(99, 1.0, 1.0))
        );
        for bad in [("anchor", 2.0), ("unchanged_policy", -1.0)] {
            assert!(matches!(
                build_intrabar_checked(
                    "intrabar_cvd",
                    &HashMap::from([(bad.0.to_string(), bad.1)])
                ),
                Err(RegistryError::InvalidParameter { .. } | RegistryError::InvalidEnumValue { .. })
            ));
        }
    }
}
