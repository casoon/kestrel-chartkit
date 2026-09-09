//! Stress testing, multi-asset block bootstrapping, path simulation, and execution uncertainty.
//!
//! Provides deterministic scenario shock testing for portfolios, stop-loss gap slippage modeling,
//! synchronized block bootstrapping across correlated assets, and Monte-Carlo equity path simulations.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::execution::ExecutionCosts;
use crate::portfolio::{CashLedger, PortfolioSnapshot, PositionSnapshot};

/// Parameters defining a stress scenario applied to a portfolio or execution environment.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StressScenario {
    /// Percentage shock to asset prices (e.g. `-0.10` represents a -10% market crash, `0.0` is neutral).
    pub price_shock_pct: f64,
    /// Multiplier on bid-ask spreads (e.g. `2.0` represents a doubling of spreads, `1.0` is neutral).
    pub spread_multiplier: f64,
    /// Multiplier on adverse slippage (e.g. `2.5` represents 2.5x normal slippage, `1.0` is neutral).
    pub slippage_multiplier: f64,
    /// Percentage shock to foreign exchange rates against account currency (e.g. `-0.05` for -5% FX devaluation, `0.0` is neutral).
    pub fx_shock_pct: f64,
    /// Multiplier on volume participation capacity (e.g. `0.5` represents liquidity halving, `1.0` is neutral).
    pub participation_cap_multiplier: f64,
}

impl Default for StressScenario {
    fn default() -> Self {
        Self::neutral()
    }
}

impl StressScenario {
    /// Creates a neutral scenario where no shocks or cost increases are applied.
    pub fn neutral() -> Self {
        Self {
            price_shock_pct: 0.0,
            spread_multiplier: 1.0,
            slippage_multiplier: 1.0,
            fx_shock_pct: 0.0,
            participation_cap_multiplier: 1.0,
        }
    }

    /// Validates that stress scenario parameters are finite and non-negative where required.
    pub fn validate(&self) -> Result<(), StressError> {
        if !self.price_shock_pct.is_finite() || self.price_shock_pct < -1.0 {
            return Err(StressError::InvalidInput(
                "price_shock_pct must be >= -1.0 and finite",
            ));
        }
        if !self.spread_multiplier.is_finite() || self.spread_multiplier < 0.0 {
            return Err(StressError::InvalidInput(
                "spread_multiplier must be non-negative and finite",
            ));
        }
        if !self.slippage_multiplier.is_finite() || self.slippage_multiplier < 0.0 {
            return Err(StressError::InvalidInput(
                "slippage_multiplier must be non-negative and finite",
            ));
        }
        if !self.fx_shock_pct.is_finite() || self.fx_shock_pct < -1.0 {
            return Err(StressError::InvalidInput(
                "fx_shock_pct must be >= -1.0 and finite",
            ));
        }
        if !self.participation_cap_multiplier.is_finite() || self.participation_cap_multiplier < 0.0
        {
            return Err(StressError::InvalidInput(
                "participation_cap_multiplier must be non-negative and finite",
            ));
        }
        Ok(())
    }

    /// Computes stressed execution costs by scaling baseline costs.
    pub fn stressed_costs(&self, base: ExecutionCosts) -> ExecutionCosts {
        ExecutionCosts {
            fee_pct: base.fee_pct,
            spread: base.spread * self.spread_multiplier,
            slippage_pct: base.slippage_pct * self.slippage_multiplier,
        }
    }
}

/// Evaluated outcome of applying a stress scenario to an active portfolio.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct StressedPortfolioResult {
    /// Stressed portfolio valuation snapshot.
    pub snapshot: PortfolioSnapshot,
    /// Absolute change in total equity (`stressed_equity - baseline_equity`).
    pub equity_change: f64,
    /// Percentage change in total equity relative to baseline equity.
    pub equity_change_pct: f64,
    /// Increase or decrease in total gross exposure.
    pub gross_exposure_change: f64,
    /// Stressed gross leverage.
    pub gross_leverage: f64,
}

/// Error type for stress operations and path simulations.
#[derive(Debug, Clone, PartialEq)]
pub enum StressError {
    InvalidInput(&'static str),
    Portfolio(crate::portfolio::PortfolioError),
}

impl fmt::Display for StressError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid stress input: {msg}"),
            Self::Portfolio(e) => write!(f, "portfolio evaluation failed under stress: {e}"),
        }
    }
}

impl std::error::Error for StressError {}

impl From<crate::portfolio::PortfolioError> for StressError {
    fn from(e: crate::portfolio::PortfolioError) -> Self {
        Self::Portfolio(e)
    }
}

