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
from fractions import Fraction

from ..fixture import provenance, render_contract
from ..gap import expected_outputs as gap_outputs
from ..point_and_figure import chart as pnf_chart
from ..indicators import rsi_lines
from ..smoothing import rma
from ..structure import advanced_zigzag, liquidity_pools, pivot_structure_scores
from .contract_pilot import case_block

NAME = "contract_structure"
RENDER = render_contract

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


# --- gap -------------------------------------------------------------------------------------

def gap(bars, **params):
    """Price gaps per the module documentation of `src/indicator/gap.rs`; the model is
    `kestrel_reference/gap.py`. `reference` is the option name there and its position (0 range,
    1 close) in the case parameters."""
    case = {key: float(value) for key, value in params.items() if key != "reference"}
    if "reference" in params:
        case["reference"] = float(["range", "close"].index(params["reference"]))
    return case, bars, gap_outputs(bars, **params)


def gap_bars():
    """Nineteen bars of range 2 climbing from 94 to 100 (true range 2 on every bar, so the ATR is
    exactly 2), then by bar:
    20  opens at 104 above the high 101 of bar 19: a gap up of 3 = 1.5 ATR, a sixth filled
    23  opens below the low of bar 22: a gap down while the gap up is still open
    26  its high reaches the top of the gap down and its low the bottom of the gap up: both
        closed, three and six bars after they formed
    27  opens above the high of bar 26 and trades back to it: formed and closed on one bar
    28  opens exactly on the high of bar 27: touching is no gap
    29  a gap down that stays open to the end, a third filled, later more than half."""
    steps = [0.5, 0.25, 0.25] * 6
    closes = [94.0]
    for step in steps:
        closes.append(closes[-1] + step)
    bars = [(closes[0], closes[0] + 1.0, closes[0] - 1.0, closes[0], 1000.0)]
    bars += [(previous, close + 1.0, close - 1.0, close, 1000.0)
             for previous, close in zip(closes, closes[1:])]
    assert len(bars) == 19 and bars[-1][1:4] == (101.0, 99.0, 100.0)
    return bars + [(o, h, l, c, 1000.0) for o, h, l, c in [
        (104.0, 105.0, 103.5, 104.5), (104.5, 105.5, 103.0, 103.5), (103.5, 104.0, 102.5, 103.0),
        (102.0, 102.2, 101.6, 101.8), (101.8, 102.4, 101.5, 102.3), (102.3, 102.45, 101.2, 101.4),
        (101.4, 102.6, 100.9, 102.0), (103.0, 103.2, 102.5, 102.8), (103.2, 103.6, 102.9, 103.4),
        (102.0, 102.3, 101.0, 101.2), (101.2, 101.6, 100.5, 100.8), (100.8, 101.3, 100.0, 100.4),
        (100.4, 102.5, 100.1, 102.2)]]


def gap_close_bars():
    """Against the previous close, ATR over 5 bars (exactly 2 after bar 5), minimum 0.5 ATR:
    6   opens 1.0 above the close of bar 5: exactly 0.5 ATR, counts (the minimum is inclusive)
    7   opens 0.5 above the close of bar 6, below the minimum 0.95: no gap
    8   opens 2.0 below the close of bar 7: a gap down; its low closes the gap up of bar 6
    10  its high touches the top of the gap down: closed by reaching the edge."""
    return [(o, h, l, c, 1000.0) for o, h, l, c in [
        (100.0, 101.0, 99.0, 100.0), (100.0, 101.25, 99.25, 100.25), (100.25, 101.5, 99.5, 100.5),
        (100.5, 101.25, 99.25, 100.25), (100.25, 101.5, 99.5, 100.5),
        (101.5, 102.0, 101.0, 101.25), (101.75, 102.25, 101.5, 102.0),
        (100.0, 100.75, 99.5, 100.25), (100.25, 101.5, 100.0, 101.25),
        (101.25, 102.0, 101.0, 101.75), (101.75, 102.0, 101.5, 101.75)]]


def gap_horizon_bars():
    """max_bars 3, ATR over 3 bars:
    4   a gap up from 101.5 to 103, followed on bars 4 to 7 and never closed there
    8   its low is below the bottom of that gap, which is no longer followed: no event
    9   a gap down from 101 to 100
    12  the last bar on which it is followed; its high touches the top: closed after 3 bars."""
    return [(o, h, l, c, 1000.0) for o, h, l, c in [
        (100.0, 101.0, 99.0, 100.0), (100.0, 101.25, 99.25, 100.25), (100.25, 101.5, 99.5, 100.5),
        (103.0, 104.0, 102.5, 103.5), (103.5, 104.0, 102.75, 103.0), (103.0, 103.5, 102.0, 102.5),
        (102.5, 103.0, 101.75, 102.0), (102.0, 102.25, 101.0, 101.25),
        (100.0, 100.5, 99.0, 99.5), (99.5, 100.25, 99.0, 100.0), (100.0, 100.75, 99.5, 100.5),
        (100.5, 101.0, 100.0, 100.75), (100.75, 101.25, 100.5, 101.0)]]


