"""contract_volatility: complete per-bar contracts of the volatility family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code. Where a golden fixture module already holds an
independent implementation of a formula (`fixtures/volatility.py`, `indicators.py`, `exact.py`),
it is imported and evaluated on every prefix of the case bars, so each bar's value is the
documented formula over the history up to that bar.

Conventions the type documentation leaves open, assumed here:

- Alert strengths that the documentation does not state are fixed constants per kind: DMI
  dominance 0.8, efficiency high 0.85 / low 0.70, volatility regime squeeze 0.7 / expansion 0.8,
  every flip and VIX Fix spike 1.0. The ADX level crosses use the crate's documented
  `threshold_strength` (excess over the level relative to the level, clamped to 0..1; with a
  level of 0: 1 for any excess, 0 at the level). Adaptive Chandelier traps: 1 - dist_atr,
  clamped to 0..1.
- Alert kinds the documentation does not name are taken as the catalog names them:
  `dmi_bullish_dominance`/`dmi_bearish_dominance`, `high_leg_efficiency`/`low_leg_efficiency`,
  `chandelier_flip_long`/`chandelier_flip_short`, and for the Adaptive Chandelier `bull_flip`,
  `bear_flip`, `bull_weak_flip`, `bear_weak_flip`, `bear_bull_trap` (the bull trap: a wick above
  the short stop while short) and `bull_bear_trap` (the bear trap: a wick below the long stop
  while long).
- DMI dominance is exclusive: the bullish test (+DI > -DI + 10) is decided first; the same for
  the efficiency alerts (high first) and the volatility regime (squeeze before expansion).
- ADX alerts compare against the previous output bar; the first output has no previous one and
  therefore no alert. Its signal line is the first-sample EMA(3) over the ADX (the registry fixes
  sig_len at 3). Crosses use the crate's documented tolerance (`contract_pilot.crossed_over`).
- Chandelier Exit and Adaptive Chandelier: on the first output bar there is no previous stop; the
  "previous" stops are that bar's raw stops (as in the golden reference `_chandelier`).
- Ties that a floating-point difference between the two implementations could decide are kept
  out of the cases: every threshold decision asserts a relative margin of MARGIN, unless the
  case is built to sit exactly on the threshold with exactly representable numbers (named in the
  case comment).
"""

import math

from ..exact import bollinger
from ..fixture import provenance
from ..indicators import directional, efficiency_ratio, keltner, true_ranges
from ..smoothing import ema_first_sample, rma
from .contract_pilot import case_block, crossed_over, crossed_under
from .volatility import (_aroon, _chande_kroll, _choppiness, _dmi, _garman_klass, _mass_index,
                         _relative_volatility, _ulcer, _vortex)

NAME = "contract_volatility"

TOLERANCE = 1e-9

# Smallest relative distance from a threshold that a decision in a case may have.
MARGIN = 1e-7


def clamp(x, low, high):
    return min(max(x, low), high)


def threshold_strength(excess, scale):
    """The crate's documented threshold strength: excess / scale clamped to 0..1, 0 for 0/0."""
    if scale == 0.0:
        return 1.0 if excess > 0.0 else 0.0
    return clamp(excess / scale, 0.0, 1.0)


def _guard(a, b, what):
    assert abs(a - b) > MARGIN * max(1.0, abs(a), abs(b)), \
        f"decision too close to call: {what} ({a} vs {b})"


def gt(a, b, what):
    _guard(a, b, what)
    return a > b


def ge(a, b, what, exact=False):
    """a >= b; `exact` admits an intended tie of exactly representable values."""
    if exact and a == b:
        return True
    _guard(a, b, what)
    return a >= b


def ohlc(bars):
    return [b[:4] for b in bars]


def none_until(first, count):
    return [(None, {}) for _ in range(min(first, count))]


# --- Case series -----------------------------------------------------------------------------

def swing_bars(n, phase=0.0, drift=0.15, amp=8.0, base=100.0):
    """Closes on two overlaid waves with a drift; the open gaps away from the previous close, and
    the wicks vary in length, so true range, directional movement and ranges all differ from bar
    to bar and the series moves both ways. Rounded to 4 decimals for a readable fixture."""
    bars, prev = [], None
    for i in range(n):
        t = i + phase
        close = round(base + amp * math.sin(t / 6.0) + 3.0 * math.sin(t / 2.3) + drift * i, 4)
        open_ = round(close - 0.2 if prev is None else prev + 0.4 * math.sin(1.7 * t), 4)
        high = round(max(open_, close) + 0.3 + 0.5 * abs(math.sin(0.9 * t)), 4)
        low = round(min(open_, close) - 0.3 - 0.4 * abs(math.cos(1.3 * t)), 4)
        bars.append((open_, high, low, close, 1000.0))
        prev = close
    return bars


