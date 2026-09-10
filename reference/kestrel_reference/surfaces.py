"""Volatility grids over maturity and strike."""

import math

from .curves import _segment


def _weight(axis, i, x):
    return (x - axis[i]) / (axis[i + 1] - axis[i])


def bilinear(times, strikes, values, t, strike):
    """Bilinear interpolation of `values[maturity][strike]`: linear in strike on the two
    bracketing maturity rows, then linear in maturity between them. Outside the grid the edge
    segments are used, which continues their slope."""
    i = _segment(times, t)
    j = _segment(strikes, strike)
    u = _weight(times, i, t)
    v = _weight(strikes, j, strike)

    def along_strike(row):
        return values[row][j] + v * (values[row][j + 1] - values[row][j])

    lower = along_strike(i)
    return lower + u * (along_strike(i + 1) - lower)


def total_variance_volatility(times, strikes, vols, t, strike):
    """Volatility under the other established convention: total variance w = vol^2 * t linear in
    maturity (with w = 0 at t = 0) and linear in strike.

    Beyond the last maturity the edge volatility is held (w grows in proportion to t); outside the
    strike range the edge strike is used.
    """
    strike = min(max(strike, strikes[0]), strikes[-1])
    variances = [[vol * vol * maturity for vol in row] for maturity, row in zip(times, vols)]
    if t > times[-1]:
        edge = bilinear(times, strikes, variances, times[-1], strike)
        return math.sqrt(edge / times[-1])
    grid_times = [0.0] + list(times)
    grid = [[0.0] * len(strikes)] + variances
    return math.sqrt(bilinear(grid_times, strikes, grid, t, strike) / t)
