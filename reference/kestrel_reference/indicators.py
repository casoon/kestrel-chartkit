"""Indicator definitions shared by several fixtures and by the analytics references.

Each follows the definition documented on the corresponding type in the crate.
"""

import math

from .smoothing import ema_first_sample, ema_published, published, rma


def true_ranges(bars):
    """TR_1 = high - low; TR_t = max(high - low, |high - close_(t-1)|, |low - close_(t-1)|)."""
    out = []
    for index, (_, high, low, _) in enumerate(bars):
        if index == 0:
            out.append(high - low)
        else:
            previous = bars[index - 1][3]
            out.append(max(high - low, abs(high - previous), abs(low - previous)))
    return out


def directional(bars):
    """+DM, -DM and TR per bar from the second one on: up = high - prev_high, down = prev_low - low;
    +DM = up if up > down and up > 0, -DM = down if down > up and down > 0."""
    plus, minus, tr = [], [], []
    for prev, bar in zip(bars, bars[1:]):
        up, down = bar[1] - prev[1], prev[2] - bar[2]
        plus.append(up if up > down and up > 0.0 else 0.0)
        minus.append(down if down > up and down > 0.0 else 0.0)
        tr.append(max(bar[1] - bar[2], abs(bar[1] - prev[3]), abs(bar[2] - prev[3])))
    return plus, minus, tr


def adx(bars, di_len, smooth):
    """Wilder averages of TR, +DM, -DM over di_len; DI = 100 DM_avg / TR_avg;
    DX = 100 |DI+ - DI-| / (DI+ + DI-); ADX = Wilder average of DX over `smooth`."""
    plus, minus, tr = directional(bars)
    dx = []
    for p, m, t in zip(rma(plus, di_len), rma(minus, di_len), rma(tr, di_len)):
        if t is None:
            continue
        di_p, di_m = (100.0 * p / t, 100.0 * m / t) if t != 0.0 else (0.0, 0.0)
        dx.append(100.0 * abs(di_p - di_m) / (di_p + di_m) if di_p + di_m != 0.0 else 0.0)
    return published(rma(dx, smooth))


def keltner(bars, ema_period, atr_period, mult):
    """Basis = first-sample EMA of the close from the first bar; ATR = plain mean of the last
    atr_period true ranges; bands basis +/- mult ATR."""
    basis = ema_first_sample([b[3] for b in bars], ema_period)[-1]
    atr = sum(true_ranges(bars)[-atr_period:]) / atr_period
    return basis, basis + mult * atr, basis - mult * atr


def efficiency_ratio(closes, length):
    window = closes[-(length + 1):]
    change = abs(window[-1] - window[0])
    path = sum(abs(b - a) for a, b in zip(window, window[1:]))
    return change / path if path > 0 else 0.0


def rsi_from_averages(gain, loss):
    """100 - 100 / (1 + gain / loss), with 50 when nothing moved, 100 without losses and 0 without
    gains."""
    if gain == 0.0 and loss == 0.0:
        return 50.0
    if loss == 0.0:
        return 100.0
    if gain == 0.0:
        return 0.0
    return 100.0 - 100.0 / (1.0 + gain / loss)


def raw_rsi(closes, length, smoothing="wilder"):
    """Published raw RSI values. Up and down moves of the close are averaged Wilder's way
    (SMA-seeded, alpha 1/N) or exponentially (alpha 2/(N+1), seeded with the first change); both
    publish from the N-th change on."""
    changes = [b - a for a, b in zip(closes, closes[1:])]
    gains = [max(c, 0.0) for c in changes]
    losses = [max(-c, 0.0) for c in changes]
    if smoothing == "wilder":
        g, l = rma(gains, length), rma(losses, length)
    else:
        g, l = ema_published(gains, length), ema_published(losses, length)
    return [rsi_from_averages(a, b) for a, b in zip(g, l) if a is not None]


def clamp(x, low=0.0, high=100.0):
    return min(max(x, low), high)


def rsi_lines(closes, length, avg_len, sig_len, ctx_len=None, smoothing="wilder"):
    """The RSI indicator's outputs: line = EMA(avg_len) of the raw RSI, signal = EMA(sig_len) of the
    line, context = EMA(avg_len) of the raw RSI over ctx_len; all EMAs first-sample-seeded on
    their first input, lines clamped to 0..100."""
    raw = raw_rsi(closes, length, smoothing)
    line = [clamp(x) for x in ema_first_sample(raw, avg_len)]
    signal = [clamp(x) for x in ema_first_sample(line, sig_len)]
    ctx = ema_first_sample(raw_rsi(closes, ctx_len, smoothing), avg_len) if ctx_len else None
    return line, signal, ctx
