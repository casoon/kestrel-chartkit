//! Deterministic test signals. Every signal sits on the level [`BASE`] and starts with [`WARM`]
//! bars of lead-in, so that long recursive smoothers have forgotten their start before anything
//! is measured.

use kestrel_chartkit::synthetic::SimpleRng;

pub const BASE: f64 = 100.0;
pub const WARM: usize = 2000;

/// An observed series next to the noise-free series it was built from.
#[derive(Clone)]
pub struct Pair {
    pub observed: Vec<f64>,
    pub clean: Vec<f64>,
}

fn noisy(clean: Vec<f64>, sigma: impl Fn(usize) -> f64, seed: u64) -> Pair {
    let mut rng = SimpleRng::new(seed);
    let observed = clean
        .iter()
        .enumerate()
        .map(|(t, c)| c + sigma(t) * rng.next_gaussian())
        .collect();
    Pair { observed, clean }
}

/// A step of `height` at bar [`WARM`], `after` bars of tail, Gaussian noise `sigma`.
pub fn step(height: f64, sigma: f64, after: usize, seed: u64) -> Pair {
    let clean = (0..WARM + after)
        .map(|t| BASE + if t >= WARM { height } else { 0.0 })
        .collect();
    noisy(clean, |_| sigma, seed)
}

/// A straight line of `slope` per bar from the first bar on.
pub fn ramp(slope: f64, sigma: f64, len: usize, seed: u64) -> Pair {
    let clean = (0..len).map(|t| BASE + slope * t as f64).collect();
    noisy(clean, |_| sigma, seed)
}

/// A sine of `period` bars and `amplitude`.
pub fn sine(period: f64, amplitude: f64, sigma: f64, len: usize, seed: u64) -> Pair {
    let w = 2.0 * std::f64::consts::PI / period;
    let clean = (0..len)
        .map(|t| BASE + amplitude * (w * t as f64).sin())
        .collect();
    noisy(clean, |_| sigma, seed)
}

/// Gaussian noise around a constant level.
pub fn noise(sigma: f64, len: usize, seed: u64) -> Pair {
    noisy(vec![BASE; len], |_| sigma, seed)
}

/// Trend plus noise: straight legs of 40 to 160 bars with alternating direction and a slope of
/// 0.05 to 0.4 per bar, so the series has both trends and turns.
pub fn zigzag(sigma: f64, len: usize, seed: u64) -> Pair {
    let mut rng = SimpleRng::new(seed ^ 0x5a5a_5a5a);
    let mut clean = Vec::with_capacity(len);
    let (mut level, mut up) = (BASE, true);
    while clean.len() < len {
        let legs = rng.next_range(40.0, 160.0) as usize;
        let slope = rng.next_range(0.05, 0.4) * if up { 1.0 } else { -1.0 };
        for _ in 0..legs {
            level += slope;
            clean.push(level);
        }
        up = !up;
    }
    clean.truncate(len);
    noisy(clean, |_| sigma, seed)
}

/// The local-level model: a random walk with step deviation `q`, observed with noise `r`. The
/// best causal linear estimate of the walk is an exponential average, which makes this the
/// control signal of the bench.
pub fn local_level(q: f64, r: f64, len: usize, seed: u64) -> Pair {
    let mut rng = SimpleRng::new(seed ^ 0x1234_5678);
    let mut level = BASE;
    let clean = (0..len)
        .map(|_| {
            level += q * rng.next_gaussian();
            level
        })
        .collect();
    noisy(clean, |_| r, seed)
}

/// Bars of the regime switch signals: quiet until `SWITCH_ON`, volatile until `SWITCH_OFF`, quiet
/// to the end.
pub const SWITCH_ON: usize = WARM + 400;
pub const SWITCH_OFF: usize = SWITCH_ON + 300;
pub const SWITCH_END: usize = SWITCH_OFF + 500;

