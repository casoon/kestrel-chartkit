//! Volume oscillators: the volume flow indicator (Katsanos, 2004), the volume zone oscillator
//! (Khalil and Steckler, 2011), the volume price confirmation indicator (Dormeier, 2007) and the
//! market facilitation index (Williams, 1995).

use std::collections::{HashMap, VecDeque};

use super::definition::{Check, IndicatorDefinition, IndicatorFacts, ParamSpec, Verification};
use super::smoothing::{crossed_over, crossed_under, Ema};
use super::{Indicator, IndicatorAlert, IndicatorOutput};
use crate::model::Bar;

/// Window of the volatility of the log typical-price ratios, fixed as in the published listing.
const VFI_VOLATILITY_BARS: usize = 30;

/// Volume flow indicator. Typical price `tp = (high + low + close) / 3`; `inter = ln(tp / tp_prev)`
/// and `vinter` is the population standard deviation of the last 30 `inter`. A bar's volume counts
/// as `vc = min(volume, vcoef * vave)`, `vave` being the mean volume of the `period` bars before
/// it, with its sign from the typical-price change `mf` against `coef * vinter * close`:
/// positive above, negative below the negated cutoff, else zero. `raw` is the sum of the signed
/// volumes over `period` bars divided by `vave` (published only for `vave > 0`); **`value` is
/// `Ema(smoothing)` of `raw`**, seeded with the first `raw`, which advances only on bars with a
/// `raw`. Extra `raw`. Alerts `bull` / `bear` when `value` crosses zero.
///
/// First output: with bar number `max(30, period) + period`. A bar whose typical price is not
/// positive leaves every `inter` it enters, and so every window containing one, undefined.
/// [`Indicator::reset`] starts over.
pub struct VolumeFlowIndicator {
    period: usize,
    coef: f64,
    vcoef: f64,
    ema: Ema,
    prev_tp: Option<f64>,
    inters: VecDeque<Option<f64>>,
    /// The last `period` volumes before the current bar.
    volumes: VecDeque<f64>,
    vcps: VecDeque<Option<f64>>,
    prev_value: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumeFlowIndicator {
    pub fn new(period: usize, coef: f64, vcoef: f64, smoothing: usize) -> Self {
        Self {
            period: period.max(2),
            coef,
            vcoef,
            ema: Ema::new(smoothing.max(1)),
            prev_tp: None,
            inters: VecDeque::new(),
            volumes: VecDeque::new(),
            vcps: VecDeque::new(),
            prev_value: None,
            alerts: Vec::new(),
        }
    }

    fn start(&self) -> usize {
        VFI_VOLATILITY_BARS.max(self.period)
    }
}

impl Indicator for VolumeFlowIndicator {
    fn name(&self) -> &str {
        "volume_flow_indicator"
    }

    fn warmup_period(&self) -> usize {
        self.start() + self.period
    }

