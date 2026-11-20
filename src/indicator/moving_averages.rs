use std::collections::VecDeque;

use crate::indicator::smoothing::{Alma, Ema, EmaInit, Jma, Kama, Rma, Sma, SuperSmoother, Wma};
use crate::indicator::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Simple Moving Average (SMA) over the closing price.
///
/// `SMA_t = (close_t + ... + close_{t-period+1}) / period`, the plain mean of the last `period`
/// closes.
///
/// First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
pub struct SmaEngine {
    period: usize,
    sma: Sma,
    alerts: Vec<IndicatorAlert>,
}

impl SmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            sma: Sma::exact(period),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for SmaEngine {
    fn name(&self) -> &str {
        "sma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.sma.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.sma.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Exponential Moving Average (EMA) over the closing price.
///
/// Wraps the shared [`Ema`] smoother rather than repeating its recurrence, so both surfaces stay
/// one calculation. The first output appears with the `period`-th bar in either initialisation:
/// with [`EmaInit::FirstSample`] the average is already running before that and its early values
/// are withheld, with [`EmaInit::Sma`] it is not defined before that at all.
///
/// [`Indicator::reset`] clears the average and the bar counter, so the next series starts
/// deterministically.
#[derive(Debug, Clone)]
pub struct EmaEngine {
    period: usize,
    ema: Ema,
    count: usize,
    alerts: Vec<IndicatorAlert>,
}

impl EmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            ema: Ema::new(period),
            count: 0,
            alerts: Vec::new(),
        }
    }

    /// Selects the initialisation; see [`EmaInit`]. Additive to [`EmaEngine::new`], which keeps
    /// the first-sample seed. Indicators that nest EMAs internally (DEMA, TEMA, MACD, ...) are
    /// deliberately not affected.
    pub fn with_init(mut self, init: EmaInit) -> Self {
        self.ema = self.ema.with_init(init);
        self
    }

    pub fn init(&self) -> EmaInit {
        self.ema.init()
    }
}

