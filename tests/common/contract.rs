//! Contract cases (plan 62): a complete expected run of one indicator, read from a generated
//! fixture (`reference/kestrel_reference/fixtures/contract_*.py`).
//!
//! A case fixes the input bars and parameters and, for every bar, whether an output exists, its
//! value, every `extra` field (and that there are no others), `secondary`, and the full set of
//! alerts with their strength. [`check`] runs the indicator built through the registry, repeats
//! the run after `reset`, and returns every deviation as a line naming case, bar, timestamp,
//! field, expected, actual and tolerance — the caller asserts the list is empty, the mutation
//! tests that it is not.

use std::collections::{BTreeMap, HashMap};

use kestrel_chartkit::indicator::registry::build_checked;
use kestrel_chartkit::indicator::{Indicator, IndicatorOutput};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::{output_unit, IndicatorUnit};

/// Spacing of the case bars, in seconds; only the order matters to the pilot indicators.
const BAR_SECONDS: i64 = 900;

#[derive(Debug, Clone, PartialEq)]
pub struct ExpectedOutput {
    pub value: f64,
    pub secondary: Option<f64>,
    pub extra: BTreeMap<String, f64>,
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ExpectedBar {
    pub output: Option<ExpectedOutput>,
    pub alerts: BTreeMap<String, f64>,
}

#[derive(Debug, Clone)]
pub struct ContractCase {
    pub name: String,
    pub indicator: String,
    pub unit: IndicatorUnit,
    pub params: HashMap<String, f64>,
    pub bars: Vec<Bar>,
    pub tolerance: f64,
    pub expected: Vec<ExpectedBar>,
}

/// The case `name` from `fixture`, for `indicator` with the declared output `unit`.
///
/// Every key under the case prefix must be understood; an unknown key panics, so a typo in the
/// generator or a case name that prefixes another cannot silently drop expectations.
pub fn load(fixture: &str, name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    let prefix = format!("{name}_");
    let mut params = HashMap::new();
    let mut bar_fields: BTreeMap<usize, HashMap<String, f64>> = BTreeMap::new();
    let mut expected: BTreeMap<usize, ExpectedBar> = BTreeMap::new();
    let mut count = None;
    let mut tolerance = None;
    for line in fixture.lines() {
        if line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let Some(rest) = key.strip_prefix(&prefix) else {
            continue;
        };
        let value: f64 = value
            .parse()
            .unwrap_or_else(|_| panic!("{key}: not a number"));
        if rest == "bars" {
            count = Some(value as usize);
        } else if rest == "tolerance" {
            tolerance = Some(value);
        } else if let Some(param) = rest.strip_prefix("param_") {
            params.insert(param.to_string(), value);
        } else if let Some((index, field)) = indexed(rest, "bar") {
            bar_fields
                .entry(index)
                .or_default()
                .insert(field.to_string(), value);
        } else if let Some((index, field)) = indexed(rest, "out") {
            let bar = expected.entry(index).or_default();
            let output = bar.output.get_or_insert_with(|| ExpectedOutput {
                value: f64::NAN,
                secondary: None,
                extra: BTreeMap::new(),
            });
            if field == "value" {
                output.value = value;
            } else if field == "secondary" {
                output.secondary = Some(value);
            } else if let Some(extra) = field.strip_prefix("extra_") {
                output.extra.insert(extra.to_string(), value);
            } else {
                panic!("{key}: unknown output field");
            }
        } else if let Some((index, kind)) = indexed(rest, "alert") {
            expected
                .entry(index)
                .or_default()
                .alerts
                .insert(kind.to_string(), value);
        } else {
            panic!("{key}: unknown contract key");
        }
    }
    let count = count.unwrap_or_else(|| panic!("case {name}: not in the fixture"));
    let bars = (1..=count)
        .map(|i| {
            let f = &bar_fields[&i];
            Bar::new(
                i as i64 * BAR_SECONDS,
                f["open"],
                f["high"],
                f["low"],
                f["close"],
                f["volume"],
            )
        })
        .collect();
    let expected = (1..=count)
        .map(|i| expected.remove(&i).unwrap_or_default())
        .collect::<Vec<_>>();
    for (i, bar) in expected.iter().enumerate() {
        if let Some(output) = &bar.output {
            assert!(
                !output.value.is_nan(),
                "case {name}: out{} without value",
                i + 1
            );
        }
    }
    ContractCase {
        name: name.to_string(),
        indicator: indicator.to_string(),
        unit,
        params,
        bars,
        tolerance: tolerance.unwrap_or_else(|| panic!("case {name}: no tolerance")),
        expected,
    }
}

/// `bar12_close` with `head` "bar" → `(12, "close")`.
fn indexed<'a>(rest: &'a str, head: &str) -> Option<(usize, &'a str)> {
    let tail = rest.strip_prefix(head)?;
    let digits = tail.find(|c: char| !c.is_ascii_digit())?;
    let index = tail[..digits].parse().ok()?;
    let field = tail[digits..].strip_prefix('_')?;
    Some((index, field))
}

/// One run: per bar the output and the alerts (kind → strength).
type Run = Vec<(Option<IndicatorOutput>, BTreeMap<String, f64>)>;

