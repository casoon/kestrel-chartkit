#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use std::collections::HashMap;

use crate::model::{Bar, MarketRegime, RiskPlan, SupportResistanceZone};
use crate::signal::{
    CompositeSignal, InvalidationZone, PermissionGrade, SignalDirection, SubScore, TargetZone,
    TriggerAction,
};

pub fn aggregate_subscores(
    subscores: Vec<SubScore>,
    weights: Option<&HashMap<String, f64>>,
    regime: MarketRegime,
    sr_zones: Vec<SupportResistanceZone>,
    latest_bar: Option<&Bar>,
    atr_val: f64,
) -> CompositeSignal {
    let subscores: Vec<SubScore> = subscores
        .into_iter()
        .filter(|sub| sub.score.is_finite() && sub.raw_value.is_finite())
        .map(|mut sub| {
            sub.score = sub.score.clamp(-1.0, 1.0);
            sub
        })
        .collect();

    if subscores.is_empty() {
        return CompositeSignal {
            direction: SignalDirection::Neutral,
            trigger: TriggerAction::Hold,
            score: 0.0,
            confidence: 0.0,
            heat_score: 0.0,
            permission: PermissionGrade::Veto,
            regime,
            target_zone: None,
            invalidation_zone: None,
            setup_duration: None,
            sr_zones: Vec::new(),
            risk_plan: None,
            reasons: Vec::new(),
            explanation: "Keine Indikatorensignale verfügbar.".to_string(),
            per_indicator: Vec::new(),
        };
    }

    let mut total_weighted_score = 0.0f64;
    let mut total_weight = 0.0f64;
    let mut reasons = Vec::new();

    for sub in &subscores {
        let raw_weight = weights
            .and_then(|w| w.get(&sub.indicator))
            .copied()
            .unwrap_or(1.0);
        let weight = if raw_weight.is_finite() && (0.0..=1_000.0).contains(&raw_weight) {
            raw_weight
        } else {
            1.0
        };

        total_weighted_score += sub.score * weight;
        total_weight += weight;

        if let Some(ref r) = sub.reason {
            reasons.push(r.clone());
        }
    }

    let final_score = if total_weight > 0.0 {
        (total_weighted_score / total_weight).clamp(-1.0, 1.0)
    } else {
        0.0
    };

    let direction = if final_score >= 0.20 {
        SignalDirection::Bullish
    } else if final_score <= -0.20 {
        SignalDirection::Bearish
    } else {
        SignalDirection::Neutral
    };

    let mut agreement_count = 0usize;
    for sub in &subscores {
        match direction {
            SignalDirection::Bullish if sub.score > 0.05 => agreement_count += 1,
            SignalDirection::Bearish if sub.score < -0.05 => agreement_count += 1,
            SignalDirection::Neutral if sub.score.abs() <= 0.20 => agreement_count += 1,
            _ => {}
        }
    }

    let confidence = (agreement_count as f64 / subscores.len() as f64).clamp(0.0, 1.0);

    // Heat score 0.0 ..= 1.0 derived from score magnitude and agreement confidence
    let heat_score = (final_score.abs() * 0.6 + confidence * 0.4).clamp(0.0, 1.0);

    let is_regime_aligned = matches!(
        (regime, direction),
        (MarketRegime::BullishExpansion, SignalDirection::Bullish)
            | (MarketRegime::BearishExpansion, SignalDirection::Bearish)
            | (MarketRegime::Consolidation, SignalDirection::Neutral)
    );

    let is_regime_opposed = matches!(
        (regime, direction),
        (MarketRegime::BullishExpansion, SignalDirection::Bearish)
            | (MarketRegime::BearishExpansion, SignalDirection::Bullish)
    );

    let permission = if direction == SignalDirection::Neutral || is_regime_opposed {
        PermissionGrade::Veto
    } else if regime == MarketRegime::Transition {
        PermissionGrade::Caution
    } else if heat_score >= 0.50
        && (is_regime_aligned || (regime == MarketRegime::Consolidation && confidence >= 0.75))
    {
        PermissionGrade::ClearToTrade
    } else if heat_score >= 0.30 {
        PermissionGrade::Caution
    } else {
        PermissionGrade::Veto
    };

    let trigger = match (permission, direction) {
        (PermissionGrade::ClearToTrade, SignalDirection::Bullish) => TriggerAction::Buy,
        (PermissionGrade::ClearToTrade, SignalDirection::Bearish) => TriggerAction::Sell,
        _ => TriggerAction::Hold,
    };

    let explanation = match direction {
        SignalDirection::Bullish => format!(
            "Gesamtsignal BULLISH (Heat Score: {:.0}%, Konfidenz: {:.0}%). Status: {}. Regime: {}. {}",
            heat_score * 100.0,
            confidence * 100.0,
            permission,
            regime,
            if !reasons.is_empty() {
                reasons.join(". ")
            } else {
                "Mehrheitliche bullische Indikatoren-Konfluenz.".to_string()
            }
        ),
        SignalDirection::Bearish => format!(
            "Gesamtsignal BEARISH (Heat Score: {:.0}%, Konfidenz: {:.0}%). Status: {}. Regime: {}. {}",
            heat_score * 100.0,
            confidence * 100.0,
            permission,
            regime,
            if !reasons.is_empty() {
                reasons.join(". ")
            } else {
                "Mehrheitliche bärische Indikatoren-Konfluenz.".to_string()
            }
        ),
        SignalDirection::Neutral => format!(
            "Gesamtsignal NEUTRAL (Heat Score: {:.0}%, Konfidenz: {:.0}%). Status: {}. Regime: {}. {}",
            heat_score * 100.0,
            confidence * 100.0,
            permission,
            regime,
            if !reasons.is_empty() {
                reasons.join(". ")
            } else {
                "Gegensätzliche oder neutrale Indikatorensignale.".to_string()
            }
        ),
    };

    let latest_bar = latest_bar.filter(|bar| bar.is_valid());
    let (target_zone, invalidation_zone, setup_duration, risk_plan) = match (trigger, latest_bar) {
        (TriggerAction::Buy, Some(bar)) => {
            let atr = if atr_val.is_finite() && atr_val > 0.0 {
                atr_val
            } else {
                bar.close * 0.01
            };
            let entry = bar.close;
            let sl = entry - 1.5 * atr;
            let tp1 = entry + 1.0 * atr;
            let tp2 = entry + 2.0 * atr;
            let rrr = if (entry - sl).abs() > 0.0 {
                (tp2 - entry) / (entry - sl)
            } else {
                1.5
            };

            (
                Some(TargetZone {
                    price: entry + 2.0 * atr,
                    name: "Target 2.0x ATR".to_string(),
                    reward_atr: 2.0,
                }),
                Some(InvalidationZone {
                    price: sl,
                    reason: "Structural Low Invalidation".to_string(),
                    risk_atr: 1.5,
                }),
                Some(crate::signal::SetupDuration {
                    max_bars: 20,
                    elapsed_bars: 0,
                }),
                Some(RiskPlan {
                    entry,
                    stop_loss: sl,
                    target_1: tp1,
                    target_2: tp2,
                    risk_reward_ratio: rrr,
                }),
            )
        }
        (TriggerAction::Sell, Some(bar)) => {
            let atr = if atr_val.is_finite() && atr_val > 0.0 {
                atr_val
            } else {
                bar.close * 0.01
            };
            let entry = bar.close;
            let sl = entry + 1.5 * atr;
            let tp1 = entry - 1.0 * atr;
            let tp2 = entry - 2.0 * atr;
            let rrr = if (sl - entry).abs() > 0.0 {
                (entry - tp2) / (sl - entry)
            } else {
                1.5
            };

            (
                Some(TargetZone {
                    price: entry - 2.0 * atr,
                    name: "Target 2.0x ATR".to_string(),
                    reward_atr: 2.0,
                }),
                Some(InvalidationZone {
                    price: sl,
                    reason: "Structural High Invalidation".to_string(),
                    risk_atr: 1.5,
                }),
                Some(crate::signal::SetupDuration {
                    max_bars: 20,
                    elapsed_bars: 0,
                }),
                Some(RiskPlan {
                    entry,
                    stop_loss: sl,
                    target_1: tp1,
                    target_2: tp2,
                    risk_reward_ratio: rrr,
                }),
            )
        }
        _ => (None, None, None, None),
    };

    CompositeSignal {
        direction,
        trigger,
        score: final_score,
        confidence,
        heat_score,
        permission,
        regime,
        target_zone,
        invalidation_zone,
        setup_duration,
        sr_zones,
        risk_plan,
        reasons,
        explanation,
        per_indicator: subscores,
    }
}

