use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Negative Volume Index (NVI) Engine.
/// Updates index value only on days when volume decreases.
#[derive(Debug, Clone)]
pub struct NviEngine {
    nvi: f64,
    prev_bar: Option<Bar>,
}

impl NviEngine {
    pub fn new() -> Self {
        Self {
            nvi: 1000.0,
            prev_bar: None,
        }
    }
}

impl Default for NviEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for NviEngine {
    fn name(&self) -> &str {
        "nvi"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.nvi = 1000.0;
        self.prev_bar = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev = match &self.prev_bar {
            Some(p) => p.clone(),
            None => {
                self.prev_bar = Some(bar.clone());
                return Some(IndicatorOutput::new(self.nvi));
            }
        };
        self.prev_bar = Some(bar.clone());

        if bar.volume < prev.volume {
            let roc = if prev.close > 0.0 {
                (bar.close - prev.close) / prev.close
            } else {
                0.0
            };
            self.nvi += self.nvi * roc;
        }

        Some(IndicatorOutput::new(self.nvi))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Positive Volume Index (PVI) Engine.
/// Updates index value only on days when volume increases.
#[derive(Debug, Clone)]
pub struct PviEngine {
    pvi: f64,
    prev_bar: Option<Bar>,
}

impl PviEngine {
    pub fn new() -> Self {
        Self {
            pvi: 1000.0,
            prev_bar: None,
        }
    }
}

impl Default for PviEngine {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for PviEngine {
    fn name(&self) -> &str {
        "pvi"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.pvi = 1000.0;
        self.prev_bar = None;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let prev = match &self.prev_bar {
            Some(p) => p.clone(),
            None => {
                self.prev_bar = Some(bar.clone());
                return Some(IndicatorOutput::new(self.pvi));
            }
        };
        self.prev_bar = Some(bar.clone());

        if bar.volume > prev.volume {
            let roc = if prev.close > 0.0 {
                (bar.close - prev.close) / prev.close
            } else {
                0.0
            };
            self.pvi += self.pvi * roc;
        }

        Some(IndicatorOutput::new(self.pvi))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nvi_pvi() {
        let mut nvi = NviEngine::new();
        let mut pvi = PviEngine::new();

        let b1 = Bar::new(1, 100.0, 105.0, 95.0, 100.0, 1000.0);
        let b2 = Bar::new(2, 100.0, 105.0, 95.0, 105.0, 500.0); // Volume decreased -> NVI updates

        nvi.on_bar(&b1);
        pvi.on_bar(&b1);

        let out_nvi = nvi.on_bar(&b2).unwrap();
        let out_pvi = pvi.on_bar(&b2).unwrap();

        assert_eq!(out_nvi.value, 1050.0);
        assert_eq!(out_pvi.value, 1000.0);
    }
}
