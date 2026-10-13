mod common;

use common::*;
use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::stoch_rsi::StochRsi;
use std::collections::HashMap;

#[test]
fn test_stoch_rsi_bounds_invariant() {
    let mut stoch_rsi = StochRsi::with_defaults();
    let bars = generate_sine_bars(350, 100.0, 15.0, 30.0, 1000.0);

    let outputs = run_indicator(&mut stoch_rsi, &bars);

    for (i, out) in outputs.iter().enumerate() {
        if let Some(out) = out {
            let k = out.value;
            assert!(
                (0.0..=100.0).contains(&k),
                "StochRSI %K out of bounds at bar {}: {}",
                i,
                k
            );
            if let Some(d) = out.secondary {
                assert!(
                    (0.0..=100.0).contains(&d),
                    "StochRSI %D out of bounds at bar {}: {}",
                    i,
                    d
                );
            }
        }
    }
}

#[test]
fn test_stoch_rsi_warmup_includes_signal_length() {
    // d_len = 200: die erste Ausgabe kommt erst mit Bar 14 + 14 + 3 + 200 - 2 = 229.
    let params: HashMap<String, f64> = [("d_len".to_string(), 200.0)].into();
    let mut stoch_rsi = build_checked("stoch_rsi", &params).unwrap();
    let warmup = stoch_rsi.warmup_period();
    let bars = generate_sine_bars(400, 100.0, 15.0, 30.0, 1000.0);
    let outputs = run_indicator(stoch_rsi.as_mut(), &bars);
    let first = outputs.iter().position(|o| o.is_some()).unwrap();
    assert_eq!(first, 228, "erste Ausgabe mit Bar 229");
    assert!(
        warmup > first,
        "warmup_period {warmup} muss die erste Ausgabe (Index {first}) abdecken"
    );
}

#[test]
fn test_stoch_rsi_zero_oversold_gives_finite_alert_strength() {
    let params: HashMap<String, f64> = [
        ("oversold".to_string(), 0.0),
        ("overbought".to_string(), 100.0),
    ]
    .into();
    let mut stoch_rsi = build_checked("stoch_rsi", &params).unwrap();
    let bars = generate_sine_bars(400, 100.0, 15.0, 30.0, 1000.0);
    let mut fired = 0;
    for bar in &bars {
        stoch_rsi.on_bar(bar);
        for alert in stoch_rsi.alerts() {
            assert!(
                (0.0..=1.0).contains(&alert.strength),
                "{} strength {}",
                alert.kind,
                alert.strength
            );
            if alert.kind == "bull_extreme" {
                fired += 1;
            }
        }
    }
    assert!(fired > 0, "Testserie muss bull_extreme auslösen");
}
