//! Streaming technical-analysis primitives, regime classification, composite scoring, and SVG
//! chart exports.
//!
//! Most consumers should start with the root re-exports. Concrete indicator and engine modules
//! remain public for advanced composition; the crate is pre-1.0 and does not yet promise API
//! stability for those lower-level modules.

/// Version dieser Rechenbibliothek, zur Laufzeit lesbar.
///
/// Eine Auswertung ist nur dann wiederholbar, wenn festgehalten ist, womit
/// gerechnet wurde. Zwei Ergebnisse mit gleichen Parametern, aber
/// verschiedener Version sind nicht vergleichbar — und ohne diese Konstante
/// kann ein Konsument die Version nicht mitschreiben, weil Cargo sie zur
/// Laufzeit nicht herausgibt.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Provider-neutral data-feed and notification integration contracts (traits), plus
/// dependency-free reference implementations.
pub mod adapters;
/// Plausibility check between an indicator's data requirements and a series' capabilities.
pub mod applicability;
/// Shared, typed result/artifact models (pivots, zones, profiles, scenarios).
pub mod artifact;
/// IANA-timezone, DST-aware exchange calendars. Requires the `calendar` feature.
#[cfg(feature = "calendar")]
pub mod calendar;
/// Versioned state snapshots for long-running engines.
pub mod checkpoint;
/// Deterministic clustering and robust adaptive-threshold primitives.
pub mod clustering;
/// Contract specifications, currencies, and linear contract valuation models.
pub mod contract;
/// Market-context and execution-support calculations.
pub mod engine;
/// Evaluation records and aggregate trade statistics.
pub mod evaluation;
/// Event/alert enrichment: timestamps, instrument/timeframe context, stable IDs, deduplication.
pub mod event;
/// Provider-neutral order/fill simulator: orders, partial fills, pyramiding, costs, position state.
pub mod execution;
/// Financial day-count conventions, cashflow discounting, and bond valuation.
pub mod finance;
/// Generic composition graph: typed indicator dependencies, shared intermediate outputs, and
/// centralized warmup/execution ordering.
pub mod graph;
/// Streaming indicators and the validated indicator registry.
pub mod indicator;
/// Lower-timeframe (intrabar) child-bar grouping under a higher-timeframe parent bucket.
pub mod intrabar;
/// Bar lifecycle events and rollback-safe, idempotent recomputation.
pub mod lifecycle;
/// Shared OHLCV and market-domain types.
pub mod model;
/// European vanilla option pricing, analytical Greeks, and implied volatility solver.
pub mod option;
/// Reference-parity fixture harness: standardized reference-value comparison with timestamp
/// alignment, warmup handling, tolerances, MTF boundaries, and explicit missing values.
pub mod parity;
/// Provider-neutral portfolio exposure, cashflow-adjusted equity returns, and risk analytics.
pub mod portfolio;
/// Market-regime classification.
pub mod regime;
/// Advanced regime-model building blocks: Markov transitions, persistence, predictability,
/// hysteretic transitions, and adaptive cycle-length tracking.
pub mod regime_advanced;
/// Provider-neutral risk and position-sizing: account risk, leverage/notional limits,
/// scale-in/out plans, break-even/time-stop rules.
pub mod risk;
/// Batch and replay execution over a full bar history.
pub mod runner;
/// Generic composite scenario state machine: multi-stage progressions with per-stage expiry and
/// explicit invalidation.
pub mod scenario;
/// Indicator scoring and composite aggregation.
pub mod scoring;
/// Historical series sliding lookback and event helpers.
pub mod series;
/// Trading session and Opening Range Breakout (ORB) tracking.
pub mod session;
/// Composite signal data types.
pub mod signal;
/// Rolling statistical primitives and linear regression.
pub mod stats;
/// Stress testing, multi-asset block bootstrapping, path simulation, and execution uncertainty.
pub mod stress;
/// Support and resistance discovery and zone lifecycle.
pub mod structure;
/// Deterministic synthetic price series and market pattern generators.
pub mod synthetic;
/// Custom timeframe types and OHLCV bar resampling.
pub mod timeframe;
/// Bar-series transformations that derive alternative candles from observed ones.
pub mod transform;
/// Shared valuation context: valuation date, market-data stamp, curves and FX.
pub mod valuation;
/// Chart DTOs and static SVG rendering.
pub mod viz;

