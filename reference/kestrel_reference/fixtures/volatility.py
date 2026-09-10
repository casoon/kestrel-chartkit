"""golden_volatility — partly derived so far.

Derived here: ATR over flat bars and over bars with gaps (raw, percent, signal), the Chande Kroll
Stop (package 29), the true-range smoothing methods (42), the Ulcer Index (34) and the Relative
Volatility Index (35), each from the definition documented on its type. Not yet: the base
volatility indicators from the 0.2.0 starting point.
"""

import math

from ..exact import population_std
from ..smoothing import ema_published, published, rma, sma, wma

NAME = "golden_volatility"
PARTIAL = True
CONSTANTS = {
    "atr_tolerance": 1e-12,
    "cks_tolerance": 1e-9,
    "ulcer_tolerance": 1e-12,
    "rvi_vol_tolerance": 1e-12,
}

GAP_BARS = [(100.0, 102.0, 99.0, 101.0), (105.0, 107.0, 104.0, 106.0),
            (103.0, 104.0, 100.0, 101.0), (95.0, 96.0, 93.0, 94.0), (94.0, 99.0, 93.0, 98.0)]
CKS_BARS = [
    (99.5, 101.5, 98.5, 100.0), (100.5, 102.0, 100.0, 101.0), (102.5, 104.0, 102.0, 103.0),
    (101.5, 103.5, 101.0, 102.0), (103.5, 105.0, 102.5, 104.0), (105.5, 107.0, 105.0, 106.0),
    (104.5, 106.5, 104.0, 105.0), (106.5, 108.0, 106.0, 107.0), (108.5, 110.0, 107.5, 109.0),
    (107.5, 109.5, 107.0, 108.0), (109.5, 111.0, 109.0, 110.0), (111.5, 113.0, 111.0, 112.0),
    (110.5, 112.5, 109.5, 111.0), (112.5, 114.0, 112.0, 113.0), (114.5, 116.0, 114.0, 115.0),
    (113.5, 115.5, 113.0, 114.0), (111.5, 113.0, 110.5, 112.0), (109.5, 111.0, 109.0, 110.0),
    (110.5, 112.5, 110.0, 111.0), (112.5, 114.0, 112.0, 113.0),
]
METHOD_BARS = [(100.0, 102.0, 99.0, 101.0), (105.0, 107.0, 104.0, 106.0),
               (103.0, 104.0, 100.0, 101.0), (95.0, 96.0, 93.0, 94.0), (94.0, 99.0, 93.0, 98.0),
               (98.0, 101.0, 97.0, 100.0)]
ULCER_PRICES = [100.0, 102.0, 104.0, 101.0, 96.0, 92.0, 95.0, 99.0, 103.0, 106.0, 104.0, 105.0]


def _true_ranges(bars):
    """TR_1 = high - low; TR_t = max(high - low, |high - close_(t-1)|, |low - close_(t-1)|)."""
    out = []
    for index, (_, high, low, _) in enumerate(bars):
        if index == 0:
            out.append(high - low)
        else:
            previous = bars[index - 1][3]
            out.append(max(high - low, abs(high - previous), abs(low - previous)))
    return out


def _atr_with_signal(bars, atr_len, sig_len):
    """Wilder ATR, value in percent of the close, signal = Wilder average of that percentage;
    output once the signal exists. Returns (raw, percent, signal) per output."""
    atr = rma(_true_ranges(bars), atr_len)
    percent = [100.0 * a / bar[3] if a is not None else None for a, bar in zip(atr, bars)]
    first = next(i for i, p in enumerate(percent) if p is not None)
    signal = rma(percent[first:], sig_len)
    return [(atr[first + i], percent[first + i], s) for i, s in enumerate(signal) if s is not None]


def _chande_kroll(bars, atr_len, stop_len, mult):
    """preliminary_long = highest(high, atr_len) - mult ATR, preliminary_short = lowest(low,
    atr_len) + mult ATR (Wilder ATR); stop_long = highest(preliminary_long, stop_len),
    stop_short = lowest(preliminary_short, stop_len)."""
    atr = rma(_true_ranges(bars), atr_len)
    longs, shorts, out = [], [], []
    for index, a in enumerate(atr):
        if a is None:
            continue
        window = bars[index - atr_len + 1:index + 1]
        longs.append(max(b[1] for b in window) - mult * a)
        shorts.append(min(b[2] for b in window) + mult * a)
        if len(longs) >= stop_len:
            out.append((max(longs[-stop_len:]), min(shorts[-stop_len:])))
    return out


