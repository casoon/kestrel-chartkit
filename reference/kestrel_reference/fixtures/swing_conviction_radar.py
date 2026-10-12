"""golden_swing_conviction_radar: Swing Conviction Radar.

Written from the published Pine Script (WavesUnchained, v1.2.2), not from the Rust code: every
series is a full array indexed by bar, `x[k]` is read as `series[t - k]`.
"""

import math

from ..fixture import provenance

NAME = "golden_swing_conviction_radar"

PIV_L = 5
PIV_R = 5
MAX_LEG = 100
W_SPEED = 0.4
W_CLEAN = 0.4
W_VOL = 0.2
ATR_LEN = 14
SPEED_REF = 1.0

N_BARS = 700
CHECKPOINTS = [150, 299, 420, 560, 699]


def _series(with_volume):
    """Deterministic OHLCV: trend, range, decline, recovery, with waves of changing amplitude."""
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.12 if i < 200 else (0.0 if i < 330 else (-0.18 if i < 480 else 0.10))
        amp = 1.0 + 0.6 * math.sin(x / 47.0)
        close = prev + drift + amp * (1.1 * math.sin(x / 4.3) + 0.5 * math.sin(x / 13.0)) * 0.6 \
            + 0.25 * math.cos(x / 1.9)
        open_ = prev + 0.15 * math.sin(x / 2.7)
        high = max(open_, close) + 0.3 + 0.25 * abs(math.sin(x / 3.9))
        low = min(open_, close) - 0.3 - 0.25 * abs(math.cos(x / 6.1))
        vol = 1000.0 + 600.0 * math.sin(x / 5.0) ** 2 + 300.0 * math.cos(x / 17.0) if with_volume else 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


def _rma(src, n):
    """Pine ta.rma: SMA of the first n values, then (x + (n - 1) * prev) / n."""
    out = [None] * len(src)
    prev = None
    for t in range(len(src)):
        if prev is None:
            if t >= n - 1:
                prev = sum(src[t - n + 1:t + 1]) / n
                out[t] = prev
        else:
            prev = (src[t] + (n - 1) * prev) / n
            out[t] = prev
    return out


