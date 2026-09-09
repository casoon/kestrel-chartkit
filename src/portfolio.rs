//! Provider-neutral portfolio exposure, cashflow-adjusted equity returns, and risk analytics.
//!
//! Evaluates open positions, cash balances, and margin in an explicit account currency.
//! Computes gross/net exposures, aggregated stop risk, drawdowns, cashflow-adjusted returns,
//! Sharpe/Sortino ratios, and non-parametric historical Value at Risk (VaR) / Expected Shortfall.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::contract::{ContractSpec, Currency, ValuationError};

/// Direction of an open position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum PositionSide {
    Long,
    Short,
}

/// A snapshot of an individual open position.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PositionSnapshot {
    pub symbol: String,
    pub spec: ContractSpec,
    pub side: PositionSide,
    /// Absolute quantity (must be positive).
    pub quantity: f64,
    pub entry_price: f64,
    pub current_price: f64,
    /// Optional stop-loss price. Stops are modeled as price levels, not guaranteed execution prices.
    pub stop_price: Option<f64>,
    /// Conversion rate from `spec.price_currency` to account currency.
    pub fx_to_account: f64,
}

/// Evaluated metrics for an individual position in account currency.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PositionEvaluation {
    pub symbol: String,
    pub side: PositionSide,
    pub quantity: f64,
    pub notional: f64,
    pub unrealized_pnl: f64,
    /// Stop risk in account currency, or `None` if no stop is configured.
    pub stop_risk: Option<f64>,
}

/// Account-level cash and ledger state.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CashLedger {
    /// Available cash balance in account currency.
    pub cash: f64,
    /// Cumulative deposits into the account.
    pub cumulative_deposits: f64,
    /// Cumulative withdrawals from the account.
    pub cumulative_withdrawals: f64,
    /// Cumulative fees and commissions paid.
    pub cumulative_fees: f64,
    /// Cumulative realized P&L from closed trades.
    pub cumulative_realized_pnl: f64,
}

impl Default for CashLedger {
    fn default() -> Self {
        Self {
            cash: 0.0,
            cumulative_deposits: 0.0,
            cumulative_withdrawals: 0.0,
            cumulative_fees: 0.0,
            cumulative_realized_pnl: 0.0,
        }
    }
}

/// Comprehensive portfolio valuation snapshot in account currency.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct PortfolioSnapshot {
    pub account_currency: Currency,
    /// Total equity = cash + unrealized P&L.
    pub equity: f64,
    pub cash: f64,
    pub unrealized_pnl: f64,
    pub cumulative_realized_pnl: f64,
    pub cumulative_fees: f64,
    /// Net external capital invested = cumulative deposits - cumulative withdrawals.
    pub net_deposits: f64,
    /// Sum of all long position notionals in account currency.
    pub long_notional: f64,
    /// Sum of all short position notionals in account currency.
    pub short_notional: f64,
    /// Gross exposure = long notional + short notional.
    pub gross_exposure: f64,
    /// Net exposure = long notional - short notional.
    pub net_exposure: f64,
    /// Gross leverage = gross exposure / equity (0.0 if equity <= 0).
    pub gross_leverage: f64,
    /// Net leverage = net exposure / equity (0.0 if equity <= 0).
    pub net_leverage: f64,
    /// Aggregated stop risk across all positions with configured stops.
    /// Note: Stops do not guarantee loss limits during gap openings or illiquid periods.
    pub total_stop_risk: f64,
    /// Concentration of the largest position: `largest_position_notional / gross_exposure`.
    pub max_position_concentration: f64,
    /// Evaluated individual positions.
    pub positions: Vec<PositionEvaluation>,
}

/// Errors when evaluating a portfolio.
#[derive(Debug, Clone, PartialEq)]
pub enum PortfolioError {
    Valuation(ValuationError),
    InvalidInput(&'static str),
}

impl fmt::Display for PortfolioError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Valuation(e) => write!(f, "valuation error: {e}"),
            Self::InvalidInput(msg) => write!(f, "invalid input: {msg}"),
        }
    }
}

impl std::error::Error for PortfolioError {}

impl From<ValuationError> for PortfolioError {
    fn from(e: ValuationError) -> Self {
        Self::Valuation(e)
    }
}

