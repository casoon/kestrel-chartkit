"""contract_oscillators: complete per-bar contracts of the oscillators family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code. Where a formula already has an independent
implementation in `fixtures/oscillators.py` (the golden oscillator fixture), it is imported from
there and only extended to a per-bar run here.

Conventions assumed where the type documentation leaves them open:

- Crosses use the crate's documented tolerance (`contract_pilot.crossed_over/under`): `a` crosses
  over `b` when it was at or below `b + tol` and is now above `b + tol`, tol = 1e-9 * max(1,
  |prev_a|, |prev_b|, |a|, |b|). Exception: CCI, whose type documentation states no tolerance; its
  crosses are taken as plain comparisons (`prev <= prev_ref and now > ref`). The bars are chosen so
  that no cross or threshold decision lies within 1e-10 (relative) of flipping (`_margin` refuses
  such a case), so rounding cannot decide an event and both readings give the same events.
- "Extreme" alerts (stoch_rsi, cci, mfi, williams_r, tsi, fisher_transform) follow the RSI pattern
  of the pilot: a cross of line and signal fires `bull_extreme` only when the lowest of the last 5
  published line values (the current one included, the window full) is at or below `oversold`,
  `bear_extreme` likewise with the highest at or above `overbought`. Strength: how far that
  extreme lies beyond the threshold relative to |threshold|, clamped to 0..1 (1 for a threshold at
  zero that was exceeded, 0 if only reached). Crosses of the mid line (50; 0 for CCI, TSI and
  Fisher) have strength 1. The signal line is an EMA(3) of the line (fixed by the registry build).
- Context lines and their divergences (stoch_rsi, cci, williams_r, fisher_transform) appear only
  after a fixed context window of the registry build (stoch_rsi from bar 100 + k_len - 1, cci from
  bar 100, williams_r from bar 50, fisher_transform from bar 40); every case of these indicators
  ends before that, so neither `ctx` nor a divergence alert can occur. CCI exhaustion needs 200
  bars of line history and cannot occur either.
- TSI publishes `ctx` from its first bar: the same construction with the registry's context
  lengths 50/25. Its divergence alerts are not documented on the type; assumed: once five line
  values exist, `fast = line_t - line_(t-4)`, `slow = ctx_t - ctx_(t-4)`; `bull_divergence` when
  fast < -5 and slow > 0, `bear_divergence` when fast > 5 and slow < 0, strength (|fast| - 5) / 5
  clamped to 0..1 (div_len 4, div_min 5 from the registry build). TSI extremes at +/-25.
- Stochastic alerts are not documented on the type; assumed: `stoch_oversold` (0.8) when %K and
  %D are both at or below 20, otherwise `stoch_overbought` (0.8) when both are at or above 80.
- MACD alerts (signal and zero crosses, strength 1) and WaveTrend alerts (cross 0.8, extreme 1.0)
  are not documented with a strength; assumed as stated.
- KST: all four rates of change start with the 31st bar, when the longest one exists (that is what
  the documented first output on bar 53 = 31 + 14 + 8 implies); Coppock likewise from bar 15
  (first output 24 = 15 + 9).
"""

from fractions import Fraction

from ..exact import central_squares, sqrt as exact_sqrt, to_decimal
from ..fixture import provenance, render_contract
from ..indicators import clamp, raw_rsi
from ..smoothing import ema_first_sample
from .contract_pilot import case_block, crossed_over, crossed_under, wave
from .oscillators import (
    _bbtrend, _cci_raw, _connors, _fisher, _mfi_raw, _percent_r, _pmo, _rci, _smi,
    _sma_series, _stochastic_k, _trix, _ultimate, _wavetrend, _wma_last,
)

NAME = "contract_oscillators"
RENDER = render_contract

# Relative to max(1, |expected|): the second implementation repeats the documented arithmetic in
# a different order, so differences are rounding only.
TOLERANCE = 1e-9

# A comparison closer than this (relative) to flipping is refused: rounding differences between
# the two implementations must not decide an event.
FRAGILE = 1e-10


def _margin(x, y, label):
    if x != y and abs(x - y) <= FRAGILE * max(1.0, abs(x), abs(y)):
        raise ValueError(f"fragile comparison {label}: {x!r} vs {y!r}")


def _tol(*values):
    return 1e-9 * max(1.0, *(abs(v) for v in values))


def over(prev_a, prev_b, a, b):
    tol = _tol(prev_a, prev_b, a, b)
    _margin(prev_a, prev_b + tol, "cross")
    _margin(a, b + tol, "cross")
    return crossed_over(prev_a, prev_b, a, b)


def under(prev_a, prev_b, a, b):
    tol = _tol(prev_a, prev_b, a, b)
    _margin(prev_a, prev_b - tol, "cross")
    _margin(a, b - tol, "cross")
    return crossed_under(prev_a, prev_b, a, b)


def plain_over(prev_a, prev_b, a, b):
    _margin(prev_a, prev_b, "cross")
    _margin(a, b, "cross")
    return prev_a <= prev_b and a > b