/// Applies a stress scenario to positions and cash ledger, evaluating the stressed outcome.
///
/// Under a neutral scenario, the resulting equity and notional matches the baseline portfolio evaluation.
pub fn apply_portfolio_stress(
    account_currency: crate::contract::Currency,
    ledger: &CashLedger,
    positions: &[PositionSnapshot],
    scenario: &StressScenario,
) -> Result<StressedPortfolioResult, StressError> {
    scenario.validate()?;

    let base_snapshot =
        crate::portfolio::evaluate_portfolio(account_currency.clone(), ledger, positions)?;

    // Apply price and FX shocks to positions
    let mut stressed_positions = Vec::with_capacity(positions.len());
    for p in positions {
        let mut sp = p.clone();
        sp.current_price *= 1.0 + scenario.price_shock_pct;
        sp.fx_to_account *= 1.0 + scenario.fx_shock_pct;
        stressed_positions.push(sp);
    }

    let stressed_snapshot = crate::portfolio::evaluate_portfolio(
        base_snapshot.account_currency,
        ledger,
        &stressed_positions,
    )?;

    let equity_change = stressed_snapshot.equity - base_snapshot.equity;
    let equity_change_pct = if base_snapshot.equity.abs() > 1e-12 {
        equity_change / base_snapshot.equity
    } else {
        0.0
    };
    let gross_exposure_change = stressed_snapshot.gross_exposure - base_snapshot.gross_exposure;

    let gross_leverage = if base_snapshot.equity > 0.0 {
        stressed_snapshot.gross_exposure / base_snapshot.equity
    } else {
        0.0
    };

    Ok(StressedPortfolioResult {
        snapshot: stressed_snapshot,
        equity_change,
        equity_change_pct,
        gross_exposure_change,
        gross_leverage,
    })
}

/// Simulates gap execution when an asset opens beyond a protective stop order.
///
/// Returns the realized execution price.
/// - For long positions (protective sell stop):
///   - If `next_open < stop`, price gapped down beyond the stop level; filled at the worse `next_open`.
///   - Otherwise, filled at `stop`.
/// - For short positions (protective buy stop):
///   - If `next_open > stop`, price gapped up beyond the stop level; filled at the worse `next_open`.
///   - Otherwise, filled at `stop`.
pub fn simulate_stop_gap_execution(stop: f64, next_open: f64, is_long: bool) -> f64 {
    if is_long {
        // Long stop-loss is a sell order
        stop.min(next_open)
    } else {
        // Short stop-loss is a buy order
        stop.max(next_open)
    }
}

/// Linear Congruential Generator for fast, deterministic pseudo-random sequences.
#[derive(Debug, Clone)]
struct LcgRng {
    state: u64,
}

impl LcgRng {
    fn new(seed: u64) -> Self {
        Self {
            state: seed.wrapping_add(1),
        }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_usize(&mut self, max: usize) -> usize {
        if max == 0 {
            return 0;
        }
        (self.next_u64() as usize) % max
    }
}

/// Synchronously resamples blocks across multiple assets to preserve contemporaneous correlations.
///
/// `asset_returns`: Slice of return series, where each inner vector is the return series of one asset.
/// All series must have equal length `T`.
///
/// Returns a vector of paths, where each path contains $K$ return vectors of length `path_length`.
pub fn multi_asset_block_bootstrap(
    asset_returns: &[Vec<f64>],
    block_size: usize,
    path_length: usize,
    num_paths: usize,
    seed: u64,
) -> Result<Vec<Vec<Vec<f64>>>, StressError> {
    if asset_returns.is_empty() {
        return Err(StressError::InvalidInput("asset_returns cannot be empty"));
    }
    let t = asset_returns[0].len();
    if t == 0 {
        return Err(StressError::InvalidInput("return series cannot be empty"));
    }
    for series in asset_returns {
        if series.len() != t {
            return Err(StressError::InvalidInput(
                "all asset return series must have identical length",
            ));
        }
        for &r in series {
            if !r.is_finite() {
                return Err(StressError::InvalidInput(
                    "return series contains non-finite values",
                ));
            }
        }
    }
    if block_size == 0 {
        return Err(StressError::InvalidInput("block_size must be >= 1"));
    }
    if path_length == 0 {
        return Err(StressError::InvalidInput("path_length must be >= 1"));
    }
    if num_paths == 0 {
        return Err(StressError::InvalidInput("num_paths must be >= 1"));
    }

    let mut rng = LcgRng::new(seed);
    let num_assets = asset_returns.len();
    let mut paths = Vec::with_capacity(num_paths);

    for _ in 0..num_paths {
        let mut path_assets = vec![Vec::with_capacity(path_length); num_assets];
        let mut steps_collected = 0;

        while steps_collected < path_length {
            let start_idx = rng.next_usize(t);
            let take = block_size.min(path_length - steps_collected);

            for offset in 0..take {
                let time_idx = (start_idx + offset) % t;
                for asset_idx in 0..num_assets {
                    path_assets[asset_idx].push(asset_returns[asset_idx][time_idx]);
                }
            }
            steps_collected += take;
        }

        paths.push(path_assets);
    }

    Ok(paths)
}

/// Summary of Monte-Carlo equity path simulations.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PathSimulationSummary {
    /// Initial starting equity.
    pub initial_equity: f64,
    /// Horizon steps simulated for each path.
    pub horizon_steps: usize,
    /// Total number of simulated paths.
    pub num_simulations: usize,
    /// Deterministic RNG seed used.
    pub seed: u64,
    /// Quantiles of terminal ending equity: `(p05, p25, median, p75, p95)`.
    pub terminal_equity_quantiles: (f64, f64, f64, f64, f64),
    /// Quantiles of maximum drawdown across paths: `(p05, median, p95)`.
    pub max_drawdown_quantiles: (f64, f64, f64),
    /// Probability that maximum drawdown exceeds a given threshold during the path.
    pub empirical_mean_terminal_equity: f64,
}

