---
title: Wyckoff transitions
description: The 27 transitions of the Wyckoff state machine between its 6 states, with bias, events, condition and parameters, generated from the transition table of the crate.
order: 3
---

> **Generated.** This page is rendered from `indicator::wyckoff::transitions()` by
> `tests/structure_catalog_doc.rs`, so it always matches the code of this version. The same
> table is bound to the behaviour by tests: on every test series each change of phase, bias or
> boundaries and each reported event belongs to one transition listed here, and every
> transition listed here is taken by at least one series.

The [`wyckoff`](../indicators/#wyckoff) indicator is a state machine. Its rules are written out in the module documentation of `indicator::wyckoff`; `transitions()` returns the same rules as data, one `WyckoffTransition` per move, so that a state diagram can be generated instead of drawn by hand. `WyckoffStateMachine::last_transition()` names the transition the last bar took.

## States

`WyckoffPhase::ALL` lists them. The output `value` of a bar is the code of its state.

| State | `id()` | `value` |
|---|---|---|
| no range | `none` | 0 |
| Phase A | `a` | 1 |
| Phase B | `b` | 2 |
| Phase C | `c` | 3 |
| Phase D | `d` | 4 |
| Phase E | `e` | 5 |

A range in play has a bias: `accumulation`, `distribution`, or `open` for a range set by the width fallback until a Spring or UTAD decides it (`WyckoffRangeBias`). The bias is not a state of its own; each transition says from which biases it can be taken and which bias it leaves.

## Transitions

Listed by the state they start from and, within one state, in the order the calculation tests them: of several whose condition holds on a bar, the first is taken. Events in parentheses are only reported when found.

| Transition | From | To | Bias before | Bias after | Events | Condition | Parameters |
|---|---|---|---|---|---|---|---|
| `none_a_selling_climax` | `none` | `a` | – | accumulation | (`PreliminarySupport`), `SellingClimax`, `AutomaticRally` | `counter_move_confirmed` | `range_lookback`, `atr_len`, `volume_k`, `climax_range_atr` |
| `none_a_buying_climax` | `none` | `a` | – | distribution | (`PreliminarySupply`), `BuyingClimax`, `AutomaticReaction` | `counter_move_confirmed` | `range_lookback`, `atr_len`, `volume_k`, `climax_range_atr` |
| `none_a_width` | `none` | `a` | – | open | – | `width_fallback` | `range_lookback`, `range_atr_max`, `atr_len` |
| `a_c_spring` | `a` | `c` | accumulation, open | accumulation | `Spring` | `boundary_swept` | `min_range_bars` |
| `a_c_utad` | `a` | `c` | distribution, open | distribution | `Utad` | `boundary_swept` | `min_range_bars` |
| `a_none_abandon` | `a` | `none` | accumulation, distribution, open | – | – | `closed_far_outside` | `abandon_atr`, `atr_len` |
| `a_b_secondary_test_accumulation` | `a` | `b` | accumulation | accumulation | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `a_b_secondary_test_distribution` | `a` | `b` | distribution | distribution | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `a_b_secondary_test_open` | `a` | `b` | open | open | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `b_c_spring` | `b` | `c` | accumulation, open | accumulation | `Spring` | `boundary_swept` | `min_range_bars` |
| `b_c_utad` | `b` | `c` | distribution, open | distribution | `Utad` | `boundary_swept` | `min_range_bars` |
| `b_none_abandon` | `b` | `none` | accumulation, distribution, open | – | – | `closed_far_outside` | `abandon_atr`, `atr_len` |
| `b_b_secondary_test_accumulation` | `b` | `b` | accumulation | accumulation | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `b_b_secondary_test_distribution` | `b` | `b` | distribution | distribution | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `b_b_secondary_test_open` | `b` | `b` | open | open | `SecondaryTest` | `boundary_tested` | `st_atr`, `atr_len` |
| `c_d_sign_of_strength` | `c` | `d` | accumulation | accumulation | `SignOfStrength` | `breakout_close` | `breakout_volume`, `volume_k`, `range_lookback` |
| `c_d_sign_of_weakness` | `c` | `d` | distribution | distribution | `SignOfWeakness` | `breakout_close` | `breakout_volume`, `volume_k`, `range_lookback` |
| `c_none_abandon` | `c` | `none` | accumulation, distribution | – | – | `closed_far_outside` | `abandon_atr`, `atr_len` |
| `d_b_failed_breakout_accumulation` | `d` | `b` | accumulation | accumulation | – | `closed_back_inside` | – |
| `d_b_failed_breakout_distribution` | `d` | `b` | distribution | distribution | – | `closed_back_inside` | – |
| `d_e_last_point_of_support` | `d` | `e` | accumulation | accumulation | `LastPointOfSupport` | `pullback_held` | `lps_atr`, `atr_len` |
| `d_e_last_point_of_supply` | `d` | `e` | distribution | distribution | `LastPointOfSupply` | `pullback_held` | `lps_atr`, `atr_len` |
| `e_b_markup_ended` | `e` | `b` | accumulation | accumulation | – | `closed_back_inside` | – |
| `e_b_markdown_ended` | `e` | `b` | distribution | distribution | – | `closed_back_inside` | – |
| `e_a_selling_climax` | `e` | `a` | accumulation, distribution | accumulation | (`PreliminarySupport`), `SellingClimax`, `AutomaticRally` | `counter_move_confirmed` | `range_lookback`, `atr_len`, `volume_k`, `climax_range_atr` |
| `e_a_buying_climax` | `e` | `a` | accumulation, distribution | distribution | (`PreliminarySupply`), `BuyingClimax`, `AutomaticReaction` | `counter_move_confirmed` | `range_lookback`, `atr_len`, `volume_k`, `climax_range_atr` |
| `e_a_width` | `e` | `a` | accumulation, distribution | open | – | `width_fallback` | `range_lookback`, `range_atr_max`, `atr_len` |

The condition of each transition in words (`text`):

| Transition | Condition |
|---|---|
| `none_a_selling_climax` | Automatic Rally confirmed after a Selling Climax: close below the low of the bar that made the rally high, and that high above the high of the climax bar; range from the climax low to the rally high |
| `none_a_buying_climax` | Automatic Reaction confirmed after a Buying Climax: close above the high of the bar that made the reaction low, and that low below the low of the climax bar; range from the reaction low to the climax high |
| `none_a_width` | Width fallback: no climax pending and the window at most range_atr_max ATR wide; range at the window extremes, bias open |
| `a_c_spring` | Spring: low below the range low and close back above it, in a range at least min_range_bars bars old; the bar does not also sweep the range high |
| `a_c_utad` | UTAD: high above the range high and close back below it, in a range at least min_range_bars bars old; the bar does not also sweep the range low |
| `a_none_abandon` | Range abandoned: close more than abandon_atr ATR above the range high or below the range low |
| `a_b_secondary_test_accumulation` | Secondary Test: low at or above the range low and within st_atr ATR of it; in a range set by a climax, on less volume and a narrower spread than the climax bar |
| `a_b_secondary_test_distribution` | Secondary Test: high at or below the range high and within st_atr ATR of it; in a range set by a climax, on less volume and a narrower spread than the climax bar |
| `a_b_secondary_test_open` | Secondary Test of either boundary: low at or above the range low and within st_atr ATR of it, or high at or below the range high and within st_atr ATR of it |
| `b_c_spring` | Spring: low below the range low and close back above it, in a range at least min_range_bars bars old; the bar does not also sweep the range high |
| `b_c_utad` | UTAD: high above the range high and close back below it, in a range at least min_range_bars bars old; the bar does not also sweep the range low |
| `b_none_abandon` | Range abandoned: close more than abandon_atr ATR above the range high or below the range low |
| `b_b_secondary_test_accumulation` | Secondary Test: low at or above the range low and within st_atr ATR of it; in a range set by a climax, on less volume and a narrower spread than the climax bar |
| `b_b_secondary_test_distribution` | Secondary Test: high at or below the range high and within st_atr ATR of it; in a range set by a climax, on less volume and a narrower spread than the climax bar |
| `b_b_secondary_test_open` | Secondary Test of either boundary: low at or above the range low and within st_atr ATR of it, or high at or below the range high and within st_atr ATR of it |
| `c_d_sign_of_strength` | Sign of Strength: close above the range high, on outlier volume unless breakout_volume is off |
| `c_d_sign_of_weakness` | Sign of Weakness: close below the range low, on outlier volume unless breakout_volume is off |
| `c_none_abandon` | Range abandoned: close more than abandon_atr ATR above the range high or below the range low |
| `d_b_failed_breakout_accumulation` | Breakout failed: close back below the range high |
| `d_b_failed_breakout_distribution` | Breakout failed: close back above the range low |
| `d_e_last_point_of_support` | Last Point of Support: first higher close of a pullback whose low held at or above the range high minus lps_atr ATR |
| `d_e_last_point_of_supply` | Last Point of Supply: first lower close of a pullback whose high held at or below the range low plus lps_atr ATR |
| `e_b_markup_ended` | Markup ended: close back below the range high |
| `e_b_markdown_ended` | Markdown ended: close back above the range low |
| `e_a_selling_climax` | Automatic Rally confirmed after a Selling Climax: close below the low of the bar that made the rally high, and that high above the high of the climax bar; range from the climax low to the rally high |
| `e_a_buying_climax` | Automatic Reaction confirmed after a Buying Climax: close above the high of the bar that made the reaction low, and that low below the low of the climax bar; range from the reaction low to the climax high |
| `e_a_width` | Width fallback: no climax pending and the window at most range_atr_max ATR wide; range at the window extremes, bias open |

## One transition per bar

A bar takes at most one transition. A bar that takes none leaves phase, bias and boundaries as they were and reports no event. This is also why a bar that reports an event, changes the phase or abandons a range does not set a range on the same bar: the transitions into Phase A start from `none` or from `e`, and are tested on the next bar at the earliest.

Two waits inside a state are not transitions and are not in the table: the pending climax before `counter_move_confirmed` (while one is pending, the width fallback does not apply), and the pullback that runs in Phase D before `pullback_held`. `Parameters` names what the condition itself reads; the parameters of the climax bar test behind a pending climax are listed with the transitions it leads to.

## Deriving a state diagram

Nodes are the states, edges the transitions, and the label of an edge its events and its condition. Filtering by bias gives one diagram per side. `examples/wyckoff_state_diagram.rs`, run with `cargo run --example wyckoff_state_diagram`:

```rust
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
```
