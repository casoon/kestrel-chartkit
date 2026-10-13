"""golden_accumulation_phase: Accumulation Phase.

Written from the original definition, not from the Rust code: series as full
arrays, `x[k]` read as `series[t - k]`, the `var` counters and phases followed bar by bar.
"""

import math

from ..fixture import provenance

NAME = "golden_accumulation_phase"

ATR_LEN = 14
LOOKBACK = 20
MIN_PHASE = 10
PIV_L = 3
PIV_R = 3
ER_RANGE = 0.3
BBW_COMP = 0.7
ATR_COMP = 0.8

N_BARS = 900
CHECKPOINTS = [76, 230, 274, 307, 569, 637, 700, 899]
EVENTS = ["bull_accumulation_developing", "bull_accumulation_mature", "bull_breakout_ready",
          "bear_distribution_developing", "bear_distribution_mature", "bear_breakdown_ready"]
STATES = ["none", "transition", "early", "developing", "mature", "ready"]


def _series():
    """Decline, base with dips under its lows, rally, top with pokes above its highs, decline."""
    bars = []
    prev = 150.0
    for i in range(N_BARS):
        x = float(i)
        if i < 150:
            drift, amp = -0.35, 0.6
        elif i < 330:
            drift, amp = 0.0, 0.3
        elif i < 480:
            drift, amp = 0.40, 0.6
        elif i < 680:
            drift, amp = 0.0, 0.3
        else:
            drift, amp = -0.38, 0.6
        pull = 0.0
        if 150 <= i < 330:
            pull = -0.2 * (prev - 97.0)
        elif 480 <= i < 680:
            pull = -0.2 * (prev - 157.0)
        close = prev + drift + pull + amp * (math.sin(x / 5.0) + 0.6 * math.sin(x / 7.9))
        open_ = prev + 0.25 * math.sin(x / 1.9)
        high = max(open_, close) + 0.3 + 0.3 * abs(math.sin(x / 3.3))
        low = min(open_, close) - 0.3 - 0.3 * abs(math.cos(x / 4.4))
        if 150 <= i < 330 and i % 13 == 7:
            low -= 1.8
        if 480 <= i < 680 and i % 13 == 9:
            high += 1.8
        bars.append((open_, high, low, close))
        prev = close
    return bars


def _rma(src, n):
    out = [None] * len(src)
    prev = None
    window = []
    for t, v in enumerate(src):
        if v is None:
            continue
        if prev is None:
            window.append(v)
            if len(window) == n:
                prev = sum(window) / n
                out[t] = prev
        else:
            prev = (v + (n - 1) * prev) / n
            out[t] = prev
    return out


def _sma(src, n):
    out = [None] * len(src)
    for t in range(n - 1, len(src)):
        w = src[t - n + 1:t + 1]
        if None not in w:
            out[t] = sum(w) / n
    return out


def _ema(src, n):
    a = 2.0 / (n + 1.0)
    out, prev = [], None
    for v in src:
        prev = v if prev is None else a * v + (1.0 - a) * prev
        out.append(prev)
    return out


