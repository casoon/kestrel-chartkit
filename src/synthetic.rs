//! Deterministic, dependency-free synthetic price series and market pattern generators.
//!
//! Provides seed-based pseudo-random OHLCV bar generators (`random_walk_bars`, `trending_bars`)
//! as well as calibrated structural presets for market analysis indicators:
//! - Wyckoff accumulation and distribution schematic sequences (`wyckoff_schematic_bars`)
//! - Break of Structure (BOS) / Change of Character (CHoCH) swing pivots (`bos_choch_swing_bars`)
//!
//! All bars are emitted as [`QualifiedBar`] with [`BarQuality::is_synthetic`] flagged as `true`.

use crate::indicator::wyckoff::WyckoffBias;
use crate::model::{Bar, BarQuality, QualifiedBar};

/// Self-contained, seed-based 64-bit pseudo-random number generator (SplitMix64).
///
/// Designed to provide deterministic, platform-independent random number generation without
/// pulling in external dependencies like `rand`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SimpleRng {
    state: u64,
}

impl SimpleRng {
    /// Creates a new PRNG with the specified 64-bit seed.
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Generates the next pseudo-random 64-bit unsigned integer.
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e3779b97f4a7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
        z ^ (z >> 31)
    }

    /// Generates a pseudo-random `f64` in the half-open interval `[0.0, 1.0)`.
    pub fn next_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Generates a pseudo-random `f64` uniformly distributed in `[min, max)`.
    pub fn next_range(&mut self, min: f64, max: f64) -> f64 {
        min + (max - min) * self.next_f64()
    }

    /// Generates a standard normally distributed variable (mean 0.0, std dev 1.0)
    /// using the Box-Muller transform.
    pub fn next_gaussian(&mut self) -> f64 {
        let u1 = self.next_f64().max(1e-15);
        let u2 = self.next_f64();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

/// Constructs a [`QualifiedBar`] marked with synthetic quality flags.
fn synthetic_bar(
    timestamp: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
) -> QualifiedBar {
    QualifiedBar::new(
        Bar::new(timestamp, open, high, low, close, volume),
        BarQuality {
            volume_available: true,
            is_synthetic: true,
            is_forward_filled: false,
            has_gap: false,
        },
    )
}

/// Generates a synthetic random walk bar series with drift and volatility.
///
/// Returns `count` bars, starting at `start_price` and stepping at 60-second intervals.
pub fn random_walk_bars(
    seed: u64,
    count: usize,
    start_price: f64,
    drift: f64,
    volatility: f64,
    volume: f64,
) -> Vec<QualifiedBar> {
    let mut rng = SimpleRng::new(seed);
    let mut bars = Vec::with_capacity(count);
    let mut current_price = start_price.max(0.01);

    for i in 0..count {
        let open = current_price;
        let change = drift + volatility * rng.next_gaussian();
        let close = (open + change).max(0.01);
        let wick_upper = rng.next_f64() * volatility.abs();
        let wick_lower = rng.next_f64() * volatility.abs();
        let high = open.max(close) + wick_upper;
        let low = (open.min(close) - wick_lower).max(0.001);
        let bar_volume = (volume + rng.next_range(-0.05, 0.05) * volume).max(0.0);

        bars.push(synthetic_bar(
            i as i64 * 60,
            open,
            high,
            low,
            close,
            bar_volume,
        ));
        current_price = close;
    }

    bars
}

/// Generates a directional trending bar series with superimposed noise.
///
/// Returns `count` bars, stepping by `trend_per_bar` per interval plus gaussian noise.
pub fn trending_bars(
    seed: u64,
    count: usize,
    start_price: f64,
    trend_per_bar: f64,
    noise: f64,
    volume: f64,
) -> Vec<QualifiedBar> {
    let mut rng = SimpleRng::new(seed);
    let mut bars = Vec::with_capacity(count);
    let mut current_price = start_price.max(0.01);

    for i in 0..count {
        let open = current_price;
        let delta = trend_per_bar + rng.next_gaussian() * noise;
        let close = (open + delta).max(0.01);
        let wick1 = rng.next_f64() * noise.abs() + 0.01;
        let wick2 = rng.next_f64() * noise.abs() + 0.01;
        let high = open.max(close) + wick1;
        let low = (open.min(close) - wick2).max(0.001);
        let bar_volume = (volume + rng.next_range(-0.05, 0.05) * volume).max(0.0);

        bars.push(synthetic_bar(
            i as i64 * 60,
            open,
            high,
            low,
            close,
            bar_volume,
        ));
        current_price = close;
    }

    bars
}

/// Shortest decline the schematic carries before its climax bar: the default `range_lookback` of
/// [`WyckoffStateMachine`](crate::indicator::wyckoff::WyckoffStateMachine). A longer lookback gets
/// that many more bars of decline in front, so the state machine's window is full at the climax.
const WYCKOFF_MIN_RANGE_LOOKBACK: usize = 20;

/// The accumulation schematic: (close, wick above the body, wick below the body, volume), in the
/// units of a range from 94 to 100 at a base volume of 100. The open is the previous close, the
/// first bar opens at [`WYCKOFF_SCHEMATIC_START`]. Each event lies on one bar:
/// Preliminary Support 16, Selling Climax 21, Automatic Rally 24, Secondary Test 29, Spring 48,
/// Sign of Strength 53, Last Point of Support 57.
const WYCKOFF_SCHEMATIC: [(f64, f64, f64, f64); 62] = [
    (109.6, 0.3, 0.4, 105.0),
    (108.9, 0.4, 0.3, 110.0),
    (109.3, 0.3, 0.3, 90.0),
    (108.2, 0.2, 0.4, 115.0),
    (107.3, 0.3, 0.3, 120.0),
    (107.7, 0.3, 0.2, 95.0),
    (106.5, 0.2, 0.4, 125.0),
    (105.6, 0.3, 0.3, 120.0),
    (105.9, 0.3, 0.3, 100.0),
    (104.7, 0.2, 0.4, 130.0),
    (103.6, 0.3, 0.3, 135.0),
    (103.9, 0.3, 0.2, 105.0),
    (102.6, 0.2, 0.4, 140.0),
    (101.6, 0.3, 0.4, 150.0),
    (101.0, 0.2, 0.4, 145.0),
    (98.6, 0.2, 0.5, 185.0),
    (98.9, 0.3, 1.1, 240.0),
    (99.3, 0.3, 0.3, 150.0),
    (98.4, 0.2, 0.4, 140.0),
    (97.4, 0.2, 0.5, 165.0),
    (97.0, 0.2, 0.4, 190.0),
    (94.8, 0.2, 0.8, 430.0),
    (96.4, 0.3, 0.2, 260.0),
    (98.3, 0.3, 0.2, 210.0),
    (99.6, 0.4, 0.2, 180.0),
    (98.7, 0.3, 0.3, 130.0),
    (97.4, 0.2, 0.4, 120.0),
    (96.2, 0.2, 0.4, 115.0),
    (95.1, 0.3, 0.5, 125.0),
    (95.3, 0.2, 0.8, 140.0),
    (96.5, 0.3, 0.2, 110.0),
    (97.8, 0.3, 0.3, 105.0),
    (98.9, 0.4, 0.2, 100.0),
    (98.2, 0.3, 0.3, 85.0),
    (97.6, 0.3, 0.3, 90.0),
    (96.5, 0.2, 0.4, 85.0),
    (95.6, 0.3, 0.4, 80.0),
    (95.0, 0.2, 0.5, 75.0),
    (95.4, 0.2, 0.8, 70.0),
    (96.6, 0.3, 0.2, 85.0),
    (97.9, 0.3, 0.3, 90.0),
    (99.0, 0.4, 0.2, 95.0),
    (98.3, 0.3, 0.3, 75.0),
    (98.9, 0.5, 0.3, 80.0),
    (97.7, 0.2, 0.4, 70.0),
    (96.5, 0.2, 0.3, 75.0),
    (95.6, 0.3, 0.4, 70.0),
    (94.8, 0.2, 0.3, 80.0),
    (94.9, 0.2, 1.4, 95.0),
    (95.8, 0.2, 0.5, 85.0),
    (96.9, 0.3, 0.2, 110.0),
    (98.2, 0.3, 0.2, 140.0),
    (99.3, 0.3, 0.3, 170.0),
    (101.6, 0.2, 0.2, 310.0),
    (102.1, 0.3, 0.4, 240.0),
    (101.3, 0.2, 0.3, 150.0),
    (100.6, 0.2, 0.3, 110.0),
    (100.7, 0.2, 0.5, 85.0),
    (101.7, 0.3, 0.2, 130.0),
    (102.6, 0.3, 0.2, 160.0),
    (103.3, 0.3, 0.3, 170.0),
    (103.9, 0.3, 0.3, 150.0),
];
const WYCKOFF_SCHEMATIC_START: f64 = 110.4;
/// Centre and half-width of the schematic's range (94 to 100).
const WYCKOFF_SCHEMATIC_CENTER: f64 = 97.0;
const WYCKOFF_SCHEMATIC_HALF_WIDTH: f64 = 3.0;
/// Bars of [`WYCKOFF_SCHEMATIC`] whose volume carries an event; the seed leaves them alone.
const WYCKOFF_SCHEMATIC_EVENT_BARS: [usize; 7] = [16, 21, 24, 29, 48, 53, 57];
/// Close-to-close step of the extra decline bars in front of the schematic, in its units.
const WYCKOFF_SCHEMATIC_LEAD_STEP: f64 = 0.6;

/// Configuration parameters for generating Wyckoff schematic bar sequences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffGeneratorConfig {
    /// Middle of the trading range.
    pub center_price: f64,
    /// The `range_lookback` of the state machine the series is meant for. Values up to 20 leave
    /// the schematic as it is (it carries 21 bars before its climax); every bar beyond 20 adds
    /// one bar of decline in front.
    pub range_lookback: usize,
    /// Half-width of the trading range: it runs from `center_price - spread` to
    /// `center_price + spread`.
    pub spread: f64,
    /// Volume of an ordinary bar of the preceding trend; the climax trades about four times that.
    pub base_volume: f64,
}

