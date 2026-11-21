"""Contract cases of `chart_patterns` and `harmonic_patterns` (part of `contract_structure`).

Both indicators raise one event per pattern instance on the bar its rule holds, so a case is a
price path whose swings are known: a series of flat bars (open = high = low = close) on straight
lines between waypoints ten bars apart, so every waypoint is a ZigZag node at exactly its price.
With the defaults (depth 3, backstep 2, 1 % deviation) a node is confirmed three bars after the
next waypoint, which is how the comments date the decisive bars (0-based bar numbers of the
waypoints, 1-based rows in the fixture). Expected rows come from `kestrel_reference.patterns`.

Conventions the cases rely on, stated where the definitions leave them open:
- Bars are 900 seconds apart from 900 on unless a case sets its timestamps; a line through two
  nodes is straight in time.
- The level tolerance of `chart_patterns` is a percentage of a price, so a series and its
  negation are the same market with highs and lows exchanged: the mirrored case is the series
  with every price negated, and gives the mirrored events.
- Alert strength is 1; two instances of one kind confirmed on one bar are one alert kind with
  `confirmed` 2.
"""

from ..patterns import CHART_KINDS, HARMONIC_TABLE, chart_patterns, harmonic_patterns
from .contract_pilot import case_block

FLAT_VOLUME = 100.0


def waypoint_bars(waypoints):
    """Flat bars on the straight lines between `(bar, price)` waypoints, one bar per index."""
    bars = []
    for (i0, p0), (i1, p1) in zip(waypoints, waypoints[1:]):
        for i in range(i0, i1):
            price = p0 + (p1 - p0) * (i - i0) / (i1 - i0)
            bars.append((price, price, price, price, FLAT_VOLUME))
    last = waypoints[-1][1]
    bars.append((last, last, last, last, FLAT_VOLUME))
    return bars


def negated(bars):
    """The same market with every price negated: highs become lows."""
    return [(-o, -l, -h, -c, v) for o, h, l, c, v in bars]


# --- chart_patterns: one series per geometry ---------------------------------------------------

# Highs 120 / 120.5 within 2 %, the trough 105 between them is the neckline; the second high is
# confirmed on bar 43.
DOUBLE_TOP = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 120.5), (40, 100.0), (50, 110.0)]
DOUBLE_BOTTOM = [(0, 120.0), (10, 100.0), (20, 115.0), (30, 99.5), (40, 120.0), (50, 110.0)]
# Three highs on one level; the neckline is the lower trough 105. The nodes also hold two double
# tops, a rising wedge and a triangle, each followed on its own.
TRIPLE_TOP = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 120.5), (40, 108.0), (50, 119.8),
              (60, 100.0), (70, 110.0)]
TRIPLE_BOTTOM = [(0, 120.0), (10, 100.0), (20, 115.0), (30, 99.5), (40, 112.0), (50, 100.2),
                 (60, 120.0), (70, 110.0)]
# Head 128 over the shoulders 118 and 117.5, troughs 108 and 109: a sloping neckline.
HEAD_AND_SHOULDERS = [(0, 100.0), (10, 118.0), (20, 108.0), (30, 128.0), (40, 109.0),
                      (50, 117.5), (60, 100.0), (70, 110.0)]
INVERSE_HEAD_AND_SHOULDERS = [(0, 130.0), (10, 112.0), (20, 122.0), (30, 102.0), (40, 121.0),
                              (50, 112.5), (60, 130.0), (70, 120.0)]
# Low 100, failed high 112, new low 95 below the first: confirmed by the first close under 95.
ONE_TWO_THREE = [(0, 110.0), (10, 100.0), (20, 112.0), (30, 95.0), (40, 105.0), (50, 90.0),
                 (60, 100.0)]
# Lows 100 / 96 / 88 and highs 112 / 106: the lines 1-3 and 2-4 converge and point 5 undercuts
# the line 1-3. The first three nodes are a bearish 1-2-3 as well.
WOLFE_WAVE = [(0, 110.0), (10, 100.0), (20, 112.0), (30, 96.0), (40, 106.0), (50, 88.0),
              (60, 100.0), (70, 98.0)]
# Falling highs 120 / 115 over rising lows 100 / 104.
TRIANGLE = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 115.0), (40, 104.0), (50, 122.0),
            (60, 110.0)]
TRIANGLE_DOWN = [(0, 110.0), (10, 100.0), (20, 120.0), (30, 105.0), (40, 114.0), (50, 95.0),
                 (60, 105.0)]
# Equal highs at 120 over rising lows: the flat top is broken upward.
ASCENDING_TRIANGLE = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 120.0), (40, 108.0),
                      (50, 130.0), (60, 115.0)]
DESCENDING_TRIANGLE = [(0, 120.0), (10, 100.0), (20, 120.0), (30, 100.0), (40, 112.0),
                       (50, 90.0), (60, 105.0)]
# Rising highs and falling lows: a widening range.
BROADENING = [(0, 105.0), (10, 110.0), (20, 100.0), (30, 115.0), (40, 95.0), (50, 125.0),
              (60, 105.0)]
