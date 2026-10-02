"""golden_deflated_sharpe: standard normal quantile and the deflated Sharpe ratio."""

import math
from statistics import NormalDist

from ..fixture import provenance

NAME = "golden_deflated_sharpe"

N = NormalDist()
EULER_GAMMA = 0.5772156649015329

QUANTILE_PS = [1e-9, 0.001, 0.02425, 0.05, 0.3, 0.5, 0.7, 0.975, 0.99, 0.999999]

# (returns, trials, variance of the trials' Sharpe ratios)
DSR_CASES = [
    ([0.5, -0.2, 0.8, 0.1, -0.4, 0.9, 0.3, -0.1, 0.6, 0.2, -0.3, 0.7], 1, 0.0),
    ([0.5, -0.2, 0.8, 0.1, -0.4, 0.9, 0.3, -0.1, 0.6, 0.2, -0.3, 0.7], 168, 0.04),
    ([1.0, -1.0, 1.0, 1.0, -1.0, 1.0, -1.0, 1.0, 1.0, 1.0, -1.0, 1.0, 1.0, -1.0, 1.0, 1.0], 50, 0.02),
    ([2.2, -1.0, -1.0, 2.5, -1.0, -0.9, 3.1, -1.0, -1.0, -1.0, 2.0, -1.0, 0.4, -1.0, 2.8], 20, 0.01),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: normal quantile and deflated Sharpe ratio",
    "",
    *provenance("deflated_sharpe"),
    "",
    "Quantile: statistics.NormalDist().inv_cdf(p) - the standard library's routine, not the",
    "crate's Acklam approximation with Newton refinement. q<i>_p / q<i>_value.",
    "",
    "Deflated Sharpe ratio (Bailey & Lopez de Prado 2014) from the paper's formula: SR = mean /",
    "sample standard deviation (n - 1), skewness m3/m2^1.5 and kurtosis m4/m2^2 from population",
    "moments, SR0 = sqrt(V) ((1 - g) Z^-1(1 - 1/N) + g Z^-1(1 - 1/(N e))) with g the",
    "Euler-Mascheroni constant (SR0 = 0 for N = 1), DSR = Z((SR - SR0) sqrt(T - 1) / sqrt(1 -",
    "skew SR + (kurt - 1)/4 SR^2)), Z and Z^-1 from statistics.NormalDist.",
    "dsr<i>_len, _v<j>, _trials, _variance; expected _sharpe, _sr0, _probability.",
]


def _dsr(returns, trials, variance):
    n = len(returns)
    mean = sum(returns) / n
    dev = [r - mean for r in returns]
    m2 = sum(d * d for d in dev) / n
    m3 = sum(d ** 3 for d in dev) / n
    m4 = sum(d ** 4 for d in dev) / n
    skew = m3 / m2 ** 1.5
    kurt = m4 / (m2 * m2)
    sd = math.sqrt(sum(d * d for d in dev) / (n - 1))
    sr = mean / sd
    if trials <= 1:
        sr0 = 0.0
    else:
        sr0 = math.sqrt(variance) * ((1 - EULER_GAMMA) * N.inv_cdf(1 - 1 / trials)
                                     + EULER_GAMMA * N.inv_cdf(1 - 1 / (trials * math.e)))
    z = (sr - sr0) * math.sqrt(n - 1) / math.sqrt(1 - skew * sr + (kurt - 1) / 4 * sr * sr)
    return sr, sr0, N.cdf(z)


def build():
    blocks = [(None, "case counts", {"meta_quantile_case_count": float(len(QUANTILE_PS)),
                                      "meta_dsr_case_count": float(len(DSR_CASES))})]
    for i, p in enumerate(QUANTILE_PS):
        blocks.append((f"q{i}", None, {"p": p, "value": N.inv_cdf(p)}))
    for i, (returns, trials, variance) in enumerate(DSR_CASES):
        sr, sr0, prob = _dsr(returns, trials, variance)
        case = {"len": float(len(returns))}
        case.update({f"v{j}": v for j, v in enumerate(returns)})
        case.update({"trials": float(trials), "variance": variance, "sharpe": sr, "sr0": sr0,
                     "probability": prob})
        blocks.append((f"dsr{i}", f"{trials} trials", case))
    blocks.append((None, None, {"quantile_tolerance": 1e-9, "dsr_tolerance": 1e-9}))
    return HEADER, blocks
