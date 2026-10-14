"""contract_pilot: complete per-bar contracts for SMA, RSI, ATR and the order block (plan 62, pilot).

Unlike the golden fixtures, which check selected values, a contract case fixes the whole run: the
input bars, the parameters, and for every bar whether an output exists, its value, every extra
field it carries (and that it carries no others), and the complete set of alerts with their
strength. A bar without `out` keys expects no output; a bar without `alert` keys expects no alert.

SMA, RSI and ATR follow the formulas documented on their types. The order block is a structure
detector without a single formula; its case is a hand-built scenario whose expected events are
derived bar by bar in the comments below.
"""

from ..fixture import provenance
from ..indicators import clamp, rsi_lines, true_ranges
from ..smoothing import rma

NAME = "contract_pilot"

# Relative to max(1, |expected|). The second implementation repeats the documented arithmetic,
# so differences are rounding only.
TOLERANCE = 1e-9

# Both cross tests use the crate's documented tolerance: `a` crosses over `b` when it was at or
# below `b + tol` and is now above `b + tol`, tol = 1e-9 * max(1, |prev_a|, |prev_b|, |a|, |b|).
CROSS_TOLERANCE_REL = 1e-9


def _tol(*values):
    return CROSS_TOLERANCE_REL * max(1.0, *(abs(v) for v in values))


def crossed_over(prev_a, prev_b, a, b):
    tol = _tol(prev_a, prev_b, a, b)
    return prev_a <= prev_b + tol and a > b + tol


def crossed_under(prev_a, prev_b, a, b):
    tol = _tol(prev_a, prev_b, a, b)
    return prev_a >= prev_b - tol and a < b - tol


def flat_bar(close, spread=1.0, volume=1000.0):
    """Open below the close and an asymmetric range, so a wrong price field shows."""
    return (close - 0.3, close + spread, close - 0.6 * spread, close, volume)


def wave(n, start, steps):
    """Closes from `start`, adding steps[i % len(steps)] per bar."""
    closes, price = [], start
    for i in range(n):
        closes.append(price)
        price += steps[i % len(steps)]
    return closes


# --- SMA -------------------------------------------------------------------------------------

def sma_case(period, closes):
    bars = [flat_bar(c) for c in closes]
    expected = []
    for i in range(len(closes)):
        if i + 1 < period:
            expected.append((None, {}))
        else:
            window = closes[i + 1 - period:i + 1]
            expected.append(((sum(window) / period, {}), {}))
    return {"period": float(period)}, bars, expected


# --- RSI -------------------------------------------------------------------------------------

def rsi_case(rsi_len, avg_len, sig_len, overbought, oversold, closes):
    """Line and signal from `rsi_lines`; alerts per the RSI type: line/signal crosses inside the
    extreme zones (lowest/highest line of the last 5 bars, current included, at or beyond the
    threshold), crosses of the 50 mid line. Strength of an extreme cross: how far the window's
    extreme lies beyond the threshold, relative to the threshold, clamped to 0..1; mid crosses 1.
    The context line needs 100 changes and the exhaustion 200 bars of line history: neither exists
    in these cases, so no `ctx` field, no divergence and no exhaustion alert."""
    assert len(closes) <= 100
    bars = [flat_bar(c) for c in closes]
    line, signal, _ = rsi_lines(closes, rsi_len, avg_len, sig_len)
    first = rsi_len  # 0-based bar of the first output: rsi_len changes need rsi_len + 1 closes
    expected = [(None, {}) for _ in range(first)]
    for k, (value, sig) in enumerate(zip(line, signal)):
        alerts = {}
        if k > 0:
            prev_value, prev_sig = line[k - 1], signal[k - 1]
            window = line[max(0, k - 4):k + 1]
            low, high = (min(window), max(window)) if len(window) == 5 else (None, None)
            if crossed_over(prev_value, prev_sig, value, sig) and low is not None and low <= oversold:
                alerts["bull_extreme"] = clamp((oversold - low) / abs(oversold), 0.0, 1.0)
            if crossed_under(prev_value, prev_sig, value, sig) and high is not None and high >= overbought:
                alerts["bear_extreme"] = clamp((high - overbought) / abs(overbought), 0.0, 1.0)
            if crossed_over(prev_value, 50.0, value, 50.0):
                alerts["bull_mid_cross"] = 1.0
            if crossed_under(prev_value, 50.0, value, 50.0):
                alerts["bear_mid_cross"] = 1.0
        expected.append(((value, {"signal": sig}), alerts))
    params = {"rsi_len": float(rsi_len), "avg_len": float(avg_len), "sig_len": float(sig_len),
              "overbought": overbought, "oversold": oversold}
    return params, bars, expected


