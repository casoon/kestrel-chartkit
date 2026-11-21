"""golden_intrabar_cvd: the intrabar cumulative volume delta, from the rule documented on
`IntrabarCvd`: every parameter option (both anchors with and without an offset, both policies for
an unchanged close), steps with and without children, day changes and unchanged closes."""

from ..fixture import provenance
from ..intrabar_cvd import compute

NAME = "golden_intrabar_cvd"

NAN = float("nan")
HOUR = 3600
DAY = 86_400


def group(stamp, *children):
    return ("group", stamp, list(children))


def missing(stamp, volume):
    return ("missing", stamp, volume)


# (open, close, volume) per child. The first child of a series is booked against its own open.
BASIC = [
    # up (own open), down, up, unchanged (equal to the child before), unchanged again
    group(0, (100.0, 101.0, 100.0), (101.0, 100.5, 200.0), (100.5, 102.0, 300.0),
          (102.0, 102.0, 40.0), (102.0, 102.0, 60.0)),
    # a single child that does not move from the close before: unattributed under the default
    group(900, (102.0, 102.0, 500.0)),
    # a child that closes above and one below the close before, then a doji against its own open
    group(1800, (102.0, 103.0, 70.0), (103.0, 101.5, 90.0), (101.5, 101.5, 25.0)),
    # no volume at all, and a child that is up with zero volume
    group(2700, (101.5, 102.5, 0.0), (102.5, 102.5, 0.0)),
    # the last child of the series is down
    group(3600, (102.5, 100.0, 120.0)),
]

# A first child that does not move from its own open: no direction to carry yet.
FIRST_UNCHANGED = [
    group(0, (50.0, 50.0, 10.0), (50.0, 50.0, 20.0), (50.0, 51.0, 30.0), (51.0, 51.0, 40.0)),
    group(900, (51.0, 50.0, 5.0), (50.0, 50.0, 15.0), (50.0, 50.0, 25.0)),
]

# Three UTC days; the steps at 86399 and 86400 lie on either side of the midnight, and the first
# child of every new day is booked against its own open (the close before is forgotten).
DAYS = [
    group(0, (10.0, 11.0, 100.0), (11.0, 12.0, 100.0)),
    group(43200, (12.0, 11.0, 50.0), (11.0, 11.0, 70.0)),
    group(DAY - 1, (11.0, 12.5, 30.0), (12.5, 12.5, 30.0)),
    group(DAY, (12.5, 12.5, 40.0), (12.5, 13.0, 40.0)),
    group(DAY + 43200, (13.0, 12.0, 60.0)),
    missing(2 * DAY - 1, 999.0),
    group(2 * DAY, (12.0, 11.0, 80.0), (11.0, 11.0, 20.0)),
    group(2 * DAY + 3 * HOUR, (11.0, 10.0, 10.0), (10.0, 10.0, 90.0)),
]

# The same steps with a daily anchor shifted by two hours: the day starts at 02:00 UTC, so 86399
# is in the day that began at 02:00 and 7199 before it is in the day before.
SHIFTED = [
    group(0, (10.0, 11.0, 100.0), (11.0, 11.0, 100.0)),
    group(2 * HOUR - 1, (11.0, 12.0, 10.0), (12.0, 12.0, 20.0)),
    group(2 * HOUR, (12.0, 12.0, 30.0), (12.0, 11.0, 40.0)),
    group(DAY + 2 * HOUR - 1, (11.0, 10.0, 50.0), (10.0, 10.0, 60.0)),
    group(DAY + 2 * HOUR, (10.0, 10.0, 70.0), (10.0, 11.0, 80.0)),
]

