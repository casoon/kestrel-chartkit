//! Financial day-count conventions, coupon schedules, cashflow discounting, and bond valuation.
//!
//! Day-count year fractions (Actual/360, Actual/365Fixed, 30/360 Bond Basis, Actual/Actual ISDA),
//! discounting, and fixed-rate bond valuation — clean and dirty price, accrued interest,
//! Macaulay/Modified duration, DV01 and yield inversion.
//!
//! Valuation runs over an explicit [`CouponSchedule`] rather than a derived time grid: coupon
//! dates are generated from an anchor in whole months, with the month-end rule and stub periods
//! ([`ScheduleStub`]) handled as their own cases, and payment dates optionally moved by a
//! [`BusinessDayConvention`] over a [`BusinessCalendar`]. Accrual stays on the unadjusted period
//! boundaries; only the payment dates move. That separation is what makes accrued interest exact
//! on a coupon date and correct between two of them.
//!
//! Deliberately not modelled: market holiday calendars (the caller supplies holidays — a bundled
//! list is a maintenance promise this crate cannot keep), non-Saturday/Sunday weekends, and
//! schedules whose periods are not whole months apart.

use std::fmt;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// A simple calendar date (year, month, day) in the Gregorian calendar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    /// Creates a new date, validating month (1..=12) and day (1..=days_in_month).
    pub fn new(year: i32, month: u32, day: u32) -> Option<Self> {
        if !(1..=12).contains(&month) || day < 1 {
            return None;
        }
        let days = Self::days_in_month(year, month);
        if day > days {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub fn is_leap_year(year: i32) -> bool {
        (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0)
    }

    pub fn days_in_month(year: i32, month: u32) -> u32 {
        match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 => {
                if Self::is_leap_year(year) {
                    29
                } else {
                    28
                }
            }
            _ => 0,
        }
    }

    /// Converts the date into an ordinal day count (days since 0001-01-01).
    pub fn to_day_number(&self) -> i64 {
        let mut y = self.year as i64;
        let mut m = self.month as i64;
        if m <= 2 {
            y -= 1;
            m += 12;
        }
        // Gregorian calendar formula
        (365 * y) + (y / 4) - (y / 100) + (y / 400) + ((153 * (m + 1)) / 5) + self.day as i64 - 428
    }

    /// Returns the number of actual elapsed calendar days from `self` to `other` (`other - self`).
    pub fn days_until(&self, other: &Date) -> i64 {
        other.to_day_number() - self.to_day_number()
    }

    /// The weekday this date falls on.
    pub fn weekday(&self) -> Weekday {
        match self.to_day_number().rem_euclid(7) {
            0 => Weekday::Sunday,
            1 => Weekday::Monday,
            2 => Weekday::Tuesday,
            3 => Weekday::Wednesday,
            4 => Weekday::Thursday,
            5 => Weekday::Friday,
            _ => Weekday::Saturday,
        }
    }

    /// Whether this is the last day of its month.
    pub fn is_month_end(&self) -> bool {
        self.day == Self::days_in_month(self.year, self.month)
    }

    /// Shifts by whole months, clamping the day to the length of the target month: 31 August
    /// minus six months is 28 (or 29) February, not an invalid 31 February.
    ///
    /// Clamping is not the same as the month-end rule — see [`CouponSchedule`], which applies that
    /// rule on top when the anchor date is itself a month end.
    pub fn add_months(&self, months: i32) -> Date {
        let total = self.year as i64 * 12 + (self.month as i64 - 1) + months as i64;
        let year = total.div_euclid(12) as i32;
        let month = total.rem_euclid(12) as u32 + 1;
        let day = self.day.min(Self::days_in_month(year, month));
        Date { year, month, day }
    }

    /// Shifts by whole calendar days.
    pub fn add_days(&self, days: i64) -> Date {
        // Round-trip through the ordinal day number rather than carrying month lengths by hand.
        let target = self.to_day_number() + days;
        let mut year = self.year + (days / 366) as i32 - 1;
        loop {
            let start = Date {
                year,
                month: 1,
                day: 1,
            }
            .to_day_number();
            let next = Date {
                year: year + 1,
                month: 1,
                day: 1,
            }
            .to_day_number();
            if target < start {
                year -= 1;
                continue;
            }
            if target >= next {
                year += 1;
                continue;
            }
            let mut remaining = target - start;
            for month in 1..=12u32 {
                let length = Self::days_in_month(year, month) as i64;
                if remaining < length {
                    return Date {
                        year,
                        month,
                        day: remaining as u32 + 1,
                    };
                }
                remaining -= length;
            }
            unreachable!("a year holds all its days");
        }
    }

    /// Moves to the last day of its month.
    pub fn to_month_end(&self) -> Date {
        Date {
            year: self.year,
            month: self.month,
            day: Self::days_in_month(self.year, self.month),
        }
    }
}

