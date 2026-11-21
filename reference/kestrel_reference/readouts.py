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


def _classify_slope(slope_atr, flat, strong):
    if slope_atr >= strong:
        return 2.0
    if slope_atr > 0.0 and slope_atr >= flat:
        return 1.0
    if slope_atr <= -strong:
        return -2.0
    if slope_atr < 0.0 and slope_atr <= -flat:
        return -1.0
    return 0.0


def vwap_regime(rows, closes, atr, regime_window, flat, strong):
    """VWAP regime reading per bar.

    `rows[i]` is None (no VWAP: no volume in the window) or (vwap, sigma, slope), the slope None
    until it exists; `atr[i]` the Wilder ATR or None. A bar needs a VWAP, its slope and a positive
    ATR to be read; other bars do not enter the window. diff = close - vwap; the window holds the
    last `regime_window` diffs. The side of a diff is +1, -1 or 0 (exactly on the VWAP). Reading:
    distance_atr = diff / atr and slope_atr = slope / atr, z_score = diff / sigma (None for
    sigma <= 0, the key is absent), slope_state the code -2..2 of slope_atr against `flat` and
    `strong` (a slope of 0 is always flat), cross_frequency = (non-zero sides differing from the
    last non-zero side before them) / window length, price_persistence = (diffs on the larger
    side) / window length, diffs on the VWAP on neither side. Returns None or the tuple
    (distance_atr, vwap, slope_atr, slope_state, z_score, cross_frequency, price_persistence).
    """
    out, diffs = [], []
    for i, row in enumerate(rows):
        if row is None or row[2] is None or atr[i] is None or atr[i] <= 0.0:
            out.append(None)
            continue
        vwap, sigma, slope = row
        diff = closes[i] - vwap
        diffs = (diffs + [diff])[-regime_window:]
        n = len(diffs)
        sides = [(d > 0.0) - (d < 0.0) for d in diffs]
        crosses, last = 0, 0
        for side in sides:
            if side != 0:
                if last != 0 and side != last:
                    crosses += 1
                last = side
        above, below = sides.count(1), sides.count(-1)
        slope_atr = slope / atr[i]
        out.append((diff / atr[i], vwap, slope_atr,
                    _classify_slope(slope_atr, flat, strong),
                    diff / sigma if sigma > 0.0 else None, crosses / n,
                    max(above, below) / n))
    return out


def _wilder_atr(bars, n):
    """Wilder ATR per bar, None before bar n: the mean of the first n true ranges, then
    (ATR * (n - 1) + TR) / n. The true range of the first bar is high - low."""
    out, atr, seed = [], None, []
    for i, b in enumerate(bars):
        if i == 0:
            tr = b[1] - b[2]
        else:
            pc = bars[i - 1][3]
            tr = max(b[1] - b[2], abs(b[1] - pc), abs(b[2] - pc))
        if atr is None:
            seed.append(tr)
            if len(seed) == n:
                atr = sum(seed) / n
        else:
            atr = (atr * (n - 1) + tr) / n
        out.append(atr)
    return out


def _median(values):
    ordered = sorted(values)
    mid = len(ordered) // 2
    if len(ordered) % 2 == 0:
        return (ordered[mid - 1] + ordered[mid]) / 2.0
    return ordered[mid]


