# Indicator Status

As of: 2026-09-04

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
| bollinger | ✅ |
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
| ppo | ✅ |
| roc | ✅ |
| rvi | ✅ |
| ultimate_oscillator | ✅ |
| wavetrend | ✅ |

## Volatility

| Indicator | Verified |
|---|---|
| atr | ✅ |
| adx | ✅ |
| aroon | ✅ |
| chandelier_exit | ✅ |
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
| vix_fix | ✅ |
| vortex | ✅ |

## Volume

| Indicator | Verified |
|---|---|
| vwap | ✅ |
| volume_profile | ✅ |
| acc_dist | ✅ |
| anchored_vwap | ✅ |
| cmf | ✅ |
| cvd | ✅ |
| eom | ✅ |
| extended_volume_profile | ✅ |
| hires_volume_flow | ✅ |
| klinger | ✅ |
| nvi | ✅ |
| obv | ✅ |
| persistent_volume_profile | ✅ |
| pvi | ✅ |
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

---

**Total: 89/89 verified.**
