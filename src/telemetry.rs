//! Market telemetry: one snapshot of six descriptive read-outs over a bar series, built only from
//! catalog entries that already exist (no second implementation of any indicator).
//!
//! | dimension | question | built from |
//! |---|---|---|
//! | [`ThermometerTelemetry`](crate::telemetry::ThermometerTelemetry) | ordered path or noise? | `fractal_dimension`, `choppiness`, `kestrel_swing_structure` |
//! | [`BarometerTelemetry`](crate::telemetry::BarometerTelemetry) | how tense is the range? | `candle_classification`, `rogers_satchell`, `atr`, `narrow_range`, `kestrel_compression_zones` |
//! | [`SeismometerTelemetry`](crate::telemetry::SeismometerTelemetry) | forced selling or buying? | `vix_fix` (lows) and its inverse (tops) |
//! | [`EkgTelemetry`](crate::telemetry::EkgTelemetry) | stretched or fading? | `ema`, `atr`, `pivot_divergence` (RSI) |
//! | [`LandkarteTelemetry`](crate::telemetry::LandkarteTelemetry) | where is unfinished business? | `liquidity_fvg` zones, `volume_profile` or `tpo_profile` |
//! | [`KostengitterTelemetry`](crate::telemetry::KostengitterTelemetry) | how much of a bar does the spread eat? | caller's spread, `atr` |
//!
//! Every status is a threshold crossing on these measurements. The thresholds are parameters
//! ([`TelemetryConfig`](crate::telemetry::TelemetryConfig)); the defaults are the usual textbook marks (fractal dimension 1.5 for a
//! random walk, choppiness 61.8/38.2, NR4/NR7, a two-sigma Vix Fix band). The boolean
//! "command" fields (`trend_model_active`, `long_veto_active`, `tighten_stops_command`, …) name
//! what a caller *could* do with the crossing; this module makes no claim that doing so pays.
//!
//! **Causal and deterministic.** The snapshot describes the last bar of `bars` and reads nothing
//! after it; every indicator is fed bar by bar from the first bar on. The same input gives the same
//! output, bit for bit.
//!
//! **Long and short alike.** Every one-sided read-out has its mirror: the panic veto on longs
//! (Vix Fix at lows) next to the blow-off veto on shorts (inverse Vix Fix at tops), the safe stop
//! below support next to the one above resistance, the chase warning above and below the average.
//!
//! **Missing data stays missing.** A value that the series cannot carry (too short, no volume for a
//! volume profile, no spread) is `None` and its status is `UNKNOWN`; nothing is substituted.

use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::indicator::liquidity_fvg::{FvgZone, LiquidityFvgEngine};
use crate::indicator::range_volatility::RogersSatchellVolatilityEngine;
use crate::indicator::registry::build_checked;
use crate::indicator::{Indicator, IndicatorOutput};
use crate::model::Bar;

/// Thresholds and lengths of the telemetry. [`Default`] holds the documented textbook marks.
#[derive(Debug, Clone, PartialEq)]
pub struct TelemetryConfig {
    /// Fractal dimension below which the path counts as ordered (`LAMINAR`).
    pub laminar_fdi_max: f64,
    /// Fractal dimension above which the path counts as noise, together with `turbulent_chop_min`.
    pub turbulent_fdi_min: f64,
    /// Choppiness above which the bars overlap like noise (61.8, the upper Fibonacci mark).
    pub turbulent_chop_min: f64,
    /// Bars over which the wick share of the range is averaged.
    pub wick_window: usize,
    /// Mean wick share of the range above which the barometer reads `TENSION_ELEVATED`.
    pub tension_wick_share_min: f64,
    /// Consecutive bars inside a compression zone from which the barometer reads `COMPRESSED_COIL`
    /// on its own (a narrow-range bar inside any zone reads so too).
    pub coil_min_bars: usize,
    /// Trailing distance in ATR suggested in normal conditions.
    pub trailing_atr_normal: f64,
    /// Trailing distance in ATR suggested while the barometer reads `TENSION_ELEVATED`.
    pub trailing_atr_tension: f64,
    /// Window of the percentile ranks (ATR, Vix Fix).
    pub percentile_window: usize,
    /// Bars after a panic (or blow-off) bar during which the phase reads as recovery.
    pub recovery_bars: usize,
    /// EMA length of the overextension distance.
    pub ema_len: usize,
    /// Distance from the EMA in ATR beyond which a chase warning is raised.
    pub overextension_atr: f64,
    /// Bars back in which a confirmed RSI divergence still counts.
    pub divergence_window: usize,
    /// Safety distance in ATR behind a zone for the safe stop levels.
    pub stop_buffer_atr: f64,
    /// Spread load (spread ÷ ATR) below which the grade is `GREEN`.
    pub green_spread_atr_max: f64,
    /// Spread load up to which the grade is `YELLOW`; above it `RED`.
    pub yellow_spread_atr_max: f64,
}

