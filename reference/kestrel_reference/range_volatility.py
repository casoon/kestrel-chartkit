"""Range volatility estimators: parkinson, rogers_satchell, yang_zhang.

Written from the formulas (Parkinson 1980, Rogers and Satchell 1991, Yang and Zhang 2000),
independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one estimator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)`. The value is the annualised volatility in percent. A window with a
non-positive price publishes nothing. Invalid parameters raise `ValueError`.
"""

import math

NAMES = ("parkinson", "rogers_satchell", "yang_zhang")

DEFAULT_P = 252


def _get_int(params, key, default, lo, hi=None):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key}: invalid")
    if isinstance(v, float):
        if not v.is_integer():
            raise ValueError(f"{key}: must be integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key}: must be integer")
    if v < lo or (hi is not None and v > hi):
        raise ValueError(f"{key}: out of range")
    return v


def _pos(*xs):
    return all(x > 0.0 for x in xs)


def _rs_term(o, h, l, c):
    return math.log(h / o) * math.log(h / c) + math.log(l / o) * math.log(l / c)


def _annual(var, p):
    return 100.0 * math.sqrt(var) * math.sqrt(p)


def _out(v):
    return {"value": v}


def _parkinson(params, bars):
    n = _get_int(params, 'period', 20, 1)
    p = _get_int(params, 'periods_per_year', DEFAULT_P, 1, 100000)
    res = [None] * len(bars)
    for i in range(n - 1, len(bars)):
        s = 0.0
        ok = True
        for j in range(i - n + 1, i + 1):
            h, l = bars[j]['high'], bars[j]['low']
            if not _pos(h, l):
                ok = False
                break
            s += math.log(h / l) ** 2
        if not ok:
            continue
        var = s / (4.0 * math.log(2.0) * n)
        res[i] = _out(_annual(var, p))
    return res


def _rogers(params, bars):
    n = _get_int(params, 'period', 20, 1)
    p = _get_int(params, 'periods_per_year', DEFAULT_P, 1, 100000)
    res = [None] * len(bars)
    for i in range(n - 1, len(bars)):
        s = 0.0
        ok = True
        for j in range(i - n + 1, i + 1):
            b = bars[j]
            if not _pos(b['open'], b['high'], b['low'], b['close']):
                ok = False
                break
            s += _rs_term(b['open'], b['high'], b['low'], b['close'])
        if not ok:
            continue
        var = max(0.0, s / n)
        res[i] = _out(_annual(var, p))
    return res


def _yang(params, bars):
    n = _get_int(params, 'period', 20, 2)
    p = _get_int(params, 'periods_per_year', DEFAULT_P, 1, 100000)
    res = [None] * len(bars)
    k = 0.34 / (1.34 + (n + 1) / (n - 1))
    for i in range(n, len(bars)):
        ok = _pos(bars[i - n]['close'])
        os_, cs, us = [], [], []
        if ok:
            for j in range(i - n + 1, i + 1):
                b = bars[j]
                if not _pos(b['open'], b['high'], b['low'], b['close']):
                    ok = False
                    break
                os_.append(math.log(b['open'] / bars[j - 1]['close']))
                cs.append(math.log(b['close'] / b['open']))
                us.append(_rs_term(b['open'], b['high'], b['low'], b['close']))
        if not ok:
            continue
        so = 0.0
        for x in os_:
            so += x
        om = so / n
        sc = 0.0
        for x in cs:
            sc += x
        cm = sc / n
        a = 0.0
        for x in os_:
            a += (x - om) ** 2
        b_ = 0.0
        for x in cs:
            b_ += (x - cm) ** 2
        su = 0.0
        for x in us:
            su += x
        var_o = a / (n - 1)
        var_c = b_ / (n - 1)
        var_rs = max(0.0, su / n)
        var = var_o + k * var_c + (1.0 - k) * var_rs
        res[i] = _out(_annual(var, p))
    return res


_DISPATCH = {"parkinson": _parkinson, "rogers_satchell": _rogers, "yang_zhang": _yang}


def compute(name, params, bars):
    if name not in _DISPATCH:
        raise ValueError("unknown indicator %r" % (name,))
    rows = [{"open": float(o), "high": float(h), "low": float(l), "close": float(c)}
            for (o, h, l, c, _v) in bars]
    return [(None, {}) if rec is None else ((rec["value"], {}), {})
            for rec in _DISPATCH[name](dict(params or {}), rows)]