def plain_under(prev_a, prev_b, a, b):
    _margin(prev_a, prev_b, "cross")
    _margin(a, b, "cross")
    return prev_a >= prev_b and a < b


def at_or_below(x, level):
    _margin(x, level, "threshold")
    return x <= level


def at_or_above(x, level):
    _margin(x, level, "threshold")
    return x >= level


def threshold_strength(excess, scale):
    """excess / scale clamped to 0..1; at scale 0: 1 if exceeded, 0 if only reached."""
    if scale == 0.0:
        return 1.0 if excess > 0.0 else 0.0
    return clamp(excess / scale, 0.0, 1.0)


def _floats(params):
    return {k: float(v) for k, v in params.items()}


# --- Bars ------------------------------------------------------------------------------------

# Bar shape cycles: gap of the open against the previous close, wick above and below the body,
# volume. Open differs from the close and the close sits away from mid-range, so a formula that
# reads the wrong price field shows.
GAPS = [0.05, -0.08, 0.12, 0.0, -0.03, 0.09, -0.11]
WICKS = [(0.3, 0.2), (0.15, 0.45), (0.5, 0.1), (0.25, 0.35), (0.4, 0.4), (0.1, 0.3)]
VOLUMES = [1000.0, 1350.0, 800.0, 1600.0, 1150.0, 900.0, 2100.0, 700.0]


def shaped(closes):
    bars = []
    for i, c in enumerate(closes):
        c = round(c, 6)
        prev = round(closes[i - 1], 6) if i else c
        o = round(prev + GAPS[i % len(GAPS)], 6)
        up, down = WICKS[i % len(WICKS)]
        bars.append((o, round(max(o, c) + up, 6), round(min(o, c) - down, 6), c,
                     VOLUMES[i % len(VOLUMES)]))
    return bars


def segments(start, parts):
    """Closes from `start`, each part (count, steps) adding its steps cyclically for count bars."""
    closes, price = [], start
    for count, steps in parts:
        for i in range(count):
            closes.append(price)
            price += steps[i % len(steps)]
    return closes


# An irregular oscillation (period 13): reaches both extremes of fast oscillators.
SWING = [1.2, 0.9, 1.4, 0.6, 1.1, 0.8, -0.4, -1.3, -1.0, -1.6, -0.7, -1.2, -0.5]
# Long runs in both directions with pullbacks: for slow lines (MACD, TSI, PMO, TRIX, KST ...).
TREND = [(22, [1.1, 0.7, -0.4, 1.3, 0.9]), (24, [-1.2, -0.6, 0.5, -1.4, -0.8]),
         (18, [0.9, 1.2, -0.7, 0.6]), (20, [-0.9, 0.4, -1.1, -0.6]),
         (16, [1.0, -0.3, 0.8, 0.5])]


def swing_bars(n, start=100.0):
    return shaped(wave(n, start, SWING))


def trend_bars(extra=()):
    return shaped(segments(100.0, TREND + list(extra)))


def flat_bars(n, price=50.0):
    """Identical bars, open = close, a fixed range around them."""
    return [(price, price + 0.5, price - 0.5, price, 1000.0) for _ in range(n)]


def point_bars(n, price=50.0):
    """Identical bars without any range."""
    return [(price, price, price, price, 1000.0) for _ in range(n)]


def offset_bars():
    """Closes near 10000 with cent-sized moves: a variance shortcut loses them in rounding."""
    cents = [5, 2, 8, 1, 9, 4, 7, 3, 6, 0, 8, 2]
    return shaped([10000.0 + c / 100.0 for c in cents])


def ties_bars():
    """RCI windows with shared prices (average ranks) and a flat stretch (0)."""
    return shaped([10.0, 11.0, 11.0, 12.0, 11.0, 13.0, 13.0, 13.0, 13.0, 13.0, 14.0, 12.0])


def doji_bars():
    """open == close on every bar, the last one without any range."""
    bars = [(p, p + 1.0, p - 0.5, p, 1000.0) for p in (10.0, 10.5, 11.0, 10.8, 10.2, 10.6)]
    return bars + [(10.6, 10.6, 10.6, 10.6, 1000.0)]


def closes_of(bars):
    return [b[3] for b in bars]


def none_until(first):
    """No output on the bars before the 0-based index `first`."""
    return [(None, {}) for _ in range(first)]


def single(first, values, extras=None):
    expected = none_until(first)
    for k, v in enumerate(values):
        expected.append(((v, extras[k] if extras else {}), {}))
    return expected


# --- Line/signal oscillators with extreme and mid alerts --------------------------------------

