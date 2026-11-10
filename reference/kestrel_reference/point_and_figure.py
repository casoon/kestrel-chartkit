"""Point-and-figure columns and the horizontal count, from the construction documented in
`src/point_and_figure.rs`.

Everything is exact rational arithmetic (`fractions.Fraction`), and the rule is written in price
space rather than in box numbers: a column is a pair of grid lines, and a price extends or
reverses it by the whole boxes it has travelled beyond the last filled line. A price that lies on
a grid line in decimal notation is therefore on it exactly.

Inputs are (high, low) pairs; a single price is the pair (p, p). `None` stands for a value that
is not finite and changes nothing.
"""

from fractions import Fraction
from math import ceil, floor


def chart(inputs, box, reversal):
    """The columns for `inputs`, oldest first: dicts with `direction` ("X" or "O"), `top` and
    `bottom` (grid lines, Fractions), `first` and `last` (0-based input positions).

    A box is filled once the price has reached its grid line. In an X column with top line T a
    high of at least T + box extends the column to the highest line at or below the high;
    otherwise a low of at most T - reversal * box opens an O column from T - box down to the
    lowest line at or above the low. O columns mirrored, reading the low first. Before the first
    column the anchor is the span of lines reached so far (the highest at or below a high, the
    lowest at or above a low); an input beyond it on exactly one side opens the first column,
    which starts at the anchor line of that side.
    """
    box = Fraction(box)
    turn = reversal * box
    columns, anchor = [], None
    for position, pair in enumerate(inputs):
        if pair is None:
            continue
        high, low = Fraction(pair[0]), Fraction(pair[1])
        if not columns:
            line_up = box * floor(high / box)
            line_down = box * ceil(low / box)
            if anchor is None:
                anchor = (line_up, line_down)
                continue
            rises, falls = line_up > anchor[0], line_down < anchor[1]
            if rises and not falls:
                columns.append({"direction": "X", "top": line_up, "bottom": anchor[0],
                                "first": position, "last": position})
            elif falls and not rises:
                columns.append({"direction": "O", "top": anchor[1], "bottom": line_down,
                                "first": position, "last": position})
            else:
                anchor = (max(anchor[0], line_up), min(anchor[1], line_down))
            continue
        column = columns[-1]
        if column["direction"] == "X":
            top = column["top"]
            if high >= top + box:
                column["top"] = top + box * floor((high - top) / box)
                column["last"] = position
            elif low <= top - turn:
                columns.append({"direction": "O", "top": top - box,
                                "bottom": top - box * floor((top - low) / box),
                                "first": position, "last": position})
        else:
            bottom = column["bottom"]
            if low <= bottom - box:
                column["bottom"] = bottom - box * floor((bottom - low) / box)
                column["last"] = position
            elif high >= bottom + turn:
                columns.append({"direction": "X", "bottom": bottom + box,
                                "top": bottom + box * floor((high - bottom) / box),
                                "first": position, "last": position})
    return columns


def horizontal_count(columns, first, last, box, reversal):
    """The count from column `last` back to column `first` (0-based, both included), or `None`
    unless first <= last, both exist and are of the same kind.

    Over O columns: the line is the bottom of column `last`, the extreme the lowest bottom of the
    counted columns, and the effect (columns * reversal * box) is added to both. Over X columns:
    tops instead of bottoms, and the effect is subtracted. `on_line` counts the columns that have
    a filled box on the line."""
    if first > last or last >= len(columns):
        return None
    counted = columns[first:last + 1]
    if counted[0]["direction"] != counted[-1]["direction"]:
        return None
    base = counted[-1]["direction"] == "O"
    line = counted[-1]["bottom"] if base else counted[-1]["top"]
    extreme = min(c["bottom"] for c in counted) if base else max(c["top"] for c in counted)
    on_line = sum(1 for c in counted if c["bottom"] <= line <= c["top"])
    unit = reversal * Fraction(box)
    effect, effect_on_line = len(counted) * unit, on_line * unit
    sign = 1 if base else -1
    return {"direction": 1 if base else -1, "line": line, "extreme": extreme,
            "columns": len(counted), "on_line": on_line, "effect": effect,
            "effect_on_line": effect_on_line, "target_extreme": extreme + sign * effect,
            "target_line": line + sign * effect}


def interval_chart(prices, box, reversal):
    """The other reading of a box, for comparison only: a box is the half-open price interval
    from its grid line up to the next one, and every price fills the box it lies in, rising or
    falling (box number floor(p / box) both ways). Returns (direction, bottom, top) per column,
    boxes named by their lower line. Under this reading a falling price fills a box whose line it
    has not reached, and a reversal can follow a move of less than `reversal` boxes."""
    box = Fraction(box)
    number = [floor(Fraction(p) / box) for p in prices]
    columns, anchor = [], number[0]
    for k in number[1:]:
        if not columns:
            if k > anchor:
                columns.append(["X", anchor, k])
            elif k < anchor:
                columns.append(["O", k, anchor])
            continue
        direction, low, high = columns[-1]
        if direction == "X":
            if k > high:
                columns[-1][2] = k
            elif k <= high - reversal:
                columns.append(["O", k, high - 1])
        elif k < low:
            columns[-1][1] = k
        elif k >= low + reversal:
            columns.append(["X", low + 1, k])
    return [(direction, low * box, high * box) for direction, low, high in columns]