# Rounding of the offset: -3600.5 rounds to -3601 (half away from zero), so the day starts at
# 82799 UTC; 3600.5 rounds to 3601, so the day starts at 3601 UTC.
NEGATIVE_HALF = [
    group(0, (20.0, 21.0, 10.0)),
    group(DAY - 3602, (21.0, 22.0, 20.0)),
    group(DAY - 3601, (22.0, 23.0, 30.0)),
    group(DAY - 3600, (23.0, 22.0, 40.0)),
    group(2 * DAY - 3602, (22.0, 24.0, 50.0)),
    group(2 * DAY - 3601, (24.0, 25.0, 60.0)),
]
POSITIVE_HALF = [
    group(0, (20.0, 21.0, 10.0)),
    group(3600, (21.0, 22.0, 20.0)),
    group(3601, (22.0, 23.0, 30.0)),
    group(DAY + 3600, (23.0, 22.0, 40.0)),
    group(DAY + 3601, (22.0, 24.0, 50.0)),
]

# Parents without children are booked as unattributed volume; the total stands still, and a
# missing parent does not touch the close or the direction the next parent compares against.
GAPS = [
    missing(0, 750.0),
    group(900, (100.0, 101.0, 100.0), (101.0, 100.0, 40.0)),
    missing(1800, 5000.0),
    group(2700, (100.0, 100.0, 60.0), (100.0, 99.0, 80.0)),
    missing(3600, 0.0),
    missing(4500, -300.0),
    missing(5400, NAN),
    group(6300, (99.0, 99.0, 25.0)),
]

# Volumes that are negative or not a number count as 0, but the child still sets the close and the
# direction the next one is booked against.
ODD_VOLUMES = [
    group(0, (10.0, 11.0, -50.0), (11.0, 11.0, 20.0), (11.0, 10.0, NAN), (10.0, 10.0, 30.0),
          (10.0, 12.0, 40.0)),
    group(900, (12.0, 12.0, -5.0), (12.0, 13.0, 10.0)),
]


def lattice(n, seed, days=False):
    """`n` parents of one to six children on an integer price lattice, so closes often equal the
    close before; with `days` the parents lie several hours apart over a few days."""
    state = seed

    def draw(count):
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        return (state >> 33) % count

    steps, price, stamp = [], 100.0, 0
    for _ in range(n):
        if days:
            stamp += (1 + draw(9)) * HOUR
        else:
            stamp += 900
        if draw(12) == 0:
            steps.append(missing(stamp, float(draw(2000))))
            continue
        children = []
        for _ in range(1 + draw(6)):
            close = price + draw(3) - 1.0
            children.append((price, close, float(draw(500))))
            price = close
        steps.append(("group", stamp, children))
    return steps


