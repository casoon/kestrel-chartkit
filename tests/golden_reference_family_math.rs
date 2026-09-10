mod common;

use kestrel_chartkit::*;

const GOLDEN: &str = include_str!("fixtures/golden_family_math.txt");

fn golden(name: &str) -> f64 {
    common::golden_value(GOLDEN, name)
}
fn near(a: f64, b: f64) {
    let tolerance = golden("family_math_tolerance");
    assert!((a - b).abs() <= tolerance, "{a} != {b} ± {tolerance}");
}
fn statement(tf: u8, direction: SignalDirection, strength: f64) -> DirectionalStatement<u8> {
    DirectionalStatement {
        tf,
        direction,
        strength,
    }
}
#[test]
fn agreement_golden_and_ordered_ties() {
    use AgreementStrategy::*;
    use SignalDirection::*;
    let input = vec![
        statement(1, Bullish, 0.2),
        statement(1, Bullish, 0.3),
        statement(2, Bearish, 0.9),
    ];
    let majority = aggregate_agreement(input.clone(), &[Majority], &[]);
    assert_eq!(majority.direction, Bullish);
    near(majority.agreement, golden("majority"));
    let weighted = aggregate_agreement(input.clone(), &[WeightedByStrength], &[]);
    assert_eq!(weighted.direction, Bearish);
    near(weighted.agreement, golden("weighted"));
    assert_eq!(
        aggregate_agreement(input, &[WeightedByStrength, Majority], &[]),
        majority
    );
    let tie = vec![statement(1, Bullish, 1.), statement(2, Bearish, 1.)];
    let unresolved = aggregate_agreement(tie.clone(), &[], &[]);
    assert!(unresolved.conflict);
    near(unresolved.agreement, 0.5);
    let resolved = aggregate_agreement(tie, &[], &[2, 1]);
    assert_eq!(resolved.direction, Bearish);
    near(resolved.agreement, 1.);
    assert!(!resolved.conflict);
    let empty = aggregate_agreement::<u8>(vec![], &[], &[]);
    assert_eq!(empty.direction, Neutral);
    assert!(!empty.conflict);
    near(empty.agreement, 0.);
    let filtered = aggregate_agreement(
        vec![statement(1, Bullish, 1.), statement(99, Bearish, 100.)],
        &[TimeframeTopDown, WeightedByStrength],
        &[1],
    );
    assert_eq!(filtered.direction, Bullish);
    near(filtered.agreement, 1.);
}
#[test]
fn aligned_returns_and_rank_golden() {
    let samples = |xs: &[f64]| {
        xs.iter()
            .enumerate()
            .map(|(i, x)| CloseSample {
                timestamp: 1001 + i as i64,
                close: *x,
            })
            .collect()
    };
    let rows = vec![
        ("A".into(), samples(&[100., 110., 132., 118.8])),
        ("B".into(), samples(&[100., 90., 72., 79.2])),
    ];
    near(correlation_matrix(&rows, 3)[0].corr, golden("correlation"));
    let rank = relative_strength_ranking(&rows, 1);
    assert_eq!(rank[0].instrument, "B");
    near(rank[1].change_pct, golden("relative_strength"));
    assert!(correlation_matrix(
        &[
            ("A".into(), samples(&[1., 1., 1., 1.])),
            ("B".into(), samples(&[2., 3., 4., 5.]))
        ],
        3
    )
    .is_empty());
}
#[test]
fn price_stats_null_and_merge_golden() {
    let v = PriceStats::compute([Some(10.), Some(-2.), Some(0.), None]);
    assert_eq!(v.closed_count, 4);
    near(v.win_rate, golden("win_rate"));
    near(v.avg_pnl, golden("average"));
    near(v.total_pnl, golden("total"));
    let merged = PriceStats::merge([
        PriceStats::compute([Some(10.), Some(-2.)]),
        PriceStats::compute([Some(0.)]),
    ]);
    near(merged.avg_pnl, 8. / 3.);
    near(merged.win_rate, 1. / 3.);
    assert_eq!(PriceStats::compute([]).closed_count, 0);
}
#[test]
fn forward_outcome_long_short_and_horizon() {
    let bars = [
        PriceObservation {
            high: 103.,
            low: 99.,
            close: 102.,
        },
        PriceObservation {
            high: 104.,
            low: 97.,
            close: 101.,
        },
        PriceObservation {
            high: 108.,
            low: 100.,
            close: 106.,
        },
    ];
    let long = ForwardPriceOutcome::compute(PriceDirection::Long, 100., &bars, 3);
    assert_eq!(long.returns, vec![2., 1., 6.]);
    near(long.mfe, 8.);
    near(long.mae, 3.);
    assert_eq!((long.bars_to_mfe, long.bars_to_mae), (3, 2));
    assert_eq!(long.return_at(5), None);
    let short = ForwardPriceOutcome::compute(PriceDirection::Short, 100., &bars, 2);
    assert_eq!(short.returns, vec![-2., -1.]);
    near(short.mfe, 3.);
    near(short.mae, 4.);
    let empty = ForwardPriceOutcome::compute(PriceDirection::Long, 100., &[], 20);
    near(empty.mfe, 0.);
    assert_eq!(empty.return_at(0), None);
    let tie = ForwardPriceOutcome::compute(PriceDirection::Long, 100., &[bars[0], bars[0]], 2);
    assert_eq!(tie.bars_to_mfe, 1);
}
#[test]
fn normalized_outcome_and_excursion_proxy_are_distinct() {
    let values = [
        PriceOutcomeSample {
            entry: 100.,
            return_value: Some(2.),
            atr: Some(2.),
            mfe: 8.,
            mae: 3.,
            strength: 0.2,
        },
        PriceOutcomeSample {
            entry: 100.,
            return_value: Some(-4.),
            atr: None,
            mfe: 2.,
            mae: 5.,
            strength: 0.8,
        },
    ];
    let v = PriceOutcomeStats::compute(&values);
    near(v.hit_rate, 0.5);
    near(v.avg_ret_pct.unwrap(), -1.);
    near(v.avg_ret_atr.unwrap(), 1.);
    near(v.avg_mfe_pct, 5.);
    near(v.avg_mae_pct, 4.);
    near(v.avg_strength, 0.5);
    use kestrel_chartkit::evaluation::excursion::*;
    let v = evaluate_outcomes(
        &[SignalRef {
            ts: 3_600_001,
            direction: 1,
        }],
        &[
            TimedPriceObservation {
                ts: 3_600_001,
                high: 100.,
                low: 100.,
                close: 100.,
            },
            TimedPriceObservation {
                ts: 3_600_002,
                high: 108.,
                low: 97.,
                close: 99.,
            },
        ],
        1,
    );
    near(v.win_rate, 1.);
    near(v.avg_mfe_pct, 8.);
    assert_eq!(v.by_hour[0].hour, 1);
}

#[test]
fn streaming_excursions_match_independent_forward_fixture() {
    let mut excursions = (0.0, 0.0);
    for bar in [
        PriceObservation {
            high: 103.,
            low: 99.,
            close: 102.,
        },
        PriceObservation {
            high: 104.,
            low: 97.,
            close: 101.,
        },
        PriceObservation {
            high: 108.,
            low: 100.,
            close: 106.,
        },
    ] {
        excursions = PriceDirection::Long.update_excursions(100., bar, excursions.0, excursions.1);
    }
    near(excursions.0, 8.);
    near(excursions.1, 3.);
}
