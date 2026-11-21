//! `docs/reference/indicators.md` is generated from the indicator definitions (plan 49), so the
//! published parameter reference cannot drift from the code. This test renders it and compares;
//! after a change to a definition, regenerate with
//!
//! ```sh
//! KESTREL_UPDATE_DOCS=1 cargo test --all-features --test indicator_reference_doc
//! ```

use std::collections::HashMap;
use std::fmt::Write;

use kestrel_chartkit::applicability::data_requirements;
use kestrel_chartkit::indicator::definition::{
    families, Check, ParamKind, ParamSpec, Verification,
};
use kestrel_chartkit::indicator::other_input::{intrabar_definitions, transform_definitions};
use kestrel_chartkit::indicator::pair::{build_pair_checked, pair_definitions};
use kestrel_chartkit::indicator::registry::{
    build_checked, catalog, output_range, IndicatorInput, OutputRange,
};
use kestrel_chartkit::output_unit;

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/reference/indicators.md");

fn number(v: f64) -> String {
    if v.fract() == 0.0 && v.abs() < 1e15 {
        format!("{}", v as i64)
    } else {
        format!("{v}")
    }
}

fn accepted(spec: &ParamSpec) -> String {
    let mut text = match spec.kind {
        ParamKind::Count { min, max, .. } => format!("whole number {min} – {max}"),
        ParamKind::Number { min, max, .. } => match spec.range_by {
            Some(by) => by
                .ranges
                .iter()
                .map(|(option, lo, hi)| {
                    format!(
                        "{} – {} ({} = {option})",
                        number(*lo),
                        number(*hi),
                        by.choice
                    )
                })
                .collect::<Vec<_>>()
                .join("; "),
            None => format!("{} – {}", number(min), number(max)),
        },
        ParamKind::Choice {
            options, numeric, ..
        } => {
            let names = options
                .iter()
                .map(|o| format!("`{o}`"))
                .collect::<Vec<_>>()
                .join(", ");
            if numeric {
                format!("{names} (f64 map: 0 – {})", options.len() - 1)
            } else {
                format!("{names} (typed map only)")
            }
        }
        ParamKind::Timestamp => "Unix seconds (typed map only)".to_string(),
        ParamKind::Source { .. } => "a price source (typed map only)".to_string(),
    };
    if spec.threshold {
        text.push_str("; threshold on the output scale");
    }
    if let Some(alias) = spec.legacy_alias {
        write!(text, "; also accepted as `{alias}`").unwrap();
    }
    text
}

fn default(spec: &ParamSpec) -> String {
    match spec.kind {
        ParamKind::Count { default, .. } => default.to_string(),
        ParamKind::Number { default, .. } => number(default),
        ParamKind::Choice { default, .. } => format!("`{default}`"),
        ParamKind::Timestamp => "—".to_string(),
        ParamKind::Source { default } => format!("`{default:?}`").to_lowercase(),
    }
}

fn range(name: &str) -> String {
    match output_range(name) {
        OutputRange::Bounded { min, max } => format!("{} – {}", number(min), number(max)),
        OutputRange::Centered { center } => format!("open, centred on {}", number(center)),
        OutputRange::NonNegative => "0 and above".to_string(),
        OutputRange::Unbounded => "not declared".to_string(),
    }
}

fn cell(text: &str) -> String {
    text.replace('|', "\\|")
}

/// The verification of the catalog entry `name`.
fn verification(name: &str) -> Verification {
    catalog()
        .into_iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("`{name}` is not in catalog()"))
        .verification
}

/// The line under the description of an entry that has not passed its acceptance yet.
fn open_notice(out: &mut String, name: &str) {
    if let Verification::Open(reason) = verification(name) {
        writeln!(out, "**Not yet verified:** {reason}.\n").unwrap();
    }
}

