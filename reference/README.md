# Reference generator

The golden fixtures under `tests/fixtures/` hold the values the Rust implementation is checked
against. This directory holds the code those values come from, so that every one of them can be
reproduced by anyone with a Python interpreter.

```bash
python3 reference/generate.py            # write the fixtures
python3 reference/generate.py --check    # verify them byte for byte
```

Python 3.10 or newer; nothing else.

## Rules

- **Written from the documented formula, not from the Rust code.** A second implementation is
  only independent if it did not start as a translation of the first. Where a formula leaves a
  convention open (seed, divisor, extrapolation), the crate's documentation decides, and every
  such choice is stated in the fixture header.
- **Python standard library only.** `generate.py` parses every module here and refuses to run if
  anything else is imported.
- **Deterministic.** The same code produces the same bytes; `--check` is part of the review chain.
- **Documented alternatives are generated, not asserted.** Where the crate deliberately follows
  one of several established conventions, the fixture carries the value under the other one as
  well, marked as such, so the choice is visible as a number.

## Coverage

| Fixture | Module |
|---|---|
| `golden_option_diff` | `kestrel_reference/fixtures/option_diff.py` |
| `golden_bond_diff` | `kestrel_reference/fixtures/bond_diff.py` |
| `golden_curve_diff` | `kestrel_reference/fixtures/curve_diff.py` |
| `golden_business_days` | `kestrel_reference/fixtures/business_days.py` |
| `golden_surface_diff` | `kestrel_reference/fixtures/surface_diff.py` |
| `golden_revaluation_diff` | `kestrel_reference/fixtures/revaluation_diff.py` |
| `golden_sample_stats` | `kestrel_reference/fixtures/sample_stats.py` |
| `golden_cross_asset_alignment` | `kestrel_reference/fixtures/cross_asset_alignment.py` |
| `golden_forward` | `kestrel_reference/fixtures/forward.py` |
| `golden_deflated_sharpe` | `kestrel_reference/fixtures/deflated_sharpe.py` |
| `golden_logistic` | `kestrel_reference/fixtures/logistic.py` |
| `golden_price_average_relation` | `kestrel_reference/fixtures/price_average_relation.py` |
| `golden_leg_conviction` | `kestrel_reference/fixtures/leg_conviction.py` |
| `golden_trend_break_risk` | `kestrel_reference/fixtures/trend_break_risk.py` |
| `golden_sweep_reversal_score` | `kestrel_reference/fixtures/sweep_reversal_score.py` |
| `golden_equal_level_liquidity` | `kestrel_reference/fixtures/equal_level_liquidity.py` |
| `golden_leg_character` | `kestrel_reference/fixtures/leg_character.py` |
| `golden_accumulation_phase` | `kestrel_reference/fixtures/accumulation_phase.py` |
| `golden_reversal_setup_score` | `kestrel_reference/fixtures/reversal_setup_score.py` |
| `golden_structure_zones` | `kestrel_reference/fixtures/structure_zones.py` |
| `golden_structural_flow` | `kestrel_reference/fixtures/structural_flow.py` |
| `golden_flow_exhaustion` | `kestrel_reference/fixtures/flow_exhaustion.py` |
| `golden_overextension_risk` | `kestrel_reference/fixtures/overextension_risk.py` |
| `golden_smoothed_supertrend` | `kestrel_reference/fixtures/smoothed_supertrend.py` |
| `golden_coil_release` | `kestrel_reference/fixtures/coil_release.py` |
| `golden_compression_zones` | `kestrel_reference/fixtures/compression_zones.py` |
| `golden_roc_context` | `kestrel_reference/fixtures/roc_context.py` |
| `golden_tradability_filter` | `kestrel_reference/fixtures/tradability_filter.py` |
| `golden_reaction_speed` | `kestrel_reference/fixtures/reaction_speed.py` |
| `golden_market_state_chain` | `kestrel_reference/fixtures/market_state_chain.py` |
| `golden_rsi_cycle_stats` | `kestrel_reference/fixtures/rsi_cycle_stats.py` |
| `golden_reversal_types` | `kestrel_reference/fixtures/reversal_types.py` |
| `contract_pilot` | `kestrel_reference/fixtures/contract_pilot.py` |

The remaining fixtures state their derivation in their own header; they are being brought under
this generator one by one.