    fn reset(&mut self) {
        self.ema.reset();
        self.prev_tp = None;
        self.inters.clear();
        self.volumes.clear();
        self.vcps.clear();
        self.prev_value = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let tp = (bar.high + bar.low + bar.close) / 3.0;
        let (inter, mf) = match self.prev_tp {
            Some(prev) => (
                (tp > 0.0 && prev > 0.0).then(|| (tp / prev).ln()),
                Some(tp - prev),
            ),
            None => (None, None),
        };
        self.prev_tp = Some(tp);
        if mf.is_some() {
            self.inters.push_back(inter);
            if self.inters.len() > VFI_VOLATILITY_BARS {
                self.inters.pop_front();
            }
        }

        let vinter = if self.inters.len() == VFI_VOLATILITY_BARS
            && self.inters.iter().all(Option::is_some)
        {
            let n = VFI_VOLATILITY_BARS as f64;
            let mean = self.inters.iter().flatten().sum::<f64>() / n;
            let var = self
                .inters
                .iter()
                .flatten()
                .map(|x| (x - mean) * (x - mean))
                .sum::<f64>()
                / n;
            Some(var.sqrt())
        } else {
            None
        };

        let vave = (self.volumes.len() == self.period)
            .then(|| self.volumes.iter().sum::<f64>() / self.period as f64);

        let vcp = match (vave, vinter, mf) {
            (Some(vave), Some(vinter), Some(mf)) => {
                let vc = bar.volume.min(self.vcoef * vave);
                let cutoff = self.coef * vinter * bar.close;
                Some(if mf > cutoff {
                    vc
                } else if mf < -cutoff {
                    -vc
                } else {
                    0.0
                })
            }
            _ => None,
        };
        self.vcps.push_back(vcp);
        if self.vcps.len() > self.period {
            self.vcps.pop_front();
        }
        self.volumes.push_back(bar.volume);
        if self.volumes.len() > self.period {
            self.volumes.pop_front();
        }

        let raw = match vave {
            Some(vave)
                if vave > 0.0
                    && self.vcps.len() == self.period
                    && self.vcps.iter().all(Option::is_some) =>
            {
                Some(self.vcps.iter().flatten().sum::<f64>() / vave)
            }
            _ => None,
        };
        let Some(raw) = raw else {
            self.prev_value = None;
            return None;
        };
        let value = self.ema.update(raw)?;
        if let Some(prev) = self.prev_value {
            if crossed_over(prev, 0.0, value, 0.0) {
                self.alerts.push(IndicatorAlert::new(
                    "bull",
                    "volume flow crossed above zero",
                    1.0,
                ));
            }
            if crossed_under(prev, 0.0, value, 0.0) {
                self.alerts.push(IndicatorAlert::new(
                    "bear",
                    "volume flow crossed below zero",
                    1.0,
                ));
            }
        }
        self.prev_value = Some(value);
        Some(IndicatorOutput::with_extra(
            value,
            HashMap::from([("raw".to_string(), raw)]),
        ))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const VOLUME_FLOW_INDICATOR_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "volume_flow_indicator",
    aliases: &["vfi"],
    description: "Volume flow indicator (Katsanos, 2004): the volume of bars whose typical price rose or fell by more than a cutoff, capped at a multiple of the average volume, summed over period bars and divided by the average volume before the bar, smoothed; the cutoff uses a fixed 30-bar volatility of log typical-price ratios. Extra raw is the unsmoothed ratio; nothing is published while the average volume is zero",
    params: &[
        ParamSpec::count("period", 130, 2, 10000).meaning("bars summed, and averaged for the volume cap"),
        ParamSpec::number("coef", 0.2, 0.0, 10.0)
            .meaning("cutoff in volatilities of the log typical-price ratio, times the close"),
        ParamSpec::number("vcoef", 2.5, 1e-9, 100.0)
            .meaning("cap on a bar's volume, in average volumes"),
        ParamSpec::count("smoothing", 3, 1, 1000).meaning("length of the average over the ratio"),
    ],
    checks: &[],
    facts: IndicatorFacts::unitless()
        .needs_real_volume()
        .range_dependent(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(VolumeFlowIndicator::new(
            p.count("period"),
            p.number("coef"),
            p.number("vcoef"),
            p.count("smoothing"),
        )))
    },
};

/// Volume zone oscillator. From the second bar `R = sign(close - close_prev) * volume` (zero when
/// unchanged); **`value = 100 * Ema(period)(R) / Ema(period)(volume)`**, both averages seeded with
/// their first input on bar 2, published only while the volume average is not zero. Alerts `bull` /
/// `bear` when `value` crosses zero. First output: with the second bar.
pub struct VolumeZoneOscillator {
    period: usize,
    prev_close: Option<f64>,
    signed: Ema,
    total: Ema,
    prev_value: Option<f64>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumeZoneOscillator {
    pub fn new(period: usize) -> Self {
        let period = period.max(1);
        Self {
            period,
            prev_close: None,
            signed: Ema::new(period),
            total: Ema::new(period),
            prev_value: None,
            alerts: Vec::new(),
        }
    }
}

impl Indicator for VolumeZoneOscillator {
    fn name(&self) -> &str {
        "volume_zone_oscillator"
    }

