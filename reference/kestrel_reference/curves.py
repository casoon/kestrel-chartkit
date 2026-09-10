"""Zero-rate curves."""

import math


def _segment(axis, x):
    """Index of the segment `[axis[i], axis[i+1]]` used for `x`. Outside the axis it is the edge
    segment, so interpolating with it continues that segment's slope."""
    if x <= axis[0]:
        return 0
    if x >= axis[-1]:
        return len(axis) - 2
    return max(i for i in range(len(axis) - 1) if axis[i] <= x)


class ZeroCurve:
    """Continuously compounded zero rates, linear in time between the nodes.

    Before the first node the first rate is held flat (as if a node at t = 0 carried it). Beyond
    the last node the instantaneous forward rate at that node is held flat:

        z(t) = (z_n t_n + f_n (t - t_n)) / t,   f_n = z_n + t_n (z_n - z_(n-1)) / (t_n - t_(n-1))

    which is the established alternative to holding the zero rate itself flat. `shift` is added to
    every rate. Discount factors are exp(-z(t) t).
    """

    def __init__(self, nodes, shift=0.0):
        self.times = [0.0] + [t for t, _ in nodes]
        self.rates = [nodes[0][1] + shift] + [rate + shift for _, rate in nodes]

    def zero(self, t):
        last = len(self.times) - 1
        if t > self.times[last]:
            slope = (self.rates[last] - self.rates[last - 1]) / (
                self.times[last] - self.times[last - 1]
            )
            forward = self.rates[last] + self.times[last] * slope
            return (self.rates[last] * self.times[last] + forward * (t - self.times[last])) / t
        i = _segment(self.times, t)
        weight = (t - self.times[i]) / (self.times[i + 1] - self.times[i])
        return self.rates[i] + weight * (self.rates[i + 1] - self.rates[i])

    def discount(self, t):
        return math.exp(-self.zero(t) * t)
