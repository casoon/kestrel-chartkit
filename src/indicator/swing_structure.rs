use crate::model::{Bar, SeriesCapabilities};

use super::atr::TrueRangeSmoothing;
use super::definition::{IndicatorDefinition, ParamSpec};
use super::kernels::{AtrKernel, PivotRule};
use super::{Indicator, IndicatorOutput};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Trend-quality classification derived from the recent correction/impulse ratio trend
/// (plan Anhang D).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum TrendQuality {
    Strengthening,
    Weakening,
    Stable,
}

/// Statistical Swing-Structure output (plan Anhang D): trend strength expressed via the
/// relative size of consecutive impulse/correction legs, ATR-normalized, instead of a
/// binary "trend up/down" flag.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SwingStructureOutput {
    pub median_impulse_atr: f64,
    pub median_correction_atr: f64,
    pub correction_impulse_ratio: f64,
    /// Most recent corrections first (newest → oldest), in ATR.
    pub recent_corrections_atr: Vec<f64>,
    pub trend_quality: TrendQuality,
    pub current_retracement_atr: f64,
    pub entry_zone_reached: bool,
    pub remaining_potential_atr: f64,
    pub required_stop_atr: f64,
    pub potential_crv: f64,
    pub last_impulse_velocity_atr: f64,
    /// Which series this output was computed on, if the caller attached one via
    /// [`SwingStructureOutput::with_capabilities`]. `None` by default: a structure result is
    /// only meaningful for the series (session cut,
    /// roll/adjustment, provenance) it was computed on.
    pub series_capabilities: Option<SeriesCapabilities>,
}

impl SwingStructureOutput {
    /// Tags this output with the series it was computed on. See
    /// [`SwingStructureOutput::series_capabilities`] for why this matters.
    pub fn with_capabilities(mut self, capabilities: SeriesCapabilities) -> Self {
        self.series_capabilities = Some(capabilities);
        self
    }
}

struct Pivot {
    /// Absolute bar index (monotonically increasing, survives window trimming).
    index: usize,
    price: f64,
    is_high: bool,
}

/// Detects swing pivots and derives `SwingStructureOutput`. Reuses the same left/right pivot
/// window as `pivots_structure::PivotStructureEngine`, but exposes the underlying leg sizes
/// instead of a single bounded score.
///
/// This is a composite/consumer of another indicator's output, not a pure `Indicator`: it
/// needs the *raw* ATR in price units on every bar, not the `%`-of-price value that
/// `indicator::atr::Atr` returns. Convert via `atr_raw = atr_pct / 100.0 * bar.close` before
/// calling `update`.
pub struct SwingStructureEngine {
    left: usize,
    right: usize,
    max_swings: usize,
    bars: Vec<(usize, Bar)>,
    pivots: Vec<Pivot>,
    next_index: usize,
}