def gap_flat_bars():
    """Three bars without a range (ATR 0), then single-price bars stepping up:
    4   opens above bar 3 while the ATR is 0: counts for every minimum, `size_atr` absent
    6   a step of 0.5 against an ATR of 1/9: 4.5 ATR
    7   a step of 0.1 against an ATR of 13/54, below the minimum of 2 ATR: no gap."""
    return [(p, p, p, p, 0.0) for p in [50.0, 50.0, 50.0, 50.5, 50.5, 51.0, 51.1, 51.1]]


def gap_walk_bars(n, seed):
    """A random walk whose open jumps away from the previous close on about a third of the bars
    (by up to 2 either way), so gaps of both directions form, overlap, close and run out."""
    state = seed

    def uniform():
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        return (state >> 11) / 2 ** 53

    bars, close = [], 100.0
    for _ in range(n):
        jump = (uniform() - 0.5) * 4.0 if uniform() < 0.35 else 0.0
        open_ = round(close + jump, 4)
        close = round(open_ + (uniform() + uniform() - 1.0) * 1.2, 4)
        high = round(max(open_, close) + uniform() * 0.5, 4)
        low = round(min(open_, close) - uniform() * 0.5, 4)
        bars.append((open_, high, low, close, 1000.0))
    return bars


def gap_summary(expected):
    """What a run reported, for the case comment."""
    outputs = [output for output, _ in expected if output is not None]
    up = sum(1 for value, _ in outputs if value > 0)
    down = sum(1 for value, _ in outputs if value < 0)
    closed = sum(int(extra["closed_count"]) for _, extra in outputs)
    same_bar = sum(1 for _, extra in outputs
                   for key, bars_since in extra.items()
                   if key.startswith("closed_") and key.endswith("_bars") and bars_since == 0.0)
    most = max(int(extra["open_count"]) for _, extra in outputs)
    left = int(outputs[-1][1]["open_count"])
    return (f"{up} gaps up, {down} down; {closed} closed ({same_bar} on their own bar), "
            f"{up + down - closed - left} dropped unclosed, {left} open at the end; "
            f"at most {most} open at once")


# --- point_and_figure ------------------------------------------------------------------------

def point_and_figure(bars, box="1", reversal=3, method="close"):
    """The point-and-figure chart as the indicator reports it per bar, per the module
    documentation of `src/indicator/point_and_figure.rs`. The model is
    `kestrel_reference/point_and_figure.py`, run afresh over the bars up to each bar: `value` is
    the direction of the last column (1 X, -1 O, 0 while there is none), `column` the number of
    columns, `top`/`bottom` the grid lines of its highest and lowest box, `boxes` their number.
    A bar after which there is one column more raises `pnf_new_x_column` or `pnf_new_o_column`
    (strength 1). `method` close reads the close, high_low the high and the low; in the case
    parameters it is its position (0 close, 1 high_low). Prices enter by their decimal text."""
    grid = Fraction(box)
    expected, before = [], 0
    for n in range(1, len(bars) + 1):
        if method == "close":
            pairs = [(repr(bar[3]), repr(bar[3])) for bar in bars[:n]]
        else:
            pairs = [(repr(bar[1]), repr(bar[2])) for bar in bars[:n]]
        columns = pnf_chart(pairs, box, reversal)
        extra, alerts, value = {"column": float(len(columns))}, {}, 0.0
        if columns:
            last = columns[-1]
            value = 1.0 if last["direction"] == "X" else -1.0
            extra.update({"top": float(last["top"]), "bottom": float(last["bottom"]),
                          "boxes": float((last["top"] - last["bottom"]) / grid + 1)})
            if len(columns) > before:
                assert len(columns) == before + 1
                alerts["pnf_new_x_column" if value > 0 else "pnf_new_o_column"] = 1.0
        before = len(columns)
        expected.append(((value, extra), alerts))
    params = {"box_size": float(grid), "reversal": float(reversal),
              "method": float(["close", "high_low"].index(method))}
    return params, bars, expected


