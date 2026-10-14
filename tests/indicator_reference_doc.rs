//! `docs/reference/indicators.mdx` is generated from the indicator definitions (plan 49), so the
//! published parameter reference cannot drift from the code. This test renders it and compares;
//! after a change to a definition, regenerate with
//!
//! ```sh
//! KESTREL_UPDATE_DOCS=1 cargo test --all-features --test indicator_reference_doc
//! ```

use std::collections::HashMap;
use std::fmt::Write;

use kestrel_chartkit::applicability::data_requirements;
use kestrel_chartkit::indicator::definition::{families, Check, ParamKind, ParamSpec};
use kestrel_chartkit::indicator::registry::{build_checked, output_range, OutputRange};
use kestrel_chartkit::output_unit;

const PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/docs/reference/indicators.mdx");

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

fn render() -> String {
    let mut out = String::new();
    let total: usize = families().map(|(_, list)| list.len()).sum();
    writeln!(
        out,
        "---\ntitle: Indicator reference\ndescription: All {total} indicators of the registry with their parameters, output unit and range, generated from the indicator definitions.\norder: 2\n---\n"
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
         result is only meaningful with traded volume. The chart under each entry runs the \
         indicator with its defaults over synthetic 4-hour bars of a fictitious CASOON share (invented prices): a \
         price result over the candles, anything else in a pane below, its latest alerts as \
         markers. It shows what the calculation produces, not a trading result.\n\n",
    );
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
            // The site renders the indicator on the demo series (examples/site_showcase.rs).
            writeln!(out, "<IndicatorChart name=\"{}\" />\n", definition.name).unwrap();
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
            if !definition.params.is_empty() {
                out.push_str("| Parameter | Default | Accepted | Meaning |\n|---|---|---|---|\n");
                for spec in definition.params {
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
            for check in definition.checks {
                match check {
                    Check::Less { lower, upper } => {
                        writeln!(out, "Requires `{lower}` to be smaller than `{upper}`.\n").unwrap()
                    }
                }
            }
        }
    }
    out
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
        "docs/reference/indicators.mdx is out of date; regenerate with \
         KESTREL_UPDATE_DOCS=1 cargo test --all-features --test indicator_reference_doc"
    );
}
