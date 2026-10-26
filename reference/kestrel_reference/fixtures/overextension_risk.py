"""golden_overextension_risk: Overextension Risk.

Written from the original definition, not from the Rust code. Preset Commodities (weights
40/35/25, money flow on where real volume exists), signal mode Normal (threshold 40), confirmation
Candle Reaction, signal gap 8, divergence assist off.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_overextension_risk"

W_STRETCH, W_EXHAUST, W_REACT = 40.0, 35.0, 25.0
BASE_THRESH = 40.0
GAP = 8
REGIME_FAST, REGIME_SLOW = 20, 100
ADX_LEN = 14
ADX_TREND = 25.0
PENALTY = 12.0
EMA_LEN, ATR_LEN, BB_LEN, BB_MULT = 50, 14, 20, 2.0
RSI_LEN, STOCH_LEN = 14, 14
WT_N1, WT_N2 = 10, 21
PIVOT = 5
DIV_MEMORY = 12
STRUCT_LEN = 80
NEAR_ATR = 0.75

N_BARS = 900
CHECKPOINTS = [120, 230, 333, 450, 617, 690, 780, 899]
EVENTS = ["bear_do_not_chase", "bull_do_not_chase", "bear_trend_exhaustion",
          "bull_trend_exhaustion", "bear_range_fade", "bull_range_fade"]


def sd(a, b):
    return 0.0 if b == 0.0 else a / b


def s01(x):
    return ps.clamp01(x) * 100.0


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    hlc3 = [(h[t] + lo[t] + c[t]) / 3.0 for t in range(n)]

    atr = ps.atr(h, lo, c, ATR_LEN)
    ema = ps.ema(c, EMA_LEN)
    basis = ps.sma(c, BB_LEN)
    dev = ps.stdev(c, BB_LEN)
    upper = ps.smap(lambda b, d: b + d * BB_MULT, basis, dev)
    lower = ps.smap(lambda b, d: b - d * BB_MULT, basis, dev)
    dist_atr = ps.smap(lambda cc, e, a: sd(cc - e, a), c, ema, atr)
    bb_z = ps.smap(lambda cc, b, d: sd(cc - b, d), c, basis, dev)

    rsi = ps.rsi(c, RSI_LEN)
    r_hi = ps.highest(rsi, STOCH_LEN)
    r_lo = ps.lowest(rsi, STOCH_LEN)
    stoch_raw = ps.smap(lambda r, a, b: 100.0 * (r - b) / (a - b) if a != b else None,
                        rsi, r_hi, r_lo)
    stoch = ps.sma(stoch_raw, 3)
    wt1, wt2 = ps.wavetrend(h, lo, c, WT_N1, WT_N2)

    mfi_raw = ps.mfi(hlc3, v, 14)
    mfi = [50.0 if m is None else m for m in mfi_raw]
    vol_ma = ps.sma(v, 30)
    vol_spike = ps.smap(lambda x, m: sd(x, m), v, vol_ma)
    cum = 0.0
    flow_on = []
    for t in range(n):
        cum += v[t]
        flow_on.append(cum > 0.0 and vol_ma[t] is not None and vol_ma[t] > 0.0)

    atr_ma = ps.sma(atr, 50)
    atr_exp = ps.smap(sd, atr, atr_ma)
    range_atr = ps.smap(lambda hh, ll, a: sd(hh - ll, a), h, lo, atr)
    recent_high = ps.highest(h, STRUCT_LEN)
    recent_low = ps.lowest(lo, STRUCT_LEN)

    ema_fast = ps.ema(c, REGIME_FAST)
    ema_slow = ps.ema(c, REGIME_SLOW)
    adx = ps.adx(h, lo, c, ADX_LEN, ADX_LEN)

    # WaveTrend divergences on strict price pivots.
    bear_div = [False] * n
    bull_div = [False] * n
    last_ph = None  # (price, wt at pivot)
    last_pl = None
    for t in range(n):
        ph = ps.pivot(h, t, PIVOT, PIVOT, False)
        pl = ps.pivot(lo, t, PIVOT, PIVOT, True)
        w = ps.at(wt1, t, PIVOT)
        if ph is not None:
            if last_ph is not None and ph > last_ph[0] and w is not None and last_ph[1] is not None \
                    and w < last_ph[1]:
                bear_div[t] = True
            last_ph = (ph, w)
        if pl is not None:
            if last_pl is not None and pl < last_pl[0] and w is not None and last_pl[1] is not None \
                    and w > last_pl[1]:
                bull_div[t] = True
            last_pl = (pl, w)

    bear_pivot = bull_pivot = None
    bear_active = [False] * n
    bull_active = [False] * n
    for t in range(n):
        if bear_div[t]:
            bear_pivot = h[t - PIVOT]
        if bull_div[t]:
            bull_pivot = lo[t - PIVOT]
        bs = ps.barssince(bear_div, t)
        bl = ps.barssince(bull_div, t)
        bear_active[t] = bs is not None and bs <= DIV_MEMORY and bear_pivot is not None \
            and h[t] <= bear_pivot
        bull_active[t] = bl is not None and bl <= DIV_MEMORY and bull_pivot is not None \
            and lo[t] >= bull_pivot

    out = {k: [None] * n for k in (
        "top", "bot", "stretch_top", "stretch_bot", "exhaust_top", "exhaust_bot", "react_top",
        "react_bot", "vol_factor", "top_thresh", "bot_thresh", "regime")}
    events = {e: [] for e in EVENTS}
    last_top = last_bot = None
    for t in range(1, n):
        need = (atr[t], ema[t], upper[t], lower[t], dist_atr[t], bb_z[t], rsi[t], stoch[t],
                wt1[t], wt2[t], wt1[t - 1], wt2[t - 1], ps.at(wt1, t, 2), atr_exp[t],
                range_atr[t], recent_high[t], recent_low[t], recent_high[t - 1],
                recent_low[t - 1], adx[t])
        if not ps.ok(*need):
            continue
        near_high = recent_high[t] - c[t] <= atr[t] * NEAR_ATR
        near_low = c[t] - recent_low[t] <= atr[t] * NEAR_ATR
        rng = h[t] - lo[t]
        upper_wick = sd(h[t] - max(o[t], c[t]), rng)
        lower_wick = sd(min(o[t], c[t]) - lo[t], rng)
        trend_up = ema_fast[t] > ema_slow[t] and c[t] > ema_slow[t]
        trend_dn = ema_fast[t] < ema_slow[t] and c[t] < ema_slow[t]
        strong = adx[t] >= ADX_TREND
        regime_up = strong and trend_up
        regime_dn = strong and trend_dn

        stretch_top = s01(0.45 * max(sd(dist_atr[t], 3.0), sd(bb_z[t], 2.5))
                          + 0.35 * (1 if near_high else 0) + 0.20 * (1 if c[t] > upper[t] else 0))
        stretch_bot = s01(0.45 * max(sd(-dist_atr[t], 3.0), sd(-bb_z[t], 2.5))
                          + 0.35 * (1 if near_low else 0) + 0.20 * (1 if c[t] < lower[t] else 0))
        mom_top = s01(0.40 * sd(rsi[t] - 60, 20) + 0.30 * sd(stoch[t] - 75, 25)
                      + 0.30 * sd(wt1[t] - 45, 25))
        mom_bot = s01(0.40 * sd(40 - rsi[t], 20) + 0.30 * sd(25 - stoch[t], 25)
                      + 0.30 * sd(-45 - wt1[t], 25))
        div_top = 100.0 if bear_active[t] else 0.0
        div_bot = 100.0 if bull_active[t] else 0.0
        if flow_on[t]:
            flow_top = s01(0.55 * sd(mfi[t] - 75, 20) + 0.45 * sd(vol_spike[t] - 1.5, 1.5))
            flow_bot = s01(0.55 * sd(25 - mfi[t], 20) + 0.45 * sd(vol_spike[t] - 1.5, 1.5))
            ex_top = 0.55 * mom_top + 0.25 * flow_top + 0.20 * div_top
            ex_bot = 0.55 * mom_bot + 0.25 * flow_bot + 0.20 * div_bot
        else:
            ex_top = 0.70 * mom_top + 0.30 * div_top
            ex_bot = 0.70 * mom_bot + 0.30 * div_bot

        top_rf = (h[t] > upper[t] and c[t] < upper[t]) or \
            (h[t] > recent_high[t - 1] and c[t] < recent_high[t - 1])
        bot_rf = (lo[t] < lower[t] and c[t] > lower[t]) or \
            (lo[t] < recent_low[t - 1] and c[t] > recent_low[t - 1])
        wt_up = wt1[t] > wt2[t] and wt1[t - 1] <= wt2[t - 1]
        wt_dn = wt1[t] < wt2[t] and wt1[t - 1] >= wt2[t - 1]
        was_high = max(wt1[t - 2:t + 1]) >= 45
        was_low = min(wt1[t - 2:t + 1]) <= -45
        top_wt = wt_dn and was_high
        bot_wt = wt_up and was_low
        hl2 = (h[t] + lo[t]) / 2.0
        top_weak = c[t] < o[t] and c[t] < hl2
        bot_weak = c[t] > o[t] and c[t] > hl2
        top_wick = top_weak and upper_wick >= 0.35
        bot_wick = bot_weak and lower_wick >= 0.35
        react_top = s01(0.45 * (1 if top_rf else 0) + 0.30 * (1 if top_wt else 0)
                        + 0.25 * (sd(upper_wick - 0.35, 0.35) if top_weak else 0.0))
        react_bot = s01(0.45 * (1 if bot_rf else 0) + 0.30 * (1 if bot_wt else 0)
                        + 0.25 * (sd(lower_wick - 0.35, 0.35) if bot_weak else 0.0))
        top_reacted = top_rf or top_wt or top_wick
        bot_reacted = bot_rf or bot_wt or bot_wick

        vol_score = s01(0.55 * sd(atr_exp[t] - 1.1, 0.7) + 0.45 * sd(range_atr[t] - 1.2, 1.2))
        vf = 0.85 + sd(vol_score, 100) * 0.30
        role = W_STRETCH + W_EXHAUST + W_REACT
        top = ps.clamp((stretch_top * W_STRETCH + ex_top * W_EXHAUST + react_top * W_REACT)
                       / role * vf, 0.0, 100.0)
        bot = ps.clamp((stretch_bot * W_STRETCH + ex_bot * W_EXHAUST + react_bot * W_REACT)
                       / role * vf, 0.0, 100.0)
        top_th = ps.clamp(BASE_THRESH + (PENALTY if regime_up else 0.0), 20.0, 80.0)
        bot_th = ps.clamp(BASE_THRESH + (PENALTY if regime_dn else 0.0), 20.0, 80.0)

        top_sig = top >= top_th and top_reacted and (last_top is None or t - last_top > GAP)
        bot_sig = bot >= bot_th and bot_reacted and (last_bot is None or t - last_bot > GAP)
        if top_sig:
            last_top = t
            kind = "bear_do_not_chase" if regime_up else (
                "bear_trend_exhaustion" if regime_dn else "bear_range_fade")
            events[kind].append(t)
        if bot_sig:
            last_bot = t
            kind = "bull_do_not_chase" if regime_dn else (
                "bull_trend_exhaustion" if regime_up else "bull_range_fade")
            events[kind].append(t)

        for k, val in (("top", top), ("bot", bot), ("stretch_top", stretch_top),
                       ("stretch_bot", stretch_bot), ("exhaust_top", ex_top),
                       ("exhaust_bot", ex_bot), ("react_top", react_top),
                       ("react_bot", react_bot), ("vol_factor", vf), ("top_thresh", top_th),
                       ("bot_thresh", bot_th),
                       ("regime", 1.0 if regime_up else (-1.0 if regime_dn else 0.0))):
            out[k][t] = val
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Overextension Risk",
    "",
    *provenance("overextension_risk"),
    "",
    "Formula: the original definition, preset",
    "Commodities (40/35/25, money flow where real volume exists), signal mode Normal, Candle",
    "Reaction confirmation, gap 8, divergence assist off. Conventions the original leaves open:",
    "EMAs seeded with the first value; RSI, ATR and ADX Wilder averages seeded with an SMA (ADX",
    "from the second bar); population standard deviation; StochRSI na when the RSI range is zero;",
    "MFI flow from the second bar, na (then 50) while its window has no volume; pivots strict on",
    "both sides. A bar reports when every role input exists. Signal kinds by regime: top in an",
    "uptrend bear_do_not_chase, in a downtrend bear_trend_exhaustion, in a range bear_range_fade;",
    "bottom mirrored.",
    "",
    "Series: pine_series.wave_series(900) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(ps.wave_series(N_BARS))
    first = next(t for t, r in enumerate(out["top"]) if r is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    keys = ("top", "bot", "stretch_top", "stretch_bot", "exhaust_top", "exhaust_bot",
            "react_top", "react_bot", "vol_factor", "top_thresh", "bot_thresh", "regime")
    for cp in CHECKPOINTS:
        blocks.append((f"cp{cp}", f"bar {cp}", {k: out[k][cp] for k in keys}))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
