# kestrel-chartkit

High-performance Rust technical analysis library for streaming indicator math, market regime classification, composite signal scoring, trade evaluation, and static SVG visualization.

The crate is currently an **0.1 alpha**. Root-level re-exports are the preferred consumer API;
lower-level modules remain public for advanced composition but may change before 1.0.

## Features

- **35+ Streaming Technical Indicators:** RSI, MACD, ATR, ADX, Bollinger Bands, Volume Profile, VWAP, Ichimoku, Supertrend, Stochastic RSI, Order Block detection, Liquidity FVG, Pivots Structure, and more.
- **Dynamic Catalog Registry:** Parameter validation and dynamic instantiation via `catalog()` and `build_checked(name, params)`.
- **Market Regime Alignment:** Automatic regime classification (`BullishExpansion`, `BearishExpansion`, `Consolidation`, `Transition`) with permission grading (`ClearToTrade`, `Caution`, `Veto`).
- **Composite Signal Scoring:** Weighted multi-indicator scoring, risk management parameter generation (entry, stop-loss, take-profit targets), and semantic neutral signal cleanup.
- **Trade Statistics & Evaluation:** Comprehensive backtest evaluation ($R$-multiples, winrate, profit factor, max drawdown, EV).
- **SVG Chart Renderer:** Export clean SVG preview charts with candlestick series, indicator polylines, market structure zones, and timestamped signal markers.

## Quickstart

```rust
use kestrel_chartkit::{build_checked, Bar, Indicator};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut params = HashMap::new();
    params.insert("rsi_len".to_string(), 14.0);

    let mut rsi = build_checked("rsi", &params)?;
    let bar = Bar::try_new(1700000000, 100.0, 105.0, 95.0, 104.0, 1000.0)?;

    if let Some(output) = rsi.on_bar(&bar) {
        println!("RSI Value: {:.2}", output.value);
    }

    Ok(())
}
```

See `examples/basic_indicator.rs` (`cargo run --example basic_indicator`) for a runnable version
that streams a full bar series through warmup.

## Input and configuration contract

- Use `Bar::try_new` or `Bar::validate` at ingestion boundaries. OHLC prices must be finite and
  positive, volume must be finite and non-negative, and the high/low range must contain open and
  close. `Bar::new` is intentionally unchecked for trusted feeds and compatibility.
- Use `Indicator::on_checked_bar` when a consumer cannot guarantee validated input.
- Prefer `build_checked` for configuration-driven construction. Periods are whole numbers in the
  supported range; invalid thresholds and parameter orderings return `RegistryError`.
- Composite scoring discards non-finite subscores, bounds weights, validates trade-geometry bars,
  and falls back to one percent of price when ATR is not finite and positive.

## Cargo features

The default `serde` feature derives `Serialize` and `Deserialize` for public DTOs. Disable it for a
smaller dependency graph:

```toml
kestrel-chartkit = { version = "0.1", default-features = false }
```

The optional `calendar` feature adds `src/calendar.rs` (`ExchangeCalendar`): IANA-timezone/DST-aware
trading sessions, holidays, and early closes, via `chrono`/`chrono-tz`. Off by default so the core
crate carries no timezone-database dependency:

```toml
kestrel-chartkit = { version = "0.1", features = ["calendar"] }
```

## Testing & Quality

Run the test suite:

```bash
cargo test
for f in tests/golden_reference_*.rs tests/scenario_reference_structure.rs; do
  cargo test --test "$(basename "$f" .rs)"
done
cargo check --no-default-features
cargo fmt --all -- --check
cargo clippy --all-targets --all-features -- -D warnings
```

## License

Licensed under the Business Source License 1.1 (`BUSL-1.1`), see [`LICENSE`](LICENSE). Free for
non-commercial use (including production use in private, academic, non-profit, and open-source
projects not offered as part of a commercial product or service); commercial use requires a
license from the Licensor. Converts to Apache-2.0 four years after publication.
