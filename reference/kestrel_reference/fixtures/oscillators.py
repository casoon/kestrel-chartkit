"""golden_oscillators: oscillators, each value from the definition documented on its type."""

import math
from fractions import Fraction

from ..smoothing import ema_published, published, rma
from ..exact import average_ranks, bollinger, correlation, sqrt
from ..fixture import provenance
from ..smoothing import ema_alpha, ema_first_sample
from ..indicators import clamp as _clamp, raw_rsi as _raw_rsi, rsi_from_averages as _rsi_from_averages, rsi_lines as _rsi_lines

NAME = "golden_oscillators"
TOLERANCES = {
    "rsi14_tolerance": 1e-12,
    "macd_tolerance": 1e-12,
    "rsi5_mixed_tolerance": 1e-12,
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


OSC_PRICES = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
              45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]
BOLLINGER_OFFSET_CLOSES = [10000.05, 10000.02, 10000.08, 10000.01, 10000.09]
OSC_BARS = [(p, p + 0.5, p - 0.5, p, 1000.0) for p in OSC_PRICES]
LINE60 = [(44.0 + i * 0.1, 44.0 + i * 0.1 + 0.5, 44.0 + i * 0.1 - 0.5, 44.0 + i * 0.1, 1000.0)
          for i in range(60)]
MACD_CLOSES = [10.0 + 0.5 * i for i in range(30)]
# Where the shared series (open = close, close mid-range) cannot tell a right implementation from
# a wrong one: bars with the close away from the open and from mid-range.
SHAPED = [(10.0, 10.8, 9.6, 10.6, 1200.0), (10.6, 11.2, 10.2, 10.3, 900.0),
          (10.3, 10.9, 10.0, 10.8, 1500.0), (10.8, 11.0, 10.1, 10.2, 700.0),
          (10.2, 10.7, 9.9, 10.6, 1100.0), (10.6, 11.4, 10.5, 11.3, 1600.0),
          (11.3, 11.5, 10.9, 11.0, 800.0), (11.0, 11.6, 10.8, 11.5, 1300.0),
          (11.5, 11.9, 11.1, 11.2, 1000.0), (11.2, 11.4, 10.7, 10.9, 1400.0)]
RSI_MIXED_CLOSES = [100.0, 101.0, 102.0, 103.0, 104.0, 104.0, 104.0, 104.0, 103.0, 101.0, 98.0,
                    96.0, 95.0, 96.0, 95.0, 97.0, 96.0, 98.0, 97.0, 99.0]
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


def _windows(values, length):
    return [values[i - length + 1:i + 1] for i in range(length - 1, len(values))]


def _stochastic_k(bars, length):
    out = []
    for window in _windows(bars, length):
        hh, ll = max(b[1] for b in window), min(b[2] for b in window)
        c = window[-1][3]
        out.append(_clamp((c - ll) / (hh - ll) * 100.0) if abs(hh - ll) > 1e-8 else 50.0)
    return out


def _percent_r(bars, length):
    """Williams %R on this crate's ascending scale: 100 (close - LL) / (HH - LL), 50 for a flat
    window."""
    out = []
    for window in _windows(bars, length):
        hh, ll = max(b[1] for b in window), min(b[2] for b in window)
        c = window[-1][3]
        out.append(100.0 * (c - ll) / (hh - ll) if hh != ll else 50.0)
    return out


def _cci_raw(bars, length):
    out = []
    for window in _windows([(b[1] + b[2] + b[3]) / 3.0 for b in bars], length):
        mean = sum(window) / length
        dev = sum(abs(v - mean) for v in window) / length
        out.append((window[-1] - mean) / (0.015 * dev) if dev != 0.0 else 0.0)
    return out


def _mfi_raw(bars, length):
    flows = []
    previous = None
    for _, h, l, c, v in bars:
        tp = (h + l + c) / 3.0
        raw = tp * v
        if previous is None or tp == previous:
            flows.append((0.0, 0.0))
        elif tp > previous:
            flows.append((raw, 0.0))
        else:
            flows.append((0.0, raw))
        previous = tp
    out = []
    for window in _windows(flows, length):
        pos, neg = sum(f[0] for f in window), sum(f[1] for f in window)
        if pos + neg == 0.0:
            out.append(50.0)
        elif neg == 0.0:
            out.append(100.0)
        elif pos == 0.0:
            out.append(0.0)
        else:
            out.append(100.0 - 100.0 / (1.0 + pos / neg))
    return out


