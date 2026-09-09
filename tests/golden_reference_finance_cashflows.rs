//! Independent golden-reference and analytical hand-calculation tests for financial day-count
//! conventions, cashflow discounting, bond pricing, duration, DV01, and YTM inversion
//! per plan/09-finanzkonventionen-und-kurven.md and CLAUDE.md.

use kestrel_chartkit::finance::{
    discount_factor, price_bond, year_fraction, yield_to_maturity, Compounding, Date,
    DayCountConvention,
};

#[test]
fn test_golden_day_count_conventions_and_fractions() {
    // Acceptance criterion: Handgerechnete Cashflows und Day-Count-Grenzen
    // Case 1: Actual/360
    // From 2026-01-01 to 2026-04-01:
    // Jan: 30 remaining days, Feb: 28 days (2026 is non-leap), Mar: 31 days -> Total 90 days.
    let d1 = Date::new(2026, 1, 1).unwrap();
    let d2 = Date::new(2026, 4, 1).unwrap();
    assert_eq!(d1.days_until(&d2), 90);

    let tau_act360 = year_fraction(d1, d2, DayCountConvention::Actual360);
    assert!((tau_act360 - 90.0 / 360.0).abs() < 1e-12); // Exactly 0.25

    // Case 2: Actual/365Fixed
    let tau_act365 = year_fraction(d1, d2, DayCountConvention::Actual365Fixed);
    assert!((tau_act365 - 90.0 / 365.0).abs() < 1e-12);

    // Case 3: 30/360 Bond Basis
    // Same dates: 3 months of 30 days = 90 days -> 90 / 360 = 0.25
    let tau_30360 = year_fraction(d1, d2, DayCountConvention::Thirty360);
    assert!((tau_30360 - 0.25).abs() < 1e-12);

    // Month end edge case in 30/360 US (Bond Basis): 2026-02-28 to 2026-03-31
    // D1 = 28. Since D1 < 30, D2 remains 31. Days = (3 - 2)*30 + (31 - 28) = 33 days.
    let d_feb28 = Date::new(2026, 2, 28).unwrap();
    let d_mar31 = Date::new(2026, 3, 31).unwrap();
    let tau_feb_mar = year_fraction(d_feb28, d_mar31, DayCountConvention::Thirty360);
    assert!((tau_feb_mar - 33.0 / 360.0).abs() < 1e-12);

    // Case 4: Actual/Actual ISDA in leap year 2024
    let d_leap1 = Date::new(2024, 1, 1).unwrap();
    let d_leap2 = Date::new(2024, 12, 31).unwrap();
    let tau_leap = year_fraction(d_leap1, d_leap2, DayCountConvention::ActualActualISDA);
    assert!((tau_leap - 365.0 / 366.0).abs() < 1e-12);
}

#[test]
fn test_golden_discount_factors_and_negative_rates() {
    // Acceptance criterion: Negative Zinsen erlauben; positive Diskontfaktoren prüfen
    // Continuous discounting at r = -0.01 (-1%), tau = 2.0 years
    // D(tau) = exp(-(-0.01) * 2) = exp(0.02) = 1.020201340
    let df_neg = discount_factor(-0.01, 2.0, Compounding::Continuous);
    assert!((df_neg - 1.0202013400267558).abs() < 1e-12);
    assert!(df_neg > 0.0);

    // Annual compounding at r = 0.05, tau = 3.0
    // D(tau) = (1 + 0.05)^(-3) = 1 / 1.157625 = 0.8638375985
    let df_ann = discount_factor(0.05, 3.0, Compounding::Annual);
    assert!((df_ann - (1.0 / 1.157625)).abs() < 1e-12);
}

#[test]
fn test_golden_bond_pricing_par_bond_hand_calc() {
    // Analytical par bond test:
    // If coupon rate == YTM (e.g. 5% annual), settlement at coupon date,
    // then Clean Price MUST be exactly equal to Face Value (100.0).
    let settlement = Date::new(2026, 1, 1).unwrap();
    let maturity = Date::new(2031, 1, 1).unwrap(); // 5-year bond

    let res = price_bond(
        100.0,
        0.05, // 5% annual coupon
        1,    // annual frequency
        settlement,
        maturity,
        0.05, // 5% YTM
        DayCountConvention::Thirty360,
    )
    .unwrap();

    // Clean price == Face Value
    assert!((res.clean_price - 100.0).abs() < 1e-6);
    assert!((res.dirty_price - 100.0).abs() < 1e-6);
    assert_eq!(res.accrued_interest, 0.0);

    // Macaulay duration of a 5-year 5% par bond:
    // Macaulay duration is strictly less than 5.0 (approx 4.546 years).
    assert!(res.macaulay_duration > 4.50 && res.macaulay_duration < 4.60);

    // Modified duration = Macaulay / (1 + 0.05)
    let expected_mod_dur = res.macaulay_duration / 1.05;
    assert!((res.modified_duration - expected_mod_dur).abs() < 1e-12);

    // DV01 = 100 * mod_dur * 0.0001
    assert!((res.dv01 - 100.0 * res.modified_duration * 0.0001).abs() < 1e-12);
}

#[test]
fn test_golden_bond_pricing_premium_and_discount() {
    let settlement = Date::new(2026, 1, 1).unwrap();
    let maturity = Date::new(2028, 1, 1).unwrap(); // 2-year bond

    // Premium bond: coupon 8% > YTM 4% -> Clean price > 100
    let res_premium = price_bond(
        100.0,
        0.08,
        1,
        settlement,
        maturity,
        0.04,
        DayCountConvention::Thirty360,
    )
    .unwrap();
    // Hand calculation for 2-year 8% coupon at 4% YTM:
    // PV = 8 / 1.04 + 108 / 1.04^2 = 7.692308 + 99.852071 = 107.544379
    assert!((res_premium.clean_price - 107.544379).abs() < 1e-4);

    // Discount bond: coupon 2% < YTM 4% -> Clean price < 100
    let res_discount = price_bond(
        100.0,
        0.02,
        1,
        settlement,
        maturity,
        0.04,
        DayCountConvention::Thirty360,
    )
    .unwrap();
    // PV = 2 / 1.04 + 102 / 1.04^2 = 1.923077 + 94.304734 = 96.227811
    assert!((res_discount.clean_price - 96.227811).abs() < 1e-4);
}

#[test]
fn test_golden_yield_to_maturity_inversion() {
    // Acceptance criterion: Rückbewertung der Kalibrierungsinstrumente
    let settlement = Date::new(2026, 1, 1).unwrap();
    let maturity = Date::new(2036, 1, 1).unwrap(); // 10-year bond
    let target_ytm = 0.0425; // 4.25%

    // First, calculate clean price at target YTM
    let pricing = price_bond(
        1000.0,
        0.04, // 4.0% semi-annual coupon
        2,    // Semi-annual
        settlement,
        maturity,
        target_ytm,
        DayCountConvention::Thirty360,
    )
    .unwrap();

    // Now, invert clean price to find YTM
    let solved_ytm = yield_to_maturity(
        pricing.clean_price,
        1000.0,
        0.04,
        2,
        settlement,
        maturity,
        DayCountConvention::Thirty360,
    )
    .unwrap();

    assert!(
        (solved_ytm - target_ytm).abs() < 1e-6,
        "YTM solver failed: solved={solved_ytm}, expected={target_ytm}"
    );
}
