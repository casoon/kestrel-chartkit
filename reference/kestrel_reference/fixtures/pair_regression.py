"""golden_pair_regression: the two-series indicators `rolling_beta` and `pair_spread`, from the
definitions documented on `RollingBetaEngine` and `PairSpreadEngine`."""

import math
from fractions import Fraction

from ..fixture import provenance
from .pair_indicators import A16, B16, B_GAPS, _block, _paired, _stamped

NAME = "golden_pair_regression"

# (first series, reference series, len), each series as [(timestamp, close), ...]
RB_CASES = [
    # By hand, len = 3: the reference returns 1 %, 2 %, 3 % and the series returns exactly twice
    # that, 2 %, 4 %, 6 %. Every deviation of the series is twice the reference's, so beta = 2,
    # alpha = 4 % - 2 * 2 % = 0 and the fit is perfect.
    (_stamped([100.0, 102.0, 106.08, 112.4448]), _stamped([100.0, 101.0, 103.02, 106.1106]), 3),
    (_stamped(A16), _stamped(B16), 5),
    (_stamped(A16), _stamped(B16), 15),
    # Pairing: returns run over consecutive *paired* bars.
    (_stamped(A16), B_GAPS, 5),
    # A reference that does not move has no beta: no output.
    (_stamped(A16[:8]), _stamped([75.0] * 8), 3),
    # The reference moves against the series.
    (_stamped(A16), _stamped([200.0 - c for c in A16]), 4),
    # A base close of 0 (bar 4) drops the return pair of bar 5: the window and the value of
    # bar 4 stay.
    (_stamped([10.0, 11.0, 12.0, 11.0, 0.0, 5.0, 6.0]),
     _stamped([20.0, 21.0, 20.5, 22.0, 21.0, 23.0, 22.5]), 3),
]

PS_CASES = [
    # By hand, len = 3: series 1, 3, 2 against reference 1, 2, 3. Both means are 2, the
    # deviations are (-1, 1, 0) and (-1, 0, 1): hedge ratio = 1 / 2 = 0.5. Spreads 0.5, 2, 0.5
    # have mean 1 and variance (0.25 + 1 + 0.25) / 3 = 0.5, so z = (0.5 - 1) / sqrt(0.5).
    (_stamped([1.0, 3.0, 2.0]), _stamped([1.0, 2.0, 3.0]), 3),
    # The spread does not vary: hedge ratio 2, spreads 1, 1, 1, z = 0.
    (_stamped([3.0, 5.0, 7.0]), _stamped([1.0, 2.0, 3.0]), 3),
    (_stamped(A16), _stamped(B16), 5),
    (_stamped(A16), _stamped(B16), 16),
    # Pairing: the window holds paired bars only.
    (_stamped(A16), B_GAPS, 5),
    # A reference that does not move has no hedge ratio: no output.
    (_stamped(A16[:8]), _stamped([75.0] * 8), 3),
    # A series that does not move: hedge ratio 0, spreads all equal, z = 0.
    (_stamped([40.0] * 8), _stamped(B16[:8]), 4),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: two-series regression indicators",
    "",
    *provenance("pair_regression"),
    "",
    "Pairing as in golden_pair_indicators: the bars of the first series in their order, each with",
    "the reference bar of the same timestamp; a bar without one is skipped, a reference bar",
    "without one is ignored, a timestamp the reference holds twice takes its later sample. Windows",
    "count paired bars.",
    "",
    "rolling_beta (len = n): per paired bar the return pair ((a_i - a_(i-1)) / a_(i-1), same for",
    "b), left out when a base close is 0. With n pairs collected, over the last n pairs with da, db",
    "the deviations of the series' and the reference's returns from their means: beta = sum(da db)",
    "/ sum(db^2), alpha = mean(a) - beta mean(b), correlation = signed sqrt(cov^2 / (sum(da^2)",
    "sum(db^2))), r_squared = correlation^2; no output when sum(db^2) is 0.",
    "",
    "pair_spread (len = n): over the last n paired closes a_i, b_i, gamma = sum(da db) / sum(db^2),",
    "s_i = a_i - gamma b_i, z = (s_last - mean(s)) / sd(s) with the population deviation, 0 when",
    "the spreads do not vary; no output when sum(db^2) is 0.",
    "",
    "The reference works in exact rational arithmetic on the doubles of the inputs",
    "(fractions.Fraction) and rounds each result once. Cases rb0 and ps0 are checked against the",
    "values worked out by hand (see the module's case comments).",
    "",
    "<kind><i>_a_ts<j>/_a_close<j> and _b_ts<j>/_b_close<j> are the inputs; per bar j of the first",
    "series <kind><i>_out<j> is 1 when there is an output, then rb<i>_beta<j>, _alpha<j>,",
    "_corr<j>, _r2<j> or ps<i>_z<j>, _hedge<j>, _spread<j>, _mean<j>, _std<j>.",
    "",
    "pair_regression_tolerance is the comparison tolerance the tests apply, stated rather than",
    "derived.",
]


