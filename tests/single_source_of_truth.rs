//! Guards for plan 141: the definition of an entry is the one place for what is true of it.
//!
//! - every `SmootherKind` is a catalog entry;
//! - every public streaming calculation is reachable through the catalog, or is on a short,
//!   justified list of module APIs;
//! - no name list beside the definitions: the lookups read the definitions, and their source
//!   holds no catalog name as a string literal.
//!
//! The source scans read `src/` as text. They are deliberately simple (a declaration at the start
//! of a line, a test module at the end of a file), the way the sources are written.

use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};

use kestrel_chartkit::indicator::definition::definitions;
use kestrel_chartkit::indicator::other_input::{intrabar_definitions, transform_definitions};
use kestrel_chartkit::indicator::pair::pair_definitions;
use kestrel_chartkit::indicator::registry::catalog;
use kestrel_chartkit::indicator::smoothing::SmootherKind;

fn src_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("src")
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let mut entries: Vec<_> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for path in entries {
        if path.is_dir() {
            rs_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

/// The text of a source file without its test module (the first `#[cfg(test)]` at the start of a
/// line ends the code).
fn code_of(path: &Path) -> String {
    let text = fs::read_to_string(path).unwrap();
    let end = text
        .lines()
        .position(|l| l.starts_with("#[cfg(test)]"))
        .unwrap_or(usize::MAX);
    text.lines().take(end).collect::<Vec<_>>().join("\n")
}

/// Every catalog name and every alias of every kind.
fn catalog_names() -> BTreeSet<&'static str> {
    let mut names: BTreeSet<&'static str> = catalog().iter().map(|e| e.name).collect();
    for d in definitions() {
        names.extend(d.aliases.iter().copied());
    }
    for d in pair_definitions() {
        names.extend(d.aliases.iter().copied());
    }
    for d in transform_definitions() {
        names.extend(d.aliases.iter().copied());
    }
    for d in intrabar_definitions() {
        names.extend(d.aliases.iter().copied());
    }
    names
}

#[test]
fn every_smoother_kind_is_a_catalog_entry() {
    let names: BTreeSet<&str> = catalog().iter().map(|e| e.name).collect();
    for kind in SmootherKind::ALL {
        assert!(
            names.contains(kind.name()),
            "SmootherKind::{kind:?} is named `{}`, which is not a catalog entry",
            kind.name()
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Reachability
// ---------------------------------------------------------------------------------------------

/// Public streaming types that are not built by a catalog definition, each with the reason. Keep
/// it short: what computes a value per bar is a catalog entry or crate-internal (plan 141).
const MODULE_APIS: &[(&str, &str)] = &[
    (
        "SourceMapped",
        "decorator that `build_typed` puts around a catalog entry to read another price",
    ),
    (
        "WithReference",
        "adapter that lets a catalog pair entry run as an `Indicator` next to its pushed reference series",
    ),
    (
        "RankChannel",
        "decorator over any `Indicator`, not a calculation of its own",
    ),
    (
        "CompositionGraph",
        "wires catalog indicators into a dependency graph; computes no value of its own",
    ),
    (
        "RegimePersistenceTracker",
        "regime-label tracker, fed with `MarketRegime` labels, not with bars",
    ),
    (
        "RegimeMarkovModel",
        "regime-label transition model, fed with `MarketRegime` labels, not with bars",
    ),
    (
        "FibonacciReactionMemory",
        "module API of `indicator::elliott` (rule checks over a supplied swing labelling), no bar formula",
    ),
    (
        "OutcomeRecorder",
        "evaluation module: follows recorded setups through later bars, not an indicator",
    ),
    (
        "FillSimulator",
        "execution module: simulates order fills over bars, not an indicator",
    ),
    (
        "BarResampler",
        "timeframe aggregation of bars into coarser bars, not an indicator",
    ),
    (
        "ConfirmedResampler",
        "timeframe aggregation of bars into coarser bars, not an indicator",
    ),
    (
        "IntrabarGrouper",
        "groups lower-timeframe bars under their parent bar; feeds the intrabar entries, computes nothing",
    ),
    (
        "ScenarioStateMachine",
        "scenario lifecycle (candidate, active, invalidated), a state machine over events and not over a formula",
    ),
    (
        "ZoneRegistry",
        "structure module: lifecycle (touched, reacted, broken) of zones that detectors hand in, no formula of its own",
    ),
    (
        "StructuralTrailingStop",
        "trade management: a stop that only tightens when handed new structure levels, no bar formula",
    ),
    (
        "AlertDeduplicator",
        "event module: suppresses repeated alerts, computes no value",
    ),
];

/// Public streaming types that are neither built by a catalog definition nor a module API, found
/// by the scan and not decided yet: each is a candidate for a catalog entry or for `pub(crate)`.
/// The test fails when this list and the findings differ, so an entry leaves it with its decision.
const OPEN_DECISIONS: &[(&str, &str)] = &[];

/// Method names by which a type is fed a stream.
fn is_stream_method(name: &str) -> bool {
    matches!(
        name,
        "update"
            | "push"
            | "push_high_low"
            | "push_reference"
            | "advance"
            | "step"
            | "tick"
            | "admit"
            | "on_group"
            | "on_child_bar"
            | "on_intrabar_group"
            | "record"
    ) || name.starts_with("on_bar")
        || name.starts_with("observe")
}

/// The word at the start of `rest` (an identifier), if any.
fn ident(rest: &str) -> Option<&str> {
    let end = rest
        .find(|c: char| !(c.is_alphanumeric() || c == '_'))
        .unwrap_or(rest.len());
    (end > 0).then(|| &rest[..end])
}

/// Modules a `mod.rs` or `lib.rs` declares without `pub` (their types are not public API).
fn private_modules() -> BTreeSet<String> {
    let mut private = BTreeSet::new();
    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);
    for path in files {
        let name = path.file_name().unwrap().to_string_lossy();
        if name != "lib.rs" && name != "mod.rs" {
            continue;
        }
        for line in code_of(&path).lines() {
            let line = line.trim();
            if let Some(rest) = line
                .strip_prefix("pub(crate) mod ")
                .or_else(|| line.strip_prefix("mod "))
            {
                if let Some(module) = ident(rest) {
                    private.insert(module.to_string());
                }
            }
        }
    }
    private
}

/// Public types that stream: they implement one of the indicator traits, or have a public method
/// that takes a stream step. Returns the type and the file it is in.
fn streaming_types() -> BTreeMap<String, PathBuf> {
    let private = private_modules();
    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);
    let mut found = BTreeMap::new();
    for path in files {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        if private.contains(&stem) {
            continue;
        }
        let code = code_of(&path);
        let public: BTreeSet<String> = code
            .lines()
            .filter_map(|l| {
                l.strip_prefix("pub struct ")
                    .or_else(|| l.strip_prefix("pub enum "))
                    .and_then(ident)
                    .map(str::to_string)
            })
            .collect();
        let mut current: Option<String> = None;
        for line in code.lines() {
            if line.starts_with("impl") {
                let mut rest = line.trim_start_matches("impl");
                if rest.starts_with('<') {
                    // skip the generic parameter list
                    let mut depth = 0;
                    let mut end = 0;
                    for (i, c) in rest.char_indices() {
                        match c {
                            '<' => depth += 1,
                            '>' => {
                                depth -= 1;
                                if depth == 0 {
                                    end = i + 1;
                                    break;
                                }
                            }
                            _ => {}
                        }
                    }
                    rest = &rest[end..];
                }
                let rest = rest.trim_start();
                let target = match rest.split_once(" for ") {
                    Some((trait_part, ty)) => {
                        let trait_name = ident(trait_part.trim()).unwrap_or("");
                        let ty = ident(ty.trim()).map(str::to_string);
                        if matches!(
                            trait_name,
                            "Indicator" | "PairIndicator" | "BarTransform" | "IntrabarIndicator"
                        ) {
                            if let Some(ty) = &ty {
                                if public.contains(ty) {
                                    found.insert(ty.clone(), path.clone());
                                }
                            }
                        }
                        None
                    }
                    None => ident(rest).map(str::to_string),
                };
                current = target;
            } else if line.starts_with('}') {
                current = None;
            } else if let (Some(ty), Some(rest)) = (&current, line.strip_prefix("    pub fn ")) {
                if public.contains(ty)
                    && ident(rest).is_some_and(is_stream_method)
                    && rest.contains("&mut self")
                {
                    found.insert(ty.clone(), path.clone());
                }
            }
        }
    }
    found
}

/// Every identifier in the files that hold a catalog definition (a `*_DEFINITION` constant).
fn names_in_definition_files() -> BTreeSet<String> {
    let mut files = Vec::new();
    rs_files(&src_dir(), &mut files);
    let mut words = BTreeSet::new();
    for path in files {
        let code = code_of(&path);
        if !code.contains("_DEFINITION: ") {
            continue;
        }
        // Comments only talk about types; a definition names one in code.
        let code: String = code
            .lines()
            .filter(|l| !l.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");
        let mut word = String::new();
        for c in code.chars().chain(std::iter::once(' ')) {
            if c.is_alphanumeric() || c == '_' {
                word.push(c);
            } else if !word.is_empty() {
                words.insert(std::mem::take(&mut word));
            }
        }
    }
    words
}

#[test]
fn every_public_streaming_type_is_built_by_a_catalog_definition_or_a_listed_module_api() {
    let streaming = streaming_types();
    assert!(
        streaming.len() > 100,
        "the scan found only {} streaming types; it no longer reads the sources",
        streaming.len()
    );
    let in_definition_files = names_in_definition_files();
    let listed: BTreeMap<&str, &str> = MODULE_APIS.iter().copied().collect();

    let unreachable: Vec<String> = streaming
        .iter()
        .filter(|(ty, _)| !in_definition_files.contains(*ty) && !listed.contains_key(ty.as_str()))
        .map(|(ty, _)| ty.clone())
        .collect();
    let open: Vec<String> = OPEN_DECISIONS
        .iter()
        .map(|(ty, _)| ty.to_string())
        .collect();
    assert_eq!(
        unreachable, open,
        "public streaming types that no catalog definition names and that are not on the \
         module-API list — make them a catalog entry, crate-internal, or list them with a reason \
         (OPEN_DECISIONS holds the ones not decided yet)"
    );

    // The list must not rot: every entry is a public streaming type that no definition names.
    let not_streaming: Vec<&str> = MODULE_APIS
        .iter()
        .map(|(ty, _)| *ty)
        .filter(|ty| !streaming.contains_key(*ty))
        .collect();
    assert!(
        not_streaming.is_empty(),
        "on the module-API list but no public streaming type (any more): {not_streaming:?}"
    );
    let named: Vec<&str> = MODULE_APIS
        .iter()
        .map(|(ty, _)| *ty)
        .filter(|ty| in_definition_files.contains(*ty))
        .collect();
    assert!(
        named.is_empty(),
        "on the module-API list but a catalog definition names it; remove it: {named:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// No name lists beside the definitions
// ---------------------------------------------------------------------------------------------

/// The string literals of `code`, skipping comments.
fn string_literals(code: &str) -> Vec<String> {
    let chars: Vec<char> = code.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        match chars[i] {
            '/' if chars.get(i + 1) == Some(&'/') => {
                while i < chars.len() && chars[i] != '\n' {
                    i += 1;
                }
            }
            '"' => {
                let mut literal = String::new();
                i += 1;
                while i < chars.len() && chars[i] != '"' {
                    if chars[i] == '\\' {
                        i += 1;
                    }
                    if i < chars.len() {
                        literal.push(chars[i]);
                    }
                    i += 1;
                }
                out.push(literal);
                i += 1;
            }
            '\'' => {
                // a char literal or a lifetime; neither holds a name
                i += 1;
            }
            _ => i += 1,
        }
    }
    out
}

#[test]
fn the_lookups_by_name_hold_no_catalog_name() {
    let names = catalog_names();
    for file in [
        "indicator/registry.rs",
        "indicator/unit.rs",
        "indicator/other_input.rs",
        "applicability.rs",
    ] {
        let code = code_of(&src_dir().join(file));
        let hits: Vec<String> = string_literals(&code)
            .into_iter()
            .filter(|s| names.contains(s.as_str()))
            .collect();
        assert!(
            hits.is_empty(),
            "{file} names catalog entries as string literals outside its tests: {hits:?} — a \
             fact about an entry belongs in its definition"
        );
    }
}

/// The weights of the presets and the reasons of the scoring are keyed by indicator name outside
/// the definitions. They are choices of the scoring, not facts of an indicator, but a renamed or
/// removed entry must not leave a key behind that matches nothing.
#[test]
fn the_names_the_scoring_keys_on_are_catalog_entries() {
    use kestrel_chartkit::scoring::composite::WeightPreset;
    let names = catalog_names();
    for preset in [
        WeightPreset::Balanced,
        WeightPreset::TrendFollowing,
        WeightPreset::MeanReversion,
        WeightPreset::VolumeLocation,
    ] {
        for name in preset.weights().keys() {
            assert!(names.contains(name.as_str()), "{preset:?} weighs `{name}`");
        }
    }

    // `"name" =>` and `"name" |` in the match of the reason builder.
    let code = code_of(&src_dir().join("scoring/reason_builder.rs"));
    let mut checked = 0;
    // Only the match on the indicator's name: the alert kinds above it are not names.
    let by_name = code
        .split_once("match name.to_lowercase().as_str() {")
        .expect("reason_builder matches on the indicator name")
        .1;
    for line in by_name.lines() {
        let line = line.trim();
        if !(line.ends_with("=> {") || line.ends_with('|')) || !line.starts_with('"') {
            continue;
        }
        for literal in string_literals(line) {
            assert!(
                names.contains(literal.as_str()),
                "reason_builder keys on `{literal}`, which is not a catalog entry"
            );
            checked += 1;
        }
    }
    assert!(
        checked > 5,
        "the scan of reason_builder found {checked} names"
    );
}
