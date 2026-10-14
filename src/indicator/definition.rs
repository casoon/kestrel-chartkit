//! One definition per indicator (plan 49): name, aliases, description, parameters and
//! construction in one place, from which the registry's lookups are answered.
//!
//! A parameter is described once as a [`ParamSpec`]. Both registry entry points — the `f64` map
//! of [`super::registry::build_checked`] and the typed map of [`super::registry::build_typed`] —
//! resolve their input the same way into [`ResolvedParams`]: unknown keys are
//! rejected, then every given value is checked for its type (and a choice for its option), then
//! for its range, each pass in the order of the specs. The definition's `build` only reads
//! resolved, valid values.
//!
//! The migration is gradual: [`definitions`] lists the indicators already defined here; every
//! other name still goes through the registry's `match`.

use std::collections::HashMap;

use super::params::{ParamValue, TypedParams};
use super::registry::{IndicatorCatalogEntry, RegistryError};
use super::Indicator;
use crate::model::Source;

/// What a parameter accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ParamKind {
    /// A whole number in `min..=max`.
    Count {
        default: usize,
        min: usize,
        max: usize,
    },
    /// A finite number in `min..=max`.
    Number { default: f64, min: f64, max: f64 },
    /// One of `options`, by name in the typed map (case-insensitive). With `numeric`, the `f64`
    /// map selects it by its position in `options` instead; without, the choice exists only in
    /// the typed map.
    Choice {
        default: &'static str,
        options: &'static [&'static str],
        numeric: bool,
    },
    /// A point in time, Unix seconds, typed map only; no default.
    Timestamp,
    /// The price the indicator reads, typed map only. Declaring it means the indicator handles
    /// `source` itself: `build_typed` then skips its generic source handling (the compatibility
    /// checks and the [`SourceMapped`](super::source_mapped::SourceMapped) wrapper).
    Source { default: Source },
}

/// One parameter of an indicator.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ParamSpec {
    pub name: &'static str,
    pub kind: ParamKind,
    /// What the parameter does, for a consumer that shows it.
    pub meaning: &'static str,
    /// The parameter is a threshold on the output's scale (see `registry::threshold_params`).
    pub threshold: bool,
    /// A deprecated name still accepted for this parameter. Given alone it is validated under its
    /// own name; given beside the parameter with a different value it is a conflict.
    pub legacy_alias: Option<&'static str>,
    /// For a number whose valid range depends on a choice: the choice parameter and the range per
    /// option. Options not listed keep the spec's own range.
    pub range_by: Option<RangeBy>,
}

/// Ranges of a number per option of a choice (see [`ParamSpec::range_by`]).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RangeBy {
    pub choice: &'static str,
    pub ranges: &'static [(&'static str, f64, f64)],
}

impl ParamSpec {
    pub const fn count(name: &'static str, default: usize, min: usize, max: usize) -> Self {
        Self {
            name,
            kind: ParamKind::Count { default, min, max },
            meaning: "",
            threshold: false,
            legacy_alias: None,
            range_by: None,
        }
    }

    pub const fn number(name: &'static str, default: f64, min: f64, max: f64) -> Self {
        Self {
            name,
            kind: ParamKind::Number { default, min, max },
            meaning: "",
            threshold: false,
            legacy_alias: None,
            range_by: None,
        }
    }

