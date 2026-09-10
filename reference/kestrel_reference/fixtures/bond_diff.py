"""golden_bond_diff: fixed-rate bonds priced at a yield over an explicit schedule."""

from datetime import date

from ..bonds import price_at_yield
from ..dates import add_months, is_end_of_month
from ..fixture import date_keys, provenance
from ..schedules import accrual_dates

NAME = "golden_bond_diff"

HEADER = [
    "kestrel-chartkit golden reference fixture: fixed-rate bond difference tests",
    "",
    *provenance("bond_diff"),
    "",
    "Conventions: unadjusted schedule without holiday calendar, Actual/365Fixed, compounded at the",
    "coupon frequency, zero settlement days. Prices are absolute amounts, not per-100 quotes.",
    "Coupons are face * rate * yearfraction(period); accrued interest runs from the start of the",
    "period containing settlement.",
    "",
    "bond<i>: regular schedules, reconstructed backward from maturity on the maturity's",
    "day-of-month. The cases include settlement exactly on a coupon date and between coupon",
    "dates.",
    "sched<i>: schedules with an explicit issue date - short first period, short last period and",
    "the month-end rule - with the generated period boundaries as date<j>_*.",
]

REGULAR = [
    ("on a coupon date, five years to maturity, semiannual",
     dict(face=1000.0, coupon=0.05, frequency=2, settlement=date(2026, 6, 15),
          maturity=date(2031, 6, 15), ytm=0.04, months_back=120)),
    ("on a coupon date, two years to maturity, annual",
     dict(face=1000.0, coupon=0.03, frequency=1, settlement=date(2026, 6, 15),
          maturity=date(2028, 6, 15), ytm=0.035, months_back=60)),
    ("between coupon dates, roughly mid-period",
     dict(face=1000.0, coupon=0.05, frequency=2, settlement=date(2026, 9, 20),
          maturity=date(2031, 6, 15), ytm=0.04, months_back=120)),
    ("between coupon dates, shortly before the next coupon",
     dict(face=1000.0, coupon=0.05, frequency=2, settlement=date(2026, 12, 10),
          maturity=date(2031, 6, 15), ytm=0.04, months_back=120)),
    ("on a coupon date, negative yield",
     dict(face=1000.0, coupon=0.01, frequency=2, settlement=date(2026, 6, 15),
          maturity=date(2029, 6, 15), ytm=-0.004, months_back=120)),
    ("on a coupon date, zero coupon over three years",
     dict(face=1000.0, coupon=0.0, frequency=1, settlement=date(2026, 6, 15),
          maturity=date(2029, 6, 15), ytm=0.03, months_back=60)),
    ("on a coupon date, across a leap day",
     dict(face=1000.0, coupon=0.04, frequency=2, settlement=date(2028, 2, 29),
          maturity=date(2030, 8, 29), ytm=0.045, months_back=60)),
]

SCHEDULED = [
    ("kurze erste Periode: Emission zwei Monate nach dem regulaeren Gitter",
     dict(face=1000.0, coupon=0.04, frequency=2, issue=date(2026, 8, 10),
          settlement=date(2026, 10, 20), maturity=date(2029, 6, 15), ytm=0.035, backward=True)),
    ("kurze letzte Periode: Vorwaertsgenerierung ab Emission",
     dict(face=1000.0, coupon=0.04, frequency=2, issue=date(2026, 6, 15),
          settlement=date(2026, 10, 20), maturity=date(2029, 4, 10), ytm=0.035, backward=False)),
    ("Monatsende-Regel: Faelligkeit am 31. August",
     dict(face=1000.0, coupon=0.05, frequency=2, issue=date(2026, 8, 31),
          settlement=date(2026, 11, 30), maturity=date(2030, 8, 31), ytm=0.045, backward=True)),
    ("Monatsende-Regel ueber den Februar, Settlement auf einem Kupontermin",
     dict(face=1000.0, coupon=0.03, frequency=2, issue=date(2026, 8, 31),
          settlement=date(2027, 2, 28), maturity=date(2030, 8, 31), ytm=0.03, backward=True)),
]


def _regular_case(face, coupon, frequency, settlement, maturity, ytm, months_back):
    months = 12 // frequency
    dates = accrual_dates(add_months(maturity, -months_back), maturity, months)
    priced = price_at_yield(face, coupon, frequency, dates, settlement, ytm)
    case = {"face": face, "coupon": coupon, "frequency": float(frequency)}
    case.update(date_keys("settlement", settlement))
    case.update(date_keys("maturity", maturity))
    case.update({
        "ytm": ytm,
        # 1 when settlement falls exactly on a coupon date, where accrued interest is zero.
        "on_coupon_date": 1.0 if priced["accrued"] == 0.0 else 0.0,
        "clean_price": priced["clean"],
        "dirty_price": priced["dirty"],
        "accrued_interest": priced["accrued"],
        "macaulay_duration": priced["macaulay"],
        "modified_duration": priced["modified"],
    })
    return case


def _scheduled_case(face, coupon, frequency, issue, settlement, maturity, ytm, backward):
    dates = accrual_dates(issue, maturity, 12 // frequency, backward=backward,
                          month_end_rule=is_end_of_month(maturity))
    priced = price_at_yield(face, coupon, frequency, dates, settlement, ytm)
    case = {"face": face, "coupon": coupon, "frequency": float(frequency)}
    case.update(date_keys("issue", issue))
    case.update(date_keys("settlement", settlement))
    case.update(date_keys("maturity", maturity))
    case.update({
        "ytm": ytm,
        "period_count": float(len(dates) - 1),
        "clean_price": priced["clean"],
        "dirty_price": priced["dirty"],
        "accrued_interest": priced["accrued"],
        "macaulay_duration": priced["macaulay"],
        "modified_duration": priced["modified"],
    })
    for index, day in enumerate(dates):
        case.update(date_keys(f"date{index}", day))
    return case


def build():
    blocks = [("meta", "case counts", {
        "bond_case_count": float(len(REGULAR)),
        "scheduled_case_count": float(len(SCHEDULED)),
    })]
    for index, (comment, kwargs) in enumerate(REGULAR, start=1):
        blocks.append((f"bond{index}", comment, _regular_case(**kwargs)))
    for index, (comment, kwargs) in enumerate(SCHEDULED, start=1):
        blocks.append((f"sched{index}", comment, _scheduled_case(**kwargs)))
    return HEADER, blocks
