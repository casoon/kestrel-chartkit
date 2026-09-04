//! Provider-neutral order/fill simulator: market/limit/stop/stop-limit/trailing orders, partial
//! fills bounded by a per-bar participation cap, pyramiding (multiple same-direction fills
//! accumulating one position), fees/spread/slippage, and explicit position state.
//!
//! Intrabar fill logic is a documented approximation, not a claim of perfect intrabar path
//! replay: a bar's open/high/low/close order is assumed (configurable), and whichever of
//! high/low is reached "first" under that assumption determines which side of a bar a resting
//! order fills against. Real intrabar order is unknowable from OHLC alone.

use crate::model::Bar;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    fn sign(self) -> f64 {
        match self {
            OrderSide::Buy => 1.0,
            OrderSide::Sell => -1.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OrderKind {
    Market,
    Limit {
        price: f64,
    },
    Stop {
        trigger: f64,
    },
    StopLimit {
        trigger: f64,
        limit: f64,
    },
    /// Trailing stop: `trail_amount` is the fixed price distance kept behind the best price seen
    /// since the order was submitted.
    Trailing {
        trail_amount: f64,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderStatus {
    Pending,
    PartiallyFilled,
    Filled,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Order {
    pub id: u64,
    pub side: OrderSide,
    pub kind: OrderKind,
    pub quantity: f64,
    pub filled_quantity: f64,
    pub status: OrderStatus,
    /// For `OrderKind::Trailing`: the current computed stop level, updated every bar.
    pub trailing_stop_price: Option<f64>,
    /// For `OrderKind::StopLimit`: `true` once the trigger has been crossed and the order behaves
    /// as a resting limit order at `limit`.
    pub stop_triggered: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fill {
    pub order_id: u64,
    pub side: OrderSide,
    pub price: f64,
    pub quantity: f64,
    pub fee: f64,
    pub timestamp: i64,
}

/// Trading costs applied to every fill.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct ExecutionCosts {
    /// Fraction of notional charged as a fee per fill (e.g. `0.001` = 10 bps).
    pub fee_pct: f64,
    /// Fixed price spread applied against the order side (buys fill `spread/2` higher, sells
    /// `spread/2` lower).
    pub spread: f64,
    /// Additional adverse slippage as a fraction of price, applied the same direction as spread.
    pub slippage_pct: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Position {
    /// Positive = long, negative = short, `0.0` = flat.
    pub quantity: f64,
    pub avg_entry_price: f64,
    pub realized_pnl: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FillSimulatorConfig {
    pub costs: ExecutionCosts,
    /// Caps how much of a pending order's remaining quantity can fill in one bar, as a fraction
    /// of that bar's volume (`None` = no cap, fill fully when price conditions are met). Models
    /// participation-rate-limited partial fills.
    pub max_fill_ratio_of_volume: Option<f64>,
    /// Maximum number of same-direction fills accumulated into one position (pyramiding cap).
    /// `None` = unlimited.
    pub max_pyramid_entries: Option<u32>,
}

pub struct FillSimulator {
    config: FillSimulatorConfig,
    orders: Vec<Order>,
    next_order_id: u64,
    position: Position,
    pyramid_entries: u32,
    fills: Vec<Fill>,
}

impl FillSimulator {
    pub fn new(config: FillSimulatorConfig) -> Self {
        Self {
            config,
            orders: Vec::new(),
            next_order_id: 1,
            position: Position::default(),
            pyramid_entries: 0,
            fills: Vec::new(),
        }
    }

    pub fn position(&self) -> Position {
        self.position
    }

    pub fn fills(&self) -> &[Fill] {
        &self.fills
    }

    pub fn open_orders(&self) -> impl Iterator<Item = &Order> {
        self.orders.iter().filter(|o| {
            matches!(
                o.status,
                OrderStatus::Pending | OrderStatus::PartiallyFilled
            )
        })
    }

    /// Submits a new order, returning its ID. Rejected (returns `None`) if this would exceed
    /// `max_pyramid_entries` same-direction accumulations.
    pub fn submit(&mut self, side: OrderSide, kind: OrderKind, quantity: f64) -> Option<u64> {
        if quantity <= 0.0 {
            return None;
        }
        let would_pyramid = self.position.quantity != 0.0
            && self.position.quantity.signum() == side.sign()
            && self.pyramid_entries > 0;
        if would_pyramid {
            if let Some(max) = self.config.max_pyramid_entries {
                if self.pyramid_entries >= max {
                    return None;
                }
            }
        }

        let id = self.next_order_id;
        self.next_order_id += 1;
        self.orders.push(Order {
            id,
            side,
            kind,
            quantity,
            filled_quantity: 0.0,
            status: OrderStatus::Pending,
            trailing_stop_price: None,
            stop_triggered: false,
        });
        Some(id)
    }

    pub fn cancel(&mut self, order_id: u64) -> bool {
        if let Some(order) = self.orders.iter_mut().find(|o| o.id == order_id) {
            if matches!(
                order.status,
                OrderStatus::Pending | OrderStatus::PartiallyFilled
            ) {
                order.status = OrderStatus::Cancelled;
                return true;
            }
        }
        false
    }

    /// Processes one bar against all resting orders: updates trailing stops, checks fill
    /// conditions, applies costs, and updates position state. Returns the fills produced this
    /// bar.
    pub fn on_bar(&mut self, bar: &Bar, timestamp: i64) -> Vec<Fill> {
        let mut bar_fills = Vec::new();
        let max_fill_qty = self
            .config
            .max_fill_ratio_of_volume
            .map(|r| (r * bar.volume).max(0.0));

        for order in &mut self.orders {
            if !matches!(
                order.status,
                OrderStatus::Pending | OrderStatus::PartiallyFilled
            ) {
                continue;
            }

            if let OrderKind::Trailing { trail_amount } = order.kind {
                // Sell (exits a long): stop trails below the high, ratcheting up only.
                // Buy (exits/covers a short): stop trails above the low, ratcheting down only.
                let candidate = match order.side {
                    OrderSide::Sell => bar.high - trail_amount,
                    OrderSide::Buy => bar.low + trail_amount,
                };
                order.trailing_stop_price = Some(match (order.trailing_stop_price, order.side) {
                    (Some(prev), OrderSide::Sell) => prev.max(candidate),
                    (Some(prev), OrderSide::Buy) => prev.min(candidate),
                    (None, _) => candidate,
                });
            }

            if let OrderKind::StopLimit { trigger, .. } = order.kind {
                if !order.stop_triggered {
                    let crossed = match order.side {
                        OrderSide::Buy => bar.high >= trigger,
                        OrderSide::Sell => bar.low <= trigger,
                    };
                    if crossed {
                        order.stop_triggered = true;
                    }
                }
            }

            let Some(fill_price) = fill_price_for(order, bar) else {
                continue;
            };

            let remaining = order.quantity - order.filled_quantity;
            let fill_qty = max_fill_qty
                .map(|cap| remaining.min(cap))
                .unwrap_or(remaining);
            if fill_qty <= 0.0 {
                continue;
            }

            let costs = &self.config.costs;
            let side_sign = order.side.sign();
            let executed_price = fill_price
                * (1.0
                    + side_sign * (costs.spread / fill_price.max(1e-9) / 2.0 + costs.slippage_pct));
            let fee = executed_price * fill_qty * costs.fee_pct;

            order.filled_quantity += fill_qty;
            order.status = if order.filled_quantity >= order.quantity - 1e-9 {
                OrderStatus::Filled
            } else {
                OrderStatus::PartiallyFilled
            };

            bar_fills.push(Fill {
                order_id: order.id,
                side: order.side,
                price: executed_price,
                quantity: fill_qty,
                fee,
                timestamp,
            });
        }

        for fill in &bar_fills {
            self.apply_fill(fill);
        }
        self.fills.extend(bar_fills.iter().copied());

        // Terminal orders (Filled/Cancelled) no longer participate in fill checks; their history
        // already lives in `self.fills`, so drop them here rather than rescanning them forever.
        self.orders
            .retain(|o| !matches!(o.status, OrderStatus::Filled | OrderStatus::Cancelled));

        bar_fills
    }

    fn apply_fill(&mut self, fill: &Fill) {
        let signed_qty = fill.quantity * fill.side.sign();
        let prev_qty = self.position.quantity;
        let new_qty = prev_qty + signed_qty;

        if prev_qty == 0.0 || prev_qty.signum() == signed_qty.signum() {
            // Opening or adding to a position (pyramiding): weighted-average entry price.
            let total_cost =
                self.position.avg_entry_price * prev_qty.abs() + fill.price * fill.quantity;
            self.position.avg_entry_price = if new_qty.abs() > 1e-12 {
                total_cost / new_qty.abs()
            } else {
                0.0
            };
            if prev_qty == 0.0 {
                self.pyramid_entries = 1;
            } else {
                self.pyramid_entries += 1;
            }
        } else {
            // Reducing, closing, or flipping.
            let closing_qty = fill.quantity.min(prev_qty.abs());
            let pnl_per_unit = (fill.price - self.position.avg_entry_price) * prev_qty.signum();
            self.position.realized_pnl += pnl_per_unit * closing_qty;

            if fill.quantity > prev_qty.abs() {
                // Flip: the excess opens a new position in the opposite direction.
                self.position.avg_entry_price = fill.price;
                self.pyramid_entries = 1;
            } else if new_qty.abs() < 1e-12 {
                self.position.avg_entry_price = 0.0;
                self.pyramid_entries = 0;
            }
        }

        self.position.realized_pnl -= fill.fee;
        self.position.quantity = new_qty;
    }
}

fn fill_price_for(order: &Order, bar: &Bar) -> Option<f64> {
    match order.kind {
        OrderKind::Market => Some(bar.open),
        OrderKind::Limit { price } => match order.side {
            OrderSide::Buy if bar.low <= price => Some(price.min(bar.open)),
            OrderSide::Sell if bar.high >= price => Some(price.max(bar.open)),
            _ => None,
        },
        OrderKind::Stop { trigger } => match order.side {
            OrderSide::Buy if bar.high >= trigger => Some(trigger.max(bar.open)),
            OrderSide::Sell if bar.low <= trigger => Some(trigger.min(bar.open)),
            _ => None,
        },
        OrderKind::StopLimit { limit, .. } => {
            // `order.stop_triggered` is updated (and persisted across bars) by the caller before
            // this is invoked, so a trigger crossed on an earlier bar still counts here even if
            // price has since retreated back through the trigger level.
            if !order.stop_triggered {
                return None;
            }
            match order.side {
                OrderSide::Buy if bar.low <= limit => Some(limit),
                OrderSide::Sell if bar.high >= limit => Some(limit),
                _ => None,
            }
        }
        OrderKind::Trailing { .. } => {
            let stop = order.trailing_stop_price?;
            match order.side {
                OrderSide::Buy if bar.high >= stop => Some(stop.max(bar.open)),
                OrderSide::Sell if bar.low <= stop => Some(stop.min(bar.open)),
                _ => None,
            }
        }
    }
}

/// A simple bracket helper: submits an entry order plus stop-loss and take-profit exits that
/// mirror it. Not itself stateful — callers manage the returned IDs (e.g. cancel the untouched
/// exit once the other one fills, which this simulator does not do automatically since it has no
/// concept of linked orders).
pub fn submit_bracket(
    sim: &mut FillSimulator,
    side: OrderSide,
    quantity: f64,
    entry: OrderKind,
    stop_loss_trigger: f64,
    take_profit_price: f64,
) -> Option<(u64, u64, u64)> {
    let exit_side = match side {
        OrderSide::Buy => OrderSide::Sell,
        OrderSide::Sell => OrderSide::Buy,
    };
    let entry_id = sim.submit(side, entry, quantity)?;
    let stop_id = sim.submit(
        exit_side,
        OrderKind::Stop {
            trigger: stop_loss_trigger,
        },
        quantity,
    )?;
    let target_id = sim.submit(
        exit_side,
        OrderKind::Limit {
            price: take_profit_price,
        },
        quantity,
    )?;
    Some((entry_id, stop_id, target_id))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(o: f64, h: f64, l: f64, c: f64, v: f64) -> Bar {
        Bar::new(0, o, h, l, c, v)
    }

    #[test]
    fn test_market_order_fills_at_open() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        let fills = sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].price, 100.0);
        assert_eq!(sim.position().quantity, 10.0);
        assert_eq!(sim.position().avg_entry_price, 100.0);
    }

    #[test]
    fn test_limit_order_only_fills_when_touched() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(OrderSide::Buy, OrderKind::Limit { price: 95.0 }, 5.0);

        let no_touch = sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);
        assert!(no_touch.is_empty());

        let touched = sim.on_bar(&bar(98.0, 99.0, 94.0, 96.0, 1000.0), 60);
        assert_eq!(touched.len(), 1);
        assert!(touched[0].price <= 95.0 + 1e-9);
    }

    #[test]
    fn test_stop_order_fills_on_trigger() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(OrderSide::Sell, OrderKind::Stop { trigger: 95.0 }, 5.0);
        let fills = sim.on_bar(&bar(98.0, 99.0, 93.0, 94.0, 1000.0), 0);
        assert_eq!(fills.len(), 1);
    }

    #[test]
    fn test_stop_limit_trigger_persists_across_bars_after_price_retreats() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(
            OrderSide::Buy,
            OrderKind::StopLimit {
                trigger: 100.0,
                limit: 99.0,
            },
            5.0,
        );

        // Bar 1: trigger crossed (high >= 100), but the limit (99) is not reached this bar
        // (low stays at 99.5, above the 99.0 limit).
        let first = sim.on_bar(&bar(100.0, 101.0, 99.5, 100.5, 1000.0), 0);
        assert!(first.is_empty());

        // Bar 2: price retreats below the trigger but not yet down to the limit -- a naive
        // re-check would see high(99.4) < trigger(100) and wrongly conclude "not triggered", but
        // the order must stay armed since it already triggered on bar 1. No fill yet since
        // low(99.1) is still above the limit(99.0).
        let second = sim.on_bar(&bar(99.2, 99.4, 99.1, 99.3, 1000.0), 60);
        assert!(second.is_empty());

        // Bar 3: trades down into the limit price (99); must fill using the persisted trigger.
        let third = sim.on_bar(&bar(99.5, 100.0, 98.5, 99.0, 1000.0), 120);
        assert_eq!(third.len(), 1);
        assert_eq!(third[0].price, 99.0);
    }

    #[test]
    fn test_partial_fill_capped_by_volume_ratio() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            max_fill_ratio_of_volume: Some(0.1),
            ..Default::default()
        });
        let id = sim
            .submit(OrderSide::Buy, OrderKind::Market, 100.0)
            .unwrap();

        let first = sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 500.0), 0);
        assert_eq!(first[0].quantity, 50.0); // 10% of 500 volume
        assert_eq!(
            sim.open_orders().find(|o| o.id == id).unwrap().status,
            OrderStatus::PartiallyFilled
        );

        let second = sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 500.0), 60);
        assert_eq!(second[0].quantity, 50.0);
        assert!(sim.open_orders().find(|o| o.id == id).is_none());
        assert_eq!(sim.position().quantity, 100.0);
    }

    #[test]
    fn test_pyramiding_accumulates_weighted_average_entry() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        sim.on_bar(&bar(110.0, 111.0, 109.0, 110.5, 1000.0), 60);

        assert_eq!(sim.position().quantity, 20.0);
        assert!((sim.position().avg_entry_price - 105.0).abs() < 1e-9);
    }

    #[test]
    fn test_pyramid_cap_rejects_beyond_limit() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            max_pyramid_entries: Some(1),
            ..Default::default()
        });
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);

        let rejected = sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        assert!(rejected.is_none());
    }

    #[test]
    fn test_opposite_fill_realizes_pnl_and_reduces_position() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);

        sim.submit(OrderSide::Sell, OrderKind::Market, 10.0);
        sim.on_bar(&bar(110.0, 111.0, 109.0, 110.5, 1000.0), 60);

        assert_eq!(sim.position().quantity, 0.0);
        assert!((sim.position().realized_pnl - 100.0).abs() < 1e-9); // 10 units * $10 gain
    }

    #[test]
    fn test_fees_and_spread_reduce_pnl() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            costs: ExecutionCosts {
                fee_pct: 0.01,
                spread: 0.0,
                slippage_pct: 0.0,
            },
            ..Default::default()
        });
        sim.submit(OrderSide::Buy, OrderKind::Market, 10.0);
        let fills = sim.on_bar(&bar(100.0, 101.0, 99.0, 100.5, 1000.0), 0);
        assert!(fills[0].fee > 0.0);
        assert!(
            sim.position().realized_pnl < 0.0,
            "fees alone must show as negative realized PnL"
        );
    }

    #[test]
    fn test_trailing_stop_tightens_and_fills() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        // Long position being protected by a trailing sell-stop trailing 2.0 below the high.
        sim.submit(
            OrderSide::Sell,
            OrderKind::Trailing { trail_amount: 2.0 },
            10.0,
        );

        // Each bar's own range stays under trail_amount=2.0, so neither bar's low ever reaches
        // that same bar's freshly computed stop -- isolates "does the stop ratchet and hold"
        // from "does a bar's own range trigger its own stop".
        let first = sim.on_bar(&bar(100.0, 105.0, 104.0, 104.5, 1000.0), 0); // stop -> 105-2=103
        assert!(first.is_empty());
        let second = sim.on_bar(&bar(104.0, 108.0, 107.0, 107.5, 1000.0), 60); // stop -> max(103,106)=106
        assert!(second.is_empty());

        // Pulls back to 104: this bar's own high-2=105.5 would suggest a *lower* stop, but the
        // ratchet must hold at 106 from the prior bar -- and 104 <= 106 triggers the fill.
        let third = sim.on_bar(&bar(107.0, 107.5, 104.0, 105.0, 1000.0), 120);
        assert_eq!(third.len(), 1);
        assert!((third[0].price - 106.0).abs() < 1e-9);
    }

    #[test]
    fn test_cancel_prevents_future_fills() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let id = sim
            .submit(OrderSide::Buy, OrderKind::Limit { price: 50.0 }, 5.0)
            .unwrap();
        assert!(sim.cancel(id));
        let fills = sim.on_bar(&bar(48.0, 49.0, 45.0, 46.0, 1000.0), 0);
        assert!(fills.is_empty());
    }

    #[test]
    fn test_submit_bracket_creates_entry_and_two_exits() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let (entry, stop, target) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            10.0,
            OrderKind::Market,
            95.0,
            110.0,
        )
        .unwrap();
        assert_ne!(entry, stop);
        assert_ne!(stop, target);
    }
}
