"""contract_moving_averages: complete per-bar contracts of the moving averages family
(plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code. Where an independent implementation already exists
in this package (`smoothing`, `indicators`, `exact`, the golden modules `moving_averages` and
`trend`), it is imported, not repeated.

Conventions the type documentation leaves open, assumed here and stated per indicator below:

- Alert kinds and strengths of Donchian, Keltner, Ichimoku, Supertrend, Parabolic SAR and MIDAS
  are not described on their types. The cases assume the reading their kind names give (stated
  at each function); a deviation there is a gap in the documentation as much as in the code.
- MIDAS `extra["bars_since_extreme"]` is not listed on the type; assumed present on every output
  as the number of bars since the running extreme was last set.
- Rolling VWAP: `slope` uses the k-th previously *published* VWAP (a bar without output adds no
  history value).
- Supertrend: on the first output bar the final bands are the basic bands (prices positive).
- Every comparison that decides an alert or a branch is asserted to clear a margin of
  1e-7 x scale (unless the tie is structural and stated), so float rounding on either side
  cannot flip it.
"""

import math
from fractions import Fraction

from ..exact import bollinger as exact_bollinger
from ..fixture import provenance
from ..indicators import keltner as keltner_last
from ..indicators import true_ranges
from ..smoothing import ema_first_sample, ema_published, least_squares, sma, wma
from .contract_pilot import case_block as pilot_case_block
from .contract_pilot import wave
from .moving_averages import _dema, _hma, _kama, _mcginley, _t3, _tema, _vidya, _vwma
from .trend import _alligator, _midpoint

NAME = "contract_moving_averages"

TOLERANCE = 1e-9
MARGIN = 1e-7


# --- Helpers ---------------------------------------------------------------------------------

def _clear(a, b):
    """a and b are far enough apart that rounding cannot change their order."""
    assert abs(a - b) > MARGIN * max(1.0, abs(a), abs(b)), (a, b)


def gt(a, b):
    _clear(a, b)
    return a > b


def lt(a, b):
    _clear(a, b)
    return a < b


def case_block(name, comment, params, bars, expected):
    """`contract_pilot.case_block`, plus `out<i>_secondary` for outputs given as
    (value, extra, secondary)."""
    plain, secondaries = [], {}
    for i, (output, alerts) in enumerate(expected, start=1):
        if output is not None and len(output) == 3:
            secondaries[i] = output[2]
            output = output[:2]
        plain.append((output, alerts))
    block = pilot_case_block(name, comment, params, bars, plain)
    for i, secondary in secondaries.items():
        block[2][f"out{i}_secondary"] = secondary
    return block


BAR_UP = [0.4, 0.9, 0.2, 0.6, 1.1]
BAR_DOWN = [0.5, 0.3, 0.8, 0.2]
VOLUMES = [1000.0, 1500.0, 800.0, 1200.0, 2000.0, 600.0, 1700.0]


def ohlc(closes, volumes=None):
    """Bars around the closes: open at the previous close (the first 0.3 below), high and low
    beyond the body by uneven amounts, so a wrong price field shows; volume cycles unless given."""
    bars, previous = [], None
    for i, close in enumerate(closes):
        open_ = close - 0.3 if previous is None else previous
        high = max(open_, close) + BAR_UP[i % len(BAR_UP)]
        low = min(open_, close) - BAR_DOWN[i % len(BAR_DOWN)]
        volume = VOLUMES[i % len(VOLUMES)] if volumes is None else volumes[i]
        bars.append((open_, high, low, close, volume))
        previous = close
    return bars


def closes_of(bars):
    return [b[3] for b in bars]


def value_series(values):
    """Expected rows from a per-bar list of values (None = no output), no extras or alerts."""
    return [(None, {}) if v is None else ((v, {}), {}) for v in values]


def aligned(n, first, values):
    """A per-bar list of length n: None before the 0-based bar `first`, then `values`."""
    assert first + len(values) == n, (first, len(values), n)
    return [None] * first + list(values)


# --- Plain averages over the close --------------------------------------------------------------

def ema_values(closes, period):
    """First-sample-seeded EMA, alpha 2/(period+1), published from the period-th bar."""
    return ema_published(closes, period)


def wma_values(closes, period):
    return [None if i + 1 < period else wma(closes[i + 1 - period:i + 1])
            for i in range(len(closes))]


def vwma_values(bars, period):
    """sum(close * volume) / sum(volume) over the last `period` bars; the current close when the
    window has no volume."""
    closes, volumes = closes_of(bars), [b[4] for b in bars]
    return [None if i + 1 < period else _vwma(closes[:i + 1], volumes[:i + 1], period)
            for i in range(len(bars))]


