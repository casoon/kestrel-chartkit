"""Moving averages and exponential smoothing, one output per input."""

import math
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


def alma_window(window, offset, sigma):
    """Arnaud Legoux moving average of one full window (Legoux and Kouzis-Loukas): with
    m = offset (N - 1) and s = N / sigma, the value i places after the oldest gets the weight
    exp(-(i - m)^2 / (2 s^2)); the result is the weight-normalised sum."""
    n = len(window)
    m = offset * (n - 1)
    s = n / sigma
    weights = [math.exp(-((i - m) ** 2) / (2.0 * s * s)) for i in range(n)]
    return sum(w * x for w, x in zip(weights, window)) / sum(weights)


def super_smoother(values, period):
    """Ehlers' 2-pole SuperSmoother (Cycle Analytics for Traders, 2013):
    a1 = exp(-1.414 pi / period), b1 = 2 a1 cos(1.414 pi / period), c2 = b1, c3 = -a1^2,
    c1 = 1 - c2 - c3. Started from the price: ss_0 = x_0 and ss_1 = x_1, then
    ss_t = c1 (x_t + x_(t-1)) / 2 + c2 ss_(t-1) + c3 ss_(t-2) from t = 2. One output per input."""
    a1 = math.exp(-1.414 * math.pi / period)
    b1 = 2.0 * a1 * math.cos(1.414 * math.pi / period)
    c2, c3 = b1, -(a1 * a1)
    c1 = 1.0 - c2 - c3
    out = []
    for t, x in enumerate(values):
        if t < 2:
            out.append(x)
        else:
            out.append(c1 * (x + values[t - 1]) / 2.0 + c2 * out[t - 1] + c3 * out[t - 2])
    return out


def jma_approximation(values, period, phase, power):
    """The openly published Jurik-style approximation (not the product of Jurik Research):
    beta = 0.45 (N - 1) / (0.45 (N - 1) + 2), alpha = beta^power, p = phase / 100 + 1.5, with
    phase clamped to -100..100 and power to at least 1, and per value
      e0 = (1 - alpha) x + alpha e0
      e1 = (x - e0) (1 - beta) + beta e1
      e2 = (e0 + p e1 - jma) (1 - alpha)^2 + alpha^2 e2
      jma = jma + e2
    The first value is x itself, with e0 = x and e1 = e2 = 0. One output per input."""
    phase = min(max(phase, -100.0), 100.0)
    power = max(power, 1.0)
    drift = 0.45 * (period - 1)
    beta = drift / (drift + 2.0)
    alpha = beta ** power
    p = phase / 100.0 + 1.5
    e0 = e1 = e2 = 0.0
    jma, out = None, []
    for x in values:
        if jma is None:
            e0, e1, e2, jma = x, 0.0, 0.0, x
            out.append(x)
            continue
        e0 = (1.0 - alpha) * x + alpha * e0
        e1 = (x - e0) * (1.0 - beta) + beta * e1
        e2 = (e0 + p * e1 - jma) * (1.0 - alpha) ** 2 + alpha ** 2 * e2
        jma = jma + e2
        out.append(jma)
    return out


def kestrel_adaptive_smoother(values, period, phase, adapt):
    """Kestrel Adaptive Smoother, from its formula: with n = (period + 1) / 2, k0 = 2 / (n + 1),
    a = 2 / (max(n, 2) + 1), b = 2 / (max(4 n, 20) + 1), c = a / (2 - a) and d = x_t - x_(t-1),
      v = v + a (d - v)             smoothed change, v_0 = 0
      q = q + a (d^2 - q)           its mean square; Q = Q + b (d^2 - Q) the long-run one
      z2 = v^2 / (c sqrt(Q_before q))   with Q_before the value of Q ahead of this bar
      S = k0^2 / (1 - k0) z2^adapt, k = (sqrt(S^2 + 4 S) - S) / 2
      s1 += k (x - s1), s2 += k (s1 - s2), s3 += k (s2 - s3), out = s2 + phase (s2 - s3).
    The first value is x_0 with all three stages on it; the first change seeds q and Q. Where
    the scale is zero, z2 is 1 for a zero v and unbounded otherwise (then k = 1); k = 1 as well
    for k0 >= 1. One output per input."""
    xs = list(values)
    if not xs:
        return []
    n = (period + 1) / 2
    k0 = 2 / (n + 1)
    a = 2 / (max(n, 2) + 1)
    b = 2 / (max(4 * n, 20) + 1)
    c = a / (2 - a)

    s1 = s2 = s3 = xs[0]
    v = 0.0
    q = None
    Q = None
    out = [xs[0]]
    for t in range(1, len(xs)):
        d = xs[t] - xs[t - 1]
        d2 = d * d
        v = v + a * (d - v)
        q = d2 if q is None else q + a * (d2 - q)
        Q_before = d2 if Q is None else Q
        Q = d2 if Q is None else Q + b * (d2 - Q)
        scale = c * math.sqrt(Q_before * q)
        if scale > 0:
            z2 = v * v / scale
        elif v * v > 0:
            z2 = math.inf
        else:
            z2 = 1.0
        if k0 >= 1:
            k = 1.0
        else:
            if adapt == 0:
                zp = 1.0
            elif z2 == math.inf:
                zp = math.inf
            else:
                zp = z2 ** adapt
            S = k0 * k0 / (1 - k0) * zp
            if math.isinf(S):
                k = 1.0
            else:
                k = (math.sqrt(S * S + 4 * S) - S) / 2
        s1 = s1 + k * (xs[t] - s1)
        s2 = s2 + k * (s1 - s2)
        s3 = s3 + k * (s2 - s3)
        out.append(s2 + phase * (s2 - s3))
    return out
