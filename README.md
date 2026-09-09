# kestrel-chartkit

High-performance Rust technical analysis library for streaming indicator math, market regime
classification, composite signal scoring, trade evaluation, instrument valuation, and static SVG
visualization.

The crate is at **0.5**, pre-1.0. Root-level re-exports are the preferred consumer API;
lower-level modules remain public for advanced composition but may change before 1.0. Breaking
changes are marked with `!` in the commit subject and raise the minor version.

## Features

- **96 Streaming Technical Indicators:** RSI, MACD, ATR, ADX, Bollinger Bands, TRIX, VIDYA, Tillson T3, Chande Kroll Stop, Elder's Force Index, Volume Profile, VWAP, Ichimoku, Supertrend, Stochastic RSI, Order Block detection, Liquidity FVG, Pivots Structure, and more (see `indicator::registry::catalog()` for the full, validated list).
- **Dynamic Catalog Registry:** Parameter validation and dynamic instantiation via `catalog()` and `build_checked(name, params)`.
- **Market Regime Alignment:** Automatic regime classification (`BullishExpansion`, `BearishExpansion`, `Consolidation`, `Transition`) with permission grading (`ClearToTrade`, `Caution`, `Veto`).
- **Composite Signal Scoring:** Weighted multi-indicator scoring, risk management parameter generation (entry, stop-loss, take-profit targets), and semantic neutral signal cleanup.
- **Trade Statistics & Evaluation:** Comprehensive backtest evaluation ($R$-multiples, winrate, profit factor, max drawdown, EV).
- **Bar Transformations:** Heikin-Ashi candles as their own result type, carrying the observed bar they were derived from so computed prices cannot pass as traded ones (`transform`).
- **Instrument Valuation:** European options (Black-Scholes-Merton, Black-76, Greeks, implied volatility) and fixed-rate bonds valued over real coupon schedules — month-end rule, stub periods, business-day conventions, day-count-driven coupons and accrued interest (`option`, `finance`).
- **Portfolio, Risk and Stress:** Exposures, cashflow-adjusted returns, drawdown, VaR and expected shortfall, scenario revaluation and path simulation (`portfolio`, `risk`, `stress`).
- **SVG Chart Renderer:** Export clean SVG preview charts with candlestick series, indicator polylines, market structure zones, and timestamped signal markers.

## Shared consumer calculations

`correlation_matrix` and `relative_strength_ranking` accept close samples with a common
caller-defined timestamp convention. `aggregate_agreement` combines directional statements;
its confidence is agreement, not a calibrated success probability.

`PriceDirection`, `ForwardPriceOutcome`, `PriceStats` and `PriceOutcomeStats` provide
price-unit paper/outcome calculations. They do not imply contract sizing, account-currency
P&L or intrabar fills. Consumers retain strategy decisions, persistence and scheduling.

`analytics` provides on-demand regime votes, price/ATR summaries, smoothed trend,
trend persistence, activity, dual VIX Fix sentiment and price levels. These preserve the
consumer snapshot conventions; they are distinct from similarly named streaming indicators
where warmup, clipping or model formulas differ. Existing EMA/RMA/ADX/smoothing kernels are reused.

## Instrument data contract

Where an instrument's terms come from is the consumer's business; what follows from them is this
crate's. The line runs through the specification types:

- `ContractSpec` carries currency, multiplier and quantity steps — how a contract trades.
- `BondSpec` carries a bond's terms: issue and maturity, coupon rate and frequency, day count,
  stub placement and business-day convention. `BondSpec::build` turns it into a valued
  `FixedRateBond`, and rejects an inconsistent product record there rather than in a price.
- `SeriesIdentity` and `SeriesCapabilities` say which series a result was computed on, and
  `applicability::check_applicability` says whether an indicator's requirements fit it.

Three things stay outside those types on purpose. **Holidays** are passed per valuation as a
`BusinessCalendar`, because they are market data with their own validity rather than a property of
the instrument — and no market calendars ship with this crate, since a stale bundled list looks
authoritative while being wrong. **Prices and yields** are observations, not terms. And **currency
appears once**, in `ContractSpec`, so there is no second truth about the same instrument.

```rust
use kestrel_chartkit::{BondSpec, BusinessCalendar, Date, DayCountConvention};

let spec = BondSpec::new(
    1_000.0,
    0.05,
    2,
    Date::new(2026, 6, 15).unwrap(),
    Date::new(2031, 6, 15).unwrap(),
    DayCountConvention::Actual365Fixed,
);
let bond = spec.build(&BusinessCalendar::weekends_only())?;
let priced = bond.price(Date::new(2026, 9, 20).unwrap(), 0.04)?;
println!("clean {:.4}, accrued {:.4}", priced.clean_price, priced.accrued_interest);
```

See `examples/bond_contract.rs` (`cargo run --example bond_contract`) for the same walk through
coupon dates, business-day adjustment and sensitivities.

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
- Valuation results are only as good as the conventions they were given. A day count is never
  assumed for a bond, and coupon amounts follow it: under Actual/365 a 183-day period pays more
  than a 182-day one, under 30/360 both pay the same.

## Cargo features

The default `serde` feature derives `Serialize` and `Deserialize` for public DTOs. Disable it for a
smaller dependency graph:

```toml
kestrel-chartkit = { version = "0.5", default-features = false }
```

The optional `calendar` feature adds `src/calendar.rs` (`ExchangeCalendar`): IANA-timezone/DST-aware
trading sessions, holidays, and early closes, via `chrono`/`chrono-tz`. Off by default so the core
crate carries no timezone-database dependency. It is about *trading hours*; the settlement holidays
a bond schedule needs are supplied as a `BusinessCalendar` instead:

```toml
kestrel-chartkit = { version = "0.5", features = ["calendar"] }
```

## Testing & Quality

Numeric results are pinned against independently derived reference values — a second
implementation of the documented formula, exact rational or decimal arithmetic, or an analytically
unambiguous case — never against this crate's own output. Where a model deviates from its
reference on purpose, the deviation is measured and stated rather than absorbed into a wide
tolerance.

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

Run the indicator benchmark suite (streams every catalog indicator over a synthetic bar series,
also run in CI as a separate `benchmark` job that uploads the Criterion HTML report as an artifact):

```bash
cargo bench
```

## License

Licensed under the Business Source License 1.1 (`BUSL-1.1`), see [`LICENSE`](LICENSE). Free for
non-commercial use (including production use in private, academic, non-profit, and open-source
projects not offered as part of a commercial product or service); commercial use requires a
license from the Licensor. Converts to Apache-2.0 four years after publication.
