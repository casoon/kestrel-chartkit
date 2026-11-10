//! Wyckoff accumulation/distribution state machine: a trading range set by a climax and the
//! counter-move after it, Phases A-E, and the events of the schematic (PS, SC, AR, ST, Spring, SOS,
//! LPS and their distribution mirrors).
//!
//! This is a codified interpretation of the textbook Wyckoff method, not a claim of canonical
//! Wyckoff analysis: the literature describes the events qualitatively and names no thresholds.
//! Every number below is therefore a decision of this crate, exposed as a named parameter of
//! [`WyckoffParams`]; every structural rule that the literature leaves open is marked
//! "Convention".
//!
//! # The calculation
//!
//! Accumulation is written out; distribution is the exact mirror image (the same rules on the
//! series with every price negated, so a Selling Climax becomes a Buying Climax, a Spring a UTAD
//! and so on). All comparisons are causal: a bar only reads itself and earlier bars.
//!
//! Per bar, once `range_lookback` bars and a positive ATR exist:
//!
//! ```text
//! atr       = Wilder average of the true range over atr_len bars
//! window    = the last range_lookback bars, the current one included
//! threshold = median(volume in window) + volume_k * 1.4826 * MAD(volume in window)
//!             none while MAD = 0: without dispersion no volume is an outlier
//! spread    = high - low
//!
//! selling climax bar:  spread >= climax_range_atr * atr     (a wide bar)
//!                      and volume > threshold               (a volume outlier)
//!                      and close < open
//!                      and low <= lowest low of the window  (it ends a decline)
//! ```
//!
//! **Setting the range.** Searched only on a bar that began without a range or in Phase E and
//! that the rules of that phase left there. A bar that reports an event, changes the phase or
//! abandons a range therefore never sets a range or marks a climax as pending; the search starts
//! with the bar after it.
//!
//! ```text
//! a selling climax bar s becomes the pending climax (it replaces an earlier one)
//!     preliminary support = the first bar of the window before s with
//!                           volume > threshold of bar s  and  close > open
//! on every later bar j, in this order:
//!     j - s >= range_lookback          -> the climax is dropped (no rally followed in time)
//!     low[j] < low[s]                  -> the climax is dropped (the decline went on)
//!     high[j] > rally high so far      -> bar j is the rally high r
//!     close[j] < low[r] and high[r] > high[s]
//!                                      -> the Automatic Rally is confirmed, the range is set:
//!                                         range_low = low[s], range_high = high[r],
//!                                         bias accumulation, Phase A,
//!                                         events PS (if found), SC and AR at their own bars
//! width fallback, only while no climax is pending:
//!     highest high - lowest low of the window <= range_atr_max * atr
//!                                      -> range_high and range_low are the window extremes,
//!                                         bias open, Phase A, no event
//! ```
//!
//! The boundaries are fixed once set. The bar that sets a range runs none of the rules below.
//!
//! **Inside the range**, one rule set per phase, evaluated in the order written:
//!
//! ```text
//! Phase A and B
//!     Spring:  low < range_low and close > range_low, not also a UTAD of the same bar,
//!              bias not distribution, at least min_range_bars bars after the range was set
//!              -> bias accumulation, Phase C
//!     abandon: close > range_high + abandon_atr * atr  or  close < range_low - abandon_atr * atr
//!              -> no range
//!     Secondary Test:  range_low <= low <= range_low + st_atr * atr
//!              and, for a range set by a climax, volume < volume[s] and spread < spread[s]
//!              -> Phase B
//!              (bias open: the test may be at either boundary)
//! Phase C
//!     Sign of Strength:  close > range_high, and volume > threshold unless breakout_volume = off
//!              -> Phase D
//!     abandon: as in Phase A and B
//! Phase D
//!     close < range_high                         -> Phase B (the breakout failed)
//!     a pullback runs from the first bar that closes below the previous close;
//!         its low is the lowest low since
//!     on the first bar of a running pullback that closes above the previous close:
//!         pullback low >= range_high - lps_atr * atr
//!                                                -> Last Point of Support at the bar of the
//!                                                   pullback low, Phase E
//!         otherwise the pullback is over and the next lower close starts a new one
//! Phase E
//!     close < range_high                         -> Phase B
//!     a new range set by the search above        -> Phase A of the new range
//! ```
//!
//! # Output
//!
//! `value` is the phase: 0 none, 1 A, 2 B, 3 C, 4 D, 5 E. The bar of the event that ends a phase
//! already carries the next phase. `extra`:
//!
//! | key | present | meaning |
//! |---|---|---|
//! | `atr` | always | the ATR of the calculation, in price units (the registry `atr` reports the same number as `extra["raw"]`; its `value` is that ATR in percent of the close) |
//! | `volume_threshold` | while the volumes of the window vary (MAD > 0) | the volume a bar must exceed to count as an outlier. Absent while there is no threshold: no bar is an outlier then |
//! | `climax` | always | the climax bar test on this bar alone: -1 selling climax bar, 1 buying climax bar, 0 neither. Whether such a bar becomes the climax of a range depends on what follows |
//! | `bias` | always | 1 accumulation, -1 distribution, 0 open or no range |
//! | `range_source` | always | 0 no range, 1 set by climax and counter-move, 2 set by the width fallback |
//! | `range_high`, `range_low` | while a range is set | its boundaries |
//! | `cause_score` | while a range is set | range width times the bars spent in Phases A to D |
//! | `event_<kind>` | on the bar that reports the event | how many bars back the event lies (0: this bar). `<kind>` is the lower-case [`WyckoffEventKind`], e.g. `event_sellingclimax` |
//!
//! PS, SC and AR are reported together on the bar that confirms the Automatic Rally, and a Last
//! Point of Support on the bar that turns the pullback; their `event_` values point back at the
//! bars they lie on. Every other event lies on the bar that reports it. Each reported event also
//! raises the alert `wyckoff_<kind>`.
//!
//! # Series without volume, or without dispersion in it
//!
//! Without dispersion there is no outlier. The MAD of the window is zero when more than half of
//! its volumes are one and the same value: a series without volume (all zero), a constant volume,
//! but also mostly identical values with a few different ones. On such a bar there is no
//! threshold, however high the bar's own volume is: it is no climax bar, no Preliminary
//! Support/Supply is found from it, and it is no Sign of Strength or Weakness unless
//! `breakout_volume = off`. `volume_threshold` is absent from `extra` on these bars.
//!
//! A series that never has dispersion finds no climax, so a range can only be set by the width
//! fallback with an open bias; with the defaults it runs to Phase C at most.

use std::collections::{HashMap, VecDeque};

use crate::clustering::RollingRobustThreshold;
use crate::model::Bar;

