"""golden_reversal_types: Reversal Type Classifier v1 (WavesUnchained) as research labels.

Written from the published Pine Script, not from the Rust code. The labels use the bars after the
pivot (the evaluation window) and are therefore research labels, not signals.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_reversal_types"

PIV = 5
EVAL = 24
MIN_MOVE = 0.30
WT_CH, WT_AVG = 10, 21
OS_LEVEL, OB_LEVEL = -45.0, 45.0
ATR_LEN = 14
RISK_ATR = 1.5
SNAP_R = 1.2
GRIND_R = 2.0
FAIL_R = 1.0
MIN_FOLLOW = 0.45
SNAP_BARS = 6
EMA_LEN = 100
ADX_LEN = 14
ADX_TREND = 22.0
RANK_LEN = 100
CHOP_ADX = 16.0

N_BARS = 1500
SEED = 6
KIND = {1: "snapback", 2: "grind", 3: "fake", 4: "chop"}


def quality(div, ext, ta, chop, mfe, mae, rank):
    s = 50.0
    s += 18.0 if div else 0.0
    s += 12.0 if ext else -8.0
    s -= 14.0 if ta else 0.0
    s -= 8.0 if chop else 0.0
    s += 12.0 if mfe >= GRIND_R else (5.0 if mfe >= MIN_FOLLOW else -12.0)
    s -= 10.0 if mae >= FAIL_R else 0.0
    s += 4.0 if (rank is not None and 20 <= rank <= 80) else -3.0
    return max(0.0, min(100.0, s))


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    wt1, _ = ps.wavetrend(h, lo, c, WT_CH, WT_AVG)
    atr = ps.atr(h, lo, c, ATR_LEN)
    ema = ps.ema(c, EMA_LEN)
    _, _, adx = ps.dmi(h, lo, c, ADX_LEN, ADX_LEN)
    a_lo = ps.lowest(atr, RANK_LEN)
    a_hi = ps.highest(atr, RANK_LEN)
    rank = ps.smap(lambda a, l, u: 100.0 * (a - l) / (u - l) if u != l else 50.0, atr, a_lo, a_hi)

    # pivots in confirmation order with the previous pivot of the same side
    pivots = []
    last = {True: None, False: None}
    for t in range(n):
        for low_side in (True, False):
            v = ps.pivot(lo if low_side else h, t, PIV, PIV, low_side)
            if v is None:
                continue
            p = t - PIV
            w = wt1[p]
            pivots.append((t, low_side, p, v, last[low_side]))
            last[low_side] = (v, w)

    labels = []
    span = EVAL + PIV
    for conf, low_side, p, price, prev in pivots:
        t = conf + EVAL
        if t >= n or atr[p] is None:
            continue
        risk = atr[p] * RISK_ATR
        win_h = max(h[p + 1:t + 1])
        win_l = min(lo[p + 1:t + 1])
        sgn = 1.0 if low_side else -1.0
        if low_side:
            impulse = win_h - price
            mfe = (win_h - price) / risk if risk > 0 else None
            mae = max(0.0, price - win_l) / risk if risk > 0 else None
        else:
            impulse = price - win_l
            mfe = (price - win_l) / risk if risk > 0 else None
            mae = max(0.0, win_h - price) / risk if risk > 0 else None
        if impulse < MIN_MOVE * atr[p]:
            continue

        def first_hits(target_r):
            ft = ff = None
            for j in range(1, span + 1):
                b = p + j
                if low_side:
                    if ft is None and h[b] >= price + target_r * risk:
                        ft = j
                    if ff is None and lo[b] <= price - FAIL_R * risk:
                        ff = j
                else:
                    if ft is None and lo[b] <= price - target_r * risk:
                        ft = j
                    if ff is None and h[b] >= price + FAIL_R * risk:
                        ff = j
            return ft, ff
        snap, fail = first_hits(SNAP_R)
        grind, _ = first_hits(GRIND_R)
        good_snap = snap is not None and (fail is None or snap < fail)
        good_grind = grind is not None and (fail is None or grind < fail)
        beyond = c[t] > price if low_side else c[t] < price
        if good_snap and snap <= SNAP_BARS:
            kind = 1
        elif good_grind or (mfe >= MIN_FOLLOW and mae < FAIL_R and beyond):
            kind = 2
        elif (fail is not None and (snap is None or fail < snap)) or mfe < MIN_FOLLOW:
            kind = 3
        else:
            kind = 4
        w = wt1[p]
        div = prev is not None and prev[1] is not None and w is not None and (
            (low_side and price < prev[0] and w > prev[1])
            or (not low_side and price > prev[0] and w < prev[1]))
        ext = w is not None and ((low_side and w <= OS_LEVEL) or (not low_side and w >= OB_LEVEL))
        ta = ema[p] is not None and adx[p] is not None and adx[p] >= ADX_TREND and (
            (low_side and c[p] < ema[p]) or (not low_side and c[p] > ema[p]))
        chop = adx[p] is not None and adx[p] <= CHOP_ADX
        labels.append({"pivot": p, "known": t, "side": sgn, "price": price, "kind": kind,
                       "mfe": mfe, "mae": mae, "div": div, "ext": ext, "ta": ta, "chop": chop,
                       "quality": quality(div, ext, ta, chop, mfe, mae, rank[p])})
    labels.sort(key=lambda r: (r["known"], -r["side"]))
    return labels


HEADER = [
    "kestrel-chartkit golden reference fixture: Reversal Type Classifier (research labels)",
    "",
    *provenance("reversal_types"),
    "",
    "Formula: the published Pine Script 'Reversal Type Classifier v1' (WavesUnchained) with its",
    "defaults (pivots 5/5, evaluation 24 bars, impulse 0.30 ATR, risk 1.5 ATR, snapback 1.2 R",
    "within 6 bars, grind 2.0 R, fail 1.0 R, follow-through 0.45 R, WT 10/21 with -45/45, EMA 100,",
    "ADX 14 with 22 / 16, ATR rank 100). Uses the 29 bars after each pivot: research labels, not",
    "signals. Conventions where Pine leaves them open: EMAs seeded with their first value; ATR and",
    "DMI Wilder averages seeded with an SMA (DMI from the second bar); pivots strict on both",
    "sides; a pivot without an ATR at the pivot bar is not labelled; missing WT/EMA/ADX values",
    "make the corresponding flag false and a missing ATR rank scores -3. Labels are ordered by",
    "the bar at which they become known, long before short on the same bar.",
    "",
    "Series: pine_series.lcg_series(1500, seed 6) (see reference/kestrel_reference/pine_series.py).",
    "",
    "label_n the number of labels; lab_<j>_* the j-th label (up to 25): pivot bar, known bar,",
    "side (+1 long from a pivot low, -1 short), price, kind (1 snapback, 2 grind, 3 fake,",
    "4 chop), mfe and mae in R, div/ext/ta/chop flags, quality; kind_<k>_n the count per kind.",
]


def build():
    labels = compute(ps.lcg_series(N_BARS, SEED))
    blocks = [(None, "labels", {"label_n": float(len(labels))})]
    counts = {f"kind_{k}_n": float(sum(1 for r in labels if r["kind"] == k)) for k in KIND}
    blocks.append((None, "per kind", counts))
    for j, r in enumerate(labels[:25]):
        blocks.append((f"lab_{j}", None, {
            "pivot": float(r["pivot"]), "known": float(r["known"]), "side": r["side"],
            "price": r["price"], "kind": float(r["kind"]), "mfe": r["mfe"], "mae": r["mae"],
            "div": float(r["div"]), "ext": float(r["ext"]), "ta": float(r["ta"]),
            "chop": float(r["chop"]), "quality": r["quality"]}))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