def swing_structure(bars, left, right, max_swings, atr_len):
    """Statistical swing structure per bar: None or a dict of the reading.

    A bar is a pivot high (low) when its high (low) is strictly above (below) the highs (lows)
    of the `left` bars before and `right` bars after it; it is known `right` bars later, and only
    bars that have a Wilder ATR of `atr_len` > 0 look for one. The pivots alternate (high, low,
    high, ...): a pivot of the same kind as the newest one continues that swing, the more extreme
    of the two stays (the higher high, the lower low; on equal prices the earlier one). A bar that
    is both pivots (an outside bar) goes in so that the alternation continues, i.e. the high
    first after a pivot low and the low first after a pivot high; with no pivot before it, the
    high first when the bar closed below its open, the low first when it closed above, and
    neither when it closed at its open. Only the newest `2 * max_swings + 4` pivots are kept. The legs between
    neighbouring pivots are |price change| / ATR (the ATR of the current bar) with direction and
    span in bars (at least 1); only the newest `max_swings` legs count. With at least 4 pivots:
    trend up when the last two pivot highs and the last two pivot lows both rose, down when both
    fell, else the direction of the last leg. Impulses are the legs along the trend, corrections
    the others; two of each are needed. median_* are the medians (mean of the middle two for an
    even count), correction_impulse_ratio = median_correction / median_impulse (0 for a median
    impulse of 0). recent_corrections are the newest three corrections, newest first;
    trend_quality compares the newest of them with the oldest of them (needs two): smaller is
    strengthening (1), larger weakening (-1), equal or fewer than two stable (0). The
    retracement is |close - last pivot price| / ATR; entry_zone_reached when it is at least
    0.8 * median_correction; remaining_potential = max(median_impulse - retracement, 0);
    required_stop = max(median_correction, 0.1); potential_crv = remaining_potential /
    required_stop; last_impulse_velocity = size / span of the newest impulse leg.
    """
    atr_series = _wilder_atr(bars, atr_len)
    out = []
    pivots = []  # (bar index, price, is_high)
    for t, bar in enumerate(bars):
        atr = atr_series[t] or 0.0
        if atr <= 0.0:
            out.append(None)
            continue
        c = t - right
        if c - left >= 0:
            window = bars[c - left:c + right + 1]
            others = [b for k, b in enumerate(window) if k != left]
            is_high = all(bars[c][1] > b[1] for b in others)
            is_low = all(bars[c][2] < b[2] for b in others)
            found = []
            if is_high and is_low:
                if pivots:
                    high_first = not pivots[-1][2]
                elif bars[c][0] == bars[c][3]:
                    high_first = None
                else:
                    high_first = bars[c][3] < bars[c][0]
                if high_first is not None:
                    found = [(c, bars[c][1], True), (c, bars[c][2], False)]
                    if not high_first:
                        found.reverse()
            elif is_high:
                found = [(c, bars[c][1], True)]
            elif is_low:
                found = [(c, bars[c][2], False)]
            for pivot in found:
                if pivots and pivots[-1][2] == pivot[2]:
                    if (pivot[1] > pivots[-1][1]) if pivot[2] else (pivot[1] < pivots[-1][1]):
                        pivots[-1] = pivot
                else:
                    pivots.append(pivot)
                pivots = pivots[-(max_swings * 2 + 4):]
        if len(pivots) < 4:
            out.append(None)
            continue
        legs = []
        for (ia, pa, _), (ib, pb, hb) in zip(pivots, pivots[1:]):
            legs.append((abs(pb - pa) / atr, hb, max(ib - ia, 1)))
        legs = legs[-max_swings:]
        highs = [p for _, p, h in pivots if h]
        lows = [p for _, p, h in pivots if not h]
        rising = len(highs) >= 2 and len(lows) >= 2 and highs[-1] > highs[-2] and lows[-1] > lows[-2]
        falling = len(highs) >= 2 and len(lows) >= 2 and highs[-1] < highs[-2] and lows[-1] < lows[-2]
        trend_up = rising or (not falling and (legs[-1][1] if legs else True))
        impulses = [(size, span) for size, up, span in legs if up == trend_up]
        corrections = [size for size, up, _ in legs if up != trend_up]
        if len(impulses) < 2 or len(corrections) < 2:
            out.append(None)
            continue
        med_impulse = _median([size for size, _ in impulses])
        med_correction = _median(corrections)
        recent = corrections[::-1][:3]
        if len(recent) >= 2 and recent[0] < recent[-1]:
            quality = 1.0
        elif len(recent) >= 2 and recent[0] > recent[-1]:
            quality = -1.0
        else:
            quality = 0.0
        retracement = abs(bar[3] - pivots[-1][1]) / atr
        required_stop = max(med_correction, 0.1)
        remaining = max(med_impulse - retracement, 0.0)
        reading = {
            "median_impulse_atr": med_impulse,
            "median_correction_atr": med_correction,
            "current_retracement_atr": retracement,
            "entry_zone_reached": 1.0 if retracement >= med_correction * 0.8 else 0.0,
            "remaining_potential_atr": remaining,
            "required_stop_atr": required_stop,
            "potential_crv": remaining / required_stop,
            "last_impulse_velocity_atr": impulses[-1][0] / impulses[-1][1],
            "trend_quality": quality,
        }
        for k, value in enumerate(recent, start=1):
            reading[f"recent_correction_atr_{k}"] = value
        out.append((med_correction / med_impulse if med_impulse > 0.0 else 0.0, reading))
    return out


def fvg_fill(bars, max_zones, step=900):
    """Fair value gaps followed as zones; per bar (open zones, created, filled, open bullish,
    open bearish).

    From the third bar on: a bullish gap when the low of bar t lies above the high of bar t-2
    (the zone runs from that high up to that low), else a bearish gap when the high of t lies
    below the low of t-2 (the zone runs from the high of t up to the low of t-2). A zone is filled
    by the first bar after its formation bar (a later timestamp) that trades back into it: a bullish
    zone when the bar's low is at or below the zone's upper edge, a bearish zone when the bar's
    high is at or above its lower edge. `created` is 1 (bullish), -1 (bearish) or 0, `filled` the
    zones filled on this bar. Filled zones are dropped, and of the open ones only the newest
    `max_zones` are kept; the open counts are those that remain.
    """
    zones = []  # [bullish, top, bottom, formed timestamp]
    out = []
    for i, bar in enumerate(bars):
        ts, high, low = step * (i + 1), bar[1], bar[2]
        created = 0.0
        if i >= 2:
            if low > bars[i - 2][1]:
                created = 1.0
                zones.append([True, low, bars[i - 2][1], ts])
            elif high < bars[i - 2][2]:
                created = -1.0
                zones.append([False, bars[i - 2][2], high, ts])
        filled = 0
        for zone in zones:
            if ts <= zone[3]:
                continue
            if (zone[0] and low <= zone[1]) or (not zone[0] and high >= zone[2]):
                zone.append("filled")
                filled += 1
        zones = [z for z in zones if len(z) == 4][-max_zones:]
        open_bullish = sum(1 for z in zones if z[0])
        open_bearish = len(zones) - open_bullish
        out.append((float(len(zones)), created, float(filled), float(open_bullish),
                    float(open_bearish)))
    return out
