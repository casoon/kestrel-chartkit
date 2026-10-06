---
title: Structure and events
description: Structure detectors report what they find in two ways, as typed artifacts and zones with a known time, and as alerts on the bar the event becomes known. Four event detectors build on confirmed pivots.
order: 5
---

## Pivots: event time and knowledge time

A pivot is only known some bars after it formed. `structure::confirmed_pivots(bars, pivot_len)`
returns every confirmed pivot over a series, oldest first, as `ConfirmedPivot`:

| Field          | Content                                                                 |
| -------------- | ----------------------------------------------------------------------- |
| `id`           | The pivot bar's timestamp. Stable across recalculations                 |
| `timestamp`    | Event time: the pivot bar (equal to `id`)                               |
| `confirmed_at` | Knowledge time: the bar `pivot_len` bars later that confirmed the pivot |
| `price`        | The bar's high or low                                                   |
| `is_high`      | `true` for a pivot high                                                 |

Until `confirmed_at`, the pivot does not exist, so nothing repaints. The test is non-strict: the
pivot's high must be at least as high as every bar `pivot_len` bars to either side (lows
mirrored). A bar can be a pivot high and a pivot low at once; the pair `(id, is_high)` stays
unique.

## Zones

`find_sr_zones(bars, pivot_len)` uses the same pivot test and returns up to two supports below and
two resistances above the last close, nearest first, as `SupportResistanceZone`. Each zone carries
`pivot_ts`, the event time of the pivot it came from. When close pivots merge into one level, the
earliest timestamp is kept: the zone formed when the area first turned. `pivot_ts` is `0` when no
pivot time is known.

The band width is set by `ZoneTolerance { half_width, dedup_distance }`, both in price points.
`find_sr_zones` uses `ZoneTolerance::relative_to_price` (0.25 % half width, 0.5 % merge distance);
`find_sr_zones_with_tolerance` takes your own, typically derived from ATR. A zero or non-finite
value falls back to the price-relative one.

`ZoneRegistry` tracks zones over time. `register_with_source` records which detector produced a
zone, `update(bar, atr)` moves it through `ZoneState` (`Active`, `Touched`, `Reacted`, `Broken`,
`Flipped`), and a close more than 0.2 ATR beyond the outer edge breaks it.

Two detectors hold their own zones and expose them directly:

- `OrderBlockEngine::active_zones()` returns every active `OrderBlockZone`
  (`is_bullish`, `top`, `bottom`, `created_bar`, `mitigated`).
- `LiquidityFvgEngine::zones()` returns the open `FvgZone`s (`is_bullish`, `top`, `bottom`,
  `created_bar`), oldest first. A bullish gap is dropped once a later low reaches its bottom, a
  bearish one once a later high reaches its top. At most 20 are held.

`created_bar` counts bars since the engine started, not a timestamp.

## Artifacts

`IndicatorOutput::artifacts` holds typed results as the `Artifact` enum: `Pivot`, `Zone`,
`Profile` and `Scenario`. `ZoneArtifact` and `ProfileArtifact` carry `from_ts`/`to_ts`, the first
and last bar they were derived from. `None` means unknown, not "now"; `span()` returns both bounds
only when both are set. The volume profile detectors emit `hvn_zone`, `lvn_zone` and
`absorption_zone` zones this way. `TaggedArtifact` pairs an artifact with the `SeriesIdentity` it
was computed on.

## Alerts

`Indicator::alerts()` lists the events of the last bar as `IndicatorAlert { kind, note, strength }`.
`kind` is the machine-readable name to match on, `note` a display label, `strength` a value
between 0 and 1. The detectors below prefix directional kinds with `bull_` or `bear_`.

Alert kinds are part of the API. Recent breaking changes:

- `volatility_regime` reports `volatility_squeeze` and `volatility_expansion` (0.16).
- `rsi` and `cci` add `bull_exhaustion` and `bear_exhaustion` (0.17).
- `wavetrend` reports `bull_cross`/`bear_cross` and `bull_extreme`/`bear_extreme` (0.22).

Crosses use `crossed_over`/`crossed_under` with a relative tolerance (`CROSS_TOLERANCE_REL`,
`1e-9`), so a line sitting a rounding error away from its signal line does not fire.

`AlertEvent` wraps an alert once its context is known: timestamp, `EventPhase` (`Setup`, `Watch`,
`Trigger`, `Invalidation`, `Expiry`, `TargetHit`), instrument and timeframe. Its `event_id` is
derived from all of them, so `AlertDeduplicator::admit` drops a replayed bar but keeps the same
event on another instrument or timeframe.

```rust
use kestrel_chartkit::{build_checked, AlertDeduplicator, AlertEvent, Bar, EventPhase};
use std::collections::HashMap;

fn events(bars: &[Bar]) -> Result<(), Box<dyn std::error::Error>> {
    let mut detector = build_checked("zone_rejection", &HashMap::new())?;
    let mut seen = AlertDeduplicator::new();
    for bar in bars {
        let Some(out) = detector.on_bar(bar) else { continue };
        for alert in detector.alerts() {
            let event = AlertEvent::new(alert, bar.timestamp, EventPhase::Trigger)
                .with_instrument("EXAMPLE");
            if seen.admit(&event) {
                println!("{} stop {:?}", event.alert.kind, out.extra.get("wick"));
            }
        }
    }
    Ok(())
}
```

## Warmup and settling

