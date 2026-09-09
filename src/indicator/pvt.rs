use crate::model::Bar;

use super::{Indicator, IndicatorOutput};

/// Price Volume Trend: volume weighted by the *relative* price change, accumulated.
///
/// `PVT_t = PVT_{t-1} + volume_t * (close_t / close_{t-1} - 1)`
///
/// The distinction to its neighbours is the weighting, and it is the whole point:
/// - **OBV** adds the full volume with the sign of the change — a one-cent move counts as much as
///   a five-percent one.
/// - **Elder's Force Index** weights by the *absolute* change, so the same percentage move counts
///   more at a higher price level.
/// - PVT weights by the relative change, which makes its steps comparable across price levels
///   within one series.
///
/// Unit: volume units. A cumulative total is only meaningful relative to itself — its level says
/// nothing without the series it was accumulated over, and comparing the level between two
/// instruments compares their volume conventions, not their flows.
///
/// The running total starts at zero on the first bar that has a predecessor. There is no output
/// on the very first bar of a series: without a previous close there is no return, and a zero
/// would claim one. [`Indicator::reset`] returns the total to zero, so a series switch starts a
/// fresh accumulation rather than carrying a level across a boundary it has no meaning over.
#[derive(Debug, Clone, Default)]
pub struct PriceVolumeTrend {
    prev_close: Option<f64>,
    total: f64,
}

impl PriceVolumeTrend {
    pub fn new() -> Self {
        Self::default()
    }
}

impl Indicator for PriceVolumeTrend {
    fn name(&self) -> &str {
        "pvt"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev_close = match self.prev_close {
            None => {
                self.prev_close = Some(bar.close);
                return None;
            }
            Some(prev) => prev,
        };
        self.prev_close = Some(bar.close);

        if prev_close != 0.0 {
            self.total += bar.volume * (bar.close / prev_close - 1.0);
        }
        Some(IndicatorOutput::new(self.total))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.total = 0.0;
    }
}
