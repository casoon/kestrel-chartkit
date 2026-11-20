use crate::model::Bar;

use super::{Indicator, IndicatorOutput};

/// Kestrel Adaptive Smoother: a cascade of exponential stages whose common gain follows how
/// directed and how unusual the recent movement is.
///
/// A fixed average sits at one point of the trade between lag and noise. This one moves that
/// point bar by bar: while the closes go back and forth it nearly rests, and when they move one
/// way by more than the market usually does it follows, up to passing the close through.
///
/// With `n = (period + 1) / 2`, the base gain `k0 = 2 / (n + 1)`, the gains `a = 2 / (max(n, 2) + 1)`
/// and `b = 2 / (max(4 n, 20) + 1)`, and `d_t = close_t - close_{t-1}`:
///
/// ```text
/// v_t = v_{t-1} + a (d_t   - v_{t-1})        smoothed change, v_0 = 0
/// q_t = q_{t-1} + a (d_t^2 - q_{t-1})        its mean square over the same window
/// Q_t = Q_{t-1} + b (d_t^2 - Q_{t-1})        the long-run mean square
/// z_t^2 = v_t^2 / (c sqrt(Q_{t-1} q_t))      c = a / (2 - a)
/// S_t = k0^2 / (1 - k0) * (z_t^2)^adapt
/// k_t = (sqrt(S_t^2 + 4 S_t) - S_t) / 2
/// s1_t = s1_{t-1} + k_t (close_t - s1_{t-1})
/// s2_t = s2_{t-1} + k_t (s1_t    - s2_{t-1})
/// s3_t = s3_{t-1} + k_t (s2_t    - s3_{t-1})
/// KAS_t = s2_t + phase (s2_t - s3_t)
/// ```
///
/// **The score `z`.** `v / sqrt(c Q)` is the standard score of the smoothed change: on a random
/// walk of constant volatility it is a unit normal. `v / sqrt(c q)` is the same change against
/// the volatility of its own window, an efficiency ratio. `z` is their geometric mean, so it
/// rises with a directed move and rises further when that move is large for this market, while a
/// change of volatility alone moves it by the square root only. It has no unit and no sign: the
/// scale of the series and its direction do not enter.
///
/// **The gain.** `k^2 / (1 - k) = S` is the steady-state gain of the filter that tracks a random
/// walk observed in noise at signal-to-noise ratio `S`. The base ratio belongs to `k0`; the score
/// scales it by `z^(2 adapt)`. So `k` stays in `0..=1` without clamping, equals `k0` at `z = 1`,
/// and reaches `1` (the close passes through) only as the score grows without bound. `adapt = 0`
/// switches the adaptation off and leaves a fixed linear filter.
///
/// **Phase.** `s2` is a twofold exponential average; `s2 - s3` is what the third stage still
/// lags behind it. At a constant gain a straight trend is followed with a lag of
/// `(2 - phase) (period - 1) / 4` bars: at `phase = 0` and `adapt = 0` as much as an exponential
/// average of the same `period`, at `phase = 2` none. The price is overshoot at a step, at a
/// constant gain `(1 + 2 phase) exp(-2 (1 + phase) / phase)` of the step: none at `0`, 0.5 % at
/// `0.5`, 5.5 % at `1`, 12 % at `1.5`, 25 % at `2`. At `phase = 0` the line cannot overshoot at
/// all, whatever the gain does, because every stage is a weighted mean of its input and its own
/// last value.
///
/// **Start.** The first value is the first close, with all three stages on it. The first change
/// seeds `q` and `Q` (and stands in for `Q_{t-1}` on that bar). Where the mean squares are zero
/// the score is `1` if the smoothed change is zero as well, and unbounded otherwise: a move out
/// of a flat series passes through.
///
/// Output: `value` in the price units of the series. [`Indicator::reset`] clears every stage and
/// average.
#[derive(Debug, Clone)]
pub struct Kas {
    period: usize,
    phase: f64,
    adapt: f64,
    base_gain: f64,
    velocity_gain: f64,
    long_gain: f64,
    last: Option<f64>,
    velocity: f64,
    short_square: Option<f64>,
    long_square: Option<f64>,
    stages: [f64; 3],
}

