"""Pattern events of ZigZag swings: chart_patterns and harmonic_patterns.

Written from the documented rules, independently of the Rust code. Python standard library only.

Both indicators read the same swing feed. A ZigZag (depth, backstep, deviation in percent of the
price or in multiples of the ATR) runs over the bars; every time it fixes a node, the newest five
confirmed nodes are scanned for patterns that end at that node. A pattern instance is found once,
on the bar that confirms its last anchor. The bars after that anchor are replayed through the
instance's lifecycle rule, a later bar is followed live, and the bar on which the rule holds
raises the event. An instance that has seen `pending_swings` further nodes confirmed behind its
last anchor without a decision is dropped.

`chart_patterns(params, bars, times)` and `harmonic_patterns(params, bars, times)` take `bars`
(tuples `(open, high, low, close, volume)`) and the bar timestamps in seconds (default: 900 apart
from 900 on) and return one row per bar: `(None, {})` before the swing feed is ready, otherwise
`((direction, extras), alerts)` with `alerts` mapping each alert kind to its strength (always 1).
Invalid parameters raise `ValueError`.
"""

import math
import sys

INF = float("inf")
EPSILON = sys.float_info.epsilon

CHART_KINDS = (
    "triangle", "ascending_triangle", "descending_triangle", "broadening", "rising_wedge",
    "falling_wedge", "one_two_three", "wolfe_wave", "double_top", "double_bottom", "triple_top",
    "triple_bottom", "head_and_shoulders", "inverse_head_and_shoulders", "flag",
    "cup_and_handle", "inverse_cup_and_handle",
)
CHART_CODES = {kind: float(code) for code, kind in enumerate(CHART_KINDS, start=1)}
BOUNDARY_KINDS = ("triangle", "ascending_triangle", "descending_triangle", "broadening",
                  "rising_wedge", "falling_wedge")
REVERSAL_KINDS = ("double_top", "double_bottom", "triple_top", "triple_bottom",
                  "head_and_shoulders", "inverse_head_and_shoulders")
TOP_KINDS = ("double_top", "triple_top", "head_and_shoulders")

# (name, B/XA, C/AB, D, basis of D, CD/BC): every interval is inclusive and is widened by the
# ratio tolerance on both sides. D is measured against XA, for the shark against XC.
HARMONIC_TABLE = (
    ("gartley", (0.586, 0.65), (0.382, 0.886), (0.75, 0.82), "XA", (1.13, 1.618)),
    ("bat", (0.382, 0.5), (0.382, 0.886), (0.85, 0.92), "XA", (1.618, 2.618)),
    ("butterfly", (0.75, 0.82), (0.382, 0.886), (1.27, 1.618), "XA", (1.618, 2.618)),
    ("crab", (0.382, 0.618), (0.382, 0.886), (1.5, 1.7), "XA", (2.24, 3.618)),
    ("deep_crab", (0.85, 0.92), (0.382, 0.886), (1.5, 1.7), "XA", (2.24, 3.618)),
    ("shark", (0.382, 0.618), (1.13, 1.618), (0.886, 1.13), "XC", (1.618, 2.24)),
)

WINDOW = 5      # nodes a scan looks at: the longest pattern has five anchors


# --- parameters ------------------------------------------------------------------------------

def _finite(value):
    if isinstance(value, bool) or not isinstance(value, (int, float)) or not math.isfinite(value):
        raise ValueError(f"{value!r} is not a finite number")
    return float(value)


def _count(params, key, default, lo, hi):
    value = _finite(params.get(key, default))
    if value != math.floor(value) or not lo <= value <= hi:
        raise ValueError(f"{key} must be a whole number from {lo} to {hi}")
    return int(value)


def _number(params, key, default, lo, hi):
    value = _finite(params.get(key, default))
    if not lo <= value <= hi:
        raise ValueError(f"{key} must lie between {lo} and {hi}")
    return value


