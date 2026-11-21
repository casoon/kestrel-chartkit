"""Oscillators and adaptive lines: schaff_trend_cycle, qqe, demarker, relative_momentum_index,
zlema, frama, squeeze_momentum, nadaraya_watson.

Written from the specifications, independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one indicator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)` with `alerts` mapping each alert kind to its strength (1.0). Invalid
parameters raise `ValueError`. The trend label of qqe is not an output field of the contract
format; it is covered through the alerts, which fire on every change of trend.
"""

import math

NAMES = (
    "schaff_trend_cycle", "qqe", "demarker", "relative_momentum_index",
    "zlema", "frama", "squeeze_momentum", "nadaraya_watson",
)


# ---------------------------------------------------------------- params
def _int(params, key, default, minimum):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key}: bool not allowed")
    if isinstance(v, float):
        if v != v or v != math.floor(v):
            raise ValueError(f"{key}: not an integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key}: not an integer")
    if v < minimum:
        raise ValueError(f"{key}: must be >= {minimum}")
    return v


def _num(params, key, default):
    v = params.get(key, default)
    if isinstance(v, bool) or not isinstance(v, (int, float)):
        raise ValueError(f"{key}: not a number")
    v = float(v)
    if v != v or math.isinf(v):
        raise ValueError(f"{key}: not finite")
    return v


# ---------------------------------------------------------------- helpers
def _ema_alpha(n):
    return 2.0 / (n + 1)


def _cross_above(pa, pb, a, b):
    tol = 1e-9 * max(1.0, abs(pa), abs(pb), abs(a), abs(b))
    return pa <= pb + tol and a > b + tol


def _cross_below(pa, pb, a, b):
    tol = 1e-9 * max(1.0, abs(pa), abs(pb), abs(a), abs(b))
    return pa >= pb - tol and a < b - tol


def _rma_series(xs, first, n):
    """RMA over inputs xs[first:]; result list aligned with xs (None before first+n-1)."""
    out = [None] * len(xs)
    r = None
    for i in range(first, len(xs)):
        k = i - first + 1  # number of inputs so far
        if k < n:
            continue
        if k == n:
            s = 0.0
            for j in range(first, i + 1):
                s += xs[j]
            r = s / n
        else:
            r = (r * (n - 1) + xs[i]) / n
        out[i] = r
    return out


def _rsi_series(closes, n):
    cnt = len(closes)
    up = [0.0] * cnt
    dn = [0.0] * cnt
    for i in range(1, cnt):
        d = closes[i] - closes[i - 1]
        up[i] = max(d, 0.0)
        dn[i] = max(-d, 0.0)
    U = _rma_series(up, 1, n)
    D = _rma_series(dn, 1, n)
    out = [None] * cnt
    for i in range(cnt):
        if U[i] is not None:
            den = U[i] + D[i]
            out[i] = 100.0 * U[i] / den if den != 0.0 else 50.0
    return out


def _sma_at(xs, i, n):
    s = 0.0
    for j in range(i - n + 1, i + 1):
        s += xs[j]
    return s / n


def _linreg_end(ys):
    n = len(ys)
    xbar = (n - 1) / 2.0
    ybar = 0.0
    for y in ys:
        ybar += y
    ybar /= n
    num = 0.0
    den = 0.0
    for k, y in enumerate(ys):
        num += (k - xbar) * (y - ybar)
        den += (k - xbar) * (k - xbar)
    b = num / den
    return ybar + b * (n - 1 - xbar)


def _finish(vals, extras, alert_fn):
    """vals: list of float|None; extras: list of dict|None; alert_fn(i, prev_rec, rec)->list."""
    out = []
    prev = None
    for i, v in enumerate(vals):
        if v is None:
            out.append(None)
            prev = None
            continue
        rec = {"value": v}
        rec.update(extras[i] or {})
        rec["alerts"] = sorted(alert_fn(i, prev, rec)) if prev is not None else []
        out.append(rec)
        prev = rec
    return out


def _level_alerts(lower, upper):
    def fn(i, prev, rec):
        a = []
        if _cross_above(prev["value"], lower, rec["value"], lower):
            a.append("bull")
        if _cross_below(prev["value"], upper, rec["value"], upper):
            a.append("bear")
        return a
    return fn


