---
title: Analytics and evaluation
description: On-demand read-outs describe the current market from a slice of bars. The evaluation module measures what happened after a signal, from forward excursions to calibration, the deflated Sharpe ratio and a logistic model.
order: 6
---

## On demand, not streaming

The `analytics` module holds pure functions over a slice of bars, oldest first. You call one when
you need a reading and get a snapshot back; nothing keeps state between calls. They are not
indicators: they do not implement the `Indicator` trait, are not in `catalog()` and do not feed
the [composite scoring](../composite-scoring/). Most return `None` until the slice is long enough.

| Function                    | Returns                   | What it reads                                                                 |
| --------------------------- | ------------------------- | ----------------------------------------------------------------------------- |
| `classify_trend_regime`     | `RegimeReading`           | Trending or ranging, by a vote of ADX, Choppiness and Kaufman efficiency      |
| `trend_reading`             | `TrendReading`            | Slope of a smoothed close, combined with the regime vote into a five-step phase |
| `price_summary`             | `PriceSummary`            | Change, window range, Wilder ATR, its percentile and an ATR stop distance     |
| `trend_persistence_reading` | `TrendPersistenceReading` | How clean and durable the current trend is, 0 to 100                          |
| `activity_reading`          | `ActivityReading`         | ATR percentile, optionally averaged with a volume percentile                  |
| `fear_gauge_reading`        | `FearGaugeReading`        | Williams VIX Fix for sell-offs and its inverse for rallies, against a spike band |
| `monthly_seasonality`       | `SeasonalityReport`       | What each calendar month did historically                                      |
| `analytics::forward::*`     | several                   | What happened after comparable situations                                      |

### Regime vote

`classify_trend_regime(bars, adx_len, window)` lets three measures vote "trending": ADX at or above
25, Choppiness at or below 38.2, efficiency ratio at or above 0.5. Two or more votes make the state
`Trending`, otherwise `Ranging`; `trend_votes` keeps the count. This is a separate classifier from
the four-state [`classify_regime`](../market-regime/) used by the composite scoring.

`trend_reading` takes that `RegimeReading`, a smoother kind and length, and a dead band in percent.
A slope inside the dead band is `Flat` and maps to `MarketPhase::Range`. `StrongUp` and
`StrongDown` require the regime vote to say `Trending`; a directional move in a ranging market is
plain `Up` or `Down`.

### Price and ATR

`price_summary(bars, atr_len, stop_mult)` spans every bar you pass, so the window length is your
choice. `range_position` places the last close between window low and high (0.5 for a flat
window). `atr_percentile` is the share of the published ATR history at or below the current value,
a quick check whether volatility is unusually high or low. `stop_distance` is `stop_mult · atr`.

### Trend persistence

`trend_persistence_reading(bars)` combines four sensors over a fixed 34-bar window: R² of the
closes against time (weight 40), efficiency ratio (25), ADX strength and slope (20) and fractal
dimension (15), smoothed over the last five bars. States run from `Strong` (75 and up) down to
`Dead` (below 30). `driver` and `drag` name the strongest and weakest sensor. `transition_risk`
is derived from the score and the efficiency sensor; it describes exposure, it does not detect a
break. The score is not a probability.

### Activity and fear gauge

`activity_reading(bars, atr_percentile, use_volume, window)` takes the ATR percentile from
`price_summary`. The volume component is off unless you enable it, and is `None` when every volume
in the window is zero. The score is then the ATR percentile alone; no substitute is used.

`fear_gauge_reading(bars)` computes the VIX Fix (`wvf`) and its inverse (`bwvf`) over 22 closes and
flags a spike when a value reaches mean plus two standard deviations of its last 20 values. A fear
spike wins over a complacency spike. `absorbed` marks a spike on a price that barely moved over
five bars, likely a thin wick. Both states are context near a possible bottom or top, not a signal.

### Seasonality

`monthly_seasonality(bars)` measures each month from the previous month's last close to its own.
The first month has no return, the last is marked incomplete and left out of the statistics.
`MonthStatistics` carries `samples`, the mean, a sample standard deviation (`None` with fewer than
two years) and `positive_share`. That share is a frequency over the years present, not a forecast
for the next one.

### Forward paths

`analytics::forward` answers questions about what followed comparable situations. Choosing which
start points are comparable is the caller's job; the functions only look forward from the index
they are given.

| Function                      | Question                                                                       |
| ----------------------------- | ------------------------------------------------------------------------------ |
| `conditional_end_probability` | Given a phase has lasted `age` bars, how likely does it end within `horizon`?  |
| `first_passage`               | Which of two levels does the path touch first, and after how many bars?        |
| `summarize_passages`          | Counts, shares with a Wilson interval, median bars to each level               |
| `forward_move`                | Close change, highest rise and deepest fall over the horizon                   |
| `empirical_band`              | Central quantile band of a set of outcomes                                     |
| `band_coverage`               | Share of outcomes that later fell inside their announced band                  |