impl Default for TelemetryConfig {
    fn default() -> Self {
        Self {
            laminar_fdi_max: 1.30,
            turbulent_fdi_min: 1.48,
            turbulent_chop_min: 61.8,
            wick_window: 10,
            tension_wick_share_min: 0.65,
            coil_min_bars: 12,
            trailing_atr_normal: 2.0,
            trailing_atr_tension: 3.5,
            percentile_window: 100,
            recovery_bars: 5,
            ema_len: 50,
            overextension_atr: 2.5,
            divergence_window: 5,
            stop_buffer_atr: 0.25,
            green_spread_atr_max: 0.03,
            yellow_spread_atr_max: 0.08,
        }
    }
}

/// Order of the price path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "SCREAMING_SNAKE_CASE")
)]
pub enum ThermometerStatus {
    /// Fractal dimension below `laminar_fdi_max` and the swing trend not weakening.
    Laminar,
    /// Neither ordered nor noise.
    Transitional,
    /// Fractal dimension above `turbulent_fdi_min` and choppiness above `turbulent_chop_min`.
    TurbulentChaos,
    /// Series too short for the fractal dimension or the choppiness.
    Unknown,
}

/// Tension of the bar ranges.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "SCREAMING_SNAKE_CASE")
)]
pub enum BarometerStatus {
    /// No compression, wicks within the usual share.
    Dormant,
    /// A compression zone of at least `coil_min_bars`, or a narrow-range bar inside a zone.
    CompressedCoil,
    /// Mean wick share above `tension_wick_share_min`.
    TensionElevated,
    /// Series too short for the wick share.
    Unknown,
}

/// Phase of a Vix Fix spike (at lows) or of its inverse (at tops).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "SCREAMING_SNAKE_CASE")
)]
pub enum WashoutPhase {
    /// No spike on this bar nor in the last `recovery_bars`.
    None,
    /// The value is at or above its band on this bar.
    W1Panic,
    /// A spike within the last `recovery_bars`, none on this bar.
    W2Recovery,
    /// Series too short for the band.
    Unknown,
}

/// Pulse of the move.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "SCREAMING_SNAKE_CASE")
)]
pub enum EkgStatus {
    /// Neither stretched nor diverging.
    HealthyPulse,
    /// Close beyond `overextension_atr` from the EMA.
    Overextended,
    /// A confirmed divergence against the side the close stands on.
    DivergenceExhaustion,
    /// Series too short for EMA and ATR.
    Unknown,
}

/// Spread load grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "SCREAMING_SNAKE_CASE")
)]
pub enum TradableGrade {
    Green,
    Yellow,
    Red,
    /// No spread given, or no ATR yet.
    Unknown,
}

/// Where the point of control came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ProfileSource {
    /// `volume_profile`: every bar of its window carries a positive volume.
    Volume,
    /// `tpo_profile`: time at price, needs no volume.
    Time,
}