def _pivot(vals, t, left, right, high):
    c = t - right
    if c - left < 0:
        return None
    v = vals[c]
    for j in range(c - left, t + 1):
        if j != c and ((high and not v > vals[j]) or (not high and not v < vals[j])):
            return None
    return v


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, ATR_LEN)
    atr_avg = _sma(atr, LOOKBACK)
    ema20 = _ema(c, 20)
    ema50 = _ema(c, 50)
    basis = _sma(c, 20)
    bbw = [None] * n
    for t in range(19, n):
        m = basis[t]
        dev = math.sqrt(sum((c[j] - m) ** 2 for j in range(t - 19, t + 1)) / 20.0)
        bbw[t] = (2 * dev * 2.0) / m if m > 0 else 0.0
    bbw_avg = _sma(bbw, LOOKBACK)
    rng = [h[t] - lo[t] for t in range(n)]
    rng_avg = _sma(rng, LOOKBACK)
    up = [None] + [max(c[t] - c[t - 1], 0.0) for t in range(1, n)]
    dn = [None] + [max(c[t - 1] - c[t], 0.0) for t in range(1, n)]
    au, ad = _rma(up, 14), _rma(dn, 14)
    rsi = [None] * n
    for t in range(n):
        if au[t] is not None:
            rsi[t] = 50.0 if au[t] == 0 and ad[t] == 0 else (
                100.0 if ad[t] == 0 else 100.0 - 100.0 / (1.0 + au[t] / ad[t]))

    last_sh = prev_sh = last_sl = prev_sl = None
    flags_prev = [False, False, False, False]
    counts = [0, 0, 0, 0]  # sweep low, spring, sweep high, upthrust
    decay_every = round(LOOKBACK / 2)
    a_start = d_start = None
    a_prev = d_prev = 0
    out = {"reported": [False] * n, "rows": [None] * n}
    events = {k: [] for k in EVENTS}

    for t in range(n):
        ph = _pivot(h, t, PIV_L, PIV_R, True)
        pl = _pivot(lo, t, PIV_L, PIV_R, False)
        if ph is not None:
            prev_sh, last_sh = last_sh, ph
        if pl is not None:
            prev_sl, last_sl = last_sl, pl
        flags = [
            last_sl is not None and lo[t] < last_sl,
            last_sl is not None and lo[t] < last_sl and c[t] > last_sl,
            last_sh is not None and h[t] > last_sh,
            last_sh is not None and h[t] > last_sh and c[t] < last_sh,
        ]
        for i in range(4):
            if flags[i] and not flags_prev[i]:
                counts[i] += 1
        if t % decay_every == 0:
            counts = [max(0, k - 1) for k in counts]
        flags_prev = flags

        need = (atr[t], atr_avg[t], bbw[t], bbw_avg[t], rng_avg[t], rsi[t])
        if None in need or t < LOOKBACK or rsi[t - LOOKBACK] is None:
            continue

        atr_ratio = atr[t] / atr_avg[t] if atr_avg[t] > 0 else 1.0
        bbw_ratio = bbw[t] / bbw_avg[t] if bbw_avg[t] > 0 else 1.0
        net = abs(c[t] - c[t - LOOKBACK])
        path = sum(abs(c[j] - c[j - 1]) for j in range(t - LOOKBACK + 1, t + 1))
        er = net / path if path > 0 else 1.0
        candle_ratio = rng[t] / rng_avg[t] if rng_avg[t] > 0 else 1.0
        hh = max(h[t - LOOKBACK + 1:t + 1])
        ll = min(lo[t - LOOKBACK + 1:t + 1])
        range_w = (hh - ll) / atr[t] if atr[t] > 0 else 0.0

        bull_bos = last_sh is not None and c[t] > last_sh and c[t - 1] <= last_sh
        bear_bos = last_sl is not None and c[t] < last_sl and c[t - 1] >= last_sl
        hl = last_sl is not None and prev_sl is not None and last_sl > prev_sl
        ll_ = last_sl is not None and prev_sl is not None and last_sl < prev_sl
        lh = last_sh is not None and prev_sh is not None and last_sh < prev_sh
        hh_ = last_sh is not None and prev_sh is not None and last_sh > prev_sh
        slope = (ema20[t] - ema20[t - 3]) / 3
        flat_slope = -0.0001 * c[t] < slope < 0.0001 * c[t]
        was_down = ema20[t - LOOKBACK] < ema50[t - LOOKBACK]
        was_up = ema20[t - LOOKBACK] > ema50[t - LOOKBACK]
        now_flat = abs(ema20[t] - ema50[t]) < atr[t] * 0.5

        tl = 0.0
        if was_down and now_flat:
            tl += 1.0
        if was_down and not bear_bos and not ll_:
            tl += 0.5
        if flat_slope and was_down:
            tl += 0.5
        tl = min(tl, 2.0)
        rs = 0.0
        if er < ER_RANGE:
            rs += 1.0
        if er < ER_RANGE * 0.5:
            rs += 0.5
        if 2.0 < range_w < 8.0:
            rs += 0.5
        rs = min(rs, 2.0)
        vc = 0.0
        if bbw_ratio < BBW_COMP:
            vc += 1.0
        if atr_ratio < ATR_COMP:
            vc += 0.5
        if candle_ratio < 0.7:
            vc += 0.5
        vc = min(vc, 2.0)
        lq = min(0.5 * (counts[0] >= 1) + 0.5 * (counts[0] >= 2) + 0.5 * (counts[1] >= 1)
                 + 0.5 * (counts[1] >= 2), 2.0)
        bs = 0.0
        if hl:
            bs += 1.0
        if bull_bos:
            bs += 1.0
        if rsi[t] > 40 and rsi[t - LOOKBACK] < 35:
            bs += 0.5
        bs = min(bs, 2.0)
        ar = rs if tl > 0 else 0.0
        av = vc if tl > 0 else 0.0
        a_score = tl + ar + av + lq + bs

        dtl = 0.0
        if was_up and now_flat:
            dtl += 1.0
        if was_up and not bull_bos and not hh_:
            dtl += 0.5
        if flat_slope and was_up:
            dtl += 0.5
        dtl = min(dtl, 2.0)
        dlq = min(0.5 * (counts[2] >= 1) + 0.5 * (counts[2] >= 2) + 0.5 * (counts[3] >= 1)
                  + 0.5 * (counts[3] >= 2), 2.0)
        ds = 0.0
        if lh:
            ds += 1.0
        if bear_bos:
            ds += 1.0
        if rsi[t] < 60 and rsi[t - LOOKBACK] > 65:
            ds += 0.5
        ds = min(ds, 2.0)
        dr = rs if dtl > 0 else 0.0
        dv = vc if dtl > 0 else 0.0
        d_score = dtl + dr + dv + dlq + ds

        def state_of(score, start, struct, liq, rng_s):
            active = score >= 3.0
            if active and start is None:
                start = t
            if not active:
                start = None
            dur = t - start if start is not None else 0
            if not active:
                st = 0
            elif dur < MIN_PHASE:
                st = 1
            elif score >= 8.0 and struct >= 1.5:
                st = 5
            elif score >= 6.0 and liq >= 1.0 and rng_s >= 1.0:
                st = 4
            elif score >= 5.0:
                st = 3
            else:
                st = 2
            return st, start, dur

        a_st, a_start, a_dur = state_of(a_score, a_start, bs, lq, ar)
        d_st, d_start, d_dur = state_of(d_score, d_start, ds, dlq, dr)
        for st, prev, names in ((a_st, a_prev, EVENTS[:3]), (d_st, d_prev, EVENTS[3:])):
            if st >= 3 and st != prev:
                events[names[st - 3]].append(t)
        a_prev, d_prev = a_st, d_st

        out["reported"][t] = True
        out["rows"][t] = {
            "accum": a_score, "dist": d_score,
            "accum_trend_loss": tl, "accum_range": ar, "accum_compression": av,
            "accum_liquidity": lq, "accum_structure": bs,
            "dist_trend_loss": dtl, "dist_range": dr, "dist_compression": dv,
            "dist_liquidity": dlq, "dist_structure": ds,
            "accum_bars": float(a_dur), "dist_bars": float(d_dur),
            "accum_state": float(a_st), "dist_state": float(d_st),
            "efficiency": er, "bbw_ratio": bbw_ratio, "atr_ratio": atr_ratio, "range_atr": range_w,
        }
    out["events"] = events
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Accumulation Phase",
    "",
    *provenance("accumulation_phase"),
    "",
    "Formula: the original definition with its",
    "defaults (ATR 14, lookback 20, minimum phase 10 bars, pivots 3/3, ER 0.3, BBW 0.7, ATR 0.8;",
    "EMA 20/50, Bollinger 20 x 2, RSI 14 fixed). Conventions the original leaves open: EMAs",
    "seeded with the first close, ATR = Wilder average of the true range (first bar high - low)",
    "seeded with an SMA, RSI = Wilder averages of the close changes seeded with an SMA, population",
    "standard deviation, pivots strict on both sides; the sweep/spring counters run from the first",
    "bar (decay when the bar index is a multiple of round(lookback/2)), scores, phases and output",
    "from the first bar on which every input exists.",
    "",
    "Series: 900 bars, prev = previous close (150 before the first bar); (drift, amp) = (-0.35,",
    "0.6) up to bar 149, (0, 0.3) to 329, (0.40, 0.6) to 479, (0, 0.3) to 679, (-0.38, 0.6) after;",
    "pull = -0.2 (prev - 97) for bars 150-329, -0.2 (prev - 157) for 480-679, else 0; close =",
    "prev + drift + pull + amp (sin(i/5) + 0.6 sin(i/7.9)); open = prev + 0.25 sin(i/1.9); high =",
    "max(open, close) + 0.3 + 0.3 |sin(i/3.3)|, plus 1.8 for 480 <= i < 680 with i mod 13 = 9; low",
    "= min(open, close) - 0.3 - 0.3 |cos(i/4.4)|, minus 1.8 for 150 <= i < 330 with i mod 13 = 7.",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n the",
    "number of alerts of that kind and ev_<kind>_<j> the bar of the j-th (up to 10).",
]


def build():
    out = compute(_series())
    first = next(t for t in range(N_BARS) if out["reported"][t])
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        blocks.append((f"cp{cp}", f"bar {cp}", dict(out["rows"][cp])))
    ev = {}
    for name in EVENTS:
        bars = out["events"][name]
        ev[f"{name}_n"] = float(len(bars))
        for j, b in enumerate(bars[:10]):
            ev[f"{name}_{j}"] = float(b)
    blocks.append(("ev", "events", ev))
    blocks.append((None, None, {"vacc_tolerance": 1e-9}))
    return HEADER, blocks