impl Kas {
    pub fn new(period: usize, phase: f64, adapt: f64) -> Self {
        let period = period.max(1);
        let n = (period as f64 + 1.0) / 2.0;
        Self {
            period,
            phase: phase.clamp(0.0, 2.0),
            adapt: adapt.clamp(0.0, 2.0),
            base_gain: 2.0 / (n + 1.0),
            velocity_gain: 2.0 / (n.max(2.0) + 1.0),
            long_gain: 2.0 / ((4.0 * n).max(20.0) + 1.0),
            last: None,
            velocity: 0.0,
            short_square: None,
            long_square: None,
            stages: [0.0; 3],
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 0.5, 1.0)
    }

    pub fn phase(&self) -> f64 {
        self.phase
    }

    pub fn adapt(&self) -> f64 {
        self.adapt
    }

    /// The squared score of this bar's change `d`, updating the three averages.
    fn score_squared(&mut self, d: f64) -> f64 {
        let (a, b) = (self.velocity_gain, self.long_gain);
        self.velocity += a * (d - self.velocity);
        let short = self.short_square.map_or(d * d, |q| q + a * (d * d - q));
        let long_before = self.long_square.unwrap_or(d * d);
        self.short_square = Some(short);
        self.long_square = Some(self.long_square.map_or(d * d, |q| q + b * (d * d - q)));

        let moved = self.velocity * self.velocity;
        let scale = a / (2.0 - a) * (long_before * short).sqrt();
        if scale > 0.0 {
            moved / scale
        } else if moved > 0.0 {
            f64::INFINITY
        } else {
            1.0
        }
    }

    /// The gain of one stage for a squared score.
    fn gain(&self, score_squared: f64) -> f64 {
        let k0 = self.base_gain;
        if k0 >= 1.0 {
            return 1.0;
        }
        let s = k0 * k0 / (1.0 - k0) * score_squared.powf(self.adapt);
        if s.is_finite() {
            ((s * s + 4.0 * s).sqrt() - s) / 2.0
        } else {
            1.0
        }
    }

    /// Feeds one value and returns the smoothed value; the first value comes back unchanged.
    pub fn update(&mut self, src: f64) -> f64 {
        let Some(last) = self.last.replace(src) else {
            self.stages = [src; 3];
            return src;
        };
        let score_squared = self.score_squared(src - last);
        let k = self.gain(score_squared);
        let [s1, s2, s3] = &mut self.stages;
        *s1 += k * (src - *s1);
        *s2 += k * (*s1 - *s2);
        *s3 += k * (*s2 - *s3);
        *s2 + self.phase * (*s2 - *s3)
    }

    pub fn reset(&mut self) {
        self.last = None;
        self.velocity = 0.0;
        self.short_square = None;
        self.long_square = None;
        self.stages = [0.0; 3];
    }
}

impl Indicator for Kas {
    fn name(&self) -> &str {
        "kas"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    /// Three stages whose gain is below the base gain about half of the time, so each is counted
    /// with twice its base time constant, and the long-run mean square with its own.
    fn convergence_period(&self) -> usize {
        let n = (self.period as f64 + 1.0) / 2.0;
        let stage = n + 1.0;
        let long = ((4.0 * n).max(20.0) + 1.0) / 2.0;
        super::settle_bars(&[stage, stage, stage, long])
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        Some(IndicatorOutput::new(self.update(bar.close)))
    }

    fn reset(&mut self) {
        Kas::reset(self)
    }
}

/// Registry definition of `kas`.
pub const KAS_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "kas",
        aliases: &[],
        description: "Kestrel Adaptive Smoother: a cascade of exponential stages whose gain follows how directed and how unusual the recent movement is, so the line rests in back-and-forth movement and follows a real move",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning(
                "length: at phase 0 without adaptation the line lags a straight trend as much as an exponential average of this period",
            ),
            super::definition::ParamSpec::number("phase", 0.5, 0.0, 2.0).meaning(
                "lead: 0 cannot overshoot, 2 follows a straight trend without lag and overshoots a step by a quarter",
            ),
            super::definition::ParamSpec::number("adapt", 1.0, 0.0, 2.0).meaning(
                "strength of the adaptation: 0 is a fixed filter, higher values rest more and jump more",
            ),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Open(
            "acceptance against the measured criteria is pending",
        ),
        build: |p| {
            Ok(Box::new(Kas::new(
                p.count("period"),
                p.number("phase"),
                p.number("adapt"),
            )))
        },
    };

