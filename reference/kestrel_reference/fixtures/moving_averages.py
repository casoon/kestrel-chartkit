"""golden_moving_averages: the moving-average family, each value from the definition documented
on its type."""

import math

from ..fixture import provenance
from ..smoothing import (
    ema_first_sample,
    ema_published,
    ema_sma_seed,
    least_squares,
    published,
    sma,
    wma,
)

NAME = "golden_moving_averages"

HEADER = [
    "kestrel-chartkit golden reference fixture: moving-average family",
    "",
    *provenance("moving_averages"),
    "",
    "Base values over the 10-bar close series [10, 11, 12, 11, 13, 14, 13, 15, 16, 15], period 5,",
    "volume 1000 on every bar (so VWMA equals SMA here), each from the definition on its type:",
    "  SMA       mean of the last 5 closes.",
    "  EMA       alpha = 2/(period+1), seeded with the first close, published from the 5th bar on.",
    "  WMA       linearly weighted, weight 5 on the most recent close.",
    "  VWMA      sum(close * volume) / sum(volume) over the last 5 bars.",
    "  HMA       WMA over round(sqrt 5) = 2 of 2 WMA(floor(5/2) = 2) - WMA(5).",
    "  DEMA      2 e1 - e2, e2 running over the published values of e1.",
    "  TEMA      3 e1 - 3 e2 + e3, all three stages running from the first close.",
    "  KAMA      efficiency ratio over the last 6 closes between EMA constants for 2 and 30,",
    "            squared; the first value is the close of the first full window.",
    "  LSMA      least-squares line over the last 5 closes, evaluated at the newest bar.",
    "  McGinley  md += (close - md) / (5 (close / md)^4), starting at the first close.",
    "ma_tolerance is the comparison tolerance the tests apply, stated rather than derived.",
    "",
    "EMA seed contract (package 19): the same recursion alpha = 2/(period+1) started from two",
    "different first values over the 12-bar series",
    "[22, 22.5, 23.25, 22.75, 23.5, 24, 23, 22.25, 23.75, 24.5, 25, 24.25], period = 5:",
    "first_sample seeded with the first close and published from the 5th bar on; sma seeded with",
    "the mean of the first 5 closes and undefined before that.",
    "",
    "LSMA regression outputs (package 23): ordinary least squares y = intercept + slope * x over",
    "x = 0..period-1, in exact rational arithmetic, for the windows",
    "[100, 102.5, 105, 107.5, 110] (linear) and [10, 12, 11, 14, 13] (noisy).",
    "",
    "VIDYA, CMO variant (package 27), cmo_len = 4, ema_len = 5, over",
    "[100, 101, 100.5, 102, 101, 103, 104, 103.5, 105, 104, 106, 105.5, 107, 106.5, 108]:",
    "alpha = 2/(ema_len+1) * |CMO(cmo_len)|/100 on the ordinary exponential recursion; the first",
    "value is the close of the bar the CMO first becomes defined for.",
    "",
    "Tillson T3 (package 28), period = 3, over",
    "[100, 101, 102, 101.5, 103, 104, 103.5, 105, 106, 105.5, 107, 108, 107.5, 109, 110]: six",
    "chained first-sample-seeded EMAs combined as c1 e6 + c2 e5 + c3 e4 + c4 e3 with c1 = -v^3,",
    "c2 = 3v^2 + 3v^3, c3 = -6v^2 - 3v - 3v^3, c4 = 1 + 3v + v^3 + 3v^2; published from the",
    "period-th bar on; v = 0.7, 0 and 1.",
]

CLOSES = [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0]
EMA_SEED_CLOSES = [22.0, 22.5, 23.25, 22.75, 23.5, 24.0, 23.0, 22.25, 23.75, 24.5, 25.0, 24.25]
LSMA_WINDOWS = {"linear": [100.0, 102.5, 105.0, 107.5, 110.0], "noisy": [10.0, 12.0, 11.0, 14.0, 13.0]}
VIDYA_CLOSES = [100.0, 101.0, 100.5, 102.0, 101.0, 103.0, 104.0, 103.5, 105.0, 104.0, 106.0,
                105.5, 107.0, 106.5, 108.0]
T3_CLOSES = [100.0, 101.0, 102.0, 101.5, 103.0, 104.0, 103.5, 105.0, 106.0, 105.5, 107.0, 108.0,
             107.5, 109.0, 110.0]


