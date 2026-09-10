"""golden_transforms: Heikin-Ashi candles, in exact rational arithmetic."""

from fractions import Fraction

from ..fixture import provenance

NAME = "golden_transforms"

BARS = [(100, 103, 99, 102), (102, 105, 101, 104), (110, 112, 108, 109), (109, 110, 104, 105),
        (105, 106, 103, Fraction(207, 2))]

HEADER = [
    "kestrel-chartkit golden reference fixture: bar-series transformations",
    "",
    *provenance("transforms"),
    "",
    "Heikin-Ashi (package 24) over five bars with an up gap on bar 3:",
    "  (100, 103,  99, 102)",
    "  (102, 105, 101, 104)",
    "  (110, 112, 108, 109)",
    "  (109, 110, 104, 105)",
    "  (105, 106, 103, 103.5)",
    "ha_close = (O+H+L+C)/4; ha_open = (previous ha_open + previous ha_close)/2, seeded on the first",
    "bar with (O+C)/2; ha_high = max(H, ha_open, ha_close); ha_low = min(L, ha_open, ha_close).",
    "Computed in exact rational arithmetic. ha_tolerance is the comparison tolerance the test",
    "applies, stated rather than derived.",
]


def build():
    case = {}
    previous = None
    for number, (o, h, l, c) in enumerate(BARS, start=1):
        o, h, l, c = (Fraction(x) for x in (o, h, l, c))
        close = (o + h + l + c) / 4
        open_ = (o + c) / 2 if previous is None else sum(previous) / 2
        case[f"bar{number}_open"] = float(open_)
        case[f"bar{number}_high"] = float(max(h, open_, close))
        case[f"bar{number}_low"] = float(min(l, open_, close))
        case[f"bar{number}_close"] = float(close)
        previous = (open_, close)
    case["tolerance"] = 1e-12
    return HEADER, [("ha", None, case)]