    pub const fn choice(
        name: &'static str,
        default: &'static str,
        options: &'static [&'static str],
        numeric: bool,
    ) -> Self {
        Self {
            name,
            kind: ParamKind::Choice {
                default,
                options,
                numeric,
            },
            meaning: "",
            threshold: false,
            legacy_alias: None,
            range_by: None,
        }
    }

    pub const fn source(default: Source) -> Self {
        Self {
            name: "source",
            kind: ParamKind::Source { default },
            meaning: "",
            threshold: false,
            legacy_alias: None,
            range_by: None,
        }
    }

    pub const fn timestamp(name: &'static str) -> Self {
        Self {
            name,
            kind: ParamKind::Timestamp,
            meaning: "",
            threshold: false,
            legacy_alias: None,
            range_by: None,
        }
    }

    pub const fn meaning(mut self, meaning: &'static str) -> Self {
        self.meaning = meaning;
        self
    }

    pub const fn threshold(mut self) -> Self {
        self.threshold = true;
        self
    }

    pub const fn range_by(
        mut self,
        choice: &'static str,
        ranges: &'static [(&'static str, f64, f64)],
    ) -> Self {
        self.range_by = Some(RangeBy { choice, ranges });
        self
    }

    pub const fn legacy_alias(mut self, alias: &'static str) -> Self {
        self.legacy_alias = Some(alias);
        self
    }

    /// Whether the `f64` map can carry this parameter.
    pub fn in_f64_map(&self) -> bool {
        match self.kind {
            ParamKind::Count { .. } | ParamKind::Number { .. } => true,
            ParamKind::Choice { numeric, .. } => numeric,
            ParamKind::Timestamp | ParamKind::Source { .. } => false,
        }
    }

    /// The default as the `f64` map encodes it, if the map can carry the parameter.
    pub fn f64_default(&self) -> Option<f64> {
        match self.kind {
            ParamKind::Count { default, .. } => Some(default as f64),
            ParamKind::Number { default, .. } => Some(default),
            ParamKind::Choice {
                default,
                options,
                numeric: true,
            } => options.iter().position(|o| *o == default).map(|i| i as f64),
            ParamKind::Choice { .. } | ParamKind::Timestamp | ParamKind::Source { .. } => None,
        }
    }
}

/// A relation between two parameters, checked after every parameter is in range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Check {
    /// `lower` must be strictly smaller than `upper`.
    Less {
        lower: &'static str,
        upper: &'static str,
    },
}

/// Everything the registry knows about one indicator.
pub struct IndicatorDefinition {
    pub name: &'static str,
    pub aliases: &'static [&'static str],
    pub description: &'static str,
    pub params: &'static [ParamSpec],
    pub checks: &'static [Check],
    pub build: fn(&ResolvedParams) -> Result<Box<dyn Indicator>, RegistryError>,
}

impl IndicatorDefinition {
    /// The catalog entry: name, description and the defaults of every parameter the `f64` map
    /// can carry.
    pub fn catalog_entry(&self) -> IndicatorCatalogEntry {
        IndicatorCatalogEntry {
            name: self.name,
            description: self.description,
            default_params: self
                .params
                .iter()
                .filter_map(|p| p.f64_default().map(|d| (p.name.to_string(), d)))
                .collect(),
        }
    }

    /// The parameters that are thresholds on the output's scale, from the lowest default to the
    /// highest — independent of the spec order, which follows the order of validation.
    pub fn threshold_params(&self) -> Vec<&'static str> {
        let mut thresholds: Vec<&ParamSpec> = self.params.iter().filter(|p| p.threshold).collect();
        thresholds.sort_by(|a, b| {
            a.f64_default()
                .partial_cmp(&b.f64_default())
                .expect("finite defaults")
        });
        thresholds.into_iter().map(|p| p.name).collect()
    }

    /// Rejects the first key (in sorted order, so the error is reproducible) that is not one of
    /// the parameters; without `typed`, also the parameters the `f64` map cannot carry.
    /// `requested` is the name the caller used (an alias stays an alias in the error).
    pub fn reject_unknown<'a>(
        &self,
        requested: &str,
        keys: impl IntoIterator<Item = &'a String>,
        typed: bool,
    ) -> Result<(), RegistryError> {
        let accepted = |key: &str| {
            self.params.iter().any(|p| {
                (p.name == key || p.legacy_alias == Some(key)) && (typed || p.in_f64_map())
            })
        };
        let mut keys: Vec<&String> = keys.into_iter().collect();
        keys.sort();
        match keys.into_iter().find(|k| !accepted(k)) {
            Some(key) => Err(RegistryError::UnknownParameter {
                parameter: key.clone(),
                indicator: requested.to_string(),
            }),
            None => Ok(()),
        }
    }

    /// Whether the indicator reads `source` itself (see [`ParamKind::Source`]).
    pub fn reads_own_source(&self) -> bool {
        self.params
            .iter()
            .any(|p| matches!(p.kind, ParamKind::Source { .. }))
    }

    pub fn build_from_f64(
        &self,
        requested: &str,
        params: &HashMap<String, f64>,
    ) -> Result<Box<dyn Indicator>, RegistryError> {
        let input: TypedParams = params
            .iter()
            .map(|(k, v)| (k.clone(), ParamValue::Float(*v)))
            .collect();
        (self.build)(&resolve(self, requested, &input, Input::F64Map, None)?)
    }

    /// `source` is the price requested through `build_typed`'s generic source handling, for an
    /// indicator that needs to know it beside the wrapper (it is `None` for one that reads its
    /// own `source` parameter).
    pub fn build_from_typed(
        &self,
        requested: &str,
        params: &TypedParams,
        source: Option<Source>,
    ) -> Result<Box<dyn Indicator>, RegistryError> {
        (self.build)(&resolve(self, requested, params, Input::Typed, source)?)
    }
}