    fn warmup_period(&self) -> usize {
        2
    }

    fn reset(&mut self) {
        self.prev_close = None;
        self.signed = Ema::new(self.period);
        self.total = Ema::new(self.period);
        self.prev_value = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        let prev = self.prev_close.replace(bar.close);
        let prev = prev?;
        let sign = if bar.close > prev {
            1.0
        } else if bar.close < prev {
            -1.0
        } else {
            0.0
        };
        let vp = self.signed.update(sign * bar.volume)?;
        let tv = self.total.update(bar.volume)?;
        if tv == 0.0 {
            self.prev_value = None;
            return None;
        }
        let value = 100.0 * vp / tv;
        if let Some(prev) = self.prev_value {
            if crossed_over(prev, 0.0, value, 0.0) {
                self.alerts.push(IndicatorAlert::new(
                    "bull",
                    "volume zone crossed above zero",
                    1.0,
                ));
            }
            if crossed_under(prev, 0.0, value, 0.0) {
                self.alerts.push(IndicatorAlert::new(
                    "bear",
                    "volume zone crossed below zero",
                    1.0,
                ));
            }
        }
        self.prev_value = Some(value);
        Some(IndicatorOutput::new(value))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const VOLUME_ZONE_OSCILLATOR_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "volume_zone_oscillator",
    aliases: &["vzo"],
    description: "Volume zone oscillator (Khalil and Steckler, 2011): 100 times the average of volume signed by the direction of the close against the average volume, both exponential averages over period bars; a close that did not change counts as zero",
    params: &[ParamSpec::count("period", 14, 1, 10000).meaning("length of both exponential averages")],
    checks: &[],
    facts: IndicatorFacts::ratio()
        .bounded(-100.0, 100.0)
        .needs_real_volume(),
    verification: Verification::Verified,
    build: |p| Ok(Box::new(VolumeZoneOscillator::new(p.count("period")))),
};

/// Volume price confirmation indicator. Over a window of `n` bars `vwma = sum(close * volume) /
/// sum(volume)` (nothing for zero volume) and `sma` are the plain means. `vpc = vwma_long -
/// sma_long(close)`, computed from the deviations of each close from the current close so that the
/// difference keeps its precision at high price levels, `vpr = vwma_short / sma_short(close)`, `vm = sma_short(volume) /
/// sma_long(volume)`; **`value = vpc * vpr * vm`**, defined from bar `long`. extra `signal` is the mean of
/// the last `smoothing` defined values; a bar publishes once the signal exists. Alerts `bull` /
/// `bear` when `value` crosses above / below `signal`. First output: with bar number
/// `long + smoothing - 1`.
pub struct VolumePriceConfirmation {
    short: usize,
    long: usize,
    smoothing: usize,
    window: VecDeque<(f64, f64)>,
    values: VecDeque<f64>,
    prev: Option<(f64, f64)>,
    alerts: Vec<IndicatorAlert>,
}

impl VolumePriceConfirmation {
    pub fn new(short: usize, long: usize, smoothing: usize) -> Self {
        Self {
            short: short.max(1),
            long: long.max(2),
            smoothing: smoothing.max(1),
            window: VecDeque::new(),
            values: VecDeque::new(),
            prev: None,
            alerts: Vec::new(),
        }
    }

    /// `(vwma - sma of close, vwma, sma of close, sma of volume)` of the last `n` bars; `None` for
    /// zero volume. The difference is taken over the deviations from `reference`: subtracting two
    /// means near a high price level would cancel most of their digits.
    fn stats(&self, n: usize, reference: f64) -> Option<(f64, f64, f64, f64)> {
        let skip = self.window.len() - n;
        let (mut cv, mut v, mut c, mut dv, mut d) = (0.0, 0.0, 0.0, 0.0, 0.0);
        for (close, volume) in self.window.iter().skip(skip) {
            cv += close * volume;
            v += volume;
            c += close;
            dv += (close - reference) * volume;
            d += close - reference;
        }
        (v > 0.0).then(|| (dv / v - d / n as f64, cv / v, c / n as f64, v / n as f64))
    }
}

impl Indicator for VolumePriceConfirmation {
    fn name(&self) -> &str {
        "volume_price_confirmation"
    }

