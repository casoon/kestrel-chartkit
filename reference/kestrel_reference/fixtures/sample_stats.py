"""golden_sample_stats: Wilson score interval and longest run, from their definitions."""

import math
from itertools import groupby
from statistics import NormalDist

from ..fixture import provenance

NAME = "golden_sample_stats"

Z95 = NormalDist().inv_cdf(0.975)
Z99 = NormalDist().inv_cdf(0.995)

# (successes, trials, z)
WILSON_CASES = [
    (0, 10, Z95),
    (10, 10, Z95),
    (7, 10, Z95),
    (1, 2, Z95),
    (1, 1, Z95),
    (55, 100, Z95),
    (3, 1000, Z95),
    (500, 1000, Z95),
    (7, 10, Z99),
]

RUN_CASES = [
    [1.0, -1.0, -2.0, 3.0, -1.0, -1.0, -1.0, 2.0],
    [],
    [-1.0, -1.0, -1.0],
    [1.0, 2.0, 3.0],
    [-1.0, 0.0, -1.0, -1.0],
    [2.0, -0.5, -0.5, 1.0, -3.0, -3.0, -3.0, -3.0, 0.1, -1.0],
]

QUANTILE_CASES = [
    ([3.0, 1.0, 2.0], 0.5),
    ([4.0, 1.0, 3.0, 2.0], 0.5),
    ([4.0, 1.0, 3.0, 2.0], 0.75),
    ([5.0, -1.0, 2.5, 7.0, 0.0, 3.0], 0.25),
    ([5.0, -1.0, 2.5, 7.0, 0.0, 3.0], 0.9),
    ([1.5], 0.3),
    ([2.0, 1.0], 0.0),
    ([2.0, 1.0], 1.0),
]

DAY = 86_400_000
# (timestamp, value) pairs; several per day so the clustering matters.
BLOCK_A = [(0, 1.0), (1000, 2.0), (DAY + 5, -1.0), (DAY + 9, 0.5), (3 * DAY, 4.0),
           (3 * DAY + 7, 2.0), (5 * DAY, -2.0)]
BLOCK_B = [(10, 0.5), (DAY + 1, 0.0), (2 * DAY, 1.0), (3 * DAY + 1, 1.5), (6 * DAY, -0.5)]
BLOCK_SINGLE = [(0, 1.0), (DAY, 3.0), (2 * DAY, 2.0), (3 * DAY, 6.0)]

HEADER = [
    "kestrel-chartkit golden reference fixture: sample statistics for trade evaluation",
    "",
    *provenance("sample_stats"),
    "",
    "Wilson score interval. The bounds are derived here as the two roots of",
    "(p - pi)^2 = z^2 pi (1 - pi) / n, i.e. of the quadratic",
    "(1 + z^2/n) pi^2 - (2p + z^2/n) pi + p^2 = 0 - a different route from the centre and",
    "half-width form the crate documents, same interval. z comes from",
    "statistics.NormalDist().inv_cdf(0.975) resp. (0.995) and is carried in the fixture, so both",
    "sides use the identical quantile. wilson<i>_successes/_trials/_z are the inputs,",
    "_estimate/_lower/_upper the expected result.",
    "",
    "wilson<i>_wald_lower/_upper is the Wald interval p -/+ z sqrt(p(1 - p)/n), clamped to [0, 1].",
    "The crate deliberately does not use it; it is recorded so the difference is on record as a",
    "number - at p = 0 or 1 Wald has zero width.",
    "",
    "Longest run: run<i>_len values run<i>_v<j>; run<i>_longest_negative is the longest stretch of",
    "consecutive values below zero (a zero breaks the stretch), counted with itertools.groupby.",
    "",
    "Nearest-rank quantile: q<i>_n values q<i>_v<j>, level q<i>_q; q<i>_result is the element at",
    "position floor((n-1)q + 1/2) of the sorted values (rounding half away from zero, written out",
    "because Python's round() rounds half to even).",
    "",
    "Block statistics (block = one day, 86 400 000 ms): series a, b and single as <s>_n,",
    "<s>_t<j>, <s>_x<j>. Mean of a; cluster-robust standard error: per day the sum of deviations",
    "from the mean, se = sqrt(sum of squares) / n * sqrt(G/(G-1)) over G days (days in ascending",
    "order). For `single` (one value per day) this is the textbook s/sqrt(n), recorded as",
    "single_textbook_se. Difference a - b: per day sum (x - mean_a)/n_a minus sum (y - mean_b)/n_b,",
    "se = sqrt(sum of squares * G/(G-1)).",
]