def path_bars(closes, wick=0.5):
    """Bars along a close path: open at the previous close, wicks of `wick` beyond the body."""
    bars, prev = [], None
    for c in closes:
        o = c if prev is None else prev
        bars.append((o, max(o, c) + wick, min(o, c) - wick, c, 1000.0))
        prev = c
    return bars


def flat_bars(n, price=100.0, spread=0.0):
    return [(price, price + spread, price - spread, price, 1000.0)] * n


def steps(start, moves):
    out, price = [start], start
    for m in moves:
        price += m
        out.append(price)
    return out


# --- ADX -------------------------------------------------------------------------------------

def adx_case(di_len, adx_smooth, level_weak, bars):
    """Directional movement and TR from the second bar on (golden `directional`), each
    Wilder-averaged over di_len; DI = 100 DM/TR (0 without range), DX = 100 |DI+ - DI-| /
    (DI+ + DI-) (0 for a zero sum), ADX = Wilder average of DX over adx_smooth; signal =
    first-sample EMA(3) of the ADX. Alerts on DI crosses (1.0) and ADX crosses of level_weak
    (threshold strength). First output with bar di_len + adx_smooth."""
    plus, minus, tr = directional(ohlc(bars))
    tp, pp, mp = rma(tr, di_len), rma(plus, di_len), rma(minus, di_len)
    dis, dx = [], []
    for t, p, m in zip(tp, pp, mp):
        if t is None:
            dis.append(None)
            continue
        di_p, di_m = (100.0 * p / t, 100.0 * m / t) if t != 0.0 else (0.0, 0.0)
        dis.append((di_p, di_m))
        dx.append(100.0 * abs(di_p - di_m) / (di_p + di_m) if di_p + di_m != 0.0 else 0.0)
    published_adx = [a for a in rma(dx, adx_smooth) if a is not None]
    signal = ema_first_sample(published_adx, 3)
    first = di_len + adx_smooth - 1  # 0-based bar of the first output
    expected = none_until(first, len(bars))
    prev = None
    for k, (a, s) in enumerate(zip(published_adx, signal)):
        di_p, di_m = dis[first + k - 1]  # change j belongs to bar j + 1
        alerts = {}
        if prev is not None:
            prev_p, prev_m, prev_a = prev
            if crossed_over(prev_p, prev_m, di_p, di_m):
                alerts["bull_di_cross"] = 1.0
            if crossed_under(prev_p, prev_m, di_p, di_m):
                alerts["bear_di_cross"] = 1.0
            if crossed_over(prev_a, level_weak, a, level_weak):
                alerts["adx_activated"] = threshold_strength(a - level_weak, level_weak)
            if crossed_under(prev_a, level_weak, a, level_weak):
                alerts["adx_deactivated"] = threshold_strength(level_weak - a, level_weak)
        prev = (di_p, di_m, a)
        expected.append(((a, {"di_plus": di_p, "di_minus": di_m, "signal": s}), alerts))
    params = {"di_len": float(di_len), "adx_smooth": float(adx_smooth), "level_weak": level_weak}
    return params, bars, expected


# --- DMI -------------------------------------------------------------------------------------

def dmi_case(period, bars):
    """Plain sums over the last `period` changes (golden `_dmi`); value +DI - -DI, extras the two
    lines and their difference; dominance alert when one line leads by more than 10. First output
    with bar period + 1."""
    expected = none_until(period, len(bars))
    for i in range(period, len(bars)):
        diff, di_p, di_m = _dmi(ohlc(bars[:i + 1]), period)
        alerts = {}
        if gt(di_p, di_m + 10.0, "dmi bullish"):
            alerts["dmi_bullish_dominance"] = 0.8
        elif gt(di_m, di_p + 10.0, "dmi bearish"):
            alerts["dmi_bearish_dominance"] = 0.8
        expected.append(((diff, {"plus_di": di_p, "minus_di": di_m, "di_diff": diff}), alerts))
    return {"period": float(period)}, bars, expected


# --- Aroon, Vortex, Choppiness ---------------------------------------------------------------

def aroon_case(period, bars):
    """Golden `_aroon` over the last period + 1 bars; first output with bar period + 1."""
    expected = none_until(period, len(bars))
    for i in range(period, len(bars)):
        osc, up, down = _aroon(bars[:i + 1], period)
        expected.append(((osc, {"aroon_up": up, "aroon_down": down, "oscillator": osc}), {}))
    return {"period": float(period)}, bars, expected


