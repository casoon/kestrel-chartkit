"""Trailing lines and trend channels: range_filter, most, ott, halftrend, ssl_channel.

Written from the specifications, independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one indicator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)` with `alerts` mapping each alert kind to its strength (1.0). Invalid
parameters raise `ValueError`.
"""

import math


# ---------------------------------------------------------------- params
def _int_param(params, key, default, minimum):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key}: bool not allowed")
    if isinstance(v, float):
        if not math.isfinite(v) or v != int(v):
            raise ValueError(f"{key}: must be an integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key}: must be an integer")
    if v < minimum:
        raise ValueError(f"{key}: must be >= {minimum}")
    return v


def _float_param(params, key, default):
    v = params.get(key, default)
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise ValueError(f"{key}: must be a number")
    v = float(v)
    if not math.isfinite(v):
        raise ValueError(f"{key}: must be finite")
    return v


def _check_keys(params, allowed):
    for k in params:
        if k not in allowed:
            raise ValueError(f"unknown parameter {k!r}")


# ---------------------------------------------------------------- blocks
def _ema_step(prev, x, n):
    alpha = 2.0 / (n + 1)
    return x if prev is None else alpha * x + (1.0 - alpha) * prev


def _rma_series(xs, n):
    """RMA(n) over xs; None for first n-1 inputs."""
    out = [None] * len(xs)
    r = None
    for i, x in enumerate(xs):
        if i < n - 1:
            continue
        if i == n - 1:
            r = sum(xs[:n]) / n
        else:
            r = (r * (n - 1) + x) / n
        out[i] = r
    return out


def _true_range(bars):
    tr = []
    for i, b in enumerate(bars):
        if i == 0:
            tr.append(b["high"] - b["low"])
        else:
            pc = bars[i - 1]["close"]
            tr.append(max(b["high"] - b["low"], abs(b["high"] - pc), abs(b["low"] - pc)))
    return tr


def _sma_series(xs, n):
    out = [None] * len(xs)
    for i in range(n - 1, len(xs)):
        out[i] = sum(xs[i - n + 1:i + 1]) / n
    return out


def _crosses_above(pa, pb, a, b):
    tol = 1e-9 * max(1.0, abs(pa), abs(pb), abs(a), abs(b))
    return pa <= pb + tol and a > b + tol


def _crosses_below(pa, pb, a, b):
    tol = 1e-9 * max(1.0, abs(pa), abs(pb), abs(a), abs(b))
    return pa >= pb - tol and a < b - tol


def _state_alerts(out, state_key, bull_val=1.0, bear_val=-1.0):
    """Add bull/bear alerts for 'becomes +1/-1' on published bars whose previous
    bar was published with a different state."""
    for i, o in enumerate(out):
        if o is None:
            continue
        al = []
        if i > 0 and out[i - 1] is not None:
            s, p = o[state_key], out[i - 1][state_key]
            if s != p:
                if s == bull_val:
                    al.append("bull")
                elif s == bear_val:
                    al.append("bear")
        o["alerts"] = sorted(al)


# ---------------------------------------------------------------- range_filter
def _range_filter(params, bars):
    _check_keys(params, {"period", "mult"})
    period = _int_param(params, "period", 100, 1)
    mult = _float_param(params, "mult", 3.0)
    if not mult > 0:
        raise ValueError("mult must be > 0")
    n = len(bars)
    out = [None] * n
    if n < 2:
        return out
    x = [b["close"] for b in bars]
    s1 = s2 = None
    f_prev = x[0]
    d_prev = 0.0
    for i in range(1, n):
        a = abs(x[i] - x[i - 1])
        s1 = _ema_step(s1, a, period)
        s2 = _ema_step(s2, s1, 2 * period - 1)
        r = mult * s2
        if x[i] > f_prev:
            f = max(f_prev, x[i] - r)
        else:
            f = min(f_prev, x[i] + r)
        tol = 1e-9 * max(1.0, abs(f), abs(f_prev))
        if f > f_prev + tol:
            d = 1.0
        elif f < f_prev - tol:
            d = -1.0
        else:
            d = d_prev
        out[i] = {"value": f, "upper": f + r, "lower": f - r, "direction": d}
        f_prev, d_prev = f, d
    _state_alerts(out, "direction")
    return out


