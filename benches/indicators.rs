use std::hint::black_box;
use std::time::Duration;

use criterion::{criterion_group, criterion_main, Criterion};

use kestrel_chartkit::indicator::registry::{build_checked, catalog};
use kestrel_chartkit::model::Bar;
use kestrel_chartkit::synthetic::random_walk_bars;

/// Streams every catalog indicator (built with its own default params) over the same synthetic
/// bar series, to catch performance regressions across the whole indicator set.
///
/// Warm-up/measurement time is shortened from Criterion's defaults (3s/5s) so the full ~91-entry
/// catalog stays runnable in CI (see `.github/workflows/ci.yml`) in a few minutes rather than
/// ~15. Still a full statistical measurement, just over a smaller time budget per benchmark.
fn bench_all_indicators(c: &mut Criterion) {
    let bars: Vec<Bar> = random_walk_bars(42, 500, 100.0, 0.01, 1.0, 1000.0)
        .into_iter()
        .map(|qb| qb.bar)
        .collect();

    let mut group = c.benchmark_group("indicator_stream_500_bars");
    group.warm_up_time(Duration::from_millis(500));
    group.measurement_time(Duration::from_secs(1));
    for entry in catalog() {
        let Ok(mut ind) = build_checked(entry.name, &entry.default_params) else {
            continue;
        };
        group.bench_function(entry.name, |b| {
            b.iter(|| {
                ind.reset();
                for bar in &bars {
                    black_box(ind.on_bar(black_box(bar)));
                }
            })
        });
    }
    group.finish();
}

criterion_group!(benches, bench_all_indicators);
criterion_main!(benches);
