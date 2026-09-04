#![allow(dead_code)]

use kestrel_chartkit::indicator::{Indicator, IndicatorOutput};
use kestrel_chartkit::model::Bar;

/// Generiert eine Flachpreis-Serie mit konstantem Preis und Volumen.
pub fn generate_flat_bars(count: usize, price: f64, volume: f64) -> Vec<Bar> {
    (0..count)
        .map(|i| Bar::new(i as i64 * 60, price, price, price, price, volume))
        .collect()
}

/// Generiert eine flache OHLC-Serie mit echtem High/Low Spread.
pub fn generate_flat_spread_bars(count: usize, price: f64, spread: f64, volume: f64) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            Bar::new(
                i as i64 * 60,
                price,
                price + spread / 2.0,
                price - spread / 2.0,
                price,
                volume,
            )
        })
        .collect()
}

/// Generiert eine Sinus-Serie (Zyklus) zur Erzeugung von Auf-/Abwärtsbewegungen.
pub fn generate_sine_bars(
    count: usize,
    base_price: f64,
    amplitude: f64,
    period: f64,
    volume: f64,
) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let angle = (i as f64) * 2.0 * std::f64::consts::PI / period;
            let close = base_price + amplitude * angle.sin();
            let high = close + amplitude.abs() * 0.1 + 0.1;
            let low = close - amplitude.abs() * 0.1 - 0.1;
            let open = (close + base_price) / 2.0;
            Bar::new(i as i64 * 60, open, high, low, close, volume)
        })
        .collect()
}

/// Generiert eine lineare Trend-Serie (Auf- oder Abwärts).
pub fn generate_trend_bars(count: usize, start_price: f64, step: f64, volume: f64) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let open = start_price + (i as f64) * step;
            let close = open + step * 0.8;
            let high = open.max(close) + step.abs() * 0.2 + 0.05;
            let low = open.min(close) - step.abs() * 0.2 - 0.05;
            Bar::new(i as i64 * 60, open, high, low, close, volume)
        })
        .collect()
}

/// Generiert eine Stufen-Serie mit einem Preissprung an einer definierten Position.
pub fn generate_step_bars(
    count: usize,
    initial_price: f64,
    step_at: usize,
    step_height: f64,
    volume: f64,
) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            let price = if i < step_at {
                initial_price
            } else {
                initial_price + step_height
            };
            Bar::new(
                i as i64 * 60,
                price,
                price + 0.1,
                price - 0.1,
                price,
                volume,
            )
        })
        .collect()
}

/// Generiert eine Serie mit 0 Volume.
pub fn generate_zero_volume_bars(count: usize, price: f64) -> Vec<Bar> {
    generate_flat_spread_bars(count, price, 1.0, 0.0)
}

/// Generiert eine Serie mit NaN/Infinity Werten zur Robustheitsprüfung.
pub fn generate_nan_bars(count: usize) -> Vec<Bar> {
    (0..count)
        .map(|i| {
            if i % 5 == 0 {
                Bar::new(
                    i as i64 * 60,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                    f64::NAN,
                )
            } else if i % 7 == 0 {
                Bar::new(
                    i as i64 * 60,
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::INFINITY,
                    1.0,
                )
            } else {
                Bar::new(i as i64 * 60, 100.0, 105.0, 95.0, 100.0, 1000.0)
            }
        })
        .collect()
}

/// Delegiert an [`kestrel_chartkit::synthetic::random_walk_bars`] und gibt unqualifizierte [`Bar`]s zurück.
pub fn generate_random_walk_bars(
    seed: u64,
    count: usize,
    start_price: f64,
    drift: f64,
    volatility: f64,
    volume: f64,
) -> Vec<Bar> {
    kestrel_chartkit::synthetic::random_walk_bars(
        seed,
        count,
        start_price,
        drift,
        volatility,
        volume,
    )
    .into_iter()
    .map(|qb| qb.bar)
    .collect()
}

/// Delegiert an [`kestrel_chartkit::synthetic::trending_bars`] und gibt unqualifizierte [`Bar`]s zurück.
pub fn generate_trending_bars(
    seed: u64,
    count: usize,
    start_price: f64,
    trend_per_bar: f64,
    noise: f64,
    volume: f64,
) -> Vec<Bar> {
    kestrel_chartkit::synthetic::trending_bars(
        seed,
        count,
        start_price,
        trend_per_bar,
        noise,
        volume,
    )
    .into_iter()
    .map(|qb| qb.bar)
    .collect()
}