impl SwingStructureEngine {
    pub fn new(left: usize, right: usize, max_swings: usize) -> Self {
        Self {
            left,
            right,
            max_swings,
            bars: Vec::new(),
            pivots: Vec::new(),
            next_index: 0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(5, 5, 50)
    }

    pub fn reset(&mut self) {
        self.bars.clear();
        self.pivots.clear();
        self.next_index = 0;
    }

    /// Feed one bar plus the current raw ATR (price units, not `%`). Returns a fresh
    /// `SwingStructureOutput` once enough alternating swing legs have been confirmed.
    pub fn update(&mut self, bar: &Bar, atr: f64) -> Option<SwingStructureOutput> {
        self.bars.push((self.next_index, bar.clone()));
        self.next_index += 1;
        let max_history = (self.left + self.right + 1) * (self.max_swings + 2);
        if self.bars.len() > max_history {
            self.bars.remove(0);
        }

        if atr <= 0.0 {
            return None;
        }

        self.detect_pivot();

        if self.pivots.len() < 4 {
            return None;
        }

        // Legs between consecutive confirmed pivots: (size in ATR, is-up-leg, bar span).
        let mut legs: Vec<(f64, bool, usize)> = Vec::new();
        for w in self.pivots.windows(2) {
            let [a, b] = w else { continue };
            let size = (b.price - a.price).abs() / atr;
            let is_up = b.price > a.price;
            let span = b.index.saturating_sub(a.index).max(1);
            legs.push((size, is_up, span));
        }
        if legs.len() > self.max_swings {
            let excess = legs.len() - self.max_swings;
            legs.drain(0..excess);
        }

        let highs: Vec<f64> = self
            .pivots
            .iter()
            .filter(|p| p.is_high)
            .map(|p| p.price)
            .collect();
        let lows: Vec<f64> = self
            .pivots
            .iter()
            .filter(|p| !p.is_high)
            .map(|p| p.price)
            .collect();
        let h_last = highs.last().copied();
        let h_prev = highs.get(highs.len().saturating_sub(2)).copied();
        let l_last = lows.last().copied();
        let l_prev = lows.get(lows.len().saturating_sub(2)).copied();

        let bullish = highs.len() >= 2
            && lows.len() >= 2
            && matches!((h_last, h_prev, l_last, l_prev), (Some(hl), Some(hp), Some(ll), Some(lp)) if hl > hp && ll > lp);
        let bearish = highs.len() >= 2
            && lows.len() >= 2
            && matches!((h_last, h_prev, l_last, l_prev), (Some(hl), Some(hp), Some(ll), Some(lp)) if hl < hp && ll < lp);
        let trend_up = bullish || (!bearish && legs.last().map(|l| l.1).unwrap_or(true));

        let mut impulses: Vec<f64> = Vec::new();
        let mut corrections: Vec<f64> = Vec::new();
        let mut impulse_velocities: Vec<f64> = Vec::new();
        for &(size, is_up, span) in &legs {
            if is_up == trend_up {
                impulses.push(size);
                impulse_velocities.push(size / span as f64);
            } else {
                corrections.push(size);
            }
        }

        if impulses.len() < 2 || corrections.len() < 2 {
            return None;
        }

        let median_impulse_atr = median(&impulses);
        let median_correction_atr = median(&corrections);
        let correction_impulse_ratio = if median_impulse_atr > 0.0 {
            median_correction_atr / median_impulse_atr
        } else {
            0.0
        };

        let recent_corrections_atr: Vec<f64> = corrections.iter().rev().take(3).copied().collect();
        let trend_quality = if recent_corrections_atr.len() >= 2 {
            let newest = recent_corrections_atr.first().copied().unwrap_or(0.0);
            let oldest = recent_corrections_atr.last().copied().unwrap_or(0.0);
            if newest < oldest {
                TrendQuality::Strengthening
            } else if newest > oldest {
                TrendQuality::Weakening
            } else {
                TrendQuality::Stable
            }
        } else {
            TrendQuality::Stable
        };

        let last_swing_price = self.pivots.last().map(|p| p.price).unwrap_or(bar.close);
        let current_retracement_atr = (bar.close - last_swing_price).abs() / atr;
        let entry_zone_reached = current_retracement_atr >= median_correction_atr * 0.8;
        let remaining_potential_atr = (median_impulse_atr - current_retracement_atr).max(0.0);
        let required_stop_atr = median_correction_atr.max(0.1);
        let potential_crv = if required_stop_atr > 0.0 {
            remaining_potential_atr / required_stop_atr
        } else {
            0.0
        };
        let last_impulse_velocity_atr = impulse_velocities.last().copied().unwrap_or(0.0);

        Some(SwingStructureOutput {
            median_impulse_atr,
            median_correction_atr,
            correction_impulse_ratio,
            recent_corrections_atr,
            trend_quality,
            current_retracement_atr,
            entry_zone_reached,
            remaining_potential_atr,
            required_stop_atr,
            potential_crv,
            last_impulse_velocity_atr,
            series_capabilities: None,
        })
    }

    fn detect_pivot(&mut self) {
        let req_len = self.left + self.right + 1;
        if self.bars.len() < req_len {
            return;
        }
        let candidate_idx = self.bars.len() - 1 - self.right;
        let cand_high = self.bars[candidate_idx].1.high;
        let cand_low = self.bars[candidate_idx].1.low;
        let rule = PivotRule::new(self.left, self.right, true, true);
        let window = &self.bars[candidate_idx - self.left..=candidate_idx + self.right];
        let is_high = rule.holds(false, window.iter().map(|(_, b)| b.high));
        let is_low = rule.holds(true, window.iter().map(|(_, b)| b.low));
        let abs_index = self.bars[candidate_idx].0;
        if is_high {
            self.push_pivot(abs_index, cand_high, true);
        }
        if is_low {
            self.push_pivot(abs_index, cand_low, false);
        }
    }

    fn push_pivot(&mut self, index: usize, price: f64, is_high: bool) {
        self.pivots.push(Pivot {
            index,
            price,
            is_high,
        });
        if self.pivots.len() > self.max_swings * 2 + 4 {
            self.pivots.remove(0);
        }
    }
}

/// The catalog entry `swing_structure`: [`SwingStructureEngine`] fed with a Wilder ATR (raw, price
/// units) of `atr_len` bars, as an [`Indicator`]. `value` is `correction_impulse_ratio`; `extra`
/// holds `median_impulse_atr`, `median_correction_atr`, `current_retracement_atr`,
/// `entry_zone_reached` (0 or 1), `remaining_potential_atr`, `required_stop_atr`, `potential_crv`,
/// `last_impulse_velocity_atr`, `recent_correction_atr_1` .. `_3` (newest first, as many as
/// exist) and `trend_quality` (1 strengthening, 0 stable, -1 weakening; also the `state` label).
///
/// Pivots are only detected from the first bar that has an ATR, so the first output needs the ATR
/// plus four confirmed pivots with two impulse and two correction legs: it depends on the data,
/// and `warmup_period` is the earliest bar. [`Indicator::reset`] starts over.
pub struct SwingStructureIndicator {
    left: usize,
    right: usize,
    atr_len: usize,
    engine: SwingStructureEngine,
    atr: AtrKernel,
}

impl SwingStructureIndicator {
    pub fn new(left: usize, right: usize, max_swings: usize, atr_len: usize) -> Self {
        Self {
            left,
            right,
            atr_len,
            engine: SwingStructureEngine::new(left, right, max_swings),
            atr: AtrKernel::new(TrueRangeSmoothing::Rma, atr_len),
        }
    }
}

impl Indicator for SwingStructureIndicator {
    fn name(&self) -> &str {
        "swing_structure"
    }