/// One table over every entry of `catalog()`: where it is described and whether it is verified.
fn overview(out: &mut String) {
    let entries = catalog();
    let open = entries
        .iter()
        .filter(|e| !e.verification.is_verified())
        .count();
    writeln!(
        out,
        "**Verification.** {} of the {} entries are verified, {open} are not yet. A verified \
         entry has passed its acceptance: its expected values come from a derivation \
         independent of the Rust code (a second implementation or a hand-derived scenario). An entry that is not yet verified computes and \
         is covered by the contract tests, but has not passed that acceptance; its entry below \
         says why.\n",
        entries.len() - open,
        entries.len()
    )
    .unwrap();
    out.push_str("| Entry | Input | Status |\n|---|---|---|\n");
    for entry in &entries {
        let input = match entry.input {
            IndicatorInput::Bars => "bars",
            IndicatorInput::Pair => "two series",
            IndicatorInput::Transform => "candle transform",
            IndicatorInput::Intrabar => "intrabar",
        };
        let status = match entry.verification {
            Verification::Verified => "verified".to_string(),
            Verification::Open(reason) => format!("**not yet verified:** {}", cell(reason)),
        };
        writeln!(out, "| `{}` | {input} | {status} |", entry.name).unwrap();
    }
    out.push('\n');
}