/// Evaluates a portfolio snapshot from open positions and cash ledger.
pub fn evaluate_portfolio(
    account_currency: Currency,
    ledger: &CashLedger,
    positions: &[PositionSnapshot],
) -> Result<PortfolioSnapshot, PortfolioError> {
    if !ledger.cash.is_finite() {
        return Err(PortfolioError::InvalidInput("cash must be finite"));
    }

    let mut long_notional = 0.0f64;
    let mut short_notional = 0.0f64;
    let mut total_unrealized_pnl = 0.0f64;
    let mut total_stop_risk = 0.0f64;
    let mut largest_notional = 0.0f64;
    let mut evaluated_positions = Vec::with_capacity(positions.len());

    for pos in positions {
        if !pos.quantity.is_finite() || pos.quantity <= 0.0 {
            return Err(PortfolioError::InvalidInput(
                "position quantity must be positive and finite",
            ));
        }
        if !pos.entry_price.is_finite() || pos.entry_price <= 0.0 {
            return Err(PortfolioError::InvalidInput(
                "entry price must be positive and finite",
            ));
        }
        if !pos.current_price.is_finite() || pos.current_price <= 0.0 {
            return Err(PortfolioError::InvalidInput(
                "current price must be positive and finite",
            ));
        }
        if !pos.fx_to_account.is_finite() || pos.fx_to_account <= 0.0 {
            return Err(PortfolioError::InvalidInput(
                "fx_to_account must be positive and finite",
            ));
        }
        pos.spec
            .validate()
            .map_err(|e| PortfolioError::Valuation(ValuationError::InvalidContract(e)))?;

        let notional = pos.quantity * pos.current_price * pos.spec.multiplier * pos.fx_to_account;
        largest_notional = largest_notional.max(notional);

        let pnl_diff = match pos.side {
            PositionSide::Long => pos.current_price - pos.entry_price,
            PositionSide::Short => pos.entry_price - pos.current_price,
        };
        let unrealized_pnl = pnl_diff * pos.quantity * pos.spec.multiplier * pos.fx_to_account;
        total_unrealized_pnl += unrealized_pnl;

        let stop_risk = if let Some(stop) = pos.stop_price {
            if !stop.is_finite() || stop <= 0.0 {
                return Err(PortfolioError::InvalidInput(
                    "stop price must be positive and finite",
                ));
            }
            let risk_diff = (pos.entry_price - stop).abs();
            let risk_amount = risk_diff * pos.quantity * pos.spec.multiplier * pos.fx_to_account;
            total_stop_risk += risk_amount;
            Some(risk_amount)
        } else {
            None
        };

        match pos.side {
            PositionSide::Long => long_notional += notional,
            PositionSide::Short => short_notional += notional,
        }

        evaluated_positions.push(PositionEvaluation {
            symbol: pos.symbol.clone(),
            side: pos.side,
            quantity: pos.quantity,
            notional,
            unrealized_pnl,
            stop_risk,
        });
    }

    let equity = ledger.cash + total_unrealized_pnl;
    let gross_exposure = long_notional + short_notional;
    let net_exposure = long_notional - short_notional;

    let gross_leverage = if equity > 0.0 {
        gross_exposure / equity
    } else {
        0.0
    };
    let net_leverage = if equity > 0.0 {
        net_exposure / equity
    } else {
        0.0
    };
    let max_position_concentration = if gross_exposure > 0.0 {
        largest_notional / gross_exposure
    } else {
        0.0
    };
    let net_deposits = ledger.cumulative_deposits - ledger.cumulative_withdrawals;

    Ok(PortfolioSnapshot {
        account_currency,
        equity,
        cash: ledger.cash,
        unrealized_pnl: total_unrealized_pnl,
        cumulative_realized_pnl: ledger.cumulative_realized_pnl,
        cumulative_fees: ledger.cumulative_fees,
        net_deposits,
        long_notional,
        short_notional,
        gross_exposure,
        net_exposure,
        gross_leverage,
        net_leverage,
        total_stop_risk,
        max_position_concentration,
        positions: evaluated_positions,
    })
}

/// Drawdown statistics across an equity curve.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct DrawdownStats {
    pub peak_equity: f64,
    pub current_drawdown: f64,
    pub current_drawdown_pct: f64,
    pub max_drawdown: f64,
    pub max_drawdown_pct: f64,
    pub max_drawdown_duration_bars: usize,
}

