"""Eight Ehlers indicators: ultimate_smoother, roofing_filter, reflex, trendflex,
correlation_trend, laguerre_rsi, cyber_cycle, mama.

Written from the specifications, independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one indicator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `None` where there is no output, otherwise
`((value, extras), alerts)` with `alerts` mapping each alert kind to its strength (1.0). Invalid
parameters raise `ValueError`. A comparison that decides an alert and lies within 1e-10 (relative)
of flipping raises `FragileComparison`, so rounding cannot decide an event.
"""

import math

PI = math.pi

FRAGILE = 1e-10


class FragileComparison(ValueError):
    pass


# ----------------------------------------------------------------- validation
def _param(params, key, default, lo, hi, whole=False):
    v = params.get(key, default)
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise ValueError("%s must be a number" % key)
    v = float(v)
    if not math.isfinite(v):
        raise ValueError("%s must be finite" % key)
    if v < lo or v > hi:
        raise ValueError("%s out of range %s..%s" % (key, lo, hi))
    if whole:
        if not v.is_integer():
            raise ValueError("%s must be a whole number" % key)
        return int(v)
    return v


def _closes(bars):
    return [float(b[3]) for b in bars]


def _hl2(bars):
    return [(float(b[1]) + float(b[2])) / 2.0 for b in bars]


# ------------------------------------------------------------------- helpers
def _ss_coeffs(period):
    a1 = math.exp(-1.414 * PI / period)
    c2 = 2.0 * a1 * math.cos(1.414 * PI / period)
    c3 = -a1 * a1
    c1 = 1.0 - c2 - c3
    return c1, c2, c3


def _supersmoother(src, period):
    c1, c2, c3 = _ss_coeffs(period)
    n = len(src)
    f = [0.0] * n
    for i in range(n):
        if i < 2:
            f[i] = src[i]
        else:
            f[i] = c1 * (src[i] + src[i - 1]) / 2.0 + c2 * f[i - 1] + c3 * f[i - 2]
    return f


def _clear(x, y):
    if x != y and abs(x - y) <= FRAGILE * max(1.0, abs(x), abs(y)):
        raise FragileComparison("fragile comparison: %r vs %r" % (x, y))


def _cross(pa, a, pb, b):
    """(above, below) crossing of line A over line B with the spec tolerance."""
    tol = 1e-9 * max(1.0, abs(pa), abs(pb), abs(a), abs(b))
    for x, y in ((pa, pb + tol), (a, b + tol), (pa, pb - tol), (a, b - tol)):
        _clear(x, y)
    return (pa <= pb + tol and a > b + tol), (pa >= pb - tol and a < b - tol)


def _build(n, values, extras=None, alert_fn=None):
    """values: list (None where no value)."""
    out = [None] * n
    for i in range(n):
        v = values[i]
        if v is None:
            continue
        extra = {}
        if extras:
            for k, arr in extras.items():
                extra[k] = arr[i]
        al = {}
        if alert_fn is not None and i > 0 and values[i - 1] is not None:
            al = {kind: 1.0 for kind in alert_fn(i)}
        out[i] = ((v, extra), al)
    return out


def _zero_alerts(values):
    def fn(i):
        a = []
        up, dn = _cross(values[i - 1], values[i], 0.0, 0.0)
        if up:
            a.append("bull_zero_cross")
        if dn:
            a.append("bear_zero_cross")
        return a
    return fn


# ---------------------------------------------------------------- the entries
def _ultimate_smoother(params, bars):
    period = _param(params, "period", 20, 2, 10000, whole=True)
    P = _closes(bars)
    n = len(P)
    a1 = math.exp(-1.414 * PI / period)
    c2 = 2.0 * a1 * math.cos(1.414 * PI / period)
    c3 = -a1 * a1
    c1 = (1.0 + c2 - c3) / 4.0
    us = [0.0] * n
    for i in range(n):
        if i < 3:
            us[i] = P[i]
        else:
            us[i] = ((1 - c1) * P[i] + (2 * c1 - c2) * P[i - 1] - (c1 + c3) * P[i - 2]
                     + c2 * us[i - 1] + c3 * us[i - 2])
    vals = [us[i] if i >= period - 1 else None for i in range(n)]
    return _build(n, vals)


def _roofing_filter(params, bars):
    hp_p = _param(params, "hp_period", 48, 3, 10000, whole=True)
    lp_p = _param(params, "lp_period", 10, 2, 10000, whole=True)
    if not lp_p < hp_p:
        raise ValueError("lp_period must be < hp_period")
    P = _closes(bars)
    n = len(P)
    th = 0.707 * 2.0 * PI / hp_p
    al = (math.cos(th) + math.sin(th) - 1.0) / math.cos(th)
    hp = [0.0] * n
    for i in range(2, n):
        hp[i] = ((1 - al / 2) ** 2 * (P[i] - 2 * P[i - 1] + P[i - 2])
                 + 2 * (1 - al) * hp[i - 1] - (1 - al) ** 2 * hp[i - 2])
    c1, c2, c3 = _ss_coeffs(lp_p)
    f = [0.0] * n
    for i in range(n):
        if i < 2:
            f[i] = hp[i]
        else:
            f[i] = c1 * (hp[i] + hp[i - 1]) / 2.0 + c2 * f[i - 1] + c3 * f[i - 2]
    vals = [f[i] if i >= hp_p - 1 else None for i in range(n)]
    return _build(n, vals, alert_fn=_zero_alerts(vals))


