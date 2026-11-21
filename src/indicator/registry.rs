use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::definition::SourceHandling;
pub use super::params::{ParamValue, TypedParams};
use super::Indicator;

/// What an entry of [`catalog`] reads, and so which builder builds it.
///
/// Every entry computes something per bar; what differs is the input of one step. The kind is
/// part of the entry so a consumer that lists the catalog sees all of them and picks the builder
/// by kind instead of by knowing names.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum IndicatorInput {
    /// One bar of one series per step: [`build_checked`] / [`build_typed`], an [`Indicator`].
    Bars,
    /// One bar of a series with the bar of a reference series of the same timestamp:
    /// [`build_pair_checked`](super::pair::build_pair_checked), a
    /// [`PairIndicator`](super::pair::PairIndicator).
    Pair,
    /// One bar in, one derived candle out:
    /// [`build_transform_checked`](super::other_input::build_transform_checked), a
    /// [`BarTransform`](super::other_input::BarTransform).
    Transform,
    /// The lower-timeframe bars of one parent bar per step:
    /// [`build_intrabar_checked`](super::other_input::build_intrabar_checked), an
    /// [`IntrabarIndicator`](super::other_input::IntrabarIndicator).
    Intrabar,
}

impl IndicatorInput {
    /// The builder function of this kind, by name, for error messages and docs.
    pub fn builder(self) -> &'static str {
        match self {
            Self::Bars => "build_checked",
            Self::Pair => "build_pair_checked",
            Self::Transform => "build_transform_checked",
            Self::Intrabar => "build_intrabar_checked",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IndicatorCatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub default_params: HashMap<String, f64>,
    /// What the entry reads per step, and so which builder builds it.
    pub input: IndicatorInput,
    /// Whether the entry has passed its acceptance; from its definition.
    pub verification: super::definition::Verification,
}

/// Everything that computes per bar, each entry from its definition (plan 49 and 141): first the
/// [`IndicatorInput::Bars`] indicators in the order of the family lists in [`super::definition`],
/// then the two-series, transform and intrabar entries of [`super::other_input`]. The
/// [`IndicatorCatalogEntry::input`] says which builder builds an entry; `build_checked` builds
/// `Bars` entries only.
pub fn catalog() -> Vec<IndicatorCatalogEntry> {
    super::definition::definitions()
        .map(|d| d.catalog_entry())
        .chain(super::other_input::other_entries())
        .collect()
}

/// The entries of [`catalog`] of one input kind.
pub fn catalog_of(input: IndicatorInput) -> Vec<IndicatorCatalogEntry> {
    catalog().into_iter().filter(|e| e.input == input).collect()
}

/// Der Wertebereich, in dem die Ausgabe eines Indikators liegt.
///
/// Eine Eigenschaft des Indikators, keine Darstellungsvorliebe: Ein RSI ist per Konstruktion auf
/// `0..=100` begrenzt, ein MACD schwankt unbegrenzt um null. Ein Konsument, der eine Achse
/// auslegt, braucht diese Angabe — ohne sie bildet er die Achse aus den zufälligen Extrema des
/// gerade verwendeten Datensatzes und beschriftet Werte, die nichts bedeuten.
///
/// [`OutputRange::Unbounded`] ist die ehrliche Voreinstellung: Sie sagt, dass über den Bereich
/// hier nichts erklärt wird — nicht, dass er unbegrenzt wäre.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum OutputRange {
    /// Fest begrenzt: Der Wert kann den Bereich konstruktionsbedingt nicht verlassen.
    Bounded { min: f64, max: f64 },
    /// Nach beiden Seiten offen, aber um eine ausgezeichnete Mitte schwankend. Die Mittellinie
    /// trägt die Aussage — ein MACD über null bedeutet etwas anderes als einer darunter.
    Centered { center: f64 },
    /// Nie negativ, nach oben offen: Spannen, Mengen, Verhältnisse.
    NonNegative,
    /// Über den Bereich wird nichts erklärt.
    Unbounded,
}

/// Der Wertebereich der Ausgabe von `name`.
///
/// Deklariert wird nur, was sich aus der Konstruktion des Indikators ergibt, und zwar in dessen
/// Definition ([`IndicatorFacts::output_range`](super::definition::IndicatorFacts)). Der Rest
/// bleibt [`OutputRange::Unbounded`] — eine Deklaration auf Verdacht wäre schlechter als keine,
/// weil ein Konsument sie für bare Münze nimmt. `tests/output_range_invariants.rs` prüft jede
/// Angabe gegen tatsächliche Läufe, damit sie nicht still veralten kann.
pub fn output_range(name: &str) -> OutputRange {
    // A one-series indicator is found by canonical name or alias; an entry of another input
    // kind by the lookup of its own kind.
    super::definition::definition(name)
        .map(|d| d.facts.output_range)
        .or_else(|| super::other_input::declared_unit_range(name).map(|(_, range)| range))
        .unwrap_or(OutputRange::Unbounded)
}

