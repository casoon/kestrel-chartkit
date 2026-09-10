//! Independent golden-reference and analytical hand-calculation tests for European vanilla
//! option pricing, Black-Scholes-Merton, Black-76, Greeks, implied volatility solver, and
//! put-call parity.

use kestrel_chartkit::option::{
    black_76, black_scholes_merton, implied_volatility, verify_put_call_parity, BlackScholesInputs,
    OptionError, OptionType,
};

#[test]
fn test_golden_black_scholes_analytical_reference() {
    // At-the-money reference case:
    // Spot S = 100.0, Strike K = 100.0 (ATM)
    // Time T = 1.0 year, Risk-free rate r = 0.05, Dividend yield q = 0.0
    // Volatility sigma = 0.20
    //
    // Hand calculation:
    // d1 = (ln(100/100) + (0.05 + 0.5 * 0.04) * 1) / (0.2 * 1) = 0.07 / 0.2 = 0.35
    // d2 = d1 - 0.2 = 0.15
    // N(0.35) = 0.63683065
    // N(0.15) = 0.55961769
    // N(-0.35) = 0.36316935
    // N(-0.15) = 0.44038231
    // e^(-0.05) = 0.95122942
    // Call = 100 * N(0.35) - 100 * e^(-0.05) * N(0.15)
    //      = 63.683065 - 95.122942 * 0.55961769 = 63.683065 - 53.232483 = 10.450582
    // Put  = 100 * e^(-0.05) * N(-0.15) - 100 * N(-0.35)
    //      = 95.122942 * 0.44038231 - 36.316935 = 41.890458 - 36.316935 = 5.573523
    let inputs = BlackScholesInputs {
        spot: 100.0,
        strike: 100.0,
        time_to_expiry_years: 1.0,
        risk_free_rate: 0.05,
        dividend_yield: 0.0,
        volatility: 0.20,
    };

    let call = black_scholes_merton(OptionType::Call, &inputs).unwrap();
    let put = black_scholes_merton(OptionType::Put, &inputs).unwrap();

    // Verify analytical price
    assert!((call.price - 10.450582).abs() < 1e-4);
    assert!((put.price - 5.573523).abs() < 1e-4);

    // Verify intrinsic and time value
    assert_eq!(call.intrinsic_value, 0.0);
    assert_eq!(put.intrinsic_value, 0.0);
    assert!((call.time_value - call.price).abs() < 1e-12);
    assert!((put.time_value - put.price).abs() < 1e-12);

    // Verify Greeks
    // Delta Call = N(d1) = 0.63683
    assert!((call.greeks.delta - 0.63683).abs() < 1e-4);
    // Delta Put = N(d1) - 1 = -0.36317
    assert!((put.greeks.delta - (-0.36317)).abs() < 1e-4);
    // Delta Call - Delta Put == 1.0 (with q=0)
    assert!((call.greeks.delta - put.greeks.delta - 1.0).abs() < 1e-12);

    // Gamma: phi(d1) / (S * sigma * sqrt(T)) = phi(0.35) / (100 * 0.2 * 1)
    // phi(0.35) = 1/sqrt(2pi) * e^(-0.35^2 / 2) = 0.39894228 * 0.940588 = 0.37524
    // Gamma = 0.37524 / 20 = 0.018762
    assert!((call.greeks.gamma - 0.018762).abs() < 1e-4);
    assert_eq!(call.greeks.gamma, put.greeks.gamma);

    // Vega = S * sqrt(T) * phi(d1) = 100 * 1 * 0.37524 = 37.524
    assert!((call.greeks.vega - 37.524).abs() < 1e-2);
    assert_eq!(call.greeks.vega, put.greeks.vega);
}

#[test]
fn test_golden_put_call_parity_exact() {
    // Acceptance criterion: Put-Call-Parität
    // C - P = S * e^(-q * T) - K * e^(-r * T)
    let inputs = BlackScholesInputs {
        spot: 120.0,
        strike: 115.0,
        time_to_expiry_years: 0.5,
        risk_free_rate: 0.04,
        dividend_yield: 0.015,
        volatility: 0.25,
    };

    let call = black_scholes_merton(OptionType::Call, &inputs).unwrap();
    let put = black_scholes_merton(OptionType::Put, &inputs).unwrap();

    let discrepancy = verify_put_call_parity(
        call.price,
        put.price,
        inputs.spot,
        inputs.strike,
        inputs.time_to_expiry_years,
        inputs.risk_free_rate,
        inputs.dividend_yield,
    );

    assert!(
        discrepancy.abs() < 1e-10,
        "Put-Call-Parity violated: discrepancy = {discrepancy}"
    );
}

#[test]
fn test_golden_black_76_futures_option() {
    // Black-76 case for commodity / index future option
    // Forward F = 50.0, Strike K = 50.0, T = 0.25, r = 0.03, vol = 0.30
    let res_call = black_76(OptionType::Call, 50.0, 50.0, 0.25, 0.03, 0.30).unwrap();
    let res_put = black_76(OptionType::Put, 50.0, 50.0, 0.25, 0.03, 0.30).unwrap();

    // Futures put-call parity: C - P = (F - K) * e^(-r * T)
    // At ATM (F == K), Call price must equal Put price!
    assert!((res_call.price - res_put.price).abs() < 1e-10);
    assert!(res_call.price > 0.0);
}

