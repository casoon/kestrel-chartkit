use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::intrabar::IntrabarGroup;
use crate::model::Bar;

/// Where a bar's split into buying and selling volume came from.
///
/// The three are not interchangeable, and the whole point of naming them is that a consumer can
/// tell which one it is looking at. A cumulative delta built on the first reads like one built on
/// the third, and only this label says otherwise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum DeltaProvenance {
    /// Estimated from the shape of a single bar — where the close sits inside its range. Needs
    /// nothing but the bar itself, and cannot distinguish aggressive buying from selling into
    /// absorption: both can close at the high. This is what [`super::volume_flow::CvdEngine`]
    /// does, and it remains this crate's default.
    BarShape,
    /// Estimated from the sequence of lower-timeframe bars inside the parent bar: each child's
    /// direction decides where its volume is booked. Finer than [`DeltaProvenance::BarShape`] —
    /// a reversal inside the bar becomes visible — but still an estimate, because a child bar is
    /// itself an aggregate and its own direction is inferred, not observed.
    IntrabarShape,
    /// Measured from classified individual trades, where each print carries the side that
    /// initiated it. The only one of the three that measures the aggressor rather than inferring
    /// it. No indicator in this crate produces it; the variant exists so data that does have it
    /// can say so.
    ClassifiedTrades,
}

impl fmt::Display for DeltaProvenance {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BarShape => "bar shape",
            Self::IntrabarShape => "intrabar shape",
            Self::ClassifiedTrades => "classified trades",
        })
    }
}

/// How an intrabar whose close did not move is booked.
///
/// A child bar that closes where it opened — or where the previous child closed — carries volume
/// that this construction cannot attribute. Guessing a side would manufacture exactly the
/// information the measurement is missing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum UnchangedIntrabarPolicy {
    /// Its volume contributes nothing to the delta, and is counted as unattributed. The default.
    #[default]
    Unattributed,
    /// Its volume keeps the direction of the previous attributed child, or is unattributed if
    /// there is none. A deliberate assumption of continuation, not a measurement.
    CarryPreviousDirection,
}

/// Cumulative volume delta of one anchor period, built from lower-timeframe bars.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IntrabarDelta {
    /// Parent bucket this belongs to.
    pub timestamp: i64,
    /// Buying minus selling volume of this parent bar alone.
    pub delta: f64,
    /// Volume booked as buying.
    pub buy_volume: f64,
    /// Volume booked as selling.
    pub sell_volume: f64,
    /// Volume that could not be attributed to either side under the chosen policy. It is reported
    /// rather than distributed, because distributing it would invent a side.
    pub unattributed_volume: f64,
    /// The cumulative delta at the parent's open, high, low and close — the four corners of the
    /// running total *within the anchor period*. Together they say whether the total moved
    /// straight or swung on its way.
    pub cumulative_open: f64,
    pub cumulative_high: f64,
    pub cumulative_low: f64,
    pub cumulative_close: f64,
    /// How many child bars this parent was built from. A parent with one child is the parent bar
    /// itself and carries no intrabar information.
    pub children: usize,
    /// Always [`DeltaProvenance::IntrabarShape`] here. Carried so a consumer that mixes sources
    /// keeps them apart without tracking which call produced which row.
    pub provenance: DeltaProvenance,
}

/// When the cumulative delta returns to zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum DeltaAnchor {
    /// Never: one running total across the whole series. The default.
    #[default]
    Continuous,
    /// At the start of every UTC day, shifted by the given number of seconds.
    Daily { start_offset_seconds: i64 },
}

/// Cumulative volume delta from lower-timeframe bars.
///
/// Each child bar is booked to one side by comparing its close to the previous child's close —
/// within a parent bar this is a sequence, so the direction of each step is what the data
/// supports. The first child of a series has no predecessor and is compared to its own open
/// instead.
///
/// What this construction cannot do is measure the aggressor. A child bar is an aggregate like
/// any other; its direction is inferred. The result is therefore
/// [`DeltaProvenance::IntrabarShape`], never [`DeltaProvenance::ClassifiedTrades`], and volume
/// that no rule attributes is reported as unattributed rather than split.
///
/// [`super::volume_flow::CvdEngine`] is untouched by this and remains the default for callers who
/// only have parent bars.
#[derive(Debug, Clone)]
pub struct IntrabarCvd {
    anchor: DeltaAnchor,
    unchanged_policy: UnchangedIntrabarPolicy,
    cumulative: f64,
    previous_close: Option<f64>,
    previous_direction: f64,
    current_day: Option<i64>,
}

