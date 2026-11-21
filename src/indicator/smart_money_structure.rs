//! Linked smart-money structure: named BSL/SSL liquidity pools (from Equal High/Low clusters)
//! with an explicit Stop-Hunt-vs-Breakout-vs-Reclaim classification, persistent FVG zones with
//! fill tracking, and a lightweight correlator that links BOS/CHOCH, liquidity, order-block, and
//! FVG-fill events across the crate's *existing*, independently-running detectors
//! ([`super::bos_choch::BosChochEngine`], [`super::liquidity_sweeps::LiquiditySweepEngine`],
//! [`super::order_block::OrderBlockEngine`], [`super::liquidity_fvg::LiquidityFvgEngine`]) —
//! rather than reimplementing their detection logic, this observes their
//! [`super::IndicatorAlert`] streams (every indicator already exposes one) and reports when
//! several independently corroborate the same bar.

use super::kernels::PivotRule;
use std::collections::VecDeque;

use crate::model::Bar;

use super::definition::{IndicatorDefinition, ParamSpec};
use super::liquidity_fvg::LiquidityFvgEngine;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

// ---------------------------------------------------------------------------------------------
// BSL/SSL liquidity pools
// ---------------------------------------------------------------------------------------------

/// Buy-side liquidity (resting above an Equal-High cluster) or sell-side liquidity (resting below
/// an Equal-Low cluster).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityPoolKind {
    Bsl,
    Ssl,
}

/// Lifecycle state of a [`LiquidityPool`]: explicitly distinguishes a stop hunt (price pierced
/// the pool then closed back inside — liquidity taken, no sustained follow-through) from a
/// breakout (price pierced and closed beyond, i.e. sustained), and a subsequent reclaim (a prior
/// breakout later reverses back through the level).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LiquidityPoolState {
    Active,
    StopHunted,
    BrokenThrough,
    Reclaimed,
}

#[derive(Debug, Clone, PartialEq)]
pub struct LiquidityPool {
    pub kind: LiquidityPoolKind,
    pub price: f64,
    /// How many near-equal pivots contributed to this pool (Equal-High/Low cluster size).
    pub touches: u32,
    pub formed_at: i64,
    pub state: LiquidityPoolState,
}

/// Detects BSL/SSL pools from Equal High/Low pivot clusters and classifies every interaction as a
/// stop hunt, a breakout, or (for a previously broken pool) a reclaim.
///
/// Over the last `2 · pivot_len + 1` bars (`pivot_len` at least 2) the middle bar is a pivot high
/// when no other bar's high is above its own and a pivot low when no other low is below its own;
/// ties count. A pivot high joins the first active BSL pool whose price lies within
/// `tolerance_pct` percent of it (relative to the pool's price): the pool gains a touch and moves
/// to `(pool + pivot) / 2`. Without such a pool the pivot forms a new one. Pivot lows form SSL
/// pools the same way.
///
/// Then every pool is checked against the current bar, including one formed on this bar. An
/// active BSL pool whose price the high exceeds is stop-hunted when the close is back below it
/// and broken through otherwise; a broken-through BSL pool is reclaimed once a close falls back
/// below it. SSL pools mirror this with the low and closes above. `value`: the number of active
/// pools. First output with bar `2 · pivot_len + 1`.
pub struct LiquidityPoolEngine {
    pivot_len: usize,
    tolerance_pct: f64,
    bars: VecDeque<Bar>,
    pools: Vec<LiquidityPool>,
    alerts: Vec<IndicatorAlert>,
}

impl LiquidityPoolEngine {
    pub fn new(pivot_len: usize, tolerance_pct: f64) -> Self {
        let pivot_len = pivot_len.max(2);
        Self {
            pivot_len,
            tolerance_pct: tolerance_pct.max(0.001),
            bars: VecDeque::with_capacity(pivot_len * 2 + 1),
            pools: Vec::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 0.2)
    }

    pub fn pools(&self) -> &[LiquidityPool] {
        &self.pools
    }

    fn register_pivot(&mut self, kind: LiquidityPoolKind, price: f64, timestamp: i64) {
        let tol = self.tolerance_pct / 100.0;
        let existing = self.pools.iter_mut().find(|p| {
            p.kind == kind
                && p.state == LiquidityPoolState::Active
                && p.price != 0.0
                && (p.price - price).abs() / p.price.abs() <= tol
        });
        match existing {
            Some(pool) => {
                pool.touches += 1;
                pool.price = (pool.price + price) / 2.0;
            }
            None => self.pools.push(LiquidityPool {
                kind,
                price,
                touches: 1,
                formed_at: timestamp,
                state: LiquidityPoolState::Active,
            }),
        }
    }
}

