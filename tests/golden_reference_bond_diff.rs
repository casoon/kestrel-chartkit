//! Differenztests der Anleihenbewertung gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Die Fixtures entstehen offline aus einer unabhängigen Zweitimplementierung; dieser Test rechnet
//! nur gegen die Fixtures. Die Referenz bewertet über einen echten Zahlungsplan mit tatsächlichen
//! Kuponterminen, `price_bond` über ein vereinfachtes Zeitraster (Kuponanzahl aus der
//! Restlaufzeit gerundet, Zahlungen auf `i/frequency` Jahre gelegt).
//!
//! Der Test hält beides auseinander:
//!
//! * **Innerhalb der Unterstützung** — Settlement genau auf einem Kupontermin — werden Dirty
//!   Price und beide Durationen gegen die Referenz geprüft. Die Toleranzen sind nicht beliebig
//!   gewählt: Sie decken genau den Unterschied zwischen `i/frequency`-Jahren und den echten
//!   Actual/365-Bruchteilen der Kupontermine ab, der bei den geprüften Laufzeiten unter 1e-4
//!   relativ bleibt.
//! * **Außerhalb der Unterstützung** — Stückzinsen, Clean Price und Settlement zwischen zwei
//!   Kuponterminen — wird die Abweichung als Grenze festgehalten, statt sie zu verschweigen oder
//!   durch eine großzügige Toleranz als bestanden auszugeben. Diese Tests dokumentieren einen
//!   bekannten Mangel; mit echten Zahlungsplänen (Paket 12) müssen sie durch strenge Vergleiche
//!   ersetzt werden.

mod common;

use kestrel_chartkit::finance::{price_bond, BondPricingResult, Date, DayCountConvention};

const GOLDEN: &str = include_str!("fixtures/golden_bond_diff.txt");

struct Case {
    name: String,
    on_coupon_date: bool,
    face: f64,
    coupon: f64,
    frequency: u32,
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
                face: value(i, "face"),
                coupon: value(i, "coupon"),
                frequency: value(i, "frequency") as u32,
                result,
            }
        })
        .collect()
}

#[test]
fn test_dirty_price_matches_reference_when_settlement_is_on_a_coupon_date() {
    let mut checked = 0;
    for (i, case) in cases().iter().enumerate() {
        if !case.on_coupon_date {
            continue;
        }
        let reference = value(i + 1, "dirty_price");
        let deviation = (case.result.dirty_price - reference).abs() / reference.abs();
        assert!(
            deviation < 1e-4,
            "{}: Dirty Price {} weicht relativ um {deviation:.3e} von {reference} ab",
            case.name,
            case.result.dirty_price
        );
        checked += 1;
    }
    assert!(checked >= 4, "zu wenige Fälle auf Kuponterminen geprüft");
}

#[test]
fn test_durations_match_reference_when_settlement_is_on_a_coupon_date() {
    for (i, case) in cases().iter().enumerate() {
        if !case.on_coupon_date {
            continue;
        }
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
            // Relativ statt absolut, damit die Grenze nicht von der Laufzeit abhängt: die
            // gemessene Abweichung stammt aus demselben Zeitraster wie beim Preis und bleibt
            // unter 1.4e-3.
            let deviation = (ours - reference).abs() / reference.abs();
            assert!(
                deviation < 2e-3,
                "{}: {label}-Duration {ours} weicht relativ um {deviation:.3e} von {reference} ab",
                case.name
            );
        }
    }
}

