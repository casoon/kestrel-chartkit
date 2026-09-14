---
title: Quickstart
description: Build an indicator from the registry by name, stream bars through it, and read the output as each bar closes.
order: 2
---

## Stream a first indicator

This is `examples/basic_indicator.rs` from the repository:

```rust
use kestrel_chartkit::{build_checked, Bar, Indicator};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut params = HashMap::new();
    params.insert("rsi_len".to_string(), 14.0);
    let mut rsi = build_checked("rsi", &params)?;

    let closes = [
        44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03, 45.61,
        46.28, 46.28, 46.00,
    ];

    for (i, &close) in closes.iter().enumerate() {
        let bar = Bar::try_new(
            1_700_000_000 + i as i64 * 60,
            close,
            close + 0.5,
            close - 0.5,
            close,
            1_000.0,
        )?;

        match rsi.on_bar(&bar) {
            Some(output) => println!("bar {i:>2}: close={close:.2}  RSI(14)={:.2}", output.value),
            None => println!("bar {i:>2}: close={close:.2}  RSI(14)=<warming up>"),
        }
    }

    Ok(())
}
```

`cargo run --example basic_indicator` prints:

```text
bar  0: close=44.34  RSI(14)=<warming up>
bar  1: close=44.09  RSI(14)=<warming up>
…
bar 13: close=46.28  RSI(14)=<warming up>
bar 14: close=46.28  RSI(14)=70.46
bar 15: close=46.00  RSI(14)=68.36
```

Three things to note:

- `build_checked` validates the parameters against the registry and returns a boxed
  `Indicator`. An unknown name or an out-of-range period is a `RegistryError`, not a panic.
- `Bar::try_new` rejects non-finite or non-positive prices and a high/low range that does not
  contain open and close. Use it wherever bars enter your program.
- `on_bar` returns `None` until the warmup is complete, then `Some` on every bar.

## Draw it

`render_chart_svg` turns bars and indicator lines into an SVG string:

```rust
use kestrel_chartkit::viz::{render_chart_svg, ChartBarData, ChartRenderData, ChartSeries};

let data = ChartRenderData {
    title: "RSI input".to_string(),
    bars: bars.iter().map(ChartBarData::from).collect(),
    series: vec![ChartSeries {
        name: "SMA 20".to_string(),
        color: "#ffb74d".to_string(),
        points: sma_points, // Vec<(timestamp, value)>
    }],
    zones: Vec::new(),
    markers: Vec::new(),
};
std::fs::write("chart.svg", render_chart_svg(&data, 800, 400))?;
```

## Next steps

- [Indicators](../../guides/indicators/): the trait, the registry and the output fields.
- [Market regime](../../guides/market-regime/) and [Composite scoring](../../guides/composite-scoring/).
- [SVG charts](../../guides/svg-charts/): both renderers and the colour contract.