impl Indicator for LiquidityPoolEngine {
    fn name(&self) -> &str {
        "liquidity_pools"
    }

    fn warmup_period(&self) -> usize {
        self.pivot_len * 2 + 1
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.pools.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();

        self.bars.push_back(bar.clone());
        if self.bars.len() > self.pivot_len * 2 + 1 {
            self.bars.pop_front();
        }
        if self.bars.len() < self.pivot_len * 2 + 1 {
            return None;
        }

        let mid_idx = self.pivot_len;
        let mid_bar = self.bars[mid_idx].clone();

        let rule = PivotRule::symmetric(self.pivot_len, false);
        let is_pivot_high = rule.holds(false, self.bars.iter().map(|b| b.high));
        let is_pivot_low = rule.holds(true, self.bars.iter().map(|b| b.low));

        if is_pivot_high {
            self.register_pivot(LiquidityPoolKind::Bsl, mid_bar.high, mid_bar.timestamp);
        }
        if is_pivot_low {
            self.register_pivot(LiquidityPoolKind::Ssl, mid_bar.low, mid_bar.timestamp);
        }

        for pool in &mut self.pools {
            match (pool.kind, pool.state) {
                (LiquidityPoolKind::Bsl, LiquidityPoolState::Active) if bar.high > pool.price => {
                    if bar.close < pool.price {
                        pool.state = LiquidityPoolState::StopHunted;
                        self.alerts.push(IndicatorAlert::new(
                            "bear_pool_stop_hunt",
                            format!(
                                "BSL pool at {:.4} swept and reclaimed (stop hunt)",
                                pool.price
                            ),
                            0.85,
                        ));
                    } else {
                        pool.state = LiquidityPoolState::BrokenThrough;
                        self.alerts.push(IndicatorAlert::new(
                            "bull_pool_breakout",
                            format!(
                                "BSL pool at {:.4} broken through (sustained breakout)",
                                pool.price
                            ),
                            0.7,
                        ));
                    }
                }
                (LiquidityPoolKind::Ssl, LiquidityPoolState::Active) if bar.low < pool.price => {
                    if bar.close > pool.price {
                        pool.state = LiquidityPoolState::StopHunted;
                        self.alerts.push(IndicatorAlert::new(
                            "bull_pool_stop_hunt",
                            format!(
                                "SSL pool at {:.4} swept and reclaimed (stop hunt)",
                                pool.price
                            ),
                            0.85,
                        ));
                    } else {
                        pool.state = LiquidityPoolState::BrokenThrough;
                        self.alerts.push(IndicatorAlert::new(
                            "bear_pool_breakout",
                            format!(
                                "SSL pool at {:.4} broken through (sustained breakout)",
                                pool.price
                            ),
                            0.7,
                        ));
                    }
                }
                (LiquidityPoolKind::Bsl, LiquidityPoolState::BrokenThrough)
                    if bar.close < pool.price =>
                {
                    pool.state = LiquidityPoolState::Reclaimed;
                    self.alerts.push(IndicatorAlert::new(
                        "bear_pool_reclaim",
                        format!(
                            "BSL breakout at {:.4} reclaimed (failed breakout)",
                            pool.price
                        ),
                        0.75,
                    ));
                }
                (LiquidityPoolKind::Ssl, LiquidityPoolState::BrokenThrough)
                    if bar.close > pool.price =>
                {
                    pool.state = LiquidityPoolState::Reclaimed;
                    self.alerts.push(IndicatorAlert::new(
                        "bull_pool_reclaim",
                        format!(
                            "SSL breakout at {:.4} reclaimed (failed breakout)",
                            pool.price
                        ),
                        0.75,
                    ));
                }
                _ => {}
            }
        }

        let active_count = self
            .pools
            .iter()
            .filter(|p| p.state == LiquidityPoolState::Active)
            .count();
        Some(IndicatorOutput::new(active_count as f64))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

// ---------------------------------------------------------------------------------------------
// FVG zones with fill tracking
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct FvgZone {
    pub is_bullish: bool,
    pub top: f64,
    pub bottom: f64,
    pub formed_at: i64,
    pub filled: bool,
}

/// Tracks Fair Value Gap zones as persistent objects and marks them filled once price trades back
/// through them — the "FVG-Fill" lifecycle no existing FVG detector in this crate tracks (they
/// only emit a one-shot creation alert).
#[derive(Debug, Clone, Default)]
pub struct FvgZoneTracker {
    zones: Vec<FvgZone>,
}

impl FvgZoneTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Registers a newly detected FVG (e.g. from [`super::liquidity_fvg::LiquidityFvgEngine`]'s
    /// per-bar gap output).
    pub fn register(&mut self, is_bullish: bool, top: f64, bottom: f64, formed_at: i64) {
        self.zones.push(FvgZone {
            is_bullish,
            top,
            bottom,
            formed_at,
            filled: false,
        });
    }

    pub fn zones(&self) -> &[FvgZone] {
        &self.zones
    }

    pub fn reset(&mut self) {
        self.zones.clear();
    }

    /// Drops the filled zones and keeps only the `max_open` newest unfilled ones.
    pub(crate) fn prune(&mut self, max_open: usize) {
        self.zones.retain(|zone| !zone.filled);
        if self.zones.len() > max_open {
            self.zones.drain(..self.zones.len() - max_open);
        }
    }

    /// Feeds a bar, marking any unfilled zone the bar traded back into as filled. Returns the
    /// zones newly filled this bar. Only bars after a zone's `formed_at` can fill it: the bar
    /// that completes the gap borders it by construction and is not a return into it.
    pub fn on_bar(&mut self, bar: &Bar) -> Vec<&FvgZone> {
        let mut newly_filled_indices = Vec::new();
        for (i, zone) in self.zones.iter_mut().enumerate() {
            if zone.filled || bar.timestamp <= zone.formed_at {
                continue;
            }
            let touched = if zone.is_bullish {
                bar.low <= zone.top
            } else {
                bar.high >= zone.bottom
            };
            if touched {
                zone.filled = true;
                newly_filled_indices.push(i);
            }
        }
        newly_filled_indices
            .iter()
            .map(|&i| &self.zones[i])
            .collect()
    }
}

/// The catalog entry `fvg_fill`: the fair value gaps of [`LiquidityFvgEngine`] (a gap is open
/// between the high of bar t-2 and the low of bar t, bullish, or the reverse, bearish) followed as
/// zones by an [`FvgZoneTracker`]. A zone is filled by the first later bar that trades back into
/// it: a bullish zone when the low reaches its upper edge, a bearish zone when the high reaches
/// its lower edge (the bar that forms a gap never fills it; later is decided by the bar
/// timestamp, so timestamps have to increase). `value` is the number of zones still open after
/// the bar; `extra` has `created` (1 bullish, -1 bearish, 0 none), `filled` (zones filled on this
/// bar), `open_bullish` and `open_bearish`. Only the newest `max_zones` open zones are kept, and
/// the counts are those that remain. There is an output on every bar. [`Indicator::reset`] starts over.
pub struct FvgFillEngine {
    max_zones: usize,
    fvg: LiquidityFvgEngine,
    tracker: FvgZoneTracker,
}

impl FvgFillEngine {
    pub fn new(max_zones: usize) -> Self {
        Self {
            max_zones,
            fvg: LiquidityFvgEngine::new(FVG_LOOKBACK),
            tracker: FvgZoneTracker::new(),
        }
    }
}

/// The gap rule does not depend on the sweep lookback; any length that keeps three bars will do.
const FVG_LOOKBACK: usize = 3;

impl Indicator for FvgFillEngine {
    fn name(&self) -> &str {
        "fvg_fill"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.fvg.reset();
        self.tracker.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let created = self.fvg.on_bar(bar)?.value;
        if created != 0.0 {
            let zone = self
                .fvg
                .zones()
                .last()
                .expect("a gap found on this bar is the newest zone");
            self.tracker
                .register(zone.is_bullish, zone.top, zone.bottom, bar.timestamp);
        }
        let filled = self.tracker.on_bar(bar).len();
        self.tracker.prune(self.max_zones);
        let open = self.tracker.zones().len();
        let open_bullish = self.tracker.zones().iter().filter(|z| z.is_bullish).count();
        let open_bearish = open - open_bullish;
        let mut output = IndicatorOutput::new(open as f64);
        for (key, value) in [
            ("created", created),
            ("filled", filled as f64),
            ("open_bullish", open_bullish as f64),
            ("open_bearish", open_bearish as f64),
        ] {
            output.extra.insert(key.to_string(), value);
        }
        Some(output)
    }
}

/// Registry definition of `fvg_fill`.
pub const FVG_FILL_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "fvg_fill",
    aliases: &[],
    description: "Fair value gaps followed as zones until a later bar trades back into them: the number of open zones as value, with created (1 bullish, -1 bearish gap on this bar), filled, open_bullish and open_bearish as extras. Purpose: gaps as objects with a lifetime, from creation to the first return, instead of a one-shot event. Use: where open gaps lie and when price comes back to them. Caveat: a fill is a touch, not a reaction; a resting order inside a zone is filled on every return, also on those that run straight through it, so read the close of the returning bars before treating a fill as a turn",
    params: &[ParamSpec::count("max_zones", 20, 1, 1000)
        .meaning("newest open zones that are kept")],
    checks: &[],
    facts: crate::indicator::definition::IndicatorFacts::unitless().range_dependent(),
    verification: crate::indicator::definition::Verification::Open(
        "no independent second implementation yet",
    ),
    build: |p| Ok(Box::new(FvgFillEngine::new(p.count("max_zones")))),
};

// ---------------------------------------------------------------------------------------------
// Cross-detector event linker
// ---------------------------------------------------------------------------------------------

/// Which structural family an [`IndicatorAlert::kind`] belongs to, for correlation purposes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StructureCategory {
    Break,
    Liquidity,
    OrderBlock,
    Fvg,
}

