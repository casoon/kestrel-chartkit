"""Pine-style series helpers for the ports of the WavesUnchained Pine scripts.

Every series is a full list with one entry per bar; `None` is Pine's `na`. Unless a helper says
otherwise, a value exists only when every input it reads exists ("strict na"), which is the
convention the Rust ports follow as well. Written from the Pine Script v6 reference, not from the
Rust code.
"""

import math


def ok(*vals):
    return all(v is not None for v in vals)


def clamp(v, lo, hi):
    return max(lo, min(hi, v))


def clamp01(v):
    return clamp(v, 0.0, 1.0)


def safe_div(n, d, fallback=0.0):
    return n / d if abs(d) > 1e-10 else fallback


def at(s, t, k):
    """s[t - k] with Pine's history operator: None before the series start."""
    j = t - k
    return s[j] if 0 <= j < len(s) else None


def smap(f, *series):
    return [f(*vals) if ok(*vals) else None for vals in zip(*series)]


def window(s, t, n):
    """The n values ending at t (oldest first), or None if one is missing or before the start."""
    if t - n + 1 < 0:
        return None
    w = s[t - n + 1:t + 1]
    return w if ok(*w) else None


def sma(s, n):
    out = [None] * len(s)
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            out[t] = sum(w) / n
    return out


def rsum(s, n):
    out = [None] * len(s)
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            out[t] = sum(w)
    return out


def stdev(s, n):
    """Population standard deviation (Pine ta.stdev, biased)."""
    out = [None] * len(s)
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            m = sum(w) / n
            out[t] = math.sqrt(sum((v - m) ** 2 for v in w) / n)
    return out


def highest(s, n):
    out = [None] * len(s)
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            out[t] = max(w)
    return out


def lowest(s, n):
    out = [None] * len(s)
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            out[t] = min(w)
    return out


def ema(s, n):
    """alpha = 2/(n+1), seeded with the first available value; a later gap stays a gap and the
    recurrence continues over it."""
    a = 2.0 / (n + 1.0)
    out = [None] * len(s)
    prev = None
    for t, v in enumerate(s):
        if v is None:
            continue
        prev = v if prev is None else a * v + (1.0 - a) * prev
        out[t] = prev
    return out


def rma(s, n):
    """Wilder average (alpha = 1/n) seeded with the SMA of the first n available values; the
    input is assumed gap-free once it has started."""
    out = [None] * len(s)
    prev = None
    seed = []
    for t, v in enumerate(s):
        if v is None:
            continue
        if prev is None:
            seed.append(v)
            if len(seed) == n:
                prev = sum(seed) / n
                out[t] = prev
        else:
            prev = (v + (n - 1) * prev) / n
            out[t] = prev
    return out


def wma(s, n):
    out = [None] * len(s)
    den = n * (n + 1) / 2.0
    for t in range(len(s)):
        w = window(s, t, n)
        if w is not None:
            out[t] = sum(v * (i + 1) for i, v in enumerate(w)) / den
    return out


def jma(s, length, phase=0.0, power=2.0):
    """The open Jurik approximation of the Pine scripts, seeded with the first available value
    (e0 = jma = first value, e1 = e2 = 0) instead of Pine's zero state."""
    pr = phase / 100.0 + 1.5
    lt = 0.45 * (length - 1)
    beta = lt / (lt + 2.0)
    alpha = beta ** power
    out = [None] * len(s)
    e0 = e1 = e2 = j = None
    for t, v in enumerate(s):
        if v is None:
            continue
        if j is None:
            e0, e1, e2, j = v, 0.0, 0.0, v
        else:
            e0 = (1.0 - alpha) * v + alpha * e0
            e1 = (v - e0) * (1.0 - beta) + beta * e1
            e2 = (e0 + pr * e1 - j) * (1.0 - alpha) ** 2 + alpha ** 2 * e2
            j = e2 + j
        out[t] = j
    return out


def change(s):
    return [None] + [s[t] - s[t - 1] if ok(s[t], s[t - 1]) else None for t in range(1, len(s))]


def true_range(h, lo, c):
    """First bar: high - low."""
    return [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                             for t in range(1, len(c))]


def atr(h, lo, c, n):
    return rma(true_range(h, lo, c), n)


def rsi(src, n):
    """Pine ta.rsi: Wilder averages of the gains and losses of `src` (first change on the second
    bar); 100 without losses, 0 without gains."""
    ch = change(src)
    up = rma(smap(lambda d: max(d, 0.0), ch), n)
    dn = rma(smap(lambda d: max(-d, 0.0), ch), n)

    def f(u, d):
        if d == 0.0:
            return 100.0
        if u == 0.0:
            return 0.0
        return 100.0 - 100.0 / (1.0 + u / d)
    return smap(f, up, dn)


def mfi(src, vol, n):
    """Money Flow Index over the last n bars: flow = src * volume, positive when src rose against
    the previous bar, negative when it fell, nothing on the first bar or when unchanged. None
    while the window has no volume; 100 without negative, 0 without positive flow."""
    out = [None] * len(src)
    pos = [0.0] + [src[t] * vol[t] if src[t] > src[t - 1] else 0.0 for t in range(1, len(src))]
    neg = [0.0] + [src[t] * vol[t] if src[t] < src[t - 1] else 0.0 for t in range(1, len(src))]
    for t in range(n - 1, len(src)):
        p = sum(pos[t - n + 1:t + 1])
        q = sum(neg[t - n + 1:t + 1])
        v = sum(vol[t - n + 1:t + 1])
        if v <= 0.0:
            continue
        if q == 0.0:
            out[t] = 100.0
        elif p == 0.0:
            out[t] = 0.0
        else:
            out[t] = 100.0 - 100.0 / (1.0 + p / q)
    return out