/// Computes peak equity, current and maximum drawdown (amount and percentage), and duration.
pub fn compute_drawdown(equity_series: &[f64]) -> DrawdownStats {
    if equity_series.is_empty() {
        return DrawdownStats {
            peak_equity: 0.0,
            current_drawdown: 0.0,
            current_drawdown_pct: 0.0,
            max_drawdown: 0.0,
            max_drawdown_pct: 0.0,
            max_drawdown_duration_bars: 0,
        };
    }

    let mut peak: f64 = equity_series[0];
    let mut max_dd: f64 = 0.0;
    let mut max_dd_pct: f64 = 0.0;
    let mut current_duration = 0;
    let mut max_duration = 0;

    for &eq in equity_series {
        if eq >= peak {
            peak = eq;
            current_duration = 0;
        } else {
            current_duration += 1;
            max_duration = max_duration.max(current_duration);
            let dd = peak - eq;
            let dd_pct = if peak > 0.0 { dd / peak } else { 0.0 };
            max_dd = max_dd.max(dd);
            max_dd_pct = max_dd_pct.max(dd_pct);
        }
    }

    let last_eq = *equity_series.last().unwrap();
    let current_dd = (peak - last_eq).max(0.0);
    let current_dd_pct = if peak > 0.0 { current_dd / peak } else { 0.0 };

    DrawdownStats {
        peak_equity: peak,
        current_drawdown: current_dd,
        current_drawdown_pct: current_dd_pct,
        max_drawdown: max_dd,
        max_drawdown_pct: max_dd_pct,
        max_drawdown_duration_bars: max_duration,
    }
}

/// Aggregated return and risk metrics across a series of periodic returns.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ReturnMetrics {
    /// Arithmetic mean periodic return.
    pub mean_return: f64,
    /// Annualized return: `mean_return * periods_per_year`.
    pub annualized_return: f64,
    /// Annualized volatility (sample standard deviation): `std_dev * sqrt(periods_per_year)`.
    pub annualized_volatility: f64,
    /// Annualized Sharpe ratio: `(mean_return - rf_per_period) / std_dev * sqrt(periods_per_year)`.
    pub sharpe_ratio: f64,
    /// Annualized Sortino ratio: `(mean_return - rf_per_period) / downside_deviation * sqrt(periods_per_year)`.
    pub sortino_ratio: f64,
    /// Number of return periods evaluated.
    pub sample_count: usize,
}

/// Computes annualized return, volatility, Sharpe ratio, and Sortino ratio.
///
/// - `returns`: slice of fractional periodic returns (e.g. `0.01` for +1%).
/// - `annual_risk_free_rate`: annualized risk-free rate (e.g. `0.03` for 3%).
/// - `periods_per_year`: number of periods per calendar year (e.g. 252 for daily, 12 for monthly).
pub fn calculate_return_metrics(
    returns: &[f64],
    annual_risk_free_rate: f64,
    periods_per_year: f64,
) -> Option<ReturnMetrics> {
    if returns.len() < 2 || periods_per_year <= 0.0 {
        return None;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;
    let rf_per_period = annual_risk_free_rate / periods_per_year;

    // Sample variance & standard deviation
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let std_dev = variance.sqrt();

    // Downside deviation relative to rf_per_period
    let downside_variance = returns
        .iter()
        .map(|r| {
            let under = (r - rf_per_period).min(0.0);
            under * under
        })
        .sum::<f64>()
        / n;
    let downside_dev = downside_variance.sqrt();

    let ann_factor = periods_per_year.sqrt();
    let annualized_return = mean * periods_per_year;
    let annualized_volatility = std_dev * ann_factor;

    let sharpe_ratio = if std_dev > 1e-12 {
        ((mean - rf_per_period) / std_dev) * ann_factor
    } else {
        0.0
    };

    let sortino_ratio = if downside_dev > 1e-12 {
        ((mean - rf_per_period) / downside_dev) * ann_factor
    } else {
        0.0
    };

    Some(ReturnMetrics {
        mean_return: mean,
        annualized_return,
        annualized_volatility,
        sharpe_ratio,
        sortino_ratio,
        sample_count: returns.len(),
    })
}

/// Cashflow-adjusted period return (Modified Dietz convention).
///
/// - `start_equity`: equity at beginning of period.
/// - `end_equity`: equity at end of period.
/// - `net_cashflow`: external deposits minus withdrawals occurring during the period.
/// - `cashflow_weight`: time-weight fraction (0.0 = end of period, 0.5 = midpoint, 1.0 = start).
///
/// Ensures external deposits or withdrawals create exactly 0% return in the absence of market moves.
pub fn cashflow_adjusted_return(
    start_equity: f64,
    end_equity: f64,
    net_cashflow: f64,
    cashflow_weight: f64,
) -> Option<f64> {
    let pnl = end_equity - start_equity - net_cashflow;
    let weighted_capital = start_equity + cashflow_weight * net_cashflow;
    if weighted_capital <= 0.0 || !pnl.is_finite() {
        None
    } else {
        Some(pnl / weighted_capital)
    }
}

/// Historical non-parametric risk metrics: Value at Risk (VaR) and Expected Shortfall (CVaR).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct HistoricalRiskStats {
    /// Confidence level (e.g. 0.95 for 95%).
    pub confidence_level: f64,
    /// Value at Risk as a positive fractional loss (e.g. 0.03 for 3% potential loss).
    pub var: f64,
    /// Expected Shortfall (Conditional VaR): mean loss in the worst `(1 - confidence_level)` quantile.
    pub expected_shortfall: f64,
    /// Number of observations in the historical return sample.
    pub sample_count: usize,
}