fn run(indicator: &mut Box<dyn Indicator>, bars: &[Bar]) -> Run {
    bars.iter()
        .map(|bar| {
            let output = indicator.on_bar(bar);
            let alerts = indicator
                .alerts()
                .into_iter()
                .map(|a| (a.kind, a.strength))
                .collect();
            (output, alerts)
        })
        .collect()
}

/// Every deviation of `case` from its contract; empty when the case holds.
///
/// Checked areas: the configuration builds; the declared unit; `warmup_period()` is the bar of
/// the first expected output; per bar presence, value, `secondary`, the exact set of `extra`
/// fields and their values, the exact set of alert kinds and their strength; after `reset` the
/// identical run again.
pub fn check(case: &ContractCase) -> Vec<String> {
    let mut failures = Vec::new();
    let context = format!(
        "{} ({} {:?})",
        case.name,
        case.indicator,
        sorted(&case.params)
    );
    let mut indicator = match build_checked(&case.indicator, &case.params) {
        Ok(indicator) => indicator,
        Err(error) => return vec![format!("{context}: configuration rejected: {error}")],
    };

    let unit = output_unit(&case.indicator);
    if unit != case.unit {
        failures.push(format!(
            "{context}: unit expected {:?}, actual {unit:?}",
            case.unit
        ));
    }

    if let Some(first) = case.expected.iter().position(|b| b.output.is_some()) {
        let warmup = indicator.warmup_period();
        if warmup != first + 1 {
            failures.push(format!(
                "{context}: warmup_period expected {} (bar of the first output), actual {warmup}",
                first + 1
            ));
        }
    }

    let first_run = run(&mut indicator, &case.bars);
    for (i, ((actual, alerts), expected)) in first_run.iter().zip(&case.expected).enumerate() {
        let at = format!("{context} bar {} (ts {})", i + 1, case.bars[i].timestamp);
        compare_output(
            &at,
            actual.as_ref(),
            expected.output.as_ref(),
            case.tolerance,
            &mut failures,
        );
        compare_alerts(&at, alerts, &expected.alerts, case.tolerance, &mut failures);
    }

    indicator.reset();
    let second_run = run(&mut indicator, &case.bars);
    if second_run != first_run {
        let bar = first_run
            .iter()
            .zip(&second_run)
            .position(|(a, b)| a != b)
            .map_or(0, |i| i + 1);
        failures.push(format!(
            "{context}: run after reset differs from bar {bar} on"
        ));
    }
    failures
}

fn compare_output(
    at: &str,
    actual: Option<&IndicatorOutput>,
    expected: Option<&ExpectedOutput>,
    tolerance: f64,
    failures: &mut Vec<String>,
) {
    let (actual, expected) = match (actual, expected) {
        (None, None) => return,
        (Some(a), None) => {
            failures.push(format!(
                "{at}: output expected none, actual value {}",
                a.value
            ));
            return;
        }
        (None, Some(e)) => {
            failures.push(format!(
                "{at}: output expected value {}, actual none",
                e.value
            ));
            return;
        }
        (Some(a), Some(e)) => (a, e),
    };
    compare_number(
        at,
        "value",
        actual.value,
        expected.value,
        tolerance,
        failures,
    );
    match (actual.secondary, expected.secondary) {
        (None, None) => {}
        (Some(a), Some(e)) => compare_number(at, "secondary", a, e, tolerance, failures),
        (a, e) => failures.push(format!("{at}: secondary expected {e:?}, actual {a:?}")),
    }
    for (key, expected_value) in &expected.extra {
        match actual.extra.get(key) {
            Some(&a) => compare_number(
                at,
                &format!("extra[{key}]"),
                a,
                *expected_value,
                tolerance,
                failures,
            ),
            None => failures.push(format!(
                "{at}: extra[{key}] expected {expected_value}, missing"
            )),
        }
    }
    for (key, value) in sorted(&actual.extra) {
        if !expected.extra.contains_key(&key) {
            failures.push(format!("{at}: extra[{key}] not expected, actual {value}"));
        }
    }
}

fn compare_alerts(
    at: &str,
    actual: &BTreeMap<String, f64>,
    expected: &BTreeMap<String, f64>,
    tolerance: f64,
    failures: &mut Vec<String>,
) {
    for (kind, strength) in expected {
        match actual.get(kind) {
            Some(&a) => compare_number(
                at,
                &format!("alert {kind} strength"),
                a,
                *strength,
                tolerance,
                failures,
            ),
            None => failures.push(format!("{at}: alert {kind} expected, missing")),
        }
    }
    for kind in actual.keys() {
        if !expected.contains_key(kind) {
            failures.push(format!("{at}: alert {kind} not expected"));
        }
    }
}

fn compare_number(
    at: &str,
    field: &str,
    actual: f64,
    expected: f64,
    tolerance: f64,
    failures: &mut Vec<String>,
) {
    let bound = tolerance * expected.abs().max(1.0);
    // Written so that a NaN on either side counts as a deviation.
    let within = (actual - expected).abs() <= bound;
    if !within {
        failures.push(format!(
            "{at}: {field} expected {expected}, actual {actual}, tolerance {bound:e}"
        ));
    }
}

fn sorted(map: &HashMap<String, f64>) -> Vec<(String, f64)> {
    let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.clone(), *v)).collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries
}
