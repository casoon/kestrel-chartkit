---
title: Visualising results
description: kestrel-chartkit computes and draws nothing itself. chartlet renders static charts from its numbers, kestrel-loom an interactive chart in the browser.
order: 4
---

kestrel-chartkit is a calculation library. What it returns — indicator values, extras, alerts,
regimes, scores, zones — is plain data. Drawing it is the job of a renderer, and there are two
that fit the crate:

| Renderer | What you get | Status |
| --- | --- | --- |
| [chartlet](https://casoon.github.io/chartlet/) | Static, accessible SVG or HTML from a JSON specification, rendered at build time (CLI, Rust crate or npm package) | usable, pre-1.0 |
| [kestrel-loom](https://github.com/casoon/kestrel-loom) | Interactive candlestick chart in the browser: Rust → WebAssembly → Canvas 2D, with indicator panes and drawing tools | alpha, in development |

Both are in development, like this crate. Expect their interfaces to change.

## Static charts with chartlet

Every chart on this site is a chartlet specification written by
[`examples/site_showcase.rs`](https://github.com/casoon/kestrel-chartkit/blob/master/examples/site_showcase.rs):
the example streams bars through the indicators, collects what they return, and writes the numbers
into a `"type": "time"` specification — candles as an `ohlc` layer, indicator values as `line`
layers in the price pane or a pane of their own, zones as `band`, levels and alerts as
`annotation`. chartlet then renders it when the site is built. The
[showcase](../../../showcase/) shows the result for every indicator.

```rust
let mut rsi = build_checked("rsi", &HashMap::new())?;
let mut points = Vec::new();
for bar in &bars {
    if let Some(output) = rsi.on_bar(bar) {
        points.push(format!(r#"{{"time":{},"value":{:.2}}}"#, bar.timestamp, output.value));
    }
}
let spec = format!(
    r#"{{"schemaVersion":1,"type":"time","title":"RSI 14","panes":[{{"layers":[{{"mark":"line","name":"RSI","points":[{}]}}]}}]}}"#,
    points.join(",")
);
// chartlet render spec.json --format svg, or chartlet::render_json from Rust
```

A few things decide whether a chart reads well:

- **Put a line where its values live.** `output_unit(name)` says whether `value` is a price; the
  extras do not have a declared unit yet, so check their range before drawing them over candles.
- **Leave gaps as gaps.** A field that is missing on some bars (a zone that ended, a pivot not yet
  confirmed) is `null` in the specification, so the line breaks instead of bridging the gap.
- **Mind the limits.** chartlet draws at most four panes, six data layers and six annotations per
  pane; it reports overlapping labels and dense candles as warnings.

## Interactive charts with kestrel-loom

kestrel-loom renders in the browser and takes its indicator math from this crate. Overlays that
belong to a chart rather than to an indicator — signal markers, levels, zones — travel as a
`viz::scene::Scene`: a renderer-neutral model of panes, axes and objects (`Polyline`,
`BoundedBox`, `Fill`, `Text`, `Tooltip`, `Table`), each with an `id` so a live chart can replace one
object per tick (`Pane::upsert_object`). The scene types are plain data, serialisable with the
default `serde` feature.

## The built-in quick-look renderers

The crate still contains two small SVG renderers, `viz::render_chart_svg` (candles, lines, zones,
markers) and `viz::render_scene_svg` (a `Scene` as SVG). They are meant for a quick look in tests
and debugging, not as a presentation layer: no axes labels to speak of, no accessibility, no
responsive layout. Colours pass through `sanitize_color` (`#rgb`, `#rrggbb`, `#rrggbbaa` or
`var(--name)`; anything else becomes `#29b6f6`). For anything a reader should see, use chartlet or
kestrel-loom.
