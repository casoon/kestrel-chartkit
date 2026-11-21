"""golden_pair_indicators: the two-series indicators `relative_strength` and `correlation`, and the
pairing of two bar series by timestamp, from the definitions documented on
`indicator::pair`, `RelativeStrengthEngine` and `CorrelationEngine`."""

import math
import statistics
from fractions import Fraction

from ..fixture import provenance

NAME = "golden_pair_indicators"

A16 = [100.0, 101.5, 100.8, 102.3, 103.9, 103.1, 104.6, 106.2,
       105.4, 104.9, 106.8, 108.1, 107.3, 109.0, 110.4, 109.6]
B16 = [50.0, 50.4, 50.9, 50.2, 51.1, 51.6, 51.0, 52.3,
       52.9, 52.1, 51.7, 52.8, 53.5, 53.0, 54.2, 54.9]


def _stamped(closes, start=0):
    return [(start + i, c) for i, c in enumerate(closes)]


def _walk(n, seed, start, scale, drift=0.0):
    """`n` closes of a random walk on four decimals (a linear congruential generator)."""
    state, price, out = seed, start, []
    for _ in range(n):
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        price = round(price + drift + ((state >> 11) / 2 ** 53 - 0.5) * scale, 4)
        out.append(price)
    return out


NAN, INF = float("nan"), float("inf")


# The reference series of the pairing cases: timestamps 4 and 9 are missing, 16 and 17 have no
# partner in the first series, and timestamp 6 is held twice (the later sample counts).
B_GAPS = [(t, c) for t, c in _stamped(B16) if t not in (4, 9)]
B_GAPS.insert(B_GAPS.index((6, B16[6])), (6, 60.0))
B_GAPS += [(16, 55.3), (17, 55.0)]

# (first series, reference series, len), each series as [(timestamp, close), ...]
RS_CASES = [
    # By hand, len = 2: A rises 10 %, B falls 12 %, alpha = 10 - (-12) = 22 percentage points,
    # ratio = 110 / 44 = 2.5.
    (_stamped([100.0, 105.0, 110.0]), _stamped([50.0, 50.0, 44.0]), 2),
    (_stamped(A16), _stamped(B16), 3),
    (_stamped(A16), _stamped(B16), 10),
    # Pairing: the first series' bars 4 and 9 have no partner and are skipped.
    (_stamped(A16), B_GAPS, 3),
    # A close of 0, now or as a base, means no output for that bar; the last bar is valid again.
    (_stamped([0.0, 10.0, 12.0, 15.0]), _stamped([5.0, 0.0, 6.0, 9.0]), 1),
    # Shorter than len + 1 paired bars: no output at all.
    (_stamped(A16[:4]), _stamped(B16[:4]), 4),
    # A close that is NaN, infinite or negative, as the current close or as the base: no output on
    # the bars that use it, whichever series it is in; publishing resumes once none is in use.
    (_stamped([10.0, 11.0, NAN, 13.0, 14.0, 15.0, 16.0, 17.0, -1.0, 19.0, 20.0, 21.0, 22.0, 23.0]),
     _stamped([5.0, 5.5, 5.2, 5.1, 5.6, 5.9, INF, 6.1, 6.2, 6.0, 6.3, 6.4, 6.5, 6.6]), 2),
    # The default len (20) over a longer walk; the first series outperforms and then lags behind.
    (_stamped(_walk(40, 20261121, 100.0, 1.5, 0.35)[:24] + _walk(40, 20261122, 108.0, 1.5, -0.6)[:16]),
     _stamped(_walk(40, 20261123, 50.0, 0.8, 0.02)), 20),
    # Moves far beyond the alert levels in both directions, len 3.
    (_stamped([100.0, 102.0, 105.0, 112.0, 118.0, 104.0, 96.0, 90.0, 99.0, 106.0]),
     _stamped([50.0, 50.2, 50.1, 50.0, 49.9, 50.3, 50.2, 50.1, 50.4, 50.0]), 3),
    # Pairing with unpaired bars at the start (the reference starts at timestamp 2), a gap at 5 and
    # a timestamp held three times, the last sample counting, in a reference that is not sorted.
    (_stamped([10.0, 10.5, 11.0, 10.8, 11.5, 12.0, 11.7, 12.4, 13.0, 12.6, 13.5, 14.0]),
     [(3, 20.0), (2, 19.5), (7, 99.0), (4, 20.3), (6, 21.0), (7, 98.0), (8, 21.5), (7, 22.0),
      (9, 22.1), (10, 22.4), (11, 23.0)], 2),
]