def extreme_alerts(line, signal, k, overbought, oversold, mid, plain=False):
    """Alerts of the RSI pattern on the k-th published value (see the module docstring)."""
    alerts = {}
    if k == 0:
        return alerts
    up, down = (plain_over, plain_under) if plain else (over, under)
    prev, prev_sig, value, sig = line[k - 1], signal[k - 1], line[k], signal[k]
    window = line[k - 4:k + 1] if k >= 4 else None
    if up(prev, prev_sig, value, sig) and window and at_or_below(min(window), oversold):
        alerts["bull_extreme"] = threshold_strength(oversold - min(window), abs(oversold))
    if down(prev, prev_sig, value, sig) and window and at_or_above(max(window), overbought):
        alerts["bear_extreme"] = threshold_strength(max(window) - overbought, abs(overbought))
    names = ("bull_zero_cross", "bear_zero_cross") if plain else ("bull_mid_cross", "bear_mid_cross")
    if up(prev, mid, value, mid):
        alerts[names[0]] = 1.0
    if down(prev, mid, value, mid):
        alerts[names[1]] = 1.0
    return alerts


def line_signal(first, line, signal, overbought, oversold, mid, plain=False):
    expected = none_until(first)
    for k, (value, sig) in enumerate(zip(line, signal)):
        expected.append(((value, {"signal": sig}),
                         extreme_alerts(line, signal, k, overbought, oversold, mid, plain)))
    return expected


def stoch_rsi_case(rsi_len, stoch_len, k_len, d_len, overbought, oversold, bars):
    """Raw Wilder RSI over rsi_len changes, placed in the range of its last stoch_len values
    (100 (rsi - min) / (max - min) clamped, 50 for a flat range); %K = SMA(k_len) of that, %D =
    SMA(d_len) of %K, both clamped; value %K, extra signal %D, published only together."""
    assert len(bars) < 100 + k_len - 1, "the context line would appear"
    raw = raw_rsi(closes_of(bars), rsi_len)
    stoch = []
    for i in range(stoch_len - 1, len(raw)):
        w = raw[i - stoch_len + 1:i + 1]
        lo, hi = min(w), max(w)
        stoch.append(clamp(100.0 * (w[-1] - lo) / (hi - lo)) if hi > lo else 50.0)
    k_all = [clamp(x) for x in _sma_series(stoch, k_len)]
    d = [clamp(x) for x in _sma_series(k_all, d_len)]
    k = k_all[d_len - 1:]
    first = rsi_len + stoch_len + k_len + d_len - 3
    params = {"rsi_len": rsi_len, "stoch_len": stoch_len, "k_len": k_len, "d_len": d_len,
              "overbought": overbought, "oversold": oversold}
    return _floats(params), bars, line_signal(first, k, d, overbought, oversold, 50.0)


def cci_case(cci_len, avg_len, overbought, oversold, bars):
    """Raw CCI of the typical price over cci_len (0 without deviation); line = EMA(avg_len) of it,
    signal = EMA(3) of the line, both first-sample seeded. Plain crosses; the mid line is 0
    (`bull/bear_zero_cross`)."""
    assert len(bars) < 100, "the context line would appear"
    line = ema_first_sample(_cci_raw(bars, cci_len), avg_len)
    signal = ema_first_sample(line, 3)
    params = {"cci_len": cci_len, "avg_len": avg_len, "overbought": overbought,
              "oversold": oversold}
    return (_floats(params), bars,
            line_signal(cci_len - 1, line, signal, overbought, oversold, 0.0, plain=True))


def mfi_case(mfi_len, avg_len, overbought, oversold, bars):
    """Raw MFI over mfi_len (typical price x volume, signed by the change of the typical price, no
    flow on the first bar or when unchanged; 50 without flow); line = EMA(avg_len), signal = EMA(3)
    of the line. No alert while the window holds no volume."""
    line = ema_first_sample(_mfi_raw(bars, mfi_len), avg_len)
    signal = ema_first_sample(line, 3)
    expected = line_signal(mfi_len - 1, line, signal, overbought, oversold, 50.0)
    for i in range(mfi_len - 1, len(bars)):
        if sum(b[4] for b in bars[i - mfi_len + 1:i + 1]) <= 0.0:
            expected[i] = (expected[i][0], {})
    params = {"mfi_len": mfi_len, "avg_len": avg_len, "overbought": overbought,
              "oversold": oversold}
    return _floats(params), bars, expected


def williams_r_case(wpr_len, avg_len, overbought, oversold, bars):
    """Ascending %R over wpr_len (50 for a flat window); line = EMA(avg_len), signal = EMA(3)."""
    assert len(bars) < 50, "the context line would appear"
    line = ema_first_sample(_percent_r(bars, wpr_len), avg_len)
    signal = ema_first_sample(line, 3)
    params = {"wpr_len": wpr_len, "avg_len": avg_len, "overbought": overbought,
              "oversold": oversold}
    return (_floats(params), bars,
            line_signal(wpr_len - 1, line, signal, overbought, oversold, 50.0))


def fisher_case(fish_len, avg_len, overbought, oversold, bars):
    """Fisher recursion over hl2 in a window of fish_len; line = EMA(avg_len), signal = EMA(3)."""
    assert len(bars) < 40, "the context line would appear"
    line, signal = _fisher(bars, fish_len, avg_len, 3)
    params = {"fish_len": fish_len, "avg_len": avg_len, "overbought": overbought,
              "oversold": oversold}
    return (_floats(params), bars,
            line_signal(fish_len - 1, line, signal, overbought, oversold, 0.0))


