"""golden_smoothed_supertrend: Smoothed Supertrend.

Written from the original definition, not from the Rust code. Manual parameters (auto timeframe
scaling needs the chart timeframe), volatility-adaptive band width, polarity and volume filters
off; the SL/TP trade overlay is not part of the indicator.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_smoothed_supertrend"

FACTOR = 12.0
ATR_LEN = 90
WMA_LEN = 40
EMA_LEN = 14
ADX_THRESHOLD = 25.0
SLOPE_LB = 3
SLOPE_TH = 0.05
STRENGTH_LB = 100
STRENGTH_TH = 70.0
TOUCHES = 2
REJ_SLOPE_LB = 5
OVEREXT_LB = 100
OVEREXT_TH = 85.0

N_BARS = 900
CHECKPOINTS = [200, 333, 450, 617, 700, 780, 899]
EVENTS = ["bull_flip", "bear_flip", "bull_rejection", "bear_rejection",
          "bull_rejection_strong", "bear_rejection_strong"]


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    atr = ps.atr(h, lo, c, ATR_LEN)

    lower = [None] * n
    upper = [None] * n
    for t in range(n):
        if atr[t] is None:
            continue
        hl2 = (h[t] + lo[t]) / 2.0
        lw = hl2 - FACTOR * atr[t]
        up = hl2 + FACTOR * atr[t]
        if t > 0 and lower[t - 1] is not None:
            pl, pu = lower[t - 1], upper[t - 1]
            lw = lw if (lw > pl or c[t - 1] < pl) else pl
            up = up if (up < pu or c[t - 1] > pu) else pu
        lower[t], upper[t] = lw, up
    mid = ps.smap(lambda a, b: (a + b) / 2.0, lower, upper)
    base = ps.ema(ps.wma(mid, WMA_LEN), EMA_LEN)
    slope = [base[t] - base[t - SLOPE_LB] if t >= SLOPE_LB and ps.ok(base[t], base[t - SLOPE_LB])
             else None for t in range(n)]
    trend = [0] * n
    cur = 0
    for t in range(n):
        if ps.ok(slope[t], atr[t]):
            if slope[t] > atr[t] * SLOPE_TH:
                cur = 1
            elif slope[t] < -atr[t] * SLOPE_TH:
                cur = -1
        trend[t] = cur
    adx = ps.adx(h, lo, c, 14, 14)
    mag = ps.smap(lambda s, a: abs(s) / a, slope, atr)
    strength = ps.percentrank(mag, STRENGTH_LB)
    dist = ps.smap(lambda cc, b: cc - b, c, base)
    dist_rank = ps.percentrank(ps.smap(abs, dist), OVEREXT_LB)

    events = {e: [] for e in EVENTS}
    reported = [ps.ok(base[t], atr[t]) for t in range(n)]
    for t in range(n):
        if not reported[t]:
            continue
        trending = adx[t] is not None and adx[t] > ADX_THRESHOLD
        prev = trend[t - 1] if t > 0 else 0
        if trend[t] == 1 and prev != 1 and trending:
            events["bull_flip"].append(t)
        if trend[t] == -1 and prev != -1 and trending:
            events["bear_flip"].append(t)
        bull_t = bear_t = 0
        for i in range(TOUCHES):
            k = t - i
            if k < 0 or base[k] is None:
                continue
            if trend[k] == 1 and lo[k] <= base[k]:
                bull_t += 1
            if trend[k] == -1 and h[k] >= base[k]:
                bear_t += 1
        past = ps.at(base, t, REJ_SLOPE_LB)
        strong = strength[t] is not None and strength[t] > STRENGTH_TH
        if past is not None:
            if lo[t] <= base[t] and c[t] > base[t] and trend[t] == 1 and base[t] > past \
                    and bull_t >= TOUCHES:
                events["bull_rejection_strong" if strong else "bull_rejection"].append(t)
            if h[t] >= base[t] and c[t] < base[t] and trend[t] == -1 and base[t] < past \
                    and bear_t >= TOUCHES:
                events["bear_rejection_strong" if strong else "bear_rejection"].append(t)
    return {"reported": reported, "baseline": base, "trend": trend, "lower": lower,
            "upper": upper, "strength": strength, "dist_rank": dist_rank, "events": events}


HEADER = [
    "kestrel-chartkit golden reference fixture: Smoothed Supertrend",
    "",
    *provenance("smoothed_supertrend"),
    "",
    "Formula: the original definition with its manual",
    "parameters (factor 12, ATR 90, WMA 40, EMA 14, slope 3 bars / 0.05 ATR, ADX 25, strength",
    "100/70, touches 2, rejection slope 5, overextension 100/85), filters off. Conventions the",
    "original leaves open: ATR and ADX Wilder averages seeded with an SMA; the supertrend bands",
    "start raw on the first bar with an ATR; EMA seeded with its first value; percentrank = share",
    "of the previous n values <= the current one, needing all of them. A bar reports when the",
    "baseline and ATR exist; a strong rejection (strength rank above 70) is reported as",
    "*_rejection_strong instead of *_rejection.",
    "",
    "Series: pine_series.wave_series(900) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(ps.wave_series(N_BARS))
    first = next(t for t, r in enumerate(out["reported"]) if r)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {"baseline": out["baseline"][cp], "trend": float(out["trend"][cp]),
                "st_lower": out["lower"][cp], "st_upper": out["upper"][cp]}
        if out["strength"][cp] is not None:
            case["strength_rank"] = out["strength"][cp]
        if out["dist_rank"][cp] is not None:
            case["distance_rank"] = out["dist_rank"][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