/// A resolved parameter value.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Resolved {
    Count(usize),
    Number(f64),
    Choice(&'static str),
    Timestamp(Option<i64>),
    Source(Source),
}

/// The parameters of one build, each validated against its spec and defaulted where absent.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedParams {
    values: HashMap<&'static str, Resolved>,
    requested_source: Option<Source>,
}

impl ResolvedParams {
    /// The whole-number parameter `name`. Panics if the definition has no such count — a
    /// definition reading a parameter it does not declare is a programming error.
    pub fn count(&self, name: &str) -> usize {
        match self.values.get(name) {
            Some(Resolved::Count(v)) => *v,
            other => panic!("parameter {name} is not a declared count: {other:?}"),
        }
    }

    pub fn number(&self, name: &str) -> f64 {
        match self.values.get(name) {
            Some(Resolved::Number(v)) => *v,
            other => panic!("parameter {name} is not a declared number: {other:?}"),
        }
    }

    /// The chosen option of `name`, one of the spec's option names.
    pub fn choice(&self, name: &str) -> &'static str {
        match self.values.get(name) {
            Some(Resolved::Choice(v)) => v,
            other => panic!("parameter {name} is not a declared choice: {other:?}"),
        }
    }

    /// A count or number as `f64`, for the relation checks.
    fn scalar(&self, name: &str) -> f64 {
        match self.values.get(name) {
            Some(Resolved::Count(v)) => *v as f64,
            Some(Resolved::Number(v)) => *v,
            other => panic!("check on {name}, which is not a count or number: {other:?}"),
        }
    }

    /// The price of an indicator that declares [`ParamKind::Source`].
    pub fn source(&self) -> Source {
        match self.values.get("source") {
            Some(Resolved::Source(v)) => *v,
            other => panic!("source is not declared: {other:?}"),
        }
    }

    /// The price requested through `build_typed`'s generic source handling, if any.
    pub fn requested_source(&self) -> Option<Source> {
        self.requested_source
    }

    pub fn timestamp(&self, name: &str) -> Option<i64> {
        match self.values.get(name) {
            Some(Resolved::Timestamp(v)) => *v,
            other => panic!("parameter {name} is not a declared timestamp: {other:?}"),
        }
    }
}

/// Which registry entry point the values came through.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Input {
    F64Map,
    Typed,
}

