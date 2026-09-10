"""golden_oscillators — partly derived so far.

Derived here: Bollinger over the 20-price series and the variance conventions near 10000
(package 18), TRIX (22), the Rank Correlation Index (33), the Stochastic Momentum Index (32),
BBTrend (36) and the Price Momentum Oscillator (38), each from the definition documented on its
type. Not yet: the base oscillators from the 0.2.0 starting point and the RSI smoothing modes,
whose line/signal/context outputs are not documented on the RSI type.
"""

import math
from fractions import Fraction

from ..exact import average_ranks, bollinger, correlation, sqrt
from ..smoothing import ema_alpha, ema_first_sample

NAME = "golden_oscillators"
PARTIAL = True
CONSTANTS = {
    "osc_tolerance": 1e-9,
    "bollinger5_band_tolerance": 1e-9,
    "bollinger5_ratio_tolerance": 1e-12,
    "trix4_signal3_tolerance": 1e-12,
    "trix_geometric_tolerance": 1e-6,
    "rci_tolerance": 1e-10,
    "smi_tolerance": 1e-12,
    "bbtrend_tolerance": 1e-9,
    "pmo_tolerance": 1e-12,
}

# The committed values read the literals of the near-10000 series as decimal numbers; this module
# takes the exact binary values the implementation actually receives. percent_b divides by a band
# width of about 0.13 on a level of 10000, which is where the two readings part, by 3e-12
# relative. Verified: reading the literals as decimals reproduces the committed values exactly.
_DECIMAL_READING = "near-10000 literals read as decimals rather than as their binary values"
DIFFERENCES = {
    "bollinger5_population_percent_b": (1e-11, _DECIMAL_READING),
    "bollinger5_sample_percent_b": (1e-11, _DECIMAL_READING),
}

OSC_PRICES = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
              45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]
BOLLINGER_OFFSET_CLOSES = [10000.05, 10000.02, 10000.08, 10000.01, 10000.09]
TRIX_CLOSES = [100.0, 101.0, 102.5, 101.5, 103.0, 104.5, 104.0, 105.5, 107.0, 106.0, 107.5, 109.0,
               108.5, 110.0, 111.5, 111.0, 112.5, 114.0, 113.5, 115.0]
RCI_SERIES = [100.0, 101.0, 99.0, 102.0, 103.0, 101.0, 104.0, 103.0, 105.0, 104.0]
BBTREND_CLOSES = [100.0, 101.0, 102.5, 101.5, 103.0, 104.5, 104.0, 105.5, 107.0, 106.0, 107.5,
                  109.0]


def _trix(closes, length, signal_len):
    """Three chained first-sample-seeded EMAs; TRIX_t = 100 (e3_t / e3_(t-1) - 1) from the
    (length+1)-th bar on; signal = EMA(signal_len) over the published TRIX values, seeded with the
    first of them."""
    e3 = ema_first_sample(ema_first_sample(ema_first_sample(closes, length), length), length)
    line = [100.0 * (e3[t] / e3[t - 1] - 1.0) for t in range(length, len(closes))]
    return line, ema_first_sample(line, signal_len)


def _rci(window):
    """100 x Pearson correlation of the time ranks with the average price ranks; 0 without
    price variance."""
    r = correlation([Fraction(i + 1) for i in range(len(window))], average_ranks(window))
    return 0.0 if r is None else float(100 * r)


def _smi(bars, length, smooth_1, smooth_2, signal_len):
    """distance = close - (HH+LL)/2, range = HH - LL over the last `length` bars, each smoothed
    by an EMA(smooth_1) and then an EMA(smooth_2). Both stages run from the first full window,
    each seeded with the first value it receives, the second taking every output of the first;
    the line is published once smooth_1 + smooth_2 - 1 windows have passed. SMI = 200 * distance
    / range, 0 for a zero range; signal = EMA(signal_len) over the published SMI values."""
    distance, span = [], []
    for i in range(length - 1, len(bars)):
        window = bars[i - length + 1:i + 1]
        highest = max(high for high, _, _ in window)
        lowest = min(low for _, low, _ in window)
        distance.append(bars[i][2] - (highest + lowest) / 2.0)
        span.append(highest - lowest)

    def double(series):
        return ema_first_sample(ema_first_sample(series, smooth_1), smooth_2)

    line = [200.0 * d / r if r != 0 else 0.0 for d, r in zip(double(distance), double(span))]
    line = line[smooth_1 + smooth_2 - 2:]
    return line, ema_first_sample(line, signal_len)


def _bbtrend(closes, short_len, long_len, mult):
    """100 (|lower_short - lower_long| - |upper_short - upper_long|) / basis_short, with both band
    sets from the population-variance Bollinger definition; from the long_len-th bar on."""
    out = []
    for i in range(long_len - 1, len(closes)):
        short = bollinger(closes[i - short_len + 1:i + 1], mult)
        long = bollinger(closes[i - long_len + 1:i + 1], mult)
        upper_gap = abs(short["upper"] - long["upper"])
        lower_gap = abs(short["lower"] - long["lower"])
        out.append({"value": float(100 * (lower_gap - upper_gap) / short["basis"]),
                    "upper_gap": float(upper_gap), "lower_gap": float(lower_gap)})
    return out


