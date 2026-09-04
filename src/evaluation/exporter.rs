use crate::model::MarketRegime;
use crate::signal::{CompositeSignal, TriggerAction};
use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Exportable snapshot record of features, regime, and signal outputs for research or logging sinks.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FeatureRecord {
    pub timestamp: i64,
    pub symbol: String,
    pub regime: MarketRegime,
    pub indicator_values: HashMap<String, f64>,
    pub subscores: HashMap<String, f64>,
    pub trigger: TriggerAction,
    pub score: f64,
    pub confidence: f64,
}

/// Feature and research dataset exporter.
#[derive(Debug, Clone, Default)]
pub struct FeatureExporter;

impl FeatureExporter {
    pub fn new() -> Self {
        Self
    }

    /// Builds a sink-independent `FeatureRecord` snapshot.
    pub fn export_record(
        &self,
        timestamp: i64,
        symbol: &str,
        regime: MarketRegime,
        indicator_values: HashMap<String, f64>,
        signal: &CompositeSignal,
    ) -> FeatureRecord {
        let mut subscores = HashMap::new();
        for sub in &signal.per_indicator {
            subscores.insert(sub.indicator.clone(), sub.score);
        }

        FeatureRecord {
            timestamp,
            symbol: symbol.to_string(),
            regime,
            indicator_values,
            subscores,
            trigger: signal.trigger,
            score: signal.score,
            confidence: signal.confidence,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signal::PermissionGrade;

    #[test]
    fn test_feature_exporter() {
        let exporter = FeatureExporter::new();
        let signal = CompositeSignal {
            score: 0.85,
            direction: crate::signal::SignalDirection::Bullish,
            permission: PermissionGrade::ClearToTrade,
            trigger: TriggerAction::Buy,
            target_zone: None,
            invalidation_zone: None,
            setup_duration: None,
            risk_plan: None,
            heat_score: 0.5,
            sr_zones: Vec::new(),
            per_indicator: Vec::new(),
            reasons: Vec::new(),
            explanation: "test".to_string(),
            confidence: 0.9,
            regime: MarketRegime::BullishExpansion,
        };

        let rec = exporter.export_record(
            1000,
            "BTCUSD",
            MarketRegime::BullishExpansion,
            HashMap::from([("rsi".to_string(), 65.0)]),
            &signal,
        );

        assert_eq!(rec.symbol, "BTCUSD");
        assert_eq!(rec.indicator_values["rsi"], 65.0);
    }
}