/// Day of the week.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Weekday {
    Sunday,
    Monday,
    Tuesday,
    Wednesday,
    Thursday,
    Friday,
    Saturday,
}

impl Weekday {
    /// Saturday and Sunday. Markets with a different weekend are not modelled here; pass those
    /// days as holidays to [`BusinessCalendar`] instead.
    pub fn is_weekend(self) -> bool {
        matches!(self, Weekday::Saturday | Weekday::Sunday)
    }
}

/// Financial day-count convention determining the fraction of a year between two dates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum DayCountConvention {
    /// Money market convention: Actual calendar days divided by 360.
    #[default]
    Actual360,
    /// Fixed year convention: Actual calendar days divided by 365.
    Actual365Fixed,
    /// 30/360 Bond Basis: `360 · (Y2 - Y1) + 30 · (M2 - M1) + (D2 - D1)` days over 360, where a
    /// start day of 31 counts as 30 and an end day of 31 counts as 30 once the start day is 30
    /// or 31. There is no separate end-of-February rule.
    Thirty360,
    /// Actual/Actual ISDA: Splits leap years and normal years proportionally.
    ActualActualISDA,
}

/// Computes the year fraction between two dates according to the chosen day-count convention.
pub fn year_fraction(d1: Date, d2: Date, convention: DayCountConvention) -> f64 {
    if d1 == d2 {
        return 0.0;
    }
    let (start, end, sign) = if d1 <= d2 {
        (d1, d2, 1.0)
    } else {
        (d2, d1, -1.0)
    };

    let fraction = match convention {
        DayCountConvention::Actual360 => start.days_until(&end) as f64 / 360.0,
        DayCountConvention::Actual365Fixed => start.days_until(&end) as f64 / 365.0,
        DayCountConvention::Thirty360 => {
            let mut d1_day = start.day;
            let mut d2_day = end.day;
            if d1_day == 31 {
                d1_day = 30;
            }
            if d2_day == 31 && d1_day >= 30 {
                d2_day = 30;
            }
            let days_360 = (end.year as i64 - start.year as i64) * 360
                + (end.month as i64 - start.month as i64) * 30
                + (d2_day as i64 - d1_day as i64);
            days_360 as f64 / 360.0
        }
        DayCountConvention::ActualActualISDA => {
            if start.year == end.year {
                let year_days = if Date::is_leap_year(start.year) {
                    366.0
                } else {
                    365.0
                };
                start.days_until(&end) as f64 / year_days
            } else {
                let end_of_first_year = Date::new(start.year, 12, 31).unwrap();
                let start_of_last_year = Date::new(end.year, 1, 1).unwrap();

                let first_year_days = if Date::is_leap_year(start.year) {
                    366.0
                } else {
                    365.0
                };
                let last_year_days = if Date::is_leap_year(end.year) {
                    366.0
                } else {
                    365.0
                };

                let days1 = start.days_until(&end_of_first_year) + 1;
                let days2 = start_of_last_year.days_until(&end);

                let middle_years = (end.year - start.year - 1).max(0) as f64;

                (days1 as f64 / first_year_days) + middle_years + (days2 as f64 / last_year_days)
            }
        }
    };

    sign * fraction
}

