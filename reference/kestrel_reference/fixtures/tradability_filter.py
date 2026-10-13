"""golden_tradability_filter: Tradability Filter.

Written from the original definition, not from the Rust code.
Preset Balanced, volume confirmation off.
"""

import math

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_tradability_filter"

Q = 20
RANGE = 30
BASE = 34
ATR_LEN = 14
ADX_LEN = 14
RANK = 150
BAL_TH = 58.0
COMP_TH = 62.0
MAX_RANGE = 7.0
READY_DIR = 24.0
BUF_ATR = 0.12
BO_QUAL = 46.0
ACCEPT = 2
CONFIRM = 2
MIN_STATE = 3
HOLD = 3
LOCK = 10
IMP_LB = 10
IMP_DIR = 55.0
IMP_TRAD = 60.0
AM_ADX = 25.0
AM_EFF = 0.12
FAST_EFF = 8
SHORT = max(3, Q // 4)

NO_TRADE, BALANCE, COMPRESSION, READY, BREAKOUT, TREND, EXHAUSTION, AFTERMATH = range(8)

N_BARS = 1500
SEED = 11
CHECKPOINTS = [200, 333, 450, 600, 777, 900, 1200, 1499]
EVENTS = ["no_trade", "compression", "bull_ready", "bear_ready", "bull_breakout",
          "bear_breakout", "bull_trend", "bear_trend", "aftermath", "bull_exhaustion",
          "bear_exhaustion"]


def sdiv(n, d, fb):
    return n / d if abs(d) > 0.0000001 else fb


def scale(v, lo, hi):
    span = hi - lo
    return ps.clamp((v - lo) / span, 0.0, 1.0) if span != 0.0 else 0.0


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    idx = range(n)
    pr = ps.percentrank

    atr = ps.atr(h, lo, c, ATR_LEN)
    atr_ref = ps.ema(atr, Q * 2)
    atr_ratio = ps.smap(lambda a, r: sdiv(a, r, 1.0), atr, atr_ref)
    base = ps.ema(c, BASE)
    slope = [base[t] - base[t - Q] if t >= Q else None for t in idx]
    nslope = ps.smap(lambda s, a: sdiv(s, a * math.sqrt(Q), 0.0), slope, atr)
    dip, dim, adx = ps.dmi(h, lo, c, ADX_LEN, ADX_LEN)
    rng = [max(h[t] - lo[t], 1e-12) for t in idx]
    body = [abs(c[t] - o[t]) / rng[t] if rng[t] > 0.0000001 else 0.0 for t in idx]
    wick = [((h[t] - max(o[t], c[t])) + (min(o[t], c[t]) - lo[t])) / rng[t]
            if rng[t] > 0.0000001 else 0.0 for t in idx]
    avg_body = ps.ema(body, Q)
    avg_wick = ps.ema(wick, Q)
    moves = [None] + [abs(c[t] - c[t - 1]) for t in range(1, n)]
    travel = ps.rsum(moves, Q)
    er = [sdiv(abs(c[t] - c[t - Q]), travel[t], 0.0) if travel[t] is not None else None
          for t in idx]
    ftravel = ps.rsum(moves, FAST_EFF)
    fer = [sdiv(abs(c[t] - c[t - FAST_EFF]), ftravel[t], 0.0) if ftravel[t] is not None
           else None for t in idx]
    step = [0.0] + [1.0 if c[t] > c[t - 1] else (-1.0 if c[t] < c[t - 1] else 0.0)
                    for t in range(1, n)]
    step_sum = ps.rsum(step, Q)
    consistency = ps.smap(lambda s: abs(s) / Q, step_sum)
    signed = ps.smap(lambda s: s / Q, step_sum)
    cross = [0.0] + [1.0 if (c[t] > base[t] and c[t - 1] <= base[t - 1])
                     or (c[t] < base[t] and c[t - 1] >= base[t - 1]) else 0.0
                     for t in range(1, n)]
    cross_rate = ps.smap(lambda s: s / Q, ps.rsum(cross, Q))
    disp = ps.smap(lambda cc, b, a: sdiv(cc - b, a, 0.0), c, base, atr)
    r_hi = [max(h[t - RANGE:t]) if t >= RANGE else None for t in idx]
    r_lo = [min(lo[t - RANGE:t]) if t >= RANGE else None for t in idx]
    p_hi = [max(h[t - 2 * RANGE:t - RANGE]) if t >= 2 * RANGE else None for t in idx]
    p_lo = [min(lo[t - 2 * RANGE:t - RANGE]) if t >= 2 * RANGE else None for t in idx]
    width_atr = [sdiv(r_hi[t] - r_lo[t], atr[t], MAX_RANGE * 2.0)
                 if ps.ok(r_hi[t], atr[t]) else None for t in idx]
    pos = [ps.clamp(sdiv(c[t] - r_lo[t], r_hi[t] - r_lo[t], 0.5), 0.0, 1.0)
           if r_hi[t] is not None else None for t in idx]
    stability = [1.0 - ps.clamp((sdiv(abs(r_hi[t] - p_hi[t]), atr[t], 0.0)
                                 + sdiv(abs(r_lo[t] - p_lo[t]), atr[t], 0.0)) * 0.5, 0.0, 1.0)
                 if ps.ok(p_hi[t], atr[t]) else None for t in idx]
    basis = ps.sma(c, Q)
    dev = ps.stdev(c, Q)
    bw = ps.smap(lambda d, b: sdiv(d * 4.0, b, 0.0), dev, basis)
    bw_ref = ps.ema(bw, Q * 2)
    bw_ratio = ps.smap(lambda x, r: sdiv(x, r, 1.0), bw, bw_ref)
    cum = 0.0
    vol_real = []
    for t in idx:
        cum += v[t]
        vol_real.append(cum > 0)
    vol_ref = ps.ema(v, Q)
    vol_ratio = [sdiv(v[t], vol_ref[t], 1.0) if vol_real[t] else 1.0 for t in idx]

    r_atr = pr(atr_ratio, RANK)
    r_bw = pr(bw_ratio, RANK)
    r_width = pr(width_atr, RANK)
    r_vol = pr(vol_ratio, RANK)
    r_er = pr(er, RANK)
    r_cons = pr(consistency, RANK)
    r_adx = pr(adx, RANK)
    r_slope = pr(ps.smap(abs, nslope), RANK)
    r_cross = pr(cross_rate, RANK)
    r_body = pr(avg_body, RANK)
    r_wick = pr(avg_wick, RANK)
    r_bar_body = pr(body, RANK)

    trad = [None] * n
    bal = [None] * n
    comp = [None] * n
    dirs = [None] * n
    for t in idx:
        need = (r_atr[t], r_bw[t], r_width[t], r_er[t], r_cons[t], r_adx[t], r_slope[t],
                r_cross[t], r_body[t], r_wick[t], stability[t], pos[t], disp[t], dip[t])
        if not ps.ok(*need) or (vol_real[t] and r_vol[t] is None):
            continue
        bal_v = ((100.0 - r_er[t]) * 0.26 + (100.0 - r_adx[t]) * 0.18 + r_cross[t] * 0.18
                 + stability[t] * 100.0 * 0.24
                 + (1.0 - scale(width_atr[t], MAX_RANGE * 0.65, MAX_RANGE * 1.25)) * 100.0
                 * 0.14)
        bal[t] = ps.clamp(bal_v, 0.0, 100.0)
        energy = (100.0 - r_body[t]) * 0.5 + r_wick[t] * 0.5
        comp_v = ((100.0 - r_atr[t]) * 0.27 + (100.0 - r_bw[t]) * 0.27
                  + (100.0 - r_width[t]) * 0.16 + stability[t] * 100.0 * 0.12
                  + (100.0 - r_er[t]) * 0.06 + energy * 0.12)
        comp[t] = ps.clamp(comp_v, 0.0, 100.0)
        di_dir = sdiv(dip[t] - dim[t], dip[t] + dim[t], 0.0)
        d = (ps.clamp(nslope[t] / 0.75, -1.0, 1.0) * 0.28
             + ps.clamp(disp[t] / 2.0, -1.0, 1.0) * 0.20 + di_dir * 0.20
             + signed[t] * 0.18 + ps.clamp((pos[t] - 0.5) * 2.0, -1.0, 1.0) * 0.14) * 100.0
        dirs[t] = ps.clamp(d, -100.0, 100.0)
        cq = r_body[t] * 0.65 + (100.0 - r_wick[t]) * 0.35
        wsum = 1.00 if vol_real[t] else 0.96
        tr_v = (r_er[t] * 0.24 + r_adx[t] * (r_er[t] / 100.0) * 0.16 + r_slope[t] * 0.15
                + r_cons[t] * 0.13 + (100.0 - r_cross[t]) * 0.12 + cq * 0.10
                + r_atr[t] * 0.06 + (r_vol[t] * 0.04 if vol_real[t] else 0.0)) / wsum
        trad[t] = ps.clamp(tr_v, 0.0, 100.0)

    state = NO_TRADE
    streak = 0
    state_bars = 0
    hold_left = 0
    active_dir = 0
    since_bo = 100000
    bull_streak = bear_streak = 0
    bull_lvl = bear_lvl = None
    out = {k: [None] * n for k in ("tradability", "balance", "compression", "direction",
                                   "state", "candidate")}
    events = {e: [] for e in EVENTS}
    reported = [False] * n
    for t in idx:
        data_ready = t > max(max(RANGE * 2 + 5, Q * 3 + 5), RANK)
        ready = data_ready and trad[t] is not None and t >= IMP_LB - 1 \
            and all(trad[t - k] is not None for k in range(IMP_LB)) \
            and ps.ok(ps.at(atr_ratio, t, 3), ps.at(er, t, SHORT), ps.at(avg_body, t, SHORT),
                      r_bar_body[t]) \
            and all(ps.ok(atr_ratio[t - k], bw_ratio[t - k]) for k in range(SHORT))
        reported[t] = ready
        cand = NO_TRADE
        bull_bo = bear_bo = False
        trend_c = exh_c = False
        if ready:
            allowed = since_bo > LOCK
            est_bal = allowed and bal[t] >= BAL_TH and width_atr[t] <= MAX_RANGE
            est_comp = est_bal and comp[t] >= COMP_TH
            bull_p = dirs[t] >= READY_DIR and pos[t] >= 0.68
            bear_p = dirs[t] <= -READY_DIR and pos[t] <= 0.32
            atr_tr = min(atr_ratio[t - SHORT + 1:t + 1])
            bw_tr = min(bw_ratio[t - SHORT + 1:t + 1])
            energy = atr_ratio[t] > atr_tr * 1.03 or bw_ratio[t] > bw_tr * 1.03
            ready_c = est_comp and energy and (bull_p or bear_p)
            imp = max(abs(dirs[t - k]) for k in range(IMP_LB)) >= IMP_DIR and \
                max(trad[t - k] for k in range(IMP_LB)) >= IMP_TRAD
            aftermath_c = imp and adx[t] >= AM_ADX and fer[t] <= AM_EFF and \
                atr_ratio[t] <= atr_ratio[t - 3]
        buf = atr[t] * BUF_ATR if atr[t] is not None else 0.0
        bull_ref = bull_lvl if bull_streak > 0 else r_hi[t]
        bear_ref = bear_lvl if bear_streak > 0 else r_lo[t]
        raw_bull = ready and c[t] > bull_ref + buf
        raw_bear = ready and c[t] < bear_ref - buf
        if raw_bull and bull_streak == 0:
            bull_lvl = r_hi[t]
        if raw_bear and bear_streak == 0:
            bear_lvl = r_lo[t]
        bull_streak = bull_streak + 1 if raw_bull else 0
        bear_streak = bear_streak + 1 if raw_bear else 0
        if ready:
            common = trad[t] >= BO_QUAL and r_atr[t] >= 35.0 and r_bar_body[t] >= 60.0
            bull_bo = raw_bull and bull_streak >= ACCEPT and dirs[t] > 0.0 and common
            bear_bo = raw_bear and bear_streak >= ACCEPT and dirs[t] < 0.0 and common
        bo = bull_bo or bear_bo
        if bull_bo:
            bull_streak = 0
        if bear_bo:
            bear_streak = 0
        since_bo = 0 if bo else since_bo + 1
        if ready:
            bull_t = trad[t] >= 55.0 and dirs[t] >= 28.0 and er[t] >= 0.20 and c[t] > base[t]
            bear_t = trad[t] >= 55.0 and dirs[t] <= -28.0 and er[t] >= 0.20 and c[t] < base[t]
            trend_c = bull_t or bear_t
            ext = abs(sdiv(c[t] - base[t], atr[t], 0.0))
            decay = er[t] < er[t - SHORT] and avg_body[t] < avg_body[t - SHORT]
            exh_c = trend_c and ext >= 2.6 and (decay or r_wick[t] >= 80.0 or r_atr[t] >= 90.0)
            if bo:
                cand = BREAKOUT
            elif exh_c:
                cand = EXHAUSTION
            elif trend_c:
                cand = TREND
            elif ready_c:
                cand = READY
            elif aftermath_c:
                cand = AFTERMATH
            elif est_comp:
                cand = COMPRESSION
            elif est_bal:
                cand = BALANCE
        streak = streak + 1 if cand != state else 0
        if bull_bo:
            active_dir = 1
        elif bear_bo:
            active_dir = -1
        elif state == TREND:
            if dirs[t] >= 20.0:
                active_dir = 1
            elif dirs[t] <= -20.0:
                active_dir = -1
        if bo:
            hold_left = HOLD
        if hold_left > 0:
            hold_left -= 1
        hold = state == BREAKOUT and hold_left > 0 and not exh_c
        may_leave = state_bars >= MIN_STATE or cand == BREAKOUT or state == BREAKOUT
        req = 1 if cand == BREAKOUT else (CONFIRM + 1 if cand == NO_TRADE else CONFIRM)
        prev_state = state
        if hold:
            state = BREAKOUT
            state_bars += 1
        elif streak >= req and may_leave and cand != state:
            state = cand
            state_bars = 0
        else:
            state_bars += 1
        if state != prev_state:
            if state == NO_TRADE:
                events["no_trade"].append(t)
            elif state == COMPRESSION:
                events["compression"].append(t)
            elif state == READY:
                if dirs[t] is not None and dirs[t] > 0.0:
                    events["bull_ready"].append(t)
                elif dirs[t] is not None and dirs[t] < 0.0:
                    events["bear_ready"].append(t)
            elif state == BREAKOUT:
                if active_dir == 1:
                    events["bull_breakout"].append(t)
                elif active_dir == -1:
                    events["bear_breakout"].append(t)
            elif state == TREND:
                events["bull_trend" if dirs[t] > 0.0 else "bear_trend"].append(t)
            elif state == AFTERMATH:
                events["aftermath"].append(t)
            elif state == EXHAUSTION:
                events["bear_exhaustion" if active_dir >= 0 else "bull_exhaustion"].append(t)
        if ready:
            for k, val in (("tradability", trad[t]), ("balance", bal[t]),
                           ("compression", comp[t]), ("direction", dirs[t]),
                           ("state", float(state)), ("candidate", float(cand))):
                out[k][t] = val
    out["events"] = events
    out["reported"] = reported
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Tradability Filter",
    "",
    *provenance("tradability_filter"),
    "",
    "Formula: the original definition,",
    "preset Balanced (quality 20, range 30, baseline 34, ATR 14, ADX 14, rank 150), volume",
    "confirmation off. Conventions the original leaves open: EMAs seeded with their first",
    "value; ATR and DMI Wilder averages seeded with an SMA (DMI from the second bar);",
    "population standard deviation; percentrank = share of the previous 150 values <= the",
    "current one, needing all of them; the bar range floor (mintick) is 1e-12; the direction",
    "step and baseline cross of the first bar are 0. A bar is ready once the original's data-ready",
    "bar has passed and every score, the last 10 scores and every lookback the states read",
    "exist; before that the candidate is NO TRADE and no breakout is raw, while the state",
    "machine counters run on every bar. Events fire on entering a state: no_trade, compression,",
    "bull/bear_ready (direction sign), bull/bear_breakout (active direction), bull/bear_trend",
    "(direction sign), aftermath, bear/bull_exhaustion (active direction >= 0 / < 0). State",
    "codes: 0 no trade, 1 balance, 2 compression, 3 ready, 4 breakout, 5 trend, 6 exhaustion,",
    "7 aftermath.",
    "",
    "Series: pine_series.lcg_series(1500, seed 11) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first ready bar; cp<bar>_* the outputs at that bar; ev_<kind>_n the",
    "number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(ps.lcg_series(N_BARS, SEED))
    first = next(t for t, r in enumerate(out["reported"]) if r)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        blocks.append((f"cp{cp}", f"bar {cp}", {k: out[k][cp] for k in (
            "tradability", "balance", "compression", "direction", "state", "candidate")}))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
