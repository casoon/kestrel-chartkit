---
title: Market telemetry
description: compute_market_telemetry reads six descriptive dimensions off a bar series — order, tension, stress, stretch, open levels and spread load — from catalog entries that already exist.
order: 3
---

## Why a snapshot of measurements

Many catalog entries are better instruments than triggers. The fractal dimension does not say
where to enter; it says whether the path right now looks like a line or like noise. The Vix Fix
does not say "buy"; it says that the low sits unusually far under the recent closes. The
telemetry collects such read-outs for the last bar of a series into one structure, so a caller can
gate, size or annotate whatever it does with them.

```rust
use kestrel_chartkit::telemetry::compute_market_telemetry;

let t = compute_market_telemetry(&bars, spread_points, value_per_point);
if !t.thermometer.trend_model_active { /* the path reads as noise */ }
```

`spread_points` is the quoted spread and `value_per_point` the price units one quoted point stands
for (`1.0` when the spread is already in price units). `compute_market_telemetry_with` takes a
`TelemetryConfig` with every threshold.

## The six dimensions

| Dimension | Question | Built from | Status |
| --- | --- | --- | --- |
| `thermometer` | ordered path or noise? | `fractal_dimension`, `choppiness`, `kestrel_swing_structure` | `LAMINAR`, `TRANSITIONAL`, `TURBULENT_CHAOS` |
| `barometer` | how tense are the ranges? | `candle_classification` (wick share), `rogers_satchell`, `atr`, `narrow_range` (NR4/NR7), `kestrel_compression_zones` | `DORMANT`, `COMPRESSED_COIL`, `TENSION_ELEVATED` |
| `seismometer` | forced selling at lows, forced buying at tops? | `vix_fix` and its inverse | `NONE`, `W1_PANIC`, `W2_RECOVERY` per side |
| `ekg` | stretched or fading? | `ema`, `atr`, `pivot_divergence` (RSI) | `HEALTHY_PULSE`, `OVEREXTENDED`, `DIVERGENCE_EXHAUSTION` |
| `landkarte` | which levels are unfinished? | open `liquidity_fvg` zones, `volume_profile` or `tpo_profile` | nearest level above/below, safe stop levels |
| `kostengitter` | how much of a bar does the spread eat? | the spread, `atr` | `GREEN`, `YELLOW`, `RED` |

Every status may also be `UNKNOWN`: a series too short for an entry, no spread, or no ATR yet.
Nothing is substituted — a volume profile is used only when every bar of its window carries
volume, otherwise the time-at-price profile, and the result says which (`poc_source`).

## Default thresholds

| Field | Default | Meaning |
| --- | --- | --- |
| `laminar_fdi_max` | 1.30 | fractal dimension below it: ordered |
| `turbulent_fdi_min`, `turbulent_chop_min` | 1.48, 61.8 | both above: noise |
| `tension_wick_share_min` | 0.65 | mean share of the range outside the body (10 bars) |
| `coil_min_bars` | 12 | bars in a compression zone that read as a coil |
| `overextension_atr` | 2.5 | distance of the close from the EMA 50, in ATR |
| `recovery_bars` | 5 | bars after a spike that read as recovery |
| `green_spread_atr_max`, `yellow_spread_atr_max` | 0.03, 0.08 | spread ÷ ATR |

They are the usual textbook marks, not fitted values. The boolean fields that name an action —
`trend_model_active`, `long_veto_active`, `short_veto_active`, `fomo_warning`,
`tighten_stops_command`, `recommended_trailing_atr` — describe the threshold crossing. Whether
acting on it pays is a question for a backtest on your own data, not a property of this function.

## Long and short alike

Each one-sided read-out has its mirror: the panic at lows next to the blow-off at tops, the safe
stop below support next to the one above resistance, the chase warning above and below the
average, a bearish divergence above the average next to a bullish one below it. A wide bar that
lifts the Vix Fix and its inverse at once counts for the side whose value exceeds its band by the
larger factor. The tests check this on the arithmetic mirror (`2m − p`) and, for the Vix Fix, on
the log mirror (`c / p`).

## Causal and deterministic

The snapshot describes the last bar of `bars` and reads nothing after it; every indicator is fed
bar by bar from the first bar on. Two calls with the same input return the same result.
