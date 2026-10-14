"""contract_structure: complete per-bar contracts of the structure family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the definition documented
on the indicator's type (doc comment, or the comments describing each rule where the type has no
summary), written here as a second implementation; structure detectors run hand-built scenarios
whose decisive bars are named in the comments. Where a definition leaves a convention open, the
convention assumed is stated next to the model ("Convention:").

Common conventions:
- Bars are (open, high, low, close, volume); indices in comments are 1-based like the fixture.
- True range of the first bar is high - low, afterwards max(high - low, |high - prev close|,
  |low - prev close|). "ATR(n)" is Wilder's average of it, seeded with the mean of the first n
  true ranges, so it exists from bar n on.
- Alert strengths are the fixed per-kind strengths the types declare with their alert kinds.
- A pivot "strict" is above (below) every other bar of its window, "non-strict" no other bar is
  above (below) it, ties allowed.
"""

import math
import statistics

from ..fixture import provenance
from ..indicators import rsi_lines
from ..smoothing import rma
from ..structure import advanced_zigzag, liquidity_pools, pivot_structure_scores
from .contract_pilot import case_block

NAME = "contract_structure"

# Relative to max(1, |expected|). Every model repeats the documented arithmetic on the same
# inputs, so differences are rounding only.
TOLERANCE = 1e-9


# --- Bars ------------------------------------------------------------------------------------

def path(start, segments):
    """Closes from `start`, then for each (step, count) `count` bars moving by `step`."""
    closes = [start]
    for step, count in segments:
        for _ in range(count):
            closes.append(round(closes[-1] + step, 6))
    return closes


def candles(closes, up=0.5, down=0.7, volume=1000.0):
    """Bars following `closes`: the open halfway from the previous close (the first a quarter
    below its close), the high `up` above and the low `down` below the body. Highs and lows are
    then strictly monotone wherever the closes are, so every turn of the closes is one pivot bar,
    and the asymmetric wicks make a high read as a low (or the reverse) show."""
    bars, previous = [], closes[0] - 0.25
    for close in closes:
        open_ = round((previous + close) / 2.0, 6)
        bars.append((open_, round(max(open_, close) + up, 6), round(min(open_, close) - down, 6),
                     close, volume))
        previous = close
    return bars


def replace(bars, overrides):
    """`bars` with the 1-based bars in `overrides` replaced by the given (open, high, low, close)
    keeping the volume."""
    out = list(bars)
    for index, (o, h, l, c) in overrides.items():
        out[index - 1] = (o, h, l, c, out[index - 1][4])
    return out


def true_ranges(bars):
    out, prev = [], None
    for _, h, l, c, _ in bars:
        out.append(h - l if prev is None else max(h - l, abs(h - prev), abs(l - prev)))
        prev = c
    return out


def wilder_atr(bars, length):
    return rma(true_ranges(bars), length)


def strict_high(window, mid):
    return all(b[1] < window[mid][1] for i, b in enumerate(window) if i != mid)


def strict_low(window, mid):
    return all(b[2] > window[mid][2] for i, b in enumerate(window) if i != mid)


def loose_high(window, mid):
    return all(b[1] <= window[mid][1] for i, b in enumerate(window) if i != mid)


def loose_low(window, mid):
    return all(b[2] >= window[mid][2] for i, b in enumerate(window) if i != mid)


def no_alerts(n):
    return [(None, {}) for _ in range(n)]


# --- liquidity_fvg ---------------------------------------------------------------------------

def liquidity_fvg(lookback, bars):
    """Fair value gap per the rule comments: bullish when the low of bar t lies above the high of
    bar t-2 (`fvg_type` 1, `gap_size` = low - that high, alert `bullish_fvg`), else bearish when
    the high lies below the low of bar t-2 (-1, that low - high, `bearish_fvg`); `value` =
    `fvg_type`. Liquidity sweep over the `lookback` bars before the current one: the low below
    their lowest low and the close back above it is `bullish_liquidity_sweep`, else the high above
    their highest high and the close back below it `bearish_liquidity_sweep`.
    Convention: the first two bars output 0 without extras (no bar t-2 yet); a sweep needs the
    full `lookback` bars before the current one. Strengths 0.85 (gap), 0.95 (sweep)."""
    expected = []
    for i, (_, high, low, close, _) in enumerate(bars):
        if i < 2:
            expected.append(((0.0, {}), {}))
            continue
        alerts = {}
        kind, gap = 0.0, 0.0
        if low > bars[i - 2][1]:
            kind, gap = 1.0, low - bars[i - 2][1]
            alerts["bullish_fvg"] = 0.85
        elif high < bars[i - 2][2]:
            kind, gap = -1.0, bars[i - 2][2] - high
            alerts["bearish_fvg"] = 0.85
        if i >= lookback:
            before = bars[i - lookback:i]
            lowest, highest = min(b[2] for b in before), max(b[1] for b in before)
            if low < lowest and close > lowest:
                alerts["bullish_liquidity_sweep"] = 0.95
            elif high > highest and close < highest:
                alerts["bearish_liquidity_sweep"] = 0.95
        expected.append(((kind, {"fvg_type": kind, "gap_size": gap}), alerts))
    return {"lookback": float(lookback)}, bars, expected


def fvg_bars():
    """Quiet drift (steps 0.4: no gap), a rally of steps 2 (each bar's low above the high two bars
    back: bullish gaps), a drop of steps -2 (bearish gaps), a quiet base; bar 34 wicks below the
    lowest low of the 20 bars before it and closes back inside, bar 38 wicks above the highest
    high of its 20 predecessors (bar 18, early in the drop) and closes back below it. With
    lookback 3 the same two bars sweep only the last three bars."""
    closes = path(100.0, [(0.4, 7), (2.0, 6), (-2.0, 8), (0.1, 6), (-0.1, 6), (0.1, 4)])
    bars = candles(closes)
    return replace(bars, {34: (98.1, 98.8, 90.0, 98.4), 38: (98.6, 114.0, 98.0, 98.7)})


def fvg_touch_bars():
    """Bar 3's low equals bar 1's high and bar 6's high equals bar 4's low: touching is no gap
    (the rule is strict). Bar 4 runs above the high of bars 2-3 (lookback 2) and closes back below
    it, bar 5 undercuts the low of bars 3-4 and closes back above it: one sweep each way."""
    return [(100.0, 101.0, 99.0, 100.5, 1000.0), (100.5, 102.0, 100.4, 101.8, 1000.0),
            (101.8, 103.0, 101.0, 102.8, 1000.0), (102.8, 103.2, 101.5, 102.0, 1000.0),
            (102.0, 102.5, 100.9, 101.2, 1000.0), (101.2, 101.5, 100.0, 100.2, 1000.0)]


# --- market_structure_breaks -----------------------------------------------------------------

def market_structure_breaks(lookback, bars):
    """Swing points are strict pivots with `lookback` bars on each side, known `lookback` bars
    late; each kind keeps its latest one until a close breaks it. A close above the swing high is
    a break of structure (`bullish_bos`, value 1) in an uptrend, otherwise a change of character
    (`bullish_choch`, value 2) that turns the trend up; a close below the swing low mirrors it
    (`bearish_bos` -1, `bearish_choch` -2, trend down). The broken swing is consumed. Trend starts
    neutral; value 0 without a break.
    Convention: the pivot confirmed on a bar is in place before that bar's close is tested; the
    high side is tested before the low side, and the value is the last break of the bar.
    Strengths 0.90 (BOS), 0.95 (CHoCH)."""
    expected, swing_high, swing_low, trend = [], None, None, 0
    for i, bar in enumerate(bars):
        if i < 2 * lookback:
            expected.append((None, {}))
            continue
        window = bars[i - 2 * lookback:i + 1]
        if strict_high(window, lookback):
            swing_high = window[lookback][1]
        if strict_low(window, lookback):
            swing_low = window[lookback][2]
        close, value, alerts = bar[3], 0.0, {}
        if swing_high is not None and close > swing_high:
            if trend == 1:
                alerts["bullish_bos"], value = 0.90, 1.0
            else:
                trend, value = 1, 2.0
                alerts["bullish_choch"] = 0.95
            swing_high = None
        if swing_low is not None and close < swing_low:
            if trend == -1:
                alerts["bearish_bos"], value = 0.90, -1.0
            else:
                trend, value = -1, -2.0
                alerts["bearish_choch"] = 0.95
            swing_low = None
        expected.append(((value, {}), alerts))
    return {"lookback": float(lookback)}, bars, expected


def structure_wave():
    """A staircase: five rising swings (up 7 bars by 1, down 6 by 0.5: each swing a higher high
    and a higher low), six falling swings (mirrored: lower lows and lower highs) and three rising
    swings again. Every turn is a pivot for windows of up to six bars each side."""
    closes = path(100.0, [(1.0, 7), (-0.5, 6)] * 5 + [(-1.0, 7), (0.5, 6)] * 6
                  + [(1.0, 7), (-0.5, 6)] * 3)
    return candles(closes)