/// Order versus noise of the path.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ThermometerTelemetry {
    /// `kestrel_swing_structure` trend quality: `1` strengthening, `0` stable, `-1` weakening.
    pub trend_quality: Option<f64>,
    /// `fractal_dimension` (Ehlers' halves, 30 bars): `1` straight line … `2` plane-filling.
    pub fractal_dimension: Option<f64>,
    /// `choppiness` (14 bars), `0..=100`.
    pub choppiness: Option<f64>,
    pub status: ThermometerStatus,
    /// `true` while the status is `LAMINAR` or `TRANSITIONAL`.
    pub trend_model_active: bool,
}

/// Tension of the ranges.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BarometerTelemetry {
    /// Mean share of the bar range outside the body over the last `wick_window` bars, `0..=1`
    /// (`1 − body_ratio` of `candle_classification`).
    pub wick_to_body_ratio: Option<f64>,
    /// Rogers-Satchell volatility (14 bars, per bar) in price units divided by the ATR.
    pub rogers_satchell_atr: Option<f64>,
    /// Percentile rank (`0..=100`) of the ATR in percent of the close within `percentile_window`.
    pub atr_percentile: Option<f64>,
    /// NR4 or NR7 on the last bar.
    pub narrow_range_active: bool,
    /// Consecutive bars up to the last one inside a `kestrel_compression_zones` zone.
    pub compression_bars: usize,
    pub status: BarometerStatus,
    /// `trailing_atr_tension` while `TENSION_ELEVATED`, else `trailing_atr_normal`.
    pub recommended_trailing_atr: f64,
}

/// Forced selling at lows, forced buying at tops.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SeismometerTelemetry {
    /// Percentile rank (`0..=100`) of the Vix Fix within `percentile_window`.
    pub vix_fix_percentile: Option<f64>,
    /// Phase of the Vix Fix against its band (panic at lows).
    pub washout_phase: WashoutPhase,
    /// `true` while `washout_phase` is `W1_PANIC`.
    pub long_veto_active: bool,
    /// Percentile rank of the inverse Vix Fix (the mirror at tops).
    pub blowoff_percentile: Option<f64>,
    /// Phase of the inverse Vix Fix against its band (blow-off at tops).
    pub blowoff_phase: WashoutPhase,
    /// `true` while `blowoff_phase` is `W1_PANIC`.
    pub short_veto_active: bool,
}

/// Stretch and fading momentum.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct EkgTelemetry {
    /// A confirmed RSI pivot divergence within `divergence_window` bars.
    pub divergence_detected: bool,
    /// `1` bullish, `-1` bearish, `0` none (the most recent one).
    pub divergence_direction: i8,
    /// `(close − EMA) / ATR`, signed: the distance from the average in ATR units.
    pub overextension_z: Option<f64>,
    /// `|overextension_z|` beyond `overextension_atr`.
    pub fomo_warning: bool,
    /// `1` stretched upwards (chasing longs), `-1` downwards (chasing shorts), `0` none.
    pub fomo_side: i8,
    /// A bearish divergence with the close above the EMA, or a bullish one below it.
    pub tighten_stops_command: bool,
    /// Positions concerned: `1` longs, `-1` shorts, `0` none.
    pub tighten_side: i8,
    pub status: EkgStatus,
}

/// Open gaps and the point of control as levels the price has not finished with.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct LandkarteTelemetry {
    /// Nearest edge above the close: lower edge of an open fair value gap, or the POC.
    pub nearest_magnet_above: Option<f64>,
    /// Nearest edge below the close: upper edge of an open fair value gap, or the POC.
    pub nearest_magnet_below: Option<f64>,
    pub poc_level: Option<f64>,
    pub value_area_high: Option<f64>,
    pub value_area_low: Option<f64>,
    pub poc_source: Option<ProfileSource>,
    /// Far side of the nearest zone below, minus `stop_buffer_atr` ATR.
    pub safe_stop_level_long: Option<f64>,
    /// Far side of the nearest zone above, plus `stop_buffer_atr` ATR.
    pub safe_stop_level_short: Option<f64>,
    pub open_gaps_above: usize,
    pub open_gaps_below: usize,
}