def _tsi_line(moves, long_len, short_len):
    mom = ema_first_sample(ema_first_sample(moves, long_len), short_len)
    absolute = ema_first_sample(ema_first_sample([abs(m) for m in moves], long_len), short_len)
    return [100.0 * m / a if a != 0.0 else 0.0 for m, a in zip(mom, absolute)]


def tsi_case(long_len, short_len, sig_len, bars):
    """TSI over the close changes from the second bar on, signal EMA(sig_len), ctx the same
    construction with 50/25; extremes at +/-25, mid 0, divergence as in the module docstring."""
    closes = closes_of(bars)
    moves = [b - a for a, b in zip(closes, closes[1:])]
    line = _tsi_line(moves, long_len, short_len)
    ctx = _tsi_line(moves, 50, 25)
    signal = ema_first_sample(line, sig_len)
    expected = none_until(1)
    for k, (value, sig, c) in enumerate(zip(line, signal, ctx)):
        alerts = extreme_alerts(line, signal, k, 25.0, -25.0, 0.0)
        if k >= 4:
            fast, slow = value - line[k - 4], c - ctx[k - 4]
            for x, y in ((fast, -5.0), (fast, 5.0), (slow, 0.0)):
                _margin(x, y, "divergence")
            if fast < -5.0 and slow > 0.0:
                alerts["bull_divergence"] = clamp((abs(fast) - 5.0) / 5.0, 0.0, 1.0)
            if fast > 5.0 and slow < 0.0:
                alerts["bear_divergence"] = clamp((abs(fast) - 5.0) / 5.0, 0.0, 1.0)
        expected.append(((value, {"signal": sig, "ctx": c}), alerts))
    params = {"long_len": long_len, "short_len": short_len, "sig_len": sig_len}
    return _floats(params), bars, expected


# --- Lines with a signal or companions ----------------------------------------------------------

def macd_case(fast_len, slow_len, signal_len, bars):
    """EMA(fast) - EMA(slow) of the close, both first-sample seeded from the first bar, published
    from the slow_len-th bar; signal = EMA(signal_len) over the published values; hist = line -
    signal. Alerts: line/signal crosses (`bull_cross`, `bear_cross`) and zero crosses."""
    closes = closes_of(bars)
    fast, slow = ema_first_sample(closes, fast_len), ema_first_sample(closes, slow_len)
    line = [f - s for f, s in zip(fast, slow)][slow_len - 1:]
    signal = ema_first_sample(line, signal_len)
    expected = none_until(slow_len - 1)
    for k, (value, sig) in enumerate(zip(line, signal)):
        alerts = {}
        if k > 0:
            if over(line[k - 1], signal[k - 1], value, sig):
                alerts["bull_cross"] = 1.0
            if under(line[k - 1], signal[k - 1], value, sig):
                alerts["bear_cross"] = 1.0
            if over(line[k - 1], 0.0, value, 0.0):
                alerts["bull_zero_cross"] = 1.0
            if under(line[k - 1], 0.0, value, 0.0):
                alerts["bear_zero_cross"] = 1.0
        expected.append(((value, {"signal": sig, "hist": value - sig}), alerts))
    params = {"fast_len": fast_len, "slow_len": slow_len, "signal_len": signal_len}
    return _floats(params), bars, expected


def ppo_case(fast, slow, signal_len, bars):
    """100 (EMA(fast) - EMA(slow)) / EMA(slow), both first-sample seeded from the first bar,
    published from the slow-th bar; signal = EMA over the published values, hist = line - signal."""
    closes = closes_of(bars)
    f, s = ema_first_sample(closes, fast), ema_first_sample(closes, slow)
    line = [100.0 * (a - b) / b if b > 0.0 else 0.0 for a, b in zip(f, s)][slow - 1:]
    signal = ema_first_sample(line, signal_len)
    extras = [{"signal": g, "hist": v - g} for v, g in zip(line, signal)]
    params = {"fast_period": fast, "slow_period": slow, "signal_period": signal_len}
    return _floats(params), bars, single(slow - 1, line, extras)


def late_signal(first, line, signal, signal_len, with_hist=False):
    """A line from the 0-based bar `first`; its signal (and hist) only from the signal_len-th
    line value on."""
    extras = []
    for k, (value, sig) in enumerate(zip(line, signal)):
        extra = {}
        if k + 1 >= signal_len:
            extra["signal"] = sig
            if with_hist:
                extra["hist"] = value - sig
        extras.append(extra)
    return single(first, line, extras)


def pmo_case(length_1, length_2, signal_len, bars):
    line, signal = _pmo(closes_of(bars), length_1, length_2, signal_len)
    params = {"length_1": length_1, "length_2": length_2, "signal_len": signal_len}
    return _floats(params), bars, late_signal(length_1 + length_2 - 1, line, signal, signal_len)


