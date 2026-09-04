use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::adx::Adx;
use super::alligator::AlligatorEngine;
use super::anchored_vwap::{AnchoredVwapEngine, VwapAnchorKind, ZeroVolumePolicy};
use super::atr::Atr;
use super::bollinger::BollingerBands;
use super::bop::BalanceOfPowerEngine;
use super::bos_choch::BosChochEngine;
use super::buy_sell_pressure::BuySellPressureEstimator;
use super::candle_story::CandleStoryEngine;
use super::cci::Cci;
use super::chaikin_osc::ChaikinOscillatorEngine;
use super::chandelier_exit::ChandelierExitEngine;
use super::choppiness::ChoppinessIndexEngine;
use super::connors_rsi::ConnorsRsiEngine;
use super::coppock::CoppockCurveEngine;
use super::dpo::DpoEngine;
use super::efficiency::LegEfficiencyEngine;
use super::envelope::EnvelopeEngine;
use super::eom::EomEngine;
use super::fisher_transform::FisherTransform;
use super::kst::KstEngine;
use super::liquidity_fvg::LiquidityFvgEngine;
use super::liquidity_sweeps::LiquiditySweepEngine;
use super::lsma::LsmaEngine;
use super::macd::Macd;
use super::market_structure_breaks::MarketStructureBreaksEngine;
use super::mass_index::MassIndexEngine;
use super::mcginley::McGinleyDynamicEngine;
use super::mfi::Mfi;
use super::midas::{MidasCurveEngine, MidasMode};
use super::momentum_indicators::{
    AwesomeOscillatorEngine, CmoEngine, ElderRayEngine, PpoEngine, RocEngine, StochasticEngine,
    UltimateOscillatorEngine,
};
use super::moving_averages::{
    DemaEngine, EmaEngine, HmaEngine, KamaEngine, SmaEngine, VwmaEngine, WmaEngine,
};
use super::multi_factor::MultiFactorMarketScore;
use super::nvi_pvi::{NviEngine, PviEngine};
use super::order_block::OrderBlockEngine;
pub use super::params::{ParamValue, TypedParams};
use super::pivot_sets::{PivotSetType, PivotSetsEngine};
use super::pivots_structure::PivotStructureEngine;
use super::rsi::Rsi;
use super::rvi::RviEngine;
use super::stoch_rsi::StochRsi;
use super::tema::TemaEngine;
use super::trend_quality::TrendQualityScoreEngine;
use super::trend_structural::{
    AroonEngine, DmiEngine, IchimokuEngine, ParabolicSarEngine, SupertrendEngine,
};
use super::tsi::Tsi;
use super::vix_fix::WilliamsVixFix;
use super::volatility_indicators::{
    DonchianChannelEngine, GarmanKlassVolatilityEngine, HistoricalVolatilityEngine,
    KeltnerChannelEngine, TrueRangeEngine,
};
use super::volatility_regime::VolatilityRegimeDetector;
use super::volume_flow::{CvdEngine, KlingerVolumeForceEngine};
use super::volume_flow_hires::HiResVolumeFlowEngine;
use super::volume_indicators::{AccDistEngine, CmfEngine, ObvEngine, RvolEngine, VolumeEngine};
use super::volume_profile::VolumeProfileEngine;
use super::volume_profile_extended::ExtendedVolumeProfileEngine;
use super::volume_profile_persistent::PersistentVolumeProfileEngine;
use super::vortex::VortexEngine;
use super::vwap::Vwap;
use super::wavetrend::WaveTrendEngine;
use super::williams_r::WilliamsR;
use super::zigzag::ZigZagEngine;
use super::zigzag_advanced::{AdvancedZigZagEngine, ZigZagDeviationMode};
use super::zscore::ZScoreEngine;
use super::Indicator;

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct IndicatorCatalogEntry {
    pub name: &'static str,
    pub description: &'static str,
    pub default_params: HashMap<String, f64>,
}