impl Default for WyckoffGeneratorConfig {
    fn default() -> Self {
        Self {
            center_price: 100.0,
            range_lookback: 20,
            spread: 2.0,
            base_volume: 100.0,
        }
    }
}

/// Generates a Wyckoff schematic that [`WyckoffStateMachine`](crate::indicator::wyckoff::WyckoffStateMachine)
/// reads through Phases A to E with its default thresholds.
///
/// Accumulation: a decline with a Preliminary Support, the Selling Climax that sets the range
/// low, the Automatic Rally that sets the range high, a Secondary Test, a Spring, a Sign of
/// Strength on expanded volume and a pullback that holds above the range (Last Point of Support).
/// Distribution is the same series reflected at `center_price`.
///
/// The prices are a fixed schematic, scaled to `center_price` and `spread`; `seed` only varies
/// the volume of the bars that carry no event, by up to 1 % of `base_volume`.
pub fn wyckoff_schematic_bars(
    seed: u64,
    bias: WyckoffBias,
    config: WyckoffGeneratorConfig,
) -> Vec<QualifiedBar> {
    let mut rng = SimpleRng::new(seed);
    let lead = config
        .range_lookback
        .saturating_sub(WYCKOFF_MIN_RANGE_LOOKBACK);
    let scale = config.spread / WYCKOFF_SCHEMATIC_HALF_WIDTH;
    let price = |schematic: f64| {
        let offset = (schematic - WYCKOFF_SCHEMATIC_CENTER) * scale;
        match bias {
            WyckoffBias::Accumulation => config.center_price + offset,
            WyckoffBias::Distribution => config.center_price - offset,
        }
    };

    // (close, wick above, wick below, volume, carries an event) in schematic units.
    let lead_rows = (0..lead).map(|k| {
        let close = WYCKOFF_SCHEMATIC_START + WYCKOFF_SCHEMATIC_LEAD_STEP * (lead - 1 - k) as f64;
        (close, 0.3, 0.3, 100.0, false)
    });
    let schematic_rows =
        WYCKOFF_SCHEMATIC
            .iter()
            .enumerate()
            .map(|(i, &(close, above, below, volume))| {
                let event = WYCKOFF_SCHEMATIC_EVENT_BARS.contains(&i);
                (close, above, below, volume, event)
            });

    let mut bars = Vec::with_capacity(lead + WYCKOFF_SCHEMATIC.len());
    let mut previous = WYCKOFF_SCHEMATIC_START + WYCKOFF_SCHEMATIC_LEAD_STEP * lead as f64;
    for (i, (close, above, below, volume, event)) in lead_rows.chain(schematic_rows).enumerate() {
        let top = price(previous.max(close) + above);
        let bottom = price(previous.min(close) - below);
        let noise = if event {
            0.0
        } else {
            rng.next_range(-0.01, 0.01)
        };
        bars.push(synthetic_bar(
            i as i64 * 60,
            price(previous),
            top.max(bottom),
            top.min(bottom),
            price(close),
            config.base_volume * (volume / 100.0 + noise),
        ));
        previous = close;
    }
    bars
}

