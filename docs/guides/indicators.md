---
title: Indicators
description: Every indicator implements one streaming trait and is registered by name, so it can be built from configuration and validated before it runs.
order: 1
---

## One trait

```rust
pub trait Indicator: Send + Sync {
    fn name(&self) -> &str;
    fn warmup_period(&self) -> usize { 0 }
    fn reset(&mut self);
    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput>;
    fn on_checked_bar(&mut self, bar: &Bar) -> Result<Option<IndicatorOutput>, BarValidationError>;
    fn alerts(&self) -> Vec<IndicatorAlert> { Vec::new() }
}
```

An indicator sees one bar at a time and keeps its own state. `on_bar` returns `None` during the
warmup. `on_checked_bar` validates the bar first, for inputs you cannot vouch for. `alerts()` lists
the events of the last bar, such as a cross or an extreme; the composite scoring reads them.

## The output

`IndicatorOutput` has the same shape for every indicator:

| Field                 | Content                                                              |
| --------------------- | -------------------------------------------------------------------- |
| `value`               | The main line                                                        |
| `secondary`, `signal` | Optional second line and signal line                                 |
| `extra`               | Further named values, `HashMap<String, f64>`                         |
| `state`, `reason`     | Machine-readable state label and a human-readable explanation        |
| `artifacts`           | Typed results emitted this bar: pivots, zones, profiles              |
| `series_capabilities` | Which series the result was computed on, when the caller attached it |

Indicators with several lines put them into `extra`. A few examples:

| Indicator        | `value`               | `extra`                                           |
| ---------------- | --------------------- | ------------------------------------------------- |
| `bollinger`      |                       | `basis`, `upper`, `lower`, `bandwidth`, `percent_b` |
| `keltner`        |                       | `upper`, `lower`, `atr`                           |
| `supertrend`     | the stop line         | `trend` (`1` long, `-1` short), `upper`, `lower`  |
| `volume_profile` |                       | `vpoc`, `vah`, `val`, `total_volume`, …           |
| `adx`            | ADX                   | `signal`, `di_plus`, `di_minus`                   |
| `atr`            | `100 * ATR / close`   | `raw` (ATR in price units), `signal`              |

Check the unit before combining values: `atr` reports a percentage in `value` and the absolute ATR
in `extra["raw"]`.

## The registry

`catalog()` returns one `IndicatorCatalogEntry` per indicator, with `name`, `description` and
`default_params`. The current version has 105 entries.

`build_checked(name, &params)` builds an indicator from a name and a parameter map. Missing
parameters take their defaults. Periods must be whole numbers in the supported range, and invalid
thresholds or orderings return a `RegistryError`:

```rust
use kestrel_chartkit::build_checked;
use std::collections::HashMap;

let params = HashMap::from([("len".to_string(), 20.0), ("mult".to_string(), 2.0)]);
let mut bollinger = build_checked("bollinger", &params)?;
```

`build_typed` additionally accepts string options where an indicator has variants, for example
`smoothing=wilder|ema` for `rsi` or `variance=population|sample` for `bollinger`. The catalog
description names them.

## Families

| Family                         | Examples from the catalog                                           |
| ------------------------------ | ------------------------------------------------------------------- |
| Moving averages                | `sma`, `ema`, `wma`, `hma`, `dema`, `tema`, `kama`, `t3`, `vidya`     |
| Oscillators                    | `rsi`, `macd`, `stochastic`, `stoch_rsi`, `cci`, `williams_r`, `tsi`  |
| Volatility and trend strength  | `atr`, `adx`, `bollinger`, `keltner`, `donchian`, `supertrend`, `parabolic_sar` |
| Volume                         | `obv`, `vwap`, `anchored_vwap`, `cmf`, `mfi`, `volume_profile`, `cvd` |
| Trend                          | `ichimoku`, `alligator`, `efficiency`, `zscore`                     |
| Composite scores               | `trend_quality`, `buy_sell_pressure`, `volatility_regime`, `multi_factor` |
| Structure and pattern detectors | `bos_choch`, `order_block`, `liquidity_fvg`, `liquidity_sweeps`, `wyckoff`, `zigzag` |

Print the full list with `catalog()`, or see `examples/site/catalog.txt` in the repository, which
`cargo run --example site_showcase` writes.

## Input contract

- `Bar::try_new` and `Bar::validate` require finite, positive prices, a finite, non-negative
  volume, and a high/low range that contains open and close. `Bar::new` skips the checks and is
  meant for trusted feeds.
- Structure detectors report their findings as typed `artifacts` rather than as single numbers.

## How values are checked

Numeric results are pinned against independently derived reference values: a second
implementation of the documented formula, exact rational or decimal arithmetic, or an analytically
unambiguous case. They are never pinned against the crate's own output. The golden-reference
suites live in `tests/golden_reference_*.rs` with fixtures in `tests/fixtures/`. Where reference
values come from a script, it lives in `reference/` (Python standard library only), and
`python3 reference/generate.py --check` reproduces the fixtures byte for byte.
