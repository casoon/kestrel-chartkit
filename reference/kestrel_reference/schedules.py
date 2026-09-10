"""Coupon period boundaries of a fixed-rate instrument."""

from .dates import add_months, end_of_month, is_end_of_month


def accrual_dates(issue, maturity, months, backward=True, month_end_rule=False):
    """Unadjusted period boundaries from `issue` to `maturity` in steps of `months`.

    Backward generation anchors on maturity; a leftover front piece becomes a short first period
    starting at issue. Forward generation anchors on issue; a leftover final piece becomes a short
    last period ending at maturity.

    Every date is computed from the anchor as anchor +/- k * months, never from its predecessor,
    so clamping one date into a short month does not shorten all later periods. With the month-end
    rule and an anchor on the last day of its month, every generated date is moved to the last day
    of its own month.
    """
    anchor = maturity if backward else issue
    to_month_end = month_end_rule and is_end_of_month(anchor)
    sign = -1 if backward else 1

    dates = [anchor]
    k = 1
    while True:
        day = add_months(anchor, sign * k * months)
        if to_month_end:
            day = end_of_month(day)
        if (backward and day <= issue) or (not backward and day >= maturity):
            break
        dates.append(day)
        k += 1

    if backward:
        dates.append(issue)
        dates.reverse()
    else:
        dates.append(maturity)
    return dates
