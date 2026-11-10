"""Price gaps between consecutive bars, from the definition documented in `src/indicator/gap.rs`.

Written as a batch calculation over the whole series: every gap from the two bars that form it,
its closing bar by a search forward, its filled fraction from the extreme of the bars since. Only
then is the result read bar by bar, so nothing here follows gaps through a running state.

Bars are (open, high, low, close, ...) tuples, oldest first; indices are 0-based.
"""

from .indicators import true_ranges
from .smoothing import rma

# Smallest relative distance from the minimum size that a gap in a case may have, unless it sits
# exactly on it.
MARGIN = 1e-7


def find_gaps(bars, reference, atr_len, min_atr, max_bars):
    """The ATR per bar and every gap that counts, oldest first.

    A gap on bar i: the open above `upper` (bottom = upper, top = open) or below `lower`
    (bottom = open, top = lower), where upper/lower are the previous high/low (`reference`
    "range") or both the previous close ("close"). It counts when top - bottom >= min_atr times
    the ATR of bar i-1, and is closed on the first bar j in i..i+max_bars whose low is at or below
    the bottom (gap up) or whose high is at or above the top (gap down).
    """
    atr = rma(true_ranges([bar[:4] for bar in bars]), atr_len)
    gaps = []
    for i in range(1, len(bars)):
        atr_prev = atr[i - 1]
        if atr_prev is None:
            continue
        open_ = bars[i][0]
        _, prev_high, prev_low, prev_close = bars[i - 1][:4]
        upper, lower = (prev_high, prev_low) if reference == "range" else (prev_close, prev_close)
        if open_ > upper:
            direction, top, bottom = 1, open_, upper
        elif open_ < lower:
            direction, top, bottom = -1, lower, open_
        else:
            continue
        size, minimum = top - bottom, min_atr * atr_prev
        assert size == minimum or abs(size - minimum) > MARGIN * max(1.0, size, minimum), \
            f"bar {i + 1}: gap size {size} too close to the minimum {minimum}"
        if size < minimum:
            continue
        last = min(i + max_bars, len(bars) - 1)
        if direction == 1:
            closed_at = next((j for j in range(i, last + 1) if bars[j][2] <= bottom), None)
        else:
            closed_at = next((j for j in range(i, last + 1) if bars[j][1] >= top), None)
        gaps.append({"bar": i, "direction": direction, "top": top, "bottom": bottom,
                     "atr_prev": atr_prev, "closed_at": closed_at})
    return atr, gaps


def filled(gap, bars, t):
    """Filled fraction of `gap` after bar t, for a gap not yet closed: how far the lowest low
    (gap up) or the highest high (gap down) of the bars gap bar..t reaches into it."""
    since = bars[gap["bar"]:t + 1]
    if gap["direction"] == 1:
        depth = gap["top"] - min(bar[2] for bar in since)
    else:
        depth = max(bar[1] for bar in since) - gap["bottom"]
    return min(max(depth / (gap["top"] - gap["bottom"]), 0.0), 1.0)


def expected_outputs(bars, reference="range", atr_len=14, min_atr=0.0, max_bars=100):
    """Per bar `(output, alerts)` in the form of the contract cases: no output before bar
    atr_len + 1; then `value` 1/-1 for a gap formed on the bar (0 for none) and the extra fields
    and alerts documented in `src/indicator/gap.rs`. Alert strengths are 1."""
    atr, gaps = find_gaps(bars, reference, atr_len, min_atr, max_bars)
    expected = []
    for t in range(len(bars)):
        if t < atr_len:
            expected.append((None, {}))
            continue
        extra, alerts = {"atr_prev": atr[t - 1]}, {}
        value = 0.0
        for gap in gaps:
            if gap["bar"] != t:
                continue
            value = float(gap["direction"])
            size = gap["top"] - gap["bottom"]
            extra.update({"gap_top": gap["top"], "gap_bottom": gap["bottom"], "size": size})
            if gap["atr_prev"] > 0.0:
                extra["size_atr"] = size / gap["atr_prev"]
            alerts["gap_up" if gap["direction"] == 1 else "gap_down"] = 1.0
        # Followed on bar t: formed at or before t, not more than max_bars bars ago, not closed
        # before t. Youngest first.
        followed = [g for g in reversed(gaps)
                    if g["bar"] <= t <= g["bar"] + max_bars
                    and (g["closed_at"] is None or g["closed_at"] >= t)]
        still_open = [g for g in followed if g["closed_at"] != t]
        closed = [g for g in followed if g["closed_at"] == t]
        extra["open_count"] = float(len(still_open))
        for n, gap in enumerate(still_open, start=1):
            extra.update({f"open_{n}_direction": float(gap["direction"]),
                          f"open_{n}_top": gap["top"], f"open_{n}_bottom": gap["bottom"],
                          f"open_{n}_fill": filled(gap, bars, t),
                          f"open_{n}_bars": float(t - gap["bar"])})
        extra["closed_count"] = float(len(closed))
        for n, gap in enumerate(closed, start=1):
            extra[f"closed_{n}_bars"] = float(t - gap["bar"])
        if closed:
            alerts["gap_closed"] = 1.0
        expected.append(((value, extra), alerts))
    return expected
