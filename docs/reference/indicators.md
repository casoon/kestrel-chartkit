---
title: Indicator reference
description: All 130 indicators of the registry with their parameters, output unit and range, generated from the indicator definitions.
order: 2
---

> **Generated, and still moving.** This page is rendered from the indicator definitions in
> `src/indicator/definition/` by `tests/indicator_reference_doc.rs`, so it always matches the
> code of this version. The crate is pre-1.0 and under active development: names, parameters
> and outputs can still change fundamentally between versions.

Every indicator is built by name with `build_checked` (an `f64` parameter map) or `build_typed` (typed values, including choices by name and a `source`). Both validate against the same definition: unknown keys, values outside the accepted range and broken relations are errors, missing parameters take their default. **Warmup** is the bar of the first output with the default parameters. **Needs real volume** marks indicators whose result is only meaningful with traded volume. The [showcase](../../../showcase/) draws every indicator with its defaults on synthetic bars of a fictitious CASOON share.

## Moving averages, bands and trend lines

### `sma`

Simple Moving Average

[See it in the showcase](../../../showcase/sma/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `anchored_vwap` · alias `avwap`

Anchored VWAP Engine

[See it in the showcase](../../../showcase/anchored_vwap/)

**Output** unit Price, range not declared · **warmup** 1 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `anchor_kind` | `session` | `session`, `day`, `week`, `month`, `external`, `manual_timestamp` (typed map only) | where the average restarts |
| `anchor_timestamp` | — | Unix seconds (typed map only) | the anchor for anchor_kind=manual_timestamp |
| `zero_volume_policy` | `equal_weight` | `equal_weight`, `skip` (typed map only) | how a bar without volume enters the average |
| `mult1` | 1 | 0.01 – 100 | inner band, in standard deviations |
| `mult2` | 2 | 0.01 – 100 | outer band, in standard deviations |

### `ema`

Exponential Moving Average (build_typed accepts init=first_sample|sma for the seed; first_sample is the default)

[See it in the showcase](../../../showcase/ema/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `init` | `first_sample` | `first_sample`, `sma` (typed map only) | seed of the average: the first sample, or the SMA of the first period samples |
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `wma`

Weighted Moving Average

[See it in the showcase](../../../showcase/wma/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `vwma`

Volume-Weighted Moving Average

[See it in the showcase](../../../showcase/vwma/)

**Output** unit Price, range not declared · **warmup** 20 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `hma`

Hull Moving Average

[See it in the showcase](../../../showcase/hma/)

**Output** unit Price, range not declared · **warmup** 23 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `dema`

Double Exponential Moving Average

[See it in the showcase](../../../showcase/dema/)

**Output** unit Price, range not declared · **warmup** 39 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged |

### `tema`

Triple Exponential Moving Average

[See it in the showcase](../../../showcase/tema/)

**Output** unit Price, range not declared · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | length of each of the three chained EMAs |

### `kama`

Kaufman's Adaptive Moving Average

[See it in the showcase](../../../showcase/kama/)

**Output** unit Price, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 10 | whole number 1 – 10000 | bars of the efficiency ratio |
| `fast_period` | 2 | whole number 1 – 10000 | EMA length of the fastest smoothing (efficiency 1) |
| `slow_period` | 30 | whole number 1 – 10000 | EMA length of the slowest smoothing (efficiency 0) |

Requires `fast_period` to be smaller than `slow_period`.

### `lsma`

Least Squares Moving Average / Linear Regression

[See it in the showcase](../../../showcase/lsma/)

**Output** unit Price, range not declared · **warmup** 25 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 25 | whole number 2 – 10000 | bars in the regression window |

### `mcginley`

McGinley Dynamic Moving Average

[See it in the showcase](../../../showcase/mcginley/)

**Output** unit Price, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | nominal length of the dynamic average |

### `t3`

Tillson T3 (weighted combination of six chained EMAs; v is a shape factor, not volume)

[See it in the showcase](../../../showcase/t3/)

**Output** unit Price, range not declared · **warmup** 5 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 5 | whole number 1 – 10000 | length of each of the six chained EMAs |
| `v` | 0.7 | 0 – 1 | shape factor weighting the stages (0 is a triple EMA of EMAs, higher reduces lag) |

### `vidya`

Variable Index Dynamic Average (CMO-scaled exponential smoothing)

[See it in the showcase](../../../showcase/vidya/)

**Output** unit Price, range not declared · **warmup** 10 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `cmo_len` | 9 | whole number 1 – 10000 | bars over which the directionality (CMO) is measured |
| `ema_len` | 12 | whole number 1 – 10000 | base exponential smoothing that the CMO scales |

### `alligator`

Williams Alligator (Jaw, Teeth, Lips)

[See it in the showcase](../../../showcase/alligator/)

**Output** unit Price, range not declared · **warmup** 13 bars

### `envelope`

Moving Average Envelopes

[See it in the showcase](../../../showcase/envelope/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars in the SMA basis |
| `percent` | 2.5 | 0.01 – 100 | band distance from the basis, in percent |

### `vwap`

Rolling Volume Weighted Average Price with sigma bands and slope

[See it in the showcase](../../../showcase/vwap/)

**Output** unit Price, range not declared · **warmup** 1 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `window` | 390 | whole number 1 – 10000 | bars in the rolling average |
| `slope_lookback` | 20 | whole number 1 – 10000 | bars back for the slope |

### `trend_relationship`

Adaptive trend relationship between two configurable smoothers (build via build_typed with fast_kind/slow_kind params: ema|sma|rma|alma|jma)

[See it in the showcase](../../../showcase/trend_relationship/)

**Output** unit Unitless, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_kind` | `ema` | `ema`, `sma`, `rma`, `alma`, `jma` (typed map only) | smoother of the fast line |
| `slow_kind` | `ema` | `ema`, `sma`, `rma`, `alma`, `jma` (typed map only) | smoother of the slow line |
| `fast_len` | 9 | whole number 1 – 10000 | length of the fast smoother |
| `slow_len` | 21 | whole number 1 – 10000 | length of the slow smoother |

Requires `fast_len` to be smaller than `slow_len`.

### `ichimoku`

Ichimoku Kinko Hyo Cloud

[See it in the showcase](../../../showcase/ichimoku/)

**Output** unit Price, range not declared · **warmup** 52 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `tenkan_p` | 9 | whole number 1 – 10000 | bars in the conversion line (Tenkan-sen) midpoint |
| `kijun_p` | 26 | whole number 1 – 10000 | bars in the base line (Kijun-sen) midpoint, also the cloud displacement |
| `senkou_b_p` | 52 | whole number 1 – 10000 | bars in the leading span B (Senkou Span B) midpoint |

Requires `tenkan_p` to be smaller than `kijun_p`.

Requires `kijun_p` to be smaller than `senkou_b_p`.

### `supertrend`

Supertrend ATR Trailing Stop

[See it in the showcase](../../../showcase/supertrend/)

**Output** unit Price, range not declared · **warmup** 10 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 10 | whole number 1 – 10000 | bars in the ATR |
| `multiplier` | 3 | 0.01 – 100 | band distance from hl2, in ATRs |

### `parabolic_sar`

Parabolic SAR

[See it in the showcase](../../../showcase/parabolic_sar/)

**Output** unit Price, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `step` | 0.02 | 0.001 – 1 | start and increment of the acceleration factor |
| `max_step` | 0.2 | 0.001 – 1 | cap of the acceleration factor |

### `donchian`

Donchian Channels

[See it in the showcase](../../../showcase/donchian/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars searched for the highest high and lowest low |

### `keltner`

Keltner Channels

[See it in the showcase](../../../showcase/keltner/)

**Output** unit Price, range not declared · **warmup** 10 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ema_period` | 20 | whole number 1 – 10000; also accepted as `ma_period` | bars in the EMA basis |
| `atr_period` | 10 | whole number 1 – 10000 | bars in the ATR |
| `multiplier` | 2 | 0.01 – 100 | band distance from the basis, in ATRs |

### `bollinger` · alias `bb`

Bollinger Bands (build_typed accepts variance=population|sample for the standard-deviation divisor; population is the default)

[See it in the showcase](../../../showcase/bollinger/)

**Output** unit Price, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `variance` | `population` | `population`, `sample` (typed map only) | divisor of the standard deviation: N (population) or N - 1 (sample) |
| `len` | 20 | whole number 1 – 10000 | bars in the SMA basis and the standard deviation |
| `mult` | 2 | 0.01 – 100 | band distance from the basis, in standard deviations |

### `twap`

Anchored Time Weighted Average Price (no volume; build_typed accepts weighting=per_bar|by_duration and anchor=continuous|daily|manual_timestamp)

[See it in the showcase](../../../showcase/twap/)

**Output** unit Price, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `weighting` | `per_bar` | `per_bar`, `by_duration` (typed map only) | each bar counts once, or by its duration |
| `day_start_offset` | 0 | -86400 – 86400 | seconds after midnight UTC at which a daily anchor restarts |
| `anchor` | `daily` | `continuous`, `daily`, `manual_timestamp` (typed map only) | where the average restarts |
| `anchor_timestamp` | — | Unix seconds (typed map only) | the anchor for anchor=manual_timestamp |

### `midas`

MIDAS launch-anchored curve with Topfinder/Bottomfinder projection (build_typed with mode=topfinder|bottomfinder)

[See it in the showcase](../../../showcase/midas/)

**Output** unit Price, range not declared · **warmup** 1 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `mode` | `topfinder` | `topfinder`, `bottomfinder` (typed map only) | project from the last high (topfinder) or low (bottomfinder) |
| `source` | `hlc3` | a price source (typed map only) | the price the curve averages |
| `maturity_bars` | 20 | whole number 1 – 10000 | bars after an extreme until the curve counts as exhausted |

## Oscillators

### `macd`

Moving Average Convergence Divergence

[See it in the showcase](../../../showcase/macd/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 26 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_len` | 12 | whole number 1 – 10000 | bars in the fast EMA of the close |
| `slow_len` | 26 | whole number 1 – 10000 | bars in the slow EMA of the close |
| `signal_len` | 9 | whole number 1 – 10000 | bars in the EMA of the MACD line (signal) |

Requires `fast_len` to be smaller than `slow_len`.

### `pmo`

Price Momentum Oscillator (twice-smoothed one-bar return with alpha = 2/length, scaled by ten)

[See it in the showcase](../../../showcase/pmo/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 55 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `length_1` | 35 | whole number 1 – 10000 | length of the first smoothing stage (alpha = 2/length) |
| `length_2` | 20 | whole number 1 – 10000 | length of the second smoothing stage (alpha = 2/length) |
| `signal_len` | 10 | whole number 1 – 10000 | bars in the EMA of the PMO line (signal) |

### `smi`

Stochastic Momentum Index (double-smoothed position of the close relative to the midpoint of the high-low range)

[See it in the showcase](../../../showcase/smi/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 10 | whole number 1 – 10000 | bars in the high-low window |
| `smooth_1` | 3 | whole number 1 – 10000 | bars in the first EMA stage |
| `smooth_2` | 3 | whole number 1 – 10000 | bars in the second EMA stage |
| `signal_len` | 3 | whole number 1 – 10000 | bars in the EMA of the SMI line (signal) |

### `rci`

Rank Correlation Index (rank of prices against rank of time, scaled by 100)

[See it in the showcase](../../../showcase/rci/)

**Output** unit Ratio, range -100 – 100 · **warmup** 9 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 9 | whole number 2 – 10000 | bars ranked against time |

### `trix`

TRIX (rate of change of a triple-smoothed price series, in percent)

[See it in the showcase](../../../showcase/trix/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 16 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 15 | whole number 1 – 10000 | bars in each of the three EMAs |
| `signal_len` | 9 | whole number 1 – 10000 | bars in the EMA of the TRIX line (signal) |

### `tsi`

True Strength Index

[See it in the showcase](../../../showcase/tsi/)

**Output** unit Ratio, range -100 – 100 · **warmup** 2 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `long_len` | 25 | whole number 1 – 10000 | bars in the first (long) EMA of the momentum |
| `short_len` | 13 | whole number 1 – 10000 | bars in the second (short) EMA of the momentum |
| `sig_len` | 7 | whole number 1 – 10000 | bars in the average of the TSI line (signal) |

Requires `short_len` to be smaller than `long_len`.

### `stochastic`

Classic Stochastic Oscillator

[See it in the showcase](../../../showcase/stochastic/)

**Output** unit Ratio, range 0 – 100 · **warmup** 16 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `k_period` | 14 | whole number 1 – 10000 | bars in the high-low window of %K |
| `d_period` | 3 | whole number 1 – 10000 | bars in the SMA of %K (%D) |

### `roc`

Rate of Change / Momentum

[See it in the showcase](../../../showcase/roc/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 13 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 12 | whole number 1 – 10000 | bars back to the compared close |

### `ultimate_oscillator`

Ultimate Oscillator

[See it in the showcase](../../../showcase/ultimate_oscillator/)

**Output** unit Ratio, range 0 – 100 · **warmup** 29 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period1` | 7 | whole number 1 – 10000 | bars in the short window (weight 4) |
| `period2` | 14 | whole number 1 – 10000 | bars in the middle window (weight 2) |
| `period3` | 28 | whole number 1 – 10000 | bars in the long window (weight 1) |

Requires `period1` to be smaller than `period2`.

Requires `period2` to be smaller than `period3`.

### `awesome_oscillator`

Awesome Oscillator

[See it in the showcase](../../../showcase/awesome_oscillator/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 34 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_period` | 5 | whole number 1 – 10000 | bars in the fast SMA of the median price |
| `slow_period` | 34 | whole number 1 – 10000 | bars in the slow SMA of the median price |

Requires `fast_period` to be smaller than `slow_period`.

### `ppo`

Percentage Price Oscillator

[See it in the showcase](../../../showcase/ppo/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 26 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_period` | 12 | whole number 1 – 10000 | bars in the fast EMA of the close |
| `slow_period` | 26 | whole number 1 – 10000 | bars in the slow EMA of the close |
| `signal_period` | 9 | whole number 1 – 10000 | bars in the EMA of the PPO line (signal) |

Requires `fast_period` to be smaller than `slow_period`.

### `cmo`

Chande Momentum Oscillator

[See it in the showcase](../../../showcase/cmo/)

**Output** unit Ratio, range -100 – 100 · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | changes of the close summed |

### `elder_ray`

Elder Ray Index (Bull/Bear Power)

[See it in the showcase](../../../showcase/elder_ray/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 13 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 13 | whole number 1 – 10000 | bars in the EMA of the close |

### `connors_rsi`

Connors RSI

[See it in the showcase](../../../showcase/connors_rsi/)

**Output** unit Ratio, range 0 – 100 · **warmup** 6 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `rsi_len` | 3 | whole number 1 – 10000 | bars in the price RSI |
| `streak_len` | 2 | whole number 1 – 10000 | bars in the streak RSI |
| `rank_len` | 100 | whole number 1 – 10000 | window of the percentile rank (has no effect: the rank uses the last 100 returns) |

### `coppock`

Coppock Curve

[See it in the showcase](../../../showcase/coppock/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 24 bars

### `dpo`

Detrended Price Oscillator

[See it in the showcase](../../../showcase/dpo/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 21 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 21 | whole number 2 – 10000 | bars in the SMA; the close is taken period/2 bars back |

### `kst`

Know Sure Thing Oscillator

[See it in the showcase](../../../showcase/kst/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 53 bars

### `rvi`

Relative Vigor Index

[See it in the showcase](../../../showcase/rvi/)

**Output** unit Ratio, range not declared · **warmup** 13 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 10 | whole number 1 – 10000 | bars in the averages of the vigor |

### `bop`

Balance of Power

[See it in the showcase](../../../showcase/bop/)

**Output** unit Ratio, range -1 – 1 · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars in the SMA of the per-bar power |

### `bbtrend`

BBTrend (how far a short Bollinger set has moved out of a long one, in percent of the short basis; build_typed accepts variance=population|sample for both sets)

[See it in the showcase](../../../showcase/bbtrend/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 50 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `variance` | `population` | `population`, `sample` (typed map only) | variance convention of both band sets |
| `short_len` | 20 | whole number 1 – 10000 | bars in the short Bollinger set |
| `long_len` | 50 | whole number 1 – 10000 | bars in the long Bollinger set |
| `mult` | 2 | 0.01 – 100 | band width in standard deviations, both sets |

Requires `short_len` to be smaller than `long_len`.

### `zscore`

Rolling Z-Score Engine

[See it in the showcase](../../../showcase/zscore/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 2 – 10000 | closes in the mean and population standard deviation |

### `rsi`

Relative Strength Index (build_typed accepts smoothing=wilder|ema for the up/down averages; wilder is the default)

[See it in the showcase](../../../showcase/rsi/)

**Output** unit Ratio, range 0 – 100 · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `smoothing` | `wilder` | `wilder`, `ema` (typed map only) | how the up and down moves are averaged |
| `rsi_len` | 14 | whole number 1 – 10000 | changes in the RSI average |
| `avg_len` | 3 | whole number 1 – 10000 | EMA over the raw RSI (the line) |
| `sig_len` | 3 | whole number 1 – 10000 | EMA over the line (signal) |
| `overbought` | 70 | 0 – 100; threshold on the output scale | upper zone, alerts above it |
| `oversold` | 30 | 0 – 100; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `stoch_rsi` · alias `srsi`

Stochastic RSI

[See it in the showcase](../../../showcase/stoch_rsi/)

**Output** unit Ratio, range 0 – 100 · **warmup** 32 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `rsi_len` | 14 | whole number 1 – 10000 | changes in the RSI |
| `stoch_len` | 14 | whole number 1 – 10000 | RSI values in the stochastic window |
| `k_len` | 3 | whole number 1 – 10000 | smoothing of %K |
| `d_len` | 3 | whole number 1 – 10000 | smoothing of %D |
| `overbought` | 80 | 0 – 100; threshold on the output scale | upper zone, alerts above it |
| `oversold` | 20 | 0 – 100; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `cci`

Commodity Channel Index (value is the EMA(avg_len) of the raw CCI; avg_len = 1 gives the raw line)

[See it in the showcase](../../../showcase/cci/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `cci_len` | 20 | whole number 1 – 10000 | bars in the CCI window |
| `avg_len` | 3 | whole number 1 – 10000 | EMA over the raw CCI (the line) |
| `overbought` | 100 | -1000 – 1000; threshold on the output scale | upper zone, alerts above it |
| `oversold` | -100 | -1000 – 1000; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `mfi`

Money Flow Index (value is the EMA(avg_len) of the raw MFI; avg_len = 1 gives the raw line)

[See it in the showcase](../../../showcase/mfi/)

**Output** unit Ratio, range 0 – 100 · **warmup** 14 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `mfi_len` | 14 | whole number 1 – 10000 | bars of money flow |
| `avg_len` | 3 | whole number 1 – 10000 | EMA over the raw MFI (the line) |
| `overbought` | 80 | 0 – 100; threshold on the output scale | upper zone, alerts above it |
| `oversold` | 20 | 0 – 100; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `williams_r` · alias `wpr`

Williams %R (0..100 convention; value is the EMA(avg_len) of the raw line, avg_len = 1 gives the raw line)

[See it in the showcase](../../../showcase/williams_r/)

**Output** unit Ratio, range 0 – 100 · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `wpr_len` | 14 | whole number 1 – 10000 | bars in the high/low window |
| `avg_len` | 3 | whole number 1 – 10000 | EMA over the raw %R (the line) |
| `overbought` | 80 | 0 – 100; threshold on the output scale | upper zone, alerts above it |
| `oversold` | 20 | 0 – 100; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `fisher_transform` · alias `fisher`

Fisher Transform (value is the EMA(avg_len) of the raw transform; avg_len = 1 gives the raw line)

[See it in the showcase](../../../showcase/fisher_transform/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 10 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fish_len` | 10 | whole number 1 – 10000 | bars in the normalising window |
| `avg_len` | 2 | whole number 1 – 10000 | EMA over the raw transform (the line) |
| `overbought` | 1.5 | -100 – 100; threshold on the output scale | upper zone, alerts above it |
| `oversold` | -1.5 | -100 – 100; threshold on the output scale | lower zone, alerts below it |

Requires `oversold` to be smaller than `overbought`.

### `wavetrend` · alias `wt`

WaveTrend Oscillator (wt1, wt2)

[See it in the showcase](../../../showcase/wavetrend/)

**Output** unit Ratio, range open, centred on 0 · **warmup** 4 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `n1` | 10 | whole number 1 – 10000 | channel length |
| `n2` | 21 | whole number 1 – 10000 | average length |
| `ob_level` | 60 | -100 – 100; threshold on the output scale | upper zone, alerts above it |
| `os_level` | -60 | -100 – 100; threshold on the output scale | lower zone, alerts below it |

## Volatility and trend strength

### `atr`

Average True Range (true-range average: build_typed accepts smoothing=rma|sma|ema|wma, build_checked the same as smoothing=0|1|2|3; rma is the default and the signal line stays Wilder-smoothed)

[See it in the showcase](../../../showcase/atr/)

**Output** unit Ratio, range 0 and above · **warmup** 33 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `smoothing` | `rma` | `rma`, `sma`, `ema`, `wma` (f64 map: 0 – 3) | how the true range is averaged |
| `atr_len` | 14 | whole number 1 – 10000 | bars in the true-range average |
| `sig_len` | 20 | whole number 1 – 10000 | bars in the Wilder average of the percentage (signal) |

### `adx`

Average Directional Index

[See it in the showcase](../../../showcase/adx/)

**Output** unit Ratio, range 0 – 100 · **warmup** 28 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `di_len` | 14 | whole number 1 – 10000 | bars in the Wilder smoothing of the directional movement (+DI/-DI) |
| `adx_smooth` | 14 | whole number 1 – 10000 | bars in the Wilder smoothing of DX into the ADX |
| `level_weak` | 20 | 0 – 100; threshold on the output scale | ADX level below which the trend counts as weak |

### `dmi`

Directional Movement Index (+DI / -DI)

[See it in the showcase](../../../showcase/dmi/)

**Output** unit Ratio, range -100 – 100 · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars in the Wilder smoothing of the directional movement |

### `aroon`

Aroon Indicator (Up, Down, Oscillator)

[See it in the showcase](../../../showcase/aroon/)

**Output** unit Ratio, range -100 – 100 · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars searched for the latest high and low |

### `vortex` · alias `vi`

Vortex Indicator (+VI, -VI)

[See it in the showcase](../../../showcase/vortex/)

**Output** unit Ratio, range 0 and above · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars summed for the vortex movements and the true range |

### `choppiness` · alias `chop`

Choppiness Index

[See it in the showcase](../../../showcase/choppiness/)

**Output** unit Ratio, range 0 – 100 · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 2 – 10000 | bars whose true ranges are compared with their overall range |

### `relative_volatility`

Relative Volatility Index (the RSI construction applied to the standard deviation; build_typed accepts variant=close|high_low). Unrelated to "rvi", the Relative Vigor Index

[See it in the showcase](../../../showcase/relative_volatility/)

**Output** unit Ratio, range 0 – 100 · **warmup** 23 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `variant` | `close` | `close`, `high_low` (typed map only) | measured on the close alone, or on highs and lows separately and averaged |
| `stdev_len` | 10 | whole number 2 – 10000 | bars in the standard deviation |
| `smooth_len` | 14 | whole number 1 – 10000 | bars in the smoothing of the up and down deviations |

### `ulcer_index`

Ulcer Index (root mean square of the percentage drawdowns below the running high)

[See it in the showcase](../../../showcase/ulcer_index/)

**Output** unit Ratio, range 0 and above · **warmup** 27 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 14 | whole number 1 – 10000 | bars in the running high and in the mean of the squared drawdowns |

### `true_range`

True Range in price units

[See it in the showcase](../../../showcase/true_range/)

**Output** unit Unitless, range 0 and above · **warmup** 1 bars

### `historical_volatility`

Annualized Historical Volatility

[See it in the showcase](../../../showcase/historical_volatility/)

**Output** unit Ratio, range 0 and above · **warmup** 21 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars in the standard deviation of the log returns |

### `garman_klass`

Garman-Klass Volatility Estimator

[See it in the showcase](../../../showcase/garman_klass/)

**Output** unit Ratio, range 0 and above · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars averaged in the open-high-low-close variance estimate |

### `mass_index`

Mass Index Reversal Detector

[See it in the showcase](../../../showcase/mass_index/)

**Output** unit Ratio, range 0 and above · **warmup** 25 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 25 | whole number 1 – 10000 | number of EMA ratios summed |

### `chande_kroll` · alias `cks`

Chande Kroll Stop (window extremes minus/plus a Wilder-smoothed ATR, passed through a second extreme window)

[See it in the showcase](../../../showcase/chande_kroll/)

**Output** unit Price, range not declared · **warmup** 18 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `atr_len` | 10 | whole number 1 – 10000 | bars in the first extreme window and the Wilder ATR |
| `stop_len` | 9 | whole number 1 – 10000 | bars in the second extreme window over the preliminary stops |
| `mult` | 3 | 0.01 – 100 | ATR multiple between the window extreme and the preliminary stop |

### `chandelier_exit` · alias `ce`

Chandelier Exit (ATR trailing stop with direction flip)

[See it in the showcase](../../../showcase/chandelier_exit/)

**Output** unit Price, range not declared · **warmup** 22 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `length` | 22 | whole number 1 – 10000 | bars in the highest-high/lowest-low lookback and the ATR |
| `atr_mult` | 3 | 0.01 – 100 | ATR multiple between the extreme and the stop |

### `adaptive_chandelier` · alias `chfr`

Chandelier Exit ratchet extended with adaptive multiplier, body-filtered weak flips, and bull/bear trap detection (use AdaptiveChandelierEngine::new directly for use_close_extremes=false or simple_adaptive=true; this f64-only entry uses the documented defaults for both)

[See it in the showcase](../../../showcase/adaptive_chandelier/)

**Output** unit Price, range not declared · **warmup** 30 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `length` | 30 | whole number 1 – 10000 | bars in the highest/lowest lookback and the internal ATR |
| `atr_mult` | 4.5 | 0.01 – 100 | ATR multiple between the extreme and the stop |
| `body_filter_atr` | 0.8 | 0 – 100 | minimum candle body, in ATRs, for a direction flip to count as real (0 disables the filter) |
| `danger_dist_atr` | 0.35 | 0 – 100 | distance from the stop, in ATRs, below which the risk state is danger |
| `warn_dist_atr` | 0.75 | 0 – 100 | distance from the stop, in ATRs, below which the risk state is warning |

### `vix_fix` · alias `wvf`

Williams Vix Fix volatility spike detector: panic lows (WVF, alert panic_bottom) and, mirrored, blow-off tops (inverse VIX Fix, extra inverse/inverse_band, alert blowoff_top)

[See it in the showcase](../../../showcase/vix_fix/)

**Output** unit Ratio, range 0 and above · **warmup** 39 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pd` | 22 | whole number 1 – 10000 | bars in the highest-close (and lowest-close) lookback |
| `bband_len` | 20 | whole number 1 – 10000 | bars in the Bollinger band over the Vix Fix |
| `mult` | 2 | 0.01 – 100 | standard deviations of the upper band |

### `efficiency` · alias `leg_efficiency`, `er`

Kaufman-style leg efficiency ratio

[See it in the showcase](../../../showcase/efficiency/)

**Output** unit Ratio, range 0 – 100 · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 14 | whole number 1 – 10000 | bars between the two closes compared with the path travelled |

### `volatility_regime`

Volatility Regime & Squeeze Detector

[See it in the showcase](../../../showcase/volatility_regime/)

**Output** unit Unitless, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars in the Bollinger and Keltner sets |
| `bb_mult` | 2 | 0.01 – 100 | standard deviations of the Bollinger band |
| `kc_mult` | 1.5 | 0.01 – 100 | ATR multiple of the Keltner channel |

## Volume

### `pvt`

Price Volume Trend (volume weighted by the relative price change, accumulated)

[See it in the showcase](../../../showcase/pvt/)

**Output** unit Volume, range not declared · **warmup** 2 bars · needs real volume

### `efi`

Elder's Force Index (price change times volume, EMA-smoothed)

[See it in the showcase](../../../showcase/efi/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 14 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ema_len` | 13 | whole number 1 – 10000 | bars in the EMA of the raw force |

### `rvat`

Relative Volume at Time (volume against the same time of day on previous days, regular and cumulative)

[See it in the showcase](../../../showcase/rvat/)

**Output** unit Ratio, range 0 and above · **warmup** 1441 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `days` | 10 | whole number 1 – 1000 | previous days compared at the same time of day |
| `day_start_offset` | 0 | -86400 – 86400 | seconds after midnight UTC at which the day starts |
| `bar_seconds` | 60 | 1 – 86400 | bar spacing in seconds; only declares the warmup |

### `hires_volume_flow`

High-resolution volume flow with absorption detection (OHLC-estimated via this registry entry point; use HiResVolumeFlowEngine::on_bar_with_aggressor/on_intrabar_group directly for direct aggressor/intrabar-delta resolution)

[See it in the showcase](../../../showcase/hires_volume_flow/)

**Output** unit Volume, range not declared · **warmup** 1 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `window_len` | 20 | whole number 2 – 10000 | bars in the rolling volume threshold for absorption |

### `extended_volume_profile` · alias `vp_extended`

Full-bin price/volume profile with HVN/LVN/AVN classification, a delta profile, and zone formation (use ExtendedVolumeProfileEngine::on_intrabar_group directly for intrabar-resolution distribution)

[See it in the showcase](../../../showcase/extended_volume_profile/)

**Output** unit Price, range not declared · **warmup** 70 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 70 | whole number 1 – 10000 | bars in the profile window |
| `num_bins` | 30 | whole number 1 – 1000 | price bins across the window's range |

### `persistent_volume_profile` · alias `vp_persistent`

Fixed-price-grid volume profile with real bin lifecycle (birth/growth/expiry across updates) and a per-bin absorption profile

[See it in the showcase](../../../showcase/persistent_volume_profile/)

**Output** unit Price, range not declared · **warmup** 70 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 70 | whole number 1 – 10000 | bars in the profile window |
| `bin_width` | 1 | 0.000001 – 1000000 | fixed price-grid resolution, in price units |

### `volume_profile` · alias `vp`

Price/volume distribution profile with point-of-control

[See it in the showcase](../../../showcase/volume_profile/)

**Output** unit Price, range not declared · **warmup** 70 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 70 | whole number 1 – 10000 | bars in the profile window |
| `num_bins` | 30 | whole number 1 – 1000 | price bins across the window's range |

### `money_flow_profile` · alias `mfp`

Volume-by-price profile binned by dollar volume (volume x price) instead of raw volume, plus an aggregate bull/bear flow-bias percentage

[See it in the showcase](../../../showcase/money_flow_profile/)

**Output** unit Price, range not declared · **warmup** 2 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 200 | whole number 2 – 10000 | bars in the rolling window; a profile needs two |
| `rows` | 25 | whole number 1 – 1000 | bins across the window's high/low range |
| `va_pct` | 0.7 | 0 – 1 | share of total flow the value area captures |

### `volume`

Volume and Average Volume

[See it in the showcase](../../../showcase/volume/)

**Output** unit Volume, range 0 and above · **warmup** 1 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ma_period` | 20 | whole number 1 – 10000 | bars in the volume average |

### `rvol`

Relative Volume vs Moving Average

[See it in the showcase](../../../showcase/rvol/)

**Output** unit Ratio, range 0 and above · **warmup** 20 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars in the volume average compared against |

### `obv`

On-Balance Volume

[See it in the showcase](../../../showcase/obv/)

**Output** unit Volume, range not declared · **warmup** 1 bars · needs real volume

### `cmf`

Chaikin Money Flow

[See it in the showcase](../../../showcase/cmf/)

**Output** unit Ratio, range -1 – 1 · **warmup** 20 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 20 | whole number 1 – 10000 | bars summed for money flow and volume |

### `acc_dist`

Accumulation / Distribution Line

[See it in the showcase](../../../showcase/acc_dist/)

**Output** unit Volume, range not declared · **warmup** 1 bars · needs real volume

### `cvd`

Cumulative Volume Delta (Aufteilung aus der Bar-Form geschätzt, nicht aus klassifizierten Abschlüssen)

[See it in the showcase](../../../showcase/cvd/)

**Output** unit Volume, range not declared · **warmup** 1 bars · needs real volume

### `klinger` · alias `kvo`

Klinger Volume Force Oscillator

[See it in the showcase](../../../showcase/klinger/)

**Output** unit Volume, range open, centred on 0 · **warmup** 55 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_len` | 34 | whole number 1 – 10000 | bars in the fast EMA of the volume force |
| `slow_len` | 55 | whole number 1 – 10000 | bars in the slow EMA of the volume force |
| `signal_len` | 13 | whole number 1 – 10000 | bars in the EMA of the oscillator (signal) |

Requires `fast_len` to be smaller than `slow_len`.

### `eom`

Ease of Movement

[See it in the showcase](../../../showcase/eom/)

**Output** unit Unitless, range open, centred on 0 · **warmup** 15 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars in the SMA of the raw value |
| `volume_divisor` | 10000 | 1 – 1000000000 | rescales volume into a readable unit |

### `nvi`

Negative Volume Index

[See it in the showcase](../../../showcase/nvi/)

**Output** unit Unitless, range not declared · **warmup** 1 bars · needs real volume

### `pvi`

Positive Volume Index

[See it in the showcase](../../../showcase/pvi/)

**Output** unit Unitless, range not declared · **warmup** 1 bars · needs real volume

### `chaikin_oscillator` · alias `cho`

Chaikin Oscillator

[See it in the showcase](../../../showcase/chaikin_oscillator/)

**Output** unit Volume, range open, centred on 0 · **warmup** 10 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `fast_len` | 3 | whole number 1 – 10000 | bars in the fast EMA of the A/D line |
| `slow_len` | 10 | whole number 1 – 10000 | bars in the slow EMA of the A/D line |

Requires `fast_len` to be smaller than `slow_len`.

## Market structure and patterns

### `order_block` · alias `ob`

Order block detection from displacement candles (ATR-filtered)

[See it in the showcase](../../../showcase/order_block/)

**Output** unit Unitless, range not declared · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `atr_len` | 14 | whole number 1 – 10000 | bars in the ATR the displacement is measured against |
| `min_disp` | 1 | 0.01 – 100 | minimum displacement candle range, in ATR |

### `liquidity_fvg` · alias `fvg`, `smc`

Fair value gap (imbalance) detection

[See it in the showcase](../../../showcase/liquidity_fvg/)

**Output** unit Unitless, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 20 | whole number 1 – 10000 | bars kept for gap and sweep detection |

### `market_structure_breaks` · alias `bos`, `choch`

Break of structure / change of character (BOS/CHoCH) detection

[See it in the showcase](../../../showcase/market_structure_breaks/)

**Output** unit Unitless, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 5 | whole number 1 – 10000 | bars on each side of a swing high/low |

### `pivots_structure` · alias `pivots`

Swing pivot detection with a rolling structure score

[See it in the showcase](../../../showcase/pivots_structure/)

**Output** unit Unitless, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `left_bars` | 5 | whole number 1 – 10000 | bars left of a pivot |
| `right_bars` | 5 | whole number 1 – 10000 | bars right of a pivot (confirmation delay) |
| `score_window` | 10 | whole number 1 – 10000 | pivots in the rolling structure score |

### `candle_classification` · alias `pinbar`

Normalised candle classification — reports every pattern it recognises

[See it in the showcase](../../../showcase/candle_classification/)

**Output** unit Unitless, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pin_wick_min` | 0.55 | 0.1 – 0.95 | minimum wick share of the range for a pinbar |
| `pin_close_pos` | 0.65 | 0.5 – 1 | how far towards the opposite end the close must sit for a pinbar |
| `marubozu_body_min` | 0.82 | 0.5 – 1 | minimum body share of the range for a marubozu |
| `belt_hold_body_min` | 0.6 | 0.3 – 1 | minimum body share of the range for a belt hold |
| `belt_hold_open_wick_max` | 0.03 | 0 – 0.3 | maximum opening-side wick share of the range for a belt hold |
| `doji_body_max` | 0.08 | 0 – 0.4 | maximum body share of the range for a doji |
| `spinning_top_body_max` | 0.3 | 0.05 – 0.6 | maximum body share of the range for a spinning top |
| `hammer_wick_body_min` | 2 | 0.5 – 20 | wick length as a multiple of the body for hammer and inverted hammer |
| `hammer_opposite_max` | 0.5 | 0 – 5 | maximum opposite wick, as a multiple of the body, for hammer and inverted hammer |
| `tweezer_tolerance` | 0.0015 | 0 – 0.1 | relative tolerance for two highs or lows counting as equal (tweezer) |
| `min_range_atr` | 0.5 | 0 – 10 | minimum range, in ATR, before a shape is reported |
| `atr_len` | 14 | whole number 1 – 10000 | bars in the ATR behind min_range_atr |
| `trend_lookback` | 10 | whole number 1 – 10000 | bars used to determine the prior move |
| `trend_min_atr` | 1 | 0 – 100 | close move over the lookback, in ATR, that counts as a trend |

### `zigzag`

ZigZag Swing Leg Engine

[See it in the showcase](../../../showcase/zigzag/)

**Output** unit Price, range not declared · **warmup** 25 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `depth` | 12 | whole number 2 – 10000 | bars a swing point must dominate |
| `deviation_pct` | 5 | 0.01 – 100 | minimum price move, in percent, that starts a new leg |

### `pivot_sets` · alias `multi_pivots`

Multi-Pivot Set Engine

[See it in the showcase](../../../showcase/pivot_sets/)

**Output** unit Price, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_type` | `classic` | `classic`, `fibonacci`, `camarilla`, `woodie`, `demark`, `cpr` (typed map only) | which pivot formula the levels follow |

### `bos_choch`

BOS and CHoCH Market Structure Engine

[See it in the showcase](../../../showcase/bos_choch/)

**Output** unit Unitless, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 2 – 10000 | bars on each side of a pivot |

### `liquidity_sweeps` · alias `sweeps`

Liquidity Sweeps and EQH/EQL Detector

[See it in the showcase](../../../showcase/liquidity_sweeps/)

**Output** unit Unitless, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 2 – 10000 | bars on each side of a pivot |
| `tolerance_pct` | 0.2 | 0.01 – 100 | distance, in percent, within which two pivots count as equal highs/lows |

### `zone_rejection`

Failed breakout at a confirmed pivot-cluster zone: wick through support/resistance, close back; extra["wick"] is the wick tip (natural stop)

[See it in the showcase](../../../showcase/zone_rejection/)

**Output** unit Unitless, range not declared · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 1 – 10000 | bars on each side of a pivot |
| `atr_len` | 14 | whole number 1 – 10000 | bars in the ATR the zone widths are measured in |
| `tolerance_atr` | 0.25 | 0 – 100 | widening of a zone's band, in ATR, within which a pivot joins it |
| `max_width_atr` | 2 | 0 – 100 | widest a zone may grow, in ATR |
| `min_touches` | 2 | whole number 1 – 1000 | pivots that confirm a zone |

### `double_pattern`

Double bottom/top as a streaming event: forming on the second pivot (equal level within tolerance_atr, counter-swing ≥ min_depth_atr), confirmed on a close through the neckline; extra: neckline, target (measured move), wick (invalidation, natural stop)

[See it in the showcase](../../../showcase/double_pattern/)

**Output** unit Unitless, range not declared · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 1 – 10000 | bars on each side of a pivot |
| `atr_len` | 14 | whole number 1 – 10000 | bars in the ATR the distances are measured in |
| `tolerance_atr` | 0.5 | 0 – 100 | distance, in ATR, within which the two pivots count as the same level |
| `min_depth_atr` | 1.5 | 0 – 100 | minimum counter-swing between the pivots, in ATR |
| `max_gap` | 100 | whole number 2 – 100000 | most bars between the two pivots |
| `max_wait` | 50 | whole number 1 – 100000 | bars after forming within which the neckline must break |

### `bottom_formation`

Bottom/top formation as a sequence of stages: attempt (new low below the last swing low of a falling sequence, reclaimed on the bar), stabilized (higher low), structure shift (close above the intermediate high), confirmed (retest holds, new high); top mirrored; extra: wick (attempt extreme, natural stop), bottom_stage, top_stage

[See it in the showcase](../../../showcase/bottom_formation/)

**Output** unit Unitless, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 1 – 10000 | bars on each side of a swing point |
| `max_stage_bars` | 30 | whole number 1 – 100000 | bars after which an open stage expires |
| `retest_atr` | 0.5 | 0 – 100 | distance to the intermediate high, in ATR, that counts as a retest |

### `pivot_divergence`

Price-pivot-anchored divergence (regular and hidden) on a selectable oscillator (0 RSI, 1 WaveTrend, 2 Stoch RSI, 3 Williams %R, 4 CCI); fires on the confirmation bar, extra["pivot_price"] is the pivot (natural stop)

[See it in the showcase](../../../showcase/pivot_divergence/)

**Output** unit Unitless, range not declared · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `oscillator` | 0 | whole number 0 – 4 | oscillator code: 0 RSI, 1 WaveTrend, 2 Stoch RSI, 3 Williams %R, 4 CCI |
| `left` | 7 | whole number 1 – 1000 | bars left of a pivot |
| `right` | 7 | whole number 1 – 1000 | bars right of a pivot (confirmation delay) |
| `min_distance` | 10 | whole number 1 – 100000 | fewest bars between the two compared pivots |
| `max_distance` | 120 | whole number 1 – 100000 | most bars between the two compared pivots |
| `max_prior_pivots` | 5 | whole number 1 – 100 | earlier pivots each pivot is compared with |

Requires `min_distance` to be smaller than `max_distance`.

### `oscillator_pivot_divergence`

Divergence with the pivots found in the oscillator (TradingView's divergence scripts: ta.pivotlow/pivothigh of the oscillator, compared with the last pivot only); price read at the same bars; same oscillators, alerts and extras as pivot_divergence

[See it in the showcase](../../../showcase/oscillator_pivot_divergence/)

**Output** unit Unitless, range not declared · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `oscillator` | 0 | whole number 0 – 4 | oscillator code: 0 RSI, 1 WaveTrend, 2 Stoch RSI, 3 Williams %R, 4 CCI |
| `left` | 5 | whole number 1 – 1000 | bars left of a pivot |
| `right` | 5 | whole number 1 – 1000 | bars right of a pivot (confirmation delay) |
| `min_distance` | 5 | whole number 1 – 100000 | fewest bars between the two compared pivots |
| `max_distance` | 60 | whole number 1 – 100000 | most bars between the two compared pivots |
| `max_prior_pivots` | 1 | whole number 1 – 100 | earlier pivots each pivot is compared with |

Requires `min_distance` to be smaller than `max_distance`.

### `liquidity_pools`

BSL/SSL liquidity pools with explicit stop-hunt/breakout/reclaim classification (see also FvgZoneTracker and SmartMoneyStructureLinker for FVG-fill tracking and cross-detector confluence)

[See it in the showcase](../../../showcase/liquidity_pools/)

**Output** unit Unitless, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 5 | whole number 2 – 10000 | bars on each side of a pivot |
| `tolerance_pct` | 0.2 | 0.001 – 100 | distance, in percent, within which pivots join one pool |

### `wyckoff`

Wyckoff accumulation/distribution state machine: range-lock, Phases A-E, Spring/UTAD, SOS/SOW/LPS/LPSY, sequence validation and Cause/Quality scoring

[See it in the showcase](../../../showcase/wyckoff/)

**Output** unit Unitless, range not declared · **warmup** 20 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `range_lookback` | 20 | whole number 3 – 10000 | bars the trading range is read from |
| `range_atr_max` | 3 | 0.1 – 1000 | widest range, in ATR, that still locks as a trading range |
| `min_range_bars` | 6 | whole number 2 – 10000 | bars inside the range before it locks |

### `zigzag_advanced`

ZigZag with backstep, running-leg/confirmation status, and ATR-mode deviation (deviation_mode=percent|atr_multiple, in build_checked as 0|1; deviation_pct is the deviation in percent or in ATR multiples, deviation_value its former typed name); AdvancedZigZagEngine::reduce/project_to_timeframe for recursive levels and HTF projection

[See it in the showcase](../../../showcase/zigzag_advanced/)

**Output** unit Price, range not declared · **warmup** 7 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `depth` | 3 | whole number 1 – 10000 | bars on each side of a pivot |
| `backstep` | 2 | whole number 0 – 10000 | bars within which a newer pivot replaces an older one |
| `deviation_mode` | `percent` | `percent`, `atr_multiple` (f64 map: 0 – 1) | unit of the deviation: percent of price or multiples of the ATR |
| `deviation_pct` | 1 | 0.001 – 100 (deviation_mode = percent); 0.001 – 1000 (deviation_mode = atr_multiple); also accepted as `deviation_value` | minimum move for a new leg, in the unit of deviation_mode |
| `atr_len` | 14 | whole number 1 – 10000 | bars in the ATR |

## Composite read-outs, part 1

### `flow_exhaustion`

Flow Exhaustion (MFI + StochRSI): value = JMA-smoothed MFI of hlc3 (needs real volume, otherwise no output), secondary = JMA-smoothed StochRSI K; extra: stoch_k, upper/lower (adaptive zones over zone_lookback MFI values), divergence_score (signed, on emitting bars); alerts bull/bear_exhaustion (MFI in/leaving its zone + K crossing stoch_os/stoch_ob; prefix = expected move), bull/bear_divergence (price-pivot divergence of the MFI, emitted with its final score after react_bars)

[See it in the showcase](../../../showcase/flow_exhaustion/)

**Output** unit Ratio, range not declared · **warmup** 30 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 7 | whole number 1 – 50 | bars left and right of a price pivot for the divergence |
| `mfi_len` | 14 | whole number 2 – 500 | bars in the money flow index |
| `zone_lookback` | 200 | whole number 10 – 5000 | MFI values the adaptive zones are taken from |
| `rsi_len` | 14 | whole number 2 – 500 | bars in the RSI under the StochRSI |
| `stoch_len` | 14 | whole number 1 – 500 | bars in the stochastic of the RSI |
| `k_smooth` | 3 | whole number 1 – 100 | bars smoothing the StochRSI K |
| `jma_len` | 20 | whole number 1 – 500 | length of the JMA smoothing MFI and K |
| `confirm_bars` | 1 | whole number 0 – 5 | bars an exhaustion must hold before it alerts |
| `react_bars` | 10 | whole number 2 – 100 | bars after a divergence until its final score |

### `overextension_risk`

Overextension Risk: overextension score per side from three roles — stretch (EMA/ATR distance, Bollinger z, structure proximity, band breach), exhaustion (RSI, StochRSI, WaveTrend, WT divergence, money flow where real volume exists), reaction (reclaim-fail, WT cross from extreme, wick rejection) — times a volatility factor; regime (EMA stack + ADX) raises the countertrend threshold; value = top score 0..100, secondary = bottom score; extra: role scores, vol_factor, thresholds, regime, adx, money_flow; alerts bull/bear_do_not_chase (top in uptrend / bottom in downtrend), bear/bull_trend_exhaustion, bear/bull_range_fade (prefix = expected move)

[See it in the showcase](../../../showcase/overextension_risk/)

**Output** unit Ratio, range not declared · **warmup** 81 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `w_stretch` | 40 | 0 – 100 | weight of the stretch role |
| `w_exhaust` | 35 | 0 – 100 | weight of the exhaustion role |
| `w_react` | 25 | 0 – 100 | weight of the reaction role |
| `base_threshold` | 40 | 20 – 80 | score an alert needs before the regime raises it |
| `confirm` | 1 | whole number 0 – 1 | 1: alert only with a reaction, 0: without |
| `min_gap` | 8 | whole number 0 – 500 | bars between two alerts of one side |
| `ema_len` | 50 | whole number 2 – 1000 | bars in the EMA the stretch is measured from |
| `structure_len` | 80 | whole number 2 – 2000 | bars the structure extremes are taken from |

### `smoothed_supertrend`

Smoothed Supertrend: baseline = EMA(ema_len) of WMA(wma_len) of the supertrend midline (factor x ATR(atr_len)); trend from the baseline slope over slope_lookback bars with an ATR threshold and hysteresis; value = baseline; extra: trend, st_lower, st_upper, adx, strength_rank (percentrank of |slope|/ATR), distance_rank, overextension (+1/-1); alerts bull/bear_flip (trend turn with ADX above adx_threshold), bull/bear_rejection and bull/bear_rejection_strong (baseline touch and close back on the trend side; strong = strength rank above strength_threshold)

[See it in the showcase](../../../showcase/smoothed_supertrend/)

**Output** unit Price, range not declared · **warmup** 129 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `factor` | 12 | 0.5 – 100 | supertrend band width, in ATRs |
| `atr_len` | 90 | whole number 1 – 1000 | bars in the ATR |
| `wma_len` | 40 | whole number 1 – 1000 | bars in the WMA of the supertrend midline |
| `ema_len` | 14 | whole number 1 – 1000 | bars in the EMA over the WMA (the baseline) |
| `adx_threshold` | 25 | 5 – 50 | ADX a trend flip needs to alert |
| `slope_lookback` | 3 | whole number 1 – 100 | bars the baseline slope is measured over |
| `slope_threshold` | 0.05 | 0 – 10 | slope in ATRs that turns the trend |
| `touches` | 2 | whole number 1 – 5 | baseline touches before a rejection alerts |

### `coil_release`

Coil Release: coil strength from symbolic complexity, box-counting fractal dimension and efficiency over len bars; value = setup score 0..1 (coil score times release dynamics); trend context = EMA(ema_len) slope over slope_len bars in ATR units on the chart timeframe (originally a higher timeframe); extra: complexity, fractal_dim, efficiency, chop, coil, coil_score, release_dyn, trend, corr_high, corr_low, was_base; alerts bull/bear_release (first break of the break_len band with the trend) and bull/bear_base_break (break against the trend out of a base; prefix = expected move)

[See it in the showcase](../../../showcase/coil_release/)

**Output** unit Ratio, range not declared · **warmup** 58 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `len` | 34 | whole number 10 – 150 | bars the coil strength is measured over |
| `break_len` | 12 | whole number 3 – 50 | bars of the band whose break is the release |
| `ema_len` | 55 | whole number 2 – 2000 | bars in the trend EMA |
| `slope_len` | 5 | whole number 1 – 200 | bars the EMA slope is measured over |
| `thr_cont` | 0.45 | 0 – 1 | setup score a release with the trend needs |
| `thr_base` | 0.55 | 0 – 1 | setup score a base break against the trend needs |
| `cooldown` | 8 | whole number 0 – 100 | bars between two alerts |
| `chop_veto` | 61.8 | 30 – 80 | choppiness above which no alert fires |

### `compression_zones`

Compression Zones: compression zones from a 0..80 score (ATR compression, ADX chop, volume dry-up where real volume exists, impulse-in, historical S/R reuse) plus range compression and boundary-touch structure; value = score; extra: range_atr, in_zone, zone_high/zone_low, upper_touches, lower_touches, read (bull/bear pressure or continuation), armed, hist_reuse; alerts bull/bear_breakout (close beyond the zone edge within watch_bars after a zone of at least min_zone_bars, with body, close position and volume checks; prefix = expected move)

[See it in the showcase](../../../showcase/compression_zones/)

**Output** unit Ratio, range not declared · **warmup** 28 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `lookback` | 16 | whole number 8 – 100 | bars the range compression is measured over |
| `min_zone_bars` | 5 | whole number 3 – 50 | bars a zone must last before its breakout alerts |
| `range_atr` | 4.2 | 0.5 – 20 | widest zone range, in ATRs |
| `adx_max` | 27 | 5 – 60 | ADX above which the market is not compressed |
| `min_score` | 35 | 0 – 80 | score a zone needs |
| `min_touches` | 2 | whole number 2 – 20 | boundary touches a zone needs |
| `watch_bars` | 20 | whole number 1 – 200 | bars after the zone a breakout still alerts |

### `roc_context`

ROC Context: percentage ROC (roc_len) smoothed (avg_len, ma_kind 0 EMA, 1 SMA, 2 RMA) with a signal line (sig_len) and a slow context ROC (ctx_len); value = line, secondary = signal; extra: hist, context, sentiment (-100..100 stretch depth), stall (+1 line falling while price holds, -1 rising), divergence (fast vs. context), quality on stretch-cross bars; alerts bull/bear_cross (signal cross out of the stretch zone, context agrees, no stall against), bull/bear_cross_weak (with stall against or counter-context), bull/bear_zero_cross, bull/bear_stall (first stall bar); prefix = expected move

[See it in the showcase](../../../showcase/roc_context/)

**Output** unit Ratio, range not declared · **warmup** 55 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ma_kind` | 0 | whole number 0 – 2 | how the ROC is smoothed: 0 EMA, 1 SMA, 2 RMA |
| `roc_len` | 12 | whole number 1 – 1000 | bars in the rate of change |
| `avg_len` | 3 | whole number 1 – 200 | bars smoothing the ROC (the line) |
| `sig_len` | 9 | whole number 1 – 200 | bars in the signal line |
| `ob_level` | 5 | 0.01 – 1000 | upper stretch level, in percent |
| `os_level` | -5 | -1000 – -0.01 | lower stretch level, in percent |
| `ctx_len` | 50 | whole number 2 – 2000 | bars in the slow context ROC |
| `stall_len` | 5 | whole number 1 – 200 | bars a stall is measured over |

### `tradability_filter`

Tradability Filter: no-trade filter with an eight-state machine (no_trade, balance, compression, ready, breakout, trend, aftermath, exhaustion) driven by rank-normalised tradability, balance, compression and direction scores; value = tradability 0..100, secondary = direction -100..100; extra: balance, compression, direction, state_code, candidate_code, efficiency, fast_efficiency, adx, range_high, range_low, range_position, atr_ratio; alerts on entering a state: no_trade, compression, aftermath (warnings), bull/bear_ready, bull/bear_breakout, bull/bear_trend, bear/bull_exhaustion (prefix = expected move)

[See it in the showcase](../../../showcase/tradability_filter/)

**Output** unit Ratio, range not declared · **warmup** 190 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `quality_len` | 20 | whole number 8 – 100 | bars the tradability scores are measured over |
| `range_len` | 30 | whole number 10 – 200 | bars of the reference range |
| `baseline_len` | 34 | whole number 5 – 200 | bars in the baseline average |
| `rank_len` | 150 | whole number 50 – 500 | bars the scores are ranked against |
| `balance_threshold` | 58 | 30 – 90 | balance score that counts as balance |
| `compression_threshold` | 62 | 30 – 95 | compression score that counts as compression |
| `max_range_atr` | 7 | 2 – 20 | widest range for balance/compression, in ATRs |
| `confirm_bars` | 2 | whole number 1 – 10 | bars a candidate state must hold before it is entered |

### `reaction_speed`

Reaction Speed: tracks one structure event at a time (BOS > liquidity sweep > order-block tap) and scores the reaction: speed to the first reaction candle within react_bars, validation (break of the reaction extreme, ATR-move progress) within valid_bars; value = activity score 50 + 50 tanh((ATR/SMA(ATR) - 1) k); extra while tracking and on the closing bar: reaction, speed, validation, event_dir, event_type (1 BOS, 2 sweep, 3 OB), final_score; no alerts (the score describes a reaction that already happened)

[See it in the showcase](../../../showcase/reaction_speed/)

**Output** unit Ratio, range not declared · **warmup** 63 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `atr_len` | 14 | whole number 1 – 500 | bars in the ATR |
| `atr_base` | 50 | whole number 5 – 1000 | bars in the SMA of the ATR the activity is measured against |
| `swing_len` | 10 | whole number 2 – 100 | bars left and right of a swing pivot |
| `react_bars` | 8 | whole number 1 – 500 | bars the first reaction candle may take |
| `valid_bars` | 20 | whole number 1 – 1000 | bars the validation may take |
| `atr_move` | 1.6 | 0.1 – 20 | move in ATRs that validates the reaction |

### `market_state_chain`

Market State Chain: six debounced states (up/down trend, up/down weak, compression, chaos) as a first-order Markov chain; over lookback bars the exits of the current (context) state give Laplace-smoothed bull/bear/neutral exit probabilities, persistence, reliability (sample adequacy x predictability) and a hysteresis bias; value = p_bull - p_bear; extra: state_code, context_state, p_bull, p_bear, p_neutral, reliability, bias, exits, bars_in_state, persistence, dwell_ratio, atr_rank, bb_rank, efficiency, adx; alerts bull/bear_breakout (edge out of compression or chaos), bull/bear_reversal, bull/bear_continuation (prefix = expected move), regime_change, state_extended

[See it in the showcase](../../../showcase/market_state_chain/)

**Output** unit Ratio, range not declared · **warmup** 119 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ema_len` | 55 | whole number 10 – 1000 | bars in the trend EMA |
| `rank_len` | 100 | whole number 20 – 2000 | bars the ATR and Bollinger width are ranked against |
| `lookback` | 500 | whole number 50 – 1500 | bars whose state exits give the probabilities |
| `min_dwell` | 2 | whole number 1 – 50 | bars a new state must hold before it counts |
| `min_samples` | 20 | whole number 5 – 1000 | exits of the context state for full reliability |
| `conf_th` | 55 | 0 – 100 | exit probability an alert needs, in percent |
| `rel_th` | 40 | 0 – 100 | reliability an alert needs, in percent |
| `bias_margin` | 8 | 0 – 100 | probability lead that turns the bias, in percent |

### `rsi_cycle_stats`

RSI Cycle Statistics: cycle state machine origin -> armed -> success / failure / timeout between the overbought and oversold zones, completed cycles stored with progress-bucket timing; the live cycle gets the progress- and age-conditioned completion frequency of past cycles within lookback bars (past data only), base rate, edge and Wilson interval; value = RSI; extra: cycle_dir, progress, max_progress, age, cond_n, conditional, base_rate, edge, ci_low, ci_high, median_duration; alerts bull/bear_cycle_opportunity (once per cycle at a bucket crossing in the zone with enough samples, edge and a narrow interval; prefix = cycle direction), cycle_success, cycle_failure, cycle_timeout

[See it in the showcase](../../../showcase/rsi_cycle_stats/)

**Output** unit Ratio, range not declared · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `rsi_len` | 14 | whole number 2 – 200 | bars in the RSI |
| `overbought` | 70 | 51 – 99 | RSI level of the upper zone |
| `oversold` | 30 | 1 – 49 | RSI level of the lower zone |
| `max_cycle` | 150 | whole number 5 – 2000 | bars after which a cycle times out |
| `lookback` | 1000 | whole number 100 – 20000 | bars of past cycles the statistics use |
| `min_samples` | 20 | whole number 1 – 500 | past cycles an opportunity needs |
| `edge_th` | 15 | 0 – 100 | edge over the base rate an opportunity needs, in percent |
| `max_ci` | 40 | 1 – 100 | widest Wilson interval an opportunity allows, in percent |

### `price_average_relation`

Price-Average Relation: price against one moving average (ma_kind 0 EMA, 1 SMA, 2 RMA, 3 ALMA); value = relationship oscillator -100..100 (signed trend quality minus an extension/exhaustion penalty), secondary = its signal line; extra: trend, respect, extension, compression, acceleration, exhaustion (0..100), ma, distance_atr, respect_samples, respect_hit_rate, direction; alerts bull/bear_pullback, _momentum_release, _exhaustion (prefix = expected move), _relationship_break, _divergence, _turn, bull/bear_do_not_chase

[See it in the showcase](../../../showcase/price_average_relation/)

**Output** unit Unitless, range not declared · **warmup** 113 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `ma_kind` | 0 | whole number 0 – 3 | the moving average price is related to: 0 EMA, 1 SMA, 2 RMA, 3 ALMA |
| `ma_len` | 50 | whole number 2 – 1000 | bars in the moving average |
| `atr_len` | 14 | whole number 2 – 200 | bars in the ATR that scales distances |
| `cooldown` | 8 | whole number 0 – 100 | bars between two alerts of one kind |
| `turn_level` | 55 | 10 – 100 | oscillator level a turn must come from |
| `turn_lookback` | 30 | whole number 5 – 150 | bars a turn looks back for that level |

## Composite read-outs, part 2

### `leg_conviction`

Leg Conviction: each confirmed pivot-to-pivot leg scored 0..100 from speed (ATR per bar against speed_ref), cleanliness (share of closes in leg direction) and volume gradient (later vs. earlier half; weight 0 without real volume); value = last up-leg conviction, secondary = minus last down-leg conviction; extra: bull_last, bull_prev, bear_last, bear_prev, pivot_price on alert bars; alerts bear/bull_conviction_divergence (new price extreme on a weaker leg, prefix = expected move)

[See it in the showcase](../../../showcase/leg_conviction/)

**Output** unit Ratio, range not declared · **warmup** 12 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_left` | 5 | whole number 1 – 50 | bars left of a pivot |
| `pivot_right` | 5 | whole number 1 – 50 | bars right of a pivot, the confirmation delay |
| `max_leg` | 100 | whole number 1 – 1000 | most bars of a leg scanned, counted back from the pivot |
| `atr_len` | 14 | whole number 1 – 500 | bars in the Wilder ATR |
| `speed_ref` | 1 | 0.01 – 100 | leg distance per bar in ATR that scores full speed |
| `w_speed` | 0.4 | 0 – 1 | weight of speed in the conviction |
| `w_clean` | 0.4 | 0 – 1 | weight of cleanliness in the conviction |
| `w_volume` | 0.2 | 0 – 1 | weight of the volume gradient (0 without real volume) |

### `trend_break_risk`

Trend Break Risk: risk 0..100 that the prevailing (EMA-latched) trend's structure breaks against its last opposite swing; weighted held sensors near break level, failed breakout, structure erosion, RSI pivot divergence; extra: risk_raw, trend_dir, near, sfp, structure, divergence, rsi, ema, break_level, distance_atr; state no_trend/structure_broken/break_pressure/critical/watch/quiet; alerts bear/bull_break_pressure, _break_critical, _structure_break, _divergence (prefix = expected move, against the trend at risk)

[See it in the showcase](../../../showcase/trend_break_risk/)

**Output** unit Ratio, range not declared · **warmup** 15 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `bias_len` | 50 | whole number 2 – 1000 | bars in the EMA that latches the trend |
| `pivot_len` | 5 | whole number 1 – 50 | bars on each side of a swing pivot |
| `near_atr` | 1.5 | 0.01 – 100 | distance to the break level, in ATR, below which the near sensor rises |
| `rsi_len` | 14 | whole number 2 – 500 | bars in the RSI of the divergence sensor |
| `hold_bars` | 10 | whole number 1 – 500 | bars over which a held sensor decays |
| `smooth_len` | 3 | whole number 1 – 100 | bars in the EMA of the raw risk |

### `sweep_reversal_score`

Sweep Reversal Score (chart-timeframe configuration, higher-timeframe gates off): liquidity sweep of the last swing_len bars' extreme with a close back inside, ATR above atr_low_factor x its SMA, quality 0..2 (long: RSI > 50, no lower low; short mirrored: RSI < 50, no higher high) against min_quality_long/short (both 2), cooldown; value 1 long / -1 short / 0; extra: long_quality, short_quality, rsi, atr, atr_base, swing_low, swing_high, entry/stop/take on signal bars; alerts bull/bear_sweep_reversal

[See it in the showcase](../../../showcase/sweep_reversal_score/)

**Output** unit Unitless, range not declared · **warmup** 63 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `swing_len` | 10 | whole number 2 – 500 | bars whose extreme a sweep must take out |
| `atr_len` | 14 | whole number 1 – 500 | bars in the Wilder ATR |
| `atr_base_len` | 50 | whole number 1 – 1000 | bars in the SMA of the ATR (the gate's base) |
| `atr_low_factor` | 0.6 | 0 – 10 | ATR gate: ATR must exceed this multiple of its SMA |
| `rsi_len` | 14 | whole number 1 – 500 | bars in the RSI of the quality score |
| `pivot_len` | 5 | whole number 1 – 50 | bars on each side of a swing pivot |
| `min_quality_long` | 2 | whole number 0 – 2 | quality points a long signal needs |
| `min_quality_short` | 2 | whole number 0 – 2 | quality points a short signal needs |
| `cooldown` | 5 | whole number 0 – 500 | bars after a signal before the next one |

### `equal_level_liquidity`

Equal-Level Liquidity: equal-high/low levels clustered within eq_tolerance_atr ATR, pruned by age and distance, ranked by touches, freshness and distance (primary/secondary per side); sweep vs. breakout, stop hunt (rejection candle + volume spike), reclaim within reclaim_bars with a 0..100 score; value = bias; extra: primary/secondary_bsl/ssl (+_score, primary _touches), bsl_count, ssl_count, recent_bull/bear_score, volume_ratio, bear/bull_reclaim_score; alerts bear_bsl_sweep, bull_ssl_sweep, bull_bsl_breakout, bear_ssl_breakout, bear_bsl/bull_ssl_stop_hunt, bear/bull_reclaim, bear/bull_premium_reclaim (prefix = expected move), primary_bsl/ssl_near

[See it in the showcase](../../../showcase/equal_level_liquidity/)

**Output** unit Unitless, range not declared · **warmup** 14 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_len` | 8 | whole number 1 – 50 | bars on each side of a level pivot |
| `eq_tolerance_atr` | 0.12 | 0 – 10 | pivots within this many ATR join one level |
| `min_touches` | 2 | whole number 1 – 20 | touches a level needs before it takes part in events |
| `max_age` | 400 | whole number 1 – 100000 | bars after which an untouched level is dropped |
| `max_distance_atr` | 8 | 0.1 – 100 | distance from the close, in ATR, beyond which a level is dropped |
| `reclaim_bars` | 3 | whole number 0 – 100 | bars after a sweep within which a reclaim counts |
| `min_volume_ratio` | 1.2 | 0 – 100 | volume against its average a reclaim needs |
| `alert_distance_atr` | 1 | 0 – 100 | distance to the primary level, in ATR, that counts as near |

### `leg_character`

Leg Character: character of the live leg from the last alternating pivot (min_leg_atr filter) to the current extreme, from ten features (efficiency, overlap, retracement, commitment, wick cleanliness, momentum/true-range expansion, EMA support, cleanliness, acceptance); value = impulse score 0..100 (correction = 100 - impulse, both pulled to 50 by maturity); extra: correction, confidence, maturity, bars, direction, class, leg_start, the ten features, internal_swings, avg_internal_swing_atr, progression_ratio, sub_leg_growth; state impulse/correction/neutral with hysteresis; alerts bull/bear_impulse (leg direction), correction_started, character_lost

[See it in the showcase](../../../showcase/leg_character/)

**Output** unit Ratio, range not declared · **warmup** 11 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `pivot_left` | 5 | whole number 1 – 50 | bars left of a pivot |
| `pivot_right` | 5 | whole number 1 – 50 | bars right of a pivot, the confirmation delay |
| `max_leg_bars` | 350 | whole number 20 – 1500 | most bars of the live leg evaluated |
| `min_leg_atr` | 0.5 | 0 – 10 | smallest leg, in ATR, that adds an opposite pivot |
| `mature_bars` | 8 | whole number 2 – 100 | bars until the leg's scores count fully |
| `ma_len` | 34 | whole number 2 – 500 | bars in the EMA of the MA-support feature |
| `impulse_threshold` | 62 | 50 – 95 | impulse score that classifies the leg as impulse |
| `correction_threshold` | 62 | 50 – 95 | correction score that classifies the leg as correction |
| `change_buffer` | 5 | 0 – 25 | points below its threshold a class is kept (hysteresis) |

### `accumulation_phase`

Accumulation Phase: accumulation (bottom) and distribution (top) as process states from five components 0..2 each (trend loss, range formation, volatility compression, liquidity sweeps/springs, early structure); value = accumulation score 0..10, secondary = distribution score; extra: components per side, accum/dist_bars, accum/dist_state (0 none .. 5 breakout/breakdown ready), efficiency, bbw_ratio, atr_ratio, range_atr; alerts bull_accumulation_developing/_mature, bull_breakout_ready, bear_distribution_developing/_mature, bear_breakdown_ready

[See it in the showcase](../../../showcase/accumulation_phase/)

**Output** unit Unitless, range not declared · **warmup** 39 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `atr_len` | 14 | whole number 1 – 500 | bars in the Wilder ATR |
| `lookback` | 20 | whole number 10 – 500 | bars of the trend-loss, range and compression comparisons |
| `min_phase_bars` | 10 | whole number 3 – 500 | bars an active phase stays in transition |
| `pivot_left` | 3 | whole number 1 – 50 | bars left of a swing pivot |
| `pivot_right` | 3 | whole number 1 – 50 | bars right of a swing pivot |
| `er_range` | 0.3 | 0.05 – 1 | efficiency ratio below which price ranges |
| `bbw_compress` | 0.7 | 0.3 – 5 | Bollinger width against its average below which volatility compresses |
| `atr_compress` | 0.8 | 0.3 – 5 | ATR against its average below which volatility compresses |

### `reversal_setup_score`

Reversal Setup Score (default parameters, momentum mix RSI+MFI+MACD): bull/bear setup score (RSI, MFI or RSI without volume, MACD histogram, EMA-20 distance, Bollinger touch, wick, falling bar efficiency, shock bar; regime-weighted) and confirmation score (spring/upthrust and BOS/CHoCH on micro 3/3 and major 7/7 swings, fake breaks, climax-test-BOS sequence, premium/discount), latched event confirmation with decay, follow-through, failure, conflicts, quality; value = bull score (setup + 1.3 confirmation), secondary = bear score; extra: bull/bear_setup, _conf, _conf_decayed, _quality, _status, conflict_count, clx_state, clx_direction, price_zone, efficiency_ratio, rsi; alerts bull/bear_review, _action, _candidate, _major_bos, _choch, bull_spring, bear_upthrust, bear_failed_bull_reversal, bull_failed_bear_reversal, long/bull_signal_invalidated, conflict

[See it in the showcase](../../../showcase/reversal_setup_score/)

**Output** unit Unitless, range not declared · **warmup** 69 bars

### `structure_zones`

Structure Zones (default parameters): micro 3/3 and major 7/7 swing events (BOS/CHoCH, spring/upthrust, fake breaks, climax-test sequence, rejections, EQH/EQL as event_code), automatic support/resistance zones (major swings, springs/upthrusts, BOS flips, climax bars) with lifecycle fresh/tested/confirmed/broken/flipped, and an assessment of the nearest support and resistance within 3 ATR (reach, hold, break 0..100 from structure, respect, approach, context, plus nearby fair value gap, order block and volume-profile POC/value area); value = number of tracked zones; extra: support_*/resistance_* (mid, upper, lower, type, status, distance_atr, reach, hold, break, structural, respect, approach, context, state), micro_trend, major_trend, clx_state, clx_direction, event_code, poc, va_high, va_low; alerts support/resistance_relevant, bull_support_hold, bear_resistance_hold, bear_support_break, bull_resistance_break

[See it in the showcase](../../../showcase/structure_zones/)

**Output** unit Unitless, range not declared · **warmup** 15 bars

### `structural_flow`

Structural Flow (Balanced preset, relative-volume participation): multi-horizon structural flow (24/48/96) with entry/hold/flip hysteresis, internal wave (momentum, velocity, acceleration) aligned to it, wave segmentation and archives, counter pressure, integrity, maturity, compression, completed-counter-owned Re-Sync, exhaustion, five/three-segment sequence context and a nine-phase read; value = structural drive, secondary = aligned wave; extra: direction, strength, confidence, internal_wave, wave_quality, counter_pressure, integrity, retracement, maturity, compression, resync_quality, resync_stage, exhaustion, deterioration, wave_decay, impulse_context, correction_context, wave_context, phase, phase_confidence; alerts bull/bear_flow_initiated, flow_transition, bull/bear_flow_flip, bull/bear_expansion, correction_start, compression, bull/bear_early_resync, bull/bear_resync, late_expansion, exhaustion, failed_expansion, bull/bear_dominant_expansion, weakening_waves

[See it in the showcase](../../../showcase/structural_flow/)

**Output** unit Unitless, range not declared · **warmup** 192 bars

### `trend_quality`

Trend Quality Score Engine

[See it in the showcase](../../../showcase/trend_quality/)

**Output** unit Unitless, range not declared · **warmup** 28 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 2 – 10000 | bars in the EMA, efficiency ratio, ADX and RVOL |

### `buy_sell_pressure` · alias `pressure`

Buy/Sell Pressure Estimator

[See it in the showcase](../../../showcase/buy_sell_pressure/)

**Output** unit Unitless, range not declared · **warmup** 1 bars

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 1 – 10000 | bars in the EMA of the raw pressure |

### `multi_factor`

Multi-Factor Composite Market Score

[See it in the showcase](../../../showcase/multi_factor/)

**Output** unit Ratio, range not declared · **warmup** 28 bars · needs real volume

| Parameter | Default | Accepted | Meaning |
|---|---|---|---|
| `period` | 14 | whole number 2 – 10000 | bars of every input (trend quality, RSI, pressure, volatility regime) |