def _cmo(changes):
    """Chande Momentum Oscillator over a window of price changes: 100 (up - down) / (up + down)."""
    up = sum(c for c in changes if c > 0)
    down = -sum(c for c in changes if c < 0)
    return 100.0 * (up - down) / (up + down) if up + down else 0.0


def _vidya(closes, cmo_len, ema_len):
    """VIDYA: the first value is the close of the bar the CMO first becomes defined for (after
    cmo_len changes); from then on an exponential step with alpha = 2/(ema_len+1) * |CMO|/100.
    Returns the published values and the last bar's CMO and alpha."""
    values, cmo, alpha = [], None, None
    for index in range(cmo_len, len(closes)):
        changes = [closes[i] - closes[i - 1] for i in range(index - cmo_len + 1, index + 1)]
        cmo = _cmo(changes)
        if not values:
            values.append(closes[index])
            continue
        alpha = 2.0 / (ema_len + 1) * abs(cmo) / 100.0
        values.append(values[-1] + alpha * (closes[index] - values[-1]))
    return values, cmo, alpha


def _t3(closes, period, v):
    """Tillson T3: six chained first-sample-seeded EMAs of `period`, combined as
    c1 e6 + c2 e5 + c3 e4 + c4 e3 with c1 = -v^3, c2 = 3v^2 + 3v^3, c3 = -6v^2 - 3v - 3v^3,
    c4 = 1 + 3v + v^3 + 3v^2; published from the period-th bar on."""
    stages = [closes]
    for _ in range(6):
        stages.append(ema_first_sample(stages[-1], period))
    e3, e4, e5, e6 = stages[3], stages[4], stages[5], stages[6]
    c1 = -v ** 3
    c2 = 3 * v ** 2 + 3 * v ** 3
    c3 = -6 * v ** 2 - 3 * v - 3 * v ** 3
    c4 = 1 + 3 * v + v ** 3 + 3 * v ** 2
    combined = [c1 * a + c2 * b + c3 * c + c4 * d for a, b, c, d in zip(e6, e5, e4, e3)]
    return combined[period - 1:]


def _window_series(values, length, reduce):
    """`reduce` over every full trailing window, from the length-th value on."""
    return [reduce(values[i - length + 1:i + 1]) for i in range(length - 1, len(values))]


def _vwma(closes, volumes, length):
    """sum(close * volume) / sum(volume) over the last `length` bars; the close itself when the
    window carries no volume."""
    c, v = closes[-length:], volumes[-length:]
    total = sum(v)
    return sum(a * b for a, b in zip(c, v)) / total if total > 0 else c[-1]