def hma_values(closes, period):
    root = max(math.floor(math.sqrt(period) + 0.5), 1)
    return aligned(len(closes), period + root - 2, _hma(closes, period))


def dema_values(closes, period):
    return aligned(len(closes), 2 * period - 2, _dema(closes, period))


def tema_values(closes, period):
    return aligned(len(closes), period - 1, _tema(closes, period))


def kama_values(closes, period, fast, slow):
    return aligned(len(closes), period, _kama(closes, period, fast, slow))


def mcginley_values(closes, period):
    return _mcginley(closes, period)


def t3_values(closes, period, v):
    return aligned(len(closes), period - 1, _t3(closes, period, v))


def vidya_values(closes, cmo_len, ema_len):
    values, _, _ = _vidya(closes, cmo_len, ema_len)
    return aligned(len(closes), cmo_len, values)


# --- LSMA -------------------------------------------------------------------------------------

def lsma_expected(closes, period):
    """Least-squares fit over x = 0 .. period-1 of the last `period` closes: value = fitted price
    at the newest bar; slope, intercept (at the oldest bar of the window) and R^2 (1 for a window
    without variance) as extras."""
    out = []
    for i in range(len(closes)):
        if i + 1 < period:
            out.append((None, {}))
            continue
        slope, intercept, r2, value = least_squares(closes[i + 1 - period:i + 1])
        out.append(((float(value), {"slope": float(slope), "intercept": float(intercept),
                                     "r2": float(r2)}), {}))
    return out


# --- Alligator --------------------------------------------------------------------------------

def alligator_expected(bars):
    """Jaw/teeth/lips Wilder averages of hl2 over 13/8/5, shifted 8/5/3 outputs through windows
    that fill from the first output on (`trend._alligator` applied to each prefix)."""
    out = []
    for i in range(len(bars)):
        if i < 12:
            out.append((None, {}))
            continue
        lines = _alligator(bars[:i + 1])
        out.append(((lines["lips"], dict(lines)), {}))
    return out


# --- Envelope ---------------------------------------------------------------------------------

def envelope_expected(closes, period, percent):
    """basis = SMA(period); bands basis +/- basis * percent / 100."""
    out = []
    for i in range(len(closes)):
        if i + 1 < period:
            out.append((None, {}))
            continue
        basis = sma(closes[i + 1 - period:i + 1])
        margin = basis * (percent / 100.0)
        out.append(((basis, {"basis": basis, "upper": basis + margin, "lower": basis - margin}),
                    {}))
    return out


# --- TWAP -------------------------------------------------------------------------------------

BAR_SECONDS = 900  # the contract loader's spacing: bar i (1-based) opens at i * 900


def twap_expected(closes, offset):
    """Per-bar mean of the closes since the start of the current UTC day shifted by `offset`
    seconds: day = floor((timestamp - offset) / 86400); a new day restarts the mean."""
    out, total, count, day = [], 0.0, 0, None
    for i, close in enumerate(closes, start=1):
        this_day = (i * BAR_SECONDS - int(offset)) // 86_400
        if this_day != day:
            day, total, count = this_day, 0.0, 0
        total += close
        count += 1
        out.append(((total / count, {}), {}))
    return out


# --- Rolling VWAP -----------------------------------------------------------------------------

def vwap_expected(bars, window, slope_lookback):
    """Over the last `window` bars: VWAP = sum(tp v) / sum(v), tp = (h + l + c)/3, no output while
    the window has no volume; sigma = sqrt(sum(v (tp - VWAP)^2) / sum(v)), bands at 1 and 2 sigma;
    slope = (VWAP - VWAP k published values back) / k once k + 1 values exist."""
    out, history = [], []
    for i in range(len(bars)):
        win = bars[max(0, i + 1 - window):i + 1]
        tps = [(b[1] + b[2] + b[3]) / 3.0 for b in win]
        vols = [b[4] for b in win]
        vol_sum = sum(vols)
        if vol_sum <= 0.0:
            out.append((None, {}))
            continue
        vwap = sum(t * v for t, v in zip(tps, vols)) / vol_sum
        sigma = math.sqrt(sum(v * (t - vwap) ** 2 for t, v in zip(tps, vols) if v > 0) / vol_sum)
        history.append(vwap)
        extra = {"sigma": sigma, "upper_1sigma": vwap + sigma, "lower_1sigma": vwap - sigma,
                 "upper_2sigma": vwap + 2.0 * sigma, "lower_2sigma": vwap - 2.0 * sigma}
        if len(history) > slope_lookback:
            extra["slope"] = (vwap - history[-1 - slope_lookback]) / slope_lookback
        out.append(((vwap, extra), {}))
    return out