/// Swing direction for BOS / CHoCH structure tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SwingDirection {
    Bullish,
    Bearish,
}

/// Generates a calibrated swing pivot and subsequent structural break bar sequence.
///
/// Produces a symmetrical window of `2 * pivot_len + 1` bars where the bar at index
/// `pivot_len` is guaranteed to be the pivot peak (for Bullish break) or pivot valley
/// (for Bearish break), followed immediately by a decisive breakout bar closing beyond
/// that confirmed pivot level.
pub fn bos_choch_swing_bars(
    seed: u64,
    direction: SwingDirection,
    pivot_len: usize,
) -> Vec<QualifiedBar> {
    let mut rng = SimpleRng::new(seed);
    let len = pivot_len.max(2);
    let window_bars = 2 * len + 1;
    let mut bars = Vec::with_capacity(window_bars + 1);

    let base_price = 100.0;
    let step_height = 3.0;

    for i in 0..window_bars {
        let dist = i.abs_diff(len) as f64;
        let noise = rng.next_range(0.05, 0.2);

        let (open, high, low, close) = match direction {
            SwingDirection::Bearish => {
                // Form a pivot low at index `len`
                let price = if i == len {
                    base_price
                } else {
                    base_price + dist * step_height + noise
                };
                (price, price + 0.5, price - 0.5, price)
            }
            SwingDirection::Bullish => {
                // Form a pivot high at index `len`
                let price = if i == len {
                    base_price
                } else {
                    base_price - dist * step_height - noise
                };
                (price, price + 0.5, price - 0.5, price)
            }
        };

        bars.push(synthetic_bar(i as i64 * 60, open, high, low, close, 1000.0));
    }

    // Break bar: closes definitively beyond the pivot formed at index `len`
    let break_timestamp = window_bars as i64 * 60;
    let break_bar = match direction {
        SwingDirection::Bearish => {
            // Closes lower than pivot valley low (99.5)
            let close = base_price - step_height * 2.0;
            synthetic_bar(
                break_timestamp,
                base_price,
                base_price + 0.2,
                close - 0.5,
                close,
                1500.0,
            )
        }
        SwingDirection::Bullish => {
            // Closes higher than pivot peak high (100.5)
            let close = base_price + step_height * 2.0;
            synthetic_bar(
                break_timestamp,
                base_price,
                close + 0.5,
                base_price - 0.2,
                close,
                1500.0,
            )
        }
    };
    bars.push(break_bar);

    bars
}

