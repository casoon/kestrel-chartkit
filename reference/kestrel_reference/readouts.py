"""Reference formulas of the read-out indicators (plan 141): each written from the definition
documented beside the Rust implementation, over a whole series with plain list slicing, never
from the Rust code. Bars are (open, high, low, close, volume); every function returns one value
(or None) per bar.
"""

import math


def fractal_dimension(bars, n, method):
    """Fractal dimension of the trailing window, one value per bar.

    `halves` (Ehlers): half = max(n // 2, 2); r1 is the high-low range of the newest `half` bars,
    r2 that of the `half` bars before them, r3 that of the newest `n` bars, each divided by its
    bar count; D = (ln(r1 + r2) - ln(r3)) / ln 2, clamped to 1..2, 1.5 when r1 + r2 or r3 is not
    positive. First value on bar max(n, 2 * half).

    `path`: D = 1 + ln(path / range) / ln n, path the sum of the n absolute close-to-close moves
    and range the high-low range of the same n + 1 bars, unclamped, 1.5 when the range or the
    path is not positive. First value on bar n + 1.
    """
    out = []
    for i in range(len(bars)):
        if method == "halves":
            half = max(n // 2, 2)
            need = max(n, 2 * half)
            if i + 1 < need:
                out.append(None)
                continue
            newest = bars[i + 1 - half:i + 1]
            before = bars[i + 1 - 2 * half:i + 1 - half]
            whole = bars[i + 1 - n:i + 1]
            r1 = (max(b[1] for b in newest) - min(b[2] for b in newest)) / half
            r2 = (max(b[1] for b in before) - min(b[2] for b in before)) / half
            r3 = (max(b[1] for b in whole) - min(b[2] for b in whole)) / n
            if r1 + r2 > 0.0 and r3 > 0.0:
                d = (math.log(r1 + r2) - math.log(r3)) / math.log(2.0)
            else:
                d = 1.5
            out.append(min(max(d, 1.0), 2.0))
        else:
            if i < n:
                out.append(None)
                continue
            window = bars[i - n:i + 1]
            rng = max(b[1] for b in window) - min(b[2] for b in window)
            path = 0.0
            for k in range(1, len(window)):
                path += abs(window[k][3] - window[k - 1][3])
            if rng > 0.0 and path > 0.0:
                out.append(math.log(path / rng) / math.log(n) + 1.0)
            else:
                out.append(1.5)
    return out


def r_squared(closes, n):
    """R^2 of the least-squares line through the last n closes against 0..n-1: 1 - SS_res / SS_tot
    limited to 0..1, and 1 when the closes do not vary (no variation to explain). First value on
    bar n."""
    out = []
    xs = list(range(n))
    mean_x = sum(xs) / n
    for i in range(len(closes)):
        if i + 1 < n:
            out.append(None)
            continue
        ys = closes[i + 1 - n:i + 1]
        mean_y = sum(ys) / n
        sxx = sum((x - mean_x) ** 2 for x in xs)
        sxy = sum((x - mean_x) * (y - mean_y) for x, y in zip(xs, ys))
        ss_tot = sum((y - mean_y) ** 2 for y in ys)
        slope = sxy / sxx
        intercept = mean_y - slope * mean_x
        ss_res = sum((y - (intercept + slope * x)) ** 2 for x, y in zip(xs, ys))
        out.append(min(max(1.0 - ss_res / ss_tot, 0.0), 1.0) if ss_tot > 0.0 else 1.0)
    return out


def percent_rank(closes, n):
    """100 * (number of the n previous closes at or below the current close) / n. First value on
    bar n + 1."""
    out = []
    for i in range(len(closes)):
        if i < n:
            out.append(None)
            continue
        out.append(100.0 * sum(1 for p in closes[i - n:i] if p <= closes[i]) / n)
    return out


def cycle_length(closes, period, history):
    """Zero-crossing swing length of close - SMA(close, period).

    The detrended series starts on bar `period`. Its sign is +1, -1 or 0; a bar crosses when the
    last non-zero sign was the opposite of its own non-zero sign. A crossing bar is the first bar
    of the new swing: the swing that ends has lasted `since` bars (the bars counted since the
    swing began, the series' first bar starting the first swing). Returns, per bar, None or
    (mean of the last `history` completed swing lengths, bars since the crossing of the swing in
    progress, crossing bar included); None until a swing has completed.
    """
    out = [None] * len(closes)
    last_sign = None
    since = 0
    lengths = []
    for i in range(period - 1, len(closes)):
        detrended = closes[i] - sum(closes[i + 1 - period:i + 1]) / period
        sign = (detrended > 0.0) - (detrended < 0.0)
        if last_sign is not None and last_sign != 0 and sign != 0 and sign != last_sign:
            lengths.append(since)
            lengths = lengths[-history:]
            since = 0
        since += 1
        if sign != 0:
            last_sign = sign
        if lengths:
            out[i] = (sum(lengths) / len(lengths), float(since))
    return out
