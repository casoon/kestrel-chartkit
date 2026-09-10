"""golden_business_days: business-day conventions and adjusted schedules."""

from datetime import date

from ..calendars import CONVENTIONS, BusinessCalendar
from ..dates import add_days, end_of_month, is_end_of_month
from ..fixture import date_keys, provenance
from ..schedules import accrual_dates

NAME = "golden_business_days"

HEADER = [
    "kestrel-chartkit golden reference fixture: business-day conventions and adjusted schedules",
    "",
    *provenance("business_days"),
    "",
    "The calendar is not a market calendar. It is Saturdays, Sundays and the holiday list written",
    "into this fixture, built the same way on both sides. What is under test is the adjustment",
    "rule, not somebody's holiday data - which is also why this crate ships no market calendars.",
    "The holidays are chosen to hit month ends, a year turn, a leap-day month end and a chain that",
    "spans a weekend.",
    "",
    "Adjustment probes: probe<i>_y/_m/_d, probe<i>_weekday (1 = Monday .. 7 = Sunday),",
    "probe<i>_business (1 = the calendar counts it as a business day) and the adjusted date under",
    "each convention as probe<i>_<convention>_y/_m/_d.",
    "",
    "Schedule cases: an unadjusted accrual grid plus the payment dates that follow from applying",
    "the convention to every accrual end. sched<i>_acc<j>_* are the accrual boundaries,",
    "sched<i>_pay<j>_* the payment dates.",
    "",
    "sched<i>_adj<j>_* records a fully adjusted schedule, in which the convention moves every",
    "accrual boundary as well. This crate deliberately does not do that: a coupon covers a",
    "calendar period regardless of which days the payment system was open, so accrual stays",
    "unadjusted and only the payment moves. Those dates are carried here so the divergence is on",
    "record with numbers instead of being asserted in prose.",
]

HOLIDAYS = [
    date(2026, 8, 31),
    date(2026, 12, 24),
    date(2026, 12, 25),
    date(2026, 12, 31),
    date(2027, 1, 1),
    date(2027, 3, 31),
    date(2027, 4, 30),
    date(2027, 5, 31),
    date(2027, 12, 24),
    date(2027, 12, 27),
    date(2028, 2, 29),
    date(2028, 8, 31),
]

SCHEDULES = [
    ("halbjaehrlich ueber zwei Jahreswechsel, Following",
     dict(issue=date(2026, 6, 30), maturity=date(2028, 6, 30), frequency=2,
          convention="following")),
    ("halbjaehrlich, Termine auf Monatsenden, ModifiedFollowing",
     dict(issue=date(2026, 8, 31), maturity=date(2028, 8, 31), frequency=2,
          convention="modified_following")),
    ("vierteljaehrlich ueber die Feiertagskette, Preceding",
     dict(issue=date(2027, 3, 31), maturity=date(2028, 3, 31), frequency=4,
          convention="preceding")),
    ("jaehrlich, Unadjusted - Zahlung und Abgrenzung fallen zusammen",
     dict(issue=date(2026, 12, 31), maturity=date(2029, 12, 31), frequency=1,
          convention="unadjusted")),
    ("monatlich ueber den Schaltmonat, Following",
     dict(issue=date(2028, 2, 29), maturity=date(2028, 8, 31), frequency=12,
          convention="following")),
]


def _probe_dates():
    """Every holiday and its neighbours, every month end from June 2026 to August 2027, and a
    plain weekday, Saturday and Sunday without holiday context."""
    days = set()
    for holiday in HOLIDAYS:
        days.update({add_days(holiday, -1), holiday, add_days(holiday, 1)})
    month_end = date(2026, 6, 30)
    for _ in range(15):
        days.add(month_end)
        month_end = end_of_month(add_days(month_end, 1))
    days.update({date(2026, 9, 17), date(2026, 9, 19), date(2026, 9, 20)})
    return sorted(days)


def _schedule_case(calendar, issue, maturity, frequency, convention):
    accrual = accrual_dates(issue, maturity, 12 // frequency,
                            month_end_rule=is_end_of_month(maturity))
    case = {
        "frequency": float(frequency),
        "convention": float(CONVENTIONS.index(convention)),
        "accrual_count": float(len(accrual)),
    }
    case.update(date_keys("issue", issue))
    case.update(date_keys("maturity", maturity))
    for index, day in enumerate(accrual):
        case.update(date_keys(f"acc{index}", day))
    for index, day in enumerate(accrual[1:]):
        case.update(date_keys(f"pay{index}", calendar.adjust(day, convention)))
    for index, day in enumerate(accrual):
        case.update(date_keys(f"adj{index}", calendar.adjust(day, convention)))
    return case


def build():
    calendar = BusinessCalendar(HOLIDAYS)
    probes = _probe_dates()

    holidays = {"holiday_count": float(len(HOLIDAYS))}
    for index, holiday in enumerate(HOLIDAYS):
        holidays.update(date_keys(f"holiday{index}", holiday))

    probe_case = {"probe_count": float(len(probes))}
    for index, day in enumerate(probes):
        probe_case.update(date_keys(f"probe{index}", day))
        probe_case[f"probe{index}_weekday"] = float(day.isoweekday())
        probe_case[f"probe{index}_business"] = 1.0 if calendar.is_business_day(day) else 0.0
        for convention in CONVENTIONS:
            probe_case.update(
                date_keys(f"probe{index}_{convention}", calendar.adjust(day, convention))
            )

    blocks = [
        ("meta", "case counts", {
            "probe_case_count": 1.0,
            "schedule_case_count": float(len(SCHEDULES)),
        }),
        ("cal", "the calendar both sides build", holidays),
        ("probes", "adjustment of single dates under every convention", probe_case),
    ]
    for index, (comment, kwargs) in enumerate(SCHEDULES, start=1):
        blocks.append((f"sched{index}", comment, _schedule_case(calendar, **kwargs)))
    return HEADER, blocks