/// Candle shape for [`bos_choch_swing_bars_shaped`].
///
/// [`bos_choch_swing_bars`] draws every bar as a flat stroke of one unit. That is enough for the
/// detectors but reads as a row of dashes in an illustration. The shape widens the bars around the
/// swing without touching the pivot bar's high and low, so the pivot and the break stay where
/// they are.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SwingCandleShape {
    /// High-to-low span of the bars on both sides of the pivot, in price units. The step between
    /// two neighbouring bars is 3.0, so a span at or above that makes neighbours overlap.
    pub range: f64,
    /// Share of the span the body takes, in `0.0..=1.0`; the rest is split evenly into the two
    /// wicks. The pivot bar keeps its span of 1.0 (high 0.5 above, low 0.5 below the pivot price)
    /// and applies the same share.
    pub body_ratio: f64,
}

impl Default for SwingCandleShape {
    fn default() -> Self {
        Self {
            range: 2.0,
            body_ratio: 0.6,
        }
    }
}

/// Like [`bos_choch_swing_bars`], but with candle bodies and wicks as given by `shape`.
///
/// Bars approach the pivot with the move of the swing and leave it against it, so bodies point
/// the way the price travels. The pivot bar and the break bar keep the highs and lows of the
/// plain generator; the pivot is confirmed and broken at the same bars.
pub fn bos_choch_swing_bars_shaped(
    seed: u64,
    direction: SwingDirection,
    pivot_len: usize,
    shape: SwingCandleShape,
) -> Vec<QualifiedBar> {
    let mut bars = bos_choch_swing_bars(seed, direction, pivot_len);
    let len = pivot_len.max(2);
    let window_bars = 2 * len + 1;
    let body_ratio = shape.body_ratio.clamp(0.0, 1.0);
    // Bullish: a pivot high, so the swing rises into it and falls away from it.
    let up = match direction {
        SwingDirection::Bullish => 1.0,
        SwingDirection::Bearish => -1.0,
    };

    for (i, qb) in bars.iter_mut().take(window_bars).enumerate() {
        let price = qb.bar.close;
        let range = if i == len { 1.0 } else { shape.range.max(0.0) };
        let body = range * body_ratio;
        let travel = if i <= len { up } else { -up };
        qb.bar.open = price - travel * body / 2.0;
        qb.bar.close = price + travel * body / 2.0;
        qb.bar.high = price + range / 2.0;
        qb.bar.low = price - range / 2.0;
    }

    bars
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::bos_choch::BosChochEngine;
    use crate::indicator::wyckoff::{WyckoffPhase, WyckoffStateMachine};
    use crate::indicator::Indicator;

    #[test]
    fn test_rng_determinism() {
        let mut rng1 = SimpleRng::new(42);
        let mut rng2 = SimpleRng::new(42);
        for _ in 0..100 {
            assert_eq!(rng1.next_u64(), rng2.next_u64());
            assert_eq!(rng1.next_f64(), rng2.next_f64());
            assert_eq!(rng1.next_gaussian(), rng2.next_gaussian());
        }
    }

    #[test]
    fn test_random_walk_determinism_and_synthetic_flag() {
        let bars1 = random_walk_bars(12345, 50, 100.0, 0.05, 1.0, 1000.0);
        let bars2 = random_walk_bars(12345, 50, 100.0, 0.05, 1.0, 1000.0);

        assert_eq!(bars1.len(), 50);
        assert_eq!(bars1, bars2);
        for qb in &bars1 {
            assert!(qb.quality.is_synthetic);
            assert!(qb.quality.volume_available);
            assert!(qb.bar.validate().is_ok());
        }
    }

    #[test]
    fn test_trending_bars_determinism_and_synthetic_flag() {
        let bars = trending_bars(999, 40, 50.0, 0.5, 0.2, 500.0);
        assert_eq!(bars.len(), 40);
        assert!(bars.last().unwrap().bar.close > 50.0);
        for qb in &bars {
            assert!(qb.quality.is_synthetic);
            assert!(qb.bar.validate().is_ok());
        }
    }

    #[test]
    fn test_wyckoff_schematic_accumulation_reaches_phase_e() {
        let bars = wyckoff_schematic_bars(
            42,
            WyckoffBias::Accumulation,
            WyckoffGeneratorConfig::default(),
        );

        let mut machine = WyckoffStateMachine::with_defaults();
        for qb in &bars {
            machine.on_bar(&qb.bar);
        }

        assert_eq!(machine.bias(), Some(WyckoffBias::Accumulation));
        assert_eq!(machine.phase(), WyckoffPhase::E);
        let score = machine.score();
        assert!(
            score.sequence_quality >= 0.5,
            "Sequence quality was {}",
            score.sequence_quality
        );
    }

    #[test]
    fn test_wyckoff_schematic_distribution_reaches_phase_e() {
        let bars = wyckoff_schematic_bars(
            42,
            WyckoffBias::Distribution,
            WyckoffGeneratorConfig::default(),
        );

        let mut machine = WyckoffStateMachine::with_defaults();
        for qb in &bars {
            machine.on_bar(&qb.bar);
        }

        assert_eq!(machine.bias(), Some(WyckoffBias::Distribution));
        assert_eq!(machine.phase(), WyckoffPhase::E);
        let score = machine.score();
        assert!(
            score.sequence_quality >= 0.5,
            "Sequence quality was {}",
            score.sequence_quality
        );
    }

    /// The generator promises the whole schematic for any seed, scale and lookback: every event
    /// once (the Secondary Test at least once), in the order of the schematic.
    #[test]
    fn test_wyckoff_schematic_holds_for_seeds_scales_and_lookbacks() {
        use crate::indicator::wyckoff::{WyckoffEventKind as Kind, WyckoffParams};

        let configs = [
            WyckoffGeneratorConfig::default(),
            WyckoffGeneratorConfig {
                center_price: 4_250.0,
                spread: 75.0,
                base_volume: 2.5e6,
                range_lookback: 20,
            },
            WyckoffGeneratorConfig {
                center_price: 1.1,
                spread: 0.004,
                base_volume: 40.0,
                range_lookback: 35,
            },
        ];
        for config in configs {
            for seed in 0..25 {
                for (bias, expected) in [
                    (
                        WyckoffBias::Accumulation,
                        [
                            Kind::PreliminarySupport,
                            Kind::SellingClimax,
                            Kind::AutomaticRally,
                            Kind::SecondaryTest,
                            Kind::Spring,
                            Kind::SignOfStrength,
                            Kind::LastPointOfSupport,
                        ],
                    ),
                    (
                        WyckoffBias::Distribution,
                        [
                            Kind::PreliminarySupply,
                            Kind::BuyingClimax,
                            Kind::AutomaticReaction,
                            Kind::SecondaryTest,
                            Kind::Utad,
                            Kind::SignOfWeakness,
                            Kind::LastPointOfSupply,
                        ],
                    ),
                ] {
                    let bars = wyckoff_schematic_bars(seed, bias, config);
                    assert!(bars.iter().all(|qb| qb.bar.validate().is_ok()));
                    let mut machine = WyckoffStateMachine::with_params(WyckoffParams {
                        range_lookback: config.range_lookback,
                        ..WyckoffParams::default()
                    });
                    for qb in &bars {
                        machine.on_bar(&qb.bar);
                    }
                    let mut kinds: Vec<Kind> = machine.events().iter().map(|e| e.kind).collect();
                    kinds.dedup();
                    assert_eq!(kinds, expected, "seed {seed}, {bias:?}, {config:?}");
                    assert_eq!(machine.phase(), WyckoffPhase::E);
                    assert_eq!(machine.score().sequence_quality, 1.0);

                    let range = machine.range().expect("range set");
                    let (low, high) = (
                        config.center_price - config.spread,
                        config.center_price + config.spread,
                    );
                    let tolerance = 1e-9 * config.center_price;
                    assert!((range.low - low).abs() < tolerance, "{range:?}");
                    assert!((range.high - high).abs() < tolerance, "{range:?}");
                }
            }
        }
    }

    #[test]
    fn test_bos_choch_swing_bars_bullish() {
        let pivot_len = 3;
        let bars = bos_choch_swing_bars(101, SwingDirection::Bullish, pivot_len);
        let mut engine = BosChochEngine::new(pivot_len);

        let mut event_codes = Vec::new();
        for qb in &bars {
            if let Some(out) = engine.on_bar(&qb.bar) {
                event_codes.push(out.value);
            }
        }

        assert!(
            event_codes.iter().any(|&c| c > 0.0),
            "Bullish swing bars must trigger Bullish BOS/CHoCH event (> 0)"
        );
    }

    #[test]
    fn test_bos_choch_swing_bars_bearish() {
        let pivot_len = 3;
        let bars = bos_choch_swing_bars(101, SwingDirection::Bearish, pivot_len);
        let mut engine = BosChochEngine::new(pivot_len);

        let mut event_codes = Vec::new();
        for qb in &bars {
            if let Some(out) = engine.on_bar(&qb.bar) {
                event_codes.push(out.value);
            }
        }

        assert!(
            event_codes.iter().any(|&c| c < 0.0),
            "Bearish swing bars must trigger Bearish BOS/CHoCH event (< 0)"
        );
    }

    #[test]
    fn swing_bars_report_pivot_and_broken_level() {
        use crate::indicator::pivots_structure::PivotStructureEngine;

        let pivot_len = 3;
        let bars = bos_choch_swing_bars(101, SwingDirection::Bullish, pivot_len);

        // The pivot high (100.5) is confirmed `pivot_len` bars after it.
        let mut engine = PivotStructureEngine::new(pivot_len, pivot_len, 10);
        let confirmed: Vec<(usize, f64, f64)> = bars
            .iter()
            .enumerate()
            .filter_map(|(i, qb)| {
                let out = engine.on_bar(&qb.bar)?;
                Some((i, *out.extra.get("pivot_high")?, out.extra["pivot_offset"]))
            })
            .collect();
        assert_eq!(confirmed, vec![(2 * pivot_len, 100.5, pivot_len as f64)]);

        // The break bar reports the level it closed beyond.
        let mut engine = BosChochEngine::new(pivot_len);
        let levels: Vec<(usize, f64)> = bars
            .iter()
            .enumerate()
            .filter_map(|(i, qb)| Some((i, *engine.on_bar(&qb.bar)?.extra.get("broken_level")?)))
            .collect();
        assert_eq!(levels, vec![(2 * pivot_len + 1, 100.5)]);
    }

    #[test]
    fn shaped_swing_bars_have_bodies_and_keep_the_pivot() {
        for direction in [SwingDirection::Bullish, SwingDirection::Bearish] {
            let pivot_len = 3;
            let plain = bos_choch_swing_bars(101, direction, pivot_len);
            let shaped =
                bos_choch_swing_bars_shaped(101, direction, pivot_len, SwingCandleShape::default());
            assert_eq!(plain.len(), shaped.len());

            // Pivot and break bar keep their high and low.
            for i in [pivot_len, plain.len() - 1] {
                assert_eq!(
                    plain[i].bar.high, shaped[i].bar.high,
                    "{direction:?} high {i}"
                );
                assert_eq!(plain[i].bar.low, shaped[i].bar.low, "{direction:?} low {i}");
            }
            // Every bar around the swing has a body and a wick on each side.
            for qb in &shaped[..2 * pivot_len + 1] {
                let b = &qb.bar;
                let (top, bottom) = (b.open.max(b.close), b.open.min(b.close));
                assert!(top - bottom > 0.1, "body missing");
                assert!(b.high - top > 0.05 && bottom - b.low > 0.05, "wick missing");
            }

            let mut engine = BosChochEngine::new(pivot_len);
            let mut codes = Vec::new();
            for qb in &shaped {
                if let Some(out) = engine.on_bar(&qb.bar) {
                    codes.push(out.value);
                }
            }
            let hit = match direction {
                SwingDirection::Bullish => codes.iter().any(|&c| c > 0.0),
                SwingDirection::Bearish => codes.iter().any(|&c| c < 0.0),
            };
            assert!(hit, "{direction:?}: shaped bars must still break the pivot");
        }
    }
}

