"""golden_logistic: L2-regularised logistic regression."""

import math

from ..fixture import provenance

NAME = "golden_logistic"

# (rows, labels, l2)
CASES = [
    ([[0.5, 1.2], [-1.0, 0.3], [2.0, -0.7], [0.1, 0.1], [-0.8, -1.5], [1.5, 0.9], [-1.7, 0.4],
      [0.9, -0.2], [-0.3, 1.8], [1.1, 1.1]],
     [1, 0, 1, 1, 0, 1, 0, 0, 0, 1], 0.0),
    ([[0.5, 1.2], [-1.0, 0.3], [2.0, -0.7], [0.1, 0.1], [-0.8, -1.5], [1.5, 0.9], [-1.7, 0.4],
      [0.9, -0.2], [-0.3, 1.8], [1.1, 1.1]],
     [1, 0, 1, 0, 0, 1, 0, 1, 0, 1], 0.5),
    ([[float(i) - 10.0] for i in range(20)], [1 if i >= 10 else 0 for i in range(20)], 1.0),
    ([[0.2, -0.4, 1.0], [1.3, 0.7, -0.6], [-0.9, 1.1, 0.3], [0.4, -1.2, -0.8], [1.8, 0.2, 0.5],
      [-1.4, -0.6, 1.2], [0.7, 0.9, -1.1], [-0.2, -0.3, 0.6], [1.0, -0.8, 0.0], [-0.6, 0.5, -0.4],
      [0.3, 1.6, 0.9], [-1.1, -1.4, -0.2]],
     [1, 1, 0, 0, 1, 0, 1, 0, 1, 0, 1, 0], 0.1),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: L2-regularised logistic regression",
    "",
    *provenance("logistic"),
    "",
    "Maximises sum[y log p + (1 - y) log(1 - p)] - l2/2 |w|^2 (intercept not penalised) with",
    "Newton steps written here from that objective: gradient X^T (y - p) - l2 w, Hessian",
    "X^T diag(p(1-p)) X + l2 I (zero for the intercept), solved by Gauss-Jordan elimination;",
    "iterated until the largest step is below 1e-12. lr<i>_n rows lr<i>_x<r>_<c>, _y<r>, _l2;",
    "expected _intercept, _w<c> and _p<r> (fitted probability per row).",
]


def _solve(a, b):
    n = len(b)
    m = [row[:] + [b[i]] for i, row in enumerate(a)]
    for col in range(n):
        pivot = max(range(col, n), key=lambda r: abs(m[r][col]))
        m[col], m[pivot] = m[pivot], m[col]
        div = m[col][col]
        m[col] = [v / div for v in m[col]]
        for r in range(n):
            if r != col:
                f = m[r][col]
                m[r] = [v - f * w for v, w in zip(m[r], m[col])]
    return [m[i][n] for i in range(n)]


def _fit(rows, labels, l2):
    k = len(rows[0])
    beta = [0.0] * (k + 1)
    for _ in range(200):
        grad = [0.0] * (k + 1)
        hess = [[0.0] * (k + 1) for _ in range(k + 1)]
        for row, y in zip(rows, labels):
            f = [1.0] + list(row)
            z = sum(b * v for b, v in zip(beta, f))
            p = 1.0 / (1.0 + math.exp(-z))
            for a in range(k + 1):
                grad[a] += (y - p) * f[a]
                for c in range(k + 1):
                    hess[a][c] += p * (1 - p) * f[a] * f[c]
        for j in range(1, k + 1):
            grad[j] -= l2 * beta[j]
            hess[j][j] += l2
        step = _solve(hess, grad)
        beta = [b + s for b, s in zip(beta, step)]
        if max(abs(s) for s in step) < 1e-12:
            break
    return beta


def build():
    blocks = [(None, "case counts", {"meta_case_count": float(len(CASES))})]
    for i, (rows, labels, l2) in enumerate(CASES):
        beta = _fit(rows, labels, l2)
        case = {"n": float(len(rows)), "k": float(len(rows[0])), "l2": l2}
        for r, row in enumerate(rows):
            for c, v in enumerate(row):
                case[f"x{r}_{c}"] = float(v)
            case[f"y{r}"] = float(labels[r])
        case["intercept"] = beta[0]
        for c, w in enumerate(beta[1:]):
            case[f"w{c}"] = w
        for r, row in enumerate(rows):
            z = beta[0] + sum(w * v for w, v in zip(beta[1:], row))
            case[f"p{r}"] = 1.0 / (1.0 + math.exp(-z))
        blocks.append((f"lr{i}", f"l2 = {l2}", case))
    blocks.append((None, None, {"logistic_tolerance": 1e-8}))
    return HEADER, blocks
