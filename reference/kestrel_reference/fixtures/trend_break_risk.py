"""golden_trend_break_risk: Trend Break Risk.

Written from the original definition, not from the Rust code. The
script is followed bar by bar with its `var` state; `x[k]` reads the stored history.
"""

import math

from ..fixture import provenance

NAME = "golden_trend_break_risk"

BIAS_LEN = 50
PIV_LEN = 5
NEAR_ATR = 1.5
RSI_LEN = 14
ATR_LEN = 14
HOLD_BARS = 10
SMOOTH_LEN = 3
W_NEAR, W_SFP, W_STRUCT, W_DIV = 25.0, 20.0, 15.0, 10.0
LVL_ELEV, LVL_HIGH, LVL_CRIT = 35.0, 60.0, 80.0
REQUIRE_CONTEXT_FLIP = True

N_BARS = 900
CHECKPOINTS = [150, 260, 400, 470, 600, 700, 899]
EVENTS = [
    "bear_break_pressure", "bull_break_pressure", "bear_break_critical", "bull_break_critical",
    "bear_structure_break", "bull_structure_break", "bear_divergence", "bull_divergence",
]


def _series():
    """Two trends, a range, a crash and a recovery with a fast wiggle (as the Price-Average Relation fixture)."""
    bars = []
    prev_close = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.18 if i < 220 else (-0.02 if i < 380 else (-0.30 if i < 470 else 0.12))
        amp = 1.0
        if 560 <= i < 650:
            drift, amp = 0.0, 0.08
        elif 650 <= i < 690:
            drift = 0.45
        base = prev_close + drift + amp * (0.9 * math.sin(x / 3.1) + 0.6 * math.sin(x / 11.0))
        close = base + amp * 0.35 * math.cos(x / 1.7)
        open_ = prev_close + 0.2 * math.sin(x / 2.3)
        high = max(open_, close) + 0.4 + 0.3 * abs(math.sin(x / 4.7))
        low = min(open_, close) - 0.4 - 0.3 * abs(math.cos(x / 5.3))
        bars.append((open_, high, low, close))
        prev_close = close
    return bars


def _clamp(x, lo, hi):
    return max(lo, min(hi, x))


def _norm(x, lo, hi):
    return 0.0 if hi == lo else _clamp((x - lo) / (hi - lo), 0.0, 1.0)


def _ema(src, n):
    a = 2.0 / (n + 1.0)
    out, prev = [], None
    for v in src:
        prev = v if prev is None else a * v + (1.0 - a) * prev
        out.append(prev)
    return out


def _rma(src, n):
    """Pine ta.rma over a series whose leading entries may be None (skipped until n values)."""
    out = [None] * len(src)
    prev = None
    window = []
    for t, v in enumerate(src):
        if v is None:
            continue
        if prev is None:
            window.append(v)
            if len(window) == n:
                prev = sum(window) / n
                out[t] = prev
        else:
            prev = (v + (n - 1) * prev) / n
            out[t] = prev
    return out