def _resolve(params, extra):
    """The ZigZag parameters, the pending swings and the entry's own `extra` (a mapping from
    key to `(default, lo, hi)`). Unknown keys and values outside their range raise."""
    params = dict(params or {})
    known = {"depth", "backstep", "deviation_mode", "deviation_pct", "atr_len",
             "pending_swings", *extra}
    for key in params:
        if key not in known:
            raise ValueError(f"unknown parameter {key!r}")
    mode = _count(params, "deviation_mode", 0, 0, 1)
    resolved = {
        "depth": _count(params, "depth", 3, 1, 10000),
        "backstep": _count(params, "backstep", 2, 0, 10000),
        "atr_mode": mode == 1,
        "deviation": _number(params, "deviation_pct", 1.0, 0.001, 1000.0 if mode else 100.0),
        "atr_len": _count(params, "atr_len", 14, 1, 10000),
        "pending": _count(params, "pending_swings", 2, 1, 100),
    }
    for key, (default, lo, hi) in extra.items():
        resolved[key] = _number(params, key, default, lo, hi)
    return resolved


# --- the swing feed --------------------------------------------------------------------------

class Node:
    """A ZigZag node: timestamp, price, high or low, confirmed once the next swing is fixed."""

    __slots__ = ("time", "price", "is_high", "confirmed")

    def __init__(self, time, price, is_high):
        self.time = time
        self.price = price
        self.is_high = is_high
        self.confirmed = False


def _signed_move(is_high, price, reference, relative):
    move = price - reference if is_high else reference - price
    if not relative:
        return move
    if reference != 0.0:
        return move / abs(reference)
    return INF if move > 0.0 else -INF


class ZigZag:
    """The advanced ZigZag as a stream: `push(bar)` takes `(time, high, low, close)` and returns
    False until `2 * depth + 1` bars are in. The bar `depth` bars back is a pivot high (low) if
    no bar of the window is above (below) it, ties allowed; one that is both takes the kind of
    the running node first, and starts nothing before the first node. A pivot of the running
    node's kind that lies beyond it moves it; a pivot of the other kind confirms the running node
    when its move is at least the deviation, it lies beyond the running node and `backstep` bars
    have passed since the last confirmation, and becomes the new running node."""

    def __init__(self, depth, backstep, atr_mode, deviation, atr_len):
        self.depth = depth
        self.backstep = backstep
        self.atr_mode = atr_mode
        self.deviation = deviation
        self.atr_len = atr_len
        self.true_ranges = []       # seed of the ATR
        self.atr = None
        self.previous_close = None
        self.window = []
        self.index = 0
        self.nodes = []
        self.direction = 0          # +1: the running node is a high, -1 a low, 0 none yet
        self.last_confirmed = None

    def _update_atr(self, high, low, close):
        if self.previous_close is None:
            true_range = high - low
        else:
            true_range = max(max(high - low, abs(high - self.previous_close)),
                             abs(low - self.previous_close))
        self.previous_close = close
        if self.atr is not None:
            alpha = 1.0 / float(self.atr_len)
            self.atr = alpha * true_range + (1.0 - alpha) * self.atr
            return
        self.true_ranges.append(true_range)
        if len(self.true_ranges) == self.atr_len:
            total = 0.0
            for value in self.true_ranges:
                total = total + value
            self.atr = total / float(self.atr_len)

    def push(self, bar):
        _, high, low, close = bar
        index = self.index
        self.index += 1
        self._update_atr(high, low, close)
        self.window.append(bar)
        size = 2 * self.depth + 1
        if len(self.window) > size:
            self.window.pop(0)
        if len(self.window) < size:
            return False
        mid = self.window[self.depth]
        mid_index = index - self.depth
        is_high = all(mid[1] >= b[1] for b in self.window)
        is_low = all(mid[2] <= b[2] for b in self.window)
        if not self.atr_mode:
            threshold = self.deviation / 100.0
        else:
            threshold = self.deviation * self.atr if self.atr is not None and self.atr > 0.0 \
                else INF
        backstep_ok = self.last_confirmed is None or \
            mid_index >= self.last_confirmed + self.backstep
        running_high = self.nodes[-1].is_high if self.nodes else None
        if is_high and is_low:
            order = [] if running_high is None else [running_high, not running_high]
        elif is_high:
            order = [True]
        elif is_low:
            order = [False]
        else:
            order = []
        for kind in order:
            price = mid[1] if kind else mid[2]
            self._offer(kind, price, mid[0], mid_index, threshold, backstep_ok)
        return True

    def _offer(self, is_high, price, time, index, threshold, backstep_ok):
        wanted = 1 if is_high else -1
        if self.direction in (0, wanted):
            last = self.nodes[-1] if self.nodes else None
            same = last is not None and not last.confirmed and last.is_high == is_high
            if same and not ((price > last.price) if is_high else (price < last.price)):
                return
            if same:
                self.nodes[-1] = Node(time, price, is_high)
            else:
                self.nodes.append(Node(time, price, is_high))
            self.direction = wanted
            return
        running = self.nodes[-1].price
        move = _signed_move(is_high, price, running, not self.atr_mode)
        beyond = price > running if is_high else price < running
        if move >= threshold and beyond and backstep_ok:
            self.nodes[-1].confirmed = True
            self.nodes.append(Node(time, price, is_high))
            self.direction = wanted
            self.last_confirmed = index


