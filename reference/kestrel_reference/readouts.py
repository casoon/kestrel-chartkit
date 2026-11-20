"""Reference formulas of the read-out indicators (plan 141): each written from the definition
documented beside the Rust implementation, over a whole series with plain list slicing, never
from the Rust code. Bars are (open, high, low, close, volume); every function returns one value
(or None) per bar.
"""

import math


def fractal_dimension(bars, n, method):
    """Fractal dimension of the trailing window, one value per bar.

    `halves` (Ehlers): half = max(n // 2, 2); r1 is the high-low range of the newest `half` bars,
    r2 that of the `half` bars before them, r3 that of the newest `n` bars, each divided by its
    bar count; D = (ln(r1 + r2) - ln(r3)) / ln 2, clamped to 1..2, 1.5 when r1 + r2 or r3 is not
    positive. First value on bar max(n, 2 * half).

    `path`: D = 1 + ln(path / range) / ln n, path the sum of the n absolute close-to-close moves
    and range the high-low range of the same n + 1 bars, unclamped, 1.5 when the range or the
    path is not positive. First value on bar n + 1.
    """
    out = []
    for i in range(len(bars)):
        if method == "halves":
            half = max(n // 2, 2)
            need = max(n, 2 * half)
            if i + 1 < need:
                out.append(None)
                continue
            newest = bars[i + 1 - half:i + 1]
            before = bars[i + 1 - 2 * half:i + 1 - half]
            whole = bars[i + 1 - n:i + 1]
            r1 = (max(b[1] for b in newest) - min(b[2] for b in newest)) / half
            r2 = (max(b[1] for b in before) - min(b[2] for b in before)) / half
            r3 = (max(b[1] for b in whole) - min(b[2] for b in whole)) / n
            if r1 + r2 > 0.0 and r3 > 0.0:
                d = (math.log(r1 + r2) - math.log(r3)) / math.log(2.0)
            else:
                d = 1.5
            out.append(min(max(d, 1.0), 2.0))
        else:
            if i < n:
                out.append(None)
                continue
            window = bars[i - n:i + 1]
            rng = max(b[1] for b in window) - min(b[2] for b in window)
            path = 0.0
            for k in range(1, len(window)):
                path += abs(window[k][3] - window[k - 1][3])
            if rng > 0.0 and path > 0.0:
                out.append(math.log(path / rng) / math.log(n) + 1.0)
            else:
                out.append(1.5)
    return out
