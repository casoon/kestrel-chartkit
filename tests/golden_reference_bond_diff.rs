//! Differenztests der Anleihenbewertung gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Die Fixtures entstehen offline aus einer unabhängigen Zweitimplementierung; dieser Test rechnet
//! nur gegen die Fixtures. Beide Seiten bewerten über einen echten Zahlungsplan mit tatsächlichen
//! Kuponterminen, weshalb hier kein Näherungsspielraum mehr nötig ist: Preise, Stückzinsen und
//! Sensitivitäten werden mit einer Toleranz von 1e-12 relativ verglichen, also auf
//! Rechengenauigkeit.
//!
//! Eine frühere Fassung dieser Datei hielt an dieser Stelle zwei bekannte Abweichungen als
//! Unterstützungsgrenze fest — Stückzinsen auf einem Kupontermin und Settlement zwischen zwei
//! Kuponterminen. Beide sind mit den echten Zahlungsplänen verschwunden; die Grenztests sind
//! deshalb durch die strengen Vergleiche unten ersetzt.

mod common;

use kestrel_chartkit::finance::{
    price_bond, BondPricingResult, BusinessCalendar, BusinessDayConvention, CouponSchedule, Date,
    DayCountConvention, FixedRateBond, ScheduleStub,
};

const GOLDEN: &str = include_str!("fixtures/golden_bond_diff.txt");

struct Case {
    name: String,
    on_coupon_date: bool,
    result: BondPricingResult,
}

fn value(case: usize, key: &str) -> f64 {
    common::golden_value(GOLDEN, &format!("bond{case}_{key}"))
}

fn cases() -> Vec<Case> {
    let n = common::golden_value(GOLDEN, "meta_bond_case_count") as usize;
    (1..=n)
        .map(|i| {
            let settlement = Date::new(
                value(i, "settlement_y") as i32,
                value(i, "settlement_m") as u32,
                value(i, "settlement_d") as u32,
            )
            .expect("gültiges Settlement-Datum");
            let maturity = Date::new(
                value(i, "maturity_y") as i32,
                value(i, "maturity_m") as u32,
                value(i, "maturity_d") as u32,
            )
            .expect("gültiges Fälligkeitsdatum");
            let result = price_bond(
                value(i, "face"),
                value(i, "coupon"),
                value(i, "frequency") as u32,
                settlement,
                maturity,
                value(i, "ytm"),
                DayCountConvention::Actual365Fixed,
            )
            .expect("Bewertung schlug fehl");
            Case {
                name: format!("bond{i}"),
                on_coupon_date: value(i, "on_coupon_date") > 0.5,
                result,
            }
        })
        .collect()
}

/// Rechengenauigkeit, kein Näherungsspielraum: Beide Seiten diskontieren dieselben Cashflows an
/// denselben Terminen.
const TOLERANCE: f64 = 1e-12;

fn close(ours: f64, reference: f64, label: &str) {
    let allowed = TOLERANCE * reference.abs().max(1.0);
    assert!(
        (ours - reference).abs() <= allowed,
        "{label}: {ours} weicht von {reference} um {} ab (erlaubt {allowed})",
        (ours - reference).abs()
    );
}

#[test]
fn test_prices_match_reference_on_and_between_coupon_dates() {
    let (mut on_coupon, mut between) = (0, 0);
    for (i, case) in cases().iter().enumerate() {
        let n = i + 1;
        close(
            case.result.dirty_price,
            value(n, "dirty_price"),
            &format!("{} Dirty Price", case.name),
        );
        close(
            case.result.clean_price,
            value(n, "clean_price"),
            &format!("{} Clean Price", case.name),
        );
        if case.on_coupon_date {
            on_coupon += 1;
        } else {
            between += 1;
        }
    }
    assert!(
        on_coupon >= 4 && between >= 2,
        "beide Lagen müssen abgedeckt sein: {on_coupon} auf, {between} zwischen Kuponterminen"
    );
}

