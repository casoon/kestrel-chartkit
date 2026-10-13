"""golden_sweep_reversal_score: Sweep Reversal Score, chart-timeframe configuration.

Written from the original definition, not from the Rust code, with the inputs
only15m = false and gate F off (the other gates at their defaults: A and D on; B, C, E, G,
H, I off). Series are full arrays indexed by bar.
"""

import math

from ..fixture import provenance

NAME = "golden_sweep_reversal_score"

SWING_LEN = 10
ATR_LEN = 14
ATR_BASE_LEN = 50
ATR_LOW_FACTOR = 0.60
RSI_LEN = 14
PIV_LEN = 5
MIN_Q_LONG = 2
MIN_Q_SHORT = 1
COOLDOWN = 5
SL_ATR_BUFFER = 0.30
TP_R_MULT = 1.50

N_BARS = 900
CHECKPOINTS = [100, 333, 517, 750, 899]


def _series():
    """Waves with sharp spikes every 23 bars that pierce recent extremes and close back."""
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.08 if i < 300 else (-0.10 if i < 600 else 0.03)
        close = prev + drift + 0.9 * math.sin(x / 5.0) + 0.4 * math.cos(x / 2.2)
        open_ = prev + 0.1 * math.sin(x / 3.3)
        high = max(open_, close) + 0.3 + 0.2 * abs(math.sin(x / 7.0))
        low = min(open_, close) - 0.3 - 0.2 * abs(math.cos(x / 4.0))
        if i % 23 == 11:
            low -= 3.0
        if i % 29 == 17:
            high += 3.0
        bars.append((open_, high, low, close))
        prev = close
    return bars


def _rma(src, n):
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


def _sma(src, n):
    out = [None] * len(src)
    for t in range(n - 1, len(src)):
        w = src[t - n + 1:t + 1]
        if None not in w:
            out[t] = sum(w) / n
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
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, ATR_LEN)
    atr_base = _sma(atr, ATR_BASE_LEN)
    up = [None] + [max(c[t] - c[t - 1], 0.0) for t in range(1, n)]
    dn = [None] + [max(c[t - 1] - c[t], 0.0) for t in range(1, n)]
    au, ad = _rma(up, RSI_LEN), _rma(dn, RSI_LEN)
    rsi = [None] * n
    for t in range(n):
        if au[t] is not None:
            rsi[t] = 50.0 if au[t] == 0 and ad[t] == 0 else (
                100.0 if ad[t] == 0 else 100.0 - 100.0 / (1.0 + au[t] / ad[t]))

    sh1 = sh2 = sl1 = sl2 = None
    last_signal = None
    out = {"reported": [False] * n, "long_q": [None] * n, "short_q": [None] * n,
           "swing_low": [None] * n, "swing_high": [None] * n, "longs": [], "shorts": []}
    for t in range(n):
        ph = _pivot(h, t, PIV_LEN, True)
        pl = _pivot(lo, t, PIV_LEN, False)
        if ph is not None:
            sh2, sh1 = sh1, ph
        if pl is not None:
            sl2, sl1 = sl1, pl
        if atr_base[t] is None or rsi[t] is None or t < SWING_LEN:
            continue
        prev_low = min(lo[t - SWING_LEN:t])
        prev_high = max(h[t - SWING_LEN:t])
        sweep_long = lo[t] < prev_low and c[t] > prev_low
        sweep_short = h[t] > prev_high and c[t] < prev_high
        atr_ok = atr[t] > atr_base[t] * ATR_LOW_FACTOR
        ll = sl1 is not None and sl2 is not None and sl1 < sl2
        hh = sh1 is not None and sh2 is not None and sh1 > sh2
        long_q = (1 if rsi[t] > 50 else 0) + (1 if not ll else 0)
        short_q = (1 if rsi[t] > 50 else 0) + (1 if not hh else 0)
        cooldown_ok = last_signal is None or t - last_signal > COOLDOWN
        long_sig = sweep_long and atr_ok and long_q >= MIN_Q_LONG and cooldown_ok
        short_sig = sweep_short and atr_ok and short_q >= MIN_Q_SHORT and cooldown_ok
        if long_sig or short_sig:
            last_signal = t
        if long_sig:
            sl = min(lo[t], prev_low) - atr[t] * SL_ATR_BUFFER
            out["longs"].append((t, float(long_q), c[t], sl, c[t] + (c[t] - sl) * TP_R_MULT))
        if short_sig:
            sl = max(h[t], prev_high) + atr[t] * SL_ATR_BUFFER
            out["shorts"].append((t, float(short_q), c[t], sl, c[t] - (sl - c[t]) * TP_R_MULT))
        out["reported"][t] = True
        out["long_q"][t] = float(long_q)
        out["short_q"][t] = float(short_q)
        out["swing_low"][t] = prev_low
        out["swing_high"][t] = prev_high
    out["rsi"], out["atr"], out["atr_base"] = rsi, atr, atr_base
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Sweep Reversal Score (chart timeframe)",
    "",
    *provenance("sweep_reversal_score"),
    "",
    "Formula: the original definition with only15m",
    "off and gate F off, everything else at its defaults (gate A sweep lookback 10, gate D ATR 14",
    "above 0.6 x SMA 50 of the ATR, RSI 14, pivots 5, minimum quality 2 long / 1 short, cooldown 5,",
    "stop buffer 0.3 ATR, target 1.5 R). Conventions the original leaves open: ATR = Wilder",
    "average of the true range (first bar high - low) seeded with an SMA, RSI = Wilder averages of",
    "the close changes seeded with an SMA, pivots strict on both sides; output from the first bar",
    "with ATR base, RSI and a full sweep window.",
    "",
    "Series: 900 bars, prev = previous close (100 before the first bar), drift 0.08 up to bar 299,",
    "-0.10 to 599, 0.03 after; close = prev + drift + 0.9 sin(i/5) + 0.4 cos(i/2.2); open = prev +",
    "0.1 sin(i/3.3); high = max(open, close) + 0.3 + 0.2 |sin(i/7)|, plus 3 when i mod 29 = 17;",
    "low = min(open, close) - 0.3 - 0.2 |cos(i/4)|, minus 3 when i mod 23 = 11.",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; <long|short>_n",
    "the number of signals, <long|short><j>_bar the bar of each and <long|short><j>_quality/_entry/",
    "_stop/_take those of the first 8.",
]


def build():
    out = compute(_series())
    first = next(t for t in range(N_BARS) if out["reported"][t])
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {"long_quality": out["long_q"][cp], "short_quality": out["short_q"][cp],
                "rsi": out["rsi"][cp], "atr": out["atr"][cp], "atr_base": out["atr_base"][cp],
                "swing_low": out["swing_low"][cp], "swing_high": out["swing_high"][cp]}
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    sig = {}
    for kind in ("long", "short"):
        rows = out[kind + "s"]
        sig[f"{kind}_n"] = float(len(rows))
        for j, (t, q, e, s, k) in enumerate(rows[:8]):
            sig.update({f"{kind}{j}_quality": q, f"{kind}{j}_entry": e, f"{kind}{j}_stop": s,
                        f"{kind}{j}_take": k})
        for j, row in enumerate(rows):
            sig[f"{kind}{j}_bar"] = float(row[0])
    blocks.append(("sig", "signals", sig))
    blocks.append((None, None, {"res_tolerance": 1e-9}))
    return HEADER, blocks
