//! The candidate formulas that were measured before `kas` was fixed. One configurable type covers
//! them: what drives the adaptation, how the driver becomes a gain, and which cascade turns the
//! gain into a line. The catalog entry is the lead cascade with the steady map and, as its
//! driver, the geometric mean of [`Driver::Velocity`] and [`Driver::SmoothedEfficiency`].

/// What the adaptation reads from the input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Driver {
    /// No adaptation: a fixed linear filter.
    None,
    /// Standard score of the smoothed bar-to-bar change: `|v| / (sigma * c)`, with `v` an
    /// exponential average of the change, `sigma` its long-run root mean square and `c` the factor
    /// that makes the score a unit normal on a random walk.
    Velocity,
    /// Smoothed change over smoothed absolute change, both over the same window: the efficiency
    /// ratio in exponential form, scaled so that a random walk reads about one.
    SmoothedEfficiency,
    /// The efficiency ratio over a window of `length` bars (net change over path), scaled by
    /// `sqrt(length)` so that a random walk reads about one.
    Efficiency,
    /// Short-run over long-run root mean square change.
    RelativeVolatility,
    /// Root of the smoothed squared velocity score: a variance ratio.
    VarianceRatio,
}

/// How the driver `z` becomes the gain of one stage.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Map {
    /// The gain `k` of the steady-state filter for a signal-to-noise ratio scaled by
    /// `z^(2 adapt)`: `k^2 / (1 - k) = s0 * z^(2 adapt)`, `s0` from the base gain.
    Steady,
    /// `k = min(1, k0 * clamp(z^adapt, lo, hi))`.
    Power { lo: f64, hi: f64 },
}

/// The cascade of exponential stages and how its output is read.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Core {
    /// One stage.
    One,
    /// Two stages, the second one read.
    Two,
    /// Three stages, read as `s2 + phase * (s2 - s3)`.
    Lead,
}

#[derive(Clone, Copy, Debug)]
pub struct Config {
    pub core: Core,
    pub driver: Driver,
    pub map: Map,
    pub phase: f64,
    pub adapt: f64,
    /// Length of the velocity average as a share of `length`.
    pub velocity_share: f64,
    /// Length of the long-run volatility average as a multiple of `length`.
    pub volatility_multiple: f64,
    /// Lower bound of the driver.
    pub floor: f64,
}

impl Config {
    pub const fn new(core: Core, driver: Driver) -> Self {
        Self {
            core,
            driver,
            map: Map::Steady,
            phase: 0.0,
            adapt: 1.0,
            velocity_share: 1.0,
            volatility_multiple: 4.0,
            floor: 0.0,
        }
    }
    pub const fn map(mut self, map: Map) -> Self {
        self.map = map;
        self
    }
    pub const fn phase(mut self, phase: f64) -> Self {
        self.phase = phase;
        self
    }
    pub const fn adapt(mut self, adapt: f64) -> Self {
        self.adapt = adapt;
        self
    }
    pub const fn velocity_share(mut self, share: f64) -> Self {
        self.velocity_share = share;
        self
    }
    pub const fn volatility_multiple(mut self, multiple: f64) -> Self {
        self.volatility_multiple = multiple;
        self
    }
    pub const fn floor(mut self, floor: f64) -> Self {
        self.floor = floor;
        self
    }
}

fn gain_of(len: f64) -> f64 {
    (2.0 / (len + 1.0)).min(1.0)
}

/// `num / den`, with an empty denominator read as "unbounded" if anything moved and as neutral
/// if nothing did.
fn ratio(num: f64, den: f64) -> f64 {
    if den > 0.0 {
        num / den
    } else if num > 0.0 {
        f64::INFINITY
    } else {
        1.0
    }
}

/// Runs a candidate with per-stage length `length` over a series; valid from the first value.
pub fn run(cfg: Config, length: usize, xs: &[f64]) -> Vec<f64> {
    let len = length as f64;
    let k0 = gain_of(len);
    let s0 = if k0 < 1.0 {
        k0 * k0 / (1.0 - k0)
    } else {
        f64::INFINITY
    };
    let a_v = gain_of((cfg.velocity_share * len).max(2.0));
    let a_s = gain_of((cfg.volatility_multiple * len).max(20.0));
    let c2 = a_v / (2.0 - a_v);

    let (mut v, mut long, mut short, mut absolute, mut variance_ratio) =
        (0.0, f64::NAN, f64::NAN, f64::NAN, 1.0);
    let (mut s1, mut s2, mut s3) = (f64::NAN, f64::NAN, f64::NAN);
    let mut out = Vec::with_capacity(xs.len());
    for (t, &x) in xs.iter().enumerate() {
        if t == 0 {
            (s1, s2, s3) = (x, x, x);
            out.push(x);
            continue;
        }
        let d = x - xs[t - 1];
        // The long-run scale of the bars before this one, so that a sudden move is read against
        // the volatility it broke out of.
        let before = if long.is_nan() { d * d } else { long };
        v += a_v * (d - v);
        long = if long.is_nan() {
            d * d
        } else {
            long + a_s * (d * d - long)
        };
        short = if short.is_nan() {
            d * d
        } else {
            short + a_v * (d * d - short)
        };
        absolute = if absolute.is_nan() {
            d.abs()
        } else {
            absolute + a_v * (d.abs() - absolute)
        };
        let z = match cfg.driver {
            Driver::None => 1.0,
            Driver::Velocity => ratio(v * v, before * c2).sqrt(),
            // 1.2533 = sqrt(pi / 2): mean absolute change to root mean square of a normal change.
            Driver::SmoothedEfficiency => ratio(v.abs(), 1.2533 * absolute * c2.sqrt()),
            Driver::Efficiency => {
                let n = length.min(t);
                let path: f64 = (t + 1 - n..=t).map(|i| (xs[i] - xs[i - 1]).abs()).sum();
                ratio((x - xs[t - n]).abs(), path) * (n as f64).sqrt()
            }
            Driver::RelativeVolatility => ratio(short, before).sqrt(),
            Driver::VarianceRatio => {
                let z2 = ratio(v * v, before * c2);
                if z2.is_finite() {
                    variance_ratio += gain_of(len) * (z2 - variance_ratio);
                    variance_ratio.sqrt()
                } else {
                    f64::INFINITY
                }
            }
        }
        .max(cfg.floor);
        let k = match cfg.map {
            Map::Steady => {
                let s = s0 * z.powf(2.0 * cfg.adapt);
                if s.is_finite() {
                    ((s * s + 4.0 * s).sqrt() - s) / 2.0
                } else {
                    1.0
                }
            }
            Map::Power { lo, hi } => (k0 * z.powf(cfg.adapt).clamp(lo, hi)).min(1.0),
        };
        s1 += k * (x - s1);
        s2 += k * (s1 - s2);
        s3 += k * (s2 - s3);
        out.push(match cfg.core {
            Core::One => s1,
            Core::Two => s2,
            Core::Lead => s2 + cfg.phase * (s2 - s3),
        });
    }
    out
}