    fn warmup_period(&self) -> usize {
        self.atr_len.max(self.left + self.right + 1)
    }

    fn reset(&mut self) {
        self.engine.reset();
        self.atr = AtrKernel::new(TrueRangeSmoothing::Rma, self.atr_len);
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        // Without an ATR the engine still takes the bar into its history but detects nothing.
        let atr = self.atr.push(bar).unwrap_or(0.0);
        let out = self.engine.update(bar, atr)?;
        let (code, label) = match out.trend_quality {
            TrendQuality::Strengthening => (1.0, "strengthening"),
            TrendQuality::Stable => (0.0, "stable"),
            TrendQuality::Weakening => (-1.0, "weakening"),
        };
        let mut output = IndicatorOutput::new(out.correction_impulse_ratio).with_state(label);
        for (key, value) in [
            ("median_impulse_atr", out.median_impulse_atr),
            ("median_correction_atr", out.median_correction_atr),
            ("current_retracement_atr", out.current_retracement_atr),
            (
                "entry_zone_reached",
                f64::from(u8::from(out.entry_zone_reached)),
            ),
            ("remaining_potential_atr", out.remaining_potential_atr),
            ("required_stop_atr", out.required_stop_atr),
            ("potential_crv", out.potential_crv),
            ("last_impulse_velocity_atr", out.last_impulse_velocity_atr),
            ("trend_quality", code),
        ] {
            output.extra.insert(key.to_string(), value);
        }
        for (i, correction) in out.recent_corrections_atr.iter().enumerate() {
            output
                .extra
                .insert(format!("recent_correction_atr_{}", i + 1), *correction);
        }
        Some(output)
    }
}

/// Registry definition of `swing_structure`.
pub const SWING_STRUCTURE_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "swing_structure",
    aliases: &[],
    description: "Statistical swing structure: the median correction over the median impulse leg (both in ATRs) as value, with the retracement, remaining potential, required stop, reward-to-risk and impulse velocity in ATRs, the newest three corrections and the trend quality (-1 weakening .. 1 strengthening) as extras",
    params: &[
        ParamSpec::count("left", 5, 1, 1000).meaning("bars left of a swing pivot"),
        ParamSpec::count("right", 5, 1, 1000).meaning("bars right of a swing pivot (the confirmation delay)"),
        ParamSpec::count("max_swings", 50, 4, 1000)
            .meaning("newest swing legs the medians are taken over (at least 4: two impulse and two correction legs)"),
        ParamSpec::count("atr_len", 14, 1, 10000).meaning("Wilder ATR length the legs are measured in"),
    ],
    checks: &[],
    build: |p| {
        Ok(Box::new(SwingStructureIndicator::new(
            p.count("left"),
            p.count("right"),
            p.count("max_swings"),
            p.count("atr_len"),
        )))
    },
};

