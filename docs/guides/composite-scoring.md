---
title: Composite scoring
description: Indicator readings become subscores between -1 and 1. aggregate_subscores weighs them into a direction, grades the result against the market regime and derives an ATR-based risk plan.
order: 3
---

## From reading to subscore

`score_indicator(name, &output, &alerts)` turns one indicator output and its alerts into a
`SubScore`:

```rust
pub struct SubScore {
    pub indicator: String,
    pub score: f64,       // -1.0 ..= 1.0, negative is bearish
    pub raw_value: f64,
    pub reason: Option<String>,
}
```

Alerts such as a bullish cross or an oversold extreme push the score towards `+1`, their bearish
counterparts towards `-1`. When an indicator raised no alert, some are scored from their level:

| Indicator                                             | Level scoring                                              |
| ----------------------------------------------------- | ---------------------------------------------------------- |
| `rsi`, `stoch_rsi`, `mfi`, `williams_r`, `connors_rsi` | above 70: `-0.4`, below 30: `+0.4`, above 55: `+0.2`, below 45: `-0.2` |
| `macd`                                                | histogram sign: `±0.3`                                     |
| `bollinger`                                           | `%b` above 1: `-0.5`, below 0: `+0.5`                       |
| `vortex`                                              | `(VI+ − VI−) × 2`, if the gap exceeds `0.05`               |
| `trend_quality`                                       | `value / 100`                                              |

The oscillators read as mean reversion: overbought is bearish. In a strong uptrend they therefore
argue against the trend, and the regime vetoes the result. Combine indicators whose reading
matches the signal you want to grade.

## Aggregation

```rust
pub fn aggregate_subscores(
    subscores: Vec<SubScore>,
    weights: Option<&HashMap<String, f64>>,
    regime: MarketRegime,
    sr_zones: Vec<SupportResistanceZone>,
    latest_bar: Option<&Bar>,
    atr_val: f64,
) -> CompositeSignal
```

Step by step:

1. **Clean up.** Subscores with a non-finite score or raw value are dropped, the rest are clamped
   to `-1..=1`. With nothing left, the result is neutral, `Hold` and `Veto`.
2. **Weigh.** Each subscore gets the weight stored under its indicator name, default `1`. A weight
   that is not finite or outside `0..=1000` falls back to `1`. The score is the weighted mean,
   clamped to `-1..=1`.
3. **Direction.** `score >= 0.20` is bullish, `score <= -0.20` bearish, anything in between
   neutral.
4. **Agreement.** The share of subscores that agree with the direction: above `0.05` for bullish,
   below `-0.05` for bearish, within `±0.20` for neutral. It is a share, not a win probability.
5. **Heat score.** `|score| * 0.6 + agreement * 0.4`, between 0 and 1.

## Permission grade

The regime decides whether a direction may be traded at all:

| Condition (checked in this order)                                                   | Grade          |
| ----------------------------------------------------------------------------------- | -------------- |
| Neutral direction, or direction against the regime (bullish in bearish expansion …) | `Veto`         |
| Regime is `Transition`                                                              | `Caution`      |
| Heat `>= 0.50` and direction matches the regime, or consolidation with agreement `>= 0.75` | `ClearToTrade` |
| Heat `>= 0.30`                                                                      | `Caution`      |
| Otherwise                                                                           | `Veto`         |

Only `ClearToTrade` produces a trigger: `Buy` for bullish, `Sell` for bearish. Every other
combination is `Hold`.

## Risk plan

For a `Buy` or `Sell` with a valid latest bar, the signal carries a `RiskPlan`, measured in ATR
from the close:

| Level       | Buy              | Sell             |
| ----------- | ---------------- | ---------------- |
| Stop loss   | close − 1.5 ATR  | close + 1.5 ATR  |
| Target 1    | close + 1.0 ATR  | close − 1.0 ATR  |
| Target 2    | close + 2.0 ATR  | close − 2.0 ATR  |

`atr_val` is the absolute ATR in price units (the `atr` indicator puts it in `extra["raw"]`). If it
is not finite and positive, one percent of the close is used instead.
`aggregate_subscores_with_instrument` additionally rounds the plan to the instrument's tick size.

## Weight presets

`WeightPreset::weights()` returns a ready-made weight map: `Balanced` (all weights `1`),
`TrendFollowing`, `MeanReversion` and `VolumeLocation`.

```rust
use kestrel_chartkit::{aggregate_subscores, score_indicator, WeightPreset};

let weights = WeightPreset::TrendFollowing.weights();
let subscores = vec![score_indicator("macd", &macd_out, &macd.alerts())];
let signal = aggregate_subscores(subscores, Some(&weights), regime, Vec::new(), Some(&bar), atr);
println!("{:?} {:?} {:.2}", signal.direction, signal.permission, signal.heat_score);
```

<!-- The explanation field is generated in German. -->

`CompositeSignal::explanation` is a generated sentence summarising direction, heat, agreement,
permission and regime. It is currently written in German; build your own text from the structured
fields if you need another language.

The [composite signal chart](../../../showcase/composite-signal/) in the showcase marks every bar
where a trigger starts.
