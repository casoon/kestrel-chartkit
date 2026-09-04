use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Volume Profile Engine.
/// Computes Volume-by-Price distribution over lookback window, identifying POC (Point of Control), VAH (Value Area High), and VAL (Value Area Low).
pub struct VolumeProfileEngine {
    lookback: usize,
    num_bins: usize,
    bars: Vec<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumeProfileEngine {
    pub fn new(lookback: usize, num_bins: usize) -> Self {
        Self {
            lookback,
            num_bins,
            bars: Vec::new(),
            alerts: Vec::new(),
        }
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
            let b_start = (((b.low - min_p) / step).floor() as usize).min(self.num_bins - 1);
            let b_end = (((b.high - min_p) / step).floor() as usize).min(self.num_bins - 1);
            let bin_count = (b_end - b_start + 1) as f64;
            let vol_per_bin = bar_vol / bin_count;

            #[allow(clippy::needless_range_loop)]
            for bin_idx in b_start..=b_end {
                bins[bin_idx] += vol_per_bin;
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

        let curr_bin_idx = (((close - min_p) / step).floor() as usize).min(self.num_bins - 1);
        let curr_bin_vol = bins[curr_bin_idx];
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

pub fn build_volume_profile(params: &HashMap<String, f64>) -> VolumeProfileEngine {
    let lookback = params.get("lookback").copied().unwrap_or(70.0) as usize;
    let num_bins = params.get("num_bins").copied().unwrap_or(30.0) as usize;
    VolumeProfileEngine::new(lookback, num_bins)
}
