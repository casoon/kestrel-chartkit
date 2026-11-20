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
from . import price_location as pl_golden
from .. import series as ps
from .contract_moving_averages import vwap_expected
from .contract_structure import candles, path, structure_wave
from .contract_volatility import SWING, SWING_SHORT, flat_bars, path_bars, swing_bars

NAME = "contract_readouts"
RENDER = render_contract

TOLERANCE = 1e-9


def defaults(case):
    """The same expected bars, run with no parameters: the registry defaults are the documented
    ones."""
    _, bars, expected = case
    return {}, bars, expected


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

def location_case(cfg, bars):
    """Value: the stretch; extras: every other reading that exists on the bar."""
    expected = []
    for rec in pl_golden.compute(bars, cfg):
        if rec["stretch"] is None:
            expected.append((None, {}))
            continue
        extra = {k: rec[k] for k in ("atr", "ema", "range_position", "sma", "sma_prior",
                                     "vol_tercile") if rec[k] is not None}
        expected.append(((rec["stretch"], extra), {}))
    return {k: float(v) for k, v in cfg.items()}, bars, expected


def midpoint_case(unit, count, offset, bars):
    params = {"unit": float(UNITS.index(unit)), "count": float(count),
              "utc_offset_seconds": float(offset)}
    expected = [(None, {}) if m is None else ((m[0], {"new_period": m[1]}), {})
                for m in ro.period_midpoint(bars, unit, count, offset)]
    return params, bars, expected


def opening_case(start, end, orb, offset, bars):
    params = {"start_hour": float(start[0]), "start_minute": float(start[1]),
              "end_hour": float(end[0]), "end_minute": float(end[1]),
              "orb_duration_mins": float(orb), "utc_offset_seconds": float(offset)}
    expected = []
    for r in ro.opening_range(bars, start[0], start[1], end[0], end[1], orb, offset):
        if r is None:
            expected.append((None, {}))
        else:
            expected.append(((r[0], {"orb_high": r[1], "orb_low": r[2], "in_orb_window": r[3],
                                     "new_session": r[4]}), {}))
    return params, bars, expected


def vwap_rows(bars, window, lookback):
    """Per bar None or (VWAP, sigma, slope or None) of the rolling VWAP (the contract of `vwap`)."""
    return [None if v[0] is None else (v[0][0], v[0][1]["sigma"], v[0][1].get("slope"))
            for v in vwap_expected(bars, window, lookback)]


def vwap_case(window, lookback, atr_len, regime_window, flat, strong, bars):
    """Value: distance of the close from the VWAP in ATRs; extras as documented on the type."""
    rows = vwap_rows(bars, window, lookback)
    closes = [b[3] for b in bars]
    atr = ro._wilder_atr([b[:4] for b in bars], atr_len)
    readings = ro.vwap_regime(rows, closes, atr, regime_window, flat, strong)
    expected = []
    for r in readings:
        if r is None:
            expected.append((None, {}))
            continue
        expected.append(((r[0], {"vwap": r[1], "slope_atr": r[2], "slope_state": r[3],
                                 "z_score": r[4], "cross_frequency": r[5],
                                 "price_persistence": r[6]}), {}))
    _margin(readings, flat, strong)
    params = {"window": float(window), "slope_lookback": float(lookback),
              "atr_len": float(atr_len), "regime_window": float(regime_window),
              "flat_threshold": flat, "strong_threshold": strong}
    return params, bars, expected


def _margin(readings, flat, strong):
    """No slope sits within a relative 1e-7 of a threshold: both implementations agree on the
    state."""
    for r in readings:
        if r is None:
            continue
        for threshold in (flat, strong):
            assert abs(abs(r[2]) - threshold) > 1e-7 * max(1.0, threshold), "slope on a threshold"


def swing_case(left, right, max_swings, atr_len, bars):
    """Value: correction / impulse ratio; extras as documented on the type."""
    expected = [(None, {}) if r is None else ((r[0], r[1]), {})
                for r in ro.swing_structure(bars, left, right, max_swings, atr_len)]
    params = {"left": float(left), "right": float(right), "max_swings": float(max_swings),
              "atr_len": float(atr_len)}
    return params, bars, expected


def zigzag(segments, start=100.0):
    return candles(path(start, segments))


UNITS = ["second", "minute", "hour", "day", "week", "month"]
DAYS = swing_bars(250)
WEEK = swing_bars(420, phase=3.0)
WAVE = ps.wave_series(330)
SMALL_LOCATION = {"atr_len": 5, "ema_len": 8, "range_len": 12, "sma_len": 30, "slope_bars": 4,
                  "vol_window": 25, "vol_warmup": 40}
MINIMAL_LOCATION = {"atr_len": 1, "ema_len": 1, "range_len": 1, "sma_len": 1, "slope_bars": 1,
                    "vol_window": 1, "vol_warmup": 1}

WAVE_VOLUME = ps.wave_series(130)
# The first six bars carry no volume: the VWAP exists from bar 7.
NO_VOLUME_START = [b[:4] + (0.0,) if i < 6 else b for i, b in enumerate(ps.wave_series(60))]
VWAP_THRESHOLDS = (0.02, 0.10)