use super::smoothing::Rma;
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// Which side of the cycle this range is developing into: accumulation (basing before markup) or
/// distribution (topping before markdown).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffBias {
    Accumulation,
    Distribution,
}

impl WyckoffBias {
    /// A price as the accumulation rules read it: unchanged for accumulation, negated for
    /// distribution. Negation is exact, so the two sides mirror each other bit for bit. The
    /// mapping is its own inverse.
    fn orient(self, price: f64) -> f64 {
        match self {
            WyckoffBias::Accumulation => price,
            WyckoffBias::Distribution => -price,
        }
    }

    fn orient_bar(self, bar: &Bar) -> Oriented {
        match self {
            WyckoffBias::Accumulation => Oriented {
                open: bar.open,
                high: bar.high,
                low: bar.low,
                close: bar.close,
            },
            WyckoffBias::Distribution => Oriented {
                open: -bar.open,
                high: -bar.low,
                low: -bar.high,
                close: -bar.close,
            },
        }
    }

    fn code(self) -> f64 {
        match self {
            WyckoffBias::Accumulation => 1.0,
            WyckoffBias::Distribution => -1.0,
        }
    }
}

/// A bar's prices seen from one side (see [`WyckoffBias::orient`]).
#[derive(Debug, Clone, Copy)]
struct Oriented {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffPhase {
    /// No range set.
    Undefined,
    /// A range is set: by a climax and the counter-move after it (Selling Climax and Automatic
    /// Rally, or Buying Climax and Automatic Reaction), or by the width fallback. Lasts until the
    /// first Secondary Test.
    A,
    /// Range building, from the first Secondary Test on: further tests of the boundary.
    B,
    /// A Spring (accumulation) or UTAD (distribution) has occurred: the decisive test.
    C,
    /// A Sign of Strength/Weakness has closed beyond the *opposite* boundary.
    D,
    /// A Last Point of Support/Supply held: the move out of the range runs (Markup/Markdown).
    /// Ends when price closes back inside the range (Phase B) or a new range is set (Phase A).
    E,
}

impl WyckoffPhase {
    fn code(self) -> f64 {
        match self {
            WyckoffPhase::Undefined => 0.0,
            WyckoffPhase::A => 1.0,
            WyckoffPhase::B => 2.0,
            WyckoffPhase::C => 3.0,
            WyckoffPhase::D => 4.0,
            WyckoffPhase::E => 5.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffEventKind {
    PreliminarySupport,
    PreliminarySupply,
    SellingClimax,
    BuyingClimax,
    AutomaticRally,
    AutomaticReaction,
    SecondaryTest,
    Spring,
    Utad,
    SignOfStrength,
    SignOfWeakness,
    LastPointOfSupport,
    LastPointOfSupply,
}

impl WyckoffEventKind {
    /// The order the schematic expects, both sides interleaved.
    const SCHEMATIC_ORDER: [WyckoffEventKind; 13] = [
        WyckoffEventKind::PreliminarySupport,
        WyckoffEventKind::PreliminarySupply,
        WyckoffEventKind::SellingClimax,
        WyckoffEventKind::BuyingClimax,
        WyckoffEventKind::AutomaticRally,
        WyckoffEventKind::AutomaticReaction,
        WyckoffEventKind::SecondaryTest,
        WyckoffEventKind::Spring,
        WyckoffEventKind::Utad,
        WyckoffEventKind::SignOfStrength,
        WyckoffEventKind::SignOfWeakness,
        WyckoffEventKind::LastPointOfSupport,
        WyckoffEventKind::LastPointOfSupply,
    ];

    /// Fixed alert strength per kind.
    fn strength(self) -> f64 {
        match self {
            WyckoffEventKind::PreliminarySupport | WyckoffEventKind::PreliminarySupply => 0.3,
            WyckoffEventKind::SellingClimax | WyckoffEventKind::BuyingClimax => 0.7,
            WyckoffEventKind::AutomaticRally | WyckoffEventKind::AutomaticReaction => 0.5,
            WyckoffEventKind::SecondaryTest => 0.4,
            WyckoffEventKind::Spring | WyckoffEventKind::Utad => 0.85,
            WyckoffEventKind::SignOfStrength | WyckoffEventKind::SignOfWeakness => 0.8,
            WyckoffEventKind::LastPointOfSupport | WyckoffEventKind::LastPointOfSupply => 0.9,
        }
    }

    fn note(self) -> &'static str {
        match self {
            WyckoffEventKind::PreliminarySupport => {
                "Wyckoff Preliminary Support: outlier volume absorbed before the climax"
            }
            WyckoffEventKind::PreliminarySupply => {
                "Wyckoff Preliminary Supply: outlier volume absorbed before the climax"
            }
            WyckoffEventKind::SellingClimax => {
                "Wyckoff Selling Climax: wide bar on outlier volume set the range low"
            }
            WyckoffEventKind::BuyingClimax => {
                "Wyckoff Buying Climax: wide bar on outlier volume set the range high"
            }
            WyckoffEventKind::AutomaticRally => {
                "Wyckoff Automatic Rally: the counter-move set the range high"
            }
            WyckoffEventKind::AutomaticReaction => {
                "Wyckoff Automatic Reaction: the counter-move set the range low"
            }
            WyckoffEventKind::SecondaryTest => "Wyckoff Secondary Test of range boundary",
            WyckoffEventKind::Spring => "Wyckoff Spring: range low swept and reclaimed",
            WyckoffEventKind::Utad => "Wyckoff UTAD: range high swept and reclaimed",
            WyckoffEventKind::SignOfStrength => {
                "Wyckoff Sign of Strength: closed beyond range high"
            }
            WyckoffEventKind::SignOfWeakness => "Wyckoff Sign of Weakness: closed beyond range low",
            WyckoffEventKind::LastPointOfSupport => {
                "Wyckoff Last Point of Support: pullback held, Markup confirmed"
            }
            WyckoffEventKind::LastPointOfSupply => {
                "Wyckoff Last Point of Supply: pullback held, Markdown confirmed"
            }
        }
    }

    /// Lower-case name: the suffix of the alert kind (`wyckoff_<slug>`) and of the `extra` key
    /// (`event_<slug>`).
    fn slug(self) -> String {
        format!("{self:?}").to_lowercase()
    }
}

/// The event kinds of one side, in the order of the schematic.
struct SideKinds {
    preliminary: WyckoffEventKind,
    climax: WyckoffEventKind,
    reaction: WyckoffEventKind,
    test: WyckoffEventKind,
    breakout: WyckoffEventKind,
    hold: WyckoffEventKind,
}

fn kinds(side: WyckoffBias) -> SideKinds {
    match side {
        WyckoffBias::Accumulation => SideKinds {
            preliminary: WyckoffEventKind::PreliminarySupport,
            climax: WyckoffEventKind::SellingClimax,
            reaction: WyckoffEventKind::AutomaticRally,
            test: WyckoffEventKind::Spring,
            breakout: WyckoffEventKind::SignOfStrength,
            hold: WyckoffEventKind::LastPointOfSupport,
        },
        WyckoffBias::Distribution => SideKinds {
            preliminary: WyckoffEventKind::PreliminarySupply,
            climax: WyckoffEventKind::BuyingClimax,
            reaction: WyckoffEventKind::AutomaticReaction,
            test: WyckoffEventKind::Utad,
            breakout: WyckoffEventKind::SignOfWeakness,
            hold: WyckoffEventKind::LastPointOfSupply,
        },
    }
}

/// An event of the current range. `timestamp` and `price` are those of the bar the event lies on,
/// which for PS, SC, AR and LPS/LPSY is earlier than the bar that reported it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffEvent {
    pub kind: WyckoffEventKind,
    pub price: f64,
    pub timestamp: i64,
}

/// Heuristic scoring of the developing range.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffScore {
    /// Range width x bars spent in Phases A to D (the point-and-figure "cause" building over
    /// time) — larger implies a larger implied subsequent move once the range resolves. Frozen in
    /// Phase E, zero without a range.
    pub cause_score: f64,
    /// `1.0` if the events were observed in the order of the schematic; 0.2 less for every event
    /// that follows a later one (e.g. a Secondary Test after a failed breakout).
    pub sequence_quality: f64,
}

/// How the boundaries of a range were set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WyckoffRangeSource {
    /// From the extreme of a climax bar and the extreme of the counter-move after it.
    Climax,
    /// Width fallback: the extremes of a window that was narrow enough, with no climax pending.
    Width,
}