# --- Anchored VWAP ----------------------------------------------------------------------------

def avwap_bars(closes, volumes):
    """high = close + 4, low = close - 1: the typical price is close + 1, and with integer closes
    and volumes every sum the engine forms is an exact binary number, so the variance
    E[tp^2] - VWAP^2 suffers no cancellation (it is exactly 0 on a fresh anchor)."""
    bars, previous = [], None
    for close, volume in zip(closes, volumes):
        open_ = close if previous is None else previous
        bars.append((open_, close + 4.0, close - 1.0, close, volume))
        previous = close
    return bars


def avwap_expected(bars, mult1, mult2):
    """Session anchor with the default config (00:00-00:00 UTC, the whole day): the accumulation
    restarts with the first bar of each UTC day. From the anchor on VWAP = sum(tp v) / sum(v),
    stddev = sqrt(sum(tp^2 v)/sum(v) - VWAP^2) floored at 0, computed exactly; a bar without
    volume counts with weight 1."""
    out, day = [], None
    pv = vol = pv2 = Fraction(0)
    for i, (_, high, low, close, volume) in enumerate(bars, start=1):
        this_day = (i * BAR_SECONDS) // 86_400
        if this_day != day:
            day, pv, vol, pv2 = this_day, Fraction(0), Fraction(0), Fraction(0)
        tp = (Fraction(high) + Fraction(low) + Fraction(close)) / 3
        weight = Fraction(volume) if volume > 0 else Fraction(1)
        pv += tp * weight
        vol += weight
        pv2 += tp * tp * weight
        vwap = pv / vol
        variance = max(pv2 / vol - vwap * vwap, Fraction(0))
        sd = math.sqrt(variance)
        v = float(vwap)
        out.append(((v, {"vwap": v, "stddev": sd,
                         "band1_upper": v + mult1 * sd, "band1_lower": v - mult1 * sd,
                         "band2_upper": v + mult2 * sd, "band2_lower": v - mult2 * sd}), {}))
    return out


# --- MIDAS ------------------------------------------------------------------------------------

def midas_expected(bars, maturity):
    """Topfinder over hlc3. curve = cum(p v) / cum(v); a new extreme is a high above the running
    one (the first bar sets it). From the first bar after the latest extreme the projection is
    extreme - (extreme - curve at extreme) sqrt(cum v at extreme / cum v). value = curve,
    secondary = projection or the curve, extra projection while it exists and
    bars_since_extreme always.
    Alerts (assumed from their kinds): `midas_extreme_reset` 0.6 on a new extreme that follows at
    least one bar without one; `midas_exhausted` 0.5 on the bar where maturity bars have passed
    since the extreme."""
    out = []
    cum_pv = cum_v = 0.0
    extreme = extreme_v = extreme_curve = None
    since = 0
    for _, high, low, close, volume in bars:
        price = (high + low + close) / 3.0
        cum_pv += price * volume
        cum_v += volume
        assert cum_v > 0.0
        curve = cum_pv / cum_v
        alerts = {}
        if extreme is None or gt(high, extreme):
            if since > 0:
                alerts["midas_extreme_reset"] = 0.6
            extreme, extreme_v, extreme_curve, since = high, cum_v, curve, 0
        else:
            since += 1
        extra = {"bars_since_extreme": float(since)}
        secondary = curve
        if since > 0:
            secondary = extreme - (extreme - extreme_curve) * math.sqrt(extreme_v / cum_v)
            extra["projection"] = secondary
            if since == maturity:
                alerts["midas_exhausted"] = 0.5
        out.append(((curve, extra, secondary), alerts))
    return out


def midas_bars():
    """Rally to a high, a pullback longer than the short maturity, a new high, then a drift."""
    closes = ([100.0, 101.0, 102.5, 103.0, 104.5, 105.0]
              + [104.0, 103.2, 103.8, 102.9, 102.0, 102.6, 101.8]
              + [103.5, 105.5, 106.8]
              + [106.0, 105.4, 105.9, 104.8, 105.2])
    return ohlc(closes)


# --- Trend relationship ----------------------------------------------------------------------

