use std::collections::HashMap;

use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Eine gebildete Fair-Value-Gap-Zone.
///
/// Der Kern hält bisher nur die **aktuelle** Lücke als `fvg_type`/`gap_size`;
/// wer die Bereiche als Zonen zeichnen oder registrieren will (Kestrel
/// plan/43 §6 Stufe 5), braucht sie als Bänder mit Grenzen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FvgZone {
    pub is_bullish: bool,
    pub top: f64,
    pub bottom: f64,
    /// Bar-Zähler ab Engine-Start, auf dem die Lücke entstand.
    pub created_bar: usize,
}

/// Höchstzahl gehaltener Lückenzonen — ältere fallen zuerst weg.
const MAX_FVG_ZONES: usize = 20;

/// Smart Money Fair Value Gap (FVG) & Liquidity Sweep Engine.
/// Detects institutional price imbalances and liquidity pool sweeps.
pub struct LiquidityFvgEngine {
    lookback: usize,
    window: Vec<Bar>,
    alerts: Vec<IndicatorAlert>,
    bar_count: usize,
    zones: Vec<FvgZone>,
}

impl LiquidityFvgEngine {
    pub fn new(lookback: usize) -> Self {
        Self {
            lookback,
            window: Vec::with_capacity(lookback + 5),
            alerts: Vec::new(),
            bar_count: 0,
            zones: Vec::new(),
        }
    }

    /// Die noch offenen Fair-Value-Gap-Zonen, älteste zuerst.
    pub fn zones(&self) -> &[FvgZone] {
        &self.zones
    }
}

impl Indicator for LiquidityFvgEngine {
    fn name(&self) -> &str {
        "liquidity_fvg"
    }

    fn warmup_period(&self) -> usize {
        self.lookback.max(3)
    }

