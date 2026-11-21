//! Measures the trade between lag and smoothness of the smoothers of the catalog, of `kas` at
//! several settings and, with `--candidates`, of the candidate formulas `kas` was chosen from.
//!
//! ```bash
//! cargo run --release --example smoother_tradeoffs -- synthetic out.csv
//! cargo run --release --example smoother_tradeoffs -- controls <dir>
//! cargo run --release --example smoother_tradeoffs -- real <dir> out.csv
//! cargo run --release --example smoother_tradeoffs -- summary out.csv
//! ```
//!
//! - `synthetic` measures on the deterministic test signals of `signals.rs`.
//! - `controls` writes three random walks (normal, fat-tailed, with clustering volatility) as
//!   close series into a directory.
//! - `real` measures on every `*.csv` of a directory that has a `close` column. A file named
//!   `<class>__<name>.csv` is grouped by its class in the summary.
//! - `summary` prints the comparison tables from the rows the other modes wrote
//!   (`series,family,length,measure,value`).

mod candidates;
mod lines;
mod metrics;
mod signals;
mod summary;

use kestrel_chartkit::synthetic::SimpleRng;
use lines::Line;
use metrics::Measures;
use std::io::Write;

fn in_parallel(lines: &[Line], work: impl Fn(&Line) -> Measures + Sync) -> Vec<Measures> {
    let threads = std::thread::available_parallelism().map_or(4, |n| n.get());
    let next = std::sync::atomic::AtomicUsize::new(0);
    let results: Vec<std::sync::Mutex<Option<Measures>>> =
        lines.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|scope| {
        for _ in 0..threads {
            scope.spawn(|| loop {
                let i = next.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if i >= lines.len() {
                    break;
                }
                *results[i].lock().unwrap() = Some(work(&lines[i]));
            });
        }
    });
    results
        .into_iter()
        .map(|m| m.into_inner().unwrap().unwrap())
        .collect()
}

fn write_rows(
    out: &mut impl Write,
    series: &str,
    lines: &[Line],
    measures: &[Measures],
) -> std::io::Result<()> {
    for (line, m) in lines.iter().zip(measures) {
        for (name, value) in m {
            writeln!(out, "{series},{},{},{name},{value}", line.family, line.len)?;
        }
    }
    Ok(())
}

fn read_closes(path: &std::path::Path) -> Vec<f64> {
    let text = std::fs::read_to_string(path).expect("readable csv");
    let mut rows = text.lines();
    let column = rows
        .next()
        .and_then(|h| h.split(',').position(|c| c.trim() == "close"))
        .expect("a close column");
    rows.filter_map(|r| r.split(',').nth(column)?.trim().parse().ok())
        .filter(|c: &f64| c.is_finite() && *c > 0.0)
        .collect()
}

/// Three random walks of 40 000 closes: normal steps, fat-tailed steps (a normal over the root
/// of a scaled chi-square with three degrees of freedom) and steps whose variance follows its
/// own past (`h = 0.02 + 0.10 e^2 + 0.88 h`).
fn write_controls(dir: &str) -> std::io::Result<()> {
    const BARS: usize = 40_000;
    std::fs::create_dir_all(dir)?;
    let mut rng = SimpleRng::new(7);
    let mut variance: f64 = 1.0;
    type Step = Box<dyn FnMut(&mut SimpleRng) -> f64>;
    let mut steps: [(&str, Step); 3] = [
        ("normal", Box::new(|rng| rng.next_gaussian())),
        (
            "fat_tails",
            Box::new(|rng| {
                let chi: f64 = (0..3).map(|_| rng.next_gaussian().powi(2)).sum::<f64>() / 3.0;
                0.6 * rng.next_gaussian() / chi.sqrt()
            }),
        ),
        (
            "clustering",
            Box::new(move |rng| {
                let step = variance.sqrt() * rng.next_gaussian();
                variance = 0.02 + 0.10 * step * step + 0.88 * variance;
                step
            }),
        ),
    ];
    for (name, step) in steps.iter_mut() {
        let path = std::path::Path::new(dir).join(format!("control__{name}.csv"));
        let mut out = std::io::BufWriter::new(std::fs::File::create(path)?);
        writeln!(out, "close")?;
        let mut close = 1000.0;
        for _ in 0..BARS {
            close += step(&mut rng);
            writeln!(out, "{close}")?;
        }
    }
    Ok(())
}

fn main() -> std::io::Result<()> {
    let args: Vec<String> = std::env::args()
        .skip(1)
        .filter(|a| a != "--candidates")
        .collect();
    let lines = || {
        let mut all = lines::catalog_lines();
        all.extend(lines::kas_lines());
        if std::env::args().any(|a| a == "--candidates") {
            all.extend(lines::candidate_lines());
        }
        all
    };
    match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["synthetic", out] => {
            let all = lines();
            let signals = signals::Signals::build(200);
            let measures = in_parallel(&all, |line| metrics::synthetic(&*line.run, &signals));
            let mut out = std::io::BufWriter::new(std::fs::File::create(out)?);
            writeln!(out, "series,family,length,measure,value")?;
            write_rows(&mut out, "synthetic", &all, &measures)?;
        }
        ["controls", dir] => write_controls(dir)?,
        ["real", dir, out] => {
            let all = lines();
            let mut files: Vec<_> = std::fs::read_dir(dir)?
                .filter_map(|e| Some(e.ok()?.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "csv"))
                .collect();
            files.sort();
            let mut out = std::io::BufWriter::new(std::fs::File::create(out)?);
            writeln!(out, "series,family,length,measure,value")?;
            for file in files {
                let closes = read_closes(&file);
                let name = file.file_stem().unwrap().to_string_lossy().to_string();
                if closes.len() < 1200 {
                    eprintln!("{name}: {} bars, skipped", closes.len());
                    continue;
                }
                let measures = in_parallel(&all, |line| {
                    metrics::real(&*line.run, &closes, &[5, 10, 20])
                });
                write_rows(&mut out, &name, &all, &measures)?;
                for (measure, value) in metrics::premise(&closes) {
                    writeln!(out, "{name},premise,0,{measure},{value}")?;
                }
                eprintln!("{name}: {} bars", closes.len());
            }
        }
        ["summary", rows] => summary::print(rows)?,
        _ => eprintln!(
            "usage: smoother_tradeoffs [--candidates] synthetic <out.csv> | controls <dir> | \
             real <dir> <out.csv> | summary <rows.csv>"
        ),
    }
    Ok(())
}
