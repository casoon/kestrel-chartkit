//! The measures of the bench. Every function takes the output of a smoother (`NaN` while it
//! warms up) and returns one number; `NaN` means the measure could not be taken.

use crate::signals::{Pair, Signals, RAMP_SLOPE, SINE_AMPLITUDE, SWITCH_OFF, SWITCH_ON, WARM};
use std::collections::BTreeMap;

pub type Run<'a> = &'a (dyn Fn(&[f64]) -> Vec<f64> + Send + Sync);
pub type Measures = BTreeMap<String, f64>;

fn mean(values: impl Iterator<Item = f64>) -> f64 {
    let (mut sum, mut n) = (0.0, 0usize);
    for v in values {
        sum += v;
        n += 1;
    }
    if n == 0 {
        f64::NAN
    } else {
        sum / n as f64
    }
}

/// Ensemble mean of the outputs, bar by bar.
fn mean_path(run: Run, set: &[Pair]) -> Vec<f64> {
    let mut acc = vec![0.0; set[0].observed.len()];
    for pair in set {
        for (a, y) in acc.iter_mut().zip(run(&pair.observed)) {
            *a += y;
        }
    }
    acc.iter().map(|a| a / set.len() as f64).collect()
}

/// Ensemble mean of the squared error against the clean series, bar by bar.
fn mse_path(run: Run, set: &[Pair]) -> Vec<f64> {
    let mut acc = vec![0.0; set[0].observed.len()];
    for pair in set {
        for ((a, y), c) in acc.iter_mut().zip(run(&pair.observed)).zip(&pair.clean) {
            *a += (y - c) * (y - c);
        }
    }
    acc.iter().map(|a| a / set.len() as f64).collect()
}

fn rmse(path: &[f64]) -> f64 {
    mean(path.iter().copied()).sqrt()
}

/// Bars after the step until the response first reaches `share` of it, interpolated between bars
/// and never below zero.
fn bars_to(response: &[f64], share: f64) -> f64 {
    let Some(t) = response.iter().position(|v| *v >= share) else {
        return f64::NAN;
    };
    if t == 0 {
        return 0.0;
    }
    let (a, b) = (response[t - 1], response[t]);
    (t as f64 - 1.0 + (share - a) / (b - a)).max(0.0)
}

/// Step measures from a response already scaled to `0 → 1`, starting at the bar of the step.
fn step_measures(prefix: &str, response: &[f64], out: &mut Measures) {
    out.insert(format!("{prefix}_t50"), bars_to(response, 0.5));
    out.insert(format!("{prefix}_t90"), bars_to(response, 0.9));
    let peak = response.iter().copied().fold(f64::NAN, f64::max);
    out.insert(format!("{prefix}_over"), 100.0 * (peak - 1.0).max(0.0));
}

/// Direction changes of the line per 1000 bars: sign changes of its first difference, flat bars
/// skipped.
pub fn turns_per_1000(y: &[f64]) -> f64 {
    let (mut last, mut turns, mut bars) = (0.0f64, 0usize, 0usize);
    for w in y.windows(2) {
        let d = w[1] - w[0];
        if !d.is_finite() {
            continue;
        }
        bars += 1;
        if d != 0.0 {
            if last != 0.0 && d.signum() != last.signum() {
                turns += 1;
            }
            last = d;
        }
    }
    1000.0 * turns as f64 / bars.max(1) as f64
}

/// Root mean square of the second difference.
pub fn second_difference_rms(y: &[f64]) -> f64 {
    mean(
        y.windows(3)
            .map(|w| w[2] - 2.0 * w[1] + w[0])
            .filter(|d| d.is_finite())
            .map(|d| d * d),
    )
    .sqrt()
}