/// Volatility switch of the noise: a slow sine (period 200, amplitude 3) observed with noise 1,
/// then 4, then 1. Nothing about the signal changes; the right reaction is to smooth more.
pub fn noise_switch(seed: u64) -> Pair {
    let w = 2.0 * std::f64::consts::PI / 200.0;
    let clean = (0..SWITCH_END)
        .map(|t| BASE + 3.0 * (w * t as f64).sin())
        .collect();
    noisy(
        clean,
        |t| {
            if (SWITCH_ON..SWITCH_OFF).contains(&t) {
                4.0
            } else {
                1.0
            }
        },
        seed,
    )
}

/// Volatility switch of the signal: a random walk whose step deviation goes 0.1, then 1.0, then
/// 0.1, observed with constant noise 1. The right reaction is to follow faster.
pub fn signal_switch(seed: u64) -> Pair {
    let mut rng = SimpleRng::new(seed ^ 0x0bad_cafe);
    let mut level = BASE;
    let clean = (0..SWITCH_END)
        .map(|t| {
            let q = if (SWITCH_ON..SWITCH_OFF).contains(&t) {
                1.0
            } else {
                0.1
            };
            level += q * rng.next_gaussian();
            level
        })
        .collect();
    noisy(clean, |_| 1.0, seed)
}

/// Every signal of the bench, generated once and shared by all lines.
pub struct Signals {
    pub step_clean: Pair,
    pub step_noisy_10: Vec<Pair>,
    pub step_noisy_3: Vec<Pair>,
    pub ramp_clean: Pair,
    pub ramp_noisy: Vec<Pair>,
    pub sines: Vec<(f64, Pair, Vec<Pair>)>,
    pub noise: Pair,
    pub zigzag: Vec<Pair>,
    pub local_level: Vec<Pair>,
    pub noise_switch: Vec<Pair>,
    pub signal_switch: Vec<Pair>,
}

pub const STEP_AFTER: usize = 400;
pub const RAMP_SLOPE: f64 = 0.2;
pub const SINE_PERIODS: [f64; 4] = [10.0, 20.0, 40.0, 80.0];
pub const SINE_AMPLITUDE: f64 = 5.0;

impl Signals {
    pub fn build(ensemble: usize) -> Self {
        let seeds = |salt: u64| (0..ensemble as u64).map(move |i| 1000 * salt + i);
        Self {
            step_clean: step(10.0, 0.0, STEP_AFTER, 1),
            step_noisy_10: seeds(1).map(|s| step(10.0, 1.0, STEP_AFTER, s)).collect(),
            step_noisy_3: seeds(2).map(|s| step(3.0, 1.0, STEP_AFTER, s)).collect(),
            ramp_clean: ramp(RAMP_SLOPE, 0.0, WARM + 1000, 1),
            ramp_noisy: seeds(3)
                .take(ensemble.min(40))
                .map(|s| ramp(RAMP_SLOPE, 1.0, WARM + 1000, s))
                .collect(),
            sines: SINE_PERIODS
                .iter()
                .enumerate()
                .map(|(i, &p)| {
                    let len = WARM + 12 * p as usize;
                    (
                        p,
                        sine(p, SINE_AMPLITUDE, 0.0, len, 1),
                        seeds(10 + i as u64)
                            .take(ensemble.min(40))
                            .map(|s| sine(p, SINE_AMPLITUDE, 1.0, WARM + 2000, s))
                            .collect(),
                    )
                })
                .collect(),
            noise: noise(1.0, WARM + 20_000, 7),
            zigzag: seeds(4)
                .take(ensemble.min(20))
                .map(|s| zigzag(1.0, WARM + 6000, s))
                .collect(),
            local_level: seeds(5)
                .take(ensemble.min(20))
                .map(|s| local_level(0.3, 1.0, WARM + 6000, s))
                .collect(),
            noise_switch: seeds(6).map(noise_switch).collect(),
            signal_switch: seeds(7).map(signal_switch).collect(),
        }
    }
}
