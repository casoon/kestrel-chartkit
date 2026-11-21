//! The own constructions carry the `kestrel_` prefix in the catalog; the name they had before
//! stays an alias for building and lookup.

mod common;

use common::wave_series;
use kestrel_chartkit::indicator::definition::definition;
use kestrel_chartkit::indicator::registry::{build_checked, canonical_name, catalog, output_range};
use kestrel_chartkit::indicator::unit::output_unit;
use kestrel_chartkit::indicator::Indicator;
use std::collections::{BTreeMap, HashMap};

const OLD_NAMES: [&str; 25] = [
    "overextension_risk",
    "reversal_setup_score",
    "sweep_reversal_score",
    "trend_break_risk",
    "tradability_filter",
    "market_state_chain",
    "coil_release",
    "compression_zones",
    "rsi_cycle_stats",
    "price_average_relation",
    "structural_flow",
    "structure_zones",
    "leg_conviction",
    "reaction_speed",
    "leg_character",
    "accumulation_phase",
    "bottom_formation",
    "equal_level_liquidity",
    "swing_structure",
    "trend_quality",
    "buy_sell_pressure",
    "multi_factor",
    "price_location",
    "vwap_regime",
    "flow_exhaustion",
];

/// Every output of a run as text, floats as bit patterns, `extra` sorted.
fn fingerprint(indicator: &mut dyn Indicator) -> Vec<String> {
    let mut out = Vec::new();
    for bar in wave_series(600) {
        let output = indicator.on_bar(&bar).map(|o| {
            let extra: BTreeMap<_, _> = o
                .extra
                .iter()
                .map(|(k, v)| (k.clone(), v.to_bits()))
                .collect();
            format!(
                "{:x} {:?} {:?} {extra:?} {:?} {:?} {:?}",
                o.value.to_bits(),
                o.secondary.map(f64::to_bits),
                o.signal.map(f64::to_bits),
                o.state,
                o.reason,
                o.artifacts
            )
        });
        out.push(format!("{output:?}"));
        out.push(format!("{:?}", indicator.alerts()));
    }
    out
}

#[test]
fn old_names_resolve_to_the_prefixed_entry() {
    for old in OLD_NAMES {
        let new = format!("kestrel_{old}");
        assert_eq!(canonical_name(old), new, "{old}");
        assert_eq!(canonical_name(&new), new, "{new}");
        assert_eq!(definition(old).expect(old).name, new, "{old}");
        assert_eq!(definition(&new).expect(&new).name, new, "{new}");
        assert_eq!(output_unit(old), output_unit(&new), "{old}");
        assert_eq!(output_range(old), output_range(&new), "{old}");
    }
}

#[test]
fn old_and_new_name_build_the_same_indicator() {
    for old in OLD_NAMES {
        let new = format!("kestrel_{old}");
        let mut by_old = build_checked(old, &HashMap::new()).expect(old);
        let mut by_new = build_checked(&new, &HashMap::new()).expect(&new);
        assert_eq!(by_old.name(), new, "{old}");
        assert_eq!(by_new.name(), new, "{new}");
        assert_eq!(
            fingerprint(by_old.as_mut()),
            fingerprint(by_new.as_mut()),
            "{old}"
        );
    }
}

#[test]
fn kestrel_entries_describe_themselves_as_kestrel() {
    let mut count = 0;
    for entry in catalog() {
        if entry.name.starts_with("kestrel_") {
            count += 1;
            assert!(
                entry.description.starts_with("Kestrel "),
                "{}: {}",
                entry.name,
                entry.description
            );
        }
    }
    assert!(count >= OLD_NAMES.len());
}
