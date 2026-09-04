//! Provider-neutral risk and position-sizing: account-risk-based sizing on top of
//! [`InstrumentMeta`] (tick size, not a broker-specific contract spec), leverage/position limits,
//! scale-in/out plans, and break-even/time-stop rules.

use crate::model::InstrumentMeta;

/// Account-level risk configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AccountRisk {
    pub equity: f64,
    /// Fraction of `equity` risked per trade (e.g. `0.01` = 1%).
    pub risk_pct_per_trade: f64,
    pub max_leverage: f64,
    /// Absolute cap on one position's notional value, regardless of leverage headroom.
    pub max_position_notional: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PositionSizeResult {
    /// Position size in instrument units (e.g. shares/contracts), already tick-rounded.
    pub size: f64,
    /// The dollar risk the sized position represents (may be less than the requested risk amount
    /// if a leverage/notional cap bound the size first).
    pub risk_amount: f64,
    pub capped_by_leverage: bool,
    pub capped_by_notional: bool,
}

/// Sizes a position from account risk, entry/stop prices, and `tick_value` (P&L per tick per unit
/// — e.g. dollars per point per share, or per-contract tick value for futures), rounding the
/// result to `instrument`'s tick size via [`InstrumentMeta::round_to_tick`] is *not* applicable
/// here (that rounds prices, not size) — sizing itself is left unrounded; round to a lot size
/// externally if the instrument requires it.
pub fn position_size(
    account: &AccountRisk,
    instrument: &InstrumentMeta,
    entry: f64,
    stop: f64,
    tick_value: f64,
) -> PositionSizeResult {
    let risk_budget = account.equity * account.risk_pct_per_trade;
    let price_risk = (entry - stop).abs();
    if price_risk <= 0.0 || instrument.tick_size <= 0.0 || tick_value <= 0.0 {
        return PositionSizeResult {
            size: 0.0,
            risk_amount: 0.0,
            capped_by_leverage: false,
            capped_by_notional: false,
        };
    }

    let ticks_at_risk = price_risk / instrument.tick_size;
    let risk_per_unit = ticks_at_risk * tick_value;
    let mut size = risk_budget / risk_per_unit;

    let leverage_cap = (account.equity * account.max_leverage) / entry.max(1e-9);
    let mut capped_by_leverage = false;
    if size > leverage_cap {
        size = leverage_cap;
        capped_by_leverage = true;
    }

    let mut capped_by_notional = false;
    if let Some(max_notional) = account.max_position_notional {
        let notional_cap = max_notional / entry.max(1e-9);
        if size > notional_cap {
            size = notional_cap;
            capped_by_notional = true;
        }
    }

    size = size.max(0.0);
    let risk_amount = size * risk_per_unit;

    PositionSizeResult {
        size,
        risk_amount,
        capped_by_leverage,
        capped_by_notional,
    }
}

/// One scale-in step: `fraction` of the total planned size to add once price reaches
/// `trigger_price`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleInStep {
    pub trigger_price: f64,
    pub fraction: f64,
}

/// One scale-out step: `fraction` of the current position to exit once the trade reaches
/// `trigger_r_multiple` (realized-favorable move, in multiples of the original risk).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ScaleOutStep {
    pub trigger_r_multiple: f64,
    pub fraction: f64,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ScalePlan {
    pub entries: Vec<ScaleInStep>,
    pub exits: Vec<ScaleOutStep>,
}

impl ScalePlan {
    /// Scale-in steps whose `trigger_price` has been reached by `current_price`, in the
    /// direction implied by comparing `current_price` to `entry_price` (`is_long` disambiguates
    /// which side "reached" means).
    pub fn triggered_entries(&self, current_price: f64, is_long: bool) -> Vec<&ScaleInStep> {
        self.entries
            .iter()
            .filter(|step| {
                if is_long {
                    current_price >= step.trigger_price
                } else {
                    current_price <= step.trigger_price
                }
            })
            .collect()
    }

    /// Scale-out steps whose `trigger_r_multiple` has been reached by `current_r_multiple`.
    pub fn triggered_exits(&self, current_r_multiple: f64) -> Vec<&ScaleOutStep> {
        self.exits
            .iter()
            .filter(|step| current_r_multiple >= step.trigger_r_multiple)
            .collect()
    }
}

/// Break-even and time-stop rule configuration.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct StopManager {
    /// Move the stop to (at/near) entry once price reaches this R-multiple in favor.
    pub breakeven_trigger_r: Option<f64>,
    /// Force an exit after this many bars if the trade has not otherwise resolved.
    pub time_stop_bars: Option<u32>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StopDecision {
    Hold,
    MoveToBreakeven(f64),
    TimeStopExit,
}

