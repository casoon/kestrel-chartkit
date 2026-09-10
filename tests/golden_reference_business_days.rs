//! Differenztests der Geschäftstagsregeln gegen unabhängig erzeugte externe Referenzwerte.
//!
//! Bisher war die Anpassungslogik nur handgerechnet belegt: Der Generator erzeugte ausschließlich
//! unangepasste Pläne. Dieser Test schließt die Lücke.
//!
//! Der Kalender ist bewusst kein Marktkalender, sondern Wochenenden plus die in der Fixture
//! ausgeschriebene Feiertagsliste. Geprüft wird die Regel, nicht fremde Feiertagsdaten — aus
//! demselben Grund, aus dem dieses Crate keine Marktkalender mitliefert. Die Liste trifft
//! Monatsenden, einen Jahreswechsel, ein Schaltjahresende und eine Kette über ein Wochenende.
//!
//! **Bei den Plänen weichen beide Seiten bewusst ab.** Der Referenz-Generator verschiebt auf
//! Wunsch die Periodengrenzen selbst; dieses Crate tut das nicht: Ein Kupon deckt einen
//! Kalenderzeitraum ab, unabhängig davon, an welchen Tagen der Zahlungsverkehr geöffnet war.
//! Angepasst wird deshalb nur die Zahlung. Die Fixture trägt beide Lesarten, und der Test hält
//! die Abweichung mit Zahlen fest, statt sie zu behaupten.

mod common;

use kestrel_chartkit::finance::{
    BusinessCalendar, BusinessDayConvention, CouponSchedule, Date, ScheduleStub, Weekday,
};

const GOLDEN: &str = include_str!("fixtures/golden_business_days.txt");

fn value(key: &str) -> f64 {
    common::golden_value(GOLDEN, key)
}

fn date(prefix: &str) -> Date {
    Date::new(
        value(&format!("{prefix}_y")) as i32,
        value(&format!("{prefix}_m")) as u32,
        value(&format!("{prefix}_d")) as u32,
    )
    .unwrap_or_else(|| panic!("ungültiges Referenzdatum unter {prefix}"))
}

/// Die vier Regeln in der Reihenfolge, in der die Fixture sie benennt.
const CONVENTIONS: [(&str, BusinessDayConvention); 4] = [
    ("unadjusted", BusinessDayConvention::Unadjusted),
    ("following", BusinessDayConvention::Following),
    (
        "modified_following",
        BusinessDayConvention::ModifiedFollowing,
    ),
    ("preceding", BusinessDayConvention::Preceding),
];

fn calendar() -> BusinessCalendar {
    let count = value("cal_holiday_count") as usize;
    BusinessCalendar::with_holidays((0..count).map(|i| date(&format!("cal_holiday{i}"))))
}

fn probe_count() -> usize {
    value("probes_probe_count") as usize
}

#[test]
fn test_weekdays_match_the_reference_calendar() {
    for probe in 0..probe_count() {
        let day = date(&format!("probes_probe{probe}"));
        let expected = match value(&format!("probes_probe{probe}_weekday")) as u32 {
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            6 => Weekday::Saturday,
            7 => Weekday::Sunday,
            other => panic!("unbekannter Wochentagsschlüssel {other}"),
        };
        assert_eq!(
            day.weekday(),
            expected,
            "Wochentag von {day:?} weicht von der Referenz ab"
        );
    }
}

#[test]
fn test_business_days_match_the_reference_calendar() {
    let calendar = calendar();
    for probe in 0..probe_count() {
        let day = date(&format!("probes_probe{probe}"));
        let expected = value(&format!("probes_probe{probe}_business")) > 0.5;
        assert_eq!(
            calendar.is_business_day(day),
            expected,
            "Geschäftstagseinstufung von {day:?} weicht von der Referenz ab"
        );
    }
}

#[test]
fn test_every_convention_adjusts_as_the_reference_does() {
    let calendar = calendar();
    let probes = probe_count();
    assert!(probes >= 30, "die Probenmenge muss breit genug sein");

    let mut moved = 0;
    for probe in 0..probes {
        let day = date(&format!("probes_probe{probe}"));
        for (name, convention) in CONVENTIONS {
            let expected = date(&format!("probes_probe{probe}_{name}"));
            assert_eq!(
                calendar.adjust(day, convention),
                expected,
                "{name} auf {day:?} weicht von der Referenz ab"
            );
            if expected != day {
                moved += 1;
            }
        }
    }
    assert!(
        moved >= 60,
        "der Vergleich muss überwiegend Fälle enthalten, in denen sich das Datum bewegt, \
         sonst prüft er nur Unadjusted; bewegte Fälle: {moved}"
    );
}

