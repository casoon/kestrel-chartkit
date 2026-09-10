"""golden_trend: trend indicators, each value from the definition documented on its type."""

import math

from ..fixture import provenance
from ..smoothing import ema_first_sample, rma

NAME = "golden_trend"

# The shared series: a straight line, open = close = p, high = p + 0.5, low = p - 0.5,
# volume 1000, p = 44 + 0.1 i for 25 bars.
LINE = [(44.0 + i * 0.1, 44.0 + i * 0.1 + 0.5, 44.0 + i * 0.1 - 0.5, 44.0 + i * 0.1, 1000.0)
        for i in range(25)]
# Where a straight line cannot tell a right implementation from a wrong one: a zigzag for the
# efficiency ratio, and a rise with a pullback so the MIDAS projection engages.
ZIGZAG = [10.0, 11.0, 10.5, 11.5, 11.0, 12.0, 11.2, 12.4]
PULLBACK = [(c, c + 0.5, c - 0.5, c, v) for c, v in zip(
    [100.0, 101.0, 102.0, 103.0, 104.0, 105.0, 104.0, 103.5, 103.0, 102.5],
    [1000.0, 1200.0, 900.0, 1500.0, 1100.0, 1300.0, 800.0, 1000.0, 1400.0, 900.0])]

HEADER = [
    "kestrel-chartkit golden reference fixture: trend indicators",
    "",
    *provenance("trend"),
    "",
    "Shared series: a straight line of 25 bars, open = close = p, high = p + 0.5, low = p - 0.5,",
    "volume 1000, p = 44 + 0.1 i. Each value from the definition on its type:",
    "  Alligator  Wilder averages (SMA-seeded) of (high + low)/2 over 13/8/5, shifted forward by",
    "             8/5/3 bars once their windows are full.",
    "  Efficiency |close_t - close_(t-len)| / sum |changes| over the last len + 1 closes, len 5.",
    "  Ichimoku   midpoints of the highest high and lowest low over 3 (tenkan), 5 (kijun) and",
    "             10 (senkou B) bars; senkou A = (tenkan + kijun)/2; spans unshifted.",
    "  MIDAS      cum(hlc3 * volume) / cum(volume) since the first bar, Topfinder, maturity 5.",
    "  Trend relationship  EMA(3) and EMA(5) of the close, first-sample-seeded from the first bar;",
    "             value fast - slow.",
    "  Z-score    (close - mean) / population standard deviation over the last 5 closes.",
    "",
    "On a straight line the efficiency ratio is 1 for any window and the MIDAS projection never",
    "engages, since every bar sets a new high: efficiency5_last and midas_proj only pin that. The",
    "*_shaped keys cover what the line cannot: the efficiency ratio over the zigzag closes",
    "[10, 11, 10.5, 11.5, 11, 12, 11.2, 12.4], and MIDAS over a rise with a pullback, closes",
    "[100, 101, 102, 103, 104, 105, 104, 103.5, 103, 102.5] with volumes",
    "[1000, 1200, 900, 1500, 1100, 1300, 800, 1000, 1400, 900], high/low = close +/- 0.5:",
    "projection = extreme - (extreme - curve at the extreme) * sqrt(cum volume at the extreme /",
    "cum volume now), from the first bar after the extreme on.",
    "",
    "trend_tolerance is the comparison tolerance the tests apply, stated rather than derived.",
]


def _midpoint(bars):
    return (max(b[1] for b in bars) + min(b[2] for b in bars)) / 2.0


def _alligator(bars):
    """Jaw/teeth/lips as published on the last bar: each Wilder average read as many outputs back
    as its offset, once all three averages exist; the offset only reaches full length after that
    many further outputs."""
    hl2 = [(b[1] + b[2]) / 2.0 for b in bars]
    lines = {name: rma(hl2, length) for name, length in (("jaw", 13), ("teeth", 8), ("lips", 5))}
    first = max(next(i for i, v in enumerate(series) if v is not None) for series in lines.values())
    last = len(bars) - 1
    offsets = {"jaw": 8, "teeth": 5, "lips": 3}
    return {name: lines[name][max(first, last - offsets[name])] for name in lines}


def _efficiency(closes, length):
    window = closes[-(length + 1):]
    change = abs(window[-1] - window[0])
    path = sum(abs(b - a) for a, b in zip(window, window[1:]))
    return change / path if path > 0 else 0.0


def _midas(bars, maturity):
    """Topfinder MIDAS curve and projection on the last bar. A new extreme is a high above the
    running one (the first bar sets it); the projection starts one bar after the latest extreme."""
    cum_pv = cum_v = 0.0
    extreme = extreme_v = extreme_curve = None
    since = 0
    curve = projection = None
    for _, high, low, close, volume in bars:
        price = (high + low + close) / 3.0
        cum_pv += price * volume
        cum_v += volume
        curve = cum_pv / cum_v
        if extreme is None or high > extreme:
            extreme, extreme_v, extreme_curve, since = high, cum_v, curve, 0
        else:
            since += 1
        projection = (extreme - (extreme - extreme_curve) * math.sqrt(extreme_v / cum_v)
                      if since > 0 else None)
    return curve, projection


def _zscore(closes, period):
    window = closes[-period:]
    mean = sum(window) / period
    sd = math.sqrt(sum((x - mean) ** 2 for x in window) / period)
    return (closes[-1] - mean) / sd if sd > 1e-8 else 0.0


def derive():
    closes = [b[3] for b in LINE]
    keys = {f"alligator_{name}": value for name, value in _alligator(LINE).items()}
    keys["efficiency5_last"] = _efficiency(closes, 5)
    tenkan, kijun = _midpoint(LINE[-3:]), _midpoint(LINE[-5:])
    keys["ichimoku_tenkan"], keys["ichimoku_kijun"] = tenkan, kijun
    keys["ichimoku_senkou_a"] = (tenkan + kijun) / 2.0
    keys["ichimoku_senkou_b"] = _midpoint(LINE[-10:])
    curve, projection = _midas(LINE, 5)
    keys["midas_curve"] = curve
    # The engine publishes the curve as its secondary value while no projection exists.
    keys["midas_proj"] = curve if projection is None else projection
    fast, slow = ema_first_sample(closes, 3)[-1], ema_first_sample(closes, 5)[-1]
    keys["trend_rel_fast"], keys["trend_rel_slow"], keys["trend_rel_diff"] = fast, slow, fast - slow
    keys["zscore5_last"] = _zscore(closes, 5)

    keys["efficiency5_shaped_last"] = _efficiency(ZIGZAG, 5)
    keys["midas5_shaped_curve"], keys["midas5_shaped_projection"] = _midas(PULLBACK, 5)
    return keys


SECTIONS = [
    ("straight-line series", ["alligator_jaw", "alligator_teeth", "alligator_lips"]),
    (None, ["efficiency5_last"]),
    (None, ["ichimoku_tenkan", "ichimoku_kijun", "ichimoku_senkou_a", "ichimoku_senkou_b"]),
    (None, ["midas_curve", "midas_proj"]),
    (None, ["trend_rel_fast", "trend_rel_slow", "trend_rel_diff"]),
    (None, ["zscore5_last"]),
    ("where the straight line cannot tell right from wrong",
     ["efficiency5_shaped_last", "midas5_shaped_curve", "midas5_shaped_projection"]),
    (None, ["trend_tolerance"]),
]


def build():
    values = {**derive(), "trend_tolerance": 1e-9}
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in SECTIONS]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
