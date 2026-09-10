"""One module per generated fixture. Each exposes `NAME` (the file stem under tests/fixtures/) and
`build()`, which returns the header lines and the case blocks."""

from . import bond_diff, business_days, curve_diff, option_diff, revaluation_diff, surface_diff

ALL = (option_diff, bond_diff, curve_diff, business_days, surface_diff, revaluation_diff)
