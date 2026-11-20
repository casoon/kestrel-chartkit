//! On-demand analytical snapshots over chronological OHLCV bars.
//! These are pure computations: no provider, persistence, strategy scheduling or UI dependencies.
//! Callers provide finite input bars and applicable volume data. Timestamp values are not inspected.
//! Output enums/fields preserve the existing consumer wire representation with the serde feature.

mod activity;
mod cheat_sheet;
mod fear_gauge;
mod fear_greed;
pub mod forward;
mod price;
pub mod price_location;
mod regime;
mod seasonality;
mod trend;
mod trend_persistence;

pub use activity::{activity_reading, ActivityReading};
pub use cheat_sheet::{
    cheat_sheet, CheatSheetLevel, CheatSheetLevelKind, CheatSheetLevelSide, CheatSheetReading,
};
pub use fear_gauge::{fear_gauge_reading, FearGaugeReading, FearGaugeState};
pub use fear_greed::{fear_greed_reading, FearGreedDriver, FearGreedReading, FearGreedState};
pub use price::{price_summary, PriceSummary};
#[allow(deprecated)]
pub use price_location::{
    price_location_series, true_range, wilder_atr_series, PriceLocation, PriceLocationConfig,
    PriceLocationEngine, WilderAtr,
};
pub use regime::{vote_regime, RegimeReading, RegimeState};
pub use seasonality::{monthly_seasonality, MonthStatistics, MonthlyReturn, SeasonalityReport};
pub use trend::{trend_reading, MarketPhase, TrendDirection, TrendReading};
pub use trend_persistence::{
    trend_persistence_reading, TrendPersistenceDirection, TrendPersistenceReading,
    TrendPersistenceSensor, TrendPersistenceState,
};

/// Former name of [`vote_regime`]; the name clashed with [`crate::regime::classify_regime`], which
/// is a different classifier.
#[deprecated(since = "0.43.0", note = "renamed to `vote_regime`")]
pub fn classify_trend_regime(
    bars: &[crate::Bar],
    adx_len: usize,
    window: usize,
) -> Option<RegimeReading> {
    vote_regime(bars, adx_len, window)
}

/// True range for a bar given the previous bar's close: the full
/// three-term form, or high−low on the very first bar. Shared by the
/// regime (Choppiness) and price (ATR) read-outs.
fn bar_true_range(bar: &crate::Bar, prev_close: Option<f64>) -> f64 {
    crate::indicator::kernels::true_range(bar, prev_close)
}

/// Kaufman Efficiency Ratio over the last `n` price changes (needs `n`+1
/// closes): net move ÷ summed absolute moves, 0 = pure chop, 1 = a perfectly
/// straight move. Shared by `regime` (regime vote) and `trend_persistence`
/// (one of its four trend sensors) — same formula, same window shape.
fn efficiency_ratio(bars: &[crate::Bar], n: usize) -> Option<f64> {
    if bars.len() < n + 1 {
        return None;
    }
    crate::indicator::kernels::efficiency_ratio(bars[bars.len() - n - 1..].iter().map(|b| b.close))
}

mod position;
pub use position::{atr_distance, position_alignment, PositionAlignment};
