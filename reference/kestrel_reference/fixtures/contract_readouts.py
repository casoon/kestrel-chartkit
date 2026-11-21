"""contract_readouts: complete per-bar contracts of the read-out indicators (plan 141).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the definition documented
beside the indicator, written as a second implementation in `kestrel_reference/readouts.py` (and
the golden modules it reuses), not from the Rust code. Bars are (open, high, low, close, volume);
indices in comments are 1-based like the fixture. Contract bars are spaced 900 seconds apart,
the first at 900.

Conventions the documentation leaves open, assumed here:

- None of these read-outs raises alerts.
- The state label of `vwap_regime` (`slope_state`) and of `swing_structure` (`trend_quality`) is
  written as the `state` of the output: strongly_falling, moderately_falling, flat,
  moderately_rising, strongly_rising; weakening, stable, strengthening.
"""

from ..fixture import provenance, render_contract
from .. import readouts as ro
from .contract_pilot import case_block
from . import price_location as pl_golden
from .. import series as ps
from .contract_moving_averages import vwap_expected
from .contract_structure import candles, fvg_bars, fvg_touch_bars, path, structure_wave
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
    """Per bar None or (VWAP, sigma, slope or None) of the rolling VWAP (the contract of `vwap`).
    A window whose bars with volume all have the same typical price has a sigma of exactly 0
    (a window of one bar, a stretch of constant prices): the rounding left in the sum of squares
    is no spread."""
    rows = []
    for i, v in enumerate(vwap_expected(bars, window, lookback)):
        if v[0] is None:
            rows.append(None)
            continue
        win = bars[max(0, i + 1 - window):i + 1]
        typical = {(b[1] + b[2] + b[3]) / 3.0 for b in win if b[4] > 0.0}
        sigma = 0.0 if len(typical) == 1 else v[0][1]["sigma"]
        rows.append((v[0][0], sigma, v[0][1].get("slope")))
    return rows


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
        extras = {"vwap": r[1], "slope_atr": r[2], "slope_state": r[3],
                  "cross_frequency": r[5], "price_persistence": r[6]}
        if r[4] is not None:
            extras["z_score"] = r[4]
        expected.append(((r[0], extras), {}))
    _margin(readings, flat, strong)
    params = {"window": float(window), "slope_lookback": float(lookback),
              "atr_len": float(atr_len), "regime_window": float(regime_window),
              "flat_threshold": flat, "strong_threshold": strong}
    return params, bars, expected


def _margin(readings, flat, strong):
    """No slope sits within a relative 1e-7 of a threshold: both implementations agree on the
    state. A slope of exactly 0 is the documented exception: it is flat at every threshold."""
    for r in readings:
        if r is None or r[2] == 0.0:
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


SLOPE_LABELS = {-2.0: "strongly_falling", -1.0: "moderately_falling", 0.0: "flat",
                1.0: "moderately_rising", 2.0: "strongly_rising"}
TREND_LABELS = {-1.0: "weakening", 0.0: "stable", 1.0: "strengthening"}


def stated(block, field, labels):
    """`block` with the `state` of every output that has the extra `field`, from `labels`."""
    case = block[2]
    for key in [k for k in case if k.endswith(f"_extra_{field}")]:
        case[key.replace(f"_extra_{field}", "_state")] = labels[case[key]]
    return block


def vwap_block(name, comment, params, bars, expected):
    return stated(case_block(name, comment, params, bars, expected), "slope_state", SLOPE_LABELS)


def swing_block(name, comment, params, bars, expected):
    return stated(case_block(name, comment, params, bars, expected), "trend_quality",
                  TREND_LABELS)


def exact_bars(tps, closes, volume=1000.0):
    """Bars on a 0.25 grid whose typical price is exactly `tps[i]` and whose close is `closes[i]`:
    the high and the low lie symmetrically about (3 tp - close) / 2, a quarter beyond the close,
    so every sum a window of two bars forms is exact in floating point."""
    bars = []
    for tp, close in zip(tps, closes):
        middle = (3.0 * tp - close) / 2.0
        reach = abs(close - middle) + 0.5
        assert (middle + reach + middle - reach + close) / 3.0 == tp
        bars.append((tp, middle + reach, middle - reach, close, volume))
    return bars


