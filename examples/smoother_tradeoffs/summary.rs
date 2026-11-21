//! Reads the rows the measuring modes wrote and prints the comparison tables: every family at
//! equal smoothness, as a ratio to the best smoother of the catalog.

use crate::lines::{CATALOG, KAS};
use std::collections::{BTreeMap, BTreeSet};

/// family -> length -> measure -> value
type Curves = BTreeMap<String, BTreeMap<usize, BTreeMap<String, f64>>>;

/// series -> curves
pub fn load(path: &str) -> std::io::Result<BTreeMap<String, Curves>> {
    let mut all: BTreeMap<String, Curves> = BTreeMap::new();
    for row in std::fs::read_to_string(path)?.lines().skip(1) {
        let cells: Vec<&str> = row.split(',').collect();
        if let [series, family, length, measure, value] = cells[..] {
            if let (Ok(length), Ok(value)) = (length.parse::<usize>(), value.parse::<f64>()) {
                all.entry(series.to_string())
                    .or_default()
                    .entry(family.to_string())
                    .or_default()
                    .entry(length)
                    .or_default()
                    .insert(measure.to_string(), value);
            }
        }
    }
    Ok(all)
}

/// The mean of every finite measure over the series whose name starts with `prefix`.
fn mean_of(all: &BTreeMap<String, Curves>, prefix: &str) -> (Curves, usize) {
    let mut sums: BTreeMap<(String, usize, String), (f64, usize)> = BTreeMap::new();
    let mut count = 0;
    for (series, curves) in all {
        if !series.starts_with(prefix) {
            continue;
        }
        count += 1;
        for (family, by_length) in curves {
            for (length, measures) in by_length {
                for (measure, value) in measures {
                    if value.is_finite() {
                        let cell = sums
                            .entry((family.clone(), *length, measure.clone()))
                            .or_insert((0.0, 0));
                        *cell = (cell.0 + value, cell.1 + 1);
                    }
                }
            }
        }
    }
    let mut out = Curves::new();
    for ((family, length, measure), (sum, n)) in sums {
        out.entry(family)
            .or_default()
            .entry(length)
            .or_default()
            .insert(measure, sum / n as f64);
    }
    (out, count)
}

/// The measure `y` of a family where its measure `x` equals `target`, interpolated along the
/// sweep of lengths (logarithmic in `x`). `None` if the sweep never reaches the target.
fn at(curves: &Curves, family: &str, x: &str, target: f64, y: &str) -> Option<f64> {
    let points: Vec<(f64, f64)> = curves
        .get(family)?
        .values()
        .filter_map(|m| Some((*m.get(x)?, *m.get(y)?)))
        .filter(|(a, b)| a.is_finite() && b.is_finite())
        .collect();
    points.windows(2).find_map(|w| {
        let ((a, ya), (b, yb)) = (w[0], w[1]);
        if a > 0.0 && b > 0.0 && a != b && (a - target) * (b - target) <= 0.0 {
            let share = (target.ln() - a.ln()) / (b.ln() - a.ln());
            Some(ya + share * (yb - ya))
        } else {
            None
        }
    })
}

/// The smallest `y` at `x = target` among the catalog's smoothers, and whose it is.
fn best_of_catalog(curves: &Curves, x: &str, target: f64, y: &str) -> Option<(f64, &'static str)> {
    CATALOG
        .iter()
        .filter_map(|name| Some((at(curves, name, x, target, y)?, *name)))
        .filter(|(value, _)| *value > 0.0)
        .min_by(|a, b| a.0.total_cmp(&b.0))
}

fn cell(value: Option<f64>, decimals: usize) -> String {
    value.map_or("–".to_string(), |v| format!("{v:.decimals$}"))
}

/// Families other than the catalog's, in the order `kas`, its variants, the candidates.
fn other_families(curves: &Curves) -> Vec<String> {
    let known: BTreeSet<&str> = CATALOG.iter().copied().collect();
    let mut out: Vec<String> = KAS
        .iter()
        .map(|k| k.0.to_string())
        .filter(|name| curves.contains_key(name))
        .collect();
    out.extend(
        curves
            .keys()
            .filter(|name| {
                !known.contains(name.as_str())
                    && !KAS.iter().any(|k| k.0 == name.as_str())
                    && name.as_str() != "premise"
            })
            .cloned(),
    );
    out
}

