use crate::indicator::{IndicatorAlert, IndicatorOutput};
use crate::signal::SubScore;

pub fn score_indicator(
    name: &str,
    output: &IndicatorOutput,
    alerts: &[IndicatorAlert],
) -> SubScore {
    let val = output.value;
    let mut score = 0.0f64;
    let mut reasons = Vec::new();

    for alert in alerts {
        match alert.kind.as_str() {
            "bull_extreme" | "bull_cross" | "bull_mid_cross" | "bull_di_cross"
            | "bull_zero_cross" | "wt_bull_cross" => {
                let s = 0.5 + 0.5 * alert.strength.clamp(0.0, 1.0);
                score += s;
                reasons.push(alert.note.clone());
            }
            "bear_extreme" | "bear_cross" | "bear_mid_cross" | "bear_di_cross"
            | "bear_zero_cross" | "wt_bear_cross" => {
                let s = 0.5 + 0.5 * alert.strength.clamp(0.0, 1.0);
                score -= s;
                reasons.push(alert.note.clone());
            }
            "bull_divergence"
            | "bullish_kangaroo_tail"
            | "bullish_engulfing"
            | "bullish_tweezer"
            | "bullish_marubozu"
            | "bullish_fvg"
            | "bullish_liquidity_sweep"
            | "high_leg_efficiency"
            | "structure_bullish_bias"
            | "bullish_order_block"
            | "ob_retest_bullish"
            | "bullish_bos"
            | "bullish_choch"
            | "price_below_val" => {
                let s = 0.6 + 0.4 * alert.strength.clamp(0.0, 1.0);
                score += s;
                reasons.push(alert.note.clone());
            }
            "bear_divergence"
            | "bearish_kangaroo_tail"
            | "bearish_engulfing"
            | "bearish_tweezer"
            | "bearish_marubozu"
            | "bearish_fvg"
            | "bearish_liquidity_sweep"
            | "structure_bearish_bias"
            | "bearish_order_block"
            | "ob_retest_bearish"
            | "bearish_bos"
            | "bearish_choch"
            | "price_above_vah" => {
                let s = 0.6 + 0.4 * alert.strength.clamp(0.0, 1.0);
                score -= s;
                reasons.push(alert.note.clone());
            }
            "panic_bottom" => {
                score += 0.9;
                reasons.push(alert.note.clone());
            }
            "expansion" => {
                reasons.push(alert.note.clone());
            }
            "contraction" | "low_leg_efficiency" | "volatility" => {
                // Volatility-state alerts (squeeze/expansion) describe market *condition*, not
                // direction — context for the explanation, deliberately no score contribution.
                reasons.push(alert.note.clone());
            }
            _ => {
                if !alert.note.is_empty() {
                    reasons.push(alert.note.clone());
                }
            }
        }
    }

    // Secondary level checks if no explicit alerts fired
    if alerts.is_empty() {
        match name.to_lowercase().as_str() {
            "rsi" | "stoch_rsi" | "mfi" | "williams_r" | "connors_rsi" => {
                if val > 70.0 {
                    score -= 0.4;
                    reasons.push(format!(
                        "{}: Im überkauften Bereich ({:.1})",
                        name.to_uppercase(),
                        val
                    ));
                } else if val < 30.0 {
                    score += 0.4;
                    reasons.push(format!(
                        "{}: Im überverkauften Bereich ({:.1})",
                        name.to_uppercase(),
                        val
                    ));
                } else if val > 55.0 {
                    score += 0.2;
                } else if val < 45.0 {
                    score -= 0.2;
                }
            }
            "macd" => {
                let hist = output.extra.get("hist").copied().unwrap_or(0.0);
                if hist > 0.0 {
                    score += 0.3;
                } else if hist < 0.0 {
                    score -= 0.3;
                }
            }
            "bollinger" => {
                let pct_b = output.extra.get("percent_b").copied().unwrap_or(0.5);
                if pct_b > 1.0 {
                    score -= 0.5;
                    reasons.push("BOLLINGER: Preis über oberem Band".to_string());
                } else if pct_b < 0.0 {
                    score += 0.5;
                    reasons.push("BOLLINGER: Preis unter unterem Band".to_string());
                }
            }
            "candle_story" => {
                if val > 30.0 {
                    score += 0.3;
                } else if val < -30.0 {
                    score -= 0.3;
                }
            }
            "efficiency" => {
                if val >= 0.50 {
                    score += 0.3;
                } else if val <= 0.25 {
                    score -= 0.2;
                }
            }
            "choppiness" => {
                // Direction-agnostic trend-strength proxy, same convention as `efficiency`
                // above: low Choppiness Index = clean trend (conviction bump), high =
                // range-bound chop (conviction penalty) — not itself a bullish/bearish call.
                if val <= 38.2 {
                    score += 0.3;
                    reasons.push(format!("CHOPPINESS: Klarer Trend ({val:.1})"));
                } else if val >= 61.8 {
                    score -= 0.2;
                    reasons.push(format!(
                        "CHOPPINESS: Ausgeprägte Seitwärtsbewegung ({val:.1})"
                    ));
                }
            }
            "vortex" => {
                // `val` is VI+ (see indicator/vortex.rs); VI- lives in `extra`. Standard
                // Vortex reading: VI+ above VI- is bullish trend dominance, and vice versa.
                let vi_minus = output.extra.get("vi_minus").copied().unwrap_or(val);
                let diff = val - vi_minus;
                if diff > 0.05 {
                    score += (diff * 2.0).clamp(0.0, 1.0);
                    reasons.push(format!("VORTEX: +VI über -VI ({val:.2} vs {vi_minus:.2})"));
                } else if diff < -0.05 {
                    score += (diff * 2.0).clamp(-1.0, 0.0);
                    reasons.push(format!("VORTEX: -VI über +VI ({vi_minus:.2} vs {val:.2})"));
                }
            }
            "trend_quality" => {
                // Already a signed, -100..100-scaled composite (direction × efficiency ×
                // ADX-strength × RVOL-participation, see indicator/trend_quality.rs) — rescale
                // into the -1..1 SubScore range directly instead of re-deriving a direction.
                score += (val / 100.0).clamp(-1.0, 1.0);
                if val.abs() >= 20.0 {
                    let label = if val > 0.0 { "Bullische" } else { "Bärische" };
                    reasons.push(format!("TREND_QUALITY: {label} Trendqualität ({val:.1})"));
                }
            }
            _ => {}
        }
    }

    let score_clamped = score.clamp(-1.0, 1.0);
    let reason_str = if !reasons.is_empty() {
        Some(reasons.join("; "))
    } else {
        None
    };

    SubScore {
        indicator: name.to_string(),
        score: score_clamped,
        raw_value: val,
        reason: reason_str,
    }
}
