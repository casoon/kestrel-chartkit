"""Elliott structure checks on a labeled pivot sequence, written from the rules documented in
`src/indicator/elliott.rs` (module documentation and the `check_*` functions), not from its code.

A node is `(kind, timestamp, price)` with kind `"H"` (a pivot high) or `"L"` (a pivot low). Every
check returns either `("error", name, index)` for a sequence that is no pivot sequence, or
`("ok", result)` with a dict.

The documentation states each rule for a rising sequence and says a falling one mirrors every
comparison. Here that is taken literally: `further(direction, x, y)` asks whether `x` lies
strictly further than `y` in a direction, and each rule is one such question.
"""

# Thresholds, as documented on the public constants of the module.
FLAT_B_MIN_RETRACE = 0.90
EXPANDED_FLAT_B_MIN = 1.05
ZIGZAG_B_TYPICAL_MAX = 0.786
FLAT_B_TYPICAL_MAX = 1.382
EXPANDING_TRIANGLE_RETRACE = (1.0, 1.5)
BARRIER_LEVEL_TOLERANCE = 0.05
EXTENSION_RATIO = 1.618
COMMON_RETRACEMENTS = (0.382, 0.5, 0.618)


def opposite(direction):
    return "down" if direction == "up" else "up"


def further(direction, x, y):
    """Whether price `x` lies strictly further than `y` in `direction`."""
    return x > y if direction == "up" else x < y


def sequence_error(nodes, counts, placed_by_rule=()):
    """The first broken pivot-sequence condition as `(name, index)`, or `None`.

    Order as documented: node count, then alternation over the whole sequence, then time order,
    then placement; within one condition the first node that breaks it.
    """
    if len(nodes) not in counts:
        return "count", None
    pairs = [(i, nodes[i - 1], nodes[i]) for i in range(1, len(nodes))]
    for i, before, node in pairs:
        if node[0] == before[0]:
            return "alternation", i
    for i, before, node in pairs:
        if node[1] < before[1]:
            return "time", i
    for i, before, node in pairs:
        if i in placed_by_rule:
            continue
        above, below = node[2] > before[2], node[2] < before[2]
        if not (above if node[0] == "H" else below):
            return "placement", i
    return None


def direction_of(nodes):
    """Direction of the first leg: a sequence that starts at a low rises."""
    return "up" if nodes[0][0] == "L" else "down"


def fib_distance(ratio):
    return min(abs(level - ratio) for level in COMMON_RETRACEMENTS)


def clamp01(value):
    return min(max(value, 0.0), 1.0)


def _checked(nodes, counts, placed_by_rule=()):
    error = sequence_error(nodes, counts, placed_by_rule)
    if error is not None:
        return None, ("error", error[0], error[1])
    return direction_of(nodes), None


def impulse(nodes):
    direction, error = _checked(nodes, (6,))
    if error:
        return error
    p = [node[2] for node in nodes]
    violations = []
    # Wave 2 never moves beyond the start of wave 1; ending exactly on it counts as a violation.
    if not further(direction, p[2], p[0]):
        violations.append("wave2_no_full_retrace")
    len1, len3, len5 = abs(p[1] - p[0]), abs(p[3] - p[2]), abs(p[5] - p[4])
    if len3 < len1 and len3 < len5:
        violations.append("wave3_not_shortest")
    # Wave 4 never moves beyond the end of wave 1; the tie counts as a violation.
    if not further(direction, p[4], p[1]):
        violations.append("wave4_no_overlap")
    rest2 = abs(p[2] - p[0]) / len1
    retrace4 = abs(p[3] - p[4]) / len3
    quality = clamp01(1.0 - (fib_distance(rest2) + fib_distance(retrace4)))
    return "ok", {"direction": direction, "violations": violations, "quality": quality}


def diagonal(nodes):
    direction, error = _checked(nodes, (6,))
    if error:
        return error
    p = [node[2] for node in nodes]
    lengths = [abs(p[i] - p[i - 1]) for i in range(1, 6)]
    len1, len2, len3, len4, len5 = lengths
    violations = []
    if not further(direction, p[2], p[0]):
        violations.append("wave2_no_full_retrace")
    if len3 < len1 and len3 < len5:
        violations.append("wave3_not_shortest")
    contracting = len5 < len3 < len1 and len4 < len2
    return "ok", {
        "direction": direction,
        "variant": "contracting" if contracting else "expanding",
        "violations": violations,
        # Wave 4 reaches wave 1's territory when it does not stay beyond the end of wave 1.
        "overlap": not further(direction, p[4], p[1]),
    }