def vortex_case(period, bars):
    """Golden `_vortex`; first output with bar period + 1."""
    expected = none_until(period, len(bars))
    for i in range(period, len(bars)):
        plus, minus = _vortex(bars[:i + 1], period)
        expected.append(((plus, {"vi_plus": plus, "vi_minus": minus}), {}))
    return {"period": float(period)}, bars, expected


def choppiness_case(period, bars):
    """Golden `_choppiness` (true ranges over the whole history, the first high - low, the range
    floored at 1e-8, clamped to 0..100); first output with bar period."""
    expected = none_until(period - 1, len(bars))
    for i in range(period - 1, len(bars)):
        prefix = ohlc(bars[:i + 1])
        # No true range at all: log10(0) is minus infinity, which the clamp takes to 0.
        value = _choppiness(prefix, period) if sum(true_ranges(prefix)[-period:]) > 0.0 else 0.0
        expected.append(((value, {}), {}))
    return {"period": float(period)}, bars, expected


# --- Relative volatility, Ulcer --------------------------------------------------------------

def relative_volatility_case(stdev_len, smooth_len, bars):
    """Close variant (the only one build_checked builds), golden `_relative_volatility`; first
    output with bar stdev_len + smooth_len - 1."""
    values = _relative_volatility([b[3] for b in bars], stdev_len, smooth_len)
    expected = none_until(stdev_len + smooth_len - 2, len(bars)) + [((v, {}), {}) for v in values]
    assert len(expected) == len(bars)
    return {"stdev_len": float(stdev_len), "smooth_len": float(smooth_len)}, bars, expected


def ulcer_case(length, bars):
    """Golden `_ulcer` over the positive closes (first value with the 2 len - 1-th); a
    non-positive close is refused: no output on its bar and the state untouched, so the next
    positive close continues the series."""
    positive = [(i, b[3]) for i, b in enumerate(bars) if b[3] > 0.0]
    values = _ulcer([c for _, c in positive], length)
    expected = [(None, {}) for _ in bars]
    for (i, _), v in zip(positive[2 * length - 2:], values):
        expected[i] = ((v, {}), {})
    return {"len": float(length)}, bars, expected


# --- True range, historical volatility, Garman-Klass, Mass Index ------------------------------

def true_range_case(bars):
    """Golden `true_ranges`: high - low on the first bar, then against the previous close."""
    return {}, bars, [((tr, {}), {}) for tr in true_ranges(ohlc(bars))]


def historical_volatility(closes, period):
    """Sample deviation (divisor period - 1, at least 1) of the last `period` log returns, a
    return with a non-positive price counting 0; times sqrt(252) and 100. (The golden
    `_historical_volatility` has no rule for non-positive prices, hence this version.)"""
    window = closes[-(period + 1):]
    returns = [math.log(b / a) if a > 0.0 and b > 0.0 else 0.0 for a, b in zip(window, window[1:])]
    mean = sum(returns) / len(returns)
    variance = sum((r - mean) ** 2 for r in returns) / max(len(returns) - 1, 1)
    return math.sqrt(variance) * math.sqrt(252.0) * 100.0


def historical_volatility_case(period, bars):
    """First output with bar period + 1."""
    closes = [b[3] for b in bars]
    expected = none_until(period, len(bars))
    for i in range(period, len(bars)):
        expected.append(((historical_volatility(closes[:i + 1], period), {}), {}))
    return {"period": float(period)}, bars, expected


def garman_klass(bars, period):
    """Golden `_garman_klass` over the bars of the window whose prices are all positive; the
    others are skipped but the mean still divides by `period`, so the golden value over the n
    positive bars (which divides by n) is scaled by sqrt(n / period)."""
    positive = [b for b in bars[-period:] if min(b) > 0.0]
    if not positive:
        return 0.0
    return _garman_klass(positive, len(positive)) * math.sqrt(len(positive) / period)


def garman_klass_case(period, bars):
    """First output with bar period."""
    expected = none_until(period - 1, len(bars))
    for i in range(period - 1, len(bars)):
        expected.append(((garman_klass(ohlc(bars[:i + 1]), period), {}), {}))
    return {"period": float(period)}, bars, expected


def mass_index_case(period, bars):
    """Golden `_mass_index` (EMAs from the first bar); first output with bar period."""
    expected = none_until(period - 1, len(bars))
    for i in range(period - 1, len(bars)):
        expected.append(((_mass_index(bars[:i + 1], period), {}), {}))
    return {"period": float(period)}, bars, expected


# --- Chande Kroll, Chandelier Exit -----------------------------------------------------------

