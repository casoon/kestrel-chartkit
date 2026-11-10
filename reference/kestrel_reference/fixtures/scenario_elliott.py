"""scenario_elliott: the Elliott structure checks on hand-built pivot sequences.

Every sequence is written once as a rising one and generated a second time mirrored (highs for
lows, each price reflected), so both directions are covered by construction. Per structure there
is a valid sequence, one sequence per rule that breaks only that rule, the three sequence errors,
and at every threshold the value just below it, on it and just above it. A grid over the
retracement of wave B and the end of wave C covers the correction forms.

The expectations come from `kestrel_reference/elliott.py`, a second implementation of the rules as
documented in `src/indicator/elliott.rs`.
"""

from .. import elliott
from ..fixture import provenance

NAME = "scenario_elliott"


def rising(prices, times=None, kinds=None):
    """Nodes of a sequence that starts at a low, kinds alternating unless given."""
    kinds = kinds or ["L" if i % 2 == 0 else "H" for i in range(len(prices))]
    times = times or [60 * i for i in range(len(prices))]
    return [(kind, time, float(price)) for kind, time, price in zip(kinds, times, prices)]


def mirrored(nodes):
    """The same sequence upside down: highs for lows, prices reflected about their mid-range.
    All prices are whole numbers, so the reflection is exact."""
    prices = [node[2] for node in nodes]
    axis = min(prices) + max(prices)
    return [("L" if kind == "H" else "H", time, axis - price) for kind, time, price in nodes]


# --- cases: (structure, name, options, rising prices or nodes) -----------------------------------

def _impulse_cases():
    yield "valid", [100, 120, 110, 140, 130, 150]
    yield "equal_timestamps_accepted", rising([100, 120, 110, 140, 130, 150],
                                              times=[0, 60, 60, 120, 180, 180])
    # wave2_no_full_retrace: P2 against P0 = 100.
    yield "wave2_inside_by_one", [100, 120, 101, 140, 130, 150]
    yield "wave2_on_start_of_wave1", [100, 120, 100, 140, 130, 150]
    yield "wave2_beyond_start_of_wave1", [100, 120, 95, 140, 130, 150]
    # wave3_not_shortest: wave 1 is 30 long, wave 5 at least as long.
    yield "wave3_shorter_than_both", [100, 130, 120, 149, 140, 175]
    yield "wave3_equal_to_wave1", [100, 130, 120, 150, 140, 175]
    yield "wave3_longer_than_wave1", [100, 130, 120, 151, 140, 175]
    yield "wave3_shorter_than_wave1_only", [100, 130, 120, 145, 135, 150]
    yield "wave3_shortest_only_rule", [100, 130, 120, 135, 131, 160]
    # wave4_no_overlap: P4 against P1 = 120.
    yield "wave4_clear_by_one", [100, 120, 110, 140, 121, 150]
    yield "wave4_on_end_of_wave1", [100, 120, 110, 140, 120, 150]
    yield "wave4_inside_wave1", [100, 120, 110, 140, 115, 150]
    # No pivot sequence.
    yield "five_nodes", [100, 120, 110, 140, 130]
    yield "two_lows_in_a_row", rising([100, 120, 110, 105, 130, 150],
                                      kinds=["L", "H", "L", "L", "H", "L"])
    yield "node_3_earlier_than_node_2", rising([100, 120, 110, 140, 130, 150],
                                               times=[0, 60, 120, 90, 180, 240])
    yield "low_above_the_high_before_it", [100, 120, 125, 140, 130, 150]
    yield "low_equal_to_the_high_before_it", [100, 120, 110, 140, 140, 150]
    yield "alternation_reported_before_time", rising([100, 120, 110, 140, 130, 150],
                                                     times=[60, 0, 120, 180, 240, 300],
                                                     kinds=["L", "H", "L", "H", "H", "L"])