// ---------------------------------------------------------------------------
// Pattern constructors
// ---------------------------------------------------------------------------
//
// The generators above answer "what does a market series look like". These answer the inverse
// question: "which series contains *this* pattern, at *this* place". Documentation and teaching
// material needs the inverse — a pattern is constructed on purpose rather than hunted for.
//
// They are deliberately calculation, not drawing: a renderer that assembled its own bars would be
// a second source of truth next to the detectors that are supposed to confirm them.

/// A single candle described by scale-free ratios instead of absolute prices.
///
/// A twenty-point wick is large at an ATR of thirty and irrelevant at four hundred. Candle
/// definitions are therefore stated as ratios, and this struct is that statement made explicit —
/// which also makes it invertible: [`bar_from_shape`] turns the condition back into a bar that
/// satisfies it.
///
/// The three ratios describe how the range is divided and are normalised to sum to one, so
/// `CandleShape { body_ratio: 2.0, upper_wick_ratio: 1.0, lower_wick_ratio: 1.0, .. }` and
/// `{ 0.5, 0.25, 0.25 }` describe the same candle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CandleShape {
    /// `|close - open| / (high - low)`.
    pub body_ratio: f64,
    /// `(high - max(open, close)) / (high - low)`.
    pub upper_wick_ratio: f64,
    /// `(min(open, close) - low) / (high - low)`.
    pub lower_wick_ratio: f64,
    /// `(high - low) / atr` — how large the candle is relative to recent volatility.
    pub relative_range: f64,
    /// `close > open`.
    pub bullish: bool,
}