def equal_highs_bars():
    """Bars 3 and 4 share the high 103: no strict pivot high (market_structure_breaks keeps no
    swing high, bar 8's close above 103 breaks nothing), but a non-strict one (bos_choch: bar 8
    is a bullish CHoCH). Bar 6's low 99.8 is a pivot low either way, confirmed on bar 8; bar 9
    closes below it."""
    return [(100.0, 100.5, 99.5, 100.2, 1000.0), (100.2, 101.5, 100.0, 101.2, 1000.0),
            (101.2, 103.0, 101.0, 102.5, 1000.0), (102.5, 103.0, 101.8, 102.2, 1000.0),
            (102.2, 102.4, 100.5, 100.8, 1000.0), (100.8, 101.0, 99.8, 100.1, 1000.0),
            (100.1, 101.2, 99.9, 101.0, 1000.0), (101.0, 103.5, 100.9, 103.4, 1000.0),
            (103.4, 103.6, 99.0, 99.5, 1000.0), (99.5, 100.0, 98.8, 99.2, 1000.0)]


# --- pivots_structure ------------------------------------------------------------------------

def pivots_structure(left, right, window, bars):
    """Structure score from `pivot_structure_scores` (the documented rule: strict pivots,
    +2/-1 per high and +1/-2 per low against the previous pivot of the kind, 100 sum /
    (2 score_window), clamped); alerts `structure_bullish_bias` at value >= 50 and
    `structure_bearish_bias` at value <= -50, strength 0.85."""
    scores = pivot_structure_scores([b[:4] for b in bars], left, right, window)
    first = left + right
    expected = no_alerts(first)
    for value in scores:
        alerts = {}
        if value >= 50.0:
            alerts["structure_bullish_bias"] = 0.85
        elif value <= -50.0:
            alerts["structure_bearish_bias"] = 0.85
        expected.append(((value, {}), alerts))
    params = {"left_bars": float(left), "right_bars": float(right), "score_window": float(window)}
    return params, bars, expected


# --- zigzag ----------------------------------------------------------------------------------

def zigzag(depth, deviation_pct, bars):
    """ZigZag swing legs. Over the last 2 depth + 1 bars the middle bar is a pivot high (low) when
    no other high is above (low below) its own. A pivot high while the leg is down or not yet set
    starts an up leg at its high if it lies at least `deviation_pct` percent above the last pivot
    price (always for the first pivot); while the leg is up, a higher pivot high moves the leg's
    extreme there. Pivot lows mirror this. `value` = `last_pivot_price` = the current leg's
    extreme (0 before any pivot), `direction` 1 up, -1 down, 0 before any pivot. No alerts.
    Convention: the high side is applied before the low side on the same bar."""
    expected, direction, last = [], 0, 0.0
    threshold = deviation_pct / 100.0
    size = 2 * depth + 1
    for i in range(len(bars)):
        if i + 1 < size:
            expected.append((None, {}))
            continue
        window = bars[i + 1 - size:i + 1]
        mid = window[depth]
        if loose_high(window, depth):
            if direction <= 0:
                change = (mid[1] - last) / last if last > 0.0 else 1.0
                if change >= threshold or direction == 0:
                    direction, last = 1, mid[1]
            elif mid[1] > last:
                last = mid[1]
        if loose_low(window, depth):
            if direction >= 0:
                change = (last - mid[2]) / last if last > 0.0 else 1.0
                if change >= threshold or direction == 0:
                    direction, last = -1, mid[2]
            elif mid[2] < last:
                last = mid[2]
        expected.append(((last, {"direction": float(direction), "last_pivot_price": last}), {}))
    return {"depth": float(depth), "deviation_pct": deviation_pct}, bars, expected


def zigzag_bars():
    """Swings of about 14 and 17 points with a 2.4-point wiggle inside the second up leg: at 5 %
    the wiggle does not turn the leg, at 1.5 % it does."""
    closes = path(100.0, [(1.0, 14), (-1.0, 9), (0.8, 3), (-0.8, 3), (1.2, 14), (-1.0, 14),
                          (0.5, 18)])
    return candles(closes)


# --- pivot_sets ------------------------------------------------------------------------------

DAY = 86400
BAR_SECONDS = 900  # the case loader's spacing: bar i has timestamp i * 900


def pivot_sets(bars):
    """Classic floor pivots of the previous UTC day: P = (H + L + C) / 3, R1 = 2P - L,
    S1 = 2P - H, R2 = P + (H - L), S2 = P - (H - L), R3 = H + 2 (P - L), S3 = L - 2 (H - P),
    `value` = P. Within the first day, before any finished day, the running day's high and low
    with the current close stand in. No alerts.
    Convention: the day of bar i is the UTC day of its timestamp i * 900 s."""
    expected, previous, running, current_day = [], None, None, None
    for i, (o, h, l, c, _) in enumerate(bars, start=1):
        day = (i * BAR_SECONDS) // DAY
        if current_day is not None and day != current_day:
            previous, running = running, None
        current_day = day
        if running is None:
            running = {"h": h, "l": l, "c": c}
        else:
            running = {"h": max(running["h"], h), "l": min(running["l"], l), "c": c}
        if previous is not None:
            hh, ll, cc = previous["h"], previous["l"], previous["c"]
        else:
            hh, ll, cc = running["h"], running["l"], c
        p = (hh + ll + cc) / 3.0
        extra = {"p": p, "r1": 2.0 * p - ll, "s1": 2.0 * p - hh, "r2": p + (hh - ll),
                 "s2": p - (hh - ll), "r3": hh + 2.0 * (p - ll), "s3": ll - 2.0 * (hh - p)}
        expected.append(((p, extra), {}))
    return {}, bars, expected


def pivot_sets_bars():
    """Two and a half days of 15-minute bars (bar 96 opens the second day, bar 192 the third),
    each day with its own range."""
    closes = path(100.0, [(0.25, 40), (-0.2, 55), (0.5, 30), (-0.3, 65), (0.1, 49)])
    return candles(closes)


# --- bos_choch -------------------------------------------------------------------------------

def bos_choch(pivot_len, bars):
    """BOS/CHoCH engine: non-strict pivots over the last 2 pivot_len + 1 bars (the middle bar);
    the latest pivot high/low is the level. A close above the pivot high is a bullish CHoCH
    (value 2, trend turns 1) unless the trend is already 1, then a bullish BOS (value 1); a close
    below the pivot low mirrors it (-2 / -1, trend -1). The broken level is reset until the next
    pivot. `extra`: `trend` (0 before the first break) and `event_code` (= value). One alert per
    bar for the bar's event, strength 0.9: `bull_choch`, `bull_bos`, `bear_choch`, `bear_bos`.
    Convention: the high side is tested before the low side; if both break on one bar the later
    (bearish) event is the bar's event."""
    expected, ph, pl, trend = [], None, None, 0
    size = 2 * pivot_len + 1
    names = {2.0: "bull_choch", 1.0: "bull_bos", -2.0: "bear_choch", -1.0: "bear_bos"}
    for i, bar in enumerate(bars):
        if i + 1 < size:
            expected.append((None, {}))
            continue
        window = bars[i + 1 - size:i + 1]
        if loose_high(window, pivot_len):
            ph = window[pivot_len][1]
        if loose_low(window, pivot_len):
            pl = window[pivot_len][2]
        event = 0.0
        if ph is not None and bar[3] > ph:
            event = 2.0 if trend <= 0 else 1.0
            trend, ph = 1, None
        if pl is not None and bar[3] < pl:
            event = -2.0 if trend >= 0 else -1.0
            trend, pl = -1, None
        alerts = {names[event]: 0.9} if event else {}
        expected.append(((event, {"trend": float(trend), "event_code": event}), alerts))
    return {"pivot_len": float(pivot_len)}, bars, expected


# --- liquidity_sweeps ------------------------------------------------------------------------

def liquidity_sweeps(pivot_len, tolerance_pct, bars):
    """Sweeps of resting pivot levels: non-strict pivots over 2 pivot_len + 1 bars are kept as
    levels (the last 20 of each kind). A bar whose high exceeds a pivot high and closes back
    below it is a bearish sweep (value -1, `bear_sweep`), else one whose low undercuts a pivot low
    and closes back above it a bullish sweep (1, `bull_sweep`). Every level the bar traded beyond
    is removed, swept or broken. `eqh_count`/`eql_count`: neighbouring pairs of the remaining
    levels (in the order formed) within `tolerance_pct` percent of the older one. `extra.sweep` =
    value. Strength 0.85."""
    expected, highs, lows = [], [], []
    size = 2 * pivot_len + 1

    def equal_pairs(levels):
        return sum(1 for a, b in zip(levels, levels[1:])
                   if abs(a - b) / a * 100.0 <= tolerance_pct)

    for i, (_, high, low, close, _) in enumerate(bars):
        if i + 1 < size:
            expected.append((None, {}))
            continue
        window = bars[i + 1 - size:i + 1]
        if loose_high(window, pivot_len):
            highs = (highs + [window[pivot_len][1]])[-20:]
        if loose_low(window, pivot_len):
            lows = (lows + [window[pivot_len][2]])[-20:]
        sweep = 0
        if any(high > ph and close < ph for ph in highs):
            sweep = -1
        elif any(low < pl and close > pl for pl in lows):
            sweep = 1
        highs = [ph for ph in highs if high <= ph]
        lows = [pl for pl in lows if low >= pl]
        extra = {"sweep": float(sweep), "eqh_count": float(equal_pairs(highs)),
                 "eql_count": float(equal_pairs(lows))}
        alerts = {}
        if sweep == 1:
            alerts["bull_sweep"] = 0.85
        elif sweep == -1:
            alerts["bear_sweep"] = 0.85
        expected.append(((float(sweep), extra), alerts))
    return {"pivot_len": float(pivot_len), "tolerance_pct": tolerance_pct}, bars, expected