/// Compounding frequency convention for interest rates and discounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum Compounding {
    /// Continuous compounding: $D(\tau) = e^{-r \tau}$.
    #[default]
    Continuous,
    /// Annual compounding: $D(\tau) = (1 + r)^{-\tau}$.
    Annual,
    /// Periodic compounding $m$ times per year: $D(\tau) = (1 + r/m)^{-m \tau}$.
    Periodic(u32),
}

/// Computes the discount factor $D(\tau)$ for a given interest rate, time fraction $\tau$, and compounding method.
///
/// Supports negative interest rates; the discount factor remains strictly positive.
pub fn discount_factor(rate: f64, tau: f64, compounding: Compounding) -> f64 {
    if !rate.is_finite() || !tau.is_finite() || tau < 0.0 {
        return 0.0;
    }
    match compounding {
        Compounding::Continuous => (-rate * tau).exp(),
        Compounding::Annual => {
            if rate <= -1.0 {
                0.0
            } else {
                (1.0 + rate).powf(-tau)
            }
        }
        Compounding::Periodic(m) => {
            let m_f = m.max(1) as f64;
            let base = 1.0 + rate / m_f;
            if base <= 0.0 {
                0.0
            } else {
                base.powf(-m_f * tau)
            }
        }
    }
}

/// Which way a payment date moves when it falls on a non-business day.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum BusinessDayConvention {
    /// The date stays where the schedule put it. The default, and the right choice when the
    /// terms of the instrument do not name a rule.
    #[default]
    Unadjusted,
    /// Move forward to the next business day.
    Following,
    /// Move forward to the next business day, unless that leaves the month — then move backward.
    ModifiedFollowing,
    /// Move backward to the previous business day.
    Preceding,
}

/// Which days are not business days: weekends, plus whatever holidays the caller supplies.
///
/// No market calendars ship with this crate. A bundled holiday list is a maintenance promise —
/// holidays are announced, moved and added per market and year — and a stale list is worse than
/// no list, because it looks authoritative. Callers that need real holidays pass them in.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BusinessCalendar {
    holidays: Vec<Date>,
}

impl BusinessCalendar {
    /// Saturdays and Sundays only.
    pub fn weekends_only() -> Self {
        Self::default()
    }

    /// Saturdays, Sundays and the given dates.
    pub fn with_holidays(holidays: impl IntoIterator<Item = Date>) -> Self {
        let mut holidays: Vec<Date> = holidays.into_iter().collect();
        holidays.sort_unstable();
        holidays.dedup();
        Self { holidays }
    }

    pub fn is_business_day(&self, date: Date) -> bool {
        !date.weekday().is_weekend() && self.holidays.binary_search(&date).is_err()
    }

    /// Applies `convention` to `date`. An [`BusinessDayConvention::Unadjusted`] date is returned
    /// unchanged even if it is a holiday.
    pub fn adjust(&self, date: Date, convention: BusinessDayConvention) -> Date {
        match convention {
            BusinessDayConvention::Unadjusted => date,
            BusinessDayConvention::Following => self.roll(date, 1),
            BusinessDayConvention::Preceding => self.roll(date, -1),
            BusinessDayConvention::ModifiedFollowing => {
                let forward = self.roll(date, 1);
                if forward.month == date.month && forward.year == date.year {
                    forward
                } else {
                    self.roll(date, -1)
                }
            }
        }
    }

    fn roll(&self, date: Date, step: i32) -> Date {
        let mut current = date;
        // A run of non-business days longer than a fortnight would mean the caller declared a
        // shutdown, not a holiday; bounding the walk keeps a bad input from spinning forever.
        for _ in 0..14 {
            if self.is_business_day(current) {
                return current;
            }
            current = current.add_days(step as i64);
        }
        current
    }
}

/// Where the period that does not fit the regular frequency is placed, and whether it is shorter
/// or longer than a regular one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum ScheduleStub {
    /// Generate backward from maturity; a leftover front period stays as a short first period.
    /// The default, and by far the most common arrangement for a fixed-rate bond.
    #[default]
    ShortFirst,
    /// Generate backward from maturity; a leftover front period is absorbed into the following
    /// one, making a long first period.
    LongFirst,
    /// Generate forward from issue; a leftover final period stays as a short last period.
    ShortLast,
    /// Generate forward from issue; a leftover final period is absorbed into the preceding one,
    /// making a long last period.
    LongLast,
}

