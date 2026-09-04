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

/// Minimum effective range lookback this generator calibrates against.
///
/// Matches [`WyckoffStateMachine::with_defaults`](crate::indicator::wyckoff::WyckoffStateMachine::with_defaults)'s
/// `range_lookback` and gives both `Rma::new(14)` (ATR warmup) and the robust MAD-based
/// volume-outlier window enough samples to be statistically meaningful before the directed climax
/// bar arrives. Smaller `WyckoffStateMachine`/`"wyckoff"` configurations are structurally
/// supported (deque and volume-outlier window are derived from the same `range_lookback` there,
/// see `src/indicator/wyckoff.rs`), but a climax-outlier band computed from very few samples is
/// noisy, so this generator does not tune below the default for its forced-bias guarantee.
const WYCKOFF_MIN_RANGE_LOOKBACK: usize = 20;

/// Configuration parameters for generating Wyckoff schematic bar sequences.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WyckoffGeneratorConfig {
    /// Baseline price around which the trading range contracts.
    pub center_price: f64,
    /// Number of bars used for the lookback window.
    ///
    /// Internally clamped up to [`WYCKOFF_MIN_RANGE_LOOKBACK`] regardless of the value supplied
    /// here — see [`WYCKOFF_MIN_RANGE_LOOKBACK`] for why.
    pub range_lookback: usize,
    /// Price spread / half-width of the trading range.
    pub spread: f64,
    /// Baseline volume during the range phase.
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