impl CandleShape {
    /// A candle with the given body ratio, the remaining range split evenly between the wicks.
    pub fn with_body(body_ratio: f64, relative_range: f64, bullish: bool) -> Self {
        let rest = (1.0 - body_ratio.clamp(0.0, 1.0)) / 2.0;
        Self {
            body_ratio: body_ratio.clamp(0.0, 1.0),
            upper_wick_ratio: rest,
            lower_wick_ratio: rest,
            relative_range,
            bullish,
        }
    }

    /// The three range shares, normalised to sum to one.
    ///
    /// Returns an even three-way split for a degenerate all-zero input rather than dividing by
    /// zero — a candle with no range is not expressible as OHLC anyway.
    fn normalised(&self) -> (f64, f64, f64) {
        let body = self.body_ratio.max(0.0);
        let upper = self.upper_wick_ratio.max(0.0);
        let lower = self.lower_wick_ratio.max(0.0);
        let sum = body + upper + lower;
        if sum <= f64::EPSILON {
            return (1.0 / 3.0, 1.0 / 3.0, 1.0 / 3.0);
        }
        (body / sum, upper / sum, lower / sum)
    }
}

/// Builds the bar that satisfies a [`CandleShape`], opening at `open_price`.
///
/// This is the normalisation of the candle metrics read backwards. A "hammer" stops being a bar
/// that happens to look like one and becomes the condition itself, solved for OHLC — which is what
/// makes a documentation figure reproducible instead of drawn.
///
/// The bar opens at `open_price` so it can be appended to a running series; `atr` sets its size.
pub fn bar_from_shape(
    timestamp: i64,
    shape: &CandleShape,
    open_price: f64,
    atr: f64,
    volume: f64,
) -> QualifiedBar {
    let (body_ratio, upper_ratio, lower_ratio) = shape.normalised();
    let range = (shape.relative_range * atr).max(f64::EPSILON);
    let body = body_ratio * range;
    let upper = upper_ratio * range;
    let lower = lower_ratio * range;

    let (open, high, low, close) = if shape.bullish {
        let open = open_price;
        let close = open + body;
        (open, close + upper, open - lower, close)
    } else {
        let open = open_price;
        let close = open - body;
        (open, open + upper, close - lower, close)
    };

    synthetic_bar(timestamp, open, high, low, close, volume.max(0.0))
}

