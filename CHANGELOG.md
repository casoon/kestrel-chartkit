# Changelog

All notable changes to kestrel-chartkit are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and kestrel-chartkit follows
[Semantic Versioning](https://semver.org/): before 1.0, a minor version can contain breaking
changes.

This file starts after 0.34.0; earlier releases are recorded by their git tags.

## [Unreleased]

## [0.53.0] - 2026-11-23

### Added

- **Trendlines:** `trendlines` names the nodes that touch a line (`support_node_touch_<i>_bars_back` / `_price`, likewise `resistance_`) and reads the channel parallel: `channel_node_touches` counts the nodes of the other kind on it, `channel_node_touch_<i>_*` names them and `channel_touch` marks a bar that reaches it. `Trendline` carries them as `touching_nodes` and `channel_touching_nodes`, two new public fields; no existing value changes. Details: [Trendlines](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#trendlines).

### Changed

- **Pages:** the project site uses the current shared theme (0.16.4).
- **Docs:** README and the installation guide name the current release tag.

### Fixed

- **Levels, lines and zones:** `kestrel_equal_level_liquidity`, `trendlines` and `kestrel_structure_zones` count a price within `indicator::MARK_TOLERANCE` (1e-9 ATR) of a mark as on it, so pivot highs at 100.00 and 100.20 with a tolerance of 0.20 form one level, where rounding gave two. Results change only where a price lies on a mark; that still asks for a minor version. Details: [A price on a mark](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#a-price-on-a-mark).

## [0.52.0] - 2026-11-22

### Added

- **Heatmap:** `heatmap::compute_state_index` collapses one heatmap row into `InstrumentStateIndex`: score `100 * direction * order gate` in `[-100, +100]`, five regimes, and stretch and wick warning flags kept out of the score. Without volume the direction weights move to the price tracks; a `NaN` input is not substituted.

## [0.51.0] - 2026-11-22

### Added

- **Heatmap:** `heatmap::evaluate_instrument_heatmap` returns ten state values per bar in five groups (direction, order, volatility, flow, momentum; momentum is the 10-bar acceleration and the stretch from the 50-bar mean), each normalised to `[-1, 1]` by a rolling 200-bar percentile. Measures that need volume stay `NaN` without it.

## [0.50.0] - 2026-11-22

### Added

- **Telemetry:** `compute_market_telemetry` reads six dimensions off a bar series — order, tension, stress, stretch, open levels and spread load — from existing catalog entries, with every threshold in `TelemetryConfig`. Details: [Market telemetry](https://casoon.github.io/kestrel-chartkit/docs/guides/market-telemetry/).
- **Pages:** the guide [How entries are verified](https://casoon.github.io/kestrel-chartkit/docs/guides/verification/) says what the mark means, which acceptance every entry passes and which tests keep it true.

### Changed

- **Pages:** the project site uses the current shared theme (0.16.2).

### Fixed

- **Pages:** the home page and the showcase no longer say that some entries are not yet verified; all 196 are.

## [0.49.0] - 2026-11-21

### Changed

- **Catalog:** `correlation`, `relative_strength`, `intrabar_cvd`, `gap`, `point_and_figure`, `fvg_fill`, `kestrel_vwap_regime`, `kestrel_swing_structure`, `trendlines`, `chart_patterns` and `harmonic_patterns` are now verified against a second implementation; 196 entries in all, all verified.
- **Catalog:** the descriptions of `kestrel_adaptive_smoother`, `kestrel_swing_structure`, `fvg_fill`, `super_smoother`, `alma`, `r_squared` and `fractal_dimension` also state the purpose of the entry, what it is used for and a caveat; they show in the indicator reference and the showcase.

### Fixed

- **zigzag_advanced:** bars before the first swing node publish nothing, instead of the middle bar's close with the state `confirmed`; the state of every output is `running`.
- **kestrel_vwap_regime:** a close exactly on the VWAP counts on neither side of it, and a slope of 0 is flat at every threshold, so a series reflected about a level gives mirrored readings. An ATR of 0 now publishes nothing and a sigma of 0 no `z_score`, instead of 0.0.
- **relative_strength:** a bar where one of the four closes used is not finite or not positive publishes nothing; before, such a return counted as 0 and the ratio as 1.
- **Swing structure:** `kestrel_swing_structure` keeps its pivots strictly alternating (a pivot of the same kind continues the swing and the more extreme one stays), orders the two pivots of an outside bar by the alternation instead of always high first, and reads the trend of an undecided structure from the newest leg; a series reflected about a level now gives the same readings, which it did not on series with equal highs and lows or outside bars. Readings on such series change.
- **Cycle length:** `cycle_length` treats a close that sits on its average up to rounding noise as exactly on it, so a reflected series counts the same crossings.
- **chart_patterns:** boundary patterns break out only above or below both boundaries (a close between crossed lines is none), two flat boundaries are a range and give no triangle or wedge, flat tests run before wedge tests, a triple needs both counter-swings off the level, the cup rule counts bars, and an instance expires after exactly `pending_swings` nodes. An instance whose window ends at a bar that is both a swing high and a swing low is no longer found and alerted twice. Events change.
- **harmonic_patterns:** the butterfly takes D from 1.27 of XA, the shark measures D on XC, and the reversal zone always contains the tolerance-widened D interval. Events change.

## [0.48.0] - 2026-11-21

### Added

- **Catalog:** `true_range_money_flow` (alias `trmf`) is a verified catalog entry; 196 entries in all, 185 of them verified. It is a money flow ratio like `cmf`: the close located in the true range (previous close included, so gaps count), weighted by volume, with Wilder averages instead of window sums and each bar's volume capped at `volume_cap` times the average volume before it (after Katsanos, 2004).

## [0.47.0] - 2026-11-21

### Added

- **Catalog:** `volume_flow_indicator` (after Katsanos, 2004), `volume_zone_oscillator` (Khalil and Steckler, 2011), `volume_price_confirmation` (Dormeier, 2007) and `market_facilitation` (Williams, 1995) are verified catalog entries; 195 entries in all, 184 of them verified. The volume price confirmation takes the difference of the volume weighted and the plain mean from deviations from the current close, so it keeps its precision at high price levels.

### Changed

- **Contributors:** a contract case can pin the published state as a text column; it is compared when the case writes it.

## [0.46.0] - 2026-11-21

### Breaking

- **Chart patterns:** `ChartPatternKind` has three more variants (`Flag`, `CupAndHandle`, `InverseCupAndHandle`), so an exhaustive `match` needs arms for them.

### Added

- **Catalog:** the volatility estimators `parkinson`, `rogers_satchell` and `yang_zhang` and the return statistics `variance_ratio`, `autocorrelation`, `hurst_rs` and `ar1_half_life` are verified catalog entries. Where they follow a convention (annualised percent, log returns as logarithms of price ratios, no small-sample correction, nothing published for a coefficient at most 0 or within 1e-9 of 1), the description says so.
- **Catalog:** `setup_countdown` (setup of nine and countdown of thirteen after DeMark, 1994, core rules without the later cancellation and recycle refinements), `narrow_range` (NR4/NR7 and inside bar after Crabel, 1990), `williams_fractals` (confirmed fractal levels after Williams, 1995) and `tpo_profile` (time-at-price profile without volume after Steidlmayer and Koy, 1986) are verified catalog entries; 191 entries in all, 180 of them verified.
- **Parameter relations:** a relation between parameters can now also be written as "`times` times `value` at most `limit`" (`Check::AtMost`).
- **Chart patterns:** `ChartPatternDetector` and `chart_patterns` find the flag (`Flag`, code 15) and cup and handle with its inverse (`CupAndHandle`, `InverseCupAndHandle`, codes 16 and 17); new constants `FLAG_PARALLEL` and `FLAG_MAX_RETRACE`. Rules and limits: [structure and events guide](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/).

### Changed

- **Chart patterns:** series with the flag or cup and handle shapes report the new kinds in addition; an inverse head and shoulders whose right shoulder stays in the cup's upper half also raises `bull_cup_and_handle`.

## [0.45.1] - 2026-11-21

### Changed

- **Catalog:** `kestrel_adaptive_smoother` is accepted and counts as verified; its formula and measurements are in the [guide](https://casoon.github.io/kestrel-chartkit/docs/guides/adaptive-smoother/).

## [0.45.0] - 2026-11-21

### Added

- **Catalog:** oscillators `schaff_trend_cycle`, `qqe`, `demarker`, `relative_momentum_index` and `squeeze_momentum` and trend lines `zlema`, `frama`, `nadaraya_watson`, `range_filter`, `most`, `ott`, `halftrend` and `ssl_channel` are verified catalog entries; 180 entries in all, 168 of them verified. Where they start differently from the usual forms (a direction starts undecided instead of up, the squeeze's Bollinger bands use their own multiplier), the description says so.
- **Indicator `kestrel_adaptive_smoother`:** the Kestrel Adaptive Smoother as a catalog entry (alias `kas`, type `Kas`): a cascade of three exponential stages whose common gain follows a score of how directed and how unusual the recent movement is. `period` (default 20), `phase` (0..2, default 0.5: lead against overshoot, 0 cannot overshoot), `adapt` (0..2, default 0.5: 0 is a fixed filter); valid from the first bar. Marked as not yet accepted; formula, measurements and the candidates it was chosen from: [The adaptive smoother](https://casoon.github.io/kestrel-chartkit/docs/guides/adaptive-smoother/).
- **Example `smoother_tradeoffs`:** measures lag, overshoot, residual error, roughness and direction changes of the smoothers of the catalog that have a length on deterministic test signals and on close series read from files (`cargo run --release --example smoother_tradeoffs -- synthetic out.csv`), and prints the comparison at equal smoothness (`summary`).

## [0.44.0] - 2026-11-21

### Breaking

- **Catalog:** the 25 own constructions carry the `kestrel_` prefix (`kestrel_trend_quality`, `kestrel_swing_structure`, `kestrel_vwap_regime` and so on; the [indicator guide](https://casoon.github.io/kestrel-chartkit/docs/guides/indicators/) says which). The old names stay aliases for building and lookup; `Indicator::name()` and the catalog report the new name, so code that compares stored names against `name()` has to resolve them through `canonical_name`.

### Added

- **Catalog:** `ultimate_smoother`, `roofing_filter`, `reflex`, `trendflex`, `correlation_trend`, `laguerre_rsi`, `cyber_cycle` and `mama` (Ehlers' filters and oscillators) are verified catalog entries; 166 entries in all, 155 of them verified. Where they start differently from Ehlers' listings (from the price instead of from 0), the description says so.

### Changed

- **Catalog:** `rma`, `alma`, `jma`, `super_smoother`, `percent_rank`, `fractal_dimension`, `r_squared`, `cycle_length`, `price_location`, `period_midpoint`, `opening_range`, `rolling_beta`, `pair_spread` and `heikin_ashi` are verified. Their descriptions state the conventions a formula leaves open, such as the start values, the unrounded `alma` peak and the week and month grid of `period_midpoint`.

### Fixed

- **Catalog:** `output_unit`, `output_range` and `threshold_params` resolve aliases like `definition` does; before, an alias (`wpr`, `srsi`, `vp`) answered `Unitless` and `Unbounded`.

## [0.43.0] - 2026-11-20

### Added

- **Docs and site:** the indicator reference and the showcase show for every catalog entry whether it is verified, with the reason for open ones; the start page counts verified entries from the catalog, and the six entries of other input kinds are listed in the showcase without a chart.
- **Indicator `rma`:** Wilder's smoothed moving average as a catalog entry (`RmaEngine`), the average inside RSI, ATR and ADX; `period` (default 14), published from the `period`-th bar.
- **Indicator `alma`:** the Arnaud Legoux moving average as a catalog entry (`AlmaEngine`); `period` (default 9), `offset` (0.85), `sigma` (6).
- **Indicator `jma`:** an approximation of the Jurik moving average as a catalog entry (`JmaEngine`), following an openly published formula; it is not the product of Jurik Research, whose original is not public, and does not reproduce its values. `period` (default 14), `phase` (-100..100), `power` (at least 1).
- **Indicator `super_smoother`:** Ehlers' 2-pole SuperSmoother as a catalog entry (`SuperSmootherEngine`); `period` (default 10), started from the price (the first two outputs equal the first two closes), published from the `period`-th bar.
- **Indicator `fractal_dimension`:** the fractal dimension of the price path over a trailing window as a catalog entry (`FractalDimensionEngine`), 1 for a straight line and towards 2 for noise. `method` names the estimate: `halves` (Ehlers' halving estimate, clamped to 1..2) or `path` (1 + ln(path / range) / ln period, unclamped); `period` (default 30, at least 4). Coil Release and the trend-persistence read-out call the same kernel (`kernels::fractal_dimension`) with their own method, so their outputs are unchanged.
- **Indicator `r_squared`:** the R-squared of a least-squares line through the last `period` closes as a catalog entry (`RSquaredEngine`, over `PredictabilityTracker`), 1 for a clean straight trend and near 0 for noise; a window of equal closes reads 1. `period` (default 20).
- **Indicator `percent_rank`:** the share, in percent, of the previous `period` closes at or below the current close as a catalog entry (`PercentRankEngine`); `period` (default 20), first output with the `(period + 1)`-th bar.
- **Indicator `cycle_length`:** the mean number of bars between the close's crossings of its simple average as a catalog entry (`CycleLengthEngine`, over `AdaptiveCycleTracker`), documented as a zero-crossing swing length and not as a spectral or Hilbert-transform cycle estimate; `period` (default 20), `history` (completed swings averaged, default 8); `extra["bars_since_crossing"]` counts the swing in progress. The first output comes with the first completed swing.
- **Indicator `price_location`:** the price-location read-out as a catalog entry (`PriceLocationIndicator`, over `PriceLocationEngine`): `value` is the stretch `(close - EMA) / ATR`, `extra` carries `atr`, `ema`, `range_position`, `sma`, `sma_prior` and `vol_tercile` where they exist; seven length parameters with the engine's defaults.
- **Indicator `opening_range`:** the opening range of a recurring session as a catalog entry (`OpeningRangeEngine`, over `SessionTracker`): `value` is the width of the range, `extra` has `orb_high`, `orb_low`, `in_orb_window` and `new_session`; session start and end, `orb_duration_mins` and a fixed `utc_offset_seconds` are parameters.
- **Indicator `period_midpoint`:** the midpoint of the last completed period as a catalog entry (`PeriodMidpointEngine`, over `PeriodMidpointTracker`), held until the next period completes; `unit` (second to month), `count` and `utc_offset_seconds` choose the period.
- **Indicator `vwap_regime`:** the VWAP as a regime reading as a catalog entry (`VwapRegimeEngine`: the rolling `Vwap`, a Wilder ATR and `VwapRegimeTracker`): `value` is the distance of the close from the VWAP in ATRs, `extra` has `vwap`, `slope_atr`, `slope_state` (-2..2), `z_score`, `cross_frequency` and `price_persistence`. Needs real volume.
- **Indicator `swing_structure`:** the statistical swing structure as a catalog entry (`SwingStructureIndicator`, feeding `SwingStructureEngine` a Wilder ATR of its own): `value` is the median correction over the median impulse leg, `extra` the retracement, remaining potential, required stop, reward-to-risk and impulse velocity in ATRs, the newest three corrections and the trend quality (also the `state` label). `SwingStructureEngine` itself is unchanged.
- **Indicator `fvg_fill`:** the fair value gaps of `liquidity_fvg` followed as zones until a later bar trades back into them, as a catalog entry (`FvgFillEngine`, over `FvgZoneTracker`): `value` is the number of open zones, `extra` has `created`, `filled`, `open_bullish` and `open_bearish`; `max_zones` (default 20) bounds the open zones kept.
- **Indicator `rolling_beta`:** the beta of the simple returns of a series against a reference series over the last `len` return pairs (default 60) as a catalog entry of kind `Pair` (`RollingBetaEngine`, over `cross_asset::compute_rolling_beta`); `extra` has `alpha`, `correlation` and `r_squared`. No output while the reference's returns do not vary.
- **Indicator `pair_spread`:** the residual z-score of the spread `close - hedge_ratio * reference close` over the last `len` paired closes (default 60) as a catalog entry of kind `Pair` (`PairSpreadEngine`, over `cross_asset::compute_pair_spread`); `extra` has `hedge_ratio`, `spread`, `mean_spread` and `std_spread`. A description of the window, not a cointegration test.
- **Indicator `chart_patterns`:** the chart patterns of `ChartPatternDetector` (triangles, broadening, wedges, 1-2-3, Wolfe wave, double and triple top and bottom, head and shoulders and its inverse) as a catalog entry (`ChartPatternsEngine`, over an `AdvancedZigZagEngine` and the detector): one alert per pattern instance on the bar its rule reports it confirmed, `bull_<kind>`/`bear_<kind>`; `value` is the direction of the latest confirmed pattern, `extra` has `pattern` (kind code 1-14), `level`, `invalidation`, `target`, `fit`, `active` and `confirmed`. ZigZag parameters as in `zigzag_advanced`, plus `tolerance_pct` (2), `flat_pct` (1.5) and `pending_swings` (2).
- **Indicator `harmonic_patterns`:** the XABCD structures of `HarmonicDetector` (gartley, bat, butterfly, crab, deep crab, shark) as a catalog entry (`HarmonicPatternsEngine`): one alert per structure on the close out of its potential reversal zone, `bull_<name>`/`bear_<name>`; `extra` has `pattern` (table row 1-6), `prz_low`, `prz_high`, `d`, `invalidation`, `active` and `confirmed`. Parameters: the ZigZag ones, `ratio_tolerance` (0.03), `min_prz_share` (0.005), `pending_swings` (2).
- **Catalog:** `IndicatorCatalogEntry` carries `verification` (`Verification::Verified`, or `Open` with the reason), declared in the entry's definition and held against the mark in `INDICATOR_STATUS.md` by a test. Entries added with the catalog work (`rma`, `alma`, `jma`, `super_smoother`, the entries of the rolling and structure read-outs, the pair entries, `heikin_ashi`, `intrabar_cvd`) are `Open`; no status changed.
- **Definitions:** `IndicatorFacts` (unit, output range, `SourceHandling`, data and pair requirements, built from the unit with chained setters such as `IndicatorFacts::ratio().bounded(0.0, 100.0)`), `Verification` and `ModuleApiDefinition` in `indicator::definition`; `registry::canonical_indicator_names()`.

### Changed

- **Catalog (BREAKING):** `catalog()` lists everything that computes per bar, whatever its input. `IndicatorCatalogEntry` has a new field `input: IndicatorInput` (`Bars`, `Pair`, `Transform`, `Intrabar`), so code that builds the struct literally must add it. New entries: `correlation` and `relative_strength` (`Pair`, `build_pair_checked`, from the same definitions as before), `heikin_ashi` (`Transform`, `build_transform_checked`, a `BarTransform` giving a `HeikinAshiBar`) and `intrabar_cvd` (`Intrabar`, `build_intrabar_checked`, an `IntrabarIndicator` giving an `IntrabarDelta`; parameters `anchor`, `anchor_offset_seconds`, `unchanged_policy`). `build_checked` and `build_typed` build `Bars` entries only and answer the name of another kind with the new `RegistryError::WrongInput { indicator, input, builder }`, naming the right builder; `build_pair_checked` does the same for names of other kinds instead of `UnknownIndicator`. Code that builds every `catalog()` entry with `build_checked` must filter with `catalog_of(IndicatorInput::Bars)`. New: `catalog_of`, `input_of`, `IndicatorInput`, `indicator::other_input` (`InputDefinition`, `transform_definitions`, `intrabar_definitions`). `PairIndicatorDefinition` is now an alias of `InputDefinition<dyn PairIndicator>`; its fields are unchanged. Outputs of the existing indicators are unchanged.
- **Pages:** the project site uses the current shared theme (0.16.1).
- **Pages:** the start page shows a computed chart up front, the areas of the crate with their counts, chartlet and kestrel-loom as the two ways to draw, and how the results are checked against the independent reference.
- **Indicators:** SMA, EMA, WMA, Wilder smoothing (RMA) and KAMA are each computed by one kernel in `indicator::smoothing`. `SmaEngine`, `WmaEngine` and `KamaEngine` wrap it instead of carrying a copy; the inline exponential averages of CCI, PPO, Keltner Channel, PMO, the price-average relation and the price-location read-out, the SMA of the cheat-sheet levels and the SMA20 of the regime classifier call it as well. Catalog outputs are unchanged.
- **Indicators:** the shared formula kernels live in `indicator::kernels`. The true range is computed once there; about 35 indicators, the evaluation helpers and the analytics read-outs call it instead of carrying an inline copy. Catalog outputs are unchanged.
- **Indicators:** the average true range is one kernel (`kernels::AtrKernel`, true range plus the smoothing chosen by `TrueRangeSmoothing`) behind `Atr`, `analytics::WilderAtr`, the price read-out's ATR series and the fear-gauge's simple ATR. `WilderAtr` now rounds like the catalog ATR (differences of a few units in the last place); catalog outputs are unchanged.
- **Indicators:** the efficiency ratio (net move over path length of the closes) is one kernel, `kernels::efficiency_ratio`, with `path_and_net` for callers that scale the two parts themselves. Efficiency, KAMA, Coil Release, Market State Chain, Structure Zones, Reversal Setup Score, Accumulation Phase, Tradability Filter, the regime and trend-persistence read-outs and `structure::path_efficiency` call it; each keeps its own value for a motionless run and its own scaling. Catalog outputs are unchanged.
- **Indicators:** the Pearson correlation sums (covariance and both variance sums) are one kernel, `kernels::pearson_sums`, behind `stats::correlation`, the pair correlation indicator and cross-asset correlations, and the correlation of closes with their bar index in the trend-persistence read-out (which now divides by `sqrt(var_x * var_y)` like the others, a difference in the last places). Catalog outputs are unchanged.
- **Indicators:** the percent rank is one kernel, `kernels::rank_weight`, with the tie rule (`AtOrBelow` or `Half`) as its parameter. The smoother-side rank, `stats::percent_rank`, Exhaustion, Rank Channel, Connors RSI and the activity and price read-outs count through it. Catalog outputs are unchanged.
- **Indicators:** the mapping from an averaged up and down move to the 0..100 index (RSI from gain and loss, MFI from positive and negative flow) is one kernel, `kernels::up_down_index`, with the value for "neither side moved" as its parameter. RSI, StochRSI, MFI and the composites that carry their own RSI or MFI call it; the RSI targets of the cheat sheet average with the Wilder smoother (a difference in the last places). Catalog outputs are unchanged.
- **Indicators:** the Choppiness formula is one kernel, `kernels::choppiness`, with the clamp to `0..=100` as its parameter: the indicator and Coil Release publish it clamped, the regime read-out raw. Catalog outputs are unchanged.
- **Indicators:** `SmootherKind` is the one list of moving-average kinds usable as a smoother parameter (new: `Wma`; `name()`, `from_name()`, `from_code()`, `ALL`, `NAMES`, each variant named like its catalog entry). `trend_relationship` now accepts every kind for `fast_kind`/`slow_kind`; the `ma_kind` of `price_average_relation` and `roc_context`, the ATR smoothing and the RSI smoothing are documented subsets of it (`MA_KINDS`, `TrueRangeSmoothing::KINDS`, `RsiSmoothing`) and convert to and from it. Existing numeric codes and names keep their meaning; catalog outputs are unchanged.
- **Indicators:** swing-pivot detection is one kernel (`kernels::PivotRule`: bars left and right, strict or tolerant ties per side, run through one window test or over a whole series). `structure::confirmed_pivots`, `find_sr_zones` and `strict_pivots`, the pivot detectors of 24 indicators (ZigZag, advanced ZigZag, BOS/CHoCH, market structure breaks, pivot structure, swing structure, liquidity sweeps, smart-money structure, double pattern, bottom formation, zone rejection, divergence, leg character, leg conviction and others) and the reversal-type labels call it instead of carrying their own window loop. Catalog outputs are unchanged.
- **Indicators:** four more formulas exist once, in `indicator::kernels`: the close location value (`close_location_value`: Accumulation/Distribution, Chaikin Money Flow and the fear-and-greed flow read-out), the Williams VIX Fix pair and its spike band (`vix_fix_pair`, `spike_band`: the `vix_fix` indicator and the fear-gauge read-out; the read-out's inverse gauge divides by the lowest close, the indicator's by the high, now a named parameter), the floor-trader pivots (`floor_pivots`: the `pivot_sets` Classic set and the cheat sheet) and, in `indicator::price_levels`, the one table of Fibonacci ratios (`FIBONACCI_RATIOS`, also read by the cheat sheet and the Fibonacci pivot set). Rolling beta and the pair spread of `cross_asset` sum through `kernels::pearson_sums`. The price-average relation keeps its own form of the efficiency ratio on purpose (documented). Catalog outputs are unchanged.
- **Engine pipeline:** `update_vwap_regime` and the `VwapRegimeEngine` entry turn the VWAP slope into ATRs in one place (`VwapRegimeTracker::update_with_slope`), and the pipeline helpers convert the ATR percentage through one function. Outputs are unchanged.
- **Definitions (BREAKING):** what was kept by name beside the definitions is declared in the definition: the output range (`output_range`), the unit (`output_unit`), the data requirements (`data_requirements`, `pair_requirements`), the native price and the sources an indicator rejects (`build_typed`). The lists are gone; the lookups keep their signatures and answer the same values for every name, alias and unknown name (a test holds a snapshot of them). `IndicatorDefinition` has the new required fields `facts` and `verification`; `InputDefinition` (`PairIndicatorDefinition` and the transform and intrabar definitions) trades `unit` and `output_range` for `facts` and gains `verification`; `IndicatorCatalogEntry` has the new field `verification`, so code that builds either literally must add them. `registry::CANONICAL_INDICATOR_NAMES` is replaced by the function `registry::canonical_indicator_names()`, derived from the definitions in catalog order. `elliott`, which has no catalog entry, is declared as a `ModuleApiDefinition`. Outputs are unchanged.

### Removed

- **Series (BREAKING):** `series::CumulativeSum` and its crate-root re-export are removed; nothing in the crate or its consumers used it and it has no catalog entry.
- **Internal building blocks (BREAKING):** what computes a value per bar is a catalog entry or crate-internal. `Exhaustion` (module `indicator::exhaustion`), `SlopeDivergence`, `ExtremeWindow`, `RollingRobustThreshold` and `RobustBand` are now `pub(crate)`; `SmootherChain`, `HysteresisBand`, `HysteresisLevel` and `PriceLevelAggregator` (with its error type) are removed, none was used outside the crate.

### Deprecated

- **`analytics::classify_trend_regime`:** renamed to `analytics::vote_regime`; the name sounded like `regime::classify_regime`, which is a different classifier (20-bar average slope with ADX and ATR). Same arguments and result.
- **`indicator::elliott::rule_catalog`:** renamed to `elliott_rule_catalog`; `rule_catalog` is also the name of the applicability rules (`kestrel_chartkit::rule_catalog`).
- **`analytics::WilderAtr` and `analytics::wilder_atr_series`:** use the catalog indicator `atr` (percent of close; `extra["raw"]` is the price distance these return).
- **`pair_catalog()`:** the pair indicators are entries of `catalog()`; `pair_catalog()` is a filter over it, and `catalog_of(IndicatorInput::Pair)` returns the same.

### Fixed

- **SuperSmoother:** the `SuperSmoother` choice of the smoother parameter (`ma_kind`, `fast_kind`, `slow_kind`, `SmootherKind::SuperSmoother`) now starts at the price level in every indicator that offers it, instead of from zero (first published values ~1.5 % off the price at period 10).
- **Chart patterns:** the Wolfe wave of `ChartPatternDetector` never matched (point 5 was tested against the wrong side). It now follows Bill Wolfe's rules: point 3 beyond point 1, point 4 inside the 1-2 range, converging lines 1-3 and 2-4, point 5 beyond the projected 1-3 line; `fit` is the convergence and `tolerance_pct` no longer applies.

## [0.42.0] - 2026-11-17

### Added

- **RSI, Williams %R:** `extra["raw"]` carries the unsmoothed value before the `avg_len` EMA
  (equal to `value` for `avg_len = 1`); `value` is unchanged.
- **Indicator:** `convergence_period()` (default `warmup_period()`) states after how many bars the
  start-up state no longer matters beyond about 1e-3; overridden for WaveTrend, TSI, MACD, TRIX,
  RSI and ATR.

### Changed

- **Pages:** the project site uses the current shared theme (0.12.14).


## [0.41.0] - 2026-11-10

### Breaking

- **Applicability:** `ApplicabilityReason` has two new variants (`PairPriceAdjustmentDiffers`,
  `PairSessionDiffers`), so exhaustive `match` expressions over it need an update.
- **Applicability:** `ApplicabilityReason` gains `StructureContinuousSession` and
  `StructureMethod` gains `PriceGaps` and `PointAndFigure`, so exhaustive `match` expressions
  over them need the new arms. `method_rule_catalog` returns 17 rules instead of 8.
- **Equal-level liquidity:** `EqualLevelLiquidityConfig` gains the public field
  `list_min_touches`, so a struct literal must set it or end in `..Default::default()`.

### Added

- **Applicability:** `check_pair_applicability`, with `pair_requirements` and `pair_rule_catalog`,
  reports when the two series of a two-series indicator differ in price adjustment or session.
- **Elliott:** the validators describe what they check as data. `elliott::rule_catalog()` lists
  the conditions of the pivot sequence in the order they are tested, per structure the variants,
  rules and guidelines with their identifiers, sentences and origin, and the threshold constants,
  among them the new `EXTENDED_WAVE_MIN_RATIO` of `identify_extended_wave`. Details: [Elliott rules](https://casoon.github.io/kestrel-chartkit/docs/reference/elliott-rules/).
- **Equal-level liquidity:** the output carries the calculation: `atr`, `tolerance` (absolute),
  and every level with at least `list_min_touches` pivots as `bsl_<i>_*` / `ssl_<i>_*` with
  price, pivots, tests, status and the events of the bar (pivot, test, sweep, breakout, stop
  hunt, reclaim). Swept levels stay listed. Details:
  [Levels and zones in the output](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#levels-and-zones-in-the-output).
- **Gap:** the new detector `gap` (`reference`, `atr_len`, `min_atr`, `max_bars`; defaults
  `range`, 14, 0, 100) reports price gaps between consecutive bars: edges, size in ATR of the
  previous bar, the filled fraction of every open gap and the closing with the bars it took.
  `data_requirements("gap")` names the series properties that limit the result. The catalog
  grows from 133 to 134 entries.
- **Options:** `option_expiry_result` gives the result of a call or put at expiry for the bought
  or sold side with premium and contract size (`OptionExpiryPosition`), `option_expiry_profile`
  its break-even, largest profit and largest loss (`PayoffBound`: `Limited` or `Unlimited`), and
  `option_expiry_series` the result over a price grid. Details:
  [Options at expiry](https://casoon.github.io/kestrel-chartkit/docs/reference/api/#options-at-expiry).
- **Point and figure:** `point_and_figure` (re-exported at the crate root, with `PnfChart`,
  `PnfColumn`, `PnfDirection`, `PnfChange` and `HorizontalCount`) builds the columns of X and O
  from a price sequence, a box size and a reversal in boxes; `PnfChart::horizontal_count` returns
  the count between two columns with its count line, effect and targets. A box is filled when
  the price reaches its grid line, so a reversal needs the full number of boxes.
- **Point and figure:** the new indicator `point_and_figure` (alias `pnf`; `box_size`,
  `reversal`, `method`; defaults 1, 3, `close`) feeds the chart from closes or from highs and
  lows and reports the current column on every bar (`column`, `top`, `bottom`, `boxes`), with the
  alerts `pnf_new_x_column` and `pnf_new_o_column`. The catalog grows from 134 to 135 entries.
- **Structure zones:** `structure_zones` lists every tracked zone as `zone_<i>_*`, the broken
  ones included, with edges, role, status, tests and the events of the bar (created, test,
  bounce, break, role reversal), plus bar and close of the latest break and `atr`.
- **Trendlines:** the `trendlines` indicator draws a support line through confirmed ZigZag lows
  and a resistance line through confirmed highs, each with its channel parallel, its touches and
  its break as an event (`TrendlineEngine`, alerts `bear_support_line_break` and
  `bull_resistance_line_break`). Details:
  [Trendlines](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#trendlines).
- **Two-series indicators:** `relative_strength` and the new `correlation` are built by name with
  `build_pair_checked`, listed by `pair_catalog()` and run with `run_pair_batch`, which pairs two
  bar series by timestamp. `catalog()` and `build_checked` are unchanged. Details:
  [Two series](https://casoon.github.io/kestrel-chartkit/docs/guides/indicators/#two-series).
- **Wyckoff:** the state machine describes itself as data. `wyckoff::transitions()` lists every
  move between the phases (`WyckoffTransition`: phases, bias, condition, events, parameters), and
  `WyckoffStateMachine::last_transition()` names the one the last bar took. Details:
  [Wyckoff transitions](https://casoon.github.io/kestrel-chartkit/docs/reference/wyckoff-transitions/).

### Changed

Output values of `zigzag_advanced` and `zigzag` change on the series named below, hence a minor
version.

- **Chart patterns:** `ChartPatternDetector::scan` returns the auto-trendline through the highs
  and the one through the lows side by side; before, the two evicted each other and only one was
  left.
- **ZigZag:** `zigzag_advanced` measures the move from the running node to a pivot of the other
  kind with its sign, so a pivot high at or below the running low (after a gap down, say) no
  longer confirms that low or becomes a node, and likewise mirrored. The nodes of both ZigZags
  are a pivot sequence after every bar.
- **ZigZag:** on a bar that is both a pivot high and a pivot low, `zigzag_advanced` and `zigzag`
  apply the pivot of the newest node's kind first, and before the first node such a bar starts
  none. A series and its mirror image now give mirrored nodes.
- **ZigZag:** `zigzag` measures the deviation against the amount of the newest node's price, so
  zero and negative prices follow the same rule as positive ones.
- **ZigZag:** `AdvancedZigZagEngine::project_to_timeframe` and `reduce` return pivot sequences:
  the projection merges neighbouring nodes of one kind, and both drop a node on the wrong side
  of its predecessor. Details:
  [ZigZag nodes](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#zigzag-nodes).

## [0.40.0] - 2026-11-10

### Breaking

- **Elliott:** `TriangleVariant::RunningOrBarrier` is removed and `TriangleVariant::Barrier` added;
  a running triangle is now the field `TriangleValidation::running`. `CorrectionVariant` gains
  `RunningFlat`. A `match` over either enum needs the new arms.
- **Elliott:** the validation structs gain public fields, so a struct literal must set them:
  `direction` on `ImpulseValidation`, `DiagonalValidation`, `CorrectionValidation` and
  `TriangleValidation`; `guidelines` on `CorrectionValidation`; `running`, `valid` and
  `violations` on `TriangleValidation`.
- **Synthetic:** `wyckoff_schematic_bars` returns the full schematic (62 bars at the default
  lookback: decline, PS, SC, AR, ST, Spring, SOS, LPS) instead of a quiet range with four event
  bars. The seed only varies the volume of the bars without an event.
- **Wyckoff:** `WyckoffEventKind` has six new variants (`PreliminarySupport`, `PreliminarySupply`,
  `SellingClimax`, `BuyingClimax`, `AutomaticRally`, `AutomaticReaction`), so exhaustive `match`
  expressions over it need an update.
- **Wyckoff:** `WyckoffStateMachine::bias()` is `None` for a range set by the width fallback until
  a Spring or UTAD decides it; before, a set range always had a bias.
- **Wyckoff:** `min_range_bars` now is the number of bars a range must exist before a Spring or
  UTAD counts. Before, it only delayed the rule that discarded a range; that rule is gone.

### Added

- **Elliott:** `check_impulse`, `check_diagonal`, `check_correction`, `check_triangle` and
  `check_combination` return a `SequenceError` that names the node when the input is no pivot
  sequence (node count, alternation, time order, a high not above or a low not below the node
  before it). The `validate_*` functions keep their signatures and return `None` in those cases.
- **Elliott:** every validation reports its `WaveDirection`, and a falling sequence is evaluated
  as the exact mirror image of the rising one.
- **Elliott:** guidelines are reported apart from rules, in `CorrectionValidation::guidelines`
  (`GuidelineDeviation`), and never change `valid`: `zigzag_b_retrace_bound`,
  `zigzag_c_ends_beyond_a`, `flat_c_ends_beyond_a`, `flat_b_retrace_bound`.
- **Elliott:** `CorrectionOptions` takes the asserted subdivision of wave A (`wave_a`,
  `WaveSubdivision`), with which the new rules `zigzag_b_not_beyond_start_of_a` and
  `flat_b_min_retrace` can fail, and the bound of the flat guideline (`flat_b_typical_max`).
- **Elliott:** triangles are checked, not only named: `TriangleValidation` carries `valid` and
  `violations`, with three rules for contracting and barrier triangles (`triangle_*`) and six for
  expanding ones (`expanding_triangle_*`), all listed on `check_triangle`.
  `TriangleOptions::barrier_tolerance` sets how close the ends of waves b and d must lie for a
  barrier triangle.
- **Elliott:** the thresholds are public constants whose comments state their origin (rule or
  guideline of the literature, or this crate's convention), for instance `FLAT_B_MIN_RETRACE`,
  `EXPANDED_FLAT_B_MIN` and `BARRIER_LEVEL_TOLERANCE`.
- **Wyckoff:** Preliminary Support/Supply, Selling/Buying Climax and Automatic Rally/Reaction are
  events, and Phase A is an output value. A climax is a wide bar on outlier volume at the extreme
  of the window; the range is set once the counter-move after it is confirmed.
- **Wyckoff:** the output carries the calculation in `extra`: `range_high`, `range_low`, `bias`,
  `range_source`, `atr`, `volume_threshold`, `climax`, `cause_score`, and `event_<kind>` with the
  number of bars the event lies back. `volume_threshold` is absent while there is no threshold.
  `extra["atr"]` equals `extra["raw"]` of the `atr` indicator; the `value` of `atr` is that
  number in percent of the close.
- **Wyckoff:** every threshold is a parameter: `atr_len`, `volume_k`, `climax_range_atr`,
  `st_atr`, `breakout_volume`, `lps_atr` and `abandon_atr` join the three existing ones.
  `WyckoffParams`, `WyckoffStateMachine::with_params`, `params()` and `range()` expose them on
  the typed API. Details:
  [indicator reference](https://casoon.github.io/kestrel-chartkit/docs/reference/indicators/#wyckoff).

### Changed

Output values of `wyckoff` and results of the Elliott validators change, hence a minor version.

- **Applicability:** the three Wyckoff explanations (rolled, back-adjusted and broker-quoted
  series) describe the new calculation: fixed range boundaries from single extremes, distances
  in ATR, and the volume comparisons of the climax test, the Secondary Test and the Sign of
  Strength. Tiers and reasons are unchanged.
- **Elliott:** all validators, `identify_extended_wave` and `is_truncated_fifth` evaluate pivot
  sequences only. Nodes whose timestamps decrease, a high that is not above or a low that is not
  below the node before it (a leg without length included) now give `None` instead of a result.
  The end of wave C of a correction is the exception and stays the violation
  `wave_c_must_extend_past_b`.
- **Elliott:** a wave B that retraces more than 78.6 % and less than 90 % of wave A no longer makes
  a zigzag invalid. `zigzag_b_retrace_bound` moved from `violations` to `guidelines`, so
  `validate_correction` and `validate_combination` return `valid` for such sequences.
- **Elliott:** the flat variants follow where waves B and C end. `ExpandedFlat` needs a wave B of
  more than 105 % of wave A (before: at least 100 %) and a wave C beyond the end of wave A; a wave
  B beyond the start of wave A with a wave C that stops short of the end of wave A is a
  `RunningFlat`; everything else from 90 % on is a `Flat`, a wave B exactly on the start of wave A
  included.
- **Elliott:** `validate_triangle` assigns a sequence by its waves c, d and e against the waves two
  before them instead of by two boundary comparisons. A triangle whose waves b and d end within
  5 % of wave b of each other is a `Barrier`; a sequence that fits neither kind is assigned to the
  kind it breaks fewer comparisons of and is invalid, where it was `RunningOrBarrier` before.
- **Wyckoff:** the range boundaries are the climax extreme and the extreme of the counter-move,
  and the bias follows from the climax, wherever in the window it lies. The window-width
  condition (`range_atr_max`) remains as a fallback for ranges without a climax, marked by
  `range_source` = 2 and an open bias. With the default parameters the textbook schematic runs
  through Phases A to E.
- **Wyckoff:** a Secondary Test tests the climax side of the range and needs less volume and a
  narrower bar than the climax; a Sign of Strength/Weakness needs a volume outlier
  (`breakout_volume = off` restores the price-only rule); a Last Point of Support/Supply needs a
  pullback that turns.
- **Wyckoff:** Phase E ends when price closes back inside the range or a new range is set, no
  longer by the width of the window. A range in Phases A to C is abandoned by a close more than
  `abandon_atr` ATR outside it. A bar that reports an event, changes the phase or abandons a
  range never sets a range; the search for the next one starts with the bar after it.
- **Wyckoff:** without dispersion there is no volume outlier: while the MAD of the window is
  zero (more than half of its volumes are the same value) no bar is a climax bar or a Sign of
  Strength/Weakness, however high its volume. A series without volume, or with a constant one,
  therefore finds no climax and, with the defaults, reaches Phase C at most.
- **Contributors:** the `wyckoff` contract cases are replaced by the textbook accumulation, its
  mirror image, a failed breakout, a full cycle, a range without a climax, a series without
  volume and a random walk (`reference/kestrel_reference/fixtures/contract_structure.py`).
- **Contributors:** scenario fixture `scenario_elliott` with a second implementation of the
  documented rules under `reference/` (`kestrel_reference/elliott.py`), covering both directions,
  every rule alone and every threshold.

## [0.39.1] - 2026-11-10

### Changed

- **Applicability:** the two Wyckoff explanations for rolled and back-adjusted futures series say
  "Climax-Test" instead of "Klimax-Test". Tiers and reasons are unchanged.

### Fixed

- **Docs:** the comments on `WyckoffPhase::A`, `TriangleVariant` and `validate_triangle` describe
  what the code computes: the range is set from the extremes of the lookback window, and
  `RunningOrBarrier` is the collective case for boundaries that neither converge nor diverge. No
  value changes.

## [0.39.0] - 2026-11-10

### Added

- **CUSUM:** `changepoint::cusum` (re-exported at the crate root, with `CusumOutput` and `CusumState`)
  computes the two-sided CUSUM of Page (1954) over any `&[f64]`: `S⁺ = max(0, S⁺ + x - mu0 - k)`,
  `S⁻ = max(0, S⁻ - x + mu0 - k)`, with the first index where a sum exceeds `h` (strict, no reset).
  The new indicator `cusum` (`len`, `k`, `h`; defaults 20, 0.5, 4) standardizes close-to-close
  returns by a fixed calibration period of `len` returns and streams both sums (`value` = `S⁺`,
  `extra["lower"]` = `S⁻`) with the alerts `cusum_upper_alarm` and `cusum_lower_alarm`. The catalog
  grows from 132 to 133 entries.

## [0.38.0] - 2026-11-09

### Changed

Output values of three indicators change, hence a minor version.

- **Parabolic SAR:** follows Wilder's order. The SAR is first held beyond the extremes of the two
  previous bars (long: not above their lows, short: not below their highs), and only then is the
  reversal tested against the held value (strict comparison: an equal low or high does not
  reverse). Before, the reversal was tested against the unheld SAR and the hold used the previous
  and the current bar. The hold applies on every bar, with or without a reversal, so many values
  change from the third bar on, not only around reversals; reversal alerts change with them. After
  a reversal the SAR is the previous extreme point and is not capped by the current bar, so it can
  lie inside the current bar's range. The start direction is long and ends with the first reversal.
- **Chandelier Exit:** when a close satisfies both flip conditions on one bar (crossed stops: above
  the previous short stop and below the previous long stop), the direction is kept instead of
  always turning long. This tie rule is this crate's own convention. Long and short now mirror
  each other exactly under a price reflection; the start direction (long) only matters until the
  first flip. `state`, `value`, `secondary` and the flip alerts change on such bars; the stops
  themselves do not. The stop ratchet is gated by the previous close against the previous stop.
- **Adaptive Chandelier:** the same tie rule for a close that is above the previous short stop and
  below the previous long stop. On such a bar the weak-flip alerts (`bull_weak_flip`,
  `bear_weak_flip`) still fire when the body filter is not met.

### Fixed

- **Docs and reference:** wording describing the reference definitions is neutral throughout the
  source comments, the reference generator and the fixture comments. No value changes.

## [0.37.0] - 2026-11-09

### Changed

Values and warmup of three indicators change, hence a minor version.

- **Connors RSI:** follows the definition, with one deliberate deviation in the rank. Price and streak RSI are raw Wilder RSIs (no longer
  the smoothed RSI line), all three parts run from the first bar, and the return is ranked among
  the `rank_len` returns before it; `rank_len` now takes effect. Equal returns count half: the
  other conventions count only smaller ones or smaller-or-equal ones; the half keeps
  the rank symmetric. Only the rank part mirrors exactly (under `1 / close`); RSI and streak mirror
  under a price reflection. Values change,
  and the first output moves to bar `max(rsi_len + 1, streak_len + 2, rank_len + 2)` (102 with the
  defaults instead of 6).
- **RVI:** Ehlers' Relative Vigor Index: the summed 1-2-2-1 weighted close-open over the same sum
  of high-low, signal the 1-2-2-1 weighting of the RVI, instead of an average of per-bar ratios
  with an SMA(4) signal. Values change; first output with bar `period + 6` instead of `period + 3`.
- **DPO:** the current close against the SMA displaced `period/2 + 1` bars back, instead of the
  close `period/2` bars back against the current SMA. Values change; first output with bar
  `period + period/2 + 1` instead of `period`.

## [0.36.1] - 2026-11-05

### Fixed

- **MFI:** a bar with missing volume (not finite) no longer turns every later value into NaN. The
  money-flow sums skip such a bar; every window that contains it reports no output, and the
  averages restart with the first complete window. Series without missing volume give bit-identical
  values.
- **Flow exhaustion:** the raw MFI of a window with a missing volume is skipped instead of being
  fed into the smoothing.

## [0.36.0] - 2026-11-05

### Breaking

- **Applicability:** `DataRequirements` has a new field `structure_method` and
  `ApplicabilityReason` has five new variants (`Structure…`), so exhaustive struct literals and
  `match` expressions over them need an update. `Scene` stays constructible only through its
  constructor, so it is not affected.

### Added

- **Applicability:** `elliott` and `wyckoff` declare requirements of their own
  (`StructureMethod`, `method_rule_catalog`) instead of the generic session/roll/adjustment
  flags. Every finding names what the method reads from the series: roll jumps counted as waves,
  additive and proportional adjustment, provider-specific extremes, the bounded history of a
  single contract, and for Wyckoff the contract change inside the range and real turnover.
- **Applicability:** `wyckoff` declares `needs_real_volume`; its climax test is a volume outlier.
- **Viz:** `Scene::with_title`, `with_description` and `with_label_id` give `render_scene_svg` an
  accessible name: `<title>`, `<desc>`, `role="img"` and `aria-labelledby`. A scene without them
  renders byte for byte as before. Details:
  [Visualising results](https://casoon.github.io/kestrel-chartkit/docs/guides/visualising/).

### Changed

- **Applicability:** a series that is a single, never-rolled contract is now `Degraded` for
  `elliott`, because its history ends with the contract's expiry. The verdicts of all other
  indicators are unchanged and pinned by `tests/golden_reference_applicability.rs`.
- **Model:** the documentation of `ContinuityKind::StitchedBackAdjusted` (additive shift) and
  `PriceAdjustment` (proportional scaling) states which kind of adjustment the applicability
  rules assume.
- **Examples:** `export_applicability` also lists `structure_method` and `method_rules`.
- **Pages:** the project site uses the current shared theme (0.12.6).


## [0.35.0] - 2026-10-31

### Added

- **Synthetic data:** `bos_choch_swing_bars_shaped` and `SwingCandleShape` build the swing and
  break sequence of `bos_choch_swing_bars` with candle bodies and wicks. The pivot bar and the
  break bar keep their highs and lows, so the pivot and the break stay where they are.
- **Indicators:** `pivots_structure` reports a confirmed pivot in `extra` (`pivot_high`,
  `pivot_low`, and `pivot_offset`, the bars between the pivot and the confirming bar);
  `bos_choch` reports the level a break closed beyond as `broken_level`. The main value of both
  is unchanged.

### Changed

- **Website:** the [start page](https://casoon.github.io/kestrel-chartkit/) shows a recorded run
  of the `basic_indicator` example and the computed price chart with a layout for phones; the
  full quickstart lives in the
  [documentation](https://casoon.github.io/kestrel-chartkit/docs/getting-started/quickstart/).
- **Website:** the footer now follows the page language instead of always showing German text,
  and the start page carries a release teaser and an FAQ (theme 0.11.1).