def _smoothed_true_range(bars, length, method):
    """The true-range average under each documented method; every method publishes from the
    length-th true range on."""
    tr = _true_ranges(bars)
    if method == "rma":
        return published(rma(tr, length))
    if method == "ema":
        return published(ema_published(tr, length))
    window_average = sma if method == "sma" else wma
    return [window_average(tr[i - length + 1:i + 1]) for i in range(length - 1, len(tr))]


def _ulcer(values, length):
    """Percentage below the running maximum of the last `length` values up to each point; the
    squares of the last `length` such percentages averaged and rooted; first after 2 len - 1."""
    drawdowns, out = [], []
    for index in range(length - 1, len(values)):
        peak = max(values[index - length + 1:index + 1])
        drawdowns.append(100.0 * (values[index] - peak) / peak)
        if len(drawdowns) >= length:
            out.append(math.sqrt(sum(d * d for d in drawdowns[-length:]) / length))
    return out


def _relative_volatility(prices, stdev_len, smooth_len):
    """Population standard deviation of the last stdev_len prices, filed under up (price rose),
    down (price fell) or neither; both Wilder-smoothed; 100 up / (up + down), 50 if both zero."""
    up, down = [], []
    for index in range(stdev_len - 1, len(prices)):
        sd = population_std(prices[index - stdev_len + 1:index + 1])
        move = prices[index] - prices[index - 1]
        up.append(sd if move > 0 else 0.0)
        down.append(sd if move < 0 else 0.0)
    out = []
    for u, d in zip(rma(up, smooth_len), rma(down, smooth_len)):
        if u is not None:
            out.append(100.0 * u / (u + d) if u + d else 50.0)
    return out


def derive():
    keys = {}
    flat = [(100.0, 101.0, 99.0, 100.0)] * 5
    keys["atr3_signal2_pct"] = _atr_with_signal(flat, 3, 2)[-1][1]
    for (raw, percent, signal), label in zip(_atr_with_signal(GAP_BARS, 3, 2), ["first", "second"]):
        keys[f"atr_gap3_signal2_raw_{label}"] = raw
        keys[f"atr_gap3_signal2_pct_{label}"] = percent
        keys[f"atr_gap3_signal2_signal_{label}"] = signal

    stops = _chande_kroll(CKS_BARS, 4, 3, 2.0)
    keys["cks4_3_mult2_output_count"] = float(len(stops))
    keys["cks4_3_mult2_long_first"], keys["cks4_3_mult2_short_first"] = stops[0]
    keys["cks4_3_mult2_long_last"], keys["cks4_3_mult2_short_last"] = stops[-1]
    keys["cks4_1_mult2_long_last"], keys["cks4_1_mult2_short_last"] = _chande_kroll(CKS_BARS, 4, 1, 2.0)[-1]

    for method in ("rma", "sma", "ema", "wma"):
        values = _smoothed_true_range(METHOD_BARS, 3, method)
        keys[f"atr3_{method}_raw_third"] = values[0]
        keys[f"atr3_{method}_raw_last"] = values[-1]
        keys["atr3_method_output_count"] = float(len(values))

    ulcer = _ulcer(ULCER_PRICES, 3)
    keys["ulcer3_output_count"] = float(len(ulcer))
    keys["ulcer3_first"], keys["ulcer3_second"], keys["ulcer3_last"] = ulcer[0], ulcer[1], ulcer[-1]

    closes = [100.0 + 6.0 * math.sin(i * 0.4) + 0.5 * (-1.0) ** i for i in range(40)]
    rvi = _relative_volatility(closes, 5, 4)
    keys["rvi_vol_close_output_count"] = float(len(rvi))
    keys["rvi_vol_close_first"], keys["rvi_vol_close_last"] = rvi[0], rvi[-1]
    # High/low variant: the mean of the index over the highs and over the lows.
    highs = _relative_volatility([c + 1.0 for c in closes], 5, 4)
    lows = _relative_volatility([c - 1.0 for c in closes], 5, 4)
    keys["rvi_vol_high_low_last"] = (highs[-1] + lows[-1]) / 2.0
    return keys
