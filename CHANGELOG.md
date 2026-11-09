# Changelog

All notable changes to kestrel-chartkit are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and kestrel-chartkit follows
[Semantic Versioning](https://semver.org/): before 1.0, a minor version can contain breaking
changes.

This file starts after 0.34.0; earlier releases are recorded by their git tags.

## [Unreleased]

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