def correction_of(nodes, direction, wave_a=None, flat_b_typical_max=FLAT_B_TYPICAL_MAX):
    """The evaluation of four nodes `[0, A, B, C]` whose wave A runs in `direction`."""
    start, a, b, c = (node[2] for node in nodes)
    against = opposite(direction)
    leg_a = abs(a - start)
    retrace_b = abs(b - a) / leg_a
    c_in_a = abs(c - b) / leg_a
    b_beyond_start = further(against, b, start)
    c_continues = further(direction, c, b)
    c_beyond_a = further(direction, c, a)

    if wave_a == "five":
        family = "zigzag"
    elif wave_a == "three":
        family = "flat"
    else:
        family = "flat" if retrace_b >= FLAT_B_MIN_RETRACE else "zigzag"

    # The rows of the documented table, one after the other.
    if family == "zigzag":
        variant = "zigzag"
    elif b_beyond_start and not c_beyond_a:
        variant = "running_flat"
    elif retrace_b > EXPANDED_FLAT_B_MIN and c_beyond_a:
        variant = "expanded_flat"
    else:
        variant = "flat"

    violations, guidelines = [], []
    if not c_continues:
        violations.append("wave_c_must_extend_past_b")
    if family == "flat":
        if retrace_b < FLAT_B_MIN_RETRACE:
            violations.append("flat_b_min_retrace")
        if retrace_b > flat_b_typical_max:
            guidelines.append("flat_b_retrace_bound")
        if variant == "flat" and not c_beyond_a:
            guidelines.append("flat_c_ends_beyond_a")
        quality = clamp01(1.0 - min(abs(1.0 - min(c_in_a, 2.0)), 1.0))
    else:
        if b_beyond_start:
            violations.append("zigzag_b_not_beyond_start_of_a")
        if retrace_b > ZIGZAG_B_TYPICAL_MAX:
            guidelines.append("zigzag_b_retrace_bound")
        if not c_beyond_a:
            guidelines.append("zigzag_c_ends_beyond_a")
        quality = clamp01(1.0 - fib_distance(min(retrace_b, 1.0)))
    return {"direction": direction, "variant": variant, "violations": violations,
            "guidelines": guidelines, "quality": quality}


def correction(nodes, wave_a=None, flat_b_typical_max=FLAT_B_TYPICAL_MAX):
    direction, error = _checked(nodes, (4,), placed_by_rule=(3,))
    if error:
        return error
    return "ok", correction_of(nodes, direction, wave_a, flat_b_typical_max)


def triangle(nodes, barrier_tolerance=BARRIER_LEVEL_TOLERANCE):
    direction, error = _checked(nodes, (6,))
    if error:
        return error
    start, a, b, c, d, e = (node[2] for node in nodes)
    against = opposite(direction)
    # a, c and e run in `direction`, b and d against it.
    beyond = {
        "c": further(direction, c, a),
        "d": further(against, d, b),
        "e": further(direction, e, c),
    }
    len_a, len_b, len_c, len_d = abs(a - start), abs(b - a), abs(c - b), abs(d - c)
    if sum(beyond.values()) >= 2:
        variant = "expanding"
    elif abs(d - b) <= barrier_tolerance * len_b:
        variant = "barrier"
    else:
        variant = "contracting"

    violations = []
    if variant == "expanding":
        for wave, earlier in (("c", "a"), ("d", "b"), ("e", "c")):
            if not beyond[wave]:
                violations.append(f"expanding_triangle_{wave}_beyond_{earlier}")
        low, high = EXPANDING_TRIANGLE_RETRACE
        for wave, retrace in (("b", len_b / len_a), ("c", len_c / len_b), ("d", len_d / len_c)):
            if retrace < low or retrace > high:
                violations.append(f"expanding_triangle_{wave}_retrace")
    else:
        if beyond["c"]:
            violations.append("triangle_c_not_beyond_a")
        if beyond["d"] and variant == "contracting":
            violations.append("triangle_d_not_beyond_b")
        if beyond["e"]:
            violations.append("triangle_e_not_beyond_c")
    running = variant != "expanding" and further(against, b, start)
    return "ok", {"direction": direction, "variant": variant, "running": running,
                  "violations": violations}


def combination(nodes, wave_a=None, flat_b_typical_max=FLAT_B_TYPICAL_MAX):
    direction, error = _checked(nodes, (8, 12), placed_by_rule=(3, 7, 11))
    if error:
        return error
    segments = [correction_of(nodes[i:i + 4], direction, wave_a, flat_b_typical_max)
                for i in range(0, len(nodes), 4)]
    return "ok", {"variant": "double_three" if len(nodes) == 8 else "triple_three",
                  "segments": segments}


def extended_wave(nodes):
    """1, 3 or 5 for the first wave at least 1.618 times the longer of the other two, else `None`
    (also for a sequence that is no pivot sequence)."""
    if sequence_error(nodes, (6,)) is not None:
        return None
    p = [node[2] for node in nodes]
    lengths = {1: abs(p[1] - p[0]), 3: abs(p[3] - p[2]), 5: abs(p[5] - p[4])}
    for wave in (1, 3, 5):
        others = max(length for other, length in lengths.items() if other != wave)
        if lengths[wave] >= EXTENSION_RATIO * others:
            return wave
    return None


def truncated_fifth(nodes):
    """Whether wave 5 fails to end beyond the end of wave 3; `None` for no pivot sequence."""
    if sequence_error(nodes, (6,)) is not None:
        return None
    return not further(direction_of(nodes), nodes[5][2], nodes[3][2])