/// Ratio tables on close series: distance to the close and error against the centred yardstick,
/// at equal direction changes and at equal roughness.
fn real_tables(all: &BTreeMap<String, Curves>) {
    const COLUMNS: [(&str, &str, f64, &str); 6] = [
        ("distance at 100 turns", "real_turns", 100.0, "real_dist"),
        ("distance at 50 turns", "real_turns", 50.0, "real_dist"),
        ("distance at 25 turns", "real_turns", 25.0, "real_dist"),
        (
            "yardstick error at 50 turns",
            "real_turns",
            50.0,
            "real_rmse_h10",
        ),
        ("distance at roughness 0.1", "real_rough", 0.1, "real_dist"),
        (
            "distance at roughness 0.03",
            "real_rough",
            0.03,
            "real_dist",
        ),
    ];
    let mut groups: BTreeSet<String> = all
        .keys()
        .filter_map(|s| s.split_once("__").map(|(class, _)| format!("{class}__")))
        .collect();
    groups.insert(String::new());
    for group in groups {
        let (curves, count) = mean_of(all, &group);
        let label = if group.is_empty() {
            "all"
        } else {
            group.trim_end_matches('_')
        };
        println!("\n### {label} ({count} series)\n");
        println!("| family | {} |", COLUMNS.map(|c| c.0).join(" | "));
        println!("|---|{}", "---:|".repeat(COLUMNS.len()));
        let best: Vec<_> = COLUMNS
            .iter()
            .map(|(_, x, t, y)| best_of_catalog(&curves, x, *t, y))
            .collect();
        println!(
            "| best of the catalog | {} |",
            best.iter()
                .map(|b| b.map_or("–".to_string(), |(v, name)| format!("{name} {v:.2}")))
                .collect::<Vec<_>>()
                .join(" | ")
        );
        for family in other_families(&curves) {
            let cells: Vec<String> = COLUMNS
                .iter()
                .zip(&best)
                .map(|((_, x, t, y), b)| {
                    cell(
                        at(&curves, &family, x, *t, y)
                            .zip(*b)
                            .map(|(v, (best, _))| v / best),
                        2,
                    )
                })
                .collect();
            println!("| `{family}` | {} |", cells.join(" | "));
        }
    }
}

/// What the price does after a move, by size class: mean over the series and the mean in units
/// of its standard error across the series.
fn premise_table(all: &BTreeMap<String, Curves>) {
    const EDGES: [&str; 6] = ["0", "0.5", "1", "2", "3", "5"];
    const HORIZONS: [usize; 4] = [1, 3, 10, 30];
    let rows: Vec<&BTreeMap<String, f64>> = all
        .values()
        .filter_map(|curves| curves.get("premise")?.get(&0))
        .collect();
    if rows.len() < 2 {
        return;
    }
    println!(
        "\n### What follows a move, by its size ({} series)\n",
        rows.len()
    );
    println!(
        "| size of the move | {} | share of bars |",
        HORIZONS.map(|k| format!("after {k}")).join(" | ")
    );
    println!("|---|{}", "---:|".repeat(HORIZONS.len() + 1));
    let total: f64 = EDGES
        .iter()
        .map(|e| {
            rows.iter()
                .filter_map(|r| r.get(&format!("size{e}_k1_n")))
                .sum::<f64>()
        })
        .sum();
    for edge in EDGES {
        let cells: Vec<String> = HORIZONS
            .iter()
            .map(|k| {
                let values: Vec<f64> = rows
                    .iter()
                    .filter_map(|r| r.get(&format!("size{edge}_k{k}_cont")).copied())
                    .filter(|v| v.is_finite())
                    .collect();
                let n = values.len() as f64;
                let mean = values.iter().sum::<f64>() / n;
                let deviation =
                    (values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (n - 1.0)).sqrt();
                format!("{mean:+.3} ({:+.1})", mean / (deviation / n.sqrt()))
            })
            .collect();
        let share: f64 = rows
            .iter()
            .filter_map(|r| r.get(&format!("size{edge}_k1_n")))
            .sum();
        println!(
            "| from {edge} | {} | {:.1} % |",
            cells.join(" | "),
            100.0 * share / total
        );
    }
}