class SwingFeed:
    """A ZigZag over the bars that reports each node as it confirms: `push(bar)` returns None
    while the ZigZag is not ready, otherwise one `(tail, clock)` per node confirmed on this bar,
    where `tail` is the newest up to five confirmed nodes ending at that node and `clock` the
    index of the bar of each of them. `bars_after(time)` lists the bars since a timestamp."""

    def __init__(self, depth, backstep, atr_mode, deviation, atr_len):
        self.zigzag = ZigZag(depth, backstep, atr_mode, deviation, atr_len)
        self.bars = []
        self.seen = 0               # confirmed nodes handed out so far
        self.node_bars = []         # bar index of each confirmed node
        self.bar_of_time = {}

    def push(self, bar):
        ready = self.zigzag.push(bar)
        self.bar_of_time[bar[0]] = len(self.bars)
        self.bars.append(bar)
        nodes = self.zigzag.nodes
        confirmed = max(len(nodes) - 1, 0)
        for index in range(self.seen, confirmed):
            self.node_bars.append(self.bar_of_time[nodes[index].time])
        fresh = []
        for index in range(self.seen, confirmed):
            first = max(index + 1 - WINDOW, 0)
            fresh.append((nodes[first:index + 1], self.node_bars[first:index + 1]))
        self.seen = confirmed
        return fresh if ready else None

    def bars_after(self, time):
        return [bar for bar in self.bars if bar[0] > time]


# --- the event loop of both entries ----------------------------------------------------------

FORMING, CONFIRMED, INVALIDATED = 0, 1, 2