/// Resolves `params` against `definition`: unknown keys, then types and choices, then ranges.
fn resolve(
    definition: &IndicatorDefinition,
    requested: &str,
    params: &TypedParams,
    input: Input,
    requested_source: Option<Source>,
) -> Result<ResolvedParams, RegistryError> {
    definition.reject_unknown(requested, params.keys(), input == Input::Typed)?;

    // Pass 1: types and choices.
    let mut values = HashMap::new();
    let mut numbers: Vec<Queued> = Vec::new();
    for spec in definition.params {
        let given = params.get(spec.name);
        let alias = spec
            .legacy_alias
            .and_then(|a| params.get(a).map(|value| (a, value)));
        if let (ParamKind::Count { .. } | ParamKind::Number { .. }, Some((alias, value))) =
            (spec.kind, alias)
        {
            let alias_value = scalar(alias, value)?;
            numbers.push(match given {
                None => Queued::new(spec, alias, alias_value),
                Some(given) => {
                    let value = scalar(spec.name, given)?;
                    let mut queued = Queued::new(spec, spec.name, value);
                    if value != alias_value {
                        queued.conflict = Some((alias, alias_value));
                    }
                    queued
                }
            });
            continue;
        }
        let resolved = match (spec.kind, given) {
            (ParamKind::Count { default, .. }, None) => Resolved::Count(default),
            (ParamKind::Number { default, .. }, None) => Resolved::Number(default),
            (ParamKind::Choice { default, .. }, None) => Resolved::Choice(default),
            (ParamKind::Timestamp, None) => Resolved::Timestamp(None),
            (ParamKind::Source { default }, None) => Resolved::Source(default),
            (ParamKind::Source { .. }, Some(ParamValue::Source(source))) => {
                Resolved::Source(*source)
            }
            (ParamKind::Source { .. }, Some(other)) => return Err(unsupported(spec.name, other)),
            (ParamKind::Count { .. } | ParamKind::Number { .. }, Some(value)) => {
                numbers.push(Queued::new(spec, spec.name, scalar(spec.name, value)?));
                continue;
            }
            (ParamKind::Choice { options, .. }, Some(value)) => match (input, value) {
                (Input::F64Map, value) => {
                    let code = scalar(spec.name, value)?;
                    let last = options.len() - 1;
                    if !code.is_finite() || code.fract() != 0.0 || code < 0.0 || code > last as f64
                    {
                        return Err(whole_number_error(spec.name, code, 0, last));
                    }
                    Resolved::Choice(options[code as usize])
                }
                (Input::Typed, ParamValue::Enum(name)) => {
                    let name = name.to_lowercase();
                    match options.iter().find(|o| **o == name) {
                        Some(option) => Resolved::Choice(option),
                        None => {
                            return Err(RegistryError::InvalidEnumValue {
                                parameter: spec.name.to_string(),
                                value: name,
                                reason: format!("expected one of {}", options.join("|")),
                            })
                        }
                    }
                }
                (Input::Typed, other) => return Err(unsupported(spec.name, other)),
            },
            (ParamKind::Timestamp, Some(ParamValue::Timestamp(ts))) => {
                Resolved::Timestamp(Some(*ts))
            }
            (ParamKind::Timestamp, Some(other)) => return Err(unsupported(spec.name, other)),
        };
        values.insert(spec.name, resolved);
    }

    // Pass 2: ranges.
    for Queued {
        spec,
        given_as,
        value,
        conflict,
    } in numbers
    {
        if let Some((alias, alias_value)) = conflict {
            return Err(RegistryError::InvalidParameter {
                parameter: spec.name.to_string(),
                value,
                reason: format!(
                    "conflicting values for '{}' ({value}) and legacy alias '{alias}' \
                     ({alias_value}); set only one",
                    spec.name
                ),
            });
        }
        let resolved = match spec.kind {
            ParamKind::Count { min, max, .. } => {
                if !value.is_finite()
                    || value.fract() != 0.0
                    || value < min as f64
                    || value > max as f64
                {
                    return Err(whole_number_error(given_as, value, min, max));
                }
                Resolved::Count(value as usize)
            }
            ParamKind::Number { min, max, .. } => {
                let (min, max) = spec
                    .range_by
                    .and_then(|by| {
                        let chosen = match values.get(by.choice) {
                            Some(Resolved::Choice(option)) => *option,
                            other => {
                                panic!("range_by names {}, not a choice: {other:?}", by.choice)
                            }
                        };
                        by.ranges
                            .iter()
                            .find(|(option, _, _)| *option == chosen)
                            .map(|(_, min, max)| (*min, *max))
                    })
                    .unwrap_or((min, max));
                if !value.is_finite() || value < min || value > max {
                    return Err(RegistryError::InvalidParameter {
                        parameter: given_as.to_string(),
                        value,
                        reason: format!(
                            "Value must be a finite number between {} and {}",
                            min, max
                        ),
                    });
                }
                Resolved::Number(value)
            }
            ParamKind::Choice { .. } | ParamKind::Timestamp | ParamKind::Source { .. } => {
                unreachable!("only numbers queued")
            }
        };
        values.insert(spec.name, resolved);
    }

    // Pass 3: relations.
    let resolved = ResolvedParams {
        values,
        requested_source,
    };
    for check in definition.checks {
        match *check {
            Check::Less { lower, upper } => {
                let (low, high) = (resolved.scalar(lower), resolved.scalar(upper));
                if low >= high {
                    return Err(RegistryError::InvalidParameter {
                        parameter: lower.to_string(),
                        value: low,
                        reason: format!("{lower} must be smaller than {upper}"),
                    });
                }
            }
        }
    }
    Ok(resolved)
}