fn median(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    if sorted.len().is_multiple_of(2) {
        let m1 = sorted.get(mid.saturating_sub(1)).copied().unwrap_or(0.0);
        let m2 = sorted.get(mid).copied().unwrap_or(0.0);
        (m1 + m2) / 2.0
    } else {
        sorted.get(mid).copied().unwrap_or(0.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, high: f64, low: f64, close: f64) -> Bar {
        Bar::new(t, close, high, low, close, 100.0)
    }

    #[test]
    fn zigzag_series_eventually_produces_output() {
        let mut engine = SwingStructureEngine::new(2, 2, 10);
        let mut result = None;
        // Alternating up/down legs so pivots + impulses/corrections accumulate.
        let mut price = 100.0;
        for i in 0..80 {
            let leg = i / 10;
            let up = leg % 2 == 0;
            price += if up { 1.0 } else { -0.5 };
            let out = engine.update(&bar(i, price + 1.0, price - 1.0, price), 2.0);
            if out.is_some() {
                result = out;
            }
        }
        assert!(
            result.is_some(),
            "expected SwingStructureOutput once enough legs are confirmed"
        );
    }

    #[test]
    fn zero_atr_yields_no_output() {
        let mut engine = SwingStructureEngine::with_defaults();
        assert!(engine.update(&bar(0, 101.0, 99.0, 100.0), 0.0).is_none());
    }

    fn sample_capabilities() -> SeriesCapabilities {
        SeriesCapabilities {
            volume: crate::model::VolumeKind::RealTurnover,
            trade_direction: false,
            session: crate::model::SessionKind::Regular,
            continuity: crate::model::ContinuityKind::SingleContract,
            price_adjustment: crate::model::PriceAdjustment::Raw,
            provenance: crate::model::Provenance::Exchange,
            liquidity_tier: crate::model::LiquidityTier::Deep,
        }
    }

    #[test]
    fn output_defaults_to_no_capabilities_and_can_be_tagged() {
        let mut engine = SwingStructureEngine::new(2, 2, 10);
        let mut result = None;
        let mut price = 100.0;
        for i in 0..80 {
            let leg = i / 10;
            let up = leg % 2 == 0;
            price += if up { 1.0 } else { -0.5 };
            let out = engine.update(&bar(i, price + 1.0, price - 1.0, price), 2.0);
            if out.is_some() {
                result = out;
            }
        }
        let result = result.expect("expected SwingStructureOutput once enough legs are confirmed");
        assert_eq!(result.series_capabilities, None);

        let tagged = result.with_capabilities(sample_capabilities());
        assert_eq!(tagged.series_capabilities, Some(sample_capabilities()));
    }
}
