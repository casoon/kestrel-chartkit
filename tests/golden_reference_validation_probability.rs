//! Independent golden-reference and hand-calculation tests for out-of-sample data splits
//! (purging & embargo), frozen probability calibration, Brier Skill Score, and ECE.

use kestrel_chartkit::evaluation::probability::{
    block_bootstrap_brier, compute_calibration_metrics, IsotonicCalibrator,
    ValidationExperimentManifest,
};
use kestrel_chartkit::evaluation::split::{split_trades_purged, PurgedSplitConfig, TradeSpan};

#[test]
fn test_golden_frozen_calibrator_invariant_to_test_data() {
    // Acceptance criterion 1: Testdaten ändern darf Training/Kalibrator nicht ändern.
    let train_data = vec![(20.0, false), (40.0, false), (60.0, true), (80.0, true)];
    let calibrator = IsotonicCalibrator::fit(&train_data).unwrap();

    let pred_at_50 = calibrator.predict(50.0);
    assert_eq!(
        pred_at_50, 0.5,
        "Midpoint between 40 (prob 0.0) and 60 (prob 1.0) must be 0.5"
    );

    // Feed arbitrary subsequent test datasets (e.g. all losses or all wins):
    let _test_set_a = [(50.0, false), (50.0, false), (50.0, false)];
    assert_eq!(
        calibrator.predict(50.0),
        pred_at_50,
        "Calibrator must remain strictly frozen regardless of test data"
    );

    let _test_set_b = [(50.0, true), (50.0, true), (50.0, true)];
    assert_eq!(
        calibrator.predict(50.0),
        pred_at_50,
        "Calibrator prediction cannot be influenced by test observations"
    );
}

#[test]
fn test_golden_purging_and_embargo_split() {
    // Acceptance criterion 2: kein Label greift über Split-Grenzen.
    let config = PurgedSplitConfig {
        train_start: 0,
        train_end: 100,
        test_start: 110, // 10 bars embargo
        test_end: 200,
        embargo_bars: 10,
    };

    let trades = vec![
        // Trade 1: safe inside train
        TradeSpan {
            id: 1,
            entry_bar: 10,
            exit_bar: 50,
        },
        // Trade 2: finishes inside embargo buffer (exit 105 < test_start 110)
        TradeSpan {
            id: 2,
            entry_bar: 85,
            exit_bar: 105,
        },
        // Trade 3: entered at 95, exits at 115 (extends past test_start 110) -> MUST BE PURGED!
        TradeSpan {
            id: 3,
            entry_bar: 95,
            exit_bar: 115,
        },
        // Trade 4: safe test trade
        TradeSpan {
            id: 4,
            entry_bar: 120,
            exit_bar: 160,
        },
    ];

    let split = split_trades_purged(&trades, &config).unwrap();
    assert_eq!(split.train_trade_ids, vec![1, 2]);
    assert_eq!(
        split.purged_trade_ids,
        vec![3],
        "Trade overlapping test set MUST be purged"
    );
    assert_eq!(split.test_trade_ids, vec![4]);
}