#[test]
fn test_unadjusted_leaves_holidays_where_they_are() {
    let calendar = calendar();
    let count = value("cal_holiday_count") as usize;
    for index in 0..count {
        let holiday = date(&format!("cal_holiday{index}"));
        assert!(
            !calendar.is_business_day(holiday),
            "{holiday:?} sollte als Feiertag gelten"
        );
        assert_eq!(
            calendar.adjust(holiday, BusinessDayConvention::Unadjusted),
            holiday,
            "Unadjusted darf auch einen Feiertag nicht verschieben"
        );
    }
}

#[test]
fn test_modified_following_stays_inside_the_month() {
    let calendar = calendar();
    let mut rolled_back = 0;
    for probe in 0..probe_count() {
        let day = date(&format!("probes_probe{probe}"));
        let adjusted = calendar.adjust(day, BusinessDayConvention::ModifiedFollowing);
        assert_eq!(
            (adjusted.year, adjusted.month),
            (day.year, day.month),
            "ModifiedFollowing darf den Monat nicht verlassen: {day:?} -> {adjusted:?}"
        );
        if adjusted < day {
            rolled_back += 1;
        }
    }
    assert!(
        rolled_back >= 5,
        "die Fixture muss Monatsenden enthalten, an denen zurückgerollt wird; gefunden: \
         {rolled_back}"
    );
}

fn schedule_case(case: usize) -> CouponSchedule {
    let prefix = format!("sched{case}");
    let convention = CONVENTIONS[value(&format!("{prefix}_convention")) as usize].1;
    CouponSchedule::generate(
        date(&format!("{prefix}_issue")),
        date(&format!("{prefix}_maturity")),
        value(&format!("{prefix}_frequency")) as u32,
        ScheduleStub::ShortFirst,
        convention,
        &calendar(),
    )
    .expect("gültiger Zahlungsplan")
}

#[test]
fn test_accrual_boundaries_stay_unadjusted() {
    let cases = value("meta_schedule_case_count") as usize;
    for case in 1..=cases {
        let prefix = format!("sched{case}");
        let schedule = schedule_case(case);
        let expected_count = value(&format!("{prefix}_accrual_count")) as usize;
        assert_eq!(
            schedule.accrual_dates().len(),
            expected_count,
            "Plan {case}: Anzahl der Periodengrenzen weicht ab"
        );
        for (index, actual) in schedule.accrual_dates().iter().enumerate() {
            assert_eq!(
                *actual,
                date(&format!("{prefix}_acc{index}")),
                "Plan {case}: Periodengrenze {index} weicht ab"
            );
        }
    }
}

#[test]
fn test_payment_dates_are_the_adjusted_accrual_ends() {
    let cases = value("meta_schedule_case_count") as usize;
    assert!(cases >= 4, "jede Regel muss mindestens einmal vorkommen");
    for case in 1..=cases {
        let prefix = format!("sched{case}");
        let schedule = schedule_case(case);
        for (index, actual) in schedule.payment_dates().iter().enumerate() {
            assert_eq!(
                *actual,
                date(&format!("{prefix}_pay{index}")),
                "Plan {case}: Zahltag {index} weicht von der Referenz ab"
            );
        }
    }
}

/// Die Abweichung, die dieses Crate bewusst in Kauf nimmt, mit Zahlen statt als Behauptung.
///
/// Wird der Referenz-Generator selbst mit der Regel aufgerufen, verschiebt er die Periodengrenzen
/// mit. Dieses Crate lässt sie stehen. Der Test hält fest, dass die Abweichung ausschließlich in
/// den Periodengrenzen liegt — die Zahltermine stimmen weiterhin überein — und dass sie in
/// mindestens einem Plan tatsächlich auftritt, damit die Zusage nicht leer läuft.
#[test]
fn test_adjusted_generation_differs_only_in_the_accrual_grid() {
    let cases = value("meta_schedule_case_count") as usize;
    let mut cases_with_divergence = 0;

    for case in 1..=cases {
        let prefix = format!("sched{case}");
        let schedule = schedule_case(case);
        let mut diverging = 0;

        for (index, accrual) in schedule.accrual_dates().iter().enumerate() {
            let fully_adjusted = date(&format!("{prefix}_adj{index}"));
            if *accrual != fully_adjusted {
                diverging += 1;
                // Wo die Grenzen auseinanderlaufen, muss der Zahltag der angepassten Grenze
                // entsprechen: Die Abweichung sitzt in der Abgrenzung, nicht im Geldfluss.
                if index > 0 {
                    assert_eq!(
                        schedule.payment_dates()[index - 1],
                        fully_adjusted,
                        "Plan {case}: Zahltag {} und angepasste Grenze müssten zusammenfallen",
                        index - 1
                    );
                }
            }
        }
        if diverging > 0 {
            cases_with_divergence += 1;
        }
    }

    assert!(
        cases_with_divergence >= 2,
        "die Fixture muss Pläne enthalten, in denen sich die beiden Lesarten unterscheiden; \
         gefunden: {cases_with_divergence}"
    );
}