/// Spread against the bar size.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KostengitterTelemetry {
    /// Spread in basis points of the close.
    pub spread_bps: Option<f64>,
    /// Spread divided by the ATR.
    pub spread_atr_ratio: Option<f64>,
    pub tradable_grade: TradableGrade,
}

/// The six read-outs for the last bar of a series.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct MarketTelemetry {
    /// Timestamp of the last bar.
    pub ts: i64,
    pub close: f64,
    /// ATR (14, Wilder) in price units.
    pub bar_atr: Option<f64>,
    /// Bars the snapshot was computed from.
    pub bars: usize,
    pub thermometer: ThermometerTelemetry,
    pub barometer: BarometerTelemetry,
    pub seismometer: SeismometerTelemetry,
    pub ekg: EkgTelemetry,
    pub landkarte: LandkarteTelemetry,
    pub kostengitter: KostengitterTelemetry,
}

/// One indicator fed over the whole series: per bar its output (if any) and the alert kinds.
struct Run {
    outputs: Vec<Option<IndicatorOutput>>,
    alerts: Vec<Vec<String>>,
}

impl Run {
    fn of(name: &str, params: &[(&str, f64)], bars: &[Bar]) -> Self {
        let map: HashMap<String, f64> = params.iter().map(|(k, v)| (k.to_string(), *v)).collect();
        let indicator = build_checked(name, &map)
            .unwrap_or_else(|e| panic!("telemetry builds catalog entry {name}: {e}"));
        Self::feed(indicator, bars)
    }

    fn feed(mut indicator: Box<dyn Indicator>, bars: &[Bar]) -> Self {
        let mut outputs = Vec::with_capacity(bars.len());
        let mut alerts = Vec::with_capacity(bars.len());
        for bar in bars {
            outputs.push(indicator.on_bar(bar));
            alerts.push(indicator.alerts().into_iter().map(|a| a.kind).collect());
        }
        Self { outputs, alerts }
    }

    fn last(&self) -> Option<&IndicatorOutput> {
        self.outputs.last().and_then(Option::as_ref)
    }

    fn last_value(&self) -> Option<f64> {
        self.last().map(|o| o.value).filter(|v| v.is_finite())
    }

    fn last_extra(&self, key: &str) -> Option<f64> {
        self.last()
            .and_then(|o| o.extra.get(key).copied())
            .filter(|v| v.is_finite())
    }

    /// The finite `pick` values of the last `n` bars that have an output.
    fn tail(&self, n: usize, pick: impl Fn(&IndicatorOutput) -> Option<f64>) -> Vec<f64> {
        let start = self.outputs.len().saturating_sub(n);
        self.outputs[start..]
            .iter()
            .filter_map(|o| o.as_ref().and_then(&pick))
            .filter(|v| v.is_finite())
            .collect()
    }
}

/// Percentile rank of the last of `values` within all of them, `0..=100` (share of values at or
/// below it, the last one included).
fn percentile_rank(values: &[f64]) -> Option<f64> {
    let last = *values.last()?;
    let at_or_below = values.iter().filter(|v| **v <= last).count();
    Some(100.0 * at_or_below as f64 / values.len() as f64)
}

/// Phase of a value against its band: spike on the last bar, within `recovery` bars before, or none.
fn phase(spikes: &[Option<bool>], recovery: usize) -> WashoutPhase {
    match spikes.last() {
        None | Some(None) => WashoutPhase::Unknown,
        Some(Some(true)) => WashoutPhase::W1Panic,
        Some(Some(false)) => {
            let n = spikes.len();
            let start = n.saturating_sub(recovery + 1);
            if spikes[start..n - 1].contains(&Some(true)) {
                WashoutPhase::W2Recovery
            } else {
                WashoutPhase::None
            }
        }
    }
}

