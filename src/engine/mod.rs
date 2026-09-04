//! Composite "engine" layer (plan `kestrel-chartkit-design-plan.md`, Anhang E–G): structures
//! that combine several `indicator::*` outputs into a higher-level context/state/permission
//! reading, sitting above the per-bar `Indicator` trait and below `scoring::CompositeSignal`.
//!
//! Every type here is explicitly a **candidate**, not a stabilized API (see the plan's "Kandidat,
//! kein Commitment" notes) — field names and enum variants follow the plan's Rust sketches as
//! closely as possible so the two stay traceable to each other.

pub mod acceptance_detector;
pub mod balance_classifier;
pub mod balance_migration;
pub mod liquidity_path;
pub mod location_quality;
pub mod market_context;
pub mod market_state;
pub mod multi_series;
pub mod pipeline;
pub mod structural_stop;
pub mod vwap_regime;

pub use acceptance_detector::{
    detect_acceptance_rejection, AcceptanceDetectorOutput, RejectionKind,
};
pub use balance_classifier::{
    classify_balance_imbalance, BalanceClassifierOutput, MarketBalanceState,
};
pub use balance_migration::{build_balance_migration, BalanceMigrationOutput, MigrationDirection};
pub use liquidity_path::{build_free_space_score, FreeSpaceScore};
pub use location_quality::{calculate_location_quality, LocationQualityScore};
pub use market_context::{
    build_market_context, classify_volume_node, AcceptanceLevel, AuctionPhase, MarketContextOutput,
    VolumeNodeKind,
};
pub use market_state::{
    derive_playbook, ExpectedPlaybook, MarketStateOutput, OpeningType, PlaybookState,
    VolatilityRegime,
};
pub use multi_series::*;
pub use pipeline::*;
pub use structural_stop::{remaining_rr, StructuralTrailingStop};
pub use vwap_regime::{classify_slope, SlopeState, VwapRegimeOutput, VwapRegimeTracker};
