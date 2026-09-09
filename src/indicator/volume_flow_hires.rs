//! High-resolution volume flow: direct aggressor/delta inputs, intrabar delta from grouped child
//! bars, absorption detection, and explicitly quality-tagged fallbacks — the capabilities
//! [`super::volume_flow::CvdEngine`]'s single OHLC-close-location heuristic per bar cannot offer.

use std::collections::HashMap;

use crate::clustering::RollingRobustThreshold;
use crate::intrabar::IntrabarGroup;
use crate::model::{Bar, BarQuality};

use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Directly known aggressor-side (taker buy vs. taker sell) volume for a bar, from real trade/
/// tick data rather than an OHLC-inferred estimate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AggressorVolume {
    pub buy_volume: f64,
    pub sell_volume: f64,
}

impl AggressorVolume {
    pub fn delta(&self) -> f64 {
        self.buy_volume - self.sell_volume
    }
}

/// Estimates a bar's aggressor split from its OHLC close-location within its range — the same
/// heuristic [`super::volume_flow::CvdEngine`] uses, kept here as the explicit, tagged fallback
/// path when no direct aggressor/tick data is available. Shared with
/// [`super::volume_profile_extended`] for its delta-profile bins.
pub(crate) fn estimate_aggressor_from_ohlc(bar: &Bar) -> AggressorVolume {
    let range = (bar.high - bar.low).max(1e-8);
    let buy_pct = ((bar.close - bar.low) / range).clamp(0.0, 1.0);
    AggressorVolume {
        buy_volume: bar.volume * buy_pct,
        sell_volume: bar.volume * (1.0 - buy_pct),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HiResVolumeFlowOutput {
    pub delta: f64,
    pub cumulative_delta: f64,
    pub quality: BarQuality,
    /// `true` when this bar's volume is an outlier-high spike (per the rolling robust volume
    /// threshold) while its price range stayed unremarkable — high participation without
    /// proportional price displacement, the classic order-flow absorption signature.
    pub absorption: bool,
}

/// Streaming high-resolution volume flow engine.
pub struct HiResVolumeFlowEngine {
    cumulative_delta: f64,
    volume_threshold: RollingRobustThreshold,
    range_threshold: RollingRobustThreshold,
    alerts: Vec<IndicatorAlert>,
}

impl HiResVolumeFlowEngine {
    pub fn new(window_len: usize) -> Self {
        Self {
            cumulative_delta: 0.0,
            volume_threshold: RollingRobustThreshold::new(window_len, 2.5),
            range_threshold: RollingRobustThreshold::new(window_len, 2.5),
            alerts: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.cumulative_delta = 0.0;
        self.volume_threshold.reset();
        self.range_threshold.reset();
        self.alerts.clear();
    }

    fn absorb_step(&mut self, bar: &Bar, delta: f64, quality: BarQuality) -> HiResVolumeFlowOutput {
        self.alerts.clear();
        self.cumulative_delta += delta;

        let volume_band = self.volume_threshold.update(bar.volume);
        let range_band = self.range_threshold.update(bar.high - bar.low);

        let absorption = match (volume_band, range_band) {
            (Some(vb), Some(rb)) => bar.volume > vb.upper && (bar.high - bar.low) <= rb.median,
            _ => false,
        };

        if absorption {
            self.alerts.push(IndicatorAlert::new(
                "volume_absorption",
                "Outlier volume with unremarkable price range: possible absorption",
                0.7,
            ));
        }

        HiResVolumeFlowOutput {
            delta,
            cumulative_delta: self.cumulative_delta,
            quality,
            absorption,
        }
    }

    /// High-resolution path: feeds a bar with directly known aggressor volume (e.g. aggregated
    /// from trade-tape data).
    pub fn on_bar_with_aggressor(
        &mut self,
        bar: &Bar,
        aggressor: AggressorVolume,
    ) -> HiResVolumeFlowOutput {
        self.absorb_step(bar, aggressor.delta(), BarQuality::observed())
    }

    /// High-resolution path: feeds a confirmed [`IntrabarGroup`] (a parent bar's full ordered
    /// child-bar sequence), summing each child's own OHLC-inferred delta instead of applying the
    /// heuristic once to the aggregate parent bar — preserves the intrabar price path the
    /// aggregate alone loses.
    pub fn on_intrabar_group(&mut self, group: &IntrabarGroup) -> HiResVolumeFlowOutput {
        let mut delta = 0.0;
        let mut open = f64::NAN;
        let mut high = f64::NEG_INFINITY;
        let mut low = f64::INFINITY;
        let mut close = f64::NAN;
        let mut volume = 0.0;

        for (i, child) in group.children.iter().enumerate() {
            let aggressor = estimate_aggressor_from_ohlc(child);
            delta += aggressor.delta();
            if i == 0 {
                open = child.open;
            }
            high = high.max(child.high);
            low = low.min(child.low);
            close = child.close;
            volume += child.volume;
        }

        let parent_bar = Bar::new(group.parent_timestamp, open, high, low, close, volume);
        let mut quality = BarQuality::observed();
        quality.is_forward_filled = false;
        self.absorb_step(&parent_bar, delta, quality)
    }

    /// Fallback path: OHLC-inferred estimate when no direct aggressor or intrabar data is
    /// available, explicitly tagged as such via [`BarQuality::volume_available`] staying accurate
    /// but the estimate not being a genuine tick-level measurement.
    pub fn on_bar_estimated(&mut self, bar: &Bar) -> HiResVolumeFlowOutput {
        let aggressor = estimate_aggressor_from_ohlc(bar);
        let mut quality = BarQuality::observed();
        quality.is_synthetic = true; // the buy/sell split itself is inferred, not observed
        self.absorb_step(bar, aggressor.delta(), quality)
    }
}

impl Indicator for HiResVolumeFlowEngine {
    fn name(&self) -> &str {
        "hires_volume_flow"
    }

    fn reset(&mut self) {
        HiResVolumeFlowEngine::reset(self)
    }

    /// Delegates to the OHLC-estimated fallback path; use
    /// [`HiResVolumeFlowEngine::on_bar_with_aggressor`] or
    /// [`HiResVolumeFlowEngine::on_intrabar_group`] directly for the high-resolution paths, which
    /// this trait's single-`Bar` signature cannot express.
    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let out = self.on_bar_estimated(bar);
        let mut extra = HashMap::new();
        extra.insert("delta".to_string(), out.delta);
        extra.insert(
            "is_estimated".to_string(),
            if out.quality.is_synthetic { 1.0 } else { 0.0 },
        );
        Some(
            IndicatorOutput::with_extra(out.cumulative_delta, extra).with_state(
                if out.absorption {
                    "absorption"
                } else {
                    "normal"
                },
            ),
        )
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intrabar::IntrabarGrouper;
    use crate::timeframe::Timeframe;

    #[test]
    fn test_direct_aggressor_delta_matches_input_exactly() {
        let mut engine = HiResVolumeFlowEngine::new(5);
        let bar = Bar::new(0, 100.0, 101.0, 99.0, 100.5, 1000.0);
        let out = engine.on_bar_with_aggressor(
            &bar,
            AggressorVolume {
                buy_volume: 700.0,
                sell_volume: 300.0,
            },
        );
        assert_eq!(out.delta, 400.0);
        assert_eq!(out.quality, BarQuality::observed());
    }

    #[test]
    fn test_estimated_fallback_is_tagged_synthetic() {
        let mut engine = HiResVolumeFlowEngine::new(5);
        let bar = Bar::new(0, 100.0, 101.0, 99.0, 100.5, 1000.0);
        let out = engine.on_bar_estimated(&bar);
        assert!(
            out.quality.is_synthetic,
            "estimated delta must be tagged as such"
        );
    }

    #[test]
    fn test_intrabar_group_sums_child_deltas() {
        let mut grouper = IntrabarGrouper::new(Timeframe::Minute(5)).unwrap();
        let children = [
            Bar::new(0, 100.0, 101.0, 100.0, 101.0, 100.0),
            Bar::new(60, 101.0, 102.0, 100.5, 101.5, 100.0),
        ];
        for child in &children {
            grouper.on_child_bar(child);
        }
        // Starts a new parent bucket, completing the first with the two children above.
        let completed = grouper.on_child_bar(&Bar::new(300, 101.5, 102.0, 101.0, 101.8, 50.0));

        let mut engine = HiResVolumeFlowEngine::new(5);
        let out = engine.on_intrabar_group(&completed.unwrap());

        let expected: f64 = children
            .iter()
            .map(|c| estimate_aggressor_from_ohlc(c).delta())
            .sum();
        assert!((out.delta - expected).abs() < 1e-9);
    }

    #[test]
    fn test_absorption_flags_outlier_volume_with_tight_range() {
        let mut engine = HiResVolumeFlowEngine::new(6);
        // Establish a normal volume/range baseline.
        for _ in 0..5 {
            engine.on_bar_estimated(&Bar::new(0, 100.0, 101.0, 99.0, 100.5, 100.0));
        }
        // One bar with a volume spike but a tight (unremarkable) range.
        let out = engine.on_bar_estimated(&Bar::new(60, 100.0, 100.3, 99.8, 100.1, 5000.0));
        assert!(
            out.absorption,
            "large volume with tight range must flag absorption"
        );
        assert!(!engine.alerts().is_empty());
    }

    #[test]
    fn test_normal_bar_does_not_flag_absorption() {
        let mut engine = HiResVolumeFlowEngine::new(6);
        let mut last = HiResVolumeFlowOutput {
            delta: 0.0,
            cumulative_delta: 0.0,
            quality: BarQuality::observed(),
            absorption: false,
        };
        for _ in 0..6 {
            last = engine.on_bar_estimated(&Bar::new(0, 100.0, 101.0, 99.0, 100.5, 100.0));
        }
        assert!(!last.absorption);
    }
}