    fn warmup_period(&self) -> usize {
        self.long + self.smoothing - 1
    }

    fn reset(&mut self) {
        self.window.clear();
        self.values.clear();
        self.prev = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        self.window.push_back((bar.close, bar.volume));
        if self.window.len() > self.long {
            self.window.pop_front();
        }
        if self.window.len() == self.long {
            let vpci = self
                .stats(self.long, bar.close)
                .zip(self.stats(self.short, bar.close))
                .and_then(|((vpc, _, _, vol_l), (_, vwma_s, sma_s, vol_s))| {
                    let v = vpc * (vwma_s / sma_s) * (vol_s / vol_l);
                    v.is_finite().then_some(v)
                });
            if let Some(v) = vpci {
                self.values.push_back(v);
                if self.values.len() > self.smoothing {
                    self.values.pop_front();
                }
            }
            if self.values.len() == self.smoothing {
                if let Some(value) = vpci {
                    let signal = self.values.iter().sum::<f64>() / self.smoothing as f64;
                    if let Some((pv, ps)) = self.prev {
                        if crossed_over(pv, ps, value, signal) {
                            self.alerts.push(IndicatorAlert::new(
                                "bull",
                                "volume price confirmation crossed above its signal",
                                1.0,
                            ));
                        }
                        if crossed_under(pv, ps, value, signal) {
                            self.alerts.push(IndicatorAlert::new(
                                "bear",
                                "volume price confirmation crossed below its signal",
                                1.0,
                            ));
                        }
                    }
                    self.prev = Some((value, signal));
                    return Some(IndicatorOutput::with_extra(
                        value,
                        HashMap::from([("signal".to_string(), signal)]),
                    ));
                }
            }
        }
        self.prev = None;
        None
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const VOLUME_PRICE_CONFIRMATION_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "volume_price_confirmation",
    aliases: &["vpci"],
    description: "Volume price confirmation indicator (Dormeier, 2007): the gap between the volume weighted and the plain average close over the long window, times the same ratio over the short window, times the short over the long average volume; the signal is the mean of the last smoothing values. The price ratio divides by the mean close",
    params: &[
        ParamSpec::count("short", 5, 1, 10000).meaning("short window in bars"),
        ParamSpec::count("long", 20, 2, 10000).meaning("long window in bars"),
        ParamSpec::count("smoothing", 20, 1, 10000).meaning("values averaged for the signal line"),
    ],
    checks: &[Check::Less {
        lower: "short",
        upper: "long",
    }],
    facts: IndicatorFacts::unitless().needs_real_volume(),
    verification: Verification::Verified,
    build: |p| {
        Ok(Box::new(VolumePriceConfirmation::new(
            p.count("short"),
            p.count("long"),
            p.count("smoothing"),
        )))
    },
};

/// Market facilitation index `(high - low) / volume`, published for positive volume, with the
/// four states of Williams (1995) as `state`: the index up with volume up is `green`, both down
/// `fade`, index up with volume down `fake`, index down with volume up `squat`; an equality or a
/// missing previous bar is `neutral`. Alert `squat` on a squat bar. First output: with the first bar.
pub struct MarketFacilitation {
    prev: Option<(f64, f64)>,
    alerts: Vec<IndicatorAlert>,
}

impl MarketFacilitation {
    pub fn new() -> Self {
        Self {
            prev: None,
            alerts: Vec::new(),
        }
    }
}

impl Default for MarketFacilitation {
    fn default() -> Self {
        Self::new()
    }
}

impl Indicator for MarketFacilitation {
    fn name(&self) -> &str {
        "market_facilitation"
    }

    fn warmup_period(&self) -> usize {
        1
    }