#[test]
fn test_golden_implied_volatility_solver_exact_inversion() {
    // Acceptance criterion: toleranzkontrollierter Solver rekonstruiert Volatilität
    let target_vol = 0.285; // 28.5%
    let inputs = BlackScholesInputs {
        spot: 150.0,
        strike: 155.0,
        time_to_expiry_years: 0.75,
        risk_free_rate: 0.035,
        dividend_yield: 0.01,
        volatility: target_vol,
    };

    let call = black_scholes_merton(OptionType::Call, &inputs).unwrap();
    let solved_vol = implied_volatility(
        OptionType::Call,
        call.price,
        inputs.spot,
        inputs.strike,
        inputs.time_to_expiry_years,
        inputs.risk_free_rate,
        inputs.dividend_yield,
    )
    .unwrap();

    assert!(
        (solved_vol - target_vol).abs() < 1e-6,
        "IV inversion failed: solved={solved_vol}, expected={target_vol}"
    );

    // Put IV inversion
    let put = black_scholes_merton(OptionType::Put, &inputs).unwrap();
    let solved_put_vol = implied_volatility(
        OptionType::Put,
        put.price,
        inputs.spot,
        inputs.strike,
        inputs.time_to_expiry_years,
        inputs.risk_free_rate,
        inputs.dividend_yield,
    )
    .unwrap();

    assert!(
        (solved_put_vol - target_vol).abs() < 1e-6,
        "Put IV inversion failed: solved={solved_put_vol}, expected={target_vol}"
    );
}

#[test]
fn test_golden_greeks_finite_difference_quotients() {
    // Acceptance criterion: "Greeks gegen Differenzenquotienten"
    let inputs = BlackScholesInputs {
        spot: 100.0,
        strike: 100.0,
        time_to_expiry_years: 0.5,
        risk_free_rate: 0.05,
        dividend_yield: 0.02,
        volatility: 0.20,
    };

    let res = black_scholes_merton(OptionType::Call, &inputs).unwrap();
    let eps = 1e-4;

    // Delta: (C(S + eps) - C(S - eps)) / (2 * eps)
    let mut inputs_up = inputs.clone();
    inputs_up.spot += eps;
    let price_up = black_scholes_merton(OptionType::Call, &inputs_up)
        .unwrap()
        .price;

    let mut inputs_down = inputs.clone();
    inputs_down.spot -= eps;
    let price_down = black_scholes_merton(OptionType::Call, &inputs_down)
        .unwrap()
        .price;

    let num_delta = (price_up - price_down) / (2.0 * eps);
    assert!(
        (res.greeks.delta - num_delta).abs() < 1e-4,
        "Delta Greek mismatch: analytical={}, numerical={}",
        res.greeks.delta,
        num_delta
    );

    // Gamma: (C(S + eps) - 2 * C(S) + C(S - eps)) / eps^2
    let num_gamma = (price_up - 2.0 * res.price + price_down) / (eps * eps);
    assert!(
        (res.greeks.gamma - num_gamma).abs() < 1e-4,
        "Gamma Greek mismatch: analytical={}, numerical={}",
        res.greeks.gamma,
        num_gamma
    );

    // Vega: (C(sigma + eps) - C(sigma - eps)) / (2 * eps)
    let mut inputs_v_up = inputs.clone();
    inputs_v_up.volatility += eps;
    let price_v_up = black_scholes_merton(OptionType::Call, &inputs_v_up)
        .unwrap()
        .price;

    let mut inputs_v_down = inputs.clone();
    inputs_v_down.volatility -= eps;
    let price_v_down = black_scholes_merton(OptionType::Call, &inputs_v_down)
        .unwrap()
        .price;

    let num_vega = (price_v_up - price_v_down) / (2.0 * eps);
    assert!(
        (res.greeks.vega - num_vega).abs() < 1e-4,
        "Vega Greek mismatch: analytical={}, numerical={}",
        res.greeks.vega,
        num_vega
    );
}

#[test]
fn test_golden_option_expiry_boundary_and_arbitrage_checks() {
    // Acceptance criterion: ungültige Optionspreise und Verfall (T=0)
    let expired_inputs = BlackScholesInputs {
        spot: 105.0,
        strike: 100.0,
        time_to_expiry_years: 0.0, // At expiry
        risk_free_rate: 0.05,
        dividend_yield: 0.0,
        volatility: 0.20,
    };

    let call_exp = black_scholes_merton(OptionType::Call, &expired_inputs).unwrap();
    let put_exp = black_scholes_merton(OptionType::Put, &expired_inputs).unwrap();

    // At expiration: Call = max(105 - 100, 0) = 5.0, Put = 0.0
    assert_eq!(call_exp.price, 5.0);
    assert_eq!(call_exp.intrinsic_value, 5.0);
    assert_eq!(call_exp.time_value, 0.0);
    assert_eq!(call_exp.greeks.delta, 1.0);
    assert_eq!(call_exp.greeks.vega, 0.0);

    assert_eq!(put_exp.price, 0.0);
    assert_eq!(put_exp.intrinsic_value, 0.0);
    assert_eq!(put_exp.time_value, 0.0);

    // Arbitrage bounds failure in IV solver: Market price below intrinsic
    let below_intrinsic_err = implied_volatility(
        OptionType::Call,
        2.0, // intrinsic is 5.0, so 2.0 is an arbitrage violation!
        105.0,
        100.0,
        0.5,
        0.05,
        0.0,
    );
    assert_eq!(below_intrinsic_err, Err(OptionError::PriceBelowIntrinsic));
}
