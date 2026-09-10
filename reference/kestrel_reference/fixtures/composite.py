"""golden_composite: composite scores, each from the combination formula documented on its type,
applied to sub-indicator definitions derived in the other indicator fixtures."""

from .. import exact
from ..fixture import provenance
from ..indicators import adx, efficiency_ratio, keltner, rsi_lines
from ..smoothing import ema_first_sample

NAME = "golden_composite"
PERIOD = 5

# (open, high, low, close, volume)
LINE = [(44.0 + i * 0.1, 44.0 + i * 0.1 + 0.5, 44.0 + i * 0.1 - 0.5, 44.0 + i * 0.1, 1000.0)
        for i in range(30)]
HAMMER = [(100.0, 100.0, 90.0, 100.0, 1000.0)] * 10
EXPANSION = [(100.0, 101.0, 99.0, 100.0, 1000.0)] * 10 + [
    (p - 1.0, p + 1.0, p - 1.0, p, 10000.0) for p in (110.0, 125.0, 145.0, 170.0, 200.0)]
OFFSETS = [0.0, 0.6, -0.3]


def _falling():
    bars, previous = [], 60.0
    for i in range(30):
        close = 60.0 - 0.3 * i + OFFSETS[i % 3]
        high = max(previous, close) + 0.05 + 0.05 * (i % 3)
        low = min(previous, close) - 0.05 - 0.05 * (i % 2)
        bars.append((previous, high, low, close, 1000.0 + 250.0 * (i % 5)))
        previous = close
    return bars


FALLING = _falling()

HEADER = [
    "kestrel-chartkit golden reference fixture: composite scores",
    "",
    *provenance("composite"),
    "",
    "Each score from the combination formula documented on its type, all inputs over period 5:",
    "  Trend quality   direction (+1 if the first-sample EMA of the close rose since the previous",
    "                  bar, else -1) * efficiency ratio * clamp(ADX / 50, 0, 1)",
    "                  * clamp(RVOL / 2, 0.2, 1) * 100, clamped to -100..100.",
    "  Buy/sell pressure  first-sample EMA of (0.6 location + 0.4 wick balance) * 100, with",
    "                  location = 2 (close - low) / range - 1 and wick balance = (lower wick -",
    "                  upper wick) / range; location and wick balance of the last bar unsmoothed.",
    "  Volatility regime  Bollinger(5, 2, population) against Keltner(EMA 5, mean TR over 10,",
    "                  1.5): -1 squeeze (bands inside the channel), else 1 if the Bollinger width",
    "                  exceeds 1.3 channel widths, else 0.",
    "  Multi-factor    0.35 trend/100 + 0.25 (RSI line - 50)/50 + 0.40 pressure/100, halved in a",
    "                  squeeze, clamped to -1..1; the RSI line is the EMA(3) over Wilder's RSI(5).",
    "The sub-indicators follow the definitions derived in golden_trend, golden_volatility,",
    "golden_oscillators and golden_volume.",
    "",
    "Series (open, high, low, close, volume):",
    "  line       30 bars, open = close = p, high/low = p +/- 0.5, volume 1000, p = 44 + 0.1 i.",
    "  hammer     (100, 100, 90, 100, 1000), 10 times.",
    "  expansion  (100, 101, 99, 100, 1000) 10 times, then (p - 1, p + 1, p - 1, p, 10000) for",
    "             p = 110, 125, 145, 170, 200.",
    "  falling    30 bars, close = 60 - 0.3 i + [0, 0.6, -0.3][i mod 3], open = previous close",
    "             (60 at first), high = max(open, close) + 0.05 + 0.05 (i mod 3),",
    "             low = min(open, close) - 0.05 - 0.05 (i mod 2), volume 1000 + 250 (i mod 5).",
    "",
    "On the line every trend factor sits at a bound (ER 1, ADX 100, RSI 100), the pressure is 0 and",
    "the regime is a squeeze; the falling series with rebounds moves each factor off its bound and",
    "leaves the regime normal, so the multi-factor score is not halved there.",
    "",
    "composite_tolerance is the comparison tolerance the tests apply, stated rather than derived.",
]


def _trend_quality(bars):
    closes = [b[3] for b in bars]
    ema = ema_first_sample(closes, PERIOD)
    direction = 1.0 if ema[-1] > ema[-2] else -1.0
    efficiency = efficiency_ratio(closes, PERIOD)
    strength = min(max(adx([b[:4] for b in bars], PERIOD, PERIOD)[-1] / 50.0, 0.0), 1.0)
    volumes = [b[4] for b in bars[-PERIOD:]]
    average = sum(volumes) / PERIOD
    rvol = bars[-1][4] / average if average > 0.0 else 1.0
    participation = min(max(rvol / 2.0, 0.2), 1.0)
    score = min(max(direction * efficiency * strength * participation * 100.0, -100.0), 100.0)
    return direction, efficiency, strength, participation, score