def _reflex_like(params, bars, reflex):
    length = _param(params, "length", 20, 2, 10000, whole=True)
    P = _closes(bars)
    n = len(P)
    F = _supersmoother(P, length / 2.0)
    vals = [None] * n
    ms = 0.0
    for i in range(length, n):
        if reflex:
            slope = (F[i - length] - F[i]) / length
            s = 0.0
            for k in range(1, length + 1):
                s += (F[i] + k * slope) - F[i - k]
            s /= length
        else:
            s = 0.0
            for k in range(1, length + 1):
                s += F[i] - F[i - k]
            s /= length
        if abs(s) <= 1e-12 * max(abs(F[i]), 1.0):
            s = 0.0
        ms = 0.04 * s * s + 0.96 * ms
        vals[i] = s / math.sqrt(ms) if ms > 0.0 else 0.0
    return _build(n, vals, alert_fn=_zero_alerts(vals))


def _reflex(params, bars):
    return _reflex_like(params, bars, True)


def _trendflex(params, bars):
    return _reflex_like(params, bars, False)


def _correlation_trend(params, bars):
    length = _param(params, "length", 20, 2, 10000, whole=True)
    P = _closes(bars)
    n = len(P)
    ybar = -(length - 1) / 2.0
    dy = [-k - ybar for k in range(length)]
    syy = sum(d * d for d in dy)
    vals = [None] * n
    for i in range(length - 1, n):
        x = [P[i - k] for k in range(length)]
        if all(v == x[0] for v in x):
            vals[i] = 0.0
            continue
        xbar = sum(x) / length
        dx = [v - xbar for v in x]
        sxy = sum(a * b for a, b in zip(dx, dy))
        sxx = sum(a * a for a in dx)
        den = math.sqrt(sxx * syy)
        r = sxy / den if den > 0.0 else 0.0
        vals[i] = max(-1.0, min(1.0, r))
    return _build(n, vals, alert_fn=_zero_alerts(vals))


def _laguerre_series(P, gamma):
    n = len(P)
    vals = [None] * n
    prev = [0.0] * 4  # previous L0..L3
    last = None
    for i in range(n):
        if i == 0:
            L = [P[0]] * 4
        else:
            L = [0.0] * 4
            L[0] = (1 - gamma) * P[i] + gamma * prev[0]
            L[1] = -gamma * L[0] + prev[0] + gamma * prev[1]
            L[2] = -gamma * L[1] + prev[1] + gamma * prev[2]
            L[3] = -gamma * L[2] + prev[2] + gamma * prev[3]
        prev = L
        cu = cd = 0.0
        for a, b in ((L[0], L[1]), (L[1], L[2]), (L[2], L[3])):
            cu += max(a - b, 0.0)
            cd += max(b - a, 0.0)
        if cu + cd > 0.0:
            last = cu / (cu + cd)
        vals[i] = last
    return vals


def _laguerre_rsi(params, bars):
    gamma = _param(params, "gamma", 0.5, 0.0, 0.99)
    lower = _param(params, "lower", 0.2, 0.0, 1.0)
    upper = _param(params, "upper", 0.8, 0.0, 1.0)
    if not lower < upper:
        raise ValueError("lower must be < upper")
    P = _closes(bars)
    vals = _laguerre_series(P, gamma)

    def fn(i):
        a = []
        if _cross(vals[i - 1], vals[i], lower, lower)[0]:
            a.append("bull_extreme")
        if _cross(vals[i - 1], vals[i], upper, upper)[1]:
            a.append("bear_extreme")
        return a
    return _build(len(P), vals, alert_fn=fn)


def _cyber_cycle(params, bars):
    alpha = _param(params, "alpha", 0.07, 0.01, 1.0)
    P = _hl2(bars)
    n = len(P)
    sm = [0.0] * n
    for i in range(3, n):
        sm[i] = (P[i] + 2 * P[i - 1] + 2 * P[i - 2] + P[i - 3]) / 6.0
    cy = [0.0] * n
    for i in range(2, n):
        if i <= 5:
            cy[i] = (P[i] - 2 * P[i - 1] + P[i - 2]) / 4.0
        else:
            cy[i] = ((1 - alpha / 2) ** 2 * (sm[i] - 2 * sm[i - 1] + sm[i - 2])
                     + 2 * (1 - alpha) * cy[i - 1] - (1 - alpha) ** 2 * cy[i - 2])
    trig = [0.0] * n
    for i in range(1, n):
        trig[i] = cy[i - 1]
    warm = max(7, math.ceil(2.0 / alpha) + 1)
    vals = [cy[i] if i >= warm - 1 else None for i in range(n)]

    def fn(i):
        a = []
        up, dn = _cross(cy[i - 1], cy[i], trig[i - 1], trig[i])
        if up:
            a.append("bull_cross")
        if dn:
            a.append("bear_cross")
        return a
    return _build(n, vals, extras={"trigger": trig}, alert_fn=fn)