/// The coupon periods of a fixed-rate instrument: when interest accrues, and when it is paid.
///
/// Two date series, deliberately separate:
///
/// * **Accrual dates** are the period boundaries. They are never business-day adjusted, which is
///   the market convention for fixed-rate bonds: a coupon covers a calendar period regardless of
///   which days the payment system was open. Coupon amounts and accrued interest come from these.
/// * **Payment dates** are the accrual end dates after applying a [`BusinessDayConvention`] over
///   a [`BusinessCalendar`]. Money moves on these, so discounting uses them.
///
/// With [`BusinessDayConvention::Unadjusted`] the two coincide.
///
/// Generation anchors on maturity (backward) or issue (forward) and steps in whole months of
/// `12 / frequency`. The **month-end rule** applies when the anchor is the last day of its month:
/// every generated date is then moved to its own month end, so a 31 August anchor yields 28/29
/// February rather than the 28th of every February and the 31st of every August. Without that
/// rule, day clamping alone would silently shorten every second period.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct CouponSchedule {
    accrual: Vec<Date>,
    payment: Vec<Date>,
}

impl CouponSchedule {
    /// Builds a schedule between `issue` and `maturity`.
    ///
    /// Fails when the dates are out of order, when `frequency` is not a whole number of months
    /// (1, 2, 3, 4, 6 or 12 per year), or when the resulting schedule would have no period.
    pub fn generate(
        issue: Date,
        maturity: Date,
        frequency: u32,
        stub: ScheduleStub,
        convention: BusinessDayConvention,
        calendar: &BusinessCalendar,
    ) -> Result<Self, FinanceError> {
        let step = months_per_period(frequency)?;
        if issue >= maturity {
            return Err(FinanceError::InvalidInput("issue must precede maturity"));
        }

        let mut accrual = match stub {
            ScheduleStub::ShortFirst | ScheduleStub::LongFirst => {
                let mut dates = Vec::new();
                let month_end = maturity.is_month_end();
                let mut k = 0i32;
                loop {
                    let date = anchored(maturity, -(k * step), month_end);
                    dates.push(date);
                    if date <= issue {
                        break;
                    }
                    k += 1;
                }
                dates.reverse();
                // `dates[0]` is the first generated date at or before issue. A date strictly
                // before issue means the instrument starts inside a period: that front piece is
                // the stub.
                if dates[0] < issue {
                    dates[0] = issue;
                    if stub == ScheduleStub::LongFirst && dates.len() > 2 {
                        dates.remove(1);
                    }
                }
                dates
            }
            ScheduleStub::ShortLast | ScheduleStub::LongLast => {
                let mut dates = Vec::new();
                let month_end = issue.is_month_end();
                let mut k = 0i32;
                loop {
                    let date = anchored(issue, k * step, month_end);
                    dates.push(date);
                    if date >= maturity {
                        break;
                    }
                    k += 1;
                }
                if *dates.last().expect("loop pushes at least once") > maturity {
                    let last = dates.len() - 1;
                    dates[last] = maturity;
                    if stub == ScheduleStub::LongLast && dates.len() > 2 {
                        dates.remove(last - 1);
                    }
                }
                dates
            }
        };
        accrual.dedup();
        Self::from_accrual_dates(accrual, convention, calendar)
    }

    /// A regular, unadjusted schedule from `issue` to `maturity` with a short first period if the
    /// dates do not divide evenly.
    pub fn regular(issue: Date, maturity: Date, frequency: u32) -> Result<Self, FinanceError> {
        Self::generate(
            issue,
            maturity,
            frequency,
            ScheduleStub::ShortFirst,
            BusinessDayConvention::Unadjusted,
            &BusinessCalendar::weekends_only(),
        )
    }

