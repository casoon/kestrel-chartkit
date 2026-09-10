"""golden_volatility: volatility indicators, each value from the definition documented on its
type."""

import math

from ..exact import population_std
from ..fixture import provenance
from ..smoothing import ema_first_sample, ema_published, published, rma, sma, wma
from ..indicators import adx as _adx, directional as _directional, keltner as _keltner, true_ranges as _true_ranges

NAME = "golden_volatility"
TOLERANCES = {
    "adx_tolerance": 1e-12,
    "vol_tolerance": 1e-9,
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
# The shared series: open = close = p, high = p + 0.5, low = p - 0.5.
VOL_PRICES = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
              45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]
VOL_BARS = [(p, p + 0.5, p - 0.5, p) for p in VOL_PRICES]
# Where the shared inputs cannot tell a right implementation from a wrong one (a strictly rising
# series for the ADX, a constant range for the Mass Index): moves in both directions with varying
# ranges.
SHAPED_CLOSES = [100.0, 102.0, 101.0, 104.0, 103.0, 106.0, 105.0, 103.0, 104.0, 101.0, 102.0, 99.0,
                 100.0, 98.0]
SHAPED = [(c, c + 1.0 + 0.3 * (i % 3), c - 1.0 - 0.2 * (i % 4), c)
          for i, c in enumerate(SHAPED_CLOSES)]
ULCER_PRICES = [100.0, 102.0, 104.0, 101.0, 96.0, 92.0, 95.0, 99.0, 103.0, 106.0, 104.0, 105.0]


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


def _aroon(bars, period):
    """Over the last period + 1 bars: the most recent highest high and lowest low (ties go to the
    later bar); up/down = (period - bars since it) / period * 100; oscillator up - down."""
    window = bars[-(period + 1):]
    high_idx = max(i for i, b in enumerate(window) if b[1] == max(x[1] for x in window))
    low_idx = max(i for i, b in enumerate(window) if b[2] == min(x[2] for x in window))
    up, down = high_idx / period * 100.0, low_idx / period * 100.0
    return up - down, up, down


def _dmi(bars, period):
    """Plain sums of TR, +DM and -DM over the last `period` changes; DI = 100 sum DM / sum TR;
    value +DI - -DI."""
    plus, minus, tr = _directional(bars)
    total = sum(tr[-period:])
    di_p = 100.0 * sum(plus[-period:]) / total if total > 0.0 else 0.0
    di_m = 100.0 * sum(minus[-period:]) / total if total > 0.0 else 0.0
    return di_p - di_m, di_p, di_m


def _choppiness(bars, period):
    tr = _true_ranges(bars)[-period:]
    window = bars[-period:]
    span = max(max(b[1] for b in window) - min(b[2] for b in window), 1e-8)
    return min(max(100.0 * math.log10(sum(tr) / span) / math.log10(period), 0.0), 100.0)


def _garman_klass(bars, period):
    """Per bar 0.5 ln(H/L)^2 - (2 ln 2 - 1) ln(C/O)^2, floored at 0; the mean over `period`
    rooted and annualised with sqrt(252), in percent."""
    total = 0.0
    for o, h, l, c in bars[-period:]:
        total += max(0.5 * math.log(h / l) ** 2 - (2.0 * math.log(2.0) - 1.0) * math.log(c / o) ** 2, 0.0)
    return math.sqrt(total / period) * math.sqrt(252.0) * 100.0


def _historical_volatility(closes, period):
    """Sample standard deviation (divisor n - 1) of the last `period` log returns, annualised with
    sqrt(252), in percent."""
    window = closes[-(period + 1):]
    returns = [math.log(b / a) for a, b in zip(window, window[1:])]
    mean = sum(returns) / len(returns)
    variance = sum((r - mean) ** 2 for r in returns) / max(len(returns) - 1, 1)
    return math.sqrt(variance) * math.sqrt(252.0) * 100.0


def _mass_index(bars, period):
    """Sum over the last `period` bars of EMA9(range) / EMA9(EMA9(range)), both EMAs
    first-sample-seeded; range floored at 1e-8, ratio 1 for a vanishing denominator."""
    ranges = [max(b[1] - b[2], 1e-8) for b in bars]
    e1 = ema_first_sample(ranges, 9)
    e2 = ema_first_sample(e1, 9)
    ratios = [a / b if b > 1e-8 else 1.0 for a, b in zip(e1, e2)]
    return sum(ratios[-period:])