fn categorize(kind: &str) -> Option<StructureCategory> {
    match kind {
        "bull_bos" | "bear_bos" | "bull_choch" | "bear_choch" => Some(StructureCategory::Break),
        "bull_sweep"
        | "bear_sweep"
        | "bull_pool_stop_hunt"
        | "bear_pool_stop_hunt"
        | "bull_pool_breakout"
        | "bear_pool_breakout"
        | "bull_pool_reclaim"
        | "bear_pool_reclaim"
        | "bullish_liquidity_sweep"
        | "bearish_liquidity_sweep" => Some(StructureCategory::Liquidity),
        "bullish_order_block"
        | "bearish_order_block"
        | "ob_retest_bullish"
        | "ob_retest_bearish" => Some(StructureCategory::OrderBlock),
        "bullish_fvg" | "bearish_fvg" | "fvg_filled" => Some(StructureCategory::Fvg),
        _ => None,
    }
}

/// A confirmed cross-detector confluence: a structure break (BOS/CHOCH) co-occurring, within the
/// linker's window, with corroborating evidence from at least one other family.
#[derive(Debug, Clone, PartialEq)]
pub struct LinkedStructureEvent {
    pub timestamp: i64,
    pub categories: Vec<StructureCategory>,
    /// `categories.len() / 4.0`, capped at `1.0`: how many of the four families corroborate.
    pub confluence_score: f64,
    pub kinds: Vec<String>,
}