def sweep_bars():
    """A range between 100 and 106: up and down legs of six bars, so pivot highs at 106.5 and
    106.44 (0.056 % apart: equal at 0.2 %, not at 0.05 %) and pivot lows at 99.3 (equal lows).
    Bar 31 runs above 106.44 and closes back below it (bearish sweep), bar 34 wicks to 107 and
    closes back at 103 (bearish sweep of the resting highs), bar 46 wicks to 98.5 and closes back
    at 102 (bullish sweep of the resting lows); a leg from bar 50 closes above the range (levels
    taken without a sweep, a breakout), a leg from bar 56 falls back through it and closes below
    it (a breakdown), and the last leg closes back inside (reclaims)."""
    closes = path(100.0, [(1.0, 6), (-1.0, 6), (0.99, 6), (-0.99, 6), (1.0, 6), (-1.0, 6),
                          (1.0, 6), (-1.0, 6), (1.5, 6), (-1.5, 9), (1.0, 6)])
    return replace(candles(closes), {34: (103.5, 107.0, 102.3, 103.0),
                                     46: (102.5, 103.0, 98.5, 102.0)})


# --- liquidity_pools -------------------------------------------------------------------------

POOL_ALERTS = {
    ("bsl", "stop_hunted"): ("bear_pool_stop_hunt", 0.85),
    ("bsl", "broken_through"): ("bull_pool_breakout", 0.7),
    ("bsl", "reclaimed"): ("bear_pool_reclaim", 0.75),
    ("ssl", "stop_hunted"): ("bull_pool_stop_hunt", 0.85),
    ("ssl", "broken_through"): ("bear_pool_breakout", 0.7),
    ("ssl", "reclaimed"): ("bull_pool_reclaim", 0.75),
}


def liquidity_pools_case(pivot_len, tolerance_pct, bars):
    """Pools from `structure.liquidity_pools` (the documented rule); `value` = active pools.
    Alerts per pool event, named by the direction it points to: a BSL stop hunt
    `bear_pool_stop_hunt` (0.85), a BSL breakout `bull_pool_breakout` (0.7), a BSL reclaim
    `bear_pool_reclaim` (0.75); SSL mirrored (`bull_pool_stop_hunt`, `bear_pool_breakout`,
    `bull_pool_reclaim`). No extras."""
    counts, events = liquidity_pools([b[:4] for b in bars], pivot_len, tolerance_pct)
    first = 2 * max(pivot_len, 2)
    expected = no_alerts(first)
    for k, count in enumerate(counts):
        alerts = {}
        for index, kind, event, _ in events:
            if index == first + k:
                name, strength = POOL_ALERTS[(kind, event)]
                alerts[name] = strength
        expected.append(((float(count), {}), alerts))
    return {"pivot_len": float(pivot_len), "tolerance_pct": tolerance_pct}, bars, expected


# --- zigzag_advanced -------------------------------------------------------------------------

def zigzag_advanced(depth, backstep, deviation_pct, atr_len, bars):
    """Values from `structure.advanced_zigzag` in percent mode (the documented rule). The alerts
    of a bar are the confirmations that bar adds (the model run over the bars up to it):
    a confirmed swing low `bull_swing_confirmed`, a confirmed swing high `bear_swing_confirmed`,
    strength 0.6. No extras."""
    plain = [b[:4] for b in bars]
    outputs, _, _ = advanced_zigzag(plain, depth, backstep, deviation_pct, atr_len=atr_len)
    first = 2 * max(depth, 1)
    expected = no_alerts(first)
    seen = 0
    for k, (value, _) in enumerate(outputs):
        _, confirmed, _ = advanced_zigzag(plain[:first + k + 1], depth, backstep, deviation_pct,
                                          atr_len=atr_len)
        alerts = {}
        for kind in confirmed[seen:]:
            alerts["bull_swing_confirmed" if kind == "low" else "bear_swing_confirmed"] = 0.6
        seen = len(confirmed)
        expected.append(((value, {}), alerts))
    params = {"depth": float(depth), "backstep": float(backstep), "deviation_pct": deviation_pct,
              "atr_len": float(atr_len)}
    return params, bars, expected


# --- zone_rejection --------------------------------------------------------------------------

def zone_rejection(pivot_len, atr_len, tolerance_atr, max_width_atr, min_touches, bars):
    """Failed breakout at a confirmed pivot-cluster zone, per the module documentation. Zones from
    strict pivots (`pivot_len` bars each side, known `pivot_len` bars late): a pivot high joins
    the first resistance zone whose band widened by `tolerance_atr` ATR contains it, provided the
    zone then stays within `max_width_atr` ATR, else opens a new one (pivot lows build supports).
    A zone with at least `min_touches` pivots is confirmed; a close more than 0.2 ATR beyond its
    outer edge drops it. Event against the zones before the bar: low below a confirmed support's
    bottom and close above it `bull_failed_breakout` (value 1, `wick` = the low); high above a
    confirmed resistance's top and close below it `bear_failed_breakout` (-1, `wick` = the high);
    both at once report neither. `zones`: confirmed zones after the bar. First output with the
    ATR(atr_len).
    Conventions: "the ATR known before the bar" is the previous bar's ATR(atr_len); events,
    breaks and the pivot entering the zones all use it, so all three start one bar after the
    first output, and a pivot confirmed before that is not entered. On a bar, the event is judged
    first, then breaks, then the pivot confirmed on this bar enters. Alert strength:
    min(touches / 5, 1) of the zone with the most touches among those the bar rejected."""
    atr = wilder_atr(bars, atr_len)
    zones, expected = [], []
    for i, (_, high, low, close, _) in enumerate(bars):
        previous = atr[i - 1] if i > 0 else None
        event, alerts, wick = 0.0, {}, None
        if previous is not None:
            confirmed = [z for z in zones if z["touches"] >= min_touches]
            bull = [z for z in confirmed if z["support"] and low < z["bottom"] < close]
            bear = [z for z in confirmed if not z["support"] and high > z["top"] > close]
            if bull and not bear:
                event, wick = 1.0, low
                alerts["bull_failed_breakout"] = min(max(z["touches"] for z in bull) / 5.0, 1.0)
            elif bear and not bull:
                event, wick = -1.0, high
                alerts["bear_failed_breakout"] = min(max(z["touches"] for z in bear) / 5.0, 1.0)
            limit = 0.2 * previous
            zones = [z for z in zones if (close >= z["bottom"] - limit if z["support"]
                                          else close <= z["top"] + limit)]
            if i >= 2 * pivot_len:
                window = bars[i - 2 * pivot_len:i + 1]
                pivots = []
                if strict_high(window, pivot_len):
                    pivots.append((window[pivot_len][1], False))
                if strict_low(window, pivot_len):
                    pivots.append((window[pivot_len][2], True))
                for price, support in pivots:
                    tol, width = tolerance_atr * previous, max_width_atr * previous
                    for z in zones:
                        if (z["support"] == support and z["bottom"] - tol <= price <= z["top"] + tol
                                and max(z["top"], price) - min(z["bottom"], price) <= width):
                            z["bottom"], z["top"] = min(z["bottom"], price), max(z["top"], price)
                            z["touches"] += 1
                            break
                    else:
                        zones.append({"support": support, "bottom": price, "top": price,
                                      "touches": 1})
        if atr[i] is None:
            expected.append((None, {}))
            continue
        extra = {"zones": float(sum(1 for z in zones if z["touches"] >= min_touches))}
        if wick is not None:
            extra["wick"] = wick
        expected.append(((event, extra), alerts))
    params = {"pivot_len": float(pivot_len), "atr_len": float(atr_len),
              "tolerance_atr": tolerance_atr, "max_width_atr": max_width_atr,
              "min_touches": float(min_touches)}
    return params, bars, expected


def range_bars():
    """A range between 100 and 106 in legs of six bars: lows of 99.3 at bars 13, 25, 37, 49, 61,
    highs of 106.5 at bars 7, 19, 31, 43, 55 (the turns; the overrides below take some of them).
    With an ATR from bar 14, the lows of bars 13 and 25 confirm a support, the highs of 19 and 31
    a resistance. Bar 38 wicks to 98.6 below the support and closes back at 101 (bull, 2
    touches); bar 44 wicks to 107.3 above the resistance and closes back at 105 (bear); bar 50,
    an outside bar, does both and reports neither; from bar 62 the price climbs out of the range
    and closes far above the resistances (broken, dropped)."""
    closes = path(100.0, [(1.0, 6), (-1.0, 6)] * 5 + [(1.5, 12)])
    return replace(candles(closes), {38: (100.5, 101.5, 98.6, 101.0),
                                     44: (105.5, 107.3, 104.3, 105.0),
                                     50: (100.5, 107.4, 98.4, 101.0)})


# --- double_pattern --------------------------------------------------------------------------

