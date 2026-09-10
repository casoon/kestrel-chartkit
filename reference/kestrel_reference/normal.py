"""The standard normal distribution."""

import math

_SQRT2 = math.sqrt(2.0)
_INV_SQRT_2PI = 1.0 / math.sqrt(2.0 * math.pi)


def cdf(x):
    """Phi(x) = erfc(-x / sqrt 2) / 2.

    The complementary error function keeps full relative precision in the lower tail, where
    `(1 + erf(x / sqrt 2)) / 2` would cancel to zero long before Phi(x) underflows.
    """
    return 0.5 * math.erfc(-x / _SQRT2)


def pdf(x):
    return _INV_SQRT_2PI * math.exp(-0.5 * x * x)
