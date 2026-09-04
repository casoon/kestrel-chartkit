//! Persistent price/volume profile: bins keyed by a fixed price grid (not the rolling window's
//! current min/max), so a specific price level's bin has a real lifecycle — it is born on first
//! touch, grows/shrinks incrementally as bars enter and leave the trailing window, and is removed
//! once its contributing bars have all rolled out — plus a dedicated per-bin absorption profile,
//! rather than the bar-level absorption [`super::volume_flow_hires::HiResVolumeFlowEngine`]
//! computes. Complements [`super::volume_profile_extended::ExtendedVolumeProfileEngine`], which
//! recomputes its bins from scratch every call over the window's current price range and has no
//! notion of a bin persisting across updates.

use std::collections::{HashMap, VecDeque};

use crate::artifact::{ProfileArtifact, ProfileBin, ZoneArtifact};
use crate::model::Bar;
use crate::stats::rolling_median;

use super::volume_flow_hires::estimate_aggressor_from_ohlc;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// MAD-to-stddev consistency constant (see [`crate::clustering`]), reused here for the
/// per-bin absorption threshold.
const MAD_CONSISTENCY_CONSTANT: f64 = 1.482_602_218_505_602;

#[derive(Debug, Clone, Copy, PartialEq)]
struct BinLifecycle {
    volume: f64,
    buy_volume: f64,
    sell_volume: f64,
    touches: u32,
    first_touched_ts: i64,
    last_touched_ts: i64,
}

/// One bar's contribution to each bin it spanned, kept so evicting the bar from the trailing
/// window can precisely reverse its effect on those bins.
struct RecordContribution {
    per_bin: Vec<(i64, f64, f64, f64)>,
}

/// A snapshot view of one persistent bin's current lifecycle state.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AbsorptionBin {
    pub price_low: f64,
    pub price_high: f64,
    pub volume: f64,
    pub touches: u32,
    pub volume_per_touch: f64,
    pub is_absorption: bool,
}

pub struct PersistentVolumeProfileEngine {
    lookback: usize,
    bin_width: f64,
    absorption_k: f64,
    bins: HashMap<i64, BinLifecycle>,
    window: VecDeque<RecordContribution>,
    alerts: Vec<IndicatorAlert>,
}