    /// The regular, unadjusted schedule ending at `maturity` that reaches back far enough to
    /// contain `settlement`.
    ///
    /// For a bond whose issue date is not known — the common case when only settlement and
    /// maturity are given — this reconstructs the period `settlement` falls in by stepping
    /// backward from maturity. Every period is regular; there is no stub.
    pub fn covering(
        settlement: Date,
        maturity: Date,
        frequency: u32,
    ) -> Result<Self, FinanceError> {
        let step = months_per_period(frequency)?;
        if settlement >= maturity {
            return Err(FinanceError::InvalidInput(
                "settlement must precede maturity",
            ));
        }

        let month_end = maturity.is_month_end();
        let mut dates = Vec::new();
        let mut k = 0i32;
        loop {
            let date = anchored(maturity, -(k * step), month_end);
            dates.push(date);
            if date <= settlement {
                break;
            }
            k += 1;
        }
        dates.reverse();
        Self::from_accrual_dates(
            dates,
            BusinessDayConvention::Unadjusted,
            &BusinessCalendar::weekends_only(),
        )
    }

    /// Builds a schedule from explicit accrual boundaries, ascending, at least two of them.
    /// Payment dates follow from `convention` over `calendar`.
    pub fn from_accrual_dates(
        accrual: Vec<Date>,
        convention: BusinessDayConvention,
        calendar: &BusinessCalendar,
    ) -> Result<Self, FinanceError> {
        if accrual.len() < 2 {
            return Err(FinanceError::InvalidInput(
                "a schedule needs at least two accrual dates",
            ));
        }
        if accrual.windows(2).any(|w| w[0] >= w[1]) {
            return Err(FinanceError::InvalidInput(
                "accrual dates must be strictly ascending",
            ));
        }

        let payment = accrual[1..]
            .iter()
            .map(|date| calendar.adjust(*date, convention))
            .collect();
        Ok(Self { accrual, payment })
    }

    /// Period boundaries, ascending. One more entry than there are periods.
    pub fn accrual_dates(&self) -> &[Date] {
        &self.accrual
    }

    /// Payment date of each period, in order.
    pub fn payment_dates(&self) -> &[Date] {
        &self.payment
    }

    pub fn period_count(&self) -> usize {
        self.payment.len()
    }

    /// Start and end of period `index`, in accrual terms.
    pub fn period(&self, index: usize) -> Option<(Date, Date)> {
        Some((*self.accrual.get(index)?, *self.accrual.get(index + 1)?))
    }

    /// The period `date` accrues in: the one whose start is at or before `date` and whose end is
    /// strictly after it. A date on a period boundary belongs to the period starting there, so a
    /// settlement on a coupon date accrues nothing.
    pub fn period_containing(&self, date: Date) -> Option<usize> {
        (0..self.period_count()).find(|&i| self.accrual[i] <= date && date < self.accrual[i + 1])
    }
}

/// Whole months between two coupon dates for a given yearly frequency.
fn months_per_period(frequency: u32) -> Result<i32, FinanceError> {
    match frequency {
        1 | 2 | 3 | 4 | 6 | 12 => Ok((12 / frequency) as i32),
        _ => Err(FinanceError::InvalidInput(
            "frequency must divide 12 evenly (1, 2, 3, 4, 6 or 12)",
        )),
    }
}

/// `anchor` shifted by `months`, with the month-end rule applied when the anchor is a month end.
fn anchored(anchor: Date, months: i32, month_end: bool) -> Date {
    let shifted = anchor.add_months(months);
    if month_end {
        shifted.to_month_end()
    } else {
        shifted
    }
}

/// One dated payment of an instrument.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Cashflow {
    /// When the money moves — the business-day adjusted date.
    pub date: Date,
    pub amount: f64,
}

/// Result of evaluating a fixed-coupon bond.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BondPricingResult {
    /// Dirty (full) price including accrued interest.
    pub dirty_price: f64,
    /// Clean price excluding accrued interest (`dirty_price - accrued_interest`).
    pub clean_price: f64,
    /// Accrued interest earned since the last coupon date.
    pub accrued_interest: f64,
    /// Macaulay duration in years: weighted average maturity of cashflows.
    pub macaulay_duration: f64,
    /// Modified duration: percentage price change per 100 bp change in yield.
    pub modified_duration: f64,
    /// Dollar value of a 1 basis point (0.01% = 0.0001) yield decrease: `dirty_price * modified_duration * 0.0001`.
    pub dv01: f64,
}