    fn reset(&mut self) {
        self.window.clear();
        self.alerts.clear();
        self.bar_count = 0;
        self.zones.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.window.push(bar.clone());
        if self.window.len() > self.lookback + 5 {
            self.window.remove(0);
        }
        self.bar_count += 1;

        self.alerts.clear();

        if self.window.len() < 3 {
            return Some(IndicatorOutput::new(0.0));
        }

        let n = self.window.len();
        let curr = &self.window[n - 1];
        let prev2 = &self.window[n - 3];

        let mut fvg_type = 0.0f64;
        let mut gap_size = 0.0f64;

        // 1. Bullish Fair Value Gap (Low[t] > High[t-2])
        if curr.low > prev2.high {
            fvg_type = 1.0;
            gap_size = curr.low - prev2.high;
            self.alerts.push(IndicatorAlert::new(
                "bullish_fvg",
                format!(
                    "Bullish Fair Value Gap (FVG Zone ${:.2} - ${:.2})",
                    prev2.high, curr.low
                ),
                0.85,
            ));
        }
        // 2. Bearish Fair Value Gap (High[t] < Low[t-2])
        else if curr.high < prev2.low {
            fvg_type = -1.0;
            gap_size = prev2.low - curr.high;
            self.alerts.push(IndicatorAlert::new(
                "bearish_fvg",
                format!(
                    "Bearish Fair Value Gap (FVG Zone ${:.2} - ${:.2})",
                    curr.high, prev2.low
                ),
                0.85,
            ));
        }

        // Zwischenstand als Zone halten: die Lücke entsteht zwischen dem Hoch
        // der Bar t-2 und dem Tief der Bar t (bullisch) bzw. umgekehrt.
        if fvg_type > 0.0 {
            self.zones.push(FvgZone {
                is_bullish: true,
                top: curr.low,
                bottom: prev2.high,
                created_bar: self.bar_count,
            });
        } else if fvg_type < 0.0 {
            self.zones.push(FvgZone {
                is_bullish: false,
                top: prev2.low,
                bottom: curr.high,
                created_bar: self.bar_count,
            });
        }
        // Gefüllte Lücken fallen weg: eine Long-Lücke ist geschlossen, sobald
        // ein späterer Bar unter ihre Unterkante läuft, eine Short-Lücke über
        // ihre Oberkante.
        self.zones.retain(|zone| {
            if zone.is_bullish {
                curr.low > zone.bottom
            } else {
                curr.high < zone.top
            }
        });
        if self.zones.len() > MAX_FVG_ZONES {
            let ueberzaehlig = self.zones.len() - MAX_FVG_ZONES;
            self.zones.drain(0..ueberzaehlig);
        }

        // 3. Liquidity Sweep Detection over lookback window
        if n > self.lookback {
            let prev_bars = &self.window[n - 1 - self.lookback..n - 1];
            let recent_highest = prev_bars
                .iter()
                .map(|b| b.high)
                .fold(f64::NEG_INFINITY, f64::max);
            let recent_lowest = prev_bars
                .iter()
                .map(|b| b.low)
                .fold(f64::INFINITY, f64::min);

            // Bullish Liquidity Sweep (Low pierced recent lowest, but Close > recent lowest)
            if curr.low < recent_lowest && curr.close > recent_lowest {
                self.alerts.push(IndicatorAlert::new(
                    "bullish_liquidity_sweep",
                    format!(
                        "Bullish Liquidity Sweep (Pierced ${:.2} Support, Reclaimed Close)",
                        recent_lowest
                    ),
                    0.95,
                ));
            }
            // Bearish Liquidity Sweep (High pierced recent highest, but Close < recent highest)
            else if curr.high > recent_highest && curr.close < recent_highest {
                self.alerts.push(IndicatorAlert::new(
                    "bearish_liquidity_sweep",
                    format!(
                        "Bearish Liquidity Sweep (Pierced ${:.2} Resistance, Reclaimed Close)",
                        recent_highest
                    ),
                    0.95,
                ));
            }
        }

        let mut extra = HashMap::new();
        extra.insert("fvg_type".to_string(), fvg_type);
        extra.insert("gap_size".to_string(), gap_size);

        Some(IndicatorOutput::with_extra(fvg_type, extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub fn build_liquidity_fvg(params: &HashMap<String, f64>) -> LiquidityFvgEngine {
    let lookback = params.get("lookback").copied().unwrap_or(20.0) as usize;
    LiquidityFvgEngine::new(lookback)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(high: f64, low: f64) -> Bar {
        Bar::new(0, (high + low) / 2.0, high, low, (high + low) / 2.0, 0.0)
    }

    /// Eine bullische Lücke entsteht zwischen dem Hoch der Bar t-2 und dem Tief
    /// der Bar t; eine spätere Bar, die unter die Unterkante läuft, füllt sie.
    #[test]
    fn bildet_und_fuellt_fvg_zonen() {
        let mut engine = LiquidityFvgEngine::new(20);
        let bars = [bar(101.0, 99.0), bar(102.0, 100.0), bar(110.0, 105.0)];
        for (i, b) in bars.iter().enumerate() {
            engine.on_bar(&Bar::new(i as i64, b.open, b.high, b.low, b.close, 0.0));
        }
        assert_eq!(engine.zones().len(), 1, "eine bullische Lücke");
        assert_eq!(engine.zones()[0].bottom, 101.0);
        assert_eq!(engine.zones()[0].top, 105.0);
        assert!(engine.zones()[0].is_bullish);

        // Füllende Bar: Tief unter der Unterkante.
        engine.on_bar(&Bar::new(3, 104.0, 104.0, 98.0, 99.0, 0.0));
        assert!(
            engine.zones().is_empty(),
            "gefüllte Lücke bleibt nicht stehen"
        );
    }

    #[test]
    fn reset_leert_die_zonen() {
        let mut engine = LiquidityFvgEngine::new(20);
        engine.on_bar(&Bar::new(0, 100.0, 101.0, 99.0, 100.0, 0.0));
        engine.on_bar(&Bar::new(1, 101.0, 102.0, 100.0, 101.0, 0.0));
        engine.on_bar(&Bar::new(2, 108.0, 110.0, 105.0, 108.0, 0.0));
        assert!(!engine.zones().is_empty());
        engine.reset();
        assert!(engine.zones().is_empty());
    }
}