def mirror_bars(bars, level=200.0):
    """The same series reflected about `level`: highs and lows change places."""
    return [(level - o, level - l, level - h, level - c, v) for o, h, l, c, v in bars]


# Typical prices of a window of two bars; the close lies `ON_VWAP_OFFSETS[i]` above the mean of
# the typical prices of the bar and the one before (the first bar: its own typical price). A bar
# with offset 0 closes exactly on its VWAP; its neighbours lie above, below or on the same side.
ON_VWAP_TPS = [100, 101, 101, 102, 102, 102, 103, 102.5, 102.5, 102, 101, 101, 100.5, 100.5, 100,
               100, 101, 102, 103, 103.25]
ON_VWAP_OFFSETS = [0.5, 0, 0, -0.25, 0, 0.25, 0, 0.5, -0.25, 0, 0, 0.75, 0, -0.5, 0.25, 0, 0, -0.25,
                   0.5, 0]


def on_vwap_bars():
    closes = [tp + off if i == 0 else (tp + ON_VWAP_TPS[i - 1]) / 2.0 + off
              for i, (tp, off) in enumerate(zip(ON_VWAP_TPS, ON_VWAP_OFFSETS))]
    return exact_bars(ON_VWAP_TPS, closes)


def flat_slope_bars():
    """Typical prices that stand still for a while: the VWAP of two bars, and so its slope over 2
    bars, is exactly 0, and a step of a quarter makes it small and non-zero."""
    tps = [100.0] * 8 + [100.25] * 6 + [100.0] * 6 + [100.5] * 5 + [100.0] * 5
    return exact_bars(tps, tps)


def constant_bars(count, price=100.0, volume=1000.0):
    return [(price, price, price, price, volume)] * count


def zero_volume(bars, start, end):
    """`bars` with no volume from the 1-based bar `start` to bar `end`."""
    return [b[:4] + (0.0,) if start <= i + 1 <= end else b for i, b in enumerate(bars)]


def fvg_case(max_zones, bars):
    """Value: zones still open; extras: created, filled, open_bullish, open_bearish."""
    expected = [((r[0], {"created": r[1], "filled": r[2], "open_bullish": r[3],
                         "open_bearish": r[4]}), {}) for r in ro.fvg_fill(bars, max_zones)]
    return {"max_zones": float(max_zones)}, bars, expected


def zigzag(segments, start=100.0):
    return candles(path(start, segments))


# Rises and falls of one per bar, a pivot at every turn (two bars each side): the turns are the
# 1-based bars 7, 12, 19, 24, 30, 36, 44, 48, 54, 59 (0-based index + 1).
TURNS_SEGMENTS = [(1.0, 6), (-1.0, 5), (1.0, 7), (-0.8, 5), (1.0, 6), (-1.0, 6), (1.0, 8),
                  (-0.5, 4), (1.0, 6), (-1.0, 5)]
TURNS = zigzag(TURNS_SEGMENTS)
TURN_INDEX = [sum(n for _, n in TURNS_SEGMENTS[:k]) for k in range(len(TURNS_SEGMENTS) + 1)]


def outside_bar(bars, index, close):
    """The 0-based bar `index` made an outside bar: its high above and its low below every bar of
    the window of two bars each side, opening in the middle and closing at `close` above (`up`)
    or below (`down`) the open, or at the open (`doji`)."""
    window = bars[max(0, index - 2):index + 3]
    high = max(b[1] for b in window) + 1.0
    low = min(b[2] for b in window) - 1.0
    open_ = (high + low) / 2.0
    closes = {"up": open_ + 0.5, "down": open_ - 0.5, "doji": open_}
    return (open_, high, low, closes[close], bars[index][4])


def with_outside(bars, edits):
    """`bars` with the outside bars `{turn: close kind}`: turn k is the bar where the k-th
    segment of `TURNS_SEGMENTS` ends."""
    out = list(bars)
    for turn, close in edits.items():
        out[TURN_INDEX[turn]] = outside_bar(bars, TURN_INDEX[turn], close)
    return out


def with_levels(bars, edits):
    """`bars` with the 0-based bar's high and/or low replaced: `{index: (high, low)}`, None keeps."""
    out = list(bars)
    for index, (high, low) in edits.items():
        o, h, l, c, v = out[index]
        out[index] = (o, h if high is None else high, l if low is None else low, c, v)
    return out