/// Telemetry of the last bar with the default thresholds ([`TelemetryConfig::default`]).
///
/// `spread_points` is the quoted spread, `value_per_point` the price units one quoted point stands
/// for (`1.0` when the spread is quoted in price units). A non-finite or negative spread counts as
/// unknown.
pub fn compute_market_telemetry(
    bars: &[Bar],
    spread_points: f64,
    value_per_point: f64,
) -> MarketTelemetry {
    compute_market_telemetry_with(
        bars,
        spread_points,
        value_per_point,
        &TelemetryConfig::default(),
    )
}

/// [`compute_market_telemetry`] with explicit thresholds.
///
/// # Panics
/// On an empty `bars`.
pub fn compute_market_telemetry_with(
    bars: &[Bar],
    spread_points: f64,
    value_per_point: f64,
    cfg: &TelemetryConfig,
) -> MarketTelemetry {
    let last = bars.last().expect("telemetry needs at least one bar");
    let close = last.close;

    let atr = Run::of("atr", &[], bars);
    let bar_atr = atr.last_extra("raw").filter(|a| *a > 0.0);

    // 1 · Thermometer
    let fdi = Run::of("fractal_dimension", &[], bars).last_value();
    let chop = Run::of("choppiness", &[], bars).last_value();
    let trend_quality = Run::of("kestrel_swing_structure", &[], bars).last_extra("trend_quality");
    let thermo_status = match (fdi, chop) {
        (Some(d), Some(c)) if d > cfg.turbulent_fdi_min && c > cfg.turbulent_chop_min => {
            ThermometerStatus::TurbulentChaos
        }
        (Some(d), Some(_)) if d < cfg.laminar_fdi_max && trend_quality.is_none_or(|t| t >= 0.0) => {
            ThermometerStatus::Laminar
        }
        (Some(_), Some(_)) => ThermometerStatus::Transitional,
        _ => ThermometerStatus::Unknown,
    };
    let thermometer = ThermometerTelemetry {
        trend_quality,
        fractal_dimension: fdi,
        choppiness: chop,
        status: thermo_status,
        trend_model_active: matches!(
            thermo_status,
            ThermometerStatus::Laminar | ThermometerStatus::Transitional
        ),
    };

    // 2 · Barometer
    let candles = Run::of("candle_classification", &[], bars);
    let wick_shares = candles.tail(cfg.wick_window, |o| {
        o.extra.get("body_ratio").map(|b| 1.0 - b)
    });
    let wick_to_body_ratio = (wick_shares.len() == cfg.wick_window.min(bars.len())
        && !wick_shares.is_empty())
    .then(|| wick_shares.iter().sum::<f64>() / wick_shares.len() as f64);
    let rs = Run::feed(Box::new(RogersSatchellVolatilityEngine::new(14, 1)), bars).last_value();
    let rogers_satchell_atr = rs.zip(bar_atr).map(|(rs, a)| rs / 100.0 * close / a);
    let atr_pct = atr.tail(cfg.percentile_window, |o| Some(o.value));
    let atr_percentile = if atr.last().is_some() {
        percentile_rank(&atr_pct)
    } else {
        None
    };
    let nr_active = [4.0, 7.0]
        .iter()
        .any(|p| Run::of("narrow_range", &[("period", *p)], bars).last_value() == Some(1.0));
    let zones = Run::of("kestrel_compression_zones", &[], bars);
    let compression_bars = zones
        .outputs
        .iter()
        .rev()
        .take_while(|o| o.as_ref().and_then(|o| o.extra.get("in_zone")).copied() == Some(1.0))
        .count();
    let baro_status =
        if compression_bars >= cfg.coil_min_bars || (nr_active && compression_bars > 0) {
            BarometerStatus::CompressedCoil
        } else {
            match wick_to_body_ratio {
                None => BarometerStatus::Unknown,
                Some(w) if w > cfg.tension_wick_share_min => BarometerStatus::TensionElevated,
                Some(_) => BarometerStatus::Dormant,
            }
        };
    let barometer = BarometerTelemetry {
        wick_to_body_ratio,
        rogers_satchell_atr,
        atr_percentile,
        narrow_range_active: nr_active,
        compression_bars,
        status: baro_status,
        recommended_trailing_atr: if baro_status == BarometerStatus::TensionElevated {
            cfg.trailing_atr_tension
        } else {
            cfg.trailing_atr_normal
        },
    };

    // 3 · Seismometer
    let vix = Run::of("vix_fix", &[], bars);
    let spikes = |value: &dyn Fn(&IndicatorOutput) -> Option<(f64, f64)>| -> Vec<Option<bool>> {
        vix.outputs
            .iter()
            .map(|o| o.as_ref().and_then(value).map(|(v, band)| v >= band))
            .collect()
    };
    let low = |o: &IndicatorOutput| o.secondary.map(|band| (o.value, band));
    let top = |o: &IndicatorOutput| Some((*o.extra.get("inverse")?, *o.extra.get("inverse_band")?));
    let mut low_spikes = spikes(&low);
    let mut top_spikes = spikes(&top);
    // A wide bar can lift both at once (its low far under the highest close, its high far over the
    // lowest close — the new close among them). Such a bar counts for the side whose value
    // exceeds its band by the larger factor only; the rule reads the same on the mirrored series.
    for (i, o) in vix.outputs.iter().enumerate() {
        if let (Some(true), Some(true), Some(o)) = (low_spikes[i], top_spikes[i], o) {
            let excess = |p: Option<(f64, f64)>| {
                p.map_or(0.0, |(v, b)| if b > 0.0 { v / b } else { f64::INFINITY })
            };
            if excess(low(o)) >= excess(top(o)) {
                top_spikes[i] = Some(false);
            } else {
                low_spikes[i] = Some(false);
            }
        }
    }
    let vix_ready = vix.last().is_some();
    let washout_phase = phase(&low_spikes, cfg.recovery_bars);
    let blowoff_phase = phase(&top_spikes, cfg.recovery_bars);
    let seismometer = SeismometerTelemetry {
        vix_fix_percentile: if vix_ready {
            percentile_rank(&vix.tail(cfg.percentile_window, |o| Some(o.value)))
        } else {
            None
        },
        washout_phase,
        long_veto_active: washout_phase == WashoutPhase::W1Panic,
        blowoff_percentile: if vix_ready {
            percentile_rank(&vix.tail(cfg.percentile_window, |o| o.extra.get("inverse").copied()))
        } else {
            None
        },
        blowoff_phase,
        short_veto_active: blowoff_phase == WashoutPhase::W1Panic,
    };

    // 4 · EKG
    let ema = Run::of("ema", &[("period", cfg.ema_len as f64)], bars).last_value();
    let overextension_z = ema
        .zip(bar_atr)
        .filter(|_| bars.len() >= cfg.ema_len)
        .map(|(e, a)| (close - e) / a);
    let divergence = Run::of("pivot_divergence", &[], bars);
    let start = divergence
        .alerts
        .len()
        .saturating_sub(cfg.divergence_window);
    let divergence_direction = divergence.alerts[start..]
        .iter()
        .flatten()
        .filter_map(|kind| match kind.as_str() {
            "bull_divergence" => Some(1i8),
            "bear_divergence" => Some(-1i8),
            _ => None,
        })
        .next_back()
        .unwrap_or(0);
    let fomo_side = match overextension_z {
        Some(z) if z > cfg.overextension_atr => 1,
        Some(z) if z < -cfg.overextension_atr => -1,
        _ => 0,
    };
    let tighten_side = match (divergence_direction, overextension_z) {
        (-1, Some(z)) if z > 0.0 => 1,
        (1, Some(z)) if z < 0.0 => -1,
        _ => 0,
    };
    let ekg = EkgTelemetry {
        divergence_detected: divergence_direction != 0,
        divergence_direction,
        overextension_z,
        fomo_warning: fomo_side != 0,
        fomo_side,
        tighten_stops_command: tighten_side != 0,
        tighten_side,
        status: if overextension_z.is_none() {
            EkgStatus::Unknown
        } else if tighten_side != 0 {
            EkgStatus::DivergenceExhaustion
        } else if fomo_side != 0 {
            EkgStatus::Overextended
        } else {
            EkgStatus::HealthyPulse
        },
    };

    // 5 · Landkarte
    let mut fvg = LiquidityFvgEngine::new(20);
    for bar in bars {
        fvg.on_bar(bar);
    }
    let landkarte = landkarte(bars, close, fvg.zones(), bar_atr, cfg);

    // 6 · Kostengitter
    let spread = spread_points * value_per_point;
    let spread = (spread.is_finite() && spread >= 0.0).then_some(spread);
    let spread_atr_ratio = spread.zip(bar_atr).map(|(s, a)| s / a);
    let kostengitter = KostengitterTelemetry {
        spread_bps: spread.filter(|_| close > 0.0).map(|s| s / close * 1e4),
        spread_atr_ratio,
        tradable_grade: match spread_atr_ratio {
            None => TradableGrade::Unknown,
            Some(r) if r < cfg.green_spread_atr_max => TradableGrade::Green,
            Some(r) if r <= cfg.yellow_spread_atr_max => TradableGrade::Yellow,
            Some(_) => TradableGrade::Red,
        },
    };

    MarketTelemetry {
        ts: last.timestamp,
        close,
        bar_atr,
        bars: bars.len(),
        thermometer,
        barometer,
        seismometer,
        ekg,
        landkarte,
        kostengitter,
    }
}

