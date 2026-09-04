use std::collections::HashMap;
use std::fmt;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Invalid [`VolumeProfileEngine`] configuration: `lookback`/`num_bins` of zero would produce an
/// empty bin vector (and panic on the first non-flat window in `on_bar`) or an unbounded lookback
/// window, so `try_new` rejects them explicitly (finding 06) rather than silently normalizing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VolumeProfileConfigError {
    /// `lookback` was `0`; it must be at least `1`.
    ZeroLookback,
    /// `num_bins` was `0`; it must be at least `1`.
    ZeroNumBins,
}

impl fmt::Display for VolumeProfileConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLookback => write!(f, "lookback must be at least 1"),
            Self::ZeroNumBins => write!(f, "num_bins must be at least 1"),
        }
    }
}

impl std::error::Error for VolumeProfileConfigError {}

/// Volume Profile Engine.
/// Computes Volume-by-Price distribution over lookback window, identifying POC (Point of Control), VAH (Value Area High), and VAL (Value Area Low).
pub struct VolumeProfileEngine {
    lookback: usize,
    num_bins: usize,
    bars: Vec<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumeProfileEngine {
    /// Constructs a profile engine, clamping `lookback` and `num_bins` to a minimum of `1` each
    /// (matching [`ExtendedVolumeProfileEngine`](super::volume_profile_extended::ExtendedVolumeProfileEngine)'s
    /// and [`PersistentVolumeProfileEngine`](super::volume_profile_persistent::PersistentVolumeProfileEngine)'s
    /// existing contract). `0` would otherwise leave `on_bar` with an empty bin vector (panicking
    /// on the first non-flat window) or a warmup that can never complete. Prefer
    /// [`VolumeProfileEngine::try_new`] for configuration-driven construction, where silently
    /// substituting `1` would compute a different profile than requested.
    pub fn new(lookback: usize, num_bins: usize) -> Self {
        Self {
            lookback: lookback.max(1),
            num_bins: num_bins.max(1),
            bars: Vec::new(),
            alerts: Vec::new(),
        }
    }

    /// Like [`VolumeProfileEngine::new`], but rejects a zero `lookback`/`num_bins` with
    /// [`VolumeProfileConfigError`] instead of silently clamping it to `1`.
    pub fn try_new(lookback: usize, num_bins: usize) -> Result<Self, VolumeProfileConfigError> {
        if lookback == 0 {
            return Err(VolumeProfileConfigError::ZeroLookback);
        }
        if num_bins == 0 {
            return Err(VolumeProfileConfigError::ZeroNumBins);
        }
        Ok(Self::new(lookback, num_bins))
    }
}

impl Indicator for VolumeProfileEngine {
    fn name(&self) -> &str {
        "volume_profile"
    }

