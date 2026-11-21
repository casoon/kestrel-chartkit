//! The lines that are measured: the smoothers of the catalog over a sweep of lengths, `kas` at
//! several settings, and the candidate formulas.

use crate::candidates::{self, Config, Core, Driver, Map};
use kestrel_chartkit::{build_checked, Bar};
use std::collections::HashMap;

pub type RunFn = dyn Fn(&[f64]) -> Vec<f64> + Send + Sync;

pub struct Line {
    pub family: String,
    pub len: usize,
    pub run: Box<RunFn>,
}

/// The lengths every family is swept over. Smoothers are compared along the whole sweep, at
/// equal smoothness, never at equal nominal length.
pub const LENGTHS: [usize; 22] = [
    3, 4, 5, 6, 8, 10, 12, 14, 17, 20, 24, 28, 34, 40, 48, 57, 68, 80, 100, 120, 150, 200,
];

/// Runs a catalog entry over a close series; `NaN` while it warms up.
fn catalog_run(name: &'static str, params: Vec<(&'static str, f64)>) -> Box<RunFn> {
    Box::new(move |xs: &[f64]| {
        let map: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let mut indicator =
            build_checked(name, &map).unwrap_or_else(|e| panic!("{name} {params:?}: {e}"));
        xs.iter()
            .enumerate()
            .map(|(i, &x)| {
                indicator
                    .on_bar(&Bar::new(i as i64 * 60, x, x, x, x, 1.0))
                    .map_or(f64::NAN, |o| o.value)
            })
            .collect()
    })
}

/// The parameters of a catalog smoother at sweep length `len`. Smoothers with more than one
/// length keep the proportions of their defaults.
fn catalog_params(name: &str, len: usize) -> Vec<(&'static str, f64)> {
    let l = len as f64;
    match name {
        "kama" => vec![
            ("period", l),
            ("fast_period", 2.0),
            ("slow_period", (3.0 * l).max(3.0)),
        ],
        "vidya" => vec![("ema_len", l), ("cmo_len", (0.75 * l).round().max(2.0))],
        _ => vec![("period", l)],
    }
}

/// The comparison set: every smoother of the catalog that reads one close series and has a
/// length.
pub const CATALOG: [&str; 16] = [
    "sma",
    "ema",
    "wma",
    "hma",
    "dema",
    "tema",
    "kama",
    "vidya",
    "t3",
    "mcginley",
    "lsma",
    "alma",
    "rma",
    "super_smoother",
    "jma",
    "ultimate_smoother",
];

pub fn catalog_lines() -> Vec<Line> {
    let mut lines = Vec::new();
    for name in CATALOG {
        for len in LENGTHS {
            lines.push(Line {
                family: name.to_string(),
                len,
                run: catalog_run(name, catalog_params(name, len)),
            });
        }
    }
    lines
}

/// `kas` at its defaults and at other settings, each a family of its own: name, phase, adapt.
/// The last one is the setting the candidate formulas are compared with.
pub const KAS: [(&str, f64, f64); 9] = [
    ("kas", 0.5, 0.5),
    ("kas_a0", 0.5, 0.0),
    ("kas_a100", 0.5, 1.0),
    ("kas_a150", 0.5, 1.5),
    ("kas_p0", 0.0, 0.5),
    ("kas_p100", 1.0, 0.5),
    ("kas_p150", 1.5, 0.5),
    ("kas_p200", 2.0, 0.5),
    ("kas_p100_a100", 1.0, 1.0),
];

pub fn kas_lines() -> Vec<Line> {
    let mut lines = Vec::new();
    for (name, phase, adapt) in KAS {
        for len in LENGTHS {
            lines.push(Line {
                family: name.to_string(),
                len,
                run: catalog_run(
                    "kestrel_adaptive_smoother",
                    vec![("period", len as f64), ("phase", phase), ("adapt", adapt)],
                ),
            });
        }
    }
    lines
}

/// The candidate formulas by name. All but the first three use the lead cascade at phase 1.
pub fn candidate_configs() -> Vec<(&'static str, Config)> {
    let lead = |driver| Config::new(Core::Lead, driver).phase(1.0);
    vec![
        (
            "cand_one_velocity",
            Config::new(Core::One, Driver::Velocity),
        ),
        (
            "cand_one_efficiency",
            Config::new(Core::One, Driver::Efficiency),
        ),
        (
            "cand_two_velocity",
            Config::new(Core::Two, Driver::Velocity),
        ),
        ("cand_fixed", lead(Driver::None)),
        ("cand_velocity", lead(Driver::Velocity)),
        (
            "cand_velocity_short",
            lead(Driver::Velocity).velocity_share(0.5),
        ),
        (
            "cand_velocity_long",
            lead(Driver::Velocity).velocity_share(2.0),
        ),
        (
            "cand_velocity_memory16",
            lead(Driver::Velocity).volatility_multiple(16.0),
        ),
        ("cand_velocity_floor", lead(Driver::Velocity).floor(1.0)),
        (
            "cand_velocity_half_strength",
            lead(Driver::Velocity).adapt(0.5),
        ),
        (
            "cand_velocity_power",
            lead(Driver::Velocity).map(Map::Power { lo: 0.25, hi: 8.0 }),
        ),
        ("cand_smoothed_efficiency", lead(Driver::SmoothedEfficiency)),
        ("cand_efficiency", lead(Driver::Efficiency)),
        ("cand_relative_volatility", lead(Driver::RelativeVolatility)),
        ("cand_variance_ratio", lead(Driver::VarianceRatio)),
    ]
}

pub fn candidate_lines() -> Vec<Line> {
    let mut lines = Vec::new();
    for (name, cfg) in candidate_configs() {
        for len in LENGTHS {
            lines.push(Line {
                family: name.to_string(),
                len,
                run: Box::new(move |xs: &[f64]| candidates::run(cfg, len, xs)),
            });
        }
    }
    lines
}