/// A turning point of a constructed series: the bar index it falls on and its price.
///
/// Whether it is a peak or a trough follows from its neighbours and is not stated separately —
/// a pivot list that disagreed with itself about direction would be the first thing to go wrong.
pub type Pivot = (usize, f64);

/// How much of the room between a bar and the pivot it heads towards may be spent on that bar.
///
/// Below one, a bar cannot reach past the pivot it is approaching. The remaining slack keeps the
/// margin visible rather than hairline.
const PIVOT_PATH_BUDGET: f64 = 0.9;

/// Upper bound on a bar's excursion in units of the local per-bar step, regardless of headroom.
///
/// Without it a long leg would grow ever larger bars towards its middle. Two and a half steps is
/// roughly the ratio between bar range and bar-to-bar drift that an ordinary series shows.
const PIVOT_PATH_MAX_STEPS: f64 = 2.5;

/// Builds a bar series that passes through the given pivots, in order.
///
/// A chart pattern is a condition on consecutive pivots, so a series containing a given pattern is
/// a pivot list: a head and shoulders is five pivots, a double top is three, a flag is an impulse
/// plus a narrow counter-channel. This turns the pattern definition into the series that satisfies
/// it, rather than searching for one.
///
/// Two properties hold by construction, and they are what a detector needs:
///
/// - the extreme of each pivot bar equals the pivot price exactly — the high at a peak, the low at
///   a trough, with the body sitting on the inside;
/// - no other bar between two pivots reaches past either of them.
///
/// The second is bought by scaling each bar to **its own distance from the nearer pivot**: a bar
/// one step away from a peak may only be small, one in the middle of a leg may be large. A single
/// global bound would have to assume the tightest spot everywhere and would draw a ruler instead
/// of a chart. Without the property the series would contain a different pattern than the one it
/// was built from — the one failure mode that goes unnoticed, because the picture still looks
/// right.
///
/// `liveliness` runs from 0 (a clean path) to 1 (as much movement as the bound allows) and is
/// clamped to that range. Pivot indices must be strictly increasing; anything else yields an empty
/// series rather than a silently wrong one.
pub fn bars_from_pivots(
    seed: u64,
    pivots: &[Pivot],
    liveliness: f64,
    volume: f64,
) -> Vec<QualifiedBar> {
    if pivots.len() < 2 || pivots.windows(2).any(|w| w[0].0 >= w[1].0) {
        return Vec::new();
    }

    let lively = liveliness.clamp(0.0, 1.0);
    let mut rng = SimpleRng::new(seed);
    let last = pivots[pivots.len() - 1].0;
    let mut bars = Vec::with_capacity(last + 1);
    let mut segment = 0usize;

    for i in 0..=last {
        while segment + 1 < pivots.len() && i > pivots[segment + 1].0 {
            segment += 1;
        }
        let (from_i, from_p) = pivots[segment];
        let (to_i, to_p) = pivots[segment + 1];
        let span = (to_i - from_i) as f64;
        let step = (to_p - from_p).abs() / span;
        let t = (i - from_i) as f64 / span;
        let path = from_p + (to_p - from_p) * t;
        let rising = to_p > from_p;

        // Distance to the nearer of the two pivots, in bars. That distance is the headroom.
        let distance = (i - from_i).min(to_i - i) as f64;
        let room = (PIVOT_PATH_BUDGET * distance * step).min(PIVOT_PATH_MAX_STEPS * step) * lively;

        let jitter = rng.next_gaussian().clamp(-1.0, 1.0) * room * 0.35;
        let half_body = rng.next_range(0.35, 1.0) * room * 0.30;
        let upper_wick = rng.next_f64() * room * 0.30;
        let lower_wick = rng.next_f64() * room * 0.30;
        let bar_volume = (volume + rng.next_range(-0.05, 0.05) * volume).max(0.0);

        // The pivot bar has no headroom but still needs a body. It gets the one a bar a single
        // step away would have, laid entirely on the inside.
        let pivot_body = PIVOT_PATH_BUDGET * step * 0.35;
        let (open, high, low, close) = match pivot_is_peak(pivots, i) {
            Some(true) => (
                path - pivot_body,
                path,
                path - pivot_body - lower_wick,
                path - pivot_body * 0.4,
            ),
            Some(false) => (
                path + pivot_body,
                path + pivot_body + upper_wick,
                path,
                path + pivot_body * 0.4,
            ),
            None => {
                let center = path + jitter;
                let (open, close) = if rising {
                    (center - half_body, center + half_body)
                } else {
                    (center + half_body, center - half_body)
                };
                (
                    open,
                    open.max(close) + upper_wick,
                    open.min(close) - lower_wick,
                    close,
                )
            }
        };

        bars.push(synthetic_bar(
            i as i64 * 60,
            open,
            high,
            low,
            close,
            bar_volume,
        ));
    }

    bars
}