/// Computes non-parametric historical Value at Risk and Expected Shortfall from a return series.
///
/// Returns positive fractions representing potential loss (e.g. 0.05 = 5% loss).
pub fn historical_var_and_es(
    returns: &[f64],
    confidence_level: f64,
) -> Option<HistoricalRiskStats> {
    if returns.is_empty() || confidence_level <= 0.0 || confidence_level >= 1.0 {
        return None;
    }

    let mut sorted = returns.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let n = sorted.len();
    let p = 1.0 - confidence_level;
    // Tail cutoff index (at least 1 item in tail)
    let tail_count = ((p * n as f64).ceil() as usize).clamp(1, n);

    let tail_slice = &sorted[..tail_count];
    // VaR is the threshold loss at the boundary of the tail
    let boundary_return = tail_slice.last().copied().unwrap_or(0.0);
    let var = (-boundary_return).max(0.0);

    // Expected shortfall is the average loss of the tail observations
    let sum_tail_losses: f64 = tail_slice.iter().map(|&r| (-r).max(0.0)).sum();
    let expected_shortfall = sum_tail_losses / tail_count as f64;

    Some(HistoricalRiskStats {
        confidence_level,
        var,
        expected_shortfall,
        sample_count: n,
    })
}

/// Pure calculation of a suggested exposure scaling factor for volatility targeting.
///
/// `scale = min(target_vol / current_vol, max_leverage)`
pub fn volatility_targeting_scale(current_vol: f64, target_vol: f64, max_leverage: f64) -> f64 {
    if !current_vol.is_finite()
        || current_vol <= 0.0
        || !target_vol.is_finite()
        || target_vol <= 0.0
    {
        return 1.0;
    }
    let raw_scale = target_vol / current_vol;
    let cap = if max_leverage.is_finite() && max_leverage > 0.0 {
        max_leverage
    } else {
        1.0
    };
    raw_scale.min(cap).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contract::InstrumentType;

    #[test]
    fn test_opposing_positions_net_zero_gross_positive() {
        let ledger = CashLedger {
            cash: 100_000.0,
            ..Default::default()
        };
        let spec = ContractSpec {
            multiplier: 25.0,
            instrument_type: InstrumentType::LinearFuture,
            ..Default::default()
        };

        // Long 1 contract at 20,000 pts (notional 500k)
        let long_pos = PositionSnapshot {
            symbol: "FDAX".to_string(),
            spec: spec.clone(),
            side: PositionSide::Long,
            quantity: 1.0,
            entry_price: 20_000.0,
            current_price: 20_000.0,
            stop_price: Some(19_980.0),
            fx_to_account: 1.0,
        };

        // Short 1 contract at 20,000 pts (notional 500k)
        let short_pos = PositionSnapshot {
            symbol: "FDAX".to_string(),
            spec,
            side: PositionSide::Short,
            quantity: 1.0,
            entry_price: 20_000.0,
            current_price: 20_000.0,
            stop_price: Some(20_020.0),
            fx_to_account: 1.0,
        };

        let snapshot =
            evaluate_portfolio(Currency::eur(), &ledger, &[long_pos, short_pos]).unwrap();

        assert_eq!(snapshot.long_notional, 500_000.0);
        assert_eq!(snapshot.short_notional, 500_000.0);
        assert_eq!(snapshot.gross_exposure, 1_000_000.0);
        assert_eq!(snapshot.net_exposure, 0.0);
        assert_eq!(snapshot.gross_leverage, 10.0);
        assert_eq!(snapshot.net_leverage, 0.0);
        assert_eq!(snapshot.equity, 100_000.0);
        // Total stop risk: 20 * 25 * 1.0 + 20 * 25 * 1.0 = 500 + 500 = 1000 EUR
        assert_eq!(snapshot.total_stop_risk, 1000.0);
    }

    #[test]
    fn test_cashflow_neutrality() {
        // Deposit of 50k on 100k starting capital with 0 market P&L yields 0.0% return
        let r = cashflow_adjusted_return(100_000.0, 150_000.0, 50_000.0, 1.0).unwrap();
        assert_eq!(r, 0.0);
    }
}