    fn warmup_period(&self) -> usize {
        self.lookback
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push(bar.clone());
        if self.bars.len() > self.lookback {
            self.bars.remove(0);
        }

        self.alerts.clear();

        if self.bars.len() < self.lookback {
            return None;
        }

        // Find min and max price across window
        let mut min_p = f64::MAX;
        let mut max_p = f64::MIN;
        for b in &self.bars {
            if b.low < min_p {
                min_p = b.low;
            }
            if b.high > max_p {
                max_p = b.high;
            }
        }

        if (max_p - min_p).abs() < 1e-8 {
            return Some(IndicatorOutput::new(bar.close));
        }

        let step = (max_p - min_p) / (self.num_bins as f64);
        let mut bins = vec![0.0f64; self.num_bins];
        let mut total_vol = 0.0f64;

        for b in &self.bars {
            let bar_vol = if b.volume > 0.0 {
                b.volume
            } else {
                b.high - b.low
            };
            total_vol += bar_vol;

            // Distribute volume proportionally across bins overlapping bar.low..bar.high
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

            for bin in &mut bins[b_start..=b_end] {
                *bin += vol_per_bin;
            }
        }

        // Find POC (bin with max volume)
        let mut max_bin_vol = 0.0f64;
        let mut poc_idx = 0;
        for (i, &v) in bins.iter().enumerate() {
            if v > max_bin_vol {
                max_bin_vol = v;
                poc_idx = i;
            }
        }

        let poc_price = min_p + (poc_idx as f64 + 0.5) * step;

        // Calculate 70% Value Area (VAH & VAL)
        let target_vol = total_vol * 0.70;
        let mut accumulated_vol = bins[poc_idx];
        let mut val_idx = poc_idx;
        let mut vah_idx = poc_idx;

        while accumulated_vol < target_vol && (val_idx > 0 || vah_idx < self.num_bins - 1) {
            let next_down_vol = if val_idx > 0 { bins[val_idx - 1] } else { -1.0 };
            let next_up_vol = if vah_idx < self.num_bins - 1 {
                bins[vah_idx + 1]
            } else {
                -1.0
            };

            if next_up_vol >= next_down_vol && vah_idx < self.num_bins - 1 {
                vah_idx += 1;
                accumulated_vol += bins[vah_idx];
            } else if val_idx > 0 {
                val_idx -= 1;
                accumulated_vol += bins[val_idx];
            } else if vah_idx < self.num_bins - 1 {
                vah_idx += 1;
                accumulated_vol += bins[vah_idx];
            }
        }

        let vah_price = min_p + (vah_idx as f64 + 1.0) * step;
        let val_price = min_p + (val_idx as f64) * step;

        // Evaluate current close relative to Volume Profile
        let close = bar.close;
        let dist_to_poc = (close - poc_price).abs();
        let rel_dist_poc = dist_to_poc / close;

        if rel_dist_poc <= 0.003 {
            self.alerts.push(IndicatorAlert::new(
                "price_at_poc",
                format!("Price at Point of Control (POC: ${:.2})", poc_price),
                0.85,
            ));
        } else if close > vah_price {
            self.alerts.push(IndicatorAlert::new(
                "price_above_vah",
                format!(
                    "Price Above Value Area High (${:.2} > VAH ${:.2})",
                    close, vah_price
                ),
                0.80,
            ));
        } else if close < val_price {
            self.alerts.push(IndicatorAlert::new(
                "price_below_val",
                format!(
                    "Price Below Value Area Low (${:.2} < VAL ${:.2})",
                    close, val_price
                ),
                0.80,
            ));
        }

        let raw_curr = ((close - min_p) / step).floor();
        let curr_bin_idx = if raw_curr.is_finite() && raw_curr >= 0.0 {
            (raw_curr as usize).min(self.num_bins.saturating_sub(1))
        } else {
            0
        };
        let curr_bin_vol = bins.get(curr_bin_idx).copied().unwrap_or(0.0);
        let curr_density = if total_vol > 0.0 {
            curr_bin_vol / total_vol
        } else {
            0.0
        };
        let vpoc_density = if total_vol > 0.0 {
            max_bin_vol / total_vol
        } else {
            0.0
        };

        let mut extra = HashMap::new();
        extra.insert("vpoc".to_string(), poc_price);
        extra.insert("vah".to_string(), vah_price);
        extra.insert("val".to_string(), val_price);
        extra.insert("total_volume".to_string(), total_vol);
        extra.insert("vpoc_density".to_string(), vpoc_density);
        extra.insert("current_density".to_string(), curr_density);
        extra.insert("lvn_width".to_string(), step * 2.0); // Approximate LVN width in price units

        Some(IndicatorOutput::with_extra(poc_price, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Builds a [`VolumeProfileEngine`] from loosely-typed params, defaulting `lookback`/`num_bins` to
/// `70`/`30` for missing, negative, non-finite, or fractional-truncating-to-zero values. Routed
/// through [`VolumeProfileEngine::new`], so (per finding 06) an explicit `0` is clamped to `1`
/// rather than reaching `on_bar` with an empty bin vector.
pub fn build_volume_profile(params: &HashMap<String, f64>) -> VolumeProfileEngine {
    let lookback = params
        .get("lookback")
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as usize)
        .unwrap_or(70);
    let num_bins = params
        .get("num_bins")
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .map(|v| v as usize)
        .unwrap_or(30);
    VolumeProfileEngine::new(lookback, num_bins)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::registry::build_checked;

    /// A non-flat window: the min/max-price early return (`(max_p - min_p).abs() < 1e-8`) would
    /// otherwise mask a `num_bins = 0` bug by never reaching the bin-slicing code at all.
    fn non_flat_bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64;
                Bar::new(i as i64, base, base + 2.0, base - 2.0, base + 0.5, 100.0)
            })
            .collect()
    }