BROADENING_DOWN = [(0, 105.0), (10, 100.0), (20, 110.0), (30, 95.0), (40, 118.0), (50, 85.0),
                   (60, 100.0)]
# Both boundaries rise, the lower faster (0.3 against 0.15 a bar).
RISING_WEDGE = [(0, 105.0), (10, 100.0), (20, 110.0), (30, 106.0), (40, 113.0), (50, 98.0),
                (60, 105.0)]
FALLING_WEDGE = [(0, 115.0), (10, 120.0), (20, 110.0), (30, 114.0), (40, 107.0), (50, 120.0),
                 (60, 112.0)]
# Pole 100 -> 124, then highs 124 / 121 and lows 116 / 113: both channel lines fall 0.15 a bar,
# and the deepest low gives back 11 of the pole's 24.
FLAG = [(0, 110.0), (10, 100.0), (20, 124.0), (30, 116.0), (40, 121.0), (50, 113.0), (60, 130.0),
        (70, 120.0)]
# Rims 120 and 119 over the bottom 100, handle 112 eight bars after the right rim against a cup
# of 40 bars, above the cup's midpoint.
CUP_AND_HANDLE = [(0, 100.0), (10, 120.0), (30, 100.0), (50, 119.0), (58, 112.0), (70, 125.0),
                  (80, 118.0)]

CHART_SCENARIOS = [
    ("double_top", DOUBLE_TOP), ("double_bottom", DOUBLE_BOTTOM),
    ("triple_top", TRIPLE_TOP), ("triple_bottom", TRIPLE_BOTTOM),
    ("head_and_shoulders", HEAD_AND_SHOULDERS),
    ("inverse_head_and_shoulders", INVERSE_HEAD_AND_SHOULDERS),
    ("one_two_three", ONE_TWO_THREE), ("wolfe_wave", WOLFE_WAVE),
    ("triangle_up", TRIANGLE), ("triangle_down", TRIANGLE_DOWN),
    ("ascending_triangle", ASCENDING_TRIANGLE), ("descending_triangle", DESCENDING_TRIANGLE),
    ("broadening_up", BROADENING), ("broadening_down", BROADENING_DOWN),
    ("rising_wedge", RISING_WEDGE), ("falling_wedge", FALLING_WEDGE),
    ("flag_breakout", FLAG), ("cup_and_handle", CUP_AND_HANDLE),
]

# --- chart_patterns: near misses, rule edges and path variations -------------------------------

# The same double top 125 / 120 apart: 4.2 % is outside the 2 % tolerance, inside 5 %.
NEAR_MISS_DOUBLE_TOP = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 125.0), (40, 100.0),
                        (50, 110.0)]
# Parallel boundaries are a channel, not a pattern.
CHANNEL = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 120.0), (40, 100.0), (50, 120.0),
           (60, 110.0)]
# Highs 120 / 119.95 / 119.9 over lows 100.1 / 100.15: both boundaries move less than 1.5 % of
# the opening gap, so the funnel is a range; with `flat_pct` 0 no boundary is flat and the same
# nodes are a triangle that the fall to 95 breaks.
RANGE = [(0, 100.0), (10, 120.0), (20, 100.1), (30, 119.95), (40, 100.15), (50, 119.9),
         (60, 95.0)]
# The upper boundary leans up by 1 % of the gap: still flat, so an ascending triangle; with
# `flat_pct` 0 it is a rising wedge.
LEANING_TOP = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 120.2), (40, 108.0), (50, 130.0),
               (60, 115.0)]
# The flat top broken downward and the flat bottom broken upward (the negation).
ASCENDING_TRIANGLE_DOWN = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 120.0), (40, 108.0),
                           (50, 114.0), (60, 96.0), (70, 105.0)]
# A high above the 1-2 high before the close under node 3: the 1-2-3 is given up.
ONE_TWO_THREE_GIVEN_UP = [(0, 110.0), (10, 100.0), (20, 112.0), (30, 95.0), (40, 113.0),
                          (50, 100.0), (60, 110.0)]
# The Wolfe wave of WOLFE_WAVE, but price jumps over the 1-4 line after point 5 and later falls
# through point 5 in one bar: continuing, given up (the jump back to 100 instead is a reversion).
WOLFE_JUMP = [(0, 110.0), (10, 100.0), (20, 112.0), (30, 96.0), (40, 106.0), (50, 88.0),
              (51, 115.0), (60, 118.0), (65, 118.0), (66, 80.0), (76, 95.0)]
WOLFE_JUMP_REVERTING = WOLFE_JUMP[:-2] + [(66, 100.0), (76, 95.0)]
# Highs 120 / 120.5 over the troughs 105 and 108: a second high above the first extreme
# invalidates the double top (the close at 121.6 is above its own extreme).
DOUBLE_TOP_GIVEN_UP = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 120.5), (40, 108.0),
                       (50, 125.0), (60, 112.0), (70, 99.0), (80, 110.0)]