# --- ATR -------------------------------------------------------------------------------------

def atr_case(atr_len, sig_len, bars):
    """TR, Wilder average over atr_len (seeded with the SMA of the first atr_len TRs); value =
    100 ATR / close, `raw` = ATR in price units, `signal` = Wilder average of the value over
    sig_len. Alerts on crosses of value and signal: `expansion` upward, `contraction` downward,
    strength |value - signal| / signal clamped to 0..1."""
    tr = true_ranges([b[:4] for b in bars])
    raw = rma(tr, atr_len)
    disp = [100.0 * r / b[3] if r is not None and b[3] > 0.0 else (0.0 if r is not None else None)
            for r, b in zip(raw, bars)]
    published = [d for d in disp if d is not None]
    sig = rma(published, sig_len)
    offset = len(disp) - len(published)
    expected = [(None, {}) for _ in range(offset)]
    previous = None
    for k, d in enumerate(published):
        i = offset + k
        if sig[k] is None:
            expected.append((None, {}))
            continue
        alerts = {}
        if previous is not None:
            strength = clamp(abs((d - sig[k]) / sig[k]), 0.0, 1.0) if sig[k] != 0.0 else 0.0
            if crossed_over(previous[0], previous[1], d, sig[k]):
                alerts["expansion"] = strength
            if crossed_under(previous[0], previous[1], d, sig[k]):
                alerts["contraction"] = strength
        previous = (d, sig[k])
        expected.append(((d, {"signal": sig[k], "raw": raw[i]}), alerts))
    return {"atr_len": float(atr_len), "sig_len": float(sig_len)}, bars, expected


def atr_bars():
    """Quiet, then gaps and wide ranges, then quiet again: the gap terms of the true range matter
    and the value crosses its signal in both directions."""
    bars, close = [], 50.0
    plan = [0.4] * 10 + [2.5, -2.0, 3.0, -2.5, 2.0] + [0.3] * 10 + [1.5, -1.5] + [0.2] * 8
    for i, move in enumerate(plan):
        gap = move if abs(move) > 1.0 else 0.0
        open_ = close + gap
        close = open_ + (0.1 if i % 2 else -0.1)
        span = abs(move) if abs(move) > 1.0 else 0.4
        bars.append((open_, max(open_, close) + span, min(open_, close) - 0.5 * span, close, 1000.0))
    return bars


# --- Order block -----------------------------------------------------------------------------

def order_block_case():
    """atr_len 3, min_disp 1.0. ATR = mean true range of the last 3 bars (each against its own
    previous close), defined from the 4th bar on. A zone is born when a bar's body is at least
    min_disp x ATR, its direction is opposite to the bar before, and the zone is that previous
    bar's high/low. A later bar reaching into the zone is a retest unless it closes beyond the far
    side, which invalidates and removes the zone on that bar. value = active_count = bullish minus
    bearish active zones; the most recent active zone is reported as top/bottom with its age."""
    bars = [
        (100.0, 100.5, 99.5, 100.0),   # 1-3 flat, TR 1
        (100.0, 100.5, 99.5, 100.0),
        (100.0, 100.5, 99.5, 100.0),
        (100.0, 100.5, 99.5, 100.0),   # 4 first output; nothing active
        (100.5, 101.0, 99.0, 99.5),    # 5 bearish, TR 2, ATR 4/3; body 1 too small
        (99.6, 106.0, 99.6, 105.5),    # 6 TR 6.5, ATR 9.5/3; body 5.9 -> bullish zone 99..101
        (105.5, 106.5, 105.0, 106.0),  # 7 stays above the zone
        (103.0, 103.5, 100.5, 101.5),  # 8 low 100.5 <= 101, close >= 99 -> retest (body 1.5 < ATR 4.5)
        (101.5, 101.5, 97.0, 98.0),    # 9 close 98 < 99 -> zone invalidated (body 3.5 < ATR 11.5/3)
        (98.0, 99.0, 97.5, 98.8),      # 10 bullish, small
        (98.7, 98.7, 92.0, 92.5),      # 11 TR 6.8, ATR 12.8/3; body 6.2 -> bearish zone 97.5..99
        (92.5, 93.0, 91.5, 92.0),      # 12 stays below the zone
        (92.0, 98.0, 92.0, 96.5),      # 13 high 98 >= 97.5, close <= 99 -> retest (body 4.5 < ATR 14.3/3)
        (96.5, 100.0, 96.0, 99.5),     # 14 close 99.5 > 99 -> zone invalidated
    ]
    bull = {"active_ob_top": 101.0, "active_ob_bottom": 99.0}
    bear = {"active_ob_top": 99.0, "active_ob_bottom": 97.5}
    expected = [
        (None, {}), (None, {}), (None, {}),
        ((0.0, {"active_count": 0.0}), {}),
        ((0.0, {"active_count": 0.0}), {}),
        ((1.0, {"active_count": 1.0, **bull, "active_ob_duration": 0.0}),
         {"bullish_order_block": 0.9}),
        ((1.0, {"active_count": 1.0, **bull, "active_ob_duration": 1.0}), {}),
        ((1.0, {"active_count": 1.0, **bull, "active_ob_duration": 2.0}),
         {"ob_retest_bullish": 0.8}),
        ((0.0, {"active_count": 0.0}), {}),
        ((0.0, {"active_count": 0.0}), {}),
        ((-1.0, {"active_count": -1.0, **bear, "active_ob_duration": 0.0}),
         {"bearish_order_block": 0.9}),
        ((-1.0, {"active_count": -1.0, **bear, "active_ob_duration": 1.0}), {}),
        ((-1.0, {"active_count": -1.0, **bear, "active_ob_duration": 2.0}),
         {"ob_retest_bearish": 0.8}),
        ((0.0, {"active_count": 0.0}), {}),
    ]
    return {"atr_len": 3.0, "min_disp": 1.0}, [b + (1000.0,) for b in bars], expected


