//! `docs/reference/wyckoff-transitions.md` is generated from `wyckoff::transitions()` and
//! `docs/reference/elliott-rules.md` from `elliott::rule_catalog()`, like the indicator
//! reference from the indicator definitions, so the published tables cannot drift from the
//! code. These tests render the pages and compare; after a change to a table, regenerate with
//!
//! ```sh
//! KESTREL_UPDATE_DOCS=1 cargo test --all-features --test structure_catalog_doc
//! ```

use std::fmt::Write;

use kestrel_chartkit::indicator::elliott::{rule_catalog, CheckEntry, WaveSubdivision};
use kestrel_chartkit::indicator::wyckoff::{transitions, WyckoffPhase, WyckoffRangeBias};

const WYCKOFF_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/reference/wyckoff-transitions.md"
);
const WYCKOFF_EXAMPLE: &str = include_str!("../examples/wyckoff_state_diagram.rs");
const ELLIOTT_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/reference/elliott-rules.md"
);
const ELLIOTT_EXAMPLE: &str = include_str!("../examples/elliott_check_flow.rs");

fn code_list<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items: Vec<String> = items.map(|item| format!("`{item}`")).collect();
    if items.is_empty() {
        "–".to_string()
    } else {
        items.join(", ")
    }
}

fn biases(list: &[WyckoffRangeBias]) -> String {
    if list.is_empty() {
        "–".to_string()
    } else {
        list.iter().map(|b| b.id()).collect::<Vec<_>>().join(", ")
    }
}

/// The example without its module comment: the page introduces it itself.
fn example_body(source: &str) -> &str {
    let start = source
        .find("\nuse ")
        .expect("the example starts with its imports");
    source[start..].trim()
}