# (comment, parameters, steps); parameters are the f64 values the registry reads, absent where
# the default is used.
CASES = [
    ("defaults: continuous, unchanged volume unattributed", {}, BASIC),
    ("continuous, carry_previous_direction over the same steps",
     {"unchanged_policy": 1.0}, BASIC),
    ("carry_previous_direction with no direction to carry at first, and after a first child that "
     "does not move", {"unchanged_policy": 1.0}, FIRST_UNCHANGED),
    ("unattributed over the same steps (two parents of unchanged children)", {}, FIRST_UNCHANGED),
    ("daily anchor, offset 0: the total, the close and the direction restart at every UTC day",
     {"anchor": 1.0}, DAYS),
    ("daily anchor with carry_previous_direction: the direction is forgotten at the day change",
     {"anchor": 1.0, "unchanged_policy": 1.0}, DAYS),
    ("continuous over the day steps: nothing resets", {}, DAYS),
    ("daily anchor, offset 7200 seconds", {"anchor": 1.0, "anchor_offset_seconds": 7200.0},
     SHIFTED),
    ("daily anchor, offset 7200 seconds, carry_previous_direction",
     {"anchor": 1.0, "anchor_offset_seconds": 7200.0, "unchanged_policy": 1.0}, SHIFTED),
    ("daily anchor, offset -3600.5 rounds to -3601", {"anchor": 1.0,
                                                      "anchor_offset_seconds": -3600.5},
     NEGATIVE_HALF),
    ("daily anchor, offset 3600.5 rounds to 3601", {"anchor": 1.0, "anchor_offset_seconds": 3600.5},
     POSITIVE_HALF),
    ("an offset under the continuous anchor does not matter",
     {"anchor_offset_seconds": 7200.0}, SHIFTED),
    ("daily anchor with the offset 86400, the largest the parameter takes",
     {"anchor": 1.0, "anchor_offset_seconds": 86400.0}, SHIFTED),
    ("daily anchor with the offset -86400, the smallest", {"anchor": 1.0, "anchor_offset_seconds": -86400.0},
     SHIFTED),
    ("parents without children", {}, GAPS),
    ("parents without children at the day change of a daily anchor",
     {"anchor": 1.0, "anchor_offset_seconds": 1800.0}, GAPS),
    ("negative and not-a-number volumes count as 0", {}, ODD_VOLUMES),
    ("negative and not-a-number volumes with carry_previous_direction",
     {"unchanged_policy": 1.0}, ODD_VOLUMES),
    ("a lattice of prices (many unchanged closes), defaults", {}, lattice(80, 20261121)),
    ("the same, carry_previous_direction", {"unchanged_policy": 1.0}, lattice(80, 20261121)),
    ("the same over several days with a daily anchor shifted by -5400 seconds",
     {"anchor": 1.0, "anchor_offset_seconds": -5400.0}, lattice(80, 20261122, days=True)),
    ("the same, carry_previous_direction, anchor offset 30000",
     {"anchor": 1.0, "anchor_offset_seconds": 30000.0, "unchanged_policy": 1.0},
     lattice(80, 20261122, days=True)),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: intrabar cumulative volume delta",
    "",
    *provenance("intrabar_cvd"),
    "",
    "Each child bar is booked against the close of the child before it (the first child of a series",
    "or of an anchor period against its own open): higher is buying, lower selling, equal is",
    "unchanged. Unchanged volume is unattributed, or with unchanged_policy 1 (carry previous",
    "direction) takes the direction of the last child that had one. Negative and NaN volumes count",
    "as 0. A parent without children is booked as unattributed volume with no movement. With",
    "anchor 1 (daily) every period of 86 400 seconds that starts anchor_offset_seconds after",
    "midnight UTC (rounded half away from zero) starts from 0 with no previous close or direction.",
    "",
    "ib<i>_n is the number of steps; the parameters given are ib<i>_anchor, ib<i>_offset and",
    "ib<i>_policy (absent: the default). Step j: s<j>_kind is 0 for a parent with children and 1",
    "for one without, s<j>_ts its timestamp; children as s<j>_m (their number) and s<j>_c<k>_open,",
    "_close and _volume, or s<j>_pv (the parent's volume). Expected per step: s<j>_delta, _buy,",
    "_sell, _un (unattributed), _cum_open, _cum_high, _cum_low, _cum_close.",
    "",
    "intrabar_cvd_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def build():
    blocks = [(None, "case count", {"meta_cases": float(len(CASES))})]
    for i, (comment, params, steps) in enumerate(CASES):
        out = compute(params, steps)
        case = {"n": float(len(steps))}
        for key, name in (("anchor", "anchor"), ("anchor_offset_seconds", "offset"),
                          ("unchanged_policy", "policy")):
            if key in params:
                case[name] = params[key]
        for j, ((kind, stamp, content), result) in enumerate(zip(steps, out)):
            case[f"s{j}_kind"] = 0.0 if kind == "group" else 1.0
            case[f"s{j}_ts"] = float(stamp)
            if kind == "group":
                case[f"s{j}_m"] = float(len(content))
                for k, (open_, close, volume) in enumerate(content):
                    case[f"s{j}_c{k}_open"] = open_
                    case[f"s{j}_c{k}_close"] = close
                    case[f"s{j}_c{k}_volume"] = volume
            else:
                case[f"s{j}_pv"] = content
            for key in ("delta", "buy", "sell", "un", "cum_open", "cum_high", "cum_low",
                        "cum_close"):
                source = {"un": "unattributed"}.get(key, key)
                case[f"s{j}_{key}"] = result[source]
        blocks.append((f"ib{i}", comment, case))
    blocks.append((None, None, {"intrabar_cvd_tolerance": 1e-9}))
    return HEADER, blocks
