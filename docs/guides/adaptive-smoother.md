---
title: The adaptive smoother
description: What the Kestrel Adaptive Smoother does, how it was chosen by measurement, and where it is ahead of and behind the other smoothers of the catalog.
order: 8
---

`kestrel_adaptive_smoother`, short `kas`, is a moving average that changes its own speed. While the
closes go back and forth it nearly rests; when they move one way by more than the market usually
does, it follows. This page states the formula, how it was measured against the other smoothers
of the catalog, and what the numbers say, including where `kas` loses.

## Why an adaptive smoother at all

A smoother weighs past closes. If all weights are positive, the line lags a trend. The lag can
only be removed with negative weights, and negative weights make the line overshoot at a step or
a turn. For a fixed filter this is arithmetic, not a matter of tuning: `hma`, `tema` and `lsma`
follow a straight trend without lag and overshoot a step by 20 to 33 %.

An adaptive filter is the one way around that: it can be slow while nothing happens and fast when
something does, without negative weights. Whether that helps on real prices is an empirical
question, and it is the question this page answers.

## The formula

With `n = (period + 1) / 2`, the base gain `k0 = 2 / (n + 1)`, `a = 2 / (max(n, 2) + 1)`,
`b = 2 / (max(4 n, 20) + 1)` and `d` the change of the close:

```text
v  = v + a (d   - v)                 smoothed change
q  = q + a (d^2 - q)                 its mean square over the same window
Q  = Q + b (d^2 - Q)                 the long-run mean square
z^2 = v^2 / (c sqrt(Q_before q))     c = a / (2 - a)
S  = k0^2 / (1 - k0) * (z^2)^adapt
k  = (sqrt(S^2 + 4 S) - S) / 2
s1 = s1 + k (close - s1)
s2 = s2 + k (s1 - s2)
s3 = s3 + k (s2 - s3)
kas = s2 + phase (s2 - s3)
```

- **The score `z`** is the geometric mean of two readings of the smoothed change: against the
  long-run volatility (a standard score, a unit normal on a random walk) and against the
  volatility of its own window (an efficiency ratio). It rises with a directed move and rises
  further when the move is large for this market. It has no unit and no sign, so the scale of the
  series and the direction of the move do not enter: the negated series gives the negated line.
- **The gain `k`** is the steady-state gain of a filter that tracks a random walk observed in
  noise, for a signal-to-noise ratio scaled by `z^(2 adapt)`. It stays between 0 and 1 without
  clamping and equals the base gain at `z = 1`. `adapt = 0` leaves a fixed linear filter.
- **`phase`** adds what the third stage still lags behind the second. At a constant gain a
  straight trend is followed with a lag of `(2 - phase) (period - 1) / 4` bars, and a step is
  overshot by `(1 + 2 phase) exp(-2 (1 + phase) / phase)`: nothing at 0, 0.5 % at the default
  0.5, 5.5 % at 1, 12 % at 1.5, 25 % at 2. At `phase = 0` the line cannot overshoot whatever the
  gain does, because every stage is a weighted mean of its input and its own last value.

The line is valid from the first bar, keeps seven numbers of state and reads only closes that
have already happened.

## How it was measured

Everything on this page comes from the example `smoother_tradeoffs`:

```bash
cargo run --release --example smoother_tradeoffs -- --candidates synthetic rows.csv
cargo run --release --example smoother_tradeoffs -- summary rows.csv
```

- **Comparison set:** `sma`, `ema`, `wma`, `hma`, `dema`, `tema`, `kama`, `vidya`, `t3`,
  `mcginley`, `lsma`, `alma`, `rma`, `super_smoother`, `jma` and `ultimate_smoother`, each swept
  over 22 lengths from 3 to 200. `mama` has no length to sweep and is not part of the set.
- **Equal smoothness, not equal length.** A length means something different for every smoother.
  So each one is read along its whole sweep at the point where it is as smooth as the others,
  and the table states what it costs there.