# The second counter-swing 118 lies within the level tolerance of the highs: three highs on one
# level, but no triple top (both counter-swings must be farther away).
TRIPLE_TOP_SHALLOW = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 120.5), (40, 118.0),
                      (50, 119.8), (60, 100.0), (70, 110.0)]
# A head at the level of the shoulders is no head and shoulders.
FLAT_HEAD = [(0, 100.0), (10, 118.0), (20, 108.0), (30, 118.0), (40, 109.0), (50, 117.5),
             (60, 100.0), (70, 110.0)]
# The channel of the flag widens by far more than a quarter between its ends (the retracement
# of 11 is within the limit of 12).
WIDE_FLAG = [(0, 110.0), (10, 100.0), (20, 124.0), (30, 120.0), (40, 122.5), (50, 113.0),
             (60, 130.0), (70, 120.0)]
# A flag with parallel boundaries that gives back 19 of the pole's 24, more than half.
DEEP_FLAG = [(0, 110.0), (10, 100.0), (20, 124.0), (30, 110.0), (40, 119.0), (50, 105.0),
             (60, 130.0), (70, 120.0)]
# Both limits met exactly: the channel's width changes by exactly a quarter (6 to 7.5) and the
# flag gives back exactly half of the pole (12 of 24); equality passes both.
LIMIT_FLAG = [(0, 110.0), (10, 100.0), (20, 124.0), (30, 116.0), (40, 121.0), (50, 112.0),
              (60, 130.0), (70, 120.0)]
# A triangle whose lines cross at about bar 48: the nodes up to bar 44 are inside the funnel,
# the closes of the bars 49 to 60 lie between the crossed lines (no breakout), and the rise
# through the rising line at bar 67 confirms it; the fall instead confirms it at bar 66.
APEX_UP = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 114.0), (40, 106.0), (44, 108.6),
           (60, 108.4), (70, 118.0), (80, 105.0)]
APEX_DOWN = [(0, 100.0), (10, 120.0), (20, 100.0), (30, 114.0), (40, 106.0), (44, 108.6),
             (60, 108.4), (70, 98.0), (80, 105.0)]
# The first double top (neckline 105) is found on bar 43 and confirmed on bar 77; three further
# nodes are confirmed by then, so `pending_swings` 3 drops it and 4 keeps it.
PENDING = [(0, 100.0), (10, 120.0), (20, 105.0), (30, 120.5), (40, 108.0), (50, 114.0),
           (60, 109.0), (70, 113.0), (80, 100.0), (90, 110.0)]
# Cup rims 120 (bar 10) and 119 (bar 50), the handle after `handle` bars of the cup of 40: the
# rule allows at most half the cup, so 20 pass and 21 do not.
def cup_with_handle(handle_bar):
    return [(0, 100.0), (10, 120.0), (30, 100.0), (50, 119.0), (handle_bar, 112.0),
            (handle_bar + 10, 125.0), (handle_bar + 20, 118.0)]


# Twenty bars (open, high, low, close): the bars 3, 6 and 15 (1-based) have a range over both
# neighbours, so each is a pivot high and a pivot low at once and the ZigZag gives two adjacent
# nodes with one timestamp. With depth 1 the window of the rising wedge ends at the earlier of
# the two nodes of bar 15 when the first is confirmed; the second node of that bar must not
# accept the same window again.
SHARED_TIMESTAMP = [
    (100.0, 100.5, 100.0, 100.5), (100.5, 100.5, 99.75, 99.75), (99.75, 101.75, 98.75, 100.0),
    (100.0, 100.0, 99.75, 99.75), (99.75, 100.25, 98.75, 99.25), (99.25, 100.5, 97.5, 100.0),
    (100.0, 100.75, 100.0, 100.75), (100.75, 100.75, 100.0, 100.0), (100.0, 100.0, 99.75, 99.75),
    (99.75, 99.75, 99.0, 99.0), (99.0, 99.75, 99.0, 99.75), (99.75, 100.25, 99.75, 100.25),
    (100.25, 101.0, 100.25, 101.0), (101.0, 101.0, 100.0, 100.0), (100.0, 101.75, 99.5, 100.5),
    (100.5, 100.5, 100.5, 100.5), (100.5, 101.25, 100.5, 101.25), (101.25, 101.25, 100.75, 100.75),
    (100.75, 100.75, 99.75, 99.75), (99.75, 99.75, 99.25, 99.25),
]


# --- walks -----------------------------------------------------------------------------------

def swing_walk(count, seed, start=100.0):
    """A random walk in legs: a drift for 6 to 19 bars, then another, noise and wicks on top, two
    decimals. The open is the previous close. Many swings, so most geometries come up and are
    rejected on a bar by bar basis."""
    state = seed

    def uniform():
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        return (state >> 11) / 2 ** 53

    bars, close, drift, left = [], start, 0.0, 0
    for _ in range(count):
        if left == 0:
            drift = (uniform() - 0.5) * 2.4
            left = 6 + int(uniform() * 14)
        left -= 1
        open_ = close
        close = round(open_ + drift + (uniform() - 0.5) * 0.8, 2)
        high = round(max(open_, close) + uniform() * 0.5, 2)
        low = round(min(open_, close) - uniform() * 0.5, 2)
        bars.append((open_, high, low, close, 1000.0))
    return bars