/// Errors originating from finance or bond pricing calculations.
#[derive(Debug, Clone, PartialEq)]
pub enum FinanceError {
    InvalidInput(&'static str),
    SolverFailedToConverge,
}

impl fmt::Display for FinanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidInput(msg) => write!(f, "invalid finance input: {msg}"),
            Self::SolverFailedToConverge => {
                write!(f, "yield to maturity solver failed to converge")
            }
        }
    }
}

impl std::error::Error for FinanceError {}

/// A fixed-rate bond: a nominal, a coupon rate, and the schedule that says when interest accrues
/// and when it is paid.
///
/// The coupon of a period is `face_value * coupon_rate * yearFraction(period)` under the bond's
/// own day count. That is the convention itself doing the work rather than a fixed
/// `rate / frequency` amount: under 30/360 both give the same number, while under an actual day
/// count a 184-day half-year pays more than a 181-day one, as it should. Accrued interest uses
/// the same expression over the part of the period already elapsed, so accrual and coupon can
/// never disagree.
///
/// Discounting uses the *payment* dates — money moves then — while accrual uses the unadjusted
/// period boundaries; see [`CouponSchedule`].
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct FixedRateBond {
    face_value: f64,
    coupon_rate: f64,
    frequency: u32,
    schedule: CouponSchedule,
    day_count: DayCountConvention,
}

impl FixedRateBond {
    pub fn new(
        face_value: f64,
        coupon_rate: f64,
        frequency: u32,
        schedule: CouponSchedule,
        day_count: DayCountConvention,
    ) -> Result<Self, FinanceError> {
        if !face_value.is_finite() || face_value <= 0.0 {
            return Err(FinanceError::InvalidInput("face_value must be positive"));
        }
        if !coupon_rate.is_finite() || coupon_rate < 0.0 {
            return Err(FinanceError::InvalidInput(
                "coupon_rate must be non-negative",
            ));
        }
        months_per_period(frequency)?;
        Ok(Self {
            face_value,
            coupon_rate,
            frequency,
            schedule,
            day_count,
        })
    }

    pub fn schedule(&self) -> &CouponSchedule {
        &self.schedule
    }

    pub fn face_value(&self) -> f64 {
        self.face_value
    }

    /// Coupon paid for period `index`, from the day count over that period.
    pub fn coupon_amount(&self, index: usize) -> Option<f64> {
        let (start, end) = self.schedule.period(index)?;
        Some(self.face_value * self.coupon_rate * year_fraction(start, end, self.day_count))
    }

    /// Interest earned but not yet paid at `settlement`.
    ///
    /// Zero when `settlement` falls on a period boundary — the coupon for the period that just
    /// ended has been paid, and the new one has not started accruing. Zero as well outside the
    /// schedule entirely.
    pub fn accrued_interest(&self, settlement: Date) -> f64 {
        let Some(index) = self.schedule.period_containing(settlement) else {
            return 0.0;
        };
        let (start, _) = self
            .schedule
            .period(index)
            .expect("period_containing returned a valid index");
        self.face_value * self.coupon_rate * year_fraction(start, settlement, self.day_count)
    }

    /// Every payment still outstanding after `settlement`, in order: the remaining coupons, with
    /// the nominal added to the last one.
    ///
    /// A coupon whose payment date equals `settlement` is *not* outstanding — it is paid that
    /// day, which is also why accrued interest is zero there.
    pub fn cashflows(&self, settlement: Date) -> Vec<Cashflow> {
        let mut flows = Vec::new();
        let last = self.schedule.period_count().saturating_sub(1);
        for index in 0..self.schedule.period_count() {
            let payment = self.schedule.payment_dates()[index];
            if payment <= settlement {
                continue;
            }
            let mut amount = self.coupon_amount(index).unwrap_or(0.0);
            if index == last {
                amount += self.face_value;
            }
            flows.push(Cashflow {
                date: payment,
                amount,
            });
        }
        flows
    }