def chande_kroll_case(atr_len, stop_len, mult, bars):
    """Golden `_chande_kroll`; value = stop_long; first output with bar atr_len + stop_len - 1."""
    stops = _chande_kroll(ohlc(bars), atr_len, stop_len, mult)
    expected = none_until(atr_len + stop_len - 2, len(bars))
    expected += [((long, {"stop_long": long, "stop_short": short}), {}) for long, short in stops]
    assert len(expected) == len(bars)
    return {"atr_len": float(atr_len), "stop_len": float(stop_len), "mult": mult}, bars, expected


def chandelier_exit_case(length, mult, bars):
    """Wilder ATR over `length` (first TR high - low); raw stops highest(high, length) - mult ATR
    and lowest(low, length) + mult ATR. The long stop keeps max(raw, previous) while the previous
    close was above the previous long stop, the short stop min(raw, previous) while the previous
    close was below the previous short stop. Direction starts long; a close above the previous
    short stop turns it long, else a close below the previous long stop short; a change of
    direction is a flip alert (1.0). value = stop of the direction, secondary = the other one.
    First output with bar `length`."""
    atr = rma(true_ranges(ohlc(bars)), length)
    expected = none_until(length - 1, len(bars))
    long_prev = short_prev = None
    direction = 1
    for i in range(length - 1, len(bars)):
        close = bars[i][3]
        window = bars[i - length + 1:i + 1]
        raw_long = max(b[1] for b in window) - mult * atr[i]
        raw_short = min(b[2] for b in window) + mult * atr[i]
        lp = raw_long if long_prev is None else long_prev
        sp = raw_short if short_prev is None else short_prev
        prev_close = bars[i - 1][3] if i > 0 else None
        long_stop = (max(raw_long, lp) if prev_close is not None and gt(prev_close, lp, "ce ratchet long")
                     else raw_long)
        short_stop = (min(raw_short, sp) if prev_close is not None and gt(sp, prev_close, "ce ratchet short")
                      else raw_short)
        new_direction = direction
        if gt(close, sp, "ce flip long"):
            new_direction = 1
        elif gt(lp, close, "ce flip short"):
            new_direction = -1
        alerts = {}
        if new_direction != direction:
            alerts["chandelier_flip_long" if new_direction == 1 else "chandelier_flip_short"] = 1.0
        direction = new_direction
        long_prev, short_prev = long_stop, short_stop
        value, other = (long_stop, short_stop) if direction == 1 else (short_stop, long_stop)
        expected.append(((value, {"long_stop": long_stop, "short_stop": short_stop,
                                  "secondary": other}), alerts))
    return {"length": float(length), "atr_mult": mult}, bars, expected


# --- Adaptive Chandelier ---------------------------------------------------------------------

def adaptive_chandelier_case(length, atr_mult, body_filter, danger, warn, bars):
    """The registry surface: close extremes, adaptive mode off, so the multiplier is atr_mult.
    atr_raw = Wilder average of the true range over `length`; hi/lo = highest/lowest of the last
    `length` closes; long = hi - mult atr_raw, short = lo + mult atr_raw; while the previous
    direction is long the long stop only rises, while it is short the short stop only falls.
    Against the previous stops: a close above the short stop flips long, below the long stop
    short, if |close - open| >= body_filter atr_raw (a filter of 0 always passes); a crossing that
    fails the filter is a weak flip. dist = |close - previous stop of the current direction| /
    atr_raw; risk state +-1 below danger, +-2 below max(warn, danger + 0.05) or with the close on
    the wrong side of the first-sample EMA(5) of the close (a pullback), +-3 otherwise, positive
    when long. Trap: a wick through that stop with the close back at or inside it. First output
    with bar `length`."""
    closes = [b[3] for b in bars]
    atr = rma(true_ranges(ohlc(bars)), length)
    ema5 = ema_first_sample(closes, 5)
    expected = none_until(length - 1, len(bars))
    prev_long = prev_short = None
    direction = 1
    for i in range(length - 1, len(bars)):
        o, h, l, c, _ = bars[i]
        a = atr[i]
        window = closes[i - length + 1:i + 1]
        long_raw, short_raw = max(window) - atr_mult * a, min(window) + atr_mult * a
        lp = long_raw if prev_long is None else prev_long
        sp = short_raw if prev_short is None else prev_short
        long_stop = max(long_raw, lp) if direction == 1 else long_raw
        short_stop = min(short_raw, sp) if direction == -1 else short_raw
        body_ok = body_filter <= 0.0 or ge(abs(c - o), a * body_filter, "body filter")
        above_short = gt(c, sp, "close vs short stop")
        below_long = gt(lp, c, "close vs long stop")
        old = direction
        if above_short and body_ok:
            direction = 1
        elif below_long and body_ok:
            direction = -1
        alerts = {}
        if direction == 1 and old == -1:
            alerts["bull_flip"] = 1.0
        if direction == -1 and old == 1:
            alerts["bear_flip"] = 1.0
        if direction == -1 and above_short and not body_ok:
            alerts["bull_weak_flip"] = 1.0
        if direction == 1 and below_long and not body_ok:
            alerts["bear_weak_flip"] = 1.0
        trigger = lp if direction == 1 else sp
        dist = abs(c - trigger) / a if a != 0.0 else 0.0
        is_danger = gt(danger, dist, "danger")
        is_warn = not is_danger and gt(max(warn, danger + 0.05), dist, "warn")
        if direction == 1:
            pullback = gt(ema5[i], c, "pullback long")
            state = 1 if is_danger else 2 if (is_warn or pullback) else 3
            if gt(lp, l, "bear trap wick") and ge(c, lp, "bear trap close"):
                alerts["bull_bear_trap"] = clamp(1.0 - dist, 0.0, 1.0)
        else:
            pullback = gt(c, ema5[i], "pullback short")
            state = -1 if is_danger else -2 if (is_warn or pullback) else -3
            if gt(h, sp, "bull trap wick") and ge(sp, c, "bull trap close"):
                alerts["bear_bull_trap"] = clamp(1.0 - dist, 0.0, 1.0)
        prev_long, prev_short = long_stop, short_stop
        value = long_stop if direction == 1 else short_stop
        expected.append(((value, {"long_stop": long_stop, "short_stop": short_stop, "dist_atr": dist,
                                  "multiplier": atr_mult, "risk_state": float(state)}), alerts))
    params = {"length": float(length), "atr_mult": atr_mult, "body_filter_atr": body_filter,
              "danger_dist_atr": danger, "warn_dist_atr": warn}
    return params, bars, expected