/// DV01 ist definitionsgemäß `dirty_price * modified_duration * 0.0001`. Der Vergleich läuft
/// gegen die Referenzgrößen, damit die Zusammensetzung nicht unabhängig von ihnen driftet.
#[test]
fn test_dv01_follows_from_the_reference_price_and_duration() {
    for (i, case) in cases().iter().enumerate() {
        if !case.on_coupon_date {
            continue;
        }
        let reference_dv01 = value(i + 1, "dirty_price") * value(i + 1, "modified_duration") * 1e-4;
        // DV01 trägt die Abweichungen von Preis und Duration gemeinsam, deshalb die etwas
        // weitere Grenze als bei den beiden Faktoren einzeln.
        let deviation = (case.result.dv01 - reference_dv01).abs() / reference_dv01;
        assert!(
            deviation < 3e-3,
            "{}: DV01 {} weicht relativ um {deviation:.3e} von {reference_dv01} ab",
            case.name,
            case.result.dv01
        );
    }
}

/// **Bekannte Grenze.** Ohne echten Zahlungsplan kann `price_bond` nicht erkennen, dass das
/// Settlement auf einem Kupontermin liegt: Es leitet die verstrichene Kuponperiode aus der
/// Restlaufzeit ab, und die ist wegen Schaltjahren auch auf einem Kupontermin kein glattes
/// Vielfaches der Periodenlänge. Die Stückzinsen fallen dadurch um bis zu einen vollen Kupon zu
/// hoch aus, und der Clean Price entsprechend zu niedrig.
///
/// Der Test hält diese Abweichung fest, damit sie nicht stillschweigend zur Norm wird. Mit echten
/// Zahlungsplänen (Paket 12) muss er durch einen strengen Vergleich gegen die Referenz ersetzt
/// werden.
#[test]
fn test_accrued_interest_on_a_coupon_date_is_a_known_deviation() {
    let mut worst = 0.0f64;
    for (i, case) in cases().iter().enumerate() {
        if !case.on_coupon_date {
            continue;
        }
        let reference = value(i + 1, "accrued_interest");
        assert_eq!(
            reference, 0.0,
            "{}: auf einem Kupontermin sind die Stückzinsen null",
            case.name
        );
        let coupon_payment = case.face * case.coupon / case.frequency as f64;
        assert!(
            case.result.accrued_interest <= coupon_payment + 1e-9,
            "{}: Stückzinsen überschreiten einen vollen Kupon",
            case.name
        );
        worst = worst.max(case.result.accrued_interest);
    }
    assert!(
        worst > 1.0,
        "die Abweichung ist verschwunden — dieser Test muss dann durch einen strengen Vergleich \
         gegen die Referenz ersetzt werden"
    );
}

/// **Bekannte Grenze.** Zwischen zwei Kuponterminen legt das vereinfachte Raster die Zahlungen
/// weiterhin auf ganze `i/frequency`-Jahre statt auf die tatsächlich verbleibenden Bruchteile.
/// Der Dirty Price weicht dadurch um mehrere Prozent ab — deutlich mehr als auf einem
/// Kupontermin. Auch dieser Test ist mit Paket 12 durch einen strengen Vergleich zu ersetzen.
#[test]
fn test_settlement_between_coupon_dates_is_outside_the_supported_range() {
    let mut checked = 0;
    for (i, case) in cases().iter().enumerate() {
        if case.on_coupon_date {
            continue;
        }
        let reference = value(i + 1, "dirty_price");
        let deviation = (case.result.dirty_price - reference).abs() / reference.abs();
        assert!(
            deviation > 1e-3,
            "{}: die Abweichung ist verschwunden — dieser Test muss dann durch einen strengen \
             Vergleich ersetzt werden",
            case.name
        );
        assert!(
            deviation < 0.05,
            "{}: die Abweichung ist über die dokumentierte Grenze hinaus gewachsen ({deviation:.3e})",
            case.name
        );
        checked += 1;
    }
    assert!(
        checked >= 2,
        "zu wenige Fälle zwischen Kuponterminen geprüft"
    );
}

/// Innerhalb des vereinfachten Modells muss der Zusammenhang trotzdem stimmen: Ein höherer
/// Kupon bei sonst gleichen Eingaben ergibt einen höheren Preis, eine höhere Rendite einen
/// niedrigeren. Das prüft die Rechnung auch dort, wo kein Referenzwert zugeordnet werden kann.
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
