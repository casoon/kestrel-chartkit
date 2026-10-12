"""golden_liquidity_hunter: Liquidity Hunter, levels and events.

Written from the published Pine Script (WavesUnchained), not from the Rust code, with the inputs
at their defaults (fair value gaps, regime context, level flip, session filter and the exhaustion
layer do not reach the outputs checked here; the session filter being off adds the session boost
of 10 to every reclaim score). The script is followed bar by bar with its arrays of levels.
"""

import math

from ..fixture import provenance

NAME = "golden_liquidity_hunter"

PIVOT_LEN = 8
ATR_LEN = 14
EQ_TOL_ATR = 0.12
MIN_TOUCHES = 2
MAX_TRACKED = 30
MAX_AGE = 400
MAX_DIST_ATR = 8.0
MIN_SWEEP_ATR = 0.05
RECLAIM_BARS = 3
MIN_CLOSE_BACK_ATR = 0.10
MIN_VOLUME_RATIO = 1.2
MAX_EVENTS = 6
HUNT_VOL_MULT = 1.5
HUNT_WICK_MIN = 0.45
HUNT_CLOSE_POS_MAX = 0.45
ALERT_DIST_ATR = 1.0
EVENT_MEMORY = 30
PREMIUM = 65.0

N_BARS = 800
CHECKPOINTS = [60, 250, 270, 345, 440, 635, 730, 799]
EVENTS = [
    "bear_bsl_sweep", "bull_bsl_breakout", "bear_bsl_stop_hunt", "bull_ssl_sweep",
    "bear_ssl_breakout", "bull_ssl_stop_hunt", "bear_reclaim", "bear_premium_reclaim",
    "bull_reclaim", "bull_premium_reclaim", "primary_bsl_near", "primary_ssl_near",
]


def _series(with_volume):
    """A range of period 32 on plateaus of 96 bars, wick spikes through the third swing of each."""
    steps = [2.5, -1.2, 3.5, -4.0]
    bars = []
    for i in range(N_BARS):
        x = float(i)
        k = i // 96
        level = 100.0 + sum(steps[j % 4] for j in range(k))
        close = level + 2.5 * math.sin(2.0 * math.pi * x / 32.0) + 0.04 * math.sin(x / 2.9)
        open_ = level + 2.5 * math.sin(2.0 * math.pi * (x - 0.6) / 32.0)
        high = max(open_, close) + 0.35 + 0.03 * abs(math.sin(x / 5.3))
        low = min(open_, close) - 0.35 - 0.03 * abs(math.cos(x / 4.1))
        vol = 1000.0 + 200.0 * math.sin(x / 6.0) ** 2
        if i % 96 == 72 and k % 4 < 2:
            high += 1.6
            vol *= 2.2
        if i % 96 == 88 and k % 4 < 2:
            low -= 1.6
            vol *= 2.2
        if i % 192 == 104:
            close = high - 0.05
            vol *= 1.6
        if not with_volume:
            vol = 0.0
        bars.append((open_, high, low, close, vol))
    return bars


def _rma(src, n):
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


def _pivot(vals, t, n, high):
    c = t - n
    if c - n < 0:
        return None
    v = vals[c]
    for j in range(c - n, t + 1):
        if j != c and ((high and not v > vals[j]) or (not high and not v < vals[j])):
            return None
    return v