CORR_CASES = [
    # By hand, len = 3: the returns are 1 %, 2 %, 3 % and 2 %, 1 %, 3 %; deviations from the
    # means (2 %) are -1, 0, 1 and 0, -1, 1 (in %), so rho = 1 / sqrt(2 * 2) = 0.5.
    (_stamped([100.0, 101.0, 103.02, 106.1106]), _stamped([100.0, 102.0, 103.02, 106.1106]), 3),
    (_stamped(A16), _stamped(B16), 5),
    (_stamped(A16), _stamped(B16), 15),
    # Pairing: returns run over consecutive *paired* bars.
    (_stamped(A16), B_GAPS, 5),
    # The reference is twice the first series: every return pair is equal, rho = 1.
    (_stamped(A16), _stamped([2.0 * c for c in A16]), 4),
    # The reference moves against the first series.
    (_stamped(A16), _stamped([200.0 - c for c in A16]), 4),
    # A reference that does not move has no correlation: no output.
    (_stamped(A16[:8]), _stamped([75.0] * 8), 3),
    # So has one whose returns are all equal (it doubles every bar).
    (_stamped(A16[:8]), _stamped([1.0, 2.0, 4.0, 8.0, 16.0, 32.0, 64.0, 128.0]), 3),
    # A base close of 0 (bar 4) drops the return pair of bar 5: the window and the value of
    # bar 4 stay.
    (_stamped([10.0, 11.0, 12.0, 11.0, 0.0, 5.0, 6.0]),
     _stamped([20.0, 21.0, 20.5, 22.0, 21.0, 23.0, 22.5]), 3),
    # The default len (20) over 40 bars of two walks that share part of their moves.
    (_stamped(_walk(40, 20261121, 100.0, 2.0)),
     _stamped([round(0.5 * a + b, 4) for a, b in zip(_walk(40, 20261121, 50.0, 1.0),
                                                      _walk(40, 20261124, 50.0, 1.0))]), 20),
    # A close that is NaN or infinite, in either series, drops the return pairs it is in (as
    # current and as base); the windows and the values of the bars in between stay.
    (_stamped([10.0, 10.4, 10.2, NAN, 10.9, 11.3, 11.0, 11.8, 12.1, 11.9, 12.6, 12.3]),
     _stamped([20.0, 20.5, 20.2, 20.9, 21.5, INF, 21.9, 22.4, 22.0, 22.9, 23.1, 22.6]), 3),
    # A base close of 0 in either series (own at bar 6, reference at bars 4 and 9): the return
    # pairs of the bars that follow are left out.
    (_stamped([10.0, 10.5, 10.2, 10.8, 11.0, 0.0, 11.5, 11.2, 11.9, 12.4, 12.1]),
     _stamped([20.0, 20.4, 20.9, 0.0, 20.7, 21.3, 21.0, 21.8, 0.0, 21.6, 22.3]), 3),
    # Pairing with unpaired bars at the start, a gap, and a timestamp held three times (the last
    # sample counting) in a reference that is not sorted.
    (_stamped([10.0, 10.5, 11.0, 10.8, 11.5, 12.0, 11.7, 12.4, 13.0, 12.6, 13.5, 14.0]),
     [(3, 20.0), (2, 19.5), (7, 99.0), (4, 20.3), (6, 21.0), (7, 98.0), (8, 21.5), (7, 22.0),
      (9, 22.1), (10, 22.4), (11, 23.0)], 3),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: two-series indicators",
    "",
    *provenance("pair_indicators"),
    "",
    "Pairing: the bars of the first series in their order, each with the reference bar of the same",
    "timestamp; a bar without one is skipped (no output, no effect on any window), a reference bar",
    "without one is ignored, and a timestamp the reference holds twice takes its later sample.",
    "Windows count paired bars.",
    "",
    "relative_strength (len = n), at the paired bar i >= n with closes a, b: own = (a_i - a_(i-n))",
    "/ a_(i-n) * 100; bench likewise from b; alpha = own - bench; ratio = a_i / b_i. No output",
    "when one of a_i, a_(i-n), b_i, b_(i-n) is not finite or not positive. The alert level is 1 where",
    "alpha > 5 (outperforming_benchmark), -1 where alpha < -5 (underperforming_benchmark), else 0.",
    "",
    "correlation (len = n): per paired bar the return pair ((a_i - a_(i-1)) / a_(i-1), same for b),",
    "left out when a base close is 0 or a close of the pair is not finite. With n pairs collected,",
    "rho = sum(da db) / sqrt(sum(da^2) sum(db^2)) over the last n pairs, da and db the deviations",
    "from the window means; no output when the denominator is 0.",
    "",
    "The reference works in exact rational arithmetic on the doubles of the inputs",
    "(fractions.Fraction) and rounds each result once; rho is the signed square root of the exact",
    "rho^2 and is also checked against statistics.correlation. Cases rs0 and corr0 are checked",
    "against the values worked out by hand (see the module's case comments).",
    "",
    "<kind><i>_a_ts<j>/_a_close<j> and _b_ts<j>/_b_close<j> are the inputs; per bar j of the first",
    "series <kind><i>_out<j> is 1 when there is an output, then rs<i>_alpha<j>, _ratio<j>, _own<j>,",
    "_bench<j>, _alert<j> or corr<i>_value<j>.",
    "",
    "pair_indicators_tolerance is the comparison tolerance the tests apply, stated rather than",
    "derived.",
]


def _exact(close):
    """The close as an exact fraction, or None where it is not a finite number."""
    return Fraction(close) if math.isfinite(close) else None