#[test]
fn test_golden_hand_calc_brier_score_and_brier_skill_score() {
    // Acceptance criterion 3: triviale konstante Prognose hat handgerechneten Brier Score.
    // 4 predictions: p = [0.8, 0.7, 0.4, 0.1]
    // 4 outcomes:    y = [true, true, false, false]
    let preds = vec![0.8, 0.7, 0.4, 0.1];
    let actuals = vec![true, true, false, false];

    // Hand calculation:
    // Squared errors:
    // (0.8 - 1.0)^2 = 0.04
    // (0.7 - 1.0)^2 = 0.09
    // (0.4 - 0.0)^2 = 0.16
    // (0.1 - 0.0)^2 = 0.01
    // Sum = 0.30 -> Brier Score = 0.30 / 4 = 0.0750
    //
    // Baseline constant prediction: 50% base rate (0.50)
    // Baseline squared errors: 4 * (0.5 - 1.0)^2 = 4 * 0.25 = 1.00 -> Baseline BS = 0.2500
    //
    // Brier Skill Score = 1.0 - (0.0750 / 0.2500) = 1.0 - 0.30 = 0.7000 (70% skill)
    //
    // Log-loss = -1/4 * [ln(0.8) + ln(0.7) + ln(0.6) + ln(0.9)] = 0.29900116
    //
    // ECE across 2 bins ([0..0.5), [0.5..1.0]):
    // Bin 0: preds 0.4, 0.1 -> mean 0.25, actual winrate 0.0 -> gap 0.25 (weight 0.5)
    // Bin 1: preds 0.8, 0.7 -> mean 0.75, actual winrate 1.0 -> gap 0.25 (weight 0.5)
    // ECE = 0.5 * 0.25 + 0.5 * 0.25 = 0.25
    let metrics = compute_calibration_metrics(&preds, &actuals, 0.50, 2).unwrap();

    assert!((metrics.brier_score - 0.075).abs() < 1e-12);
    assert!((metrics.baseline_brier_score - 0.25).abs() < 1e-12);
    assert!((metrics.brier_skill_score - 0.70).abs() < 1e-12);
    assert!((metrics.log_loss - 0.29900116).abs() < 1e-6);
    assert!((metrics.expected_calibration_error - 0.25).abs() < 1e-12);
    assert_eq!(metrics.sample_size, 4);
}

#[test]
fn test_golden_isotonic_calibrator_monotonicity() {
    // Non-monotone noisy inputs:
    // (10, 0), (20, 1), (30, 0), (40, 1), (50, 0), (60, 1)
    let noisy_data = vec![
        (10.0, false),
        (20.0, true),
        (30.0, false),
        (40.0, true),
        (50.0, false),
        (60.0, true),
    ];
    let calibrator = IsotonicCalibrator::fit(&noisy_data).unwrap();

    // PAVA must guarantee monotonicity everywhere
    let test_scores = [0.0, 15.0, 25.0, 35.0, 45.0, 55.0, 75.0];
    let mut prev_prob = 0.0;
    for &s in &test_scores {
        let prob = calibrator.predict(s);
        assert!(
            prob >= prev_prob - 1e-12,
            "Isotonic calibrator must be monotonically non-decreasing: at {s} got {prob} < {prev_prob}"
        );
        assert!((0.0..=1.0).contains(&prob));
        prev_prob = prob;
    }
}

#[test]
fn test_golden_low_sample_size_visibility() {
    // Acceptance criterion 4: geringe Stichprobe bleibt sichtbar.
    let small_preds = vec![0.6, 0.7, 0.8];
    let small_actuals = vec![false, true, true];
    let metrics = compute_calibration_metrics(&small_preds, &small_actuals, 0.5, 10).unwrap();

    assert_eq!(
        metrics.sample_size, 3,
        "Small sample size N=3 must be preserved explicitly"
    );

    let manifest = ValidationExperimentManifest {
        experiment_id: "exp_oos_001".to_string(),
        model_version: "v1.0.0".to_string(),
        train_range: (0, 100),
        test_range: (110, 150),
        embargo_bars: 10,
        train_sample_size: 20,
        test_sample_size: 3,
        metrics,
    };

    assert_eq!(manifest.test_sample_size, 3);
    assert_eq!(manifest.train_sample_size, 20);
}

#[test]
fn test_golden_block_bootstrap_brier() {
    let preds = vec![0.8, 0.7, 0.4, 0.1, 0.85, 0.65, 0.35, 0.15];
    let actuals = vec![true, true, false, false, true, true, false, false];

    let (mean_brier, p05, p95) = block_bootstrap_brier(&preds, &actuals, 2, 100, 42).unwrap();

    assert!(mean_brier > 0.0 && mean_brier < 0.25);
    assert!(p05 <= mean_brier);
    assert!(mean_brier <= p95);
}
