---
title: Market regime
description: classify_regime sorts the market into one of four regimes from a 20-bar moving average, ADX and ATR. The rules are few and fixed, so the result is easy to explain.
order: 2
---

## The four regimes

`MarketRegime` has four variants:

| Regime             | Meaning                                                    |
| ------------------ | ---------------------------------------------------------- |
| `BullishExpansion` | A strong trend, pointing up, with price above its average   |
| `BearishExpansion` | A strong trend, pointing down, with price below its average |
| `Consolidation`    | No strong trend and low volatility                          |
| `Transition`       | Everything in between, and the warmup                       |

## The rules

```rust
pub fn classify_regime(bars: &[Bar], adx_val: f64, atr_val: f64) -> MarketRegime
```

`bars` is the history up to and including the current bar. `adx_val` is the current ADX value,
`atr_val` the current ATR **as a fraction of price** (`ATR / close`).

1. Fewer than 21 bars: `Transition`.
2. The function computes the 20-bar simple moving average of the close, for the current and the
   previous bar, and its slope: `(sma_20 - prev_sma_20) / prev_sma_20`.
3. **Trending** means `adx_val > 20`.
   - Slope above `0.001` and close above the average: `BullishExpansion`.
   - Slope below `-0.001` and close below the average: `BearishExpansion`.
   - Otherwise: `Transition`.
4. **Not trending:** `atr_val > 0.02` gives `Transition`, anything else `Consolidation`.

## ATR units

The `atr` indicator reports `100 * ATR / close`, a percentage. `classify_regime` expects the
fraction, so divide by 100 before passing it on. The crate's own pipeline does the same.

```rust
use kestrel_chartkit::{build_checked, classify_regime, Bar};
use std::collections::HashMap;

fn regimes(bars: &[Bar]) -> Result<(), Box<dyn std::error::Error>> {
    let mut adx = build_checked("adx", &HashMap::new())?;
    let mut atr = build_checked("atr", &HashMap::new())?;

    for (i, bar) in bars.iter().enumerate() {
        let (Some(a), Some(t)) = (adx.on_bar(bar), atr.on_bar(bar)) else {
            continue; // still warming up
        };
        let regime = classify_regime(&bars[..=i], a.value, t.value / 100.0);
        println!("{} {}", bar.timestamp, regime);
    }
    Ok(())
}
```

The [market regime chart](../../../showcase/market-regime/) in the showcase runs exactly this loop
over synthetic bars and shades every run of equal regimes.

## Building on the regime

`regime_advanced` holds small, independent helpers for consumers that track the regime over time:

- `RegimeMarkovModel` counts observed transitions and returns `transition_probability(from, to)`
  and `next_state_distribution(from)`.
- `RegimePersistenceTracker` reports how long the current regime has lasted.
- `HysteresisBand` switches levels only after a value has crossed separate enter and exit
  thresholds, so a reading that hovers at a boundary does not flip back and forth.

The composite scoring uses the regime to grade a signal: see
[Composite scoring](../composite-scoring/).

The `analytics` module has its own on-demand regime vote with different inputs. It is a separate
function, not a wrapper around `classify_regime`.