/// Delegiert an [`kestrel_chartkit::synthetic::wyckoff_schematic_bars`] und gibt unqualifizierte [`Bar`]s zurück.
pub fn generate_wyckoff_schematic_bars(
    seed: u64,
    bias: kestrel_chartkit::indicator::wyckoff::WyckoffBias,
    config: kestrel_chartkit::synthetic::WyckoffGeneratorConfig,
) -> Vec<Bar> {
    kestrel_chartkit::synthetic::wyckoff_schematic_bars(seed, bias, config)
        .into_iter()
        .map(|qb| qb.bar)
        .collect()
}

/// Delegiert an [`kestrel_chartkit::synthetic::bos_choch_swing_bars`] und gibt unqualifizierte [`Bar`]s zurück.
pub fn generate_bos_choch_swing_bars(
    seed: u64,
    direction: kestrel_chartkit::synthetic::SwingDirection,
    pivot_len: usize,
) -> Vec<Bar> {
    kestrel_chartkit::synthetic::bos_choch_swing_bars(seed, direction, pivot_len)
        .into_iter()
        .map(|qb| qb.bar)
        .collect()
}

/// Führt einen Indikator über eine Bar-Serie aus und sammelt alle Outputs.
pub fn run_indicator<I: Indicator + ?Sized>(
    indicator: &mut I,
    bars: &[Bar],
) -> Vec<Option<IndicatorOutput>> {
    bars.iter().map(|b| indicator.on_bar(b)).collect()
}

/// Stellt sicher, dass ein Durchlauf über eine beliebige Bar-Serie niemals panict.
pub fn assert_no_panic<I: Indicator + ?Sized>(indicator: &mut I, bars: &[Bar]) {
    indicator.reset();
    for bar in bars {
        let res = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| indicator.on_bar(bar)));
        assert!(
            res.is_ok(),
            "Indicator '{}' panicked during execution on bar {:?}",
            indicator.name(),
            bar
        );
    }
}

/// Liest einen benannten Wert aus dem Text einer Golden-Reference-Fixture-Datei
/// (`key=value`-Zeilen, siehe `tests/fixtures/golden_*.txt`). Geteilt über alle
/// `tests/golden_reference_*.rs`-Dateien, damit das Parsing nicht pro Gruppe dupliziert wird.
pub fn golden_value(fixture_text: &str, key: &str) -> f64 {
    fixture_text
        .lines()
        .filter_map(|line| line.split_once('='))
        .find_map(|(candidate, value)| (candidate == key).then_some(value))
        .unwrap_or_else(|| panic!("missing golden value: {key}"))
        .parse()
        .unwrap_or_else(|_| panic!("invalid golden value: {key}"))
}

/// Vergleicht zwei Werte auf eine feste Toleranz, mit sprechender Fehlermeldung.
pub fn assert_close(actual: f64, expected: f64, tolerance: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "{label}: got {actual}, expected {expected} ± {tolerance}"
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    struct PanickingIndicator;
    impl Indicator for PanickingIndicator {
        fn name(&self) -> &str {
            "panicker"
        }
        fn warmup_period(&self) -> usize {
            0
        }
        fn on_bar(&mut self, _bar: &Bar) -> Option<IndicatorOutput> {
            panic!("Intentional test panic");
        }
        fn reset(&mut self) {}
        fn alerts(&self) -> Vec<kestrel_chartkit::indicator::IndicatorAlert> {
            vec![]
        }
    }

    #[test]
    #[should_panic(expected = "Indicator 'panicker' panicked")]
    fn test_assert_no_panic_catches_panics() {
        let mut ind = PanickingIndicator;
        let bars = vec![Bar::new(1000, 100.0, 101.0, 99.0, 100.0, 1000.0)];
        assert_no_panic(&mut ind, &bars);
    }

    #[test]
    fn test_synthetic_common_wrappers() {
        let rw = generate_random_walk_bars(1, 10, 100.0, 0.0, 1.0, 100.0);
        assert_eq!(rw.len(), 10);
        let tr = generate_trending_bars(1, 10, 100.0, 1.0, 0.1, 100.0);
        assert_eq!(tr.len(), 10);
        let wy = generate_wyckoff_schematic_bars(
            1,
            kestrel_chartkit::indicator::wyckoff::WyckoffBias::Accumulation,
            kestrel_chartkit::synthetic::WyckoffGeneratorConfig::default(),
        );
        assert!(!wy.is_empty());
        let bc = generate_bos_choch_swing_bars(
            1,
            kestrel_chartkit::synthetic::SwingDirection::Bullish,
            2,
        );
        assert_eq!(bc.len(), 6);
    }
}