def _run(bars, times, resolved, scan, follow, event):
    """The bars through the swing feed and the instances it finds. `scan(tail, clock)` yields
    the instances ending at the newest node of `tail`, `follow(instance, bar)` is the lifecycle
    rule of one, `event(instance, bar)` the event of an instance the rule confirmed on `bar`.
    An event is `(alert kind, direction, extras)`."""
    times = [900 * (i + 1) for i in range(len(bars))] if times is None else times
    assert len(times) == len(bars)
    feed = SwingFeed(resolved["depth"], resolved["backstep"], resolved["atr_mode"],
                     resolved["deviation"], resolved["atr_len"])
    live, direction, rows = [], 0.0, []
    for (_, high, low, close, _), time in zip(bars, times):
        bar = (time, high, low, close)
        events, kept = [], []
        for item in live:
            state = follow(item[0], bar)
            if state == FORMING:
                kept.append(item)
            elif state == CONFIRMED:
                events.append(event(item[0], bar))
        live = kept
        fresh = feed.push(bar)
        count = feed.seen
        first = count - (0 if fresh is None else len(fresh))
        for offset, (tail, clock) in enumerate(fresh or []):
            for instance in scan(tail, clock):
                state, decided = FORMING, None
                for later in feed.bars_after(tail[-1].time):
                    state = follow(instance, later)
                    if state != FORMING:
                        decided = later
                        break
                if state == CONFIRMED:
                    events.append(event(instance, decided))
                elif state == FORMING:
                    live.append((instance, first + offset))
        live = [item for item in live if max(count - 1 - item[1], 0) < resolved["pending"]]
        if fresh is None:
            rows.append((None, {}))
            continue
        if events:
            direction = events[-1][1]
        extras = {"active": float(len(live)), "confirmed": float(len(events))}
        if events:
            extras.update(events[0][2])
        rows.append(((direction, extras), {kind: 1.0 for kind, _, _ in events}))
    return rows


def _alert(bullish, kind):
    return ("bull_" if bullish else "bear_") + kind


# --- chart_patterns --------------------------------------------------------------------------

class Line:
    """The straight line through two nodes, in time; a horizontal line from one node twice."""

    def __init__(self, a, b):
        self.time = a.time
        self.price = a.price
        span = float(b.time - a.time)
        self.slope = 0.0 if span == 0.0 else (b.price - a.price) / span

    def at(self, time):
        return self.price + self.slope * float(time - self.time)


def _clamp(x):
    return min(max(x, 0.0), 1.0)


def _alternate(nodes):
    return all(a.is_high != b.is_high for a, b in zip(nodes, nodes[1:]))


class ChartInstance:
    def __init__(self, kind, nodes, upper, lower, fit):
        self.kind = kind
        self.nodes = nodes
        self.upper = upper
        self.lower = lower
        self.fit = fit


def _boundary(window, flat):
    """Triangle, wedge or broadening over four alternating nodes, or None."""
    if not _alternate(window):
        return None
    highs = [n for n in window if n.is_high]
    lows = [n for n in window if not n.is_high]
    upper, lower = Line(highs[0], highs[1]), Line(lows[0], lows[1])
    start, end = window[0].time, window[3].time
    gap_start = upper.at(start) - lower.at(start)
    gap_end = upper.at(end) - lower.at(end)
    if gap_start <= 0.0 or gap_end <= 0.0:
        return None
    limit = flat / 100.0
    upper_flat = abs(upper.at(end) - upper.at(start)) / max(gap_start, 1e-9) < limit
    lower_flat = abs(lower.at(end) - lower.at(start)) / max(gap_start, 1e-9) < limit
    up, down = upper.slope, lower.slope
    if gap_end > gap_start:
        if upper_flat or lower_flat or up <= 0.0 or down >= 0.0:
            return None
        return ChartInstance("broadening", window, upper, lower,
                             _clamp(1.0 - (gap_start / gap_end)))
    if gap_end >= gap_start or (upper_flat and lower_flat):
        return None
    if upper_flat and not lower_flat and down > 0.0:
        kind = "ascending_triangle"
    elif lower_flat and not upper_flat and up < 0.0:
        kind = "descending_triangle"
    elif up > 0.0 and down > 0.0:
        kind = "rising_wedge"
    elif up < 0.0 and down < 0.0:
        kind = "falling_wedge"
    elif up <= 0.0 and down >= 0.0:
        kind = "triangle"
    else:
        return None
    return ChartInstance(kind, window, upper, lower, _clamp(1.0 - (gap_end / gap_start)))