def _fisher(bars, length, avg_len, sig_len):
    """Over hl2 in a window of `length`: v = clamp(0.66 ((x - LL)/(HH - LL) - 0.5) + 0.67 v_prev,
    +/-0.999) and fish = 0.5 ln((1 + v)/(1 - v)) + 0.5 fish_prev, both recursions starting at 0;
    line = EMA(avg_len) of fish, signal = EMA(sig_len) of the line."""
    value = fish = 0.0
    raw = []
    for window in _windows([(b[1] + b[2]) / 2.0 for b in bars], length):
        hh, ll = max(window), min(window)
        normalized = (window[-1] - ll) / (hh - ll) - 0.5 if hh != ll else 0.0
        value = min(max(0.66 * normalized + 0.67 * value, -0.999), 0.999)
        fish = 0.5 * math.log((1.0 + value) / (1.0 - value)) + 0.5 * fish
        raw.append(fish)
    line = ema_first_sample(raw, avg_len)
    return line, ema_first_sample(line, sig_len)


def _connors(closes, rsi_len, streak_len):
    """As the implementation runs it: the close RSI is the RSI indicator's smoothed line; streak and
    one-bar return only start updating on the bar after that line first exists, the streak RSI
    runs over the streak values from the line's first bar on, and the percentile rank is taken
    over all returns seen so far (at most 100)."""
    line, _, _ = _rsi_lines(closes, rsi_len, 3, 3)
    first = len(closes) - len(line)
    streak, streaks, returns = 0.0, [], []
    previous = None
    for t in range(first, len(closes)):
        c = closes[t]
        if previous is not None:
            if c > previous:
                streak = streak + 1.0 if streak >= 0.0 else 1.0
            elif c < previous:
                streak = streak - 1.0 if streak <= 0.0 else -1.0
            else:
                streak = 0.0
            returns.append((c - previous) / previous if previous > 0.0 else 0.0)
            returns = returns[-100:]
        previous = c
        streaks.append(streak)
    streak_line, _, _ = _rsi_lines(streaks, streak_len, 3, 3)
    last = returns[-1] if returns else 0.0
    rank = 100.0 * sum(1 for r in returns if r <= last) / len(returns) if returns else 0.0
    return _clamp((line[-1] + streak_line[-1] + rank) / 3.0)


def _sma_series(values, length):
    return [sum(w) / length for w in _windows(values, length)]


def _wma_last(values, length):
    window = values[-length:]
    return sum((i + 1) * v for i, v in enumerate(window)) / (length * (length + 1) / 2.0)


def _ultimate(bars, p1, p2, p3):
    window = bars[-(p3 + 1):]

    def ratio(p):
        part = window[-(p + 1):]
        bp = tr = 0.0
        for prev, bar in zip(part, part[1:]):
            low, high = min(bar[2], prev[3]), max(bar[1], prev[3])
            bp += bar[3] - low
            tr += high - low
        return bp / tr if tr > 0.0 else 0.0

    return _clamp((4.0 * ratio(p1) + 2.0 * ratio(p2) + ratio(p3)) / 7.0 * 100.0)


def _wavetrend(bars, n1, n2):
    tp = [(b[1] + b[2] + b[3]) / 3.0 for b in bars]
    esa = ema_first_sample(tp, n1)
    d = ema_first_sample([abs(a - e) for a, e in zip(tp, esa)], n1)
    ci = [(a - e) / (0.015 * x) if x > 1e-8 else 0.0 for a, e, x in zip(tp, esa, d)]
    wt1 = ema_first_sample(ci, n2)
    return wt1[-1], sum(wt1[-4:]) / 4.0


