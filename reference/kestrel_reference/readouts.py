"""Reference formulas of the read-out indicators (plan 141): each written from the definition
documented beside the Rust implementation, over a whole series with plain list slicing, never
from the Rust code. Bars are (open, high, low, close, volume); every function returns one value
(or None) per bar.
"""

import datetime
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


_EPOCH_ORDINAL = datetime.date(1970, 1, 1).toordinal()
_UNIT_SECONDS = {"second": 1, "minute": 60, "hour": 3600, "day": 86400}


def period_start(ts, unit, count, offset):
    """Start of the period of `count` units that holds the instant `ts`, in UTC seconds.

    The periods are cut in local time `ts + offset`. Seconds up to days align to the epoch
    (multiples of count * unit). Weeks start on Mondays; a period of several weeks counts them
    from the Monday of 29 December 1969. Months align to the calendar: the month index
    `12 * year + month - 1` is cut into multiples of `count`.
    """
    local = ts + offset
    if unit in _UNIT_SECONDS:
        width = _UNIT_SECONDS[unit] * count
        start = local // width * width
    elif unit == "week":
        day = local // 86400
        monday = day - datetime.date.fromordinal(_EPOCH_ORDINAL + day).weekday()
        weeks_since_anchor = (monday + 3) // 7
        start = (weeks_since_anchor // count * count * 7 - 3) * 86400
    else:
        day = local // 86400
        date = datetime.date.fromordinal(_EPOCH_ORDINAL + day)
        index = 12 * date.year + date.month - 1
        index = index // count * count
        first = datetime.date(index // 12, index % 12 + 1, 1)
        start = (first.toordinal() - _EPOCH_ORDINAL) * 86400
    return start - offset


def period_midpoint(bars, unit, count, offset, step=900):
    """Per bar: None, or (midpoint of the last completed period, 1.0 on the bar that completed
    it else 0.0). `bars` are (open, high, low, close, volume) with timestamps step, 2 * step, ...;
    or (timestamp, high, low) triples when `step` is None. A period completes on the first bar of
    a different period; the midpoint is (highest high + lowest low) / 2 of its bars."""
    out = []
    current = high = low = last = None
    for k, bar in enumerate(bars):
        if step is None:
            ts, h, l = bar
        else:
            ts, h, l = step * (k + 1), bar[1], bar[2]
        start = period_start(ts, unit, count, offset)
        new = 0.0
        if current is None:
            current, high, low = start, h, l
        elif start != current:
            last = (high + low) / 2.0
            new = 1.0
            current, high, low = start, h, l
        else:
            high, low = max(high, h), min(low, l)
        out.append(None if last is None else (last, new))
    return out


def opening_range(bars, start_hour, start_minute, end_hour, end_minute, orb_minutes, offset,
                  step=900):
    """Per bar: None, or (width of the opening range, high, low, in window, new session).

    A bar belongs to the session whose start lies at or before its local time `ts + offset`
    (start == end: the session is the whole day; start < end: from start to end, end exclusive;
    start > end: overnight, from start to end of the next day). The session opens at its start
    (UTC: start - offset). The range is the highest high and lowest low of the bars whose open
    time lies within `orb_minutes` of the session open; a new session clears the range; a bar
    outside every session leaves it as it is and has no output. Without a bar inside the window
    (the first bar of the session arrived late) there is no range and no output.
    """
    start = start_hour * 3600 + start_minute * 60
    end = end_hour * 3600 + end_minute * 60
    session_open = high = low = None
    out = []
    for k, bar in enumerate(bars):
        ts = step * (k + 1)
        local = ts + offset
        day, second = local // 86400, local % 86400
        if start == end:
            session_day = day
        elif start < end:
            session_day = day if start <= second < end else None
        elif second >= start:
            session_day = day
        elif second < end:
            session_day = day - 1
        else:
            session_day = None
        if session_day is None:
            out.append(None)
            continue
        opened = session_day * 86400 + start - offset
        new = opened != session_open
        if new:
            session_open, high, low = opened, None, None
        elapsed = ts - opened
        in_window = 0 <= elapsed < orb_minutes * 60
        if in_window:
            high = bar[1] if high is None else max(high, bar[1])
            low = bar[2] if low is None else min(low, bar[2])
        if high is None:
            out.append(None)
        else:
            out.append((high - low, high, low, 1.0 if in_window else 0.0, 1.0 if new else 0.0))
    return out
