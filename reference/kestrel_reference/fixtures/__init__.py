"""One module per generated fixture. Each exposes `NAME` (the file stem under tests/fixtures/) and
either `build()`, which returns the header lines and the case blocks, or — while only part of a
fixture is derived — `PARTIAL = True`, `derive()` and `CONSTANTS`."""

from . import (
    bond_diff,
    business_days,
    composite,
    curve_diff,
    family_math,
    moving_averages,
    option_diff,
    oscillators,
    revaluation_diff,
    surface_diff,
    transforms,
    trend,
    volatility,
    volume,
)

ALL = (
    option_diff,
    bond_diff,
    curve_diff,
    business_days,
    surface_diff,
    revaluation_diff,
    transforms,
    trend,
    moving_averages,
    oscillators,
    volatility,
    volume,
    family_math,
    composite,
)
