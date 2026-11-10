---
title: Elliott rules
description: What the Elliott validators check, generated from the rule catalogue of the crate, with the 4 conditions of the pivot sequence, the rules and guidelines of the 5 structures and the thresholds with their origin.
order: 4
---

> **Generated.** This page is rendered from `indicator::elliott::rule_catalog()` by
> `tests/structure_catalog_doc.rs`, so it always matches the code of this version. The same
> catalogue is bound to the behaviour by tests: every identifier a validator reports is listed
> here with its sentence, variants and assertion, every entry is reported by at least one test
> sequence for every variant it names, and every threshold is where the behaviour turns.

The validators of `indicator::elliott` check a given labeling; they do not count waves. A **rule** makes a labeling invalid and is reported in `violations`. A **guideline** is reported in `guidelines` and leaves `valid` untouched. The reference for both is the summary of rules and guidelines of Frost and Prechter's *Elliott Wave Principle*, read in reproductions; where it leaves a choice open, the choice is this crate's convention and is marked as such below.

## The pivot sequence

Every validator first requires a pivot sequence. The conditions are tested in this order, each over the whole sequence, and the first that fails is reported: `check_*` returns the `SequenceError` naming the node, `validate_*` returns `None`. A refused sequence gets no verdict.

| Order | `SequenceError` | `kind().id()` | Condition |
|---|---|---|---|
| 1 | `NodeCount` | `node_count` | The sequence has a node count the structure accepts |
| 2 | `NotAlternating` | `not_alternating` | Highs and lows alternate |
| 3 | `TimeOrder` | `time_order` | No node is earlier than the node before it; equal timestamps are accepted |
| 4 | `PivotPlacement` | `pivot_placement` | Every high lies above the node before it and every low below it, except the nodes whose placement a rule of the structure reports |

## Structures

| Structure | Function | Nodes | Placement left to a rule | Rules | Guidelines |
|---|---|---|---|---|---|
| `impulse` | `check_impulse` | 6 | – | 3 | 0 |
| `diagonal` | `check_diagonal` | 6 | – | 2 | 0 |
| `correction` | `check_correction` | 4 | node 3 | 3 | 4 |
| `triangle` | `check_triangle` | 6 | – | 9 | 0 |
| `combination` | `check_combination` | 8, 12 | node 3, 7, 11 | those of `correction`, per segment of four nodes | those of `correction` |

The end of wave C of a correction is not held by the placement condition: a wave C that does not continue past wave B is reported as the violation `wave_c_must_extend_past_b` instead.

## Variants

Listed in the order the assignment tests them; a sequence takes the first variant whose condition holds. An impulse has no variants.

| Structure | Variant | Condition |
|---|---|---|
| `diagonal` | `contracting` | Wave 3 is shorter than wave 1, wave 5 shorter than wave 3 and wave 4 shorter than wave 2 |
| `diagonal` | `expanding` | Every other sequence |
| `correction` | `zigzag` | Wave A is asserted to subdivide into five waves, or nothing is asserted and wave B retraces less than 90 percent of wave A |
| `correction` | `running_flat` | Not a zigzag, wave B ends beyond the start of wave A and wave C does not end beyond the end of wave A |
| `correction` | `expanded_flat` | Not a zigzag, wave B retraces more than 105 percent of wave A and wave C ends beyond the end of wave A |
| `correction` | `flat` | Every other sequence that is not a zigzag |
| `triangle` | `expanding` | At least two of waves c, d and e end beyond the end of the wave two before them |
| `triangle` | `barrier` | Not expanding, and the ends of waves b and d lie within the barrier tolerance of each other |
| `triangle` | `contracting` | Every other sequence |
| `combination` | `double_three` | Eight nodes: two corrections W and Y joined by one X wave |
| `combination` | `triple_three` | Twelve nodes: three corrections W, Y and Z joined by two X waves |

## Rules and guidelines

Listed per structure in the order they are tested and reported. **Variants** are the variants a result can have when it reports the entry; a variant that is missing cannot depart from it, because the check is not made for it or its definition already satisfies it. **Needs assertion** marks the two rules that can only fail when the subdivision of wave A is asserted (`CorrectionOptions::wave_a`); without it they hold by construction. A combination reports the entries of `correction` for each of its segments.