def double_pattern(pivot_len, atr_len, tolerance_atr, min_depth_atr, max_gap, max_wait, bars):
    """Double bottom/top per the module documentation. A pivot low is lower than each of the
    `pivot_len` bars before it and no higher than any of the `pivot_len` bars after it (pivot
    highs mirrored), known `pivot_len` bars late. On the bar confirming a pivot low, it pairs with
    the most recent earlier pivot low for which: |difference| <= tolerance_atr ATR; the highest
    high strictly between them (the neckline) lies at least min_depth_atr ATR above the higher of
    the two lows; they are 2 pivot_len to max_gap bars apart; no close between them is below the
    lower low. That bar reports forming (value 1, `bull_double_bottom_forming`). The pattern
    confirms on the first close above the neckline within `max_wait` bars after forming (value 2,
    `bull_double_bottom`); a close below the lower low invalidates it, and after `max_wait` bars it
    expires, both silently. Tops mirror this (-1/-2, `bear_double_top_forming`,
    `bear_double_top`). Event bars carry `neckline`, `target` = neckline +/- (neckline - extreme)
    and `wick` = the lower low (higher high); every output carries `active`, the open patterns
    after the bar. Strengths 0.5 (forming), 1.0 (confirmed). First output with the ATR(atr_len).
    Conventions: the ATR is the previous bar's, as in zone_rejection, so pivots enter from one
    bar after the first output; open patterns are settled before this bar's pivots; per bar the
    first confirmation (in order of forming) and the first forming event are reported, a
    confirmation before a forming event; a pattern whose `max_wait` bars passed is gone before
    the bar's close is compared with its neckline; pivot lows are paired before pivot highs."""
    atr = wilder_atr(bars, atr_len)
    lows, highs, active, expected = [], [], [], []
    size = 2 * pivot_len + 1
    for i, (_, high, low, close, _) in enumerate(bars):
        event, info = 0.0, None
        remaining = []
        for p in active:
            if i - p["formed"] > max_wait:
                continue
            through = close > p["neckline"] if p["bottom"] else close < p["neckline"]
            if through and event == 0.0:
                event, info = (2.0 if p["bottom"] else -2.0), p
                continue
            broken = close < p["extreme"] if p["bottom"] else close > p["extreme"]
            if not broken:
                remaining.append(p)
        active = remaining
        previous = atr[i - 1] if i > 0 else None
        if previous is not None and i + 1 >= size:
            window = bars[i + 1 - size:i + 1]
            at = i - pivot_len
            mid = window[pivot_len]
            is_low = (all(b[2] > mid[2] for b in window[:pivot_len])
                      and all(b[2] >= mid[2] for b in window[pivot_len + 1:]))
            is_high = (all(b[1] < mid[1] for b in window[:pivot_len])
                       and all(b[1] <= mid[1] for b in window[pivot_len + 1:]))
            for is_pivot, bottom, price, earlier in ((is_low, True, mid[2], lows),
                                                     (is_high, False, mid[1], highs)):
                if not is_pivot:
                    continue
                for j, other in reversed(earlier):
                    pattern = double_pair(bars, j, other, at, price, bottom, previous,
                                          pivot_len, tolerance_atr, min_depth_atr, max_gap)
                    if pattern is not None:
                        pattern["formed"] = i
                        active.append(pattern)
                        if event == 0.0:
                            event, info = (1.0 if bottom else -1.0), pattern
                        break
                earlier.append((at, price))
                earlier[:] = [(j, q) for j, q in earlier if at - j <= max_gap]
        if atr[i] is None:
            expected.append((None, {}))
            continue
        extra, alerts = {"active": float(len(active))}, {}
        if info is not None:
            height = abs(info["neckline"] - info["extreme"])
            extra["neckline"] = info["neckline"]
            extra["wick"] = info["extreme"]
            extra["target"] = info["neckline"] + (height if info["bottom"] else -height)
            alerts[{1.0: "bull_double_bottom_forming", 2.0: "bull_double_bottom",
                    -1.0: "bear_double_top_forming", -2.0: "bear_double_top"}[event]] = (
                1.0 if abs(event) == 2.0 else 0.5)
        expected.append(((event, extra), alerts))
    params = {"pivot_len": float(pivot_len), "atr_len": float(atr_len),
              "tolerance_atr": tolerance_atr, "min_depth_atr": min_depth_atr,
              "max_gap": float(max_gap), "max_wait": float(max_wait)}
    return params, bars, expected


def double_pair(bars, j, first, k, second, bottom, atr, pivot_len, tolerance_atr, min_depth_atr,
                max_gap):
    """The pattern of pivots at bars j and k (0-based) with prices first/second, or None."""
    if not 2 * pivot_len <= k - j <= max_gap or abs(second - first) > tolerance_atr * atr:
        return None
    between = bars[j + 1:k]
    if not between:
        return None
    if bottom:
        neckline = max(b[1] for b in between)
        extreme = min(first, second)
        if neckline - max(first, second) < min_depth_atr * atr or any(b[3] < extreme
                                                                      for b in between):
            return None
    else:
        neckline = min(b[2] for b in between)
        extreme = max(first, second)
        if min(first, second) - neckline < min_depth_atr * atr or any(b[3] > extreme
                                                                      for b in between):
            return None
    return {"bottom": bottom, "neckline": neckline, "extreme": extreme}


def double_bars():
    """A decline to a low at bar 11 (99.3), a rally to 106 (neckline 106.5 at bar 17), a second
    low at bar 23 (99.0, within half an ATR), then a rally that closes above the neckline (double
    bottom); further up to a top at bar 39 (116.2), down to 110, a second top at bar 51 (116.08)
    and a fall that closes below the neckline (double top). Then a third low pair whose rally
    stalls below its neckline and falls back through the lows (bars 78-80: formed, invalidated),
    and a fourth pairing still open at the end."""
    closes = path(110.0, [(-1.0, 10), (1.0, 6), (-1.05, 6), (1.0, 16), (-1.0, 6), (0.98, 6),
                          (-1.0, 12), (0.8, 5), (-0.8, 5), (0.5, 4), (-1.0, 6), (0.5, 6)])
    return candles(closes)


# --- bottom_formation ------------------------------------------------------------------------

def bottom_side(bars, pivot_len, max_stage_bars, retest_atr):
    """One side of the bottom formation, read as a bottom (the top runs on mirrored bars). Per bar:
    (stage reached on the bar or 0, stage of the open run after the bar or 0, invalidation of the
    run that reached the stage). Stages per the module table: attempt (no run open, the last two
    confirmed swing lows falling, the bar's low below the last one and its close above it),
    stabilized (a swing low confirmed after the attempt, above the attempt low; the intermediate
    high is the highest high from the attempt to that swing low), structure shift (a close above
    the intermediate high), confirmed (after the shift a bar whose low comes within `retest_atr`
    ATR(14) of the intermediate high, then a close above the highest high since the shift bar). A
    close below the attempt low fails the run, after the shift also one below the stabilizing low;
    a stage older than `max_stage_bars` expires it; both silently. Swing lows: non-strict on both
    sides, confirmed `pivot_len` bars late.
    Conventions: a stage is reached on the bar it becomes known (the stabilizing low on the bar
    that confirms it) and counts its age from there; a bar that ends a run starts none; the retest
    bar itself does not confirm; the peak spans the shift bar up to the bar before the current
    one; without an ATR yet the retest needs the low at the level itself."""
    atr = wilder_atr(bars, 14)
    lows, run, out = [], None, []
    size = 2 * pivot_len + 1
    for i, (_, high, low, close, _) in enumerate(bars):
        if i + 1 >= size:
            window = bars[i + 1 - size:i + 1]
            if loose_low(window, pivot_len):
                lows = (lows + [(i - pivot_len, window[pivot_len][2])])[-2:]
        reached, wick = 0, None
        if run is not None:
            since = run["since"]
            if close < run["attempt_low"] or i - since > max_stage_bars:
                run = None
            elif run["stage"] == 1:
                if lows and lows[-1][0] > run["attempt"] and lows[-1][1] > run["attempt_low"]:
                    run["stabilizing"] = lows[-1][1]
                    run["level"] = max(b[1] for b in bars[run["attempt"]:lows[-1][0] + 1])
                    run["stage"], run["since"], reached = 2, i, 2
            elif run["stage"] == 2:
                if close > run["level"]:
                    run["stage"], run["since"], run["shift"], reached = 3, i, i, 3
            elif run["stage"] == 3:
                peak = max(b[1] for b in bars[run["shift"]:i])
                near = retest_atr * atr[i] if atr[i] is not None else 0.0
                if close < run["stabilizing"]:
                    run = None
                elif run["retest"] is None and i > run["shift"] and low <= run["level"] + near:
                    run["retest"] = i
                elif run["retest"] is not None and close > peak:
                    reached, wick, run = 4, run["attempt_low"], None
        elif (len(lows) == 2 and lows[1][1] < lows[0][1] and low < lows[1][1]
              and close > lows[1][1]):
            run = {"attempt": i, "attempt_low": low, "stage": 1, "since": i, "retest": None}
            reached = 1
        if reached and wick is None:
            wick = run["attempt_low"]
        out.append((reached, run["stage"] if run is not None else 0, wick))
    return out


