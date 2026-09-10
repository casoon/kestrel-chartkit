"""Exact arithmetic for the cases where a floating-point shortcut would decide the result.

Inputs are taken as the exact binary values of their floats — that is what the implementation
under test receives — and carried as Fractions; square roots are taken to 50 significant digits.
"""

from decimal import Decimal, localcontext
from fractions import Fraction

DIGITS = 50


def to_decimal(q):
    with localcontext() as ctx:
        ctx.prec = DIGITS
        return Decimal(q.numerator) / Decimal(q.denominator)


def sqrt(q):
    """Square root of a non-negative Fraction to DIGITS significant digits, as a Decimal."""
    with localcontext() as ctx:
        ctx.prec = DIGITS
        return (Decimal(q.numerator) / Decimal(q.denominator)).sqrt()


def central_squares(window):
    """sum((x - mean)^2) over the window, exactly, together with the mean."""
    xs = [Fraction(x) for x in window]
    mean = sum(xs) / len(xs)
    return sum((x - mean) ** 2 for x in xs), mean


def population_std(window):
    squares, _ = central_squares(window)
    return float(sqrt(squares / len(window)))


def bollinger(window, mult, sample=False):
    """Bollinger bands over the whole window: basis = mean, bands = basis +/- mult * sd with
    sd = sqrt(sum((x - basis)^2) / divisor), divisor n (population) or n - 1 (sample);
    bandwidth = (upper - lower) / basis, percent_b = (close - lower) / (upper - lower), 0.5 for a
    degenerate band. The close is the last value of the window. Returns Decimals."""
    squares, mean = central_squares(window)
    divisor = len(window) - 1 if sample else len(window)
    with localcontext() as ctx:
        ctx.prec = DIGITS
        basis = to_decimal(mean)
        sd = sqrt(squares / divisor)
        spread = to_decimal(Fraction(mult)) * sd
        upper, lower = basis + spread, basis - spread
        bandwidth = (upper - lower) / basis if basis != 0 else Decimal(0)
        close = to_decimal(Fraction(window[-1]))
        percent_b = (close - lower) / (upper - lower) if upper != lower else Decimal("0.5")
    return {"basis": basis, "upper": upper, "lower": lower,
            "bandwidth": bandwidth, "percent_b": percent_b}


def average_ranks(values):
    """1-based ranks; equal values share the average of the ranks they occupy."""
    order = sorted(range(len(values)), key=lambda i: values[i])
    ranks = [None] * len(values)
    start = 0
    while start < len(order):
        end = start
        while end + 1 < len(order) and values[order[end + 1]] == values[order[start]]:
            end += 1
        shared = Fraction(start + 1 + end + 1, 2)
        for k in range(start, end + 1):
            ranks[order[k]] = shared
        start = end + 1
    return ranks


def correlation(a, b):
    """Pearson correlation of two exact series as a Decimal; `None` without variance."""
    ma, mb = sum(a) / len(a), sum(b) / len(b)
    cov = sum((x - ma) * (y - mb) for x, y in zip(a, b))
    sa = sum((x - ma) ** 2 for x in a)
    sb = sum((y - mb) ** 2 for y in b)
    if sa == 0 or sb == 0:
        return None
    with localcontext() as ctx:
        ctx.prec = DIGITS
        return to_decimal(cov) / sqrt(sa * sb)