    fn reset(&mut self) {
        self.prev = None;
        self.alerts.clear();
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        self.alerts.clear();
        if bar.volume.is_nan() || bar.volume <= 0.0 {
            self.prev = None;
            return None;
        }
        let value = (bar.high - bar.low) / bar.volume;
        let state = match self.prev {
            Some((pv, pvol)) => {
                if value > pv && bar.volume > pvol {
                    "green"
                } else if value < pv && bar.volume < pvol {
                    "fade"
                } else if value > pv && bar.volume < pvol {
                    "fake"
                } else if value < pv && bar.volume > pvol {
                    "squat"
                } else {
                    "neutral"
                }
            }
            None => "neutral",
        };
        self.prev = Some((value, bar.volume));
        if state == "squat" {
            self.alerts.push(IndicatorAlert::new(
                "squat",
                "index down on rising volume",
                1.0,
            ));
        }
        Some(IndicatorOutput::new(value).with_state(state))
    }

    fn alerts(&self) -> Vec<IndicatorAlert> {
        self.alerts.clone()
    }
}

pub const MARKET_FACILITATION_DEFINITION: IndicatorDefinition = IndicatorDefinition {
    name: "market_facilitation",
    aliases: &[],
    description: "Market facilitation index (Williams, 1995): the bar's range per unit of volume, with the state green, fade, fake or squat from the change of the index and of the volume against the previous bar; neutral where either is unchanged",
    params: &[],
    checks: &[],
    facts: IndicatorFacts::unitless()
        .non_negative()
        .needs_real_volume()
        .range_dependent(),
    verification: Verification::Verified,
    build: |_| Ok(Box::new(MarketFacilitation::new())),
};

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(t: i64, h: f64, l: f64, c: f64, v: f64) -> Bar {
        Bar::new(t, c, h, l, c, v)
    }

    fn close_bar(t: i64, c: f64, v: f64) -> Bar {
        bar(t, c, c, c, v)
    }

    fn alert_kinds(i: &dyn Indicator) -> Vec<String> {
        i.alerts().into_iter().map(|a| a.kind).collect()
    }

    #[test]
    fn vzo_matches_the_hand_computed_averages() {
        // period 1: alpha 1, so the averages equal the latest inputs.
        let mut vzo = VolumeZoneOscillator::new(1);
        assert!(vzo.on_bar(&close_bar(0, 10.0, 100.0)).is_none());
        let v = vzo.on_bar(&close_bar(1, 11.0, 100.0)).unwrap().value;
        assert!((v - 100.0).abs() < 1e-12);
        let v = vzo.on_bar(&close_bar(2, 10.0, 50.0)).unwrap().value;
        assert!((v + 100.0).abs() < 1e-12);
        assert_eq!(alert_kinds(&vzo), ["bear"]);
        let v = vzo.on_bar(&close_bar(3, 10.0, 50.0)).unwrap().value;
        assert_eq!(v, 0.0, "no change counts as zero");

        // period 3: alpha 0.5, seeded on bar 1 with R = 100 and V = 100.
        let mut vzo = VolumeZoneOscillator::new(3);
        vzo.on_bar(&close_bar(0, 10.0, 999.0));
        let first = vzo.on_bar(&close_bar(1, 11.0, 100.0)).unwrap().value;
        assert!((first - 100.0).abs() < 1e-12);
        // VP = 0.5 * (-50) + 0.5 * 100 = 25, TV = 0.5 * 50 + 0.5 * 100 = 75.
        let second = vzo.on_bar(&close_bar(2, 10.0, 50.0)).unwrap().value;
        assert!((second - 100.0 * 25.0 / 75.0).abs() < 1e-12);
    }

    #[test]
    fn vzo_publishes_nothing_without_volume() {
        let mut vzo = VolumeZoneOscillator::new(3);
        vzo.on_bar(&close_bar(0, 10.0, 0.0));
        assert!(vzo.on_bar(&close_bar(1, 11.0, 0.0)).is_none());
        assert!(vzo.on_bar(&close_bar(2, 12.0, 10.0)).is_some());
    }