def _level_score(touches, age, dist):
    touch = min(touches, 5) * 20.0
    age_pen = min(age / MAX_AGE, 1.0) * 18.0
    dist_pen = min(dist, MAX_DIST_ATR) * 7.0
    return max(0.0, touch + 18.0 - age_pen - dist_pen)


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, ATR_LEN)

    bsl, ssl, recent = [], [], []
    cum = 0.0
    near_prev = (False, False)
    out = {"reported": [False] * n, "bias": [None] * n, "levels": [None] * n,
           "counts": [None] * n, "recent": [None] * n, "volume_ratio": [None] * n}
    events = {k: [] for k in EVENTS}
    scores = []

    for t in range(n):
        cum += v[t]
        volume_real = cum > 0
        avg_vol = sum(v[t - 19:t + 1]) / 20.0 if t >= 19 else None
        vol_ratio = v[t] / avg_vol if avg_vol is not None and avg_vol > 0 else 1.0
        a = atr[t]
        tol = a * EQ_TOL_ATR if a is not None else None

        for levels, high in ((bsl, True), (ssl, False)):
            piv = _pivot(h if high else lo, t, PIVOT_LEN, high)
            if piv is None:
                continue
            piv_bar = t - PIVOT_LEN
            found = False
            for lvl in levels:
                if tol is not None and abs(lvl["price"] - piv) <= tol:
                    lvl["touches"] += 1
                    lvl["last"] = piv_bar
                    lvl["price"] = (lvl["price"] * (lvl["touches"] - 1) + piv) / lvl["touches"]
                    found = True
                    break
            if not found:
                levels.append({"price": piv, "pivot": piv_bar, "last": piv_bar, "touches": 1,
                               "swept": False, "sweep_bar": None, "reclaimed": False})
            if len(levels) > MAX_TRACKED:
                levels.pop(0)

        if a is not None and a > 0:
            for levels in (bsl, ssl):
                levels[:] = [l for l in levels
                             if not (t - l["last"] > MAX_AGE or abs(c[t] - l["price"]) / a > MAX_DIST_ATR)]

        rng = max(h[t] - lo[t], 1e-12)
        upper_wick = (h[t] - max(o[t], c[t])) / rng
        lower_wick = (min(o[t], c[t]) - lo[t]) / rng
        close_pos = (c[t] - lo[t]) / rng

        def event_score(touches, wick_atr, back_atr, struct_bonus):
            touch = min(touches, 5) * 12.0
            wick = min(wick_atr, 1.5) * 18.0
            back = min(back_atr, 1.5) * 24.0
            vol = max(0.0, min(vol_ratio, 3.0) - 1.0) * 12.0 if volume_real else 0.0
            scale = 1.0 if volume_real else 147.0 / 123.0
            return min(100.0, (touch + wick + back + vol) * scale + 10.0 + struct_bonus)

        fired = set()
        reclaim = {"bear": None, "bull": None}
        if a is not None:
            sweep_abs = a * MIN_SWEEP_ATR
            back_abs = a * MIN_CLOSE_BACK_ATR
            for lvl in bsl:
                if lvl["touches"] >= MIN_TOUCHES and lvl["pivot"] < t:
                    wick_atr = max(0.0, h[t] - lvl["price"]) / a if a > 0 else 0.0
                    back_atr = max(0.0, lvl["price"] - c[t]) / a if a > 0 else 0.0
                    if not lvl["swept"] and h[t] >= lvl["price"] + sweep_abs:
                        lvl["swept"], lvl["sweep_bar"] = True, t
                        if c[t] >= lvl["price"] + sweep_abs:
                            fired.add("bull_bsl_breakout")
                        else:
                            fired.add("bear_bsl_sweep")
                            if upper_wick >= HUNT_WICK_MIN and close_pos <= HUNT_CLOSE_POS_MAX and \
                                    (not volume_real or vol_ratio >= HUNT_VOL_MULT):
                                fired.add("bear_bsl_stop_hunt")
                    can = lvl["swept"] and not lvl["reclaimed"] and t <= lvl["sweep_bar"] + RECLAIM_BARS
                    if can and c[t] <= lvl["price"] - back_abs and \
                            (not volume_real or vol_ratio >= MIN_VOLUME_RATIO) and h[t] >= lvl["price"]:
                        lvl["reclaimed"] = True
                        bonus = 10.0 if t >= 1 and c[t] < lo[t - 1] else 0.0
                        s = event_score(lvl["touches"], wick_atr, back_atr, bonus)
                        reclaim["bear"] = s if reclaim["bear"] is None else max(reclaim["bear"], s)
                        recent.insert(0, (t, -1, s))
                        if len(recent) > MAX_EVENTS:
                            recent.pop()
            for lvl in ssl:
                if lvl["touches"] >= MIN_TOUCHES and lvl["pivot"] < t:
                    wick_atr = max(0.0, lvl["price"] - lo[t]) / a if a > 0 else 0.0
                    back_atr = max(0.0, c[t] - lvl["price"]) / a if a > 0 else 0.0
                    if not lvl["swept"] and lo[t] <= lvl["price"] - sweep_abs:
                        lvl["swept"], lvl["sweep_bar"] = True, t
                        if c[t] <= lvl["price"] - sweep_abs:
                            fired.add("bear_ssl_breakout")
                        else:
                            fired.add("bull_ssl_sweep")
                            if lower_wick >= HUNT_WICK_MIN and (1.0 - close_pos) <= HUNT_CLOSE_POS_MAX and \
                                    (not volume_real or vol_ratio >= HUNT_VOL_MULT):
                                fired.add("bull_ssl_stop_hunt")
                    can = lvl["swept"] and not lvl["reclaimed"] and t <= lvl["sweep_bar"] + RECLAIM_BARS
                    if can and c[t] >= lvl["price"] + back_abs and \
                            (not volume_real or vol_ratio >= MIN_VOLUME_RATIO) and lo[t] <= lvl["price"]:
                        lvl["reclaimed"] = True
                        bonus = 10.0 if t >= 1 and c[t] > h[t - 1] else 0.0
                        s = event_score(lvl["touches"], wick_atr, back_atr, bonus)
                        reclaim["bull"] = s if reclaim["bull"] is None else max(reclaim["bull"], s)
                        recent.insert(0, (t, 1, s))
                        if len(recent) > MAX_EVENTS:
                            recent.pop()
        recent[:] = [e for e in recent if t - e[0] <= EVENT_MEMORY]

        if a is None:
            continue

        def rank(levels, above):
            best = second = None
            count = 0
            for lvl in levels:
                side_ok = lvl["price"] > c[t] if above else lvl["price"] < c[t]
                if lvl["touches"] >= MIN_TOUCHES and not lvl["swept"] and not lvl["reclaimed"] \
                        and side_ok and a > 0:
                    score = _level_score(lvl["touches"], t - lvl["last"], abs(lvl["price"] - c[t]) / a)
                    count += 1
                    if score > (best[1] if best else 0.0):
                        second, best = best, (lvl["price"], score, lvl["touches"])
                    elif score > (second[1] if second else 0.0):
                        second = (lvl["price"], score, lvl["touches"])
            return best, second, count

        p_bsl, s_bsl, n_bsl = rank(bsl, True)
        p_ssl, s_ssl, n_ssl = rank(ssl, False)
        near_bsl = p_bsl is not None and a > 0 and (p_bsl[0] - c[t]) / a <= ALERT_DIST_ATR
        near_ssl = p_ssl is not None and a > 0 and (c[t] - p_ssl[0]) / a <= ALERT_DIST_ATR
        r_bull = max([e[2] for e in recent if e[1] == 1], default=0.0)
        r_bear = max([e[2] for e in recent if e[1] == -1], default=0.0)
        pb = p_bsl[1] if p_bsl else 0.0
        ps = p_ssl[1] if p_ssl else 0.0
        bias = 0
        bias += 2 if reclaim["bull"] is not None else (1 if r_bull >= PREMIUM else 0)
        bias -= 2 if reclaim["bear"] is not None else (1 if r_bear >= PREMIUM else 0)
        bias += 1 if ps > pb and near_ssl else 0
        bias -= 1 if pb > ps and near_bsl else 0

        for side in ("bear", "bull"):
            if reclaim[side] is not None:
                fired.add(f"{side}_reclaim")
                scores.append((t, reclaim[side]))
                if reclaim[side] >= PREMIUM:
                    fired.add(f"{side}_premium_reclaim")
        if near_bsl and not near_prev[0]:
            fired.add("primary_bsl_near")
        if near_ssl and not near_prev[1]:
            fired.add("primary_ssl_near")
        near_prev = (near_bsl, near_ssl)
        for k in fired:
            events[k].append(t)

        out["reported"][t] = True
        out["bias"][t] = float(bias)
        out["levels"][t] = (p_bsl, s_bsl, p_ssl, s_ssl)
        out["counts"][t] = (float(n_bsl), float(n_ssl))
        out["recent"][t] = (r_bull, r_bear)
        out["volume_ratio"][t] = vol_ratio
    out["events"] = events
    out["scores"] = scores
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Liquidity Hunter",
    "",
    *provenance("liquidity_hunter"),
    "",
    "Formula: the published Pine Script 'Liquidity Hunter' (WavesUnchained) with its defaults",
    "(pivots 8, tolerance 0.12 ATR, 2 touches, 30 levels per side, age 400, distance 8 ATR,",
    "sweep 0.05 ATR, reclaim window 3, close-back 0.10 ATR, volume ratio 1.2, 6 remembered events,",
    "hunt volume 1.5 / wick 0.45 / close position 0.45, alert distance 1 ATR, memory 30, premium",
    "65). Conventions where Pine leaves them open: ATR(14) = Wilder average of the true range",
    "(first bar high - low) seeded with an SMA, no merge/prune/sweep while it is missing, the",
    "2 x mintick tolerance floor dropped and the bar range floored at 1e-12, volume ratio 1 while",
    "the 20-bar average is missing or 0, real volume = cumulative volume so far positive, pivots",
    "strict on both sides; output from the first bar with an ATR.",
    "",
    "Series: 800 bars on plateaus k = floor(i/96), level = 100 + the sum of the first k steps of",
    "the cycle 2.5, -1.2, 3.5, -4.0; close = level + 2.5 sin(2 pi i/32) + 0.04 sin(i/2.9); open =",
    "level + 2.5 sin(2 pi (i - 0.6)/32); high = max(open, close) + 0.35 + 0.03 |sin(i/5.3)|; low =",
    "min(open, close) - 0.35 - 0.03 |cos(i/4.1)|; volume = 1000 + 200 sin(i/6)^2. When i mod 96 =",
    "72 and k mod 4 < 2: high + 1.6, volume x 2.2; when i mod 96 = 88 and k mod 4 < 2: low - 1.6,",
    "volume x 2.2; when i mod 192 =",
    "104 (after those): close = high - 0.05, volume x 1.6. Block 'v' with that volume, block 'nv'",
    "with volume 0 everywhere.",
    "",
    "<v|nv>_first is the first reporting bar; _cp<bar>_* the outputs at that bar (level keys only",
    "where the level exists); _ev_<kind>_n the number of alerts of that kind and _ev_<kind>_<j>",
    "the bar of the j-th (up to 10); _score<j>_bar/_value the first 10 reclaim scores.",
]