    #[test]
    fn test_new_clamps_zero_num_bins_and_does_not_panic() {
        let mut engine = VolumeProfileEngine::new(5, 0);
        assert_eq!(engine.num_bins, 1);
        for bar in non_flat_bars(10) {
            engine.on_bar(&bar); // must not panic
        }
    }

    #[test]
    fn test_new_clamps_zero_lookback() {
        let engine = VolumeProfileEngine::new(0, 10);
        assert_eq!(engine.lookback, 1);
        assert_eq!(engine.warmup_period(), 1);
    }

    #[test]
    fn test_try_new_rejects_zero_lookback() {
        let err = match VolumeProfileEngine::try_new(0, 10) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for zero lookback"),
        };
        assert_eq!(err, VolumeProfileConfigError::ZeroLookback);
    }

    #[test]
    fn test_try_new_rejects_zero_num_bins() {
        let err = match VolumeProfileEngine::try_new(10, 0) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for zero num_bins"),
        };
        assert_eq!(err, VolumeProfileConfigError::ZeroNumBins);
    }

    #[test]
    fn test_try_new_accepts_valid_config() {
        let engine = VolumeProfileEngine::try_new(10, 5).unwrap();
        assert_eq!(engine.lookback, 10);
        assert_eq!(engine.num_bins, 5);
    }

    #[test]
    fn test_build_volume_profile_handles_invalid_values_without_panicking() {
        for (lookback, num_bins) in [
            (0.0, 0.0),
            (-5.0, -5.0),
            (f64::NAN, f64::NAN),
            (f64::INFINITY, f64::INFINITY),
            (5.9, 5.9),
        ] {
            let mut engine = build_volume_profile(&HashMap::from([
                ("lookback".to_string(), lookback),
                ("num_bins".to_string(), num_bins),
            ]));
            for bar in non_flat_bars(10) {
                engine.on_bar(&bar); // must not panic for any of these inputs
            }
        }
    }

    #[test]
    fn test_build_volume_profile_zero_is_clamped_not_defaulted() {
        // Zero is a valid-looking (finite, non-negative) input, distinct from "missing"; it must
        // clamp to 1 via the constructor, not silently fall back to the unrelated 70/30 default.
        let engine = build_volume_profile(&HashMap::from([
            ("lookback".to_string(), 0.0),
            ("num_bins".to_string(), 0.0),
        ]));
        assert_eq!(engine.lookback, 1);
        assert_eq!(engine.num_bins, 1);
    }

    /// `build_checked` (registry, strict), `build_volume_profile` (loose builder), and
    /// `VolumeProfileEngine::new` (direct) must all agree for the same valid parameters.
    #[test]
    fn test_registry_builder_and_constructor_are_equivalent_for_valid_params() {
        let bars = non_flat_bars(20);
        let params = HashMap::from([
            ("lookback".to_string(), 10.0),
            ("num_bins".to_string(), 5.0),
        ]);

        let mut via_registry = build_checked("volume_profile", &params).unwrap();
        let mut via_builder = build_volume_profile(&params);
        let mut via_constructor = VolumeProfileEngine::new(10, 5);

        for bar in &bars {
            let a = via_registry.on_bar(bar).map(|o| o.value);
            let b = via_builder.on_bar(bar).map(|o| o.value);
            let c = via_constructor.on_bar(bar).map(|o| o.value);
            assert_eq!(a, b);
            assert_eq!(a, c);
        }
    }
}
