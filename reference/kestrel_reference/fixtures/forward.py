"""golden_forward: forward-path statistics (conditional phase end, first passage, bands)."""

import math
from statistics import NormalDist, quantiles

from ..fixture import provenance

NAME = "golden_forward"

Z95 = NormalDist().inv_cdf(0.975)

# (durations as (bars, ended), age, horizon)
KM_CASES = [
    ([(3, True), (5, True), (5, True), (8, False), (9, True), (12, True), (12, False), (20, True)], 4, 6),
    ([(3, True), (5, True), (5, True), (8, False), (9, True), (12, True), (12, False), (20, True)], 0, 30),
    ([(10, True), (10, True), (10, True)], 9, 1),
    ([(4, False), (6, True), (7, False), (7, True), (11, True), (15, False)], 5, 8),
]

# (bars as (high, low, close), start, upper, lower, horizon, tie: 0 = lower first, 1 = upper first)
PASSAGE_CASES = [
    ([(100, 100, 100), (101, 99, 100), (103, 98.5, 102), (106, 101, 105), (107, 104, 106)], 0, 105.5, 97.0, 4, 0),
    ([(100, 100, 100), (101, 99, 100), (100.5, 96.0, 97), (106, 95, 99)], 0, 105.0, 96.5, 3, 0),
    ([(100, 100, 100), (106, 94, 100)], 0, 105.0, 95.0, 1, 0),
    ([(100, 100, 100), (106, 94, 100)], 0, 105.0, 95.0, 1, 1),
    ([(100, 100, 100), (101, 99, 100), (102, 98, 101), (101.5, 99.5, 100.5)], 0, 110.0, 90.0, 3, 0),
    ([(50, 49, 49.5), (51, 48, 50), (52, 49, 51.5), (53, 50.5, 52)], 1, 52.5, 47.0, 2, 0),
]

# (samples, coverage)
BAND_CASES = [
    ([-3.0, -1.5, -0.2, 0.4, 0.9, 1.1, 1.8, 2.5, 3.9, 5.0, -0.7], 0.8),
    ([2.0, 1.0, 4.0, 3.0], 0.5),
    ([0.3, -1.2, 2.2, 0.1, -0.4, 1.7, -2.8, 0.9, 0.0, 1.1, -0.6, 3.3, -1.9, 0.5, 2.6, -0.1, 1.4, -0.9, 0.7, 2.0], 0.9),
]

# (bars as (high, low, close), start, horizon)
MOVE_CASES = [
    ([(100, 100, 100), (101, 99, 100.5), (103, 98.5, 102), (102.5, 97, 98)], 0, 3),
    ([(10, 9, 9.5), (9.8, 8.7, 9.0), (9.6, 8.9, 9.4)], 0, 2),
]

# outcome pairs for coverage: ((lower, upper), outcome)
COVERAGE_CASE = [((-1.0, 1.0), -1.0), ((-1.0, 1.0), 1.0), ((-1.0, 1.0), 1.5), ((0.0, 2.0), 0.5), ((0.0, 2.0), -0.01)]

HEADER = [
    "kestrel-chartkit golden reference fixture: forward-path statistics",
    "",
    *provenance("forward"),
    "",
    "Conditional phase end (Kaplan-Meier): derived here bar by bar - for every length t in",
    "(age, age + horizon] the hazard is (#phases that ended exactly at t) / (#phases of length >= t),",
    "the conditional survival the product of (1 - hazard), the probability its complement. The crate",
    "multiplies only over the distinct lengths with an event; at the other lengths the factor is 1,",
    "so both routes give the same number. Greenwood: survival * sqrt(sum d/(n(n-d))) over the same",
    "steps, terms with n = d left out. at_risk = #phases longer than age.",
    "km<i>_n durations km<i>_d<j>_bars/_ended (1 = ended), _age, _horizon; expected _probability,",
    "_at_risk, _std_error.",
    "",
    "First passage: bars after the start are scanned in order; a bar whose high reaches the upper",
    "level or whose low reaches the lower one ends the path; both on one bar follow the tie rule",
    "(0 = lower first, 1 = upper first). kind 0 = neither, 1 = upper, 2 = lower; bars = 1-based",
    "position after the start.",
    "",
    "Empirical band: statistics.quantiles(samples, n=K, method='inclusive') with K chosen so the",
    "cut points include (1 - coverage)/2 and (1 + coverage)/2 - the linear interpolation between",
    "order statistics (type 7) the crate uses, computed through the standard library's routine.",
    "",
    "Forward move: change of close to the horizon, highest high minus start close, start close minus",
    "lowest low, over the bars after the start up to the horizon.",
    "",
    "Coverage: share of outcomes within their band, bounds included.",
]