    /// Prices the bond at `settlement` for a given yield, compounded at the coupon frequency.
    ///
    /// Every figure comes from the same cashflows: dirty price is their present value, clean
    /// price is that minus accrued interest, Macaulay duration their present-value-weighted time,
    /// and DV01 follows from dirty price and modified duration.
    pub fn price(&self, settlement: Date, ytm: f64) -> Result<BondPricingResult, FinanceError> {
        if !ytm.is_finite() {
            return Err(FinanceError::InvalidInput("ytm must be finite"));
        }
        let flows = self.cashflows(settlement);
        if flows.is_empty() {
            return Err(FinanceError::InvalidInput(
                "no cashflows remain after settlement",
            ));
        }

        let compounding = Compounding::Periodic(self.frequency);
        let mut dirty_price = 0.0f64;
        let mut weighted_pv_sum = 0.0f64;
        for flow in &flows {
            let tau = year_fraction(settlement, flow.date, self.day_count);
            let pv = flow.amount * discount_factor(ytm, tau, compounding);
            dirty_price += pv;
            weighted_pv_sum += tau * pv;
        }

        let macaulay_duration = if dirty_price > 0.0 {
            weighted_pv_sum / dirty_price
        } else {
            0.0
        };
        let modified_duration = macaulay_duration / (1.0 + ytm / self.frequency as f64);
        let accrued_interest = self.accrued_interest(settlement);

        Ok(BondPricingResult {
            dirty_price,
            clean_price: dirty_price - accrued_interest,
            accrued_interest,
            macaulay_duration,
            modified_duration,
            dv01: dirty_price * modified_duration * 0.0001,
        })
    }

    /// Inverts [`FixedRateBond::price`]: the yield at which the bond's clean price equals
    /// `clean_price`.
    pub fn yield_to_maturity(
        &self,
        settlement: Date,
        clean_price: f64,
    ) -> Result<f64, FinanceError> {
        if !clean_price.is_finite() || clean_price <= 0.0 {
            return Err(FinanceError::InvalidInput("clean_price must be positive"));
        }

        let mut ytm = self.coupon_rate.max(0.01);
        for _ in 0..100 {
            let priced = self.price(settlement, ytm)?;
            let diff = priced.clean_price - clean_price;
            if diff.abs() < 1e-8 {
                return Ok(ytm);
            }
            // dPrice/dYield = -dirty_price * modified_duration.
            let derivative = -priced.dirty_price * priced.modified_duration;
            if derivative.abs() < 1e-12 {
                return Err(FinanceError::SolverFailedToConverge);
            }
            ytm = (ytm - diff / derivative).max(-0.5);
        }
        Ok(ytm)
    }
}

/// The terms a consumer supplies so this crate can build a bond's schedule and value it.
///
/// This is the data contract between a consuming application and `kestrel-chartkit`, and it is
/// drawn along one line: **the consumer owns what the instrument *is*, this crate owns what
/// follows from it.** A consumer reads these fields from wherever its product master lives and
/// hands them over; it does not compute coupon dates, accrual or prices itself, and this crate
/// does not go looking for instrument data.
///
/// What is deliberately *not* in here:
///
/// * **Holidays.** They are market data with their own validity — announced, moved and revised
///   per market and year — so they are passed to [`BondSpec::build`] as a [`BusinessCalendar`]
///   rather than frozen into the instrument's terms.
/// * **Currency, multiplier and quantity steps.** Those belong to
///   [`ContractSpec`](crate::contract::ContractSpec). Repeating them here would create a second
///   truth about the same instrument.
/// * **Market prices and yields.** They are observations, not terms, and are passed per
///   valuation.
///
/// The day count has no default: it decides every coupon amount and every accrual, and a silently
/// assumed one would be wrong more often than right.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct BondSpec {
    /// Redeemed at maturity, and the base of every coupon.
    pub face_value: f64,
    /// Annual coupon rate as a fraction, e.g. `0.05` for 5%.
    pub coupon_rate: f64,
    /// Coupon payments per year; must divide 12 evenly.
    pub frequency: u32,
    /// Start of the first accrual period — usually the issue or dated date, not the settlement of
    /// a later trade.
    pub issue: Date,
    pub maturity: Date,
    /// Governs coupon amounts, accrued interest and discounting alike.
    pub day_count: DayCountConvention,
    /// Where an irregular period sits, if the dates do not divide evenly.
    pub stub: ScheduleStub,
    /// How payment dates move off non-business days. The accrual dates never move.
    pub business_day_convention: BusinessDayConvention,
}