/// The range currently set.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffTradingRange {
    pub high: f64,
    pub low: f64,
    pub source: WyckoffRangeSource,
}

/// Every threshold of the calculation. The Wyckoff literature names none of these numbers; the
/// defaults are decisions of this crate.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffParams {
    /// Bars of the window: volume statistics, the window extremes of the climax test, the life of
    /// a pending climax and the width fallback. Default 20, about a month of daily bars.
    pub range_lookback: usize,
    /// Width fallback: the widest window, in ATR, that is set as a range while no climax is
    /// pending. Default 3.
    pub range_atr_max: f64,
    /// Bars a range must exist before a Spring or UTAD counts. Default 6.
    pub min_range_bars: usize,
    /// Bars of the Wilder ATR every distance is measured in. Default 14, Wilder's own length.
    pub atr_len: usize,
    /// Volume outlier: scaled MADs above the window median. Default 2, two robust standard
    /// deviations. While the MAD is zero (more than half of the window's volumes are the same
    /// value) no volume is an outlier.
    pub volume_k: f64,
    /// Climax bar: the smallest high-low spread, in ATR. Default 1.5.
    pub climax_range_atr: f64,
    /// Secondary Test: the largest distance of the bar's extreme from the boundary, in ATR.
    /// Default 0.25.
    pub st_atr: f64,
    /// Whether a Sign of Strength/Weakness needs a volume outlier. Default `true`.
    pub breakout_volume: bool,
    /// Last Point of Support/Supply: how far the pullback may reach back beyond the broken
    /// boundary, in ATR. Default 0.5.
    pub lps_atr: f64,
    /// A close this far outside the range, in ATR, abandons the range in Phases A to C.
    /// Default 1, one average bar.
    pub abandon_atr: f64,
}

impl Default for WyckoffParams {
    fn default() -> Self {
        Self {
            range_lookback: 20,
            range_atr_max: 3.0,
            min_range_bars: 6,
            atr_len: 14,
            volume_k: 2.0,
            climax_range_atr: 1.5,
            st_atr: 0.25,
            breakout_volume: true,
            lps_atr: 0.5,
            abandon_atr: 1.0,
        }
    }
}

/// A bar an event lies on. `index` counts the bars fed so far.
#[derive(Debug, Clone, Copy)]
struct Marker {
    index: u64,
    timestamp: i64,
    price: f64,
}

/// The counter-move after a pending climax, in the climax's frame: its highest high so far and
/// the low of the bar that made it.
#[derive(Debug, Clone, Copy)]
struct CounterMove {
    bar: Marker,
    high: f64,
    low: f64,
}

/// A climax bar whose counter-move is not confirmed yet. `low` and `high` are those of the climax
/// bar in the frame of `side`.
#[derive(Debug, Clone, Copy)]
struct PendingClimax {
    side: WyckoffBias,
    bar: Marker,
    low: f64,
    high: f64,
    volume: f64,
    spread: f64,
    preliminary: Option<Marker>,
    counter: Option<CounterMove>,
}

#[derive(Debug, Clone, Copy)]
struct ActiveRange {
    high: f64,
    low: f64,
    source: WyckoffRangeSource,
    bias: Option<WyckoffBias>,
    /// Volume and spread of the climax bar, for a range set by a climax.
    climax: Option<(f64, f64)>,
}

impl ActiveRange {
    /// The boundary the test of `side` pierces and the one its breakout crosses, in that side's
    /// frame.
    fn bounds(&self, side: WyckoffBias) -> (f64, f64) {
        match side {
            WyckoffBias::Accumulation => (self.low, self.high),
            WyckoffBias::Distribution => (-self.high, -self.low),
        }
    }

    /// Whether `close` lies more than `tolerance` outside the range.
    fn left_behind(&self, close: f64, tolerance: f64) -> bool {
        close > self.high + tolerance || close < self.low - tolerance
    }
}

pub struct WyckoffStateMachine {
    params: WyckoffParams,
    atr: Rma,
    prev_close: Option<f64>,
    volume_threshold: RollingRobustThreshold,
    bars: VecDeque<Bar>,
    /// Bars fed so far.
    index: u64,
    bars_in_range: u32,
    range: Option<ActiveRange>,
    phase: WyckoffPhase,
    pending: Option<PendingClimax>,
    /// Phase D: the lowest low (in the bias frame) of the pullback that is running.
    pullback: Option<Marker>,
    events: Vec<WyckoffEvent>,
    /// Events reported on the current bar, with the number of bars they lie back.
    reported: Vec<(WyckoffEventKind, u64)>,
    alerts: Vec<IndicatorAlert>,
}