| Structure | Identifier | Kind | Variants | Needs assertion | Origin | Thresholds |
|---|---|---|---|---|---|---|
| `impulse` | `wave2_no_full_retrace` | rule | – | – | literature rule | – |
| `impulse` | `wave3_not_shortest` | rule | – | – | literature rule | – |
| `impulse` | `wave4_no_overlap` | rule | – | – | literature rule | – |
| `diagonal` | `wave2_no_full_retrace` | rule | `contracting`, `expanding` | – | literature rule | – |
| `diagonal` | `wave3_not_shortest` | rule | `expanding` | – | literature rule | – |
| `correction` | `wave_c_must_extend_past_b` | rule | `zigzag`, `flat`, `running_flat` | – | crate convention | – |
| `correction` | `zigzag_b_not_beyond_start_of_a` | rule | `zigzag` | `wave_a` = five | literature rule | – |
| `correction` | `flat_b_min_retrace` | rule | `flat` | `wave_a` = three | literature rule | `FLAT_B_MIN_RETRACE` |
| `correction` | `zigzag_b_retrace_bound` | guideline | `zigzag` | – | literature guideline | `ZIGZAG_B_TYPICAL_MAX` |
| `correction` | `zigzag_c_ends_beyond_a` | guideline | `zigzag` | – | literature guideline | – |
| `correction` | `flat_b_retrace_bound` | guideline | `flat`, `expanded_flat`, `running_flat` | – | literature guideline | `FLAT_B_TYPICAL_MAX` |
| `correction` | `flat_c_ends_beyond_a` | guideline | `flat` | – | literature guideline | – |
| `triangle` | `triangle_c_not_beyond_a` | rule | `contracting`, `barrier` | – | literature rule | – |
| `triangle` | `triangle_d_not_beyond_b` | rule | `contracting` | – | literature rule | – |
| `triangle` | `triangle_e_not_beyond_c` | rule | `contracting`, `barrier` | – | literature rule | – |
| `triangle` | `expanding_triangle_c_beyond_a` | rule | `expanding` | – | literature rule | – |
| `triangle` | `expanding_triangle_d_beyond_b` | rule | `expanding` | – | literature rule | – |
| `triangle` | `expanding_triangle_e_beyond_c` | rule | `expanding` | – | literature rule | – |
| `triangle` | `expanding_triangle_b_retrace` | rule | `expanding` | – | literature rule | `EXPANDING_TRIANGLE_RETRACE_MIN`, `EXPANDING_TRIANGLE_RETRACE_MAX` |
| `triangle` | `expanding_triangle_c_retrace` | rule | `expanding` | – | literature rule | `EXPANDING_TRIANGLE_RETRACE_MIN`, `EXPANDING_TRIANGLE_RETRACE_MAX` |
| `triangle` | `expanding_triangle_d_retrace` | rule | `expanding` | – | literature rule | `EXPANDING_TRIANGLE_RETRACE_MIN`, `EXPANDING_TRIANGLE_RETRACE_MAX` |

What each entry demands (`statement`), the sentence reported with its identifier under the default options (`detail`), and what of it is this crate's convention:

| Structure | Identifier | Statement | Sentence reported | Convention |
|---|---|---|---|---|
| `impulse` | `wave2_no_full_retrace` | Wave 2 never moves beyond the start of wave 1 | Wave 2 retraced beyond the start of wave 1 | a wave that ends exactly on the level counts as beyond it |
| `impulse` | `wave3_not_shortest` | Wave 3 is never the shortest of waves 1, 3 and 5: it is not shorter than both others | Wave 3 is the shortest of waves 1, 3, and 5 | – |
| `impulse` | `wave4_no_overlap` | Wave 4 never moves beyond the end of wave 1 | Wave 4 entered wave 1's price territory | a wave that ends exactly on the level counts as beyond it |
| `diagonal` | `wave2_no_full_retrace` | Wave 2 never moves beyond the start of wave 1 | Wave 2 retraced beyond the start of wave 1 | taken over from the impulse, and whether the literature states it for diagonals in this form is not established; a wave that ends exactly on the level counts as beyond it |
| `diagonal` | `wave3_not_shortest` | Wave 3 is never the shortest of waves 1, 3 and 5: it is not shorter than both others | Wave 3 is the shortest of waves 1, 3, and 5 | taken over from the impulse, and whether the literature states it for diagonals in this form is not established |
| `correction` | `wave_c_must_extend_past_b` | Wave C continues past the end of wave B in wave A's direction | Wave C did not continue past wave B in wave A's direction | – |
| `correction` | `zigzag_b_not_beyond_start_of_a` | In a zigzag, wave B never moves beyond the start of wave A | Wave B moved beyond the start of wave A | a wave that ends exactly on the level counts as not beyond it |
| `correction` | `flat_b_min_retrace` | In a flat, wave B retraces at least 90 percent of wave A | Wave B retraced less than 90% of wave A | – |
| `correction` | `zigzag_b_retrace_bound` | A zigzag's wave B retraces at most 78.6 percent of wave A | Wave B retraced more than a Zigzag's typical bound (78.6%) without qualifying as a Flat | 79 percent is written as 0.786 |
| `correction` | `zigzag_c_ends_beyond_a` | A zigzag's wave C ends beyond the end of wave A | Wave C did not end beyond the end of wave A | a wave that ends exactly on the level counts as not beyond it |
| `correction` | `flat_b_retrace_bound` | A flat's wave B retraces at most 138.2 percent of wave A, or the bound set as an option | Wave B retraced more than a Flat's typical bound (138.2%) | 138 percent is written as 1.382 |
| `correction` | `flat_c_ends_beyond_a` | The wave C of a flat that is neither expanded nor running ends beyond the end of wave A | Wave C did not end beyond the end of wave A | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `triangle_c_not_beyond_a` | In a contracting or barrier triangle, wave c never moves beyond the end of wave a | Wave C moved beyond the end of wave A | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `triangle_d_not_beyond_b` | In a contracting triangle, wave d never moves beyond the end of wave b | Wave D moved beyond the end of wave B | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `triangle_e_not_beyond_c` | In a contracting or barrier triangle, wave e never moves beyond the end of wave c | Wave E moved beyond the end of wave C | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `expanding_triangle_c_beyond_a` | In an expanding triangle, wave c moves beyond the end of wave a | Wave C did not move beyond the end of wave A | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `expanding_triangle_d_beyond_b` | In an expanding triangle, wave d moves beyond the end of wave b | Wave D did not move beyond the end of wave B | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `expanding_triangle_e_beyond_c` | In an expanding triangle, wave e moves beyond the end of wave c | Wave E did not move beyond the end of wave C | a wave that ends exactly on the level counts as not beyond it |
| `triangle` | `expanding_triangle_b_retrace` | In an expanding triangle, wave b retraces at least 100 and at most 150 percent of wave a | Wave B retraced less than 100% or more than 150% of wave A | – |
| `triangle` | `expanding_triangle_c_retrace` | In an expanding triangle, wave c retraces at least 100 and at most 150 percent of wave b | Wave C retraced less than 100% or more than 150% of wave B | – |
| `triangle` | `expanding_triangle_d_retrace` | In an expanding triangle, wave d retraces at least 100 and at most 150 percent of wave c | Wave D retraced less than 100% or more than 150% of wave C | – |

