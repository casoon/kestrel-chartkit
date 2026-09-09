mod common;

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::Bar;
use std::collections::HashMap;

const GOLDEN: &str = include_str!("fixtures/golden_moving_averages.txt");

fn expected(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

// Shared 10-bar close series for the moving-average family, see
// tests/fixtures/golden_moving_averages.txt for the independently derived reference values.
const CLOSES: [f64; 10] = [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0];

fn run_last(name: &str) -> f64 {
    let mut indicator = build_checked(name, &HashMap::from([("period".to_string(), 5.0)])).unwrap();
    let mut last = None;
    for (i, &c) in CLOSES.iter().enumerate() {
        let bar = Bar::new(i as i64, c, c + 0.5, c - 0.5, c, 1000.0);
        if let Some(out) = indicator.on_bar(&bar) {
            last = Some(out.value);
        }
    }
    last.expect("indicator produced no output")
}

#[test]
fn test_golden_sma_reference_values() {
    common::assert_close(
        run_last("sma"),
        expected("sma5_last"),
        expected("ma_tolerance"),
        "SMA(5)",
    );
}

#[test]
fn test_golden_ema_reference_values() {
    common::assert_close(
        run_last("ema"),
        expected("ema5_last"),
        expected("ma_tolerance"),
        "EMA(5)",
    );
}

#[test]
fn test_golden_wma_reference_values() {
    common::assert_close(
        run_last("wma"),
        expected("wma5_last"),
        expected("ma_tolerance"),
        "WMA(5)",
    );
}

#[test]
fn test_golden_vwma_reference_values() {
    common::assert_close(
        run_last("vwma"),
        expected("vwma5_last"),
        expected("ma_tolerance"),
        "VWMA(5)",
    );
}

#[test]
fn test_golden_hma_reference_values() {
    common::assert_close(
        run_last("hma"),
        expected("hma5_last"),
        expected("ma_tolerance"),
        "HMA(5)",
    );
}

#[test]
fn test_golden_dema_reference_values() {
    common::assert_close(
        run_last("dema"),
        expected("dema5_last"),
        expected("ma_tolerance"),
        "DEMA(5)",
    );
}

#[test]
fn test_golden_kama_reference_values() {
    common::assert_close(
        run_last("kama"),
        expected("kama5_last"),
        expected("ma_tolerance"),
        "KAMA(5)",
    );
}

#[test]
fn test_golden_tema_reference_values() {
    common::assert_close(
        run_last("tema"),
        expected("tema5_last"),
        expected("ma_tolerance"),
        "TEMA(5)",
    );
}

#[test]
fn test_golden_lsma_reference_values() {
    common::assert_close(
        run_last("lsma"),
        expected("lsma5_last"),
        expected("ma_tolerance"),
        "LSMA(5)",
    );
}

#[test]
fn test_golden_mcginley_reference_values() {
    common::assert_close(
        run_last("mcginley"),
        expected("mcginley5_last"),
        expected("ma_tolerance"),
        "McGinley(5)",
    );
}