def mirror(bars):
    return [(-o, -l, -h, -c, v) for o, h, l, c, v in bars]


BOTTOM_ALERTS = {1: ("bull_bottom_attempt", 0.4), 2: ("bull_bottom_stabilized", 0.6),
                 3: ("bull_structure_shift", 0.8), 4: ("bull_bottom_confirmed", 1.0)}
TOP_ALERTS = {1: ("bear_top_attempt", 0.4), 2: ("bear_top_stabilized", 0.6),
              3: ("bear_structure_shift", 0.8), 4: ("bear_top_confirmed", 1.0)}


def bottom_formation(pivot_len, max_stage_bars, retest_atr, bars):
    """Both sides as the indicator reports them: `value` +n for the stage a bottom reached on the
    bar, else -n for a top, else 0; `bottom_stage`/`top_stage` the open stage on every bar (0 once
    a run ended, confirmed or not); `wick` on event bars, the bottom's attempt low if the bottom
    reported, else the top's attempt high. Every reached stage alerts, both sides on one bar
    included. Output from the first bar."""
    bottom = bottom_side(bars, pivot_len, max_stage_bars, retest_atr)
    top = bottom_side(mirror(bars), pivot_len, max_stage_bars, retest_atr)
    expected = []
    for (b_reached, b_stage, b_wick), (t_reached, t_stage, t_wick) in zip(bottom, top):
        extra = {"bottom_stage": float(b_stage), "top_stage": float(t_stage)}
        alerts = {}
        if b_reached:
            name, strength = BOTTOM_ALERTS[b_reached]
            alerts[name] = strength
        if t_reached:
            name, strength = TOP_ALERTS[t_reached]
            alerts[name] = strength
        if b_reached:
            value, extra["wick"] = float(b_reached), b_wick
        elif t_reached:
            value, extra["wick"] = -float(t_reached), -t_wick
        else:
            value = 0.0
        expected.append(((value, extra), alerts))
    params = {"pivot_len": float(pivot_len), "max_stage_bars": float(max_stage_bars),
              "retest_atr": retest_atr}
    return params, bars, expected


def bottom_bars():
    """Falling swing lows at bar 9 (111.3) and bar 23 (106.3), a drift down to 108; bar 34
    undercuts 106.3 to 105.8 and closes back at 107.5 (attempt). A rally to 110.5 (bar 39, the
    intermediate high 111.0), a higher low at bar 43 (108.2, confirmed on bar 48: stabilized), a
    close above 111.0 on bar 49 (structure shift), a pullback to 111.3 whose lows reach the
    intermediate high plus half an ATR (retest), then a close above the high since the shift
    (bar 57, confirmed). Read as a top, bar 46 makes a new high above the last falling swing high
    and closes back below it (top attempt), and bar 47 closes above that high (failed)."""
    closes = path(120.0, [(-1.0, 8), (0.5, 6), (-1.0, 8), (0.5, 6), (-0.5, 4), (-0.5, 1),
                          (0.6, 5), (-0.4, 4), (0.6, 8), (-0.8, 3), (1.0, 6)])
    return replace(candles(closes), {34: (107.75, 108.0, 105.8, 107.5)})


def top_bars():
    """`bottom_bars` mirrored at 240: the same run as a top, every stage with the opposite sign."""
    return [(240.0 - o, 240.0 - l, 240.0 - h, 240.0 - c, v) for o, h, l, c, v in bottom_bars()]


# --- pivot_divergence, oscillator_pivot_divergence -------------------------------------------

def divergence_oscillator(code, bars):
    """The oscillator value per bar (None before its first output), each in its plain form as the
    `DivergenceOscillator` documentation states: 0 RSI 14 (raw RSI, Wilder averages: `rsi_lines`
    with avg_len 1), 3 Williams %R 14 on the ascending scale, 100 (close - lowest low) /
    (highest high - lowest low) over the last 14 bars, 50 without range. The other codes
    (WaveTrend, Stochastic RSI, CCI) are not modelled here."""
    if code == 0:
        line, _, _ = rsi_lines([b[3] for b in bars], 14, 1, 3)
        return [None] * (len(bars) - len(line)) + line
    if code == 3:
        out = []
        for i in range(len(bars)):
            if i < 13:
                out.append(None)
                continue
            window = bars[i - 13:i + 1]
            hh, ll = max(b[1] for b in window), min(b[2] for b in window)
            out.append(100.0 * (bars[i][3] - ll) / (hh - ll) if hh > ll else 50.0)
        return out
    raise ValueError(code)


def divergence_quality(prev_price, prev_osc, price, osc, between, min_distance, max_distance):
    """The documented quality, 60 % magnitude and 40 % spacing.
    Conventions: magnitude = min(1, mean of the relative price change and the relative oscillator
    change, each against the earlier pivot's absolute value, at least 1e-9); spacing =
    1 - min(1, |between - centre| / max(1, span / 2)), span = max(1, max - min), centre =
    min + span / 2; the sum clamped to 0..1."""
    price_move = abs(price - prev_price) / max(abs(prev_price), 1e-9)
    osc_move = abs(osc - prev_osc) / max(abs(prev_osc), 1e-9)
    magnitude = min((price_move + osc_move) / 2.0, 1.0)
    span = float(max(max_distance - min_distance, 1))
    centre = min_distance + span / 2.0
    spacing = 1.0 - min(abs(between - centre) / max(span / 2.0, 1.0), 1.0)
    return min(max(0.6 * magnitude + 0.4 * spacing, 0.0), 1.0)


DIVERGENCE_KINDS = {
    (True, "regular"): ("bull_divergence", 1.0), (True, "hidden"): ("bull_hidden_divergence", 1.0),
    (False, "regular"): ("bear_divergence", -1.0),
    (False, "hidden"): ("bear_hidden_divergence", -1.0),
}


def pivot_divergence(code, left, right, min_distance, max_distance, max_prior, bars,
                     oscillator_pivots=False):
    """Pivot-anchored divergence per the `PivotDivergence` documentation. `value` is the
    oscillator on every bar it exists. Over the oscillator bars, a strict pivot with `left`/`right`
    bars each side (of price: lows/highs; with `oscillator_pivots` of the oscillator, price read
    as that bar's low/high) is compared, on its confirmation bar `right` bars later, with up to
    `max_prior` earlier pivots of its type `min_distance..=max_distance` bars back, the
    oscillator read at the pivot bars: lower low with higher oscillator low `bull_divergence`,
    higher low with lower oscillator low `bull_hidden_divergence`, higher high with lower
    oscillator high `bear_divergence`, lower high with higher oscillator high
    `bear_hidden_divergence`. Per side the best-quality candidate is reported, strength = its
    quality. `extra` on such bars: `pivot_price`, `previous_price`, `pivot_age` (= right),
    `bars_between`, `direction`; with events on both sides the bearish one (handled last).
    Conventions: equal qualities keep the later prior; the earlier pivots are kept whether or not
    they paired."""
    osc = divergence_oscillator(code, bars)
    first = next(i for i, v in enumerate(osc) if v is not None)
    samples = [(bars[i][1], bars[i][2], osc[i]) for i in range(first, len(bars))]
    priors = {True: [], False: []}
    expected = no_alerts(first)
    size = left + right + 1
    for k in range(len(samples)):
        extra, alerts = {}, {}
        if k + 1 >= size:
            c = k - right
            others = [samples[j] for j in range(k + 1 - size, k + 1) if j != c]
            high, low, value = samples[c]
            if oscillator_pivots:
                is_low = all(o[2] > value for o in others)
                is_high = all(o[2] < value for o in others)
            else:
                is_low = all(o[1] > low for o in others)
                is_high = all(o[0] < high for o in others)
            for side, is_pivot, price in ((True, is_low, low), (False, is_high, high)):
                if not is_pivot:
                    continue
                best = None
                for j, prev_price, prev_osc in priors[side]:
                    between = c - j
                    if not min_distance <= between <= max_distance:
                        continue
                    if side:
                        kind = ("regular" if price < prev_price and value > prev_osc else
                                "hidden" if price > prev_price and value < prev_osc else None)
                    else:
                        kind = ("regular" if price > prev_price and value < prev_osc else
                                "hidden" if price < prev_price and value > prev_osc else None)
                    if kind is None:
                        continue
                    quality = divergence_quality(prev_price, prev_osc, price, value, between,
                                                 min_distance, max_distance)
                    if best is None or quality >= best[0]:
                        best = (quality, kind, prev_price, between)
                if best is not None:
                    quality, kind, prev_price, between = best
                    name, direction = DIVERGENCE_KINDS[(side, kind)]
                    alerts[name] = quality
                    extra = {"pivot_price": price, "previous_price": prev_price,
                             "pivot_age": float(right), "bars_between": float(between),
                             "direction": direction}
                priors[side] = (priors[side] + [(c, price, value)])[-max_prior:]
        expected.append(((samples[k][2], extra), alerts))
    params = {"oscillator": float(code), "left": float(left), "right": float(right),
              "min_distance": float(min_distance), "max_distance": float(max_distance),
              "max_prior_pivots": float(max_prior)}
    return params, bars, expected


