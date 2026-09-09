//! Independent golden-reference and hand-calculation tests for rolling beta,
//! market breadth snapshots, pair spreads, and signal redundancy correlation matrices
//! per plan/06-marktuebergreifende-analysen.md and CLAUDE.md.

use kestrel_chartkit::cross_asset::{
    compute_market_breadth, compute_pair_spread, compute_rolling_beta,
    compute_signal_correlation_matrix, UniverseMemberObservation,
};

#[test]
fn test_golden_rolling_beta_self_and_leveraged() {
    // Acceptance criterion: "Identische Reihen ergeben erwartetes Beta"
    // Asset returns: [0.01, -0.02, 0.03, -0.01, 0.02]
    let bench = [0.01, -0.02, 0.03, -0.01, 0.02];

    // Case 1: Identical series against itself -> Beta = 1.0, Alpha = 0.0, R^2 = 1.0, Corr = 1.0
    let res_self = compute_rolling_beta(&bench, &bench).unwrap();
    assert!((res_self.beta - 1.0).abs() < 1e-12);
    assert!(res_self.alpha.abs() < 1e-12);
    assert!((res_self.r_squared - 1.0).abs() < 1e-12);
    assert!((res_self.correlation - 1.0).abs() < 1e-12);
    assert_eq!(res_self.periods, 5);

    // Case 2: Leveraged 2x asset: [0.02, -0.04, 0.06, -0.02, 0.04]
    let asset_2x = [0.02, -0.04, 0.06, -0.02, 0.04];
    let res_2x = compute_rolling_beta(&asset_2x, &bench).unwrap();
    assert!((res_2x.beta - 2.0).abs() < 1e-12);
    assert!(res_2x.alpha.abs() < 1e-12);
    assert!((res_2x.r_squared - 1.0).abs() < 1e-12);
    assert!((res_2x.correlation - 1.0).abs() < 1e-12);

    // Case 3: Inverse -1x asset: [-0.01, 0.02, -0.03, 0.01, -0.02]
    let asset_inv = [-0.01, 0.02, -0.03, 0.01, -0.02];
    let res_inv = compute_rolling_beta(&asset_inv, &bench).unwrap();
    assert!((res_inv.beta - (-1.0)).abs() < 1e-12);
    assert!(res_inv.alpha.abs() < 1e-12);
    assert!((res_inv.r_squared - 1.0).abs() < 1e-12);
    assert!((res_inv.correlation - (-1.0)).abs() < 1e-12);
}

#[test]
fn test_golden_rolling_beta_hand_calculation() {
    // Hand calculation:
    // Asset Y = [0.02, 0.04, -0.01, 0.03]
    // Benchmark X = [0.01, 0.02, -0.02, 0.01]
    // N = 4
    // Mean X = (0.01 + 0.02 - 0.02 + 0.01) / 4 = 0.02 / 4 = 0.005
    // Mean Y = (0.02 + 0.04 - 0.01 + 0.03) / 4 = 0.08 / 4 = 0.020
    //
    // dX = [0.005, 0.015, -0.025, 0.005]
    // dY = [0.000, 0.020, -0.030, 0.010]
    //
    // dX * dY = [0.000, 0.000300, 0.000750, 0.000050] -> Cov = 0.001100
    // dX^2 = [0.000025, 0.000225, 0.000625, 0.000025] -> Var(X) = 0.000900
    // Beta = Cov / Var(X) = 0.001100 / 0.000900 = 11 / 9 = 1.222222222222...
    // Alpha = Mean Y - Beta * Mean X = 0.020 - (11/9) * 0.005 = 0.020 - 0.006111111111 = 0.013888888889...
    let y = [0.02, 0.04, -0.01, 0.03];
    let x = [0.01, 0.02, -0.02, 0.01];

    let res = compute_rolling_beta(&y, &x).unwrap();

    let expected_beta = 11.0 / 9.0;
    let expected_alpha = 0.020 - expected_beta * 0.005;

    assert!((res.beta - expected_beta).abs() < 1e-12);
    assert!((res.alpha - expected_alpha).abs() < 1e-12);
}