def pnf_range_bars():
    """Bars whose closes are the turning points of a decline, a trading range and a breakout (the
    series of `golden_point_and_figure`): with box 1 and reversal 3 every close after the first
    opens a column until the breakout, which extends the last one."""
    return candles([120.0, 100.0, 111.0, 104.0, 108.0, 101.0, 109.0, 105.0, 110.0, 103.0, 108.0,
                    104.0, 107.0, 102.0, 110.0, 106.0, 111.0, 104.0, 109.0, 105.0, 108.0, 113.0,
                    117.0, 120.0])


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
    `structure_bearish_bias` at value <= -50, strength 0.85. A bar that confirms a strict pivot
    also reports it in `extra`: `pivot_high` / `pivot_low` (the pivot's high or low) and
    `pivot_offset` (= right); the keys are absent otherwise, and the first pivot of a kind is
    reported too."""
    scores = pivot_structure_scores([b[:4] for b in bars], left, right, window)
    first = left + right
    expected = no_alerts(first)
    for k, value in enumerate(scores):
        i = first + k
        centre = i - right
        others = [bars[j] for j in range(centre - left, i + 1) if j != centre]
        extra = {}
        if all(b[1] < bars[centre][1] for b in others):
            extra["pivot_high"] = bars[centre][1]
        if all(b[2] > bars[centre][2] for b in others):
            extra["pivot_low"] = bars[centre][2]
        if extra:
            extra["pivot_offset"] = float(right)
        alerts = {}
        if value >= 50.0:
            alerts["structure_bullish_bias"] = 0.85
        elif value <= -50.0:
            alerts["structure_bearish_bias"] = 0.85
        expected.append(((value, extra), alerts))
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
    pivot. `extra`: `trend` (0 before the first break), `event_code` (= value) and, on a bar that
    breaks a level, `broken_level` (the pivot price that was closed beyond; on a double break the
    later, bearish one). One alert per
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
        event, broken = 0.0, None
        if ph is not None and bar[3] > ph:
            event, broken = (2.0 if trend <= 0 else 1.0), ph
            trend, ph = 1, None
        if pl is not None and bar[3] < pl:
            event, broken = (-2.0 if trend >= 0 else -1.0), pl
            trend, pl = -1, None
        alerts = {names[event]: 0.9} if event else {}
        extra = {"trend": float(trend), "event_code": event}
        if broken is not None:
            extra["broken_level"] = broken
        expected.append(((event, extra), alerts))
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

WYCKOFF_DEFAULTS = {
    "range_lookback": 20, "range_atr_max": 3.0, "min_range_bars": 6, "atr_len": 14,
    "volume_k": 2.0, "climax_range_atr": 1.5, "st_atr": 0.25, "breakout_volume": 1,
    "lps_atr": 0.5, "abandon_atr": 1.0,
}

# Event names per side (accumulation, distribution) and the fixed alert strength of each pair.
WYCKOFF_EVENTS = {
    "preliminary": ("preliminarysupport", "preliminarysupply", 0.3),
    "climax": ("sellingclimax", "buyingclimax", 0.7),
    "counter": ("automaticrally", "automaticreaction", 0.5),
    "test": ("spring", "utad", 0.85),
    "breakout": ("signofstrength", "signofweakness", 0.8),
    "hold": ("lastpointofsupport", "lastpointofsupply", 0.9),
}
SECONDARY_TEST = ("secondarytest", 0.4)
WYCKOFF_STRENGTH = {names[k]: names[2] for names in WYCKOFF_EVENTS.values() for k in (0, 1)}
WYCKOFF_STRENGTH[SECONDARY_TEST[0]] = SECONDARY_TEST[1]


def wyckoff(bars, **overrides):
    """The Wyckoff state machine as the module documentation of `src/indicator/wyckoff.rs` states
    its rules; `overrides` replace the documented defaults.

    Outputs once `range_lookback` bars and a positive Wilder ATR(`atr_len`) exist. Per bar: the
    volume threshold is median + `volume_k` MAD-sigma of the last `range_lookback` volumes (the
    current one included; MAD scaled by 1 / Phi^-1(3/4)); while the MAD is zero there is no
    threshold and no volume exceeds it. A selling climax bar is at least
    `climax_range_atr` ATR wide (high - low), exceeds the volume threshold, closes below its open
    and has the lowest low of the window; a buying climax bar is the mirror image (closes above
    its open, highest high of the window).

    Search, only on a bar that began without a range or in phase E and that the rules of that
    phase left there (so never on a bar that reported an event, changed the phase or abandoned a
    range): a pending climax is followed first. It is dropped once `range_lookback` bars have passed or its
    extreme is exceeded; a bar beyond the counter-move's extreme so far becomes the counter-move
    bar; otherwise the range is set when the close has crossed back over that bar's opposite
    extreme (selling climax: close below the low of the rally-high bar) and the counter-move has
    gone beyond the climax bar's own opposite extreme (rally high above the climax bar's high).
    The range runs from the climax extreme to the counter-move extreme, phase A, bias from the
    climax; the events climax and counter-move are reported with the number of bars they lie
    back, and the preliminary support/supply if there is one: the first bar of the window before
    the climax bar whose volume exceeds the climax bar's threshold and that closed against the
    climax (above its open before a selling climax). Then a climax bar becomes the pending climax
    (replacing an earlier one). Last, only while no climax is pending: a window at most
    `range_atr_max` ATR wide sets the range from its extremes, phase A, bias open (0).

    Range in play (the setting bar runs none of this), `in_range` counting the bars of phases A
    to D. A/B: a Spring (low below the range low, close back above, not also the mirror image on
    the same bar, bias not distribution, `in_range` >= `min_range_bars`) fixes the bias and moves
    to C, the UTAD likewise; else a close more than `abandon_atr` ATR outside the range abandons
    it; else a Secondary Test (the low within `st_atr` ATR above the range low without undercutting
    it - bias open: or the high at the range high - and, for a range set by a climax, volume and
    spread below the climax bar's) moves to B. C: a close beyond the opposite boundary, with
    volume above the threshold unless `breakout_volume` is 0, is the SOS/SOW and moves to D; else
    the same abandonment. D: a close back inside moves to B; a pullback starts with a close
    against the breakout (accumulation: below the previous close) and tracks its extreme; the
    first later close with the breakout again ends it, and if its extreme held within `lps_atr`
    ATR of the broken boundary it is the LPS/LPSY at the bar of that extreme, phase E. E: a close
    back inside moves to B.

    `value`: the phase, 1 A ... 5 E. Extras: `atr`, `volume_threshold` (absent while there is
    none), `climax` (-1 selling, 1 buying climax bar), `bias` (1 accumulation, -1 distribution, 0 open or none), `range_source`
    (0 none, 1 climax, 2 width); with a range `range_high`, `range_low` and `cause_score` (width
    x `in_range`); `event_<name>` = bars back on the reporting bar, with the alert
    `wyckoff_<name>`.
    Convention: the median of an even count is the mean of the two middle values."""
    p = dict(WYCKOFF_DEFAULTS, **overrides)
    lookback = p["range_lookback"]
    atr = wilder_atr(bars, p["atr_len"])
    expected = []
    # rng: [high, low, bias, source, climax volume, climax spread]; pending: the climax awaiting
    # its counter-move; pullback: (extreme, bar) in phase D.
    phase, rng, pending, pullback, in_range = 0, None, None, None, 0

    def beyond(side, a, b):
        """a lies beyond b in the direction of a climax of this side (down for accumulation)."""
        return a < b if side == 1 else a > b

    def name(key, side):
        return WYCKOFF_EVENTS[key][0 if side == 1 else 1]

    for i, (open_, high, low, close, volume) in enumerate(bars):
        if i + 1 < lookback or atr[i] is None or atr[i] <= 0.0:
            expected.append((None, {}))
            continue
        a = atr[i]
        window = bars[i + 1 - lookback:i + 1]
        volumes = [b[4] for b in window]
        median = statistics.median(volumes)
        mad = statistics.median([abs(v - median) for v in volumes]) * MAD_TO_SIGMA
        threshold = median + p["volume_k"] * mad if mad > 0.0 else None

        def outlier(amount):
            return threshold is not None and amount > threshold

        window_high, window_low = max(b[1] for b in window), min(b[2] for b in window)
        climax = 0  # the side a range from this bar would take: 1 after a selling climax
        if high - low >= p["climax_range_atr"] * a and outlier(volume):
            if close < open_ and low <= window_low:
                climax = 1
            elif close > open_ and high >= window_high:
                climax = -1
        events = {}  # name -> bars back
        previous_close = bars[i - 1][3]
        far = p["abandon_atr"] * a

        started_in = phase
        if phase in (1, 2, 3, 4):
            in_range += 1
        if phase in (1, 2):
            spring = low < rng[1] and close > rng[1]
            utad = high > rng[0] and close < rng[0]
            old_enough = in_range >= p["min_range_bars"]
            if old_enough and spring and not utad and rng[2] != -1:
                rng[2], phase = 1, 3
                events["spring"] = 0
            elif old_enough and utad and not spring and rng[2] != 1:
                rng[2], phase = -1, 3
                events["utad"] = 0
            elif close > rng[0] + far or close < rng[1] - far:
                phase, rng = 0, None
            else:
                at_low = rng[2] != -1 and rng[1] <= low <= rng[1] + p["st_atr"] * a
                at_high = rng[2] != 1 and rng[0] - p["st_atr"] * a <= high <= rng[0]
                quieter = rng[3] != 1 or (volume < rng[4] and high - low < rng[5])
                if (at_low or at_high) and quieter:
                    phase = 2
                    events[SECONDARY_TEST[0]] = 0
        elif phase == 3:
            side = rng[2]
            out = close > rng[0] if side == 1 else close < rng[1]
            if out and (p["breakout_volume"] == 0 or outlier(volume)):
                phase, pullback = 4, None
                events[name("breakout", side)] = 0
            elif close > rng[0] + far or close < rng[1] - far:
                phase, rng = 0, None
        elif phase == 4:
            side = rng[2]
            broken = rng[0] if side == 1 else rng[1]
            extreme = low if side == 1 else high
            if beyond(side, close, broken):
                phase, pullback = 2, None
            elif pullback is None:
                if beyond(side, close, previous_close):
                    pullback = (extreme, i)
            else:
                if beyond(side, extreme, pullback[0]):
                    pullback = (extreme, i)
                if beyond(-side, close, previous_close):
                    limit = broken - p["lps_atr"] * a if side == 1 else broken + p["lps_atr"] * a
                    if not beyond(side, pullback[0], limit):
                        phase = 5
                        events[name("hold", side)] = i - pullback[1]
                    pullback = None
        elif phase == 5:
            side = rng[2]
            if beyond(side, close, rng[0] if side == 1 else rng[1]):
                phase = 2

        searching = started_in in (0, 5) and phase == started_in
        if searching and pending is not None:
            side = pending["side"]
            extreme, opposite = (low, high) if side == 1 else (high, low)
            if i - pending["bar"] >= lookback or beyond(side, extreme, pending["extreme"]):
                pending = None
            elif pending["counter"] is None or beyond(-side, opposite, pending["counter"][0]):
                pending["counter"] = (opposite, i, extreme)
            else:
                counter, counter_bar, counter_back = pending["counter"]
                if (beyond(side, close, counter_back)
                        and beyond(-side, counter, pending["opposite"])):
                    rng = [max(counter, pending["extreme"]), min(counter, pending["extreme"]),
                           side, 1, pending["volume"], pending["spread"]]
                    phase, in_range, pullback = 1, 0, None
                    if pending["preliminary"] is not None:
                        events[name("preliminary", side)] = i - pending["preliminary"]
                    events[name("climax", side)] = i - pending["bar"]
                    events[name("counter", side)] = i - counter_bar
                    pending = None
        if not searching:
            pending = None
        elif phase == started_in:  # the pending climax has not just set the range
            if climax != 0:
                preliminary = None
                for k in range(i + 1 - lookback, i):
                    against = bars[k][3] > bars[k][0] if climax == 1 else bars[k][3] < bars[k][0]
                    if outlier(bars[k][4]) and against:
                        preliminary = k
                        break
                pending = {"side": climax, "bar": i, "extreme": low if climax == 1 else high,
                           "opposite": high if climax == 1 else low, "volume": volume,
                           "spread": high - low, "preliminary": preliminary, "counter": None}
            if pending is None and window_high - window_low <= p["range_atr_max"] * a:
                rng = [window_high, window_low, 0, 2, None, None]
                phase, in_range, pullback = 1, 0, None

        extra = {"atr": a, "climax": float(-climax), "bias": float(rng[2]) if rng else 0.0,
                 "range_source": float(rng[3]) if rng else 0.0}
        if threshold is not None:
            extra["volume_threshold"] = threshold
        if rng:
            extra.update({"range_high": rng[0], "range_low": rng[1],
                          "cause_score": (rng[0] - rng[1]) * in_range})
        alerts = {}
        for event, back in events.items():
            extra[f"event_{event}"] = float(back)
            alerts[f"wyckoff_{event}"] = WYCKOFF_STRENGTH[event]
        expected.append(((float(phase), extra), alerts))
    return {key: float(value) for key, value in overrides.items()}, bars, expected


# The textbook accumulation: (close, wick above the body, wick below the body, volume); the open
# is the previous close, the first bar opens at 110.4. Built bar by bar so that each event of the
# schematic lies on one bar and shows there what the literature describes (0-based bar numbers):
#   16 Preliminary Support - the highest volume so far, a long lower wick, close above the open;
#      the decline continues afterwards
#   21 Selling Climax      - the widest bar (3.2) on the highest volume (430), low 94.0
#   24 Automatic Rally     - third rising bar after the climax, high 100.0
#   29 Secondary Test      - low 94.3, a third of the climax volume, 1.2 wide
#   48 Spring              - low 93.4 below the climax low, close 94.9 back above
#   53 Sign of Strength    - close 101.6 above the rally high on the highest volume since the climax
#   57 Last Point of Support - the pullback low 100.1 holds above the former range high
WYCKOFF_TEXTBOOK = [
    (109.6, 0.3, 0.4, 105.0), (108.9, 0.4, 0.3, 110.0), (109.3, 0.3, 0.3, 90.0),
    (108.2, 0.2, 0.4, 115.0), (107.3, 0.3, 0.3, 120.0), (107.7, 0.3, 0.2, 95.0),
    (106.5, 0.2, 0.4, 125.0), (105.6, 0.3, 0.3, 120.0), (105.9, 0.3, 0.3, 100.0),
    (104.7, 0.2, 0.4, 130.0), (103.6, 0.3, 0.3, 135.0), (103.9, 0.3, 0.2, 105.0),
    (102.6, 0.2, 0.4, 140.0), (101.6, 0.3, 0.4, 150.0), (101.0, 0.2, 0.4, 145.0),
    (98.6, 0.2, 0.5, 185.0),
    (98.9, 0.3, 1.1, 240.0),  # 16 PS
    (99.3, 0.3, 0.3, 150.0), (98.4, 0.2, 0.4, 140.0), (97.4, 0.2, 0.5, 165.0),
    (97.0, 0.2, 0.4, 190.0),
    (94.8, 0.2, 0.8, 430.0),  # 21 SC
    (96.4, 0.3, 0.2, 260.0), (98.3, 0.3, 0.2, 210.0),
    (99.6, 0.4, 0.2, 180.0),  # 24 AR
    (98.7, 0.3, 0.3, 130.0), (97.4, 0.2, 0.4, 120.0), (96.2, 0.2, 0.4, 115.0),
    (95.1, 0.3, 0.5, 125.0),
    (95.3, 0.2, 0.8, 140.0),  # 29 ST
    (96.5, 0.3, 0.2, 110.0), (97.8, 0.3, 0.3, 105.0), (98.9, 0.4, 0.2, 100.0),
    (98.2, 0.3, 0.3, 85.0), (97.6, 0.3, 0.3, 90.0), (96.5, 0.2, 0.4, 85.0),
    (95.6, 0.3, 0.4, 80.0), (95.0, 0.2, 0.5, 75.0), (95.4, 0.2, 0.8, 70.0),
    (96.6, 0.3, 0.2, 85.0), (97.9, 0.3, 0.3, 90.0), (99.0, 0.4, 0.2, 95.0),
    (98.3, 0.3, 0.3, 75.0), (98.9, 0.5, 0.3, 80.0), (97.7, 0.2, 0.4, 70.0),
    (96.5, 0.2, 0.3, 75.0), (95.6, 0.3, 0.4, 70.0), (94.8, 0.2, 0.3, 80.0),
    (94.9, 0.2, 1.4, 95.0),  # 48 Spring
    (95.8, 0.2, 0.5, 85.0), (96.9, 0.3, 0.2, 110.0), (98.2, 0.3, 0.2, 140.0),
    (99.3, 0.3, 0.3, 170.0),
    (101.6, 0.2, 0.2, 310.0),  # 53 SOS
    (102.1, 0.3, 0.4, 240.0), (101.3, 0.2, 0.3, 150.0), (100.6, 0.2, 0.3, 110.0),
    (100.7, 0.2, 0.5, 85.0),  # 57 LPS
    (101.7, 0.3, 0.2, 130.0), (102.6, 0.3, 0.2, 160.0), (103.3, 0.3, 0.3, 170.0),
    (103.9, 0.3, 0.3, 150.0),
]
WYCKOFF_TEXTBOOK_START = 110.4

# The same series up to the bar after the Sign of Strength, then a pullback that closes back
# inside the range (bar 56) and stays there.
WYCKOFF_FAILED_TAIL = [
    (100.9, 0.2, 0.3, 150.0), (99.4, 0.2, 0.3, 140.0), (98.6, 0.3, 0.3, 110.0),
    (97.9, 0.2, 0.4, 95.0), (98.7, 0.3, 0.2, 90.0), (99.3, 0.3, 0.3, 85.0),
    (98.5, 0.2, 0.3, 80.0), (97.6, 0.3, 0.3, 85.0),
]


def table_bars(start, table):
    """Bars from rows (close, wick above the body, wick below the body, volume): the open is the
    previous close, the first one `start`. Prices rounded to 1e-6."""
    bars, previous = [], start
    for close, above, below, volume in table:
        bars.append((previous, round(max(previous, close) + above, 6),
                     round(min(previous, close) - below, 6), close, volume))
        previous = close
    return bars


def mirror_at(bars, level):
    """`bars` reflected at the price `level`: highs and lows trade places, volumes stay."""
    return [(round(level - o, 6), round(level - l, 6), round(level - h, 6), round(level - c, 6), v)
            for o, h, l, c, v in bars]


def wyckoff_textbook_bars():
    return table_bars(WYCKOFF_TEXTBOOK_START, WYCKOFF_TEXTBOOK)


def wyckoff_failed_breakout_bars():
    return table_bars(WYCKOFF_TEXTBOOK_START, WYCKOFF_TEXTBOOK[:55] + WYCKOFF_FAILED_TAIL)


def wyckoff_deep_pullback_bars():
    """The textbook series with a longer lower wick on bar 57: the pullback low is 99.5, half a
    point below the former range high, and the bar still closes above it."""
    table = list(WYCKOFF_TEXTBOOK)
    table[57] = (100.7, 0.2, 1.1, 85.0)
    return table_bars(WYCKOFF_TEXTBOOK_START, table)


def wyckoff_cycle_bars():
    """The textbook accumulation, then its mirror image at 214.3: that one opens where the
    accumulation closed (103.9), its mirrored decline is the markup, and it ends as a
    distribution with a Buying Climax at 120.3. Phase E of the first range lasts until the second
    range is set."""
    bars = wyckoff_textbook_bars()
    return bars + mirror_at(bars, 2 * 103.9 + (WYCKOFF_TEXTBOOK_START - 103.9))


def wyckoff_quiet_bars(volume):
    """A range without a climax: closes swinging between 100 and 102 in steps of 0.5 for 33 bars
    at one constant `volume`, so no bar is a volume outlier and only the width fallback can set
    the range; only the bars at the turns come near a boundary. Bar 34 undercuts the range and
    closes back inside, bar 36 closes above it, bar 37 pulls back, bar 38 turns up again, then
    the advance runs on."""
    closes = path(100.0, [(0.5, 4), (-0.5, 4)] * 4
                  + [(0.1, 1), (0.8, 1), (2.4, 1), (-0.4, 1), (0.4, 1), (0.8, 8)])
    return replace(candles(closes, volume=volume), {34: (100.0, 100.3, 98.2, 100.1)})


def wyckoff_quiet_volume_bars(settled, breakout):
    """The quiet range with volumes that vary: 90, 100, 110, 100 in turn. From bar 25 on
    (`settled` true) every bar trades 100 instead, so that by bar 36 more than half of the window
    is one value and its MAD is zero. Bar 36, the close above the range, trades `breakout`."""
    bars = wyckoff_quiet_bars(100.0)
    cycle = (90.0, 100.0, 110.0, 100.0)
    out = []
    for i, bar in enumerate(bars, start=1):
        volume = 100.0 if settled and i >= 25 else cycle[(i - 1) % 4]
        out.append(bar[:4] + (breakout if i == 36 else volume,))
    return out


def wyckoff_walk_bars(n, seed):
    """A random walk with no structure built in. A 64-bit linear congruential generator (Knuth's
    MMIX constants) gives uniform numbers in [0, 1) from its top 53 bits; the close moves by the
    sum of three of them minus 1.5, times 1.6; the wicks are 0.6 times one each; the volume is
    1000 x (0.5 + u) x (0.6 + 0.5 x spread), so wide bars tend to trade more. Prices rounded to
    1e-4, volumes to 1e-2."""
    state = seed

    def uniform():
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        return (state >> 11) / 2 ** 53

    bars, close = [], 100.0
    for _ in range(n):
        open_ = close
        close = round(open_ + (uniform() + uniform() + uniform() - 1.5) * 1.6, 4)
        high = round(max(open_, close) + uniform() * 0.6, 4)
        low = round(min(open_, close) - uniform() * 0.6, 4)
        volume = round(1000.0 * (0.5 + uniform()) * (0.6 + 0.5 * (high - low)), 2)
        bars.append((open_, high, low, close, volume))
    return bars


def wyckoff_summary(expected):
    """What a run reported, for the case comment: bars per phase and events by name."""
    phases, events, ranges, last = {}, {}, {1: 0, 2: 0}, None
    for output, alerts in expected:
        if output is None:
            continue
        value, extra = output
        phases[int(value)] = phases.get(int(value), 0) + 1
        for kind in alerts:
            events[kind[len("wyckoff_"):]] = events.get(kind[len("wyckoff_"):], 0) + 1
        boundaries = (extra.get("range_high"), extra.get("range_low"))
        if boundaries != last and boundaries[0] is not None:
            ranges[int(extra["range_source"])] += 1
        last = boundaries
    letters = "0ABCDE"
    return ("bars per phase " + ", ".join(f"{letters[k]} {phases[k]}" for k in sorted(phases))
            + f"; ranges set: {ranges[1]} by climax, {ranges[2]} by width; events: "
            + (", ".join(f"{events[k]} {k}" for k in sorted(events)) or "none"))


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
    # Both need the classic opening gap: no match when the bar opens at the previous close.
    if prev_bullish != m["bullish"] and not nearly_equal(o, pc, tol):
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
        "Format: see contract_pilot (one table per case, `-` = absent).",
    ]
    wave, zz, sw, rng, dbl = (structure_wave(), zigzag_bars(), sweep_bars(),
                                range_bars(), double_bars())
    waves = wave_bars(350)
    textbook, quiet = wyckoff_textbook_bars(), wyckoff_quiet_bars(1000.0)
    walk = wyckoff_walk_bars(400, 20261110)
    walk_run = wyckoff(walk)
    # Every threshold off its default. Resetting any single one changes the reported phases or
    # events of at least one of the two runs (`lps_atr`: see the deep pullback cases).
    tuned = {"range_lookback": 15, "range_atr_max": 4.0, "min_range_bars": 3, "atr_len": 10,
             "volume_k": 3.0, "climax_range_atr": 1.8, "st_atr": 0.4, "breakout_volume": 0,
             "lps_atr": 0.2, "abandon_atr": 1.5}
    tuned_cycle, tuned_walk = wyckoff(wyckoff_cycle_bars(), **tuned), wyckoff(walk, **tuned)
    gap_walk = gap(gap_walk_bars(150, 20261110), min_atr=0.25, max_bars=20)
    blocks = [
        case_block("liquidity_fvg_defaults", "liquidity_fvg, lookback 20: gaps both ways, sweeps both ways",
                   *liquidity_fvg(20, fvg_bars())),
        case_block("liquidity_fvg_short", "liquidity_fvg, lookback 3: sweeps against the last 3 bars only",
                   *liquidity_fvg(3, fvg_bars())),
        case_block("liquidity_fvg_touch", "liquidity_fvg, lookback 2: touching levels are no gap",
                   *liquidity_fvg(2, fvg_touch_bars())),
        case_block("gap_defaults", "gap, defaults: a gap of 1.5 ATR closed six bars later, two gaps "
                   "open at once and closed by one bar, a gap closed on its own bar",
                   *gap(gap_bars())),
        case_block("gap_close_reference", "gap against the previous close, minimum 0.5 ATR: on the "
                   "minimum, below it, closed by touching the edge",
                   *gap(gap_close_bars(), reference="close", atr_len=5, min_atr=0.5)),
        case_block("gap_horizon", "gap, max_bars 3: dropped unclosed after the last followed bar, "
                   "closed on the last followed bar",
                   *gap(gap_horizon_bars(), atr_len=3, max_bars=3)),
        case_block("gap_flat", "gap, minimum 2 ATR after bars without a range: no size in ATR "
                   "while the ATR is 0",
                   *gap(gap_flat_bars(), atr_len=3, min_atr=2.0)),
        case_block("point_and_figure_defaults", "point_and_figure, defaults (box 1, reversal 3, "
                   "closes): 20 columns, the last one extended by three bars",
                   *point_and_figure(pnf_range_bars())),
        case_block("point_and_figure_high_low", "point_and_figure, box 2, reversal 2, highs and "
                   "lows: the bar is read in the direction of the column first",
                   *point_and_figure(pnf_range_bars(), box="2", reversal=2, method="high_low")),
        case_block("point_and_figure_fine_grid", "point_and_figure, box 0.25, reversal 4, closes "
                   "between the grid lines: quiet bars change nothing",
                   *point_and_figure(fvg_bars(), box="0.25", reversal=4)),
        case_block("gap_walk", "gap on a random walk with jumping opens, minimum 0.25 ATR, "
                   f"max_bars 20: {gap_summary(gap_walk[2])}", *gap_walk),
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
        case_block("wyckoff_textbook_accumulation",
                   "wyckoff, defaults, textbook accumulation: PS, SC, AR, ST, Spring, SOS, LPS; phases A to E",
                   *wyckoff(textbook)),
        case_block("wyckoff_textbook_distribution",
                   "wyckoff, defaults, the textbook series mirrored at 200: PSY, BC, AR, ST, UTAD, SOW, LPSY",
                   *wyckoff(mirror_at(textbook, 200.0))),
        case_block("wyckoff_failed_breakout",
                   "wyckoff, defaults: the pullback after the SOS closes back inside the range (D back to B)",
                   *wyckoff(wyckoff_failed_breakout_bars())),
        case_block("wyckoff_cycle",
                   "wyckoff, defaults: accumulation, markup, distribution; phase E ends when the next range is set",
                   *wyckoff(wyckoff_cycle_bars())),
        case_block("wyckoff_fallback_spring",
                   "wyckoff, breakout_volume off, constant volume: range by the width fallback, bias open until the Spring",
                   *wyckoff(quiet, breakout_volume=0)),
        case_block("wyckoff_fallback_utad",
                   "wyckoff, breakout_volume off, the same bars mirrored at 200: the UTAD decides the open bias",
                   *wyckoff(mirror_at(quiet, 200.0), breakout_volume=0)),
        case_block("wyckoff_no_volume",
                   "wyckoff, defaults, volume 0 on every bar: no climax, no SOS; phase C until the range is abandoned",
                   *wyckoff(wyckoff_quiet_bars(0.0))),
        case_block("wyckoff_no_dispersion",
                   "wyckoff, defaults: from bar 25 on every bar trades 100, the close above the range 101; "
                   "the MAD of the window is zero by then, so 101 is no outlier and there is no SOS",
                   *wyckoff(wyckoff_quiet_volume_bars(True, 101.0))),
        case_block("wyckoff_fallback_carried",
                   "wyckoff, defaults: the same range on varying volumes, the close above it on 300: "
                   "an outlier, so SOS, LPS and phase E",
                   *wyckoff(wyckoff_quiet_volume_bars(False, 300.0))),
        case_block("wyckoff_lps_then_new_range",
                   "wyckoff, range_atr_max 5.9, textbook series: bar 58 reports the LPS and phase E; "
                   "the width fallback sets the next range one bar later, not on the same bar",
                   *wyckoff(textbook, range_atr_max=5.9)),
        case_block("wyckoff_lpsy_then_new_range",
                   "wyckoff, range_atr_max 5.9, the textbook series mirrored at 200: the same with the LPSY",
                   *wyckoff(mirror_at(textbook, 200.0), range_atr_max=5.9)),
        case_block("wyckoff_random_walk",
                   "wyckoff, defaults, random walk without built-in structure: " + wyckoff_summary(walk_run[2]),
                   *walk_run),
        case_block("wyckoff_pullback_held",
                   "wyckoff, defaults: the pullback reaches half a point below the former range high and holds (lps_atr 0.5)",
                   *wyckoff(wyckoff_deep_pullback_bars())),
        case_block("wyckoff_pullback_too_deep",
                   "wyckoff, lps_atr 0.2: the same pullback reaches too far back; no LPS, phase D to the end",
                   *wyckoff(wyckoff_deep_pullback_bars(), lps_atr=0.2)),
        case_block("wyckoff_tuned_cycle",
                   "wyckoff, every threshold off its default, accumulation and distribution: "
                   + wyckoff_summary(tuned_cycle[2]),
                   *tuned_cycle),
        case_block("wyckoff_tuned_walk",
                   "wyckoff, every threshold off its default, the random walk: "
                   + wyckoff_summary(tuned_walk[2]),
                   *tuned_walk),
        case_block("wyckoff_short_lookback",
                   "wyckoff, range_lookback 10: the window is full before the ATR(14) exists",
                   *wyckoff(textbook, range_lookback=10)),
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