def _diagonal_cases():
    yield "contracting_with_overlap", "ending", [100, 120, 110, 125, 116, 124]
    yield "expanding_with_overlap", "leading", [100, 110, 102, 125, 90, 140]
    yield "wave2_beyond_start_of_wave1", "leading", [100, 120, 95, 140, 130, 150]
    yield "wave2_on_start_of_wave1", "leading", [100, 120, 100, 140, 130, 150]
    yield "wave3_shortest", "ending", [100, 130, 120, 135, 125, 160]
    yield "wave4_on_end_of_wave1", "ending", [100, 120, 110, 140, 120, 150]
    yield "wave4_clear_by_one", "ending", [100, 120, 110, 140, 121, 150]
    yield "shrinking_135_but_not_24", "ending", [100, 120, 110, 125, 114, 124]
    yield "four_nodes", "leading", [100, 120, 110, 125]
    yield "high_below_the_low_before_it", "leading", [100, 120, 110, 105, 100, 124]


# Corrections: wave A rises from 1000 to 2000, so B = 2000 - 1000 r and the thresholds are whole
# prices. C is compared with the end of A (2000) and the end of B.
def _correction_cases():
    def abc(b, c):
        return [1000, 2000, b, c]

    yield "zigzag", {}, abc(1400, 2600)
    yield "zigzag_c_short_of_a", {}, abc(1400, 1900)
    yield "zigzag_c_on_end_of_a", {}, abc(1400, 2000)
    yield "zigzag_c_beyond_a_by_one", {}, abc(1400, 2001)
    # zigzag_b_retrace_bound, guideline at 0.786.
    yield "b_0785", {}, abc(1215, 2500)
    yield "b_0786", {}, abc(1214, 2500)
    yield "b_0787", {}, abc(1213, 2500)
    yield "b_0850_former_gap", {}, abc(1150, 2500)
    # Zigzag against flat at 0.90.
    yield "b_0899", {}, abc(1101, 2200)
    yield "b_0900", {}, abc(1100, 2200)
    yield "b_0901", {}, abc(1099, 2200)
    yield "flat", {}, abc(1050, 2200)
    yield "flat_c_short_of_a", {}, abc(1050, 1900)
    yield "flat_c_on_end_of_a", {}, abc(1050, 2000)
    # B against the start of A at 1.00.
    yield "b_1000_c_beyond", {}, abc(1000, 2200)
    yield "b_1000_c_short", {}, abc(1000, 1900)
    yield "b_1001_c_beyond", {}, abc(999, 2200)
    yield "b_1001_c_short", {}, abc(999, 1900)
    # Expanded flat above 1.05.
    yield "b_1049_c_beyond", {}, abc(951, 2200)
    yield "b_1050_c_beyond", {}, abc(950, 2200)
    yield "b_1051_c_beyond", {}, abc(949, 2200)
    yield "expanded_flat", {}, abc(900, 2350)
    # Expanded against running at the end of A.
    yield "b_1100_c_short_by_one", {}, abc(900, 1999)
    yield "b_1100_c_on_end_of_a", {}, abc(900, 2000)
    yield "b_1100_c_beyond_by_one", {}, abc(900, 2001)
    yield "running_flat", {}, abc(900, 1750)
    # flat_b_retrace_bound, guideline at 1.382 or at the option.
    yield "b_1381", {}, abc(619, 2200)
    yield "b_1382", {}, abc(618, 2200)
    yield "b_1383", {}, abc(617, 2200)
    yield "b_1383_running", {}, abc(617, 1900)
    yield "b_1200_bound_1200", {"flat_b_typical_max": 1.2}, abc(800, 2200)
    yield "b_1250_bound_1200", {"flat_b_typical_max": 1.2}, abc(750, 2200)
    # wave_c_must_extend_past_b.
    yield "c_on_end_of_b", {}, abc(1400, 1400)
    yield "c_behind_b", {}, abc(1400, 1300)
    yield "c_ahead_of_b_by_one", {}, abc(1400, 1401)
    yield "flat_c_behind_b", {}, abc(1050, 1000)
    # Wave A asserted to be five waves: a zigzag, B never beyond the start of A.
    yield "five_b_0600", {"wave_a": "five"}, abc(1400, 2600)
    yield "five_b_0950", {"wave_a": "five"}, abc(1050, 2200)
    yield "five_b_1000", {"wave_a": "five"}, abc(1000, 2200)
    yield "five_b_1001", {"wave_a": "five"}, abc(999, 2200)
    # Wave A asserted to be three waves: a flat, B at least 0.90.
    yield "three_b_0600", {"wave_a": "three"}, abc(1400, 2600)
    yield "three_b_0899", {"wave_a": "three"}, abc(1101, 2200)
    yield "three_b_0900", {"wave_a": "three"}, abc(1100, 2200)
    yield "three_b_1100_c_short", {"wave_a": "three"}, abc(900, 1900)
    # No pivot sequence.
    yield "three_nodes", {}, [1000, 2000, 1400]
    yield "two_highs_in_a_row", {}, rising(abc(1400, 2600), kinds=["L", "H", "H", "L"])
    yield "b_earlier_than_a", {}, rising(abc(1400, 2600), times=[0, 120, 60, 180])
    yield "a_on_start", {}, [1000, 1000, 900, 1500]
    yield "b_above_a", {}, abc(2100, 2600)
    yield "b_on_end_of_a", {}, abc(2000, 2600)


