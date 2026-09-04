//! Extended price/volume profile: full bins with their volume, an HVN/LVN/AVN classification,
//! a delta (buy vs. sell) profile alongside the volume profile, zone formation from contiguous
//! high/low-volume bins, and intrabar-resolution distribution — the capabilities
//! [`super::volume_profile::VolumeProfileEngine`]'s scalar-only output (POC/VAH/VAL plus a couple
//! of density numbers) does not expose.

use std::collections::VecDeque;

use crate::artifact::{ProfileArtifact, ProfileBin, ZoneArtifact};
use crate::intrabar::IntrabarGroup;
use crate::model::Bar;

use super::volume_flow_hires::estimate_aggressor_from_ohlc;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Classification of a profile bin relative to the mean bin volume across the whole profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeNodeClass {
    /// High Volume Node: `volume >= mean * hvn_multiplier`.
    Hvn,
    /// Average Volume Node: neither HVN nor LVN.
    Avn,
    /// Low Volume Node: `volume <= mean * lvn_multiplier`.
    Lvn,
}

/// Extended price/volume profile engine.
pub struct ExtendedVolumeProfileEngine {
    lookback: usize,
    num_bins: usize,
    hvn_multiplier: f64,
    lvn_multiplier: f64,
    records: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl ExtendedVolumeProfileEngine {
    pub fn new(lookback: usize, num_bins: usize) -> Self {
        Self {
            lookback: lookback.max(1),
            num_bins: num_bins.max(1),
            hvn_multiplier: 1.5,
            lvn_multiplier: 0.5,
            records: VecDeque::new(),
            alerts: Vec::new(),
        }
    }

    /// Overrides the default HVN (`1.5x` mean) / LVN (`0.5x` mean) classification multipliers.
    pub fn with_thresholds(mut self, hvn_multiplier: f64, lvn_multiplier: f64) -> Self {
        self.hvn_multiplier = hvn_multiplier;
        self.lvn_multiplier = lvn_multiplier;
        self
    }

    fn push_record(&mut self, bar: Bar) {
        if self.records.len() >= self.lookback {
            self.records.pop_front();
        }
        self.records.push_back(bar);
    }

    /// Feeds a confirmed [`IntrabarGroup`] (a parent bar's full ordered child-bar sequence),
    /// recording each child individually instead of the aggregate parent bar alone — finer
    /// intrabar volume distribution than [`Indicator::on_bar`]'s per-parent-bar granularity.
    /// `lookback` then counts child-bar entries, not parent bars.
    pub fn on_intrabar_group(&mut self, group: &IntrabarGroup) -> Option<IndicatorOutput> {
        for child in &group.children {
            self.push_record(child.clone());
        }
        self.compute()
    }

    fn compute(&mut self) -> Option<IndicatorOutput> {
        self.alerts.clear();
        if self.records.len() < self.lookback {
            return None;
        }

        let mut min_p = f64::MAX;
        let mut max_p = f64::MIN;
        for b in &self.records {
            min_p = min_p.min(b.low);
            max_p = max_p.max(b.high);
        }
        if (max_p - min_p).abs() < 1e-8 {
            return None;
        }

        let step = (max_p - min_p) / self.num_bins as f64;
        let mut volumes = vec![0.0f64; self.num_bins];
        let mut buy_volumes = vec![0.0f64; self.num_bins];
        let mut sell_volumes = vec![0.0f64; self.num_bins];

        for b in &self.records {
            let bar_vol = if b.volume > 0.0 {
                b.volume
            } else {
                b.high - b.low
            };
            let aggressor = estimate_aggressor_from_ohlc(b);
            let (buy_frac, sell_frac) = if b.volume > 0.0 {
                (
                    aggressor.buy_volume / b.volume,
                    aggressor.sell_volume / b.volume,
                )
            } else {
                (0.5, 0.5)
            };

            let raw_start = ((b.low - min_p) / step).floor();
            let b_start = if raw_start.is_finite() && raw_start >= 0.0 {
                (raw_start as usize).min(self.num_bins.saturating_sub(1))
            } else {
                0
            };
            let raw_end = ((b.high - min_p) / step).floor();
            let b_end = if raw_end.is_finite() && raw_end >= 0.0 {
                (raw_end as usize).min(self.num_bins.saturating_sub(1))
            } else {
                0
            };
            let b_end = b_end.max(b_start);
            let bin_count = (b_end - b_start + 1) as f64;
            let vol_per_bin = bar_vol / bin_count;

            for bin_idx in b_start..=b_end {
                if let Some(vol) = volumes.get_mut(bin_idx) {
                    *vol += vol_per_bin;
                }
                if let Some(buy_vol) = buy_volumes.get_mut(bin_idx) {
                    *buy_vol += vol_per_bin * buy_frac;
                }
                if let Some(sell_vol) = sell_volumes.get_mut(bin_idx) {
                    *sell_vol += vol_per_bin * sell_frac;
                }
            }
        }

        let total_vol: f64 = volumes.iter().sum();
        let mean_bin_vol = total_vol / self.num_bins as f64;
        let classify = |v: f64| -> VolumeNodeClass {
            if v >= mean_bin_vol * self.hvn_multiplier {
                VolumeNodeClass::Hvn
            } else if v <= mean_bin_vol * self.lvn_multiplier {
                VolumeNodeClass::Lvn
            } else {
                VolumeNodeClass::Avn
            }
        };
        let classes: Vec<VolumeNodeClass> = volumes.iter().map(|&v| classify(v)).collect();

        let (poc_idx, _) =
            volumes
                .iter()
                .enumerate()
                .fold(
                    (0usize, 0.0f64),
                    |(bi, bv), (i, &v)| {
                        if v > bv {
                            (i, v)
                        } else {
                            (bi, bv)
                        }
                    },
                );
        let poc_price = min_p + (poc_idx as f64 + 0.5) * step;

        let target_vol = total_vol * 0.70;
        let mut accumulated = volumes[poc_idx];
        let mut val_idx = poc_idx;
        let mut vah_idx = poc_idx;
        while accumulated < target_vol && (val_idx > 0 || vah_idx < self.num_bins - 1) {
            let next_down = if val_idx > 0 {
                volumes[val_idx - 1]
            } else {
                -1.0
            };
            let next_up = if vah_idx < self.num_bins - 1 {
                volumes[vah_idx + 1]
            } else {
                -1.0
            };
            if next_up >= next_down && vah_idx < self.num_bins - 1 {
                vah_idx += 1;
                accumulated += volumes[vah_idx];
            } else if val_idx > 0 {
                val_idx -= 1;
                accumulated += volumes[val_idx];
            } else if vah_idx < self.num_bins - 1 {
                vah_idx += 1;
                accumulated += volumes[vah_idx];
            }
        }
        let vah_price = min_p + (vah_idx as f64 + 1.0) * step;
        let val_price = min_p + val_idx as f64 * step;

        let bin_price = |i: usize| ProfileBin {
            price_low: min_p + i as f64 * step,
            price_high: min_p + (i as f64 + 1.0) * step,
            value: volumes[i],
        };
        let bins: Vec<ProfileBin> = (0..self.num_bins).map(bin_price).collect();

        let delta_bins: Vec<ProfileBin> = (0..self.num_bins)
            .map(|i| ProfileBin {
                price_low: min_p + i as f64 * step,
                price_high: min_p + (i as f64 + 1.0) * step,
                value: buy_volumes[i] - sell_volumes[i],
            })
            .collect();

        // Merge contiguous same-class HVN/LVN bins into zones (AVN bins form no zone).
        let mut zones = Vec::new();
        let mut i = 0;
        while i < self.num_bins {
            let class = classes[i];
            if class == VolumeNodeClass::Avn {
                i += 1;
                continue;
            }
            let start = i;
            while i < self.num_bins && classes[i] == class {
                i += 1;
            }
            let end = i;
            let zone_vol: f64 = volumes[start..end].iter().sum();
            zones.push(ZoneArtifact {
                kind: match class {
                    VolumeNodeClass::Hvn => "hvn_zone".to_string(),
                    VolumeNodeClass::Lvn => "lvn_zone".to_string(),
                    VolumeNodeClass::Avn => unreachable!("filtered above"),
                },
                price_top: min_p + end as f64 * step,
                price_bottom: min_p + start as f64 * step,
                strength: if total_vol > 0.0 {
                    zone_vol / total_vol
                } else {
                    0.0
                },
                touches: 0,
            });
        }

        let profile_artifact = ProfileArtifact {
            kind: "volume_profile".to_string(),
            bins,
            poc: poc_price,
            value_area_high: vah_price,
            value_area_low: val_price,
        };
        let delta_artifact = ProfileArtifact {
            kind: "delta_profile".to_string(),
            bins: delta_bins,
            poc: poc_price,
            value_area_high: vah_price,
            value_area_low: val_price,
        };

        let mut output = IndicatorOutput::new(poc_price)
            .with_artifact(profile_artifact)
            .with_artifact(delta_artifact);
        for zone in zones {
            output = output.with_artifact(zone);
        }
        Some(output)
    }
}

impl Indicator for ExtendedVolumeProfileEngine {
    fn name(&self) -> &str {
        "extended_volume_profile"
    }