fn render_wyckoff() -> String {
    let table = transitions();
    let mut out = String::new();
    writeln!(
        out,
        "---\ntitle: Wyckoff transitions\ndescription: The {} transitions of the Wyckoff state machine between its {} states, with bias, events, condition and parameters, generated from the transition table of the crate.\norder: 3\n---\n",
        table.len(),
        WyckoffPhase::ALL.len()
    )
    .unwrap();
    out.push_str(
        "> **Generated.** This page is rendered from `indicator::wyckoff::transitions()` by\n\
         > `tests/structure_catalog_doc.rs`, so it always matches the code of this version. The same\n\
         > table is bound to the behaviour by tests: on every test series each change of phase, bias or\n\
         > boundaries and each reported event belongs to one transition listed here, and every\n\
         > transition listed here is taken by at least one series.\n\n",
    );
    out.push_str(
        "The [`wyckoff`](../indicators/#wyckoff) indicator is a state machine. Its rules are written \
         out in the module documentation of `indicator::wyckoff`; `transitions()` returns the same \
         rules as data, one `WyckoffTransition` per move, so that a state diagram can be generated \
         instead of drawn by hand. `WyckoffStateMachine::last_transition()` names the transition \
         the last bar took.\n\n",
    );

    out.push_str("## States\n\n");
    out.push_str(
        "`WyckoffPhase::ALL` lists them. The output `value` of a bar is the code of its state.\n\n",
    );
    out.push_str("| State | `id()` | `value` |\n|---|---|---|\n");
    for phase in WyckoffPhase::ALL {
        let name = match phase {
            WyckoffPhase::Undefined => "no range".to_string(),
            other => format!("Phase {other:?}"),
        };
        writeln!(out, "| {name} | `{}` | {} |", phase.id(), phase.code()).unwrap();
    }
    out.push_str(
        "\nA range in play has a bias: `accumulation`, `distribution`, or `open` for a range set by \
         the width fallback until a Spring or UTAD decides it (`WyckoffRangeBias`). The bias is not \
         a state of its own; each transition says from which biases it can be taken and which \
         bias it leaves.\n\n",
    );

    out.push_str("## Transitions\n\n");
    out.push_str(
        "Listed by the state they start from and, within one state, in the order the calculation \
         tests them: of several whose condition holds on a bar, the first is taken. Events in \
         parentheses are only reported when found.\n\n",
    );
    out.push_str(
        "| Transition | From | To | Bias before | Bias after | Events | Condition | Parameters |\n\
         |---|---|---|---|---|---|---|---|\n",
    );
    for t in table {
        let events: Vec<String> = t
            .events
            .iter()
            .map(|kind| {
                if t.optional_events.contains(kind) {
                    format!("(`{kind:?}`)")
                } else {
                    format!("`{kind:?}`")
                }
            })
            .collect();
        writeln!(
            out,
            "| `{}` | `{}` | `{}` | {} | {} | {} | `{}` | {} |",
            t.id,
            t.from.id(),
            t.to.id(),
            biases(t.bias_from),
            t.bias_to.map_or("–", |b| b.id()),
            if events.is_empty() {
                "–".to_string()
            } else {
                events.join(", ")
            },
            t.condition.id(),
            code_list(t.params.iter().copied()),
        )
        .unwrap();
    }
    out.push_str("\nThe condition of each transition in words (`text`):\n\n");
    out.push_str("| Transition | Condition |\n|---|---|\n");
    for t in table {
        writeln!(out, "| `{}` | {} |", t.id, t.text).unwrap();
    }

    out.push_str(
        "\n## One transition per bar\n\n\
         A bar takes at most one transition. A bar that takes none leaves phase, bias and \
         boundaries as they were and reports no event. This is also why a bar that reports an \
         event, changes the phase or abandons a range does not set a range on the same bar: the \
         transitions into Phase A start from `none` or from `e`, and are tested on the next bar at \
         the earliest.\n\n\
         Two waits inside a state are not transitions and are not in the table: the pending climax \
         before `counter_move_confirmed` (while one is pending, the width fallback does not apply), \
         and the pullback that runs in Phase D before `pullback_held`. `Parameters` names what the \
         condition itself reads; the parameters of the climax bar test behind a pending climax are \
         listed with the transitions it leads to.\n\n",
    );

    out.push_str("## Deriving a state diagram\n\n");
    out.push_str(
        "Nodes are the states, edges the transitions, and the label of an edge its events and its \
         condition. Filtering by bias gives one diagram per side. \
         `examples/wyckoff_state_diagram.rs`, run with `cargo run --example wyckoff_state_diagram`:\n\n",
    );
    writeln!(out, "```rust\n{}\n```", example_body(WYCKOFF_EXAMPLE)).unwrap();
    out
}

fn assert_current(path: &str, rendered: &str) {
    if std::env::var_os("KESTREL_UPDATE_DOCS").is_some() {
        std::fs::write(path, rendered).unwrap();
        return;
    }
    let committed = std::fs::read_to_string(path).unwrap_or_default();
    assert!(
        committed == rendered,
        "{path} is out of date; regenerate with \
         KESTREL_UPDATE_DOCS=1 cargo test --all-features --test structure_catalog_doc"
    );
}

#[test]
fn wyckoff_transitions_page_matches_the_table() {
    assert_current(WYCKOFF_PATH, &render_wyckoff());
}

fn number(v: f64) -> String {
    format!("{v}")
}

