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
/// Scalar lines `<case>_tolerance`, `<case>_bars`, `<case>_param_<key>`; then the table headed
/// `@<case> open high low close volume <columns...>` with one row per bar and `-` for an absent
/// column (format written by `render_contract` in `reference/kestrel_reference/fixture.py`).
/// Every scalar key under the case prefix and every column must be understood; anything else
/// panics, so a generator typo or a case name that prefixes another cannot drop expectations.
pub fn load(fixture: &str, name: &str, indicator: &str, unit: IndicatorUnit) -> ContractCase {
    let prefix = format!("{name}_");
    let table_head = format!("@{name} ");
    let mut params = HashMap::new();
    let mut count = None;
    let mut tolerance = None;
    let mut columns: Option<Vec<String>> = None;
    let mut bars = Vec::new();
    let mut expected = Vec::new();
    let mut lines = fixture.lines();
    while let Some(line) = lines.next() {
        if let Some(head) = line.strip_prefix(&table_head) {
            let head: Vec<String> = head.split(' ').map(str::to_string).collect();
            assert_eq!(
                head[..5],
                ["open", "high", "low", "close", "volume"],
                "case {name}: table must start with the bar columns"
            );
            let rows = count.unwrap_or_else(|| panic!("case {name}: table before `bars`"));
            for i in 1..=rows {
                let row = lines
                    .next()
                    .unwrap_or_else(|| panic!("case {name}: row {i} missing"));
                let cells: Vec<&str> = row.split(' ').collect();
                assert_eq!(cells.len(), head.len(), "case {name}: row {i} width");
                let number = |cell: &str| -> Option<f64> {
                    (cell != "-").then(|| {
                        cell.parse()
                            .unwrap_or_else(|_| panic!("case {name}: row {i}: `{cell}`"))
                    })
                };
                let bar = |k: usize| {
                    number(cells[k]).unwrap_or_else(|| panic!("case {name}: row {i} bar field"))
                };
                bars.push(Bar::new(
                    i as i64 * BAR_SECONDS,
                    bar(0),
                    bar(1),
                    bar(2),
                    bar(3),
                    bar(4),
                ));
                let mut out = ExpectedBar::default();
                let mut output = ExpectedOutput {
                    value: f64::NAN,
                    secondary: None,
                    extra: BTreeMap::new(),
                };
                let mut has_output = false;
                for (column, cell) in head.iter().zip(&cells).skip(5) {
                    let Some(value) = number(cell) else {
                        continue;
                    };
                    if column == "value" {
                        output.value = value;
                        has_output = true;
                    } else if column == "secondary" {
                        output.secondary = Some(value);
                    } else if let Some(field) = column.strip_prefix("extra:") {
                        output.extra.insert(field.to_string(), value);
                    } else if let Some(kind) = column.strip_prefix("alert:") {
                        out.alerts.insert(kind.to_string(), value);
                    } else {
                        panic!("case {name}: unknown column `{column}`");
                    }
                }
                assert!(
                    has_output || (output.secondary.is_none() && output.extra.is_empty()),
                    "case {name}: row {i} has output fields without a value"
                );
                out.output = has_output.then_some(output);
                expected.push(out);
            }
            columns = Some(head);
            continue;
        }
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
        } else {
            panic!("{key}: unknown contract key");
        }
    }
    assert!(columns.is_some(), "case {name}: not in the fixture");
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
