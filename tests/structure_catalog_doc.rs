//! `docs/reference/wyckoff-transitions.md` is generated from `wyckoff::transitions()`, like the
//! indicator reference from the indicator definitions, so the published table cannot drift from
//! the code. This test renders the page and compares; after a change to the table, regenerate
//! with
//!
//! ```sh
//! KESTREL_UPDATE_DOCS=1 cargo test --all-features --test structure_catalog_doc
//! ```

use std::fmt::Write;

use kestrel_chartkit::indicator::wyckoff::{transitions, WyckoffPhase, WyckoffRangeBias};

const WYCKOFF_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/docs/reference/wyckoff-transitions.md"
);
const WYCKOFF_EXAMPLE: &str = include_str!("../examples/wyckoff_state_diagram.rs");

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