def _parabolic_sar(bars, step, max_step):
    """Starts long with SAR = first low and EP = first high. Each bar: next = SAR + AF (EP - SAR);
    a low below it (long) or a high above it (short) reverses, SAR = EP, EP = that low/high,
    AF = step; otherwise a new extreme moves EP and raises AF by step up to max_step, and the SAR
    is held at or below the previous and current low (long) or at or above the previous and
    current high (short)."""
    long, sar, ep, af = True, bars[0][2], bars[0][1], step
    for prev, bar in zip(bars, bars[1:]):
        nxt = sar + af * (ep - sar)
        if long:
            if bar[2] < nxt:
                long, nxt, ep, af = False, ep, bar[2], step
            else:
                if bar[1] > ep:
                    ep, af = bar[1], min(af + step, max_step)
                nxt = min(nxt, prev[2], bar[2])
        else:
            if bar[1] > nxt:
                long, nxt, ep, af = True, ep, bar[1], step
            else:
                if bar[2] < ep:
                    ep, af = bar[2], min(af + step, max_step)
                nxt = max(nxt, prev[1], bar[1])
        sar = nxt
    return sar


def _supertrend(bars, period, mult):
    """ATR = plain mean of the last `period` true ranges; basic bands (high + low)/2 +/- mult ATR;
    a final band only moves against the trend when the previous close crossed it; the trend starts
    up and flips when the close crosses the opposite final band; the line is the lower band in an
    uptrend and the upper band in a downtrend."""
    tr = _true_ranges(bars)
    trend, upper, lower, line = 1, 0.0, 0.0, None
    prev_close = None
    for i, bar in enumerate(bars):
        if i < period - 1:
            prev_close = bar[3]
            continue
        atr = sum(tr[i - period + 1:i + 1]) / period
        mid = (bar[1] + bar[2]) / 2.0
        basic_upper, basic_lower = mid + mult * atr, mid - mult * atr
        pc = bar[3] if prev_close is None else prev_close
        final_upper = basic_upper if basic_upper < upper or pc > upper else upper
        final_lower = basic_lower if basic_lower > lower or pc < lower else lower
        if trend == 1 and bar[3] < final_lower:
            trend = -1
        elif trend == -1 and bar[3] > final_upper:
            trend = 1
        upper, lower = final_upper, final_lower
        line = final_lower if trend == 1 else final_upper
        prev_close = bar[3]
    return line, float(trend)


def _vortex(bars, period):
    """VM+ = |high - prev_low|, VM- = |low - prev_high|; VI+/- = sum VM+/- over the last `period`
    bars / sum TR (floored at 1e-8)."""
    vm_p, vm_m, tr = [], [], []
    for prev, bar in zip(bars, bars[1:]):
        vm_p.append(abs(bar[1] - prev[2]))
        vm_m.append(abs(bar[2] - prev[1]))
        tr.append(max(bar[1] - bar[2], abs(bar[1] - prev[3]), abs(bar[2] - prev[3])))
    total = max(sum(tr[-period:]), 1e-8)
    return sum(vm_p[-period:]) / total, sum(vm_m[-period:]) / total


def _chandelier(bars, length, mult):
    """Wilder ATR over `length` (first true range high - low); raw stops highest(high, length) -
    mult ATR (long) and lowest(low, length) + mult ATR (short). A stop only tightens while the
    previous close was on its side of the previous stop; the direction turns long on a close above
    the previous short stop and short on a close below the previous long stop. Returns the last
    long and short stop."""
    atr = rma(_true_ranges(bars), length)
    long_prev = short_prev = prev_close = None
    long = short = None
    for i, bar in enumerate(bars):
        if i < length - 1 or atr[i] is None:
            prev_close = bar[3]
            continue
        window = bars[i - length + 1:i + 1]
        raw_long = max(b[1] for b in window) - mult * atr[i]
        raw_short = min(b[2] for b in window) + mult * atr[i]
        lp = raw_long if long_prev is None else long_prev
        sp = raw_short if short_prev is None else short_prev
        long = max(raw_long, lp) if prev_close is not None and prev_close > lp else raw_long
        short = min(raw_short, sp) if prev_close is not None and prev_close < sp else raw_short
        long_prev, short_prev, prev_close = long, short, bar[3]
    return long, short


