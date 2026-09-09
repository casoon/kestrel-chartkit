use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorOutput};

/// Which prices the Relative Volatility Index is measured on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum RelativeVolatilityVariant {
    /// The closing price alone. The original construction.
    #[default]
    Close,
    /// The same measurement run separately on highs and on lows, then averaged. The later
    /// revision, which reacts to the range rather than to one price per bar.
    HighLow,
}

/// Relative Volatility Index: the Relative Strength Index construction applied to *volatility*
/// instead of to price change.
///
/// For each bar the standard deviation of the last `stdev_len` prices is computed. That figure is
/// then filed under "up" or "down" depending on which way the price moved, and the two are
/// Wilder-smoothed over `smooth_len`:
///
/// ```text
/// up_t    = stdev_t if price rose, else 0
/// down_t  = stdev_t if price fell, else 0
/// RVI     = 100 * rma(up) / (rma(up) + rma(down))
/// ```
///
/// So it answers "is the recent movement concentrated on the up side or the down side", where
/// *movement* means dispersion, not distance. A market that falls steadily with little scatter
/// can therefore read differently from an RSI on the same bars.
///
/// **This is not the Relative Vigor Index**, which this crate registers as `rvi` and which
/// compares the close-open span to the high-low span. Same three letters, unrelated measurement;
/// hence the separate name.
///
/// No claim of numerical agreement with any other implementation is made: the published
/// descriptions of this indicator leave the direction rule, the smoothing and the seed open, and
/// what is implemented here is the contract stated above.
///
/// Unit: `0..=100`. A bar where the price did not move contributes to neither side. A window in
/// which nothing moved at all leaves both averages at zero; the documented convention there is
/// `50`, the same neutral reading this crate's RSI uses.
///
/// First output: once both the deviation window and the Wilder averages are ready, i.e. after
/// `stdev_len + smooth_len` bars. [`Indicator::reset`] clears the window and both averages.
#[derive(Debug, Clone)]
pub struct RelativeVolatilityIndex {
    stdev_len: usize,
    smooth_len: usize,
    variant: RelativeVolatilityVariant,
    close: DirectionalDeviation,
    high: DirectionalDeviation,
    low: DirectionalDeviation,
}

/// One price series' dispersion, split by the direction that series moved.
#[derive(Debug, Clone)]
struct DirectionalDeviation {
    len: usize,
    window: VecDeque<f64>,
    previous: Option<f64>,
    up: Rma,
    down: Rma,
}

impl DirectionalDeviation {
    fn new(len: usize, smooth_len: usize) -> Self {
        Self {
            len,
            window: VecDeque::with_capacity(len),
            previous: None,
            up: Rma::new(smooth_len),
            down: Rma::new(smooth_len),
        }
    }

    fn update(&mut self, value: f64) -> Option<f64> {
        self.window.push_back(value);
        if self.window.len() > self.len {
            self.window.pop_front();
        }
        let previous = self.previous.replace(value);
        if self.window.len() < self.len {
            return None;
        }

        // Population standard deviation over the centred window, the same form the band
        // calculation in this crate uses.
        let mean = self.window.iter().sum::<f64>() / self.len as f64;
        let variance = self
            .window
            .iter()
            .map(|entry| {
                let diff = entry - mean;
                diff * diff
            })
            .sum::<f64>()
            / self.len as f64;
        let deviation = variance.sqrt();

        let previous = previous?;
        let (up, down) = if value > previous {
            (deviation, 0.0)
        } else if value < previous {
            (0.0, deviation)
        } else {
            (0.0, 0.0)
        };

        // Beide Glätter müssen jede Beobachtung sehen. Ein `?` auf dem ersten würde den zweiten
        // während des Warmups überspringen, und die beiden Zustände liefen um die Warmup-Länge
        // auseinander.
        let up_avg = self.up.update(up);
        let down_avg = self.down.update(down);
        let (up_avg, down_avg) = (up_avg?, down_avg?);
        let total = up_avg + down_avg;
        Some(if total > 0.0 {
            100.0 * up_avg / total
        } else {
            50.0
        })
    }

    fn reset(&mut self) {
        self.window.clear();
        self.previous = None;
        self.up.reset();
        self.down.reset();
    }
}

impl RelativeVolatilityIndex {
    pub fn new(stdev_len: usize, smooth_len: usize, variant: RelativeVolatilityVariant) -> Self {
        let stdev_len = stdev_len.max(2);
        let smooth_len = smooth_len.max(1);
        Self {
            stdev_len,
            smooth_len,
            variant,
            close: DirectionalDeviation::new(stdev_len, smooth_len),
            high: DirectionalDeviation::new(stdev_len, smooth_len),
            low: DirectionalDeviation::new(stdev_len, smooth_len),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 14, RelativeVolatilityVariant::Close)
    }

    pub fn variant(&self) -> RelativeVolatilityVariant {
        self.variant
    }
}

impl Indicator for RelativeVolatilityIndex {
    fn name(&self) -> &str {
        "relative_volatility"
    }

    fn warmup_period(&self) -> usize {
        self.stdev_len + self.smooth_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        match self.variant {
            RelativeVolatilityVariant::Close => {
                self.close.update(bar.close).map(IndicatorOutput::new)
            }
            RelativeVolatilityVariant::HighLow => {
                // Both sides are advanced on every bar; the average is only published once both
                // have a value, which they reach together.
                let high = self.high.update(bar.high);
                let low = self.low.update(bar.low);
                match (high, low) {
                    (Some(high), Some(low)) => Some(IndicatorOutput::new((high + low) / 2.0)),
                    _ => None,
                }
            }
        }
    }

    fn reset(&mut self) {
        self.close.reset();
        self.high.reset();
        self.low.reset();
    }
}