# ---------------------------------------------------------------- entries
def _stc(params, bars):
    fast = _int(params, "fast", 23, 1)
    slow = _int(params, "slow", 50, 1)
    cycle = _int(params, "cycle", 10, 2)
    factor = _num(params, "factor", 0.5)
    lower = _num(params, "lower", 25.0)
    upper = _num(params, "upper", 75.0)
    if slow <= fast:
        raise ValueError("slow must be > fast")
    if not (0.0 < factor <= 1.0):
        raise ValueError("factor")
    if not (0.0 <= lower < upper <= 100.0):
        raise ValueError("lower/upper")
    n = len(bars)
    C = [b["close"] for b in bars]
    af, as_ = _ema_alpha(fast), _ema_alpha(slow)
    macd = [0.0] * n
    eslow = [0.0] * n
    ef = es = None
    for i in range(n):
        if i == 0:
            ef, es = C[0], C[0]
        else:
            ef = af * C[i] + (1 - af) * ef
            es = as_ * C[i] + (1 - as_) * es
        macd[i] = ef - es
        eslow[i] = es
    pf = [None] * n
    k1_prev = None
    for i in range(cycle - 1, n):
        w = macd[i - cycle + 1:i + 1]
        lo, hi = min(w), max(w)
        if hi - lo > 1e-12 * max(1.0, abs(eslow[i])):
            k1 = 100.0 * (macd[i] - lo) / (hi - lo)
        else:
            k1 = k1_prev if k1_prev is not None else 50.0
        k1_prev = k1
        if i == cycle - 1:
            pf[i] = k1
        else:
            pf[i] = pf[i - 1] + factor * (k1 - pf[i - 1])
    stc = [None] * n
    k2_prev = None
    for i in range(2 * (cycle - 1), n):
        w = pf[i - cycle + 1:i + 1]
        lo, hi = min(w), max(w)
        if hi - lo > 1e-10:
            k2 = 100.0 * (pf[i] - lo) / (hi - lo)
        else:
            k2 = k2_prev if k2_prev is not None else pf[i]
        k2_prev = k2
        if i == 2 * (cycle - 1):
            stc[i] = k2
        else:
            stc[i] = stc[i - 1] + factor * (k2 - stc[i - 1])
    start = max(slow - 1, 2 * (cycle - 1))
    vals = [stc[i] if i >= start else None for i in range(n)]
    return _finish(vals, [None] * n, _level_alerts(lower, upper))


def _qqe(params, bars):
    rl = _int(params, "rsi_len", 14, 2)
    sm = _int(params, "smoothing", 5, 1)
    factor = _num(params, "factor", 4.236)
    if not factor > 0.0:
        raise ValueError("factor")
    n = len(bars)
    C = [b["close"] for b in bars]
    rsi = _rsi_series(C, rl)
    asm = _ema_alpha(sm)
    W = 2 * rl - 1
    aw = _ema_alpha(W)
    M = [None] * n
    for i in range(rl, n):
        M[i] = rsi[i] if i == rl else asm * rsi[i] + (1 - asm) * M[i - 1]
    vals = [None] * n
    ext = [None] * n
    s1 = s2 = None
    lng = sht = None
    trend = 0
    for i in range(rl + 1, n):
        a = abs(M[i] - M[i - 1])
        if i == rl + 1:
            s1 = a
            s2 = s1
        else:
            s1 = aw * a + (1 - aw) * s1
            s2 = aw * s1 + (1 - aw) * s2
        dar = factor * s2
        nl = M[i] - dar
        ns = M[i] + dar
        if i == rl + 1:
            lng, sht, trend = nl, ns, 0
        else:
            pl, ps = lng, sht
            if M[i - 1] > pl and M[i] > pl:
                lng = max(pl, nl)
            else:
                lng = nl
            if M[i - 1] < ps and M[i] < ps:
                sht = min(ps, ns)
            else:
                sht = ns
            tol_up = 1e-9 * max(1.0, abs(M[i]), abs(ps))
            tol_dn = 1e-9 * max(1.0, abs(M[i]), abs(pl))
            if trend != 1 and M[i] > ps + tol_up:
                trend = 1
            elif trend != -1 and M[i] < pl - tol_dn:
                trend = -1
        trail = lng if trend == 1 else (sht if trend == -1 else M[i])
        vals[i] = M[i]
        ext[i] = {"trail": trail, "trend": float(trend)}

    def alerts(i, prev, rec):
        a = []
        if rec["trend"] != prev["trend"]:
            if rec["trend"] == 1.0:
                a.append("bull")
            elif rec["trend"] == -1.0:
                a.append("bear")
        return a
    return _finish(vals, ext, alerts)