def derive():
    keys = {}
    rising = [(100.0 + i, 100.0 + i + 1.0, 100.0 + i - 1.0, 100.0 + i) for i in range(10)]
    keys["adx3_smooth3"] = _adx(rising, 3, 3)[-1]
    keys["aroon5_osc"], keys["aroon5_up"], keys["aroon5_down"] = _aroon(VOL_BARS, 5)
    keys["chandelier5_long"], keys["chandelier5_short"] = _chandelier(VOL_BARS, 5, 3.0)
    keys["choppiness5_last"] = _choppiness(VOL_BARS, 5)
    keys["dmi5_dx"], keys["dmi5_plus"], keys["dmi5_minus"] = _dmi(VOL_BARS, 5)
    window = VOL_BARS[-5:]
    upper, lower = max(b[1] for b in window), min(b[2] for b in window)
    keys["donchian5_basis"], keys["donchian5_upper"], keys["donchian5_lower"] = (
        (upper + lower) / 2.0, upper, lower)
    basis = sum(VOL_PRICES[-5:]) / 5.0
    keys["envelope5_basis"] = basis
    keys["envelope5_upper"], keys["envelope5_lower"] = basis + basis * 0.02, basis - basis * 0.02
    keys["garman_klass5_last"] = _garman_klass(VOL_BARS, 5)
    keys["hv5_last"] = _historical_volatility(VOL_PRICES, 5)
    keys["keltner5_basis"], keys["keltner5_upper"], keys["keltner5_lower"] = _keltner(VOL_BARS, 5, 5, 2.0)
    keys["mass_index5_last"] = _mass_index(VOL_BARS, 5)
    keys["psar_last"] = _parabolic_sar(VOL_BARS, 0.02, 0.20)
    keys["supertrend5_line"], keys["supertrend5_dir"] = _supertrend(VOL_BARS, 5, 3.0)
    keys["true_range_last"] = _true_ranges(VOL_BARS)[-1]
    highest = max(VOL_PRICES[-5:])
    keys["vix_fix5_last"] = (highest - VOL_BARS[-1][2]) / highest * 100.0
    keys["vortex5_plus"], keys["vortex5_minus"] = _vortex(VOL_BARS, 5)
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
    keys["adx3_shaped_last"] = _adx(SHAPED, 3, 3)[-1]
    keys["mass_index5_shaped_last"] = _mass_index(SHAPED, 5)
    return keys


