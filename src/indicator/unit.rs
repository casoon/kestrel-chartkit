//! What unit an indicator's primary output is quoted in.
//!
//! A chart has to decide, per indicator, whether its result belongs on the price axis or in a
//! pane of its own. That is a property of the indicator — not of the chart — but until now the
//! catalog did not say it, so every consumer rebuilt the knowledge by hand and every new
//! indicator silently inherited whatever the consumer's fallback was.
//!
//! The lookup is a standalone function rather than a field on
//! [`crate::indicator::registry::IndicatorCatalogEntry`], for the same reason
//! [`crate::applicability::data_requirements`] is: not every indicator is a catalog entry.
//! `elliott`, `swing_structure` and `relative_strength` are kept out of the single-bar registry
//! by their call signatures, and a function can grow to cover them without a signature change on
//! those engines — a struct field cannot. It also keeps the addition non-breaking:
//! `IndicatorCatalogEntry` has public fields and no `#[non_exhaustive]`, so a fourth field would
//! break every caller that builds one by struct literal.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The unit an indicator's primary output ([`crate::indicator::IndicatorOutput::value`]) is
/// quoted in.
///
/// The distinction that carries weight is [`IndicatorUnit::Price`] versus everything else: a
/// price-level result can be drawn on the price axis of the chart it was computed from, and
/// anything else distorts that axis and needs a scale of its own.
///
/// `Price` means a price *level* — a number comparable to the bars' open/high/low/close. A
/// quantity that merely has price *dimension* is not `Price`: MACD, `awesome_oscillator`, `dpo`
/// and `true_range` are distances and differences, they share no zero with the price axis, and
/// they are classified [`IndicatorUnit::Unitless`].
///
/// Marked `#[non_exhaustive]`: a finer unit (a price *delta*, say) can be added without a major
/// version. Match with a `_` arm.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[non_exhaustive]
pub enum IndicatorUnit {
    /// A price level, on the same scale as the input bars — moving averages, bands, stops,
    /// profile levels, pivots. Belongs on the price axis.
    Price,
    /// A dimensionless, bounded or normalized number — percentages, 0–100 oscillators, z-scores,
    /// ratios of two prices or two ranges.
    Ratio,
    /// The instrument's volume units, including cumulative and signed sums of them.
    Volume,
    /// No scale shared with the chart's price or volume axis: price differences and spreads,
    /// compound price×volume quantities, index levels with an arbitrary base, state codes and
    /// counts.
    Unitless,
}

/// The unit of `name`'s primary output (case-insensitive, matching the `.to_lowercase()`
/// convention of [`crate::indicator::registry::build_checked`]).
///
/// Unknown names return [`IndicatorUnit::Unitless`] — deliberately the conservative answer: a
/// result wrongly placed in its own pane costs a pane, a non-price result wrongly drawn on the
/// price axis destroys the price scale.
///
/// Only the primary output is described. Several indicators emit price levels as *secondary*
/// series while their `value` is something else — `ichimoku` puts the cloud width in `value` and
/// the prices in `tenkan`/`kijun`/`senkou_a`/`senkou_b`, `atr` reports a percentage in `value`
/// and the price distance in `extra["raw"]`, `trend_relationship` reports a spread next to two
/// price-level averages. `ichimoku` is `Price` because its subject is those levels; `atr` and
/// `trend_relationship` are not.
pub fn output_unit(name: &str) -> IndicatorUnit {
    let lowered = name.to_lowercase();
    OUTPUT_UNITS
        .iter()
        .find(|(candidate, _)| *candidate == lowered)
        .map(|(_, unit)| *unit)
        .unwrap_or(IndicatorUnit::Unitless)
}