# --- Rendering -------------------------------------------------------------------------------

def case_block(name, comment, params, bars, expected):
    assert len(bars) == len(expected), name
    case = {"tolerance": TOLERANCE, "bars": float(len(bars))}
    for key, value in params.items():
        case[f"param_{key}"] = value
    for i, (o, h, l, c, v) in enumerate(bars, start=1):
        case.update({f"bar{i}_open": o, f"bar{i}_high": h, f"bar{i}_low": l,
                     f"bar{i}_close": c, f"bar{i}_volume": v})
    for i, (output, alerts) in enumerate(expected, start=1):
        if output is not None:
            value, extra = output
            case[f"out{i}_value"] = value
            for key in sorted(extra):
                case[f"out{i}_extra_{key}"] = extra[key]
        for kind in sorted(alerts):
            case[f"alert{i}_{kind}"] = alerts[kind]
    return (name, comment, case)


RSI_WAVE = wave(60, 100.0, [1.5, 1.2, 1.0, 0.8, -0.3, -1.6, -1.4, -1.1, -0.9, 0.4])


def build():
    header = provenance("contract_pilot") + [
        "",
        "Per case: param_*, bars and bar<i>_open/high/low/close/volume (1-based); out<i>_value and",
        "out<i>_extra_<field> for every bar with an output; alert<i>_<kind> = strength for every",
        "alert. Missing out keys mean no output on that bar, missing alert keys no alert, missing",
        "extra keys that the field must be absent.",
    ]
    blocks = [
        case_block("sma_rising", "SMA(3) over 100, 101, 102, 103: plan 62 hand example",
                   *sma_case(3, [100.0, 101.0, 102.0, 103.0])),
        case_block("sma_single", "SMA(1) is the close itself, from the first bar",
                   *sma_case(1, [100.0, 104.0, 98.0])),
        case_block("sma_constant", "SMA(4) of a constant series is the constant",
                   *sma_case(4, [50.0] * 7)),
        case_block("rsi_rawline", "RSI(5) with avg_len 1 (the line is the raw RSI), signal EMA(3)",
                   *rsi_case(5, 1, 3, 70.0, 30.0, RSI_WAVE)),
        case_block("rsi_defaults", "RSI with the catalog defaults (14, 3, 3, 70/30)",
                   *rsi_case(14, 3, 3, 70.0, 30.0, RSI_WAVE)),
        case_block("rsi_flatrise", "RSI(3): flat closes give 50, a rise without losses 100",
                   *rsi_case(3, 1, 2, 70.0, 30.0, [10.0] * 6 + [11.0, 12.0, 13.0, 14.0])),
        case_block("atr_gaps", "ATR(5), signal 3: gaps and wide ranges between quiet phases",
                   *atr_case(5, 3, atr_bars())),
        case_block("order_block_cycle", "Order block: bullish and bearish zone, each born, retested, invalidated",
                   *order_block_case()),
    ]
    return header, blocks