def smi_case(length, smooth_1, smooth_2, signal_len, bars):
    line, signal = _smi([b[1:4] for b in bars], length, smooth_1, smooth_2, signal_len)
    params = {"len": length, "smooth_1": smooth_1, "smooth_2": smooth_2, "signal_len": signal_len}
    return (_floats(params), bars,
            late_signal(length + smooth_1 + smooth_2 - 3, line, signal, signal_len))


def trix_case(length, signal_len, bars):
    line, signal = _trix(closes_of(bars), length, signal_len)
    params = {"len": length, "signal_len": signal_len}
    return _floats(params), bars, late_signal(length, line, signal, signal_len, with_hist=True)


def wavetrend_case(n1, n2, ob, os_, bars):
    """wt1 = EMA(n2) of ci, ci from the typical price with EMA(n1) for esa and d (0 for d <=
    1e-8), wt2 = SMA(4) of wt1; from the 4th bar. Alerts: wt1 crossing wt2 (`bull_cross`,
    `bear_cross`, 0.8), the same cross with wt1 at or beyond os/ob (`bull_extreme`, `bear_extreme`,
    1.0)."""
    expected = none_until(3)
    previous = None
    for i in range(3, len(bars)):
        wt1, wt2 = _wavetrend(bars[:i + 1], n1, n2)
        alerts = {}
        if previous is not None:
            if over(previous[0], previous[1], wt1, wt2):
                alerts["bull_cross"] = 0.8
                if at_or_below(wt1, os_):
                    alerts["bull_extreme"] = 1.0
            if under(previous[0], previous[1], wt1, wt2):
                alerts["bear_cross"] = 0.8
                if at_or_above(wt1, ob):
                    alerts["bear_extreme"] = 1.0
        previous = (wt1, wt2)
        expected.append(((wt1, {"wt1": wt1, "wt2": wt2, "hist": wt1 - wt2, "ob_level": ob,
                                "os_level": os_}), alerts))
    return _floats({"n1": n1, "n2": n2, "ob_level": ob, "os_level": os_}), bars, expected


def stochastic_case(k_period, d_period, bars):
    """%K over k_period (50 without range, clamped), %D = SMA(d_period) of %K; value %K."""
    k = _stochastic_k(bars, k_period)
    d = [clamp(x) for x in _sma_series(k, d_period)]
    k = k[d_period - 1:]
    expected = none_until(k_period + d_period - 2)
    for kv, dv in zip(k, d):
        alerts = {}
        if at_or_below(kv, 20.0) and at_or_below(dv, 20.0):
            alerts["stoch_oversold"] = 0.8
        elif at_or_above(kv, 80.0) and at_or_above(dv, 80.0):
            alerts["stoch_overbought"] = 0.8
        expected.append(((kv, {"percent_k": kv, "percent_d": dv}), alerts))
    return _floats({"k_period": k_period, "d_period": d_period}), bars, expected


# --- Single lines ------------------------------------------------------------------------------

def _roc(closes, t, n):
    past = closes[t - n]
    return 100.0 * (closes[t] - past) / past if past > 0.0 else 0.0


def roc_case(period, bars):
    closes = closes_of(bars)
    values = [_roc(closes, t, period) for t in range(period, len(closes))]
    extras = [{"abs_momentum": closes[t] - closes[t - period]} for t in range(period, len(closes))]
    return _floats({"period": period}), bars, single(period, values, extras)


def rci_case(length, bars):
    closes = closes_of(bars)
    values = [_rci(closes[t - length + 1:t + 1]) for t in range(length - 1, len(closes))]
    return _floats({"len": length}), bars, single(length - 1, values)


def ultimate_case(p1, p2, p3, bars):
    values = [_ultimate(bars[:t + 1], p1, p2, p3) for t in range(p3, len(bars))]
    return _floats({"period1": p1, "period2": p2, "period3": p3}), bars, single(p3, values)


def awesome_case(fast, slow, bars):
    hl2 = [(b[1] + b[2]) / 2.0 for b in bars]
    values = [sum(hl2[t - fast + 1:t + 1]) / fast - sum(hl2[t - slow + 1:t + 1]) / slow
              for t in range(slow - 1, len(bars))]
    return _floats({"fast_period": fast, "slow_period": slow}), bars, single(slow - 1, values)


def cmo_case(period, bars):
    """Over the last `period` close changes, 100 (up - down) / (up + down), 0 when nothing moved
    (sum at or below 1e-8)."""
    closes = closes_of(bars)
    moves = [b - a for a, b in zip(closes, closes[1:])]
    values = []
    for t in range(period - 1, len(moves)):
        w = moves[t - period + 1:t + 1]
        up, down = sum(m for m in w if m > 0.0), sum(-m for m in w if m < 0.0)
        values.append(clamp(100.0 * (up - down) / (up + down), -100.0, 100.0)
                      if up + down > 1e-8 else 0.0)
    return _floats({"period": period}), bars, single(period, values)