impl BondSpec {
    /// The terms every bond needs. Stub placement and business-day handling take their documented
    /// defaults ([`ScheduleStub::ShortFirst`], [`BusinessDayConvention::Unadjusted`]) and are set
    /// with [`BondSpec::with_stub`] and [`BondSpec::with_business_day_convention`].
    pub fn new(
        face_value: f64,
        coupon_rate: f64,
        frequency: u32,
        issue: Date,
        maturity: Date,
        day_count: DayCountConvention,
    ) -> Self {
        Self {
            face_value,
            coupon_rate,
            frequency,
            issue,
            maturity,
            day_count,
            stub: ScheduleStub::default(),
            business_day_convention: BusinessDayConvention::default(),
        }
    }

    pub fn with_stub(mut self, stub: ScheduleStub) -> Self {
        self.stub = stub;
        self
    }

    pub fn with_business_day_convention(mut self, convention: BusinessDayConvention) -> Self {
        self.business_day_convention = convention;
        self
    }

    /// The coupon schedule these terms describe, against the holidays of the market it trades in.
    pub fn schedule(&self, calendar: &BusinessCalendar) -> Result<CouponSchedule, FinanceError> {
        CouponSchedule::generate(
            self.issue,
            self.maturity,
            self.frequency,
            self.stub,
            self.business_day_convention,
            calendar,
        )
    }

    /// The valuable instrument these terms describe. Rejects the same inputs
    /// [`CouponSchedule::generate`] and [`FixedRateBond::new`] reject, so a consumer finds a bad
    /// product record here rather than in a price.
    pub fn build(&self, calendar: &BusinessCalendar) -> Result<FixedRateBond, FinanceError> {
        FixedRateBond::new(
            self.face_value,
            self.coupon_rate,
            self.frequency,
            self.schedule(calendar)?,
            self.day_count,
        )
    }
}

/// Prices a standard fixed-rate bond with regular coupon payments.
///
/// A convenience over [`FixedRateBond`] for the common case where only settlement and maturity
/// are known: the coupon dates are reconstructed backward from maturity via
/// [`CouponSchedule::covering`], so every period is regular and unadjusted. Bonds with a stub
/// period, a business-day rule or an explicitly known issue date need [`CouponSchedule::generate`]
/// and [`FixedRateBond`] instead — this entry point cannot infer any of those from its arguments.
///
/// Returns clean price, dirty price, accrued interest, Macaulay/Modified duration and DV01, all
/// from the same schedule and the same cashflows.
pub fn price_bond(
    face_value: f64,
    coupon_rate: f64,
    frequency: u32,
    settlement: Date,
    maturity: Date,
    ytm: f64,
    convention: DayCountConvention,
) -> Result<BondPricingResult, FinanceError> {
    let schedule = CouponSchedule::covering(settlement, maturity, frequency)?;
    FixedRateBond::new(face_value, coupon_rate, frequency, schedule, convention)?
        .price(settlement, ytm)
}

/// Solves for the Yield to Maturity (YTM) given a clean bond market price.
///
/// Same schedule reconstruction as [`price_bond`], and the same limits.
pub fn yield_to_maturity(
    clean_price: f64,
    face_value: f64,
    coupon_rate: f64,
    frequency: u32,
    settlement: Date,
    maturity: Date,
    convention: DayCountConvention,
) -> Result<f64, FinanceError> {
    let schedule = CouponSchedule::covering(settlement, maturity, frequency)?;
    FixedRateBond::new(face_value, coupon_rate, frequency, schedule, convention)?
        .yield_to_maturity(settlement, clean_price)
}