/// Convenience wrapper around [`aggregate_subscores`] that rounds the resulting
/// [`crate::model::RiskPlan`] to `instrument`'s tick size (see
/// [`crate::model::RiskPlan::rounded_to`]), so ATR-derived entry/stop/target prices land on a
/// tradable price grid instead of arbitrary floating-point values.
#[allow(clippy::too_many_arguments)]
pub fn aggregate_subscores_with_instrument(
    subscores: Vec<SubScore>,
    weights: Option<&HashMap<String, f64>>,
    regime: MarketRegime,
    sr_zones: Vec<SupportResistanceZone>,
    latest_bar: Option<&Bar>,
    atr_val: f64,
    instrument: &crate::model::InstrumentMeta,
) -> CompositeSignal {
    let mut signal = aggregate_subscores(subscores, weights, regime, sr_zones, latest_bar, atr_val);
    if let Some(plan) = &signal.risk_plan {
        signal.risk_plan = Some(plan.rounded_to(instrument));
    }
    signal
}

/// Pre-curated indicator weighting profiles (plan Anhang F & Offene Fragen).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum WeightPreset {
    #[default]
    Balanced,
    TrendFollowing,
    MeanReversion,
    VolumeLocation,
}

impl WeightPreset {
    pub fn weights(&self) -> HashMap<String, f64> {
        let mut map = HashMap::new();
        match self {
            WeightPreset::Balanced => {}
            WeightPreset::TrendFollowing => {
                map.insert("macd".to_string(), 2.0);
                map.insert("adx".to_string(), 2.0);
                map.insert("supertrend".to_string(), 2.0);
                map.insert("ema".to_string(), 1.5);
                map.insert("sma".to_string(), 1.5);
                map.insert("rsi".to_string(), 0.5);
                map.insert("stochastic".to_string(), 0.5);
            }
            WeightPreset::MeanReversion => {
                map.insert("rsi".to_string(), 2.0);
                map.insert("stoch_rsi".to_string(), 2.0);
                map.insert("stochastic".to_string(), 2.0);
                map.insert("bollinger".to_string(), 2.0);
                map.insert("cci".to_string(), 1.5);
                map.insert("mfi".to_string(), 1.5);
                map.insert("macd".to_string(), 0.5);
            }
            WeightPreset::VolumeLocation => {
                map.insert("volume_profile".to_string(), 2.5);
                map.insert("order_block".to_string(), 2.5);
                map.insert("liquidity_fvg".to_string(), 2.0);
                map.insert("pivots_structure".to_string(), 2.0);
                map.insert("vwap".to_string(), 1.5);
            }
        }
        map
    }
}