impl WyckoffStateMachine {
    /// The three thresholds this type has always taken; every other one at its default (see
    /// [`WyckoffParams`]).
    pub fn new(range_lookback: usize, range_atr_max: f64, min_range_bars: usize) -> Self {
        Self::with_params(WyckoffParams {
            range_lookback,
            range_atr_max,
            min_range_bars,
            ..WyckoffParams::default()
        })
    }

    pub fn with_params(params: WyckoffParams) -> Self {
        let params = WyckoffParams {
            range_lookback: params.range_lookback.max(3),
            min_range_bars: params.min_range_bars.max(2),
            atr_len: params.atr_len.max(1),
            ..params
        };
        Self {
            params,
            atr: Rma::new(params.atr_len),
            prev_close: None,
            // Same length as the bar deque below: the climax test reads the volume threshold and
            // the window extremes of one and the same window.
            volume_threshold: RollingRobustThreshold::new(params.range_lookback, params.volume_k),
            bars: VecDeque::with_capacity(params.range_lookback),
            index: 0,
            bars_in_range: 0,
            range: None,
            phase: WyckoffPhase::Undefined,
            pending: None,
            pullback: None,
            events: Vec::new(),
            reported: Vec::new(),
            alerts: Vec::new(),
        }
    }

    pub fn with_defaults() -> Self {
        Self::with_params(WyckoffParams::default())
    }

    pub fn params(&self) -> WyckoffParams {
        self.params
    }

    pub fn phase(&self) -> WyckoffPhase {
        self.phase
    }

    /// The bias of the range in play. `None` without a range, and for a range set by the width
    /// fallback until a Spring or UTAD decides it.
    pub fn bias(&self) -> Option<WyckoffBias> {
        self.range.and_then(|r| r.bias)
    }

    /// The range in play, if one is set.
    pub fn range(&self) -> Option<WyckoffTradingRange> {
        self.range.map(|r| WyckoffTradingRange {
            high: r.high,
            low: r.low,
            source: r.source,
        })
    }

    /// Full event history observed for the current (or most recently completed) range.
    pub fn events(&self) -> &[WyckoffEvent] {
        &self.events
    }

    pub fn score(&self) -> WyckoffScore {
        let cause_score = self.cause_score();

        let rank = |k: WyckoffEventKind| {
            WyckoffEventKind::SCHEMATIC_ORDER
                .iter()
                .position(|&e| e == k)
                .unwrap_or(0)
        };

        let mut sequence_quality = 1.0f64;
        for pair in self.events.windows(2) {
            if rank(pair[1].kind) < rank(pair[0].kind) {
                sequence_quality -= 0.2;
            }
        }
        sequence_quality = sequence_quality.clamp(0.0, 1.0);

        WyckoffScore {
            cause_score,
            sequence_quality,
        }
    }

    fn cause_score(&self) -> f64 {
        self.range
            .map_or(0.0, |r| (r.high - r.low) * self.bars_in_range as f64)
    }

    fn here(&self, bar: &Bar, price: f64) -> Marker {
        Marker {
            index: self.index,
            timestamp: bar.timestamp,
            price,
        }
    }

    fn report(&mut self, kind: WyckoffEventKind, at: Marker) {
        self.events.push(WyckoffEvent {
            kind,
            price: at.price,
            timestamp: at.timestamp,
        });
        self.reported.push((kind, self.index - at.index));
        self.alerts.push(IndicatorAlert::new(
            format!("wyckoff_{}", kind.slug()),
            kind.note(),
            kind.strength(),
        ));
    }

    fn set_range(&mut self, range: ActiveRange) {
        self.range = Some(range);
        self.phase = WyckoffPhase::A;
        self.events.clear();
        self.bars_in_range = 0;
        self.pullback = None;
        self.pending = None;
    }

    fn abandon_range(&mut self) {
        self.range = None;
        self.phase = WyckoffPhase::Undefined;
        self.pullback = None;
    }

    /// Phases A and B: Spring or UTAD, abandonment, Secondary Test.
    fn step_building(&mut self, bar: &Bar, atr: f64) {
        let range = self.range.expect("phases A and B have a range");
        let pierced = |side: WyckoffBias| {
            let seen = side.orient_bar(bar);
            let (tested, _) = range.bounds(side);
            seen.low < tested && seen.close > tested
        };
        let other = |side: WyckoffBias| match side {
            WyckoffBias::Accumulation => WyckoffBias::Distribution,
            WyckoffBias::Distribution => WyckoffBias::Accumulation,
        };

        if self.bars_in_range as usize >= self.params.min_range_bars {
            for side in [WyckoffBias::Accumulation, WyckoffBias::Distribution] {
                if pierced(side) && !pierced(other(side)) && range.bias != Some(other(side)) {
                    self.range = Some(ActiveRange {
                        bias: Some(side),
                        ..range
                    });
                    self.phase = WyckoffPhase::C;
                    let extreme = side.orient(side.orient_bar(bar).low);
                    self.report(kinds(side).test, self.here(bar, extreme));
                    return;
                }
            }
        }

        if range.left_behind(bar.close, self.params.abandon_atr * atr) {
            self.abandon_range();
            return;
        }

        let near = |side: WyckoffBias| {
            let seen = side.orient_bar(bar);
            let (tested, _) = range.bounds(side);
            seen.low >= tested && seen.low <= tested + self.params.st_atr * atr
        };
        let at_boundary = [WyckoffBias::Accumulation, WyckoffBias::Distribution]
            .into_iter()
            .any(|side| range.bias != Some(other(side)) && near(side));
        let quieter = range
            .climax
            .is_none_or(|(volume, spread)| bar.volume < volume && bar.high - bar.low < spread);
        if at_boundary && quieter {
            self.phase = WyckoffPhase::B;
            self.report(WyckoffEventKind::SecondaryTest, self.here(bar, bar.close));
        }
    }

    /// Phase C: Sign of Strength or Weakness, abandonment.
    fn step_tested(&mut self, bar: &Bar, atr: f64, threshold: f64) {
        let range = self.range.expect("phase C has a range");
        let side = range.bias.expect("phase C has a bias");
        let (_, broken) = range.bounds(side);
        let carried = !self.params.breakout_volume || bar.volume > threshold;
        if side.orient_bar(bar).close > broken && carried {
            self.phase = WyckoffPhase::D;
            self.pullback = None;
            self.report(kinds(side).breakout, self.here(bar, bar.close));
        } else if range.left_behind(bar.close, self.params.abandon_atr * atr) {
            self.abandon_range();
        }
    }

