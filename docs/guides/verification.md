---
title: How entries are verified
description: What "verified" means for a catalog entry, the acceptance every entry passes, and the tests that keep it true from version to version.
order: 8
---

Every entry of the catalog carries a `verification`: `Verified`, or `Open` with the reason. The
[indicator reference](../../reference/indicators/) shows it for each entry, and
`INDICATOR_STATUS.md` in the repository lists the same marks, held against the definitions by a
test. This page explains what stands behind the mark.

## What "verified" means

An entry is verified when its output has been compared with a calculation that does not start
from the Rust code, and every difference has been resolved.

The mark says the entry computes what its documented definition says, on every bar, including
the edges: warmup, gaps, flat bars, zero volume, ties and very high price levels.

It does **not** say an indicator is useful for trading. Nor does it say that an entry the crate
defines itself (the `kestrel_` entries and the composite scores) matches some outside standard.
Those are checked against their own written definition.

## The acceptance

An entry gets its mark only after these steps. Entries accepted in earlier rounds passed the same
principle: an expected value from a second implementation or a hand-derived scenario written from
the definition, never from the Rust code.

1. **A written specification.** It covers the formula and every convention it leaves open:
   - the seed of a smoothing;
   - the divisor of a variance;
   - which bar counts in a window;
   - what happens on a tie;
   - what is published while data is missing.

   It is written from the literature and the module documentation, and it is precise enough to
   reproduce the output to the last bit of the float order where rounding matters.
2. **A second implementation in Python.** It is written from the specification by someone who has
   not seen the Rust code, in plain floats with the standard library only. Where the specification
   was ambiguous, the implementer writes down the reading they chose. Every such note is checked.
3. **A comparison run.** Both implementations run on:
   - synthetic series: waves, trends, flat stretches, price gaps, an intraday session, volume
     spikes and a series at a price level around 10⁸;
   - small constructed series for the rare branches;
   - the mirror image of each series.

   The runs cover every parameter option and the bounds. Values, extras, states, alerts and the
   bars without output must agree. Floats agree within `1e-10`, relative above 1 and absolute
   below.
4. **An outside check where one exists:**
   - an exact recalculation in rational or 50-digit decimal arithmetic;
   - the formula listings of the original publication;
   - a textbook definition;
   - an independent library.

   An outside check that disagrees is resolved like any other difference.
5. **Contracts in the repository.** The second implementation goes into `reference/`, and the
   contract cases are generated from it: the expected values on every bar, the alerts, and the
   rejected parameter maps. An entry reaches full coverage only when its cases fix values, alerts
   and configuration completely.

A difference is never settled by adjusting the expected value. It is traced to its cause:
- the specification, which then gets corrected;
- the second implementation;
- or the Rust code, which gets fixed and listed under **Fixed** in the changelog.

Acceptance has corrected the code in places such as these:
- a long/short asymmetry at a tie;
- a substitute value published where the data allows none;
- a cancellation at high price levels;
- a pattern found twice.

## Rules every entry is held to

- **Long and short mirror each other.** Reflect a series about a price level and a directional
  entry gives the mirrored reading: values negated or reflected, bullish and bearish alerts
  swapped. An entry without a direction gives the same reading. This holds at ties too.
- **No substitute values.** Where an input the calculation needs is missing or zero (no
  volume, a range of zero, too short a history), the entry publishes nothing instead of a
  placeholder such as 0.
- **Prefix invariance.** The output on a bar does not change when later bars arrive, except
  where an entry documents a repaint, such as the newest swing node.
- **Precision at any price level.** Differences of nearly equal sums are computed from
  deviations or ratios, so a series at 10⁸ keeps its digits.

## Tests that keep it true

- **Contract tests** (`tests/indicator_contracts/`). There is one coverage row per entry, and
  every case is checked bar by bar against the fixture generated from the second implementation.
- **Golden fixtures** (`tests/fixtures/`). `python3 reference/generate.py --check` rebuilds them
  and compares them byte for byte.
- **Mirror, warmup and invariance tests** (`tests/mirror_and_other_inputs.rs`,
  `tests/warmup_contract.rs`).
- **A status test.** It keeps `INDICATOR_STATUS.md`, the reference page and each definition's
  `verification` in step.

To reproduce the checks:

```bash
python3 reference/generate.py --check
cargo test --release --all-features
```

A new entry starts as `Open` with the reason and gets its mark only after the same acceptance.
