//! README and `INDICATOR_STATUS.md` against the code they describe (plan 45).
//!
//! Both files restate facts whose truth lives in the crate: the version, how many indicators
//! there are, which Cargo features exist, and which names the catalog carries. A restated fact
//! is a copy, and a copy drifts — the README claimed `0.5` and `96 indicators` while the crate
//! was at `0.11.1` with `105`, and nothing failed. These tests make that drift a test failure
//! instead of a reader's problem.
//!
//! Deliberately *not* checked here: whether the installation snippet resolves against
//! crates.io. That is plan 46's open decision (published there: `0.1.x`), not a fact this crate
//! can assert about a registry it does not control.

use std::collections::HashSet;

use kestrel_chartkit::indicator::registry::catalog;

const README: &str = include_str!("../README.md");
const STATUS: &str = include_str!("../INDICATOR_STATUS.md");

/// `0.15.3` → `0.15`. Pre-1.0 the minor is the breaking level, so that is the granularity a
/// dependency line has to name; a patch release must not force a README edit.
fn minor(version: &str) -> String {
    let mut parts = version.split('.');
    let major = parts.next().unwrap_or_default();
    let minor = parts.next().unwrap_or_default();
    format!("{major}.{minor}")
}

#[test]
fn readme_nennt_die_eigene_version() {
    let expected = minor(env!("CARGO_PKG_VERSION"));

    // Every `kestrel-chartkit = "x.y"` / `version = "x.y"` in a dependency snippet, plus the
    // prose line "The crate is at **x.y**".
    let mut found = Vec::new();
    for line in README.lines() {
        // `> ` because one snippet sits inside a block quote.
        let line = line.trim().trim_start_matches("> ").trim();
        if let Some((_, rest)) = line.split_once("tag = \"v") {
            // The git route names the full version, not just the minor.
            let tag = rest.split('"').next().unwrap_or_default();
            assert_eq!(
                tag,
                env!("CARGO_PKG_VERSION"),
                "README zeigt Tag v{tag}, Cargo.toml steht auf {}",
                env!("CARGO_PKG_VERSION")
            );
        }
        if let Some(rest) = line.strip_prefix("kestrel-chartkit = ") {
            if rest.contains("git =") {
                continue; // named by its tag, checked above
            }
            found.push(extract_version(rest));
        } else if line.starts_with("The crate is at") {
            found.push(
                line.split("**")
                    .nth(1)
                    .map(str::to_string)
                    .unwrap_or_default(),
            );
        }
    }
    assert!(
        !found.is_empty(),
        "README nennt die Version nirgends — dieser Test prüft dann nichts"
    );
    for claim in found {
        assert_eq!(
            claim,
            expected,
            "README nennt Version {claim}, Cargo.toml steht auf {}",
            env!("CARGO_PKG_VERSION")
        );
    }
}

/// `"0.15"` or `{ version = "0.15", default-features = false }` → `0.15`.
fn extract_version(rest: &str) -> String {
    let after = rest.split_once("version = ").map_or(rest, |(_, v)| v);
    after
        .trim_start()
        .trim_start_matches('"')
        .split('"')
        .next()
        .unwrap_or_default()
        .to_string()
}

#[test]
fn readme_nennt_die_katalogzahl() {
    let count = catalog().len();
    assert!(
        README.contains(&format!("**{count} Streaming Indicators")),
        "README nennt nicht {count} Indikatoren — catalog() ist gewachsen oder geschrumpft"
    );

    // The same bullet splits the total into three groups. A split that no longer adds up is the
    // more likely drift: someone adds a detector and only bumps the total.
    let bullet = README
        .lines()
        .find(|l| l.contains("Streaming Indicators and Detectors"))
        .expect("Indikator-Aufzählung im README");
    // Only the part before "all built by name" — the examples behind it carry digits of their
    // own (Tillson T3), and those are names, not counts.
    let split_part = bullet
        .split_once("all built by name")
        .map_or(bullet, |(head, _)| head);
    let numbers: Vec<usize> = split_part
        .split(|c: char| !c.is_ascii_digit())
        .filter(|s| !s.is_empty())
        .filter_map(|s| s.parse().ok())
        .collect();
    let split: Vec<usize> = numbers.iter().copied().filter(|n| *n != count).collect();
    assert_eq!(
        split.iter().sum::<usize>(),
        count,
        "Die Aufteilung {split:?} im README summiert sich nicht auf {count}"
    );
}

#[test]
fn readme_nennt_nur_vorhandene_features() {
    let declared: HashSet<&str> = ["serde", "calendar", "default"].into_iter().collect();
    for line in README.lines() {
        let Some((_, rest)) = line.split_once("features = [") else {
            continue;
        };
        let Some((list, _)) = rest.split_once(']') else {
            continue;
        };
        for name in list.split(',') {
            let name = name.trim().trim_matches('"');
            if name.is_empty() {
                continue;
            }
            assert!(
                declared.contains(name),
                "README zeigt Feature `{name}`, das Cargo.toml nicht deklariert"
            );
        }
    }
}

#[test]
fn indicator_status_fuehrt_jeden_katalogeintrag_genau_einmal() {
    // The file's own rule is "add every new indicator here as ❌". That was discipline; here it
    // is a test, so a new catalog entry without a status line fails instead of quietly missing.
    let listed: Vec<&str> = STATUS
        .lines()
        .filter_map(|line| {
            let line = line.trim();
            let rest = line.strip_prefix('|')?;
            let name = rest.split('|').next()?.trim();
            (!name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'))
            .then_some(name)
        })
        .collect();

    for entry in catalog() {
        let hits = listed.iter().filter(|n| **n == entry.name).count();
        assert_eq!(
            hits, 1,
            "`{}` steht {hits}× in INDICATOR_STATUS.md, erwartet genau einmal",
            entry.name
        );
    }

    let known: HashSet<&str> = catalog().into_iter().map(|e| e.name).collect();
    for name in &listed {
        assert!(
            known.contains(name),
            "INDICATOR_STATUS.md führt `{name}`, das der Katalog nicht kennt"
        );
    }
}
