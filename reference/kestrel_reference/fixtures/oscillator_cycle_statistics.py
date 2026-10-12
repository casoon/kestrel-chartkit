"""golden_oscillator_cycle_statistics: Oscillator Cycle Statistics v1.5.6 (WavesUnchained).

Written from the published Pine Script, not from the Rust code. RSI 14 with its 70/30 zones,
maximum cycle 150 bars, medium lookback 1000, 1500 stored samples; the regime, reliability and
lookback-comparison layers are not part of the port.
"""

import math

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_oscillator_cycle_statistics"

RSI_LEN = 14
OB = 70.0
OS = 30.0
MAX_CYCLE = 150
LOOKBACK = 1000
MAX_SAMPLES = 1500
MIN_SAMPLES = 20
EDGE_TH = 15.0
ZONE_MIN = 30
ZONE_MAX = 65
MAX_CI = 40.0
BUCKETS = 10

N_BARS = 4000
SEED = 9
CHECKPOINTS = [300, 1000, 1700, 2222, 2900, 3333, 3700, 3999]
EVENTS = ["cycle_success", "cycle_failure", "cycle_timeout", "bull_cycle_opportunity",
          "bear_cycle_opportunity"]
STATE_CODE = {"idle": 0.0, "low_data": 1.0, "stalled": 2.0, "uncertain": 3.0, "late": 4.0,
              "opportunity": 5.0, "watch": 6.0, "wait": 7.0}


def median(vals):
    if not vals:
        return None
    s = sorted(vals)
    m = len(s) // 2
    return s[m] if len(s) % 2 == 1 else (s[m - 1] + s[m]) * 0.5


def wilson(success, total):
    if total <= 0:
        return None, None
    z = 1.96
    n = float(total)
    p = success / n
    z2 = z * z
    center = (p + z2 / (2.0 * n)) / (1.0 + z2 / n)
    margin = z / (1.0 + z2 / n) * math.sqrt(p * (1.0 - p) / n + z2 / (4.0 * n * n))
    return 100.0 * max(0.0, center - margin), 100.0 * min(1.0, center + margin)


def statistics(samples, t, direction, progress, age):
    """(conditional success, conditional total, median duration of conditioned successes)."""
    bucket = int(math.floor(ps.clamp(progress, 0.0, 1.0) * BUCKETS + 0.000001))
    bucket_progress = bucket * 1.0 / BUCKETS
    cs = ct = 0
    durations = []
    for s in samples:
        if t - s["end"] > LOOKBACK or s["dir"] != direction:
            continue
        reached = True
        if age >= 0 and bucket > 0:
            first = s["reach"][bucket - 1]
            reached = 0 <= first <= age
        if s["max"] >= bucket_progress and (age < 0 or s["dur"] > age) and reached:
            ct += 1
            cs += s["ok"]
            if s["ok"] == 1:
                durations.append(float(s["dur"]))
    return cs, ct, median(durations)


def base_rate(samples, t, direction):
    succ = tot = 0
    for s in samples:
        if t - s["end"] <= LOOKBACK and s["dir"] == direction:
            succ += s["ok"]
            tot += 1
    return 100.0 * succ / tot if tot > 0 else None


