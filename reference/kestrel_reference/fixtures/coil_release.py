"""golden_coil_release: Coil Release.

Written from the original definition, not from the Rust code. The higher-timeframe trend
context (request.security) is replaced by the same EMA slope on the chart timeframe.
"""

import math

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_coil_release"

LEN = 34
BREAK_LEN = 12
SYMBOL_ATR = 0.15
EMA_LEN = 55
SLOPE_LEN = 5
SLOPE_MIN = 0.10
COMPLEX_HIGH = 62.0
COMPLEX_LOW = 48.0
EFF_HIGH = 42.0
EFF_LOW = 25.0
REL_FLOOR = 0.35
COIL_NOW_MIN = 0.45
THR_CONT = 0.45
THR_BASE = 0.55
BASE_SHARE = 0.35
BREAK_ATR_MIN = 0.10
COOLDOWN = 8
CHOP_LEN = 14
CHOP_VETO = 61.8

N_BARS = 1500
SEED = 3
CHECKPOINTS = [120, 300, 450, 600, 777, 900, 1200, 1499]
EVENTS = ["bull_release", "bear_release", "bull_base_break", "bear_base_break"]


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    atr = ps.atr(h, lo, c, 14)
    tr = ps.true_range(h, lo, c)

    complexity = [None] * n
    for t in range(n):
        if t - (LEN - 1) < 0 or any(atr[t - i] is None for i in range(LEN - 1)):
            continue
        counts = {-2: 0, -1: 0, 0: 0, 1: 0, 2: 0}
        trans = 0
        prev = None
        for i in range(LEN - 1):
            r = c[t - i] - c[t - i - 1]
            unit = atr[t - i] * SYMBOL_ATR
            s = 2 if r > unit else (1 if r > 0 else (-2 if r < -unit else (-1 if r < 0 else 0)))
            counts[s] += 1
            if prev is not None and s != prev:
                trans += 1
            prev = s
        total = max(LEN - 1, 1)
        ent = 0.0
        for k in (-2, -1, 0, 1, 2):
            p = counts[k] / total
            if p > 0:
                ent += -p * math.log(p)
        ent /= math.log(5)
        rate = trans / max(total - 1, 1)
        complexity[t] = ps.clamp((ent * 0.70 + rate * 0.30) * 100, 0.0, 100.0)

    half = int(max(2, LEN / 2))
    hh_half = ps.highest(h, half)
    ll_half = ps.lowest(lo, half)
    hh = ps.highest(h, LEN)
    ll = ps.lowest(lo, LEN)
    fd = [None] * n
    for t in range(n):
        if not ps.ok(hh_half[t], ll_half[t], ps.at(hh_half, t, half), hh[t]):
            continue
        r1 = (hh_half[t] - ll_half[t]) / half
        r2 = (hh_half[t - half] - ll_half[t - half]) / half
        r3 = (hh[t] - ll[t]) / LEN
        d = 1.5
        if r1 + r2 > 0.0 and r3 > 0.0:
            d = (math.log(r1 + r2) - math.log(r3)) / math.log(2.0)
        fd[t] = ps.clamp(d, 1.0, 2.0)

    eff = [None] * n
    for t in range(LEN - 1, n):
        path = sum(abs(c[t - i] - c[t - i - 1]) for i in range(LEN - 1))
        direct = abs(c[t] - c[t - LEN + 1])
        eff[t] = ps.clamp(direct / path * 100, 0.0, 100.0) if path > 0 else 0.0

    tr_sum = ps.rsum(tr, CHOP_LEN)
    ch_hh = ps.highest(h, CHOP_LEN)
    ch_ll = ps.lowest(lo, CHOP_LEN)
    chop = ps.smap(lambda s, a, b: ps.clamp(100 * math.log10(s / (a - b)) / math.log10(CHOP_LEN),
                                            0.0, 100.0) if a - b > 0 else 0.0,
                   tr_sum, ch_hh, ch_ll)

    ema = ps.ema(c, EMA_LEN)
    slope_atr = [((ema[t] - ema[t - SLOPE_LEN]) / atr[t] if atr[t] > 0 else 0.0)
                 if t >= SLOPE_LEN and atr[t] is not None else None for t in range(n)]
    trend = ps.smap(lambda s: 1 if s > SLOPE_MIN else (-1 if s < -SLOPE_MIN else 0), slope_atr)

    coil = ps.smap(lambda cm, f, e: ps.clamp(cm / 100 * 0.5 + (f - 1.0) * 0.3
                                             + (1.0 - e / 100) * 0.2, 0.0, 1.0),
                   complexity, fd, eff)
    coil_hi = ps.highest(coil, BREAK_LEN)
    cmp_sma = ps.sma(complexity, 3)
    eff_sma = ps.sma(eff, 3)
    base = [ps.ok(complexity[t], eff[t]) and complexity[t] > COMPLEX_HIGH and eff[t] < EFF_LOW
            for t in range(n)]
    base_ok = [ps.ok(complexity[t], eff[t]) for t in range(n)]
    hi12 = ps.highest(h, BREAK_LEN)
    lo12 = ps.lowest(lo, BREAK_LEN)

    out = {k: [None] * n for k in ("setup", "complexity", "fd", "eff", "chop", "coil",
                                   "coil_score", "release_dyn", "trend")}
    events = {e: [] for e in EVENTS}
    last_up = last_dn = None
    prev_break_up = prev_break_dn = None
    for t in range(1, n):
        corr_h, corr_l = hi12[t - 1], lo12[t - 1]
        bu = bd = None
        if corr_h is not None:
            bu = c[t] > corr_h
            bd = c[t] < corr_l
        need = (complexity[t], fd[t], eff[t], chop[t], trend[t], coil[t], coil_hi[t - 1],
                cmp_sma[t - 1], eff_sma[t - 1], corr_h, atr[t], prev_break_up)
        base_window = t - BREAK_LEN >= 0 and all(base_ok[t - k] for k in range(1, BREAK_LEN + 1))
        ready = ps.ok(*need) and base_window
        pbu, pbd = prev_break_up, prev_break_dn
        prev_break_up, prev_break_dn = bu, bd
        if not ready:
            continue
        coil_recent = coil_hi[t - 1]
        blend = ps.clamp(coil_recent * 0.70 + coil[t] * 0.30, 0.0, 1.0)
        coil_score = blend if coil[t] >= COIL_NOW_MIN else \
            blend * ps.clamp(coil[t] / max(COIL_NOW_MIN, 0.01), 0.0, 1.0)
        drop = ps.clamp((cmp_sma[t - 1] - complexity[t]) / 8.0, 0.0, 1.0)
        gain = ps.clamp((eff[t] - eff_sma[t - 1]) / 8.0, 0.0, 1.0)
        dyn = (drop + gain) / 2.0
        setup = ps.clamp(coil_score * (REL_FLOOR + (1.0 - REL_FLOOR) * dyn), 0.0, 1.0)
        base_bars = sum(1.0 for k in range(1, BREAK_LEN + 1) if base[t - k])
        was_base = base_bars >= ps.pine_round(BREAK_LEN * BASE_SHARE)
        first_up = bu and not pbu
        first_dn = bd and not pbd
        a = atr[t]
        if first_up:
            impulse = abs(c[t] - corr_h) / a if a > 0 else 0.0
        elif first_dn:
            impulse = abs(c[t] - corr_l) / a if a > 0 else 0.0
        else:
            impulse = 0.0
        quality = chop[t] < CHOP_VETO and impulse >= BREAK_ATR_MIN
        up_ok = last_up is None or t - last_up >= COOLDOWN
        dn_ok = last_dn is None or t - last_dn >= COOLDOWN
        rel_l = first_up and trend[t] == 1 and setup >= THR_CONT and quality and up_ok
        rel_s = first_dn and trend[t] == -1 and setup >= THR_CONT and quality and dn_ok
        bb_l = first_up and trend[t] == -1 and was_base and setup >= THR_BASE and quality and up_ok
        bb_s = first_dn and trend[t] == 1 and was_base and setup >= THR_BASE and quality and dn_ok
        if rel_l:
            events["bull_release"].append(t)
        if bb_l:
            events["bull_base_break"].append(t)
        if rel_s:
            events["bear_release"].append(t)
        if bb_s:
            events["bear_base_break"].append(t)
        if rel_l or bb_l:
            last_up = t
        if rel_s or bb_s:
            last_dn = t
        for k, val in (("setup", setup), ("complexity", complexity[t]), ("fd", fd[t]),
                       ("eff", eff[t]), ("chop", chop[t]), ("coil", coil[t]),
                       ("coil_score", coil_score), ("release_dyn", dyn),
                       ("trend", float(trend[t]))):
            out[k][t] = val
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Coil Release",
    "",
    *provenance("coil_release"),
    "",
    "Formula: the original definition with its",
    "defaults (length 34, break length 12, symbol step 0.15 ATR, EMA 55 / slope 5 / 0.10 ATR,",
    "thresholds 0.45 / 0.55, cooldown 8, chop 14 / 61.8). The higher-timeframe EMA slope is taken",
    "on the chart timeframe. Conventions the original leaves open: ATR Wilder average seeded",
    "with an SMA, first true range = high - low; EMA seeded with its first value; every quantity",
    "needs all of its inputs (the complexity every ATR it reads); base bars need all 12 earlier",
    "classifications; a first break needs the previous bar's break state. A bar reports once all",
    "of that exists.",
    "",
    "Series: pine_series.lcg_series(1500, seed 3) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(ps.lcg_series(N_BARS, SEED))
    first = next(t for t, r in enumerate(out["setup"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    keys = ("setup", "complexity", "fd", "eff", "chop", "coil", "coil_score", "release_dyn",
            "trend")
    for cp in CHECKPOINTS:
        blocks.append((f"cp{cp}", f"bar {cp}", {k: out[k][cp] for k in keys}))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