def _hma(closes, length):
    """Hull: WMA over round(sqrt(length)) of 2 WMA(floor(length/2)) - WMA(length), the inner
    difference formed once both inner averages exist. Rounding is half away from zero."""
    half = max(length // 2, 1)
    root = max(math.floor(math.sqrt(length) + 0.5), 1)
    inner_half = _window_series(closes, half, wma)
    inner_full = _window_series(closes, length, wma)
    offset = length - half
    diff = [2.0 * inner_half[offset + k] - full for k, full in enumerate(inner_full)]
    return _window_series(diff, root, wma)


def _ema_engine(values, length):
    """The EMA indicator: first-sample recursion from the first value, published from the
    length-th value on."""
    return published(ema_published(values, length))


def _dema(closes, length):
    """2 e1 - e2, where e1 is the published EMA of the closes and e2 the published EMA of e1's
    published values: the second stage starts with the first published value of the first."""
    e1 = _ema_engine(closes, length)
    e2 = _ema_engine(e1, length)
    return [2.0 * e1[length - 1 + k] - b for k, b in enumerate(e2)]


def _tema(closes, length):
    """3 e1 - 3 e2 + e3 over three first-sample-seeded EMAs that all run from the first close;
    published from the length-th bar on."""
    e1 = ema_first_sample(closes, length)
    e2 = ema_first_sample(e1, length)
    e3 = ema_first_sample(e2, length)
    return [3.0 * a - 3.0 * b + c for a, b, c in zip(e1, e2, e3)][length - 1:]


def _kama(closes, length, fast, slow):
    """Kaufman: over the last length+1 closes, ER = |last - first| / sum |changes| (0 without
    movement); sc = (ER (2/(fast+1) - 2/(slow+1)) + 2/(slow+1))^2; the first value is the close,
    then kama += sc (close - kama)."""
    fast_sc, slow_sc = 2.0 / (fast + 1), 2.0 / (slow + 1)
    level, out = None, []
    for i in range(length, len(closes)):
        window = closes[i - length:i + 1]
        change = abs(window[-1] - window[0])
        path = sum(abs(b - a) for a, b in zip(window, window[1:]))
        er = change / path if path > 0 else 0.0
        sc = (er * (fast_sc - slow_sc) + slow_sc) ** 2
        level = closes[i] if level is None else level + sc * (closes[i] - level)
        out.append(level)
    return out


def _mcginley(closes, length):
    """McGinley Dynamic in its original form: md += (price - md) / (length (price / md)^4), the
    first value being the first price."""
    level, out = None, []
    for price in closes:
        level = price if level is None else level + (price - level) / (length * (price / level) ** 4)
        out.append(level)
    return out


def derive():
    keys = {
        "sma5_last": sma(CLOSES[-5:]),
        "ema5_last": ema_first_sample(CLOSES, 5)[-1],
        "wma5_last": wma(CLOSES[-5:]),
        "lsma5_last": float(least_squares(CLOSES[-5:])[3]),
        "vwma5_last": _vwma(CLOSES, [1000.0] * len(CLOSES), 5),
        "hma5_last": _hma(CLOSES, 5)[-1],
        "dema5_last": _dema(CLOSES, 5)[-1],
        "kama5_last": _kama(CLOSES, 5, 2, 30)[-1],
        "tema5_last": _tema(CLOSES, 5)[-1],
        "mcginley5_last": _mcginley(CLOSES, 5)[-1],
    }

    sma_seeded = [x for x in ema_sma_seed(EMA_SEED_CLOSES, 5) if x is not None]
    keys["ema5_seed_sma_first_output"] = sma_seeded[0]
    keys["ema5_seed_sma_last"] = sma_seeded[-1]
    keys["ema5_seed_first_sample_last"] = ema_first_sample(EMA_SEED_CLOSES, 5)[-1]

    for label, window in LSMA_WINDOWS.items():
        slope, intercept, r2, value = least_squares(window)
        keys[f"lsma5_{label}_slope"] = float(slope)
        keys[f"lsma5_{label}_intercept"] = float(intercept)
        keys[f"lsma5_{label}_r2"] = float(r2)
        keys[f"lsma5_{label}_value"] = float(value)

    vidya, cmo, alpha = _vidya(VIDYA_CLOSES, 4, 5)
    keys["vidya4_5_output_count"] = float(len(vidya))
    keys["vidya4_5_seed"] = vidya[0]
    keys["vidya4_5_second"] = vidya[1]
    keys["vidya4_5_last"] = vidya[-1]
    keys["vidya4_5_last_cmo"] = cmo
    keys["vidya4_5_last_alpha"] = alpha

    t3 = _t3(T3_CLOSES, 3, 0.7)
    keys["t3_3_v07_output_count"] = float(len(t3))
    keys["t3_3_v07_first"] = t3[0]
    keys["t3_3_v07_last"] = t3[-1]
    keys["t3_3_v00_last"] = _t3(T3_CLOSES, 3, 0.0)[-1]
    keys["t3_3_v10_last"] = _t3(T3_CLOSES, 3, 1.0)[-1]
    return keys


SECTIONS = [
    (None, "base values, period 5, over the 10-bar series",
     ["sma5_last", "ema5_last", "wma5_last", "vwma5_last", "hma5_last", "dema5_last", "kama5_last",
      "tema5_last", "lsma5_last", "mcginley5_last"]),
    ("ema5_seed", "EMA seed contract (package 19)",
     ["sma_first_output", "sma_last", "first_sample_last"]),
    ("lsma5", "LSMA regression outputs (package 23)",
     [f"{window}_{field}" for window in ("linear", "noisy")
      for field in ("slope", "intercept", "r2", "value")]),
    ("vidya4_5", "VIDYA, CMO variant (package 27)",
     ["output_count", "seed", "second", "last", "last_cmo", "last_alpha"]),
    ("t3_3", "Tillson T3 (package 28)",
     ["v07_output_count", "v07_first", "v07_last", "v00_last", "v10_last"]),
]


def build():
    values = derive()
    blocks = []
    for prefix, comment, keys in SECTIONS:
        case = {key: values[key if prefix is None else f"{prefix}_{key}"] for key in keys}
        if prefix is None:
            case["ma_tolerance"] = 1e-9
        blocks.append((prefix, comment, case))
    assert sum(len(case) for _, _, case in blocks) == len(values) + 1, "every value is placed"
    return HEADER, blocks