# Triangles: wave a rises, so a, c and e end at highs and b and d at lows.
def _triangle_cases():
    yield "contracting", {}, [100, 130, 110, 125, 115, 122]
    yield "contracting_running", {}, [100, 130, 95, 122, 104, 118]
    yield "b_on_start_of_a_not_running", {}, [100, 130, 100, 122, 108, 118]
    # Contracting rules, with the tie on each.
    yield "c_on_end_of_a", {}, [100, 130, 110, 130, 115, 122]
    yield "c_beyond_a", {}, [100, 130, 110, 131, 115, 122]
    yield "d_beyond_b", {}, [100, 130, 110, 125, 108, 120]
    yield "e_on_end_of_c", {}, [100, 130, 110, 125, 115, 125]
    yield "e_beyond_c", {}, [100, 130, 110, 125, 115, 126]
    # Barrier: wave b is 20 long, the default tolerance 5 percent of it, one point.
    yield "barrier_tie", {}, [100, 130, 110, 125, 110, 120]
    yield "barrier_d_inside_by_tolerance", {}, [100, 130, 110, 125, 111, 120]
    yield "barrier_d_outside_by_tolerance", {}, [100, 130, 110, 125, 109, 120]
    yield "d_inside_past_tolerance", {}, [100, 130, 110, 125, 112, 120]
    yield "barrier_running", {}, [100, 130, 95, 122, 95, 118]
    yield "barrier_c_beyond_a", {}, [100, 130, 110, 131, 110, 120]
    yield "barrier_e_beyond_c", {}, [100, 130, 110, 125, 110, 126]
    yield "tolerance_zero_tie", {"barrier_tolerance": 0.0}, [100, 130, 110, 125, 110, 120]
    yield "tolerance_zero_one_off", {"barrier_tolerance": 0.0}, [100, 130, 110, 125, 111, 120]
    yield "tolerance_quarter", {"barrier_tolerance": 0.25}, [100, 130, 110, 125, 115, 122]
    # Expanding, valid, and each of its rules alone.
    yield "expanding", {}, [100, 110, 98, 113, 94, 120]
    yield "expanding_c_on_end_of_a", {}, [100, 110, 98, 110, 94, 120]
    yield "expanding_d_on_end_of_b", {}, [100, 110, 98, 113, 98, 120]
    yield "expanding_e_on_end_of_c", {}, [100, 110, 98, 113, 94, 113]
    yield "expanding_b_retraces_090", {}, [100, 110, 101, 114, 96, 120]
    yield "expanding_b_retraces_100", {}, [100, 110, 100, 113, 96, 120]
    yield "expanding_b_retraces_150", {}, [100, 110, 95, 113, 90, 120]
    yield "expanding_b_retraces_160", {}, [100, 110, 94, 113, 90, 120]
    yield "expanding_c_retraces_167", {}, [100, 110, 95, 120, 85, 140]
    yield "expanding_d_retraces_160", {}, [100, 110, 98, 113, 89, 120]
    # Neither: both boundaries rise. Two of three waves are beyond, so it is measured as expanding.
    yield "both_boundaries_rise", {}, [100, 120, 110, 122, 116, 123]
    # No pivot sequence.
    yield "seven_nodes", {}, [100, 130, 110, 125, 115, 122, 118]
    yield "two_lows_in_a_row", {}, rising([100, 130, 110, 105, 115, 122],
                                          kinds=["L", "H", "L", "L", "H", "L"])
    yield "e_earlier_than_d", {}, rising([100, 130, 110, 125, 115, 122],
                                         times=[0, 60, 120, 180, 240, 200])
    yield "c_below_b", {}, [100, 130, 110, 105, 100, 122]


