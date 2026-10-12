"""golden_roc_advanced: ROC Advanced (WavesUnchained).

Written from the published Pine Script, not from the Rust code. EMA smoothing, default levels.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_roc_advanced"

ROC_LEN = 12
AVG_LEN = 3
SIG_LEN = 9
MID = 0.0
OB = 5.0
OS = -5.0
STALL_LEN = 5
STALL_FLAT = 0.5
STALL_MIN = 3.0
CTX_LEN = 50
LB_EXTREME = 5
DIV_LEN = 4
DIV_MIN = 2.0

N_BARS = 900
CHECKPOINTS = [80, 200, 333, 450, 617, 690, 780, 899]
EVENTS = ["bull_cross", "bull_cross_weak", "bear_cross", "bear_cross_weak", "bull_zero_cross",
          "bear_zero_cross", "bull_stall", "bear_stall"]


def roc(c, n):
    return [((100.0 * (c[t] - c[t - n]) / c[t - n]) if c[t - n] != 0.0 else 0.0)
            if t >= n else None for t in range(len(c))]


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    line = ps.ema(roc(c, ROC_LEN), AVG_LEN)
    sig = ps.ema(line, SIG_LEN)
    ctx = ps.ema(roc(c, CTX_LEN), AVG_LEN)
    atr = ps.atr(h, lo, c, 14)
    lo_ext = ps.lowest(line, LB_EXTREME)
    hi_ext = ps.highest(line, LB_EXTREME)
    mid = [MID] * n

    out = {k: [None] * n for k in ("line", "signal", "ctx", "sentiment", "stall", "divergence",
                                   "quality")}
    events = {e: [] for e in EVENTS}
    prev_stall = 0
    for t in range(n):
        need = (line[t], sig[t], ctx[t], atr[t], lo_ext[t], ps.at(line, t, STALL_LEN),
                ps.at(ctx, t, DIV_LEN), ps.at(c, t, STALL_LEN))
        if not ps.ok(*need):
            continue
        roc_chg = line[t] - line[t - STALL_LEN]
        price_chg = (c[t] - c[t - STALL_LEN]) / atr[t] if atr[t] != 0.0 else 0.0
        flat = abs(price_chg) < STALL_FLAT
        stall_bull = roc_chg < -STALL_MIN and flat
        stall_bear = roc_chg > STALL_MIN and flat
        stall = 1 if stall_bull else (-1 if stall_bear else 0)
        ctx_bull = ctx[t] > MID
        ctx_bear = ctx[t] < MID
        bull_x = bool(ps.crossover(line, sig, t))
        bear_x = bool(ps.crossunder(line, sig, t))
        bull_ext = bull_x and lo_ext[t] <= OS
        bear_ext = bear_x and hi_ext[t] >= OB
        bull_counter = bull_ext and ctx_bear
        bear_counter = bear_ext and ctx_bull
        bull_depth = ps.clamp01((OS - lo_ext[t]) / abs(MID - OS))
        bear_depth = ps.clamp01((hi_ext[t] - OB) / abs(OB - MID))
        bull_q = min(100.0, bull_depth * 50.0 + (25.0 if ctx_bull else 0.0)
                     + (25.0 if not stall_bear else 0.0))
        bear_q = min(100.0, bear_depth * 50.0 + (25.0 if ctx_bear else 0.0)
                     + (25.0 if not stall_bull else 0.0))
        if bull_ext:
            events["bull_cross_weak" if (stall_bear or bull_counter) else "bull_cross"].append(t)
        if bear_ext:
            events["bear_cross_weak" if (stall_bull or bear_counter) else "bear_cross"].append(t)
        if ps.crossover(line, mid, t):
            events["bull_zero_cross"].append(t)
        if ps.crossunder(line, mid, t):
            events["bear_zero_cross"].append(t)
        if stall == 1 and prev_stall != 1:
            events["bull_stall"].append(t)
        if stall == -1 and prev_stall != -1:
            events["bear_stall"].append(t)
        prev_stall = stall
        bull_pct = max(0.0, line[t] - MID) / (OB - MID) * 100.0
        bear_pct = max(0.0, MID - line[t]) / (MID - OS) * 100.0
        fast_dir = line[t] - line[t - DIV_LEN]
        slow_dir = ctx[t] - ctx[t - DIV_LEN]
        div = 1 if (fast_dir < -DIV_MIN and slow_dir > 0.0) else (
            -1 if (fast_dir > DIV_MIN and slow_dir < 0.0) else 0)
        for k, val in (("line", line[t]), ("signal", sig[t]), ("ctx", ctx[t]),
                       ("sentiment", ps.clamp(bull_pct - bear_pct, -100.0, 100.0)),
                       ("stall", float(stall)), ("divergence", float(div)),
                       ("quality", bull_q if bull_ext else (bear_q if bear_ext else None))):
            out[k][t] = val
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: ROC Advanced",
    "",
    *provenance("roc_advanced"),
    "",
    "Formula: the published Pine Script 'ROC Advanced' (WavesUnchained), EMA smoothing, ROC 12,",
    "average 3, signal 9, levels 0 / +5 / -5, stall 5 bars / 0.5 ATR / 3, context ROC 50,",
    "stretch lookback 5, divergence 4 / 2. Conventions where Pine leaves them open: EMAs seeded",
    "with the first ROC value; ATR Wilder average seeded with an SMA. A bar reports once line,",
    "signal, context, ATR and every lookback it reads exist. A stretch cross is *_cross_weak when",
    "a stall against it or a context on the other side of zero is present, *_cross otherwise;",
    "stall alerts fire on the first bar of a stall.",
    "",
    "Series: pine_series.wave_series(900) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6); q_<j> the",
    "quality of the j-th stretch cross in bar order (up to 8).",
]


def build():
    out = compute(ps.wave_series(N_BARS))
    first = next(t for t, r in enumerate(out["line"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        blocks.append((f"cp{cp}", f"bar {cp}", {k: out[k][cp] for k in (
            "line", "signal", "ctx", "sentiment", "stall", "divergence")}))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    qs = [q for q in out["quality"] if q is not None][:8]
    blocks.append(("q", "cross quality", {str(j): q for j, q in enumerate(qs)}))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