def wave_bars(n):
    """Two overlaid waves with a slight drift (periods of about 57 and 295 bars): the slow wave
    shifts the fast wave's lows and highs while the RSI follows the fast one, so price and
    oscillator pivots disagree in both directions. Prices rounded to 1e-4."""
    bars = []
    for i in range(n):
        x = float(i)
        mid = 100.0 + 6.0 * math.sin(x / 9.0) + 5.0 * math.cos(x / 47.0) - 0.01 * x
        bars.append((round(mid, 4), round(mid + 0.8, 4), round(mid - 0.9, 4), round(mid + 0.1, 4),
                     1000.0))
    return bars


# --- wyckoff ---------------------------------------------------------------------------------

MAD_TO_SIGMA = 1.0 / statistics.NormalDist().inv_cdf(0.75)


def wyckoff(range_lookback, range_atr_max, min_range_bars, bars):
    """The Wyckoff state machine as its rules state them. ATR(14) of the true range; outputs once
    `range_lookback` bars and a positive ATR exist. Unlocked: when the window of the last
    `range_lookback` bars spans at most `range_atr_max` ATR, its high and low become the range
    (locked, phase A, output 0); the bias is accumulation after a climax bar falling (close below
    open), distribution after a rising one, else accumulation if the close fell from the previous
    close and distribution otherwise. A climax bar's volume exceeds median + 2 MAD-sigma of the
    last `range_lookback` volumes (the current one included; MAD scaled by 1 / Phi^-1(3/4)).
    Locked, per bar: once the window spans more than twice the limit after more than 3
    `min_range_bars` bars in the range, the range unlocks (output 0). Phases A/B (B from the first
    bar): a Spring (accumulation: low below the range low, close back above) moves to C
    (`wyckoff_spring`), a UTAD (distribution: high above the range high, close back below) to C
    (`wyckoff_utad`); else a high within 0.25 ATR below the range high or a low within 0.25 ATR
    above the range low is a Secondary Test (`wyckoff_secondarytest`). C: a close beyond the
    opposite boundary moves to D (`wyckoff_signofstrength` / `wyckoff_signofweakness`). D: a bar
    holding within 0.5 ATR of the broken boundary and closing beyond it moves to E
    (`wyckoff_lastpointofsupport` / `wyckoff_lastpointofsupply`); a close back inside returns to
    B. E stays. `value`: the phase, 1 A ... 5 E. Strengths: ST 0.4, Spring/UTAD 0.85, SOS/SOW
    0.8, LPS/LPSY 0.9.
    Convention: the median of an even count is the mean of the two middle values."""
    atr = wilder_atr(bars, 14)
    expected = []
    locked, bias, phase, rh, rl, in_range = False, None, 0, None, None, 0
    for i, (open_, high, low, close, volume) in enumerate(bars):
        if i + 1 < range_lookback or atr[i] is None or atr[i] <= 0.0:
            expected.append((None, {}))
            continue
        a = atr[i]
        window = bars[i + 1 - range_lookback:i + 1]
        wh, wl = max(b[1] for b in window), min(b[2] for b in window)
        width = (wh - wl) / a
        if not locked:
            if width <= range_atr_max:
                volumes = [b[4] for b in window]
                median = statistics.median(volumes)
                mad = statistics.median([abs(v - median) for v in volumes]) * MAD_TO_SIGMA
                climax = volume > median + 2.0 * mad
                if climax and close < open_:
                    bias = "acc"
                elif climax and close > open_:
                    bias = "dist"
                else:
                    bias = "acc" if close < bars[i - 1][3] else "dist"
                locked, phase, rh, rl, in_range = True, 1, wh, wl, 0
            expected.append(((0.0, {}), {}))
            continue
        in_range += 1
        if width > 2.0 * range_atr_max and in_range > 3 * min_range_bars:
            locked, bias, phase = False, None, 0
            expected.append(((0.0, {}), {}))
            continue
        alerts = {}
        if phase in (1, 2):
            phase = 2
            if bias == "acc" and low < rl and close > rl:
                phase, alerts["wyckoff_spring"] = 3, 0.85
            elif bias == "dist" and high > rh and close < rh:
                phase, alerts["wyckoff_utad"] = 3, 0.85
            elif rh - 0.25 * a <= high <= rh or rl <= low <= rl + 0.25 * a:
                alerts["wyckoff_secondarytest"] = 0.4
        elif phase == 3:
            if bias == "acc" and close > rh:
                phase, alerts["wyckoff_signofstrength"] = 4, 0.8
            elif bias == "dist" and close < rl:
                phase, alerts["wyckoff_signofweakness"] = 4, 0.8
        elif phase == 4:
            if bias == "acc" and low >= rh - 0.5 * a and close > rh:
                phase, alerts["wyckoff_lastpointofsupport"] = 5, 0.9
            elif bias == "dist" and high <= rl + 0.5 * a and close < rl:
                phase, alerts["wyckoff_lastpointofsupply"] = 5, 0.9
            elif (bias == "acc" and close < rh) or (bias == "dist" and close > rl):
                phase = 2
        expected.append(((float(phase), {}), alerts))
    params = {"range_lookback": float(range_lookback), "range_atr_max": range_atr_max,
              "min_range_bars": float(min_range_bars)}
    return params, bars, expected


def wyckoff_bars(climax=False, failed_breakout=False):
    """A decline from 120 into a sideways range around 108-109 (closes stepping +-0.4), which the
    window locks on bar 30 (a falling close: accumulation; spanning bars 11-30, range 107.3 to
    111.0); bar 52 undercuts the range low and closes back inside (Spring), bar 54 closes above
    the range (SOS), bar 55 holds above it (LPS), and the markup finally drags the window far
    beyond the range (unlock). `climax`: bar 30 rises on five times the volume, a climax that
    makes the range a distribution. `failed_breakout`: bar 55 closes back inside the range
    instead (phase D back to B)."""
    closes = path(120.0, [(-1.0, 12), (0.4, 1)] + [(0.4, 2), (-0.4, 2)] * 9 + [(0.4, 2)]
                  + [(1.2, 4), (0.3, 2), (1.5, 30)])
    bars = replace(candles(closes), {52: (108.6, 109.0, 106.0, 108.9)})
    if climax:
        bars = replace(bars, {30: (107.9, 108.9, 107.6, 108.6)})
        bars[29] = bars[29][:4] + (5000.0,)
    if failed_breakout:
        bars = replace(bars, {55: (112.0, 112.3, 109.9, 110.2)})
    return bars


def wyckoff_distribution_bars():
    """`wyckoff_bars` mirrored at 240: a rally into a range, a UTAD, a breakdown (SOW, LPSY)."""
    return [(240.0 - o, 240.0 - l, 240.0 - h, 240.0 - c, v) for o, h, l, c, v in wyckoff_bars()]


# --- candle_classification -------------------------------------------------------------------

CANDLE_DEFAULTS = {
    "pin_wick_min": 0.55, "pin_close_pos": 0.65, "marubozu_body_min": 0.82,
    "doji_body_max": 0.08, "hammer_wick_body_min": 2.0, "min_range_atr": 0.5, "atr_len": 14.0,
    "trend_lookback": 10.0, "belt_hold_body_min": 0.6, "belt_hold_open_wick_max": 0.03,
    "spinning_top_body_max": 0.3, "hammer_opposite_max": 0.5, "tweezer_tolerance": 0.0015,
    "trend_min_atr": 1.0,
}

CANDLE_STRENGTH = {
    "doji": 0.6, "dragonfly_doji": 0.7, "gravestone_doji": 0.7, "long_legged_doji": 0.6,
    "spinning_top": 0.5, "bullish_pinbar": 0.9, "bearish_pinbar": 0.9, "hammer_shape": 0.6,
    "inverted_hammer_shape": 0.6, "bullish_marubozu": 0.8, "bearish_marubozu": 0.8,
    "bullish_belt_hold_shape": 0.55, "bearish_belt_hold_shape": 0.55, "hammer": 0.75,
    "hanging_man": 0.75, "bullish_belt_hold": 0.7, "bearish_belt_hold": 0.7,
    "inverted_hammer": 0.7, "shooting_star": 0.75, "bullish_engulfing": 0.85,
    "bullish_engulfing_range": 0.9, "bearish_engulfing": 0.85, "bearish_engulfing_range": 0.9,
    "bullish_harami": 0.7, "bearish_harami": 0.7, "harami_cross": 0.75, "homing_pigeon": 0.7,
    "kicking": 0.85, "counterattack_lines": 0.7, "separating_lines": 0.7, "matching_low": 0.7,
    "bullish_tweezer": 0.8, "bearish_tweezer": 0.8,
}


def candle_metrics(bar):
    """The normalised bar: body, range, wicks and their shares of the range, close position;
    None without a positive range. Bullish includes open == close."""
    o, h, l, c = bar[:4]
    rng = h - l
    if rng <= 0.0:
        return None
    body, upper, lower = abs(c - o), h - max(c, o), min(c, o) - l
    return {"body": body, "range": rng, "upper": upper, "lower": lower, "body_ratio": body / rng,
            "upper_ratio": upper / rng, "lower_ratio": lower / rng,
            "close_position": (c - l) / rng, "bullish": c >= o}


