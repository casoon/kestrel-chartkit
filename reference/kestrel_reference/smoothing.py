"""Moving averages and exponential smoothing, one output per input."""

from fractions import Fraction


def ema_first_sample(values, period):
    """Exponential average with alpha = 2 / (period + 1), seeded with the first value and
    recursed from the second one on: e_0 = x_0, e_t = e_(t-1) + alpha (x_t - e_(t-1))."""
    return ema_alpha(values, 2.0 / (period + 1))


def ema_alpha(values, alpha):
    """Exponential recursion with a given alpha, seeded with the first value."""
    out = []
    level = None
    for x in values:
        level = x if level is None else level + alpha * (x - level)
        out.append(level)
    return out


def ema_sma_seed(values, period):
    """Exponential average with alpha = 2 / (period + 1), seeded with the mean of the first
    `period` values. `None` before that; the seed itself is the first defined value."""
    alpha = 2.0 / (period + 1)
    out = []
    level = None
    for index, x in enumerate(values):
        if index < period - 1:
            out.append(None)
            continue
        if level is None:
            level = sum(values[:period]) / period
        else:
            level = level + alpha * (x - level)
        out.append(level)
    return out


def sma(window):
    return sum(window) / len(window)


def wma(window):
    """Linearly weighted: weight 1 on the oldest value, len(window) on the most recent."""
    weights = range(1, len(window) + 1)
    return sum(w * x for w, x in zip(weights, window)) / sum(weights)


def least_squares(window):
    """Ordinary least squares y = intercept + slope * x over x = 0 .. n-1, in exact rational
    arithmetic. Returns slope, intercept, R^2 and the fitted value at the last x, as Fractions."""
    # Exact binary values: that is what the implementation under test receives.
    ys = [Fraction(y) for y in window]
    n = len(ys)
    xs = [Fraction(x) for x in range(n)]
    mean_x = sum(xs) / n
    mean_y = sum(ys) / n
    sxy = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys))
    sxx = sum((x - mean_x) ** 2 for x in xs)
    slope = sxy / sxx
    intercept = mean_y - slope * mean_x
    fitted = [intercept + slope * x for x in xs]
    ss_res = sum((y - f) ** 2 for y, f in zip(ys, fitted))
    ss_tot = sum((y - mean_y) ** 2 for y in ys)
    r2 = 1 - ss_res / ss_tot if ss_tot else Fraction(1)
    return slope, intercept, r2, fitted[-1]


def ema_published(values, period, alpha=None):
    """First-sample-seeded exponential recursion (alpha = 2/(period+1) unless given) that
    publishes from the period-th input on; earlier positions are `None`. The recursion itself runs
    from the first input, so the published values are never re-seeded."""
    alpha = 2.0 / (period + 1) if alpha is None else alpha
    return [level if index >= period - 1 else None
            for index, level in enumerate(ema_alpha(values, alpha))]


def published(values):
    """The defined values of a series, without its unpublished leading `None` entries."""
    return [v for v in values if v is not None]


def rma(values, period):
    """Wilder's smoothing, alpha = 1/period, seeded with the mean of the first `period` values;
    `None` before that."""
    out = []
    level = None
    for index, x in enumerate(values):
        if index < period - 1:
            out.append(None)
            continue
        level = sum(values[:period]) / period if level is None else level + (x - level) / period
        out.append(level)
    return out