    #[test]
    fn vpci_matches_the_hand_computed_windows() {
        // short 1, long 2, smoothing 2. Closes 10, 12, 11 with volumes 100, 300, 200.
        let mut vpci = VolumePriceConfirmation::new(1, 2, 2);
        assert!(vpci.on_bar(&close_bar(0, 10.0, 100.0)).is_none());
        // Bar 1: vwma_l = (1000 + 3600) / 400 = 11.5, sma_l = 11, vpc = 0.5; vpr = 1;
        // vm = 300 / 200 = 1.5, so 0.75. One value only: no signal yet.
        assert!(vpci.on_bar(&close_bar(1, 12.0, 300.0)).is_none());
        // Bar 2: vwma_l = (3600 + 2200) / 500 = 11.6, sma_l = 11.5, vpc = 0.1; vpr = 1;
        // vm = 200 / 250 = 0.8, so 0.08. Signal = (0.75 + 0.08) / 2.
        let out = vpci.on_bar(&close_bar(2, 11.0, 200.0)).unwrap();
        assert!((out.value - 0.08).abs() < 1e-12);
        assert!((out.extra["signal"] - 0.415).abs() < 1e-12);
        assert_eq!(alert_kinds(&vpci), Vec::<String>::new());
        assert_eq!(vpci.warmup_period(), 3);
    }

    #[test]
    fn vpci_alerts_on_a_cross_of_the_signal() {
        let mut vpci = VolumePriceConfirmation::new(1, 2, 2);
        let mut kinds = Vec::new();
        for (i, (c, v)) in [
            (10.0, 100.0),
            (12.0, 300.0),
            (11.0, 200.0),
            (13.0, 400.0),
            (12.0, 100.0),
        ]
        .iter()
        .enumerate()
        {
            vpci.on_bar(&close_bar(i as i64, *c, *v));
            kinds.push(alert_kinds(&vpci));
        }
        // Bar 2: value 0.08 under signal 0.415. Bar 3: vwma_l = (2200 + 5200) / 600 = 12.3333,
        // sma_l = 12, vpc = 1/3; vpr = 1; vm = 400 / 300; value = 0.4444 against signal 0.2622.
        assert_eq!(kinds[3], ["bull"]);
        assert_eq!(kinds[2], Vec::<String>::new());
    }

    #[test]
    fn market_facilitation_names_the_four_states() {
        let mut mf = MarketFacilitation::new();
        let mut run = |h: f64, l: f64, v: f64| {
            let out = mf.on_bar(&bar(0, h, l, l, v)).unwrap();
            (out.value, out.state.unwrap(), mf.alerts().len())
        };
        assert_eq!(run(11.0, 10.0, 100.0), (0.01, "neutral".to_string(), 0));
        // Index 0.02 (up) on volume 50 (down): fake.
        assert_eq!(run(11.0, 10.0, 50.0).1, "fake");
        // Index 0.04 (up) on volume 60 (up): green.
        assert_eq!(run(12.4, 10.0, 60.0).1, "green");
        // Index 0.02 (down) on volume 100 (up): squat, with its alert.
        let (_, state, alerts) = run(12.0, 10.0, 100.0);
        assert_eq!((state.as_str(), alerts), ("squat", 1));
        // Index 0.0125 (down) on volume 80 (down): fade.
        assert_eq!(run(11.0, 10.0, 80.0).1, "fade");
        // Equal volume: neutral.
        assert_eq!(run(12.0, 10.0, 80.0).1, "neutral");
    }

    #[test]
    fn market_facilitation_skips_zero_volume_and_restarts_the_state() {
        let mut mf = MarketFacilitation::new();
        mf.on_bar(&bar(0, 11.0, 10.0, 10.0, 100.0));
        assert!(mf.on_bar(&bar(1, 11.0, 10.0, 10.0, 0.0)).is_none());
        let out = mf.on_bar(&bar(2, 12.0, 10.0, 10.0, 200.0)).unwrap();
        assert_eq!(out.state.as_deref(), Some("neutral"));
    }