The sentence of `flat_b_retrace_bound` carries its bound, which is an option (`CorrectionOptions::flat_b_typical_max`), as a percentage.

## Thresholds

The public constants of the module and what each decides. A constant with two uses has two rows. `EXTENDED_WAVE_MIN_RATIO` belongs to the measure `identify_extended_wave`, not to an entry of the table above.

| Constant | Value | Role | Origin | Option | Meaning | Convention |
|---|---|---|---|---|---|---|
| `FLAT_B_MIN_RETRACE` | 0.9 | form boundary | crate convention | – | Without an assertion about wave A, a correction whose wave B retraces at least this share of wave A is a flat, below it a zigzag | – |
| `FLAT_B_MIN_RETRACE` | 0.9 | rule | literature rule | – | In a flat, wave B retraces at least this share of wave A | – |
| `EXPANDED_FLAT_B_MIN` | 1.05 | form boundary | literature rule | – | A flat whose wave B retraces more than this share of wave A and whose wave C ends beyond the end of wave A is an expanded flat | – |
| `ZIGZAG_B_TYPICAL_MAX` | 0.786 | guideline | literature guideline | – | A zigzag's wave B typically retraces at most this share of wave A | 79 percent is written as 0.786 |
| `FLAT_B_TYPICAL_MAX` | 1.382 | guideline | literature guideline | `flat_b_typical_max` | A flat's wave B usually retraces at most this share of wave A | 138 percent is written as 1.382 |
| `EXPANDING_TRIANGLE_RETRACE_MIN` | 1 | rule | literature rule | – | In an expanding triangle, waves b, c and d each retrace at least this share of the wave before them | – |
| `EXPANDING_TRIANGLE_RETRACE_MAX` | 1.5 | rule | literature rule | – | In an expanding triangle, waves b, c and d each retrace at most this share of the wave before them | – |
| `BARRIER_LEVEL_TOLERANCE` | 0.05 | form boundary | crate convention | `barrier_tolerance` | A triangle that is not expanding is a barrier triangle if the ends of waves b and d lie at most this share of the length of wave b apart | – |
| `EXTENDED_WAVE_MIN_RATIO` | 1.618 | guideline | crate convention | – | A wave 1, 3 or 5 of an impulse at least this many times as long as the longer of the other two is reported as extended by identify_extended_wave | – |

## Not among the checks

The checks above are what is reported as a rule or a guideline. `identify_extended_wave` and `is_truncated_fifth` are measures of the impulse with their own functions and no identifier, and only the threshold of the former is listed; `TriangleValidation::running` and `DiagonalValidation::wave4_overlaps_wave1` are properties of a result; the asserted `DiagonalKind` enters no check. Rules of the literature that need the subdivision of a wave or its place in a larger count are not checked, and the documentation of each `check_*` function names them.

## Deriving the flow of the checks

The conditions of the pivot sequence are steps that end the flow when they fail; the rules and guidelines are steps that each report on their own. `examples/elliott_check_flow.rs`, run with `cargo run --example elliott_check_flow`:

```rust
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
```
