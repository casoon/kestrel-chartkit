"""golden_price_location: Wilder ATR and the price-location reading, from their definitions.

Written from the module documentation of `analytics::price_location`, not from the Rust code:
each quantity is computed over the whole series with plain list slicing instead of the
incremental queues the crate uses.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_price_location"

N_BARS = 330
CHECKPOINTS = [12, 13, 19, 49, 50, 120, 199, 219, 220, 250, 300, 329]

DEFAULT = {"atr_len": 14, "ema_len": 20, "range_len": 50, "sma_len": 200, "slope_bars": 20,
           "vol_window": 100, "vol_warmup": 200}
SMALL = {"atr_len": 5, "ema_len": 8, "range_len": 12, "sma_len": 30, "slope_bars": 4,
         "vol_window": 25, "vol_warmup": 40}

HEADER = [
    "kestrel-chartkit golden reference fixture: price location (analytics::price_location)",
    "",
    *provenance("price_location"),
    "",
    "Series: pine_series.wave_series(330), bars (open, high, low, close).",
    "ATR: true range max(H-L, |H-C1|, |L-C1|), H-L on the first bar; first value = mean of the",
    "first atr_len true ranges (on bar atr_len-1, zero-based), then (ATR1*(n-1)+TR)/n.",
    "EMA: alpha 2/(len+1), seeded with the first close. stretch = (close-EMA)/ATR from bar",
    "ema_len-1 on, when ATR > 0. range_position = (close-min low)/(max high-min low) over the last",
    "range_len bars, 0.5 when flat. sma = mean of the last sma_len closes; sma_prior = sma",
    "slope_bars bars earlier. vol_tercile = min(floor(3 * #{x < now}/count), 2) over the finite",
    "atr/close values of the last vol_window bars, from bar vol_warmup-1 on.",
    "<cfg>_cp<i>_<field> holds the value on bar i; a missing value is written as <field>_none=1.",
    "Configurations: default (14/20/50/200/20/100/200) and small (5/8/12/30/4/25/40).",
]


def _atr(bars, n):
    out = []
    value = None
    for i, (_, h, l, _c, _v) in enumerate(bars):
        if i == 0:
            tr = h - l
        else:
            pc = bars[i - 1][3]
            tr = max(h - l, abs(h - pc), abs(l - pc))
        if value is None:
            if i == n - 1:
                trs = []
                for k in range(n):
                    hk, lk = bars[k][1], bars[k][2]
                    if k == 0:
                        trs.append(hk - lk)
                    else:
                        pk = bars[k - 1][3]
                        trs.append(max(hk - lk, abs(hk - pk), abs(lk - pk)))
                total = 0.0
                for t in trs:
                    total += t
                value = total / n
        else:
            value = (value * (n - 1.0) + tr) / n
        out.append(value)
    return out


def _ema(closes, n):
    alpha = 2.0 / (n + 1.0)
    out = []
    e = None
    for c in closes:
        e = c if e is None else e + alpha * (c - e)
        out.append(e)
    return out


def _sma(closes, n, i):
    if i + 1 < n:
        return None
    total = 0.0
    # Same summation order as a running sum: add every close, subtract the ones that left.
    for k in range(i + 1):
        total += closes[k]
        if k >= n:
            total -= closes[k - n]
    return total / n


def compute(bars, cfg):
    closes = [b[3] for b in bars]
    atr = _atr(bars, cfg["atr_len"])
    ema = _ema(closes, cfg["ema_len"])
    out = []
    for i in range(len(bars)):
        c = closes[i]
        rec = {"atr": atr[i], "ema": ema[i]}
        a = atr[i]
        rec["stretch"] = (c - ema[i]) / a if (a is not None and a > 0.0
                                              and i + 1 >= cfg["ema_len"]) else None
        if i + 1 >= cfg["range_len"]:
            window = bars[i + 1 - cfg["range_len"]:i + 1]
            hi = max(b[1] for b in window)
            lo = min(b[2] for b in window)
            rec["range_position"] = (c - lo) / (hi - lo) if hi > lo else 0.5
        else:
            rec["range_position"] = None
        rec["sma"] = _sma(closes, cfg["sma_len"], i)
        j = i - cfg["slope_bars"]
        rec["sma_prior"] = _sma(closes, cfg["sma_len"], j) if j >= 0 else None
        rel_now = a / c if (a is not None and c > 0.0) else None
        if i + 1 >= cfg["vol_warmup"] and rel_now is not None:
            lo_i = max(0, i + 1 - cfg["vol_window"])
            window = []
            for k in range(lo_i, i + 1):
                ak = atr[k]
                if ak is not None and closes[k] > 0.0:
                    window.append(ak / closes[k])
            share = sum(1 for x in window if x < rel_now) / len(window)
            rec["vol_tercile"] = float(min(int(share * 3.0), 2))
        else:
            rec["vol_tercile"] = None
        out.append(rec)
    return out


FIELDS = ["atr", "ema", "stretch", "range_position", "sma", "sma_prior", "vol_tercile"]


def build():
    bars = ps.wave_series(N_BARS)
    blocks = [(None, "checkpoints", {"meta_cp_count": float(len(CHECKPOINTS)),
                                     **{f"meta_cp{k}": float(cp) for k, cp in enumerate(CHECKPOINTS)}})]
    for name, cfg in (("default", DEFAULT), ("small", SMALL)):
        res = compute(bars, cfg)
        case = {}
        for cp in CHECKPOINTS:
            for f in FIELDS:
                v = res[cp][f]
                if v is None:
                    case[f"cp{cp}_{f}_none"] = 1.0
                else:
                    case[f"cp{cp}_{f}"] = v
        blocks.append((name, f"configuration {name}", case))
    blocks.append((None, None, {"price_location_tolerance": 1e-12}))
    return HEADER, blocks
