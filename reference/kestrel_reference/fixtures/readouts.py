"""golden_readouts: the read-out indicators (plan 141) at checkpoints of `series.wave_series`.

Written from the definitions documented beside the Rust implementations (formulas in
`kestrel_reference/readouts.py`), not from the Rust code. `<label>_cp<i>` holds the value on bar
i (zero-based); a bar without a value is written as `<label>_cp<i>_none=1`.
"""

from .. import readouts as ro
from .. import series as ps
from ..fixture import provenance

NAME = "golden_readouts"

N_BARS = 700
# Bars across the series' phases: warm-up, uptrend, drift, crash, tight range, breakout.
CHECKPOINTS = [3, 9, 15, 29, 30, 31, 60, 150, 260, 420, 520, 600, 640, 680, 699]

HEADER = [
    "kestrel-chartkit golden reference fixture: read-out indicators (plan 141)",
    "",
    *provenance("readouts"),
    "",
    "Series: series.wave_series(700), bars (open, high, low, close, volume).",
    "fractal_dimension: `halves` (Ehlers): half = max(n/2, 2); r1, r2, r3 the high-low ranges of",
    "the newest half bars, the half before and the newest n bars, each over its bar count;",
    "D = (ln(r1 + r2) - ln r3) / ln 2 clamped to 1..2, 1.5 without range. `path`:",
    "D = 1 + ln(path / range) / ln n over n close-to-close steps and the range of n + 1 bars.",
]


def specs(bars):
    """(label, series of values) of every checked configuration."""
    return [
        ("fd_halves_default", ro.fractal_dimension(bars, 30, "halves")),
        ("fd_halves_odd", ro.fractal_dimension(bars, 9, "halves")),
        ("fd_halves_min", ro.fractal_dimension(bars, 4, "halves")),
        ("fd_path_default", ro.fractal_dimension(bars, 30, "path")),
        ("fd_path_short", ro.fractal_dimension(bars, 5, "path")),
    ]


def build():
    bars = ps.wave_series(N_BARS)
    blocks = [(None, "checkpoints", {"meta_cp_count": float(len(CHECKPOINTS)),
                                     **{f"meta_cp{k}": float(cp) for k, cp in enumerate(CHECKPOINTS)}})]
    for label, values in specs(bars):
        case = {}
        for cp in CHECKPOINTS:
            if values[cp] is None:
                case[f"cp{cp}_none"] = 1.0
            else:
                case[f"cp{cp}"] = values[cp]
        blocks.append((label, label, case))
    blocks.append((None, None, {"readouts_tolerance": 1e-9}))
    return HEADER, blocks