`warmup_period()` says when the first value arrives. Recursive state keeps the imprint of the
series start much longer. `indicator::settle::settle_bars(name, &params, bar_seconds)` measures
after how many bars all outputs no longer depend on where the series began;
`alert_settle_bars` asks the same for alert kinds only, the measure for warming up a backtest or a
live process. Both return `None` for indicators that never converge, such as cumulative ones.

## Event detectors

All four are causal: an event is reported on the bar on which it becomes known, never earlier.
The natural invalidation level of an event is in `extra["wick"]` for the first three and in
`extra["pivot_price"]` for the divergences, present only on event bars.

### zone_rejection

A failed breakout at a confirmed zone. Zones are built from strict pivots: a pivot joins an intact
zone of its kind if it lies within `tolerance_atr` ATR of its band and the zone stays narrower
than `max_width_atr` ATR; otherwise it opens a new one. A zone is confirmed after `min_touches`
pivots and dropped on a close more than 0.2 ATR beyond its outer edge.

| Parameter       | Default |
| --------------- | ------- |
| `pivot_len`     | 5       |
| `atr_len`       | 14      |
| `tolerance_atr` | 0.25    |
| `max_width_atr` | 2       |
| `min_touches`   | 2       |

`bull_failed_breakout`: the low pierces a confirmed support's bottom, the close is back above it.
`bear_failed_breakout` mirrors it at resistance. An outside bar doing both reports neither.
`value` is `1`, `-1` or `0`; `extra["wick"]` is the event bar's low (high); `extra["zones"]`
counts the confirmed zones. Strength grows with the zone's touches.

### double_pattern

Double bottom and double top while they form. A new pivot low is paired with an earlier one that
lies within `tolerance_atr` ATR, at least `2 · pivot_len` and at most `max_gap` bars back, with a
counter-swing of at least `min_depth_atr` ATR in between and no close below the lower low. The
highest high of the counter-swing is the neckline.

| Parameter       | Default |
| --------------- | ------- |
| `pivot_len`     | 5       |
| `atr_len`       | 14      |
| `tolerance_atr` | 0.5     |
| `min_depth_atr` | 1.5     |
| `max_gap`       | 100     |
| `max_wait`      | 50      |

`bull_double_bottom_forming` fires when the second pivot is confirmed (`value` `1`),
`bull_double_bottom` on the first close above the neckline within `max_wait` bars (`value` `2`);
`bear_double_top_forming` and `bear_double_top` mirror them with `-1`/`-2`. A close beyond the
pattern's extreme or the end of `max_wait` ends it without an alert. Event bars carry `neckline`,
`target` (neckline plus pattern height) and `wick` (the lower low, or higher high).

### bottom_formation

A bottom as a sequence of four stages, a top as the same sequence on mirrored prices:

1. **Attempt:** a new low below the last swing low of a falling sequence, reclaimed on the same bar.
2. **Stabilized:** a confirmed higher low above the attempt low.
3. **Structure shift:** a close above the intermediate high between attempt and higher low.
4. **Confirmed:** a retest within `retest_atr` ATR of that high holds, then a close above the
   highest high since the shift.

| Parameter        | Default |
| ---------------- | ------- |
| `pivot_len`      | 5       |
| `max_stage_bars` | 30      |
| `retest_atr`     | 0.5     |

A close below the attempt low fails the run, and after the shift so does a close below the
stabilizing low. A stage that waits longer than `max_stage_bars` expires. Only one run per side is
open at a time. Alerts: `bull_bottom_attempt`, `bull_bottom_stabilized`, `bull_structure_shift`,
`bull_bottom_confirmed` and the `bear_top_*`/`bear_structure_shift` counterparts. `value` is the
stage reached on this bar (`1` to `4`, negative for a top); `extra["bottom_stage"]` and
`extra["top_stage"]` carry the open stage on every bar, `extra["wick"]` the attempt extreme.
`indicator::bottom_formation::runs(bars, side)` returns every run over a series as `Run`, with the
bar index of each stage.

### pivot_divergence and oscillator_pivot_divergence

Both compare pivots of price with an oscillator selected by `oscillator`: `0` RSI, `1` WaveTrend,
`2` Stochastic RSI, `3` Williams %R, `4` CCI. They differ in where the pivots are found.

| Parameter          | `pivot_divergence` | `oscillator_pivot_divergence` |
| ------------------ | ------------------ | ----------------------------- |
| `left`, `right`    | 7, 7               | 5, 5                          |
| `min_distance`     | 10                 | 5                             |
| `max_distance`     | 120                | 60                            |
| `max_prior_pivots` | 5                  | 1                             |

`pivot_divergence` finds strict pivots in the bar lows and highs and reads the oscillator at the
pivot bar. `oscillator_pivot_divergence` finds strict pivots in the oscillator and reads the bar
low or high at the same bar, as TradingView's divergence scripts do. Each new pivot is compared
with up to `max_prior_pivots` earlier pivots of its type, `min_distance` to `max_distance` bars
back, and the best-scoring candidate is reported.

| Price       | Oscillator  | Alert                    |
| ----------- | ----------- | ------------------------ |
| lower low   | higher low  | `bull_divergence`        |
| higher high | lower high  | `bear_divergence`        |
| higher low  | lower low   | `bull_hidden_divergence` |
| lower high  | higher high | `bear_hidden_divergence` |

Alerts fire `right` bars after the pivot; strength is the quality score. `value` is the oscillator.
On event bars `extra` carries `pivot_price`, `previous_price`, `pivot_age`, `bars_between` and
`direction` (`1` or `-1`).
