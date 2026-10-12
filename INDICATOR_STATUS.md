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
| market_exhaustion | ✅ |
| exhaustion_scanner | ✅ |
| roc_advanced | ✅ |
| oscillator_cycle_statistics | ✅ |

## Volatility

| Indicator | Verified |
|---|---|
| atr | ✅ |
| adx | ✅ |
| aroon | ✅ |
| chande_kroll | ✅ |
| chandelier_exit | ✅ |
| chandelier_flip_radar | ✅ |
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
| compression_fractal_release | ✅ |
| time_to_react | ✅ |

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
| smooth_trend_radar | ✅ |

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
| candle_story | ✅ |
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
| bottom_formation | ✅ |
| market_average_relationship | ✅ |
| swing_conviction_radar | ✅ |
| structure_break_risk | ✅ |
| reversal_engine_score | ✅ |
| liquidity_hunter | ✅ |
| market_motion_dna | ✅ |
| vein_accumulation | ✅ |
| vein_reversal_score | ✅ |
| vein_structure_zones | ✅ |
| structural_wave_field | ✅ |
| coilforge_zones | ✅ |
| market_tradability_engine | ✅ |
| markov_state_engine | ✅ |

---

**Total: 130/130 verified** — the same 130 names `catalog()` returns: 96 indicators (the first
five groups), 4 composite scores and 30 structure/pattern detectors.