fn usize_list(items: &[usize]) -> String {
    if items.is_empty() {
        "–".to_string()
    } else {
        items
            .iter()
            .map(usize::to_string)
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn check_rows(out: &mut String, structure: &str, checks: &[CheckEntry]) {
    for check in checks {
        let assertion = match check.asserted_wave_a {
            Some(WaveSubdivision::Five) => "`wave_a` = five",
            Some(WaveSubdivision::Three) => "`wave_a` = three",
            None => "–",
        };
        writeln!(
            out,
            "| {structure} | `{}` | {} | {} | {assertion} | {} | {} |",
            check.id,
            check.kind.id(),
            code_list(check.variants.iter().map(|v| v.id())),
            check.origin.id().replace('_', " "),
            code_list(check.thresholds.iter().copied()),
        )
        .unwrap();
    }
}

fn render_elliott() -> String {
    let catalog = rule_catalog();
    let checks = || {
        catalog
            .structures
            .iter()
            .filter(|entry| entry.segments.is_none())
            .flat_map(|entry| {
                entry
                    .rules
                    .iter()
                    .chain(entry.guidelines)
                    .map(move |check| (entry.structure.id(), check))
            })
    };
    let mut out = String::new();
    writeln!(
        out,
        "---\ntitle: Elliott rules\ndescription: What the Elliott validators check, generated from the rule catalogue of the crate, with the {} conditions of the pivot sequence, the rules and guidelines of the {} structures and the thresholds with their origin.\norder: 4\n---\n",
        catalog.sequence_conditions.len(),
        catalog.structures.len()
    )
    .unwrap();
    out.push_str(
        "> **Generated.** This page is rendered from `indicator::elliott::rule_catalog()` by\n\
         > `tests/structure_catalog_doc.rs`, so it always matches the code of this version. The same\n\
         > catalogue is bound to the behaviour by tests: every identifier a validator reports is listed\n\
         > here with its sentence, variants and assertion, every entry is reported by at least one test\n\
         > sequence for every variant it names, and every threshold is where the behaviour turns.\n\n",
    );
    out.push_str(
        "The validators of `indicator::elliott` check a given labeling; they do not count waves. \
         A **rule** makes a labeling invalid and is reported in `violations`. A **guideline** is \
         reported in `guidelines` and leaves `valid` untouched. The reference for both is the \
         summary of rules and guidelines of Frost and Prechter's *Elliott Wave Principle*, read in \
         reproductions; where it leaves a choice open, the choice is this crate's convention and \
         is marked as such below.\n\n",
    );

    out.push_str("## The pivot sequence\n\n");
    out.push_str(
        "Every validator first requires a pivot sequence. The conditions are tested in this \
         order, each over the whole sequence, and the first that fails is reported: `check_*` \
         returns the `SequenceError` naming the node, `validate_*` returns `None`. A refused \
         sequence gets no verdict.\n\n",
    );
    out.push_str("| Order | `SequenceError` | `kind().id()` | Condition |\n|---|---|---|---|\n");
    for (i, condition) in catalog.sequence_conditions.iter().enumerate() {
        writeln!(
            out,
            "| {} | `{:?}` | `{}` | {} |",
            i + 1,
            condition.error,
            condition.error.id(),
            condition.text
        )
        .unwrap();
    }

    out.push_str("\n## Structures\n\n");
    out.push_str(
        "| Structure | Function | Nodes | Placement left to a rule | Rules | Guidelines |\n\
         |---|---|---|---|---|---|\n",
    );
    for entry in catalog.structures {
        let (rules, guidelines) = match entry.segments {
            Some(segments) => (
                format!("those of `{}`, per segment of four nodes", segments.id()),
                format!("those of `{}`", segments.id()),
            ),
            None => (
                entry.rules.len().to_string(),
                entry.guidelines.len().to_string(),
            ),
        };
        writeln!(
            out,
            "| `{}` | `{}` | {} | {} | {rules} | {guidelines} |",
            entry.structure.id(),
            entry.checker,
            usize_list(entry.node_counts),
            match entry.placement_left_to_rule {
                [] => "–".to_string(),
                nodes => format!("node {}", usize_list(nodes)),
            },
        )
        .unwrap();
    }
    out.push_str(
        "\nThe end of wave C of a correction is not held by the placement condition: a wave C \
         that does not continue past wave B is reported as the violation \
         `wave_c_must_extend_past_b` instead.\n\n",
    );

    out.push_str("## Variants\n\n");
    out.push_str(
        "Listed in the order the assignment tests them; a sequence takes the first variant whose \
         condition holds. An impulse has no variants.\n\n",
    );
    out.push_str("| Structure | Variant | Condition |\n|---|---|---|\n");
    for entry in catalog.structures {
        for variant in entry.variants {
            writeln!(
                out,
                "| `{}` | `{}` | {} |",
                entry.structure.id(),
                variant.variant.id(),
                variant.text
            )
            .unwrap();
        }
    }

    out.push_str("\n## Rules and guidelines\n\n");
    out.push_str(
        "Listed per structure in the order they are tested and reported. **Variants** are the \
         variants a result can have when it reports the entry; a variant that is missing cannot \
         depart from it, because the check is not made for it or its definition already satisfies \
         it. **Needs assertion** marks the two rules that can only fail when the subdivision of \
         wave A is asserted (`CorrectionOptions::wave_a`); without it they hold by construction. \
         A combination reports the entries of `correction` for each of its segments.\n\n",
    );
    out.push_str(
        "| Structure | Identifier | Kind | Variants | Needs assertion | Origin | Thresholds |\n\
         |---|---|---|---|---|---|---|\n",
    );
    for entry in catalog
        .structures
        .iter()
        .filter(|entry| entry.segments.is_none())
    {
        let structure = format!("`{}`", entry.structure.id());
        check_rows(&mut out, &structure, entry.rules);
        check_rows(&mut out, &structure, entry.guidelines);
    }
    out.push_str(
        "\nWhat each entry demands (`statement`), the sentence reported with its identifier under \
         the default options (`detail`), and what of it is this crate's convention:\n\n",
    );
    out.push_str(
        "| Structure | Identifier | Statement | Sentence reported | Convention |\n|---|---|---|---|---|\n",
    );
    for (structure, check) in checks() {
        writeln!(
            out,
            "| `{structure}` | `{}` | {} | {} | {} |",
            check.id,
            check.statement,
            check.detail,
            check.convention.unwrap_or("–")
        )
        .unwrap();
    }
    out.push_str(
        "\nThe sentence of `flat_b_retrace_bound` carries its bound, which is an option \
         (`CorrectionOptions::flat_b_typical_max`), as a percentage.\n\n",
    );

    out.push_str("## Thresholds\n\n");
    out.push_str(
        "The public constants of the module and what each decides. A constant with two uses has \
         two rows. `EXTENDED_WAVE_MIN_RATIO` belongs to the measure `identify_extended_wave`, not \
         to an entry of the table above.\n\n",
    );
    out.push_str(
        "| Constant | Value | Role | Origin | Option | Meaning | Convention |\n|---|---|---|---|---|---|---|\n",
    );
    for threshold in catalog.thresholds {
        writeln!(
            out,
            "| `{}` | {} | {} | {} | {} | {} | {} |",
            threshold.constant,
            number(threshold.value),
            threshold.role.id().replace('_', " "),
            threshold.origin.id().replace('_', " "),
            threshold
                .option
                .map_or("–".to_string(), |o| format!("`{o}`")),
            threshold.text,
            threshold.convention.unwrap_or("–")
        )
        .unwrap();
    }

    out.push_str(
        "\n## Not among the checks\n\n\
         The checks above are what is reported as a rule or a guideline. `identify_extended_wave` \
         and `is_truncated_fifth` are measures of the impulse with their own functions and no \
         identifier, and only the threshold of the former is listed; \
         `TriangleValidation::running` and `DiagonalValidation::wave4_overlaps_wave1` are \
         properties of a result; the asserted `DiagonalKind` enters no check. Rules of the \
         literature that need the subdivision of a wave or its place in a larger count are not \
         checked, and the documentation of each `check_*` function names them.\n\n",
    );

    out.push_str("## Deriving the flow of the checks\n\n");
    out.push_str(
        "The conditions of the pivot sequence are steps that end the flow when they fail; the \
         rules and guidelines are steps that each report on their own. \
         `examples/elliott_check_flow.rs`, run with `cargo run --example elliott_check_flow`:\n\n",
    );
    writeln!(out, "```rust\n{}\n```", example_body(ELLIOTT_EXAMPLE)).unwrap();
    out
}

#[test]
fn elliott_rules_page_matches_the_catalogue() {
    assert_current(ELLIOTT_PATH, &render_elliott());
}
