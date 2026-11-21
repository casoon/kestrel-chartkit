"""Cumulative volume delta booked from lower-timeframe bars: intrabar_cvd.

Written from the rule documented in `src/indicator/cvd_intrabar.rs`, independently of the Rust
code. Python standard library only.

A step is `("group", parent_timestamp, children)` with `children` a list of `(open, close,
volume)` in arrival order, or `("missing", parent_timestamp, parent_volume)` for a parent bar
without children. `compute(params, steps)` returns one dict per step.

Each child is booked against the close of the child before it, the first child of a series (or of
an anchor period) against its own open: a higher close is buying, a lower one selling, an equal
one is unchanged. An unchanged child is unattributed, or with `unchanged_policy`
`carry_previous_direction` takes the direction of the last child that had one (unattributed when
there is none yet). A volume that is negative or not a number counts as 0. The running total
moves by direction times volume child by child; its open, high, low and close within the parent
are the four corners of that path. With the `daily` anchor, the first step of every period of 86
400 seconds that starts `anchor_offset_seconds` after midnight UTC (rounded half away from zero)
starts from a total of 0 with no previous close and no previous direction. A missing parent is
booked as no movement and its volume as unattributed. Invalid parameters raise `ValueError`.
"""

import math

ANCHORS = ("continuous", "daily")
POLICIES = ("unattributed", "carry_previous_direction")
DAY = 86_400


def _choice(params, key, options):
    v = params.get(key, 0.0)
    if not math.isfinite(v) or v != math.floor(v) or not 0 <= v < len(options):
        raise ValueError(f"{key} must be the position of one of {options}")
    return options[int(v)]


def _volume(v):
    return v if v > 0.0 else 0.0


def _round_half_away(x):
    r = math.floor(abs(x) + 0.5)
    return int(-r if x < 0 else r)


def compute(params, steps):
    params = dict(params or {})
    for key in params:
        if key not in ("anchor", "anchor_offset_seconds", "unchanged_policy"):
            raise ValueError(f"unknown parameter {key!r} for intrabar_cvd")
    anchor = _choice(params, "anchor", ANCHORS)
    policy = _choice(params, "unchanged_policy", POLICIES)
    offset = params.get("anchor_offset_seconds", 0.0)
    if not math.isfinite(offset) or not -DAY <= offset <= DAY:
        raise ValueError("anchor_offset_seconds must lie within one day")
    shift = _round_half_away(offset)

    total, previous_close, previous_direction, period = 0.0, None, 0.0, None
    out = []
    for kind, stamp, content in steps:
        if anchor == "daily":
            now = (stamp - shift) // DAY
            if now != period:
                period, total, previous_close, previous_direction = now, 0.0, None, 0.0
        opened = total
        high = low = total
        buy = sell = unattributed = 0.0
        if kind == "missing":
            unattributed = _volume(content)
            children = 0
        else:
            children = len(content)
            for open_, close, volume in content:
                base = open_ if previous_close is None else previous_close
                if close > base:
                    direction = 1.0
                elif close < base:
                    direction = -1.0
                else:
                    direction = previous_direction if policy == POLICIES[1] else 0.0
                previous_close = close
                if direction != 0.0:
                    previous_direction = direction
                volume = _volume(volume)
                if direction > 0:
                    buy += volume
                elif direction < 0:
                    sell += volume
                else:
                    unattributed += volume
                total += direction * volume
                high = max(high, total)
                low = min(low, total)
        out.append({"timestamp": stamp, "delta": buy - sell, "buy": buy, "sell": sell,
                    "unattributed": unattributed, "cum_open": opened, "cum_high": high,
                    "cum_low": low, "cum_close": total, "children": children})
    return out
