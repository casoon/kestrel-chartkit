//! Price location and Wilder ATR against `reference/kestrel_reference/fixtures/price_location.py`.

mod common;

use kestrel_chartkit::analytics::{
    price_location_series, wilder_atr_series, PriceLocation, PriceLocationConfig,
};

const GOLDEN: &str = include_str!("fixtures/golden_price_location.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn has(key: &str) -> bool {
    GOLDEN
        .lines()
        .any(|l| l.split_once('=').is_some_and(|(k, _)| k == key))
}

fn configs() -> [(&'static str, PriceLocationConfig); 2] {
    [
        ("default", PriceLocationConfig::default()),
        (
            "small",
            PriceLocationConfig {
                atr_len: 5,
                ema_len: 8,
                range_len: 12,
                sma_len: 30,
                slope_bars: 4,
                vol_window: 25,
                vol_warmup: 40,
            },
        ),
    ]
}

fn field(p: &PriceLocation, name: &str) -> Option<f64> {
    match name {
        "atr" => p.atr,
        "ema" => p.ema,
        "stretch" => p.stretch,
        "range_position" => p.range_position,
        "sma" => p.sma,
        "sma_prior" => p.sma_prior,
        "vol_tercile" => p.vol_tercile.map(f64::from),
        _ => unreachable!(),
    }
}

const FIELDS: [&str; 7] = [
    "atr",
    "ema",
    "stretch",
    "range_position",
    "sma",
    "sma_prior",
    "vol_tercile",
];

#[test]
fn test_price_location_matches_the_reference() {
    let bars = common::wave_series(330);
    let tol = value("price_location_tolerance");
    let n = value("meta_cp_count") as usize;
    for (name, cfg) in configs() {
        let out = price_location_series(&bars, cfg);
        for k in 0..n {
            let cp = value(&format!("meta_cp{k}")) as usize;
            for f in FIELDS {
                let key = format!("{name}_cp{cp}_{f}");
                let got = field(&out[cp], f);
                if has(&format!("{key}_none")) {
                    assert_eq!(got, None, "{key}");
                } else {
                    let want = value(&key);
                    let got = got.unwrap_or_else(|| panic!("{key} missing"));
                    assert!(
                        (got - want).abs() <= tol * want.abs().max(1.0),
                        "{key}: {got} vs {want}"
                    );
                }
            }
        }
    }
}

#[test]
fn test_wilder_atr_series_is_the_engine_atr() {
    let bars = common::wave_series(330);
    let atr = wilder_atr_series(&bars, 14);
    let loc = price_location_series(&bars, PriceLocationConfig::default());
    assert!(atr.iter().zip(&loc).all(|(a, l)| *a == l.atr));
    assert_eq!(atr.iter().position(Option::is_some), Some(13));
}