impl Indicator for EmaEngine {
    fn name(&self) -> &str {
        "ema"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.count = 0;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let ema = self.ema.update(bar.close);

        self.alerts.clear();
        if self.count < self.period {
            return None;
        }

        ema.map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Weighted Moving Average (WMA) over the closing price.
///
/// Linearly weighted over the last `period` closes: weight `period` on the most recent close down
/// to `1` on the oldest, divided by the weight sum `period * (period + 1) / 2`.
///
/// First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
pub struct WmaEngine {
    period: usize,
    wma: Wma,
    alerts: Vec<IndicatorAlert>,
}

impl WmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            wma: Wma::new(period),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for WmaEngine {
    fn name(&self) -> &str {
        "wma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.wma.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.wma.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Volume-Weighted Moving Average (VWMA) over the closing price.
///
/// `sum(close * volume) / sum(volume)` over the last `period` bars. A window without any volume
/// has nothing to weight by; the value is then the current close rather than a division by zero.
///
/// First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
pub struct VwmaEngine {
    period: usize,
    bars: VecDeque<Bar>,
    alerts: Vec<IndicatorAlert>,
}

impl VwmaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            bars: VecDeque::new(),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for VwmaEngine {
    fn name(&self) -> &str {
        "vwma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.bars.push_back(bar.clone());
        if self.bars.len() > self.period {
            self.bars.pop_front();
        }

        self.alerts.clear();
        if self.bars.len() < self.period {
            return None;
        }

        let mut pv_sum = 0.0f64;
        let mut v_sum = 0.0f64;
        for b in &self.bars {
            pv_sum += b.close * b.volume;
            v_sum += b.volume;
        }

        let vwma = if v_sum > 0.0 {
            pv_sum / v_sum
        } else {
            bar.close
        };
        Some(IndicatorOutput::new(vwma))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Hull Moving Average (HMA) over the closing price.
///
/// `HMA = WMA(2 * WMA(half) - WMA(period), root)` with `half = floor(period / 2)` and
/// `root = round(sqrt(period))`, both at least 1, the rounding half away from zero. The inner
/// difference is formed once both inner averages exist — from the `period`-th bar on — and the
/// outer average runs over that difference series.
///
/// First output: once the outer average holds `root` differences, i.e. with the
/// `period + root - 1`-th bar. [`Indicator::reset`] clears all three averages.
pub struct HmaEngine {
    period: usize,
    wma_half: WmaEngine,
    wma_full: WmaEngine,
    wma_sqrt: WmaEngine,
    alerts: Vec<IndicatorAlert>,
}

impl HmaEngine {
    pub fn new(period: usize) -> Self {
        let half = (period / 2).max(1);
        let sqrt = ((period as f64).sqrt().round() as usize).max(1);
        Self {
            period,
            wma_half: WmaEngine::new(half),
            wma_full: WmaEngine::new(period),
            wma_sqrt: WmaEngine::new(sqrt),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for HmaEngine {
    fn name(&self) -> &str {
        "hma"
    }

    fn warmup_period(&self) -> usize {
        let root = ((self.period as f64).sqrt().round() as usize).max(1);
        self.period + root - 1
    }

    fn reset(&mut self) {
        self.wma_half.reset();
        self.wma_full.reset();
        self.wma_sqrt.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let h_out = self.wma_half.on_bar(bar);
        let f_out = self.wma_full.on_bar(bar);

        self.alerts.clear();
        if let (Some(h), Some(f)) = (h_out, f_out) {
            let diff = 2.0 * h.value - f.value;
            let synthetic_bar = Bar::new(bar.timestamp, diff, diff, diff, diff, 1.0);
            return self.wma_sqrt.on_bar(&synthetic_bar);
        }

        None
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Double Exponential Moving Average (DEMA) over the closing price.
///
/// `DEMA = 2 * e1 - e2`. `e1` is this crate's EMA of the closes ([`EmaEngine`]: seeded with the
/// first close, published from the `period`-th bar on); `e2` is the same EMA taken over the
/// *published* values of `e1`, so the second stage starts with the first published value of the
/// first rather than with the first close.
///
/// That chaining differs from [`super::tema::TemaEngine`], whose three stages all run from the
/// first close. The two produce different early values and converge as the seeds decay.
///
/// First output: once `e2` publishes, i.e. with the `2 * period - 1`-th bar.
/// [`Indicator::reset`] clears both averages.
pub struct DemaEngine {
    period: usize,
    ema1: EmaEngine,
    ema2: EmaEngine,
    alerts: Vec<IndicatorAlert>,
}

impl DemaEngine {
    pub fn new(period: usize) -> Self {
        Self {
            period,
            ema1: EmaEngine::new(period),
            ema2: EmaEngine::new(period),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for DemaEngine {
    fn name(&self) -> &str {
        "dema"
    }

    fn warmup_period(&self) -> usize {
        self.period * 2 - 1
    }

    fn reset(&mut self) {
        self.ema1.reset();
        self.ema2.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let e1_out = self.ema1.on_bar(bar);
        self.alerts.clear();

        if let Some(e1) = e1_out {
            let synth_bar = Bar::new(bar.timestamp, e1.value, e1.value, e1.value, e1.value, 1.0);
            let e2_out = self.ema2.on_bar(&synth_bar);
            if let Some(e2) = e2_out {
                let dema = 2.0 * e1.value - e2.value;
                return Some(IndicatorOutput::new(dema));
            }
        }

        None
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Kaufman's Adaptive Moving Average (KAMA) over the closing price.
///
/// Over the last `period + 1` closes the efficiency ratio is net movement over path length,
/// `ER = |close_t - close_{t-period}| / sum(|close_i - close_{i-1}|)`, and `0` when nothing moved.
/// It places the smoothing constant between a fast and a slow exponential constant:
///
/// ```text
/// sc   = (ER * (2/(fast_period+1) - 2/(slow_period+1)) + 2/(slow_period+1))^2
/// KAMA = KAMA_{t-1} + sc * (close_t - KAMA_{t-1})
/// ```
///
/// The first value is the close of the first bar with a full window; there is no separate seed.
/// Registry defaults: `period = 10`, `fast_period = 2`, `slow_period = 30`.
///
/// First output: with the `period + 1`-th bar. [`Indicator::reset`] clears the window and the
/// average.
pub struct KamaEngine {
    period: usize,
    kama: Kama,
    alerts: Vec<IndicatorAlert>,
}

impl KamaEngine {
    pub fn new(period: usize, fast_period: usize, slow_period: usize) -> Self {
        Self {
            period,
            kama: Kama::new(period, fast_period, slow_period),
            alerts: Vec::new(),
        }
    }
}

impl Indicator for KamaEngine {
    fn name(&self) -> &str {
        "kama"
    }

    fn warmup_period(&self) -> usize {
        self.period + 1
    }

    fn reset(&mut self) {
        self.kama.reset();
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.kama.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

/// Wilder's smoothed moving average (RMA) over the closing price.
///
/// `alpha = 1/period`, `RMA_t = RMA_{t-1} + (close_t - RMA_{t-1}) / period`, seeded with the mean
/// of the first `period` closes (Wilder, *New Concepts in Technical Trading Systems*, 1978). It is
/// the average inside Wilder's RSI, ATR and ADX; as an exponential average it reacts like an
/// [`EmaEngine`] of length `2 * period - 1`.
///
/// First output: with the `period`-th bar. [`Indicator::convergence_period`] states when the seed
/// no longer matters. [`Indicator::reset`] clears the seed window and the average.
#[derive(Debug, Clone)]
pub struct RmaEngine {
    period: usize,
    rma: Rma,
}

impl RmaEngine {
    pub fn new(period: usize) -> Self {
        let period = period.max(1);
        Self {
            period,
            rma: Rma::new(period),
        }
    }
}

impl Indicator for RmaEngine {
    fn name(&self) -> &str {
        "rma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn convergence_period(&self) -> usize {
        self.period.max(super::settle_bars(&[self.period as f64]))
    }

    fn reset(&mut self) {
        self.rma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.rma.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Arnaud Legoux Moving Average (ALMA) over the closing price.
///
/// A Gaussian-weighted average of the last `period` closes (Legoux and Kouzis-Loukas, 2009). With
/// `m = offset * (period - 1)` and `s = period / sigma`, the close `i` bars after the oldest of
/// the window gets the weight `w_i = exp(-(i - m)^2 / (2 s^2))`, and
/// `ALMA_t = sum(w_i * close_i) / sum(w_i)`. `offset` in `0..=1` places the peak of the weights
/// from the oldest (`0`) to the newest (`1`) bar of the window; `sigma` narrows the bell the
/// larger it is. A finite window, so there is no start-up state.
///
/// First output: with the `period`-th bar. [`Indicator::reset`] clears the window.
#[derive(Debug, Clone)]
pub struct AlmaEngine {
    period: usize,
    alma: Alma,
}

impl AlmaEngine {
    pub fn new(period: usize, offset: f64, sigma: f64) -> Self {
        let period = period.max(1);
        Self {
            period,
            alma: Alma::new(period, offset, sigma),
        }
    }
}

impl Indicator for AlmaEngine {
    fn name(&self) -> &str {
        "alma"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    fn reset(&mut self) {
        self.alma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alma.update(bar.close).map(IndicatorOutput::new)
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// An approximation of the Jurik moving average (JMA) over the closing price, following an openly
/// published formula. It is not the product of Jurik Research: the original is not public, so
/// this type reproduces its published shape, not its values.
///
/// With `beta = 0.45 (period - 1) / (0.45 (period - 1) + 2)`, `alpha = beta^power` and
/// `p = phase / 100 + 1.5`, three stages run per close:
///
/// ```text
/// e0 = (1 - alpha) close + alpha e0
/// e1 = (close - e0) (1 - beta) + beta e1
/// e2 = (e0 + p e1 - JMA) (1 - alpha)^2 + alpha^2 e2
/// JMA = JMA + e2
/// ```
///
/// `phase` is clamped to `-100..=100` (a negative phase lags, a positive one overshoots), `power`
/// to at least `1`. The first value is the first close, with `e1 = e2 = 0`.
///
/// First output: with the first bar. [`Indicator::reset`] clears all stages.
#[derive(Debug, Clone)]
pub struct JmaEngine {
    period: usize,
    jma: Jma,
}

impl JmaEngine {
    pub fn new(period: usize, phase: f64, power: f64) -> Self {
        let period = period.max(1);
        Self {
            period,
            jma: Jma::new(period, phase, power),
        }
    }
}

impl Indicator for JmaEngine {
    fn name(&self) -> &str {
        "jma"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    /// Three exponential stages of time constant `1 / (1 - beta)` at most (reached at
    /// `power = 1`), whatever the phase.
    fn convergence_period(&self) -> usize {
        let drift = 0.45 * (self.period - 1) as f64;
        let tau = (drift + 2.0) / 2.0;
        super::settle_bars(&[tau, tau, tau])
    }

    fn reset(&mut self) {
        self.jma.reset();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        Some(IndicatorOutput::new(self.jma.update(bar.close)))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Ehlers' SuperSmoother over the closing price: a 2-pole Butterworth low-pass filter
/// (Ehlers, *Cybernetic Analysis for Stocks and Futures*, 2004; *Cycle Analytics for Traders*,
/// 2013).
///
/// With `a1 = exp(-1.414 pi / period)`, `c2 = 2 a1 cos(1.414 pi / period)`, `c3 = -a1^2` and
/// `c1 = 1 - c2 - c3`: `SS_t = c1 (close_t + close_{t-1}) / 2 + c2 SS_{t-1} + c3 SS_{t-2}`,
/// started from the price: `SS_0 = close_0` and `SS_1 = close_1`, the recursion running from bar
/// 2. A start at the price level leaves a transient only as large as the price change of the
/// first bars (none on a constant series), not as large as the level itself, so the average is
/// published from the `period`-th bar, the cutoff period of the filter.
/// [`Indicator::convergence_period`] states when the remaining transient has decayed.
///
/// [`Indicator::reset`] returns the filter to its start.
#[derive(Debug, Clone)]
pub struct SuperSmootherEngine {
    period: usize,
    ss: SuperSmoother,
    count: usize,
}

impl SuperSmootherEngine {
    pub fn new(period: usize) -> Self {
        let period = period.max(1);
        Self {
            period,
            ss: SuperSmoother::price_seeded(period),
            count: 0,
        }
    }
}

impl Indicator for SuperSmootherEngine {
    fn name(&self) -> &str {
        "super_smoother"
    }

    fn warmup_period(&self) -> usize {
        self.period
    }

    /// The start-up error (the price change of the first bars, not the price level) decays like
    /// `exp(-1.414 pi n / period)`; `20 * period` bars, the shared bound, leave it far below
    /// `1e-3` of that change, so the bound of the zero start still holds.
    fn convergence_period(&self) -> usize {
        self.period.max(super::settle_bars(&[self.period as f64]))
    }

    fn reset(&mut self) {
        self.ss.reset();
        self.count = 0;
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.count += 1;
        let value = self.ss.update(bar.close);
        (self.count >= self.period).then(|| IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        Vec::new()
    }
}

/// Registry definition of `sma` (plan 49).
pub const SMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "sma",
        aliases: &[],
        description: "Simple Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged")
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(SmaEngine::new(p.count("period")))),
    };

/// Registry definition of `ema` (plan 49).
pub const EMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "ema",
        aliases: &[],
        description: "Exponential Moving Average (build_typed accepts init=first_sample|sma for the seed; first_sample is the default)",
        params: &[
            super::definition::ParamSpec::choice("init", "first_sample", &["first_sample", "sma"], false)
                .meaning("seed of the average: the first sample, or the SMA of the first period samples"),
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| {
            let init = match p.choice("init") {
                "sma" => EmaInit::Sma,
                _ => EmaInit::FirstSample,
            };
            Ok(Box::new(EmaEngine::new(p.count("period")).with_init(init)))
        },
    };

/// Registry definition of `wma` (plan 49).
pub const WMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "wma",
        aliases: &[],
        description: "Weighted Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged")
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(WmaEngine::new(p.count("period")))),
    };

/// Registry definition of `vwma` (plan 49).
pub const VWMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "vwma",
        aliases: &[],
        description: "Volume-Weighted Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged")
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price().needs_real_volume(),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(VwmaEngine::new(p.count("period")))),
    };

/// Registry definition of `hma` (plan 49).
pub const HMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "hma",
        aliases: &[],
        description: "Hull Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged")
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(HmaEngine::new(p.count("period")))),
    };

/// Registry definition of `dema` (plan 49).
pub const DEMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "dema",
        aliases: &[],
        description: "Double Exponential Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 20, 1, 10000).meaning("bars averaged")
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| Ok(Box::new(DemaEngine::new(p.count("period")))),
    };

/// Registry definition of `kama` (plan 49).
pub const KAMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "kama",
        aliases: &[],
        description: "Kaufman's Adaptive Moving Average",
        params: &[
            super::definition::ParamSpec::count("period", 10, 1, 10000)
                .meaning("bars of the efficiency ratio"),
            super::definition::ParamSpec::count("fast_period", 2, 1, 10000)
                .meaning("EMA length of the fastest smoothing (efficiency 1)"),
            super::definition::ParamSpec::count("slow_period", 30, 1, 10000)
                .meaning("EMA length of the slowest smoothing (efficiency 0)"),
        ],
        checks: &[super::definition::Check::Less {
            lower: "fast_period",
            upper: "slow_period",
        }],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Verified,
        build: |p| {
            Ok(Box::new(KamaEngine::new(
                p.count("period"),
                p.count("fast_period"),
                p.count("slow_period"),
            )))
        },
    };

/// Registry definition of `rma`.
pub const RMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "rma",
        aliases: &[],
        description: "Wilder's smoothed moving average (alpha = 1/period, seeded with the mean of the first period closes)",
        params: &[super::definition::ParamSpec::count("period", 14, 1, 10000)
            .meaning("bars of the Wilder average")],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| Ok(Box::new(RmaEngine::new(p.count("period")))),
    };

/// Registry definition of `alma`.
pub const ALMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "alma",
        aliases: &[],
        description:
            "Arnaud Legoux Moving Average (Gaussian-weighted average of the last period closes)",
        params: &[
            super::definition::ParamSpec::count("period", 9, 1, 10000)
                .meaning("bars in the weighting window"),
            super::definition::ParamSpec::number("offset", 0.85, 0.0, 1.0)
                .meaning("position of the weight peak in the window, 0 oldest bar to 1 newest"),
            super::definition::ParamSpec::number("sigma", 6.0, 0.1, 100.0)
                .meaning("narrowness of the weight bell: its width is period / sigma"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(AlmaEngine::new(
                p.count("period"),
                p.number("offset"),
                p.number("sigma"),
            )))
        },
    };

/// Registry definition of `jma`.
pub const JMA_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "jma",
        aliases: &[],
        description: "An approximation of the Jurik moving average, following an openly published formula; it is not the product of Jurik Research, whose original is not public, and does not reproduce its values",
        params: &[
            super::definition::ParamSpec::count("period", 14, 1, 10000)
                .meaning("length: the larger, the smoother and the later"),
            super::definition::ParamSpec::number("phase", 0.0, -100.0, 100.0)
                .meaning("-100 lags, 100 overshoots (the type clamps to this range)"),
            super::definition::ParamSpec::number("power", 2.0, 1.0, 10.0)
                .meaning("exponent of the smoothing factor (the type clamps it to at least 1)"),
        ],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| {
            Ok(Box::new(JmaEngine::new(
                p.count("period"),
                p.number("phase"),
                p.number("power"),
            )))
        },
    };

/// Registry definition of `super_smoother`.
pub const SUPER_SMOOTHER_DEFINITION: super::definition::IndicatorDefinition =
    super::definition::IndicatorDefinition {
        name: "super_smoother",
        aliases: &[],
        description: "Ehlers' SuperSmoother, a 2-pole Butterworth low-pass filter",
        params: &[super::definition::ParamSpec::count("period", 10, 1, 10000)
            .meaning("cutoff period of the filter in bars")],
        checks: &[],
        facts: super::definition::IndicatorFacts::price(),
        verification: super::definition::Verification::Open(
            "no independent second implementation yet",
        ),
        build: |p| Ok(Box::new(SuperSmootherEngine::new(p.count("period")))),
    };