def _pivot(vals, t, n, high):
    c = t - n
    if c - n < 0:
        return None
    v = vals[c]
    for j in range(c - n, t + 1):
        if j != c and ((high and not v > vals[j]) or (not high and not v < vals[j])):
            return None
    return v


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]

    ema = _ema(c, BIAS_LEN)
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = [None if a is None else max(a, 1e-12) for a in _rma(tr, ATR_LEN)]
    up = [None] + [max(c[t] - c[t - 1], 0.0) for t in range(1, n)]
    dn = [None] + [max(c[t - 1] - c[t], 0.0) for t in range(1, n)]
    au, ad = _rma(up, RSI_LEN), _rma(dn, RSI_LEN)
    rsi = [None] * n
    for t in range(n):
        if au[t] is not None:
            if au[t] == 0 and ad[t] == 0:
                rsi[t] = 50.0
            elif ad[t] == 0:
                rsi[t] = 100.0
            else:
                rsi[t] = 100.0 - 100.0 / (1.0 + au[t] / ad[t])

    trend_dir = 0
    trend_hist = []
    last_ph = prev_ph = last_pl = prev_pl = None
    last_rsi_ph = prev_rsi_ph = last_rsi_pl = prev_rsi_pl = None
    dist_hist = []
    decay = 0.0 if HOLD_BARS <= 1 else 1.0 - 1.0 / HOLD_BARS
    near_h = sfp_h = struct_h = div_h = 0.0
    consumed_pl = consumed_ph = None
    weight_sum = W_NEAR + W_SFP + W_STRUCT + W_DIV
    reachable = weight_sum - min(W_STRUCT, W_DIV)
    risk_raw_hist = []
    a_risk = 2.0 / (SMOOTH_LEN + 1.0)
    risk_prev = None
    out = {k: [None] * n for k in ("risk", "risk_raw", "trend", "near", "sfp", "structure",
                                   "divergence", "rsi", "ema", "break_level", "distance", "state")}
    events = {k: [] for k in EVENTS}

    for t in range(n):
        slope = ema[t] - ema[t - 5] if t >= 5 else None
        if slope is not None and c[t] > ema[t] and slope > 0:
            raw = 1
        elif slope is not None and c[t] < ema[t] and slope < 0:
            raw = -1
        else:
            raw = 0
        if raw != 0:
            trend_dir = raw
        trend_hist.append(trend_dir)

        ph = _pivot(h, t, PIV_LEN, True)
        pl = _pivot(lo, t, PIV_LEN, False)
        if ph is not None:
            prev_ph, last_ph = last_ph, ph
            prev_rsi_ph, last_rsi_ph = last_rsi_ph, rsi[t - PIV_LEN]
        if pl is not None:
            prev_pl, last_pl = last_pl, pl
            prev_rsi_pl, last_rsi_pl = last_rsi_pl, rsi[t - PIV_LEN]

        s_near = s_sfp = s_struct = s_div = 0.0
        dist = None
        break_level = None
        a = atr[t]
        prev_dist = dist_hist[t - 1] if t >= 1 else None
        if trend_dir == 1:
            break_level = last_pl
            if last_pl is not None and a is not None:
                dist = (c[t] - last_pl) / a
                approaching = prev_dist is not None and dist < prev_dist
                s_near = 1.0 - _norm(dist, 0.0, NEAR_ATR) if dist >= 0.0 and approaching else 0.0
            if last_ph is not None and h[t] > last_ph and c[t] < last_ph:
                s_sfp = 1.0
            if ph is not None and last_ph is not None and prev_ph is not None and last_ph < prev_ph \
                    and a is not None:
                s_struct = _norm((prev_ph - last_ph) / a, 0.0, 2.0)
            if ph is not None and None not in (last_ph, prev_ph, last_rsi_ph, prev_rsi_ph):
                if last_ph > prev_ph and last_rsi_ph < prev_rsi_ph:
                    s_div = 1.0
        elif trend_dir == -1:
            break_level = last_ph
            if last_ph is not None and a is not None:
                dist = (last_ph - c[t]) / a
                approaching = prev_dist is not None and dist < prev_dist
                s_near = 1.0 - _norm(dist, 0.0, NEAR_ATR) if dist >= 0.0 and approaching else 0.0
            if last_pl is not None and lo[t] < last_pl and c[t] > last_pl:
                s_sfp = 1.0
            if pl is not None and last_pl is not None and prev_pl is not None and last_pl > prev_pl \
                    and a is not None:
                s_struct = _norm((last_pl - prev_pl) / a, 0.0, 2.0)
            if pl is not None and None not in (last_pl, prev_pl, last_rsi_pl, prev_rsi_pl):
                if last_pl < prev_pl and last_rsi_pl > prev_rsi_pl:
                    s_div = 1.0
        dist_hist.append(dist)

        near_h = max(s_near, near_h * decay)
        sfp_h = max(s_sfp, sfp_h * decay)
        struct_h = max(s_struct, struct_h * decay)
        div_h = max(s_div, div_h * decay)

        bos_trend = trend_hist[t - 1] if t >= 1 else 0
        c1 = c[t - 1] if t >= 1 else None
        raw_up = bos_trend == 1 and last_pl is not None and c[t] < last_pl and c1 is not None \
            and c1 >= last_pl
        raw_dn = bos_trend == -1 and last_ph is not None and c[t] > last_ph and c1 is not None \
            and c1 <= last_ph
        ctx_ok = (not REQUIRE_CONTEXT_FLIP) \
            or (raw_up and c[t] < ema[t] and slope is not None and slope < 0) \
            or (raw_dn and c[t] > ema[t] and slope is not None and slope > 0)
        flip_up = REQUIRE_CONTEXT_FLIP and bos_trend == 1 and trend_dir == -1 and not raw_up \
            and last_pl is not None and c[t] < last_pl and (consumed_pl is None or last_pl != consumed_pl)
        flip_dn = REQUIRE_CONTEXT_FLIP and bos_trend == -1 and trend_dir == 1 and not raw_dn \
            and last_ph is not None and c[t] > last_ph and (consumed_ph is None or last_ph != consumed_ph)
        bos_up = (raw_up and ctx_ok) or flip_up
        bos_dn = (raw_dn and ctx_ok) or flip_dn
        bos_pressure = (raw_up or raw_dn) and not ctx_ok
        if bos_up:
            consumed_pl = last_pl
        if bos_dn:
            consumed_ph = last_ph

        risk_raw = 0.0
        if trend_dir != 0 and reachable > 0.0:
            risk_raw = min((near_h * W_NEAR + sfp_h * W_SFP + struct_h * W_STRUCT + div_h * W_DIV)
                           / reachable * 100.0, 100.0)
        risk_raw_hist.append(risk_raw)
        risk = risk_raw if risk_prev is None else a_risk * risk_raw + (1.0 - a_risk) * risk_prev
        last_risk, risk_prev = risk_prev, risk

        if atr[t] is None or rsi[t] is None:
            continue

        if trend_dir == 0:
            state = 0
        elif bos_up or bos_dn:
            state = 1
        elif bos_pressure:
            state = 2
        elif risk >= LVL_CRIT:
            state = 3
        elif risk >= LVL_HIGH:
            state = 2
        elif risk >= LVL_ELEV:
            state = 4
        else:
            state = 5

        def crossover(level):
            return last_risk is not None and risk > level and last_risk <= level

        side = "bear" if trend_dir == 1 else "bull"
        if trend_dir != 0 and crossover(LVL_HIGH):
            events[f"{side}_break_pressure"].append(t)
        if trend_dir != 0 and crossover(LVL_CRIT):
            events[f"{side}_break_critical"].append(t)
        if bos_up:
            events["bear_structure_break"].append(t)
        if bos_dn:
            events["bull_structure_break"].append(t)
        if s_div > 0:
            events[f"{side}_divergence"].append(t)

        for k, v in (("risk", risk), ("risk_raw", risk_raw), ("trend", float(trend_dir)),
                     ("near", near_h), ("sfp", sfp_h), ("structure", struct_h),
                     ("divergence", div_h), ("rsi", rsi[t]), ("ema", ema[t]),
                     ("break_level", break_level), ("distance", dist), ("state", float(state))):
            out[k][t] = v
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Trend Break Risk",
    "",
    *provenance("trend_break_risk"),
    "",
    "Formula: the original definition with its",
    "defaults (EMA 50, pivots 5, near 1.5 ATR, RSI 14, ATR 14, memory 10, smoothing 3, weights",
    "25/20/15/10, levels 35/60/80, context flip required). Conventions the original leaves",
    "open: EMA seeded with the first close, ATR = Wilder average of the true range (first bar",
    "high - low) seeded with an SMA and floored at 1e-12, RSI = Wilder averages of the close",
    "changes seeded with an SMA (50 when nothing moved), pivots strict on both sides; a sensor",
    "that needs ATR or RSI is 0 while they are missing; output from the first bar with ATR and",
    "RSI.",
    "",
    "Series: as golden_price_average_relation (900 bars, prev = previous close, 100 before",
    "the first bar, drift 0.18 up to bar 219, -0.02 to 379, -0.30 to 469, 0.12 after, except 0",
    "with amp 0.08 for bars 560-649 and 0.45 for 650-689; close = prev + drift + amp",
    "(0.9 sin(i/3.1) + 0.6 sin(i/11) + 0.35 cos(i/1.7)); open = prev + 0.2 sin(i/2.3); high =",
    "max(open, close) + 0.4 + 0.3 |sin(i/4.7)|; low = min(open, close) - 0.4 - 0.3 |cos(i/5.3)|).",
    "",
    "state codes: 0 no_trend, 1 structure_broken, 2 break_pressure, 3 critical, 4 watch, 5 quiet.",
    "ev_<kind>_n is the number of alerts of that kind, ev_<kind>_<j> the bar of the j-th (up to 8).",
]


def build():
    out = compute(_series())
    first = next(t for t in range(N_BARS) if out["risk"][t] is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {k: out[k][cp] for k in ("risk", "risk_raw", "trend", "near", "sfp", "structure",
                                        "divergence", "rsi", "ema", "break_level", "distance",
                                        "state")}
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    ev = {}
    for name in EVENTS:
        bars = out["events"][name]
        ev[f"{name}_n"] = float(len(bars))
        for j, b in enumerate(bars[:8]):
            ev[f"{name}_{j}"] = float(b)
    blocks.append(("ev", "events", ev))
    blocks.append((None, None, {"sbr_tolerance": 1e-9}))
    return HEADER, blocks