/// Tables on the synthetic signals.
fn synthetic_tables(curves: &Curves) {
    let reference = |x: &str| curves["super_smoother"][&20][x];

    println!("\n### Overshoot at a step, largest over the sweep (% of the step)\n");
    println!("| family | clean step | step of 10 sigma in noise | step of 3 sigma in noise |");
    println!("|---|---:|---:|---:|");
    let mut families: Vec<String> = CATALOG.iter().map(|s| s.to_string()).collect();
    families.extend(other_families(curves));
    for family in &families {
        let largest = |measure: &str| {
            curves[family]
                .values()
                .filter_map(|m| m.get(measure).copied())
                .filter(|v| v.is_finite())
                .fold(f64::NAN, f64::max)
        };
        println!(
            "| `{family}` | {:.1} | {:.1} | {:.1} |",
            largest("step_over"),
            largest("nstep10_over"),
            largest("nstep3_over")
        );
    }

    const COLUMNS: [(&str, &str, usize); 11] = [
        ("bars to 50 %, step of 10 sigma", "nstep10_t50", 1),
        ("bars to 90 %", "nstep10_t90", 1),
        ("bars to 50 %, step of 3 sigma", "nstep3_t50", 1),
        ("bars to 90 %", "nstep3_t90", 1),
        ("ramp lag in noise", "nramp_lag", 1),
        ("gain, sine of 40 bars", "sine40_gain", 2),
        ("error, trend with turns", "zigzag_rmse", 2),
        ("error, walk in noise", "level_rmse", 3),
        ("error, noise switch", "nswitch_rmse", 3),
        ("error, signal switch", "sswitch_rmse", 3),
        ("roughness in noise", "noise_rough", 3),
    ];
    for (title, x) in [
        ("direction changes", "level_turns"),
        ("roughness", "level_rough"),
    ] {
        let target = reference(x);
        println!(
            "\n### Every family at the {title} of `super_smoother(20)` on the walk in noise\n"
        );
        println!("| family | period | {} |", COLUMNS.map(|c| c.0).join(" | "));
        println!("|---|---:|{}", "---:|".repeat(COLUMNS.len()));
        for family in &families {
            let period = curves[family]
                .iter()
                .map(|(length, m)| (*length as f64, m.get(x).copied().unwrap_or(f64::NAN)))
                .collect::<Vec<_>>()
                .windows(2)
                .find_map(|w| {
                    let ((la, a), (lb, b)) = (w[0], w[1]);
                    ((a - target) * (b - target) <= 0.0 && a != b)
                        .then(|| la + (target - a) / (b - a) * (lb - la))
                });
            let cells: Vec<String> = COLUMNS
                .iter()
                .map(|(_, y, decimals)| cell(at(curves, family, x, target, y), *decimals))
                .collect();
            println!(
                "| `{family}` | {} | {} |",
                cell(period, 1),
                cells.join(" | ")
            );
        }
    }

    const PLANES: [(&str, &str, &str); 9] = [
        (
            "trend with turns: error at equal direction changes",
            "zigzag_turns",
            "zigzag_rmse",
        ),
        (
            "trend with turns: error at equal roughness",
            "zigzag_rough",
            "zigzag_rmse",
        ),
        (
            "walk in noise: error at equal direction changes",
            "level_turns",
            "level_rmse",
        ),
        (
            "walk in noise: error at equal roughness",
            "level_rough",
            "level_rmse",
        ),
        (
            "signal switch: error at equal roughness",
            "sswitch_rough",
            "sswitch_rmse",
        ),
        (
            "noise switch: error at equal roughness",
            "nswitch_rough",
            "nswitch_rmse",
        ),
        (
            "step of 10 sigma: error at equal roughness",
            "nstep10_rough",
            "nstep10_rmse",
        ),
        (
            "step of 10 sigma: bars to 90 % at equal roughness in noise",
            "noise_rough",
            "nstep10_t90",
        ),
        (
            "ramp in noise: lag at equal roughness in noise",
            "noise_rough",
            "nramp_lag",
        ),
    ];
    let shown: Vec<String> = other_families(curves)
        .into_iter()
        .filter(|f| f.starts_with("kas"))
        .collect();
    println!("\n### Ratio to the best smoother of the catalog, at the smoothness of `super_smoother(20)`\n");
    println!(
        "| plane | best of the catalog | {} |",
        shown
            .iter()
            .map(|f| format!("`{f}`"))
            .collect::<Vec<_>>()
            .join(" | ")
    );
    println!("|---|---|{}", "---:|".repeat(shown.len()));
    for (title, x, y) in PLANES {
        let target = reference(x);
        let Some((best, name)) = best_of_catalog(curves, x, target, y) else {
            continue;
        };
        let cells: Vec<String> = shown
            .iter()
            .map(|f| cell(at(curves, f, x, target, y).map(|v| v / best), 2))
            .collect();
        println!("| {title} | {name} {best:.3} | {} |", cells.join(" | "));
    }
}

pub fn print(path: &str) -> std::io::Result<()> {
    let all = load(path)?;
    if let Some(curves) = all.get("synthetic") {
        synthetic_tables(curves);
    } else {
        real_tables(&all);
        premise_table(&all);
    }
    Ok(())
}
