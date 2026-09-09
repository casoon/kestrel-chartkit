#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::{Bar, Source};

use super::{Indicator, IndicatorOutput};

/// How the bars since the anchor are weighted into the average.
///
/// The two answer different questions and the published descriptions of "TWAP" do not always say
/// which one they mean, so it is named here rather than assumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TwapWeighting {
    /// Every bar counts once, whatever span it covers. The plain mean of the source since the
    /// anchor — which is what a "TWAP" drawn over a bar series usually is. The default.
    #[default]
    PerBar,
    /// Every price is weighted by how long it stood, measured as the gap to the next observation.
    /// On evenly spaced bars this equals [`TwapWeighting::PerBar`]; on gappy or irregular data it
    /// does not, and that difference is the reason the option exists.
    ByDuration,
}

/// Where the average starts over.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TwapAnchor {
    /// Never: one average over the whole series.
    Continuous,
    /// At the start of every UTC day, shifted by the given number of seconds.
    Daily { start_offset_seconds: i64 },
    /// At one fixed point in time. Bars before it produce no output.
    ManualTimestamp(i64),
}

/// Time Weighted Average Price since an anchor.
///
/// The average of a chosen price source over the bars since the anchor, weighted as
/// [`TwapWeighting`] says. It is **not** a VWAP: no volume enters this at any point, and a bar
/// that traded a thousand contracts counts exactly as much as one that traded ten. It is also not
/// an execution algorithm — this is a line on a chart, not a schedule for working an order.
///
/// Under [`TwapWeighting::ByDuration`] a price is taken to hold until the next bar arrives, so
/// its weight is the gap to that bar. The current bar's own price has not stood for any time yet
/// and therefore carries no weight; with a single bar since the anchor there is nothing to weight
/// at all, and the value is that bar's source price.
///
/// Gaps are what separates the two weightings: a missing hour makes the preceding price count for
/// that hour under `ByDuration`, and count once under `PerBar`. Neither is wrong, and neither is
/// guessed at here.
///
/// Output: `value`, in the price units of the series. First output: the first bar at or after the
/// anchor. [`Indicator::reset`] clears the average.
#[derive(Debug, Clone)]
pub struct AnchoredTwap {
    anchor: TwapAnchor,
    source: Source,
    weighting: TwapWeighting,
    sum: f64,
    weight: f64,
    bars: usize,
    previous: Option<(i64, f64)>,
    current_period: Option<i64>,
}

impl AnchoredTwap {
    pub fn new(anchor: TwapAnchor, source: Source, weighting: TwapWeighting) -> Self {
        Self {
            anchor,
            source,
            weighting,
            sum: 0.0,
            weight: 0.0,
            bars: 0,
            previous: None,
            current_period: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(
            TwapAnchor::Daily {
                start_offset_seconds: 0,
            },
            Source::Close,
            TwapWeighting::PerBar,
        )
    }

    fn restart(&mut self) {
        self.sum = 0.0;
        self.weight = 0.0;
        self.bars = 0;
        self.previous = None;
    }
}

impl Indicator for AnchoredTwap {
    fn name(&self) -> &str {
        "twap"
    }

    fn warmup_period(&self) -> usize {
        0
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        match self.anchor {
            TwapAnchor::Continuous => {}
            TwapAnchor::Daily {
                start_offset_seconds,
            } => {
                let period = (bar.timestamp - start_offset_seconds).div_euclid(86_400);
                if self.current_period != Some(period) {
                    self.current_period = Some(period);
                    self.restart();
                }
            }
            TwapAnchor::ManualTimestamp(anchor) => {
                if bar.timestamp < anchor {
                    return None;
                }
            }
        }

        let price = self.source.extract(bar);
        if !price.is_finite() {
            return None;
        }

        match self.weighting {
            TwapWeighting::PerBar => {
                self.sum += price;
                self.bars += 1;
            }
            TwapWeighting::ByDuration => {
                if let Some((previous_timestamp, previous_price)) = self.previous {
                    let duration = (bar.timestamp - previous_timestamp).max(0) as f64;
                    self.sum += previous_price * duration;
                    self.weight += duration;
                }
            }
        }
        self.previous = Some((bar.timestamp, price));

        let value = match self.weighting {
            TwapWeighting::PerBar => self.sum / self.bars as f64,
            // Nothing has stood for any time yet: the single observation is the average.
            TwapWeighting::ByDuration if self.weight <= 0.0 => price,
            TwapWeighting::ByDuration => self.sum / self.weight,
        };
        Some(IndicatorOutput::new(value))
    }

    fn reset(&mut self) {
        self.restart();
        self.current_period = None;
    }
}