def _wilson(successes, trials, z):
    n = float(trials)
    p = successes / n
    a = 1.0 + z * z / n
    b = -(2.0 * p + z * z / n)
    c = p * p
    root = math.sqrt(b * b - 4.0 * a * c)
    lower = (-b - root) / (2.0 * a)
    upper = (-b + root) / (2.0 * a)
    return p, min(max(lower, 0.0), 1.0), min(max(upper, 0.0), 1.0)


def _wald(successes, trials, z):
    p = successes / trials
    half = z * math.sqrt(p * (1.0 - p) / trials)
    return max(p - half, 0.0), min(p + half, 1.0)


def _longest_negative(values):
    return max((len(list(group)) for negative, group in groupby(values, key=lambda v: v < 0.0)
                if negative), default=0)


def _nearest_rank(values, q):
    v = sorted(values)
    return v[math.floor((len(v) - 1) * q + 0.5)]


def _block_mean(series):
    n = len(series)
    mean = sum(x for _, x in series) / n
    sums = {}
    for t, x in series:
        day = t // DAY
        sums[day] = sums.get(day, 0.0) + (x - mean)
    g = len(sums)
    sq = 0.0
    for day in sorted(sums):
        sq += sums[day] * sums[day]
    return mean, math.sqrt(sq) / n * math.sqrt(g / (g - 1)), g


def _block_difference(a, b):
    ma = sum(x for _, x in a) / len(a)
    mb = sum(x for _, x in b) / len(b)
    sums = {}
    for t, x in a:
        sums[t // DAY] = sums.get(t // DAY, 0.0) + (x - ma) / len(a)
    for t, x in b:
        sums[t // DAY] = sums.get(t // DAY, 0.0) - (x - mb) / len(b)
    g = len(sums)
    sq = 0.0
    for day in sorted(sums):
        sq += sums[day] * sums[day]
    return ma - mb, math.sqrt(sq * g / (g - 1))


def build():
    blocks = [(None, "case counts", {"meta_wilson_case_count": float(len(WILSON_CASES)),
                                      "meta_run_case_count": float(len(RUN_CASES))})]
    for i, (successes, trials, z) in enumerate(WILSON_CASES):
        estimate, lower, upper = _wilson(successes, trials, z)
        wald_lower, wald_upper = _wald(successes, trials, z)
        blocks.append((f"wilson{i}", f"{successes} of {trials}", {
            "successes": float(successes),
            "trials": float(trials),
            "z": z,
            "estimate": estimate,
            "lower": lower,
            "upper": upper,
            "wald_lower": wald_lower,
            "wald_upper": wald_upper,
        }))
    for i, values in enumerate(RUN_CASES):
        case = {"len": float(len(values))}
        case.update({f"v{j}": v for j, v in enumerate(values)})
        case["longest_negative"] = float(_longest_negative(values))
        blocks.append((f"run{i}", None, case))
    blocks.append((None, "quantile cases", {"meta_quantile_case_count": float(len(QUANTILE_CASES))}))
    for i, (values, q) in enumerate(QUANTILE_CASES):
        case = {"n": float(len(values)), "q": q}
        case.update({f"v{j}": v for j, v in enumerate(values)})
        case["result"] = _nearest_rank(values, q)
        blocks.append((f"q{i}", None, case))
    for name, series in (("a", BLOCK_A), ("b", BLOCK_B), ("single", BLOCK_SINGLE)):
        case = {"n": float(len(series))}
        for j, (t, x) in enumerate(series):
            case[f"t{j}"] = float(t)
            case[f"x{j}"] = x
        mean, se, g = _block_mean(series)
        case.update({"mean": mean, "se": se, "blocks": float(g)})
        if name == "single":
            n = len(series)
            var = sum((x - mean) ** 2 for _, x in series) / (n - 1)
            case["textbook_se"] = math.sqrt(var / n)
        blocks.append((f"block_{name}", f"block statistics, series {name}", case))
    diff, se = _block_difference(BLOCK_A, BLOCK_B)
    blocks.append(("block_diff", "a minus b", {"value": diff, "se": se}))
    blocks.append((None, None, {"sample_stats_tolerance": 1e-12}))
    return HEADER, blocks