def _block(prefix, out):
    case = {"first": float(next(t for t in range(N_BARS) if out["reported"][t]))}
    for cp in CHECKPOINTS:
        p = f"cp{cp}"
        case[f"{p}_bias"] = out["bias"][cp]
        names = ("primary_bsl", "secondary_bsl", "primary_ssl", "secondary_ssl")
        for name, lv in zip(names, out["levels"][cp]):
            if lv is not None:
                case[f"{p}_{name}"] = lv[0]
                case[f"{p}_{name}_score"] = lv[1]
        case[f"{p}_bsl_count"], case[f"{p}_ssl_count"] = out["counts"][cp]
        case[f"{p}_recent_bull_score"], case[f"{p}_recent_bear_score"] = out["recent"][cp]
        case[f"{p}_volume_ratio"] = out["volume_ratio"][cp]
    for k in EVENTS:
        bars = out["events"][k]
        case[f"ev_{k}_n"] = float(len(bars))
        for j, b in enumerate(bars[:10]):
            case[f"ev_{k}_{j}"] = float(b)
    for j, (t, s) in enumerate(out["scores"][:10]):
        case[f"score{j}_bar"] = float(t)
        case[f"score{j}_value"] = s
    return (prefix, None, case)


def build():
    blocks = [
        _block("v", compute(_series(True))),
        _block("nv", compute(_series(False))),
        (None, None, {"lqh_tolerance": 1e-9}),
    ]
    return HEADER, blocks