    /// Phase D: failed breakout, pullback, Last Point of Support or Supply.
    fn step_breakout(&mut self, bar: &Bar, atr: f64, prev_close: f64) {
        let range = self.range.expect("phase D has a range");
        let side = range.bias.expect("phase D has a bias");
        let (_, broken) = range.bounds(side);
        let seen = side.orient_bar(bar);
        let prev_close = side.orient(prev_close);

        if seen.close < broken {
            self.phase = WyckoffPhase::B;
            self.pullback = None;
            return;
        }
        let Some(mut low) = self.pullback else {
            if seen.close < prev_close {
                self.pullback = Some(self.here(bar, seen.low));
            }
            return;
        };
        if seen.low < low.price {
            low = self.here(bar, seen.low);
        }
        if seen.close > prev_close {
            self.pullback = None;
            if low.price >= broken - self.params.lps_atr * atr {
                self.phase = WyckoffPhase::E;
                self.report(
                    kinds(side).hold,
                    Marker {
                        price: side.orient(low.price),
                        ..low
                    },
                );
            }
        } else {
            self.pullback = Some(low);
        }
    }

    /// Phase E: back inside the range.
    fn step_trend(&mut self, bar: &Bar) {
        let range = self.range.expect("phase E has a range");
        let side = range.bias.expect("phase E has a bias");
        let (_, broken) = range.bounds(side);
        if side.orient_bar(bar).close < broken {
            self.phase = WyckoffPhase::B;
        }
    }

    /// The climax bar test on `bar` alone: the side a range set by this bar would take.
    fn climax_side(
        &self,
        bar: &Bar,
        atr: f64,
        threshold: f64,
        window_high: f64,
        window_low: f64,
    ) -> Option<WyckoffBias> {
        let wide = bar.high - bar.low >= self.params.climax_range_atr * atr;
        if !wide || bar.volume <= threshold || bar.volume.is_nan() {
            return None;
        }
        [WyckoffBias::Accumulation, WyckoffBias::Distribution]
            .into_iter()
            .find(|side| {
                let seen = side.orient_bar(bar);
                let extreme = match side {
                    WyckoffBias::Accumulation => window_low,
                    WyckoffBias::Distribution => -window_high,
                };
                seen.close < seen.open && seen.low <= extreme
            })
    }

    /// A climax bar becomes the pending climax; the Preliminary Support/Supply is looked up in the
    /// window before it.
    fn pending_climax(&self, side: WyckoffBias, bar: &Bar, threshold: f64) -> PendingClimax {
        let seen = side.orient_bar(bar);
        let before = self.bars.len() - 1;
        let preliminary = self
            .bars
            .iter()
            .take(before)
            .enumerate()
            .find(|(_, earlier)| {
                let earlier_seen = side.orient_bar(earlier);
                earlier.volume > threshold && earlier_seen.close > earlier_seen.open
            })
            .map(|(k, earlier)| Marker {
                index: self.index - (before - k) as u64,
                timestamp: earlier.timestamp,
                price: side.orient(side.orient_bar(earlier).low),
            });
        PendingClimax {
            side,
            bar: self.here(bar, side.orient(seen.low)),
            low: seen.low,
            high: seen.high,
            volume: bar.volume,
            spread: bar.high - bar.low,
            preliminary,
            counter: None,
        }
    }

    /// Follows the pending climax on a later bar. Returns `true` when the range was set.
    fn follow_pending(&mut self, bar: &Bar) -> bool {
        let Some(mut pending) = self.pending.take() else {
            return false;
        };
        let side = pending.side;
        let seen = side.orient_bar(bar);
        let expired = self.index - pending.bar.index >= self.params.range_lookback as u64;
        if expired || seen.low < pending.low {
            return false;
        }
        match pending.counter {
            Some(counter) if seen.high <= counter.high => {
                if seen.close < counter.low && counter.high > pending.high {
                    let (low, high) = (pending.bar.price, counter.bar.price);
                    self.set_range(ActiveRange {
                        high: low.max(high),
                        low: low.min(high),
                        source: WyckoffRangeSource::Climax,
                        bias: Some(side),
                        climax: Some((pending.volume, pending.spread)),
                    });
                    let kinds = kinds(side);
                    if let Some(preliminary) = pending.preliminary {
                        self.report(kinds.preliminary, preliminary);
                    }
                    self.report(kinds.climax, pending.bar);
                    self.report(kinds.reaction, counter.bar);
                    return true;
                }
            }
            _ => {
                pending.counter = Some(CounterMove {
                    bar: self.here(bar, side.orient(seen.high)),
                    high: seen.high,
                    low: seen.low,
                });
            }
        }
        self.pending = Some(pending);
        false
    }

    /// Searches a range while none is in play: climax and counter-move first, width fallback last.
    fn search(
        &mut self,
        bar: &Bar,
        atr: f64,
        threshold: f64,
        climax: Option<WyckoffBias>,
        window_high: f64,
        window_low: f64,
    ) {
        if self.follow_pending(bar) {
            return;
        }
        if let Some(side) = climax {
            self.pending = Some(self.pending_climax(side, bar, threshold));
        }
        if self.pending.is_none() && window_high - window_low <= self.params.range_atr_max * atr {
            self.set_range(ActiveRange {
                high: window_high,
                low: window_low,
                source: WyckoffRangeSource::Width,
                bias: None,
                climax: None,
            });
        }
    }
}

impl Indicator for WyckoffStateMachine {
    fn name(&self) -> &str {
        "wyckoff"
    }

    /// The window of `range_lookback` bars and the ATR must both exist: the first output comes
    /// with whichever is complete last.
    fn warmup_period(&self) -> usize {
        self.params.range_lookback.max(self.params.atr_len)
    }

