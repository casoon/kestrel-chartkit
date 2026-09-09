use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Bar;

use super::smoothing::{crossed_over, crossed_under, Ema, Rma, Sma, Wma};
use super::{Indicator, IndicatorAlert, IndicatorOutput};

/// How the true range is averaged.
///
/// This is the ATR's own smoothing. The separate averaging of the percentage series into
/// `extra["signal"]` is not affected — it stays Wilder's, so the alerts keep their meaning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TrueRangeSmoothing {
    /// Wilder's smoothing, `alpha = 1/N`, seeded with the SMA of the first `N` true ranges. The
    /// default, and the historical behaviour of this indicator.
    #[default]
    Rma,
    /// Plain average of the last `N` true ranges.
    Sma,
    /// Exponential, `alpha = 2/(N+1)`, seeded with the first true range. Published from the
    /// `N`-th bar on, so the output start does not depend on the method.
    Ema,
    /// Linearly weighted over the last `N` true ranges, heaviest on the most recent.
    Wma,
}

/// The true-range average in whichever form was selected. Every mode publishes from the `N`-th
/// true range on, so switching the method moves the values, never the first output.
#[derive(Debug, Clone)]
enum TrSmoother {
    Rma(Rma),
    Sma(Sma),
    Ema { ema: Ema, len: usize, seen: usize },
    Wma(Wma),
}

impl TrSmoother {
    fn new(method: TrueRangeSmoothing, len: usize) -> Self {
        match method {
            TrueRangeSmoothing::Rma => Self::Rma(Rma::new(len)),
            TrueRangeSmoothing::Sma => Self::Sma(Sma::new(len)),
            TrueRangeSmoothing::Ema => Self::Ema {
                ema: Ema::new(len),
                len,
                seen: 0,
            },
            TrueRangeSmoothing::Wma => Self::Wma(Wma::new(len)),
        }
    }

    fn update(&mut self, tr: f64) -> Option<f64> {
        match self {
            Self::Rma(rma) => rma.update(tr),
            Self::Sma(sma) => sma.update(tr),
            Self::Ema { ema, len, seen } => {
                let value = ema.update(tr)?;
                *seen += 1;
                (*seen >= *len).then_some(value)
            }
            Self::Wma(wma) => wma.update(tr),
        }
    }

    fn reset(&mut self) {
        match self {
            Self::Rma(rma) => rma.reset(),
            Self::Sma(sma) => sma.reset(),
            Self::Ema { ema, seen, .. } => {
                ema.reset();
                *seen = 0;
            }
            Self::Wma(wma) => wma.reset(),
        }
    }
}

/// Average True Range, emitted in two units.
///
/// True range: `TR_1 = high - low`, then
/// `TR_t = max(high - low, |high - close_{t-1}|, |low - close_{t-1}|)` — both gap terms are
/// absent on the first bar because there is no previous close.
///
/// Averaged with [`TrueRangeSmoothing`], Wilder's by default: the seed is the SMA of the first
/// `atr_len` true ranges, then `ATR_t = ATR_{t-1} + (TR_t - ATR_{t-1}) / atr_len`. Every method
/// publishes from the `atr_len`-th true range on, so the choice changes the values but not when
/// they start.
///
/// Per-bar outputs:
/// - `value`: `100 * ATR / close`, in percent of the closing price (0 for `close <= 0`).
/// - `extra["raw"]`: the same ATR in the series' price units — neither a second calculation nor
///   a back-conversion from the percentage. A price distance, not money or contract risk: a
///   monetary amount only follows from contract size and tick value (see [`crate::contract`]).
/// - `extra["signal"]`: `Rma_{sig_len}` over the percentage series, in percent.
///
/// First output: with the `sig_len`-th percentage observation, i.e. after `atr_len + sig_len - 1`
/// bars — there is no partial output before that, `raw` included. [`Indicator::reset`] clears the
/// previous close, both smoothers and the alerts, so the next series starts deterministically.
#[derive(Debug, Clone)]
pub struct Atr {
    atr_len: usize,
    smoothing: TrueRangeSmoothing,
    prev_close: Option<f64>,
    tr_average: TrSmoother,
    signal_rma: Rma,

