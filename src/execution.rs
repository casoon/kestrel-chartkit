//! Provider-neutral order/fill simulator: market/limit/stop/stop-limit/trailing orders, partial
//! fills bounded by a per-bar participation cap, pyramiding (multiple same-direction fills
//! accumulating one position), fees/spread/slippage, and explicit position state.
//!
//! Intrabar fill logic is a documented approximation, not a claim of perfect intrabar path
//! replay: a bar's open/high/low/close order is assumed (configurable), and whichever of
//! high/low is reached "first" under that assumption determines which side of a bar a resting
//! order fills against. Real intrabar order is unknowable from OHLC alone.

use crate::model::Bar;
use std::collections::BTreeMap;

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
    /// Set for orders submitted through [`submit_bracket`]: identifies this order's bracket group
    /// and its role within it. `None` for a standalone order submitted through
    /// [`FillSimulator::submit`].
    pub bracket: Option<BracketLink>,
}

/// Identifies an order as part of a bracket (entry + stop-loss + take-profit) submitted via
/// [`submit_bracket`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BracketLink {
    /// Order ID of this bracket's entry order (the entry's own `BracketLink::entry_id` equals its
    /// own `id`).
    pub entry_id: u64,
    /// This order's role within the bracket.
    pub role: BracketRole,
}

/// An order's role within a bracket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BracketRole {
    /// The order that opens the position.
    Entry,
    /// The stop-loss exit.
    StopLoss,
    /// The take-profit exit.
    TakeProfit,
}

/// How a bracket's stop-loss and take-profit are ordered when a single bar's OHLC range touches
/// both. Real intrabar order is unknowable from OHLC alone; this makes the assumption explicit and
/// deterministic instead of leaving it to fill-collection order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum IntrabarFillPolicy {
    /// The stop-loss is assumed to be touched first, i.e. the conservative (worse-for-the-
    /// position) outcome is realized. Default.
    #[default]
    StopFirst,
    /// The take-profit is assumed to be touched first.
    TargetFirst,
}

/// Per-bracket bookkeeping: how much of the entry has filled so far, and how much of that has
/// already been closed by one of its exits. `entry_filled - exit_closed` is the exit capacity
/// available to the stop-loss/take-profit this bracket's exits may still consume.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
struct BracketState {
    entry_filled: f64,
    exit_closed: f64,
    /// `true` once the entry can no longer contribute more fills (fully `Filled` or
    /// `Cancelled`), so a subsequently-exhausted exit capacity is permanent rather than just
    /// "not yet replenished by a later partial entry fill".
    entry_done: bool,
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
    /// Same-bar tie-break when a bracket's stop-loss and take-profit are both touched by the same
    /// OHLC bar. See [`IntrabarFillPolicy`].
    pub bracket_intrabar_policy: IntrabarFillPolicy,
}