/// A numeric value waiting for its range check, with the name it was given under.
struct Queued<'a> {
    spec: &'a ParamSpec,
    given_as: &'static str,
    value: f64,
    /// The legacy alias given beside it with a different value.
    conflict: Option<(&'static str, f64)>,
}

impl<'a> Queued<'a> {
    fn new(spec: &'a ParamSpec, given_as: &'static str, value: f64) -> Self {
        Self {
            spec,
            given_as,
            value,
            conflict: None,
        }
    }
}

/// The scalar form of a numeric parameter; values without one (enum, text …) are a type error.
fn scalar(name: &str, value: &ParamValue) -> Result<f64, RegistryError> {
    value.as_f64().ok_or_else(|| unsupported(name, value))
}

fn unsupported(name: &str, value: &ParamValue) -> RegistryError {
    RegistryError::UnsupportedParameterType {
        parameter: name.to_string(),
        type_name: value.type_name().to_string(),
    }
}

fn whole_number_error(name: &str, value: f64, min: usize, max: usize) -> RegistryError {
    RegistryError::InvalidParameter {
        parameter: name.to_string(),
        value,
        reason: format!(
            "Value must be a whole, finite number between {} and {}",
            min, max
        ),
    }
}

mod composites_a;
mod composites_b;
mod moving_averages;
mod oscillators;
mod pilot;
mod structure;
mod volatility;
mod volume;

/// The indicators already defined, one list per family; the registry routes them through their
/// definition. Each definition lives beside its implementation.
const FAMILIES: &[&[&IndicatorDefinition]] = &[
    pilot::DEFINITIONS,
    moving_averages::DEFINITIONS,
    oscillators::DEFINITIONS,
    volatility::DEFINITIONS,
    volume::DEFINITIONS,
    structure::DEFINITIONS,
    composites_a::DEFINITIONS,
    composites_b::DEFINITIONS,
];

/// Every indicator definition.
pub fn definitions() -> impl Iterator<Item = &'static IndicatorDefinition> {
    FAMILIES.iter().flat_map(|family| family.iter().copied())
}

/// The definition of `name` (canonical name or alias, case-insensitive), if it has one yet.
pub fn definition(name: &str) -> Option<&'static IndicatorDefinition> {
    let lower = name.to_lowercase();
    definitions().find(|d| d.name == lower || d.aliases.contains(&lower.as_str()))
}

#[cfg(test)]
mod tests {
    //! Difference test (plan 49, step 2): for every defined indicator, the definition and the
    //! registry's legacy `match` give the same result — the same error, or indicators that agree
    //! on name, warmup and every output and alert over a reference series.

    use super::*;
    use crate::indicator::registry::{
        build_checked_with, build_typed_with, canonical_name, catalog, threshold_params, Route,
    };
    use crate::indicator::settle::REFERENCE_SHAPES;
    use crate::model::Source;