def _one_two_three(window):
    n1, n2, n3 = window
    if n1.is_high == n2.is_high or n2.is_high == n3.is_high:
        return None
    bearish = not n1.is_high and n2.is_high and not n3.is_high and n3.price < n1.price
    bullish = n1.is_high and not n2.is_high and n3.is_high and n3.price > n1.price
    if not (bearish or bullish):
        return None
    move = abs(n3.price - n1.price) / max(abs(n1.price), 1e-9)
    return ChartInstance("one_two_three", window, None, None, min(move, 1.0))


def _wolfe(window):
    if not _alternate(window):
        return None
    n1, n2, n3, n4, n5 = window
    sign = -1.0 if n1.is_high else 1.0
    if sign * (n3.price - n1.price) >= 0.0:
        return None
    if not (sign * (n4.price - n1.price) > 0.0 and sign * (n4.price - n2.price) < 0.0):
        return None
    line13, line24 = Line(n1, n3), Line(n2, n4)
    if sign * (line24.slope - line13.slope) >= 0.0:
        return None
    if sign * (n5.price - line13.at(n5.time)) >= 0.0:
        return None
    gap_first = sign * (line24.at(n1.time) - line13.at(n1.time))
    gap_last = sign * (line24.at(n5.time) - line13.at(n5.time))
    fit = _clamp(1.0 - gap_last / max(gap_first, 1e-9))
    upper, lower = (line13, line24) if n1.is_high else (line24, line13)
    return ChartInstance("wolfe_wave", window, upper, lower, fit)


class _Levels:
    """Prices that count as one level: within `tolerance` percent of a scale price."""

    def __init__(self, tolerance):
        self.tolerance = tolerance

    def within(self, a, b, scale):
        return abs(a - b) <= (self.tolerance / 100.0) * max(abs(scale), 1e-9)

    def match(self, a, b, scale):
        allowed = (self.tolerance / 100.0) * max(abs(scale), 1e-9)
        return _clamp(1.0 - abs(a - b) / allowed)


def _double(window, levels):
    n1, n2, n3 = window
    if n1.is_high != n3.is_high or n1.is_high == n2.is_high:
        return None
    if not levels.within(n1.price, n3.price, n1.price) or \
            levels.within(n1.price, n2.price, n1.price):
        return None
    neckline = Line(n2, n2)
    fit = levels.match(n1.price, n3.price, n1.price)
    if n1.is_high:
        return ChartInstance("double_top", window, None, neckline, fit)
    return ChartInstance("double_bottom", window, neckline, None, fit)


def _triple(window, levels):
    if not _alternate(window):
        return None
    n1, n2, n3, n4, n5 = window
    if not (levels.within(n1.price, n3.price, n1.price)
            and levels.within(n1.price, n5.price, n1.price)
            and not levels.within(n1.price, n2.price, n1.price)
            and not levels.within(n1.price, n4.price, n1.price)):
        return None
    if n1.is_high:
        deepest = n2 if n2.price <= n4.price else n4
    else:
        deepest = n2 if n2.price >= n4.price else n4
    neckline = Line(deepest, deepest)
    fit = levels.match(n1.price, n3.price, n1.price) * levels.match(n1.price, n5.price, n1.price)
    if n1.is_high:
        return ChartInstance("triple_top", window, None, neckline, fit)
    return ChartInstance("triple_bottom", window, neckline, None, fit)


def _head_and_shoulders(window, levels):
    if not _alternate(window):
        return None
    n1, n2, n3, n4, n5 = window
    if n1.is_high:
        head = n3.price > n1.price and n3.price > n5.price
    else:
        head = n3.price < n1.price and n3.price < n5.price
    if not head or not levels.within(n1.price, n5.price, n3.price) \
            or not levels.within(n2.price, n4.price, n3.price):
        return None
    neckline = Line(n2, n4)
    fit = levels.match(n1.price, n5.price, n3.price) * levels.match(n2.price, n4.price, n3.price)
    if n1.is_high:
        return ChartInstance("head_and_shoulders", window, None, neckline, fit)
    return ChartInstance("inverse_head_and_shoulders", window, neckline, None, fit)