pub struct FillSimulator {
    config: FillSimulatorConfig,
    orders: Vec<Order>,
    next_order_id: u64,
    position: Position,
    pyramid_entries: u32,
    fills: Vec<Fill>,
    /// Keyed by entry order ID. Tracks how much of each bracket's entry has filled and how much
    /// has already been closed by an exit, so exits stay bounded by the position they are
    /// actually protecting (finding 02).
    bracket_state: BTreeMap<u64, BracketState>,
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
            bracket_state: BTreeMap::new(),
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
        self.submit_internal(side, kind, quantity, None)
    }

    fn submit_internal(
        &mut self,
        side: OrderSide,
        kind: OrderKind,
        quantity: f64,
        bracket: Option<BracketLink>,
    ) -> Option<u64> {
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
            bracket,
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
                if let Some(BracketLink {
                    entry_id,
                    role: BracketRole::Entry,
                }) = order.bracket
                {
                    // The entry can no longer contribute more fills; any resting exit capacity
                    // it already granted is now permanent, not "pending more".
                    self.bracket_state.entry(entry_id).or_default().entry_done = true;
                }
                return true;
            }
        }
        false
    }

    /// Processes one bar against all resting orders: updates trailing stops, checks fill
    /// conditions, applies costs, and updates position state. Returns the fills produced this
    /// bar.
    ///
    /// Bracket orders (submitted via [`submit_bracket`]) are evaluated in two passes: standalone
    /// orders and bracket entries first, then bracket exits. A bracket's stop-loss/take-profit
    /// only become active once (and only for as much quantity as) the entry has actually filled,
    /// and the two exits share one OCO capacity budget (`entry_filled - exit_closed`) so that
    /// whichever fills first — per [`IntrabarFillPolicy`] when a single bar touches both —
    /// immediately caps the other within the same bar. This prevents a bracket's exits from
    /// filling before its entry, or from jointly closing more than the position they protect.
    pub fn on_bar(&mut self, bar: &Bar, timestamp: i64) -> Vec<Fill> {
        let mut bar_fills = Vec::new();
        let max_fill_qty = self
            .config
            .max_fill_ratio_of_volume
            .map(|r| (r * bar.volume).max(0.0));

        let primary_ids: Vec<u64> = self
            .orders
            .iter()
            .filter(|o| {
                matches!(
                    o.status,
                    OrderStatus::Pending | OrderStatus::PartiallyFilled
                )
            })
            .filter(|o| {
                !matches!(
                    o.bracket,
                    Some(BracketLink {
                        role: BracketRole::StopLoss | BracketRole::TakeProfit,
                        ..
                    })
                )
            })
            .map(|o| o.id)
            .collect();
        for id in primary_ids {
            self.attempt_fill(id, bar, timestamp, max_fill_qty, None, &mut bar_fills);
        }

        let mut brackets: BTreeMap<u64, (Option<u64>, Option<u64>)> = BTreeMap::new();
        for order in self.orders.iter().filter(|o| {
            matches!(
                o.status,
                OrderStatus::Pending | OrderStatus::PartiallyFilled
            )
        }) {
            if let Some(BracketLink { entry_id, role }) = order.bracket {
                let slot = brackets.entry(entry_id).or_default();
                match role {
                    BracketRole::StopLoss => slot.0 = Some(order.id),
                    BracketRole::TakeProfit => slot.1 = Some(order.id),
                    BracketRole::Entry => {}
                }
            }
        }
        for (entry_id, (stop_id, target_id)) in brackets {
            let ordered = match self.config.bracket_intrabar_policy {
                IntrabarFillPolicy::StopFirst => [stop_id, target_id],
                IntrabarFillPolicy::TargetFirst => [target_id, stop_id],
            };
            for id in ordered.into_iter().flatten() {
                let capacity = self
                    .bracket_state
                    .get(&entry_id)
                    .map(|s| s.entry_filled - s.exit_closed)
                    .unwrap_or(0.0);
                if capacity <= 0.0 {
                    continue;
                }
                self.attempt_fill(
                    id,
                    bar,
                    timestamp,
                    max_fill_qty,
                    Some(capacity),
                    &mut bar_fills,
                );
            }
        }

        for fill in &bar_fills {
            self.apply_fill(fill);
        }
        self.fills.extend(bar_fills.iter().copied());

        // Once a bracket's exit capacity is exhausted (the position it protects is fully
        // closed), cancel the untouched sibling instead of leaving a dead resting order that can
        // never fill again.
        self.cancel_exhausted_bracket_exits();

        // Terminal orders (Filled/Cancelled) no longer participate in fill checks; their history
        // already lives in `self.fills`, so drop them here rather than rescanning them forever.
        self.orders
            .retain(|o| !matches!(o.status, OrderStatus::Filled | OrderStatus::Cancelled));

        bar_fills
    }

    /// Evaluates a single order against `bar` and, if its fill conditions are met, records a fill
    /// (capped by `max_fill_qty` and, for bracket exits, by `capacity_cap`) and updates the
    /// order's own state. Bracket accounting (`bracket_state`) is updated here too, so a sibling
    /// exit evaluated later in the same bar sees an up-to-date capacity.
    fn attempt_fill(
        &mut self,
        order_id: u64,
        bar: &Bar,
        timestamp: i64,
        max_fill_qty: Option<f64>,
        capacity_cap: Option<f64>,
        bar_fills: &mut Vec<Fill>,
    ) {
        let Some(idx) = self.orders.iter().position(|o| o.id == order_id) else {
            return;
        };

        {
            let order = &mut self.orders[idx];
            if !matches!(
                order.status,
                OrderStatus::Pending | OrderStatus::PartiallyFilled
            ) {
                return;
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
        }

        let (oid, side, bracket, executed_price, fill_qty, fee) = {
            let order = &self.orders[idx];
            let Some(fill_price) = fill_price_for(order, bar) else {
                return;
            };

            let remaining = order.quantity - order.filled_quantity;
            let mut fill_qty = max_fill_qty
                .map(|cap| remaining.min(cap))
                .unwrap_or(remaining);
            if let Some(cap) = capacity_cap {
                fill_qty = fill_qty.min(cap.max(0.0));
            }
            if fill_qty <= 0.0 {
                return;
            }

            let costs = self.config.costs;
            let side_sign = order.side.sign();
            let executed_price = fill_price
                * (1.0
                    + side_sign * (costs.spread / fill_price.max(1e-9) / 2.0 + costs.slippage_pct));
            let fee = executed_price * fill_qty * costs.fee_pct;
            (
                order.id,
                order.side,
                order.bracket,
                executed_price,
                fill_qty,
                fee,
            )
        };

        let new_status = {
            let order = &mut self.orders[idx];
            order.filled_quantity += fill_qty;
            order.status = if order.filled_quantity >= order.quantity - 1e-9 {
                OrderStatus::Filled
            } else {
                OrderStatus::PartiallyFilled
            };
            order.status
        };

        bar_fills.push(Fill {
            order_id: oid,
            side,
            price: executed_price,
            quantity: fill_qty,
            fee,
            timestamp,
        });

        if let Some(link) = bracket {
            let state = self.bracket_state.entry(link.entry_id).or_default();
            match link.role {
                BracketRole::Entry => {
                    state.entry_filled += fill_qty;
                    if new_status == OrderStatus::Filled {
                        // Fully filled: no more capacity will ever be granted to this bracket's
                        // exits, so an exhausted capacity from here on is permanent.
                        state.entry_done = true;
                    }
                }
                BracketRole::StopLoss | BracketRole::TakeProfit => state.exit_closed += fill_qty,
            }
        }
    }

    /// Cancels a bracket's still-resting exit(s) once the entry can no longer contribute more
    /// fills (fully filled or cancelled) *and* the capacity already granted has been fully
    /// consumed, so a stale, permanently-unfillable order does not linger. A capacity of zero
    /// while the entry is still `Pending`/`PartiallyFilled` (more fills may still arrive) is left
    /// alone.
    fn cancel_exhausted_bracket_exits(&mut self) {
        let exhausted: Vec<u64> = self
            .bracket_state
            .iter()
            .filter(|(_, s)| s.entry_done && s.entry_filled - s.exit_closed <= 1e-9)
            .map(|(entry_id, _)| *entry_id)
            .collect();
        if exhausted.is_empty() {
            return;
        }
        for order in &mut self.orders {
            if let Some(BracketLink { entry_id, role }) = order.bracket {
                if exhausted.contains(&entry_id)
                    && matches!(role, BracketRole::StopLoss | BracketRole::TakeProfit)
                    && matches!(
                        order.status,
                        OrderStatus::Pending | OrderStatus::PartiallyFilled
                    )
                {
                    order.status = OrderStatus::Cancelled;
                }
            }
        }
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

/// Submits a bracket: an entry order plus stop-loss and take-profit exits linked to it as one OCO
/// ("one cancels other") group. The exits are inactive until the entry actually fills, become
/// active for at most the entry's filled-but-not-yet-closed quantity (so a partial entry fill
/// cannot be over-closed), and share one capacity budget so that whichever fills first — per
/// [`FillSimulatorConfig::bracket_intrabar_policy`] when a single bar touches both — immediately
/// caps the other within the same bar. Callers still get the three order IDs back and may cancel
/// them individually (e.g. to tear down a bracket whose entry never filled).
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
    // `next_order_id` is only consumed on a successful submission (see `submit_internal`), so it
    // reliably predicts the entry's own ID for its self-referencing `BracketLink`.
    let entry_id = sim.next_order_id;
    let confirmed_entry_id = sim.submit_internal(
        side,
        entry,
        quantity,
        Some(BracketLink {
            entry_id,
            role: BracketRole::Entry,
        }),
    )?;
    debug_assert_eq!(entry_id, confirmed_entry_id);

    let stop_id = sim.submit_internal(
        exit_side,
        OrderKind::Stop {
            trigger: stop_loss_trigger,
        },
        quantity,
        Some(BracketLink {
            entry_id,
            role: BracketRole::StopLoss,
        }),
    )?;
    let target_id = sim.submit_internal(
        exit_side,
        OrderKind::Limit {
            price: take_profit_price,
        },
        quantity,
        Some(BracketLink {
            entry_id,
            role: BracketRole::TakeProfit,
        }),
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

    /// Finding 02, scenario A: an exit must never fill before its own entry has filled, even if
    /// the exit's price condition is independently met on the very first bar after submission.
    #[test]
    fn test_bracket_exit_cannot_fill_before_entry() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let (entry_id, stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            10.0,
            OrderKind::Limit { price: 90.0 },
            80.0,
            110.0,
        )
        .unwrap();

        // Entry (buy limit 90) is not touched (low 119 > 90), but the target (sell limit 110) is
        // independently marketable against this bar (high 121 >= 110).
        let fills = sim.on_bar(&bar(120.0, 121.0, 119.0, 120.0, 1000.0), 0);
        assert!(
            fills.is_empty(),
            "target must not fill before its entry: {fills:?}"
        );
        assert_eq!(sim.position().quantity, 0.0);
        assert_eq!(
            sim.open_orders().find(|o| o.id == entry_id).unwrap().status,
            OrderStatus::Pending
        );
        assert_eq!(
            sim.open_orders().find(|o| o.id == stop_id).unwrap().status,
            OrderStatus::Pending
        );
        assert_eq!(
            sim.open_orders()
                .find(|o| o.id == target_id)
                .unwrap()
                .status,
            OrderStatus::Pending
        );
    }

    /// Finding 02, scenario B: a single bar that touches both stop and target after the entry
    /// fills must produce exactly one exit fill (per the configured intrabar policy), leaving the
    /// position flat and cancelling the untouched sibling rather than filling both.
    #[test]
    fn test_bracket_same_bar_stop_and_target_only_stop_fires_under_default_policy() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let (_entry_id, stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            10.0,
            OrderKind::Market,
            95.0,
            110.0,
        )
        .unwrap();

        let fills = sim.on_bar(&bar(100.0, 112.0, 94.0, 105.0, 1000.0), 0);
        assert_eq!(
            fills.len(),
            2,
            "expected entry fill + exactly one exit fill: {fills:?}"
        );
        assert_eq!(fills[1].order_id, stop_id, "default policy is stop-first");
        assert_eq!(sim.position().quantity, 0.0);
        assert!(
            sim.open_orders().find(|o| o.id == target_id).is_none(),
            "untouched sibling must be cancelled, not left resting"
        );
    }

    /// Same same-bar collision as above, but under `IntrabarFillPolicy::TargetFirst`: the target
    /// wins instead, still exactly one exit fill.
    #[test]
    fn test_bracket_same_bar_stop_and_target_target_first_policy() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            bracket_intrabar_policy: IntrabarFillPolicy::TargetFirst,
            ..Default::default()
        });
        let (_entry_id, stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            10.0,
            OrderKind::Market,
            95.0,
            110.0,
        )
        .unwrap();

        let fills = sim.on_bar(&bar(100.0, 112.0, 94.0, 105.0, 1000.0), 0);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[1].order_id, target_id);
        assert_eq!(sim.position().quantity, 0.0);
        assert!(sim.open_orders().find(|o| o.id == stop_id).is_none());
    }

    /// Finding 02, scenario C: a partially filled entry must never let its exits close more than
    /// the quantity actually opened so far.
    #[test]
    fn test_bracket_exit_never_exceeds_partially_filled_entry_quantity() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            max_fill_ratio_of_volume: Some(0.1),
            ..Default::default()
        });
        let (entry_id, stop_id, _target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            100.0,
            OrderKind::Market,
            95.0,
            110.0,
        )
        .unwrap();

        // Bar 1: entry and stop are both capped to 10% of 500 volume = 50. Entry fills 50 (of
        // 100), and the stop, touched the same bar, may close at most that same 50 -- not the
        // full 100 quantity it was submitted with.
        let fills = sim.on_bar(&bar(100.0, 100.0, 90.0, 95.0, 500.0), 0);
        let entry_filled: f64 = fills
            .iter()
            .filter(|f| f.order_id == entry_id)
            .map(|f| f.quantity)
            .sum();
        let stop_filled: f64 = fills
            .iter()
            .filter(|f| f.order_id == stop_id)
            .map(|f| f.quantity)
            .sum();
        assert_eq!(entry_filled, 50.0);
        assert!(
            stop_filled <= entry_filled + 1e-9,
            "exit filled {stop_filled} against only {entry_filled} of entry"
        );
    }

    /// Long and short brackets must behave symmetrically: the finding's scenario A mirrored for a
    /// sell entry with a buy-side stop/target.
    #[test]
    fn test_short_bracket_exit_cannot_fill_before_entry() {
        let mut sim = FillSimulator::new(FillSimulatorConfig::default());
        let (_entry_id, _stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Sell,
            10.0,
            OrderKind::Limit { price: 110.0 },
            120.0,
            90.0,
        )
        .unwrap();

        // Entry (sell limit 110) is not touched (high 108 < 110), but the buy target (limit 90)
        // is independently marketable (low 85 <= 90).
        let fills = sim.on_bar(&bar(100.0, 108.0, 85.0, 95.0, 1000.0), 0);
        assert!(
            fills.is_empty(),
            "target must not fill before its entry: {fills:?}"
        );
        assert_eq!(sim.position().quantity, 0.0);
        assert_eq!(
            sim.open_orders()
                .find(|o| o.id == target_id)
                .unwrap()
                .status,
            OrderStatus::Pending
        );
    }

    /// Documents current, accepted behavior (not a bug fixed by finding 02): pending same-
    /// direction entry orders are not counted against `max_pyramid_entries` while the position is
    /// flat, only already-filled positions are. Two pending brackets can therefore both go on to
    /// fill even with a pyramid cap of 1.
    #[test]
    fn test_pending_same_direction_entries_not_capped_by_pyramid_limit_while_flat() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            max_pyramid_entries: Some(1),
            ..Default::default()
        });
        let first = sim.submit(OrderSide::Buy, OrderKind::Limit { price: 100.0 }, 5.0);
        let second = sim.submit(OrderSide::Buy, OrderKind::Limit { price: 100.0 }, 5.0);
        assert!(first.is_some());
        assert!(
            second.is_some(),
            "pending pyramid cap is not yet enforced pre-fill"
        );

        let fills = sim.on_bar(&bar(100.0, 100.0, 99.0, 100.0, 1000.0), 0);
        assert_eq!(fills.len(), 2);
        assert_eq!(sim.position().quantity, 10.0);
    }

    /// Fees/spread/slippage change fill *prices*, not the bracket linkage/capacity logic: the
    /// same scenario-B invariants (one exit fires, position ends flat, sibling cancelled) must
    /// still hold with non-zero costs configured.
    #[test]
    fn test_bracket_linkage_unaffected_by_costs() {
        let mut sim = FillSimulator::new(FillSimulatorConfig {
            costs: ExecutionCosts {
                fee_pct: 0.001,
                spread: 0.5,
                slippage_pct: 0.0005,
            },
            ..Default::default()
        });
        let (_entry_id, stop_id, target_id) = submit_bracket(
            &mut sim,
            OrderSide::Buy,
            10.0,
            OrderKind::Market,
            95.0,
            110.0,
        )
        .unwrap();

        let fills = sim.on_bar(&bar(100.0, 112.0, 94.0, 105.0, 1000.0), 0);
        assert_eq!(fills.len(), 2);
        assert_eq!(fills[1].order_id, stop_id);
        assert_eq!(sim.position().quantity, 0.0);
        assert!(sim.open_orders().find(|o| o.id == target_id).is_none());
    }
}
