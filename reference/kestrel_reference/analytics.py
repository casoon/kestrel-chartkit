"""Analytics read-outs, each following the definition documented on its function in the crate.

Bars are (open, high, low, close, volume) tuples, oldest first.
"""

import math

from .indicators import adx, efficiency_ratio, true_ranges
from .smoothing import ema_first_sample, published, rma


def _closes(bars):
    return [b[3] for b in bars]


def _clamp(x, low, high):
    return min(max(x, low), high)


def _norm(x, low, high):
    return 0.0 if high == low else _clamp((x - low) / (high - low), 0.0, 1.0)


def _adx_by_bar(bars, length):
    """ADX(length, length) per bar, `None` before its first value."""
    series = adx([b[:4] for b in bars], length, length)
    return [None] * (len(bars) - len(series)) + series


def choppiness(bars, window):
    """100 log10(sum TR / (highest high - lowest low)) / log10(window) over the last `window`
    bars, the first true range against the close before them."""
    previous = bars[-window - 1][3]
    total = 0.0
    for _, high, low, close, _ in bars[-window:]:
        total += max(high - low, abs(high - previous), abs(low - previous))
        previous = close
    span = max(b[1] for b in bars[-window:]) - min(b[2] for b in bars[-window:])
    return 100.0 * math.log10(total / span) / math.log10(window)


def regime(bars, adx_len, window):
    adx_value = _adx_by_bar(bars, adx_len)[-1]
    chop = choppiness(bars, window)
    efficiency = efficiency_ratio(_closes(bars), window)
    votes = (adx_value >= 25.0) + (chop <= 38.2) + (efficiency >= 0.5)
    return {"adx": adx_value, "choppiness": chop, "efficiency": efficiency, "votes": votes,
            "trending": votes >= 2}


def _correlation_with_index(values):
    n = len(values)
    mean_y = (n - 1) / 2.0
    mean_x = sum(values) / n
    cov = var_x = var_y = 0.0
    for i, x in enumerate(values):
        dx, dy = x - mean_x, i - mean_y
        cov += dx * dy
        var_x += dx * dx
        var_y += dy * dy
    if var_x <= 0.0 or var_y <= 0.0:
        return 0.0
    return cov / (math.sqrt(var_x) * math.sqrt(var_y))


def _adx_scores(bars):
    """Undamped ADX sub-score per bar: strength and the first-sample EMA(3) of the bar-to-bar
    ADX changes, from the second ADX value on."""
    values = _adx_by_bar(bars, 14)
    first = next(i for i, v in enumerate(values) if v is not None)
    changes = [values[i] - values[i - 1] for i in range(first + 1, len(values))]
    slopes = ema_first_sample(changes, 3)
    out = [None] * len(bars)
    for offset, slope in enumerate(slopes):
        i = first + 1 + offset
        out[i] = (_norm(values[i], 12.0, 35.0) * 0.7 + _norm(slope, -1.0, 1.5) * 0.3) * 100.0
    return out


def _sensors(bars, idx, length=34):
    window = bars[idx - length:idx + 1]
    closes = _closes(window)
    corr = _correlation_with_index(closes[1:])
    r2_score = corr * corr * 100.0
    er_score = efficiency_ratio(closes, length) * 100.0
    span = max(b[1] for b in window) - min(b[2] for b in window)
    path = sum(abs(b - a) for a, b in zip(closes, closes[1:]))
    fdi = math.log(path / span) / math.log(length) + 1.0 if span > 0 and path > 0 else 1.5
    fdi_score = (1.0 - _norm(fdi, 1.20, 1.65)) * 100.0
    return r2_score, er_score, fdi_score, corr


def trend_persistence(bars):
    adx_scores = _adx_scores(bars)
    raws = []
    for idx in range(len(bars) - 5, len(bars)):
        r2_score, er_score, fdi_score, corr = _sensors(bars, idx)
        adx_score = adx_scores[idx]
        if r2_score < 20.0 and er_score < 20.0:
            adx_score *= 0.35
        raws.append((r2_score * 40.0 + er_score * 25.0 + adx_score * 20.0 + fdi_score * 15.0)
                    / 100.0)
    score = ema_first_sample(raws, 5)[-1]
    return {"score": score, "r2_score": r2_score, "er_score": er_score, "adx_score": adx_score,
            "fdi_score": fdi_score, "corr": corr,
            "transition_risk": (100.0 - score) * 0.7 + (100.0 - er_score) * 0.3}


def price_summary(bars, atr_len, stop_mult):
    last, previous = bars[-1][3], bars[-2][3]
    atrs = published(rma(true_ranges([b[:4] for b in bars]), atr_len))
    atr = atrs[-1]
    high, low = max(b[1] for b in bars), min(b[2] for b in bars)
    return {
        "change_pct": 100.0 * (last - previous) / previous,
        "range_position": _clamp((last - low) / (high - low), 0.0, 1.0),
        "atr": atr,
        "atr_pct": 100.0 * atr / last,
        "atr_percentile": sum(1 for a in atrs if a <= atr) / len(atrs),
        "stop_distance": stop_mult * atr,
    }


