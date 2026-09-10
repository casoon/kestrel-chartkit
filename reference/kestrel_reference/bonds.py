"""Fixed-rate bonds over an explicit list of period boundaries."""

from .dates import act365


def _periods(dates):
    return list(zip(dates[:-1], dates[1:]))


def accrued_interest(face, coupon, dates, settlement, year_fraction=act365):
    """Coupon accrued from the start of the period containing `settlement` up to it, Actual/365
    Fixed unless another `year_fraction` is given. Zero on a coupon date, where a new period
    starts."""
    for start, end in _periods(dates):
        if start <= settlement < end:
            return face * coupon * year_fraction(start, settlement)
    return 0.0


def cashflows(face, coupon, dates, settlement, payment_dates=None, year_fraction=act365):
    """`(payment date, amount)` of every coupon paid strictly after `settlement`, the nominal
    added to the last one. A coupon paid on the settlement date itself is not outstanding.

    Each coupon is `face * coupon * yearfraction(period)`, so a short or long period pays
    proportionally less or more. Payment dates default to the period ends.
    """
    flows = []
    last = len(dates) - 2
    for index, (start, end) in enumerate(_periods(dates)):
        paid = end if payment_dates is None else payment_dates[index]
        if paid <= settlement:
            continue
        amount = face * coupon * year_fraction(start, end)
        if index == last:
            amount += face
        flows.append((paid, amount))
    return flows


def price_at_yield(face, coupon, frequency, dates, settlement, ytm, year_fraction=act365):
    """Dirty and clean price, accrued interest and durations at one yield.

    Every outstanding cashflow is discounted with (1 + ytm/f)^(-f t), t the year fraction
    (Actual/365 Fixed unless another is given) from settlement to payment. Macaulay duration is the present-value weighted mean of those t;
    modified duration, -dP/dy / P, is Macaulay / (1 + ytm/f) for this compounding.
    """
    base = 1.0 + ytm / frequency
    dirty = 0.0
    weighted_time = 0.0
    for paid, amount in cashflows(face, coupon, dates, settlement, year_fraction=year_fraction):
        t = year_fraction(settlement, paid)
        present = amount * base ** (-frequency * t)
        dirty += present
        weighted_time += t * present
    accrued = accrued_interest(face, coupon, dates, settlement, year_fraction)
    macaulay = weighted_time / dirty
    return {
        "dirty": dirty,
        "clean": dirty - accrued,
        "accrued": accrued,
        "macaulay": macaulay,
        "modified": macaulay / base,
    }


def value_on_curve(face, coupon, dates, reference, curve):
    """Dirty value of the outstanding cashflows discounted on `curve`, plus accrued interest and
    the clean value. Times run Actual/365 Fixed from `reference`, the curve's own origin."""
    dirty = 0.0
    for paid, amount in cashflows(face, coupon, dates, reference):
        dirty += amount * curve.discount(act365(reference, paid))
    accrued = accrued_interest(face, coupon, dates, reference)
    return {"dirty": dirty, "clean": dirty - accrued, "accrued": accrued}