def elder_ray_case(period, bars):
    ema = ema_first_sample(closes_of(bars), period)
    values, extras = [], []
    for t in range(period - 1, len(bars)):
        bull, bear = bars[t][1] - ema[t], bars[t][2] - ema[t]
        values.append(bull)
        extras.append({"bull_power": bull, "bear_power": bear, "ema": ema[t]})
    return _floats({"period": period}), bars, single(period - 1, values, extras)


def connors_case(rsi_len, streak_len, rank_len, bars):
    """As documented on the type (and as `oscillators._connors` implements it): RSI lines (EMA(3)
    over the raw RSI), the streak RSI from the price RSI's first bar, the rank over all returns so
    far, at most 100; rank_len has no effect."""
    closes = closes_of(bars)
    first = rsi_len + streak_len
    values = [_connors(closes[:t + 1], rsi_len, streak_len) for t in range(first, len(closes))]
    params = {"rsi_len": rsi_len, "streak_len": streak_len, "rank_len": rank_len}
    return _floats(params), bars, single(first, values)


def coppock_case(bars):
    """WMA(10) of ROC(14) + ROC(11), the ROCs from the 15th bar on; first output bar 24."""
    closes = closes_of(bars)
    raw = [_roc(closes, t, 14) + _roc(closes, t, 11) for t in range(14, len(closes))]
    values = [_wma_last(raw[:k + 1], 10) for k in range(9, len(raw))]
    return {}, bars, single(23, values)


def kst_case(bars):
    """SMA10(ROC10) + 2 SMA10(ROC15) + 3 SMA10(ROC20) + 4 SMA15(ROC30), every ROC from the 31st bar
    on; signal SMA9 of KST; first output bar 53 with extras kst, signal, hist."""
    closes = closes_of(bars)
    rocs = {n: [_roc(closes, t, n) for t in range(30, len(closes))] for n in (10, 15, 20, 30)}
    parts = [_sma_series(rocs[10], 10)[5:], _sma_series(rocs[15], 10)[5:],
             _sma_series(rocs[20], 10)[5:], _sma_series(rocs[30], 15)]
    kst = [a + 2.0 * b + 3.0 * c + 4.0 * d for a, b, c, d in zip(*parts)]
    signal = _sma_series(kst, 9)
    kst = kst[8:]
    extras = [{"kst": k, "signal": s, "hist": k - s} for k, s in zip(kst, signal)]
    return {}, bars, single(52, kst, extras)