def _bar_pressure(bar):
    o, high, low, close, _ = bar
    span = max(high - low, 1e-8)
    location = (2.0 * (close - low) / span) - 1.0
    upper = high - min(high, max(o, close))
    lower = max(low, min(o, close)) - low
    wick_balance = (lower - upper) / span
    return location, wick_balance, (location * 0.6 + wick_balance * 0.4) * 100.0


def _pressure(bars):
    parts = [_bar_pressure(b) for b in bars]
    smoothed = ema_first_sample([raw for _, _, raw in parts], PERIOD)[-1]
    location, wick_balance, _ = parts[-1]
    return min(max(smoothed, -100.0), 100.0), location, wick_balance


def _regime(bars):
    bands = exact.bollinger([b[3] for b in bars[-PERIOD:]], 2.0)
    bb_upper, bb_lower = float(bands["upper"]), float(bands["lower"])
    _, kc_upper, kc_lower = keltner([b[:4] for b in bars], PERIOD, 10, 1.5)
    bb_width, kc_width = bb_upper - bb_lower, max(kc_upper - kc_lower, 1e-8)
    if bb_upper <= kc_upper and bb_lower >= kc_lower:
        state = -1.0
    elif bb_width > kc_width * 1.3:
        state = 1.0
    else:
        state = 0.0
    return state, bb_width, kc_width


def _multi_factor(bars):
    trend = _trend_quality(bars)[-1] / 100.0
    rsi = (rsi_lines([b[3] for b in bars], PERIOD, 3, 3)[0][-1] - 50.0) / 50.0
    pressure = _pressure(bars)[0] / 100.0
    state = _regime(bars)[0]
    raw = trend * 0.35 + rsi * 0.25 + pressure * 0.40
    final = min(max(raw * 0.5 if state < 0.0 else raw, -1.0), 1.0)
    return trend, rsi, pressure, state, final


def _scores(bars, tag):
    keys = {}
    names = ("direction", "efficiency", "strength", "participation", "score")
    keys.update({f"trend_quality5{tag}_{n}": v for n, v in zip(names, _trend_quality(bars))})
    value, location, wick_balance = _pressure(bars)
    keys[f"buy_sell_pressure5{tag}" if tag else "buy_sell_pressure5_neutral"] = value
    prefix = f"buy_sell_pressure5{tag}" if tag else "buy_sell_pressure5_neutral"
    keys[f"{prefix}_location"], keys[f"{prefix}_wick_balance"] = location, wick_balance
    state, bb_width, kc_width = _regime(bars)
    regime = f"volatility_regime5{tag}" if tag else "volatility_regime5_squeeze"
    keys[f"{regime}_state"] = state
    keys[f"{regime}_bb_width"], keys[f"{regime}_kc_width"] = bb_width, kc_width
    names = ("trend_factor", "rsi_factor", "pressure_factor", "vol_state", "final_score")
    keys.update({f"multi_factor5{tag}_{n}": v for n, v in zip(names, _multi_factor(bars))})
    return keys


def derive():
    keys = _scores(LINE, "")
    value, location, wick_balance = _pressure(HAMMER)
    keys["buy_sell_pressure_bullish_hammer"] = value
    keys["buy_sell_pressure_hammer_location"] = location
    keys["buy_sell_pressure_hammer_wick_balance"] = wick_balance
    state, bb_width, kc_width = _regime(EXPANSION)
    keys["volatility_regime5_expansion_state"] = state
    keys["volatility_regime5_expansion_bb_width"] = bb_width
    keys["volatility_regime5_expansion_kc_width"] = kc_width
    keys.update(_scores(FALLING, "_shaped"))
    return keys


def _group(values, stem):
    return [k for k in values if k.startswith(stem)]


def build():
    values = {**derive(), "composite_tolerance": 1e-9}
    line = [k for k in values if "_shaped" not in k and "hammer" not in k
            and "expansion" not in k and k != "composite_tolerance"]
    sections = [
        ("line", [k for k in line if k.startswith("trend_quality")]),
        (None, [k for k in line if k.startswith("buy_sell")]),
        (None, [k for k in line if k.startswith("volatility")]),
        (None, [k for k in line if k.startswith("multi_factor")]),
        ("hammer", _group(values, "buy_sell_pressure_")),
        ("expansion", _group(values, "volatility_regime5_expansion")),
        ("falling series with rebounds", _group(values, "trend_quality5_shaped")),
        (None, _group(values, "buy_sell_pressure5_shaped")),
        (None, _group(values, "volatility_regime5_shaped")),
        (None, _group(values, "multi_factor5_shaped")),
        (None, ["composite_tolerance"]),
    ]
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in sections]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
