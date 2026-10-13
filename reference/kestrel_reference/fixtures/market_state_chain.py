"""golden_market_state_chain: Market State Chain.

Written from the original definition, not from the Rust code. Default parameters.
"""

import math

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_market_state_chain"

EMA_LEN = 55
ATR_LEN = 14
EFF_LEN = 34
ADX_LEN = 14
RANK_LEN = 100
BB_LEN = 20
TREND_TH = 22.0
EFF_TH = 38.0
CHAOS_RANK = 70.0
COMP_RANK = 35.0
SLOPE_TH = 0.05
LOOKBACK = 500
LAPLACE = 1.0
MIN_SAMPLES = 20
MIN_DWELL = 2
CONF_TH = 55.0
REL_TH = 40.0
BIAS_MARGIN = 8.0

RELAXED_CONF = 45.0
RELAXED_REL = 3.0

N_BARS = 2000
SEED = 4
CHECKPOINTS = [150, 400, 640, 900, 1111, 1400, 1700, 1999]
EVENTS = ["bull_reversal", "bull_breakout", "bull_continuation", "bear_reversal",
          "bear_breakout", "bear_continuation", "regime_change", "state_extended"]


def compute(bars, conf_th=CONF_TH, rel_th=REL_TH):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    ema = ps.ema(c, EMA_LEN)
    _, _, adx = ps.dmi(h, lo, c, ADX_LEN, ADX_LEN)
    atr = ps.atr(h, lo, c, ATR_LEN)
    a_lo = ps.lowest(atr, RANK_LEN)
    a_hi = ps.highest(atr, RANK_LEN)
    basis = ps.sma(c, BB_LEN)
    dev = ps.stdev(c, BB_LEN)
    bbw = ps.smap(lambda d, b: d / b * 100 if b != 0 else 0.0, dev, basis)
    b_lo = ps.lowest(bbw, RANK_LEN)
    b_hi = ps.highest(bbw, RANK_LEN)

    state = [None] * n
    ctx = [None] * n
    raw_prev = None
    raw_run = 0
    cur = 2
    bias = 0
    prev_edge = {1: False, -1: False}
    prev_coiled = False
    last_change = None
    out = {k: [None] * n for k in ("net", "state", "p_bull", "p_bear", "p_neutral",
                                   "persistence", "reliability", "bias", "dwell_ratio",
                                   "exits")}
    events = {e: [] for e in EVENTS}
    for t in range(n):
        need = (ema[t], adx[t], atr[t], ps.at(ema, t, 5), a_lo[t], b_lo[t])
        if not ps.ok(*need) or t < EFF_LEN - 1:
            raw_prev = None
            continue
        slope = (ema[t] - ema[t - 5]) / atr[t] if atr[t] != 0 else 0.0
        a_rank = (atr[t] - a_lo[t]) / (a_hi[t] - a_lo[t]) * 100 if a_hi[t] != a_lo[t] else 50.0
        b_rank = (bbw[t] - b_lo[t]) / (b_hi[t] - b_lo[t]) * 100 if b_hi[t] != b_lo[t] else 50.0
        path = sum(abs(c[t - i] - c[t - i - 1]) for i in range(EFF_LEN - 1))
        direct = abs(c[t] - c[t - EFF_LEN + 1])
        eff = ps.clamp(direct / path * 100, 0.0, 100.0) if path > 0 else 0.0
        up = c[t] > ema[t] and slope > SLOPE_TH
        dn = c[t] < ema[t] and slope < -SLOPE_TH
        high_trend = adx[t] > TREND_TH and eff > EFF_TH
        if a_rank > CHAOS_RANK and eff < EFF_TH:
            raw = 5
        elif a_rank < COMP_RANK and b_rank < COMP_RANK and eff < EFF_TH:
            raw = 2
        elif up and high_trend:
            raw = 0
        elif dn and high_trend:
            raw = 3
        elif up:
            raw = 1
        elif dn:
            raw = 4
        else:
            raw = 2
        raw_run = 1 if (raw_prev is None or raw != raw_prev) else raw_run + 1
        raw_prev = raw
        prev_state = state[t - 1] if t > 0 else None
        if raw_run >= MIN_DWELL:
            cur = raw
        state[t] = cur
        ctx[t] = (6 if up else (7 if dn else 2)) if cur == 2 else cur

        counts = [0.0] * 6
        c_self = 0.0
        for i in range(1, LOOKBACK + 1):
            if t - i < 0:
                break
            if ctx[t - i] is not None and ctx[t - i] == ctx[t]:
                to = state[t - i + 1]
                if to == cur:
                    c_self += 1
                else:
                    counts[to] += 1
        total = sum(counts)
        total_all = total + c_self
        persistence = c_self / total_all * 100 if total_all > 0 else None
        denom = total + 5 * LAPLACE
        p = [0.0 if k == cur else (counts[k] + LAPLACE) / denom * 100 for k in range(6)]
        p_bull, p_bear, p_neu = p[0] + p[1], p[3] + p[4], p[2] + p[5]

        def term(x):
            q = x / 100.0
            return -q * math.log(q) if q > 0 else 0.0
        ent = term(p_bull) + term(p_bear) + term(p_neu)
        pred = ps.clamp(1 - ent / math.log(3), 0.0, 1.0)
        rel = min(1.0, total / MIN_SAMPLES) * pred * 100
        dprob = max(p_bull, max(p_bear, p_neu))
        if p_bull - max(p_bear, p_neu) >= BIAS_MARGIN:
            bias = 1
        elif p_bear - max(p_bull, p_neu) >= BIAS_MARGIN:
            bias = -1
        elif p_neu - max(p_bull, p_bear) >= BIAS_MARGIN:
            bias = 0
        ok_sig = dprob >= conf_th and rel >= rel_th
        bull_edge = bias == 1 and ok_sig and cur != 0
        bear_edge = bias == -1 and ok_sig and cur != 3
        if bull_edge and not prev_edge[1]:
            kind = "bull_reversal" if cur in (3, 4) else ("bull_breakout" if cur in (2, 5)
                                                         else "bull_continuation")
            events[kind].append(t)
        if bear_edge and not prev_edge[-1]:
            kind = "bear_reversal" if cur in (0, 1) else ("bear_breakout" if cur in (2, 5)
                                                         else "bear_continuation")
            events[kind].append(t)
        prev_edge = {1: bull_edge, -1: bear_edge}
        changed = prev_state is not None and cur != prev_state
        if changed:
            events["regime_change"].append(t)
            last_change = t
        bars_in = t - last_change if last_change is not None else 0
        pf = (persistence if persistence is not None else 0.0) / 100.0
        typical = 1.0 / (1.0 - pf) if 0 < pf < 0.999 else None
        dwell = bars_in / typical if typical is not None and typical > 0 else None
        coiled = dwell is not None and dwell >= 1.5
        if coiled and not prev_coiled:
            events["state_extended"].append(t)
        prev_coiled = coiled
        for k, val in (("net", p_bull - p_bear), ("state", float(cur)), ("p_bull", p_bull),
                       ("p_bear", p_bear), ("p_neutral", p_neu), ("persistence", persistence),
                       ("reliability", rel), ("bias", float(bias)), ("dwell_ratio", dwell),
                       ("exits", total)):
            out[k][t] = val
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Market State Chain",
    "",
    *provenance("market_state_chain"),
    "",
    "Formula: the original definition with its defaults",
    "(EMA 55, ATR 14, efficiency 34, ADX 14, rank 100, BB 20, thresholds 22/38/70/35/0.05,",
    "lookback 500, Laplace 1, min samples 20, dwell 2, confidence 55, reliability 40, margin 8).",
    "Conventions the original leaves open: EMA seeded with its first value; ATR and DMI Wilder",
    "averages seeded with an SMA (DMI from the second bar); population standard deviation; a bar",
    "is classified once every input exists, earlier bars carry no state and never match; the",
    "debounced state starts at Compression (2); bars-in-state is 0 until the first state change.",
    "Edge events are typed by the state they come out of: reversal, breakout (out of Compression",
    "or Chaos), continuation; state_extended fires when the dwell ratio first reaches 1.5.",
    "",
    "Series: pine_series.lcg_series(2000, seed 4) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first classified bar; cp<bar>_* the outputs at that bar (persistence",
    "and dwell_ratio only when they exist); ev_<kind>_n the number of events of that kind and",
    "ev_<kind>_<j> the bar of the j-th (up to 6); rx_ev_* the same with confidence 45 and",
    "reliability 3 (the defaults never reach an edge on this series).",
]


def build():
    out = compute(ps.lcg_series(N_BARS, SEED))
    first = next(t for t, r in enumerate(out["state"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {k: out[k][cp] for k in ("net", "state", "p_bull", "p_bear", "p_neutral",
                                        "reliability", "bias", "exits")}
        for k in ("persistence", "dwell_ratio"):
            if out[k][cp] is not None:
                case[k] = out[k][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    relaxed = compute(ps.lcg_series(N_BARS, SEED), RELAXED_CONF, RELAXED_REL)
    blocks.append(("rx_ev", "events with confidence 45 / reliability 3",
                   ps.events_block(relaxed["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
