# Indicator Status

As of: 2026-11-21

Overview of all indicators/detectors from `src/indicator/registry.rs`: name and whether it has
been verified (✅) or is still open (❌). Only the verification status — not what it was
verified against or how (details for that are maintained internally under `plan/`).

For every new indicator, add it here as ❌ by default and only flip it to ✅ once verification
is complete. The mark must match the `verification` of the entry's definition
(`Verification::Verified` for ✅, `Verification::Open` with a reason for ❌); a test checks it.

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
| rma | ✅ |
| alma | ✅ |
| jma | ✅ |
| kestrel_adaptive_smoother | ✅ |
| super_smoother | ✅ |
| ultimate_smoother | ✅ |
| mama | ✅ |
| zlema | ✅ |
| frama | ✅ |
| nadaraya_watson | ✅ |
| range_filter | ✅ |
| most | ✅ |
| ott | ✅ |
| halftrend | ✅ |
| ssl_channel | ✅ |
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
| kestrel_flow_exhaustion | ✅ |
| kestrel_overextension_risk | ✅ |
| roc_context | ✅ |
| kestrel_rsi_cycle_stats | ✅ |
| percent_rank | ✅ |
| roofing_filter | ✅ |
| reflex | ✅ |
| trendflex | ✅ |
| correlation_trend | ✅ |
| laguerre_rsi | ✅ |
| cyber_cycle | ✅ |
| schaff_trend_cycle | ✅ |
| qqe | ✅ |
| demarker | ✅ |
| relative_momentum_index | ✅ |
| squeeze_momentum | ✅ |

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
| parkinson | ✅ |
| rogers_satchell | ✅ |
| yang_zhang | ✅ |
| historical_volatility | ✅ |
| keltner | ✅ |
| mass_index | ✅ |
| parabolic_sar | ✅ |
| supertrend | ✅ |
| true_range | ✅ |
| ulcer_index | ✅ |
| vix_fix | ✅ |
| vortex | ✅ |
| kestrel_coil_release | ✅ |
| kestrel_reaction_speed | ✅ |

## Volume

| Indicator | Verified |
|---|---|
| twap | ✅ |
| vwap | ✅ |
| volume_profile | ✅ |
| tpo_profile | ✅ |
| volume_flow_indicator | ❌ |
| volume_zone_oscillator | ❌ |
| volume_price_confirmation | ❌ |
| market_facilitation | ❌ |
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
| kestrel_vwap_regime | ❌ |

## Trend

| Indicator | Verified |
|---|---|
| alligator | ✅ |
| efficiency | ✅ |
| fractal_dimension | ✅ |
| r_squared | ✅ |
| cycle_length | ✅ |
| variance_ratio | ✅ |
| autocorrelation | ✅ |
| hurst_rs | ✅ |
| ar1_half_life | ✅ |
| ichimoku | ✅ |
| midas | ✅ |
| trend_relationship | ✅ |
| kestrel_price_location | ✅ |
| zscore | ✅ |
| smoothed_supertrend | ✅ |

## Composite Scores

| Indicator | Verified |
|---|---|
| kestrel_trend_quality | ✅ |
| kestrel_buy_sell_pressure | ✅ |
| volatility_regime | ✅ |
| cusum | ✅ |
| kestrel_multi_factor | ✅ |

## Structure/Pattern Detectors

| Indicator | Verified |
|---|---|
| bos_choch | ✅ |
| candle_classification | ✅ |
| gap | ❌ |
| liquidity_fvg | ✅ |
| liquidity_pools | ✅ |
| liquidity_sweeps | ✅ |
| market_structure_breaks | ✅ |
| order_block | ✅ |
| pivot_sets | ✅ |
| pivots_structure | ✅ |
| point_and_figure | ❌ |
| wyckoff | ✅ |
| zigzag | ✅ |
| zigzag_advanced | ✅ |
| trendlines | ❌ |
| zone_rejection | ✅ |
| double_pattern | ✅ |
| pivot_divergence | ✅ |
| oscillator_pivot_divergence | ✅ |
| wavetrend_divergence | ✅ |
| stoch_rsi_divergence | ✅ |
| kestrel_bottom_formation | ✅ |
| kestrel_price_average_relation | ✅ |
| kestrel_leg_conviction | ✅ |
| kestrel_trend_break_risk | ✅ |
| kestrel_sweep_reversal_score | ✅ |
| kestrel_equal_level_liquidity | ✅ |
| kestrel_leg_character | ✅ |
| kestrel_accumulation_phase | ✅ |
| kestrel_reversal_setup_score | ✅ |
| kestrel_structure_zones | ✅ |
| kestrel_structural_flow | ✅ |
| kestrel_compression_zones | ✅ |
| kestrel_tradability_filter | ✅ |
| kestrel_market_state_chain | ✅ |
| period_midpoint | ✅ |
| opening_range | ✅ |
| kestrel_swing_structure | ❌ |
| fvg_fill | ❌ |
| chart_patterns | ❌ |
| harmonic_patterns | ❌ |
| setup_countdown | ✅ |
| narrow_range | ✅ |
| williams_fractals | ✅ |

## Other Input Kinds

Entries of `catalog()` that do not read one bar of one series per step, each with its own
builder: two series (`build_pair_checked`), candle transform (`build_transform_checked`) and
intrabar (`build_intrabar_checked`).

| Indicator | Verified |
|---|---|
| correlation | ❌ |
| relative_strength | ❌ |
| rolling_beta | ✅ |
| pair_spread | ✅ |
| heikin_ashi | ✅ |
| intrabar_cvd | ❌ |

---

**Total: 180/195 verified** — the same 195 names `catalog()` returns.