- **Two meanings of smooth.** *Roughness* is the root mean square of the second difference of
  the line, relative to that of the input: how much the curve wiggles. *Direction changes* count
  how often the line turns per 1000 bars. The two are not the same thing, and the result depends
  on which one is meant.
- **Synthetic signals:** a step, a ramp, sines of 10 to 80 bars, each also in normal noise; pure
  noise; straight trends with turns in noise; a random walk observed in noise; and two volatility
  switches, one where only the noise grows and one where the signal starts to move.
- **Close series:** indices, currency pairs, commodities and shares at 15 minutes, 1 hour,
  4 hours and 1 day, 201 series in all. The data is not part of this repository. Two costs are
  read: the distance of the line from the close, and the error against a centred average that
  looks 10 bars into the future and serves only as the yardstick. Both are in units of the
  local volatility.
- **Controls:** three random walks written by the example itself (`controls`): normal steps,
  fat-tailed steps, and steps with clustering volatility.

The shape of the formula and its windows were chosen on half of the 1 hour and 4 hour series.
The other half, and all 15 minute and daily series, were not used for choosing.

## Close series

Each number is a ratio to the best smoother of the catalog at the same smoothness: below 1,
`kas` is closer to the price than anything in the catalog; above 1, it is further away.
`kas` is the default, `adapt = 0.5`; `kas_a100` and `kas_a0` are `kas` at `adapt` 1 and 0.

| series | family | distance at 100 turns | at 50 turns | at 25 turns | yardstick error at 50 turns | distance at roughness 0.1 | at roughness 0.03 |
|---|---|---:|---:|---:|---:|---:|---:|
| 15 minutes (24) | `kas` | 0.81 | 0.81 | 0.82 | 0.79 | 1.21 | 1.26 |
| | `kas_a100` | 0.64 | 0.67 | 0.70 | 0.65 | 1.51 | 1.55 |
| | `kas_a0` | 1.09 | 1.06 | 1.05 | 1.09 | 1.01 | 1.00 |
| 1 hour (36) | `kas` | 0.81 | 0.83 | 0.84 | 0.80 | 1.19 | 1.27 |
| | `kas_a100` | 0.66 | 0.69 | 0.71 | 0.67 | 1.45 | 1.55 |
| | `kas_a0` | 1.09 | 1.06 | 1.05 | 1.10 | 1.01 | 1.00 |
| 4 hours (71) | `kas` | 0.84 | 0.85 | 0.85 | 0.82 | 1.18 | 1.29 |
| | `kas_a100` | 0.69 | 0.72 | 0.73 | 0.69 | 1.42 | 1.58 |
| | `kas_a0` | 1.09 | 1.07 | 1.06 | 1.10 | 1.01 | 1.00 |
| 1 day (70) | `kas` | 0.86 | 0.86 | 0.86 | 0.84 | 1.19 | 1.29 |
| | `kas_a100` | 0.72 | 0.74 | 0.75 | 0.72 | 1.44 | 1.56 |
| | `kas_a0` | 1.09 | 1.06 | 1.04 | 1.10 | 1.01 | 0.99 |
| random walks (3) | `kas` | 0.85 | 0.85 | 0.84 | 0.83 | 1.18 | 1.29 |
| | `kas_a100` | 0.71 | 0.72 | 0.72 | 0.69 | 1.42 | 1.52 |

The best smoother of the catalog is `t3`, `jma` or `super_smoother` in every column, with `alma`
and `hma` in a few cells.

Read by asset class, the picture does not change: over the 16 cells of class and timeframe the
distance at 50 turns lies between 0.80 and 0.88, the distance at roughness 0.03 between 1.25
and 1.33. With `adapt = 1` the two ranges are 0.65 to 0.77 and 1.53 to 1.70.

Three things follow.

1. **At equal direction changes `kas` is 14 to 19 % closer to the price** than the best smoother
   of the catalog, on every timeframe and in every asset class. With `adapt = 1` it is a quarter
   to a third closer; put the other way round, at the same distance it then turns about half as
   often.
2. **At equal roughness it is about a quarter further away,** and half as far again with
   `adapt = 1`. The line rests and then moves, and that change of pace is itself roughness.
   Whoever wants the smoothest curve is better served by `super_smoother`, `jma` or `t3`, or by
   `kas` with `adapt = 0`, which matches them.