/// Correlates [`IndicatorAlert`] streams from independently-running detectors within a trailing
/// bar window, without depending on their internal types — any indicator's alerts can feed this
/// via [`SmartMoneyStructureLinker::observe`].
pub struct SmartMoneyStructureLinker {
    window_bars: i64,
    events: VecDeque<(i64, String)>,
}

impl SmartMoneyStructureLinker {
    pub fn new(window_bars: i64) -> Self {
        Self {
            window_bars: window_bars.max(1),
            events: VecDeque::new(),
        }
    }

    pub fn reset(&mut self) {
        self.events.clear();
    }

    /// Records `alerts` at `timestamp` and prunes anything older than the window.
    pub fn observe(&mut self, timestamp: i64, alerts: &[IndicatorAlert]) {
        for alert in alerts {
            self.events.push_back((timestamp, alert.kind.clone()));
        }
        while self
            .events
            .front()
            .map(|(ts, _)| timestamp - ts > self.window_bars)
            .unwrap_or(false)
        {
            self.events.pop_front();
        }
    }

    /// Call after [`SmartMoneyStructureLinker::observe`] for `timestamp`: if a structure-break
    /// event occurred exactly at `timestamp`, checks whether other families co-occurred within
    /// the trailing window and returns the linked confluence event.
    pub fn check_confluence(&self, timestamp: i64) -> Option<LinkedStructureEvent> {
        let break_now = self.events.iter().any(|(ts, kind)| {
            *ts == timestamp && categorize(kind) == Some(StructureCategory::Break)
        });
        if !break_now {
            return None;
        }

        let mut categories: Vec<StructureCategory> = self
            .events
            .iter()
            .filter_map(|(_, kind)| categorize(kind))
            .collect();
        categories.sort();
        categories.dedup();

        if categories.len() < 2 {
            return None;
        }

        let kinds: Vec<String> = self.events.iter().map(|(_, k)| k.clone()).collect();
        let confluence_score = (categories.len() as f64 / 4.0).min(1.0);

        Some(LinkedStructureEvent {
            timestamp,
            categories,
            confluence_score,
            kinds,
        })
    }
}