fn render() -> String {
    let mut out = String::new();
    let all = catalog().len();
    writeln!(
        out,
        "---\ntitle: Indicator reference\ndescription: All {all} entries of the catalog with their parameters, output unit and range and whether each is verified, generated from the indicator definitions.\norder: 2\n---\n"
    )
    .unwrap();
    out.push_str(
        "> **Generated, and still moving.** This page is rendered from the indicator definitions in\n\
         > `src/indicator/definition/` by `tests/indicator_reference_doc.rs`, so it always matches the\n\
         > code of this version. The crate is pre-1.0 and under active development: names, parameters\n\
         > and outputs can still change fundamentally between versions.\n\n",
    );
    out.push_str(
        "Every indicator is built by name with `build_checked` (an `f64` parameter map) or \
         `build_typed` (typed values, including choices by name and a `source`). Both validate \
         against the same definition: unknown keys, values outside the accepted range and broken \
         relations are errors, missing parameters take their default. **Warmup** is the bar of the \
         first output with the default parameters. **Needs real volume** marks indicators whose \
         result is only meaningful with traded volume. The [showcase](../../../showcase/) draws \
         every indicator with its defaults on synthetic bars of a fictitious CASOON share.\n\n",
    );
    overview(&mut out);
    for (family, list) in families() {
        writeln!(out, "## {family}\n").unwrap();
        for definition in list {
            let aliases = if definition.aliases.is_empty() {
                String::new()
            } else {
                format!(
                    " · alias {}",
                    definition
                        .aliases
                        .iter()
                        .map(|a| format!("`{a}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            };
            writeln!(out, "### `{}`{aliases}\n", definition.name).unwrap();
            writeln!(out, "{}\n", definition.description).unwrap();
            open_notice(&mut out, definition.name);
            // The showcase draws the indicator on the demo series (examples/site_showcase.rs).
            writeln!(
                out,
                "[See it in the showcase](../../../showcase/{}/)\n",
                definition.name
            )
            .unwrap();
            let warmup = build_checked(definition.name, &HashMap::new())
                .map(|i| i.warmup_period().to_string())
                .unwrap_or_else(|e| format!("({e})"));
            let volume = if data_requirements(definition.name).needs_real_volume {
                " · needs real volume"
            } else {
                ""
            };
            writeln!(
                out,
                "**Output** unit {:?}, range {} · **warmup** {warmup} bars{volume}\n",
                output_unit(definition.name),
                range(definition.name)
            )
            .unwrap();
            parameters(&mut out, definition.params, definition.checks);
        }
    }
    other_inputs(&mut out);
    out
}

/// The parameter table of one definition and the relations between its parameters.
fn parameters(out: &mut String, params: &[ParamSpec], checks: &[Check]) {
    if !params.is_empty() {
        out.push_str("| Parameter | Default | Accepted | Meaning |\n|---|---|---|---|\n");
        for spec in params {
            writeln!(
                out,
                "| `{}` | {} | {} | {} |",
                spec.name,
                default(spec),
                cell(&accepted(spec)),
                cell(spec.meaning)
            )
            .unwrap();
        }
        out.push('\n');
    }
    for check in checks {
        match check {
            Check::Less { lower, upper } => {
                writeln!(out, "Requires `{lower}` to be smaller than `{upper}`.\n").unwrap()
            }
            Check::AtMost {
                value,
                times,
                limit,
            } => writeln!(
                out,
                "Requires {times} times `{value}` not to exceed `{limit}`.\n"
            )
            .unwrap(),
        }
    }
}

/// The entries of `catalog()` that do not read one bar of one series, which are not part of the
/// families above.
fn other_inputs(out: &mut String) {
    out.push_str(
        "## Other input kinds\n\n\
         These are entries of `catalog()` whose `input` is not `Bars`; `build_checked` answers their \
         names with `RegistryError::WrongInput`, naming the builder to call instead.\n\n\
         ### Two series\n\n\
         `IndicatorInput::Pair`, built with `build_pair_checked` and run with `run_pair_batch`, \
         which pairs the bars of the two series by timestamp: they compare the series with a \
         second one, the reference. A bar whose timestamp the reference lacks is skipped and has \
         no output. **Warmup** counts paired bars. See [Indicators](../../guides/indicators/#two-series) \
         for the rules.\n\n",
    );
    for definition in pair_definitions() {
        writeln!(out, "#### `{}`\n", definition.name).unwrap();
        writeln!(out, "{}\n", definition.description).unwrap();
        open_notice(out, definition.name);
        let warmup = build_pair_checked(definition.name, &HashMap::new())
            .map(|i| i.warmup_period().to_string())
            .unwrap_or_else(|e| format!("({e})"));
        writeln!(
            out,
            "**Output** unit {:?}, range {} · **warmup** {warmup} paired bars\n",
            output_unit(definition.name),
            range(definition.name)
        )
        .unwrap();
        parameters(out, definition.params, definition.checks);
    }

    out.push_str(
        "### Candle transform\n\n\
         `IndicatorInput::Transform`, built with `build_transform_checked`: one bar in, one \
         derived candle out, with no warmup. The unit describes the candle's prices.\n\n",
    );
    for definition in transform_definitions() {
        writeln!(out, "#### `{}`\n", definition.name).unwrap();
        writeln!(out, "{}\n", definition.description).unwrap();
        open_notice(out, definition.name);
        writeln!(
            out,
            "**Output** unit {:?}, range {}\n",
            output_unit(definition.name),
            range(definition.name)
        )
        .unwrap();
        parameters(out, definition.params, definition.checks);
    }

    out.push_str(
        "### Intrabar\n\n\
         `IndicatorInput::Intrabar`, built with `build_intrabar_checked`: one step is the group of \
         lower-timeframe bars of one parent bar (`IntrabarGrouper`), the result an \
         `IntrabarDelta` per parent bar. The unit describes the primary value, the cumulative \
         delta at the parent's close.\n\n",
    );
    for definition in intrabar_definitions() {
        writeln!(out, "#### `{}`\n", definition.name).unwrap();
        writeln!(out, "{}\n", definition.description).unwrap();
        open_notice(out, definition.name);
        writeln!(
            out,
            "**Output** unit {:?}, range {}\n",
            output_unit(definition.name),
            range(definition.name)
        )
        .unwrap();
        parameters(out, definition.params, definition.checks);
    }
}

#[test]
fn indicator_reference_matches_the_definitions() {
    let rendered = render();
    if std::env::var_os("KESTREL_UPDATE_DOCS").is_some() {
        std::fs::write(PATH, &rendered).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(PATH).unwrap_or_default();
    assert!(
        committed == rendered,
        "docs/reference/indicators.md is out of date; regenerate with \
         KESTREL_UPDATE_DOCS=1 cargo test --all-features --test indicator_reference_doc"
    );
}
