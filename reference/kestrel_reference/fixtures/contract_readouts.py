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


def r2_case(n, bars):
    return {"period": float(n)}, bars, values_case(ro.r_squared([b[3] for b in bars], n))


def rank_case(n, bars):
    return {"period": float(n)}, bars, values_case(ro.percent_rank([b[3] for b in bars], n))


def cycle_case(period, history, bars):
    """Value: mean length of the last `history` completed swings; extra bars_since_crossing."""
    cycles = ro.cycle_length([b[3] for b in bars], period, history)
    expected = [(None, {}) if c is None else ((c[0], {"bars_since_crossing": c[1]}), {})
                for c in cycles]
    return {"period": float(period), "history": float(history)}, bars, expected


def closes_bars(closes):
    """Bars of whole-number closes with a one-point range (exact sums keep ties exact)."""
    return [(float(c), c + 1.0, c - 1.0, float(c), 1000.0) for c in closes]


# Closes in blocks around a flat average: above for 4 bars, below for 3, above for 5, below for 2,
# above for 3 (period 2: the average of the last two closes), with a bar exactly on the average.
SWINGS = closes_bars([10, 12, 14, 16, 18, 14, 10, 6, 4, 8, 12, 16, 20, 24, 14, 6, 10, 20, 30])
# Equal closes: no variation (R^2 1, rank 100, no crossing).
EQUAL = closes_bars([50] * 12)
# Closes with ties against the window (percent rank counts equal closes).
TIES = closes_bars([5, 5, 5, 5, 5, 3, 5, 7, 5, 5, 9, 1, 5, 5])
# A bar exactly on its 3-bar average belongs to the swing in progress: 1, 3, 2 averages 2.
TOUCH = closes_bars([10, 20, 30, 40, 50, 40, 30, 40, 30, 20, 25, 20, 10, 20, 10, 30, 20, 10])

# A straight climb (D near 1 under `halves`, exactly the path = net move under `path`) and zig-zag
# noise (D towards 2).
CLIMB = path_bars([100.0 + i for i in range(40)], 0.5)
ZIGZAG = path_bars([100.0 + (3.0 if i % 2 else 0.0) for i in range(40)], 0.5)


def _guard():
    """The touch case really has bars on the average (the model's own arithmetic, exact on whole
    numbers)."""
    closes = [b[3] for b in TOUCH]
    on_average = [i for i in range(2, len(closes))
                  if closes[i] - sum(closes[i - 2:i + 1]) / 3 == 0.0]
    assert on_average, "TOUCH has no bar on its average"


def build():
    _guard()
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
        case_block("r_squared_defaults", "r_squared, defaults (20) over 90 swing bars",
                   *r2_case(20, SWING)),
        case_block("r_squared_two", "r_squared, period 2: two points always fit exactly, 1",
                   *r2_case(2, SWING_SHORT)),
        case_block("r_squared_line", "r_squared, period 5 over a straight climb: 1",
                   *r2_case(5, CLIMB)),
        case_block("r_squared_noise", "r_squared, period 6 over zig-zag noise: low",
                   *r2_case(6, ZIGZAG)),
        case_block("r_squared_flat", "r_squared, period 4 over equal closes: no variation to "
                   "explain, 1", *r2_case(4, EQUAL)),
        case_block("percent_rank_defaults", "percent_rank, defaults (20) over 90 swing bars",
                   *rank_case(20, SWING)),
        case_block("percent_rank_short", "percent_rank, period 3 over 40 swing bars",
                   *rank_case(3, SWING_SHORT)),
        case_block("percent_rank_one", "percent_rank, period 1: 100 when the close is not below "
                   "the previous one, else 0", *rank_case(1, SWING_SHORT)),
        case_block("percent_rank_ties", "percent_rank, period 4 over closes with ties: an equal "
                   "close counts as at or below", *rank_case(4, TIES)),
        case_block("percent_rank_flat", "percent_rank, period 4 over equal closes: every previous "
                   "close is at or below, 100", *rank_case(4, EQUAL)),
        case_block("cycle_length_defaults", "cycle_length, defaults (20, 8) over 90 swing bars",
                   *cycle_case(20, 8, SWING)),
        case_block("cycle_length_short", "cycle_length, period 3, history 2 over 40 swing bars: "
                   "only the last two swings count", *cycle_case(3, 2, SWING_SHORT)),
        case_block("cycle_length_blocks", "cycle_length, period 2, history 4 over closes in "
                   "blocks above and below their average", *cycle_case(2, 4, SWINGS)),
        case_block("cycle_length_touch", "cycle_length, period 3, history 3: bars exactly on the "
                   "average do not change the side", *cycle_case(3, 3, TOUCH)),
        case_block("cycle_length_flat", "cycle_length, period 3 over equal closes: always on the "
                   "average, no crossing, no output", *cycle_case(3, 3, EQUAL)),
    ]
    return header, blocks