pub use adapters::{
    DataFeedAdapter, InMemoryDataFeed, LoggingNotificationSink, NotificationEvent,
    NotificationSeverity, NotificationSink, WebhookNotificationSink,
};
pub use applicability::{
    check_applicability, data_requirements, rule_catalog, Applicability, ApplicabilityNote,
    ApplicabilityReason, ApplicabilityTier, DataRequirements, RuleDescription,
};
pub use artifact::{
    Artifact, PivotArtifact, ProfileArtifact, ProfileBin, ScenarioArtifact, ZoneArtifact,
};
#[cfg(feature = "calendar")]
pub use calendar::{ExchangeCalendar, SessionSegment};
pub use checkpoint::{Checkpoint, CheckpointStore};
pub use clustering::{kmeans_1d, KMeansResult, RobustBand, RollingRobustThreshold};
pub use contract::{
    contract_pnl, contract_tick_value, notional_value, stop_risk_amount, ContractSpec,
    ContractSpecError, Currency, FxConversionError, FxRate, InstrumentType, ValuationError,
};
pub use event::{AlertDeduplicator, AlertEvent, EventPhase};
pub use execution::{
    submit_bracket, ExecutionCosts, Fill, FillSimulator, FillSimulatorConfig, Order, OrderKind,
    OrderSide, OrderStatus, Position,
};
pub use finance::{
    discount_factor, price_bond, year_fraction, yield_to_maturity, BondPricingResult, BondSpec,
    BusinessCalendar, BusinessDayConvention, Cashflow, Compounding, CouponSchedule, Date,
    DayCountConvention, FinanceError, FixedRateBond, ScheduleStub, Weekday,
};
pub use graph::{ComposedNode, CompositionGraph, GraphError, GraphIndicator, Leaf};
pub use indicator::registry::{
    build, build_checked, build_typed, catalog, ParamValue, RegistryError, TypedParams,
};
pub use indicator::{Indicator, IndicatorAlert, IndicatorOutput};
pub use intrabar::{IntrabarGroup, IntrabarGrouper};
pub use lifecycle::{BarLifecycle, LifecycleError, LifecycleRunner};
pub use model::{
    Bar, BarQuality, BarValidationError, ContinuityKind, InstrumentMeta, InstrumentMetaError,
    LiquidityTier, MarketRegime, PriceAdjustment, Provenance, QualifiedBar, Resolution, RiskPlan,
    SeriesCapabilities, SeriesIdentity, SessionKind, Source, SupportResistanceZone, VolumeKind,
    ZoneKind,
};
pub use option::{
    black_76, black_scholes_merton, implied_volatility, normal_cdf, normal_pdf,
    verify_put_call_parity, BlackScholesInputs, OptionError, OptionGreeks, OptionPricingResult,
    OptionStyle, OptionType,
};
pub use parity::{
    ParityFixture, ParityFixtureError, ParityFixtureRow, ParityOutcome, ParityReport,
};
pub use portfolio::{
    calculate_return_metrics, cashflow_adjusted_return, compute_drawdown, evaluate_portfolio,
    historical_var_and_es, volatility_targeting_scale, CashLedger, DrawdownStats,
    HistoricalRiskStats, PortfolioError, PortfolioSnapshot, PositionEvaluation, PositionSide,
    PositionSnapshot, ReturnMetrics,
};
pub use regime::classify_regime;
pub use regime_advanced::{
    AdaptiveCycleOutput, AdaptiveCycleTracker, HysteresisBand, HysteresisLevel,
    PredictabilityTracker, RegimeMarkovModel, RegimePersistenceOutput, RegimePersistenceTracker,
};
pub use risk::{
    position_size, position_size_contract, AccountRisk, PositionSizeResult, ScaleInStep,
    ScaleOutStep, ScalePlan, StopDecision, StopManager,
};
pub use runner::{
    run_batch, run_batch_checked, run_batch_with_applicability, BatchResult, TimestampedOutput,
};
pub use scenario::{ScenarioStateMachine, ScenarioStatus, StageConfig};
pub use scoring::{
    aggregate_subscores, aggregate_subscores_with_instrument, score_indicator, WeightPreset,
};
pub use series::{CumulativeSum, Series, SeriesEvents};
pub use session::{SessionConfig, SessionConfigError, SessionTracker};
pub use signal::{CompositeSignal, PermissionGrade, SignalDirection, SubScore};
pub use stats::{correlation, linear_regression};
pub use stress::{
    apply_portfolio_stress, multi_asset_block_bootstrap, simulate_equity_paths,
    simulate_stop_gap_execution, PathSimulationSummary, StressError, StressScenario,
    StressedPortfolioResult,
};
pub use structure::{find_sr_zones, ManagedZone, ZoneRegistry, ZoneState};
pub use synthetic::{
    bos_choch_swing_bars, random_walk_bars, trending_bars, wyckoff_schematic_bars, SimpleRng,
    SwingDirection, WyckoffGeneratorConfig,
};
pub use timeframe::{BarResampler, ConfirmedResampler, Timeframe, TimeframeError};
pub use valuation::{
    BondCurveValuation, DiscountCurve, ForwardCurve, ValuationContext, ValuationContextError,
    ValuationStamp, Valued, YieldCurve,
};

/// Cross-instrument return analysis over aligned close samples.
pub mod cross_asset;
pub use cross_asset::{
    compute_market_breadth, compute_pair_spread, compute_rolling_beta,
    compute_signal_correlation_matrix, correlation_matrix,
    relative_strength as relative_strength_ranking, CloseSample, MarketBreadthSnapshot,
    PairSpreadResult, RollingBetaResult, SignalCorrelationCell, UniverseMemberObservation,
};
pub use scoring::agreement::{
    aggregate_agreement, Agreement, AgreementStrategy, DirectionalStatement,
};

pub use evaluation::price::{
    ForwardPriceOutcome, PriceDirection, PriceObservation, PriceOutcomeSample, PriceOutcomeStats,
    PriceStats,
};
pub use evaluation::probability::{
    block_bootstrap_brier, compute_calibration_metrics, CalibratedProbability, CalibrationMetrics,
    IsotonicCalibrator, ValidationExperimentManifest,
};
pub use evaluation::split::{
    split_trades_purged, PurgedSplitConfig, PurgedTrainTestSplit, SplitError, TradeSpan,
};

/// Pure on-demand market-state and price-level snapshots.
pub mod analytics;