# --- cases -----------------------------------------------------------------------------------

def _params(values):
    return {key: float(value) for key, value in (values or {}).items()}


def chart_case(name, comment, bars, params=None, times=None):
    values = _params(params)
    return case_block("chart_patterns_" + name, comment, values, bars,
                      chart_patterns(values, bars, times), times)


def harmonic_case(name, comment, bars, params=None, times=None):
    values = _params(params)
    return case_block("harmonic_patterns_" + name, comment, values, bars,
                      harmonic_patterns(values, bars, times), times)


def alert_kinds(block):
    """The alert kinds a case block expects anywhere."""
    return {key.split("_", 1)[1] for key in block[2] if key.startswith("alert")}


def with_mirror(make, name, comment, series, **kwargs):
    """The case on `series` and, as `negated_<name>`, on its negation."""
    return [make(name, comment, series, **kwargs),
            make("negated_" + name, "the same market with every price negated: " + comment,
                 negated(series), **kwargs)]


def gap_after(count, index, gap=1000000):
    """Timestamps 900 s apart from 900 on, with `gap` more seconds from bar `index` (0-based)."""
    return [900 * (i + 1) + (gap if i >= index else 0) for i in range(count)]


FINE = {"depth": 2, "backstep": 0, "deviation_pct": 0.5, "atr_len": 5, "pending_swings": 3}
ATR_MODE = {"deviation_mode": 1, "deviation_pct": 2.0, "atr_len": 7, "depth": 3, "backstep": 1,
            "pending_swings": 1}