def _pmo(closes, length_1, length_2, signal_len):
    """roc = 100 (close_t / close_(t-1) - 1); stage 1 = EMA(roc, alpha 2/length_1); PMO =
    EMA(10 * stage 1, alpha 2/length_2). Both stages run from the first return, each seeded with
    the first value it receives; the line is published once length_1 + length_2 - 1 returns have
    passed. Signal = ordinary EMA(signal_len) over the published PMO values."""
    roc = [100.0 * (closes[t] / closes[t - 1] - 1.0) for t in range(1, len(closes))]
    stage_1 = ema_alpha(roc, 2.0 / length_1)
    line = ema_alpha([10.0 * x for x in stage_1], 2.0 / length_2)[length_1 + length_2 - 2:]
    return line, ema_first_sample(line, signal_len)


def derive():
    keys = {}
    for field, value in bollinger(OSC_PRICES[-5:], 2.0).items():
        keys[f"bollinger5_{field}"] = float(value)
    for label, sample in (("population", False), ("sample", True)):
        for field, value in bollinger(BOLLINGER_OFFSET_CLOSES, 2.0, sample=sample).items():
            keys[f"bollinger5_{label}_{field}"] = float(value)
    # The two band distances differ by the divisor alone: sqrt(n / (n - 1)) at n = 20.
    keys["bollinger20_sample_over_population_distance"] = float(sqrt(Fraction(20, 19)))

    line, signal = _trix(TRIX_CLOSES, 4, 3)
    keys["trix4_signal3_output_count"] = float(len(line))
    keys["trix4_signal3_line_first"] = line[0]
    keys["trix4_signal3_line_last"] = line[-1]
    keys["trix4_signal3_signal_last"] = signal[-1]
    # On x_t = x_0 * 1.01^t every stage settles into the same growth, so TRIX tends to exactly
    # 100 * (1.01 - 1) percentage points.
    keys["trix_geometric_growth_percent"] = float(100 * (Fraction(101, 100) - 1))

    keys["rci5_rising"] = _rci([10.0, 11.0, 12.0, 13.0, 14.0])
    keys["rci5_falling"] = _rci([14.0, 13.0, 12.0, 11.0, 10.0])
    keys["rci5_mixed"] = _rci([10.0, 12.0, 11.0, 14.0, 13.0])
    keys["rci5_with_ties"] = _rci([10.0, 11.0, 11.0, 11.0, 13.0])
    series = [_rci(RCI_SERIES[i - 4:i + 1]) for i in range(4, len(RCI_SERIES))]
    keys["rci5_series_output_count"] = float(len(series))
    keys["rci5_series_first"] = series[0]
    keys["rci5_series_last"] = series[-1]

    sine = [(100.0 + 6.0 * math.sin(i * 0.5) + 1.0, 100.0 + 6.0 * math.sin(i * 0.5) - 1.0,
             100.0 + 6.0 * math.sin(i * 0.5)) for i in range(24)]
    line, signal = _smi(sine, 5, 3, 3, 3)
    keys["smi5_output_count"] = float(len(line))
    keys["smi5_first"] = line[0]
    keys["smi5_last"] = line[-1]
    keys["smi5_signal_last"] = signal[-1]
    # On a straight line with a fixed candle shape the range (6) and the distance from the
    # midpoint (2) are constant once the window is full: exactly 200 * 2 / 6.
    keys["smi5_linear_ramp"] = float(Fraction(200 * 2, 6))

    bbtrend = _bbtrend(BBTREND_CLOSES, 3, 6, 2.0)
    keys["bbtrend_3_6_output_count"] = float(len(bbtrend))
    keys["bbtrend_3_6_first"] = bbtrend[0]["value"]
    keys["bbtrend_3_6_first_upper_gap"] = bbtrend[0]["upper_gap"]
    keys["bbtrend_3_6_first_lower_gap"] = bbtrend[0]["lower_gap"]
    keys["bbtrend_3_6_last"] = bbtrend[-1]["value"]

    sawtooth = [100.0 * (1.0 + 0.01 * ((i % 7) - 3.0)) for i in range(30)]
    line, signal = _pmo(sawtooth, 5, 3, 3)
    keys["pmo_5_3_output_count"] = float(len(line))
    keys["pmo_5_3_first"] = line[0]
    keys["pmo_5_3_last"] = line[-1]
    keys["pmo_5_3_signal_last"] = signal[-1]
    # A constant 1 % return: both stages settle on their input, so PMO tends to ten times it.
    keys["pmo_constant_growth"] = float(10 * 100 * (Fraction(101, 100) - 1))
    return keys