def _mama(params, bars):
    fast = _param(params, "fast_limit", 0.5, 0.01, 1.0)
    slow = _param(params, "slow_limit", 0.05, 0.01, 1.0)
    if not slow < fast:
        raise ValueError("slow_limit must be < fast_limit")
    P = _hl2(bars)
    n = len(P)
    names = ("smooth", "det", "q1", "i1", "ji", "jq", "i2", "q2", "re", "im",
             "period", "phase", "mama", "fama")
    S = {k: [0.0] * n for k in names}

    def g(arr, i):
        return arr[i] if i >= 0 else 0.0

    def H(arr, i):
        return (0.0962 * g(arr, i) + 0.5769 * g(arr, i - 2)
                - 0.5769 * g(arr, i - 4) - 0.0962 * g(arr, i - 6))

    # Smooth is seeded with the price for i < 3, the chain starts with bar 6 (0-based 6),
    # MAMA = FAMA = price before.
    start = 6
    for i in range(n):
        if i < start:
            S["mama"][i] = P[i]
            S["fama"][i] = P[i]
            if i < 3:
                S["smooth"][i] = P[i]
            else:
                S["smooth"][i] = (4 * P[i] + 3 * P[i - 1] + 2 * P[i - 2] + P[i - 3]) / 10.0
            continue
        k = 0.075 * S["period"][i - 1] + 0.54
        S["smooth"][i] = (4 * P[i] + 3 * P[i - 1] + 2 * P[i - 2] + P[i - 3]) / 10.0
        S["det"][i] = H(S["smooth"], i) * k
        S["q1"][i] = H(S["det"], i) * k
        S["i1"][i] = g(S["det"], i - 3)
        S["ji"][i] = H(S["i1"], i) * k
        S["jq"][i] = H(S["q1"], i) * k
        i2 = S["i1"][i] - S["jq"][i]
        q2 = S["q1"][i] + S["ji"][i]
        S["i2"][i] = 0.2 * i2 + 0.8 * S["i2"][i - 1]
        S["q2"][i] = 0.2 * q2 + 0.8 * S["q2"][i - 1]
        re = S["i2"][i] * S["i2"][i - 1] + S["q2"][i] * S["q2"][i - 1]
        im = S["i2"][i] * S["q2"][i - 1] - S["q2"][i] * S["i2"][i - 1]
        S["re"][i] = 0.2 * re + 0.8 * S["re"][i - 1]
        S["im"][i] = 0.2 * im + 0.8 * S["im"][i - 1]
        pp = S["period"][i - 1]
        p = pp
        if S["im"][i] != 0.0 and S["re"][i] != 0.0:
            ang = math.degrees(math.atan(S["im"][i] / S["re"][i]))
            if ang != 0.0:  # atan underflow to exactly 0 -> keep previous
                p = 360.0 / ang
        if p > 1.5 * pp:
            p = 1.5 * pp
        if p < 0.67 * pp:
            p = 0.67 * pp
        if p < 6.0:
            p = 6.0
        if p > 50.0:
            p = 50.0
        S["period"][i] = 0.2 * p + 0.8 * pp
        if S["i1"][i] != 0.0:
            S["phase"][i] = math.degrees(math.atan(S["q1"][i] / S["i1"][i]))
        else:
            S["phase"][i] = S["phase"][i - 1]
        delta = S["phase"][i - 1] - S["phase"][i]
        if delta < 1.0:
            delta = 1.0
        a = fast / delta
        if a < slow:
            a = slow
        S["mama"][i] = a * P[i] + (1 - a) * S["mama"][i - 1]
        S["fama"][i] = 0.5 * a * S["mama"][i] + (1 - 0.5 * a) * S["fama"][i - 1]

    vals = [S["mama"][i] if i >= 49 else None for i in range(n)]
    fama = S["fama"]

    def fn(i):
        a = []
        up, dn = _cross(S["mama"][i - 1], S["mama"][i], fama[i - 1], fama[i])
        if up:
            a.append("bull_cross")
        if dn:
            a.append("bear_cross")
        return a
    return _build(n, vals, extras={"fama": fama, "period": S["period"]}, alert_fn=fn)


# ------------------------------------------------------------------ interface
_DISPATCH = {
    "ultimate_smoother": _ultimate_smoother,
    "roofing_filter": _roofing_filter,
    "reflex": _reflex,
    "trendflex": _trendflex,
    "correlation_trend": _correlation_trend,
    "laguerre_rsi": _laguerre_rsi,
    "cyber_cycle": _cyber_cycle,
    "mama": _mama,
}


def compute(name, params, bars):
    if name not in _DISPATCH:
        raise ValueError("unknown indicator %r" % (name,))
    return [(None, {}) if row is None else row
            for row in _DISPATCH[name](dict(params or {}), bars)]
