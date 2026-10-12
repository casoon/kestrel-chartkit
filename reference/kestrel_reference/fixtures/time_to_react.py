"""golden_time_to_react: Time-to-React + Volatility-Time (WavesUnchained).

Written from the published Pine Script, not from the Rust code. The 1h preset tier converted to
bars (reaction window 8 bars, validation window 20 bars, ATR move 1.6, weights 0.7/0.3 and
0.40/0.60), priority BOS > SWEEP > OB.
"""

import math

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_time_to_react"

ATR_LEN = 14
ATR_BASE = 50
ACT_K = 1.2
SWING = 10
REACT_BARS = 8
VALID_BARS = 20
ATR_MOVE = 1.6
W_A, W_B = 0.7, 0.3
W_SPEED, W_VAL = 0.40, 0.60
DISP_BODY = 1.0
OB_SEARCH = 12

N_BARS = 900
CHECKPOINTS = [100, 230, 333, 450, 617, 690, 780, 899]


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    atr = ps.atr(h, lo, c, ATR_LEN)
    atr_base = ps.sma(atr, ATR_BASE)
    hh = ps.highest(h, SWING)
    ll = ps.lowest(lo, SWING)
    wa, wb = W_A / max(0.0001, W_A + W_B), W_B / max(0.0001, W_A + W_B)
    ws, wv = W_SPEED / max(0.0001, W_SPEED + W_VAL), W_VAL / max(0.0001, W_SPEED + W_VAL)

    bull_ob = bear_ob = None  # (high, low)
    active = False
    ev = {}
    out = {k: [None] * n for k in ("activity", "reaction", "speed", "validation", "event_dir")}
    finals = []  # (bar, type, dir, final score)
    for t in range(n):
        lsh = ps.at(hh, t, 1)
        lsl = ps.at(ll, t, 1)
        a = atr[t]
        if a is not None and t >= 1:
            body = abs(c[t] - o[t])
            if c[t] > o[t] and body > DISP_BODY * a and c[t] > h[t - 1]:
                for i in range(1, OB_SEARCH + 1):
                    if t - i >= 0 and c[t - i] < o[t - i]:
                        bull_ob = (h[t - i], lo[t - i])
                        break
            if c[t] < o[t] and body > DISP_BODY * a and c[t] < lo[t - 1]:
                for i in range(1, OB_SEARCH + 1):
                    if t - i >= 0 and c[t - i] > o[t - i]:
                        bear_ob = (h[t - i], lo[t - i])
                        break
        bull_tap = bull_ob is not None and lo[t] <= bull_ob[0] and h[t] >= bull_ob[1]
        bear_tap = bear_ob is not None and h[t] >= bear_ob[1] and lo[t] <= bear_ob[0]
        bos_up = bos_dn = sweep_up = sweep_dn = False
        if lsh is not None:
            prev_lsh = ps.at(hh, t, 2)
            prev_lsl = ps.at(ll, t, 2)
            bos_up = c[t] > lsh and c[t - 1] <= (prev_lsh if prev_lsh is not None else lsh)
            bos_dn = c[t] < lsl and c[t - 1] >= (prev_lsl if prev_lsl is not None else lsl)
            sweep_up = h[t] > lsh and c[t] < lsh
            sweep_dn = lo[t] < lsl and c[t] > lsl
        fire = None
        if bos_up or bos_dn:
            fire = ("bos", 1 if bos_up else -1)
        elif sweep_up or sweep_dn:
            fire = ("sweep", 1 if sweep_dn else -1)
        elif bull_tap or bear_tap:
            fire = ("ob", 1 if bull_tap and not (bear_tap and c[t] < o[t]) else -1)
        final = None
        if not active and fire is not None and a is not None:
            active = True
            ev = {"type": fire[0], "dir": fire[1], "bar": t, "ref": c[t], "atr": a,
                  "reac": None, "hi": None, "lo": None, "val_a": False, "speed": None,
                  "val": None, "score": None}
        if active:
            age = t - ev["bar"]
            d = ev["dir"]
            if ev["reac"] is None and age >= 1:
                bull_r = c[t] > o[t] or lo[t] >= lo[t - 1]
                bear_r = c[t] < o[t] or h[t] <= h[t - 1]
                if (d == 1 and bull_r) or (d == -1 and bear_r):
                    ev["reac"], ev["hi"], ev["lo"] = t, h[t], lo[t]
            if ev["reac"] is not None and ev["speed"] is None:
                to_react = ev["reac"] - ev["bar"] - 1
                ev["speed"] = 100.0 * ps.clamp01(1.0 - to_react / max(1.0, REACT_BARS - 1))
            if ev["reac"] is not None:
                since = t - ev["reac"]
                if not ev["val_a"] and since <= VALID_BARS:
                    ev["val_a"] = c[t] > ev["hi"] if d == 1 else c[t] < ev["lo"]
                target = ATR_MOVE * ev["atr"]
                move = h[t] - ev["ref"] if d == 1 else ev["ref"] - lo[t]
                prog = ps.clamp01(move / target if target > 0 else 0.0)
                ev["val"] = 100.0 * (wa * (1.0 if ev["val_a"] else 0.0) + wb * prog)
                ev["score"] = ps.clamp(ws * ev["speed"] + wv * ev["val"], 0.0, 100.0)
                if since > VALID_BARS:
                    active = False
                    final = ev["score"]
            if ev["reac"] is None and age > REACT_BARS:
                ev["speed"] = ev["val"] = ev["score"] = 0.0
                active = False
                final = 0.0
        if atr_base[t] is None:
            continue
        if final is not None:
            finals.append((t, ev["type"], ev["dir"], final))
        ratio = a / atr_base[t] if atr_base[t] > 0 else 1.0
        out["activity"][t] = ps.clamp(50.0 + 50.0 * math.tanh((ratio - 1.0) * ACT_K), 0.0, 100.0)
        if active or final is not None:
            out["reaction"][t] = ev["score"]
            out["speed"][t] = ev["speed"]
            out["validation"][t] = ev["val"]
            out["event_dir"][t] = float(ev["dir"])
    out["finals"] = finals
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Time-to-React + Volatility-Time",
    "",
    *provenance("time_to_react"),
    "",
    "Formula: the published Pine Script 'Time-to-React + Volatility-Time' (WavesUnchained), the",
    "1h preset tier in bars (reaction 8, validation 20, ATR move 1.6, weights 0.7/0.3 and",
    "0.40/0.60), swing lookback 10, OB displacement 1.0 ATR / search 12, priority BOS > SWEEP >",
    "OB. Conventions where Pine leaves them open: ATR Wilder average seeded with an SMA, first",
    "true range = high - low; an event needs an ATR; the reaction fields are reported while an",
    "event is active and on the bar it closes (Pine keeps plotting 0 after a no-reaction expiry);",
    "a bar reports once the ATR base exists; events closing before that are not listed.",
    "",
    "Series: pine_series.wave_series(900) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar (reaction",
    "fields only while reported); final_n the number of closed events and final_<j>_bar / _type",
    "(1 bos, 2 sweep, 3 ob) / _dir / _score the j-th closed event (up to 10).",
]

TYPE_CODE = {"bos": 1.0, "sweep": 2.0, "ob": 3.0}


def build():
    out = compute(ps.wave_series(N_BARS))
    first = next(t for t, r in enumerate(out["activity"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {"activity": out["activity"][cp]}
        if out["reaction"][cp] is not None:
            for k in ("reaction", "speed", "validation", "event_dir"):
                case[k] = out[k][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    fin = {"n": float(len(out["finals"]))}
    for j, (b, typ, d, s) in enumerate(out["finals"][:10]):
        fin[f"{j}_bar"] = float(b)
        fin[f"{j}_type"] = TYPE_CODE[typ]
        fin[f"{j}_dir"] = float(d)
        fin[f"{j}_score"] = s
    blocks.append(("final", "closed events", fin))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
