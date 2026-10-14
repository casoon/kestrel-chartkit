"""golden_data_snooping: White's Reality Check and Hansen's SPA with the stationary bootstrap.

Written from the module documentation of `data_snooping`, not from the Rust code. SplitMix64 is
implemented with Python integers masked to 64 bits; the bootstrap index sequence is generated in
full per resample and every series is averaged over it.
"""

import math

from ..fixture import provenance

NAME = "golden_data_snooping"

MASK = (1 << 64) - 1

HEADER = [
    "kestrel-chartkit golden reference fixture: data snooping (Reality Check, SPA)",
    "",
    *provenance("data_snooping"),
    "",
    "Random numbers: SplitMix64 (state += 0x9E3779B97F4A7C15; z = (z ^ z>>30) * 0xBF58476D1CE4E5B9;",
    "z = (z ^ z>>27) * 0x94D049BB133111EB; z ^ z>>31), u = (z >> 11) * 2^-53, index = floor(u * T).",
    "Stationary bootstrap: first index uniform, then with probability 1/L a new uniform index,",
    "otherwise the next one modulo T. Reality Check p = #{max_k sqrt(T)(m*_k - m_k) >= max_k sqrt(T) m_k}/B.",
    "SPA: omega_k^2 = mean_b (sqrt(T)(m*_k - m_k))^2; T = max(0, max_k sqrt(T) m_k/omega_k); mu_k = m_k",
    "if sqrt(T) m_k/omega_k >= -sqrt(2 ln ln T) else 0; p = #{max(0, max_k sqrt(T)(m*_k - mu_k)/omega_k) >= T}/B.",
    "Series: <case>_k<j>_t<i> (excess of rule j in period i); deterministic sin/cos patterns.",
]


class SplitMix:
    def __init__(self, seed):
        self.state = seed & MASK

    def next(self):
        self.state = (self.state + 0x9E3779B97F4A7C15) & MASK
        z = self.state
        z = ((z ^ (z >> 30)) * 0xBF58476D1CE4E5B9) & MASK
        z = ((z ^ (z >> 27)) * 0x94D049BB133111EB) & MASK
        return z ^ (z >> 31)

    def uniform(self):
        return (self.next() >> 11) * (1.0 / float(1 << 53))

    def index(self, n):
        return min(int(self.uniform() * n), n - 1)


def snooping(series, mean_block, resamples, seed):
    k = len(series)
    t = len(series[0])
    tf = float(t)
    root = math.sqrt(tf)
    means = [sum(d) / tf for d in series]
    best = 0
    for j in range(k):
        if means[j] > means[best]:
            best = j
    rng = SplitMix(seed)
    p_new = 1.0 / mean_block
    boot = [[] for _ in range(k)]
    for _ in range(resamples):
        idx = [rng.index(t)]
        for _s in range(1, t):
            if rng.uniform() < p_new:
                idx.append(rng.index(t))
            else:
                idx.append((idx[-1] + 1) % t)
        for j in range(k):
            boot[j].append(sum(series[j][i] for i in idx) / tf)
    v = max(root * m for m in means)
    rc = 0
    for b in range(resamples):
        vb = max(root * (boot[j][b] - means[j]) for j in range(k))
        if vb >= v:
            rc += 1
    omega = []
    for j in range(k):
        q = 0.0
        for x in boot[j]:
            z = root * (x - means[j])
            q += z * z
        omega.append(math.sqrt(q / resamples))
    thr = math.sqrt(2.0 * math.log(math.log(tf))) if t >= 3 else 0.0
    active = [j for j in range(k) if omega[j] > 0.0]
    t_spa = max([0.0] + [root * means[j] / omega[j] for j in active])
    mu = [means[j] if (omega[j] > 0.0 and root * means[j] / omega[j] >= -thr) else 0.0 for j in range(k)]
    spa = 0
    for b in range(resamples):
        tb = max([0.0] + [root * (boot[j][b] - mu[j]) / omega[j] for j in active])
        if tb >= t_spa:
            spa += 1
    return best, means[best], rc / resamples, spa / resamples


def _case(k, t, edges):
    out = []
    for j in range(k):
        row = []
        for i in range(t):
            x = float(i)
            row.append(edges[j] + 0.9 * math.sin(x * (0.7 + 0.13 * j)) + 0.4 * math.cos(x / (1.3 + j)))
        out.append(row)
    return out


CASES = [
    ("clear", 6, 120, [0.35, 0.0, -0.05, 0.02, -0.1, 0.0], 12.0, 400, 7),
    ("noise", 8, 150, [0.0, 0.01, -0.01, 0.02, 0.0, -0.02, 0.01, 0.0], 8.0, 400, 11),
    ("bad_rules", 5, 100, [0.12, -0.8, -0.9, -1.0, -0.7], 5.0, 300, 3),
]


def build():
    blocks = [(None, "case count", {"meta_case_count": float(len(CASES))})]
    for name, k, t, edges, block, resamples, seed in CASES:
        series = _case(k, t, edges)
        best, best_mean, p_rc, p_spa = snooping(series, block, resamples, seed)
        case = {"k": float(k), "t": float(t), "mean_block": block, "resamples": float(resamples),
                "seed": float(seed)}
        for j in range(k):
            for i in range(t):
                case[f"k{j}_t{i}"] = series[j][i]
        case.update({"best": float(best), "best_mean": best_mean, "p_rc": p_rc, "p_spa": p_spa})
        blocks.append((name, f"case {name}", case))
    blocks.append((None, None, {"data_snooping_tolerance": 1e-12}))
    return HEADER, blocks