/// Der Fall, an dem die vorherige Fassung scheiterte: Ohne Zahlungsplan war nicht erkennbar, dass
/// ein Settlement-Datum ein Kupontermin ist, und die Stückzinsen fielen um fast einen vollen
/// Kupon zu hoch aus.
#[test]
fn test_accrued_interest_matches_reference() {
    for (i, case) in cases().iter().enumerate() {
        let reference = value(i + 1, "accrued_interest");
        if case.on_coupon_date {
            assert_eq!(
                reference, 0.0,
                "{}: auf einem Kupontermin sind die Stückzinsen null",
                case.name
            );
            assert_eq!(
                case.result.accrued_interest, 0.0,
                "{}: Stückzinsen auf einem Kupontermin",
                case.name
            );
        } else {
            assert!(
                reference > 0.0,
                "{}: Referenz erwartet Stückzinsen",
                case.name
            );
            close(
                case.result.accrued_interest,
                reference,
                &format!("{} Stückzinsen", case.name),
            );
        }
    }
}

#[test]
fn test_durations_match_reference() {
    for (i, case) in cases().iter().enumerate() {
        for (label, ours, reference) in [
            (
                "Macaulay",
                case.result.macaulay_duration,
                value(i + 1, "macaulay_duration"),
            ),
            (
                "Modified",
                case.result.modified_duration,
                value(i + 1, "modified_duration"),
            ),
        ] {
            close(ours, reference, &format!("{} {label}-Duration", case.name));
        }
    }
}

/// DV01 ist definitionsgemäß `dirty_price * modified_duration * 0.0001`. Der Vergleich läuft
/// gegen die Referenzgrößen, damit die Zusammensetzung nicht unabhängig von ihnen driftet.
#[test]
fn test_dv01_follows_from_the_reference_price_and_duration() {
    for (i, case) in cases().iter().enumerate() {
        let reference_dv01 = value(i + 1, "dirty_price") * value(i + 1, "modified_duration") * 1e-4;
        close(
            case.result.dv01,
            reference_dv01,
            &format!("{} DV01", case.name),
        );
    }
}

/// Zusätzlich zur Referenz die Richtung: Ein höherer Kupon bei sonst gleichen Eingaben ergibt
/// einen höheren Preis, eine höhere Rendite einen niedrigeren.
#[test]
fn test_price_reacts_monotonically_to_coupon_and_yield() {
    let settlement = Date::new(2026, 6, 15).unwrap();
    let maturity = Date::new(2031, 6, 15).unwrap();
    let price = |coupon: f64, ytm: f64| {
        price_bond(
            1000.0,
            coupon,
            2,
            settlement,
            maturity,
            ytm,
            DayCountConvention::Actual365Fixed,
        )
        .unwrap()
        .dirty_price
    };

    assert!(price(0.06, 0.04) > price(0.05, 0.04));
    assert!(price(0.05, 0.05) < price(0.05, 0.04));
}

// --- Zahlungspläne mit ausdrücklichem Emissionsdatum ----------------------------------------

/// Fälle mit Stub-Perioden und Monatsende-Regel. Hier wird nicht nur der Preis verglichen,
/// sondern zuerst der erzeugte Zahlungsplan selbst: Stimmen die Kupontermine nicht, ist jede
/// Preisübereinstimmung Zufall.
fn scheduled_case(index: usize) -> (FixedRateBond, Date, f64) {
    let v = |key: &str| common::golden_value(GOLDEN, &format!("sched{index}_{key}"));
    let date = |prefix: &str| {
        Date::new(
            v(&format!("{prefix}_y")) as i32,
            v(&format!("{prefix}_m")) as u32,
            v(&format!("{prefix}_d")) as u32,
        )
        .expect("gültiges Datum")
    };

    let frequency = v("frequency") as u32;
    // Rückwärts ab Fälligkeit mit kurzer erster Periode; der Fall mit Vorwärtsgenerierung
    // (kurze letzte Periode) ist der zweite.
    let stub = if index == 2 {
        ScheduleStub::ShortLast
    } else {
        ScheduleStub::ShortFirst
    };
    let schedule = CouponSchedule::generate(
        date("issue"),
        date("maturity"),
        frequency,
        stub,
        BusinessDayConvention::Unadjusted,
        &BusinessCalendar::weekends_only(),
    )
    .expect("Zahlungsplan");

    let bond = FixedRateBond::new(
        v("face"),
        v("coupon"),
        frequency,
        schedule,
        DayCountConvention::Actual365Fixed,
    )
    .expect("Anleihe");

    (bond, date("settlement"), v("ytm"))
}

