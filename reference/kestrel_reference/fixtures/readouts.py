"""golden_readouts: the read-out indicators (plan 141) at checkpoints of `series.wave_series`.

Written from the definitions documented beside the Rust implementations (formulas in
`kestrel_reference/readouts.py`), not from the Rust code. `<label>_cp<i>` holds the value on bar
i (zero-based); a bar without a value is written as `<label>_cp<i>_none=1`.
"""

from .. import readouts as ro
from .. import series as ps
from ..fixture import provenance
from . import price_location as pl
from .contract_readouts import vwap_rows

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
    "r_squared: 1 - SS_res / SS_tot of the least-squares line through the last n closes, 0..1.",
    "percent_rank: 100 * (previous n closes at or below the close) / n.",
    "price_location: the stretch (close - EMA) / ATR of the price-location read-out",
    "(fixtures/price_location.py), default and small windows.",
    "period_midpoint: scenarios over daily bars with explicit timestamps (pmscn_*): bar i has",
    "ts, high, low; out<i> is the held midpoint of the last completed period, new<i> 1 on the",
    "bar that completed one; `none` before the first period completed.",
    "vwap_regime: the distance of the close from the rolling VWAP in ATRs (value only).",
    "swing_structure: median correction over median impulse leg, in ATRs (value only).",
    "cycle_length: mean of the last `history` swing lengths between sign changes of",
    "close - SMA(close, period) (value only; bars_since_crossing is in the contract cases).",
]


def _vwap_regime(bars, window, lookback, atr_len, regime_window, flat, strong):
    atr = ro._wilder_atr([b[:4] for b in bars], atr_len)
    readings = ro.vwap_regime(vwap_rows(bars, window, lookback), [b[3] for b in bars], atr,
                              regime_window, flat, strong)
    return [None if r is None else r[0] for r in readings]


def _swing(bars, left, right, max_swings, atr_len):
    return [None if r is None else r[0]
            for r in ro.swing_structure([b[:4] for b in bars], left, right, max_swings, atr_len)]


def specs(bars):
    """(label, series of values) of every checked configuration."""
    closes = [b[3] for b in bars]
    return [
        ("fd_halves_default", ro.fractal_dimension(bars, 30, "halves")),
        ("fd_halves_odd", ro.fractal_dimension(bars, 9, "halves")),
        ("fd_halves_min", ro.fractal_dimension(bars, 4, "halves")),
        ("fd_path_default", ro.fractal_dimension(bars, 30, "path")),
        ("fd_path_short", ro.fractal_dimension(bars, 5, "path")),
        ("r2_default", ro.r_squared(closes, 20)),
        ("r2_short", ro.r_squared(closes, 2)),
        ("r2_long", ro.r_squared(closes, 60)),
        ("pr_default", ro.percent_rank(closes, 20)),
        ("pr_one", ro.percent_rank(closes, 1)),
        ("pr_long", ro.percent_rank(closes, 100)),
        ("cl_default", [None if v is None else v[0] for v in ro.cycle_length(closes, 20, 8)]),
        ("pl_default", [None if r["stretch"] is None else r["stretch"]
                        for r in pl.compute(bars, pl.DEFAULT)]),
        ("pl_small", [None if r["stretch"] is None else r["stretch"]
                      for r in pl.compute(bars, pl.SMALL)]),
        ("vr_default", _vwap_regime(bars, 390, 20, 14, 20, 0.1, 0.3)),
        ("vr_small", _vwap_regime(bars, 60, 5, 7, 10, 0.02, 0.10)),
        ("ss_default", _swing(bars, 5, 5, 50, 14)),
        ("ss_small", _swing(bars, 3, 3, 10, 7)),
        ("cl_fast", [None if v is None else v[0] for v in ro.cycle_length(closes, 5, 3)]),
    ]


SCENARIOS = [
    # label, unit, count, utc offset, first day (days since the epoch), bar time of day (s)
    ("month1", "month", 1, 0, 20, 0),
    ("month2", "month", 2, 0, 20, 0),
    ("week2", "week", 2, 0, 0, 43200),
    ("month_offset", "month", 1, -3600, 20, 1800),
]
SCENARIO_DAYS = 130


def scenario_bars(first_day, time_of_day):
    """Daily bars: timestamp, high, low from a fixed integer pattern."""
    bars = []
    for k in range(SCENARIO_DAYS):
        high = 100.0 + (k * 7) % 13
        low = high - 3.0 - (k * 5) % 4
        bars.append(((first_day + k) * 86400 + time_of_day, high, low))
    return bars


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
    for label, unit, count, offset, first_day, time_of_day in SCENARIOS:
        bars = scenario_bars(first_day, time_of_day)
        case = {"n": float(len(bars))}
        for k, ((ts, high, low), out) in enumerate(
                zip(bars, ro.period_midpoint(bars, unit, count, offset, step=None))):
            case[f"bar{k}_ts"] = float(ts)
            case[f"bar{k}_high"] = high
            case[f"bar{k}_low"] = low
            if out is None:
                case[f"out{k}_none"] = 1.0
            else:
                case[f"out{k}"], case[f"new{k}"] = out
        blocks.append((f"pmscn_{label}", f"period_midpoint scenario {label}", case))
    blocks.append((None, None, {"readouts_tolerance": 1e-9}))
    return HEADER, blocks
