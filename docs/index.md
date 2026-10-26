---
title: Overview
description: What kestrel-chartkit computes, where it stops, and how this documentation is organised.
order: 0
---

> **Work in progress.** kestrel-chartkit is under active development and has not reached a
> stable release. Names, parameters, outputs and whole modules can still change fundamentally
> from one version to the next. Pin a release tag, and expect to adapt when you upgrade.

kestrel-chartkit is a Rust library for technical analysis. It streams price bars through
indicators, classifies the market regime, combines indicator readings into one composite signal
and returns plain data. Everything runs in-process on data you pass in; drawing is left to a
renderer such as [chartlet](https://casoon.github.io/chartlet/) or
[kestrel-loom](https://github.com/casoon/kestrel-loom).

## What it covers

- **Indicators:** 132 entries in the registry (`catalog()`), from moving averages and oscillators
  to volume profiles and market-structure detectors. All of them implement one streaming trait,
  and each is described by one definition: parameters with defaults, ranges and meaning (see the
  [indicator reference](reference/indicators/)).
- **Market regime:** `classify_regime` sorts the current market into bullish expansion, bearish
  expansion, consolidation or transition.
- **Composite scoring:** `score_indicator` turns one indicator reading into a subscore,
  `aggregate_subscores` weighs them into a direction, a permission grade and an ATR-based risk
  plan.
- **Data for charts:** a renderer-neutral scene model (`viz::scene`) for overlays, and every
  output as plain data for [chartlet](https://casoon.github.io/chartlet/) or
  [kestrel-loom](https://github.com/casoon/kestrel-loom). See
  [Visualising results](guides/visualising/).
- **Structure and events:** confirmed pivots and zones with a known event time, and detectors
  for failed breakouts, double bottoms and tops, bottom formation and divergences that alert on
  the bar the event becomes known.
- **Analytics and evaluation:** on-demand read-outs of the current market, forward paths of
  comparable situations, and what followed a signal: excursions, R-multiples, calibration, the
  deflated Sharpe ratio and a logistic model.

The crate also contains option and bond valuation, portfolio risk and stress scenarios. This
documentation focuses on the areas above; the README and the API docs cover the rest.

## Where it stops

- **No data, no brokers.** The crate is vendor-neutral: it does not fetch market data, connect to
  exchanges or ship market calendars. Where bars and instrument terms come from is your
  application's business.
- **No strategy.** A composite signal is a rule-based summary of indicator readings. Its
  `agreement` is the share of subscores that agree, not a probability of success. Nothing in this
  crate or on this site is investment advice.
- **Pre-1.0, in development.** Root-level re-exports are the preferred API. Lower-level modules
  are public for advanced use. Nothing here is a stable foundation yet: expect breaking changes.
- **Pure computation.** The crate holds mathematics only: indicators, statistics, valuation
  formulas. It contains no credentials, no account or personal data and no
  connection to any service.

## License

Business Source License 1.1 (`BUSL-1.1`). Free for non-commercial use, including production use
in private, academic, non-profit and open-source projects that are not part of a commercial
product or service. Commercial use requires a license from the Licensor. Each version converts to
Apache-2.0 four years after publication. The [LICENSE](https://github.com/casoon/kestrel-chartkit/blob/master/LICENSE)
file is authoritative.

## How the docs are organised

- **Getting started:** add the crate and stream a first indicator.
- **Guides:** indicators, market regime, composite scoring, visualising results, structure and events,
  analytics and evaluation.
- **Reference:** overview of the public entry points, the generated list of all indicators with
  their parameters, and where the API docs live.
