# Changelog

All notable changes to kestrel-chartkit are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and kestrel-chartkit follows
[Semantic Versioning](https://semver.org/): before 1.0, a minor version can contain breaking
changes.

This file starts after 0.34.0; earlier releases are recorded by their git tags.

## [Unreleased]

### Added

- **Trendlines:** the `trendlines` indicator draws a support line through confirmed ZigZag lows
  and a resistance line through confirmed highs, each with its channel parallel, its touches and
  its break as an event (`TrendlineEngine`, alerts `bear_support_line_break` and
  `bull_resistance_line_break`). Details:
  [Trendlines](https://casoon.github.io/kestrel-chartkit/docs/guides/structure-and-events/#trendlines).

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