HEADER = [
    "kestrel-chartkit golden reference fixture: volatility indicators",
    "",
    *provenance("volatility"),
    "",
    "ATR(3)/Signal(2): over five flat bars (H 101, L 99, C 100) the true range is 2 throughout, so",
    "the percentage is exactly 2; over five bars with an up gap and a down gap raw value (price",
    "units), percentage of the close and Wilder signal of the percentage, first and second output.",
    "ADX(3, 3) over ten strictly rising bars (centre 100 + i, +/- 1): DX is 100 on every bar there.",
    "",
    "Base values over the shared 20-bar series, open = close = p, high = p + 0.5, low = p - 0.5,",
    "p = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,",
    "45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64], each from the definition on its type:",
    "Aroon(5), Chandelier Exit(5, 3), Choppiness(5), DMI(5) - its value is +DI - -DI, stored as",
    "dmi5_dx for historical reasons -, Donchian(5), Envelope(5, 2 %), Garman-Klass(5), historical",
    "volatility(5), Keltner(5, 5, 2), Mass Index(5), Parabolic SAR(0.02, 0.2), Supertrend(5, 3), true",
    "range, VIX Fix(5, 5, 2), Vortex(5). Keltner and Supertrend take the plain mean of the true",
    "range, Chandelier the Wilder average; the SAR is held beyond the previous bar's extreme.",
    "",
    "On their inputs the ADX (strictly rising) and the Mass Index (constant range of 1, ratio 1)",
    "only pin a limit. The *_shaped keys come from 14 bars moving both ways with varying ranges:",
    "closes [100, 102, 101, 104, 103, 106, 105, 103, 104, 101, 102, 99, 100, 98], open = close,",
    "high = close + 1 + 0.3 (i mod 3), low = close - 1 - 0.2 (i mod 4).",
    "",
    "Chande Kroll Stop (package 29), atr_len 4, stop_len 3 and 1, mult 2, over the 20 OHLC bars in",
    "the test: preliminary long = highest(high, 4) - 2 ATR, short = lowest(low, 4) + 2 ATR (Wilder",
    "ATR), then the highest of the long and the lowest of the short series over stop_len values.",
    "True-range smoothing methods (package 42), atr_len 3, over six bars with gaps: RMA, SMA, EMA",
    "and WMA of the same true ranges [3, 6, 6, 8, 6, 4], each published from the third on.",
    "Ulcer Index (package 34), len 3, over [100, 102, 104, 101, 96, 92, 95, 99, 103, 106, 104,",
    "105]: percentage below the running maximum of the last 3 values, squares averaged over 3 and",
    "rooted, first value with the fifth observation.",
    "Relative Volatility Index (package 35), stdev_len 5, smooth_len 4, over",
    "close = 100 + 6 sin(0.4 i) + 0.5 (-1)^i, 40 bars, high/low = close +/- 1: population standard",
    "deviation filed under up or down by the direction of the price, Wilder-smoothed,",
    "100 up / (up + down). The high/low variant averages the index over highs and lows.",
    "",
    "The *_tolerance keys are the comparison tolerances the tests apply, stated rather than derived.",
]

SECTIONS = [
    ("ATR over flat and gapped bars, ADX over a strictly rising series",
     ["atr3_signal2_pct", "atr_tolerance", "atr_gap3_signal2_raw_first",
      "atr_gap3_signal2_pct_first", "atr_gap3_signal2_signal_first", "atr_gap3_signal2_raw_second",
      "atr_gap3_signal2_pct_second", "atr_gap3_signal2_signal_second", "adx3_smooth3",
      "adx_tolerance"]),
    ("base values over the shared 20-bar series",
     ["aroon5_osc", "aroon5_up", "aroon5_down", "chandelier5_long", "chandelier5_short",
      "choppiness5_last", "dmi5_dx", "dmi5_plus", "dmi5_minus", "donchian5_basis",
      "donchian5_upper", "donchian5_lower", "envelope5_basis", "envelope5_upper",
      "envelope5_lower", "garman_klass5_last", "hv5_last", "keltner5_basis", "keltner5_upper",
      "keltner5_lower", "mass_index5_last", "psar_last", "supertrend5_line", "supertrend5_dir",
      "true_range_last", "vix_fix5_last", "vortex5_plus", "vortex5_minus", "vol_tolerance"]),
    ("the ADX and the Mass Index where the formulas actually act",
     ["adx3_shaped_last", "mass_index5_shaped_last"]),
    ("Chande Kroll Stop (package 29)",
     ["cks4_3_mult2_output_count", "cks4_3_mult2_long_first", "cks4_3_mult2_short_first",
      "cks4_3_mult2_long_last", "cks4_3_mult2_short_last", "cks4_1_mult2_long_last",
      "cks4_1_mult2_short_last", "cks_tolerance"]),
    ("true-range smoothing methods (package 42)",
     [f"atr3_{m}_raw_{w}" for m in ("rma", "sma", "ema", "wma") for w in ("third", "last")]
     + ["atr3_method_output_count"]),
    ("Ulcer Index (package 34)",
     ["ulcer3_first", "ulcer3_second", "ulcer3_last", "ulcer3_output_count", "ulcer_tolerance"]),
    ("Relative Volatility Index (package 35)",
     ["rvi_vol_close_first", "rvi_vol_close_last", "rvi_vol_close_output_count",
      "rvi_vol_high_low_last", "rvi_vol_tolerance"]),
]


def build():
    values = {**derive(), **TOLERANCES}
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in SECTIONS]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