#[test]
fn test_generated_coupon_dates_match_reference() {
    let count = common::golden_value(GOLDEN, "meta_scheduled_case_count") as usize;
    assert!(
        count >= 4,
        "Stub- und Monatsende-Fälle müssen abgedeckt sein"
    );

    for index in 1..=count {
        let (bond, _, _) = scheduled_case(index);
        let v = |key: &str| common::golden_value(GOLDEN, &format!("sched{index}_{key}"));

        let dates = bond.schedule().accrual_dates();
        assert_eq!(
            dates.len() as f64 - 1.0,
            v("period_count"),
            "sched{index}: Anzahl der Perioden"
        );
        for (i, date) in dates.iter().enumerate() {
            let expected = Date::new(
                v(&format!("date{i}_y")) as i32,
                v(&format!("date{i}_m")) as u32,
                v(&format!("date{i}_d")) as u32,
            )
            .expect("gültiges Datum");
            assert_eq!(*date, expected, "sched{index}: Kupontermin {i}");
        }
    }
}

#[test]
fn test_scheduled_bonds_match_reference_prices_and_sensitivities() {
    let count = common::golden_value(GOLDEN, "meta_scheduled_case_count") as usize;
    for index in 1..=count {
        let (bond, settlement, ytm) = scheduled_case(index);
        let v = |key: &str| common::golden_value(GOLDEN, &format!("sched{index}_{key}"));
        let priced = bond.price(settlement, ytm).expect("Bewertung");

        for (label, ours, reference) in [
            ("Dirty Price", priced.dirty_price, v("dirty_price")),
            ("Clean Price", priced.clean_price, v("clean_price")),
            (
                "Stückzinsen",
                priced.accrued_interest,
                v("accrued_interest"),
            ),
            (
                "Macaulay-Duration",
                priced.macaulay_duration,
                v("macaulay_duration"),
            ),
            (
                "Modified Duration",
                priced.modified_duration,
                v("modified_duration"),
            ),
        ] {
            close(ours, reference, &format!("sched{index} {label}"));
        }
    }
}

/// Die Cashflows selbst, nicht nur ihr Barwert: Summe der Kupons plus Nominal, und das Nominal
/// ausschließlich in der letzten Zahlung.
#[test]
fn test_scheduled_cashflows_carry_the_nominal_only_at_maturity() {
    for index in 1..=(common::golden_value(GOLDEN, "meta_scheduled_case_count") as usize) {
        let (bond, settlement, _) = scheduled_case(index);
        let flows = bond.cashflows(settlement);
        assert!(!flows.is_empty(), "sched{index}: keine Cashflows");

        let last = flows.last().unwrap();
        assert!(
            last.amount > bond.face_value(),
            "sched{index}: die letzte Zahlung enthält das Nominal"
        );
        for flow in &flows[..flows.len() - 1] {
            assert!(
                flow.amount < bond.face_value(),
                "sched{index}: nur die letzte Zahlung enthält das Nominal"
            );
            assert!(
                flow.date > settlement,
                "sched{index}: Zahlung nach Settlement"
            );
        }
    }
}
