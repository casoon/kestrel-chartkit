"""golden_point_and_figure: point-and-figure columns and the horizontal count on hand-built price
sequences, from `kestrel_reference/point_and_figure.py` (exact rational arithmetic)."""

from fractions import Fraction

from ..fixture import provenance
from ..point_and_figure import chart, horizontal_count, interval_chart

NAME = "golden_point_and_figure"

# Turning points of a decline, a trading range and a breakout: the worked example of a horizontal
# count. With a box of 1 and a reversal of 3 every swing is a column.
RANGE = ["120", "100", "111", "104", "108", "101", "109", "105", "110", "103", "108", "104",
         "107", "102", "110", "106", "111", "104", "109", "105", "108", "113", "117", "120"]

# The twenty columns of RANGE with box 1 and reversal 3, written out by hand: direction, lowest
# and highest box.
RANGE_COLUMNS = [
    ("O", 100, 120), ("X", 101, 111), ("O", 104, 110), ("X", 105, 108), ("O", 101, 107),
    ("X", 102, 109), ("O", 105, 108), ("X", 106, 110), ("O", 103, 109), ("X", 104, 108),
    ("O", 104, 107), ("X", 105, 107), ("O", 102, 106), ("X", 103, 110), ("O", 106, 109),
    ("X", 107, 111), ("O", 104, 110), ("X", 105, 109), ("O", 105, 108), ("X", 106, 120),
]

# The eight columns of RANGE with box 2 and reversal 3 (a turn of 6), walked through by hand:
# 100 opens the O column from 120; 111 has reached 110 >= 100 + 6; 104 has reached 104 <= 110 - 6;
# 101 has reached 102 only; 109 has reached 108 >= 102 + 6; 110 extends; 103 has reached 104;
# 102 extends to 102; 110 >= 102 + 6 turns; 104 <= 110 - 6 turns; 113 has reached 112 >= 104 + 6.
RANGE_COLUMNS_BOX2 = [
    ("O", 100, 120), ("X", 102, 110), ("O", 102, 108), ("X", 104, 110), ("O", 102, 108),
    ("X", 104, 110), ("O", 104, 108), ("X", 106, 120),
]

# (name, box, reversal, method, inputs, count) with method 0 = single prices, 1 = (high, low)
# pairs, and count = (first, last) column of a horizontal count or None.
CASES = [
    ("range", "1", 3, 0, RANGE, (0, 18)),
    ("range_one_box_reversal", "1", 1, 0, RANGE, (0, 18)),
    ("range_box_2", "2", 3, 0, RANGE, (0, 6)),
    ("range_box_2_reversal_2", "2", 2, 0, RANGE, (0, 12)),
    # RANGE reflected at 220: a rise, a range and a breakdown; the count runs over X columns.
    ("range_reflected", "1", 3, 0, [str(220 - int(p)) for p in RANGE], (0, 18)),
    # Prices between the grid lines: a rising price fills up to the line below it, a falling one
    # down to the line above it. 104.2 has reached 105, not 104; 108.6 reaches 108 = 105 + 3.
    ("between_the_lines", "1", 3, 0,
     ["120.5", "100.4", "111.7", "104.2", "107.9", "108.6", "105.1", "112.3", "109.2", "109.0",
      "106.5", "108.9", "109.0"], (0, 4)),
    # A decimal box size: 10.3 / 0.1 is not a whole number in binary floating point, the price is
    # on the line 10.3 all the same. 10.02 has not come back the three boxes to 10.0, 10.0 has.
    ("decimal_grid", "0.1", 3, 0,
     ["10.0", "10.3", "10.02", "10.0", "10.29", "10.3", "10.35", "10.4", "10.11", "10.1", "9.7",
      "9.99", "10.0"], None),
    # Before the first column: prices inside the anchor box, then the first move that fills a box.
    ("slow_start", "5", 2, 0,
     ["102", "104", "100.5", "104.9", "100", "95.1", "95", "104.9", "105", "112", "100"], None),
    # Highs and lows (positions from 0): input 1 leaves the anchor on both sides and only widens
    # it. Input 3 makes a new high and reaches three boxes down as well: the X column is
    # extended, not reversed. Input 6 does the same in the O column: a new low, and a high three
    # boxes up that does not count.
    ("high_low", "0.5", 3, 1,
     [("100.2", "99.8"), ("100.6", "99.4"), ("101.6", "99.9"), ("102.2", "100.4"),
      ("102.1", "100.6"), ("101.0", "99.7"), ("101.4", "99.2"), ("100.6", "99.3"),
      ("100.9", "99.6"), ("102.6", "100.7"), ("104.1", "101.0"), ("103.9", "102.5")], (0, 2)),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: point-and-figure columns and the horizontal count",
    "",
    *provenance("point_and_figure"),
    "",
    "Construction as documented in src/point_and_figure.rs: a box is named by its grid line (a",
    "whole multiple of the box size) and filled once the price has reached that line; an X column",
    "is extended by a price at or above the next line and reversed by a price at or below the",
    "line `reversal` boxes under its top (O columns mirrored); every reversal opens a new column.",
    "The reference works in exact rational arithmetic on the decimal text of the inputs and in",
    "price space (whole boxes travelled beyond the last filled line). The columns of `range` and",
    "of `range_box_2` are also written out by hand in the generator and compared.",
    "",
    "pnf<i>_box, _reversal, _method (0 single prices _p<j>, 1 pairs _h<j>/_l<j>), _n inputs;",
    "_columns, then per column _c<k>_dir (1 X, -1 O), _c<k>_top/_bottom (grid lines, price",
    "units), _c<k>_top_box/_bottom_box (line numbers), _c<k>_first/_last (0-based input",
    "positions). _count is 1 when a horizontal count is given: _count_first/_last (columns),",
    "_count_dir (1 projected upward, over O columns; -1 downward, over X columns), _count_line,",
    "_count_extreme, _count_columns, _count_on_line, _count_effect, _count_effect_on_line,",
    "_count_target_extreme, _count_target_line.",
    "",
    "Documented alternative, generated and not asserted against the crate: _alt_interval_* is the",
    "same count when a box is read as the price interval above its line and every price fills the",
    "box it lies in (floor both ways). The two readings agree while every price is on a grid",
    "line (box 1) and differ for box 2, where under the interval reading a column can turn on a",
    "move of 5 against a reversal of 6 (103 to 108): 10 columns, count 9, effect 54 there, against",
    "8 columns, count 7, effect 42 here.",
]