def chart_cases():
    cases = []

    def add(name, comment, waypoints, mirror=False, **kwargs):
        bars = waypoint_bars(waypoints)
        if mirror:
            cases.extend(with_mirror(chart_case, name, comment, bars, **kwargs))
        else:
            cases.append(chart_case(name, comment, bars, **kwargs))

    scenario_notes = {
        "double_top": "chart_patterns, defaults: two highs within the tolerance, the neckline "
                      "is the trough between; confirmed on bar 44 with the second high",
        "double_bottom": "chart_patterns, defaults: two lows on one level, the neckline is the "
                         "peak between",
        "triple_top": "chart_patterns, defaults: three highs on one level; the double tops, the "
                      "wedge and the triangle in the same nodes are followed on their own",
        "triple_bottom": "chart_patterns, defaults: three lows on one level",
        "head_and_shoulders": "chart_patterns, defaults: a head over two shoulders with a "
                              "sloping neckline (also an inverse cup and a triangle)",
        "inverse_head_and_shoulders": "chart_patterns, defaults: the inverse head and "
                                      "shoulders",
        "one_two_three": "chart_patterns, defaults: a bearish 1-2-3, node 3 below node 1, "
                         "confirmed by the first close under node 3",
        "wolfe_wave": "chart_patterns, defaults: a bullish Wolfe wave, point 5 below the 1-3 "
                      "line, with the 1-4 line as target",
        "triangle_up": "chart_patterns, defaults: falling highs over rising lows, broken "
                       "upward",
        "triangle_down": "chart_patterns, defaults: a triangle broken downward",
        "ascending_triangle": "chart_patterns, defaults: a flat top over rising lows, broken "
                              "upward",
        "descending_triangle": "chart_patterns, defaults: a flat bottom under falling highs, "
                               "broken downward",
        "broadening_up": "chart_patterns, defaults: rising highs and falling lows, left upward",
        "broadening_down": "chart_patterns, defaults: a broadening formation left downward",
        "rising_wedge": "chart_patterns, defaults: both boundaries rise, the lower faster; "
                        "left downward",
        "falling_wedge": "chart_patterns, defaults: both boundaries fall; left upward",
        "flag_breakout": "chart_patterns, defaults: a pole of 24 and a channel against it, "
                         "broken in the pole's direction",
        "cup_and_handle": "chart_patterns, defaults: rims on one level over the bottom, a "
                          "handle above the cup's midpoint, confirmed over the right rim",
    }
    for name, waypoints in CHART_SCENARIOS:
        add(name, scenario_notes[name], waypoints, mirror=True)
    add("downward_ascending_triangle", "chart_patterns, defaults: a flat top over rising lows "
        "broken downward", ASCENDING_TRIANGLE_DOWN, mirror=True)
    add("near_miss_double_top", "chart_patterns, defaults: the second high is 4.2 % above the "
        "first, outside the 2 % tolerance: no double top", NEAR_MISS_DOUBLE_TOP)
    add("loose_near_miss_double_top", "chart_patterns, tolerance_pct 5: the same highs are one "
        "level", NEAR_MISS_DOUBLE_TOP, mirror=True, params={"tolerance_pct": 5.0})
    add("channel", "chart_patterns, defaults: parallel boundaries are a channel, nothing is "
        "found", CHANNEL, mirror=True)
    add("flat_range", "chart_patterns, defaults: both boundaries flat within 1.5 % of the gap: "
        "a range, no triangle or wedge", RANGE, mirror=True)
    add("unflat_range", "chart_patterns, flat_pct 0: no boundary is flat, the same nodes are a "
        "triangle that the fall breaks", RANGE, params={"flat_pct": 0.0})
    add("leaning_top", "chart_patterns, defaults: a top that leans up by 1 % of the gap is "
        "still flat: an ascending triangle, not a wedge", LEANING_TOP, mirror=True)
    add("unflat_leaning_top", "chart_patterns, flat_pct 0: the leaning top is no longer flat, "
        "a rising wedge", LEANING_TOP, params={"flat_pct": 0.0})
    add("apex_up", "chart_patterns, defaults: a triangle whose lines cross at bar 48; closes "
        "between the crossed lines are no breakout, the rise through the rising line is a "
        "bullish one (bar 67)", APEX_UP, mirror=True)
    add("apex_down", "chart_patterns, defaults: the same funnel, left downward through the "
        "falling line past the crossing (bar 66)", APEX_DOWN, mirror=True)
    add("given_up_one_two_three", "chart_patterns, defaults: a close above the 1-2 high before "
        "any close under node 3 gives the 1-2-3 up", ONE_TWO_THREE_GIVEN_UP, mirror=True)
    add("jump_wolfe_wave", "chart_patterns, defaults: price jumps over the 1-4 line after point "
        "5 and falls through point 5 in one bar: the Wolfe wave is given up",
        WOLFE_JUMP, mirror=True)
    add("reverting_jump_wolfe_wave", "chart_patterns, defaults: the same jump, then a fall that "
        "stays above point 5: the reversion confirms the Wolfe wave", WOLFE_JUMP_REVERTING,
        mirror=True)
    add("given_up_double_top", "chart_patterns, defaults: the highs 120 / 120.5, but a later "
        "close above the pattern's extreme gives the double top up", DOUBLE_TOP_GIVEN_UP,
        mirror=True)
    add("shallow_triple_top", "chart_patterns, defaults: three highs on one level, but the "
        "second counter-swing is within the tolerance: no triple top", TRIPLE_TOP_SHALLOW,
        mirror=True)
    add("flat_head", "chart_patterns, defaults: a head at the level of the shoulders is no head "
        "and shoulders", FLAT_HEAD, mirror=True)
    add("wide_flag", "chart_patterns, defaults: the channel widens by far more than a quarter, "
        "no flag", WIDE_FLAG, mirror=True)
    add("deep_flag", "chart_patterns, defaults: parallel boundaries but more than half of the "
        "pole given back, no flag", DEEP_FLAG, mirror=True)
    add("limit_flag", "chart_patterns, defaults: the width changes by exactly a quarter and "
        "half of the pole is given back exactly: equality passes both", LIMIT_FLAG, mirror=True)

    cup = waypoint_bars(CUP_AND_HANDLE)
    cases.extend(with_mirror(
        chart_case, "gapped_cup_and_handle", "chart_patterns, defaults: the cup with a gap of "
        "1000000 seconds between the right rim and the handle: the rule counts bars, so the "
        "cup stands", cup, times=gap_after(len(cup), 54)))
    slow = waypoint_bars(cup_with_handle(75))
    cases.extend(with_mirror(
        chart_case, "slow_cup_and_handle", "chart_patterns, defaults: the handle comes 25 bars "
        "after the right rim against a cup of 40: more than half, no cup", slow))
    cases.extend(with_mirror(
        chart_case, "gapped_slow_cup_and_handle", "chart_patterns, defaults: the same handle "
        "with a gap of 1000000 seconds in the left half of the cup: in seconds the handle "
        "would be short, in bars it is not, no cup", slow, times=gap_after(len(slow), 20)))
    cases.extend(with_mirror(
        chart_case, "limit_cup_and_handle", "chart_patterns, defaults: the handle comes 20 bars "
        "after the right rim, exactly half of the 40-bar cup: equality passes",
        waypoint_bars(cup_with_handle(70))))
    cases.extend(with_mirror(
        chart_case, "late_cup_and_handle", "chart_patterns, defaults: the handle comes 21 bars "
        "after the right rim: one bar too late, no cup", waypoint_bars(cup_with_handle(71))))

    pending = waypoint_bars(PENDING)
    for count in (1, 2, 3, 4, 10):
        cases.append(chart_case(
            f"pending_swings_{count}", f"chart_patterns, pending_swings {count}: the double top "
            "found on bar 43 is followed through the nodes of the bars 40, 50 and 60 and "
            "confirmed on bar 77; it has expired by then below 4",
            pending, {"pending_swings": count}))
    cases.append(chart_case(
        "negated_pending_swings_4", "chart_patterns, pending_swings 4 on the negated market",
        negated(pending), {"pending_swings": 4}))

    shared = [(o, h, l, c, 100.0) for o, h, l, c in SHARED_TIMESTAMP]
    shared_params = {"depth": 1, "backstep": 0, "deviation_pct": 0.05, "pending_swings": 1}
    cases.extend(with_mirror(
        chart_case, "shared_timestamp", "chart_patterns, depth 1: bars that are a pivot high and "
        "a pivot low at once give two nodes with one timestamp; the window ending at the "
        "earlier of them is found once", shared, params=shared_params))

    double_top = waypoint_bars(DOUBLE_TOP)
    cases.extend(with_mirror(
        chart_case, "atr_double_top", "chart_patterns, deviation_mode 1 (2 ATR over 5 bars): "
        "the double top with the deviation in ATR multiples",
        double_top, params={"deviation_mode": 1, "deviation_pct": 2.0, "atr_len": 5}))
    cases.append(chart_case(
        "atr_late_double_top", "chart_patterns, deviation_mode 1 with an ATR over 45 bars: the "
        "ATR does not exist when the nodes would be confirmed, nothing is",
        double_top, {"deviation_mode": 1, "deviation_pct": 2.0, "atr_len": 45}))
    cases.append(chart_case(
        "atr_flat_prices", "chart_patterns, deviation_mode 1 over constant prices: the ATR is 0 "
        "and confirms no swing", [(50.0, 50.0, 50.0, 50.0, 100.0)] * 40,
        {"deviation_mode": 1, "deviation_pct": 1.0, "atr_len": 5}))
    cases.append(chart_case(
        "flat_prices", "chart_patterns, defaults over constant prices: no swing",
        [(50.0, 50.0, 50.0, 50.0, 100.0)] * 40))

    walk, other = swing_walk(260, 20261121), swing_walk(260, 20261122)
    cases.append(chart_case("walk_defaults", "chart_patterns, defaults over 260 bars of a swing "
                            "walk", walk))
    cases.append(chart_case("negated_walk_defaults", "chart_patterns, defaults over the "
                            "negated walk", negated(walk)))
    cases.append(chart_case("walk_fine", "chart_patterns, depth 2, backstep 0, 0.5 %, ATR over 5, "
                            "tolerance 3, flat 3, pending 3 over the walk", walk,
                            dict(FINE, tolerance_pct=3.0, flat_pct=3.0)))
    cases.append(chart_case("walk_atr_multiple", "chart_patterns, deviation_mode 1 (2 ATR over "
                            "7), backstep 1, pending 1 over a second walk", other, ATR_MODE))
    cases.append(chart_case("walk_wide_levels", "chart_patterns, tolerance_pct 50, flat_pct 100, "
                            "pending_swings 100, depth 2, backstep 0, 0.5 % over the second "
                            "walk", other, {"depth": 2, "backstep": 0, "deviation_pct": 0.5,
                                            "tolerance_pct": 50.0, "flat_pct": 100.0,
                                            "pending_swings": 100}))
    cases.append(chart_case("walk_minimal_levels", "chart_patterns, depth 1, backstep 0, "
                            "deviation 0.001 %, ATR over 1, tolerance 0.001, flat 0, pending 1 "
                            "over the second walk", other,
                            {"depth": 1, "backstep": 0, "deviation_pct": 0.001, "atr_len": 1,
                             "tolerance_pct": 0.001, "flat_pct": 0.0, "pending_swings": 1}))
    cases.append(chart_case("walk_widest_deviation", "chart_patterns, depth 10, backstep 10, "
                            "deviation 100 %, ATR over 100, pending 100: no swing is confirmed",
                            other, {"depth": 10, "backstep": 10, "deviation_pct": 100.0,
                                    "atr_len": 100, "pending_swings": 100}))
    return cases