# --- VIX Fix ---------------------------------------------------------------------------------

def _band(window, mult):
    """SMA of the window + mult x its population deviation around that SMA."""
    mean = sum(window) / len(window)
    return mean + mult * math.sqrt(sum((v - mean) ** 2 for v in window) / len(window))


def vix_fix_case(pd, bband_len, mult, bars, exact_ties=False):
    """WVF = (highest close - low) / highest close * 100 and inverse = (high - lowest close) /
    high * 100, the extremes over the last pd closes and the current close alone before pd
    closes exist; each band = SMA(bband_len) + mult x population deviation of the last bband_len
    values; first output with bar 2 bband_len - 1. Alerts panic_bottom (WVF at or above its
    band) and blowoff_top (inverse at or above its band), 1.0. value = WVF, secondary = its band."""
    closes = [b[3] for b in bars]
    wvf, inv = [], []
    for i, (_, h, l, c, _) in enumerate(bars):
        window = closes[i - pd + 1:i + 1] if i >= pd - 1 else [c]
        hc, lc = max(window), min(window)
        wvf.append((hc - l) / hc * 100.0 if hc > 0.0 else 0.0)
        inv.append((h - lc) / h * 100.0 if h > 0.0 else 0.0)
    expected = none_until(2 * bband_len - 2, len(bars))
    for i in range(2 * bband_len - 2, len(bars)):
        band = _band(wvf[i - bband_len + 1:i + 1], mult)
        inv_band = _band(inv[i - bband_len + 1:i + 1], mult)
        alerts = {}
        if ge(wvf[i], band, "panic bottom", exact=exact_ties):
            alerts["panic_bottom"] = 1.0
        if ge(inv[i], inv_band, "blowoff top", exact=exact_ties):
            alerts["blowoff_top"] = 1.0
        expected.append(((wvf[i], {"inverse": inv[i], "inverse_band": inv_band, "secondary": band}),
                         alerts))
    return {"pd": float(pd), "bband_len": float(bband_len), "mult": mult}, bars, expected


# --- Efficiency ------------------------------------------------------------------------------

def efficiency_case(length, bars, exact_ties=False):
    """Golden `efficiency_ratio` over the last len + 1 closes (0 without movement); alerts at
    >= 0.65 (high) and <= 0.20 (low). First output with bar len + 1."""
    closes = [b[3] for b in bars]
    expected = none_until(length, len(bars))
    for i in range(length, len(bars)):
        er = efficiency_ratio(closes[:i + 1], length)
        alerts = {}
        if ge(er, 0.65, "er high", exact=exact_ties):
            alerts["high_leg_efficiency"] = 0.85
        elif ge(0.20, er, "er low", exact=exact_ties):
            alerts["low_leg_efficiency"] = 0.70
        expected.append(((er, {}), alerts))
    return {"len": float(length)}, bars, expected


# --- Volatility regime -----------------------------------------------------------------------