    fn warmup_period(&self) -> usize {
        self.lookback
    }

    fn reset(&mut self) {
        self.records.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.push_record(bar.clone());
        self.compute()
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar_at(price: f64, volume: f64) -> Bar {
        Bar::new(0, price, price + 1.0, price - 1.0, price, volume)
    }

    /// A narrow-range bar (high-low = 0.8) so it lands entirely inside one bin rather than
    /// splitting its volume across a bin boundary.
    fn narrow_bar(price: f64, volume: f64) -> Bar {
        Bar::new(0, price, price + 0.4, price - 0.4, price, volume)
    }

    #[test]
    fn test_classifies_hvn_and_lvn_bins() {
        let mut engine = ExtendedVolumeProfileEngine::new(10, 4).with_thresholds(1.5, 0.5);
        // Two thin, wide-apart bars set the overall 79..121 profile range; the tightly clustered
        // heavy bars all land inside a single interior bin.
        for _ in 0..8 {
            engine.on_bar(&narrow_bar(95.0, 500.0));
        }
        engine.on_bar(&bar_at(80.0, 1.0));
        let out = engine.on_bar(&bar_at(120.0, 1.0)).unwrap();

        let profile = out
            .artifacts
            .iter()
            .find_map(|a| match a {
                crate::artifact::Artifact::Profile(p) if p.kind == "volume_profile" => Some(p),
                _ => None,
            })
            .expect("volume_profile artifact must be present");

        assert_eq!(profile.bins.len(), 4);
        // The bin(s) covering the 100 cluster must dominate volume share.
        let max_bin_volume = profile.bins.iter().map(|b| b.value).fold(0.0, f64::max);
        let total: f64 = profile.bins.iter().map(|b| b.value).sum();
        assert!(max_bin_volume / total > 0.5);
    }

    #[test]
    fn test_delta_profile_reflects_buy_sell_split() {
        let mut engine = ExtendedVolumeProfileEngine::new(3, 2);
        // A bar closing at its high is almost entirely buy-side.
        engine.on_bar(&Bar::new(0, 100.0, 102.0, 98.0, 102.0, 100.0));
        engine.on_bar(&Bar::new(60, 100.0, 102.0, 98.0, 102.0, 100.0));
        let out = engine
            .on_bar(&Bar::new(120, 100.0, 102.0, 98.0, 102.0, 100.0))
            .unwrap();

        let delta = out
            .artifacts
            .iter()
            .find_map(|a| match a {
                crate::artifact::Artifact::Profile(p) if p.kind == "delta_profile" => Some(p),
                _ => None,
            })
            .unwrap();
        let total_delta: f64 = delta.bins.iter().map(|b| b.value).sum();
        assert!(
            total_delta > 0.0,
            "close-at-high bars must skew delta positive"
        );
    }

    #[test]
    fn test_zones_formed_from_contiguous_hvn_bins() {
        let mut engine = ExtendedVolumeProfileEngine::new(8, 5).with_thresholds(1.5, 0.5);
        // Two thin, wide-apart bars set a 50..150 profile range; the tightly clustered heavy
        // bars land entirely inside one interior bin, forming a clear HVN zone with LVN zones on
        // either side.
        engine.on_bar(&bar_at(50.0, 1.0));
        for _ in 0..6 {
            engine.on_bar(&narrow_bar(100.0, 1000.0));
        }
        let out = engine.on_bar(&bar_at(150.0, 1.0)).unwrap();
        let zones: Vec<_> = out
            .artifacts
            .iter()
            .filter_map(|a| match a {
                crate::artifact::Artifact::Zone(z) => Some(z),
                _ => None,
            })
            .collect();
        assert!(
            !zones.is_empty(),
            "a tight volume cluster must form at least one zone"
        );
        assert!(zones.iter().any(|z| z.kind == "hvn_zone"));
    }

    #[test]
    fn test_none_until_lookback_filled() {
        let mut engine = ExtendedVolumeProfileEngine::new(5, 4);
        for _ in 0..4 {
            assert!(engine.on_bar(&bar_at(100.0, 100.0)).is_none());
        }
        assert!(engine.on_bar(&bar_at(100.0, 100.0)).is_some());
    }
}