# --- harmonic_patterns -----------------------------------------------------------------------

def xabcd(b, c, d, rally=125.0):
    """X = 100, A = 120 and the prices of B, C and D; D is a swing low confirmed on bar 63 (the
    rally to `rally` is the next waypoint)."""
    return [(0, 110.0), (10, 100.0), (20, 120.0), (30, b), (40, c), (50, d), (60, rally),
            (70, 110.0)]


# (name, B, C, D): ratios inside the row's intervals on every leg.
ROWS = [
    ("gartley", 107.64, 113.92, 104.3),
    ("bat", 111.18, 117.33, 102.3),
    ("butterfly", 104.3, 111.29, 94.96),
    ("crab", 108.82, 117.32, 88.0),
    ("deep_crab", 102.3, 110.18, 89.0),
    ("shark", 110.0, 122.52, 100.2),
]

# (name, side of the D interval, B, C): B and C inside their rows, so only D decides.
EDGE_ROWS = [
    ("gartley", "hi", 107.64, 115.4762),
    ("bat", "lo", 111.18, 116.7719),
    ("butterfly", "hi", 104.3, 116.232),
    ("crab", "lo", 110.0, 117.6),
    ("deep_crab", "hi", 102.3, 113.5218),
    ("shark", "lo", 110.0, 121.3),
]
TOLERANCE = 0.03