3. **The random walks show the same numbers.** The advantage does not come from anything
   predictable in the markets. It is a property of the filter, and it says nothing about where
   the price goes next.

`adapt` moves between the two: the default 0.5 takes about half of the advantage of `adapt = 1`
for about half of its cost.

## Synthetic signals

Every family at the number of direction changes `super_smoother(20)` has on a random walk in
noise (159 per 1000 bars). Bars to 50 % and 90 % are for a step in noise of one standard
deviation; errors are root mean square against the noise-free signal.

| family | period | step of 10: to 50 % | to 90 % | step of 3: to 50 % | to 90 % | ramp lag | trend with turns | walk in noise | noise switch | signal switch | roughness in noise |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| `super_smoother` | 20.0 | 3.6 | 7.5 | 3.7 | 7.6 | 4.0 | 1.05 | 0.615 | 0.771 | 0.927 | 0.026 |
| `jma` | 21.8 | 3.5 | 7.1 | 3.5 | 7.2 | 3.1 | 0.91 | 0.612 | 0.776 | 0.914 | 0.027 |
| `t3` | 7.1 | 3.2 | 6.4 | 3.3 | 6.5 | 2.7 | 0.82 | 0.615 | 0.817 | 0.903 | 0.030 |
| `hma` | 29.8 | 3.4 | 6.5 | 3.5 | 6.6 | 1.1 | 0.69 | 0.666 | 0.842 | 0.985 | 0.029 |
| `kas` | 14.0 | 2.8 | 11.8 | 4.3 | 13.3 | 6.1 | 1.39 | 0.651 | 0.737 | 0.919 | 0.032 |
| `kas` phase 1 | 15.3 | 2.3 | 8.0 | 3.6 | 10.1 | 4.0 | 1.06 | 0.632 | 0.728 | 0.874 | 0.036 |
| `kas` adapt 1 | 11.0 | 0.6 | 8.3 | 2.7 | 11.4 | 5.3 | 1.18 | 0.640 | 0.844 | 0.819 | 0.055 |
| `kas` adapt 0 | 22.3 | 6.4 | 15.7 | 6.4 | 16.2 | 7.9 | 1.90 | 0.707 | 0.734 | 1.139 | 0.017 |

The other smoothers of the catalog reach this number of direction
changes only at lengths where they need 13 to 144 bars for half of the step (`kama`, `vidya`,
`sma`, `rma`, `mcginley`, `alma`), or not at all (`ema`, `wma`, `dema`, `tema`, `lsma`,
`ultimate_smoother`).

Overshoot at a step, the largest value over all lengths, in percent of the step:

| family | clean step | step of 10 in noise |
|---|---:|---:|
| `kas` (phase 0.5) | 0.0 | 1.4 |
| `kas` phase 0 | 0.0 | 1.2 |
| `kas` phase 1 | 0.0 | 6.1 |
| `kas` phase 1.5 | 0.0 | 15.3 |
| `kas` phase 2 | 0.0 | 26.3 |
| `kas` adapt 1 | 0.0 | 1.7 |
| `super_smoother` | 4.5 | 4.8 |
| `jma` | 10.7 | 10.6 |
| `t3` | 16.2 | 16.2 |
| `ultimate_smoother` | 26.2 | 26.2 |
| `hma` | 28.2 | 28.4 |
| `ema`, `sma`, `wma`, `kama`, `alma` | 0.0 | 1.0 to 1.6 |

A clean step passes through `kas` at once, because a move out of a flat series is unbounded
against its volatility. The values in noise are the ones that count; 1 to 1.6 % is what the noise
alone leaves in the measure.

What the signals say:

- **A large step:** `kas` is at half of a step of 10 standard deviations after 2.8 bars, where
  the best of the catalog needs 3.2 to 3.6. Reaching 90 % takes 12 bars against 6 to 7.5,
  unless `phase` is raised. With `adapt = 1` half of the step is reached within one bar.