def trend_relationship_expected(closes, fast_len, slow_len):
    """EMA(fast) and EMA(slow) of the close, both first-sample-seeded from the first bar: value
    fast - slow, secondary slow, extras fast and slow. A cross alert (strength 1) fires when the
    strict relation (fast above / below slow) differs from the last strict one; equal lines have
    no relation (on the first bar both are the first close)."""
    fast_line, slow_line = ema_first_sample(closes, fast_len), ema_first_sample(closes, slow_len)
    out, previous = [], None
    for i, (fast, slow) in enumerate(zip(fast_line, slow_line)):
        if i == 0:
            assert fast == slow
            above = None
        else:
            above = gt(fast, slow)
        alerts = {}
        if previous is not None and above is not None and above != previous:
            kind = "cross_up" if above else "cross_down"
            alerts[f"trend_relationship_{kind}"] = 1.0
        if above is not None:
            previous = above
        out.append(((fast - slow, {"fast": fast, "slow": slow}, slow), alerts))
    return out


# --- Ichimoku ---------------------------------------------------------------------------------

def ichimoku_expected(bars, tenkan_p, kijun_p, senkou_b_p):
    """Midpoints of highest high and lowest low over tenkan_p, kijun_p, senkou_b_p bars, senkou_a
    = (tenkan + kijun)/2, all unshifted; value tenkan - kijun; from the senkou_b_p-th bar.
    Alerts (assumed from their kinds), strength 0.85: `ichimoku_above_cloud` when the close is
    above both spans, `ichimoku_below_cloud` when below both."""
    out = []
    for i in range(len(bars)):
        if i + 1 < senkou_b_p:
            out.append((None, {}))
            continue
        tenkan = _midpoint(bars[i + 1 - tenkan_p:i + 1])
        kijun = _midpoint(bars[i + 1 - kijun_p:i + 1])
        senkou_b = _midpoint(bars[i + 1 - senkou_b_p:i + 1])
        senkou_a = (tenkan + kijun) / 2.0
        close = bars[i][3]
        alerts = {}
        a_above, b_above = gt(close, senkou_a), gt(close, senkou_b)
        if a_above and b_above:
            alerts["ichimoku_above_cloud"] = 0.85
        elif not a_above and not b_above:
            alerts["ichimoku_below_cloud"] = 0.85
        out.append(((tenkan - kijun, {"tenkan": tenkan, "kijun": kijun, "senkou_a": senkou_a,
                                      "senkou_b": senkou_b}), alerts))
    return out


# --- Supertrend -------------------------------------------------------------------------------

def supertrend_expected(bars, period, mult):
    """ATR = plain mean of the last `period` true ranges; basic bands hl2 +/- mult ATR. The final
    upper band takes the basic one when that is lower or the previous close was above the old
    final band, the lower one when higher or the previous close was below; on the first output
    the final bands are the basic bands. The trend starts up and flips down on a close below the
    final lower band, up on a close above the final upper band. value = lower band in an uptrend,
    upper band in a downtrend; extras trend (1/-1), upper, lower.
    Alerts (assumed from their kinds), strength 0.9: `supertrend_bullish` / `supertrend_bearish`
    on the bar of a flip."""
    tr = true_ranges([b[:4] for b in bars])
    out, upper, lower, trend = [], None, None, 1
    for i, (_, high, low, close, _) in enumerate(bars):
        if i + 1 < period:
            out.append((None, {}))
            continue
        atr = sum(tr[i + 1 - period:i + 1]) / period
        hl2 = (high + low) / 2.0
        basic_upper, basic_lower = hl2 + mult * atr, hl2 - mult * atr
        assert basic_lower > 0.0
        if upper is None:
            upper, lower = basic_upper, basic_lower
        else:
            prev_close = bars[i - 1][3]
            # Where the basic band equals the old final band, both branches give the same band.
            if basic_upper != upper and (lt(basic_upper, upper) or gt(prev_close, upper)):
                upper = basic_upper
            if basic_lower != lower and (gt(basic_lower, lower) or lt(prev_close, lower)):
                lower = basic_lower
        alerts = {}
        if trend == 1 and lt(close, lower):
            trend = -1
            alerts["supertrend_bearish"] = 0.9
        elif trend == -1 and gt(close, upper):
            trend = 1
            alerts["supertrend_bullish"] = 0.9
        value = lower if trend == 1 else upper
        out.append(((value, {"trend": float(trend), "upper": upper, "lower": lower}), alerts))
    return out


# --- Parabolic SAR ----------------------------------------------------------------------------