pub fn catalog() -> Vec<IndicatorCatalogEntry> {
    vec![
        IndicatorCatalogEntry {
            name: "rsi",
            description: "Relative Strength Index",
            default_params: [
                ("rsi_len".to_string(), 14.0),
                ("avg_len".to_string(), 3.0),
                ("sig_len".to_string(), 3.0),
                ("overbought".to_string(), 70.0),
                ("oversold".to_string(), 30.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "macd",
            description: "Moving Average Convergence Divergence",
            default_params: [
                ("fast_len".to_string(), 12.0),
                ("slow_len".to_string(), 26.0),
                ("signal_len".to_string(), 9.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "bollinger",
            description: "Bollinger Bands",
            default_params: [("len".to_string(), 20.0), ("mult".to_string(), 2.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "adx",
            description: "Average Directional Index",
            default_params: [
                ("di_len".to_string(), 14.0),
                ("adx_smooth".to_string(), 14.0),
                ("level_weak".to_string(), 20.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "stoch_rsi",
            description: "Stochastic RSI",
            default_params: [
                ("rsi_len".to_string(), 14.0),
                ("stoch_len".to_string(), 14.0),
                ("k_len".to_string(), 3.0),
                ("d_len".to_string(), 3.0),
                ("overbought".to_string(), 80.0),
                ("oversold".to_string(), 20.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "cci",
            description: "Commodity Channel Index",
            default_params: [
                ("cci_len".to_string(), 20.0),
                ("overbought".to_string(), 100.0),
                ("oversold".to_string(), -100.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "mfi",
            description: "Money Flow Index",
            default_params: [
                ("mfi_len".to_string(), 14.0),
                ("overbought".to_string(), 80.0),
                ("oversold".to_string(), 20.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "atr",
            description: "Average True Range",
            default_params: [("atr_len".to_string(), 14.0), ("sig_len".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "chandelier_exit",
            description: "Chandelier Exit (ATR trailing stop with direction flip)",
            default_params: [("length".to_string(), 22.0), ("atr_mult".to_string(), 3.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "midas",
            description: "MIDAS launch-anchored curve with Topfinder/Bottomfinder projection (build_typed with mode=topfinder|bottomfinder)",
            default_params: [("maturity_bars".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "hires_volume_flow",
            description: "High-resolution volume flow with absorption detection (OHLC-estimated via this registry entry point; use HiResVolumeFlowEngine::on_bar_with_aggressor/on_intrabar_group directly for direct aggressor/intrabar-delta resolution)",
            default_params: [("window_len".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "extended_volume_profile",
            description: "Full-bin price/volume profile with HVN/LVN/AVN classification, a delta profile, and zone formation (use ExtendedVolumeProfileEngine::on_intrabar_group directly for intrabar-resolution distribution)",
            default_params: [("lookback".to_string(), 70.0), ("num_bins".to_string(), 30.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "persistent_volume_profile",
            description: "Fixed-price-grid volume profile with real bin lifecycle (birth/growth/expiry across updates) and a per-bin absorption profile",
            default_params: [("lookback".to_string(), 70.0), ("bin_width".to_string(), 1.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "trend_relationship",
            description: "Adaptive trend relationship between two configurable smoothers (build via build_typed with fast_kind/slow_kind params: ema|sma|rma|alma|jma)",
            default_params: [("fast_len".to_string(), 9.0), ("slow_len".to_string(), 21.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "williams_r",
            description: "Williams %R",
            default_params: [
                ("wpr_len".to_string(), 14.0),
                ("overbought".to_string(), 80.0),
                ("oversold".to_string(), 20.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "tsi",
            description: "True Strength Index",
            default_params: [
                ("long_len".to_string(), 25.0),
                ("short_len".to_string(), 13.0),
                ("sig_len".to_string(), 7.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "fisher_transform",
            description: "Fisher Transform",
            default_params: [
                ("fish_len".to_string(), 10.0),
                ("overbought".to_string(), 1.5),
                ("oversold".to_string(), -1.5),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "vwap",
            description: "Rolling Volume Weighted Average Price with sigma bands and slope",
            default_params: [
                ("window".to_string(), 390.0),
                ("slope_lookback".to_string(), 20.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "volume",
            description: "Volume and Average Volume",
            default_params: [("ma_period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "rvol",
            description: "Relative Volume vs Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "obv",
            description: "On-Balance Volume",
            default_params: HashMap::new(),
        },
        IndicatorCatalogEntry {
            name: "cmf",
            description: "Chaikin Money Flow",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "acc_dist",
            description: "Accumulation / Distribution Line",
            default_params: HashMap::new(),
        },
        IndicatorCatalogEntry {
            name: "true_range",
            description: "True Range in price units",
            default_params: HashMap::new(),
        },
        IndicatorCatalogEntry {
            name: "keltner",
            description: "Keltner Channels",
            default_params: [
                ("ema_period".to_string(), 20.0),
                ("atr_period".to_string(), 10.0),
                ("multiplier".to_string(), 2.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "donchian",
            description: "Donchian Channels",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "historical_volatility",
            description: "Annualized Historical Volatility",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "garman_klass",
            description: "Garman-Klass Volatility Estimator",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "sma",
            description: "Simple Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "ema",
            description: "Exponential Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "wma",
            description: "Weighted Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "vwma",
            description: "Volume-Weighted Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "hma",
            description: "Hull Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "dema",
            description: "Double Exponential Moving Average",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "kama",
            description: "Kaufman's Adaptive Moving Average",
            default_params: [
                ("period".to_string(), 10.0),
                ("fast_period".to_string(), 2.0),
                ("slow_period".to_string(), 30.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "dmi",
            description: "Directional Movement Index (+DI / -DI)",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "aroon",
            description: "Aroon Indicator (Up, Down, Oscillator)",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "parabolic_sar",
            description: "Parabolic SAR",
            default_params: [("step".to_string(), 0.02), ("max_step".to_string(), 0.20)].into(),
        },
        IndicatorCatalogEntry {
            name: "supertrend",
            description: "Supertrend ATR Trailing Stop",
            default_params: [
                ("period".to_string(), 10.0),
                ("multiplier".to_string(), 3.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "ichimoku",
            description: "Ichimoku Kinko Hyo Cloud",
            default_params: [
                ("tenkan_p".to_string(), 9.0),
                ("kijun_p".to_string(), 26.0),
                ("senkou_b_p".to_string(), 52.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "stochastic",
            description: "Classic Stochastic Oscillator",
            default_params: [
                ("k_period".to_string(), 14.0),
                ("d_period".to_string(), 3.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "roc",
            description: "Rate of Change / Momentum",
            default_params: [("period".to_string(), 12.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "ultimate_oscillator",
            description: "Ultimate Oscillator",
            default_params: [
                ("period1".to_string(), 7.0),
                ("period2".to_string(), 14.0),
                ("period3".to_string(), 28.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "awesome_oscillator",
            description: "Awesome Oscillator",
            default_params: [
                ("fast_period".to_string(), 5.0),
                ("slow_period".to_string(), 34.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "ppo",
            description: "Percentage Price Oscillator",
            default_params: [
                ("fast_period".to_string(), 12.0),
                ("slow_period".to_string(), 26.0),
                ("signal_period".to_string(), 9.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "wavetrend",
            description: "WaveTrend Oscillator (wt1, wt2)",
            default_params: [
                ("n1".to_string(), 10.0),
                ("n2".to_string(), 21.0),
                ("ob_level".to_string(), 60.0),
                ("os_level".to_string(), -60.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "cmo",
            description: "Chande Momentum Oscillator",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "elder_ray",
            description: "Elder Ray Index (Bull/Bear Power)",
            default_params: [("period".to_string(), 13.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "anchored_vwap",
            description: "Anchored VWAP Engine",
            default_params: [("mult1".to_string(), 1.0), ("mult2".to_string(), 2.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "cvd",
            description: "Cumulative Volume Delta",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "klinger",
            description: "Klinger Volume Force Oscillator",
            default_params: [
                ("fast_len".to_string(), 34.0),
                ("slow_len".to_string(), 55.0),
                ("signal_len".to_string(), 13.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "zigzag",
            description: "ZigZag Swing Leg Engine",
            default_params: [
                ("depth".to_string(), 12.0),
                ("deviation_pct".to_string(), 5.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "zigzag_advanced",
            description: "ZigZag with backstep, running-leg/confirmation status, and ATR-mode deviation (build_typed with deviation_mode=percent|atr_multiple); AdvancedZigZagEngine::reduce/project_to_timeframe for recursive levels and HTF projection",
            default_params: [
                ("depth".to_string(), 3.0),
                ("backstep".to_string(), 2.0),
                ("deviation_pct".to_string(), 1.0),
                ("atr_len".to_string(), 14.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "pivot_sets",
            description: "Multi-Pivot Set Engine",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "tema",
            description: "Triple Exponential Moving Average",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "lsma",
            description: "Least Squares Moving Average / Linear Regression",
            default_params: [("period".to_string(), 25.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "mcginley",
            description: "McGinley Dynamic Moving Average",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "envelope",
            description: "Moving Average Envelopes",
            default_params: [("period".to_string(), 20.0), ("percent".to_string(), 2.5)].into(),
        },
        IndicatorCatalogEntry {
            name: "choppiness",
            description: "Choppiness Index",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "vortex",
            description: "Vortex Indicator (+VI, -VI)",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "alligator",
            description: "Williams Alligator (Jaw, Teeth, Lips)",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "connors_rsi",
            description: "Connors RSI",
            default_params: [
                ("rsi_len".to_string(), 3.0),
                ("streak_len".to_string(), 2.0),
                ("rank_len".to_string(), 100.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "coppock",
            description: "Coppock Curve",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "dpo",
            description: "Detrended Price Oscillator",
            default_params: [("period".to_string(), 21.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "kst",
            description: "Know Sure Thing Oscillator",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "mass_index",
            description: "Mass Index Reversal Detector",
            default_params: [("period".to_string(), 25.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "rvi",
            description: "Relative Vigor Index",
            default_params: [("period".to_string(), 10.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "bop",
            description: "Balance of Power",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "eom",
            description: "Ease of Movement",
            default_params: [
                ("period".to_string(), 14.0),
                ("volume_divisor".to_string(), 10000.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "nvi",
            description: "Negative Volume Index",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "pvi",
            description: "Positive Volume Index",
            default_params: [].into(),
        },
        IndicatorCatalogEntry {
            name: "chaikin_oscillator",
            description: "Chaikin Oscillator",
            default_params: [
                ("fast_len".to_string(), 3.0),
                ("slow_len".to_string(), 10.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "bos_choch",
            description: "BOS and CHoCH Market Structure Engine",
            default_params: [("pivot_len".to_string(), 5.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "liquidity_sweeps",
            description: "Liquidity Sweeps and EQH/EQL Detector",
            default_params: [
                ("pivot_len".to_string(), 5.0),
                ("tolerance_pct".to_string(), 0.2),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "liquidity_pools",
            description: "BSL/SSL liquidity pools with explicit stop-hunt/breakout/reclaim classification (see also FvgZoneTracker and SmartMoneyStructureLinker for FVG-fill tracking and cross-detector confluence)",
            default_params: [
                ("pivot_len".to_string(), 5.0),
                ("tolerance_pct".to_string(), 0.2),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "wyckoff",
            description: "Wyckoff accumulation/distribution state machine: range-lock, Phases A-E, Spring/UTAD, SOS/SOW/LPS/LPSY, sequence validation and Cause/Quality scoring",
            default_params: [
                ("range_lookback".to_string(), 20.0),
                ("range_atr_max".to_string(), 3.0),
                ("min_range_bars".to_string(), 6.0),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "trend_quality",
            description: "Trend Quality Score Engine",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "buy_sell_pressure",
            description: "Buy/Sell Pressure Estimator",
            default_params: [("period".to_string(), 14.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "volatility_regime",
            description: "Volatility Regime & Squeeze Detector",
            default_params: [
                ("period".to_string(), 20.0),
                ("bb_mult".to_string(), 2.0),
                ("kc_mult".to_string(), 1.5),
            ]
            .into(),
        },
        IndicatorCatalogEntry {
            name: "zscore",
            description: "Rolling Z-Score Engine",
            default_params: [("period".to_string(), 20.0)].into(),
        },
        IndicatorCatalogEntry {
            name: "multi_factor",
            description: "Multi-Factor Composite Market Score",
            default_params: [("period".to_string(), 14.0)].into(),
        },
    ]
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum RegistryError {
    UnknownIndicator(String),
    InvalidParameter {
        parameter: String,
        value: f64,
        reason: String,
    },
    /// A [`ParamValue`] with no numeric equivalent (`Enum`/`Text`/`Timeframe`/`Source`) was
    /// passed to [`build_typed`] for an indicator whose current parameter surface is `f64`-only.
    UnsupportedParameterType {
        parameter: String,
        type_name: String,
    },
    /// A [`ParamValue::Enum`] string did not match any variant accepted for this parameter.
    InvalidEnumValue {
        parameter: String,
        value: String,
        reason: String,
    },
    /// A parameter is a known, supported type but cannot be applied to this specific indicator
    /// (e.g. a non-`Close` `source` on a range/OHLC-dependent indicator — see
    /// [`super::source_mapped::SourceMapped`]'s doc comment for why).
    IncompatibleParameter {
        parameter: String,
        indicator: String,
        reason: String,
    },
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::UnknownIndicator(name) => write!(f, "Unknown indicator: {}", name),
            RegistryError::InvalidParameter {
                parameter,
                value,
                reason,
            } => write!(
                f,
                "Invalid parameter '{}' (value {}): {}",
                parameter, value, reason
            ),
            RegistryError::UnsupportedParameterType {
                parameter,
                type_name,
            } => write!(
                f,
                "Parameter '{}' has unsupported type '{}' for this indicator",
                parameter, type_name
            ),
            RegistryError::InvalidEnumValue {
                parameter,
                value,
                reason,
            } => write!(
                f,
                "Invalid value '{}' for parameter '{}': {}",
                value, parameter, reason
            ),
            RegistryError::IncompatibleParameter {
                parameter,
                indicator,
                reason,
            } => write!(
                f,
                "Parameter '{}' is not compatible with indicator '{}': {}",
                parameter, indicator, reason
            ),
        }
    }
}

impl std::error::Error for RegistryError {}

fn get_usize_p(
    params: &HashMap<String, f64>,
    name: &str,
    default: usize,
    min: usize,
    max: usize,
) -> Result<usize, RegistryError> {
    if let Some(&val) = params.get(name) {
        if !val.is_finite() || val.fract() != 0.0 || val < (min as f64) || val > (max as f64) {
            return Err(RegistryError::InvalidParameter {
                parameter: name.to_string(),
                value: val,
                reason: format!(
                    "Value must be a whole, finite number between {} and {}",
                    min, max
                ),
            });
        }
        Ok(val as usize)
    } else {
        Ok(default)
    }
}

fn get_f64_p(
    params: &HashMap<String, f64>,
    name: &str,
    default: f64,
    min: f64,
    max: f64,
) -> Result<f64, RegistryError> {
    if let Some(&val) = params.get(name) {
        if !val.is_finite() || val < min || val > max {
            return Err(RegistryError::InvalidParameter {
                parameter: name.to_string(),
                value: val,
                reason: format!("Value must be a finite number between {} and {}", min, max),
            });
        }
        Ok(val)
    } else {
        Ok(default)
    }
}

fn ensure_less(
    parameter: &str,
    value: f64,
    upper_parameter: &str,
    upper_value: f64,
) -> Result<(), RegistryError> {
    if value < upper_value {
        return Ok(());
    }

    Err(RegistryError::InvalidParameter {
        parameter: parameter.to_string(),
        value,
        reason: format!("{} must be smaller than {}", parameter, upper_parameter),
    })
}

pub fn build_checked(
    name: &str,
    params: &HashMap<String, f64>,
) -> Result<Box<dyn Indicator>, RegistryError> {
    match name.to_lowercase().as_str() {
        "rsi" => {
            let rsi_len = get_usize_p(params, "rsi_len", 14, 1, 10000)?;
            let avg_len = get_usize_p(params, "avg_len", 3, 1, 10000)?;
            let sig_len = get_usize_p(params, "sig_len", 3, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 70.0, 0.0, 100.0)?;
            let oversold = get_f64_p(params, "oversold", 30.0, 0.0, 100.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(Rsi::new(
                rsi_len, avg_len, sig_len, 50.0, overbought, oversold, 5, true, 100, 4, 10.0,
            )))
        }
        "macd" => {
            let fast_len = get_usize_p(params, "fast_len", 12, 1, 10000)?;
            let slow_len = get_usize_p(params, "slow_len", 26, 1, 10000)?;
            let signal_len = get_usize_p(params, "signal_len", 9, 1, 10000)?;
            ensure_less("fast_len", fast_len as f64, "slow_len", slow_len as f64)?;
            Ok(Box::new(Macd::new(fast_len, slow_len, signal_len)))
        }
        "bollinger" | "bb" => {
            let len = get_usize_p(params, "len", 20, 1, 10000)?;
            let mult = get_f64_p(params, "mult", 2.0, 0.01, 100.0)?;
            Ok(Box::new(BollingerBands::new(len, mult)))
        }
        "adx" => {
            let di_len = get_usize_p(params, "di_len", 14, 1, 10000)?;
            let adx_smooth = get_usize_p(params, "adx_smooth", 14, 1, 10000)?;
            let level_weak = get_f64_p(params, "level_weak", 20.0, 0.0, 100.0)?;
            Ok(Box::new(Adx::new(di_len, adx_smooth, 3, level_weak)))
        }
        "stoch_rsi" | "srsi" => {
            let rsi_len = get_usize_p(params, "rsi_len", 14, 1, 10000)?;
            let stoch_len = get_usize_p(params, "stoch_len", 14, 1, 10000)?;
            let k_len = get_usize_p(params, "k_len", 3, 1, 10000)?;
            let d_len = get_usize_p(params, "d_len", 3, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 80.0, 0.0, 100.0)?;
            let oversold = get_f64_p(params, "oversold", 20.0, 0.0, 100.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(StochRsi::new(
                rsi_len, stoch_len, k_len, d_len, 50.0, overbought, oversold, 5, true, 50, 50, 4,
                10.0,
            )))
        }
        "cci" => {
            let cci_len = get_usize_p(params, "cci_len", 20, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 100.0, -1000.0, 1000.0)?;
            let oversold = get_f64_p(params, "oversold", -100.0, -1000.0, 1000.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(Cci::new(
                cci_len, 3, 3, 5, oversold, overbought, true, 100, 4, 25.0,
            )))
        }
        "mfi" => {
            let mfi_len = get_usize_p(params, "mfi_len", 14, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 80.0, 0.0, 100.0)?;
            let oversold = get_f64_p(params, "oversold", 20.0, 0.0, 100.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(Mfi::new(
                mfi_len, 3, 3, 50.0, overbought, oversold, 5, true,
            )))
        }
        "atr" => {
            let atr_len = get_usize_p(params, "atr_len", 14, 1, 10000)?;
            let sig_len = get_usize_p(params, "sig_len", 20, 1, 10000)?;
            Ok(Box::new(Atr::new(atr_len, sig_len)))
        }
        "chandelier_exit" | "ce" => {
            let length = get_usize_p(params, "length", 22, 1, 10000)?;
            let atr_mult = get_f64_p(params, "atr_mult", 3.0, 0.01, 100.0)?;
            Ok(Box::new(ChandelierExitEngine::new(length, atr_mult)))
        }
        "midas" => {
            // Fixed Topfinder/Hlc3 via this f64-only entry point; use `build_typed` with `mode`
            // (ParamValue::Enum) and `source` to select Bottomfinder or another price source.
            let maturity_bars = get_usize_p(params, "maturity_bars", 20, 1, 10000)?;
            Ok(Box::new(MidasCurveEngine::new(
                MidasMode::Topfinder,
                crate::model::Source::Hlc3,
                maturity_bars as u32,
            )))
        }
        "trend_relationship" => {
            // Fixed EMA/EMA via this f64-only entry point; use `build_typed` with
            // `fast_kind`/`slow_kind` (ParamValue::Enum) to select other smoother kinds.
            let fast_len = get_usize_p(params, "fast_len", 9, 1, 10000)?;
            let slow_len = get_usize_p(params, "slow_len", 21, 1, 10000)?;
            ensure_less("fast_len", fast_len as f64, "slow_len", slow_len as f64)?;
            Ok(Box::new(
                super::trend_relationship::AdaptiveTrendRelationship::new(
                    super::smoothing::SmootherKind::Ema,
                    fast_len,
                    super::smoothing::SmootherKind::Ema,
                    slow_len,
                ),
            ))
        }
        "williams_r" | "wpr" => {
            let wpr_len = get_usize_p(params, "wpr_len", 14, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 80.0, 0.0, 100.0)?;
            let oversold = get_f64_p(params, "oversold", 20.0, 0.0, 100.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(WilliamsR::new(
                wpr_len, 3, 3, 50.0, overbought, oversold, 5, true, 50, 4, 10.0,
            )))
        }
        "tsi" => {
            let long_len = get_usize_p(params, "long_len", 25, 1, 10000)?;
            let short_len = get_usize_p(params, "short_len", 13, 1, 10000)?;
            let sig_len = get_usize_p(params, "sig_len", 7, 1, 10000)?;
            ensure_less("short_len", short_len as f64, "long_len", long_len as f64)?;
            Ok(Box::new(Tsi::new(
                long_len, short_len, sig_len, 0.0, 25.0, -25.0, 5, true, 50, 25, 4, 5.0,
            )))
        }
        "fisher_transform" | "fisher" => {
            let fish_len = get_usize_p(params, "fish_len", 10, 1, 10000)?;
            let overbought = get_f64_p(params, "overbought", 1.5, -100.0, 100.0)?;
            let oversold = get_f64_p(params, "oversold", -1.5, -100.0, 100.0)?;
            ensure_less("oversold", oversold, "overbought", overbought)?;
            Ok(Box::new(FisherTransform::new(
                fish_len, 2, 3, 0.0, overbought, oversold, 5, true, 40, 4, 0.5,
            )))
        }
        "order_block" | "ob" => {
            let atr_len = get_usize_p(params, "atr_len", 14, 1, 10000)?;
            let min_disp = get_f64_p(params, "min_disp", 1.0, 0.01, 100.0)?;
            Ok(Box::new(OrderBlockEngine::new(atr_len, min_disp)))
        }
        "liquidity_fvg" | "fvg" | "smc" => {
            let lookback = get_usize_p(params, "lookback", 20, 1, 10000)?;
            Ok(Box::new(LiquidityFvgEngine::new(lookback)))
        }
        "market_structure_breaks" | "bos" | "choch" => {
            let lookback = get_usize_p(params, "lookback", 5, 1, 10000)?;
            Ok(Box::new(MarketStructureBreaksEngine::new(lookback)))
        }
        "pivots_structure" | "pivots" => {
            let left_bars = get_usize_p(params, "left_bars", 5, 1, 10000)?;
            let right_bars = get_usize_p(params, "right_bars", 5, 1, 10000)?;
            let score_window = get_usize_p(params, "score_window", 10, 1, 10000)?;
            Ok(Box::new(PivotStructureEngine::new(
                left_bars,
                right_bars,
                score_window,
            )))
        }
        "volume_profile" | "vp" => {
            let lookback = get_usize_p(params, "lookback", 70, 1, 10000)?;
            let num_bins = get_usize_p(params, "num_bins", 30, 1, 1000)?;
            Ok(Box::new(VolumeProfileEngine::new(lookback, num_bins)))
        }
        "extended_volume_profile" | "vp_extended" => {
            let lookback = get_usize_p(params, "lookback", 70, 1, 10000)?;
            let num_bins = get_usize_p(params, "num_bins", 30, 1, 1000)?;
            Ok(Box::new(ExtendedVolumeProfileEngine::new(
                lookback, num_bins,
            )))
        }
        "persistent_volume_profile" | "vp_persistent" => {
            let lookback = get_usize_p(params, "lookback", 70, 1, 10000)?;
            let bin_width = get_f64_p(params, "bin_width", 1.0, 1e-6, 1_000_000.0)?;
            Ok(Box::new(PersistentVolumeProfileEngine::new(
                lookback, bin_width,
            )))
        }
        "vwap" => {
            let window = get_usize_p(params, "window", 390, 1, 10000)?;
            let slope_lookback = get_usize_p(params, "slope_lookback", 20, 1, 10000)?;
            Ok(Box::new(Vwap::new(window, slope_lookback)))
        }
        "vix_fix" | "wvf" => {
            let pd = get_usize_p(params, "pd", 22, 1, 10000)?;
            let bband_len = get_usize_p(params, "bband_len", 20, 1, 10000)?;
            let mult = get_f64_p(params, "mult", 2.0, 0.01, 100.0)?;
            Ok(Box::new(WilliamsVixFix::new(pd, bband_len, mult)))
        }
        "candle_story" | "pinbar" => Ok(Box::new(CandleStoryEngine::new())),
        "efficiency" | "leg_efficiency" | "er" => {
            let len = get_usize_p(params, "len", 14, 1, 10000)?;
            Ok(Box::new(LegEfficiencyEngine::new(len)))
        }
        "volume" => {
            let ma = get_usize_p(params, "ma_period", 20, 1, 10000)?;
            Ok(Box::new(VolumeEngine::new(ma)))
        }
        "rvol" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(RvolEngine::new(p)))
        }
        "obv" => Ok(Box::new(ObvEngine::new())),
        "cmf" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(CmfEngine::new(p)))
        }
        "acc_dist" => Ok(Box::new(AccDistEngine::new())),
        "true_range" => Ok(Box::new(TrueRangeEngine::new())),
        "keltner" => {
            let ma = get_usize_p(params, "ma_period", 20, 1, 10000)?;
            let atr = get_usize_p(params, "atr_period", 10, 1, 10000)?;
            let mult = get_f64_p(params, "multiplier", 2.0, 0.01, 100.0)?;
            Ok(Box::new(KeltnerChannelEngine::new(ma, atr, mult)))
        }
        "donchian" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(DonchianChannelEngine::new(p)))
        }
        "historical_volatility" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(HistoricalVolatilityEngine::new(p)))
        }
        "garman_klass" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(GarmanKlassVolatilityEngine::new(p)))
        }
        "sma" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(SmaEngine::new(p)))
        }
        "ema" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(EmaEngine::new(p)))
        }
        "wma" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(WmaEngine::new(p)))
        }
        "vwma" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(VwmaEngine::new(p)))
        }
        "hma" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(HmaEngine::new(p)))
        }
        "dema" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            Ok(Box::new(DemaEngine::new(p)))
        }
        "kama" => {
            let p = get_usize_p(params, "period", 10, 1, 10000)?;
            let fast = get_usize_p(params, "fast_period", 2, 1, 10000)?;
            let slow = get_usize_p(params, "slow_period", 30, 1, 10000)?;
            ensure_less("fast_period", fast as f64, "slow_period", slow as f64)?;
            Ok(Box::new(KamaEngine::new(p, fast, slow)))
        }
        "dmi" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(DmiEngine::new(p)))
        }
        "aroon" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(AroonEngine::new(p)))
        }
        "parabolic_sar" => {
            let step = get_f64_p(params, "step", 0.02, 0.001, 1.0)?;
            let max_step = get_f64_p(params, "max_step", 0.20, 0.001, 1.0)?;
            if step > max_step {
                return Err(RegistryError::InvalidParameter {
                    parameter: "step".to_string(),
                    value: step,
                    reason: "step must not exceed max_step".to_string(),
                });
            }
            Ok(Box::new(ParabolicSarEngine::new(step, max_step)))
        }
        "supertrend" => {
            let p = get_usize_p(params, "period", 10, 1, 10000)?;
            let mult = get_f64_p(params, "multiplier", 3.0, 0.01, 100.0)?;
            Ok(Box::new(SupertrendEngine::new(p, mult)))
        }
        "ichimoku" => {
            let tenkan = get_usize_p(params, "tenkan_p", 9, 1, 10000)?;
            let kijun = get_usize_p(params, "kijun_p", 26, 1, 10000)?;
            let senkou_b = get_usize_p(params, "senkou_b_p", 52, 1, 10000)?;
            ensure_less("tenkan_p", tenkan as f64, "kijun_p", kijun as f64)?;
            ensure_less("kijun_p", kijun as f64, "senkou_b_p", senkou_b as f64)?;
            Ok(Box::new(IchimokuEngine::new(tenkan, kijun, senkou_b)))
        }
        "stochastic" => {
            let k = get_usize_p(params, "k_period", 14, 1, 10000)?;
            let d = get_usize_p(params, "d_period", 3, 1, 10000)?;
            Ok(Box::new(StochasticEngine::new(k, d)))
        }
        "roc" => {
            let p = get_usize_p(params, "period", 12, 1, 10000)?;
            Ok(Box::new(RocEngine::new(p)))
        }
        "ultimate_oscillator" => {
            let p1 = get_usize_p(params, "period1", 7, 1, 10000)?;
            let p2 = get_usize_p(params, "period2", 14, 1, 10000)?;
            let p3 = get_usize_p(params, "period3", 28, 1, 10000)?;
            ensure_less("period1", p1 as f64, "period2", p2 as f64)?;
            ensure_less("period2", p2 as f64, "period3", p3 as f64)?;
            Ok(Box::new(UltimateOscillatorEngine::new(p1, p2, p3)))
        }
        "awesome_oscillator" => {
            let fast = get_usize_p(params, "fast_period", 5, 1, 10000)?;
            let slow = get_usize_p(params, "slow_period", 34, 1, 10000)?;
            ensure_less("fast_period", fast as f64, "slow_period", slow as f64)?;
            Ok(Box::new(AwesomeOscillatorEngine::new(fast, slow)))
        }
        "ppo" => {
            let fast = get_usize_p(params, "fast_period", 12, 1, 10000)?;
            let slow = get_usize_p(params, "slow_period", 26, 1, 10000)?;
            let signal = get_usize_p(params, "signal_period", 9, 1, 10000)?;
            ensure_less("fast_period", fast as f64, "slow_period", slow as f64)?;
            Ok(Box::new(PpoEngine::new(fast, slow, signal)))
        }
        "wavetrend" | "wt" => {
            let n1 = get_usize_p(params, "n1", 10, 1, 10000)?;
            let n2 = get_usize_p(params, "n2", 21, 1, 10000)?;
            let ob = get_f64_p(params, "ob_level", 60.0, -100.0, 100.0)?;
            let os = get_f64_p(params, "os_level", -60.0, -100.0, 100.0)?;
            Ok(Box::new(WaveTrendEngine::new(n1, n2, ob, os)))
        }
        "cmo" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(CmoEngine::new(p)))
        }
        "elder_ray" => {
            let p = get_usize_p(params, "period", 13, 1, 10000)?;
            Ok(Box::new(ElderRayEngine::new(p)))
        }
        "anchored_vwap" | "avwap" => {
            let m1 = get_f64_p(params, "mult1", 1.0, 0.01, 100.0)?;
            let m2 = get_f64_p(params, "mult2", 2.0, 0.01, 100.0)?;
            Ok(Box::new(AnchoredVwapEngine::new(
                VwapAnchorKind::Session,
                m1,
                m2,
            )))
        }
        "cvd" => Ok(Box::new(CvdEngine::new())),
        "hires_volume_flow" => {
            let window_len = get_usize_p(params, "window_len", 20, 2, 10000)?;
            Ok(Box::new(HiResVolumeFlowEngine::new(window_len)))
        }
        "klinger" | "kvo" => {
            let fast = get_usize_p(params, "fast_len", 34, 1, 10000)?;
            let slow = get_usize_p(params, "slow_len", 55, 1, 10000)?;
            let sig = get_usize_p(params, "signal_len", 13, 1, 10000)?;
            ensure_less("fast_len", fast as f64, "slow_len", slow as f64)?;
            Ok(Box::new(KlingerVolumeForceEngine::new(fast, slow, sig)))
        }
        "zigzag" => {
            let depth = get_usize_p(params, "depth", 12, 2, 10000)?;
            let dev = get_f64_p(params, "deviation_pct", 5.0, 0.01, 100.0)?;
            Ok(Box::new(ZigZagEngine::new(depth, dev)))
        }
        "zigzag_advanced" => {
            // Fixed Percent deviation via this f64-only entry point; use `build_typed` with
            // `deviation_mode` (ParamValue::Enum) to select AtrMultiple instead.
            let depth = get_usize_p(params, "depth", 3, 1, 10000)?;
            let backstep = get_usize_p(params, "backstep", 2, 0, 10000)?;
            let deviation_pct = get_f64_p(params, "deviation_pct", 1.0, 0.001, 100.0)?;
            let atr_len = get_usize_p(params, "atr_len", 14, 1, 10000)?;
            Ok(Box::new(AdvancedZigZagEngine::new(
                depth,
                backstep,
                ZigZagDeviationMode::Percent(deviation_pct),
                atr_len,
            )))
        }
        "pivot_sets" | "multi_pivots" => Ok(Box::new(PivotSetsEngine::new(PivotSetType::Classic))),
        "tema" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(TemaEngine::new(p)))
        }
        "lsma" => {
            let p = get_usize_p(params, "period", 25, 2, 10000)?;
            Ok(Box::new(LsmaEngine::new(p)))
        }
        "mcginley" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(McGinleyDynamicEngine::new(p)))
        }
        "envelope" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            let pct = get_f64_p(params, "percent", 2.5, 0.01, 100.0)?;
            Ok(Box::new(EnvelopeEngine::new(p, pct)))
        }
        "choppiness" | "chop" => {
            let p = get_usize_p(params, "period", 14, 2, 10000)?;
            Ok(Box::new(ChoppinessIndexEngine::new(p)))
        }
        "vortex" | "vi" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(VortexEngine::new(p)))
        }
        "alligator" => Ok(Box::new(AlligatorEngine::new())),
        "connors_rsi" => {
            let rsi_len = get_usize_p(params, "rsi_len", 3, 1, 10000)?;
            let streak_len = get_usize_p(params, "streak_len", 2, 1, 10000)?;
            let rank_len = get_usize_p(params, "rank_len", 100, 1, 10000)?;
            Ok(Box::new(ConnorsRsiEngine::new(
                rsi_len, streak_len, rank_len,
            )))
        }
        "coppock" => Ok(Box::new(CoppockCurveEngine::new())),
        "dpo" => {
            let p = get_usize_p(params, "period", 21, 2, 10000)?;
            Ok(Box::new(DpoEngine::new(p)))
        }
        "kst" => Ok(Box::new(KstEngine::new())),
        "mass_index" => {
            let p = get_usize_p(params, "period", 25, 1, 10000)?;
            Ok(Box::new(MassIndexEngine::new(p)))
        }
        "rvi" => {
            let p = get_usize_p(params, "period", 10, 1, 10000)?;
            Ok(Box::new(RviEngine::new(p)))
        }
        "bop" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(BalanceOfPowerEngine::new(p)))
        }
        "eom" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            let div = get_f64_p(params, "volume_divisor", 10000.0, 1.0, 1e9)?;
            Ok(Box::new(EomEngine::new(p, div)))
        }
        "nvi" => Ok(Box::new(NviEngine::new())),
        "pvi" => Ok(Box::new(PviEngine::new())),
        "chaikin_oscillator" | "cho" => {
            let fast = get_usize_p(params, "fast_len", 3, 1, 10000)?;
            let slow = get_usize_p(params, "slow_len", 10, 1, 10000)?;
            ensure_less("fast_len", fast as f64, "slow_len", slow as f64)?;
            Ok(Box::new(ChaikinOscillatorEngine::new(fast, slow)))
        }
        "bos_choch" => {
            let pivot_len = get_usize_p(params, "pivot_len", 5, 2, 10000)?;
            Ok(Box::new(BosChochEngine::new(pivot_len)))
        }
        "liquidity_sweeps" | "sweeps" => {
            let p = get_usize_p(params, "pivot_len", 5, 2, 10000)?;
            let tol = get_f64_p(params, "tolerance_pct", 0.2, 0.01, 100.0)?;
            Ok(Box::new(LiquiditySweepEngine::new(p, tol)))
        }
        "liquidity_pools" => {
            let p = get_usize_p(params, "pivot_len", 5, 2, 10000)?;
            let tol = get_f64_p(params, "tolerance_pct", 0.2, 0.001, 100.0)?;
            Ok(Box::new(
                super::smart_money_structure::LiquidityPoolEngine::new(p, tol),
            ))
        }
        "wyckoff" => {
            let lookback = get_usize_p(params, "range_lookback", 20, 3, 10000)?;
            let atr_max = get_f64_p(params, "range_atr_max", 3.0, 0.1, 1000.0)?;
            let min_bars = get_usize_p(params, "min_range_bars", 6, 2, 10000)?;
            Ok(Box::new(super::wyckoff::WyckoffStateMachine::new(
                lookback, atr_max, min_bars,
            )))
        }
        "trend_quality" => {
            let p = get_usize_p(params, "period", 14, 2, 10000)?;
            Ok(Box::new(TrendQualityScoreEngine::new(p)))
        }
        "buy_sell_pressure" | "pressure" => {
            let p = get_usize_p(params, "period", 14, 1, 10000)?;
            Ok(Box::new(BuySellPressureEstimator::new(p)))
        }
        "volatility_regime" => {
            let p = get_usize_p(params, "period", 20, 1, 10000)?;
            let bb_mult = get_f64_p(params, "bb_mult", 2.0, 0.01, 100.0)?;
            let kc_mult = get_f64_p(params, "kc_mult", 1.5, 0.01, 100.0)?;
            Ok(Box::new(VolatilityRegimeDetector::new(p, bb_mult, kc_mult)))
        }
        "zscore" => {
            let p = get_usize_p(params, "period", 20, 2, 10000)?;
            Ok(Box::new(ZScoreEngine::new(p)))
        }
        "multi_factor" => {
            let p = get_usize_p(params, "period", 14, 2, 10000)?;
            Ok(Box::new(MultiFactorMarketScore::new(p)))
        }
        _ => Err(RegistryError::UnknownIndicator(name.to_string())),
    }
}

/// Dynamically builds an `Indicator` instance by its catalog name and parameters.
/// Delegates to `build_checked` and discards errors to return `Option`.
pub fn build(name: &str, params: &HashMap<String, f64>) -> Option<Box<dyn Indicator>> {
    build_checked(name, params).ok()
}

/// Builds an `Indicator` instance from typed parameters (see [`ParamValue`]).
///
/// Numeric-compatible values (`Float`, `Int`, `Bool`, `Timestamp`) are flattened to `f64` and
/// forwarded to [`build_checked`], reusing its full per-indicator validation. Values with no
/// scalar form (`Enum`, `Text`, `Timeframe`, `Source`) are rejected with
/// [`RegistryError::UnsupportedParameterType`], since no indicator in this registry currently
/// consumes them through the `f64` parameter map.
/// Indicators known to depend on the genuine OHLC range (true range, high/low pivots, volume-at-
/// price, market-structure detection, ...), for which [`super::source_mapped::SourceMapped`]
/// would silently collapse `high == low == open == close` to the selected source and degenerate
/// their math. `build_typed` rejects a non-`Close` `source` for these rather than silently
/// applying it. Not necessarily exhaustive over the full catalog — extend as new range-dependent
/// indicators are added.
const RANGE_DEPENDENT_INDICATORS: &[&str] = &[
    "atr",
    "true_range",
    "adx",
    "dmi",
    "chandelier_exit",
    "wyckoff",
    "volume_profile",
    "vp",
    "extended_volume_profile",
    "vp_extended",
    "persistent_volume_profile",
    "vp_persistent",
    "pivots_structure",
    "pivots",
    "pivot_sets",
    "multi_pivots",
    "zigzag",
    "zigzag_advanced",
    "liquidity_pools",
    "liquidity_sweeps",
    "sweeps",
    "liquidity_fvg",
    "fvg",
    "smc",
    "order_block",
    "ob",
    "ce",
    "bos_choch",
    "market_structure_breaks",
    "bos",
    "choch",
    "vix_fix",
    "wvf",
    "keltner",
    "donchian",
    "vortex",
    "vi",
    "choppiness",
    "chop",
    "mass_index",
    "supertrend",
    "parabolic_sar",
    "ichimoku",
    "aroon",
    "garman_klass",
    "hires_volume_flow",
    "cvd",
    "klinger",
    "kvo",
    "volatility_regime",
    "swing_structure",
];

pub fn build_typed(name: &str, params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    if name.to_lowercase() == "midas" {
        // Bypasses the generic `source` handling below; see `build_midas_typed`.
        return build_midas_typed(params);
    }

    let source = match params.get("source") {
        None => None,
        Some(ParamValue::Source(s)) => Some(*s),
        Some(other) => {
            return Err(RegistryError::UnsupportedParameterType {
                parameter: "source".to_string(),
                type_name: other.type_name().to_string(),
            });
        }
    };

    if let Some(s) = source {
        if s != crate::model::Source::Close
            && RANGE_DEPENDENT_INDICATORS.contains(&name.to_lowercase().as_str())
        {
            return Err(RegistryError::IncompatibleParameter {
                parameter: "source".to_string(),
                indicator: name.to_string(),
                reason: "range/OHLC-dependent indicator; SourceMapped would collapse its true range to zero"
                    .to_string(),
            });
        }
    }

    // `source` is a cross-cutting concern applied uniformly via `SourceMapped` below, not an
    // indicator-specific parameter, so it is stripped before delegating to the per-indicator
    // builders.
    let mut remaining = params.clone();
    remaining.remove("source");

    let built = match name.to_lowercase().as_str() {
        "anchored_vwap" | "avwap" => build_anchored_vwap_typed(&remaining)?,
        "pivot_sets" | "multi_pivots" => build_pivot_sets_typed(&remaining)?,
        "trend_relationship" => build_trend_relationship_typed(&remaining)?,
        "zigzag_advanced" => build_zigzag_advanced_typed(&remaining)?,
        _ => build_typed_by_flattening(name, &remaining)?,
    };

    Ok(match source {
        Some(s) if s != crate::model::Source::Close => {
            Box::new(super::source_mapped::SourceMapped::new(built, s))
        }
        _ => built,
    })
}

/// Default typed-build strategy for indicators whose full configuration surface is numeric:
/// flattens every value to `f64` via [`ParamValue::as_f64`] and delegates to [`build_checked`],
/// reusing its per-indicator validation. Rejects any value with no scalar form.
fn build_typed_by_flattening(
    name: &str,
    params: &TypedParams,
) -> Result<Box<dyn Indicator>, RegistryError> {
    let mut flat = HashMap::with_capacity(params.len());
    for (key, value) in params {
        match value.as_f64() {
            Some(v) => {
                flat.insert(key.clone(), v);
            }
            None => {
                return Err(RegistryError::UnsupportedParameterType {
                    parameter: key.clone(),
                    type_name: value.type_name().to_string(),
                });
            }
        }
    }
    build_checked(name, &flat)
}

/// Reads a `ParamValue::Enum` parameter, lower-cased. Returns `Ok(None)` if the key is absent, and
/// [`RegistryError::UnsupportedParameterType`] if present with a non-`Enum` type.
fn get_enum_p(params: &TypedParams, name: &str) -> Result<Option<String>, RegistryError> {
    match params.get(name) {
        None => Ok(None),
        Some(ParamValue::Enum(value)) => Ok(Some(value.to_lowercase())),
        Some(other) => Err(RegistryError::UnsupportedParameterType {
            parameter: name.to_string(),
            type_name: other.type_name().to_string(),
        }),
    }
}

/// Reads a `ParamValue::Timestamp` parameter. Returns `Ok(None)` if the key is absent, and
/// [`RegistryError::UnsupportedParameterType`] if present with a non-`Timestamp` type.
fn get_timestamp_p(params: &TypedParams, name: &str) -> Result<Option<i64>, RegistryError> {
    match params.get(name) {
        None => Ok(None),
        Some(ParamValue::Timestamp(value)) => Ok(Some(*value)),
        Some(other) => Err(RegistryError::UnsupportedParameterType {
            parameter: name.to_string(),
            type_name: other.type_name().to_string(),
        }),
    }
}

/// Extracts the numeric-compatible entries of `keys` from `params` into a fresh `f64` map, for
/// forwarding to the existing `get_f64_p`/`get_usize_p` validators.
fn extract_numeric_subset(
    params: &TypedParams,
    keys: &[&str],
) -> Result<HashMap<String, f64>, RegistryError> {
    let mut numeric = HashMap::new();
    for key in keys {
        if let Some(value) = params.get(*key) {
            match value.as_f64() {
                Some(v) => {
                    numeric.insert((*key).to_string(), v);
                }
                None => {
                    return Err(RegistryError::UnsupportedParameterType {
                        parameter: (*key).to_string(),
                        type_name: value.type_name().to_string(),
                    });
                }
            }
        }
    }
    Ok(numeric)
}

fn build_anchored_vwap_typed(params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    let anchor_kind = match get_enum_p(params, "anchor_kind")?.as_deref() {
        None | Some("session") => VwapAnchorKind::Session,
        Some("day") => VwapAnchorKind::Day,
        Some("week") => VwapAnchorKind::Week,
        Some("month") => VwapAnchorKind::Month,
        Some("external") => VwapAnchorKind::External,
        Some("manual_timestamp") => {
            let ts = get_timestamp_p(params, "anchor_timestamp")?.ok_or_else(|| {
                RegistryError::InvalidEnumValue {
                    parameter: "anchor_kind".to_string(),
                    value: "manual_timestamp".to_string(),
                    reason: "requires an accompanying 'anchor_timestamp' Timestamp parameter"
                        .to_string(),
                }
            })?;
            VwapAnchorKind::ManualTimestamp(ts)
        }
        Some(other) => {
            return Err(RegistryError::InvalidEnumValue {
                parameter: "anchor_kind".to_string(),
                value: other.to_string(),
                reason: "expected one of session|day|week|month|external|manual_timestamp"
                    .to_string(),
            });
        }
    };

    let zero_volume_policy = match get_enum_p(params, "zero_volume_policy")?.as_deref() {
        None | Some("equal_weight") => ZeroVolumePolicy::EqualWeight,
        Some("skip") => ZeroVolumePolicy::Skip,
        Some(other) => {
            return Err(RegistryError::InvalidEnumValue {
                parameter: "zero_volume_policy".to_string(),
                value: other.to_string(),
                reason: "expected one of equal_weight|skip".to_string(),
            });
        }
    };

    let numeric = extract_numeric_subset(params, &["mult1", "mult2"])?;
    let m1 = get_f64_p(&numeric, "mult1", 1.0, 0.01, 100.0)?;
    let m2 = get_f64_p(&numeric, "mult2", 2.0, 0.01, 100.0)?;

    Ok(Box::new(
        AnchoredVwapEngine::new(anchor_kind, m1, m2).with_zero_volume_policy(zero_volume_policy),
    ))
}

fn build_pivot_sets_typed(params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    let pivot_type = match get_enum_p(params, "pivot_type")?.as_deref() {
        None | Some("classic") => PivotSetType::Classic,
        Some("fibonacci") => PivotSetType::Fibonacci,
        Some("camarilla") => PivotSetType::Camarilla,
        Some("woodie") => PivotSetType::Woodie,
        Some("demark") => PivotSetType::DeMark,
        Some("cpr") => PivotSetType::Cpr,
        Some(other) => {
            return Err(RegistryError::InvalidEnumValue {
                parameter: "pivot_type".to_string(),
                value: other.to_string(),
                reason: "expected one of classic|fibonacci|camarilla|woodie|demark|cpr".to_string(),
            });
        }
    };

    Ok(Box::new(PivotSetsEngine::new(pivot_type)))
}

fn parse_smoother_kind(
    params: &TypedParams,
    parameter_name: &str,
    default: super::smoothing::SmootherKind,
) -> Result<super::smoothing::SmootherKind, RegistryError> {
    use super::smoothing::SmootherKind;

    match get_enum_p(params, parameter_name)?.as_deref() {
        None => Ok(default),
        Some("ema") => Ok(SmootherKind::Ema),
        Some("sma") => Ok(SmootherKind::Sma),
        Some("rma") => Ok(SmootherKind::Rma),
        Some("alma") => Ok(SmootherKind::Alma),
        Some("jma") => Ok(SmootherKind::Jma),
        Some(other) => Err(RegistryError::InvalidEnumValue {
            parameter: parameter_name.to_string(),
            value: other.to_string(),
            reason: "expected one of ema|sma|rma|alma|jma".to_string(),
        }),
    }
}

/// Builds a MIDAS engine directly from typed params, bypassing the generic `source`
/// strip-then-`SourceMapped`-wrap path in [`build_typed`]: MIDAS needs its own `Source` for the
/// cumulative curve while still reading genuine `bar.high`/`bar.low` for Topfinder/Bottomfinder
/// extreme tracking, which a bar flattened to a single OHLC value (what `SourceMapped` produces)
/// would break.
fn build_midas_typed(params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    let mode = match get_enum_p(params, "mode")?.as_deref() {
        None | Some("topfinder") => MidasMode::Topfinder,
        Some("bottomfinder") => MidasMode::Bottomfinder,
        Some(other) => {
            return Err(RegistryError::InvalidEnumValue {
                parameter: "mode".to_string(),
                value: other.to_string(),
                reason: "expected one of topfinder|bottomfinder".to_string(),
            });
        }
    };

    let source = match params.get("source") {
        None => crate::model::Source::Hlc3,
        Some(ParamValue::Source(s)) => *s,
        Some(other) => {
            return Err(RegistryError::UnsupportedParameterType {
                parameter: "source".to_string(),
                type_name: other.type_name().to_string(),
            });
        }
    };

    let numeric = extract_numeric_subset(params, &["maturity_bars"])?;
    let maturity_bars = get_usize_p(&numeric, "maturity_bars", 20, 1, 10000)?;

    Ok(Box::new(MidasCurveEngine::new(
        mode,
        source,
        maturity_bars as u32,
    )))
}

fn build_zigzag_advanced_typed(params: &TypedParams) -> Result<Box<dyn Indicator>, RegistryError> {
    let numeric =
        extract_numeric_subset(params, &["depth", "backstep", "deviation_value", "atr_len"])?;
    let depth = get_usize_p(&numeric, "depth", 3, 1, 10000)?;
    let backstep = get_usize_p(&numeric, "backstep", 2, 0, 10000)?;
    let atr_len = get_usize_p(&numeric, "atr_len", 14, 1, 10000)?;

    let deviation = match get_enum_p(params, "deviation_mode")?.as_deref() {
        None | Some("percent") => {
            let value = get_f64_p(&numeric, "deviation_value", 1.0, 0.001, 100.0)?;
            ZigZagDeviationMode::Percent(value)
        }
        Some("atr_multiple") => {
            let value = get_f64_p(&numeric, "deviation_value", 1.0, 0.001, 1000.0)?;
            ZigZagDeviationMode::AtrMultiple(value)
        }
        Some(other) => {
            return Err(RegistryError::InvalidEnumValue {
                parameter: "deviation_mode".to_string(),
                value: other.to_string(),
                reason: "expected one of percent|atr_multiple".to_string(),
            });
        }
    };

    Ok(Box::new(AdvancedZigZagEngine::new(
        depth, backstep, deviation, atr_len,
    )))
}

fn build_trend_relationship_typed(
    params: &TypedParams,
) -> Result<Box<dyn Indicator>, RegistryError> {
    use super::smoothing::SmootherKind;
    use super::trend_relationship::AdaptiveTrendRelationship;

    let fast_kind = parse_smoother_kind(params, "fast_kind", SmootherKind::Ema)?;
    let slow_kind = parse_smoother_kind(params, "slow_kind", SmootherKind::Ema)?;

    let numeric = extract_numeric_subset(params, &["fast_len", "slow_len"])?;
    let fast_len = get_usize_p(&numeric, "fast_len", 9, 1, 10000)?;
    let slow_len = get_usize_p(&numeric, "slow_len", 21, 1, 10000)?;
    ensure_less("fast_len", fast_len as f64, "slow_len", slow_len as f64)?;

    Ok(Box::new(AdaptiveTrendRelationship::new(
        fast_kind, fast_len, slow_kind, slow_len,
    )))
}

// Note: `indicator::swing_structure::SwingStructureEngine` and
// `indicator::relative_strength::RelativeStrengthEngine` are intentionally not registered in the
// single-bar `build(name, params)` registry. `SwingStructureEngine` requires raw ATR injected per
// bar (`update(bar, atr)`), while `RelativeStrengthEngine` requires dual-series input (`update(own_bar, bench_bar)`).
// Both are exported directly from `kestrel_chartkit::indicator` for explicit use.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_checked_valid_and_invalid_params() {
        let valid_params = HashMap::from([("period".to_string(), 14.0)]);
        assert!(build_checked("rsi", &valid_params).is_ok());

        // Period 0 is invalid
        let zero_params = HashMap::from([("rsi_len".to_string(), 0.0)]);
        let err = match build_checked("rsi", &zero_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for zero period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Negative period is invalid
        let neg_params = HashMap::from([("period".to_string(), -5.0)]);
        let err = match build_checked("sma", &neg_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for negative period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Fractional periods are rejected instead of silently truncated
        let fractional_params = HashMap::from([("period".to_string(), 14.5)]);
        assert!(matches!(
            build_checked("sma", &fractional_params),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // Values above the allocation guard are rejected
        let huge_params = HashMap::from([("period".to_string(), 10_001.0)]);
        assert!(matches!(
            build_checked("sma", &huge_params),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // NaN is invalid
        let nan_params = HashMap::from([("period".to_string(), f64::NAN)]);
        let err = match build_checked("sma", &nan_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for NaN period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Threshold and period relationships are validated
        let reversed_rsi = HashMap::from([
            ("oversold".to_string(), 80.0),
            ("overbought".to_string(), 20.0),
        ]);
        assert!(matches!(
            build_checked("rsi", &reversed_rsi),
            Err(RegistryError::InvalidParameter { .. })
        ));

        let reversed_ultimate = HashMap::from([
            ("period1".to_string(), 28.0),
            ("period2".to_string(), 14.0),
            ("period3".to_string(), 7.0),
        ]);
        assert!(matches!(
            build_checked("ultimate_oscillator", &reversed_ultimate),
            Err(RegistryError::InvalidParameter { .. })
        ));

        let oversized_bins = HashMap::from([("num_bins".to_string(), 1_001.0)]);
        assert!(matches!(
            build_checked("volume_profile", &oversized_bins),
            Err(RegistryError::InvalidParameter { .. })
        ));

        // Infinity is invalid
        let inf_params = HashMap::from([("period".to_string(), f64::INFINITY)]);
        let err = match build_checked("sma", &inf_params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for Infinity period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // MACD fast_len >= slow_len is invalid
        let macd_bad = HashMap::from([
            ("fast_len".to_string(), 30.0),
            ("slow_len".to_string(), 20.0),
        ]);
        let err = match build_checked("macd", &macd_bad) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for fast >= slow"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));

        // Unknown indicator
        let err = match build_checked("non_existent_ind", &HashMap::new()) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown indicator"),
        };
        assert_eq!(
            err,
            RegistryError::UnknownIndicator("non_existent_ind".to_string())
        );
    }

    #[test]
    fn test_build_typed_flattens_numeric_params() {
        let params: TypedParams = HashMap::from([("period".to_string(), ParamValue::Int(14))]);
        assert!(build_typed("rsi", &params).is_ok());

        let bool_params: TypedParams =
            HashMap::from([("period".to_string(), ParamValue::Bool(true))]);
        // Bool flattens to 1.0, which build_checked's own validation then accepts or rejects.
        assert!(build_typed("rsi", &bool_params).is_ok());
    }

    #[test]
    fn test_build_typed_rejects_non_numeric_params() {
        let params: TypedParams =
            HashMap::from([("period".to_string(), ParamValue::Enum("fast".to_string()))]);
        let err = match build_typed("rsi", &params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for enum parameter"),
        };
        assert!(matches!(
            err,
            RegistryError::UnsupportedParameterType { .. }
        ));
    }

    #[test]
    fn test_build_typed_propagates_indicator_validation_errors() {
        let params: TypedParams =
            HashMap::from([("period".to_string(), ParamValue::Float(f64::INFINITY))]);
        let err = match build_typed("sma", &params) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for infinite period"),
        };
        assert!(matches!(err, RegistryError::InvalidParameter { .. }));
    }

    #[test]
    fn test_build_typed_anchored_vwap_native_enum_selection() {
        // Default (no anchor_kind given) stays Session, matching build_checked's behavior.
        let defaulted = build_typed("anchored_vwap", &TypedParams::new());
        assert!(defaulted.is_ok());

        let day_params: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("day".to_string()),
        )]);
        assert!(build_typed("avwap", &day_params).is_ok());

        let skip_zero_vol: TypedParams = HashMap::from([(
            "zero_volume_policy".to_string(),
            ParamValue::Enum("skip".to_string()),
        )]);
        assert!(build_typed("anchored_vwap", &skip_zero_vol).is_ok());

        let manual_without_timestamp: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("manual_timestamp".to_string()),
        )]);
        let err = match build_typed("anchored_vwap", &manual_without_timestamp) {
            Err(e) => e,
            Ok(_) => panic!("Expected error: manual_timestamp requires anchor_timestamp"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));

        let manual_with_timestamp: TypedParams = HashMap::from([
            (
                "anchor_kind".to_string(),
                ParamValue::Enum("manual_timestamp".to_string()),
            ),
            (
                "anchor_timestamp".to_string(),
                ParamValue::Timestamp(1_700_000_000),
            ),
        ]);
        assert!(build_typed("anchored_vwap", &manual_with_timestamp).is_ok());

        let unknown_kind: TypedParams = HashMap::from([(
            "anchor_kind".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("anchored_vwap", &unknown_kind) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown anchor_kind"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_typed_pivot_sets_native_enum_selection() {
        for kind in [
            "classic",
            "fibonacci",
            "camarilla",
            "woodie",
            "demark",
            "cpr",
        ] {
            let params: TypedParams =
                HashMap::from([("pivot_type".to_string(), ParamValue::Enum(kind.to_string()))]);
            assert!(
                build_typed("pivot_sets", &params).is_ok(),
                "expected {kind} to build"
            );
        }

        let unknown: TypedParams = HashMap::from([(
            "pivot_type".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("multi_pivots", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown pivot_type"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_typed_source_propagates_to_computation() {
        use crate::model::{Bar, Source};

        let bars = [
            Bar::new(0, 10.0, 12.0, 8.0, 11.0, 100.0),
            Bar::new(60, 20.0, 22.0, 18.0, 21.0, 100.0),
        ];

        let close_params: TypedParams = HashMap::from([("period".to_string(), ParamValue::Int(2))]);
        let mut close_sma = build_typed("sma", &close_params).unwrap();

        let open_params: TypedParams = HashMap::from([
            ("period".to_string(), ParamValue::Int(2)),
            ("source".to_string(), ParamValue::Source(Source::Open)),
        ]);
        let mut open_sma = build_typed("sma", &open_params).unwrap();

        let mut close_out = None;
        let mut open_out = None;
        for bar in &bars {
            close_out = close_sma.on_bar(bar);
            open_out = open_sma.on_bar(bar);
        }

        assert_eq!(close_out.unwrap().value, (11.0 + 21.0) / 2.0);
        assert_eq!(open_out.unwrap().value, (10.0 + 20.0) / 2.0);
    }

    #[test]
    fn test_build_typed_source_close_is_a_no_op() {
        let params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Close),
        )]);
        assert!(build_typed("sma", &params).is_ok());
    }

    #[test]
    fn test_adx_with_defaults_matches_registry_default() {
        let mut via_struct = Adx::with_defaults();
        let mut via_registry = build_checked("adx", &HashMap::new()).unwrap();

        let bars = crate::model::Bar::new(0, 100.0, 101.0, 99.0, 100.5, 1000.0);
        let mut struct_out = None;
        let mut registry_out = None;
        for i in 0..60 {
            let price = 100.0 + (i as f64 * 0.3).sin() * 5.0;
            let bar =
                crate::model::Bar::new(i, price, price + 1.0, price - 1.0, price + 0.5, 1000.0);
            struct_out = via_struct.on_bar(&bar);
            registry_out = via_registry.on_bar(&bar);
        }
        let _ = bars;
        assert_eq!(
            struct_out.map(|o| o.value),
            registry_out.map(|o| o.value),
            "Adx::with_defaults() must produce identical output to the registry's \"adx\" default"
        );
    }

    #[test]
    fn test_atr_with_defaults_matches_registry_default() {
        let mut via_struct = Atr::with_defaults();
        let mut via_registry = build_checked("atr", &HashMap::new()).unwrap();

        let mut struct_out = None;
        let mut registry_out = None;
        for i in 0..40 {
            let price = 100.0 + (i as f64 * 0.3).sin() * 5.0;
            let bar =
                crate::model::Bar::new(i, price, price + 1.0, price - 1.0, price + 0.5, 1000.0);
            struct_out = via_struct.on_bar(&bar);
            registry_out = via_registry.on_bar(&bar);
        }
        assert_eq!(
            struct_out.map(|o| o.value),
            registry_out.map(|o| o.value),
            "Atr::with_defaults() must produce identical output to the registry's \"atr\" default"
        );
    }

    #[test]
    fn test_build_typed_rejects_non_close_source_on_range_dependent_indicators() {
        let params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Open),
        )]);
        for name in [
            "atr",
            "wyckoff",
            "chandelier_exit",
            "ce",
            "extended_volume_profile",
            "zigzag_advanced",
            "keltner",
            "donchian",
            "garman_klass",
        ] {
            let err = match build_typed(name, &params) {
                Err(e) => e,
                Ok(_) => panic!("expected '{name}' to reject a non-Close source"),
            };
            assert!(
                matches!(err, RegistryError::IncompatibleParameter { .. }),
                "'{name}' returned {err:?} instead of IncompatibleParameter"
            );
        }

        // Close is always a no-op regardless of range-dependence, so it must still succeed.
        let close_params: TypedParams = HashMap::from([(
            "source".to_string(),
            ParamValue::Source(crate::model::Source::Close),
        )]);
        assert!(build_typed("atr", &close_params).is_ok());
    }

    #[test]
    fn test_build_typed_trend_relationship_native_smoother_kind_selection() {
        let params: TypedParams = HashMap::from([
            ("fast_kind".to_string(), ParamValue::Enum("jma".to_string())),
            (
                "slow_kind".to_string(),
                ParamValue::Enum("alma".to_string()),
            ),
            ("fast_len".to_string(), ParamValue::Int(5)),
            ("slow_len".to_string(), ParamValue::Int(20)),
        ]);
        assert!(build_typed("trend_relationship", &params).is_ok());

        let unknown: TypedParams = HashMap::from([(
            "fast_kind".to_string(),
            ParamValue::Enum("bogus".to_string()),
        )]);
        let err = match build_typed("trend_relationship", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown fast_kind"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_checked_trend_relationship_defaults_to_ema() {
        assert!(build_checked("trend_relationship", &HashMap::new()).is_ok());
    }

    #[test]
    fn test_build_typed_midas_native_mode_and_source_selection() {
        let params: TypedParams = HashMap::from([
            (
                "mode".to_string(),
                ParamValue::Enum("bottomfinder".to_string()),
            ),
            (
                "source".to_string(),
                ParamValue::Source(crate::model::Source::Close),
            ),
        ]);
        assert!(build_typed("midas", &params).is_ok());

        let unknown: TypedParams =
            HashMap::from([("mode".to_string(), ParamValue::Enum("bogus".to_string()))]);
        let err = match build_typed("midas", &unknown) {
            Err(e) => e,
            Ok(_) => panic!("Expected error for unknown mode"),
        };
        assert!(matches!(err, RegistryError::InvalidEnumValue { .. }));
    }

    #[test]
    fn test_build_checked_midas_defaults_to_topfinder() {
        assert!(build_checked("midas", &HashMap::new()).is_ok());
    }
}
