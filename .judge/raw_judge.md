# Judge summary

- 511 evidence-backed findings · 153 advisory heuristics
- boundary rules: 0 not checked (no judge.toml)

## Evidence-backed findings: 511

| rule | count | representative location |
|---|---|---|
| duplicate-code | 389 | src/indicator/cci.rs:234 (Cci::alerts) |
| panic-in-lib | 88 | src/calendar.rs:67 (ExchangeCalendar::local_datetime) |
| suppression-debt | 19 | src/engine/market_context.rs:77 (clippy::too_many_arguments) |
| assertion-free-test | 13 | src/indicator/smart_money_structure.rs:556 (tests::test_smoke_no_panic_across_trending_bars) |
| swallowed-result | 2 | tests/composite_signal.rs:26 (test_composite_signal_generation) |

## Advisory heuristics (no verdict or score effect): 153

| rule | count | representative location |
|---|---|---|
| complexity-inflation | 76 | examples/synthetic_patterns.rs:21 (main) |
| integer-cast-risk | 40 | src/engine/acceptance_detector.rs:64 (detect_acceptance_rejection) |
| maintainability-index | 35 | src/evaluation/recorder.rs:1 (src/evaluation/recorder.rs) |
| abstraction-inflation | 1 | src/adapters.rs:56 (<InMemoryDataFeed as DataFeedAdapter>) |
| context-free-propagation | 1 | examples/basic_indicator.rs:12 (main) |

## Next steps

- cargo judge dupes clone families, grouped and prioritized
- cargo judge --details every finding and location
- cargo judge --format json full machine-readable report in .judge/judge.json