    /// Debug strings throughout: they compare bit for bit, NaN included (in values and errors).
    fn fingerprint(
        result: Result<Box<dyn Indicator>, RegistryError>,
    ) -> Result<Vec<String>, String> {
        let mut indicator = result.map_err(|e| format!("{e:?}"))?;
        let mut out = vec![
            indicator.name().to_string(),
            indicator.warmup_period().to_string(),
        ];
        for bar in REFERENCE_SHAPES[0].series(900).iter().take(400) {
            let output = indicator.on_bar(bar).map(|o| {
                // `extra` sorted: a HashMap's Debug order differs between instances.
                let extra: std::collections::BTreeMap<_, _> = o
                    .extra
                    .iter()
                    .map(|(k, v)| (k.clone(), format!("{v:?}")))
                    .collect();
                format!(
                    "{:?} {:?} {:?} {extra:?} {:?} {:?} {:?}",
                    o.value, o.secondary, o.signal, o.state, o.reason, o.artifacts
                )
            });
            out.push(format!("{output:?}"));
            out.push(format!("{:?}", indicator.alerts()));
        }
        Ok(out)
    }

    /// Valid and invalid values of one spec as the `f64` map carries them.
    fn f64_values(spec: &ParamSpec) -> Vec<f64> {
        let mut values = vec![f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
        match spec.kind {
            ParamKind::Count { default, min, max } => values.extend([
                min as f64,
                max as f64,
                default as f64,
                default as f64 + 1.0,
                default as f64 + 0.5,
                min as f64 - 1.0,
                max as f64 + 1.0,
            ]),
            ParamKind::Number { default, min, max } => {
                values.extend([min, max, default, (min + max) / 2.0, min - 0.001, max + 1.0])
            }
            ParamKind::Choice { options, .. } => {
                values.extend((0..=options.len()).map(|i| i as f64));
                values.extend([-1.0, 0.5]);
            }
            ParamKind::Timestamp | ParamKind::Source { .. } => {}
        }
        values
    }

    /// Typed values of one spec beyond the numeric ones.
    fn typed_values(spec: &ParamSpec) -> Vec<ParamValue> {
        let mut values = vec![
            ParamValue::Enum("nope".into()),
            ParamValue::Text("x".into()),
            ParamValue::Bool(true),
        ];
        match spec.kind {
            ParamKind::Count { default, .. } => {
                values.push(ParamValue::Int(default as i64));
                values.push(ParamValue::Int(0));
            }
            ParamKind::Number { default, .. } => values.push(ParamValue::Float(default)),
            ParamKind::Choice { options, .. } => {
                for option in options {
                    values.push(ParamValue::Enum(option.to_string()));
                    values.push(ParamValue::Enum(option.to_uppercase()));
                }
                values.push(ParamValue::Int(1));
            }
            ParamKind::Timestamp | ParamKind::Source { .. } => {}
        }
        values
    }

    fn f64_cases(definition: &IndicatorDefinition) -> Vec<HashMap<String, f64>> {
        let numeric: Vec<&ParamSpec> = definition
            .params
            .iter()
            .filter(|p| p.in_f64_map())
            .collect();
        let mut cases = vec![HashMap::new(), HashMap::from([("peroid".to_string(), 2.0)])];
        for spec in &numeric {
            for value in f64_values(spec) {
                cases.push(HashMap::from([(spec.name.to_string(), value)]));
            }
        }
        // Two invalid values at once: which one is reported must not change.
        for (i, a) in numeric.iter().enumerate() {
            for b in &numeric[i + 1..] {
                cases.push(HashMap::from([
                    (a.name.to_string(), f64::NAN),
                    (b.name.to_string(), f64::NAN),
                ]));
            }
        }
        cases
    }

    fn typed_cases(definition: &IndicatorDefinition) -> Vec<TypedParams> {
        let mut cases: Vec<TypedParams> = f64_cases(definition)
            .into_iter()
            .map(|c| {
                c.into_iter()
                    .map(|(k, v)| (k, ParamValue::Float(v)))
                    .collect()
            })
            .collect();
        for spec in definition.params {
            if spec.kind == ParamKind::Timestamp {
                continue;
            }
            for value in typed_values(spec) {
                cases.push(HashMap::from([(spec.name.to_string(), value)]));
            }
        }
        cases.push(HashMap::from([(
            "source".to_string(),
            ParamValue::Enum("hl2".into()),
        )]));
        for source in [Source::Close, Source::Hl2, Source::Hlc3] {
            cases.push(HashMap::from([(
                "source".to_string(),
                ParamValue::Source(source),
            )]));
        }
        // A timestamp is only meaningful beside the choice that asks for it.
        let manual = definition.params.iter().find(|p| {
            matches!(p.kind, ParamKind::Choice { options, .. } if options.contains(&"manual_timestamp"))
        });
        if let Some(choice) = manual {
            for spec in definition
                .params
                .iter()
                .filter(|p| p.kind == ParamKind::Timestamp)
            {
                let kind = (
                    choice.name.to_string(),
                    ParamValue::Enum("manual_timestamp".into()),
                );
                cases.push(HashMap::from([kind.clone()]));
                cases.push(HashMap::from([
                    kind,
                    (spec.name.to_string(), ParamValue::Timestamp(1_700_000_000)),
                ]));
            }
        }
        cases
    }

    /// Cases where the definition deliberately differs from the legacy path (operator decision
    /// 2026-10-14): `zigzag_advanced` takes the same parameters on both entry points, so the
    /// legacy f64 path's rejection of `deviation_mode`/`deviation_value` and the legacy typed
    /// path's silent neglect of `deviation_pct` are not reproduced.
    fn intended_difference(name: &str, keys: &[&String]) -> bool {
        name == "zigzag_advanced"
            && keys.iter().any(|k| {
                ["deviation_mode", "deviation_value", "deviation_pct"].contains(&k.as_str())
            })
    }

    #[test]
    fn definitions_build_exactly_what_the_legacy_registry_builds() {
        let mut checked = 0;
        for definition in definitions() {
            for name in std::iter::once(definition.name).chain(definition.aliases.iter().copied()) {
                for params in f64_cases(definition) {
                    if intended_difference(name, &params.keys().collect::<Vec<_>>()) {
                        continue;
                    }
                    let new = fingerprint(build_checked_with(name, &params, Route::Definitions));
                    let old = fingerprint(build_checked_with(name, &params, Route::Legacy));
                    assert_eq!(new, old, "{name} f64 {params:?}");
                    checked += 1;
                }
                for params in typed_cases(definition) {
                    if intended_difference(name, &params.keys().collect::<Vec<_>>()) {
                        continue;
                    }
                    let new = fingerprint(build_typed_with(name, &params, Route::Definitions));
                    let old = fingerprint(build_typed_with(name, &params, Route::Legacy));
                    assert_eq!(new, old, "{name} typed {params:?}");
                    checked += 1;
                }
            }
        }
        assert!(checked > 200, "only {checked} combinations");
    }

    #[test]
    fn definitions_agree_with_catalog_thresholds_and_aliases() {
        let catalog = catalog();
        for definition in definitions() {
            let entry = catalog
                .iter()
                .find(|e| e.name == definition.name)
                .unwrap_or_else(|| panic!("{} not in the catalog", definition.name));
            assert_eq!(&definition.catalog_entry(), entry, "{}", definition.name);
            assert_eq!(
                definition.threshold_params(),
                threshold_params(definition.name),
                "{}",
                definition.name
            );
            for alias in definition.aliases {
                assert_eq!(canonical_name(alias), definition.name);
            }
        }
    }

    const PAIR: IndicatorDefinition = IndicatorDefinition {
        name: "pair",
        aliases: &[],
        description: "test definition",
        params: &[
            ParamSpec::count("fast", 3, 1, 100).legacy_alias("quick"),
            ParamSpec::count("slow", 9, 1, 100),
        ],
        checks: &[Check::Less {
            lower: "fast",
            upper: "slow",
        }],
        build: |p| {
            Ok(Box::new(crate::indicator::moving_averages::SmaEngine::new(
                p.count("slow") - p.count("fast"),
            )))
        },
    };

    fn pair(params: &[(&str, f64)]) -> Result<usize, RegistryError> {
        let params = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        PAIR.build_from_f64("pair", &params)
            .map(|i| i.warmup_period())
    }

    #[test]
    fn legacy_alias_and_relation_match_the_registry_helpers() {
        assert_eq!(pair(&[("quick", 4.0)]), Ok(5));
        assert_eq!(pair(&[("fast", 4.0), ("quick", 4.0)]), Ok(5));
        // Same wording as `registry::get_usize_p_aliased` and `ensure_less`.
        assert_eq!(
            pair(&[("fast", 4.0), ("quick", 5.0)]),
            Err(RegistryError::InvalidParameter {
                parameter: "fast".into(),
                value: 4.0,
                reason: "conflicting values for 'fast' (4) and legacy alias 'quick' (5); set \
                         only one"
                    .into(),
            })
        );
        assert!(matches!(
            pair(&[("quick", 0.0)]),
            Err(RegistryError::InvalidParameter { parameter, .. }) if parameter == "quick"
        ));
        assert_eq!(
            pair(&[("fast", 9.0)]),
            Err(RegistryError::InvalidParameter {
                parameter: "fast".into(),
                value: 9.0,
                reason: "fast must be smaller than slow".into(),
            })
        );
        assert!(matches!(
            pair(&[("old", 1.0)]),
            Err(RegistryError::UnknownParameter { parameter, .. }) if parameter == "old"
        ));
    }

    /// The unified `zigzag_advanced`: the same deviation through either entry point and either
    /// name, mode-dependent ranges, and the legacy typed build reproduced for `deviation_value`.
    #[test]
    fn zigzag_advanced_takes_the_same_parameters_on_both_entry_points() {
        let f64_build = |params: &[(&str, f64)]| {
            let params: HashMap<String, f64> =
                params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
            fingerprint(build_checked_with(
                "zigzag_advanced",
                &params,
                Route::Definitions,
            ))
        };
        let typed_build = |params: Vec<(&str, ParamValue)>, route| {
            let params: TypedParams = params
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect();
            fingerprint(build_typed_with("zigzag_advanced", &params, route))
        };
        let atr_typed = || {
            vec![
                ("deviation_mode", ParamValue::Enum("atr_multiple".into())),
                ("deviation_value", ParamValue::Float(2.5)),
            ]
        };
        // The legacy typed path is the reference for the ATR mode.
        let reference = typed_build(atr_typed(), Route::Legacy);
        assert!(reference.is_ok());
        assert_eq!(typed_build(atr_typed(), Route::Definitions), reference);
        assert_eq!(
            typed_build(
                vec![
                    ("deviation_mode", ParamValue::Enum("atr_multiple".into())),
                    ("deviation_pct", ParamValue::Float(2.5)),
                ],
                Route::Definitions
            ),
            reference
        );
        assert_eq!(
            f64_build(&[("deviation_mode", 1.0), ("deviation_pct", 2.5)]),
            reference
        );
        assert_eq!(
            f64_build(&[("deviation_mode", 1.0), ("deviation_value", 2.5)]),
            reference
        );

        // Percent: up to 100; ATR multiples: up to 1000.
        let percent_error = f64_build(&[("deviation_pct", 500.0)]).unwrap_err();
        assert!(
            percent_error.contains("between 0.001 and 100"),
            "{percent_error}"
        );
        assert!(f64_build(&[("deviation_mode", 1.0), ("deviation_pct", 500.0)]).is_ok());
        assert!(f64_build(&[("deviation_mode", 2.0)]).is_err());

        // The typed path no longer ignores deviation_pct.
        assert_ne!(
            typed_build(
                vec![("deviation_pct", ParamValue::Float(5.0))],
                Route::Definitions
            ),
            typed_build(vec![], Route::Definitions)
        );
    }

    #[test]
    fn resolve_reports_types_before_ranges() {
        let atr = &crate::indicator::atr::ATR_DEFINITION;
        let params: TypedParams = HashMap::from([
            ("atr_len".to_string(), ParamValue::Float(0.0)),
            ("sig_len".to_string(), ParamValue::Enum("x".into())),
        ]);
        assert!(matches!(
            atr.build_from_typed("atr", &params, None),
            Err(RegistryError::UnsupportedParameterType { parameter, .. }) if parameter == "sig_len"
        ));
    }
}