/// Generates a calibrated Wyckoff schematic sequence completing Phases A through E.
///
/// The sequence consists of:
/// 1. Warmup contraction range bars establishing baseline ATR and volume statistics.
/// 2. Directed climax bar (high volume down-close for Accumulation, up-close for Distribution)
///    locking the trading range with the target [`WyckoffBias`] deterministically.
/// 3. Secondary boundary test / pause bar (Phase B).
/// 4. Decisive Spring (Accumulation) or UTAD (Distribution) test bar (Phase C).
/// 5. Sign of Strength (Accumulation) or Sign of Weakness (Distribution) breakout bar (Phase D).
/// 6. Last Point of Support (Accumulation) or Last Point of Supply (Distribution) confirmation bar (Phase E).
pub fn wyckoff_schematic_bars(
    seed: u64,
    bias: WyckoffBias,
    config: WyckoffGeneratorConfig,
) -> Vec<QualifiedBar> {
    let mut rng = SimpleRng::new(seed);
    let warmup_bars = config
        .range_lookback
        .max(WYCKOFF_MIN_RANGE_LOOKBACK)
        .saturating_sub(1);
    let mut bars = Vec::with_capacity(warmup_bars + 6);
    let center = config.center_price;
    let spread = config.spread;
    let base_volume = config.base_volume;

    // 1. Warmup oscillating range bars (contracting range, stable ATR and volume)
    // Runs for exactly `lookback - 1` bars so range lock evaluates on the subsequent climax bar.
    for i in 0..warmup_bars {
        let pattern_offset = ((i % 4) as f64 - 1.5) * spread * 0.15;
        let noise = rng.next_range(-0.02, 0.02) * spread;
        let price = center + pattern_offset + noise;
        let open = price - 0.05 * spread;
        let close = price + 0.05 * spread;
        let high = price + spread * 0.2;
        let low = price - spread * 0.2;
        let vol = base_volume + rng.next_range(-1.0, 1.0);

        bars.push(synthetic_bar(i as i64 * 60, open, high, low, close, vol));
    }

    let mut timestamp = warmup_bars as i64 * 60;

    // 2. Climax Bar: Evaluated as the `lookback`-th bar. Outlier volume + directional close
    // locks the range and picks the bias deterministically.
    let climax_vol = base_volume * 4.0;
    let (c_open, c_high, c_low, c_close) = match bias {
        WyckoffBias::Accumulation => {
            // Down climax: close < open and close < prev_close
            (
                center + 0.2 * spread,
                center + 0.3 * spread,
                center - 0.4 * spread,
                center - 0.3 * spread,
            )
        }
        WyckoffBias::Distribution => {
            // Up climax: close > open and close > prev_close
            (
                center - 0.2 * spread,
                center + 0.4 * spread,
                center - 0.3 * spread,
                center + 0.3 * spread,
            )
        }
    };
    bars.push(synthetic_bar(
        timestamp, c_open, c_high, c_low, c_close, climax_vol,
    ));
    timestamp += 60;

    // 3. Decisive Test (Phase A/B -> Phase C): Spring (Accumulation) or UTAD (Distribution)
    match bias {
        WyckoffBias::Accumulation => {
            // Spring: low < range_low && close > range_low
            let s_open = center - 0.2 * spread;
            let s_low = center - 1.5 * spread;
            let s_high = center;
            let s_close = center - 0.1 * spread;
            bars.push(synthetic_bar(
                timestamp,
                s_open,
                s_high,
                s_low,
                s_close,
                base_volume,
            ));
        }
        WyckoffBias::Distribution => {
            // UTAD: high > range_high && close < range_high
            let u_open = center + 0.2 * spread;
            let u_high = center + 1.5 * spread;
            let u_low = center;
            let u_close = center + 0.1 * spread;
            bars.push(synthetic_bar(
                timestamp,
                u_open,
                u_high,
                u_low,
                u_close,
                base_volume,
            ));
        }
    }
    timestamp += 60;

    // 4. Breakout (Phase C -> Phase D): SOS (Accumulation) or SOW (Distribution)
    match bias {
        WyckoffBias::Accumulation => {
            // SOS: close > range_high
            let sos_open = center;
            let sos_high = center + 1.6 * spread;
            let sos_low = center - 0.1 * spread;
            let sos_close = center + 1.5 * spread;
            bars.push(synthetic_bar(
                timestamp,
                sos_open,
                sos_high,
                sos_low,
                sos_close,
                base_volume * 1.5,
            ));
        }
        WyckoffBias::Distribution => {
            // SOW: close < range_low
            let sow_open = center;
            let sow_low = center - 1.6 * spread;
            let sow_high = center + 0.1 * spread;
            let sow_close = center - 1.5 * spread;
            bars.push(synthetic_bar(
                timestamp,
                sow_open,
                sow_high,
                sow_low,
                sow_close,
                base_volume * 1.5,
            ));
        }
    }
    timestamp += 60;

    // 5. Confirmation (Phase D -> Phase E): LPS (Accumulation) or LPSY (Distribution)
    match bias {
        WyckoffBias::Accumulation => {
            // LPS: low >= range_high - atr * 0.5 && close > range_high
            let lps_open = center + 1.2 * spread;
            let lps_low = center + 0.8 * spread;
            let lps_high = center + 1.5 * spread;
            let lps_close = center + 1.3 * spread;
            bars.push(synthetic_bar(
                timestamp,
                lps_open,
                lps_high,
                lps_low,
                lps_close,
                base_volume,
            ));
        }
        WyckoffBias::Distribution => {
            // LPSY: high <= range_low + atr * 0.5 && close < range_low
            let lpsy_open = center - 1.2 * spread;
            let lpsy_high = center - 0.8 * spread;
            let lpsy_low = center - 1.5 * spread;
            let lpsy_close = center - 1.3 * spread;
            bars.push(synthetic_bar(
                timestamp,
                lpsy_open,
                lpsy_high,
                lpsy_low,
                lpsy_close,
                base_volume,
            ));
        }
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
        let dist = (i as isize - len as isize).unsigned_abs() as f64;
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

        let mut machine = WyckoffStateMachine::new(20, 5.0, 3);
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

        let mut machine = WyckoffStateMachine::new(20, 5.0, 3);
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
}