# A rise whose corrections shrink (strengthening), one whose corrections grow (weakening), their
# mirror images, and a staircase with the corrections of constant size.
STRENGTHENING = zigzag([(1.0, 12), (-0.5, 8), (1.0, 12), (-0.5, 6), (1.0, 12), (-0.5, 4),
                        (1.0, 12), (-0.5, 3), (1.0, 12), (-0.5, 2)])
WEAKENING = zigzag([(1.0, 12), (-0.5, 2), (1.0, 12), (-0.5, 4), (1.0, 12), (-0.5, 6), (1.0, 12),
                    (-0.5, 8), (1.0, 12), (-0.5, 10)])
FALLING = zigzag([(-1.0, 12), (0.5, 8), (-1.0, 12), (0.5, 6), (-1.0, 12), (0.5, 4), (-1.0, 12),
                  (0.5, 3), (-1.0, 12), (0.5, 2)], 200.0)
STAIRCASE = zigzag([(1.0, 8), (-0.5, 6)] * 6, 100.0)

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
                   "over 90 swing bars", *defaults(fractal_case(30, "halves", SWING))),
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
                   *defaults(r2_case(20, SWING))),
        case_block("r_squared_two", "r_squared, period 2: two points always fit exactly, 1",
                   *r2_case(2, SWING_SHORT)),
        case_block("r_squared_line", "r_squared, period 5 over a straight climb: 1",
                   *r2_case(5, CLIMB)),
        case_block("r_squared_noise", "r_squared, period 6 over zig-zag noise: low",
                   *r2_case(6, ZIGZAG)),
        case_block("r_squared_flat", "r_squared, period 4 over equal closes: no variation to "
                   "explain, 1", *r2_case(4, EQUAL)),
        case_block("percent_rank_defaults", "percent_rank, defaults (20) over 90 swing bars",
                   *defaults(rank_case(20, SWING))),
        case_block("percent_rank_short", "percent_rank, period 3 over 40 swing bars",
                   *rank_case(3, SWING_SHORT)),
        case_block("percent_rank_one", "percent_rank, period 1: 100 when the close is not below "
                   "the previous one, else 0", *rank_case(1, SWING_SHORT)),
        case_block("percent_rank_ties", "percent_rank, period 4 over closes with ties: an equal "
                   "close counts as at or below", *rank_case(4, TIES)),
        case_block("percent_rank_flat", "percent_rank, period 4 over equal closes: every previous "
                   "close is at or below, 100", *rank_case(4, EQUAL)),
        case_block("cycle_length_defaults", "cycle_length, defaults (20, 8) over 90 swing bars",
                   *defaults(cycle_case(20, 8, SWING))),
        case_block("cycle_length_short", "cycle_length, period 3, history 2 over 40 swing bars: "
                   "only the last two swings count", *cycle_case(3, 2, SWING_SHORT)),
        case_block("cycle_length_blocks", "cycle_length, period 2, history 4 over closes in "
                   "blocks above and below their average", *cycle_case(2, 4, SWINGS)),
        case_block("cycle_length_touch", "cycle_length, period 3, history 3: bars exactly on the "
                   "average do not change the side", *cycle_case(3, 3, TOUCH)),
        case_block("cycle_length_flat", "cycle_length, period 3 over equal closes: always on the "
                   "average, no crossing, no output", *cycle_case(3, 3, EQUAL)),
        case_block("price_location_defaults", "price_location, defaults (14, 20, 50, 200, 20, 100, "
                   "200) over 330 wave bars: stretch from bar 20, the volatility tercile from "
                   "bar 200", *defaults(location_case(pl_golden.DEFAULT, WAVE))),
        case_block("price_location_small", "price_location, small windows over 120 wave bars",
                   *location_case(SMALL_LOCATION, WAVE[:120])),
        case_block("price_location_minimal", "price_location, every length 1 over 30 wave bars: "
                   "the ATR is the true range, the EMA the close", *location_case(
                       MINIMAL_LOCATION, WAVE[:30])),
        case_block("price_location_flat", "price_location over bars without range: the ATR stays "
                   "0, the stretch is undefined, no output", *location_case(
                       SMALL_LOCATION, flat_bars(50))),
        case_block("period_midpoint_defaults", "period_midpoint, defaults (1 hour) over 90 bars of "
                   "15 minutes: the first midpoint on the first bar of hour 2",
                   *defaults(midpoint_case("hour", 1, 0, SWING))),
        case_block("period_midpoint_day", "period_midpoint, 1 day over 250 bars of 15 minutes: the "
                   "first midpoint on the first bar of day 2", *midpoint_case("day", 1, 0, DAYS)),
        case_block("period_midpoint_minutes", "period_midpoint, 30 minutes over 40 bars: every "
                   "second bar completes a period", *midpoint_case("minute", 30, 0, SWING_SHORT)),
        case_block("period_midpoint_hours", "period_midpoint, 2 hours with an offset of 30 "
                   "minutes over 90 bars: periods cut in local time",
                   *midpoint_case("hour", 2, 1800, SWING)),
        case_block("period_midpoint_seconds", "period_midpoint, 1800 seconds over 40 bars: the "
                   "same periods as 30 minutes", *midpoint_case("second", 1800, 0, SWING_SHORT)),
        case_block("period_midpoint_week", "period_midpoint, 1 week over 420 bars: Monday "
                   "5 January 1970 (bar 384) completes the first week",
                   *midpoint_case("week", 1, 0, WEEK)),
        case_block("period_midpoint_negative_offset", "period_midpoint, 1 day with an offset of "
                   "-2 hours over 250 bars: the day changes at 02:00 UTC",
                   *midpoint_case("day", 1, -7200, DAYS)),
        case_block("opening_range_defaults", "opening_range, defaults (a session of 24 hours, "
                   "30 minutes) over 250 bars: the first bar (00:15) builds day 1's range, the "
                   "second (00:30) is outside the window", *defaults(opening_case((0, 0), (0, 0), 30, 0,
                                                                          DAYS))),
        case_block("opening_range_session", "opening_range, session 00:30-04:00, 45 minutes: no "
                   "output before the session, the range builds over three bars, the bar at the "
                   "end time is outside the session", *opening_case((0, 30), (4, 0), 45, 0,
                                                                     SWING)),
        case_block("opening_range_overnight", "opening_range, overnight session 22:00-02:00 "
                   "local with an offset of 22 hours: the session opens at 00:00 UTC",
                   *opening_case((22, 0), (2, 0), 60, 79200, DAYS)),
        case_block("opening_range_negative_offset", "opening_range, overnight session 23:00-03:00 "
                   "local with an offset of -1 hour: the first bars lie on the day before the "
                   "epoch day", *opening_case((23, 0), (3, 0), 30, -3600, DAYS)),
        case_block("opening_range_late", "opening_range, 15 minutes over a session of 24 hours: "
                   "the first bar (00:15) already lies past the window, so day 1 has a session "
                   "but no range; day 2 forms one", *opening_case((0, 0), (0, 0), 15, 0, DAYS)),
        case_block("opening_range_all_day", "opening_range, window of 1440 minutes: the range "
                   "keeps widening through the whole day", *opening_case((0, 0), (0, 0), 1440, 0,
                                                                         DAYS)),
        case_block("vwap_regime_defaults", "vwap_regime, defaults (390, 20, 14, 20, 0.1, 0.3) over "
                   "130 wave bars: from bar 21 (the slope), mostly a flat VWAP slope",
                   *defaults(vwap_case(390, 20, 14, 20, 0.1, 0.3, WAVE_VOLUME))),
        case_block("vwap_regime_small", "vwap_regime, window 30, slope over 5 bars, ATR 5, regime "
                   "window 8, thresholds 0.02 / 0.10 over 130 wave bars: every slope state",
                   *vwap_case(30, 5, 5, 8, *VWAP_THRESHOLDS, WAVE_VOLUME)),
        case_block("vwap_regime_no_volume", "vwap_regime over 60 wave bars whose first six carry "
                   "no volume: no VWAP before bar 7", *vwap_case(20, 3, 4, 6, *VWAP_THRESHOLDS,
                                                                 NO_VOLUME_START)),
        case_block("vwap_regime_flat", "vwap_regime over bars without range and a constant "
                   "close: the ATR is 0, distance and slope read 0", *vwap_case(
                       5, 2, 3, 4, 0.02, 0.10, flat_bars(14, 100.0, 0.0))),
        case_block("swing_structure_defaults", "swing_structure, defaults (5, 5, 50, 14) over the "
                   "staircase of five rising, six falling and three rising swings: every turn "
                   "a pivot", *defaults(swing_case(5, 5, 50, 14, structure_wave()))),
        case_block("swing_structure_strengthening", "swing_structure (2, 2, 50, 5) over a rise "
                   "whose corrections shrink: strengthening", *swing_case(2, 2, 50, 5,
                                                                          STRENGTHENING)),
        case_block("swing_structure_weakening", "swing_structure (2, 2, 50, 5) over a rise whose "
                   "corrections grow: weakening", *swing_case(2, 2, 50, 5, WEAKENING)),
        case_block("swing_structure_falling", "swing_structure (2, 2, 50, 5) over a fall whose "
                   "corrections shrink: the trend is down, the rises are the corrections",
                   *swing_case(2, 2, 50, 5, FALLING)),
        case_block("swing_structure_capped", "swing_structure (2, 2, 4, 5) over a staircase: only "
                   "the newest four legs count, the pivot list is cut to twelve",
                   *swing_case(2, 2, 4, 5, STAIRCASE)),
        case_block("swing_structure_flat", "swing_structure over bars without range: the ATR is "
                   "0, nothing is detected, no output", *swing_case(2, 2, 50, 3,
                                                                    flat_bars(30, 100.0, 0.0))),
    ]
    return header, blocks