def psar_expected(bars, step, max_step):
    """Starts long on the first bar with SAR = low, EP = high, AF = step and publishes that SAR.
    Then next = SAR + AF (EP - SAR). Long: a low below `next` reverses (SAR = EP, EP = low,
    AF = step); otherwise a new high moves EP and raises AF by step up to max_step, and the SAR is
    held at or below the previous and the current low. Short mirrors that with highs. extras
    is_long (1/0) and af on every output, the first included (the type lists them for `value`
    without exception).
    Alerts (assumed from their kinds), strength 0.85: `psar_reversal_bearish` /
    `psar_reversal_bullish` on the bar of a reversal."""
    out = []
    first = bars[0]
    sar, ep, af, is_long = first[2], first[1], step, True
    out.append(((sar, {"is_long": 1.0, "af": af}), {}))
    for prev, bar in zip(bars, bars[1:]):
        _, high, low, _, _ = bar
        nxt = sar + af * (ep - sar)
        alerts = {}
        if is_long:
            if lt(low, nxt):
                is_long, nxt, ep, af = False, ep, low, step
                alerts["psar_reversal_bearish"] = 0.85
            else:
                if gt(high, ep):
                    ep, af = high, min(af + step, max_step)
                nxt = min(nxt, prev[2], low)
        else:
            if gt(high, nxt):
                is_long, nxt, ep, af = True, ep, high, step
                alerts["psar_reversal_bullish"] = 0.85
            else:
                if lt(low, ep):
                    ep, af = low, min(af + step, max_step)
                nxt = max(nxt, prev[1], high)
        sar = nxt
        out.append(((sar, {"is_long": 1.0 if is_long else 0.0, "af": af}), alerts))
    return out


# --- Donchian ---------------------------------------------------------------------------------

def donchian_expected(bars, period):
    """upper = highest high, lower = lowest low of the last `period` bars; value = midpoint,
    width = upper - lower. Alerts (assumed from their kinds), strength 0.85:
    `donchian_new_high` when the bar's own high is the window's highest high, otherwise
    `donchian_new_low` when its low is the lowest low. The cases contain no bar that is both."""
    out = []
    for i in range(len(bars)):
        if i + 1 < period:
            out.append((None, {}))
            continue
        window = bars[i + 1 - period:i + 1]
        upper, lower = max(b[1] for b in window), min(b[2] for b in window)
        high, low = bars[i][1], bars[i][2]
        alerts = {}
        is_high, is_low = high == upper, low == lower
        assert not (is_high and is_low)
        if is_high:
            alerts["donchian_new_high"] = 0.85
        elif is_low:
            alerts["donchian_new_low"] = 0.85
        out.append((((upper + lower) / 2.0, {"upper": upper, "lower": lower,
                                             "width": upper - lower}), alerts))
    return out


# --- Keltner ----------------------------------------------------------------------------------

def keltner_expected(bars, ema_period, atr_period, mult):
    """`indicators.keltner` on every prefix: basis = first-sample EMA of the close from the first
    bar, ATR = plain mean of the last atr_period true ranges, bands basis +/- mult ATR; output from
    the atr_period-th bar. Alerts (assumed from their kinds), strength 0.8:
    `keltner_upper_breakout` on a close above the upper band, `keltner_lower_breakout` below the
    lower."""
    out = []
    tr = true_ranges([b[:4] for b in bars])
    for i in range(len(bars)):
        if i + 1 < atr_period:
            out.append((None, {}))
            continue
        basis, upper, lower = keltner_last([b[:4] for b in bars[:i + 1]], ema_period, atr_period,
                                           mult)
        atr = sum(tr[i + 1 - atr_period:i + 1]) / atr_period
        close = bars[i][3]
        alerts = {}
        if gt(close, upper):
            alerts["keltner_upper_breakout"] = 0.8
        elif lt(close, lower):
            alerts["keltner_lower_breakout"] = 0.8
        out.append(((basis, {"upper": upper, "lower": lower, "atr": atr}), alerts))
    return out


# --- Bollinger --------------------------------------------------------------------------------

def bollinger_expected(closes, length, mult, flat_ok=False):
    """`exact.bollinger` over every full window (population divisor). Alerts `lower_touch` /
    `upper_touch` (strength 1) when the close is at or beyond the band — both on a degenerate
    band, which only the flat case has."""
    out = []
    for i in range(len(closes)):
        if i + 1 < length:
            out.append((None, {}))
            continue
        window = closes[i + 1 - length:i + 1]
        bands = {k: float(v) for k, v in exact_bollinger(window, mult).items()}
        close = closes[i]
        alerts = {}
        if bands["upper"] == bands["lower"]:
            assert flat_ok
            alerts = {"lower_touch": 1.0, "upper_touch": 1.0}
        else:
            if not gt(close, bands["lower"]):
                alerts["lower_touch"] = 1.0
            if not lt(close, bands["upper"]):
                alerts["upper_touch"] = 1.0
        out.append(((bands["basis"], bands), alerts))
    return out