# ---------------------------------------------------------------- most / ott core
def _stops(m_series, percent):
    """Stops and direction as in `most` for M series. Returns (ls, ss, d) lists."""
    n = len(m_series)
    ls = [0.0] * n
    ss = [0.0] * n
    d = [0.0] * n
    for i in range(n):
        m = m_series[i]
        fark = m * percent / 100.0
        ls_raw = m - fark
        ss_raw = m + fark
        if i == 0:
            ls[0], ss[0], d[0] = ls_raw, ss_raw, 0.0
            continue
        ls[i] = max(ls_raw, ls[i - 1]) if m > ls[i - 1] else ls_raw
        ss[i] = min(ss_raw, ss[i - 1]) if m < ss[i - 1] else ss_raw
        if d[i - 1] != 1.0 and m > ss[i - 1]:
            d[i] = 1.0
        elif d[i - 1] != -1.0 and m < ls[i - 1]:
            d[i] = -1.0
        else:
            d[i] = d[i - 1]
    return ls, ss, d


def _most(params, bars):
    _check_keys(params, {"length", "percent"})
    length = _int_param(params, "length", 3, 1)
    percent = _float_param(params, "percent", 2.0)
    if not (0 < percent < 100):
        raise ValueError("percent must satisfy 0 < p < 100")
    n = len(bars)
    m = []
    e = None
    for b in bars:
        e = _ema_step(e, b["close"], length)
        m.append(e)
    ls, ss, d = _stops(m, percent)
    out = [None] * n
    for i in range(n):
        if d[i] == 0.0:
            continue
        v = ls[i] if d[i] == 1.0 else ss[i]
        out[i] = {"value": v, "ema": m[i], "direction": d[i]}
    _state_alerts(out, "direction")
    return out


def _ott(params, bars):
    _check_keys(params, {"length", "percent", "cmo_len"})
    length = _int_param(params, "length", 2, 1)
    percent = _float_param(params, "percent", 1.4)
    if not (0 < percent < 100):
        raise ValueError("percent must satisfy 0 < p < 100")
    cmo_len = _int_param(params, "cmo_len", 9, 1)
    n = len(bars)
    c = [b["close"] for b in bars]
    u = [0.0] * n
    dn = [0.0] * n
    for i in range(1, n):
        u[i] = max(c[i] - c[i - 1], 0.0)
        dn[i] = max(c[i - 1] - c[i], 0.0)
    a = 2.0 / (length + 1)
    v = []
    for i in range(n):
        lo = max(0, i - cmo_len + 1)
        su = sum(u[lo:i + 1])
        sd = sum(dn[lo:i + 1])
        k = (su - sd) / (su + sd) if (su + sd) != 0 else 0.0
        if i == 0:
            v.append(c[0])
        else:
            w = a * abs(k)
            v.append(w * c[i] + (1.0 - w) * v[i - 1])
    ls, ss, d = _stops(v, percent)
    out = [None] * n
    ott = [None] * n
    for i in range(n):
        if i < cmo_len or d[i] == 0.0:
            continue
        mt = ls[i] if d[i] == 1.0 else ss[i]
        o = mt * (200.0 + percent) / 200.0 if v[i] > mt else mt * (200.0 - percent) / 200.0
        ott[i] = o
        out[i] = {"value": o, "var": v[i], "stop": mt, "direction": d[i]}
    for i in range(n):
        if out[i] is None:
            continue
        al = []
        if i >= 3 and all(out[j] is not None for j in (i - 3, i - 2, i - 1, i)):
            pa, pb, aa, bb = v[i - 1], ott[i - 3], v[i], ott[i - 2]
            if _crosses_above(pa, pb, aa, bb):
                al.append("bull")
            if _crosses_below(pa, pb, aa, bb):
                al.append("bear")
        out[i]["alerts"] = sorted(al)
    return out