def derive():
    keys = {}
    closes = [b[3] for b in OSC_BARS]
    keys["rsi14_last"] = _rsi_lines(closes, 14, 3, 3)[0][-1]
    for smoothing in ("wilder", "ema"):
        line, signal, ctx = _rsi_lines(RSI_MIXED_CLOSES, 5, 3, 3, ctx_len=8, smoothing=smoothing)
        keys[f"rsi5_mixed_{smoothing}_line"] = line[-1]
        keys[f"rsi5_mixed_{smoothing}_signal"] = signal[-1]
        keys[f"rsi5_mixed_{smoothing}_ctx"] = ctx[-1]

    fast, slow = ema_first_sample(MACD_CLOSES, 12), ema_first_sample(MACD_CLOSES, 26)
    macd = [f - sl for f, sl in zip(fast, slow)][25:]
    macd_signal = ema_first_sample(macd, 9)
    keys["macd_line_last"], keys["macd_signal_last"] = macd[-1], macd_signal[-1]
    keys["macd_hist_last"] = macd[-1] - macd_signal[-1]

    keys["cci5_last"] = ema_first_sample(_cci_raw(OSC_BARS, 5), 3)[-1]
    k = _stochastic_k(OSC_BARS, 5)
    keys["stoch5_k"], keys["stoch5_d"] = k[-1], _clamp(sum(k[-3:]) / 3.0)
    rsi_raw = _raw_rsi(closes, 5)
    stoch_raw = [_clamp(100.0 * (w[-1] - min(w)) / (max(w) - min(w))) if max(w) > min(w) else 50.0
                 for w in _windows(rsi_raw, 5)]
    k_line = [_clamp(x) for x in _sma_series(stoch_raw, 3)]
    keys["stoch_rsi5_k"], keys["stoch_rsi5_d"] = k_line[-1], _clamp(sum(k_line[-3:]) / 3.0)
    keys["mfi5_last"] = ema_first_sample(_mfi_raw(OSC_BARS, 5), 3)[-1]
    keys["williams_r5_last"] = ema_first_sample(_percent_r(OSC_BARS, 5), 3)[-1]
    moves = [b - a for a, b in zip(closes, closes[1:])]
    double_mom = ema_first_sample(ema_first_sample(moves, 5), 3)
    double_abs = ema_first_sample(ema_first_sample([abs(m) for m in moves], 5), 3)
    tsi = [100.0 * m / a if a != 0.0 else 0.0 for m, a in zip(double_mom, double_abs)]
    keys["tsi5_line"], keys["tsi5_signal"] = tsi[-1], ema_first_sample(tsi, 3)[-1]
    fisher_line, fisher_signal = _fisher(OSC_BARS, 5, 2, 3)
    keys["fisher5_line"], keys["fisher5_signal"] = fisher_line[-1], fisher_signal[-1]
    hl2 = [(b[1] + b[2]) / 2.0 for b in OSC_BARS]
    keys["ao3_5_last"] = sum(hl2[-3:]) / 3.0 - sum(hl2[-5:]) / 5.0
    keys["bop5_last"] = _clamp(sum((c - o) / max(h - l, 1e-8) for o, h, l, c, _ in OSC_BARS[-5:]) / 5.0,
                               -1.0, 1.0)
    ad, total = [], 0.0
    for _, h, l, c, v in OSC_BARS:
        total += (((c - l) - (h - c)) / (h - l) if h - l > 1e-8 else 0.0) * v
        ad.append(total)
    keys["chaikin_osc_last"] = ema_first_sample(ad, 3)[-1] - ema_first_sample(ad, 5)[-1]
    gains = sum(max(m, 0.0) for m in moves[-5:])
    losses = sum(max(-m, 0.0) for m in moves[-5:])
    keys["cmo5_last"] = (_clamp(100.0 * (gains - losses) / (gains + losses), -100.0, 100.0)
                         if gains + losses > 1e-8 else 0.0)
    keys["connors_rsi_last"] = _connors(closes, 3, 2)
    closes60 = [b[3] for b in LINE60]
    raw_coppock = [(closes60[t] - closes60[t - 14]) / closes60[t - 14] * 100.0
                   + (closes60[t] - closes60[t - 11]) / closes60[t - 11] * 100.0
                   for t in range(14, 60)]
    keys["coppock_60bar_last"] = _wma_last(raw_coppock, 10)
    window = closes[-5:]
    keys["dpo5_last"] = window[5 - (5 // 2 + 1)] - sum(window) / 5.0
    ema5 = ema_first_sample(closes, 5)[-1]
    keys["elder_bull_last"], keys["elder_bear_last"] = OSC_BARS[-1][1] - ema5, OSC_BARS[-1][2] - ema5
    rocs = {n: [(closes60[t] - closes60[t - n]) / closes60[t - n] * 100.0 for t in range(30, 60)]
            for n in (10, 15, 20, 30)}
    smoothed = [_sma_series(rocs[10], 10), _sma_series(rocs[15], 10), _sma_series(rocs[20], 10),
                _sma_series(rocs[30], 15)]
    shortest = min(len(x) for x in smoothed)
    kst = [sum((i + 1) * series[-shortest + j] for i, series in enumerate(smoothed))
           for j in range(shortest)]
    keys["kst60_line"], keys["kst60_signal"] = kst[-1], sum(kst[-9:]) / 9.0
    ppo_fast, ppo_slow = ema_first_sample(closes, 3), ema_first_sample(closes, 5)
    ppo = [(f - sl) / sl * 100.0 if sl > 0.0 else 0.0 for f, sl in zip(ppo_fast, ppo_slow)][4:]
    keys["ppo_line"], keys["ppo_signal"] = ppo[-1], ema_first_sample(ppo, 3)[-1]
    keys["roc5_last"] = (closes[-1] - closes[-6]) / closes[-6] * 100.0
    vigor = _sma_series([(c - o) / max(h - l, 1e-8) for o, h, l, c, _ in OSC_BARS], 5)
    keys["rvi_line"], keys["rvi_signal"] = vigor[-1], sum(vigor[-4:]) / 4.0
    keys["uo_last"] = _ultimate(OSC_BARS, 3, 5, 10)
    keys["wt1_last"], keys["wt2_last"] = _wavetrend(OSC_BARS, 3, 5)

    keys["bop5_shaped_last"] = _clamp(
        sum((c - o) / max(h - l, 1e-8) for o, h, l, c, _ in SHAPED[-5:]) / 5.0, -1.0, 1.0)
    ad, total = [], 0.0
    for _, h, l, c, v in SHAPED:
        total += (((c - l) - (h - c)) / (h - l) if h - l > 1e-8 else 0.0) * v
        ad.append(total)
    keys["chaikin_osc_shaped_last"] = ema_first_sample(ad, 3)[-1] - ema_first_sample(ad, 5)[-1]
    vigor = _sma_series([(c - o) / max(h - l, 1e-8) for o, h, l, c, _ in SHAPED], 5)
    keys["rvi_shaped_line"], keys["rvi_shaped_signal"] = vigor[-1], sum(vigor[-4:]) / 4.0
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


HEADER = [
    "kestrel-chartkit golden reference fixture: oscillators",
    "",
    *provenance("oscillators"),
    "",
    "Base values over the shared 20-bar series, open = close = p, high = p + 0.5, low = p - 0.5,",
    "volume 1000, p = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89,",
    "46.03, 45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64], each from the definition on its",
    "type: RSI(14), Bollinger(5, 2), CCI(5), Stochastic(5, 3), Stochastic RSI(5, 5, 3, 3), MFI(5),",
    "Williams %R(5), TSI(5, 3, 3), Fisher(5, 2, 3), Awesome(3, 5), BOP(5), Chaikin(3, 5), CMO(5),",
    "Connors RSI(3, 2, 5), DPO(5), Elder Ray(5), PPO(3, 5, 3), ROC(5), RVI(5), Ultimate(3, 5, 10) and",
    "WaveTrend(3, 5). MACD(12, 26, 9) runs over 30 closes 10 + 0.5 i, Coppock and KST over 60 closes",
    "44 + 0.1 i.",
    "",
    "The RSI, CCI, MFI, Williams %R and Fisher values are those indicators' published lines, an EMA",
    "over the raw figure (avg_len 3, Fisher 2), not the raw figure itself. rsi14_last is that line",
    "exactly; it used to be stored rounded to two decimals and compared with a tolerance of 0.1.",
    "Williams %R is on this crate's ascending 0..100 scale. Connors RSI is computed as the",
    "implementation runs it, including the smoothed RSI lines and the rank over all returns so far.",
    "",
    "On the shared series BOP, the Chaikin oscillator and RVI are identically 0 (open = close, close",
    "mid-range): those keys pin that. The *_shaped keys come from ten bars with the close away from",
    "the open and from mid-range: (o, h, l, c, v) = (10, 10.8, 9.6, 10.6, 1200), (10.6, 11.2, 10.2,",
    "10.3, 900), (10.3, 10.9, 10, 10.8, 1500), (10.8, 11, 10.1, 10.2, 700), (10.2, 10.7, 9.9, 10.6,",
    "1100), (10.6, 11.4, 10.5, 11.3, 1600), (11.3, 11.5, 10.9, 11, 800), (11, 11.6, 10.8, 11.5, 1300),",
    "(11.5, 11.9, 11.1, 11.2, 1000), (11.2, 11.4, 10.7, 10.9, 1400).",
    "",
    "RSI smoothing modes: RSI(5, avg 3, sig 3, ctx 8) over one series with rising, flat, falling and",
    "alternating closes. The up/down averages are Wilder's (alpha 1/N, SMA-seeded) or exponential",
    "(alpha 2/(N+1), seeded with the first change); both publish from the N-th change on.",
    "",
    "Bollinger variance conventions (package 18): Bollinger(5, 2) over closes near 10000 with",
    "cent-sized fluctuations, where a variance shortcut would lose the signal in rounding; computed",
    "exactly from basis = SMA(N) and sum((x - basis)^2) / divisor, divisor N resp. N - 1, on the",
    "exact binary values of the closes. sqrt(20/19): the factor between the two band distances at",
    "N = 20.",
    "",
    "TRIX (package 22), len 4, signal 3, over the 20 closes in the test; on x_t = x_0 * 1.01^t TRIX",
    "tends to 100 (1.01 - 1) = 1. Rank Correlation Index (33), len 5, average ranks for ties.",
    "Stochastic Momentum Index (32), len 5, smoothing 3 and 3, signal 3, over close = 100 + 6",
    "sin(i/2), high/low = close +/- 1; on a straight line exactly 200 * 2/6. BBTrend (36), short 3,",
    "long 6, mult 2, population variance. Price Momentum Oscillator (38), 5, 3, signal 3, over a",
    "sawtooth 100 (1 + 0.01 ((i mod 7) - 3)); at constant 1 % growth it tends to 10.",
    "",
    "The *_tolerance keys are the comparison tolerances the tests apply, stated rather than derived.",
]

SECTIONS = [
    ("base values",
     ["rsi14_last", "rsi14_tolerance", "macd_line_last", "macd_signal_last", "macd_hist_last",
      "macd_tolerance", "bollinger5_basis", "bollinger5_upper", "bollinger5_lower",
      "bollinger5_bandwidth", "bollinger5_percent_b", "cci5_last", "stoch5_k", "stoch5_d",
      "stoch_rsi5_k", "stoch_rsi5_d", "mfi5_last", "williams_r5_last", "tsi5_line", "tsi5_signal",
      "fisher5_line", "fisher5_signal", "ao3_5_last", "bop5_last", "chaikin_osc_last", "cmo5_last",
      "connors_rsi_last", "coppock_60bar_last", "dpo5_last", "elder_bull_last", "elder_bear_last",
      "kst60_line", "kst60_signal", "ppo_line", "ppo_signal", "roc5_last", "rvi_line", "rvi_signal",
      "uo_last", "wt1_last", "wt2_last", "osc_tolerance"]),
    ("BOP, Chaikin and RVI where the formulas actually act",
     ["bop5_shaped_last", "chaikin_osc_shaped_last", "rvi_shaped_line", "rvi_shaped_signal"]),
    ("RSI smoothing modes",
     [f"rsi5_mixed_{m}_{f}" for m in ("wilder", "ema") for f in ("line", "signal", "ctx")]
     + ["rsi5_mixed_tolerance"]),
    ("Bollinger variance conventions (package 18)",
     [f"bollinger5_{c}_{f}" for c in ("population", "sample")
      for f in ("basis", "upper", "lower", "bandwidth", "percent_b")]
     + ["bollinger5_band_tolerance", "bollinger5_ratio_tolerance",
        "bollinger20_sample_over_population_distance"]),
    ("TRIX (package 22)",
     ["trix4_signal3_output_count", "trix4_signal3_line_first", "trix4_signal3_line_last",
      "trix4_signal3_signal_last", "trix4_signal3_tolerance", "trix_geometric_growth_percent",
      "trix_geometric_tolerance"]),
    ("Rank Correlation Index (package 33)",
     ["rci5_rising", "rci5_falling", "rci5_mixed", "rci5_with_ties", "rci5_series_first",
      "rci5_series_last", "rci5_series_output_count", "rci_tolerance"]),
    ("Stochastic Momentum Index (package 32)",
     ["smi5_first", "smi5_last", "smi5_signal_last", "smi5_output_count", "smi5_linear_ramp",
      "smi_tolerance"]),
    ("BBTrend (package 36)",
     ["bbtrend_3_6_first", "bbtrend_3_6_first_upper_gap", "bbtrend_3_6_first_lower_gap",
      "bbtrend_3_6_last", "bbtrend_3_6_output_count", "bbtrend_tolerance"]),
    ("Price Momentum Oscillator (package 38)",
     ["pmo_5_3_first", "pmo_5_3_last", "pmo_5_3_signal_last", "pmo_5_3_output_count",
      "pmo_constant_growth", "pmo_tolerance"]),
]


def build():
    values = {**derive(), **TOLERANCES}
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in SECTIONS]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
