"""Return statistics: variance_ratio, autocorrelation, hurst_rs, ar1_half_life.

Written from the formulas (Lo and MacKinlay 1988, Box and Jenkins, Hurst 1951 with Mandelbrot and
Wallis 1969, least-squares AR(1)), independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one statistic over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)`; the extras are `z` for variance_ratio and `phi` for ar1_half_life.
Log returns are logarithms of price ratios; a window with a non-positive price publishes nothing.
Invalid parameters raise `ValueError`.
"""

import math

NAMES = ("variance_ratio", "autocorrelation", "hurst_rs", "ar1_half_life")

def _int_param(params, key, default):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f'{key} must be an integer')
    if isinstance(v, float):
        if v != int(v):
            raise ValueError(f'{key} must be an integer')
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f'{key} must be an integer')
    return v


def _returns(closes):
    """r_i for i>=1; None where a log is undefined (non-positive price)."""
    out = [None] * len(closes)
    for i in range(1, len(closes)):
        a, b = closes[i - 1], closes[i]
        if a > 0 and b > 0:
            out[i] = math.log(b / a)
    return out


def _autocorrelation(closes, params):
    n = _int_param(params, 'period', 50)
    k = _int_param(params, 'lag', 1)
    if n < 3:
        raise ValueError('period must be >= 3')
    if k < 1:
        raise ValueError('lag must be >= 1')
    if k >= n:
        raise ValueError('lag must be < period')
    res = [None] * len(closes)
    for i in range(n, len(closes)):
        # returns r_{i-n+1}..r_i need closes i-n..i all positive
        if any(not (c > 0) for c in closes[i - n:i + 1]):
            continue
        x = [math.log(closes[j] / closes[j - 1]) for j in range(i - n + 1, i + 1)]
        s = 0.0
        for v in x:
            s += v
        mu = s / n
        den = 0.0
        for v in x:
            den += (v - mu) ** 2
        if den == 0.0:
            continue
        num = 0.0
        for t in range(k, n):  # 0-based t = (1-based t) - 1
            num += (x[t] - mu) * (x[t - k] - mu)
        val = num / den
        val = max(-1.0, min(1.0, val))
        res[i] = {'value': val, 'alerts': []}
    return res


def _rs_of_chunk(a):
    n = len(a)
    s = 0.0
    for v in a:
        s += v
    mean = s / n
    y = 0.0
    ymax = -math.inf
    ymin = math.inf
    ss = 0.0
    for v in a:
        y += v - mean
        ymax = max(ymax, y)
        ymin = min(ymin, y)
        ss += (v - mean) ** 2
    r = ymax - ymin
    if r == 0.0:
        return None
    sd = math.sqrt(ss / n)
    return r / sd


def _hurst(closes, params):
    n_ = _int_param(params, 'period', 128)
    m = _int_param(params, 'min_chunk', 8)
    if n_ < 8:
        raise ValueError('period must be >= 8')
    if m < 4:
        raise ValueError('min_chunk must be >= 4')
    if n_ < 2 * m:
        raise ValueError('period must be >= 2*min_chunk')
    sizes = []
    j = 0
    while True:
        n = n_ // (2 ** j)
        if n < m:
            break
        sizes.append(n)
        j += 1
    res = [None] * len(closes)
    for i in range(n_, len(closes)):
        if any(not (c > 0) for c in closes[i - n_:i + 1]):
            continue
        x = [math.log(closes[q] / closes[q - 1]) for q in range(i - n_ + 1, i + 1)]
        pts = []
        for n in sizes:
            c = n_ // n
            vals = []
            for ch in range(c):
                end = n_ - ch * n
                chunk = x[end - n:end]
                rs = _rs_of_chunk(chunk)
                if rs is not None:
                    vals.append(rs)
            if not vals:
                continue
            tot = 0.0
            for v in vals:
                tot += v
            pts.append((math.log(n), math.log(tot / len(vals))))
        if len(pts) < 2:
            continue
        k = len(pts)
        mx = sum(p[0] for p in pts) / k
        my = sum(p[1] for p in pts) / k
        sxx = sum((p[0] - mx) ** 2 for p in pts)
        sxy = sum((p[0] - mx) * (p[1] - my) for p in pts)
        res[i] = {'value': sxy / sxx, 'alerts': []}
    return res


def _ar1(closes, params):
    n = _int_param(params, 'period', 100)
    if n < 3:
        raise ValueError('period must be >= 3')
    res = [None] * len(closes)
    for i in range(n - 1, len(closes)):
        y = closes[i - n + 1:i + 1]
        xs = y[:-1]
        zs = y[1:]
        cnt = n - 1
        sx = 0.0
        sz = 0.0
        for a in xs:
            sx += a
        for b in zs:
            sz += b
        xm = sx / cnt
        zm = sz / cnt
        sxx = 0.0
        sxz = 0.0
        for a, b in zip(xs, zs):
            sxx += (a - xm) ** 2
            sxz += (a - xm) * (b - zm)
        if sxx == 0.0:
            continue
        phi = sxz / sxx
        if phi <= 0.0 or phi >= 1.0 - 1e-9:
            continue
        res[i] = {'value': -math.log(2.0) / math.log(phi), 'phi': phi, 'alerts': []}
    return res


def _vr(params, bars):
    N = _int_param(params, 'period', 100)
    q = _int_param(params, 'lag', 4)
    if N < 4:
        raise ValueError('period must be >= 4')
    if q < 2:
        raise ValueError('lag must be >= 2')
    if q > N / 2:
        raise ValueError("lag must be <= period/2")
    res = [None] * len(bars)
    m = q * (N - q + 1) * (1.0 - q / N)
    zden = math.sqrt(2.0 * (2 * q - 1) * (q - 1) / (3.0 * q))
    for i in range(N, len(bars)):
        cl = [bars[i - N + t]['close'] for t in range(N + 1)]
        if not all(c > 0.0 for c in cl):
            continue
        mu = math.log(cl[N] / cl[0]) / N
        sa = 0.0
        for t in range(1, N + 1):
            sa += (math.log(cl[t] / cl[t - 1]) - mu) ** 2
        sa2 = sa / (N - 1)
        if sa2 == 0.0:
            continue
        sq = 0.0
        for t in range(q, N + 1):
            sq += (math.log(cl[t] / cl[t - q]) - q * mu) ** 2
        sq2 = sq / m
        vr = sq2 / sa2
        z = math.sqrt(N) * (vr - 1.0) / zden
        res[i] = {"value": vr, "z": z, "alerts": []}
    return res


_DISPATCH = {
    "variance_ratio": _vr,
    "autocorrelation": lambda params, bars: _autocorrelation(_closes(bars), params),
    "hurst_rs": lambda params, bars: _hurst(_closes(bars), params),
    "ar1_half_life": lambda params, bars: _ar1(_closes(bars), params),
}


def _closes(bars):
    return [b["close"] for b in bars]


def compute(name, params, bars):
    if name not in _DISPATCH:
        raise ValueError("unknown indicator %r" % (name,))
    rows = [{"close": float(c)} for (_o, _h, _l, c, _v) in bars]
    result = []
    for rec in _DISPATCH[name](dict(params or {}), rows):
        if rec is None:
            result.append((None, {}))
            continue
        extras = {k: v for k, v in rec.items() if k not in ("value", "alerts")}
        result.append(((rec["value"], extras), {}))
    return result