pub const LIQUIDITY_POOLS_DEFINITION: super::definition::IndicatorDefinition = super::definition::IndicatorDefinition {
    name: "liquidity_pools",
    aliases: &[],
    description: "BSL/SSL liquidity pools with explicit stop-hunt/breakout/reclaim classification (see also the `fvg_fill` entry for FVG-fill tracking and SmartMoneyStructureLinker for cross-detector confluence)",
    params: &[
        super::definition::ParamSpec::count("pivot_len", 5, 2, 10000).meaning("bars on each side of a pivot"),
        super::definition::ParamSpec::number("tolerance_pct", 0.2, 0.001, 100.0).meaning("distance, in percent, within which pivots join one pool"),
    ],
    checks: &[],
    facts: super::definition::IndicatorFacts::unitless().range_dependent(),
    verification: super::definition::Verification::Verified,
    build: |p| Ok(Box::new(LiquidityPoolEngine::new(p.count("pivot_len"), p.number("tolerance_pct")))),
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fvg_is_not_filled_by_the_bar_that_forms_it() {
        let mut tracker = FvgZoneTracker::new();
        tracker.register(true, 105.0, 100.0, 120);
        assert!(tracker
            .on_bar(&Bar::new(120, 106.0, 110.0, 105.0, 109.0, 1.0))
            .is_empty());
        assert_eq!(
            tracker
                .on_bar(&Bar::new(180, 108.0, 109.0, 104.0, 106.0, 1.0))
                .len(),
            1
        );
    }

    fn trending_bars(n: usize, step: f64) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let base = 100.0 + i as f64 * step;
                Bar::new(
                    i as i64 * 60,
                    base,
                    base + 3.0,
                    base - 3.0,
                    base + 1.0,
                    100.0,
                )
            })
            .collect()
    }

    #[test]
    fn test_liquidity_pool_stop_hunt_vs_breakout_are_distinguished() {
        let mut engine = LiquidityPoolEngine::new(2, 0.1);
        // Build up bars to form a swing high pivot around 110.
        let bars = vec![
            Bar::new(0, 100.0, 105.0, 99.0, 100.0, 10.0),
            Bar::new(60, 100.0, 108.0, 99.0, 100.0, 10.0),
            Bar::new(120, 100.0, 110.0, 99.0, 100.0, 10.0), // pivot high
            Bar::new(180, 100.0, 106.0, 99.0, 100.0, 10.0),
            Bar::new(240, 100.0, 104.0, 99.0, 100.0, 10.0),
        ];
        for bar in &bars {
            engine.on_bar(bar);
        }
        assert!(
            !engine.pools().is_empty(),
            "a BSL pool must have formed at the swing high"
        );

        // Stop hunt: pierce above 110 but close back below it.
        let hunt = engine.on_bar(&Bar::new(300, 100.0, 111.0, 99.0, 105.0, 10.0));
        assert!(hunt.is_some());
        assert!(engine
            .pools()
            .iter()
            .any(|p| p.state == LiquidityPoolState::StopHunted));
        assert!(
            engine
                .alerts()
                .iter()
                .any(|a| a.kind == "bear_pool_stop_hunt"),
            "a hunted BSL pool (highs) turns down"
        );
    }

    #[test]
    fn test_liquidity_pool_breakout_and_reclaim() {
        let mut engine = LiquidityPoolEngine::new(2, 0.1);
        let bars = vec![
            Bar::new(0, 100.0, 105.0, 99.0, 100.0, 10.0),
            Bar::new(60, 100.0, 108.0, 99.0, 100.0, 10.0),
            Bar::new(120, 100.0, 110.0, 99.0, 100.0, 10.0),
            Bar::new(180, 100.0, 106.0, 99.0, 100.0, 10.0),
            Bar::new(240, 100.0, 104.0, 99.0, 100.0, 10.0),
        ];
        for bar in &bars {
            engine.on_bar(bar);
        }

        // Breakout: pierce and close above 110 — with the break, up.
        engine.on_bar(&Bar::new(300, 100.0, 112.0, 99.0, 111.0, 10.0));
        assert!(engine
            .alerts()
            .iter()
            .any(|a| a.kind == "bull_pool_breakout"));
        assert!(engine
            .pools()
            .iter()
            .any(|p| p.state == LiquidityPoolState::BrokenThrough));

        // Reclaim: price reverses back below the broken level.
        let reclaim = engine.on_bar(&Bar::new(360, 111.0, 111.5, 108.0, 109.0, 10.0));
        assert!(reclaim.is_some());
        assert!(engine
            .pools()
            .iter()
            .any(|p| p.state == LiquidityPoolState::Reclaimed));
        assert!(
            engine
                .alerts()
                .iter()
                .any(|a| a.kind == "bear_pool_reclaim"),
            "a failed BSL breakout turns down"
        );
    }

    #[test]
    fn test_fvg_zone_tracker_marks_fill() {
        let mut tracker = FvgZoneTracker::new();
        tracker.register(true, 105.0, 100.0, 0);
        assert!(!tracker.zones()[0].filled);

        // Price stays above the gap: not filled yet.
        tracker.on_bar(&Bar::new(60, 110.0, 112.0, 108.0, 111.0, 10.0));
        assert!(!tracker.zones()[0].filled);

        // Price trades back down into the gap zone [100, 105].
        let filled = tracker.on_bar(&Bar::new(120, 106.0, 107.0, 102.0, 103.0, 10.0));
        assert_eq!(filled.len(), 1);
        assert!(tracker.zones()[0].filled);
    }

    #[test]
    fn test_linker_requires_break_plus_corroboration() {
        let mut linker = SmartMoneyStructureLinker::new(5);

        // A structure break alone, no corroboration, must not link.
        linker.observe(10, &[IndicatorAlert::new("bull_bos", "BOS", 0.9)]);
        assert!(linker.check_confluence(10).is_none());

        // A liquidity sweep shortly before a later break must corroborate it.
        let mut linker = SmartMoneyStructureLinker::new(5);
        linker.observe(8, &[IndicatorAlert::new("bull_sweep", "swept", 0.85)]);
        linker.observe(10, &[IndicatorAlert::new("bull_bos", "BOS", 0.9)]);

        let event = linker.check_confluence(10).unwrap();
        assert!(event.categories.contains(&StructureCategory::Break));
        assert!(event.categories.contains(&StructureCategory::Liquidity));
        assert!(event.confluence_score > 0.0);
    }

    #[test]
    fn test_linker_ignores_break_outside_current_bar() {
        let mut linker = SmartMoneyStructureLinker::new(5);
        linker.observe(8, &[IndicatorAlert::new("bull_sweep", "swept", 0.85)]);
        linker.observe(9, &[IndicatorAlert::new("bull_bos", "BOS", 0.9)]);
        // Querying a bar where no break occurred must return None even if other events exist.
        assert!(linker.check_confluence(10).is_none());
    }

    /// A strictly monotonic trend has no local swing high/low: within any fixed-size window, the
    /// highest high is always the window's newest bar and the lowest low always its oldest, so
    /// the *middle* bar (what the pivot check requires) can never be either. No pivot therefore
    /// means no liquidity pool can legitimately form -- a concrete, provably-true invariant for
    /// this input, not the `X || !X` tautology this test previously asserted (finding 07). The
    /// loop itself is still what guards against a panic across a long single-direction run.
    #[test]
    fn test_no_panic_and_no_spurious_pools_across_a_strictly_monotonic_trend() {
        let mut engine = LiquidityPoolEngine::with_defaults();
        for bar in trending_bars(60, 1.5) {
            engine.on_bar(&bar);
        }
        assert!(
            engine.pools().is_empty(),
            "a strictly monotonic trend has no swing pivots and must not form any liquidity pool"
        );
    }
}