def _wvf(bars, i):
    closes = _closes(bars[i - 21:i + 1])
    hc, lc = max(closes), min(closes)
    return 100.0 * (hc - bars[i][2]) / hc, 100.0 * (bars[i][1] - lc) / lc


def _spike_band(values):
    mean = sum(values) / len(values)
    variance = sum((v - mean) ** 2 for v in values) / len(values)
    return mean + 2.0 * math.sqrt(variance)


def fear_gauge(bars):
    last = len(bars) - 1
    band = [_wvf(bars, i) for i in range(last - 19, last + 1)]
    wvf, bwvf = band[-1]
    if wvf >= _spike_band([w for w, _ in band]):
        state = "fear_spike"
    elif bwvf >= _spike_band([b for _, b in band]):
        state = "complacency_spike"
    else:
        state = "neutral"
    trs = []
    for k in range(last - 13, last + 1):
        high, low, previous = bars[k][1], bars[k][2], bars[k - 1][3]
        trs.append(max(high - low, abs(high - previous), abs(low - previous)))
    atr = sum(trs) / 14
    moved = abs(bars[last][3] - bars[last - 5][3]) / atr if atr != 0.0 else 0.0
    stalled = wvf - _wvf(bars, last - 5)[0] > 2.0 and moved < 0.5
    return {"wvf": wvf, "bwvf": bwvf, "state": state,
            "absorbed": state != "neutral" and stalled}


def volume_percentile(bars, window):
    series = bars[-max(window, 1):]
    last = series[-1][4]
    return sum(1 for b in series if b[4] <= last) / len(series)


def fear_greed(bars, fear=None, regime_reading=None, atr_percentile=None, persistence=None):
    """Score from `bars` and whichever readings are given; a missing component counts 50."""
    if fear is None:
        fear_score = 50.0
    else:
        base = {"fear_spike": 15.0, "complacency_spike": 85.0}.get(
            fear["state"], 50.0 + _clamp(fear["bwvf"] - fear["wvf"], -20.0, 20.0) * 1.25)
        fear_score = (base + 50.0) / 2.0 if fear["absorbed"] else base
    window = bars[-34:]
    use_volume = any(b[4] > 0.0 for b in window)
    weighted = weight_sum = 0.0
    for _, high, low, close, volume in window:
        span = high - low
        if span <= 0.0:
            continue
        clv = _clamp(((close - low) - (high - close)) / span, -1.0, 1.0)
        weight = max(volume, 0.0) if use_volume else 1.0
        if weight == 0.0:
            continue
        weighted += clv * weight
        weight_sum += weight
    flow = 50.0 if weight_sum == 0.0 else (weighted / weight_sum + 1.0) * 50.0
    volatility = 50.0 if atr_percentile is None else 100.0 * (1.0 - atr_percentile)
    persistence_score = 50.0 if persistence is None else persistence
    if regime_reading is None:
        regime_score = 50.0
    elif regime_reading["trending"]:
        regime_score = 55.0 + regime_reading["votes"] / 3.0 * 25.0
    else:
        regime_score = 45.0 + regime_reading["votes"] / 3.0 * 15.0
    score = (fear_score * 0.30 + volatility * 0.20 + flow * 0.20 + persistence_score * 0.20
             + regime_score * 0.10)
    return _clamp(score, 0.0, 100.0)


def sma_slope(bars, length):
    closes = _closes(bars)
    series = [sum(closes[i - length + 1:i + 1]) / length for i in range(length - 1, len(closes))]
    return 100.0 * (series[-1] / series[-1 - length] - 1.0)


def _wilder_gain_loss(closes, length):
    changes = [b - a for a, b in zip(closes, closes[1:])]
    gain = sum(max(c, 0.0) for c in changes[:length]) / length
    loss = sum(max(-c, 0.0) for c in changes[:length]) / length
    for c in changes[length:]:
        gain = (gain * (length - 1.0) + max(c, 0.0)) / length
        loss = (loss * (length - 1.0) + max(-c, 0.0)) / length
    return gain, loss


def rsi_target(last, gain, loss, length, target):
    rs = target / (100.0 - target)
    k = length - 1.0
    up = rs * loss * k - gain * k
    down = loss * k - gain * k / rs
    return last + (up if up >= 0.0 else down if down <= 0.0 else up)


def levels(bars, window):
    bars = bars[-window:]
    last = bars[-1][3]
    high, low = max(b[1] for b in bars), min(b[2] for b in bars)
    pivot = (high + low + last) / 3.0
    closes = _closes(bars)
    gain, loss = _wilder_gain_loss(closes, 14)
    return {
        "Pivot": pivot,
        "Pivot S1": 2.0 * pivot - high,
        "Pivot R1": 2.0 * pivot - low,
        "Pivot S2": pivot - (high - low),
        "Pivot R2": pivot + (high - low),
        "Fib 50.0%": high - (high - low) * 0.5,
        "SMA20": sum(closes[-20:]) / 20.0,
        **{f"RSI {t:.0f}": rsi_target(last, gain, loss, 14, t) for t in (30.0, 50.0, 70.0)},
    }
