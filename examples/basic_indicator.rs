//! Minimal streaming-indicator walkthrough: build an indicator from the registry by name,
//! feed it a series of validated bars one at a time, and read its output as each bar closes.
//!
//! Run with:
//! ```bash
//! cargo run --example basic_indicator
//! ```

use kestrel_chartkit::{build_checked, Bar, Indicator};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // `build_checked` validates parameters against the registry's bounds/ordering rules for the
    // named indicator (see `kestrel_chartkit::catalog()` for the full list of names and their
    // default parameters) and returns a boxed `Indicator` trait object.
    let mut params = HashMap::new();
    params.insert("rsi_len".to_string(), 14.0);
    let mut rsi = build_checked("rsi", &params)?;

    // A short synthetic closing-price series. Real feeds should validate bars at the ingestion
    // boundary with `Bar::try_new` (as here) rather than the unchecked `Bar::new`.
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

        // `on_bar` returns `None` while the indicator is still warming up (fewer bars seen than
        // its `warmup_period()`); once warmed up it returns `Some` on every subsequent bar.
        match rsi.on_bar(&bar) {
            Some(output) => println!("bar {i:>2}: close={close:.2}  RSI(14)={:.2}", output.value),
            None => println!("bar {i:>2}: close={close:.2}  RSI(14)=<warming up>"),
        }
    }

    Ok(())
}
