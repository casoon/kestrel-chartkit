"""golden_coilforge_zones: CoilForge Zones v1.2.2 (WavesUnchained).

Written from the published Pine Script, not from the Rust code. Sensitivity Balanced, all
modules on, breakout quality Confirmed, contrary-read filter on.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_coilforge_zones"

LOOKBACK = 16
MIN_ZONE_BARS = 5
RANGE_ATR = 4.2
ADX_MAX = 27.0
MIN_SCORE = 35.0
MIN_TOUCHES = 2
MIN_STRUCT = 2
ATR_LEN = 14
TOUCH_TOL = 0.25
VOL_LEN = 20
IMPULSE_LB = 12
IMPULSE_ATR = 1.8
HIST_LB = 250
HIST_TOL = 0.5
PIVOT = 5
WATCH = 20
BUFFER = 0.05
MIN_BODY = 0.12
MIN_CLOSE_POS = 0.60

N_BARS = 1500
SEED = 11
CHECKPOINTS = [100, 160, 328, 449, 587, 743, 1021, 1499]
EVENTS = ["bull_breakout", "bear_breakout"]
READ_CODE = {"none": 0.0, "balance": 0.0, "bull_pressure": 1.0, "bear_pressure": -1.0,
             "bull_continuation": 0.5, "bear_continuation": -0.5}


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    atr = ps.atr(h, lo, c, ATR_LEN)
    adx = ps.adx(h, lo, c, 14, 14)
    r_hi = ps.highest(h, LOOKBACK)
    r_lo = ps.lowest(lo, LOOKBACK)
    vol_ma = ps.sma(v, VOL_LEN)
    vol_ma_short = ps.sma(v, max(3, VOL_LEN // 2))
    ema_fast = ps.ema(c, int(max(2, ps.pine_round(LOOKBACK / 2))))
    ema_slow = ps.ema(c, LOOKBACK)
    cum = 0.0

    hist = []  # (level, bar) in push order
    in_zone = False
    zone_start = zone_high = zone_low = None
    up_t = lo_t = 0
    read = "none"
    armed = False
    armed_until = armed_high = armed_low = None
    armed_read = "none"

    out = {k: [None] * n for k in ("score", "range_atr", "in_zone", "zone_high", "zone_low",
                                   "upper_touches", "lower_touches", "read", "armed",
                                   "hist_reuse")}
    events = {e: [] for e in EVENTS}
    for t in range(n):
        cum += v[t]
        ph = ps.pivot(h, t, PIVOT, PIVOT, False)
        pl = ps.pivot(lo, t, PIVOT, PIVOT, True)
        if ph is not None:
            hist.append((ph, t - PIVOT))
        if pl is not None:
            hist.append((pl, t - PIVOT))
        while hist and t - hist[0][1] > HIST_LB:
            hist.pop(0)
        if not ps.ok(atr[t], adx[t], r_hi[t], ps.at(c, t, IMPULSE_LB)):
            continue
        a = atr[t]
        rng = r_hi[t] - r_lo[t]
        ratio = rng / a if a > 0 else None
        vol_avail = cum > 0 and v[t] > 0 and vol_ma[t] is not None
        drying = vol_avail and v[t] < vol_ma[t] and vol_ma_short[t] < vol_ma[t]
        move = c[t] - c[t - IMPULSE_LB]
        impulse = 1 if move > a * IMPULSE_ATR else (-1 if move < -a * IMPULSE_ATR else 0)
        range_ok = ratio is not None and ratio <= RANGE_ATR
        tol = a * TOUCH_TOL
        trend_up = ema_fast[t] > ema_slow[t]
        trend_dn = ema_fast[t] < ema_slow[t]
        adx_strong = adx[t] >= 20.0
        win_h = h[t - LOOKBACK + 1:t + 1]
        win_l = lo[t - LOOKBACK + 1:t + 1]
        # offset of the extreme bar (0 = current); on ties the most recent bar
        hi_off = min(i for i in range(LOOKBACK) if win_h[LOOKBACK - 1 - i] == r_hi[t])
        lo_off = min(i for i in range(LOOKBACK) if win_l[LOOKBACK - 1 - i] == r_lo[t])
        ut = sum(1 for i in range(LOOKBACK) if i != hi_off and h[t - i] >= r_hi[t] - tol)
        lt = sum(1 for i in range(LOOKBACK) if i != lo_off and lo[t - i] <= r_lo[t] + tol)
        touch_ok = ut > 0 and lt > 0 and ut + lt >= MIN_TOUCHES

        near_top = near_bot = False
        cutoff = t - LOOKBACK + 1
        for lvl, b in hist:
            if b >= cutoff:
                continue
            if abs(lvl - r_hi[t]) <= a * HIST_TOL:
                near_top = True
            if abs(lvl - r_lo[t]) <= a * HIST_TOL:
                near_bot = True
        reuse = near_top or near_bot

        score = 0.0
        score += 25.0 * (1.0 - min(ratio / RANGE_ATR, 1.0)) if range_ok else 0.0
        score += 15.0 * (1.0 - min(adx[t] / ADX_MAX, 1.0)) if adx[t] <= ADX_MAX else 0.0
        score += 15.0 if (not vol_avail or drying) else 0.0
        score += 15.0 if impulse != 0 else 0.0
        score += 10.0 if reuse else 0.0
        evidence = (1 if range_ok else 0) + (1 if touch_ok else 0)
        candidate = evidence >= min(MIN_STRUCT, 2) and score >= MIN_SCORE

        zone_age = t - zone_start + 1 if in_zone else 0
        start = candidate and not in_zone
        cont = candidate and in_zone
        end = not candidate and in_zone
        if start:
            in_zone = True
            zone_start = t - LOOKBACK + 1
            zone_high, zone_low = r_hi[t], r_lo[t]
            up_t = lo_t = 0
        if cont:
            zone_high = max(zone_high, r_hi[t])
            zone_low = min(zone_low, r_lo[t])
        if in_zone:
            if h[t] >= zone_high - tol:
                up_t += 1
            if lo[t] <= zone_low + tol:
                lo_t += 1
            p = up_t - lo_t
            need = 1 if adx_strong else 2
            if p >= 3 or (p >= need and trend_up):
                read = "bull_pressure"
            elif p <= -3 or (p <= -need and trend_dn):
                read = "bear_pressure"
            elif impulse == 1:
                read = "bull_continuation"
            elif impulse == -1:
                read = "bear_continuation"
            else:
                read = "balance"
        rec = {"in_zone": 1.0 if in_zone else 0.0, "zone_high": zone_high,
               "zone_low": zone_low, "upper_touches": float(up_t),
               "lower_touches": float(lo_t), "read": READ_CODE[read]}
        if end:
            if zone_age >= MIN_ZONE_BARS:
                armed = True
                armed_until = t + WATCH
                armed_high, armed_low, armed_read = zone_high, zone_low, read
                hist.append((zone_high, t))
                hist.append((zone_low, t))
            in_zone = False
            zone_start = zone_high = zone_low = None
            up_t = lo_t = 0
            read = "none"
        if armed and t > armed_until:
            armed = False
            armed_read = "none"
        vol_ok = not vol_avail or v[t] > vol_ma[t]
        buf = a * BUFFER
        brange = h[t] - lo[t]
        body = abs(c[t] - o[t])
        pos = (c[t] - lo[t]) / brange if brange > 0 else 0.5
        body_ok = body >= a * MIN_BODY if a > 0 else True
        read_up = armed_read not in ("bear_pressure", "bear_continuation")
        read_dn = armed_read not in ("bull_pressure", "bull_continuation")
        up = armed and c[t] > armed_high + buf and vol_ok and body_ok and pos >= MIN_CLOSE_POS \
            and read_up
        dn = armed and c[t] < armed_low - buf and vol_ok and body_ok \
            and pos <= 1.0 - MIN_CLOSE_POS and read_dn
        if up:
            events["bull_breakout"].append(t)
        if dn:
            events["bear_breakout"].append(t)
        if up or dn:
            armed = False
            armed_read = "none"
        rec.update({"score": score, "range_atr": ratio, "armed": 1.0 if armed else 0.0,
                    "hist_reuse": 1.0 if reuse else 0.0})
        for k, val in rec.items():
            out[k][t] = val
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: CoilForge Zones",
    "",
    *provenance("coilforge_zones"),
    "",
    "Formula: the published Pine Script 'CoilForge Zones v1.2.2' (WavesUnchained), sensitivity",
    "Balanced (range 4.2 ATR, ADX 27, score 35, touches 2), all modules on, breakout quality",
    "Confirmed, contrary-read filter on. Conventions where Pine leaves them open: ATR and ADX",
    "Wilder averages seeded with an SMA (ADX from the second bar); EMAs seeded with their first",
    "value; on ties the range extreme is the most recent bar; pivots strict on both sides; the",
    "historical level list expires from its front only, as in Pine. A bar reports once ATR, ADX,",
    "the range window and close[12] exist; volume counts as unavailable until its MA exists.",
    "Zone fields: in_zone/zone_high/zone_low/touches/read as seen on the bar before an ending zone",
    "is cleared; read code 1 bull pressure, 0.5 bull continuation, -0.5 / -1 bear, 0 otherwise.",
    "",
    "Series: pine_series.lcg_series(1500, seed 11) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(ps.lcg_series(N_BARS, SEED))
    first = next(t for t, r in enumerate(out["score"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {k: out[k][cp] for k in ("score", "range_atr", "in_zone", "upper_touches",
                                        "lower_touches", "read", "armed", "hist_reuse")}
        if out["zone_high"][cp] is not None:
            case["zone_high"] = out["zone_high"][cp]
            case["zone_low"] = out["zone_low"][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    zones = sum(1 for t in range(1, N_BARS) if out["in_zone"][t] == 1.0
                and out["in_zone"][t - 1] != 1.0)
    blocks.append((None, "zones started", {"zones_started": float(zones)}))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