def _sqrt(x):
    """Square root of a non-negative Fraction, rounded once to a double."""
    return math.sqrt(float(x))


def _beta(pairs):
    n = len(pairs)
    mean_a = sum(a for a, _ in pairs) / n
    mean_b = sum(b for _, b in pairs) / n
    cov = sum((a - mean_a) * (b - mean_b) for a, b in pairs)
    var_a = sum((a - mean_a) ** 2 for a, _ in pairs)
    var_b = sum((b - mean_b) ** 2 for _, b in pairs)
    if var_b == 0:
        return None
    beta = cov / var_b
    alpha = mean_a - beta * mean_b
    r2 = Fraction(0) if var_a == 0 else cov * cov / (var_a * var_b)
    corr = math.copysign(_sqrt(r2), float(cov)) if cov != 0 else 0.0
    return {"beta": float(beta), "alpha": float(alpha), "corr": corr, "r2": float(r2)}


def _rolling_beta(series, reference, n):
    previous, returns, out = None, [], []
    for pair in _paired(series, reference):
        if pair is None:
            out.append(None)
            continue
        if previous is not None and previous[0] != 0 and previous[1] != 0:
            returns.append(((pair[0] - previous[0]) / previous[0],
                            (pair[1] - previous[1]) / previous[1]))
        previous = pair
        out.append(_beta(returns[-n:]) if len(returns) >= n else None)
    return out


def _spread(pairs):
    n = len(pairs)
    mean_a = sum(a for a, _ in pairs) / n
    mean_b = sum(b for _, b in pairs) / n
    cov = sum((a - mean_a) * (b - mean_b) for a, b in pairs)
    var_b = sum((b - mean_b) ** 2 for _, b in pairs)
    if var_b == 0:
        return None
    hedge = cov / var_b
    spreads = [a - hedge * b for a, b in pairs]
    mean = sum(spreads) / n
    var = sum((s - mean) ** 2 for s in spreads) / n
    z = Fraction(0) if var == 0 else (spreads[-1] - mean) / Fraction(_sqrt(var))
    return {"z": float(z), "hedge": float(hedge), "spread": float(spreads[-1]),
            "mean": float(mean), "std": _sqrt(var)}


def _pair_spread(series, reference, n):
    seen, out = [], []
    for pair in _paired(series, reference):
        if pair is None:
            out.append(None)
            continue
        seen.append(pair)
        out.append(_spread(seen[-n:]) if len(seen) >= n else None)
    return out


def build():
    blocks = [(None, "case counts", {"meta_rb_cases": float(len(RB_CASES)),
                                     "meta_ps_cases": float(len(PS_CASES))})]
    for i, (series, reference, n) in enumerate(RB_CASES):
        outputs = _rolling_beta(series, reference, n)
        if i == 0:
            assert outputs[:3] == [None] * 3
            last = outputs[3]
            assert abs(last["beta"] - 2.0) < 1e-9 and abs(last["alpha"]) < 1e-9
            assert abs(last["r2"] - 1.0) < 1e-9 and abs(last["corr"] - 1.0) < 1e-9
        if i == 4:
            assert outputs == [None] * 8
        if i == 6:
            assert outputs[3] is not None and outputs[5] == outputs[4]
        blocks.append(_block("rb", i, series, reference, n, outputs))
    for i, (series, reference, n) in enumerate(PS_CASES):
        outputs = _pair_spread(series, reference, n)
        if i == 0:
            assert outputs[:2] == [None] * 2
            assert abs(outputs[2]["hedge"] - 0.5) < 1e-12
            assert abs(outputs[2]["z"] + math.sqrt(0.5)) < 1e-12
            assert abs(outputs[2]["std"] - math.sqrt(0.5)) < 1e-12
        if i == 1:
            assert outputs[2]["z"] == 0.0 and outputs[2]["hedge"] == 2.0
        if i == 5:
            assert outputs == [None] * 8
        if i == 6:
            assert all(o is None or (o["z"] == 0.0 and o["hedge"] == 0.0) for o in outputs)
        blocks.append(_block("ps", i, series, reference, n, outputs))
    blocks.append((None, None, {"pair_regression_tolerance": 1e-9}))
    return HEADER, blocks
