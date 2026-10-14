use std::collections::HashMap;

use crate::model::Bar;

use super::bollinger::{BollingerBands, VarianceConvention};
use super::{Indicator, IndicatorOutput};

/// BBTrend: how far a short Bollinger set has moved out of a long one.
///
/// Two band sets over the same prices, one short and one long, with the same multiplier and the
/// same variance convention. The published value compares how far each edge of the short set sits
/// from the matching edge of the long set:
///
/// ```text
/// BBTrend = 100 * (|lower_short - lower_long| - |upper_short - upper_long|) / basis_short
/// ```
///
/// Positive means the short set has pushed further out at the top than at the bottom — the recent
/// window is trending up relative to the longer one — and negative the reverse. Around zero the
/// two sets sit concentrically, which is what a range looks like.
///
/// Both sets come from the same [`BollingerBands`] engine this crate already has, so there is one
/// band calculation, not a second one written for this indicator. The variance convention applies
/// to both sets: comparing a population-based set against a sample-based one would measure the
/// convention rather than the market.
///
/// Unit: percent of the short basis. Division by a zero basis cannot arise from valid bars, whose
/// prices are positive; should it, the value is `0`.
///
/// Per-bar outputs:
/// - `value`: the BBTrend line.
/// - `extra["upper_gap"]` / `extra["lower_gap"]`: the two absolute edge distances it is built
///   from, so a reader can see which side moved.
///
/// First output: with the `long_len`-th bar, when both sets exist. [`Indicator::reset`] clears
/// both.
#[derive(Debug, Clone)]
pub struct BbTrend {
    short: BollingerBands,
    long: BollingerBands,
    long_len: usize,
}

impl BbTrend {
    pub fn new(short_len: usize, long_len: usize, mult: f64, variance: VarianceConvention) -> Self {
        Self {
            short: BollingerBands::new(short_len, mult).with_variance(variance),
            long: BollingerBands::new(long_len, mult).with_variance(variance),
            long_len,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(20, 50, 2.0, VarianceConvention::Population)
    }
}

impl Indicator for BbTrend {
    fn name(&self) -> &str {
        "bbtrend"
    }

    fn warmup_period(&self) -> usize {
        self.long_len
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        let short = self.short.on_bar(bar);
        let long = self.long.on_bar(bar);
        let (short, long) = (short?, long?);

        let upper_gap = (short.extra["upper"] - long.extra["upper"]).abs();
        let lower_gap = (short.extra["lower"] - long.extra["lower"]).abs();
        let basis = short.value;

        let value = if basis != 0.0 {
            100.0 * (lower_gap - upper_gap) / basis
        } else {
            0.0
        };

        let extra = HashMap::from([
            ("upper_gap".to_string(), upper_gap),
            ("lower_gap".to_string(), lower_gap),
        ]);
        Some(IndicatorOutput::with_extra(value, extra))
    }

    fn reset(&mut self) {
        self.short.reset();
        self.long.reset();
    }
}

pub const BBTREND_DEFINITION: super::definition::IndicatorDefinition = super::definition::IndicatorDefinition {
    name: "bbtrend",
    aliases: &[],
    description: "BBTrend (how far a short Bollinger set has moved out of a long one, in percent of the short basis; build_typed accepts variance=population|sample for both sets)",
    params: &[
            super::definition::ParamSpec::choice("variance", "population", &["population", "sample"], false).meaning("variance convention of both band sets"),
            super::definition::ParamSpec::count("short_len", 20, 1, 10000).meaning("bars in the short Bollinger set"),
            super::definition::ParamSpec::count("long_len", 50, 1, 10000).meaning("bars in the long Bollinger set"),
            super::definition::ParamSpec::number("mult", 2.0, 0.01, 100.0).meaning("band width in standard deviations, both sets"),
    ],
    checks: &[
            super::definition::Check::Less {
                lower: "short_len",
                upper: "long_len",
            },
        ],
    build: |p| {
        let variance = match p.choice("variance") {
            "sample" => VarianceConvention::Sample,
            _ => VarianceConvention::Population,
        };
        let short_len = p.count("short_len");
        if variance == VarianceConvention::Sample && short_len < 2 {
            return Err(super::registry::RegistryError::IncompatibleParameter {
                parameter: "variance".to_string(),
                indicator: "bbtrend".to_string(),
                reason: "sample variance divides by len - 1 and is undefined for len < 2"
                    .to_string(),
            });
        }
        Ok(Box::new(BbTrend::new(
            short_len,
            p.count("long_len"),
            p.number("mult"),
            variance,
        )))
    },
};