def _number(value):
    return float(value)


def _alternative(prices, box, reversal):
    """Columns, count, count on the line and effect under the interval reading, counted from the
    first column to the last column before the final one, the count line at its lowest box."""
    columns = interval_chart(prices, box, reversal)
    counted = columns[:-1]
    assert counted[0][0] == counted[-1][0] == "O"
    line = counted[-1][1]
    on_line = sum(1 for _, low, high in counted if low <= line <= high)
    effect = len(counted) * reversal * Fraction(box)
    return {"alt_interval_columns": float(len(columns)),
            "alt_interval_count": float(len(counted)),
            "alt_interval_on_line": float(on_line),
            "alt_interval_effect": float(effect)}


def build():
    blocks = [(None, "case count", {"meta_case_count": float(len(CASES))})]
    for i, (name, box, reversal, method, inputs, count) in enumerate(CASES):
        pairs = [(p, p) for p in inputs] if method == 0 else inputs
        columns = chart(pairs, box, reversal)
        as_boxes = [(c["direction"], int(c["bottom"] / Fraction(box)), int(c["top"] / Fraction(box)))
                    for c in columns]
        if name == "range":
            assert as_boxes == RANGE_COLUMNS
        if name == "range_box_2":
            assert as_boxes == [(d, low // 2, high // 2) for d, low, high in RANGE_COLUMNS_BOX2]
        if name == "range_reflected":
            assert as_boxes == [("X" if d == "O" else "O", 220 - high, 220 - low)
                                for d, low, high in RANGE_COLUMNS]

        case = {"box": _number(Fraction(box)), "reversal": float(reversal),
                "method": float(method), "n": float(len(inputs))}
        for j, (high, low) in enumerate(pairs):
            if method == 0:
                case[f"p{j}"] = _number(Fraction(high))
            else:
                case[f"h{j}"] = _number(Fraction(high))
                case[f"l{j}"] = _number(Fraction(low))
        case["columns"] = float(len(columns))
        for k, column in enumerate(columns):
            case[f"c{k}_dir"] = 1.0 if column["direction"] == "X" else -1.0
            case[f"c{k}_top"] = _number(column["top"])
            case[f"c{k}_bottom"] = _number(column["bottom"])
            case[f"c{k}_top_box"] = _number(column["top"] / Fraction(box))
            case[f"c{k}_bottom_box"] = _number(column["bottom"] / Fraction(box))
            case[f"c{k}_first"] = float(column["first"])
            case[f"c{k}_last"] = float(column["last"])
        case["count"] = 0.0 if count is None else 1.0
        if count is not None:
            result = horizontal_count(columns, count[0], count[1], box, reversal)
            assert result is not None, name
            case["count_first"], case["count_last"] = float(count[0]), float(count[1])
            case["count_dir"] = float(result["direction"])
            for key in ("line", "extreme", "columns", "on_line", "effect", "effect_on_line",
                        "target_extreme", "target_line"):
                case[f"count_{key}"] = _number(result[key])
        if name.startswith("range") and name != "range_reflected":
            case.update(_alternative(inputs, box, reversal))
        blocks.append((f"pnf{i}", name, case))

    # The worked example by hand: columns 1 to 19 between the first and the last O column, the
    # count line on the lowest box (105) of column 19, the low of the range at 100.
    by_name = {block[1]: block[2] for block in blocks[1:]}
    worked = by_name["range"]
    assert (worked["columns"], worked["count_columns"], worked["count_on_line"]) == (20, 19, 16)
    assert (worked["count_line"], worked["count_extreme"]) == (105, 100)
    assert (worked["count_effect"], worked["count_effect_on_line"]) == (57, 48)
    assert (worked["count_target_extreme"], worked["count_target_line"]) == (157, 162)
    one_box = by_name["range_one_box_reversal"]
    assert (one_box["columns"], one_box["count_effect"], one_box["count_effect_on_line"]) == \
        (20, 19, 16)
    box_2 = by_name["range_box_2"]
    assert (box_2["columns"], box_2["count_columns"], box_2["count_effect"]) == (8, 7, 42)
    assert (box_2["count_target_extreme"], box_2["count_target_line"]) == (142, 146)
    # The interval reading: the numbers of the hand calculation that uses it.
    alternative = [(c["alt_interval_columns"], c["alt_interval_count"], c["alt_interval_on_line"],
                    c["alt_interval_effect"]) for c in
                   (worked, one_box, box_2, by_name["range_box_2_reversal_2"])]
    assert alternative == [(20, 19, 16, 57), (20, 19, 16, 19), (10, 9, 9, 54), (18, 17, 12, 68)]

    blocks.append((None, None, {"point_and_figure_tolerance": 1e-9}))
    return HEADER, blocks