    fn vfi_bars(count: usize, volume: impl Fn(usize) -> f64) -> Vec<Bar> {
        // Typical price rises by 1 each bar, high = low = close.
        (0..count)
            .map(|i| close_bar(i as i64, 100.0 + i as f64, volume(i)))
            .collect()
    }

    #[test]
    fn vfi_first_output_comes_at_the_documented_bar() {
        // period 2: start = 30, warmup = 32, bar index 31 is the first.
        let mut vfi = VolumeFlowIndicator::new(2, 0.0, 2.5, 3);
        assert_eq!(vfi.warmup_period(), 32);
        let bars = vfi_bars(40, |_| 10.0);
        let first = bars.iter().position(|b| vfi.on_bar(b).is_some()).unwrap();
        assert_eq!(first, 31);
    }

    #[test]
    fn vfi_counts_rising_bars_at_the_capped_volume() {
        // coef 0: cutoff 0, so every rise counts positive. Volume 10 each bar: vave 10,
        // vc = 10, vcp = 10, raw = (2 * 10) / 10 = 2, the EMA seeded with it.
        let mut vfi = VolumeFlowIndicator::new(2, 0.0, 2.5, 3);
        let mut last = None;
        for b in vfi_bars(33, |_| 10.0) {
            last = vfi.on_bar(&b);
        }
        let out = last.unwrap();
        assert!((out.value - 2.0).abs() < 1e-12);
        assert!((out.extra["raw"] - 2.0).abs() < 1e-12);

        // A volume spike on the last bar is capped at 2.5 * vave = 25: raw = (10 + 25) / 10.
        let mut vfi = VolumeFlowIndicator::new(2, 0.0, 2.5, 1);
        let mut last = None;
        for b in vfi_bars(33, |i| if i == 32 { 1000.0 } else { 10.0 }) {
            last = vfi.on_bar(&b);
        }
        assert!((last.unwrap().extra["raw"] - 3.5).abs() < 1e-12);
    }

    #[test]
    fn vfi_uses_the_average_before_the_bar_and_treats_a_small_move_as_zero() {
        // Closes alternate by 2 around 100: vinter is about 0.02, so the cutoff is about
        // coef * 2, far above the move of 2 for coef 10 and below it for coef 0.1.
        let swing = |i: usize| {
            close_bar(
                i as i64,
                if i.is_multiple_of(2) { 100.0 } else { 102.0 },
                10.0,
            )
        };
        let mut vfi = VolumeFlowIndicator::new(3, 10.0, 2.5, 1);
        let mut last = None;
        for i in 0..34 {
            last = vfi.on_bar(&swing(i));
        }
        assert_eq!(last.unwrap().extra["raw"], 0.0);
        let mut vfi = VolumeFlowIndicator::new(3, 0.1, 2.5, 1);
        let mut last = None;
        for i in 0..34 {
            last = vfi.on_bar(&swing(i));
        }
        // The last three bars move +2, -2, +2, all beyond the cutoff: flows +10 - 10 + 10 = 10
        // over the average volume 10.
        assert!((last.unwrap().extra["raw"] - 1.0).abs() < 1e-12);
    }

    #[test]
    fn vfi_publishes_nothing_while_the_average_volume_is_zero() {
        let mut vfi = VolumeFlowIndicator::new(2, 0.0, 2.5, 1);
        for b in vfi_bars(40, |_| 0.0) {
            assert!(vfi.on_bar(&b).is_none());
        }
    }

    #[test]
    fn vfi_alerts_when_the_value_crosses_zero() {
        // Rising prices, then falling: the value goes from positive to negative.
        let mut vfi = VolumeFlowIndicator::new(2, 0.0, 2.5, 1);
        let mut bears = 0;
        for i in 0..50 {
            let c = if i < 40 {
                100.0 + i as f64
            } else {
                140.0 - (i - 39) as f64 * 2.0
            };
            vfi.on_bar(&close_bar(i, c, 10.0));
            if alert_kinds(&vfi).contains(&"bear".to_string()) {
                bears += 1;
            }
        }
        assert_eq!(bears, 1);
    }
}
