//! Independent golden-reference and analytical hand-calculation tests for financial day-count
//! conventions, cashflow discounting, bond pricing, duration, DV01, and YTM inversion
//!.

mod common;

use kestrel_chartkit::finance::{
    discount_factor, price_bond, year_fraction, yield_to_maturity, BondSpec, BusinessCalendar,
    BusinessDayConvention, Compounding, CouponSchedule, Date, DayCountConvention, FixedRateBond,
    ScheduleStub, Weekday,
};

const GOLDEN: &str = include_str!("fixtures/golden_finance_cashflows.txt");

fn golden(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

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
    // Hand calculation: 30/360 puts the annual coupon dates exactly 1..5 years out, so
    // Macaulay = sum(t * CF_t / 1.05^t) / sum(CF_t / 1.05^t) with CF = 5, 5, 5, 5, 105.
    let flows = [(1.0, 5.0), (2.0, 5.0), (3.0, 5.0), (4.0, 5.0), (5.0, 105.0)];
    let pv = |(t, cf): (f64, f64)| cf / 1.05_f64.powf(t);
    let expected_macaulay =
        flows.iter().map(|&f| f.0 * pv(f)).sum::<f64>() / flows.iter().map(|&f| pv(f)).sum::<f64>();
    assert!((res.macaulay_duration - expected_macaulay).abs() < 1e-12);

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
    // PV = 8 / 1.04 + 108 / 1.04^2, about 107.544379; generated in golden_finance_cashflows.txt.
    common::assert_close(
        res_premium.clean_price,
        golden("bond2y_premium_clean"),
        golden("finance_cashflows_tolerance"),
        "Premium-Anleihe",
    );

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
    // PV = 2 / 1.04 + 102 / 1.04^2, about 96.227811; generated in golden_finance_cashflows.txt.
    common::assert_close(
        res_discount.clean_price,
        golden("bond2y_discount_clean"),
        golden("finance_cashflows_tolerance"),
        "Discount-Anleihe",
    );
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

// --- Paket 12: Zahlungspläne, Konventionen und Stückzinsen ----------------------------------

/// Wochentage gegen bekannte Kalendertage: Ohne diese Verankerung wäre jede Geschäftstagsregel
/// nur intern konsistent.
#[test]
fn test_weekday_matches_known_calendar_dates() {
    for (year, month, day, expected) in [
        (2026, 6, 15, Weekday::Monday),
        (2026, 10, 30, Weekday::Friday),
        (2026, 10, 31, Weekday::Saturday),
        (2026, 11, 2, Weekday::Monday),
        (2028, 2, 29, Weekday::Tuesday),
        (2026, 1, 1, Weekday::Thursday),
    ] {
        assert_eq!(
            Date::new(year, month, day).unwrap().weekday(),
            expected,
            "{year}-{month}-{day}"
        );
    }
}

/// Monatsschritte klemmen den Tag auf die Länge des Zielmonats — das ist noch nicht die
/// Monatsende-Regel, sondern nur die Vermeidung eines ungültigen Datums.
#[test]
fn test_add_months_clamps_the_day_to_the_target_month() {
    let august_31 = Date::new(2027, 8, 31).unwrap();
    assert_eq!(august_31.add_months(-6), Date::new(2027, 2, 28).unwrap());
    assert_eq!(august_31.add_months(-18), Date::new(2026, 2, 28).unwrap());
    // 2028 ist ein Schaltjahr.
    assert_eq!(august_31.add_months(6), Date::new(2028, 2, 29).unwrap());
    assert_eq!(august_31.add_months(12), Date::new(2028, 8, 31).unwrap());
}

#[test]
fn test_add_days_crosses_month_and_year_boundaries() {
    assert_eq!(
        Date::new(2026, 12, 31).unwrap().add_days(1),
        Date::new(2027, 1, 1).unwrap()
    );
    assert_eq!(
        Date::new(2027, 1, 1).unwrap().add_days(-1),
        Date::new(2026, 12, 31).unwrap()
    );
    assert_eq!(
        Date::new(2028, 2, 28).unwrap().add_days(1),
        Date::new(2028, 2, 29).unwrap()
    );
    assert_eq!(
        Date::new(2026, 1, 1).unwrap().add_days(365),
        Date::new(2027, 1, 1).unwrap()
    );
}

/// Die Monatsende-Regel greift, wenn der Anker selbst ein Monatsende ist: Aus dem 31. August
/// wird der 28./29. Februar und wieder der 31. August — nicht der 28. jedes Februars und der
/// 28. jedes August.
#[test]
fn test_month_end_rule_snaps_generated_dates_to_month_end() {
    let schedule = CouponSchedule::regular(
        Date::new(2026, 8, 31).unwrap(),
        Date::new(2028, 8, 31).unwrap(),
        2,
    )
    .unwrap();

    assert_eq!(
        schedule.accrual_dates(),
        [
            Date::new(2026, 8, 31).unwrap(),
            Date::new(2027, 2, 28).unwrap(),
            Date::new(2027, 8, 31).unwrap(),
            Date::new(2028, 2, 29).unwrap(),
            Date::new(2028, 8, 31).unwrap(),
        ]
    );
}

/// Ohne Monatsende-Anker bleibt der Tag im Monat stehen.
#[test]
fn test_schedule_without_month_end_anchor_keeps_the_day_of_month() {
    let schedule = CouponSchedule::regular(
        Date::new(2026, 6, 15).unwrap(),
        Date::new(2027, 6, 15).unwrap(),
        2,
    )
    .unwrap();
    assert_eq!(
        schedule.accrual_dates(),
        [
            Date::new(2026, 6, 15).unwrap(),
            Date::new(2026, 12, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
        ]
    );
}

/// Kurze und lange erste Periode aus derselben Emission: Bei `ShortFirst` bleibt das
/// angebrochene Stück eine eigene Periode, bei `LongFirst` verschmilzt es mit der folgenden.
#[test]
fn test_first_stub_is_kept_or_absorbed_as_selected() {
    let issue = Date::new(2026, 8, 10).unwrap();
    let maturity = Date::new(2028, 6, 15).unwrap();
    let generate = |stub| {
        CouponSchedule::generate(
            issue,
            maturity,
            2,
            stub,
            BusinessDayConvention::Unadjusted,
            &BusinessCalendar::weekends_only(),
        )
        .unwrap()
    };

    assert_eq!(
        generate(ScheduleStub::ShortFirst).accrual_dates(),
        [
            issue,
            Date::new(2026, 12, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
            Date::new(2027, 12, 15).unwrap(),
            maturity,
        ]
    );
    assert_eq!(
        generate(ScheduleStub::LongFirst).accrual_dates(),
        [
            issue,
            Date::new(2027, 6, 15).unwrap(),
            Date::new(2027, 12, 15).unwrap(),
            maturity,
        ]
    );
}

/// Geschäftstagsregeln: Der 31. Oktober 2026 ist ein Samstag. `Following` geht auf Montag, den
/// 2. November — und damit in den Folgemonat, weshalb `ModifiedFollowing` stattdessen auf
/// Freitag, den 30. Oktober zurückgeht.
#[test]
fn test_business_day_conventions_move_dates_as_documented() {
    let calendar = BusinessCalendar::weekends_only();
    let saturday = Date::new(2026, 10, 31).unwrap();

    assert_eq!(
        calendar.adjust(saturday, BusinessDayConvention::Unadjusted),
        saturday
    );
    assert_eq!(
        calendar.adjust(saturday, BusinessDayConvention::Following),
        Date::new(2026, 11, 2).unwrap()
    );
    assert_eq!(
        calendar.adjust(saturday, BusinessDayConvention::ModifiedFollowing),
        Date::new(2026, 10, 30).unwrap()
    );
    assert_eq!(
        calendar.adjust(saturday, BusinessDayConvention::Preceding),
        Date::new(2026, 10, 30).unwrap()
    );
}

/// Ein übergebener Feiertag wirkt wie ein Wochenendtag; Freitag der 25. Dezember 2026 als
/// Feiertag schiebt `Preceding` auf Donnerstag den 24.
#[test]
fn test_supplied_holidays_are_treated_as_non_business_days() {
    let christmas = Date::new(2026, 12, 25).unwrap();
    let calendar = BusinessCalendar::with_holidays([christmas]);

    assert!(!calendar.is_business_day(christmas));
    assert_eq!(
        calendar.adjust(christmas, BusinessDayConvention::Preceding),
        Date::new(2026, 12, 24).unwrap()
    );
    // Samstag 26., Sonntag 27. -> Montag 28.
    assert_eq!(
        calendar.adjust(christmas, BusinessDayConvention::Following),
        Date::new(2026, 12, 28).unwrap()
    );
}

/// Zahlungstermine bewegen sich, Abgrenzungstermine nicht: Ein Kupon deckt einen Kalenderzeitraum
/// ab, unabhängig davon, an welchen Tagen der Zahlungsverkehr geöffnet hat.
#[test]
fn test_adjustment_moves_payment_dates_but_not_accrual_dates() {
    let schedule = CouponSchedule::generate(
        Date::new(2026, 4, 30).unwrap(),
        Date::new(2026, 10, 31).unwrap(),
        2,
        ScheduleStub::ShortFirst,
        BusinessDayConvention::ModifiedFollowing,
        &BusinessCalendar::weekends_only(),
    )
    .unwrap();

    assert_eq!(
        schedule.accrual_dates().last().unwrap(),
        &Date::new(2026, 10, 31).unwrap(),
        "die Abgrenzung endet am Kalendertermin"
    );
    assert_eq!(
        schedule.payment_dates().last().unwrap(),
        &Date::new(2026, 10, 30).unwrap(),
        "gezahlt wird am vorangehenden Geschäftstag"
    );
}

/// Der Kupon folgt dem Day Count: Unter Actual/365 zahlt eine 183-Tage-Periode mehr als eine mit
/// 182 Tagen. Handrechnung: 1000 * 5% * 183/365 und 1000 * 5% * 182/365.
#[test]
fn test_coupon_amount_follows_the_day_count() {
    let bond = FixedRateBond::new(
        1000.0,
        0.05,
        2,
        CouponSchedule::regular(
            Date::new(2026, 6, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
            2,
        )
        .unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    assert!((bond.coupon_amount(0).unwrap() - 1000.0 * 0.05 * 183.0 / 365.0).abs() < 1e-12);
    assert!((bond.coupon_amount(1).unwrap() - 1000.0 * 0.05 * 182.0 / 365.0).abs() < 1e-12);

    // Unter 30/360 ist jede Halbjahresperiode exakt ein halbes Jahr, die Beträge sind gleich.
    let thirty = FixedRateBond::new(
        1000.0,
        0.05,
        2,
        CouponSchedule::regular(
            Date::new(2026, 6, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
            2,
        )
        .unwrap(),
        DayCountConvention::Thirty360,
    )
    .unwrap();
    assert!((thirty.coupon_amount(0).unwrap() - 25.0).abs() < 1e-12);
    assert!((thirty.coupon_amount(1).unwrap() - 25.0).abs() < 1e-12);
}

/// Stückzinsen sind null auf einem Kupontermin und wachsen innerhalb der Periode linear im Sinne
/// des Day Counts. Handrechnung: 1000 * 5% * 92/365 vom 15. Juni bis 15. September.
#[test]
fn test_accrued_interest_is_zero_on_a_coupon_date_and_day_counted_between() {
    let bond = FixedRateBond::new(
        1000.0,
        0.05,
        2,
        CouponSchedule::regular(
            Date::new(2026, 6, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
            2,
        )
        .unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    assert_eq!(
        bond.accrued_interest(Date::new(2026, 6, 15).unwrap()),
        0.0,
        "Emissionstag"
    );
    assert_eq!(
        bond.accrued_interest(Date::new(2026, 12, 15).unwrap()),
        0.0,
        "Kupontermin"
    );
    let mid = bond.accrued_interest(Date::new(2026, 9, 15).unwrap());
    assert!((mid - 1000.0 * 0.05 * 92.0 / 365.0).abs() < 1e-12);
}

/// Eine Zahlung am Settlement-Tag ist nicht mehr ausstehend — genau deshalb sind die Stückzinsen
/// dort null.
#[test]
fn test_cashflow_on_the_settlement_date_is_not_outstanding() {
    let bond = FixedRateBond::new(
        1000.0,
        0.05,
        2,
        CouponSchedule::regular(
            Date::new(2026, 6, 15).unwrap(),
            Date::new(2027, 6, 15).unwrap(),
            2,
        )
        .unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    let before = bond.cashflows(Date::new(2026, 12, 14).unwrap());
    let on = bond.cashflows(Date::new(2026, 12, 15).unwrap());
    assert_eq!(before.len(), 2);
    assert_eq!(on.len(), 1);
    assert_eq!(on[0].date, Date::new(2027, 6, 15).unwrap());
}

// --- Datenvertrag in Richtung Konsumenten ----------------------------------------------------

/// Aus derselben Produktspezifikation muss dieselbe Bewertung folgen wie aus dem von Hand
/// zusammengesetzten Plan — sonst wäre der Vertrag ein zweiter Rechenweg statt einer Eingabe.
#[test]
fn test_bond_spec_builds_the_same_instrument_as_manual_construction() {
    let calendar = BusinessCalendar::weekends_only();
    let issue = Date::new(2026, 6, 15).unwrap();
    let maturity = Date::new(2029, 6, 15).unwrap();
    let settlement = Date::new(2026, 9, 20).unwrap();

    let from_spec = BondSpec::new(
        1000.0,
        0.05,
        2,
        issue,
        maturity,
        DayCountConvention::Actual365Fixed,
    )
    .build(&calendar)
    .unwrap();

    let manual = FixedRateBond::new(
        1000.0,
        0.05,
        2,
        CouponSchedule::generate(
            issue,
            maturity,
            2,
            ScheduleStub::ShortFirst,
            BusinessDayConvention::Unadjusted,
            &calendar,
        )
        .unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .unwrap();

    assert_eq!(from_spec, manual);
    assert_eq!(
        from_spec.price(settlement, 0.04).unwrap(),
        manual.price(settlement, 0.04).unwrap()
    );
}

/// Stub-Lage und Geschäftstagsregel aus der Spezifikation erreichen den Plan.
#[test]
fn test_bond_spec_carries_stub_and_business_day_convention_into_the_schedule() {
    let calendar = BusinessCalendar::weekends_only();
    let spec = BondSpec::new(
        1000.0,
        0.04,
        2,
        Date::new(2026, 4, 30).unwrap(),
        Date::new(2026, 10, 31).unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .with_stub(ScheduleStub::LongFirst)
    .with_business_day_convention(BusinessDayConvention::ModifiedFollowing);

    let schedule = spec.schedule(&calendar).unwrap();
    assert_eq!(
        schedule.accrual_dates().last().unwrap(),
        &Date::new(2026, 10, 31).unwrap()
    );
    assert_eq!(
        schedule.payment_dates().last().unwrap(),
        &Date::new(2026, 10, 30).unwrap(),
        "die Geschäftstagsregel aus der Spezifikation wirkt auf den Zahlungstermin"
    );
}

/// Ein fehlerhafter Produktdatensatz fällt beim Bauen auf, nicht erst in einem Preis.
#[test]
fn test_bond_spec_rejects_invalid_product_records() {
    let calendar = BusinessCalendar::weekends_only();
    let base = |frequency, face, issue, maturity| {
        BondSpec::new(
            face,
            0.04,
            frequency,
            issue,
            maturity,
            DayCountConvention::Actual365Fixed,
        )
        .build(&calendar)
    };
    let issue = Date::new(2026, 6, 15).unwrap();
    let maturity = Date::new(2029, 6, 15).unwrap();

    assert!(base(2, 1000.0, issue, maturity).is_ok());
    // Frequenz teilt 12 nicht
    assert!(base(5, 1000.0, issue, maturity).is_err());
    // Nominal nicht positiv
    assert!(base(2, 0.0, issue, maturity).is_err());
    // Fälligkeit vor Emission
    assert!(base(2, 1000.0, maturity, issue).is_err());
}
