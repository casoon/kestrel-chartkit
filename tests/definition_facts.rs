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

fn fnv(hash: &mut u64, value: u64) {
    for byte in value.to_le_bytes() {
        *hash ^= byte as u64;
        *hash = hash.wrapping_mul(0x100000001b3);
    }
}

/// What `build_typed` does with a requested `source`: its error, or a hash of the outputs over a
/// fixed series (which differs when the indicator was wrapped to read another price).
fn source_probe(name: &str, source: Source, bars: &[Bar]) -> String {
    let params: HashMap<String, ParamValue> =
        HashMap::from([("source".to_string(), ParamValue::Source(source))]);
    match build_typed(name, &params) {
        Err(e) => format!("err {e:?}"),
        Ok(mut indicator) => {
            let mut hash = 0xcbf29ce484222325u64;
            for bar in bars {
                match indicator.on_bar(bar) {
                    Some(o) => {
                        fnv(&mut hash, 1);
                        fnv(&mut hash, o.value.to_bits());
                    }
                    None => fnv(&mut hash, 0),
                }
            }
            format!("ok {hash:016x}")
        }
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