def _paired(series, reference):
    """Per bar of `series`: (close, reference close) as exact fractions (None for a close that is
    not finite), or None without partner."""
    partner = dict(reference)  # a later sample replaces an earlier one
    return [(_exact(c), _exact(partner[t])) if t in partner else None for t, c in series]


def _relative_strength(series, reference, n):
    seen, out = [], []
    for pair in _paired(series, reference):
        if pair is None:
            out.append(None)
            continue
        seen.append(pair)
        if len(seen) <= n:
            out.append(None)
            continue
        (a, b), (a0, b0) = seen[-1], seen[-1 - n]
        if None in (a, a0, b, b0) or min(a, a0, b, b0) <= 0:
            out.append(None)
            continue
        own = (a - a0) / a0 * 100
        bench = (b - b0) / b0 * 100
        ratio = a / b
        alpha = own - bench
        out.append({"alpha": float(alpha), "ratio": float(ratio),
                    "own": float(own), "bench": float(bench),
                    "alert": 1.0 if alpha > 5 else -1.0 if alpha < -5 else 0.0})
    return out


def _pearson(pairs):
    n = len(pairs)
    mean_a = sum(a for a, _ in pairs) / n
    mean_b = sum(b for _, b in pairs) / n
    cov = sum((a - mean_a) * (b - mean_b) for a, b in pairs)
    var = sum((a - mean_a) ** 2 for a, _ in pairs) * sum((b - mean_b) ** 2 for _, b in pairs)
    if var == 0:
        return None
    rho = math.copysign(math.sqrt(float(cov * cov / var)), cov)
    if cov == 0:
        rho = 0.0
    other = statistics.correlation([float(a) for a, _ in pairs], [float(b) for _, b in pairs])
    assert abs(rho - other) < 1e-9, (rho, other)
    return rho


def _correlation(series, reference, n):
    previous, returns, out = None, [], []
    for pair in _paired(series, reference):
        if pair is None:
            out.append(None)
            continue
        if (previous is not None and None not in previous and None not in pair
                and previous[0] != 0 and previous[1] != 0):
            returns.append(((pair[0] - previous[0]) / previous[0],
                            (pair[1] - previous[1]) / previous[1]))
        previous = pair
        if len(returns) < n:
            out.append(None)
            continue
        rho = _pearson(returns[-n:])
        out.append(None if rho is None else {"value": rho})
    return out


def _block(prefix, i, series, reference, n, outputs):
    case = {"len": float(n), "a_n": float(len(series)), "b_n": float(len(reference))}
    for name, samples in (("a", series), ("b", reference)):
        for j, (ts, close) in enumerate(samples):
            case[f"{name}_ts{j}"] = float(ts)
            case[f"{name}_close{j}"] = close
    for j, output in enumerate(outputs):
        case[f"out{j}"] = 0.0 if output is None else 1.0
        for key, value in (output or {}).items():
            case[f"{key}{j}"] = value
    return (f"{prefix}{i}", f"len {n}", case)


def build():
    blocks = [(None, "case counts", {"meta_rs_cases": float(len(RS_CASES)),
                                     "meta_corr_cases": float(len(CORR_CASES))})]
    for i, (series, reference, n) in enumerate(RS_CASES):
        outputs = _relative_strength(series, reference, n)
        if i == 0:
            assert outputs == [None, None,
                               {"alpha": 22.0, "ratio": 2.5, "own": 10.0, "bench": -12.0,
                        "alert": 1.0}]
        if i == 3:
            assert outputs[4] is None and outputs[9] is None
        if i == 4:
            assert outputs[:3] == [None] * 3
            assert outputs[3] == {"alpha": -25.0, "ratio": 15.0 / 9.0, "own": 25.0, "bench": 50.0,
                                "alert": -1.0}
        if i == 5:
            assert outputs == [None] * 4
        if i == 6:
            # NaN (bar 3), infinite (bar 7) and negative (bar 9) closes silence the bar itself and
            # the bar two on, which has it as its base.
            assert [o is not None for o in outputs] == [
                False, False, False, True, False, True, False, True, False, True, False, True,
                True, True]
        if i == 7:
            alerts = {o["alert"] for o in outputs if o}
            assert alerts == {-1.0, 0.0, 1.0}, alerts
        if i == 8:
            assert {o["alert"] for o in outputs if o} == {-1.0, 0.0, 1.0}
        if i == 9:
            assert outputs[:4] == [None] * 4 and outputs[4] is not None and outputs[5] is None
        blocks.append(_block("rs", i, series, reference, n, outputs))
    for i, (series, reference, n) in enumerate(CORR_CASES):
        outputs = _correlation(series, reference, n)
        if i == 0:
            assert outputs[:3] == [None] * 3 and abs(outputs[3]["value"] - 0.5) < 1e-12
        if i == 4:
            assert all(o is None or abs(o["value"] - 1.0) < 1e-12 for o in outputs)
            assert outputs[4] is not None
        if i in (6, 7):
            assert outputs == [None] * 8
        if i == 8:
            assert outputs[3] is not None and outputs[5] == outputs[4]
        blocks.append(_block("corr", i, series, reference, n, outputs))
    blocks.append((None, None, {"pair_indicators_tolerance": 1e-9}))
    return HEADER, blocks