def volatility_regime_case(period, bb_mult, kc_mult, bars):
    """Bollinger (SMA basis, population deviation, golden `bollinger`) against a Keltner channel
    with an EMA(period) basis and kc_mult x the plain mean of the last 10 true ranges (golden
    `keltner`). Squeeze (-1) when both Bollinger bands lie inside the channel, else expansion (1)
    when the Bollinger width exceeds 1.3 x the channel width (floored at 1e-8), else normal (0).
    First output with bar max(period, 10)."""
    closes = [b[3] for b in bars]
    first = max(period, 10) - 1
    expected = none_until(first, len(bars))
    for i in range(first, len(bars)):
        bb = bollinger(closes[i - period + 1:i + 1], bb_mult)
        bb_upper, bb_lower = float(bb["upper"]), float(bb["lower"])
        _, kc_upper, kc_lower = keltner(ohlc(bars[:i + 1]), period, 10, kc_mult)
        bb_width = bb_upper - bb_lower
        kc_width = max(kc_upper - kc_lower, 1e-8)
        squeeze = ge(kc_upper, bb_upper, "squeeze upper") and ge(bb_lower, kc_lower, "squeeze lower")
        if squeeze:
            state, alerts = -1.0, {"volatility_squeeze": 0.7}
        elif gt(bb_width, 1.3 * kc_width, "expansion"):
            state, alerts = 1.0, {"volatility_expansion": 0.8}
        else:
            state, alerts = 0.0, {}
        expected.append(((state, {"bb_width": bb_width, "kc_width": kc_width,
                                  "squeeze": 1.0 if squeeze else 0.0}), alerts))
    return {"period": float(period), "bb_mult": bb_mult, "kc_mult": kc_mult}, bars, expected


# --- Rendering -------------------------------------------------------------------------------

def block(name, comment, case):
    """`case_block`, with an expected `secondary` (carried in the extras under that key) written
    as `out<i>_secondary`."""
    params, bars, expected = case
    plain, secondaries = [], []
    for output, alerts in expected:
        secondary = None
        if output is not None and "secondary" in output[1]:
            extra = dict(output[1])
            secondary = extra.pop("secondary")
            output = (output[0], extra)
        plain.append((output, alerts))
        secondaries.append(secondary)
    name, comment, data = case_block(name, comment, params, bars, plain)
    for i, secondary in enumerate(secondaries, start=1):
        if secondary is not None:
            data[f"out{i}_secondary"] = secondary
    return (name, comment, data)


SWING = swing_bars(90)
SWING_SHORT = swing_bars(40, phase=7.0, drift=-0.1)

# ADX with the defaults: swings, then tight chop (the ADX sinks below 20), then a trend (it rises
# above 20 again).
ADX_BARS = swing_bars(40) + path_bars(steps(swing_bars(40)[-1][3], [0.6, -0.6, -0.6, 0.6] * 10 + [0.8, 1.1, 0.6, 1.4] * 8),
                                      0.4)[1:]

# ADX at level 0: flat bars give ADX 0, then a rise lifts it off the zero level, which has no
# scale, so the threshold strength is 1.
ADX_ZERO = flat_bars(8, 100.0, 0.5) + path_bars(steps(100.0, [1.0, 1.5, 0.5, 2.0, 1.0, -0.5, 1.5, 2.0]))[1:]

# Equal highs and equal lows inside the window: the later bar counts.
AROON_TIES = path_bars([100.0, 103.0, 101.0, 103.0, 99.0, 102.0, 99.0, 101.0, 103.0, 100.0, 99.0, 104.0])

# Ulcer: a close of 0 is refused and leaves the state untouched.
_ULCER = path_bars([100.0, 98.0, 101.0, 97.0, 95.0, 99.0, 102.0, 100.0, 96.0, 103.0])
ULCER_GAP = _ULCER[:5] + [(95.0, 95.5, 0.0, 0.0, 1000.0)] + _ULCER[5:]

# Garman-Klass: a bar with a low of 0 is skipped.
GK_GAP = SWING_SHORT[:3] + [(100.0, 101.0, 0.0, 100.5, 1000.0)] + SWING_SHORT[3:9]

# Choppiness: zero-range bars at 100, then at 101. With period 2 the window over the gap has a
# true range of 1 and no span (floored at 1e-8): far above 100 before the clamp. Two bars at 101
# have no true range: minus infinity before the clamp.
CHOP_DEGENERATE = flat_bars(2, 100.0) + flat_bars(3, 101.0)

# Efficiency exactly on the thresholds: |net| 1 over a path of 5 (0.2) and 13 over 20 (0.65),
# integer moves, so both ratios are the correctly rounded doubles of the threshold literals.
ER_TIES = path_bars(steps(100.0, [1.0, 1.0, 1.0, -1.0, -1.0, 4.0, 4.0, 4.0, -3.5, 4.5]))

