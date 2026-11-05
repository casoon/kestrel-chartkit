# Changelog

All notable changes to kestrel-chartkit are listed here. The format is based on
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and kestrel-chartkit follows
[Semantic Versioning](https://semver.org/): before 1.0, a minor version can contain breaking
changes.

This file starts after 0.34.0; earlier releases are recorded by their git tags.

## [Unreleased]

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
