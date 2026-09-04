//! Lower-timeframe (intrabar) child-bar grouping.
//!
//! [`BarResampler`](crate::timeframe::BarResampler) goes from lower-timeframe (LTF) bars to an
//! *aggregated* higher-timeframe (HTF) bar. Some Pine calculations
//! (`request.security_lower_tf`-style: intrabar delta, aggressor volume, absorption) instead need
//! the **full ordered sequence** of LTF child bars that belong to each HTF parent bucket, not just
//! their aggregate. [`IntrabarGrouper`] provides that, reusing the same `Timeframe::bucket_start`
//! bucketing [`BarResampler`] uses, so the two stay consistent with each other.

use crate::model::Bar;
use crate::timeframe::{Timeframe, TimeframeError};

/// A parent bucket's complete, time-ordered child-bar sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct IntrabarGroup {
    /// The parent bucket's open timestamp (same convention as
    /// [`crate::timeframe::ResamplerOutput::completed_bar`]).
    pub parent_timestamp: i64,
    /// Child bars in arrival order. Never empty.
    pub children: Vec<Bar>,
}

/// Groups incoming lower-timeframe (child) bars by the higher-timeframe (parent) bucket they fall
/// into, using the same bucketing as [`crate::timeframe::BarResampler`].
#[derive(Debug, Clone)]
pub struct IntrabarGrouper {
    parent_tf: Timeframe,
    utc_offset_seconds: i32,
    current_parent_start: Option<i64>,
    current_children: Vec<Bar>,
}

impl IntrabarGrouper {
    pub fn new(parent_tf: Timeframe) -> Result<Self, TimeframeError> {
        Self::with_utc_offset(parent_tf, 0)
    }

    pub fn with_utc_offset(
        parent_tf: Timeframe,
        utc_offset_seconds: i32,
    ) -> Result<Self, TimeframeError> {
        Ok(Self {
            parent_tf: parent_tf.validate()?,
            utc_offset_seconds,
            current_parent_start: None,
            current_children: Vec::new(),
        })
    }

    pub fn reset(&mut self) {
        self.current_parent_start = None;
        self.current_children.clear();
    }

    /// Feeds one lower-timeframe child bar. Returns the *previous* parent bucket's complete,
    /// ordered child sequence once a child bar belonging to a new parent bucket arrives — the
    /// child-bar analogue of [`crate::timeframe::BarResampler::on_bar`]'s `completed_bar`.
    pub fn on_child_bar(&mut self, bar: &Bar) -> Option<IntrabarGroup> {
        let parent_start = self
            .parent_tf
            .bucket_start(bar.timestamp, self.utc_offset_seconds);

        let completed = match self.current_parent_start {
            Some(start) if start == parent_start => None,
            Some(start) => Some(IntrabarGroup {
                parent_timestamp: start,
                children: std::mem::take(&mut self.current_children),
            }),
            None => None,
        };

        if completed.is_some() || self.current_parent_start.is_none() {
            self.current_parent_start = Some(parent_start);
        }
        self.current_children.push(bar.clone());

        completed
    }

    /// The still-forming parent bucket's child bars so far. Grows/repaints as more child bars
    /// arrive, mirroring [`crate::timeframe::ResamplerOutput::current_unconfirmed`]'s lookahead
    /// caveat: only [`IntrabarGrouper::on_child_bar`]'s returned [`IntrabarGroup`] is confirmed.
    pub fn current_children(&self) -> &[Bar] {
        &self.current_children
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn child_bar(ts: i64) -> Bar {
        Bar::new(ts, 100.0, 101.0, 99.0, 100.0, 10.0)
    }

    #[test]
    fn test_groups_children_uniquely_under_parent_bucket() {
        let mut grouper = IntrabarGrouper::new(Timeframe::Minute(5)).unwrap();

        // Five 1-minute children belong to the same 5-minute parent bucket [0, 300).
        for ts in [0, 60, 120, 180, 240] {
            let completed = grouper.on_child_bar(&child_bar(ts));
            assert!(completed.is_none());
        }
        assert_eq!(grouper.current_children().len(), 5);

        // A child at t=300 starts the next parent bucket, completing the first.
        let completed = grouper.on_child_bar(&child_bar(300)).unwrap();
        assert_eq!(completed.parent_timestamp, 0);
        assert_eq!(completed.children.len(), 5);
        assert_eq!(
            completed
                .children
                .iter()
                .map(|b| b.timestamp)
                .collect::<Vec<_>>(),
            vec![0, 60, 120, 180, 240]
        );

        // The new bucket now holds exactly the t=300 child.
        assert_eq!(grouper.current_children().len(), 1);
    }

    #[test]
    fn test_reset_clears_in_progress_group() {
        let mut grouper = IntrabarGrouper::new(Timeframe::Minute(5)).unwrap();
        grouper.on_child_bar(&child_bar(0));
        assert_eq!(grouper.current_children().len(), 1);

        grouper.reset();
        assert!(grouper.current_children().is_empty());

        // After reset, the next child starts a fresh bucket rather than completing the old one.
        let completed = grouper.on_child_bar(&child_bar(600));
        assert!(completed.is_none());
        assert_eq!(grouper.current_children().len(), 1);
    }

    #[test]
    fn test_consistent_with_bar_resampler_bucketing() {
        use crate::timeframe::BarResampler;

        let mut resampler = BarResampler::new(Timeframe::Minute(5)).unwrap();
        let mut grouper = IntrabarGrouper::new(Timeframe::Minute(5)).unwrap();

        let mut resampled_completed = None;
        let mut grouped_completed = None;
        for ts in [0, 60, 120, 180, 240, 300] {
            let bar = child_bar(ts);
            if let Some(c) = resampler.on_bar(&bar).completed_bar {
                resampled_completed = Some(c);
            }
            if let Some(g) = grouper.on_child_bar(&bar) {
                grouped_completed = Some(g);
            }
        }

        assert_eq!(
            resampled_completed.unwrap().timestamp,
            grouped_completed.unwrap().parent_timestamp
        );
    }
}
