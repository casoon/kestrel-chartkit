---
title: SVG charts
description: Two renderers produce self-contained SVG strings, one for a single price chart, one for a multi-pane scene. No JavaScript, no front end.
order: 4
---

The charts in the [showcase](../../../showcase/) are produced by these two functions. The code
next to each chart is taken from `examples/site_showcase.rs`, which writes the SVG files.

## Price chart: `render_chart_svg`

```rust
pub fn render_chart_svg(data: &ChartRenderData, width: u32, height: u32) -> String
```

`ChartRenderData` holds everything the chart shows:

| Field     | Type                   | Drawn as                                                      |
| --------- | ---------------------- | ------------------------------------------------------------- |
| `title`   | `String`               | Heading, top left                                             |
| `bars`    | `Vec<ChartBarData>`    | Candles; `ChartBarData::from(&bar)` converts a `Bar`          |
| `series`  | `Vec<ChartSeries>`     | One polyline per series, points as `(timestamp, value)`       |
| `zones`   | `Vec<ChartZoneData>`   | Shaded horizontal price band with its name                    |
| `markers` | `Vec<ChartMarkerData>` | Dot and label; the colour follows the `TriggerAction`         |

The price scale spans the lows and highs of `bars`; the time axis spans the first to the last bar.
Series names are not drawn, so describe the lines next to the chart.

## Multi-pane scene: `render_scene_svg`

```rust
pub fn render_scene_svg(scene: &Scene, width: u32, height: u32, theme: &Theme) -> String
```

A `Scene` is a stack of `Pane`s. Each pane gets a share of the height from its `height_ratio` and
maps data to pixels through its axes: `Axis::time(label, from_ts, to_ts)` for x and
`Axis::value(label, min, max)` for y. Objects are drawn in `z_order`:

- `Polyline` with `LineStyle::Solid`, `Dashed` or `Dotted`
- `BoundedBox`, for example a zone or a time range
- `Fill`, a polygon such as the area between two lines
- `Text`, `Tooltip` (a native SVG `<title>`) and `Table`

Objects have an `id`. `Pane::upsert_object` replaces an object with the same id, so a live chart
can update one object per tick instead of rebuilding the pane. `scene_from_artifacts` builds a
scene from indicator artifacts.

`Theme` sets background, text, candle and marker colours; `Theme::default()` is the dark palette
the showcase uses.

## Colours

Every colour passes through `sanitize_color`. It accepts:

- `#rgb`, `#rrggbb` and `#rrggbbaa`
- `var(--name)`, a CSS custom property

Anything else, including named colours, `rgb()` and `var()` with a fallback, becomes the neutral
fallback `#29b6f6`. A wrong colour stays visible instead of silently dropping the object.

`var(--name)` is how one exported chart serves a light and a dark page: the library emits the
reference, the page defines the value.

```rust
ChartSeries {
    name: "Basis".to_string(),
    color: "var(--chart-basis)".to_string(),
    points,
}
```

Titles, zone names and labels are XML-escaped.

## Interactive front ends

`ChartRenderData` and the scene types are plain data, serialisable with the default `serde`
feature. A web, canvas or terminal front end can consume the same structures instead of the SVG
string.