/// A level with the far side of the zone it belongs to.
struct Level {
    near: f64,
    far: f64,
}

fn landkarte(
    bars: &[Bar],
    close: f64,
    zones: &[FvgZone],
    atr: Option<f64>,
    cfg: &TelemetryConfig,
) -> LandkarteTelemetry {
    // Volume profile only where its whole window carries volume; otherwise time at price.
    const VP_LOOKBACK: usize = 70;
    let window = &bars[bars.len().saturating_sub(VP_LOOKBACK)..];
    let has_volume = window.len() == VP_LOOKBACK
        && window
            .iter()
            .all(|b| b.volume.is_finite() && b.volume > 0.0);
    let (profile, source) = if has_volume {
        (Run::of("volume_profile", &[], bars), ProfileSource::Volume)
    } else {
        (Run::of("tpo_profile", &[], bars), ProfileSource::Time)
    };
    let poc = profile.last_value();
    let (vah, val) = if poc.is_some() {
        (profile.last_extra("vah"), profile.last_extra("val"))
    } else {
        (None, None)
    };

    let mut above: Vec<Level> = zones
        .iter()
        .filter(|z| z.bottom > close)
        .map(|z| Level {
            near: z.bottom,
            far: z.top,
        })
        .collect();
    let mut below: Vec<Level> = zones
        .iter()
        .filter(|z| z.top < close)
        .map(|z| Level {
            near: z.top,
            far: z.bottom,
        })
        .collect();
    let (open_gaps_above, open_gaps_below) = (above.len(), below.len());
    if let Some(p) = poc {
        if p > close {
            above.push(Level { near: p, far: p });
        } else if p < close {
            below.push(Level { near: p, far: p });
        }
    }
    let nearest_above = above.iter().min_by(|a, b| a.near.total_cmp(&b.near));
    let nearest_below = below.iter().max_by(|a, b| a.near.total_cmp(&b.near));
    LandkarteTelemetry {
        nearest_magnet_above: nearest_above.map(|l| l.near),
        nearest_magnet_below: nearest_below.map(|l| l.near),
        poc_level: poc,
        value_area_high: vah,
        value_area_low: val,
        poc_source: poc.map(|_| source),
        safe_stop_level_long: nearest_below
            .zip(atr)
            .map(|(l, a)| l.far - cfg.stop_buffer_atr * a),
        safe_stop_level_short: nearest_above
            .zip(atr)
            .map(|(l, a)| l.far + cfg.stop_buffer_atr * a),
        open_gaps_above,
        open_gaps_below,
    }
}