# VIX Fix: a drift, a crash and a recovery, so both spikes fire.
VIX_PATH = steps(100.0, [0.5, -0.3, 0.6, 0.2, -0.4] * 9
                 + [-3.0, -5.0, -4.0, 1.0, 2.0, 3.0, 4.0, 5.0, 3.0, 1.0, -1.0, 0.5, -0.5, 0.3])
VIX_BARS = path_bars(VIX_PATH, 0.4)

# Volatility regime: chop with wide ranges (squeeze), then a trend (expansion), then chop.
REGIME_PATH = steps(100.0, [0.3, -0.3] * 15 + [1.5] * 12 + [0.2, -0.2] * 8)
REGIME_BARS = [(o, h + 1.5, l - 1.5, c, v) for o, h, l, c, v in path_bars(REGIME_PATH, 0.0)]

# Adaptive Chandelier: a long rise, a crash through the long stop, a fall, then a recovery.
CHFR_PATH = steps(100.0, [1.0, 0.6, -0.4, 1.2, 0.8] * 7 + [-6.0, -8.0, -5.0, -2.0, -3.0, 1.0, -2.0, -1.0]
                  + [-0.5, 0.5] * 4 + [6.0, 7.0, 5.0, 2.0, 3.0, -1.0, 2.0, 1.0, 1.5, -0.5])
CHFR_BARS = path_bars(CHFR_PATH, 0.8)


