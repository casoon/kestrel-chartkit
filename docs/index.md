---
title: Overview
description: What kestrel-chartkit computes, where it stops, and how this documentation is organised.
order: 0
---

kestrel-chartkit is a Rust library for technical analysis. It streams price bars through
indicators, classifies the market regime, combines indicator readings into one composite signal
and renders the results as static SVG. Everything runs in-process on data you pass in.

## What it covers

- **Indicators:** 105 entries in the registry (`catalog()`), from moving averages and oscillators
  to volume profiles and market-structure detectors. All of them implement one streaming trait.
- **Market regime:** `classify_regime` sorts the current market into bullish expansion, bearish
  expansion, consolidation or transition.
- **Composite scoring:** `score_indicator` turns one indicator reading into a subscore,
  `aggregate_subscores` weighs them into a direction, a permission grade and an ATR-based risk
  plan.
- **SVG output:** `render_chart_svg` draws candles, indicator lines, zones and markers;
  `render_scene_svg` draws a multi-pane scene.

The crate also contains trade statistics, option and bond valuation, portfolio risk and stress
scenarios. This documentation focuses on the four areas above; the README and the API docs cover
the rest.

## Where it stops

- **No data, no brokers.** The crate is vendor-neutral: it does not fetch market data, connect to
  exchanges or ship market calendars. Where bars and instrument terms come from is your
  application's business.
- **No strategy.** A composite signal is a rule-based summary of indicator readings. Its
  `agreement` is the share of subscores that agree, not a probability of success. Nothing in this
  crate or on this site is investment advice.
- **Pre-1.0.** Root-level re-exports are the preferred API. Lower-level modules are public for
  advanced use and may change before 1.0.

## License

Business Source License 1.1 (`BUSL-1.1`). Free for non-commercial use, including production use
in private, academic, non-profit and open-source projects that are not part of a commercial
product or service. Commercial use requires a license from the Licensor. Each version converts to
Apache-2.0 four years after publication. The [LICENSE](https://github.com/casoon/kestrel-chartkit/blob/master/LICENSE)
file is authoritative.

## How the docs are organised

- **Getting started:** add the crate and stream a first indicator.
- **Guides:** indicators, market regime, composite scoring and SVG charts.
- **Reference:** overview of the public entry points and where the generated API docs live.
