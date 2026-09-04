//! Generic `Source` propagation for any [`Indicator`].
//!
//! Every concrete indicator computes over `bar.close` (or another hardcoded OHLCV field)
//! internally; [`Source`] existed as a type but had no way to reach an indicator's computation.
//! [`SourceMapped`] closes that gap generically, for the whole catalog at once, instead of
//! threading a `source` field through every indicator's internals: it rewrites each incoming bar
//! to a synthetic one where every OHLC field equals the selected source's extracted value
//! (volume is preserved), then forwards that to the wrapped indicator. Any indicator that reduces
//! its input to a single per-bar scalar (moving averages, oscillators, ...) transparently starts
//! computing over the chosen source.
//!
//! This intentionally does **not** make sense for indicators that need genuine OHLC range data
//! (True Range/ATR, volume/price profiles, structure/pivot detection): source-mapping them would
//! flatten `high == low == open == close`, degenerating their range-dependent math. That mirrors
//! Pine itself, where a `source` parameter is only ever offered on scalar-series functions.

use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::{Bar, BarValidationError, Source};

/// Wraps `inner`, feeding it a synthetic bar of `source.extract(bar)` on every OHLC field
/// (volume unchanged) instead of the original bar.
pub struct SourceMapped<I: Indicator> {
    inner: I,
    source: Source,
}

impl<I: Indicator> SourceMapped<I> {
    pub fn new(inner: I, source: Source) -> Self {
        Self { inner, source }
    }

    fn map_bar(&self, bar: &Bar) -> Bar {
        let value = self.source.extract(bar);
        Bar::new(bar.timestamp, value, value, value, value, bar.volume)
    }
}

impl<I: Indicator> Indicator for SourceMapped<I> {
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn warmup_period(&self) -> usize {
        self.inner.warmup_period()
    }
    fn reset(&mut self) {
        self.inner.reset()
    }
    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let mapped = self.map_bar(bar);
        self.inner.on_bar(&mapped)
    }
    fn on_checked_bar(&mut self, bar: &Bar) -> Result<Option<IndicatorOutput>, BarValidationError> {
        bar.validate()?;
        Ok(self.on_bar(bar))
    }
    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.inner.alerts()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::moving_averages::SmaEngine;

    #[test]
    fn test_source_mapped_uses_selected_source_not_close() {
        let mut close_sma = SmaEngine::new(2);
        let mut open_sma = SourceMapped::new(SmaEngine::new(2), Source::Open);

        // open/close deliberately diverge so the two SMAs must disagree once source-mapped.
        let bars = [
            Bar::new(0, 10.0, 12.0, 8.0, 11.0, 100.0),
            Bar::new(60, 20.0, 22.0, 18.0, 21.0, 100.0),
        ];

        let mut close_out = None;
        let mut open_out = None;
        for bar in &bars {
            close_out = close_sma.on_bar(bar);
            open_out = open_sma.on_bar(bar);
        }

        assert_eq!(close_out.unwrap().value, (11.0 + 21.0) / 2.0);
        assert_eq!(open_out.unwrap().value, (10.0 + 20.0) / 2.0);
    }

    #[test]
    fn test_source_mapped_preserves_volume_and_delegates_metadata() {
        let mut hl2_sma = SourceMapped::new(SmaEngine::new(1), Source::Hl2);
        let bar = Bar::new(0, 10.0, 20.0, 0.0, 15.0, 250.0);
        // Hl2 = (high + low) / 2 = 10.0
        let out = hl2_sma.on_bar(&bar).unwrap();
        assert_eq!(out.value, 10.0);
        assert_eq!(hl2_sma.name(), "sma");
        assert_eq!(hl2_sma.warmup_period(), SmaEngine::new(1).warmup_period());
    }
}