def build():
    header = provenance("contract_volatility") + [
        "", "Format: see contract_pilot; out<i>_secondary is the secondary output where the",
        "indicator has one."]
    blocks = [
        block("adx_defaults", "ADX with the catalog defaults (14, 14, level 20): swings, chop, trend",
              adx_case(14, 14, 20.0, ADX_BARS)),
        block("adx_short", "ADX(5, 3), level 40 over 40 swing bars", adx_case(5, 3, 40.0, SWING_SHORT)),
        block("adx_zero_level", "ADX(3, 2), level 0: flat bars (ADX 0), then a rise; activation strength 1",
              adx_case(3, 2, 0.0, ADX_ZERO)),
        block("dmi_defaults", "DMI(14) over 90 swing bars", dmi_case(14, SWING)),
        block("dmi_short", "DMI(5) over 40 swing bars", dmi_case(5, SWING_SHORT)),
        block("dmi_flat", "DMI(3) over flat zero-range bars: no range, both lines 0",
              dmi_case(3, flat_bars(6))),
        block("aroon_defaults", "Aroon(14) over 90 swing bars", aroon_case(14, SWING)),
        block("aroon_ties", "Aroon(4): equal highs and lows in the window, the later one counts",
              aroon_case(4, AROON_TIES)),
        block("vortex_defaults", "Vortex(14) over 90 swing bars", vortex_case(14, SWING)),
        block("vortex_short", "Vortex(3) over 40 swing bars", vortex_case(3, SWING_SHORT)),
        block("vortex_flat", "Vortex(2) over zero-range bars: TR sum floored at 1e-8, both 0",
              vortex_case(2, flat_bars(5))),
        block("choppiness_defaults", "Choppiness(14) over 90 swing bars", choppiness_case(14, SWING)),
        block("choppiness_short", "Choppiness(2), the smallest period, over 40 swing bars",
              choppiness_case(2, SWING_SHORT)),
        block("choppiness_degenerate", "Choppiness(2) over zero-range bars: a gap into a zero span (clamped "
              "to 100), then no true range at all (clamped to 0)", choppiness_case(2, CHOP_DEGENERATE)),
        block("relative_volatility_defaults", "Relative Volatility Index (10, 14), close, over 90 swing bars",
              relative_volatility_case(10, 14, SWING)),
        block("relative_volatility_short", "Relative Volatility Index (5, 4) over 40 swing bars",
              relative_volatility_case(5, 4, SWING_SHORT)),
        block("relative_volatility_flat", "Relative Volatility Index (3, 2) over flat closes: 50",
              relative_volatility_case(3, 2, flat_bars(8, 100.0, 1.0))),
        block("ulcer_index_defaults", "Ulcer Index (14) over 90 swing bars", ulcer_case(14, SWING)),
        block("ulcer_index_short", "Ulcer Index (3) over 40 swing bars", ulcer_case(3, SWING_SHORT)),
        block("ulcer_index_single", "Ulcer Index (1): the running high is the value itself, always 0",
              ulcer_case(1, SWING_SHORT[:6])),
        block("ulcer_index_zero_close", "Ulcer Index (2): a close of 0 on bar 6 is refused, no output there",
              ulcer_case(2, ULCER_GAP)),
        block("true_range_swing", "True range over 40 swing bars with gaps", true_range_case(SWING_SHORT)),
        block("historical_volatility_defaults", "Historical volatility (20) over 90 swing bars",
              historical_volatility_case(20, SWING)),
        block("historical_volatility_short", "Historical volatility (5) over 40 swing bars",
              historical_volatility_case(5, SWING_SHORT)),
        block("historical_volatility_single", "Historical volatility (1): one return, divisor 1, always 0",
              historical_volatility_case(1, SWING_SHORT[:5])),
        block("historical_volatility_zero_close",
              "Historical volatility (3): a close of 0 on bar 6, both returns touching it count 0",
              historical_volatility_case(3, ULCER_GAP)),
        block("garman_klass_defaults", "Garman-Klass (20) over 90 swing bars", garman_klass_case(20, SWING)),
        block("garman_klass_nonpositive", "Garman-Klass (3): bar 4 has a low of 0 and is skipped while in the window",
              garman_klass_case(3, GK_GAP)),
        block("garman_klass_short", "Garman-Klass (3) over 40 swing bars", garman_klass_case(3, SWING_SHORT)),
        block("mass_index_defaults", "Mass Index (25) over 90 swing bars", mass_index_case(25, SWING)),
        block("mass_index_short", "Mass Index (5) over 40 swing bars", mass_index_case(5, SWING_SHORT)),
        block("mass_index_constant", "Mass Index (4) over a constant range: exactly 4",
              mass_index_case(4, flat_bars(7, 100.0, 1.0))),
        block("chande_kroll_defaults", "Chande Kroll Stop (10, 9, 3) over 90 swing bars",
              chande_kroll_case(10, 9, 3.0, SWING)),
        block("chande_kroll_short", "Chande Kroll Stop (4, 3, 2) over 40 swing bars",
              chande_kroll_case(4, 3, 2.0, SWING_SHORT)),
        block("chande_kroll_single", "Chande Kroll Stop (3, 1, 1.5): the second stage passes the first through",
              chande_kroll_case(3, 1, 1.5, SWING_SHORT[:12])),
        block("chandelier_exit_defaults", "Chandelier Exit (22, 3) over 90 swing bars",
              chandelier_exit_case(22, 3.0, SWING)),
        block("chandelier_exit_short", "Chandelier Exit (5, 1.5) over 40 swing bars",
              chandelier_exit_case(5, 1.5, SWING_SHORT)),
        block("adaptive_chandelier_defaults",
              "Adaptive Chandelier with the catalog defaults over a rise, a crash and a recovery",
              adaptive_chandelier_case(30, 4.5, 0.8, 0.35, 0.75, CHFR_BARS)),
        block("adaptive_chandelier_short", "Adaptive Chandelier (5, 2) without body filter over 40 swing bars",
              adaptive_chandelier_case(5, 2.0, 0.0, 0.5, 1.0, SWING_SHORT)),
        block("adaptive_chandelier_body",
              "Adaptive Chandelier (5, 1.5), body filter 0.8 over 40 swing bars: weak flips both ways",
              adaptive_chandelier_case(5, 1.5, 0.8, 0.35, 0.75, SWING_SHORT)),
        block("vix_fix_defaults", "VIX Fix (22, 20, 2) over a drift, a crash and a recovery",
              vix_fix_case(22, 20, 2.0, VIX_BARS)),
        block("vix_fix_short", "VIX Fix (5, 4, 1) over 40 swing bars", vix_fix_case(5, 4, 1.0, SWING_SHORT)),
        block("vix_fix_flat", "VIX Fix (3, 2, 2) over bars with high = low = close: everything 0, "
              "both values exactly on their bands, both alerts",
              vix_fix_case(3, 2, 2.0, flat_bars(6), exact_ties=True)),
        block("efficiency_defaults", "Efficiency ratio (14) over 90 swing bars", efficiency_case(14, SWING)),
        block("efficiency_ties", "Efficiency ratio (5): exactly 0.2 and exactly 0.65 among the values",
              efficiency_case(5, ER_TIES, exact_ties=True)),
        block("efficiency_flat", "Efficiency ratio (3) over flat closes: 0, a low-efficiency alert",
              efficiency_case(3, flat_bars(6, 100.0, 1.0))),
        block("volatility_regime_defaults", "Volatility regime (20, 2, 1.5): chop, trend, chop",
              volatility_regime_case(20, 2.0, 1.5, REGIME_BARS)),
        block("volatility_regime_short", "Volatility regime (5, 2, 1): period below the fixed ATR window 10",
              volatility_regime_case(5, 2.0, 1.0, SWING_SHORT)),
    ]
    return header, blocks