def _demarker(params, bars):
    p = _int(params, "period", 14, 1)
    lower = _num(params, "lower", 0.3)
    upper = _num(params, "upper", 0.7)
    if not (0.0 <= lower < upper <= 1.0):
        raise ValueError("lower/upper")
    n = len(bars)
    up = [0.0] * n
    dn = [0.0] * n
    for i in range(1, n):
        up[i] = max(bars[i]["high"] - bars[i - 1]["high"], 0.0)
        dn[i] = max(bars[i - 1]["low"] - bars[i]["low"], 0.0)
    vals = [None] * n
    for i in range(p, n):
        su = 0.0
        sd = 0.0
        for j in range(i - p + 1, i + 1):
            su += up[j]
            sd += dn[j]
        vals[i] = su / (su + sd) if su + sd != 0.0 else 0.5
    return _finish(vals, [None] * n, _level_alerts(lower, upper))


def _rmi(params, bars):
    length = _int(params, "length", 20, 1)
    m = _int(params, "momentum", 5, 1)
    lower = _num(params, "lower", 30.0)
    upper = _num(params, "upper", 70.0)
    if not (0.0 <= lower < upper <= 100.0):
        raise ValueError("lower/upper")
    n = len(bars)
    C = [b["close"] for b in bars]
    up = [0.0] * n
    dn = [0.0] * n
    for i in range(m, n):
        d = C[i] - C[i - m]
        up[i] = max(d, 0.0)
        dn[i] = max(-d, 0.0)
    U = _rma_series(up, m, length)
    D = _rma_series(dn, m, length)
    vals = [None] * n
    for i in range(n):
        if U[i] is not None:
            den = U[i] + D[i]
            vals[i] = 100.0 * U[i] / den if den != 0.0 else 50.0
    return _finish(vals, [None] * n, _level_alerts(lower, upper))


def _zlema(params, bars):
    p = _int(params, "period", 20, 1)
    n = len(bars)
    C = [b["close"] for b in bars]
    lag = (p - 1) // 2
    a = _ema_alpha(p)
    Z = [None] * n
    for i in range(lag, n):
        d = C[i] + (C[i] - C[i - lag])
        Z[i] = d if i == lag else a * d + (1 - a) * Z[i - 1]

    def alerts(i, prev, rec):
        out = []
        if _cross_above(C[i - 1], prev["value"], C[i], rec["value"]):
            out.append("bull")
        if _cross_below(C[i - 1], prev["value"], C[i], rec["value"]):
            out.append("bear")
        return out
    return _finish(Z, [None] * n, alerts)


def _frama(params, bars):
    p = _int(params, "period", 16, 4)
    if p % 2 != 0:
        raise ValueError("period must be even")
    h = p // 2
    n = len(bars)
    H = [b["high"] for b in bars]
    L = [b["low"] for b in bars]
    P = [(H[i] + L[i]) / 2.0 for i in range(n)]
    D = [None] * n
    F = [None] * n
    d_prev = None
    ln2 = math.log(2.0)
    for i in range(p - 1, n):
        n1 = (max(H[i - h + 1:i + 1]) - min(L[i - h + 1:i + 1])) / h
        n2 = (max(H[i - p + 1:i - h + 1]) - min(L[i - p + 1:i - h + 1])) / h
        n3 = (max(H[i - p + 1:i + 1]) - min(L[i - p + 1:i + 1])) / p
        if n1 > 0 and n2 > 0 and n3 > 0:
            d = (math.log(n1 + n2) - math.log(n3)) / ln2
        else:
            d = d_prev if d_prev is not None else 1.0
        d_prev = d
        D[i] = d
        alpha = math.exp(-4.6 * (d - 1.0))
        alpha = min(max(alpha, 0.01), 1.0)
        if i == p - 1:
            F[i] = P[i]
        else:
            F[i] = alpha * P[i] + (1 - alpha) * F[i - 1]
    vals = [F[i] if i >= p else None for i in range(n)]
    ext = [{"dimension": D[i]} if D[i] is not None else None for i in range(n)]
    return _finish(vals, ext, lambda i, prev, rec: [])