# --- Cases ------------------------------------------------------------------------------------

def params(**kw):
    return {k: float(v) for k, v in kw.items()}


def simple(name, comment, period, closes, values):
    return case_block(name, comment, {"period": float(period)}, ohlc(closes),
                      value_series(values))


LONG = wave(45, 100.0, [1.2, -0.7, 0.9, 1.5, -1.1, 0.4, -0.6, 1.0, -1.4, 0.8, 0.3])
SHORT = wave(14, 20.0, [0.5, -0.25, 1.0, -0.75, 0.25])
FLAT_THEN_MOVE = [50.0] * 8 + [51.0, 52.5, 51.5, 53.0]
UP_DOWN_UP = (wave(20, 100.0, [1.4, 0.8, -0.5, 1.1, 0.6])
              + wave(20, 112.0, [-1.3, -0.7, 0.4, -1.2, -0.5])
              + wave(20, 96.0, [1.2, 0.9, -0.3, 1.0, 0.5]))
WIGGLE = wave(40, 50.0, [0.8, -1.1, 0.5, 1.3, -0.9, -0.4, 1.0, -1.2, 0.6, 0.2])


def build_blocks():
    blocks = []
    add = blocks.append

    # EMA
    add(simple("ema_defaults", "EMA(20), first-sample seed, published from bar 20", 20, LONG,
               ema_values(LONG, 20)))
    add(simple("ema_short", "EMA(4)", 4, SHORT, ema_values(SHORT, 4)))
    add(simple("ema_unit", "EMA(1) is the close itself", 1, SHORT[:5], ema_values(SHORT[:5], 1)))

    # WMA
    add(simple("wma_defaults", "WMA(20)", 20, LONG[:30], wma_values(LONG[:30], 20)))
    add(simple("wma_short", "WMA(4)", 4, SHORT, wma_values(SHORT, 4)))

    # VWMA
    long_bars = ohlc(LONG[:30])
    add(case_block("vwma_defaults", "VWMA(20) with varying volume", params(period=20), long_bars,
                   value_series(vwma_values(long_bars, 20))))
    vols = [1000.0, 3000.0, 500.0, 0.0, 0.0, 0.0, 2000.0, 100.0, 0.0, 4000.0]
    zero_bars = ohlc(SHORT[:10], vols)
    add(case_block("vwma_zerovolume", "VWMA(3): bar 6 has a window without volume -> its close",
                   params(period=3), zero_bars, value_series(vwma_values(zero_bars, 3))))

    # HMA
    add(simple("hma_defaults", "HMA(20): half 10, root 4, first output bar 23", 20, LONG[:35],
               hma_values(LONG[:35], 20)))
    add(simple("hma_seven", "HMA(7): half 3, root round(2.646) = 3, first output bar 9", 7, SHORT,
               hma_values(SHORT, 7)))
    add(simple("hma_unit", "HMA(1) = 2 close - close = close", 1, SHORT[:4],
               hma_values(SHORT[:4], 1)))

    # DEMA
    add(simple("dema_defaults", "DEMA(20): second stage over published e1, first output bar 39",
               20, LONG, dema_values(LONG, 20)))
    add(simple("dema_short", "DEMA(3), first output bar 5", 3, SHORT, dema_values(SHORT, 3)))

    # TEMA
    add(simple("tema_defaults", "TEMA(14), all stages from the first close", 14, LONG[:25],
               tema_values(LONG[:25], 14)))
    add(simple("tema_short", "TEMA(4)", 4, SHORT, tema_values(SHORT, 4)))

    # KAMA
    add(case_block("kama_defaults", "KAMA(10, 2, 30)",
                   params(period=10, fast_period=2, slow_period=30),
                   ohlc(LONG[:25]), value_series(kama_values(LONG[:25], 10, 2, 30))))
    add(case_block("kama_custom", "KAMA(5, 3, 20)", params(period=5, fast_period=3, slow_period=20),
                   ohlc(SHORT), value_series(kama_values(SHORT, 5, 3, 20))))
    add(case_block("kama_flat", "KAMA(4, 2, 30): no movement -> ER 0, slow constant",
                   params(period=4, fast_period=2, slow_period=30), ohlc(FLAT_THEN_MOVE),
                   value_series(kama_values(FLAT_THEN_MOVE, 4, 2, 30))))

    # LSMA
    add(case_block("lsma_defaults", "LSMA(25): endpoint, slope, intercept, R^2", params(period=25),
                   ohlc(LONG[:32]), lsma_expected(LONG[:32], 25)))
    add(case_block("lsma_minimum", "LSMA(2), the smallest period: the line through two closes",
                   params(period=2), ohlc(SHORT[:6]), lsma_expected(SHORT[:6], 2)))
    add(case_block("lsma_flat", "LSMA(4) over a flat start: R^2 = 1 by convention",
                   params(period=4), ohlc(FLAT_THEN_MOVE), lsma_expected(FLAT_THEN_MOVE, 4)))

    # McGinley
    add(simple("mcginley_defaults", "McGinley(14), from the first close", 14, LONG[:25],
               mcginley_values(LONG[:25], 14)))
    add(simple("mcginley_short", "McGinley(3)", 3, SHORT, mcginley_values(SHORT, 3)))

    # T3
    add(case_block("t3_defaults", "T3(5, v 0.7)", params(period=5, v=0.7), ohlc(LONG[:20]),
                   value_series(t3_values(LONG[:20], 5, 0.7))))
    add(case_block("t3_vzero", "T3(3, v 0) is the third EMA stage", params(period=3, v=0.0),
                   ohlc(SHORT), value_series(t3_values(SHORT, 3, 0.0))))
    add(case_block("t3_vone", "T3(4, v 1)", params(period=4, v=1.0), ohlc(SHORT),
                   value_series(t3_values(SHORT, 4, 1.0))))

    # VIDYA
    add(case_block("vidya_defaults", "VIDYA(cmo 9, ema 12)", params(cmo_len=9, ema_len=12),
                   ohlc(LONG[:25]), value_series(vidya_values(LONG[:25], 9, 12))))
    add(case_block("vidya_custom", "VIDYA(cmo 3, ema 5)", params(cmo_len=3, ema_len=5),
                   ohlc(SHORT), value_series(vidya_values(SHORT, 3, 5))))
    add(case_block("vidya_flat", "VIDYA(3, 4): a flat stretch has CMO 0 and holds the line",
                   params(cmo_len=3, ema_len=4), ohlc(FLAT_THEN_MOVE),
                   value_series(vidya_values(FLAT_THEN_MOVE, 3, 4))))

    # Alligator
    gator_bars = ohlc(LONG[:30])
    add(case_block("alligator_defaults", "Alligator (13/8/5, shifted 8/5/3): shift windows fill",
                   {}, gator_bars, alligator_expected(gator_bars)))

    # Envelope
    add(case_block("envelope_defaults", "Envelope(20, 2.5 %)", params(period=20, percent=2.5),
                   ohlc(LONG[:25]), envelope_expected(LONG[:25], 20, 2.5)))
    add(case_block("envelope_custom", "Envelope(3, 10 %)", params(period=3, percent=10.0),
                   ohlc(SHORT[:8]), envelope_expected(SHORT[:8], 3, 10.0)))

    # TWAP
    add(case_block("twap_defaults", "TWAP, daily anchor at 00:00 UTC: one day, running mean",
                   params(day_start_offset=0.0), ohlc(SHORT), twap_expected(SHORT, 0.0)))
    add(case_block("twap_offset", "TWAP, day starts at 01:00 UTC: bars 1-3 belong to the day "
                   "before, bar 4 (ts 3600) restarts", params(day_start_offset=3600.0),
                   ohlc(SHORT[:8]), twap_expected(SHORT[:8], 3600.0)))

    # Rolling VWAP
    vwap_bars = ohlc(LONG[:26])
    add(case_block("vwap_defaults", "VWAP(390, slope 20): window never full, slope from bar 21",
                   params(window=390, slope_lookback=20), vwap_bars,
                   vwap_expected(vwap_bars, 390, 20)))
    vgap = ohlc(SHORT[:12], [1000.0, 2000.0, 500.0, 0.0, 0.0, 0.0, 1500.0, 800.0, 1200.0, 0.0,
                             900.0, 1100.0])
    add(case_block("vwap_gap", "VWAP(3, slope 2): bar 6 has a window without volume, no output",
                   params(window=3, slope_lookback=2), vgap, vwap_expected(vgap, 3, 2)))

    # Anchored VWAP
    av_closes = wave(100, 100.0, [2.0, -1.0, 3.0, -2.0, 1.0, -3.0, 2.0, 1.0])
    av_volumes = [float(v) for v in ([100, 250, 0, 175, 300, 50, 125] * 15)[:100]]
    av_bars = avwap_bars(av_closes, av_volumes)
    add(case_block("anchored_vwap_defaults", "Anchored VWAP (session, 1/2 sigma): day boundary "
                   "at bar 96 (ts 86400) restarts; zero volume counts with weight 1",
                   params(mult1=1.0, mult2=2.0), av_bars, avwap_expected(av_bars, 1.0, 2.0)))
    add(case_block("anchored_vwap_mults", "Anchored VWAP, bands at 0.5 and 3 sigma",
                   params(mult1=0.5, mult2=3.0), av_bars[:12],
                   avwap_expected(av_bars[:12], 0.5, 3.0)))

    # MIDAS
    mb = midas_bars()
    add(case_block("midas_defaults", "MIDAS Topfinder, maturity 20: pullback, new high",
                   params(maturity_bars=20), mb, midas_expected(mb, 20)))
    add(case_block("midas_mature", "MIDAS maturity 3: exhausted during the pullback, reset by "
                   "the new high", params(maturity_bars=3), mb, midas_expected(mb, 3)))

    # Trend relationship
    add(case_block("trend_relationship_defaults", "EMA(9) vs EMA(21) over up, down, up",
                   params(fast_len=9, slow_len=21), ohlc(UP_DOWN_UP),
                   trend_relationship_expected(UP_DOWN_UP, 9, 21)))
    add(case_block("trend_relationship_short", "EMA(2) vs EMA(5) over a wiggle",
                   params(fast_len=2, slow_len=5), ohlc(WIGGLE[:24]),
                   trend_relationship_expected(WIGGLE[:24], 2, 5)))

    # Ichimoku
    udu = ohlc(UP_DOWN_UP)
    add(case_block("ichimoku_defaults", "Ichimoku(9, 26, 52) over up, down, up",
                   params(tenkan_p=9, kijun_p=26, senkou_b_p=52), udu,
                   ichimoku_expected(udu, 9, 26, 52)))
    add(case_block("ichimoku_short", "Ichimoku(3, 5, 10)",
                   params(tenkan_p=3, kijun_p=5, senkou_b_p=10), udu[:40],
                   ichimoku_expected(udu[:40], 3, 5, 10)))

    # Supertrend
    add(case_block("supertrend_defaults", "Supertrend(10, 3) over up, down, up",
                   params(period=10, multiplier=3.0), udu, supertrend_expected(udu, 10, 3.0)))
    add(case_block("supertrend_tight", "Supertrend(3, 1): flips both ways",
                   params(period=3, multiplier=1.0), udu, supertrend_expected(udu, 3, 1.0)))

    # Parabolic SAR
    add(case_block("parabolic_sar_defaults", "PSAR(0.02, 0.2) over up, down, up",
                   params(step=0.02, max_step=0.2), udu, psar_expected(udu, 0.02, 0.2)))
    add(case_block("parabolic_sar_fast", "PSAR(0.05, 0.1): AF reaches its cap",
                   params(step=0.05, max_step=0.1), udu, psar_expected(udu, 0.05, 0.1)))

    # Donchian
    add(case_block("donchian_defaults", "Donchian(20) over up, down, up", params(period=20), udu,
                   donchian_expected(udu, 20)))
    add(case_block("donchian_short", "Donchian(5)", params(period=5), udu[:30],
                   donchian_expected(udu[:30], 5)))

    # Keltner
    add(case_block("keltner_defaults", "Keltner(EMA 20, ATR 10, 2): output from bar 10",
                   params(ema_period=20, atr_period=10, multiplier=2.0), udu,
                   keltner_expected(udu, 20, 10, 2.0)))
    add(case_block("keltner_alias", "Keltner via the legacy key ma_period 5, ATR 3, 0.75: "
                   "breakouts both ways", params(ma_period=5, atr_period=3, multiplier=0.75),
                   udu[:30], keltner_expected(udu[:30], 5, 3, 0.75)))

    # Bollinger
    add(case_block("bollinger_defaults", "Bollinger(20, 2), population divisor",
                   params(len=20, mult=2.0), udu, bollinger_expected(UP_DOWN_UP, 20, 2.0)))
    add(case_block("bollinger_tight", "Bollinger(5, 1): touches of both bands",
                   params(len=5, mult=1.0), ohlc(WIGGLE[:25]),
                   bollinger_expected(WIGGLE[:25], 5, 1.0)))
    add(case_block("bollinger_flat", "Bollinger(4, 2) over a flat start: degenerate band, "
                   "%B 0.5, both touches", params(len=4, mult=2.0), ohlc(FLAT_THEN_MOVE),
                   bollinger_expected(FLAT_THEN_MOVE, 4, 2.0, flat_ok=True)))
    return blocks


def build():
    header = provenance("contract_moving_averages") + [
        "",
        "Format: see contract_pilot; out<i>_secondary where the output carries one.",
    ]
    return header, build_blocks()