# ---------------------------------------------------------------- halftrend
def _halftrend(params, bars):
    _check_keys(params, {"amplitude", "deviation", "atr_len"})
    amp = _int_param(params, "amplitude", 2, 1)
    deviation = _float_param(params, "deviation", 2.0)
    if deviation < 0:
        raise ValueError("deviation must be >= 0")
    atr_len = _int_param(params, "atr_len", 100, 1)
    n = len(bars)
    out = [None] * n
    atr = _rma_series(_true_range(bars), atr_len)
    s = max(amp - 1, 1)
    if n <= s:
        return out
    H = [b["high"] for b in bars]
    L = [b["low"] for b in bars]
    C = [b["close"] for b in bars]
    max_low = L[s - 1]
    min_high = H[s - 1]
    trend_prev = 0
    up = down = None
    for i in range(s, n):
        w = range(i - amp + 1, i + 1)
        hp = max(H[j] for j in w)
        lp = min(L[j] for j in w)
        hma = sum(H[j] for j in w) / amp
        lma = sum(L[j] for j in w) / amp
        watch_down = trend_prev in (1, 0)
        watch_up = trend_prev in (-1, 0)
        if watch_down:
            max_low = max(lp, max_low)
        if watch_up:
            min_high = min(hp, min_high)
        trend = trend_prev
        if watch_down and hma < max_low and C[i] < L[i - 1]:
            trend = -1
            min_high = hp
        elif watch_up and lma > min_high and C[i] > H[i - 1]:
            trend = 1
            max_low = lp
        if trend == 1:
            if trend_prev == -1:
                up = down
            elif trend_prev == 0:
                up = max_low
            else:
                up = max(max_low, up)
        elif trend == -1:
            if trend_prev == 1:
                down = up
            elif trend_prev == 0:
                down = min_high
            else:
                down = min(min_high, down)
        if trend != 0 and i >= atr_len - 1:
            ht = up if trend == 1 else down
            dev = deviation * atr[i] / 2.0
            out[i] = {"value": ht, "trend": float(trend),
                      "atr_high": ht + dev, "atr_low": ht - dev}
        trend_prev = trend
    _state_alerts(out, "trend")
    return out


# ---------------------------------------------------------------- ssl_channel
def _ssl_channel(params, bars):
    _check_keys(params, {"period"})
    period = _int_param(params, "period", 10, 1)
    n = len(bars)
    sh = _sma_series([b["high"] for b in bars], period)
    sl = _sma_series([b["low"] for b in bars], period)
    out = [None] * n
    hlv = 0.0
    for i in range(period - 1, n):
        c = bars[i]["close"]
        if c > sh[i]:
            hlv = 1.0
        elif c < sl[i]:
            hlv = -1.0
        if hlv == 0.0:
            continue
        up = sl[i] if hlv < 0 else sh[i]
        dn = sh[i] if hlv < 0 else sl[i]
        out[i] = {"value": up, "ssl_down": dn, "hlv": hlv}
    _state_alerts(out, "hlv")
    return out




_DISPATCH = {
    "range_filter": _range_filter,
    "most": _most,
    "ott": _ott,
    "halftrend": _halftrend,
    "ssl_channel": _ssl_channel,
}


def compute(name, params, bars):
    if name not in _DISPATCH:
        raise ValueError("unknown indicator %r" % (name,))
    rows = [{"open": float(o), "high": float(h), "low": float(l), "close": float(c)}
            for (o, h, l, c, _v) in bars]
    result = []
    for rec in _DISPATCH[name](dict(params or {}), rows):
        if rec is None:
            result.append((None, {}))
            continue
        extras = {k: v for k, v in rec.items() if k not in ("value", "alerts")}
        result.append(((rec["value"], extras), {kind: 1.0 for kind in rec["alerts"]}))
    return result