/// `Some(true)` if bar `i` is a peak pivot, `Some(false)` for a trough, `None` if it is neither.
///
/// The first and last pivot have only one neighbour; their direction follows from that side alone.
fn pivot_is_peak(pivots: &[Pivot], i: usize) -> Option<bool> {
    let at = pivots.iter().position(|(index, _)| *index == i)?;
    let price = pivots[at].1;
    let neighbour = if at == 0 {
        pivots[1].1
    } else {
        pivots[at - 1].1
    };
    Some(price > neighbour)
}

/// The three ratios that distinguish one harmonic pattern from another.
///
/// The patterns do not differ in shape — every one of them is a five-point zigzag. They differ in
/// these numbers, which is why a figure for them is worth generating from the same table the
/// documentation prints rather than drawing twice.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HarmonicRatios {
    /// How far B retraces the XA leg.
    pub b: f64,
    /// How far C retraces the AB leg.
    pub c: f64,
    /// Where D sits relative to XA — below one it stays inside XA, above one it extends past X.
    pub d: f64,
}

impl HarmonicRatios {
    /// Gartley: B at 0.618 of XA, D at 0.786 — the retracement case, D stays inside XA.
    pub const GARTLEY: Self = Self {
        b: 0.618,
        c: 0.5,
        d: 0.786,
    };
    /// Bat: a shallower B and a deeper D than Gartley, still a retracement.
    pub const BAT: Self = Self {
        b: 0.5,
        c: 0.5,
        d: 0.886,
    };
    /// Butterfly: D extends past X — the extension case.
    pub const BUTTERFLY: Self = Self {
        b: 0.786,
        c: 0.5,
        d: 1.27,
    };
}

/// The five prices X, A, B, C, D of a harmonic structure, from its ratio table.
///
/// ```text
/// B = A − b · (A − X)
/// C = B + c · (A − B)
/// D = A − d · (A − X)
/// ```
///
/// Works in both directions: a bullish structure has `A > X`, a bearish one `A < X`, and the
/// signs carry through unchanged.
pub fn xabcd_prices(x: f64, a: f64, ratios: &HarmonicRatios) -> [f64; 5] {
    let xa = a - x;
    let b = a - ratios.b * xa;
    let c = b + ratios.c * (a - b);
    let d = a - ratios.d * xa;
    [x, a, b, c, d]
}

/// The same five points as a pivot list at regular spacing, ready for [`bars_from_pivots`].
///
/// `spacing` is the number of bars between consecutive points. `start` shifts the whole structure
/// so it can be placed inside a longer series.
pub fn xabcd_pivots(
    start: usize,
    spacing: usize,
    x: f64,
    a: f64,
    ratios: &HarmonicRatios,
) -> Vec<Pivot> {
    xabcd_prices(x, a, ratios)
        .into_iter()
        .enumerate()
        .map(|(i, price)| (start + i * spacing.max(1), price))
        .collect()
}