def edge_d(name, side, c, inside):
    """D at 0.004 of the leg inside or outside the row's D interval widened by the default
    ratio tolerance, on its upper or lower end; measured from XA, for the shark from XC."""
    row = next(row for row in HARMONIC_TABLE if row[0] == name)
    interval = row[3]
    end = interval[1] + TOLERANCE if side == "hi" else interval[0] - TOLERANCE
    ratio = end - 0.004 if (side == "hi") == inside else end + 0.004
    anchor = c if row[4] == "XC" else 120.0
    return round(anchor - ratio * (anchor - 100.0), 4)


def harmonic_cases():
    cases = []

    def add(name, comment, bars, mirror=False, **kwargs):
        if mirror:
            cases.extend(with_mirror(harmonic_case, name, comment, bars, **kwargs))
        else:
            cases.append(harmonic_case(name, comment, bars, **kwargs))

    for name, b, c, d in ROWS:
        add(name, f"harmonic_patterns, defaults: a {name.replace('_', ' ')} with every leg "
            "inside its row of the table, D a swing low confirmed on bar 63 and a rally out of "
            "the zone", waypoint_bars(xabcd(b, c, d)), mirror=True)
    for name, side, b, c in EDGE_ROWS:
        inside = waypoint_bars(xabcd(b, c, edge_d(name, side, c, True)))
        outside = waypoint_bars(xabcd(b, c, edge_d(name, side, c, False)))
        end = "upper" if side == "hi" else "lower"
        stands = harmonic_case(
            f"edge_inside_{name}", f"harmonic_patterns, defaults: D just inside the {end} end of "
            f"the {name.replace('_', ' ')}'s D interval widened by 0.03: the pattern stands",
            inside)
        gone = with_mirror(
            harmonic_case, f"edge_outside_{name}", f"harmonic_patterns, defaults: D just outside "
            f"the {end} end of the {name.replace('_', ' ')}'s D interval widened by 0.03, every "
            "other leg unchanged inside its row: nothing is found", outside)
        assert f"bull_{name}" in alert_kinds(stands), name
        assert not alert_kinds(gone[0]) and not alert_kinds(gone[1]), name
        cases.extend([stands] + gone)

    add("between_rows", "harmonic_patterns, defaults: B at 0.70 of XA lies between Gartley and "
        "Butterfly: no pattern", waypoint_bars(xabcd(106.0, 112.0, 104.3)), mirror=True)
    add("default_tolerance_b_leg", "harmonic_patterns, defaults: B at 0.66 of XA is outside Gartley's "
        "0.586 to 0.65 and inside it widened by 0.03", waypoint_bars(xabcd(106.8, 113.92, 104.3)),
        mirror=True)
    add("zero_tolerance_b_leg", "harmonic_patterns, ratio_tolerance 0: the same B is outside",
        waypoint_bars(xabcd(106.8, 113.92, 104.3)), params={"ratio_tolerance": 0.0})
    add("zero_tolerance_rows", "harmonic_patterns, ratio_tolerance 0: a Gartley whose legs are "
        "inside their intervals still stands, the zone without the widening",
        waypoint_bars(xabcd(107.64, 113.92, 104.3)), mirror=True, params={"ratio_tolerance": 0.0})
    add("zero_tolerance_edge", "harmonic_patterns, ratio_tolerance 0: B at exactly 0.65 of XA "
        "(the end of Gartley's interval, inclusive), C at 0.5 of AB, D at 0.8 of XA",
        waypoint_bars(xabcd(107.0, 113.5, 104.0)), mirror=True, params={"ratio_tolerance": 0.0})
    gartley = waypoint_bars(xabcd(107.64, 113.92, 104.3))
    add("wide_zone", "harmonic_patterns, min_prz_share 0.5: the zone of 4.04 is widened to half "
        "of XA around its middle", gartley, mirror=True, params={"min_prz_share": 0.5})
    add("narrow_zone", "harmonic_patterns, min_prz_share 0: the zone is the span of the "
        "projections", gartley, params={"min_prz_share": 0.0})
    add("atr_multiple", "harmonic_patterns, deviation_mode 1 (2 ATR over 7 bars): the Gartley "
        "with the deviation in ATR multiples", gartley,
        params={"deviation_mode": 1, "deviation_pct": 2.0, "atr_len": 7})

    def bat(rally):
        return [(0, 110.0), (10, 100.0), (20, 120.0), (30, 111.18), (40, 117.33), (50, 101.2),
                (60, rally), (70, 95.0), (80, 110.0)]
    add("rally_from_bat_zone", "harmonic_patterns, defaults: a bat with D at 0.94 of XA (inside "
        "the widened interval, inside its zone 101.0 to 108.51); the rally out of the zone "
        "confirms it", waypoint_bars(bat(125.0)), mirror=True)
    add("given_up_bat_zone", "harmonic_patterns, defaults: the same bat, but the swing after D stops "
        "at 102.25, inside the zone, and the fall to 95 closes through its low edge: the "
        "structure is given up", waypoint_bars(bat(102.25)), mirror=True)

    expiry = [(0, 110.0), (10, 100.0), (20, 120.0), (30, 107.64), (40, 113.92), (50, 104.3),
              (60, 105.5), (70, 102.0), (80, 105.5), (90, 102.0), (100, 110.0), (110, 100.0)]
    for count in (2, 3, 4, 5):
        add(f"pending_swings_{count}", f"harmonic_patterns, pending_swings {count}: after D the "
            "swings stay inside the zone for four nodes and leave it upward on bar 96; the "
            "structure expires before that for small values", waypoint_bars(expiry),
            mirror=count in (2, 5), params={"pending_swings": count})

    walk, other, third = (swing_walk(260, 20261121), swing_walk(260, 20261122),
                          swing_walk(260, 20261123))
    add("walk_defaults", "harmonic_patterns, defaults over 260 bars of a swing walk", walk)
    wide = {"ratio_tolerance": 0.5, "min_prz_share": 0.2, "depth": 2, "backstep": 0,
            "deviation_pct": 0.5, "atr_len": 5, "pending_swings": 3}
    add("walk_wide_tolerance", "harmonic_patterns, ratio_tolerance 0.5, min_prz_share 0.2, depth "
        "2, 0.5 % over a swing walk: many rows match, some together", third, params=wide)
    add("negated_walk_wide_tolerance", "harmonic_patterns, the same parameters over the negated "
        "walk", negated(third), params=wide)
    add("walk_atr_multiple", "harmonic_patterns, deviation_mode 1 (1.5 ATR over 5), "
        "ratio_tolerance 0.5, pending 5 over a second walk", other,
        params={"ratio_tolerance": 0.5, "deviation_mode": 1, "deviation_pct": 1.5,
                "atr_len": 5, "depth": 2, "pending_swings": 5})
    add("walk_third_tolerance", "harmonic_patterns, ratio_tolerance 0.3, backstep 1, 0.8 %, "
        "pending 4 over the third walk", third, params={"ratio_tolerance": 0.3, "depth": 2,
                                                 "backstep": 1, "deviation_pct": 0.8,
                                                 "pending_swings": 4})
    add("walk_minimal_levels", "harmonic_patterns, depth 1, backstep 0, 0.001 %, ATR over 1, "
        "tolerance 0, share 0, pending 1 over a swing walk", other,
        params={"depth": 1, "backstep": 0, "deviation_pct": 0.001, "atr_len": 1,
                "ratio_tolerance": 0.0, "min_prz_share": 0.0, "pending_swings": 1})
    add("walk_widest_levels", "harmonic_patterns, depth 2, backstep 2, 1 %, ratio_tolerance 0.5, "
        "min_prz_share 1, pending 100 over the third walk", third,
        params={"depth": 2, "backstep": 2, "deviation_pct": 1.0, "atr_len": 3,
                "ratio_tolerance": 0.5, "min_prz_share": 1.0, "pending_swings": 100})
    add("walk_widest_deviation", "harmonic_patterns, depth 10, backstep 10, 100 %, ATR over 100, "
        "pending 100: no swing is confirmed", other,
        params={"depth": 10, "backstep": 10, "deviation_pct": 100.0, "atr_len": 100,
                "pending_swings": 100})
    add("flat_prices", "harmonic_patterns, defaults over constant prices: no swing",
        [(50.0, 50.0, 50.0, 50.0, 100.0)] * 40)
    return cases


