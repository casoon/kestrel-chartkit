//! Streaming technical-analysis primitives, regime classification, composite scoring, and SVG
//! chart exports.
//!
//! Most consumers should start with the root re-exports. Concrete indicator and engine modules
//! remain public for advanced composition; the crate is pre-1.0 and does not yet promise API
//! stability for those lower-level modules.

/// Provider-neutral data-feed and notification integration contracts (traits), plus
/// dependency-free reference implementations.
pub mod adapters;
/// Shared, typed result/artifact models (pivots, zones, profiles, scenarios).
pub mod artifact;
/// IANA-timezone, DST-aware exchange calendars. Requires the `calendar` feature.
#[cfg(feature = "calendar")]
pub mod calendar;
/// Versioned state snapshots for long-running engines.
pub mod checkpoint;
/// Deterministic clustering and robust adaptive-threshold primitives.
pub mod clustering;
/// Market-context and execution-support calculations.
pub mod engine;
/// Evaluation records and aggregate trade statistics.
pub mod evaluation;
/// Event/alert enrichment: timestamps, instrument/timeframe context, stable IDs, deduplication.
pub mod event;
/// Provider-neutral order/fill simulator: orders, partial fills, pyramiding, costs, position state.
pub mod execution;
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
/// Pine-parity fixture harness: standardized reference-value comparison with timestamp
/// alignment, warmup handling, tolerances, MTF boundaries, and explicit missing values.
pub mod parity;
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
/// Support and resistance discovery and zone lifecycle.
pub mod structure;
/// Deterministic synthetic price series and market pattern generators.
pub mod synthetic;
/// Custom timeframe types and OHLCV bar resampling.
pub mod timeframe;
/// Chart DTOs and static SVG rendering.
pub mod viz;

pub use adapters::{
    DataFeedAdapter, InMemoryDataFeed, LoggingNotificationSink, NotificationEvent,
    NotificationSeverity, NotificationSink, WebhookNotificationSink,
};
pub use artifact::{
    Artifact, PivotArtifact, ProfileArtifact, ProfileBin, ScenarioArtifact, ZoneArtifact,
};
#[cfg(feature = "calendar")]
pub use calendar::{ExchangeCalendar, SessionSegment};
pub use checkpoint::{Checkpoint, CheckpointStore};
pub use clustering::{kmeans_1d, KMeansResult, RobustBand, RollingRobustThreshold};
pub use event::{AlertDeduplicator, AlertEvent, EventPhase};
pub use execution::{
    submit_bracket, ExecutionCosts, Fill, FillSimulator, FillSimulatorConfig, Order, OrderKind,
    OrderSide, OrderStatus, Position,
};
pub use graph::{ComposedNode, CompositionGraph, GraphError, GraphIndicator, Leaf};
pub use indicator::registry::{
    build, build_checked, build_typed, catalog, ParamValue, RegistryError, TypedParams,
};
pub use indicator::{Indicator, IndicatorAlert, IndicatorOutput};
pub use intrabar::{IntrabarGroup, IntrabarGrouper};
pub use lifecycle::{BarLifecycle, LifecycleRunner};
pub use model::{
    Bar, BarQuality, BarValidationError, InstrumentMeta, InstrumentMetaError, MarketRegime,
    QualifiedBar, Resolution, RiskPlan, Source, SupportResistanceZone, ZoneKind,
};
pub use parity::{
    ParityFixture, ParityFixtureError, ParityFixtureRow, ParityOutcome, ParityReport,
};
pub use regime::classify_regime;
pub use regime_advanced::{
    AdaptiveCycleOutput, AdaptiveCycleTracker, HysteresisBand, HysteresisLevel,
    PredictabilityTracker, RegimeMarkovModel, RegimePersistenceOutput, RegimePersistenceTracker,
};
pub use risk::{
    position_size, AccountRisk, PositionSizeResult, ScaleInStep, ScaleOutStep, ScalePlan,
    StopDecision, StopManager,
};
pub use runner::{run_batch, run_batch_checked, TimestampedOutput};
pub use scenario::{ScenarioStateMachine, ScenarioStatus, StageConfig};
pub use scoring::{
    aggregate_subscores, aggregate_subscores_with_instrument, score_indicator, WeightPreset,
};
pub use series::{CumulativeSum, Series, SeriesEvents};
pub use session::{SessionConfig, SessionConfigError, SessionTracker};
pub use signal::{CompositeSignal, PermissionGrade, SignalDirection, SubScore};
pub use stats::{correlation, linear_regression};
pub use structure::{find_sr_zones, ManagedZone, ZoneRegistry, ZoneState};
pub use synthetic::{
    bos_choch_swing_bars, random_walk_bars, trending_bars, wyckoff_schematic_bars, SimpleRng,
    SwingDirection, WyckoffGeneratorConfig,
};
pub use timeframe::{BarResampler, ConfirmedResampler, Timeframe, TimeframeError};