/// Amplitude gain and phase lag in bars of the response to a sine, by least squares over a whole
/// number of periods at the end of the series.
fn sine_fit(y: &[f64], period: f64) -> (f64, f64) {
    let w = 2.0 * std::f64::consts::PI / period;
    let n = (6.0 * period) as usize;
    let start = y.len() - n;
    let (mut a, mut b) = (0.0, 0.0);
    for (t, v) in y.iter().enumerate().skip(start) {
        a += (v - crate::signals::BASE) * (w * t as f64).sin();
        b += (v - crate::signals::BASE) * (w * t as f64).cos();
    }
    let (a, b) = (2.0 * a / n as f64, 2.0 * b / n as f64);
    let mut lag = -b.atan2(a) / w;
    if lag < -period / 2.0 {
        lag += period;
    }
    ((a * a + b * b).sqrt() / SINE_AMPLITUDE, lag)
}

/// Sum of the error above a steady level, in units of that level: the bars of steady error a
/// transient costs on top.
fn excess_bars(path: &[f64], steady: f64) -> f64 {
    path.iter().map(|e| (e - steady).max(0.0)).sum::<f64>() / steady
}

/// Every synthetic measure of one smoother.
pub fn synthetic(run: Run, s: &Signals) -> Measures {
    let mut out = Measures::new();

    // Step without noise, and the ensemble mean of the step in noise.
    let y = run(&s.step_clean.observed);
    let scaled: Vec<f64> = y[WARM..].iter().map(|v| (v - 100.0) / 10.0).collect();
    step_measures("step", &scaled, &mut out);
    for (name, set, height) in [
        ("nstep10", &s.step_noisy_10, 10.0),
        ("nstep3", &s.step_noisy_3, 3.0),
    ] {
        let path = mean_path(run, set);
        let scaled: Vec<f64> = path[WARM..].iter().map(|v| (v - 100.0) / height).collect();
        step_measures(name, &scaled, &mut out);
        let err = mse_path(run, set);
        out.insert(format!("{name}_rmse"), rmse(&err[WARM..WARM + 100]));
    }

    // Ramp: horizontal offset at the end, without noise and as the ensemble mean in noise.
    let y = run(&s.ramp_clean.observed);
    let last = y.len() - 1;
    out.insert(
        "ramp_lag".into(),
        (s.ramp_clean.clean[last] - y[last]) / RAMP_SLOPE,
    );
    let path = mean_path(run, &s.ramp_noisy);
    out.insert(
        "nramp_lag".into(),
        mean((last - 500..=last).map(|t| (s.ramp_noisy[0].clean[t] - path[t]) / RAMP_SLOPE)),
    );
    out.insert(
        "nramp_rmse".into(),
        rmse(&mse_path(run, &s.ramp_noisy)[WARM..]),
    );

    // Sines: gain and lag without noise, error in noise.
    for (period, clean, noisy) in &s.sines {
        let (gain, lag) = sine_fit(&run(&clean.observed), *period);
        out.insert(format!("sine{period}_gain"), gain);
        out.insert(format!("sine{period}_lag"), lag);
        out.insert(
            format!("nsine{period}_rmse"),
            rmse(&mse_path(run, noisy)[WARM..]),
        );
    }

    // Pure noise.
    let y = run(&s.noise.observed);
    let (y, x) = (&y[WARM..], &s.noise.observed[WARM..]);
    out.insert("noise_turns".into(), turns_per_1000(y));
    out.insert(
        "noise_rough".into(),
        second_difference_rms(y) / second_difference_rms(x),
    );
    out.insert(
        "noise_gain".into(),
        mean(y.iter().map(|v| (v - 100.0) * (v - 100.0))).sqrt(),
    );

    // Trend plus noise, and the local-level control.
    out.insert(
        "zigzag_rmse".into(),
        rmse(&mse_path(run, &s.zigzag)[WARM..]),
    );
    out.insert(
        "zigzag_turns".into(),
        mean(
            s.zigzag
                .iter()
                .map(|p| turns_per_1000(&run(&p.observed)[WARM..])),
        ),
    );
    out.insert(
        "level_rmse".into(),
        rmse(&mse_path(run, &s.local_level)[WARM..]),
    );
    for (name, set) in [
        ("zigzag", &s.zigzag[..]),
        ("level", &s.local_level[..]),
        ("nswitch", &s.noise_switch[..s.noise_switch.len().min(20)]),
        ("sswitch", &s.signal_switch[..s.signal_switch.len().min(20)]),
        ("nstep10", &s.step_noisy_10[..s.step_noisy_10.len().min(20)]),
    ] {
        out.insert(
            format!("{name}_rough"),
            mean(set.iter().map(|p| {
                second_difference_rms(&run(&p.observed)[WARM..])
                    / second_difference_rms(&p.observed[WARM..])
            })),
        );
    }
    out.insert(
        "level_turns".into(),
        mean(
            s.local_level
                .iter()
                .map(|p| turns_per_1000(&run(&p.observed)[WARM..])),
        ),
    );

    // Volatility switches: steady error in each regime and the cost of the two transitions.
    for (name, set) in [("nswitch", &s.noise_switch), ("sswitch", &s.signal_switch)] {
        let err = mse_path(run, set);
        let quiet = mean(err[SWITCH_ON - 300..SWITCH_ON].iter().copied());
        let loud = mean(err[SWITCH_ON + 150..SWITCH_OFF].iter().copied());
        out.insert(format!("{name}_rmse_quiet"), quiet.sqrt());
        out.insert(format!("{name}_rmse_loud"), loud.sqrt());
        out.insert(
            format!("{name}_react"),
            excess_bars(&err[SWITCH_ON..SWITCH_ON + 150], loud),
        );
        out.insert(
            format!("{name}_recover"),
            excess_bars(&err[SWITCH_OFF..], quiet),
        );
        out.insert(format!("{name}_rmse"), rmse(&err[WARM..]));
    }
    out
}