impl PersistentVolumeProfileEngine {
    /// `bin_width` is a fixed price-grid resolution (not derived from the rolling window's
    /// min/max), the mechanism that gives bins a stable identity across updates.
    pub fn new(lookback: usize, bin_width: f64) -> Self {
        Self {
            lookback: lookback.max(1),
            bin_width: bin_width.max(1e-9),
            absorption_k: 2.5,
            bins: HashMap::new(),
            window: VecDeque::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_absorption_k(mut self, k: f64) -> Self {
        self.absorption_k = k;
        self
    }

    fn bin_key(&self, price: f64) -> i64 {
        (price / self.bin_width).floor() as i64
    }

    fn bin_price_range(&self, key: i64) -> (f64, f64) {
        (
            key as f64 * self.bin_width,
            (key as f64 + 1.0) * self.bin_width,
        )
    }

    /// Currently live bins (born and not yet expired), price-ascending.
    fn live_bins(&self) -> Vec<(i64, BinLifecycle)> {
        let mut entries: Vec<(i64, BinLifecycle)> =
            self.bins.iter().map(|(&k, &v)| (k, v)).collect();
        entries.sort_by_key(|(k, _)| *k);
        entries
    }

    /// Per-bin absorption view: a bin is flagged when its volume-per-touch is a robust outlier
    /// (median + `absorption_k` scaled-MAD) across the currently live bins — a small number of
    /// bars depositing disproportionate volume at one price level without it rolling away.
    pub fn absorption_profile(&self) -> Vec<AbsorptionBin> {
        let live = self.live_bins();
        if live.is_empty() {
            return Vec::new();
        }

        let ratios: Vec<f64> = live
            .iter()
            .map(|(_, b)| b.volume / b.touches.max(1) as f64)
            .collect();
        let median = rolling_median(&ratios);
        let abs_dev: Vec<f64> = ratios.iter().map(|r| (r - median).abs()).collect();
        let mad = rolling_median(&abs_dev) * MAD_CONSISTENCY_CONSTANT;
        let threshold = median + self.absorption_k * mad;

        live.into_iter()
            .zip(ratios)
            .map(|((key, bin), ratio)| {
                let (price_low, price_high) = self.bin_price_range(key);
                AbsorptionBin {
                    price_low,
                    price_high,
                    volume: bin.volume,
                    touches: bin.touches,
                    volume_per_touch: ratio,
                    // Note: when a majority of bins share the same ratio, MAD is 0 and the
                    // threshold collapses to the median itself — any value strictly above it is
                    // still a real outlier (a tight majority plus one clear outsider), so this
                    // does not require `mad > 0.0` as an extra gate.
                    is_absorption: ratio > threshold,
                }
            })
            .collect()
    }

    fn record_bar(&mut self, bar: &Bar) {
        // A non-finite or inverted range cannot be mapped onto the fixed price grid (e.g.
        // `high = inf` yields `bin_key` = `i64::MAX`, overflowing the span arithmetic below).
        // Skip such a bar rather than panic; it simply contributes nothing this call.
        if !bar.low.is_finite()
            || !bar.high.is_finite()
            || !bar.volume.is_finite()
            || bar.high < bar.low
        {
            return;
        }

        let bar_vol = if bar.volume > 0.0 {
            bar.volume
        } else {
            bar.high - bar.low
        };
        let aggressor = estimate_aggressor_from_ohlc(bar);
        let (buy_frac, sell_frac) = if bar.volume > 0.0 {
            (
                aggressor.buy_volume / bar.volume,
                aggressor.sell_volume / bar.volume,
            )
        } else {
            (0.5, 0.5)
        };

        let start_key = self.bin_key(bar.low);
        let end_key = self.bin_key(bar.high).max(start_key);
        let bin_count = (end_key - start_key + 1) as f64;
        let vol_per_bin = bar_vol / bin_count;
        let buy_per_bin = vol_per_bin * buy_frac;
        let sell_per_bin = vol_per_bin * sell_frac;

        let mut contribution = Vec::with_capacity((end_key - start_key + 1) as usize);
        for key in start_key..=end_key {
            let entry = self.bins.entry(key).or_insert(BinLifecycle {
                volume: 0.0,
                buy_volume: 0.0,
                sell_volume: 0.0,
                touches: 0,
                first_touched_ts: bar.timestamp,
                last_touched_ts: bar.timestamp,
            });
            entry.volume += vol_per_bin;
            entry.buy_volume += buy_per_bin;
            entry.sell_volume += sell_per_bin;
            entry.touches += 1;
            entry.last_touched_ts = bar.timestamp;
            contribution.push((key, vol_per_bin, buy_per_bin, sell_per_bin));
        }

        self.window.push_back(RecordContribution {
            per_bin: contribution,
        });
        if self.window.len() > self.lookback {
            let evicted = self
                .window
                .pop_front()
                .expect("just checked len > lookback");
            for (key, vol, buy, sell) in evicted.per_bin {
                let expired = if let Some(entry) = self.bins.get_mut(&key) {
                    entry.volume -= vol;
                    entry.buy_volume -= buy;
                    entry.sell_volume -= sell;
                    entry.touches = entry.touches.saturating_sub(1);
                    entry.touches == 0 || entry.volume <= 1e-9
                } else {
                    false
                };
                if expired {
                    self.bins.remove(&key);
                    let (price_low, price_high) = self.bin_price_range(key);
                    self.alerts.push(IndicatorAlert::new(
                        "bin_expired",
                        format!("Price bin [{:.4}, {:.4}) expired", price_low, price_high),
                        0.3,
                    ));
                }
            }
        }
    }

    fn build_output(&self) -> Option<IndicatorOutput> {
        let live = self.live_bins();
        if live.is_empty() {
            return None;
        }

        let bins: Vec<ProfileBin> = live
            .iter()
            .map(|(key, b)| {
                let (price_low, price_high) = self.bin_price_range(*key);
                ProfileBin {
                    price_low,
                    price_high,
                    value: b.volume,
                }
            })
            .collect();

        let (poc_pos, poc_volume) =
            live.iter()
                .enumerate()
                .fold((0usize, f64::MIN), |(bi, bv), (i, (_, b))| {
                    if b.volume > bv {
                        (i, b.volume)
                    } else {
                        (bi, bv)
                    }
                });
        let _ = poc_volume;
        let poc_key = live[poc_pos].0;
        let (poc_low, poc_high) = self.bin_price_range(poc_key);
        let poc_price = (poc_low + poc_high) / 2.0;

        let profile_artifact = ProfileArtifact {
            kind: "persistent_volume_profile".to_string(),
            bins,
            poc: poc_price,
            value_area_high: poc_high,
            value_area_low: poc_low,
        };

        let absorption = self.absorption_profile();
        let absorption_bins: Vec<ProfileBin> = absorption
            .iter()
            .map(|a| ProfileBin {
                price_low: a.price_low,
                price_high: a.price_high,
                value: a.volume_per_touch,
            })
            .collect();
        let absorption_artifact = ProfileArtifact {
            kind: "absorption_profile".to_string(),
            bins: absorption_bins,
            poc: poc_price,
            value_area_high: poc_high,
            value_area_low: poc_low,
        };

        let mut output = IndicatorOutput::new(poc_price)
            .with_artifact(profile_artifact)
            .with_artifact(absorption_artifact);

        for a in absorption.iter().filter(|a| a.is_absorption) {
            output = output.with_artifact(ZoneArtifact {
                kind: "absorption_zone".to_string(),
                price_top: a.price_high,
                price_bottom: a.price_low,
                strength: (a.volume_per_touch).min(1.0),
                touches: a.touches,
            });
        }

        Some(output)
    }
}

impl Indicator for PersistentVolumeProfileEngine {
    fn name(&self) -> &str {
        "persistent_volume_profile"
    }

    fn warmup_period(&self) -> usize {
        self.lookback
    }

    fn reset(&mut self) {
        self.bins.clear();
        self.window.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.record_bar(bar);
        if self.window.len() < self.lookback {
            return None;
        }
        self.build_output()
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A narrow-range bar (high-low = 0.1) so it always lands inside a single 1.0-wide bin
    /// regardless of where `price` falls relative to a bin boundary.
    fn bar_at(price: f64, volume: f64) -> Bar {
        Bar::new(0, price, price + 0.05, price - 0.05, price, volume)
    }

    #[test]
    fn test_bin_persists_and_grows_across_updates() {
        let mut engine = PersistentVolumeProfileEngine::new(3, 1.0);
        engine.on_bar(&bar_at(100.2, 100.0));
        let key = engine.bin_key(100.2);
        assert_eq!(engine.bins.get(&key).unwrap().volume, 100.0);

        engine.on_bar(&bar_at(100.3, 50.0));
        // Same bin (same 1.0-wide grid cell around 100), volume accumulated rather than replaced.
        assert_eq!(engine.bin_key(100.3), key);
        assert_eq!(engine.bins.get(&key).unwrap().volume, 150.0);
        assert_eq!(engine.bins.get(&key).unwrap().touches, 2);
    }

    #[test]
    fn test_bin_dies_once_its_contributing_bars_roll_out() {
        let mut engine = PersistentVolumeProfileEngine::new(2, 1.0);
        let key = engine.bin_key(50.0);
        engine.on_bar(&bar_at(50.0, 100.0));
        assert!(engine.bins.contains_key(&key));

        // Two more bars at a distant price roll the original bar out of the lookback=2 window.
        engine.on_bar(&bar_at(200.0, 10.0));
        engine.on_bar(&bar_at(200.0, 10.0));

        assert!(
            !engine.bins.contains_key(&key),
            "bin must expire once its only contributing bar leaves the window"
        );
        assert!(engine.alerts().iter().any(|a| a.kind == "bin_expired"));
    }

    // Baseline/spike prices deliberately offset from whole numbers (bin_width = 1.0) so their
    // narrow +/-0.05 range never straddles a bin boundary.
    const BASELINE_PRICES: [f64; 9] = [90.3, 92.3, 94.3, 96.3, 98.3, 102.3, 104.3, 106.3, 108.3];
    const SPIKE_PRICE: f64 = 100.3;

    #[test]
    fn test_absorption_flags_concentrated_single_bar_volume() {
        let mut engine = PersistentVolumeProfileEngine::new(10, 1.0).with_absorption_k(1.5);
        // Baseline: modest, evenly distributed volume across several distinct price levels.
        for price in BASELINE_PRICES {
            engine.on_bar(&bar_at(price, 50.0));
        }
        // One outlier bar dumps a huge amount of volume into a single new bin in one touch.
        engine.on_bar(&bar_at(SPIKE_PRICE, 5000.0));

        let absorption = engine.absorption_profile();
        let flagged = absorption.iter().find(|a| a.is_absorption);
        assert!(
            flagged.is_some(),
            "a single-touch volume spike must be flagged as absorption"
        );
        assert!(
            flagged.unwrap().price_low <= SPIKE_PRICE && flagged.unwrap().price_high > SPIKE_PRICE
        );
    }

    #[test]
    fn test_evenly_touched_bin_is_not_absorption() {
        // Large enough lookback that none of the 19 bars fed below roll out of the window.
        let mut engine = PersistentVolumeProfileEngine::new(19, 1.0).with_absorption_k(1.5);
        for price in BASELINE_PRICES {
            engine.on_bar(&bar_at(price, 50.0));
        }
        // Same per-touch volume (50) as the baseline bins, just reached over 10 touches at one
        // level instead of a single one -> normal participation, not absorption.
        for _ in 0..10 {
            engine.on_bar(&bar_at(SPIKE_PRICE, 50.0));
        }

        let absorption = engine.absorption_profile();
        let bin_spike = absorption
            .iter()
            .find(|a| a.price_low <= SPIKE_PRICE && a.price_high > SPIKE_PRICE)
            .unwrap();
        assert!(!bin_spike.is_absorption);
    }

    #[test]
    fn test_none_until_lookback_filled() {
        let mut engine = PersistentVolumeProfileEngine::new(4, 1.0);
        for _ in 0..3 {
            assert!(engine.on_bar(&bar_at(100.0, 10.0)).is_none());
        }
        assert!(engine.on_bar(&bar_at(100.0, 10.0)).is_some());
    }
}