impl StopManager {
    /// Evaluates the current bar against the configured rules. `risk_per_unit` is the original
    /// `|entry - stop|` distance (used as the R-multiple denominator); `bars_held` is elapsed
    /// bars since entry.
    pub fn evaluate(
        &self,
        entry: f64,
        current_price: f64,
        risk_per_unit: f64,
        bars_held: u32,
        is_long: bool,
    ) -> StopDecision {
        if let Some(max_bars) = self.time_stop_bars {
            if bars_held >= max_bars {
                return StopDecision::TimeStopExit;
            }
        }

        if let (Some(trigger_r), true) = (self.breakeven_trigger_r, risk_per_unit > 0.0) {
            let favorable = if is_long {
                current_price - entry
            } else {
                entry - current_price
            };
            let current_r = favorable / risk_per_unit;
            if current_r >= trigger_r {
                return StopDecision::MoveToBreakeven(entry);
            }
        }

        StopDecision::Hold
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn account() -> AccountRisk {
        AccountRisk {
            equity: 100_000.0,
            risk_pct_per_trade: 0.01, // $1,000 risk budget
            max_leverage: 100.0,
            max_position_notional: None,
        }
    }

    fn instrument() -> InstrumentMeta {
        InstrumentMeta {
            symbol: "TEST".to_string(),
            tick_size: 0.25,
            price_precision: 2,
            timezone: "UTC".to_string(),
        }
    }

    #[test]
    fn test_position_size_from_account_risk() {
        // entry=100, stop=98 -> 2.0 price risk = 8 ticks (0.25 each). tick_value=$10/tick.
        // risk_per_unit = 8 * 10 = $80. size = 1000 / 80 = 12.5.
        let result = position_size(&account(), &instrument(), 100.0, 98.0, 10.0);
        assert!((result.size - 12.5).abs() < 1e-9);
        assert!((result.risk_amount - 1000.0).abs() < 1e-6);
        assert!(!result.capped_by_leverage);
    }

    #[test]
    fn test_position_size_capped_by_leverage() {
        let tight_account = AccountRisk {
            max_leverage: 0.001,
            ..account()
        };
        let result = position_size(&tight_account, &instrument(), 100.0, 98.0, 10.0);
        assert!(result.capped_by_leverage);
        assert!(result.size < 12.5);
    }

    #[test]
    fn test_position_size_capped_by_notional() {
        let capped_account = AccountRisk {
            max_position_notional: Some(500.0),
            ..account()
        };
        let result = position_size(&capped_account, &instrument(), 100.0, 98.0, 10.0);
        assert!(result.capped_by_notional);
        assert!((result.size - 5.0).abs() < 1e-9); // 500 notional / 100 entry
    }

    #[test]
    fn test_position_size_degenerate_inputs_return_zero() {
        let result = position_size(&account(), &instrument(), 100.0, 100.0, 10.0); // zero risk distance
        assert_eq!(result.size, 0.0);
    }

    #[test]
    fn test_scale_plan_triggers() {
        let plan = ScalePlan {
            entries: vec![
                ScaleInStep {
                    trigger_price: 101.0,
                    fraction: 0.5,
                },
                ScaleInStep {
                    trigger_price: 103.0,
                    fraction: 0.5,
                },
            ],
            exits: vec![
                ScaleOutStep {
                    trigger_r_multiple: 1.0,
                    fraction: 0.5,
                },
                ScaleOutStep {
                    trigger_r_multiple: 2.0,
                    fraction: 0.5,
                },
            ],
        };

        let triggered = plan.triggered_entries(102.0, true);
        assert_eq!(triggered.len(), 1);
        assert_eq!(triggered[0].trigger_price, 101.0);

        let triggered_exits = plan.triggered_exits(1.5);
        assert_eq!(triggered_exits.len(), 1);
    }

    #[test]
    fn test_stop_manager_breakeven_trigger() {
        let manager = StopManager {
            breakeven_trigger_r: Some(1.0),
            time_stop_bars: None,
        };
        let decision = manager.evaluate(100.0, 102.0, 2.0, 5, true); // 1R reached (2.0 favorable / 2.0 risk)
        assert_eq!(decision, StopDecision::MoveToBreakeven(100.0));

        let no_trigger = manager.evaluate(100.0, 100.5, 2.0, 5, true);
        assert_eq!(no_trigger, StopDecision::Hold);
    }

    #[test]
    fn test_stop_manager_time_stop_takes_priority() {
        let manager = StopManager {
            breakeven_trigger_r: Some(1.0),
            time_stop_bars: Some(3),
        };
        // Even though breakeven would also trigger, time stop is evaluated first / takes
        // priority once bars_held exceeds the limit.
        let decision = manager.evaluate(100.0, 105.0, 2.0, 10, true);
        assert_eq!(decision, StopDecision::TimeStopExit);
    }

    #[test]
    fn test_stop_manager_short_side_direction() {
        let manager = StopManager {
            breakeven_trigger_r: Some(1.0),
            time_stop_bars: None,
        };
        let decision = manager.evaluate(100.0, 98.0, 2.0, 5, false); // short: price fell 2.0 = 1R favorable
        assert_eq!(decision, StopDecision::MoveToBreakeven(100.0));
    }
}