`conditional_end_probability` is a Kaplan–Meier estimate: phases still running are counted as
censored (`PhaseDuration { ended: false }`), and the result carries `at_risk` and a Greenwood
standard error. `first_passage` returns `None` when the data ends before the horizon, so
incomplete paths do not count as `Neither`. When one bar touches both levels, `Tie` decides; OHLC
does not record the order. `band_coverage` is the check that an 80 % band contains about 80 % of
what happened.

## Evaluating signals

The `evaluation` module looks at signals after the fact. It does not decide whether a signal is
good; it reports what followed, in units you can compare.

### Forward price outcomes

`ForwardPriceOutcome::compute(direction, entry, bars, horizon)` walks the bars after entry and
records the return per bar, the maximum favourable and adverse excursion (MFE, MAE, never
negative) and the bar on which each was reached. `PriceDirection` handles the sign for `Long` and
`Short`. Everything is in price units of one instrument: no contract size, no currency conversion.

`PriceOutcomeStats::compute` aggregates samples into hit rate, average return in percent and in
ATR, and average MFE and MAE in percent. `PriceStats` does the same for plain profit and loss
values, counting a missing value as closed but not won.

### Excursion summary

`evaluation::excursion::evaluate_outcomes(signals, bars, horizon)` enters each signal at the close
of the first bar at or after its timestamp and measures MFE and MAE over the next `horizon` bars,
in percent of entry. Signals without enough forward bars are skipped. `win_rate` here is the share
of signals with MFE at least MAE, a proxy and explicitly not a calibrated success rate. `by_hour`
groups the average MFE by UTC hour of the signal.

### Trade statistics

`OutcomeRecorder` follows setups bar by bar: `record_setup` takes entry, target, stop and a maximum
duration, `on_bar` checks target and stop. A setup closes as `Win`, `Loss` or `Expired`; when one
bar reaches both target and stop, `IntrabarFillPolicy` decides, `StopFirst` by default. Each
completed setup becomes a `SignalEvaluationRecord` with its realised R-multiple: exit minus entry,
divided by the initial risk.

`TradeStats::compute(&records)` aggregates them:

| Field                | Meaning                                                               |
| -------------------- | --------------------------------------------------------------------- |
| `winrate`            | Share of `Win` records                                                |
| `profit_factor`      | Sum of gains over sum of losses in R; infinite with gains and no losses |
| `average_r_multiple` | Mean R per trade                                                      |
| `expectancy_r`       | The same mean, read as expected value per trade in R                  |
| `max_drawdown_r`     | Largest drop of cumulative R from its peak                            |

The outcome decides the sign: a `Win` counts positive, a `Loss` negative, `BreakEven` as zero.
`cohort_aggregate` computes the same statistics per group, keyed by any label you derive from a
record.

### Calibration

A record's `agreement` is the share of inputs that agreed when the signal fired. It is not a
probability. `compute_calibration(&records, num_buckets)` tests whether it behaves like one: it
treats agreement as a predicted win rate, reports the Brier score and sorts records into buckets
of roughly equal size, each with its mean agreement and observed win rate. A well-calibrated
signal shows the two tracking each other.

To turn a raw score into a probability, `IsotonicCalibrator::fit` learns a monotone mapping from
training data only. `compute_calibration_metrics` scores predictions on unseen data: Brier score,
a constant baseline, the Brier skill score, log loss and expected calibration error.
`block_bootstrap_brier` gives a mean and 5 % / 95 % percentiles of the Brier score under time
dependence. `split_trades_purged` keeps training trades from overlapping the test period.

### Deflated Sharpe ratio

`deflated_sharpe(returns, trials, trial_sharpe_variance)` follows Bailey and López de Prado (2014).
Trying many strategies and keeping the best inflates its Sharpe ratio. The function estimates the
Sharpe ratio the best of `trials` strategies without skill would reach (`expected_max_sharpe`) and
returns the probability that the true Sharpe ratio exceeds it, given the sample length, skewness
and kurtosis. With one trial it is the probabilistic Sharpe ratio against zero. The Sharpe ratio
is per observation, not annualised. `normal_quantile` is the inverse normal CDF it uses, also
exported on its own.

### Logistic regression

`LogisticRegression::fit(x, y, l2)` fits `P(y = 1 | x) = σ(b + w·x)` with an L2 penalty on the
weights (not the intercept), by Newton–Raphson. It is meant for tens of features, one weight each,
so the model stays explainable. `fit` returns `None` for empty or ragged input, a negative `l2` or
a singular system, such as perfectly separable data without a penalty. `predict(row)` returns the
probability. Fit and check it on separate data; an in-sample fit says little about the next signal.