def _combination_cases():
    yield "double_zigzag", {}, [1000, 2000, 1400, 2600, 2100, 3100, 2500, 3700]
    yield "zigzag_then_flat", {}, [1000, 2000, 1400, 2600, 2100, 3100, 2150, 3200]
    yield "w_without_three_waves", {}, [1000, 2000, 1400, 1300, 1200, 2200, 1600, 2800]
    yield "triple_three", {}, [1000, 2000, 1400, 2600, 2100, 3100, 2150, 3200,
                               2900, 3900, 3300, 4500]
    yield "asserted_five", {"wave_a": "five"}, [1000, 2000, 1400, 2600, 2100, 3100, 2150, 3200]
    yield "nine_nodes", {}, [1000, 2000, 1400, 2600, 2100, 3100, 2500, 3700, 3300]
    yield "x_above_end_of_w", {}, [1000, 2000, 1400, 2600, 2700, 3100, 2900, 3700]
    yield "y_b_above_y_a", {}, [1000, 2000, 1400, 2600, 2100, 3100, 3200, 3700]


def _measure_cases():
    yield "wave3_extended", [100, 110, 105, 145, 135, 150]
    yield "no_extension", [100, 120, 110, 142, 130, 148]
    yield "wave1_extended", [100, 200, 170, 210, 190, 230]
    yield "wave5_extended", [100, 120, 110, 140, 130, 200]
    yield "truncated", [100, 130, 115, 150, 135, 145]
    yield "fifth_on_end_of_third", [100, 130, 115, 150, 135, 150]
    yield "fifth_beyond_third", [100, 130, 115, 150, 135, 160]
    yield "low_above_the_high_before_it", [100, 120, 125, 140, 130, 150]


# --- rendering ------------------------------------------------------------------------------------

def _nodes(spec):
    return spec if spec and isinstance(spec[0], tuple) else rising(spec)


def _names(items):
    return ",".join(items) if items else "-"


def _flag(value):
    return "1" if value else "0"


def _outcome(result, fields):
    if result[0] == "error":
        _, name, index = result
        return f"error={name}" if index is None else f"error={name}@{index}"
    return " ".join(fields(result[1]))


def _valid(violations):
    return _flag(not violations)


def _impulse(nodes, _):
    return _outcome(elliott.impulse(nodes), lambda r: [
        f"direction={r['direction']}", f"valid={_valid(r['violations'])}",
        f"violations={_names(r['violations'])}", f"quality={r['quality']!r}"])


def _diagonal(nodes, _):
    return _outcome(elliott.diagonal(nodes), lambda r: [
        f"direction={r['direction']}", f"variant={r['variant']}",
        f"valid={_valid(r['violations'])}", f"violations={_names(r['violations'])}",
        f"overlap={_flag(r['overlap'])}"])


def _correction_fields(r):
    return [f"direction={r['direction']}", f"variant={r['variant']}",
            f"valid={_valid(r['violations'])}", f"violations={_names(r['violations'])}",
            f"guidelines={_names(r['guidelines'])}", f"quality={r['quality']!r}"]


def _correction(nodes, options):
    return _outcome(elliott.correction(nodes, **options), _correction_fields)


def _triangle(nodes, options):
    return _outcome(elliott.triangle(nodes, **options), lambda r: [
        f"direction={r['direction']}", f"variant={r['variant']}",
        f"running={_flag(r['running'])}", f"valid={_valid(r['violations'])}",
        f"violations={_names(r['violations'])}"])


def _combination(nodes, options):
    def fields(r):
        segments = ";".join(
            "/".join([s["variant"], _valid(s["violations"]), _names(s["violations"]),
                      _names(s["guidelines"])]) for s in r["segments"])
        valid = all(not s["violations"] for s in r["segments"])
        return [f"variant={r['variant']}", f"valid={_flag(valid)}", f"segments={segments}"]
    return _outcome(elliott.combination(nodes, **options), fields)


def _measures(nodes, _):
    wave = elliott.extended_wave(nodes)
    truncated = elliott.truncated_fifth(nodes)
    return (f"extended={'none' if wave is None else wave} "
            f"truncated={'none' if truncated is None else _flag(truncated)}")