/// Measures on a real close series. The yardstick is a centred triangular average of half-width
/// `half` (not causal, used only as the reference); errors are taken in units of the local scale,
/// a causal exponential average of the absolute close change over 200 bars.
pub fn real(run: Run, close: &[f64], halves: &[usize]) -> Measures {
    let mut out = Measures::new();
    let n = close.len();
    let y = run(close);
    let mut scale = vec![f64::NAN; n];
    let mut s = f64::NAN;
    for t in 1..n {
        let d = (close[t] - close[t - 1]).abs();
        s = if s.is_nan() { d } else { s + (d - s) / 100.5 };
        scale[t] = s;
    }
    let start = 1500.min(n / 3);
    // Distance of the line from the price itself, in units of the local scale: the measure that
    // needs no reference.
    out.insert(
        "real_dist".into(),
        mean(
            (start..n)
                .map(|t| (y[t] - close[t]) / scale[t])
                .filter(|e| e.is_finite())
                .map(|e| e * e),
        )
        .sqrt(),
    );
    for &half in halves {
        let reference = centred_triangle(close, half);
        let end = n - 2 * half - 1;
        let (mut se, mut cov, mut var, mut count) = (0.0, 0.0, 0.0, 0usize);
        for t in start.max(2 * half + 1)..end {
            let (e, d) = (
                (y[t] - reference[t]) / scale[t],
                (reference[t + 1] - reference[t - 1]) / 2.0 / scale[t],
            );
            if !(e.is_finite() && d.is_finite()) {
                continue;
            }
            se += e * e;
            cov += e * d;
            var += d * d;
            count += 1;
        }
        out.insert(format!("real_rmse_h{half}"), (se / count as f64).sqrt());
        out.insert(format!("real_lag_h{half}"), -cov / var);
    }
    let scaled = |v: &[f64]| -> f64 {
        mean(
            (start.max(2)..n)
                .map(|t| (v[t] - 2.0 * v[t - 1] + v[t - 2]) / scale[t])
                .filter(|d| d.is_finite())
                .map(|d| d * d),
        )
        .sqrt()
    };
    out.insert("real_rough".into(), scaled(&y) / scaled(close));
    out.insert("real_turns".into(), turns_per_1000(&y[start..]));
    out
}

