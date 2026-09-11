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
    blocks.append((None, None, {"sample_stats_tolerance": 1e-12}))
    return HEADER, blocks
