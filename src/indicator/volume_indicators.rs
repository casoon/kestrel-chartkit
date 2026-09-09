use std::collections::{HashMap, VecDeque};

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Volume & Average Volume Indicator.
pub struct VolumeEngine {
    ma_period: usize,
    volumes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumeEngine {
    pub fn new(ma_period: usize) -> Self {
        Self {
            ma_period,
            volumes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for VolumeEngine {
    fn name(&self) -> &str {
        "volume"
    }

    fn warmup_period(&self) -> usize {
        self.ma_period
    }

    fn reset(&mut self) {
        self.volumes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let vol = bar.volume;
        self.volumes.push_back(vol);
        if self.volumes.len() > self.ma_period {
            self.volumes.pop_front();
        }

        self.alerts.clear();
        if self.volumes.len() < self.ma_period {
            return Some(IndicatorOutput::new(vol));
        }

        let avg_vol: f64 = self.volumes.iter().sum::<f64>() / self.ma_period as f64;
        let mut extra = HashMap::new();
        extra.insert("avg_volume".to_string(), avg_vol);
        extra.insert(
            "volume_ratio".to_string(),
            if avg_vol > 0.0 { vol / avg_vol } else { 1.0 },
        );

        if avg_vol > 0.0 && vol > 2.0 * avg_vol {
            self.alerts.push(IndicatorAlert::new(
                "high_volume_spike",
                format!("High Volume Spike: {:.0} (>2.0x avg {:.0})", vol, avg_vol),
                0.80,
            ));
        }

        Some(IndicatorOutput::with_extra(vol, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Relative Volume (RVOL) Indicator.
#[derive(Debug, Clone)]
pub struct RvolEngine {
    period: usize,
    volumes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl RvolEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            volumes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for RvolEngine {
    fn name(&self) -> &str {
        "rvol"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.volumes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let vol = bar.volume;
        self.volumes.push_back(vol);
        if self.volumes.len() > self.period {
            self.volumes.pop_front();
        }

        self.alerts.clear();
        if self.volumes.len() < self.period {
            return None;
        }

        let avg_vol: f64 = self.volumes.iter().sum::<f64>() / self.period as f64;
        let rvol = if avg_vol > 0.0 { vol / avg_vol } else { 1.0 };

        if rvol >= 2.5 {
            self.alerts.push(IndicatorAlert::new(
                "extreme_rvol",
                format!("Extreme Relative Volume: {:.2}x", rvol),
                0.90,
            ));
        }

        Some(IndicatorOutput::new(rvol))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// On-Balance Volume (OBV) Indicator.
pub struct ObvEngine {
    prev_close: Option<f64>,
    cum_obv: f64,
    alerts: Vec<IndicatorAlert>,
}

impl ObvEngine {
    pub fn new() -> Self {
        Self {
            prev_close: None,
            cum_obv: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Default for ObvEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for ObvEngine {
    fn name(&self) -> &str {
        "obv"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.cum_obv = 0.0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if let Some(prev) = self.prev_close {
            if bar.close > prev {
                self.cum_obv += bar.volume;
            } else if bar.close < prev {
                self.cum_obv -= bar.volume;
            }
        }
        self.prev_close = Some(bar.close);

        Some(IndicatorOutput::new(self.cum_obv))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Chaikin Money Flow (CMF) Indicator.
pub struct CmfEngine {
    period: usize,
    mf_volumes: VecDeque<f64>,
    volumes: VecDeque<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl CmfEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            mf_volumes: VecDeque::new(),
            volumes: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for CmfEngine {
    fn name(&self) -> &str {
        "cmf"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.mf_volumes.clear();
        self.volumes.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let high_low = bar.high - bar.low;
        let mfm = if high_low > 1e-8 {
            ((bar.close - bar.low) - (bar.high - bar.close)) / high_low
        } else {
            0.0
        };
        let mfv = mfm * bar.volume;

        self.mf_volumes.push_back(mfv);
        self.volumes.push_back(bar.volume);

        if self.mf_volumes.len() > self.period {
            self.mf_volumes.pop_front();
            self.volumes.pop_front();
        }

        self.alerts.clear();
        if self.mf_volumes.len() < self.period {
            return None;
        }

        let sum_mfv: f64 = self.mf_volumes.iter().sum();
        let sum_vol: f64 = self.volumes.iter().sum();

        let cmf = if sum_vol > 0.0 {
            sum_mfv / sum_vol
        } else {
            0.0
        };

        if cmf > 0.20 {
            self.alerts.push(IndicatorAlert::new(
                "cmf_bullish",
                format!("Strong Buying Pressure (CMF: {:.2})", cmf),
                0.80,
            ));
        } else if cmf < -0.20 {
            self.alerts.push(IndicatorAlert::new(
                "cmf_bearish",
                format!("Strong Selling Pressure (CMF: {:.2})", cmf),
                0.80,
            ));
        }

        Some(IndicatorOutput::new(cmf.clamp(-1.0, 1.0)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Accumulation / Distribution Line (A/D) Indicator.
#[derive(Debug, Clone)]
pub struct AccDistEngine {
    cum_ad: f64,
    alerts: Vec<IndicatorAlert>,
}

impl AccDistEngine {
    pub fn new() -> Self {
        Self {
            cum_ad: 0.0,
            alerts: Vec::new(),
        }
    }
}

impl Default for AccDistEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for AccDistEngine {
    fn name(&self) -> &str {
        "acc_dist"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.cum_ad = 0.0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let high_low = bar.high - bar.low;
        let mfm = if high_low > 1e-8 {
            ((bar.close - bar.low) - (bar.high - bar.close)) / high_low
        } else {
            0.0
        };
        let mfv = mfm * bar.volume;
        self.cum_ad += mfv;

        Some(IndicatorOutput::new(self.cum_ad))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}
