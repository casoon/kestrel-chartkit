//! The order in which an Elliott labeling is checked, as a flow per structure: the steps and
//! their outcomes are read from `elliott::rule_catalog()`, nothing is written out by hand.
//!
//! `cargo run --example elliott_check_flow`

use kestrel_chartkit::indicator::elliott::{rule_catalog, CheckEntry};

/// The variants a check can be reported for, or `all` for a structure without variants.
fn applies_to(check: &CheckEntry) -> String {
    if check.variants.is_empty() {
        return "all".to_string();
    }
    let ids: Vec<&str> = check.variants.iter().map(|variant| variant.id()).collect();
    ids.join(", ")
}

fn main() {
    let catalog = rule_catalog();
    for entry in catalog.structures {
        let counts: Vec<String> = entry.node_counts.iter().map(usize::to_string).collect();
        println!(
            "flow {} ({} nodes)",
            entry.structure.id(),
            counts.join(" or ")
        );

        // First the pivot sequence. The conditions are tested in this order, and the first one
        // that fails ends the flow: the sequence is refused, there is no verdict.
        for condition in catalog.sequence_conditions {
            println!("  step {}: {}", condition.error.id(), condition.text);
            println!("    fails -> refused ({})", condition.error.id());
        }

        // Then the variant: the first whose condition holds.
        for variant in entry.variants {
            println!("  variant {}: {}", variant.variant.id(), variant.text);
        }

        // Then every rule and guideline of the variant. None ends the flow: each one that fails
        // is reported, a rule as a violation, a guideline as a deviation.
        for check in entry.rules.iter().chain(entry.guidelines) {
            println!(
                "  step {} [{}]: {}",
                check.id,
                applies_to(check),
                check.statement
            );
            println!("    fails -> {} \"{}\"", check.kind.id(), check.detail);
        }
    }
}