#[test]
fn test_golden_market_breadth_universe_changes_and_missing_members() {
    // Acceptance criterion: "Universe-Wechsel und fehlende Titel verändern Nenner korrekt"
    // Universe members at timestamp t:
    // A: return +0.02, price 105, MA 100 -> Advancing, Above MA
    // B: return -0.01, price 95,  MA 100 -> Declining, Below MA
    // C: return  0.00, price 50,  MA 45  -> Unchanged, Above MA
    // D: missing / non-finite return -> excluded from active denominator
    // E: return +0.05, price 20,  no MA -> Advancing, no MA info
    let members = vec![
        UniverseMemberObservation {
            symbol: "A".to_string(),
            period_return: 0.02,
            current_price: 105.0,
            ma_reference: Some(100.0),
        },
        UniverseMemberObservation {
            symbol: "B".to_string(),
            period_return: -0.01,
            current_price: 95.0,
            ma_reference: Some(100.0),
        },
        UniverseMemberObservation {
            symbol: "C".to_string(),
            period_return: 0.0,
            current_price: 50.0,
            ma_reference: Some(45.0),
        },
        UniverseMemberObservation {
            symbol: "D".to_string(),
            period_return: f64::NAN, // Missing observation
            current_price: 10.0,
            ma_reference: Some(12.0),
        },
        UniverseMemberObservation {
            symbol: "E".to_string(),
            period_return: 0.05,
            current_price: 20.0,
            ma_reference: None,
        },
    ];

    let breadth = compute_market_breadth(&members).unwrap();

    // Denominator should be exactly 4 active members (A, B, C, E), NOT 5!
    assert_eq!(breadth.total_active, 4);
    assert_eq!(breadth.advancing, 2); // A, E
    assert_eq!(breadth.declining, 1); // B
    assert_eq!(breadth.unchanged, 1); // C

    // Advance/Decline ratio: advancing / max(1, declining) = 2 / 1 = 2.0
    assert_eq!(breadth.advance_decline_ratio, 2.0);

    // Net advancing pct: (2 - 1) / 4 = 1 / 4 = 0.25 (25%)
    assert_eq!(breadth.net_advancing_pct, 0.25);

    // Pct above MA: Members with MA = A, B, C (3 members).
    // Above MA: A (105 > 100), C (50 > 45) -> 2 out of 3 = 66.666666667%
    assert!((breadth.pct_above_ma.unwrap() - (2.0 / 3.0)).abs() < 1e-12);
}

#[test]
fn test_golden_pair_spread_and_residual_z_score() {
    // Hand calculation for Pair Spread:
    // Asset A prices: [100.0, 102.0, 104.0, 106.0, 108.0]
    // Asset B prices: [50.0, 51.0, 52.0, 53.0, 54.0]
    // Asset A = 2 * Asset B perfectly -> Hedge ratio gamma = 2.0
    // Spread S_t = P_A - 2 * P_B = 0.0 for all t
    // Residual Z-score = 0.0
    let a = [100.0, 102.0, 104.0, 106.0, 108.0];
    let b = [50.0, 51.0, 52.0, 53.0, 54.0];

    let spread = compute_pair_spread(&a, &b).unwrap();
    assert!((spread.hedge_ratio - 2.0).abs() < 1e-12);
    assert!(spread.current_spread.abs() < 1e-12);
    assert!(spread.mean_spread.abs() < 1e-12);
    assert!(spread.std_spread.abs() < 1e-12);
    assert_eq!(spread.residual_z_score, 0.0);

    // Now test a deviation at the last step:
    // A prices: [100.0, 102.0, 104.0, 106.0, 114.0] (+6 point divergence on last bar)
    let a_div = [100.0, 102.0, 104.0, 106.0, 114.0];
    let spread_div = compute_pair_spread(&a_div, &b).unwrap();
    assert!(spread_div.residual_z_score > 1.0); // positive divergence manifests as high z-score
}

#[test]
fn test_golden_signal_correlation_matrix_detects_redundancy() {
    // Acceptance criterion: "Signal-Korrelationsmatrix ... gegen Doppelzählung ähnlicher Trend-/Momentum-Indikatoren"
    // Signal 1 (e.g. MACD Bullish Score): [0.2, 0.4, 0.6, 0.8, 0.9]
    // Signal 2 (e.g. Supertrend Bullish Score, near identical): [0.22, 0.38, 0.61, 0.82, 0.88]
    // Signal 3 (e.g. Mean Reversion Oscillator, uncorrelated or opposite): [0.9, 0.7, 0.1, -0.4, -0.8]
    let sig1 = ("MACD".to_string(), vec![0.2, 0.4, 0.6, 0.8, 0.9]);
    let sig2 = ("Supertrend".to_string(), vec![0.22, 0.38, 0.61, 0.82, 0.88]);
    let sig3 = ("MeanReversion".to_string(), vec![0.9, 0.7, 0.1, -0.4, -0.8]);

    let matrix = compute_signal_correlation_matrix(&[sig1, sig2, sig3], 0.85);

    assert_eq!(matrix.len(), 3); // (MACD, Supertrend), (MACD, MeanReversion), (Supertrend, MeanReversion)

    // Check MACD vs Supertrend
    let macd_st = matrix
        .iter()
        .find(|c| c.signal_a == "MACD" && c.signal_b == "Supertrend")
        .unwrap();
    assert!(macd_st.correlation > 0.95);
    assert!(macd_st.is_redundant); // Identified as redundant!

    // Check MACD vs MeanReversion
    let macd_mr = matrix
        .iter()
        .find(|c| c.signal_a == "MACD" && c.signal_b == "MeanReversion")
        .unwrap();
    assert!(macd_mr.correlation < -0.90);
    // Negative correlation > 0.85 in absolute magnitude is also collinear/redundant in information space
    assert!(macd_mr.is_redundant);
}