def crossover(a, b, t):
    """a crosses above b at t (needs both at t and t-1)."""
    if t < 1 or not ok(a[t], b[t], a[t - 1], b[t - 1]):
        return None
    return a[t] > b[t] and a[t - 1] <= b[t - 1]


def crossunder(a, b, t):
    if t < 1 or not ok(a[t], b[t], a[t - 1], b[t - 1]):
        return None
    return a[t] < b[t] and a[t - 1] >= b[t - 1]


def pivot(src, t, left, right, low_side):
    """Strict pivot at t - right: below (above) every bar within `left` before and `right` after
    it. Returns the pivot value or None."""
    k = t - right
    if k - left < 0:
        return None
    centre = src[k]
    others = src[k - left:k] + src[k + 1:t + 1]
    if low_side:
        return centre if all(centre < v for v in others) else None
    return centre if all(centre > v for v in others) else None


def pine_round(x):
    """Pine math.round: half away from zero."""
    return math.floor(x + 0.5) if x >= 0 else -math.floor(-x + 0.5)


def wave_series(n_bars, with_volume=True):
    """The deterministic OHLCV series shared by the WavesUnchained ports: an uptrend, a drift, a
    crash, a tight range and a breakout, with a fast wiggle; volume oscillates and stays positive.

    prev = previous close (100 before the first bar); drift 0.18 up to bar 219, -0.02 to 379,
    -0.30 to 469, 0.12 after, except 0 with amp 0.08 for bars 560-649 and 0.45 for 650-689 (amp 1
    elsewhere); close = prev + drift + amp (0.9 sin(i/3.1) + 0.6 sin(i/11) + 0.35 cos(i/1.7));
    open = prev + 0.2 sin(i/2.3); high = max(open, close) + 0.4 + 0.3 |sin(i/4.7)|;
    low = min(open, close) - 0.4 - 0.3 |cos(i/5.3)|; volume = 1000 + 400 sin(i/5.3) +
    250 cos(i/2.9) + 300 |sin(i/17)|.
    """
    bars = []
    prev = 100.0
    for i in range(n_bars):
        x = float(i)
        drift = 0.18 if i < 220 else (-0.02 if i < 380 else (-0.30 if i < 470 else 0.12))
        amp = 1.0
        if 560 <= i < 650:
            drift, amp = 0.0, 0.08
        elif 650 <= i < 690:
            drift = 0.45
        base = prev + drift + amp * (0.9 * math.sin(x / 3.1) + 0.6 * math.sin(x / 11.0))
        close = base + amp * 0.35 * math.cos(x / 1.7)
        open_ = prev + 0.2 * math.sin(x / 2.3)
        high = max(open_, close) + 0.4 + 0.3 * abs(math.sin(x / 4.7))
        low = min(open_, close) - 0.4 - 0.3 * abs(math.cos(x / 5.3))
        vol = (1000.0 + 400.0 * math.sin(x / 5.3) + 250.0 * math.cos(x / 2.9)
               + 300.0 * abs(math.sin(x / 17.0))) if with_volume else 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


def events_block(events, names, limit=6):
    """ev_<kind>_n and the bars of the first `limit` events of each kind."""
    ev = {}
    for name in names:
        bars = events[name]
        ev[f"{name}_n"] = float(len(bars))
        for j, b in enumerate(bars[:limit]):
            ev[f"{name}_{j}"] = float(b)
    return ev


def adx(h, lo, c, di_len, adx_len):
    """Pine ta.dmi ADX: +DM/-DM/TR from the second bar (TR without a previous close is na),
    Wilder averages seeded with an SMA, DI = 100 DM/TR (0 for TR = 0), DX = 100 |DI+ - DI-| /
    (DI+ + DI-) (0 for a zero sum), ADX = Wilder average of DX."""
    n = len(c)
    up = [None] + [h[t] - h[t - 1] for t in range(1, n)]
    dn = [None] + [lo[t - 1] - lo[t] for t in range(1, n)]
    pdm = smap(lambda u, d: u if u > d and u > 0.0 else 0.0, up, dn)
    mdm = smap(lambda u, d: d if d > u and d > 0.0 else 0.0, up, dn)
    tr = [None] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                   for t in range(1, n)]
    trr, pr, mr = rma(tr, di_len), rma(pdm, di_len), rma(mdm, di_len)

    def dx(t, p, m):
        dip, dim = (100.0 * p / t, 100.0 * m / t) if t != 0.0 else (0.0, 0.0)
        return 100.0 * abs(dip - dim) / (dip + dim) if dip + dim != 0.0 else 0.0
    return rma(smap(dx, trr, pr, mr), adx_len)


def wavetrend(h, lo, c, n1, n2):
    """WaveTrend of hlc3: esa = EMA(n1), de = EMA(n1) of |ap - esa|, ci = (ap - esa) /
    (0.015 de) (0 for de <= 1e-8), wt1 = EMA(n2) of ci, wt2 = SMA(4) of wt1; first-sample EMAs."""
    ap = [(h[t] + lo[t] + c[t]) / 3.0 for t in range(len(c))]
    esa = ema(ap, n1)
    de = ema([abs(a - e) for a, e in zip(ap, esa)], n1)
    ci = [(a - e) / (0.015 * d) if d > 1e-8 else 0.0 for a, e, d in zip(ap, esa, de)]
    wt1 = ema(ci, n2)
    return wt1, sma(wt1, 4)


def barssince(cond, t):
    """Bars since cond was last True at or before t, None if never."""
    for k in range(t + 1):
        if cond[t - k]:
            return k
    return None
