//! The Wyckoff state machine as a state diagram: nodes, edges and edge labels are read from
//! `wyckoff::transitions()`, nothing is drawn by hand.
//!
//! `cargo run --example wyckoff_state_diagram`

use kestrel_chartkit::indicator::wyckoff::{
    transitions, WyckoffPhase, WyckoffRangeBias, WyckoffTransition,
};

/// The label of an edge: the events the transition reports, or the name of its condition where
/// it reports none, and the condition in words.
fn label(t: &WyckoffTransition) -> String {
    let events: Vec<String> = t
        .events
        .iter()
        .map(|kind| {
            if t.optional_events.contains(kind) {
                format!("({kind:?})")
            } else {
                format!("{kind:?}")
            }
        })
        .collect();
    let head = if events.is_empty() {
        t.condition.id().to_string()
    } else {
        events.join(" + ")
    };
    format!("{head}: {}", t.text)
}

fn main() {
    // One diagram per side. An edge belongs to a side if it starts from or leads to a range
    // with that bias; the other edges set, test or abandon a range whose bias is still open.
    for side in [
        WyckoffRangeBias::Accumulation,
        WyckoffRangeBias::Distribution,
    ] {
        println!("diagram {}", side.id());
        for phase in WyckoffPhase::ALL {
            println!("  node {}", phase.id());
        }
        for t in transitions()
            .iter()
            .filter(|t| t.bias_from.contains(&side) || t.bias_to == Some(side))
        {
            println!("  edge {} -> {} [{}]", t.from.id(), t.to.id(), label(t));
        }
    }
}