# The first pivot is an outside bar: no pivot before it, so the close against the open decides
# which of its two pivots comes first (a close above the open: the low), and a close at the open
# gives none at all.
OUTSIDE_LOW_FIRST = with_outside(TURNS, {1: "up"})
OUTSIDE_HIGH_FIRST = with_outside(TURNS, {1: "down"})
OUTSIDE_DOJI = with_outside(TURNS, {1: "doji"})
# Outside bars after pivots: at turn 3 a high (the pivot before is a low: the high first), at turn 4
# and 6 lows (the pivot before is a high or, after the outside bar of turn 3, the low of that bar),
# whatever they close at.
OUTSIDE_LATER = with_outside(TURNS, {3: "up", 4: "down", 6: "up"})
# Equal highs: the dip between the highs of turn 9 (bar 54) and 11 (bar 65) has two bars with the
# same low, so neither is a pivot low; the two highs are equal and the earlier one stays. At turn 5
# the two highest bars are equal, a flat top, which is no pivot.
TIES_SEGMENTS = TURNS_SEGMENTS + [(1.0, 6), (-1.0, 5), (1.0, 6)]
TIES_BARS = zigzag(TIES_SEGMENTS)
TIES_TURNS = [sum(n for _, n in TIES_SEGMENTS[:k]) for k in range(len(TIES_SEGMENTS) + 1)]
TIES_HIGHS = with_levels(TIES_BARS, {
    TIES_TURNS[10] + 1: (None, TIES_BARS[TIES_TURNS[10]][2]),
    TIES_TURNS[9]: (TIES_BARS[TIES_TURNS[11]][1], None),
    TIES_TURNS[5] + 1: (TIES_BARS[TIES_TURNS[5]][1], None)})

# Equal lows, the mirror image of the highs: the rise between the lows of turn 10 (bar 59) and 12
# (bar 70) has two bars with the same high, the two lows are equal and the earlier one stays; the
# series ends on a pivot high, so the newest impulse leg starts at the low that stays.
TIES_LOW_SEGMENTS = TURNS_SEGMENTS + [(1.0, 6), (-1.0, 5), (1.0, 7), (-1.0, 2)]
TIES_LOW_BARS = zigzag(TIES_LOW_SEGMENTS)
TIES_LOW_TURNS = [sum(n for _, n in TIES_LOW_SEGMENTS[:k]) for k in range(len(TIES_LOW_SEGMENTS) + 1)]
TIES_LOWS = with_levels(TIES_LOW_BARS, {
    TIES_LOW_TURNS[11] + 1: (TIES_LOW_BARS[TIES_LOW_TURNS[11]][1], None),
    TIES_LOW_TURNS[12]: (None, TIES_LOW_BARS[TIES_LOW_TURNS[10]][2]),
    TIES_LOW_TURNS[5] + 1: (TIES_LOW_BARS[TIES_LOW_TURNS[5]][1], None)})


def swing_mirror_case(left, right, max_swings, atr_len, bars):
    """The case of `bars` reflected about 200. A reflected series gives the readings of the
    original (they are distances in ATRs), which is asserted here."""
    original = swing_case(left, right, max_swings, atr_len, bars)
    mirrored = swing_case(left, right, max_swings, atr_len, mirror_bars(bars))
    assert len(original[2]) == len(mirrored[2])
    for (a, _), (b, _) in zip(original[2], mirrored[2]):
        assert (a is None) == (b is None)
        if a is not None:
            assert a[1].keys() == b[1].keys()
            assert all(abs(a[1][k] - b[1][k]) <= 1e-9 * max(1.0, abs(a[1][k])) for k in a[1])
            assert abs(a[0] - b[0]) <= 1e-9 * max(1.0, abs(a[0]))
    return mirrored


FVG_HAND = [(10.0, 9.0), (11.0, 10.0), (13.0, 12.0), (14.0, 12.0), (16.0, 13.0), (18.0, 15.0),
            (20.0, 17.0), (19.0, 14.5), (14.0, 12.0), (12.0, 10.0), (11.0, 9.0), (12.0, 10.5),
            (15.0, 13.0), (14.0, 13.5), (13.0, 12.8)]


