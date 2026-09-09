use crate::model::{Bar, MarketRegime};

pub fn classify_regime(bars: &[Bar], adx_val: f64, atr_val: f64) -> MarketRegime {
    if bars.len() < 21 {
        return MarketRegime::Transition;
    }

    let len = bars.len();
    let current_close = bars[len - 1].close;
    let sma_20: f64 = bars[len - 20..].iter().map(|b| b.close).sum::<f64>() / 20.0;
    let prev_sma_20: f64 = bars[len - 21..len - 1].iter().map(|b| b.close).sum::<f64>() / 20.0;

    let slope = if prev_sma_20 != 0.0 {
        (sma_20 - prev_sma_20) / prev_sma_20
    } else {
        0.0
    };
    let is_trending = adx_val > 20.0;

    if is_trending {
        if slope > 0.001 && current_close > sma_20 {
            MarketRegime::BullishExpansion
        } else if slope < -0.001 && current_close < sma_20 {
            MarketRegime::BearishExpansion
        } else {
            MarketRegime::Transition
        }
    } else {
        if atr_val > 0.02 {
            MarketRegime::Transition
        } else {
            MarketRegime::Consolidation
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_regime_warmup_lengths() {
        let make_bars = |n: usize| -> Vec<Bar> {
            (0..n)
                .map(|i| {
                    Bar::new(
                        i as i64,
                        100.0,
                        101.0,
                        99.0,
                        100.0 + (i as f64 * 0.5),
                        1000.0,
                    )
                })
                .collect()
        };

        // 0, 19, 20 bars should return Transition during warmup without panic
        assert_eq!(
            classify_regime(&make_bars(0), 25.0, 0.01),
            MarketRegime::Transition
        );
        assert_eq!(
            classify_regime(&make_bars(19), 25.0, 0.01),
            MarketRegime::Transition
        );
        assert_eq!(
            classify_regime(&make_bars(20), 25.0, 0.01),
            MarketRegime::Transition
        );

        // 21 bars has enough history for prev_sma_20
        let regime_21 = classify_regime(&make_bars(21), 25.0, 0.01);
        assert_ne!(regime_21, MarketRegime::Transition);
    }
}