/// Centred triangular average: a simple average of `half + 1` bars applied twice, shifted back
/// by `half` bars, so it reads `half` bars into the future. `NaN` where the window is incomplete.
pub fn centred_triangle(x: &[f64], half: usize) -> Vec<f64> {
    let w = half + 1;
    let once = |v: &[f64]| -> Vec<f64> {
        let mut out = vec![f64::NAN; v.len()];
        for t in w - 1..v.len() {
            out[t] = v[t + 1 - w..=t].iter().sum::<f64>() / w as f64;
        }
        out
    };
    let twice = once(&once(x));
    let mut out = vec![f64::NAN; x.len()];
    let kept = x.len().saturating_sub(half);
    out[..kept].copy_from_slice(&twice[half..half + kept]);
    out
}

/// What the price does after a move, by the size of the move: for bars whose close change, in
/// units of the local scale of the bars before, falls into a size class, the share of the change
/// that is added (positive) or taken back (negative) over the next `k` bars. A random walk reads
/// zero in every class. Also the same after a smoothed change (an exponential average over 10
/// bars), by the size of its standard score.
pub fn premise(close: &[f64]) -> Measures {
    const EDGES: [f64; 6] = [0.0, 0.5, 1.0, 2.0, 3.0, 5.0];
    const HORIZONS: [usize; 4] = [1, 3, 10, 30];
    let n = close.len();
    let mut out = Measures::new();
    let mut scale = f64::NAN;
    let mut v = 0.0;
    let a_v: f64 = 2.0 / 11.0;
    let c = (a_v / (2.0 - a_v)).sqrt();
    // [class][horizon] -> (sum change * future, sum change^2, count)
    let mut by_size = [[(0.0, 0.0, 0usize); 4]; 6];
    // [class][horizon] -> (sum sign(v) * future / (scale sqrt k), count)
    let mut by_score = [[(0.0, 0usize); 4]; 6];
    let class = |u: f64| EDGES.iter().rposition(|e| u >= *e).unwrap_or(0);
    for t in 1..n {
        let d = close[t] - close[t - 1];
        let before = scale;
        scale = if scale.is_nan() {
            d.abs()
        } else {
            scale + (d.abs() - scale) / 100.5
        };
        v += a_v * (d - v);
        if t < 400 || before.is_nan() || before <= 0.0 {
            continue;
        }
        // 1.2533 = sqrt(pi / 2): mean absolute change to root mean square of a normal change.
        let (u, z) = (d / before, v / (1.2533 * before * c));
        for (h, k) in HORIZONS.iter().enumerate() {
            if t + k >= n {
                continue;
            }
            let future = (close[t + k] - close[t]) / before;
            let cell = &mut by_size[class(u.abs())][h];
            *cell = (cell.0 + u * future, cell.1 + u * u, cell.2 + 1);
            let cell = &mut by_score[class(z.abs())][h];
            *cell = (
                cell.0 + z.signum() * future / (*k as f64).sqrt(),
                cell.1 + 1,
            );
        }
    }
    for (b, edge) in EDGES.iter().enumerate() {
        for (h, k) in HORIZONS.iter().enumerate() {
            let (sxy, sxx, count) = by_size[b][h];
            out.insert(format!("size{edge}_k{k}_cont"), sxy / sxx);
            out.insert(format!("size{edge}_k{k}_n"), count as f64);
            let (sum, count) = by_score[b][h];
            out.insert(format!("score{edge}_k{k}_drift"), sum / count as f64);
            out.insert(format!("score{edge}_k{k}_n"), count as f64);
        }
    }
    out
}