impl PathSimulationSummary {
    /// Calculates the empirical fraction of paths where maximum drawdown exceeded `threshold_pct` (e.g. 0.20 for 20%).
    pub fn probability_drawdown_exceeds(all_max_drawdowns: &[f64], threshold_pct: f64) -> f64 {
        if all_max_drawdowns.is_empty() {
            return 0.0;
        }
        let exceeds = all_max_drawdowns
            .iter()
            .filter(|&&dd| dd >= threshold_pct)
            .count();
        exceeds as f64 / all_max_drawdowns.len() as f64
    }
}

/// Simulates equity curves using circular block bootstrap of historical periodic returns.
///
/// Returns statistical quantiles for terminal capital and path drawdowns under deterministic execution.
pub fn simulate_equity_paths(
    initial_equity: f64,
    periodic_returns: &[f64],
    block_size: usize,
    horizon_steps: usize,
    num_simulations: usize,
    seed: u64,
) -> Result<PathSimulationSummary, StressError> {
    if !initial_equity.is_finite() || initial_equity <= 0.0 {
        return Err(StressError::InvalidInput(
            "initial_equity must be positive and finite",
        ));
    }
    if periodic_returns.is_empty() {
        return Err(StressError::InvalidInput(
            "periodic_returns cannot be empty",
        ));
    }
    for &r in periodic_returns {
        if !r.is_finite() {
            return Err(StressError::InvalidInput("returns must be finite"));
        }
    }
    if block_size == 0 {
        return Err(StressError::InvalidInput("block_size must be >= 1"));
    }
    if horizon_steps == 0 {
        return Err(StressError::InvalidInput("horizon_steps must be >= 1"));
    }
    if num_simulations == 0 {
        return Err(StressError::InvalidInput("num_simulations must be >= 1"));
    }

    let t = periodic_returns.len();
    let mut rng = LcgRng::new(seed);

    let mut terminal_equities = Vec::with_capacity(num_simulations);
    let mut max_drawdowns = Vec::with_capacity(num_simulations);

    for _ in 0..num_simulations {
        let mut equity = initial_equity;
        let mut peak = initial_equity;
        let mut max_dd = 0.0f64;
        let mut steps = 0;

        while steps < horizon_steps {
            let start_idx = rng.next_usize(t);
            let take = block_size.min(horizon_steps - steps);

            for offset in 0..take {
                let idx = (start_idx + offset) % t;
                let r = periodic_returns[idx];
                equity *= 1.0 + r;
                if equity > peak {
                    peak = equity;
                } else if peak > 0.0 {
                    let dd = (peak - equity) / peak;
                    if dd > max_dd {
                        max_dd = dd;
                    }
                }
            }
            steps += take;
        }

        terminal_equities.push(equity);
        max_drawdowns.push(max_dd);
    }

    terminal_equities.sort_by(|a, b| a.total_cmp(b));
    max_drawdowns.sort_by(|a, b| a.total_cmp(b));

    let quantile = |slice: &[f64], q: f64| -> f64 {
        let idx = ((q * slice.len() as f64).floor() as usize).min(slice.len() - 1);
        slice[idx]
    };

    let p05 = quantile(&terminal_equities, 0.05);
    let p25 = quantile(&terminal_equities, 0.25);
    let median = quantile(&terminal_equities, 0.50);
    let p75 = quantile(&terminal_equities, 0.75);
    let p95 = quantile(&terminal_equities, 0.95);

    let dd_p05 = quantile(&max_drawdowns, 0.05);
    let dd_med = quantile(&max_drawdowns, 0.50);
    let dd_p95 = quantile(&max_drawdowns, 0.95);

    let mean_terminal = terminal_equities.iter().sum::<f64>() / num_simulations as f64;

    Ok(PathSimulationSummary {
        initial_equity,
        horizon_steps,
        num_simulations,
        seed,
        terminal_equity_quantiles: (p05, p25, median, p75, p95),
        max_drawdown_quantiles: (dd_p05, dd_med, dd_p95),
        empirical_mean_terminal_equity: mean_terminal,
    })
}