impl IntrabarCvd {
    pub fn new(anchor: DeltaAnchor, unchanged_policy: UnchangedIntrabarPolicy) -> Self {
        Self {
            anchor,
            unchanged_policy,
            cumulative: 0.0,
            previous_close: None,
            previous_direction: 0.0,
            current_day: None,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(DeltaAnchor::Continuous, UnchangedIntrabarPolicy::default())
    }

    /// The running cumulative delta, as it stands.
    pub fn cumulative(&self) -> f64 {
        self.cumulative
    }

    fn anchor_period(&self, timestamp: i64) -> Option<i64> {
        match self.anchor {
            DeltaAnchor::Continuous => None,
            DeltaAnchor::Daily {
                start_offset_seconds,
            } => Some((timestamp - start_offset_seconds).div_euclid(86_400)),
        }
    }

    /// Books one parent bucket's children and returns its delta together with the four corners of
    /// the cumulative total.
    pub fn on_group(&mut self, group: &IntrabarGroup) -> IntrabarDelta {
        if let Some(period) = self.anchor_period(group.parent_timestamp) {
            if self.current_day != Some(period) {
                self.current_day = Some(period);
                self.cumulative = 0.0;
                self.previous_close = None;
                self.previous_direction = 0.0;
            }
        }

        let cumulative_open = self.cumulative;
        let (mut high, mut low) = (self.cumulative, self.cumulative);
        let (mut buy, mut sell, mut unattributed) = (0.0, 0.0, 0.0);

        for child in &group.children {
            let reference = self.previous_close.unwrap_or(child.open);
            let direction = if child.close > reference {
                1.0
            } else if child.close < reference {
                -1.0
            } else {
                match self.unchanged_policy {
                    UnchangedIntrabarPolicy::Unattributed => 0.0,
                    UnchangedIntrabarPolicy::CarryPreviousDirection => self.previous_direction,
                }
            };
            self.previous_close = Some(child.close);
            if direction != 0.0 {
                self.previous_direction = direction;
            }

            let volume = child.volume.max(0.0);
            if direction > 0.0 {
                buy += volume;
            } else if direction < 0.0 {
                sell += volume;
            } else {
                unattributed += volume;
            }

            self.cumulative += direction * volume;
            high = high.max(self.cumulative);
            low = low.min(self.cumulative);
        }

        IntrabarDelta {
            timestamp: group.parent_timestamp,
            delta: buy - sell,
            buy_volume: buy,
            sell_volume: sell,
            unattributed_volume: unattributed,
            cumulative_open,
            cumulative_high: high,
            cumulative_low: low,
            cumulative_close: self.cumulative,
            children: group.children.len(),
            provenance: DeltaProvenance::IntrabarShape,
        }
    }

    /// Books a parent bar for which no children are available.
    ///
    /// The bar is *not* treated as one intrabar of its own: doing so would present an estimate
    /// from the parent's shape as if it had come from the finer data. The volume is reported as
    /// unattributed, the cumulative total does not move, and `children` is zero — which is how a
    /// consumer sees that this parent contributed nothing measurable.
    pub fn on_missing_children(&mut self, parent: &Bar) -> IntrabarDelta {
        if let Some(period) = self.anchor_period(parent.timestamp) {
            if self.current_day != Some(period) {
                self.current_day = Some(period);
                self.cumulative = 0.0;
                self.previous_close = None;
                self.previous_direction = 0.0;
            }
        }

        IntrabarDelta {
            timestamp: parent.timestamp,
            delta: 0.0,
            buy_volume: 0.0,
            sell_volume: 0.0,
            unattributed_volume: parent.volume.max(0.0),
            cumulative_open: self.cumulative,
            cumulative_high: self.cumulative,
            cumulative_low: self.cumulative,
            cumulative_close: self.cumulative,
            children: 0,
            provenance: DeltaProvenance::IntrabarShape,
        }
    }

    pub fn reset(&mut self) {
        self.cumulative = 0.0;
        self.previous_close = None;
        self.previous_direction = 0.0;
        self.current_day = None;
    }
}
