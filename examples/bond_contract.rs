//! The instrument data contract, end to end: a consumer supplies an instrument's terms, this
//! crate derives the schedule and the valuation.
//!
//! Run with `cargo run --example bond_contract`.

use kestrel_chartkit::{
    BondSpec, BusinessCalendar, BusinessDayConvention, Date, DayCountConvention, ScheduleStub,
};

/// `Date` ist bewusst eine schlichte Kalenderdarstellung ohne Formatierung; für die Ausgabe
/// genügt hier ISO-Schreibweise.
fn iso(date: &Date) -> String {
    format!("{:04}-{:02}-{:02}", date.year, date.month, date.day)
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // What the consumer owns: the terms, read from its own product master.
    let spec = BondSpec::new(
        1_000.0,
        0.05,
        2,
        Date::new(2026, 6, 15).unwrap(),
        Date::new(2031, 6, 15).unwrap(),
        DayCountConvention::Actual365Fixed,
    );

    // What the consumer also owns, but separately: the holidays of the market it settles in.
    // They change per market and year, which is why they are not part of the terms.
    let calendar = BusinessCalendar::weekends_only();

    // What this crate owns: everything that follows.
    let bond = spec.build(&calendar)?;
    let settlement = Date::new(2026, 9, 20).unwrap();
    let priced = bond.price(settlement, 0.04)?;

    println!("Kupontermine:");
    for (accrual, payment) in bond
        .schedule()
        .accrual_dates()
        .iter()
        .skip(1)
        .zip(bond.schedule().payment_dates())
    {
        println!(
            "  Abgrenzung bis {}, Zahlung am {}",
            iso(accrual),
            iso(payment)
        );
    }

    println!();
    println!("Settlement {} bei 4% Rendite:", iso(&settlement));
    println!("  Dirty  {:.4}", priced.dirty_price);
    println!("  Clean  {:.4}", priced.clean_price);
    println!("  Stückzinsen {:.4}", priced.accrued_interest);
    println!("  Modified Duration {:.4}", priced.modified_duration);
    println!("  DV01 {:.4}", priced.dv01);

    // Eine Geschäftstagsregel bewegt den Zahlungstermin, nicht die Abgrenzung.
    let adjusted = BondSpec::new(
        1_000.0,
        0.04,
        2,
        Date::new(2026, 4, 30).unwrap(),
        Date::new(2026, 10, 31).unwrap(),
        DayCountConvention::Actual365Fixed,
    )
    .with_stub(ScheduleStub::ShortFirst)
    .with_business_day_convention(BusinessDayConvention::ModifiedFollowing)
    .schedule(&calendar)?;

    println!();
    println!(
        "Fälligkeit {} fällt auf einen Samstag, gezahlt wird am {}",
        iso(adjusted.accrual_dates().last().unwrap()),
        iso(adjusted.payment_dates().last().unwrap())
    );

    Ok(())
}
