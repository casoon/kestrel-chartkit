//! `settle_bars` is a measurement on a few reference paths. It is only useful if it also holds on
//! a path it was not measured on: different seed, drift, phase length and noise, and second starts
//! that are not among the measured ones.

use kestrel_chartkit::indicator::{registry, settle};

#[test]
fn settle_bars_holds_on_a_differently_shaped_series() {
    let other = settle::reference_series(900, 0x2545_f491_4f6c_dd1d, 0.15, 60, 1.2);
    let mut failures = Vec::new();
    for (criterion, entry) in registry::catalog().into_iter().flat_map(|e| {
        [settle::Criterion::Outputs, settle::Criterion::Alerts].map(move |c| (c, e.clone()))
    }) {
        let declared = match criterion {
            settle::Criterion::Outputs => {
                settle::settle_bars(entry.name, &entry.default_params, 900)
            }
            settle::Criterion::Alerts => {
                settle::alert_settle_bars(entry.name, &entry.default_params, 900)
            }
        };
        let Some(declared) = declared else {
            continue;
        };
        for late_start in [450, 1300, 2100] {
            match settle::measure(entry.name, &entry.default_params, &other, late_start, criterion) {
                Some(measured) if measured <= declared => {}
                Some(measured) => failures.push(format!(
                    "{} ({criterion:?}): declared {declared}, settles only after {measured} (late start {late_start})",
                    entry.name
                )),
                None => failures.push(format!(
                    "{} ({criterion:?}): declared {declared}, never settles on the second series (late start {late_start})",
                    entry.name
                )),
            }
        }
    }
    assert!(
        failures.is_empty(),
        "settle_bars too short:\n  {}",
        failures.join("\n  ")
    );
}

#[test]
fn settle_bars_is_never_shorter_than_warmup() {
    for entry in registry::catalog() {
        let Some(settle) = settle::settle_bars(entry.name, &entry.default_params, 900) else {
            continue;
        };
        let warmup = registry::build(entry.name, &entry.default_params)
            .expect("catalog entry builds")
            .warmup_period();
        assert!(
            settle >= warmup,
            "{}: settle {settle} < warmup {warmup}",
            entry.name
        );
    }
}

/// Cumulative indicators have no start-independent level; `None` says so instead of a number.
#[test]
fn cumulative_indicators_never_settle() {
    for name in ["obv", "pvt", "acc_dist", "nvi", "cvd"] {
        let params = registry::catalog()
            .into_iter()
            .find(|e| e.name == name)
            .unwrap_or_else(|| panic!("{name} missing from catalog"))
            .default_params;
        assert_eq!(settle::settle_bars(name, &params, 900), None, "{name}");
    }
}

/// The context line of the RSI is a 100-bar Wilder RSI: it needs far longer than its first value
/// to forget the start — the case that motivated `settle_bars`.
#[test]
fn rsi_settles_well_after_its_warmup() {
    let params = registry::catalog()
        .into_iter()
        .find(|e| e.name == "rsi")
        .unwrap()
        .default_params;
    let warmup = registry::build("rsi", &params).unwrap().warmup_period();
    let settle = settle::settle_bars("rsi", &params, 900).unwrap();
    assert!(settle > 2 * warmup, "settle {settle}, warmup {warmup}");
}

/// The alert-based measure is what a backtest warms up with; for the RSI it is far shorter than
/// the numeric one, which is why there are two.
#[test]
fn rsi_alerts_settle_before_its_outputs() {
    let params = registry::catalog()
        .into_iter()
        .find(|e| e.name == "rsi")
        .unwrap()
        .default_params;
    let alerts = settle::alert_settle_bars("rsi", &params, 900).unwrap();
    let outputs = settle::settle_bars("rsi", &params, 900).unwrap();
    assert!(alerts * 2 < outputs, "alerts {alerts}, outputs {outputs}");
}