#[cfg(test)]
mod tests {
    use super::Kas;

    fn run(kas: &mut Kas, values: &[f64]) -> Vec<f64> {
        values.iter().map(|v| kas.update(*v)).collect()
    }

    /// A deterministic series with trends, turns and a jump.
    fn series() -> Vec<f64> {
        (0..400)
            .map(|i| {
                let t = i as f64;
                100.0
                    + 4.0 * (t / 23.0).sin()
                    + 1.5 * (t / 3.1).cos()
                    + if i > 250 { 12.0 } else { 0.0 }
                    + 0.01 * t
            })
            .collect()
    }

    #[test]
    fn first_value_is_the_first_input() {
        assert_eq!(Kas::with_defaults().update(123.5), 123.5);
    }

    #[test]
    fn period_one_passes_the_input_through() {
        let values = series();
        assert_eq!(run(&mut Kas::new(1, 0.5, 1.0), &values), values);
    }

    #[test]
    fn negated_series_gives_the_negated_line() {
        let values = series();
        let negated: Vec<f64> = values.iter().map(|v| -v).collect();
        let up = run(&mut Kas::with_defaults(), &values);
        let down = run(&mut Kas::with_defaults(), &negated);
        for (u, d) in up.iter().zip(&down) {
            assert_eq!(*u, -*d);
        }
    }

    #[test]
    fn scale_and_level_of_the_series_carry_over() {
        let values = series();
        let moved: Vec<f64> = values.iter().map(|v| 250.0 * v + 4000.0).collect();
        let plain = run(&mut Kas::with_defaults(), &values);
        let scaled = run(&mut Kas::with_defaults(), &moved);
        for (p, s) in plain.iter().zip(&scaled) {
            assert!((250.0 * p + 4000.0 - s).abs() < 1e-7, "{p} {s}");
        }
    }

    #[test]
    fn without_phase_the_line_stays_inside_the_range_of_the_input() {
        let values = series();
        for adapt in [0.0, 0.5, 1.0, 2.0] {
            let (mut low, mut high) = (f64::INFINITY, f64::NEG_INFINITY);
            let mut kas = Kas::new(14, 0.0, adapt);
            for v in &values {
                low = low.min(*v);
                high = high.max(*v);
                let out = kas.update(*v);
                assert!(
                    out >= low - 1e-12 && out <= high + 1e-12,
                    "{out} outside {low}..{high}"
                );
            }
        }
    }

    #[test]
    fn without_adaptation_it_is_the_fixed_cascade() {
        let values = series();
        let period = 9;
        let k = 2.0 / ((period as f64 + 1.0) / 2.0 + 1.0);
        let phase = 0.75;
        let (mut s1, mut s2, mut s3) = (values[0], values[0], values[0]);
        let mut kas = Kas::new(period, phase, 0.0);
        for (i, v) in values.iter().enumerate() {
            if i > 0 {
                s1 += k * (v - s1);
                s2 += k * (s1 - s2);
                s3 += k * (s2 - s3);
            }
            let expected = s2 + phase * (s2 - s3);
            assert!((kas.update(*v) - expected).abs() < 1e-10);
        }
    }

    #[test]
    fn a_move_out_of_a_flat_series_passes_through() {
        let mut kas = Kas::with_defaults();
        for _ in 0..50 {
            assert_eq!(kas.update(10.0), 10.0);
        }
        assert_eq!(kas.update(11.0), 11.0);
    }

    #[test]
    fn reset_restores_the_start() {
        let values = series();
        let mut kas = Kas::with_defaults();
        let first = run(&mut kas, &values);
        kas.reset();
        assert_eq!(run(&mut kas, &values), first);
    }
}