def nearly_equal(a, b, tolerance):
    scale = max(abs(a), abs(b))
    return scale > 0.0 and abs(a - b) / scale < tolerance


def candle_single(m, cfg):
    """The one-bar shapes as their field documentation and rule comments define them."""
    found = []
    if m["body_ratio"] <= cfg["doji_body_max"]:
        found.append("doji")
        if m["lower_ratio"] >= cfg["pin_wick_min"]:
            found.append("dragonfly_doji")
        if m["upper_ratio"] >= cfg["pin_wick_min"]:
            found.append("gravestone_doji")
        if m["lower_ratio"] >= 0.3 and m["upper_ratio"] >= 0.3:
            found.append("long_legged_doji")
    elif m["body_ratio"] <= cfg["spinning_top_body_max"]:
        found.append("spinning_top")
    if m["lower_ratio"] >= cfg["pin_wick_min"] and m["close_position"] >= cfg["pin_close_pos"]:
        found.append("bullish_pinbar")
    if m["upper_ratio"] >= cfg["pin_wick_min"] and m["close_position"] <= 1.0 - cfg["pin_close_pos"]:
        found.append("bearish_pinbar")
    if m["body"] > 0.0:
        wick_min, opposite = cfg["hammer_wick_body_min"] * m["body"], cfg["hammer_opposite_max"] * m["body"]
        if m["lower"] >= wick_min and m["upper"] <= opposite:
            found.append("hammer_shape")
        if m["upper"] >= wick_min and m["lower"] <= opposite:
            found.append("inverted_hammer_shape")
    if m["body_ratio"] >= cfg["marubozu_body_min"]:
        found.append("bullish_marubozu" if m["bullish"] else "bearish_marubozu")
    if m["body_ratio"] >= cfg["belt_hold_body_min"]:
        if m["bullish"] and m["lower_ratio"] <= cfg["belt_hold_open_wick_max"]:
            found.append("bullish_belt_hold_shape")
        if not m["bullish"] and m["upper_ratio"] <= cfg["belt_hold_open_wick_max"]:
            found.append("bearish_belt_hold_shape")
    return found


def candle_named(found, trend, cfg):
    """Named readings of a shape, only with a known prior move of at least `trend_min_atr` ATR."""
    if trend is None:
        return []
    down, up = trend <= -cfg["trend_min_atr"], trend >= cfg["trend_min_atr"]
    named = []
    if "hammer_shape" in found:
        if down:
            named.append("hammer")
        elif up:
            named.append("hanging_man")
    if "bullish_belt_hold_shape" in found and down:
        named.append("bullish_belt_hold")
    if "bearish_belt_hold_shape" in found and up:
        named.append("bearish_belt_hold")
    if "inverted_hammer_shape" in found:
        if down:
            named.append("inverted_hammer")
        elif up:
            named.append("shooting_star")
    return named


def candle_pair(m, prev, bar, found, cfg):
    """The two-bar patterns as the rule comments define them."""
    o, h, l, c = bar[:4]
    po, ph, pl, pc = prev[:4]
    prev_body, prev_bearish, prev_bullish = abs(pc - po), pc < po, pc > po
    out = []
    engulfs = m["body"] > prev_body
    if m["bullish"] and prev_bearish and engulfs and c > po and o <= pc:
        out.append("bullish_engulfing")
        if h >= ph and l <= pl:
            out.append("bullish_engulfing_range")
    elif not m["bullish"] and prev_bullish and engulfs and c < po and o >= pc:
        out.append("bearish_engulfing")
        if h >= ph and l <= pl:
            out.append("bearish_engulfing_range")
    inside = max(o, c) <= max(po, pc) and min(o, c) >= min(po, pc)
    if inside and m["body"] < prev_body:
        if prev_bearish:
            out.append("bullish_harami")
        elif prev_bullish:
            out.append("bearish_harami")
        if "doji" in found:
            out.append("harami_cross")
        if prev_bearish and not m["bullish"]:
            out.append("homing_pigeon")
    pm = candle_metrics(prev)
    if (pm is not None and pm["body_ratio"] >= cfg["marubozu_body_min"]
            and m["body_ratio"] >= cfg["marubozu_body_min"]):
        if (prev_bullish and not m["bullish"] and h < pl) or (prev_bearish and m["bullish"] and l > ph):
            out.append("kicking")
    tol = cfg["tweezer_tolerance"]
    if prev_bullish != m["bullish"]:
        if nearly_equal(c, pc, tol):
            out.append("counterattack_lines")
        if nearly_equal(o, po, tol):
            out.append("separating_lines")
    if prev_bearish and not m["bullish"] and nearly_equal(c, pc, tol):
        out.append("matching_low")
    if pl != 0.0 and ph != 0.0:
        if abs(l - pl) / abs(pl) < tol and m["bullish"] and prev_bearish:
            out.append("bullish_tweezer")
        elif abs(h - ph) / abs(ph) < tol and not m["bullish"] and prev_bullish:
            out.append("bearish_tweezer")
    return out


def candle_classification(bars, **overrides):
    """Candle classification for cases of at most two bars (the three- to five-bar patterns are
    not modelled). Every recognised pattern is an `extra` flag (1) and an alert; `pattern_count`
    counts the flags; `value` = `pressure` = (close position - 0.5) 200; the metrics
    `body_ratio`, `upper_wick_ratio`, `lower_wick_ratio`, `close_position` always, `range_atr`
    (range / ATR(atr_len)) once the ATR exists, `trend_context` ((close - close trend_lookback
    bars back) / ATR) once both exist. Shapes need a range of at least `min_range_atr` ATR (no
    floor before the ATR exists); named readings need the trend context. A bar without range
    outputs 0 and nothing else.
    Conventions: the pair patterns are checked whatever the size floor says; the pair rules read
    a bar as bearish/bullish strictly (close below/above open) for the previous bar and the
    current bar as bullish when close >= open."""
    assert len(bars) <= 2
    bars = [b + (1000.0,) for b in bars]
    cfg = dict(CANDLE_DEFAULTS, **overrides)
    atr = wilder_atr(bars, int(cfg["atr_len"]))
    lookback = int(cfg["trend_lookback"])
    expected = []
    for i, bar in enumerate(bars):
        m = candle_metrics(bar)
        if m is None:
            expected.append(((0.0, {}), {}))
            continue
        a = atr[i] if atr[i] is not None and atr[i] > 0.0 else None
        trend = (bar[3] - bars[i - lookback][3]) / a if a is not None and i >= lookback else None
        found = []
        if a is None or m["range"] / a >= cfg["min_range_atr"]:
            found = candle_single(m, cfg)
            found += candle_named(found, trend, cfg)
        if i >= 1:
            found += candle_pair(m, bars[i - 1], bar, found, cfg)
        pressure = (m["close_position"] - 0.5) * 200.0
        extra = {name: 1.0 for name in found}
        extra.update({"pattern_count": float(len(found)), "pressure": pressure,
                      "body_ratio": m["body_ratio"], "upper_wick_ratio": m["upper_ratio"],
                      "lower_wick_ratio": m["lower_ratio"], "close_position": m["close_position"]})
        if a is not None:
            extra["range_atr"] = m["range"] / a
        if trend is not None:
            extra["trend_context"] = trend
        expected.append(((pressure, extra), {name: CANDLE_STRENGTH[name] for name in found}))
    params = {key: float(value) for key, value in overrides.items()}
    return params, bars, expected


# --- Rendering -------------------------------------------------------------------------------