FLAG_PARALLEL = 0.25        # most the channel's width may change between its ends, relative
FLAG_RETRACE = 0.5          # most of the pole the flag may give back


def _flag(window):
    if not _alternate(window):
        return None
    n1, n2, n3, n4, n5 = window
    sign = -1.0 if n1.is_high else 1.0
    pole = sign * (n2.price - n1.price)
    if pole <= 0.0:
        return None
    if sign * (n4.price - n2.price) >= 0.0 or sign * (n5.price - n3.price) >= 0.0:
        return None
    pole_side, far_side = Line(n2, n4), Line(n3, n5)
    gap_start = sign * (pole_side.at(n2.time) - far_side.at(n2.time))
    gap_end = sign * (pole_side.at(n5.time) - far_side.at(n5.time))
    if gap_start <= 0.0 or gap_end <= 0.0:
        return None
    change = abs(gap_end - gap_start) / gap_start
    if change > FLAG_PARALLEL:
        return None
    deepest = min(n3.price, n5.price) if sign > 0.0 else max(n3.price, n5.price)
    if sign * (n2.price - deepest) > FLAG_RETRACE * pole:
        return None
    upper, lower = (pole_side, far_side) if sign > 0.0 else (far_side, pole_side)
    return ChartInstance("flag", window, upper, lower, _clamp(1.0 - change / FLAG_PARALLEL))


def _cup(window, clock, levels):
    """Cup and handle (rims are highs) or its inverse. The handle may take at most half the
    cup's time, counted in bars (`clock`), not in seconds."""
    if not _alternate(window):
        return None
    left, bottom, right, handle = window
    sign = 1.0 if left.is_high else -1.0
    if not levels.within(left.price, right.price, left.price):
        return None
    if levels.within(left.price, bottom.price, left.price):
        return None
    if sign * (handle.price - (right.price + bottom.price) / 2.0) < 0.0:
        return None
    if 2 * (clock[3] - clock[2]) > clock[2] - clock[0]:
        return None
    rim = Line(right, right)
    fit = levels.match(left.price, right.price, left.price)
    if sign > 0:
        return ChartInstance("cup_and_handle", window, rim, None, fit)
    return ChartInstance("inverse_cup_and_handle", window, None, rim, fit)


def _scan_chart(tail, clock, levels, flat):
    """The instances ending at the newest node of `tail`, in the order the scanners run."""
    found = []
    if len(tail) >= 4:
        found.append(_boundary(tail[-4:], flat))
    if len(tail) >= 3:
        found.append(_one_two_three(tail[-3:]))
    if len(tail) >= 5:
        found.append(_wolfe(tail[-5:]))
    if len(tail) >= 3:
        found.append(_double(tail[-3:], levels))
    if len(tail) >= 5:
        found.append(_triple(tail[-5:], levels))
        found.append(_head_and_shoulders(tail[-5:], levels))
        found.append(_flag(tail[-5:]))
    if len(tail) >= 4:
        found.append(_cup(tail[-4:], clock[-4:], levels))
    return [instance for instance in found if instance is not None]


