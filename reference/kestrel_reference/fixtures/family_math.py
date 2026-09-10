"""golden_family_math: agreement, outcome statistics and cross-asset math, each value from the
definition documented on its function."""

from fractions import Fraction

from .. import exact
from ..fixture import provenance

NAME = "golden_family_math"

# (timeframe, direction, strength); direction +1 bullish, -1 bearish.
STATEMENTS = [(1, 1, 0.2), (1, 1, 0.3), (2, -1, 0.9)]
OUTCOMES = [10.0, -2.0, 0.0, None]
CLOSES_A = [100.0, 110.0, 132.0, 118.8]
CLOSES_B = [100.0, 90.0, 72.0, 79.2]

HEADER = [
    "kestrel-chartkit golden reference fixture: agreement, outcome statistics, cross-asset math",
    "",
    *provenance("family_math"),
    "",
    "Each value from the definition documented on its function:",
    "  aggregate_agreement  statements (tf 1, bullish, 0.2), (tf 1, bullish, 0.3),",
    "             (tf 2, bearish, 0.9); agreement = winning weight / total weight of the",
    "             non-neutral statements, weight 1 (Majority) or the strength (WeightedByStrength).",
    "  PriceStats::compute  values [10, -2, 0, None]: closed_count counts None too,",
    "             win_rate = values > 0 / closed_count, avg_pnl = total / present values.",
    "  correlation_matrix  closes A [100, 110, 132, 118.8], B [100, 90, 72, 79.2] on common",
    "             timestamps: returns (c_t - c_(t-1)) / c_(t-1), Pearson over the last 3, computed",
    "             exactly from the binary inputs.",
    "  relative_strength_ranking  lookback 1: 100 (c_last - c_base) / c_base for A.",
    "",
    "family_math_tolerance is the comparison tolerance the tests apply, stated rather than derived.",
]


def _agreement(statements, by_strength):
    bull = bear = 0.0
    for _, direction, strength in statements:
        weight = strength if by_strength else 1.0
        if direction > 0:
            bull += weight
        else:
            bear += weight
    return max(bull, bear) / (bull + bear)


def _returns(closes):
    exact_closes = [Fraction(c) for c in closes]
    return [(b - a) / a for a, b in zip(exact_closes, exact_closes[1:])]


def derive():
    present = [v for v in OUTCOMES if v is not None]
    total = sum(present)
    a = Fraction(CLOSES_A[-1])
    base = Fraction(CLOSES_A[-2])
    return {
        "majority": _agreement(STATEMENTS, by_strength=False),
        "weighted": _agreement(STATEMENTS, by_strength=True),
        "win_rate": sum(1 for v in present if v > 0.0) / len(OUTCOMES),
        "average": total / len(present),
        "total": total,
        "correlation": float(exact.correlation(_returns(CLOSES_A)[-3:], _returns(CLOSES_B)[-3:])),
        "relative_strength": float(100 * (a - base) / base),
    }


SECTIONS = [
    ("aggregate_agreement", ["majority", "weighted"]),
    ("PriceStats::compute", ["win_rate", "average", "total"]),
    ("correlation_matrix and relative_strength_ranking", ["correlation", "relative_strength"]),
    (None, ["family_math_tolerance"]),
]


def build():
    values = {**derive(), "family_math_tolerance": 1e-10}
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in SECTIONS]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
