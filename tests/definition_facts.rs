//! The per-indicator facts the registry answers by name — output range, unit, data and pair
//! requirements, canonical name and how a requested price `source` is handled — recorded for
//! every catalog name, every alias (in upper case too) and a few unknown names in
//! `tests/fixtures/definition_facts.txt`.
//!
//! The snapshot was taken from the hand-kept lists these facts used to live in; moving them into
//! the definitions must leave it byte-identical. Regenerate it only for a deliberate change of a
//! fact: `cargo test --test definition_facts -- --ignored regenerate`.

mod common;

use std::collections::{BTreeSet, HashMap};

use common::generate_random_walk_bars;
use kestrel_chartkit::applicability::{data_requirements, pair_requirements};
use kestrel_chartkit::indicator::definition::definitions;
use kestrel_chartkit::indicator::other_input::{intrabar_definitions, transform_definitions};
use kestrel_chartkit::indicator::pair::pair_definitions;
use kestrel_chartkit::indicator::registry::{
    build_typed, canonical_name, catalog, output_range, ParamValue,
};
use kestrel_chartkit::indicator::unit::output_unit;
use kestrel_chartkit::model::{Bar, Source};

const FIXTURE: &str = include_str!("fixtures/definition_facts.txt");

const UNKNOWN: &[&str] = &["", "nope", "Nope", "elliott", "ELLIOTT", "swing", "cci "];

fn names() -> Vec<String> {
    let mut all: BTreeSet<String> = BTreeSet::new();
    let mut add = |name: &str, aliases: &[&str]| {
        for n in std::iter::once(name).chain(aliases.iter().copied()) {
            all.insert(n.to_string());
            all.insert(n.to_uppercase());
        }
    };
    for e in catalog() {
        add(e.name, &[]);
    }
    for d in definitions() {
        add(d.name, d.aliases);
    }
    for d in pair_definitions() {
        add(d.name, d.aliases);
    }
    for d in transform_definitions() {
        add(d.name, d.aliases);
    }
    for d in intrabar_definitions() {
        add(d.name, d.aliases);
    }
    for u in UNKNOWN {
        all.insert(u.to_string());
    }
    all.into_iter().collect()
}

/// The outputs of `name` built by `build_typed` with `params` over `bars`, or the build error.
fn run(
    name: &str,
    params: &HashMap<String, ParamValue>,
    bars: &[Bar],
) -> Result<Vec<Option<u64>>, String> {
    match build_typed(name, params) {
        Err(e) => Err(format!("err {e:?}")),
        Ok(mut indicator) => Ok(bars
            .iter()
            .map(|bar| indicator.on_bar(bar).map(|o| o.value.to_bits()))
            .collect()),
    }
}

/// What `build_typed` does with a requested `source`: its error, or whether the outputs over a
/// fixed series equal those of the same entry built without a `source` (they differ when the
/// indicator was wrapped to read another price). Compared within one run, so the record does not
/// depend on the platform's last-place rounding.
fn source_probe(name: &str, source: Source, bars: &[Bar]) -> String {
    let params: HashMap<String, ParamValue> =
        HashMap::from([("source".to_string(), ParamValue::Source(source))]);
    match run(name, &params, bars) {
        Err(e) => e,
        Ok(with_source) => match run(name, &HashMap::new(), bars) {
            Ok(native) if native == with_source => "ok same as without source".to_string(),
            Ok(_) => "ok differs from without source".to_string(),
            Err(e) => format!("ok, but without source: {e}"),
        },
    }
}

fn snapshot() -> String {
    let bars: Vec<Bar> = generate_random_walk_bars(7, 120, 100.0, 0.0, 1.5, 1000.0)
        .into_iter()
        .enumerate()
        .map(|(i, b)| Bar::new(i as i64 * 3600, b.open, b.high, b.low, b.close, b.volume))
        .collect();
    let mut out = String::new();
    for name in names() {
        out.push_str(&format!(
            "{name:?}\tcanonical={:?}\trange={:?}\tunit={:?}\tdata={:?}\tpair={:?}\n",
            canonical_name(&name),
            output_range(&name),
            output_unit(&name),
            data_requirements(&name),
            pair_requirements(&name),
        ));
        for source in [Source::Close, Source::Open, Source::Hlc3] {
            out.push_str(&format!(
                "\tsource {source:?}: {}\n",
                source_probe(&name, source, &bars)
            ));
        }
    }
    for entry in catalog() {
        out.push_str(&format!(
            "entry {:?} {:?} range={:?}\n",
            entry.name,
            entry.input,
            entry.output_range()
        ));
    }
    out
}

#[test]
fn the_facts_by_name_match_the_snapshot() {
    let now = snapshot();
    if now != FIXTURE {
        let first = now
            .lines()
            .zip(FIXTURE.lines())
            .position(|(a, b)| a != b)
            .unwrap_or(now.lines().count().min(FIXTURE.lines().count()));
        panic!(
            "definition facts differ from tests/fixtures/definition_facts.txt at line {}:\n now:  {:?}\n file: {:?}",
            first + 1,
            now.lines().nth(first),
            FIXTURE.lines().nth(first)
        );
    }
}

#[test]
#[ignore]
fn regenerate() {
    std::fs::write(
        concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/definition_facts.txt"
        ),
        snapshot(),
    )
    .unwrap();
}