def _follow_chart(instance, bar):
    time, _, _, close = bar
    kind, nodes = instance.kind, instance.nodes
    if kind in BOUNDARY_KINDS:
        a, b = instance.upper.at(time), instance.lower.at(time)
        return CONFIRMED if close > max(a, b) or close < min(a, b) else FORMING
    if kind == "one_two_three":
        second, third = nodes[1], nodes[2]
        if second.is_high:
            if close < third.price:
                return CONFIRMED
            return INVALIDATED if close > second.price else FORMING
        if close > third.price:
            return CONFIRMED
        return INVALIDATED if close < second.price else FORMING
    if kind in REVERSAL_KINDS:
        top = kind in TOP_KINDS
        level = (instance.lower if top else instance.upper).at(time)
        prices = [n.price for n in nodes]
        if top:
            if close < level:
                return CONFIRMED
            return INVALIDATED if close > max(prices) else FORMING
        if close > level:
            return CONFIRMED
        return INVALIDATED if close < min(prices) else FORMING
    if kind == "wolfe_wave":
        last = nodes[4]
        target = Line(nodes[0], nodes[3]).at(time)
        if last.is_high:
            reverting = close < last.price and close >= min(target, last.price)
            continuing = close > last.price
        else:
            reverting = close > last.price and close <= max(target, last.price)
            continuing = close < last.price
        if reverting:
            return CONFIRMED
        return INVALIDATED if continuing else FORMING
    if kind == "flag":
        upper, lower = instance.upper.at(time), instance.lower.at(time)
        if not nodes[0].is_high:
            beyond, against = close > upper, close < lower
        else:
            beyond, against = close < lower, close > upper
        if beyond:
            return CONFIRMED
        return INVALIDATED if against else FORMING
    sign = 1.0 if nodes[0].is_high else -1.0
    if sign * (close - nodes[2].price) > 0.0:
        return CONFIRMED
    return INVALIDATED if sign * (close - nodes[3].price) < 0.0 else FORMING


def _chart_event(instance, bar):
    time, _, _, close = bar
    kind, nodes = instance.kind, instance.nodes
    extras = {"pattern": CHART_CODES[kind]}
    if kind in BOUNDARY_KINDS:
        a, b = instance.upper.at(time), instance.lower.at(time)
        bullish = close > max(a, b)
        extras["level"] = max(a, b) if bullish else min(a, b)
    elif kind == "one_two_three":
        extras["level"] = nodes[2].price
        extras["invalidation"] = nodes[1].price
        bullish = not nodes[1].is_high
    elif kind == "wolfe_wave":
        extras["invalidation"] = nodes[4].price
        extras["target"] = Line(nodes[0], nodes[3]).at(time)
        bullish = not nodes[4].is_high
    elif kind == "flag":
        bullish = not nodes[0].is_high
        broken, other = (instance.upper, instance.lower) if bullish \
            else (instance.lower, instance.upper)
        extras["level"] = broken.at(time)
        extras["invalidation"] = other.at(time)
    elif kind in ("cup_and_handle", "inverse_cup_and_handle"):
        extras["level"] = nodes[2].price
        extras["invalidation"] = nodes[3].price
        bullish = kind == "cup_and_handle"
    else:
        top = kind in TOP_KINDS
        extras["level"] = (instance.lower if top else instance.upper).at(time)
        prices = [n.price for n in nodes]
        extras["invalidation"] = max(prices) if top else min(prices)
        bullish = not top
    extras["fit"] = instance.fit
    return _alert(bullish, kind), 1.0 if bullish else -1.0, extras


CHART_EXTRA = {"tolerance_pct": (2.0, 0.001, 50.0), "flat_pct": (1.5, 0.0, 100.0)}


def chart_patterns(params, bars, times=None):
    """One row per bar for `chart_patterns`: the direction of the latest confirmed instance
    (1, -1, 0 before any), `active` and `confirmed`, and on a bar with events the extras of the
    first (`pattern`, `level`, `invalidation`, `target`, `fit`); alerts `bull_<kind>` and
    `bear_<kind>`, one per instance."""
    resolved = _resolve(params, CHART_EXTRA)
    levels = _Levels(resolved["tolerance_pct"])
    return _run(bars, times, resolved,
                lambda tail, clock: _scan_chart(tail, clock, levels, resolved["flat_pct"]),
                _follow_chart, _chart_event)


# --- harmonic_patterns -----------------------------------------------------------------------

class HarmonicInstance:
    def __init__(self, row, bullish, d, low, high):
        self.row = row
        self.bullish = bullish
        self.d = d
        self.low = low
        self.high = high