# Every alert kind an entry can raise: the boundary patterns, the 1-2-3, the Wolfe wave and the
# flag in both directions, the reversal family and the cups in the direction their name implies.
CHART_ALERTS = (
    {f"{side}_{kind}" for side in ("bull", "bear")
     for kind in CHART_KINDS[:8] + ("flag",)}
    | {"bear_double_top", "bull_double_bottom", "bear_triple_top", "bull_triple_bottom",
       "bear_head_and_shoulders", "bull_inverse_head_and_shoulders", "bull_cup_and_handle",
       "bear_inverse_cup_and_handle"})
HARMONIC_ALERTS = {f"{side}_{row[0]}" for side in ("bull", "bear") for row in HARMONIC_TABLE}


def pattern_event_cases():
    """All cases of both entries. A case name must not be followed by `_` at the start of
    another case's name (the loader reads keys by prefix), and every alert kind either entry can
    raise must occur in some case."""
    blocks = chart_cases() + harmonic_cases()
    names = [block[0] for block in blocks]
    for name in names:
        assert not any(other != name and other.startswith(name + "_") for other in names), name
    chart = set().union(*(alert_kinds(b) for b in blocks if b[0].startswith("chart_patterns_")))
    harmonic = set().union(*(alert_kinds(b) for b in blocks if b[0].startswith("harmonic_")))
    assert chart == CHART_ALERTS, sorted(CHART_ALERTS ^ chart)
    assert harmonic == HARMONIC_ALERTS, sorted(HARMONIC_ALERTS ^ harmonic)
    return blocks
