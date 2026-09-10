"""golden_finance_cashflows: premium and discount bond prices of the cashflow tests, from the
documented discounting."""

import datetime

from ..bonds import price_at_yield
from ..dates import thirty360
from ..fixture import provenance
from ..schedules import accrual_dates

NAME = "golden_finance_cashflows"
SETTLEMENT = datetime.date(2026, 1, 1)
MATURITY = datetime.date(2028, 1, 1)

HEADER = [
    "kestrel-chartkit golden reference fixture: bond cashflows",
    "",
    *provenance("finance_cashflows"),
    "",
    "Two-year annual bonds, face 100, settled 2026-01-01 on a coupon date, maturing 2028-01-01,",
    "yield 4 %. 30/360 Bond Basis puts the coupon dates exactly one and two years out; every",
    "cashflow is discounted with (1 + y)^(-t), so clean = dirty = sum CF_t / 1.04^t:",
    "  premium   8 % coupon: 8 / 1.04 + 108 / 1.04^2",
    "  discount  2 % coupon: 2 / 1.04 + 102 / 1.04^2",
    "",
    "finance_cashflows_tolerance is the comparison tolerance the tests apply, stated rather than",
    "derived.",
]


def _clean(coupon):
    dates = accrual_dates(SETTLEMENT, MATURITY, 12)
    return price_at_yield(100.0, coupon, 1, dates, SETTLEMENT, 0.04,
                          year_fraction=thirty360)["clean"]


def build():
    case = {"bond2y_premium_clean": _clean(0.08), "bond2y_discount_clean": _clean(0.02)}
    return HEADER, [(None, "two-year bonds at 4 %", case),
                    (None, None, {"finance_cashflows_tolerance": 1e-12})]