def _squeeze(params, bars):
    length = _int(params, "length", 20, 2)
    bbm = _num(params, "bb_mult", 2.0)
    kcl = _int(params, "kc_length", 20, 2)
    kcm = _num(params, "kc_mult", 1.5)
    if not bbm > 0.0 or not kcm > 0.0:
        raise ValueError("multipliers must be > 0")
    n = len(bars)
    H = [b["high"] for b in bars]
    L = [b["low"] for b in bars]
    C = [b["close"] for b in bars]
    TR = [0.0] * n
    for i in range(n):
        if i == 0:
            TR[i] = H[0] - L[0]
        else:
            TR[i] = max(H[i] - L[i], abs(H[i] - C[i - 1]), abs(L[i] - C[i - 1]))
    vals = [None] * n
    ext = [None] * n
    start = max(2 * kcl - 2, length - 1)
    delta = [None] * n
    for i in range(kcl - 1, n):
        ma = _sma_at(C, i, kcl)
        mid = ((max(H[i - kcl + 1:i + 1]) + min(L[i - kcl + 1:i + 1])) / 2.0 + ma) / 2.0
        delta[i] = C[i] - mid
    for i in range(start, n):
        basis = _sma_at(C, i, length)
        s = 0.0
        for j in range(i - length + 1, i + 1):
            s += (C[j] - basis) ** 2
        sd = math.sqrt(s / length)
        ubb = basis + bbm * sd
        lbb = basis - bbm * sd
        ma = _sma_at(C, i, kcl)
        rng = _sma_at(TR, i, kcl)
        ukc = ma + kcm * rng
        lkc = ma - kcm * rng
        if lbb > lkc and ubb < ukc:
            sq = 1.0
        elif lbb < lkc and ubb > ukc:
            sq = -1.0
        else:
            sq = 0.0
        vals[i] = _linreg_end(delta[i - kcl + 1:i + 1])
        ext[i] = {"squeeze": sq}

    def alerts(i, prev, rec):
        a = []
        if prev["squeeze"] == 1.0 and rec["squeeze"] != 1.0:
            a.append("squeeze_fired")
        if _cross_above(prev["value"], 0.0, rec["value"], 0.0):
            a.append("bull")
        if _cross_below(prev["value"], 0.0, rec["value"], 0.0):
            a.append("bear")
        return a
    return _finish(vals, ext, alerts)


def _nw(params, bars):
    bw = _num(params, "bandwidth", 8.0)
    lb = _int(params, "lookback", 50, 2)
    mult = _num(params, "mult", 3.0)
    if not bw > 0.0:
        raise ValueError("bandwidth")
    if mult < 0.0:
        raise ValueError("mult")
    n = len(bars)
    C = [b["close"] for b in bars]
    w = [math.exp(-(j * j) / (2.0 * bw * bw)) for j in range(lb)]
    wsum = 0.0
    for x in w:
        wsum += x
    y = [None] * n
    for i in range(lb - 1, n):
        s = 0.0
        for j in range(lb):
            s += w[j] * C[i - j]
        y[i] = s / wsum
    vals = [None] * n
    ext = [None] * n
    for i in range(2 * lb - 2, n):
        s = 0.0
        for k in range(i - lb + 1, i + 1):
            s += abs(C[k] - y[k])
        mae = s / lb
        vals[i] = y[i]
        ext[i] = {"upper": y[i] + mult * mae, "lower": y[i] - mult * mae}

    def alerts(i, prev, rec):
        a = []
        if _cross_above(C[i - 1], prev["upper"], C[i], rec["upper"]):
            a.append("above_upper")
        if _cross_below(C[i - 1], prev["lower"], C[i], rec["lower"]):
            a.append("below_lower")
        return a
    return _finish(vals, ext, alerts)




_DISPATCH = {
    "schaff_trend_cycle": _stc,
    "qqe": _qqe,
    "demarker": _demarker,
    "relative_momentum_index": _rmi,
    "zlema": _zlema,
    "frama": _frama,
    "squeeze_momentum": _squeeze,
    "nadaraya_watson": _nw,
}

# Working values that are not output fields: the trend of qqe is a state label, covered through
# its alerts.
_HIDDEN = {"qqe": ("trend",)}


def compute(name, params, bars):
    if name not in _DISPATCH:
        raise ValueError("unknown indicator %r" % (name,))
    rows = [{"open": float(o), "high": float(h), "low": float(l), "close": float(c)}
            for (o, h, l, c, _v) in bars]
    hidden = _HIDDEN.get(name, ())
    result = []
    for rec in _DISPATCH[name](dict(params or {}), rows):
        if rec is None:
            result.append((None, {}))
            continue
        extras = {k: v for k, v in rec.items()
                  if k not in ("value", "alerts") and k not in hidden}
        result.append(((rec["value"], extras), {kind: 1.0 for kind in rec["alerts"]}))
    return result
