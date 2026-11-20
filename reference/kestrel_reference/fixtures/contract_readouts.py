"""contract_readouts: complete per-bar contracts of the read-out indicators (plan 141).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the definition documented
beside the indicator, written as a second implementation in `kestrel_reference/readouts.py` (and
the golden modules it reuses), not from the Rust code. Bars are (open, high, low, close, volume);
indices in comments are 1-based like the fixture. Contract bars are spaced 900 seconds apart,
the first at 900.

Conventions the documentation leaves open, assumed here:

- None of these read-outs raises alerts.
"""

from ..fixture import provenance, render_contract
from .. import readouts as ro
from .contract_pilot import case_block
from .contract_volatility import SWING, SWING_SHORT, flat_bars, path_bars

NAME = "contract_readouts"
RENDER = render_contract

TOLERANCE = 1e-9


def values_case(values):
    """Expected bars of an indicator that publishes one value per bar (None: no output)."""
    return [(None if v is None else (v, {}), {}) for v in values]


def fractal_case(n, method, bars):
    return ({"period": float(n), "method": 0.0 if method == "halves" else 1.0}, bars,
            values_case(ro.fractal_dimension(bars, n, method)))


# A straight climb (D near 1 under `halves`, exactly the path = net move under `path`) and zig-zag
# noise (D towards 2).
CLIMB = path_bars([100.0 + i for i in range(40)], 0.5)
ZIGZAG = path_bars([100.0 + (3.0 if i % 2 else 0.0) for i in range(40)], 0.5)


def build():
    header = provenance("contract_readouts") + [
        "",
        "Format: see contract_pilot (one table per case, `-` = absent).",
    ]
    blocks = [
        case_block("fractal_dimension_defaults", "fractal_dimension, defaults (halves, period 30) "
                   "over 90 swing bars", *fractal_case(30, "halves", SWING)),
        case_block("fractal_dimension_odd", "fractal_dimension, halves, period 9: an odd window, "
                   "two halves of 4 inside it", *fractal_case(9, "halves", SWING_SHORT)),
        case_block("fractal_dimension_min", "fractal_dimension, halves, period 4: the smallest "
                   "window, two halves of 2", *fractal_case(4, "halves", SWING_SHORT)),
        case_block("fractal_dimension_path_long", "fractal_dimension, path, period 30 over 90 swing "
                   "bars: first value on bar 31", *fractal_case(30, "path", SWING)),
        case_block("fractal_dimension_path_short", "fractal_dimension, path, period 4",
                   *fractal_case(4, "path", SWING_SHORT)),
        case_block("fractal_dimension_climb", "fractal_dimension, halves, period 8 over a straight "
                   "climb: near 1", *fractal_case(8, "halves", CLIMB)),
        case_block("fractal_dimension_zigzag", "fractal_dimension, halves, period 8 over zig-zag "
                   "noise: clamped to 2 or near it", *fractal_case(8, "halves", ZIGZAG)),
        case_block("fractal_dimension_flat", "fractal_dimension, period 6 over bars without any "
                   "range: 1.5 for both methods' windows",
                   *fractal_case(6, "halves", flat_bars(10))),
        case_block("fractal_dimension_path_flat", "fractal_dimension, path, period 6 over bars "
                   "without any range: 1.5", *fractal_case(6, "path", flat_bars(10))),
    ]
    return header, blocks