def build():
    header = provenance("contract_structure") + [
        "",
        "Format: see contract_pilot (param_*, bar<i>_*, out<i>_value, out<i>_extra_<field>,",
        "alert<i>_<kind>; missing keys mean no output, no such extra field, no such alert).",
    ]
    wave, zz, sw, rng, dbl = (structure_wave(), zigzag_bars(), sweep_bars(),
                                range_bars(), double_bars())
    waves = wave_bars(350)
    blocks = [
        case_block("liquidity_fvg_defaults", "liquidity_fvg, lookback 20: gaps both ways, sweeps both ways",
                   *liquidity_fvg(20, fvg_bars())),
        case_block("liquidity_fvg_short", "liquidity_fvg, lookback 3: sweeps against the last 3 bars only",
                   *liquidity_fvg(3, fvg_bars())),
        case_block("liquidity_fvg_touch", "liquidity_fvg, lookback 2: touching levels are no gap",
                   *liquidity_fvg(2, fvg_touch_bars())),
        case_block("market_structure_breaks_equal_highs",
                   "market_structure_breaks, lookback 2: equal highs are no strict swing high",
                   *market_structure_breaks(2, equal_highs_bars())),
        case_block("market_structure_breaks_defaults", "market_structure_breaks, lookback 5, staircase",
                   *market_structure_breaks(5, wave)),
        case_block("market_structure_breaks_short", "market_structure_breaks, lookback 2, staircase",
                   *market_structure_breaks(2, wave)),
        case_block("pivots_structure_defaults", "pivots_structure 5/5/10, staircase",
                   *pivots_structure(5, 5, 10, wave)),
        case_block("pivots_structure_short", "pivots_structure 2/3/3, staircase",
                   *pivots_structure(2, 3, 3, wave)),
        case_block("zigzag_defaults", "zigzag depth 12, 5 %: the wiggle does not turn the leg",
                   *zigzag(12, 5.0, zz)),
        case_block("zigzag_fine", "zigzag depth 3, 1.5 %: the wiggle turns the leg twice",
                   *zigzag(3, 1.5, zz)),
        case_block("pivot_sets_days", "pivot_sets: first day running, then the previous day's pivots",
                   *pivot_sets(pivot_sets_bars())),
        case_block("bos_choch_defaults", "bos_choch, pivot_len 5, staircase",
                   *bos_choch(5, wave)),
        case_block("bos_choch_short", "bos_choch, pivot_len 3, staircase",
                   *bos_choch(3, wave)),
        case_block("bos_choch_equal_highs", "bos_choch, pivot_len 2: equal highs are a pivot",
                   *bos_choch(2, equal_highs_bars())),
        case_block("liquidity_sweeps_defaults", "liquidity_sweeps 5 / 0.2 %: equal highs and lows, sweeps",
                   *liquidity_sweeps(5, 0.2, sw)),
        case_block("liquidity_sweeps_tight", "liquidity_sweeps 2 / 0.05 %",
                   *liquidity_sweeps(2, 0.05, sw)),
        case_block("liquidity_pools_defaults", "liquidity_pools 5 / 0.2 %: stop hunts, breakouts, reclaims",
                   *liquidity_pools_case(5, 0.2, sw)),
        case_block("liquidity_pools_tight", "liquidity_pools 2 / 0.05 %",
                   *liquidity_pools_case(2, 0.05, sw)),
        case_block("zigzag_advanced_defaults", "zigzag_advanced 3/2/1 %/14",
                   *zigzag_advanced(3, 2, 1.0, 14, zz)),
        case_block("zigzag_advanced_coarse", "zigzag_advanced 2/0/3 %/5",
                   *zigzag_advanced(2, 0, 3.0, 5, zz)),
        case_block("zigzag_advanced_backstep", "zigzag_advanced 3/6/1 %: the backstep holds the wiggle's turn",
                   *zigzag_advanced(3, 6, 1.0, 14, zz)),
        case_block("zone_rejection_defaults",
                   "zone_rejection 5/14/0.25/2/2: rejections at support and resistance, an outside bar, breaks",
                   *zone_rejection(5, 14, 0.25, 2.0, 2, rng)),
        case_block("zone_rejection_strict",
                   "zone_rejection 3/5/0.5/1/3: three touches needed, narrower zones",
                   *zone_rejection(3, 5, 0.5, 1.0, 3, rng)),
        case_block("double_pattern_defaults",
                   "double_pattern 5/14/0.5/1.5/100/50: bottom and top formed and confirmed, a third invalidated",
                   *double_pattern(5, 14, 0.5, 1.5, 100, 50, dbl)),
        case_block("double_pattern_fast",
                   "double_pattern 3/5/1/1/30/4: each confirmation on the 4th bar after forming",
                   *double_pattern(3, 5, 1.0, 1.0, 30, 4, dbl)),
        case_block("double_pattern_expiry",
                   "double_pattern 3/5/1/1/30/3: the same patterns expire one bar before the neckline close",
                   *double_pattern(3, 5, 1.0, 1.0, 30, 3, dbl)),
        case_block("bottom_formation_defaults",
                   "bottom_formation 5/30/0.5: a bottom through all four stages, a failed top attempt",
                   *bottom_formation(5, 30, 0.5, bottom_bars())),
        case_block("bottom_formation_mirrored",
                   "bottom_formation 5/30/0.5 on the mirrored bars: the same run as a top",
                   *bottom_formation(5, 30, 0.5, top_bars())),
        case_block("bottom_formation_expiry",
                   "bottom_formation 3/8/0.2: the attempt expires before a higher low is confirmed",
                   *bottom_formation(3, 8, 0.2, bottom_bars())),
        case_block("pivot_divergence_defaults",
                   "pivot_divergence, RSI 7/7/10/120/5: all four kinds over two overlaid waves",
                   *pivot_divergence(0, 7, 7, 10, 120, 5, waves)),
        case_block("pivot_divergence_williams", "pivot_divergence, Williams %R 4/3/5/80/3",
                   *pivot_divergence(3, 4, 3, 5, 80, 3, waves)),
        case_block("oscillator_pivot_divergence_defaults",
                   "oscillator_pivot_divergence, RSI 5/5/5/60/1: pivots of the oscillator",
                   *pivot_divergence(0, 5, 5, 5, 60, 1, waves, oscillator_pivots=True)),
        case_block("oscillator_pivot_divergence_williams",
                   "oscillator_pivot_divergence, Williams %R 4/3/5/80/3",
                   *pivot_divergence(3, 4, 3, 5, 80, 3, waves, oscillator_pivots=True)),
        case_block("wyckoff_accumulation",
                   "wyckoff 20/3/6: range lock, Spring, SOS, LPS, unlock",
                   *wyckoff(20, 3.0, 6, wyckoff_bars())),
        case_block("wyckoff_distribution",
                   "wyckoff 20/3/6 on the mirrored bars: UTAD, SOW, LPSY",
                   *wyckoff(20, 3.0, 6, wyckoff_distribution_bars())),
        case_block("wyckoff_climax",
                   "wyckoff 20/3/6: a rising climax bar on the lock makes it a distribution",
                   *wyckoff(20, 3.0, 6, wyckoff_bars(climax=True))),
        case_block("wyckoff_failed_breakout",
                   "wyckoff 20/3/6: the bar after the SOS closes back inside (D back to B)",
                   *wyckoff(20, 3.0, 6, wyckoff_bars(failed_breakout=True))),
        case_block("wyckoff_wide", "wyckoff 15/4/3: an earlier, wider range",
                   *wyckoff(15, 4.0, 3, wyckoff_bars())),
        case_block("wyckoff_short_lookback",
                   "wyckoff 10/3/2: the window is full before the ATR(14) exists",
                   *wyckoff(10, 3.0, 2, wyckoff_bars())),
    ] + candle_cases()
    return header, blocks


def candle_cases():
    """Two-bar candle cases: each pair named after what its second bar shows."""
    pairs = [
        ("pinbar", "a bearish bar, then a bullish pinbar with the hammer shape (no trend known)",
         [(102.0, 102.5, 100.5, 101.0), (100.8, 101.6, 97.0, 101.4)], {}),
        ("engulfing", "a small bearish bar engulfed body and range by a bullish bar on nearly its low",
         [(101.0, 101.3, 100.2, 100.5), (100.4, 102.2, 100.1, 102.0)], {}),
        ("harami_cross", "a bullish marubozu, then a long-legged doji inside its body",
         [(100.0, 104.1, 99.9, 104.0), (102.0, 103.5, 100.5, 102.1)], {}),
        ("kicking", "a bearish marubozu, then a bullish marubozu gapping above its high",
         [(104.0, 104.05, 99.9, 100.0), (105.0, 109.1, 104.95, 109.0)], {}),
        ("tweezer", "a bearish bar, then a bullish bar on the same low and the same open",
         [(101.0, 101.4, 99.0, 99.5), (101.05, 101.8, 99.05, 101.6)], {}),
        ("homing_pigeon", "two bearish bars, the second inside the first's body, equal closes",
         [(104.0, 104.5, 99.5, 100.0), (102.0, 102.6, 99.8, 100.1)], {}),
        ("gravestone", "a bearish pinbar doji at the low, then a bearish engulfing range bar",
         [(100.0, 103.0, 99.98, 100.05), (100.1, 103.5, 99.5, 99.9)], {}),
        ("flat", "a bar without range outputs 0 only; the next bar pairs with it (equal opens)",
         [(100.0, 100.0, 100.0, 100.0), (100.0, 100.8, 99.2, 100.6)], {}),
        ("hammer_trend", "ATR(1), lookback 1: a decline of at least 0.2 ATR names the hammer",
         [(106.5, 107.0, 105.8, 106.0), (102.4, 102.65, 98.0, 102.6)],
         {"atr_len": 1.0, "trend_lookback": 1.0, "trend_min_atr": 0.2}),
        ("hanging_man", "ATR(1), lookback 1: the same shape after an advance is a hanging man",
         [(100.0, 101.1, 99.8, 101.0), (102.2, 102.5, 97.8, 102.4)],
         {"atr_len": 1.0, "trend_lookback": 1.0, "trend_min_atr": 0.2}),
        ("size_floor", "ATR(1), floor 1.5 ATR: the small second bar gets no shape, pairs remain",
         [(100.0, 104.1, 99.9, 104.0), (103.0, 103.5, 102.5, 103.02)],
         {"atr_len": 1.0, "min_range_atr": 1.5}),
        ("thresholds", "doji up to 0.2, pinbar wick 0.4, marubozu 0.7: a looser classification",
         [(100.0, 101.0, 99.0, 99.2), (99.3, 101.0, 98.0, 100.9)],
         {"doji_body_max": 0.2, "pin_wick_min": 0.4, "marubozu_body_min": 0.7,
          "spinning_top_body_max": 0.5, "pin_close_pos": 0.6}),
    ]
    return [case_block(f"candle_classification_{name}", f"candle_classification: {comment}",
                       *candle_classification(bars, **params))
            for name, comment, bars, params in pairs]