def _line(structure, name, options, nodes, expected):
    shown = " ".join(f"{key}={value}" for key, value in options.items()) or "-"
    points = " ".join(f"{kind}@{time}:{price!r}" for kind, time, price in nodes)
    return "\t".join([structure, name, shown, points, expected])


def _both(structure, name, options, spec, evaluate, call_options=None):
    call_options = options if call_options is None else call_options
    nodes = _nodes(spec)
    for suffix, sequence in (("", nodes), ("_mirrored", mirrored(nodes))):
        yield _line(structure, name + suffix, options, sequence, evaluate(sequence, call_options))


# The grid over the correction forms: B at every twentieth of wave A from 0.10 to 1.60 and one
# thousandth to either side of every threshold; C one point past B, around the end of A, well
# beyond it, on B and behind B.
GRID_B = sorted(set(range(100, 1601, 50)) | {
    k + step for k in (786, 900, 1000, 1050, 1382) for step in (-1, 0, 1)})
CODES = {"zigzag": "Z", "flat": "F", "expanded_flat": "E", "running_flat": "R"}


def _grid():
    for direction in ("up", "down"):
        for wave_a in (None, "five", "three"):
            for k in GRID_B:
                b = 2000 - k
                ends = sorted({b - 50, b, b + 1, 1999, 2000, 2001, 2500, 4500})
                cells, shown = [], None
                for c in ends:
                    nodes = rising([1000, 2000, b, c])
                    if direction == "down":
                        nodes = [("L" if kind == "H" else "H", time, 6000.0 - price)
                                 for kind, time, price in nodes]
                    shown = shown or nodes[:3]
                    _, result = elliott.correction(nodes, wave_a=wave_a)
                    cells.append(f"{nodes[3][2]!r}:{CODES[result['variant']]}"
                                 f"{_valid(result['violations'])}")
                prices = " ".join(repr(node[2]) for node in shown)
                yield "\t".join(["correction_grid", direction, wave_a or "-", prices,
                                 " ".join(cells)])


HEADER = [
    "kestrel-chartkit scenario reference fixture: Elliott structure checks",
    "",
    *provenance("scenario_elliott"),
    "",
    "The second implementation is reference/kestrel_reference/elliott.py, written from the rules",
    "documented in src/indicator/elliott.rs. Every sequence is listed twice: as built, rising, and",
    "mirrored (highs for lows, prices reflected about their mid-range).",
    "",
    "Line: structure<TAB>case<TAB>options<TAB>nodes<TAB>expected.",
    "  nodes     kind@timestamp:price, kind H (pivot high) or L (pivot low)",
    "  expected  error=<condition>[@<node>] for a sequence that is no pivot sequence, with the",
    "            condition count, alternation, time or placement; otherwise key=value fields,",
    "            lists comma-separated in the order they are reported, - for an empty list.",
    "  combination segments: variant/valid/violations/guidelines per segment, joined by ;",
    "  measures: identify_extended_wave and is_truncated_fifth on the same six nodes.",
    "",
    "correction_grid<TAB>direction<TAB>asserted wave A<TAB>prices of 0, A, B<TAB>cells.",
    "  cell      <price of C>:<form><valid>, form Z zigzag, F flat, E expanded flat, R running flat",
    "  Wave A is 1000 long, so the retracement of B is its distance from the end of A in",
    "  thousandths. The grid covers both directions and the three assertions about wave A.",
]


def render(header, rows):
    return "\n".join([f"# {line}".rstrip() for line in header] + rows) + "\n"


RENDER = render


def build():
    rows = []
    for name, spec in _impulse_cases():
        rows += _both("impulse", name, {}, spec, _impulse)
    for name, kind, spec in _diagonal_cases():
        rows += _both("diagonal", name, {"kind": kind}, spec, _diagonal, {})
    for name, options, spec in _correction_cases():
        rows += _both("correction", name, options, spec, _correction)
    for name, options, spec in _triangle_cases():
        rows += _both("triangle", name, options, spec, _triangle)
    for name, options, spec in _combination_cases():
        rows += _both("combination", name, options, spec, _combination)
    for name, spec in _measure_cases():
        rows += _both("measures", name, {}, spec, _measures)
    rows += _grid()
    return HEADER, rows