def _inside(interval, value, tolerance):
    return interval[0] - tolerance <= value <= interval[1] + tolerance


def _zone(row, x, a, b, c, tolerance, share):
    """The potential reversal zone of a pattern X, A, B, C: the span of the projections of the
    row's D interval (widened by the tolerance; from A and XA, for the shark from C and XC), of
    the BC extension at the centre of the row's CD interval and of the AB = CD leg, at least
    `share` of XA wide, centred where the span is."""
    _, _, _, d_interval, basis, cd_interval = row
    if basis == "XA":
        anchor, leg = a, a - x
    else:
        anchor, leg = c, c - x
    centre = (cd_interval[0] + cd_interval[1]) / 2.0
    points = [anchor - (d_interval[0] - tolerance) * leg,
              anchor - (d_interval[1] + tolerance) * leg,
              c + centre * -(c - b),
              c + (b - a)]
    low, high = min(points), max(points)
    floor = share * abs(a - x)
    if high - low < floor:
        middle = (high + low) / 2.0
        low, high = middle - floor / 2.0, middle + floor / 2.0
    return low, high


def _scan_harmonic(tail, tolerance, share):
    if len(tail) < WINDOW or not _alternate(tail[-WINDOW:]):
        return []
    window = tail[-WINDOW:]
    x, a, b, c, d = [n.price for n in window]
    xa, ab, bc = abs(a - x), abs(b - a), abs(c - b)
    if xa <= EPSILON or ab <= EPSILON or bc <= EPSILON:
        return []
    ratio_b, ratio_c, ratio_cd = ab / xa, bc / ab, abs(d - c) / bc
    found = []
    for number, row in enumerate(HARMONIC_TABLE, start=1):
        _, b_interval, c_interval, d_interval, basis, cd_interval = row
        if basis == "XA":
            ratio_d = abs(a - d) / xa
        else:
            xc = abs(c - x)
            if xc <= EPSILON:
                continue
            ratio_d = abs(c - d) / xc
        if _inside(b_interval, ratio_b, tolerance) and _inside(c_interval, ratio_c, tolerance) \
                and _inside(d_interval, ratio_d, tolerance) \
                and _inside(cd_interval, ratio_cd, tolerance):
            low, high = _zone(row, x, a, b, c, tolerance, share)
            found.append(HarmonicInstance(number, not window[0].is_high, d, low, high))
    return found


def _follow_harmonic(instance, bar):
    close = bar[3]
    if instance.bullish:
        left, resolved = close < instance.low, close > instance.high
    else:
        left, resolved = close > instance.high, close < instance.low
    if left:
        return INVALIDATED
    return CONFIRMED if resolved else FORMING


def _harmonic_event(instance, bar):
    name = HARMONIC_TABLE[instance.row - 1][0]
    extras = {"pattern": float(instance.row), "prz_low": instance.low,
              "prz_high": instance.high, "d": instance.d,
              "invalidation": instance.low if instance.bullish else instance.high}
    return _alert(instance.bullish, name), 1.0 if instance.bullish else -1.0, extras


HARMONIC_EXTRA = {"ratio_tolerance": (0.03, 0.0, 0.5), "min_prz_share": (0.005, 0.0, 1.0)}


def harmonic_patterns(params, bars, times=None):
    """One row per bar for `harmonic_patterns`: the direction of the latest confirmed structure,
    `active`, `confirmed`, and on a bar with events the extras of the first (`pattern` as the
    1-based table row, `prz_low`, `prz_high`, `d`, `invalidation`); alerts `bull_<name>` and
    `bear_<name>`."""
    resolved = _resolve(params, HARMONIC_EXTRA)
    return _run(bars, times, resolved,
                lambda tail, clock: _scan_harmonic(tail, resolved["ratio_tolerance"],
                                                   resolved["min_prz_share"]),
                _follow_harmonic, _harmonic_event)