def fvg_hand_bars():
    """Fifteen bars given as (high, low), the open and close at the middle:
    3   a bullish zone 10-12
    4   its low 12 touches the top of that zone on the first bar after it: filled; the bar itself
        opens the bullish zone 11-12, which its own low does not fill
    6, 7  bullish zones 14-15 and 16-17
    8   its low reaches both: two zones filled on one bar
    9   a bearish zone 14-17; its low 12 touches the top of the zone 11-12: filled
    10, 11  bearish zones 12-14.5 and 11-12
    12  its high 12 touches the bottom of the zone 12-14.5 and reaches the zone 11-12: two filled,
        no new zone
    13  fills the zone 14-17 and opens a bullish zone 11-13
    14  opens a bullish zone 12-13.5
    15  its low 12.8 fills both."""
    return [((h + l) / 2.0, h, l, (h + l) / 2.0, 1000.0) for h, l in FVG_HAND]


def regime_walk(n, seed):
    """A walk on a quarter grid whose drift changes every 30 bars, so rallies and drops leave
    gaps of both kinds, some of them touched by the very next bar."""
    state = seed

    def draw(count):
        nonlocal state
        state = (state * 6364136223846793005 + 1442695040888963407) % 2 ** 64
        return (state >> 33) % count

    bars, price, drift = [], 100.0, 0.0
    for i in range(n):
        if i % 30 == 0:
            drift = (draw(7) - 3) * 0.75 + (100.0 - price) * 0.05
        open_ = price
        close = open_ + drift + (draw(5) - 2) * 0.5
        bars.append((open_, max(open_, close) + draw(4) * 0.25, min(open_, close) - draw(4) * 0.25,
                     close, 1000.0))
        price = close
    return bars


FVG_WALK = regime_walk(300, 20261121)
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


def _vwap_mirror_guard():
    """The reflected bars of `vwap_regime_on_vwap` read as the mirror image: distance, slope and
    z_score negated, the VWAP reflected about 200, everything else the same, and bars on the VWAP
    stay on it."""
    args = (2, 2, 3, 5, 0.05, 0.20)
    original = vwap_case(*args, on_vwap_bars())[2]
    mirrored = vwap_case(*args, mirror_bars(on_vwap_bars()))[2]
    assert any(o[0] is not None and o[0][0] == 0.0 for o in original)
    for (a, _), (b, _) in zip(original, mirrored):
        assert (a is None) == (b is None)
        if a is None:
            continue
        (value, extra), (mirror, mirror_extra) = a, b
        assert extra.keys() == mirror_extra.keys()
        assert abs(value + mirror) < 1e-9
        assert abs(extra["vwap"] + mirror_extra["vwap"] - 200.0) < 1e-9
        for key in ("slope_atr", "slope_state", "z_score"):
            if key in extra:
                assert abs(extra[key] + mirror_extra[key]) < 1e-9
        for key in ("cross_frequency", "price_persistence"):
            assert extra[key] == mirror_extra[key]