/// Die Parameter von `name`, die eine Schwelle im Wertebereich der Ausgabe bezeichnen.
///
/// Als Parameternamen statt als Zahlen: Die Schwelle folgt damit dem, was der Aufrufer eingestellt
/// hat, statt eine zweite Wahrheit daneben zu führen. Ein Konsument, der die Ausgabe zeichnet,
/// findet so die Linien, ohne die Bedeutung einzelner Parameter kennen zu müssen.
pub fn threshold_params(name: &str) -> &'static [&'static str] {
    use std::sync::OnceLock;
    // Derived once from the definitions, so the answer can stay a `'static` slice.
    static THRESHOLDS: OnceLock<HashMap<&'static str, Vec<&'static str>>> = OnceLock::new();
    THRESHOLDS
        .get_or_init(|| {
            super::definition::definitions()
                .map(|d| (d.name, d.threshold_params()))
                .collect()
        })
        .get(canonical_name(name).as_str())
        .map_or(&[], Vec::as_slice)
}

impl IndicatorCatalogEntry {
    /// Siehe [`output_range`].
    pub fn output_range(&self) -> OutputRange {
        output_range(self.name)
    }

    /// Die Schwellenwerte dieses Eintrags, aufgelöst über seine Voreinstellungen.
    ///
    /// Aufsteigend sortiert, damit ein Zeichner sie ohne weitere Annahme von unten nach oben
    /// abarbeiten kann.
    pub fn thresholds(&self) -> Vec<f64> {
        let mut werte: Vec<f64> = threshold_params(self.name)
            .iter()
            .filter_map(|p| self.default_params.get(*p).copied())
            .filter(|v| v.is_finite())
            .collect();
        werte.sort_by(|a, b| a.partial_cmp(b).expect("filtered to finite values"));
        werte
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RegistryError {
    UnknownIndicator(String),
    InvalidParameter {
        parameter: String,
        value: f64,
        reason: String,
    },
    /// A [`ParamValue`] with no numeric equivalent (`Enum`/`Text`/`Timeframe`/`Source`) was
    /// passed to [`build_typed`] for an indicator whose current parameter surface is `f64`-only.
    UnsupportedParameterType {
        parameter: String,
        type_name: String,
    },
    /// A [`ParamValue::Enum`] string did not match any variant accepted for this parameter.
    InvalidEnumValue {
        parameter: String,
        value: String,
        reason: String,
    },
    /// A parameter is a known, supported type but cannot be applied to this specific indicator
    /// (e.g. a non-`Close` `source` on a range/OHLC-dependent indicator — see
    /// [`super::source_mapped::SourceMapped`]'s doc comment for why).
    IncompatibleParameter {
        parameter: String,
        indicator: String,
        reason: String,
    },
    /// A parameter key the indicator does not read — typically a typo. Rejected instead of
    /// silently building with the default for the parameter that was meant.
    UnknownParameter {
        parameter: String,
        indicator: String,
    },
    /// The name is in the catalog, but its entry reads another input than the builder called:
    /// `build_checked` builds [`IndicatorInput::Bars`] entries only. `builder` names the right one.
    WrongInput {
        indicator: String,
        input: IndicatorInput,
        builder: String,
    },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::UnknownIndicator(name) => write!(f, "Unknown indicator: {}", name),
            RegistryError::InvalidParameter {
                parameter,
                value,
                reason,
            } => write!(
                f,
                "Invalid parameter '{}' (value {}): {}",
                parameter, value, reason
            ),
            RegistryError::UnsupportedParameterType {
                parameter,
                type_name,
            } => write!(
                f,
                "Parameter '{}' has unsupported type '{}' for this indicator",
                parameter, type_name
            ),
            RegistryError::InvalidEnumValue {
                parameter,
                value,
                reason,
            } => write!(
                f,
                "Invalid value '{}' for parameter '{}': {}",
                value, parameter, reason
            ),
            RegistryError::IncompatibleParameter {
                parameter,
                indicator,
                reason,
            } => write!(
                f,
                "Parameter '{}' is not compatible with indicator '{}': {}",
                parameter, indicator, reason
            ),
            RegistryError::UnknownParameter {
                parameter,
                indicator,
            } => write!(
                f,
                "Unknown parameter '{}' for indicator '{}'",
                parameter, indicator
            ),
            RegistryError::WrongInput {
                indicator,
                input,
                builder,
            } => write!(
                f,
                "Indicator '{}' reads {:?} input and is built with {}",
                indicator, input, builder
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Builds an indicator from its catalog name (or an alias) and an `f64` parameter map, validated
/// against its definition: unknown keys, whole numbers, ranges and relations are errors, missing
/// parameters take their defaults.
pub fn build_checked(
    name: &str,
    params: &HashMap<String, f64>,
) -> Result<Box<dyn Indicator>, RegistryError> {
    super::definition::definition(name)
        .ok_or_else(|| super::other_input::not_bars(name))?
        .build_from_f64(name, params)
}

/// Dynamically builds an `Indicator` instance by its catalog name and parameters.
/// Delegates to `build_checked` and discards errors to return `Option`.
pub fn build(name: &str, params: &HashMap<String, f64>) -> Option<Box<dyn Indicator>> {
    build_checked(name, params).ok()
}

/// The canonical registry name for `name`: lower-cased, with a registry alias (`avwap`, `vp`,
/// `wt`, ...) resolved to the name of the indicator it builds. Unknown names come back
/// lower-cased and otherwise unchanged.
///
/// Every lookup of per-indicator metadata (source handling here, data requirements in
/// [`crate::applicability::data_requirements`]) goes through this, so an alias can never escape a
/// rule that is written against the canonical name.
pub fn canonical_name(name: &str) -> String {
    super::definition::definition(name).map_or_else(|| name.to_lowercase(), |d| d.name.to_string())
}

/// Builds an indicator from typed parameters (see [`ParamValue`]), validated against its
/// definition like [`build_checked`]. Numeric values (`Float`, `Int`, `Bool`, `Timestamp`) count
/// as numbers; choices take `Enum` names. `source` maps the bar to another price through
/// [`super::source_mapped::SourceMapped`], unless the indicator reads its own `source`.
pub fn build_typed(name: &str, params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    let definition =
        super::definition::definition(name).ok_or_else(|| super::other_input::not_bars(name))?;
    definition.reject_unknown(name, params.keys().filter(|k| *k != "source"), true)?;

    // An indicator that reads `source` itself bypasses the generic handling below.
    if definition.reads_own_source() {
        return definition.build_from_typed(name, params, None);
    }

    let source = match params.get("source") {
        None => None,
        Some(ParamValue::Source(s)) => Some(*s),
        Some(other) => {
            return Err(RegistryError::UnsupportedParameterType {
                parameter: "source".to_string(),
                type_name: other.type_name().to_string(),
            });
        }
    };

    // How the source is handled is declared in the definition, see [`SourceHandling`].
    let handling = definition.facts.source;
    if let Some(s) = source {
        if s != crate::model::Source::Close {
            match handling {
                SourceHandling::FixedToClose => {
                    return Err(RegistryError::IncompatibleParameter {
                        parameter: "source".to_string(),
                        indicator: name.to_string(),
                        reason: "price source is fixed to the close by contract; another source \
                                 would redefine the series under the same name"
                            .to_string(),
                    });
                }
                SourceHandling::RangeDependent => {
                    return Err(RegistryError::IncompatibleParameter {
                        parameter: "source".to_string(),
                        indicator: name.to_string(),
                        reason: "range/OHLC-dependent indicator; SourceMapped would collapse its \
                                 true range to zero"
                            .to_string(),
                    });
                }
                SourceHandling::Mapped(_) => {}
            }
        }
    }

    // `source` is a cross-cutting concern applied uniformly via `SourceMapped` below, not an
    // indicator-specific parameter, so it is stripped before the definition resolves the rest.
    let mut remaining = params.clone();
    remaining.remove("source");
    let built = definition.build_from_typed(name, &remaining, source)?;

    // Wrapped whenever the requested source differs from what the indicator reads natively —
    // for a close-native indicator that is any non-`Close` source, for CCI (`hlc3`) an explicit
    // `Close` as well. Range-dependent indicators only get here with `Close` (see above), which
    // is their native source, so they are never wrapped.
    Ok(match source {
        Some(s) if s != handling.native() => {
            Box::new(super::source_mapped::SourceMapped::new(built, s))
        }
        _ => built,
    })
}

// Note: `indicator::relative_strength::RelativeStrengthEngine` requires dual-series input
// (`update(own_bar, bench_bar)`) and is built by name through the two-series registry,
// `super::pair::build_pair_checked`. `indicator::swing_structure::SwingStructureEngine` takes the
// raw ATR per bar (`update(bar, atr)`); its catalog entry `kestrel_swing_structure` feeds it a Wilder ATR
// of its own.

/// Every canonical name `build_checked` can construct, in the order of [`catalog`]: the names of
/// the [`IndicatorInput::Bars`] entries, derived from the definitions. Aliases (e.g. "vp", "ob",
/// "wvf") are not listed: they stay buildable but are not separate catalog entries.
pub fn canonical_indicator_names() -> Vec<&'static str> {
    super::definition::definitions().map(|d| d.name).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::adx::Adx;
    use crate::indicator::atr::Atr;
    use std::collections::HashSet;

    /// The canonical names are the `Bars` entries of the catalog, derived from the definitions
    /// (no second list), and each of them builds from its defaults.
    #[test]
    fn test_canonical_names_are_the_bars_entries_and_build() {
        let bars = catalog_of(IndicatorInput::Bars);
        let catalog_names: Vec<&str> = bars.iter().map(|e| e.name).collect();
        assert_eq!(canonical_indicator_names(), catalog_names);
        for entry in &bars {
            assert!(
                build_checked(entry.name, &entry.default_params).is_ok(),
                "{}",
                entry.name
            );
        }
        assert_eq!(bars.len(), 185);
        // Pair (4), transform (1) and intrabar (1) entries on top of the 185 bar indicators.
        assert_eq!(catalog().len(), 191);
        for (input, count) in [
            (IndicatorInput::Pair, 4),
            (IndicatorInput::Transform, 1),
            (IndicatorInput::Intrabar, 1),
        ] {
            assert_eq!(catalog_of(input).len(), count, "{input:?}");
        }
    }

    /// `build_checked` and `build_typed` build `Bars` entries only; the others answer with the
    /// builder to call instead.
    #[test]
    fn test_one_series_builders_name_the_right_builder_for_other_inputs() {
        for entry in catalog().iter().filter(|e| e.input != IndicatorInput::Bars) {
            let expected = RegistryError::WrongInput {
                indicator: entry.name.to_string(),
                input: entry.input,
                builder: entry.input.builder().to_string(),
            };
            assert_eq!(
                build_checked(entry.name, &entry.default_params).err(),
                Some(expected.clone()),
                "{}",
                entry.name
            );
            assert_eq!(
                build_typed(entry.name, &HashMap::new()).err(),
                Some(expected),
                "{}",
                entry.name
            );
        }
        assert!(matches!(
            build_checked("no_such_indicator", &HashMap::new()),
            Err(RegistryError::UnknownIndicator(_))
        ));
    }

    #[test]
    fn test_catalog_has_no_duplicate_names() {
        let names: Vec<&str> = catalog().iter().map(|e| e.name).collect();
        let unique: HashSet<&str> = names.iter().copied().collect();
        assert_eq!(
            names.len(),
            unique.len(),
            "catalog() contains a duplicate indicator name"
        );
    }

    #[test]
    fn test_every_catalog_entry_builds_with_its_default_params() {
        for entry in catalog_of(IndicatorInput::Bars) {
            let built = build_checked(entry.name, &entry.default_params);
            assert!(
                built.is_ok(),
                "catalog entry '{}' failed to build with its own default params: {:?}",
                entry.name,
                built.err()
            );
        }
    }

    /// Negative test: a name that is only meant to be an alias must not have snuck in as a
    /// second, separate catalog entry alongside its canonical name.
    #[test]
    fn test_catalog_does_not_contain_aliases() {
        let catalog_names: HashSet<&str> = catalog().iter().map(|e| e.name).collect();
        for alias in [
            "vp",
            "wvf",
            "ob",
            "fvg",
            "smc",
            "bos",
            "choch",
            "pivots",
            "pinbar",
            "leg_efficiency",
            "er",
            "vp_extended",
            "vp_persistent",
        ] {
            assert!(
                !catalog_names.contains(alias),
                "'{alias}' is an alias, not a canonical name, and must not be its own catalog entry"
            );
        }
    }

    #[test]
    fn test_build_checked_valid_and_invalid_params() {
        let valid_params = HashMap::from([("rsi_len".to_string(), 14.0)]);
        assert!(build_checked("rsi", &valid_params).is_ok());

        // Period 0 is invalid
        let zero_params = HashMap::from([("rsi_len".to_string(), 0.0)]);
        let err = match build_checked("rsi", &zero_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for zero period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Negative period is invalid
        let neg_params = HashMap::from([("period".to_string(), -5.0)]);
        let err = match build_checked("sma", &neg_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for negative period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Fractional periods are rejected instead of silently truncated
        let fractional_params = HashMap::from([("period".to_string(), 14.5)]);
        assert!(matches!(
            build_checked("sma", &fractional_params),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // Values above the allocation guard are rejected
        let huge_params = HashMap::from([("period".to_string(), 10_001.0)]);
        assert!(matches!(
            build_checked("sma", &huge_params),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // NaN is invalid
        let nan_params = HashMap::from([("period".to_string(), f64::NAN)]);
        let err = match build_checked("sma", &nan_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for NaN period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Threshold and period relationships are validated
        let reversed_rsi = HashMap::from([
            ("oversold".to_string(), 80.0),
            ("overbought".to_string(), 20.0),
        ]);
        assert!(matches!(
            build_checked("rsi", &reversed_rsi),
            Err(RegistryError::InvalidParameter { .. })
        ));

        let reversed_ultimate = HashMap::from([
            ("period1".to_string(), 28.0),
            ("period2".to_string(), 14.0),
            ("period3".to_string(), 7.0),
        ]);
        assert!(matches!(
            build_checked("ultimate_oscillator", &reversed_ultimate),
            Err(RegistryError::InvalidParameter { .. })
        ));

        let oversized_bins = HashMap::from([("num_bins".to_string(), 1_001.0)]);
        assert!(matches!(
            build_checked("volume_profile", &oversized_bins),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // Infinity is invalid
        let inf_params = HashMap::from([("period".to_string(), f64::INFINITY)]);
        let err = match build_checked("sma", &inf_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for Infinity period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // MACD fast_len >= slow_len is invalid
        let macd_bad = HashMap::from([
            ("fast_len".to_string(), 30.0),
            ("slow_len".to_string(), 20.0),
        ]);
        let err = match build_checked("macd", &macd_bad) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for fast >= slow"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Unknown indicator
        let err = match build_checked("non_existent_ind", &HashMap::new()) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown indicator"),
        };
        assert_eq!(
            err,
            RegistryError::UnknownIndicator("non_existent_ind".to_string())
        );
    }

    /// Keltner basis over a short rising series; the EMA length is the only thing that moves it.
    fn keltner_basis(params: &HashMap<String, f64>) -> Vec<f64> {
        let mut ind = build_checked("keltner", params).unwrap();
        (0..10)
            .filter_map(|i| {
                let close = 100.0 + i as f64;
                ind.on_bar(&crate::model::Bar::new(
                    i,
                    close,
                    close + 1.0,
                    close - 1.0,
                    close,
                    1.0,
                ))
            })
            .map(|o| o.value)
            .collect()
    }

    #[test]
    fn test_keltner_ema_period_is_the_canonical_catalog_key() {
        // Finding 05: the catalog publishes "ema_period"; the builder must actually read it
        // rather than silently ignoring it and falling back to the period-20 default.
        // `warmup_period` follows `atr_period` alone, so the basis shows what ema_period drove.
        let ind = keltner_basis(&HashMap::from([
            ("ema_period".to_string(), 5.0),
            ("atr_period".to_string(), 1.0),
        ]));
        let explicit_default = keltner_basis(&HashMap::from([
            ("ema_period".to_string(), 20.0),
            ("atr_period".to_string(), 1.0),
        ]));
        let default_ind = keltner_basis(&HashMap::from([("atr_period".to_string(), 1.0)]));
        assert_ne!(ind, default_ind);
        assert_eq!(default_ind, explicit_default);
    }

    #[test]
    fn test_keltner_ma_period_legacy_alias_still_works() {
        let alias = keltner_basis(&HashMap::from([
            ("ma_period".to_string(), 5.0),
            ("atr_period".to_string(), 1.0),
        ]));
        let canonical = keltner_basis(&HashMap::from([
            ("ema_period".to_string(), 5.0),
            ("atr_period".to_string(), 1.0),
        ]));
        assert_eq!(alias, canonical);
    }

    #[test]
    fn test_keltner_ema_period_invalid_value_rejected() {
        let err = match build_checked("keltner", &HashMap::from([("ema_period".to_string(), 0.0)]))
        {
            Err(e) => e,
            Ok(_) => panic!("Expected error for zero ema_period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));
    }

    #[test]
    fn test_keltner_conflicting_canonical_and_alias_rejected() {
        let err = match build_checked(
            "keltner",
            &HashMap::from([
                ("ema_period".to_string(), 5.0),
                ("ma_period".to_string(), 10.0),
            ]),
        ) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for conflicting ema_period/ma_period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Identical values under both names is not a conflict.
        assert!(build_checked(
            "keltner",
            &HashMap::from([
                ("ema_period".to_string(), 5.0),
                ("ma_period".to_string(), 5.0),
            ]),
        )
        .is_ok());
    }

    /// Plan 62/49: a typo must not silently build with the default — `sma` with `peroid` used to
    /// build an SMA(20) instead of the SMA(2) that was meant.
    #[test]
    fn test_unknown_parameter_is_rejected() {
        let typo = HashMap::from([("peroid".to_string(), 2.0)]);
        let err = match build_checked("sma", &typo) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown parameter"),
        };
        assert_eq!(
            err,
            RegistryError::UnknownParameter {
                parameter: "peroid".to_string(),
                indicator: "sma".to_string(),
            }
        );
        assert!(build_checked("sma", &HashMap::from([("period".to_string(), 2.0)])).is_ok());

        let typed_typo: TypedParams = HashMap::from([("peroid".to_string(), ParamValue::Int(2))]);
        assert!(matches!(
            build_typed("sma", &typed_typo),
            Err(RegistryError::UnknownParameter { .. })
        ));

        // Aliases resolve to the canonical entry's parameters.
        assert!(build_checked("bb", &HashMap::from([("len".to_string(), 20.0)])).is_ok());
        assert!(matches!(
            build_checked("bb", &HashMap::from([("length".to_string(), 20.0)])),
            Err(RegistryError::UnknownParameter { .. })
        ));
    }

    #[test]
    fn test_typed_only_and_legacy_params_are_accepted_where_they_are_read() {
        let smoothing: TypedParams =
            HashMap::from([("smoothing".to_string(), ParamValue::Enum("ema".to_string()))]);
        assert!(build_typed("rsi", &smoothing).is_ok());
        // `build_checked` does not read an RSI smoothing; accepting it there would ignore it.
        assert!(matches!(
            build_checked("rsi", &HashMap::from([("smoothing".to_string(), 1.0)])),
            Err(RegistryError::UnknownParameter { .. })
        ));

        let source: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Hl2),
        )]);
        assert!(build_typed("sma", &source).is_ok());

        assert!(
            build_checked("keltner", &HashMap::from([("ma_period".to_string(), 20.0)])).is_ok()
        );
    }

    #[test]
    fn test_build_typed_flattens_numeric_params() {
        let params: TypedParams = HashMap::from([("rsi_len".to_string(), ParamValue::Int(14))]);
        assert!(build_typed("rsi", &params).is_ok());

        let bool_params: TypedParams =
            HashMap::from([("rsi_len".to_string(), ParamValue::Bool(true))]);
        // Bool flattens to 1.0, which build_checked's own validation then accepts or rejects.
        assert!(build_typed("rsi", &bool_params).is_ok());
    }

    #[test]
    fn test_build_typed_rejects_non_numeric_params() {
        let params: TypedParams =
            HashMap::from([("rsi_len".to_string(), ParamValue::Enum("fast".to_string()))]);
        let err = match build_typed("rsi", &params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for enum parameter"),
        };
        assert!(matches!(
            err,
            RegistryError::UnsupportedParameterType { .. }
        ));
    }

    #[test]
    fn test_build_typed_propagates_indicator_validation_errors() {
        let params: TypedParams =
            HashMap::from([("period".to_string(), ParamValue::Float(f64::INFINITY))]);
        let err = match build_typed("sma", &params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for infinite period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));
    }

    #[test]
    fn test_build_typed_anchored_vwap_native_enum_selection() {
        // Default (no anchor_kind given) stays Session, matching build_checked's behavior.
        let defaulted = build_typed("anchored_vwap", &TypedParams::new());
        assert!(defaulted.is_ok());

        let day_params: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("day".to_string()),
        )]);
        assert!(build_typed("avwap", &day_params).is_ok());

        let skip_zero_vol: TypedParams = HashMap::from([(
            "zero_volume_policy".to_string(),
            ParamValue::Enum("skip".to_string()),
        )]);
        assert!(build_typed("anchored_vwap", &skip_zero_vol).is_ok());

        let manual_without_timestamp: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("manual_timestamp".to_string()),
        )]);
        let err = match build_typed("anchored_vwap", &manual_without_timestamp) {
            Err(e) => e,
            Ok(_) => panic!("Expected error: manual_timestamp requires anchor_timestamp"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));

        let manual_with_timestamp: TypedParams = HashMap::from([
            (
                "anchor_kind".to_string(),
                ParamValue::Enum("manual_timestamp".to_string()),
            ),
            (
                "anchor_timestamp".to_string(),
                ParamValue::Timestamp(1_700_000_000),
            ),
        ]);
        assert!(build_typed("anchored_vwap", &manual_with_timestamp).is_ok());

        let unknown_kind: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("anchored_vwap", &unknown_kind) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown anchor_kind"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_typed_pivot_sets_native_enum_selection() {
        for kind in [
            "classic",
            "fibonacci",
            "camarilla",
            "woodie",
            "demark",
            "cpr",
        ] {
            let params: TypedParams =
                HashMap::from([("pivot_type".to_string(), ParamValue::Enum(kind.to_string()))]);
            assert!(
                build_typed("pivot_sets", &params).is_ok(),
                "expected {kind} to build"
            );
        }

        let unknown: TypedParams = HashMap::from([(
            "pivot_type".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("multi_pivots", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown pivot_type"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_typed_source_propagates_to_computation() {
        use crate::model::{Bar, Source};

        let bars = [
            Bar::new(0, 10.0, 12.0, 8.0, 11.0, 100.0),
            Bar::new(60, 20.0, 22.0, 18.0, 21.0, 100.0),
        ];

        let close_params: TypedParams = HashMap::from([("period".to_string(), ParamValue::Int(2))]);
        let mut close_sma = build_typed("sma", &close_params).unwrap();

        let open_params: TypedParams = HashMap::from([
            ("period".to_string(), ParamValue::Int(2)),
            ("source".to_string(), ParamValue::Source(Source::Open)),
        ]);
        let mut open_sma = build_typed("sma", &open_params).unwrap();

        let mut close_out = None;
        let mut open_out = None;
        for bar in &bars {
            close_out = close_sma.on_bar(bar);
            open_out = open_sma.on_bar(bar);
        }

        assert_eq!(close_out.unwrap().value, (11.0 + 21.0) / 2.0);
        assert_eq!(open_out.unwrap().value, (10.0 + 20.0) / 2.0);
    }

    #[test]
    fn test_build_typed_source_close_is_a_no_op() {
        let params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Close),
        )]);
        assert!(build_typed("sma", &params).is_ok());
    }

    #[test]
    fn test_adx_with_defaults_matches_registry_default() {
        let mut via_struct = Adx::with_defaults();
        let mut via_registry = build_checked("adx", &HashMap::new()).unwrap();

        let bars = crate::model::Bar::new(0, 100.0, 101.0, 99.0, 100.5, 1000.0);
        let mut struct_out = None;
        let mut registry_out = None;
        for i in 0..60 {
            let price = 100.0 + (i as f64 * 0.3).sin() * 5.0;
            let bar =
                crate::model::Bar::new(i, price, price + 1.0, price - 1.0, price + 0.5, 1000.0);
            struct_out = via_struct.on_bar(&bar);
            registry_out = via_registry.on_bar(&bar);
        }
        let _ = bars;
        assert_eq!(
            struct_out.map(|o| o.value),
            registry_out.map(|o| o.value),
            "Adx::with_defaults() must produce identical output to the registry's \"adx\" default"
        );
    }

    #[test]
    fn test_atr_with_defaults_matches_registry_default() {
        let mut via_struct = Atr::with_defaults();
        let mut via_registry = build_checked("atr", &HashMap::new()).unwrap();

        let mut struct_out = None;
        let mut registry_out = None;
        for i in 0..40 {
            let price = 100.0 + (i as f64 * 0.3).sin() * 5.0;
            let bar =
                crate::model::Bar::new(i, price, price + 1.0, price - 1.0, price + 0.5, 1000.0);
            struct_out = via_struct.on_bar(&bar);
            registry_out = via_registry.on_bar(&bar);
        }
        assert_eq!(
            struct_out.map(|o| o.value),
            registry_out.map(|o| o.value),
            "Atr::with_defaults() must produce identical output to the registry's \"atr\" default"
        );
    }

    /// `efi` weighs a close-to-close change by the bar's volume; a derived price source would be
    /// a different series under the same name, so it is refused until that contract exists.
    #[test]
    fn test_build_typed_rejects_non_close_source_on_efi() {
        let params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Hlc3),
        )]);
        let err = match build_typed("efi", &params) {
            Err(e) => e,
            Ok(_) => panic!("expected 'efi' to reject a non-Close source"),
        };
        assert!(
            matches!(err, RegistryError::IncompatibleParameter { .. }),
            "efi returned {err:?} instead of IncompatibleParameter"
        );

        let close: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Close),
        )]);
        assert!(build_typed("efi", &close).is_ok());
    }

    #[test]
    fn test_build_typed_rejects_non_close_source_on_range_dependent_indicators() {
        let params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Open),
        )]);
        for name in [
            "atr",
            "wyckoff",
            "chandelier_exit",
            "ce",
            "extended_volume_profile",
            "zigzag_advanced",
            "keltner",
            "donchian",
            "garman_klass",
        ] {
            let err = match build_typed(name, &params) {
                Err(e) => e,
                Ok(_) => panic!("expected '{name}' to reject a non-Close source"),
            };
            assert!(
                matches!(err, RegistryError::IncompatibleParameter { .. }),
                "'{name}' returned {err:?} instead of IncompatibleParameter"
            );
        }

        // Close is always a no-op regardless of range-dependence, so it must still succeed.
        let close_params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Close),
        )]);
        assert!(build_typed("atr", &close_params).is_ok());
    }

    fn source_params(source: crate::model::Source) -> TypedParams {
        HashMap::from([("source".to_string(), ParamValue::Source(source))])
    }

    /// Deterministic random walk with distinct open/high/low/close and varying volume.
    fn walk_bars(n: usize) -> Vec<crate::model::Bar> {
        let mut seed: u64 = 7;
        let mut next = move || {
            seed = seed
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            (seed >> 33) as f64 / (1u64 << 31) as f64
        };
        let mut close = 100.0;
        (0..n)
            .map(|i| {
                let open = close;
                close += (next() - 0.5) * 3.0;
                let high = open.max(close) + next() * 1.5;
                let low = open.min(close) - next() * 1.5;
                crate::model::Bar::new(
                    i as i64 * 60,
                    open,
                    high,
                    low,
                    close,
                    500.0 + next() * 1000.0,
                )
            })
            .collect()
    }

    /// N16: an explicit `Close` on an indicator whose native price is `hlc3` must switch it to the
    /// close. With `open == close == 100` and rising highs, `Close` and `Open` are the same series
    /// and must give the same result.
    #[test]
    fn test_build_typed_explicit_close_overrides_native_hlc3_source() {
        use crate::model::Source;
        let bars: Vec<crate::model::Bar> = (0..60)
            .map(|i| crate::model::Bar::new(i * 60, 100.0, 101.0 + i as f64, 100.0, 100.0, 1000.0))
            .collect();
        for name in ["cci", "wavetrend", "wt"] {
            let mut via_close = build_typed(name, &source_params(Source::Close)).unwrap();
            let mut via_open = build_typed(name, &source_params(Source::Open)).unwrap();
            let mut native = build_typed(name, &TypedParams::new()).unwrap();
            let (mut c, mut o, mut n) = (None, None, None);
            for bar in &bars {
                c = via_close.on_bar(bar).map(|x| x.value);
                o = via_open.on_bar(bar).map(|x| x.value);
                n = native.on_bar(bar).map(|x| x.value);
            }
            assert!(c.is_some(), "{name}: no output");
            assert_eq!(c, o, "{name}: Close and Open are identical here");
            assert_ne!(
                c, n,
                "{name}: Close must differ from the native hlc3 reading"
            );
        }
    }

    /// N17: indicators that read more than one bar field reject a single-price source.
    #[test]
    fn test_build_typed_rejects_single_price_source_on_bop_stochastic_rvi() {
        for name in [
            "bop",
            "stochastic",
            "rvi",
            "williams_r",
            "wpr",
            "cmf",
            "pressure",
        ] {
            let err = match build_typed(name, &source_params(crate::model::Source::Open)) {
                Err(e) => e,
                Ok(_) => panic!("expected '{name}' to reject a non-Close source"),
            };
            assert!(
                matches!(err, RegistryError::IncompatibleParameter { .. }),
                "'{name}' returned {err:?} instead of IncompatibleParameter"
            );
        }
    }

    /// Every catalog entry not declared [`SourceHandling::RangeDependent`] reads exactly one
    /// scalar per bar, and that scalar is its declared native source: feeding it a bar collapsed
    /// to that source changes nothing. An entry that reads a second field fails here and must be
    /// declared range-dependent (or its native source is wrong).
    #[test]
    fn test_source_mappable_entries_read_only_their_native_source() {
        let bars = walk_bars(400);
        for entry in catalog_of(IndicatorInput::Bars) {
            let handling = crate::indicator::definition::definition(entry.name)
                .unwrap()
                .facts
                .source;
            if handling == SourceHandling::RangeDependent || entry.name == "midas" {
                continue;
            }
            let mut raw = build_checked(entry.name, &entry.default_params).unwrap();
            let mut mapped = super::super::source_mapped::SourceMapped::new(
                build_checked(entry.name, &entry.default_params).unwrap(),
                handling.native(),
            );
            for bar in &bars {
                let a = raw.on_bar(bar).map(|o| o.value);
                let b = mapped.on_bar(bar).map(|o| o.value);
                let same = match (a, b) {
                    (Some(x), Some(y)) => (x - y).abs() < 1e-9 || (x.is_nan() && y.is_nan()),
                    (None, None) => true,
                    _ => false,
                };
                assert!(
                    same,
                    "'{}' reads more than its native source ({:?}): {a:?} vs {b:?}",
                    entry.name,
                    handling.native()
                );
            }
        }
    }

    /// N18: `lookback = 1` never produced a profile; it is rejected, `2` is the smallest
    /// window that outputs.
    #[test]
    fn test_money_flow_profile_requires_lookback_of_two() {
        let one = HashMap::from([("lookback".to_string(), 1.0)]);
        assert!(matches!(
            build_checked("money_flow_profile", &one),
            Err(RegistryError::InvalidParameter { .. })
        ));
        let two = HashMap::from([("lookback".to_string(), 2.0)]);
        let mut mfp = build_checked("mfp", &two).unwrap();
        let outputs = walk_bars(5).iter().filter_map(|b| mfp.on_bar(b)).count();
        assert!(outputs > 0);
    }

    #[test]
    fn test_aliases_resolve_to_buildable_canonical_names() {
        let aliases = crate::indicator::definition::definitions()
            .flat_map(|d| d.aliases.iter().map(move |alias| (alias, &d.name)));
        for (alias, canonical) in aliases {
            assert_eq!(canonical_name(alias), *canonical);
            assert_eq!(canonical_name(&alias.to_uppercase()), *canonical);
            assert!(
                canonical_indicator_names().contains(canonical),
                "alias '{alias}' points at non-canonical '{canonical}'"
            );
            assert!(
                build_checked(alias, &HashMap::new()).is_ok(),
                "alias '{alias}' does not build"
            );
        }
        assert_eq!(canonical_name("unknown_thing"), "unknown_thing");
    }

    #[test]
    fn test_build_typed_trend_relationship_native_smoother_kind_selection() {
        let params: TypedParams = HashMap::from([
            ("fast_kind".to_string(), ParamValue::Enum("jma".to_string())),
            (
                "slow_kind".to_string(),
                ParamValue::Enum("alma".to_string()),
            ),
            ("fast_len".to_string(), ParamValue::Int(5)),
            ("slow_len".to_string(), ParamValue::Int(20)),
        ]);
        assert!(build_typed("trend_relationship", &params).is_ok());

        let unknown: TypedParams = HashMap::from([(
            "fast_kind".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("trend_relationship", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown fast_kind"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_checked_trend_relationship_defaults_to_ema() {
        assert!(build_checked("trend_relationship", &HashMap::new()).is_ok());
    }

    #[test]
    fn test_build_typed_midas_native_mode_and_source_selection() {
        let params: TypedParams = HashMap::from([
            (
                "mode".to_string(),
                ParamValue::Enum("bottomfinder".to_string()),
            ),
            (
                "source".to_string(),
                ParamValue::Source(crate::model::Source::Close),
            ),
        ]);
        assert!(build_typed("midas", &params).is_ok());

        let unknown: TypedParams =
            HashMap::from([("mode".to_string(), ParamValue::Enum("bogus".to_string()))]);
        let err = match build_typed("midas", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown mode"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_checked_midas_defaults_to_topfinder() {
        assert!(build_checked("midas", &HashMap::new()).is_ok());
    }
}
