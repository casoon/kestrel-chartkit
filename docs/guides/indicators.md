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
`default_params`. The current version has 130 entries; the
[indicator reference](../../reference/indicators/) lists every one of them with its parameters.

Behind the catalog, every indicator has one definition
(`kestrel_chartkit::indicator::definition`): its name and aliases, each parameter with type,
default, accepted range and meaning, the relations between parameters, and how it is built.
`definition::families()` returns them grouped by family, for a parameter form or a picker that
should not hard-code what the crate already knows.

`output_unit(name)` says what the indicator's `value` is quoted in — `Price`, `Ratio`, `Volume`
or `Unitless`. A charting consumer needs this to decide where the result belongs: `Price` means a
price level and can be drawn on the price axis, everything else needs a scale of its own.

```rust
use kestrel_chartkit::{output_unit, IndicatorUnit};

assert_eq!(output_unit("sma"), IndicatorUnit::Price);
assert_eq!(output_unit("rsi"), IndicatorUnit::Ratio);
// A difference, not a level — it shares no zero with the price axis.
assert_eq!(output_unit("macd"), IndicatorUnit::Unitless);
```

Only the primary output is described. `ichimoku` is `Price` because its subject is
`tenkan`/`kijun`/`senkou_*`, even though its `value` holds the cloud width; `atr` is `Ratio`
because its `value` is a percentage, even though `extra["raw"]` is a price distance. Unknown
names return `Unitless`, and the enum is `#[non_exhaustive]` — match with a `_` arm.

`build_checked(name, &params)` builds an indicator from a name and a parameter map. Missing
parameters take their defaults. Periods must be whole numbers in the supported range, and invalid
thresholds or orderings return a `RegistryError`. So does a key the indicator does not read — a
typo like `peroid` fails instead of silently building with the default period. Both entry points
validate against the same definition; `build_typed` additionally takes `source` and the choices
and timestamps that only a typed value can carry (e.g. `smoothing` for `rsi`):

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
| Structure and pattern detectors | `bos_choch`, `order_block`, `liquidity_fvg`, `liquidity_sweeps`, `wyckoff`, `zigzag`, `double_pattern`, `bottom_formation` |
| Divergence                     | `pivot_divergence`, `oscillator_pivot_divergence`                   |

Print the full list with `catalog()`. The [indicator reference](../../reference/indicators/) lists
every entry with its parameters, the [showcase](../../../showcase/) draws each one.

## Two series

A relative strength or a return correlation compares the series with a second one, the
*reference*: a benchmark, a sector, another instrument. `Indicator::on_bar` sees one bar, so these
indicators have a path of their own that mirrors the one above step by step. They are listed by
`pair_catalog()` and not by `catalog()`, and `input_series(name)` says for any name whether it
reads one series or two.

| One series       | Two series             |
| ---------------- | ---------------------- |
| `catalog()`      | `pair_catalog()`       |
| `build_checked`  | `build_pair_checked`   |
| `on_bar(&bar)`   | `on_bars(&bar, &reference_bar)` |
| `run_batch`      | `run_pair_batch`       |

```rust
use kestrel_chartkit::{build_pair_checked, run_pair_batch};
use std::collections::HashMap;

let params = HashMap::from([("len".to_string(), 10.0)]);
let mut relative_strength = build_pair_checked("relative_strength", &params)?;
// One entry per bar of `bars`, as `run_batch` gives.
let series = run_pair_batch(relative_strength.as_mut(), &bars, &benchmark_bars);
```

Parameters are validated as in `build_checked`, and the result is the same `IndicatorOutput`.
`WithReference::new(pair, &reference_bars)` binds a reference to a pair indicator and is an
`Indicator` again, for code that keeps a `Box<dyn Indicator>` and feeds it bar by bar.

The two series are paired on their timestamps, by the rule `align_closes` uses:

- A bar whose timestamp the reference lacks is skipped. The indicator does not see it, and the
  output for that bar is `None`. Nothing is carried forward into the gap.
- A bar of the reference whose timestamp the first series lacks is ignored.
- Where the reference holds a timestamp twice, the later bar counts.
- Windows and warmup count paired bars.

| Name                | Value                                                         | `extra`                                               |
| ------------------- | ------------------------------------------------------------- | ----------------------------------------------------- |
| `relative_strength` | `alpha_pct`: return of the series minus return of the reference over `len` bars, in percentage points | `ratio` (close / reference close), `own_perf_pct`, `bench_perf_pct`, `alpha_pct` |
| `correlation`       | Pearson correlation of the last `len` pairs of simple returns, −1 to 1 | —                                                     |

Equal timestamps do not make two series comparable. `check_pair_applicability` names the two
cases the declared series properties show: different price adjustment (a dividend or split stands
in one series only) and a different session. Two series with the same session kind can still
trade at different hours; that is not declared in `SeriesCapabilities` and stays with the caller.

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
