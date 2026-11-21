"""Time-at-price profile without volume: tpo_profile.

Written from the rule (Steidlmayer and Koy 1986), independently of the Rust code. Python standard
library only.

`compute(params, bars)` runs the profile over `bars` (tuples `(open, high, low, close, volume)`)
and returns one row per bar: `(None, {})` before the window is full, otherwise
`((poc, {"vah": ..., "val": ...}), {})`. The window's range is split into equal bins; a bin counts
a bar when the bar's range reaches into it. The point of control is the bin with the most counts;
ties go to the bin closest to the centre of the range, then to the one closest to the close, then
to the lower one. The value area grows from it to 70 percent of all counts, taking the larger
neighbour (both on a tie). A window without range publishes the close for all three. Invalid
parameters raise `ValueError`.
"""

import math


def _get_int(params, key, default, lo):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key} must be an integer")
    if isinstance(v, float):
        if v != v or v != int(v):
            raise ValueError(f"{key} must be an integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key} must be an integer")
    if v < lo:
        raise ValueError(f"{key} must be at least {lo}")
    return v


def compute(params, bars):
    params = dict(params or {})
    for key in params:
        if key not in ("lookback", "num_bins"):
            raise ValueError(f"unknown parameter {key!r} for tpo_profile")
    length = _get_int(params, "lookback", 48, 1)
    bins = _get_int(params, "num_bins", 30, 1)
    out = []
    for i in range(len(bars)):
        if i < length - 1:
            out.append((None, {}))
            continue
        window = bars[i - length + 1:i + 1]
        lo = min(b[2] for b in window)
        hi = max(b[1] for b in window)
        close = bars[i][3]
        if hi == lo:
            out.append(((close, {"vah": close, "val": close}), {}))
            continue
        step = (hi - lo) / bins
        count = [0] * bins
        for b in window:
            first = max(0, math.ceil((b[2] - lo) / step) - 1)
            last = min(bins - 1, math.floor((b[1] - lo) / step))
            for k in range(first, last + 1):
                count[k] += 1
        most = max(count)
        poc = min((k for k in range(bins) if count[k] == most),
                  key=lambda k: (abs(2 * k - (bins - 1)), abs(lo + (k + 0.5) * step - close), k))
        total = sum(count)
        top = bottom = poc
        s = count[poc]
        while s < 0.7 * total:
            above = count[top + 1] if top + 1 < bins else None
            below = count[bottom - 1] if bottom - 1 >= 0 else None
            if above is None and below is None:
                break
            if below is None:
                top += 1
                s += above
            elif above is None:
                bottom -= 1
                s += below
            elif above > below:
                top += 1
                s += above
            elif below > above:
                bottom -= 1
                s += below
            else:
                top += 1
                bottom -= 1
                s += above + below
        out.append(((lo + (poc + 0.5) * step,
                     {"vah": lo + (top + 1) * step, "val": lo + bottom * step}), {}))
    return out