    prev_atr_disp: Option<f64>,
    prev_signal: Option<f64>,
    bars_seen: usize,
    warmup_period: usize,

    alerts: AtrAlerts,
}

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct AtrAlerts {
    pub expansion: bool,
    pub contraction: bool,
    pub regime_strength: f64,
}

impl Atr {
    pub fn new(atr_len: usize, sig_len: usize) -> Self {
        Self {
            atr_len,
            smoothing: TrueRangeSmoothing::Rma,
            prev_close: None,
            tr_average: TrSmoother::new(TrueRangeSmoothing::Rma, atr_len),
            signal_rma: Rma::new(sig_len),
            prev_atr_disp: None,
            prev_signal: None,
            bars_seen: 0,
            warmup_period: atr_len + sig_len - 1,
            alerts: AtrAlerts::default(),
        }
    }

    pub fn with_defaults() -> Self {
        // Matches the registry's "atr" catalog default (atr_len=14, sig_len=20) -- sig_len was
        // previously 14 here, silently diverging from the registry-built default.
        Self::new(14, 20)
    }

    pub fn with_period(atr_len: usize) -> Self {
        Self::new(atr_len, 14)
    }

    /// Selects how the true range is averaged; see [`TrueRangeSmoothing`].
    ///
    /// Additive to the existing constructors, which keep Wilder's. The signal line stays
    /// Wilder-smoothed either way — carrying this choice over to it would change the alerts
    /// without anyone asking for that.
    ///
    /// Resets the true-range average, so this belongs before the first bar.
    pub fn with_smoothing(mut self, method: TrueRangeSmoothing) -> Self {
        self.smoothing = method;
        self.tr_average = TrSmoother::new(method, self.atr_len);
        self
    }

    pub fn smoothing(&self) -> TrueRangeSmoothing {
        self.smoothing
    }
}

impl Indicator for Atr {
    fn name(&self) -> &str {
        "atr"
    }

    fn warmup_period(&self) -> usize {
        self.warmup_period
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts = AtrAlerts::default();
        self.bars_seen += 1;

        let tr = match self.prev_close {
            None => bar.high - bar.low,
            Some(prev_close) => (bar.high - bar.low)
                .max((bar.high - prev_close).abs())
                .max((bar.low - prev_close).abs()),
        };
        self.prev_close = Some(bar.close);

        let atr_raw = self.tr_average.update(tr)?;
        let atr_disp = if bar.close > 0.0 {
            100.0 * atr_raw / bar.close
        } else {
            0.0
        };
        let atr_signal = self.signal_rma.update(atr_disp)?;

        if let (Some(prev_disp), Some(prev_sig)) = (self.prev_atr_disp, self.prev_signal) {
            self.alerts.expansion = crossed_over(prev_disp, prev_sig, atr_disp, atr_signal);
            self.alerts.contraction = crossed_under(prev_disp, prev_sig, atr_disp, atr_signal);
            self.alerts.regime_strength = if atr_signal != 0.0 {
                ((atr_disp - atr_signal) / atr_signal).abs().clamp(0.0, 1.0)
            } else {
                0.0
            };
        }
        self.prev_atr_disp = Some(atr_disp);
        self.prev_signal = Some(atr_signal);

        let mut extra = HashMap::new();
        extra.insert("signal".to_string(), atr_signal);
        extra.insert("raw".to_string(), atr_raw);

        Some(IndicatorOutput::with_extra(atr_disp, extra))
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.tr_average.reset();
        self.signal_rma.reset();
        self.prev_atr_disp = None;
        self.prev_signal = None;
        self.bars_seen = 0;
        self.alerts = AtrAlerts::default();
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        let a = self.alerts;
        let mut out = Vec::new();
        if a.expansion {
            out.push(IndicatorAlert {
                kind: "expansion".to_string(),
                note: "ATR · VOLA EXPANSION".to_string(),
                strength: a.regime_strength,
            });
        }
        if a.contraction {
            out.push(IndicatorAlert {
                kind: "contraction".to_string(),
                note: "ATR · VOLA CONTRACTION".to_string(),
                strength: a.regime_strength,
            });
        }
        out
    }
}