/// Every canonical indicator name with the unit of its primary output.
///
/// Kept as an explicit table rather than a `match` with a catch-all so that
/// `every_canonical_indicator_declares_its_unit` can prove the two lists agree in both
/// directions. A new indicator therefore fails the test until someone decides its unit, instead
/// of silently defaulting — which is exactly how nine price-unit indicators added between 0.2.0
/// and 0.12.0 ended up misplaced in consumers.
const OUTPUT_UNITS: &[(&str, IndicatorUnit)] = &[
    ("acc_dist", IndicatorUnit::Volume),
    ("adx", IndicatorUnit::Ratio),
    ("alligator", IndicatorUnit::Price),
    ("anchored_vwap", IndicatorUnit::Price),
    ("aroon", IndicatorUnit::Ratio),
    ("atr", IndicatorUnit::Ratio),
    ("awesome_oscillator", IndicatorUnit::Unitless),
    ("bbtrend", IndicatorUnit::Ratio),
    ("bollinger", IndicatorUnit::Price),
    ("bop", IndicatorUnit::Ratio),
    ("bos_choch", IndicatorUnit::Unitless),
    ("buy_sell_pressure", IndicatorUnit::Unitless),
    ("candle_story", IndicatorUnit::Unitless),
    ("cci", IndicatorUnit::Ratio),
    ("chaikin_oscillator", IndicatorUnit::Volume),
    ("chande_kroll", IndicatorUnit::Price),
    ("chandelier_exit", IndicatorUnit::Price),
    ("chandelier_flip_radar", IndicatorUnit::Price),
    ("choppiness", IndicatorUnit::Ratio),
    ("cmf", IndicatorUnit::Ratio),
    ("cmo", IndicatorUnit::Ratio),
    ("connors_rsi", IndicatorUnit::Ratio),
    ("coppock", IndicatorUnit::Ratio),
    ("cvd", IndicatorUnit::Volume),
    ("dema", IndicatorUnit::Price),
    ("dmi", IndicatorUnit::Ratio),
    ("donchian", IndicatorUnit::Price),
    ("dpo", IndicatorUnit::Unitless),
    ("efficiency", IndicatorUnit::Ratio),
    ("efi", IndicatorUnit::Unitless),
    ("elder_ray", IndicatorUnit::Unitless),
    ("ema", IndicatorUnit::Price),
    ("envelope", IndicatorUnit::Price),
    ("eom", IndicatorUnit::Unitless),
    ("extended_volume_profile", IndicatorUnit::Price),
    ("fisher_transform", IndicatorUnit::Ratio),
    ("garman_klass", IndicatorUnit::Ratio),
    ("hires_volume_flow", IndicatorUnit::Volume),
    ("historical_volatility", IndicatorUnit::Ratio),
    ("hma", IndicatorUnit::Price),
    ("ichimoku", IndicatorUnit::Price),
    ("kama", IndicatorUnit::Price),
    ("keltner", IndicatorUnit::Price),
    ("klinger", IndicatorUnit::Volume),
    ("kst", IndicatorUnit::Ratio),
    ("liquidity_fvg", IndicatorUnit::Unitless),
    ("liquidity_pools", IndicatorUnit::Unitless),
    ("liquidity_sweeps", IndicatorUnit::Unitless),
    ("lsma", IndicatorUnit::Price),
    ("macd", IndicatorUnit::Unitless),
    ("market_structure_breaks", IndicatorUnit::Unitless),
    ("mass_index", IndicatorUnit::Ratio),
    ("mcginley", IndicatorUnit::Price),
    ("mfi", IndicatorUnit::Ratio),
    ("midas", IndicatorUnit::Price),
    ("money_flow_profile", IndicatorUnit::Price),
    ("multi_factor", IndicatorUnit::Ratio),
    ("nvi", IndicatorUnit::Unitless),
    ("obv", IndicatorUnit::Volume),
    ("order_block", IndicatorUnit::Unitless),
    ("parabolic_sar", IndicatorUnit::Price),
    ("persistent_volume_profile", IndicatorUnit::Price),
    ("pivot_sets", IndicatorUnit::Price),
    ("pivots_structure", IndicatorUnit::Unitless),
    ("pmo", IndicatorUnit::Ratio),
    ("ppo", IndicatorUnit::Ratio),
    ("pvi", IndicatorUnit::Unitless),
    ("pvt", IndicatorUnit::Volume),
    ("rci", IndicatorUnit::Ratio),
    ("relative_volatility", IndicatorUnit::Ratio),
    ("roc", IndicatorUnit::Ratio),
    ("rsi", IndicatorUnit::Ratio),
    ("rvat", IndicatorUnit::Ratio),
    ("rvi", IndicatorUnit::Ratio),
    ("rvol", IndicatorUnit::Ratio),
    ("sma", IndicatorUnit::Price),
    ("smi", IndicatorUnit::Ratio),
    ("stoch_rsi", IndicatorUnit::Ratio),
    ("stochastic", IndicatorUnit::Ratio),
    ("supertrend", IndicatorUnit::Price),
    ("t3", IndicatorUnit::Price),
    ("tema", IndicatorUnit::Price),
    ("trend_quality", IndicatorUnit::Unitless),
    ("trend_relationship", IndicatorUnit::Unitless),
    ("trix", IndicatorUnit::Ratio),
    ("true_range", IndicatorUnit::Unitless),
    ("tsi", IndicatorUnit::Ratio),
    ("twap", IndicatorUnit::Price),
    ("ulcer_index", IndicatorUnit::Ratio),
    ("ultimate_oscillator", IndicatorUnit::Ratio),
    ("vidya", IndicatorUnit::Price),
    ("vix_fix", IndicatorUnit::Ratio),
    ("volatility_regime", IndicatorUnit::Unitless),
    ("volume", IndicatorUnit::Volume),
    ("volume_profile", IndicatorUnit::Price),
    ("vortex", IndicatorUnit::Ratio),
    ("vwap", IndicatorUnit::Price),
    ("vwma", IndicatorUnit::Price),
    ("wavetrend", IndicatorUnit::Ratio),
    ("williams_r", IndicatorUnit::Ratio),
    ("wma", IndicatorUnit::Price),
    ("wyckoff", IndicatorUnit::Unitless),
    ("zigzag", IndicatorUnit::Price),
    ("zigzag_advanced", IndicatorUnit::Price),
    ("zscore", IndicatorUnit::Ratio),
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indicator::registry::{build_checked, CANONICAL_INDICATOR_NAMES};
    use crate::model::Bar;
    use std::collections::{HashMap, HashSet};

    /// The reason this is a table and not a `match` with a catch-all: a new indicator must not be
    /// able to inherit a default. Whoever adds it to the registry decides its unit here, or this
    /// test fails.
    #[test]
    fn every_canonical_indicator_declares_its_unit() {
        let declared: HashSet<&str> = OUTPUT_UNITS.iter().map(|(name, _)| *name).collect();
        let canonical: HashSet<&str> = CANONICAL_INDICATOR_NAMES.iter().copied().collect();

        let undeclared: Vec<&&str> = canonical.difference(&declared).collect();
        assert!(
            undeclared.is_empty(),
            "buildable but no declared output unit — they would silently fall back to Unitless \
             and be drawn in their own pane: {undeclared:?}"
        );

        let stale: Vec<&&str> = declared.difference(&canonical).collect();
        assert!(
            stale.is_empty(),
            "declared here but not a canonical indicator name: {stale:?}"
        );

        assert_eq!(
            OUTPUT_UNITS.len(),
            declared.len(),
            "duplicate name in OUTPUT_UNITS — the first entry would win silently"
        );
    }

    #[test]
    fn the_table_is_sorted_so_additions_land_in_one_obvious_place() {
        let names: Vec<&str> = OUTPUT_UNITS.iter().map(|(name, _)| *name).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        assert_eq!(names, sorted, "keep OUTPUT_UNITS in alphabetical order");
    }

    #[test]
    fn lookup_is_case_insensitive_and_conservative_about_strangers() {
        assert_eq!(output_unit("SMA"), IndicatorUnit::Price);
        assert_eq!(output_unit("sma"), IndicatorUnit::Price);
        assert_eq!(output_unit("rsi"), IndicatorUnit::Ratio);
        assert_eq!(output_unit("obv"), IndicatorUnit::Volume);
        assert_eq!(output_unit("macd"), IndicatorUnit::Unitless);
        assert_eq!(output_unit("no_such_indicator"), IndicatorUnit::Unitless);
    }

    fn probe_bars(n: usize, base: f64) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let drift = (i as f64) * 0.7;
                let wave = ((i as f64) * 0.35).sin() * 18.0;
                let close = base + drift + wave;
                let open = close - wave * 0.25;
                Bar {
                    timestamp: 1_700_000_000 + (i as i64) * 60,
                    open,
                    high: open.max(close) + 6.0,
                    low: open.min(close) - 6.0,
                    close,
                    volume: 1000.0 + (i % 17) as f64 * 40.0,
                }
            })
            .collect()
    }

    /// Every emitted series of an indicator, keyed by name, as the fraction of its finite values
    /// that lie within `band`.
    fn series_in_band(name: &str, bars: &[Bar], band: (f64, f64)) -> HashMap<String, f64> {
        let mut indicator = build_checked(name, &HashMap::new())
            .unwrap_or_else(|e| panic!("{name} does not build with its defaults: {e:?}"));
        let mut counts: HashMap<String, (usize, usize)> = HashMap::new();
        let note = |counts: &mut HashMap<String, (usize, usize)>, key: &str, v: f64| {
            if !v.is_finite() {
                return;
            }
            let entry = counts.entry(key.to_string()).or_insert((0, 0));
            entry.1 += 1;
            if v >= band.0 && v <= band.1 {
                entry.0 += 1;
            }
        };

        for bar in bars {
            if let Some(out) = indicator.on_bar(bar) {
                note(&mut counts, "value", out.value);
                if let Some(v) = out.secondary {
                    note(&mut counts, "secondary", v);
                }
                if let Some(v) = out.signal {
                    note(&mut counts, "signal", v);
                }
                for (key, v) in &out.extra {
                    note(&mut counts, key, *v);
                }
            }
        }

        counts
            .into_iter()
            .map(|(key, (inside, total))| (key, inside as f64 / total as f64))
            .collect()
    }

    /// The declaration above is a claim about arithmetic, so it is checked against arithmetic:
    /// every indicator is fed bars around an unmistakable price and its output is compared to the
    /// band those bars span.
    ///
    /// Two different assertions, because the two directions are not symmetric. A `Price`
    /// indicator must have **some** series sitting in the band — not `value`, since `ichimoku`
    /// keeps the cloud width there and the prices in `tenkan`/`kijun`/`senkou_*`. A non-`Price`
    /// indicator must keep its **primary** series out of it — its extras may well be price levels
    /// (`trend_relationship` reports two averages next to its spread, `atr` the raw distance).
    ///
    /// The thresholds sit in a wide measured gap: every `Price` indicator has a series at 1.00,
    /// and the highest `value` fraction among the rest is `efi` at 0.38.
    #[test]
    fn the_declared_units_match_what_the_indicators_compute() {
        const BASE: f64 = 4321.0;
        const IN_BAND: f64 = 0.9;
        const OUT_OF_BAND: f64 = 0.5;

        let bars = probe_bars(400, BASE);
        let low = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
        let high = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
        let band = (low * 0.5, high * 1.5);

        let mut not_price_after_all = Vec::new();
        let mut price_in_disguise = Vec::new();

        for (name, unit) in OUTPUT_UNITS {
            let series = series_in_band(name, &bars, band);
            // `rvat` needs a session history no synthetic run provides and stays silent.
            if series.is_empty() {
                continue;
            }

            if *unit == IndicatorUnit::Price {
                if !series.values().any(|fraction| *fraction >= IN_BAND) {
                    not_price_after_all.push(*name);
                }
            } else if series.get("value").copied().unwrap_or(0.0) > OUT_OF_BAND {
                price_in_disguise.push(*name);
            }
        }

        assert!(
            not_price_after_all.is_empty(),
            "declared Price but no output series lands on the price scale — drawn on the price \
             axis they would be invisible: {not_price_after_all:?}"
        );
        assert!(
            price_in_disguise.is_empty(),
            "compute price levels but are not declared Price — consumers will bury them in a \
             pane of their own: {price_in_disguise:?}"
        );
    }
}