- **A small step, a slow trend, a trend with turns:** behind. A trend of a fifth of a standard
  deviation per bar is not directed enough for the score to rise much, and `hma`, `t3` and `jma`
  follow it more closely. They pay for it with 11 to 28 % overshoot; `phase = 1` brings `kas`
  level with `super_smoother` at 6 %, still behind the other three.
- **A random walk in noise:** 6 % behind at equal direction changes, 21 % behind at equal
  roughness. On a signal that is mostly noise an adaptive gain has nothing to hold on to.
- **Volatility switches:** when only the noise grows, `kas` has about 4 % less error than the
  best of the catalog; when the signal starts to move, it is level with them (2 % more). With
  `adapt = 1` the two change sides: 10 % less error when the signal starts to move, 9 % more
  when only the noise grows, because for some bars a burst of noise looks like a move.

## The candidates

The formula was the end of four rounds. The table gives the 4 hour series (ratios as above); all
candidates use the cascade of `kas` at `phase = 1`, and all but `cand_velocity_half_strength` the
full strength `adapt = 1`. So the row to compare with is `kas` at `phase = 1` and `adapt = 1`,
0.69 and 1.60, and for the candidate at half strength `kas` at `phase = 1`: 0.80 and 1.27.

| candidate | what drives the gain | distance at 50 turns | distance at roughness 0.03 |
|---|---|---:|---:|
| `cand_fixed` | nothing | 1.02 | 0.99 |
| `cand_one_velocity` | velocity score, one stage only | 2.00 | 1.85 |
| `cand_one_efficiency` | efficiency ratio, one stage only | 2.17 | 1.72 |
| `cand_two_velocity` | velocity score, two stages, no lead | 0.72 | 1.77 |
| `cand_velocity` | standard score of the smoothed change against long-run volatility | 0.67 | 1.71 |
| `cand_velocity_short` | the same, change smoothed over half the length | 0.71 | 1.97 |
| `cand_velocity_long` | the same, over twice the length | 0.75 | 1.67 |
| `cand_velocity_memory16` | the same, volatility over 16 instead of 4 lengths | 0.67 | 1.77 |
| `cand_velocity_floor` | the same, never slower than the base gain | 0.86 | 1.46 |
| `cand_velocity_power` | the same, gain as a clamped power of the score | 0.68 | 1.71 |
| `cand_velocity_half_strength` | the same, `adapt = 0.5` | 0.78 | 1.30 |
| `cand_smoothed_efficiency` | smoothed change against the volatility of its own window | 0.71 | 1.54 |
| `cand_efficiency` | efficiency ratio over a fixed window | 0.80 | 1.47 |
| `cand_relative_volatility` | short-run over long-run volatility | 0.95 | 1.22 |
| `cand_variance_ratio` | smoothed squared velocity score | 0.87 | 1.15 |

- **One stage is not enough.** With a single stage the line turns with every bar on which the
  close crosses it, whatever the gain. The second stage is what makes few direction changes
  possible; this is also why `kama` and `vidya` do badly here.
- **Volatility alone is the weakest driver.** A volatility ratio cannot tell a move from a
  burst of noise. The drivers that work read whether the movement is directed.
- **The two best drivers fail in opposite places.** The velocity score reacts most to a move out
  of a quiet market, but it remembers the old volatility: after a noisy phase it stays slow for
  several lengths. The smoothed efficiency forgets at once and reacts less. `kas` uses their
  geometric mean, which keeps the result on the close series and shortens the slow recovery by about a third.
- **The steady-state map** does as well as a clamped power of the score and needs no bounds.

## When to use which

- For a line that should turn rarely and still stay near the price: `kas`, and more so with
  `adapt = 1`, at the price of a rougher curve.
- For the smoothest curve, or the least error against a noisy but steady signal:
  `super_smoother`, `jma`, `t3`, or `kas` with `adapt = 0`.
- For the least lag on a steady trend, accepting overshoot: `hma`, or `kas` with a higher
  `phase`.

None of this is a statement about returns. A line that turns less often gives fewer signals of
direction, not better ones.
