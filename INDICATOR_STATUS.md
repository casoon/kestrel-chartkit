# Indicator Status

As of: 2026-09-14

Overview of all indicators/detectors from `src/indicator/registry.rs`: name and whether it has
been verified (✅) or is still open (❌). Only the verification status — not what it was
verified against or how (details for that are maintained internally under `plan/`).

For every new indicator, add it here as ❌ by default and only flip it to ✅ once verification
is complete.

## Moving Averages & Core Oscillators

| Indicator | Verified |
|---|---|
| sma | ✅ |
| ema | ✅ |
| wma | ✅ |
| vwma | ✅ |
| hma | ✅ |
| dema | ✅ |
| kama | ✅ |
| tema | ✅ |
| lsma | ✅ |
| mcginley | ✅ |
| vidya | ✅ |
| t3 | ✅ |
| bollinger | ✅ |
| bbtrend | ✅ |
| cci | ✅ |
| stochastic | ✅ |
| stoch_rsi | ✅ |
| mfi | ✅ |
| williams_r | ✅ |
| tsi | ✅ |
| fisher_transform | ✅ |
| rsi | ✅ |
| macd | ✅ |

## Oscillators

| Indicator | Verified |
|---|---|
| awesome_oscillator | ✅ |
| bop | ✅ |
| chaikin_oscillator | ✅ |
| cmo | ✅ |
| connors_rsi | ✅ |
| coppock | ✅ |
| dpo | ✅ |
| elder_ray | ✅ |
| kst | ✅ |
| pmo | ✅ |
| ppo | ✅ |
| rci | ✅ |
| smi | ✅ |
| roc | ✅ |
| trix | ✅ |
| relative_volatility | ✅ |
| rvi | ✅ |
| ultimate_oscillator | ✅ |
| wavetrend | ✅ |
| flow_exhaustion | ✅ |
| overextension_risk | ✅ |
| roc_context | ✅ |
| rsi_cycle_stats | ✅ |

## Volatility

| Indicator | Verified |
|---|---|
| atr | ✅ |
| adx | ✅ |
| aroon | ✅ |
| chande_kroll | ✅ |
| chandelier_exit | ✅ |
| adaptive_chandelier | ✅ |
| choppiness | ✅ |
| dmi | ✅ |
| donchian | ✅ |
| envelope | ✅ |
| garman_klass | ✅ |
| historical_volatility | ✅ |
| keltner | ✅ |
| mass_index | ✅ |
| parabolic_sar | ✅ |
| supertrend | ✅ |
| true_range | ✅ |
| ulcer_index | ✅ |
| vix_fix | ✅ |
| vortex | ✅ |
| coil_release | ✅ |
| reaction_speed | ✅ |

## Volume

| Indicator | Verified |
|---|---|
| twap | ✅ |
| vwap | ✅ |
| volume_profile | ✅ |
| acc_dist | ✅ |
| anchored_vwap | ✅ |
| cmf | ✅ |
| cvd | ✅ |
| efi | ✅ |
| eom | ✅ |
| extended_volume_profile | ✅ |
| hires_volume_flow | ✅ |
| klinger | ✅ |
| money_flow_profile | ✅ |
| nvi | ✅ |
| obv | ✅ |
| persistent_volume_profile | ✅ |
| pvi | ✅ |
| pvt | ✅ |
| rvat | ✅ |
| rvol | ✅ |
| volume | ✅ |

## Trend

| Indicator | Verified |
|---|---|
| alligator | ✅ |
| efficiency | ✅ |
| ichimoku | ✅ |
| midas | ✅ |
| trend_relationship | ✅ |
| zscore | ✅ |
| smoothed_supertrend | ✅ |

## Composite Scores

| Indicator | Verified |
|---|---|
| trend_quality | ✅ |
| buy_sell_pressure | ✅ |
| volatility_regime | ✅ |
| multi_factor | ✅ |

## Structure/Pattern Detectors

| Indicator | Verified |
|---|---|
| bos_choch | ✅ |
| candle_classification | ✅ |
| liquidity_fvg | ✅ |
| liquidity_pools | ✅ |
| liquidity_sweeps | ✅ |
| market_structure_breaks | ✅ |
| order_block | ✅ |
| pivot_sets | ✅ |
| pivots_structure | ✅ |
| wyckoff | ✅ |
| zigzag | ✅ |
| zigzag_advanced | ✅ |
| zone_rejection | ✅ |
| double_pattern | ✅ |
| pivot_divergence | ✅ |
| oscillator_pivot_divergence | ✅ |
| wavetrend_divergence | ✅ |
| stoch_rsi_divergence | ✅ |
| bottom_formation | ✅ |
| price_average_relation | ✅ |
| leg_conviction | ✅ |
| trend_break_risk | ✅ |
| sweep_reversal_score | ✅ |
| equal_level_liquidity | ✅ |
| leg_character | ✅ |
| accumulation_phase | ✅ |
| reversal_setup_score | ✅ |
| structure_zones | ✅ |
| structural_flow | ✅ |
| compression_zones | ✅ |
| tradability_filter | ✅ |
| market_state_chain | ✅ |

---

**Total: 132/132 verified** — the same 132 names `catalog()` returns: 96 indicators (the first
five groups), 4 composite scores and 32 structure/pattern detectors.