def _pivot(vals, t, left, right, high):
    """ta.pivothigh/pivotlow at bar t: the value `right` bars back, strictly beyond the left
    neighbours and at least equal to the right ones (the first of equal extremes counts)."""
    c = t - right
    if c - left < 0:
        return None
    v = vals[c]
    for j in range(c - left, t + 1):
        if j == c:
            continue
        strict = j < c
        if high and not (v > vals[j] if strict else v >= vals[j]):
            return None
        if not high and not (v < vals[j] if strict else v <= vals[j]):
            return None
    return v


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    vol = [b[4] for b in bars]
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, ATR_LEN)

    has_low = has_high = False
    last_low_bar = last_high_bar = 0
    last_low_val = last_high_val = 0.0
    last_high_conv = last_high_price = None
    last_low_conv = last_low_price = None
    prev_high_conv = prev_low_conv = None
    high_step = low_step = None
    cum = 0.0
    out = {"high_step": [None] * n, "low_step": [None] * n, "bull_prev": [None] * n,
           "bear_prev": [None] * n, "legs": [], "bear_div": [], "bull_div": []}

    for t in range(n):
        cum += vol[t]
        volume_is_real = cum > 0
        atr_val = atr[t]

        def leg_score(is_bull, leg_bars, leg_dist):
            speed_raw = leg_dist / leg_bars / atr_val if (leg_bars > 0 and atr_val is not None
                                                          and atr_val > 0) else 0.0
            speed_n = min(speed_raw / SPEED_REF, 1.0)
            max_i = min(leg_bars - 1, MAX_LEG - 1)
            on_count = 0
            vol1 = vol2 = 0.0
            ct1 = ct2 = 0
            for i in range(max_i + 1):
                off = PIV_R + i
                if is_bull:
                    is_on = c[t - off] > c[t - off - 1]
                else:
                    is_on = c[t - off] < c[t - off - 1]
                if is_on:
                    on_count += 1
                if i * 2 <= max_i:
                    vol1 += vol[t - off]
                    ct1 += 1
                else:
                    vol2 += vol[t - off]
                    ct2 += 1
            clean_n = on_count / (max_i + 1)
            avg1 = vol1 / ct1 if ct1 > 0 else 0.0
            avg2 = vol2 / ct2 if ct2 > 0 else 1.0
            grad = avg1 / avg2 - 1.0 if avg2 > 0 else 0.0
            vol_n = max(min(grad + 0.5, 1.0), 0.0)
            eff_w_vol = W_VOL if volume_is_real else 0.0
            total = W_SPEED + W_CLEAN + eff_w_vol
            return (speed_n * W_SPEED + clean_n * W_CLEAN + vol_n * eff_w_vol) / total * 100.0

        ph = _pivot(h, t, PIV_L, PIV_R, True)
        if ph is not None:
            piv_bar = t - PIV_R
            leg_bars = piv_bar - last_low_bar if has_low else 0
            leg_dist = abs(ph - last_low_val) if has_low else 0.0
            conv = leg_score(True, leg_bars, leg_dist) if has_low and leg_bars > 0 else None
            if conv is not None:
                if last_high_conv is not None and ph > last_high_price and conv < last_high_conv:
                    out["bear_div"].append(t)
                out["legs"].append((t, 1.0, conv))
                prev_high_conv = last_high_conv
                last_high_conv, last_high_price = conv, ph
                high_step = conv
            has_high, last_high_bar, last_high_val = True, piv_bar, ph

        pl = _pivot(lo, t, PIV_L, PIV_R, False)
        if pl is not None:
            piv_bar = t - PIV_R
            leg_bars = piv_bar - last_high_bar if has_high else 0
            leg_dist = abs(pl - last_high_val) if has_high else 0.0
            conv = leg_score(False, leg_bars, leg_dist) if has_high and leg_bars > 0 else None
            if conv is not None:
                if last_low_conv is not None and pl < last_low_price and conv < last_low_conv:
                    out["bull_div"].append(t)
                out["legs"].append((t, -1.0, conv))
                prev_low_conv = last_low_conv
                last_low_conv, last_low_price = conv, pl
                low_step = -conv
            has_low, last_low_bar, last_low_val = True, piv_bar, pl

        out["high_step"][t] = high_step
        out["low_step"][t] = low_step
        out["bull_prev"][t] = prev_high_conv
        out["bear_prev"][t] = prev_low_conv
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Swing Conviction Radar",
    "",
    *provenance("swing_conviction_radar"),
    "",
    "Formula: the published Pine Script 'Swing Conviction Radar' (WavesUnchained, v1.2.2) with",
    "its defaults (pivots 5/5, max leg 100, weights 0.4/0.4/0.2, ATR 14, speed reference 1).",
    "Conventions where Pine leaves them open: pivots strict to the left, non-strict to the right (the first of equal extremes counts), the pivot high of a bar",
    "processed before its pivot low, ATR = Wilder average of the true range (first bar high - low)",
    "seeded with an SMA, speed 0 while the ATR is not available, real volume = cumulative volume",
    "so far positive.",
    "",
    "Series: 700 bars, prev = previous close (100 before the first bar), drift 0.12 up to bar",
    "199, 0 to 329, -0.18 to 479, 0.10 after; amp = 1 + 0.6 sin(i/47); close = prev + drift +",
    "0.6 amp (1.1 sin(i/4.3) + 0.5 sin(i/13)) + 0.25 cos(i/1.9); open = prev + 0.15 sin(i/2.7);",
    "high = max(open, close) + 0.3 + 0.25 |sin(i/3.9)|; low = min(open, close) - 0.3 -",
    "0.25 |cos(i/6.1)|; volume = 1000 + 600 sin(i/5)^2 + 300 cos(i/17) (block 'v'), or 0 for",
    "every bar (block 'nv').",
    "",
    "<v|nv>_first is the first bar with both step lines; <v|nv>_cp<bar>_* the step lines and",
    "previous convictions at that bar; <v|nv>_legs_n the number of scored legs and",
    "<v|nv>_leg<j>_bar/_dir/_conv the j-th (up to 12); <v|nv>_<bear|bull>_div_n and _<j> the",
    "divergence bars.",
]


def _block(prefix, out):
    case = {}
    first = next(t for t in range(N_BARS)
                 if out["high_step"][t] is not None and out["low_step"][t] is not None)
    case["first"] = float(first)
    for cp in CHECKPOINTS:
        case[f"cp{cp}_high_step"] = out["high_step"][cp]
        case[f"cp{cp}_low_step"] = out["low_step"][cp]
        case[f"cp{cp}_bull_prev"] = out["bull_prev"][cp]
        case[f"cp{cp}_bear_prev"] = out["bear_prev"][cp]
    case["legs_n"] = float(len(out["legs"]))
    for j, (t, d, conv) in enumerate(out["legs"][:12]):
        case[f"leg{j}_bar"] = float(t)
        case[f"leg{j}_dir"] = d
        case[f"leg{j}_conv"] = conv
    for kind in ("bear_div", "bull_div"):
        case[f"{kind}_n"] = float(len(out[kind]))
        for j, t in enumerate(out[kind]):
            case[f"{kind}_{j}"] = float(t)
    return (prefix, None, case)


def build():
    blocks = [
        _block("v", compute(_series(True))),
        _block("nv", compute(_series(False))),
        (None, None, {"scr_tolerance": 1e-9}),
    ]
    return HEADER, blocks
