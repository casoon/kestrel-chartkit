use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::engine::market_state::PlaybookState;
use crate::model::{MarketRegime, RiskPlan, SupportResistanceZone};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum SignalDirection {
    Bullish,
    #[default]
    Neutral,
    Bearish,
}

impl fmt::Display for SignalDirection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SignalDirection::Bullish => write!(f, "BULLISH"),
            SignalDirection::Neutral => write!(f, "NEUTRAL"),
            SignalDirection::Bearish => write!(f, "BEARISH"),
        }
    }
}

/// Konkreter Exekutions-Trigger ("Geh rein" / "Raus").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TriggerAction {
    Buy,
    Sell,
    Exit,
    #[default]
    Hold,
}

impl fmt::Display for TriggerAction {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            TriggerAction::Buy => write!(f, "BUY"),
            TriggerAction::Sell => write!(f, "SELL"),
            TriggerAction::Exit => write!(f, "EXIT"),
            TriggerAction::Hold => write!(f, "HOLD"),
        }
    }
}

/// Rechnerische Zielzone ("wie weit?").
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct TargetZone {
    pub price: f64,
    pub name: String,
    pub reward_atr: f64,
}

/// Strukturelles Ungültigkeits-Level / Stop-Loss ("wo ungültig?").
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct InvalidationZone {
    pub price: f64,
    pub reason: String,
    pub risk_atr: f64,
}

/// Zeitliche / Bar-basierte Gültigkeit des Signal-Kontexts ("wie lange?").
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SetupDuration {
    pub max_bars: u32,
    pub elapsed_bars: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum PermissionGrade {
    ClearToTrade,
    Caution,
    #[default]
    Veto,
}

impl fmt::Display for PermissionGrade {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            PermissionGrade::ClearToTrade => write!(f, "CLEAR TO TRADE"),
            PermissionGrade::Caution => write!(f, "CAUTION / PROTECT"),
            PermissionGrade::Veto => write!(f, "NO TRADE / VETO"),
        }
    }
}

impl From<PlaybookState> for PermissionGrade {
    fn from(state: PlaybookState) -> Self {
        match state {
            PlaybookState::Preferred => PermissionGrade::ClearToTrade,
            PlaybookState::Neutral => PermissionGrade::Caution,
            PlaybookState::Suppressed | PlaybookState::NotAllowed => PermissionGrade::Veto,
        }
    }
}

impl From<PermissionGrade> for PlaybookState {
    fn from(grade: PermissionGrade) -> Self {
        match grade {
            PermissionGrade::ClearToTrade => PlaybookState::Preferred,
            PermissionGrade::Caution => PlaybookState::Neutral,
            PermissionGrade::Veto => PlaybookState::Suppressed,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SubScore {
    pub indicator: String,
    pub score: f64, // -1.0 ..= 1.0
    pub raw_value: f64,
    pub reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CompositeSignal {
    pub direction: SignalDirection,
    pub trigger: TriggerAction,
    pub score: f64,
    pub confidence: f64,
    pub heat_score: f64, // 0.0 ..= 1.0
    pub permission: PermissionGrade,
    pub regime: MarketRegime,
    pub target_zone: Option<TargetZone>,
    pub invalidation_zone: Option<InvalidationZone>,
    pub setup_duration: Option<SetupDuration>,
    pub sr_zones: Vec<SupportResistanceZone>,
    pub risk_plan: Option<RiskPlan>,
    pub reasons: Vec<String>,
    pub explanation: String,
    pub per_indicator: Vec<SubScore>,
}