def _km(durations, age, horizon):
    at_risk = sum(1 for bars, _ in durations if bars > age)
    survival = 1.0
    greenwood = 0.0
    for t in range(age + 1, age + horizon + 1):
        n = sum(1 for bars, _ in durations if bars >= t)
        d = sum(1 for bars, ended in durations if ended and bars == t)
        if n == 0 or d == 0:
            continue
        survival *= 1.0 - d / n
        if n > d:
            greenwood += d / (n * (n - d))
    return 1.0 - survival, at_risk, survival * math.sqrt(greenwood)


def _passage(bars, start, upper, lower, horizon, tie):
    for k in range(1, horizon + 1):
        high, low, _ = bars[start + k]
        up = high >= upper
        down = low <= lower
        if up and down:
            return (1 if tie == 1 else 2), k
        if up:
            return 1, k
        if down:
            return 2, k
    return 0, 0


def _band(samples, coverage):
    lo_p = (1.0 - coverage) / 2.0
    hi_p = (1.0 + coverage) / 2.0
    # Smallest K whose cut points k/K hit lo_p exactly.
    for k_total in range(2, 1001):
        k = round(lo_p * k_total)
        if abs(k / k_total - lo_p) < 1e-12 and abs((k_total - k) / k_total - hi_p) < 1e-12:
            cuts = quantiles(samples, n=k_total, method="inclusive")
            return cuts[k - 1], cuts[k_total - k - 1]
    raise ValueError("no cut-point grid for this coverage")


def _move(bars, start, horizon):
    base = bars[start][2]
    window = bars[start + 1:start + horizon + 1]
    return (window[-1][2] - base,
            max(h for h, _, _ in window) - base,
            base - min(lo for _, lo, _ in window))


def build():
    blocks = [(None, "case counts", {
        "meta_km_case_count": float(len(KM_CASES)),
        "meta_passage_case_count": float(len(PASSAGE_CASES)),
        "meta_band_case_count": float(len(BAND_CASES)),
        "meta_move_case_count": float(len(MOVE_CASES)),
    })]
    for i, (durations, age, horizon) in enumerate(KM_CASES):
        p, at_risk, se = _km(durations, age, horizon)
        case = {"n": float(len(durations))}
        for j, (bars, ended) in enumerate(durations):
            case[f"d{j}_bars"] = float(bars)
            case[f"d{j}_ended"] = 1.0 if ended else 0.0
        case.update({"age": float(age), "horizon": float(horizon), "probability": p,
                     "at_risk": float(at_risk), "std_error": se})
        blocks.append((f"km{i}", f"age {age}, horizon {horizon}", case))
    for i, (bars, start, upper, lower, horizon, tie) in enumerate(PASSAGE_CASES):
        kind, after = _passage(bars, start, upper, lower, horizon, tie)
        case = {"len": float(len(bars))}
        for j, (high, low, close) in enumerate(bars):
            case[f"b{j}_high"] = float(high)
            case[f"b{j}_low"] = float(low)
            case[f"b{j}_close"] = float(close)
        case.update({"start": float(start), "upper": float(upper), "lower": float(lower),
                     "horizon": float(horizon), "tie": float(tie), "kind": float(kind),
                     "bars": float(after)})
        blocks.append((f"fp{i}", None, case))
    for i, (samples, coverage) in enumerate(BAND_CASES):
        lower, upper = _band(samples, coverage)
        case = {"len": float(len(samples))}
        case.update({f"v{j}": v for j, v in enumerate(samples)})
        case.update({"coverage": coverage, "lower": lower, "upper": upper})
        blocks.append((f"band{i}", None, case))
    for i, (bars, start, horizon) in enumerate(MOVE_CASES):
        change, up, down = _move(bars, start, horizon)
        case = {"len": float(len(bars))}
        for j, (high, low, close) in enumerate(bars):
            case[f"b{j}_high"] = float(high)
            case[f"b{j}_low"] = float(low)
            case[f"b{j}_close"] = float(close)
        case.update({"start": float(start), "horizon": float(horizon),
                     "close_change": change, "max_up": up, "max_down": down})
        blocks.append((f"move{i}", None, case))
    cov = {"len": float(len(COVERAGE_CASE))}
    inside = 0
    for j, ((lo, hi), outcome) in enumerate(COVERAGE_CASE):
        cov[f"p{j}_lower"] = lo
        cov[f"p{j}_upper"] = hi
        cov[f"p{j}_outcome"] = outcome
        inside += 1 if lo <= outcome <= hi else 0
    cov["coverage"] = inside / len(COVERAGE_CASE)
    blocks.append(("cov", None, cov))
    blocks.append((None, None, {"forward_tolerance": 1e-12}))
    return HEADER, blocks