def dpo_case(period, bars):
    """The close floor(period/2) bars ago minus the SMA of the last `period` closes."""
    closes = closes_of(bars)
    values = [closes[t - period // 2] - sum(closes[t - period + 1:t + 1]) / period
              for t in range(period - 1, len(closes))]
    return _floats({"period": period}), bars, single(period - 1, values)


def vigor(bars):
    return [(c - o) / max(h - l, 1e-8) for o, h, l, c, _ in bars]


def rvi_case(period, bars):
    """SMA(period) of (close - open) / range (range floored at 1e-8), signal SMA(4) of that, from
    the period + 3-th bar; extras rvi (the value again) and signal."""
    rvi = _sma_series(vigor(bars), period)
    signal = _sma_series(rvi, 4)
    rvi = rvi[3:]
    extras = [{"rvi": r, "signal": s} for r, s in zip(rvi, signal)]
    return _floats({"period": period}), bars, single(period + 2, rvi, extras)


def bop_case(period, bars):
    values = [clamp(x, -1.0, 1.0) for x in _sma_series(vigor(bars), period)]
    return _floats({"period": period}), bars, single(period - 1, values)


def bbtrend_case(short_len, long_len, mult, bars):
    rows = _bbtrend(closes_of(bars), short_len, long_len, mult)
    values = [r["value"] for r in rows]
    extras = [{"upper_gap": r["upper_gap"], "lower_gap": r["lower_gap"]} for r in rows]
    params = {"short_len": short_len, "long_len": long_len, "mult": mult}
    return _floats(params), bars, single(long_len - 1, values, extras)


def zscore_case(period, bars):
    """(close - mean) / population sd over the last `period` closes, computed exactly; 0 when the
    sd is below 1e-8."""
    closes = closes_of(bars)
    values = []
    for t in range(period - 1, len(closes)):
        squares, mean = central_squares(closes[t - period + 1:t + 1])
        sd = exact_sqrt(squares / period)
        values.append(float(to_decimal(Fraction(closes[t]) - mean) / sd) if sd > 1e-8 else 0.0)
    return _floats({"period": period}), bars, single(period - 1, values)


# --- Cases -------------------------------------------------------------------------------------

def build():
    header = provenance("contract_oscillators") + [
        "", "Format: see contract_pilot. Conventions assumed where the type documentation is silent:",
        "see the module docstring (crosses with the crate tolerance, CCI plain; extreme alerts over",
        "the last 5 line values; TSI divergence and Stochastic alerts as stated there).",
    ]
    swing60 = swing_bars(60)
    swing99 = swing_bars(99)
    swing49 = swing_bars(49)
    swing39 = swing_bars(39)
    swing20 = swing_bars(20)
    swing15 = swing_bars(15)
    trend = trend_bars()
    zero_volume = [b[:4] + (0.0,) for b in swing_bars(30)]
    blocks = [
        case_block("macd_defaults", "MACD(12, 26, 9) over the trend series",
                   *macd_case(12, 26, 9, trend)),
        case_block("macd_short", "MACD(3, 7, 4) over the swing series",
                   *macd_case(3, 7, 4, swing60)),
        case_block("macd_flat", "MACD(3, 5, 2) of a constant close is 0, no cross",
                   *macd_case(3, 5, 2, flat_bars(10))),

        case_block("stoch_rsi_defaults", "Stochastic RSI, catalog defaults (14, 14, 3, 3, 80/20)",
                   *stoch_rsi_case(14, 14, 3, 3, 80.0, 20.0, swing99)),
        case_block("stoch_rsi_short", "Stochastic RSI(5, 7, 2, 4), zones 70/30",
                   *stoch_rsi_case(5, 7, 2, 4, 70.0, 30.0, swing60)),
        case_block("stoch_rsi_flat", "Flat closes: RSI 50, flat stochastic range 50, no alert",
                   *stoch_rsi_case(3, 3, 2, 2, 80.0, 20.0, flat_bars(12))),

        case_block("cci_defaults", "CCI, catalog defaults (20, 3, +/-100)",
                   *cci_case(20, 3, 100.0, -100.0, swing99)),
        case_block("cci_raw", "CCI(7) with avg_len 1 (the raw line), zones +/-80",
                   *cci_case(7, 1, 80.0, -80.0, swing60)),
        case_block("cci_flat", "Flat bars: no mean deviation, raw CCI 0",
                   *cci_case(5, 2, 100.0, -100.0, flat_bars(9))),

        case_block("mfi_defaults", "MFI, catalog defaults (14, 3, 80/20)",
                   *mfi_case(14, 3, 80.0, 20.0, trend)),
        case_block("mfi_short", "MFI(5) with avg_len 1, zones 70/30",
                   *mfi_case(5, 1, 70.0, 30.0, swing60)),
        case_block("mfi_zero_volume", "No volume: no flow, raw MFI 50, no alert",
                   *mfi_case(6, 2, 80.0, 20.0, zero_volume)),

        case_block("pmo_defaults", "PMO(35, 20, 10) over the trend series",
                   *pmo_case(35, 20, 10, trend)),
        case_block("pmo_short", "PMO(5, 3, 4) over the swing series", *pmo_case(5, 3, 4, swing60)),
        case_block("pmo_flat", "Constant closes: zero returns, PMO 0",
                   *pmo_case(3, 2, 2, flat_bars(9))),

        case_block("smi_defaults", "SMI(10, 3, 3, 3) over the swing series",
                   *smi_case(10, 3, 3, 3, swing60)),
        case_block("smi_custom", "SMI(6, 2, 4, 5)", *smi_case(6, 2, 4, 5, swing60)),
        case_block("smi_flat", "Bars without any range: SMI 0 by convention",
                   *smi_case(3, 2, 2, 2, point_bars(9))),

        case_block("rci_defaults", "RCI(9) over the swing series", *rci_case(9, swing60)),
        case_block("rci_ties", "RCI(4): shared prices take average ranks, a flat window gives 0",
                   *rci_case(4, ties_bars())),

        case_block("trix_defaults", "TRIX(15, 9) over the trend series",
                   *trix_case(15, 9, trend)),
        case_block("trix_short", "TRIX(4, 3) over the swing series", *trix_case(4, 3, swing60)),

        case_block("williams_r_defaults", "Williams %R, catalog defaults (14, 3, 80/20)",
                   *williams_r_case(14, 3, 80.0, 20.0, swing49)),
        case_block("williams_r_raw", "Williams %R(6) with avg_len 1 (the raw line), zones 90/10",
                   *williams_r_case(6, 1, 90.0, 10.0, swing49)),
        case_block("williams_r_flat", "A window without range gives 50",
                   *williams_r_case(4, 2, 80.0, 20.0, flat_bars(8))),

        case_block("tsi_defaults", "TSI(25, 13, 7) over the trend series",
                   *tsi_case(25, 13, 7, trend)),
        case_block("tsi_short", "TSI(8, 4, 3) over the swing series", *tsi_case(8, 4, 3, swing60)),
        case_block("tsi_rising", "TSI(10, 5, 4) over a rising swing: the context line keeps rising",
                   *tsi_case(10, 5, 4, shaped(wave(60, 100.0, [x + 0.25 for x in SWING[7:] + SWING[:7]])))),
        case_block("tsi_flat", "Constant closes: zero denominator, TSI 0",
                   *tsi_case(5, 3, 3, flat_bars(10))),

        case_block("fisher_transform_defaults", "Fisher, catalog defaults (10, 2, +/-1.5)",
                   *fisher_case(10, 2, 1.5, -1.5, swing39)),
        case_block("fisher_transform_raw", "Fisher(6) with avg_len 1, zones +/-1",
                   *fisher_case(6, 1, 1.0, -1.0, swing39)),
        case_block("fisher_transform_flat", "A flat window: normalised term 0, line 0",
                   *fisher_case(4, 2, 1.5, -1.5, flat_bars(8))),

        case_block("stochastic_defaults", "Stochastic(14, 3) over the swing series",
                   *stochastic_case(14, 3, swing60)),
        case_block("stochastic_short", "Stochastic(5, 2)", *stochastic_case(5, 2, swing60)),
        case_block("stochastic_flat", "A window without range gives 50, no alert",
                   *stochastic_case(3, 2, flat_bars(6))),

        case_block("roc_defaults", "ROC(12) over the swing series", *roc_case(12, swing60)),
        case_block("roc_short", "ROC(1): the one-bar change", *roc_case(1, swing15)),

        case_block("ultimate_oscillator_defaults", "Ultimate oscillator(7, 14, 28)",
                   *ultimate_case(7, 14, 28, swing60)),
        case_block("ultimate_oscillator_short", "Ultimate oscillator(2, 3, 5)",
                   *ultimate_case(2, 3, 5, swing20)),
        case_block("ultimate_oscillator_flat", "Bars without range against the previous close: 0",
                   *ultimate_case(1, 2, 3, point_bars(6))),

        case_block("awesome_oscillator_defaults", "Awesome oscillator(5, 34)",
                   *awesome_case(5, 34, swing60)),
        case_block("awesome_oscillator_short", "Awesome oscillator(2, 6)",
                   *awesome_case(2, 6, swing20)),

        case_block("ppo_defaults", "PPO(12, 26, 9) over the trend series",
                   *ppo_case(12, 26, 9, trend)),
        case_block("ppo_short", "PPO(3, 8, 4)", *ppo_case(3, 8, 4, swing60)),

        case_block("wavetrend_defaults", "WaveTrend(10, 21, 60/-60) over the swing series",
                   *wavetrend_case(10, 21, 60.0, -60.0, swing60)),
        case_block("wavetrend_short", "WaveTrend(4, 6), levels 40/-40",
                   *wavetrend_case(4, 6, 40.0, -40.0, swing60)),

        case_block("cmo_defaults", "CMO(14) over the swing series", *cmo_case(14, swing60)),
        case_block("cmo_short", "CMO(4)", *cmo_case(4, swing20)),
        case_block("cmo_flat", "Constant closes: nothing moved, 0", *cmo_case(3, flat_bars(6))),

        case_block("elder_ray_defaults", "Elder ray(13) over the swing series",
                   *elder_ray_case(13, swing60)),
        case_block("elder_ray_short", "Elder ray(4)", *elder_ray_case(4, swing15)),

        case_block("connors_rsi_defaults",
                   "Connors RSI(3, 2, 100) over 115 bars: the rank window is capped at 100 returns",
                   *connors_case(3, 2, 100, trend_bars([(15, [-0.5, 0.8, -1.1])]))),
        case_block("connors_rsi_rank_len", "Connors RSI(5, 3) with rank_len 10: documented to have no effect",
                   *connors_case(5, 3, 10, swing60)),

        case_block("coppock_trend", "Coppock curve over the trend series", *coppock_case(trend)),
        case_block("kst_trend", "KST over the trend series", *kst_case(trend)),

        case_block("dpo_defaults", "DPO(21) over the swing series", *dpo_case(21, swing60)),
        case_block("dpo_even", "DPO(4): the close 2 bars ago against SMA(4)",
                   *dpo_case(4, swing15)),

        case_block("rvi_defaults", "RVI(10) over the swing series", *rvi_case(10, swing60)),
        case_block("rvi_short", "RVI(3)", *rvi_case(3, swing15)),
        case_block("rvi_doji", "open == close: 0 throughout", *rvi_case(2, doji_bars())),

        case_block("bop_defaults", "BOP(14) over the swing series", *bop_case(14, swing60)),
        case_block("bop_short", "BOP(3)", *bop_case(3, swing15)),
        case_block("bop_doji", "open == close: 0 throughout", *bop_case(2, doji_bars())),

        case_block("bbtrend_defaults", "BBTrend(20, 50, 2) over the trend series",
                   *bbtrend_case(20, 50, 2.0, trend)),
        case_block("bbtrend_short", "BBTrend(3, 7, 1.5)", *bbtrend_case(3, 7, 1.5, swing20)),

        case_block("zscore_defaults", "Z-score(20) over the swing series",
                   *zscore_case(20, swing60)),
        case_block("zscore_offset", "Z-score(5) of closes near 10000 moving in cents",
                   *zscore_case(5, offset_bars())),
        case_block("zscore_flat", "Constant closes: no deviation, 0", *zscore_case(3, flat_bars(6))),
    ]
    return header, blocks
