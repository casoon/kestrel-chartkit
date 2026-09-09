//! On-demand analytical snapshots over chronological OHLCV bars.
//! These are pure computations: no provider, persistence, strategy scheduling or UI dependencies.
//! Callers provide finite input bars and applicable volume data. Timestamp values are not inspected.
//! Output enums/fields preserve the existing consumer wire representation with the serde feature.

mod activity;
mod cheat_sheet;
mod fear_gauge;
mod fear_greed;
mod price;
mod regime;
mod trend;
mod trend_persistence;

pub use activity::{activity_reading, ActivityReading};
pub use cheat_sheet::{
    cheat_sheet, CheatSheetLevel, CheatSheetLevelKind, CheatSheetLevelSide, CheatSheetReading,
};
pub use fear_gauge::{fear_gauge_reading, FearGaugeReading, FearGaugeState};
pub use fear_greed::{fear_greed_reading, FearGreedDriver, FearGreedReading, FearGreedState};
pub use price::{price_summary, PriceSummary};
pub use regime::{classify_regime as classify_trend_regime, RegimeReading, RegimeState};
pub use trend::{trend_reading, MarketPhase, TrendDirection, TrendReading};
pub use trend_persistence::{
    trend_persistence_reading, TrendPersistenceDirection, TrendPersistenceReading,
    TrendPersistenceSensor, TrendPersistenceState,
};

/// True range for a bar given the previous bar's close: the full
/// three-term form, or high−low on the very first bar. Shared by the
/// regime (Choppiness) and price (ATR) read-outs.
fn true_range(bar: &crate::Bar, prev_close: Option<f64>) -> f64 {
    match prev_close {
        None => bar.high - bar.low,
        Some(prev) => (bar.high - bar.low)
            .max((bar.high - prev).abs())
            .max((bar.low - prev).abs()),
    }
}

/// Kaufman Efficiency Ratio over the last `n` price changes (needs `n`+1
/// closes): net move ÷ summed absolute moves, 0 = pure chop, 1 = a perfectly
/// straight move. Shared by `regime` (regime vote) and `trend_persistence`
/// (one of its four trend sensors) — same formula, same window shape.
fn efficiency_ratio(bars: &[crate::Bar], n: usize) -> Option<f64> {
    if bars.len() < n + 1 {
        return None;
    }
    let closes = &bars[bars.len() - n - 1..];
    let net = (closes[closes.len() - 1].close - closes[0].close).abs();
    let mut volatility = 0.0;
    for pair in closes.windows(2) {
        volatility += (pair[1].close - pair[0].close).abs();
    }
    if volatility <= 0.0 {
        return None;
    }
    Some(net / volatility)
}

mod position;
pub use position::{atr_distance, position_alignment, PositionAlignment};