def _guard():
    """The touch case really has bars on the average (the model's own arithmetic, exact on whole
    numbers)."""
    closes = [b[3] for b in TOUCH]
    on_average = [i for i in range(2, len(closes))
                  if closes[i] - sum(closes[i - 2:i + 1]) / 3 == 0.0]
    assert on_average, "TOUCH has no bar on its average"
    _vwap_mirror_guard()


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
        case_block("fvg_fill_defaults", "fvg_fill, defaults (20 zones) over a quiet drift, a rally "
                   "of bullish gaps, a drop of bearish gaps (which fills the bullish zones and "
                   "opens bearish ones) and a quiet base", *defaults(fvg_case(20, fvg_bars()))),
        case_block("fvg_fill_capped", "fvg_fill, only the newest 2 open zones kept, over the same "
                   "bars", *fvg_case(2, fvg_bars())),
        case_block("fvg_fill_touch", "fvg_fill over bars whose lows and highs only touch the "
                   "bars two before them: no gap, no zone", *fvg_case(20, fvg_touch_bars())),
        case_block("fvg_fill_hand_defaults", "fvg_fill, defaults over 15 hand-built bars: a zone "
                   "filled on the first bar after it by touching its edge, a bar that opens a zone "
                   "while filling another, two zones filled on one bar, bearish zones touched on "
                   "their edge", *defaults(fvg_case(20, fvg_hand_bars()))),
        case_block("fvg_fill_hand_one", "fvg_fill, only the newest zone kept, over the same "
                   "bars: an older open zone is dropped when a newer one forms and never fills",
                   *fvg_case(1, fvg_hand_bars())),
        case_block("fvg_fill_walk_defaults", "fvg_fill, defaults over 300 bars of a walk with a changing "
                   "drift: gaps of both kinds", *defaults(fvg_case(20, FVG_WALK))),
        case_block("fvg_fill_walk_capped", "fvg_fill, newest 3 zones kept, over the same walk",
                   *fvg_case(3, FVG_WALK)),
        vwap_block("vwap_regime_defaults", "vwap_regime, defaults (390, 20, 14, 20, 0.1, 0.3) over "
                   "130 wave bars: from bar 21 (the slope), mostly a flat VWAP slope",
                   *defaults(vwap_case(390, 20, 14, 20, 0.1, 0.3, WAVE_VOLUME))),
        vwap_block("vwap_regime_small", "vwap_regime, window 30, slope over 5 bars, ATR 5, regime "
                   "window 8, thresholds 0.02 / 0.10 over 130 wave bars: every slope state",
                   *vwap_case(30, 5, 5, 8, *VWAP_THRESHOLDS, WAVE_VOLUME)),
        vwap_block("vwap_regime_no_volume", "vwap_regime over 60 wave bars whose first six carry "
                   "no volume: no VWAP before bar 7", *vwap_case(20, 3, 4, 6, *VWAP_THRESHOLDS,
                                                                 NO_VOLUME_START)),
        vwap_block("vwap_regime_flat", "vwap_regime over bars without range and a constant "
                   "close: the ATR is 0, so there is no output", *vwap_case(
                       5, 2, 3, 4, 0.02, 0.10, flat_bars(14, 100.0, 0.0))),
        vwap_block("vwap_regime_on_vwap", "vwap_regime, window 2, slope over 2 bars, ATR 3, regime "
                   "window 5, thresholds 0.05 / 0.20 over 20 bars on a 0.25 grid: nine closes lie "
                   "exactly on the VWAP, counting on neither side and not breaking a run of "
                   "crosses; a sigma of 0 on equal typical prices has no z_score",
                   *vwap_case(2, 2, 3, 5, 0.05, 0.20, on_vwap_bars())),
        vwap_block("vwap_regime_mirror_on_vwap", "vwap_regime over the bars of vwap_regime_on_vwap "
                   "reflected about 200: every reading the mirror image (distance, slope, z_score "
                   "negated, the VWAP reflected), the same cross_frequency and price_persistence",
                   *vwap_case(2, 2, 3, 5, 0.05, 0.20, mirror_bars(on_vwap_bars()))),
        vwap_block("vwap_regime_slope_zero", "vwap_regime, window 2, slope over 2 bars, ATR 3, "
                   "regime window 4, flat threshold 0: a slope of exactly 0 is flat, the "
                   "smallest step is moderate, a larger one strong",
                   *vwap_case(2, 2, 3, 4, 0.0, 0.1, flat_slope_bars())),
        vwap_block("vwap_regime_constant_start", "vwap_regime, window 6, slope over 2 bars, ATR 3, "
                   "regime window 5 over 12 bars of one constant price and then 40 wave bars: the "
                   "ATR is 0 and nothing is published until the first true range, then output "
                   "resumes", *vwap_case(6, 2, 3, 5, *VWAP_THRESHOLDS,
                                         constant_bars(12) + ps.wave_series(40))),
        vwap_block("vwap_regime_constant_stretch", "vwap_regime, window 6, slope over 2 bars, ATR "
                   "3, regime window 5 over 30 wave bars, 30 bars of one constant price and 30 "
                   "wave bars: the ATR only decays, so every bar publishes; once the window holds "
                   "only the constant price its sigma is 0 (no z_score) and the slope flat",
                   *vwap_case(6, 2, 3, 5, *VWAP_THRESHOLDS,
                              ps.wave_series(30) + constant_bars(30) + ps.wave_series(30))),
        vwap_block("vwap_regime_window_one", "vwap_regime with a window of 1 over 60 wave bars: "
                   "the VWAP is the typical price, its sigma is always 0 and there is no z_score",
                   *vwap_case(1, 3, 4, 5, *VWAP_THRESHOLDS, ps.wave_series(60))),
        vwap_block("vwap_regime_volume_gap", "vwap_regime, window 8, slope over 3 bars, ATR 4, "
                   "regime window 6 over 90 wave bars without volume from bar 31 to bar 50: the "
                   "window loses its volume on bar 38 (no output), the VWAP history and the "
                   "regime window wait and go on from bar 51",
                   *vwap_case(8, 3, 4, 6, *VWAP_THRESHOLDS,
                              zero_volume(ps.wave_series(90), 31, 50))),
        swing_block("swing_structure_defaults", "swing_structure, defaults (5, 5, 50, 14) over the "
                   "staircase of five rising, six falling and three rising swings: every turn "
                   "a pivot", *defaults(swing_case(5, 5, 50, 14, structure_wave()))),
        swing_block("swing_structure_strengthening", "swing_structure (2, 2, 50, 5) over a rise "
                   "whose corrections shrink: strengthening", *swing_case(2, 2, 50, 5,
                                                                          STRENGTHENING)),
        swing_block("swing_structure_weakening", "swing_structure (2, 2, 50, 5) over a rise whose "
                   "corrections grow: weakening", *swing_case(2, 2, 50, 5, WEAKENING)),
        swing_block("swing_structure_falling", "swing_structure (2, 2, 50, 5) over a fall whose "
                   "corrections shrink: the trend is down, the rises are the corrections",
                   *swing_case(2, 2, 50, 5, FALLING)),
        swing_block("swing_structure_capped", "swing_structure (2, 2, 4, 5) over a staircase: only "
                   "the newest four legs count, the pivot list is cut to twelve",
                   *swing_case(2, 2, 4, 5, STAIRCASE)),
        swing_block("swing_structure_flat", "swing_structure over bars without range: the ATR is "
                   "0, nothing is detected, no output", *swing_case(2, 2, 50, 3,
                                                                    flat_bars(30, 100.0, 0.0))),
        swing_block("swing_structure_outside_low_first", "swing_structure (2, 2, 50, 5) over a "
                    "staircase whose first pivot is an outside bar closing above its open: its "
                    "low comes first", *swing_case(2, 2, 50, 5, OUTSIDE_LOW_FIRST)),
        swing_block("swing_structure_outside_high_first", "swing_structure (2, 2, 50, 5) over the "
                    "same staircase with the first outside bar closing below its open: its high "
                    "comes first", *swing_case(2, 2, 50, 5, OUTSIDE_HIGH_FIRST)),
        swing_block("swing_structure_outside_doji", "swing_structure (2, 2, 50, 5) over the same "
                    "staircase with the first outside bar closing at its open: neither pivot "
                    "counts", *swing_case(2, 2, 50, 5, OUTSIDE_DOJI)),
        swing_block("swing_structure_outside_later", "swing_structure (2, 2, 50, 5) over the same "
                    "staircase with outside bars at the third, fourth and sixth turn: the order "
                    "of an outside bar's two pivots follows the pivot before it",
                    *swing_case(2, 2, 50, 5, OUTSIDE_LATER)),
        swing_block("swing_structure_ties_highs", "swing_structure (2, 2, 50, 5): a flat top is "
                    "no pivot, two equal highs with no pivot low between them leave the earlier "
                    "one standing", *swing_case(2, 2, 50, 5, TIES_HIGHS)),
        swing_block("swing_structure_ties_lows", "swing_structure (2, 2, 50, 5): a flat top is "
                    "no pivot, two equal lows with no pivot high between them leave the earlier "
                    "one standing", *swing_case(2, 2, 50, 5, TIES_LOWS)),
        swing_block("swing_structure_mirror_ties", "swing_structure over the bars of "
                    "swing_structure_ties_highs reflected about 200: the same readings",
                    *swing_mirror_case(2, 2, 50, 5, TIES_HIGHS)),
        swing_block("swing_structure_mirror_outside_later", "swing_structure over the bars of "
                    "swing_structure_outside_later reflected about 200: the same readings",
                    *swing_mirror_case(2, 2, 50, 5, OUTSIDE_LATER)),
        swing_block("swing_structure_mirror_outside_low_first", "swing_structure over the bars of "
                    "swing_structure_outside_low_first reflected about 200: the same readings",
                    *swing_mirror_case(2, 2, 50, 5, OUTSIDE_LOW_FIRST)),
    ]
    return header, blocks