def compute(bars, min_samples=MIN_SAMPLES, max_ci=MAX_CI):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    osc = ps.rsi(c, RSI_LEN)
    atr = ps.atr(h, lo, c, 14)

    samples = []
    cyc = None  # active cycle
    out = {k: [None] * n for k in ("osc", "dir", "progress", "conditional", "cond_n", "edge",
                                   "state")}
    events = {e: [] for e in EVENTS}

    def new_cycle(direction, t):
        return {"dir": direction, "start": t, "price": c[t], "atr": atr[t], "armed": False,
                "max": 0.0, "mfe": 0.0, "mae": 0.0, "reach": [-1] * BUCKETS,
                "marked": False}

    def store(cy, ok, t):
        samples.append({"dir": cy["dir"], "ok": ok, "dur": t - cy["start"], "end": t,
                        "max": cy["max"], "reach": list(cy["reach"])})
        if len(samples) > MAX_SAMPLES:
            samples.pop(0)

    def progress_of(cy, v):
        if cy is None:
            return 0.0
        if cy["dir"] == -1:
            return ps.clamp((OB - v) / (OB - OS), 0.0, 1.0)
        return ps.clamp((v - OS) / (OB - OS), 0.0, 1.0)

    for t in range(n):
        v = osc[t]
        if v is None:
            continue
        pv = osc[t - 1] if t > 0 and osc[t - 1] is not None else None
        entered_ob = v >= OB and (pv if pv is not None else OB - 1) < OB
        entered_os = v <= OS and (pv if pv is not None else OS + 1) > OS
        prog = progress_of(cyc, v)
        if cyc is not None and cyc["atr"] is not None and cyc["atr"] > 0:
            if cyc["dir"] == -1:
                fav = max(0.0, (cyc["price"] - lo[t]) / cyc["atr"])
                adv = max(0.0, (h[t] - cyc["price"]) / cyc["atr"])
            else:
                fav = max(0.0, (h[t] - cyc["price"]) / cyc["atr"])
                adv = max(0.0, (cyc["price"] - lo[t]) / cyc["atr"])
            cyc["mfe"] = max(cyc["mfe"], fav)
            cyc["mae"] = max(cyc["mae"], adv)
        if cyc is not None:
            age = t - cyc["start"]
            target = (cyc["dir"] == -1 and v <= OS) or (cyc["dir"] == 1 and v >= OB)
            timeout = age >= MAX_CYCLE
            if prog > cyc["max"]:
                cyc["max"] = prog
            for b in range(BUCKETS):
                th = (b + 1.0) / BUCKETS
                if prog >= th and cyc["reach"][b] < 0:
                    cyc["reach"][b] = age
                    if not cyc["marked"] and not target and not timeout \
                            and th >= ZONE_MIN / 100.0 and th <= ZONE_MAX / 100.0:
                        cs, ct, _ = statistics(samples, t, cyc["dir"], th, age)
                        cond = 100.0 * cs / ct if ct > 0 else None
                        base = base_rate(samples, t, cyc["dir"])
                        edge = cond - base if cond is not None and base is not None else None
                        lw, up = wilson(cs, ct)
                        width = up - lw if lw is not None else None
                        if ct >= min_samples and edge is not None and edge >= EDGE_TH \
                                and width is not None and width <= max_ci:
                            events["bull_cycle_opportunity" if cyc["dir"] == 1
                                   else "bear_cycle_opportunity"].append(t)
                            cyc["marked"] = True
        # state machine
        if cyc is None:
            if entered_ob:
                cyc = new_cycle(-1, t)
            elif entered_os:
                cyc = new_cycle(1, t)
        else:
            d = cyc["dir"]
            age = t - cyc["start"]
            if not cyc["armed"] and ((d == -1 and v < OB) or (d == 1 and v > OS)):
                cyc["armed"] = True
            if (d == -1 and v <= OS) or (d == 1 and v >= OB):
                store(cyc, 1, t)
                events["cycle_success"].append(t)
                cyc = new_cycle(-d, t)
            elif cyc["armed"] and ((d == -1 and v >= OB) or (d == 1 and v <= OS)):
                store(cyc, 0, t)
                events["cycle_failure"].append(t)
                cyc = new_cycle(d, t)
            elif age >= MAX_CYCLE:
                store(cyc, 0, t)
                events["cycle_timeout"].append(t)
                cyc = None
        out["osc"][t] = v
        if cyc is None:
            out["dir"][t] = 0.0
            out["state"][t] = STATE_CODE["idle"]
            continue
        age = t - cyc["start"]
        cs, ct, med = statistics(samples, t, cyc["dir"], cyc["max"], age)
        cond = 100.0 * cs / ct if ct > 0 else None
        base = base_rate(samples, t, cyc["dir"])
        edge = cond - base if cond is not None and base is not None else None
        lw, up = wilson(cs, ct)
        width = up - lw if lw is not None else None
        cur = progress_of(cyc, v)
        if ct < min_samples or edge is None:
            word = "low_data"
        elif med is not None and age > med:
            word = "stalled"
        elif width is None or width > max_ci:
            word = "uncertain"
        elif cur > ZONE_MAX / 100.0:
            word = "late"
        elif edge >= EDGE_TH and ZONE_MIN / 100.0 <= cur <= ZONE_MAX / 100.0:
            word = "opportunity"
        elif edge >= EDGE_TH / 2.0:
            word = "watch"
        else:
            word = "wait"
        out["dir"][t] = float(cyc["dir"])
        out["progress"][t] = cur
        out["conditional"][t] = cond
        out["cond_n"][t] = float(ct)
        out["edge"][t] = edge
        out["state"][t] = STATE_CODE[word]
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Oscillator Cycle Statistics",
    "",
    *provenance("oscillator_cycle_statistics"),
    "",
    "Formula: the published Pine Script 'Oscillator Cycle Statistics' v1.5.6 (WavesUnchained),",
    "RSI 14 with zones 70/30, maximum cycle 150 bars, medium lookback 1000, 1500 stored samples,",
    "minimum 20 conditional samples, edge 15 pp, opportunity zone 30-65 %, Wilson width 40 pp.",
    "Conventions where Pine leaves them open: RSI and ATR Wilder averages seeded with an SMA; the",
    "first RSI bar counts as entering a zone it is in; the live state word uses the progress of",
    "the cycle active after the bar (Pine reuses the pre-transition value on a changing bar). A",
    "bar reports once the RSI exists. State codes: 0 idle, 1 low data, 2 stalled, 3 uncertain,",
    "4 late, 5 opportunity, 6 watch, 7 wait.",
    "",
    "Series: pine_series.lcg_series(4000, seed 9) (see reference/kestrel_reference/pine_series.py).",
    "",
    "cp<bar>_* the outputs at that bar (conditional and edge only when they exist); ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6); rx_ev_*",
    "the same with minimum 5 samples and Wilson width 60 (the defaults rarely fire here).",
]

RELAXED_MIN = 5
RELAXED_CI = 60.0


def build():
    bars = ps.lcg_series(N_BARS, SEED)
    out = compute(bars)
    first = next(t for t, r in enumerate(out["osc"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {k: out[k][cp] for k in ("osc", "dir", "state")}
        for k in ("progress", "conditional", "cond_n", "edge"):
            if out[k][cp] is not None:
                case[k] = out[k][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    relaxed = compute(bars, RELAXED_MIN, RELAXED_CI)
    blocks.append(("rx_ev", "events with minimum 5 samples and Wilson width 60",
                   ps.events_block(relaxed["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