    fn reset(&mut self) {
        self.atr.reset();
        self.prev_close = None;
        self.volume_threshold.reset();
        self.bars.clear();
        self.index = 0;
        self.bars_in_range = 0;
        self.abandon_range();
        self.pending = None;
        self.events.clear();
        self.reported.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.reported.clear();
        self.index += 1;

        let prev_close = self.prev_close.replace(bar.close);
        let tr = match prev_close {
            Some(pc) => (bar.high - bar.low)
                .max((bar.high - pc).abs())
                .max((bar.low - pc).abs()),
            None => bar.high - bar.low,
        };
        let atr = self.atr.update(tr);
        let volume_band = self.volume_threshold.update(bar.volume);

        self.bars.push_back(bar.clone());
        if self.bars.len() > self.params.range_lookback {
            self.bars.pop_front();
        }
        if self.bars.len() < self.params.range_lookback {
            return None;
        }
        let atr = atr.filter(|a| *a > 0.0)?;
        // Without dispersion there is no threshold: nothing exceeds infinity.
        let threshold = match volume_band? {
            band if band.mad > 0.0 => band.upper,
            _ => f64::INFINITY,
        };
        let prev_close = prev_close.unwrap_or(bar.close);

        let window_high = self.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let window_low = self.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let climax = self.climax_side(bar, atr, threshold, window_high, window_low);

        if !matches!(self.phase, WyckoffPhase::Undefined | WyckoffPhase::E) {
            self.bars_in_range += 1;
        }
        let started_in = self.phase;
        match self.phase {
            WyckoffPhase::A | WyckoffPhase::B => self.step_building(bar, atr),
            WyckoffPhase::C => self.step_tested(bar, atr, threshold),
            WyckoffPhase::D => self.step_breakout(bar, atr, prev_close),
            WyckoffPhase::E => self.step_trend(bar),
            WyckoffPhase::Undefined => {}
        }
        // The search runs only on a bar that began with no range in play and stayed so: a bar
        // that reported an event, changed the phase or abandoned a range sets no range.
        let free = matches!(started_in, WyckoffPhase::Undefined | WyckoffPhase::E);
        if free && self.phase == started_in {
            self.search(bar, atr, threshold, climax, window_high, window_low);
        } else {
            self.pending = None;
        }

        let mut extra = HashMap::new();
        extra.insert("atr".to_string(), atr);
        if threshold.is_finite() {
            extra.insert("volume_threshold".to_string(), threshold);
        }
        extra.insert(
            "climax".to_string(),
            // A selling climax bar (the accumulation side) is the falling one.
            climax.map_or(0.0, |side| -side.code()),
        );
        extra.insert(
            "bias".to_string(),
            self.bias().map_or(0.0, WyckoffBias::code),
        );
        extra.insert(
            "range_source".to_string(),
            match self.range.map(|r| r.source) {
                None => 0.0,
                Some(WyckoffRangeSource::Climax) => 1.0,
                Some(WyckoffRangeSource::Width) => 2.0,
            },
        );
        if let Some(range) = self.range {
            extra.insert("range_high".to_string(), range.high);
            extra.insert("range_low".to_string(), range.low);
            extra.insert("cause_score".to_string(), self.cause_score());
        }
        for (kind, bars_back) in &self.reported {
            extra.insert(format!("event_{}", kind.slug()), *bars_back as f64);
        }
        Some(IndicatorOutput::with_extra(self.phase.code(), extra))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const WYCKOFF_DEFINITION: super::definition::IndicatorDefinition = super::definition::IndicatorDefinition {
    name: "wyckoff",
    aliases: &[],
    description: "Wyckoff accumulation/distribution state machine: range from climax and counter-move (PS, SC/BC, AR) or from a width fallback, Phases A-E, ST, Spring/UTAD, SOS/SOW, LPS/LPSY; boundaries, bias, ATR, climax test and events in extra",
    params: &[
        super::definition::ParamSpec::count("range_lookback", 20, 3, 10000).meaning("bars of the window: volume statistics, window extremes of the climax test, life of a pending climax, width fallback"),
        super::definition::ParamSpec::number("range_atr_max", 3.0, 0.1, 1000.0).meaning("width fallback: widest window, in ATR, that is set as a range while no climax is pending"),
        super::definition::ParamSpec::count("min_range_bars", 6, 2, 10000).meaning("bars a range must exist before a Spring or UTAD counts"),
        super::definition::ParamSpec::count("atr_len", 14, 1, 10000).meaning("bars of the Wilder ATR every distance is measured in"),
        super::definition::ParamSpec::number("volume_k", 2.0, 0.0, 100.0).meaning("volume outlier: scaled MADs above the window median; no outlier while the MAD is zero"),
        super::definition::ParamSpec::number("climax_range_atr", 1.5, 0.1, 100.0).meaning("climax bar: smallest high-low spread, in ATR"),
        super::definition::ParamSpec::number("st_atr", 0.25, 0.0, 100.0).meaning("Secondary Test: largest distance of the bar's extreme from the boundary, in ATR"),
        super::definition::ParamSpec::choice("breakout_volume", "on", &["off", "on"], true).meaning("whether a Sign of Strength/Weakness needs a volume outlier"),
        super::definition::ParamSpec::number("lps_atr", 0.5, 0.0, 100.0).meaning("Last Point of Support/Supply: how far the pullback may reach back beyond the broken boundary, in ATR"),
        super::definition::ParamSpec::number("abandon_atr", 1.0, 0.0, 1000.0).meaning("a close this far outside the range, in ATR, abandons it in Phases A to C"),
    ],
    checks: &[],
    build: |p| {
        Ok(Box::new(WyckoffStateMachine::with_params(WyckoffParams {
            range_lookback: p.count("range_lookback"),
            range_atr_max: p.number("range_atr_max"),
            min_range_bars: p.count("min_range_bars"),
            atr_len: p.count("atr_len"),
            volume_k: p.number("volume_k"),
            climax_range_atr: p.number("climax_range_atr"),
            st_atr: p.number("st_atr"),
            breakout_volume: p.choice("breakout_volume") == "on",
            lps_atr: p.number("lps_atr"),
            abandon_atr: p.number("abandon_atr"),
        })))
    },
};

#[cfg(test)]
mod tests {
    use super::*;

    /// Quiet bars around `center`: lows down to `center - 0.75 * half_width`, highs up to
    /// `center + 0.75 * half_width`, every bar `0.6 * half_width` wide, one constant volume. No
    /// bar is a climax, so only the width fallback can set a range.
    fn range_bars(n: usize, center: f64, half_width: f64, volume: f64) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let offset = ((i % 4) as f64 - 1.5) * half_width * 0.3;
                let price = center + offset;
                Bar::new(
                    i as i64 * 60,
                    price,
                    price + half_width * 0.3,
                    price - half_width * 0.3,
                    price,
                    volume,
                )
            })
            .collect()
    }

    /// A machine fed 26 quiet bars: the range 98.5 to 101.5 is set by the width fallback on bar
    /// 19 (the first with a full window) and is `min_range_bars` = 6 bars old on bar 25.
    fn machine_in_quiet_range(params: WyckoffParams) -> WyckoffStateMachine {
        let mut machine = WyckoffStateMachine::with_params(params);
        for bar in range_bars(26, 100.0, 2.0, 100.0) {
            machine.on_bar(&bar);
        }
        machine
    }

    fn bar(n: i64, open: f64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(n * 60, open, high, low, close, 100.0)
    }

    #[test]
    fn test_new_keeps_the_other_thresholds_at_their_defaults() {
        let machine = WyckoffStateMachine::new(30, 4.0, 8);
        assert_eq!(
            machine.params(),
            WyckoffParams {
                range_lookback: 30,
                range_atr_max: 4.0,
                min_range_bars: 8,
                ..WyckoffParams::default()
            }
        );
        assert_eq!(
            WyckoffStateMachine::with_defaults().params(),
            WyckoffParams::default()
        );
        // Degenerate lengths are raised to the smallest that works.
        let clamped = WyckoffStateMachine::new(1, 3.0, 0).params();
        assert_eq!((clamped.range_lookback, clamped.min_range_bars), (3, 2));
    }

    #[test]
    fn test_width_fallback_sets_a_range_with_open_bias() {
        let mut machine = WyckoffStateMachine::with_defaults();
        let bars = range_bars(20, 100.0, 2.0, 100.0);
        for bar in &bars[..19] {
            assert!(machine.on_bar(bar).is_none());
        }
        let out = machine.on_bar(&bars[19]).expect("window full");

        assert_eq!(machine.phase(), WyckoffPhase::A);
        assert_eq!(machine.bias(), None);
        let range = machine.range().expect("range set");
        assert_eq!(range.source, WyckoffRangeSource::Width);
        assert!((range.low - 98.5).abs() < 1e-9 && (range.high - 101.5).abs() < 1e-9);
        assert!(machine.events().is_empty());

        assert_eq!(out.value, 1.0);
        assert_eq!(out.extra["range_source"], 2.0);
        assert_eq!(out.extra["bias"], 0.0);
        assert_eq!(out.extra["climax"], 0.0);
        assert_eq!(out.extra["cause_score"], 0.0);
        assert!((out.extra["range_low"] - 98.5).abs() < 1e-9);
        assert!((out.extra["range_high"] - 101.5).abs() < 1e-9);
        // Constant volume: no dispersion, so there is no threshold.
        assert!(!out.extra.contains_key("volume_threshold"));
    }

    #[test]
    fn test_spring_and_utad_decide_an_open_bias() {
        let mut machine = machine_in_quiet_range(WyckoffParams::default());
        assert_eq!(machine.bias(), None);
        let out = machine.on_bar(&bar(26, 99.5, 100.0, 97.5, 99.6)).unwrap();
        assert_eq!(machine.bias(), Some(WyckoffBias::Accumulation));
        assert_eq!(machine.phase(), WyckoffPhase::C);
        assert_eq!(out.extra["event_spring"], 0.0);
        assert_eq!(out.extra["bias"], 1.0);
        let spring = machine.events().last().unwrap();
        assert_eq!(
            (spring.kind, spring.price),
            (WyckoffEventKind::Spring, 97.5)
        );
        assert_eq!(machine.alerts()[0].kind, "wyckoff_spring");

        let mut machine = machine_in_quiet_range(WyckoffParams::default());
        let out = machine
            .on_bar(&bar(26, 100.5, 102.5, 100.0, 100.4))
            .unwrap();
        assert_eq!(machine.bias(), Some(WyckoffBias::Distribution));
        assert_eq!(machine.phase(), WyckoffPhase::C);
        assert_eq!(out.extra["event_utad"], 0.0);
        assert_eq!(out.extra["bias"], -1.0);
        let utad = machine.events().last().unwrap();
        assert_eq!((utad.kind, utad.price), (WyckoffEventKind::Utad, 102.5));
    }

    /// The same bar four bars earlier: the range is only three bars old.
    #[test]
    fn test_spring_needs_min_range_bars() {
        let mut machine = WyckoffStateMachine::with_defaults();
        for bar in range_bars(22, 100.0, 2.0, 100.0) {
            machine.on_bar(&bar);
        }
        machine.on_bar(&bar(22, 99.5, 100.0, 97.5, 99.6));
        assert_eq!(machine.bias(), None);
        assert_eq!(machine.phase(), WyckoffPhase::B);
        assert!(machine
            .events()
            .iter()
            .all(|e| e.kind == WyckoffEventKind::SecondaryTest));
    }

    /// A bar through both boundaries that closes inside is neither a Spring nor a UTAD.
    #[test]
    fn test_bar_piercing_both_boundaries_is_no_event() {
        let mut machine = machine_in_quiet_range(WyckoffParams::default());
        let events = machine.events().len();
        let out = machine.on_bar(&bar(26, 100.0, 102.5, 97.5, 100.0)).unwrap();
        assert_eq!(machine.bias(), None);
        assert_eq!(machine.events().len(), events);
        assert!(out.extra.keys().all(|key| !key.starts_with("event_")));
        assert!(machine.range().is_some());
    }

    #[test]
    fn test_close_far_outside_abandons_the_range() {
        // ATR is about 1.5: 104.0 is more than one ATR above the range high of 101.5.
        let mut machine = machine_in_quiet_range(WyckoffParams::default());
        let out = machine
            .on_bar(&bar(26, 100.9, 104.2, 100.8, 104.0))
            .unwrap();
        assert_eq!(machine.phase(), WyckoffPhase::Undefined);
        assert!(machine.range().is_none());
        assert_eq!(out.value, 0.0);
        assert_eq!(out.extra["range_source"], 0.0);
        assert!(!out.extra.contains_key("range_high"));
        assert!(!out.extra.contains_key("cause_score"));
    }

    /// Without the volume condition the sequence is a matter of price alone: Spring, a close
    /// above the range, a lower close (the pullback, low 101.6) and a higher close again. The
    /// Last Point of Support lies on the pullback bar, one bar before the bar that reports it.
    #[test]
    fn test_price_only_sequence_reaches_phase_e() {
        let params = WyckoffParams {
            breakout_volume: false,
            ..WyckoffParams::default()
        };
        let mut machine = machine_in_quiet_range(params);
        machine.on_bar(&bar(26, 99.5, 100.0, 97.5, 99.6));
        assert_eq!(machine.phase(), WyckoffPhase::C);
        let out = machine.on_bar(&bar(27, 99.6, 102.6, 99.5, 102.4)).unwrap();
        assert_eq!(machine.phase(), WyckoffPhase::D);
        assert_eq!(out.extra["event_signofstrength"], 0.0);
        let out = machine
            .on_bar(&bar(28, 102.4, 102.5, 101.6, 101.9))
            .unwrap();
        assert_eq!(
            out.value, 4.0,
            "a pullback alone is no Last Point of Support"
        );
        let out = machine
            .on_bar(&bar(29, 101.9, 102.8, 101.8, 102.6))
            .unwrap();
        assert_eq!(machine.phase(), WyckoffPhase::E);
        assert_eq!(out.extra["event_lastpointofsupport"], 1.0);
        let hold = machine.events().last().unwrap();
        assert_eq!(
            (hold.kind, hold.price, hold.timestamp),
            (WyckoffEventKind::LastPointOfSupport, 101.6, 28 * 60)
        );

        // The range is 3.0 wide and spent bars 20 to 29 in Phases A to D.
        let score = machine.score();
        assert!((score.cause_score - 30.0).abs() < 1e-9, "{score:?}");
        assert_eq!(score.sequence_quality, 1.0);
        // Phase E adds nothing to the cause.
        let out = machine
            .on_bar(&bar(30, 102.6, 103.5, 102.5, 103.4))
            .unwrap();
        assert_eq!(out.value, 5.0);
        assert!((out.extra["cause_score"] - 30.0).abs() < 1e-9);
    }

    /// With the default the same close above the range needs a volume outlier, and without
    /// dispersion there is none: against a constant 100 neither 101 nor 300 is one, and
    /// `volume_threshold` is absent.
    #[test]
    fn test_no_volume_outlier_without_dispersion() {
        for breakout_volume in [100.0, 101.0, 300.0] {
            let mut machine = machine_in_quiet_range(WyckoffParams::default());
            machine.on_bar(&bar(26, 99.5, 100.0, 97.5, 99.6));
            let out = machine
                .on_bar(&Bar::new(
                    27 * 60,
                    99.6,
                    102.6,
                    99.5,
                    102.4,
                    breakout_volume,
                ))
                .unwrap();
            assert_eq!(machine.phase(), WyckoffPhase::C, "{breakout_volume}");
            assert!(!out.extra.contains_key("volume_threshold"));
            assert_eq!(out.extra["climax"], 0.0);
        }
    }

    /// On volumes that vary (90, 100, 110, 100 in turn: median 100, scaled MAD 7.4, threshold
    /// about 114.8) a close above the range on 300 is the Sign of Strength, one on 110 is not.
    #[test]
    fn test_sign_of_strength_needs_a_volume_outlier_by_default() {
        let run = |breakout_volume: f64| {
            let mut machine = WyckoffStateMachine::with_defaults();
            for (i, mut bar) in range_bars(26, 100.0, 2.0, 100.0).into_iter().enumerate() {
                bar.volume = [90.0, 100.0, 110.0, 100.0][i % 4];
                machine.on_bar(&bar);
            }
            machine.on_bar(&bar(26, 99.5, 100.0, 97.5, 99.6));
            let out = machine
                .on_bar(&Bar::new(
                    27 * 60,
                    99.6,
                    102.6,
                    99.5,
                    102.4,
                    breakout_volume,
                ))
                .unwrap();
            (machine.phase(), out.extra["volume_threshold"])
        };
        let (phase, threshold) = run(300.0);
        assert_eq!(phase, WyckoffPhase::D);
        assert!((threshold - 114.826).abs() < 1e-3, "{threshold}");
        assert_eq!(run(110.0).0, WyckoffPhase::C);
    }

    /// A bar that abandons a range sets none: the width fallback would apply on the very bar
    /// (a narrow range, `abandon_atr` small enough for a close just outside), but the search
    /// starts with the next bar.
    #[test]
    fn test_abandoning_bar_sets_no_range() {
        let params = WyckoffParams {
            abandon_atr: 0.1,
            ..WyckoffParams::default()
        };
        let mut machine = machine_in_quiet_range(params);
        // Closes 0.3 above the range high of 101.5; the window is still under 3 ATR wide.
        let out = machine
            .on_bar(&bar(26, 100.9, 101.9, 100.8, 101.8))
            .unwrap();
        assert_eq!(out.value, 0.0);
        assert_eq!(out.extra["range_source"], 0.0);
        assert!(machine.range().is_none());
        // The next bar, back inside: the fallback sets the new range from the window.
        let out = machine
            .on_bar(&bar(27, 101.0, 101.4, 100.6, 101.0))
            .unwrap();
        assert_eq!(out.value, 1.0);
        assert_eq!(out.extra["range_source"], 2.0);
        assert!((out.extra["range_high"] - 101.9).abs() < 1e-9);
        assert_eq!(out.extra["cause_score"], 0.0);
    }

    #[test]
    fn test_reset_restores_the_initial_state() {
        let mut machine = machine_in_quiet_range(WyckoffParams::default());
        machine.on_bar(&bar(26, 99.5, 100.0, 97.5, 99.6));
        machine.reset();
        assert_eq!(machine.phase(), WyckoffPhase::Undefined);
        assert!(machine.range().is_none());
        assert!(machine.events().is_empty());
        assert!(machine.alerts().is_empty());
        assert_eq!(machine.score().cause_score, 0.0);
        let bars = range_bars(20, 100.0, 2.0, 100.0);
        for bar in &bars[..19] {
            assert!(machine.on_bar(bar).is_none());
        }
        assert_eq!(machine.on_bar(&bars[19]).unwrap().value, 1.0);
    }

    /// Robustness on a pseudo-random walk, and the invariants of the output: the range keys are
    /// present exactly while a phase is reported, the codes stay in their sets, an event never
    /// points further back than the window.
    #[test]
    fn test_output_invariants_across_random_walk() {
        let mut machine = WyckoffStateMachine::with_defaults();
        let mut price = 100.0;
        for i in 0..600 {
            let open = price;
            price += ((i * 37) % 7) as f64 * 0.3 - 0.9 + ((i * 11) % 5) as f64 * 0.01;
            let bar = Bar::new(
                i as i64 * 60,
                open,
                open.max(price) + 0.2 + ((i * 13) % 4) as f64 * 0.2,
                open.min(price) - 0.2 - ((i * 7) % 4) as f64 * 0.2,
                price,
                100.0 + ((i * 29) % 11) as f64 * 20.0,
            );
            let Some(out) = machine.on_bar(&bar) else {
                assert!(i < 19);
                continue;
            };
            let in_range = out.value != 0.0;
            for key in ["range_high", "range_low", "cause_score"] {
                assert_eq!(out.extra.contains_key(key), in_range, "bar {i}: {key}");
            }
            assert_eq!(out.extra["range_source"] != 0.0, in_range);
            assert!([0.0, 1.0, 2.0, 3.0, 4.0, 5.0].contains(&out.value));
            assert!([-1.0, 0.0, 1.0].contains(&out.extra["bias"]));
            assert!([-1.0, 0.0, 1.0].contains(&out.extra["climax"]));
            assert!(out.extra["atr"] > 0.0);
            assert!(out.extra["volume_threshold"].is_finite());
            if in_range {
                assert!(out.extra["range_high"] > out.extra["range_low"]);
                assert_eq!(machine.range().unwrap().high, out.extra["range_high"]);
            }
            let events: Vec<_> = out
                .extra
                .iter()
                .filter(|(key, _)| key.starts_with("event_"))
                .collect();
            assert_eq!(events.len(), machine.alerts().len());
            for (key, back) in events {
                assert!((0.0..40.0).contains(back), "bar {i}: {key} = {back}");
            }
        }
    }
}
