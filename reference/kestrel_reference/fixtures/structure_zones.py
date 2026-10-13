"""golden_structure_zones: Structure Zones, zones and assessment.

Written from the original definition, not from the Rust code, with the
inputs at their defaults. Series are full arrays (`x[k]` read as `series[t - k]`); the arrays of
gaps, order blocks and zones are followed bar by bar as in the script.
"""

import math

from ..fixture import provenance

NAME = "golden_structure_zones"

N_BARS = 900
CHECKPOINTS = [60, 150, 260, 333, 444, 555, 666, 777, 899]
EVENTS = ["support_relevant", "resistance_relevant", "bull_support_hold", "bear_resistance_hold",
          "bear_support_break", "bull_resistance_break"]
ZONE_KEYS = ["mid", "upper", "lower", "type", "status", "distance_atr", "reach", "hold", "break",
             "structural", "respect", "approach", "context", "state"]
STATE = {"STABLE": 0, "HOLD POSSIBLE": 1, "WEAKENING": 2, "UNDER PRESSURE": 3,
         "REACTION LIKELY": 4, "BREAK LIKELY": 5}


def _series(with_volume):
    """Ranges and trends with wick spikes, wide bars and gaps."""
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.3 * math.sin(x / 70.0)
        close = prev + drift + 0.8 * math.sin(x / 5.3) + 0.4 * math.cos(x / 1.9)
        open_ = prev + 0.3 * math.sin(x / 2.4)
        high = max(open_, close) + 0.2 + 0.3 * abs(math.sin(x / 3.1))
        low = min(open_, close) - 0.2 - 0.3 * abs(math.cos(x / 2.6))
        vol = 1000.0 + 600.0 * math.sin(x / 4.5) ** 2
        if i % 29 == 13:
            low -= 1.6
            vol *= 2.0
        if i % 31 == 21:
            high += 1.6
            vol *= 2.0
        if i % 47 == 33:
            close = prev + 3.2
            open_ = prev - 0.2
            high = close + 0.2
            low = open_ - 0.2
            vol *= 2.8
        if not with_volume:
            vol = 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


def _ema(src, n):
    a = 2.0 / (n + 1.0)
    out, prev = [], None
    for v in src:
        prev = v if prev is None else a * v + (1.0 - a) * prev
        out.append(prev)
    return out


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
        out[t] = sum(src[t - n + 1:t + 1]) / n
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


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, 14)
    ema20, ema50 = _ema(c, 20), _ema(c, 50)
    up = [None] + [max(c[t] - c[t - 1], 0.0) for t in range(1, n)]
    dn = [None] + [max(c[t - 1] - c[t], 0.0) for t in range(1, n)]
    au, ad = _rma(up, 14), _rma(dn, 14)
    rsi = [None] * n
    for t in range(n):
        if au[t] is not None:
            rsi[t] = 50.0 if au[t] == 0 and ad[t] == 0 else (
                100.0 if ad[t] == 0 else 100.0 - 100.0 / (1.0 + au[t] / ad[t]))
    hlc3 = [(h[t] + lo[t] + c[t]) / 3.0 for t in range(n)]
    mfi = [None] * n
    for t in range(14, n):
        pos = neg = 0.0
        for j in range(t - 13, t + 1):
            if hlc3[j] > hlc3[j - 1]:
                pos += hlc3[j] * v[j]
            elif hlc3[j] < hlc3[j - 1]:
                neg += hlc3[j] * v[j]
        mfi[t] = 50.0 if pos == 0 and neg == 0 else (100.0 if neg == 0 else 100.0 - 100.0 / (1.0 + pos / neg))
    avg_vol = _sma(v, 20)
    rng = [h[t] - lo[t] for t in range(n)]
    approach_avg = _sma(rng, 5)
    evr = [v[t] / rng[t] if rng[t] > 0 else 0.0 for t in range(n)]
    evr_avg = _sma(evr, 14)
    evr_norm = [evr[t] / evr_avg[t] if evr_avg[t] is not None and evr_avg[t] > 0 else 1.0
                for t in range(n)]
    uwr = [(h[t] - max(o[t], c[t])) / rng[t] if rng[t] > 0 else 0.0 for t in range(n)]
    lwr = [(min(o[t], c[t]) - lo[t]) / rng[t] if rng[t] > 0 else 0.0 for t in range(n)]

    cum = 0.0
    sh = sl = msh = msl = None
    prev_sh = prev_sl = None
    prev_msh = prev_msl = None
    prev_msh_bar = prev_msl_bar = None
    poc = va_hi = va_lo = None
    micro_trend = major_trend = 0
    clx_state = clx_dir = 0
    clx = None
    clx_test_bar = None
    bars_since_test = 999
    gaps, bull_obs, bear_obs, zones = [], [], [], []
    prev_flags = [False] * 6
    out = {"reported": [False] * n, "rows": [None] * n, "events": {k: [] for k in EVENTS}}

    def nz_evr(t, k):
        return evr_norm[t - k] if t - k >= 0 else 0.0

    for t in range(n):
        cum += v[t]
        real = cum > 0
        a = atr[t]
        rel_vol = v[t] / avg_vol[t] if real and avg_vol[t] is not None and avg_vol[t] > 0 else 1.0
        body_ratio = abs(c[t] - o[t]) / rng[t] if rng[t] > 0 else 0.0
        catr = rng[t] / a if a is not None and a > 0 else 0.0
        if t >= 10:
            path = sum(abs(c[j] - c[j - 1]) for j in range(t - 9, t + 1))
            er = abs(c[t] - c[t - 10]) / path if path > 0 else 0.0
        else:
            er = 0.0
        t_up = ema20[t] > ema50[t] and er > 0.35
        t_dn = ema20[t] < ema50[t] and er > 0.35
        is_rng = er <= 0.35

        ph, pl = _pivot(h, t, 3, True), _pivot(lo, t, 3, False)
        if ph is not None:
            sh = ph
        if pl is not None:
            sl = pl
        mph, mpl = _pivot(h, t, 7, True), _pivot(lo, t, 7, False)
        if mph is not None:
            msh = mph
        if mpl is not None:
            msl = mpl
        eqh = eql = None
        if ph is not None:
            if prev_sh is not None and a is not None and a > 0 and abs(ph - prev_sh) < 0.1 * a:
                eqh = (ph + prev_sh) / 2.0
            prev_sh = ph
        if pl is not None:
            if prev_sl is not None and a is not None and a > 0 and abs(pl - prev_sl) < 0.1 * a:
                eql = (pl + prev_sl) / 2.0
            prev_sl = pl
        # A first major pivot counts as new (IEEE: na != x).
        new_msh = mph is not None and mph != prev_msh
        new_msl = mpl is not None and mpl != prev_msl
        if new_msh:
            prev_msh, prev_msh_bar = mph, t - 7
        if new_msl:
            prev_msl, prev_msl_bar = mpl, t - 7
        if (new_msh or new_msl) and real:
            leg_start = leg_end = None
            if new_msh and prev_msl_bar is not None:
                leg_start, leg_end = prev_msl_bar, t - 7
            elif new_msl and prev_msh_bar is not None:
                leg_start, leg_end = prev_msh_bar, t - 7
            if leg_start is not None and leg_end > leg_start:
                leg_len = leg_end - leg_start
                lh, ll = h[t], lo[t]
                for bk in range(0, min(leg_len, 200) + 1):
                    off = t - leg_end + bk
                    lh = max(lh, h[t - off])
                    ll = min(ll, lo[t - off])
                step = (lh - ll) / 15 if lh > ll else 0.0
                if step > 0:
                    bins = [0.0] * 15
                    for bk in range(0, min(leg_len, 200) + 1):
                        j = t - (t - leg_end + bk)
                        if v[j] > 0:
                            for row in range(15):
                                rlo = ll + row * step
                                rhi = rlo + step
                                if h[j] >= rlo and lo[j] < rhi:
                                    ov = min(h[j], rhi) - max(lo[j], rlo)
                                    br = h[j] - lo[j]
                                    bins[row] += v[j] * (ov / br if br > 0 else 1.0 / 15)
                    best, best_v = 0, 0.0
                    for i in range(15):
                        if bins[i] > best_v:
                            best_v, best = bins[i], i
                    poc = ll + (best + 0.5) * step
                    tot = sum(bins)
                    va_v, vlo, vhi = best_v, best, best
                    for _ in range(15):
                        if va_v >= tot * 0.70:
                            break
                        below = bins[vlo - 1] if vlo > 0 else 0.0
                        above = bins[vhi + 1] if vhi < 14 else 0.0
                        if below >= above and vlo > 0:
                            vlo -= 1
                            va_v += below
                        elif vhi < 14:
                            vhi += 1
                            va_v += above
                        elif vlo > 0:
                            vlo -= 1
                            va_v += below
                        else:
                            break
                    va_hi = ll + (vhi + 1) * step
                    va_lo = ll + vlo * step

        c1 = c[t - 1] if t >= 1 else None
        micro_bull = sh is not None and c1 is not None and c[t] > sh and c1 <= sh
        micro_bear = sl is not None and c1 is not None and c[t] < sl and c1 >= sl
        major_bull = msh is not None and c1 is not None and c[t] > msh and c1 <= msh
        major_bear = msl is not None and c1 is not None and c[t] < msl and c1 >= msl
        bull_choch = (micro_bull and micro_trend == -1) or (major_bull and major_trend == -1)
        bear_choch = (micro_bear and micro_trend == 1) or (major_bear and major_trend == 1)
        if micro_bull:
            micro_trend = 1
        if micro_bear:
            micro_trend = -1
        if major_bull:
            major_trend = 1
        if major_bear:
            major_trend = -1
        fake_up = sh is not None and h[t] > sh and c[t] <= sh
        fake_dn = sl is not None and lo[t] < sl and c[t] >= sl
        mfake_up = msh is not None and h[t] > msh and c[t] <= msh
        mfake_dn = msl is not None and lo[t] < msl and c[t] >= msl
        spring = fake_dn and lwr[t] >= 0.35 and catr >= 0.5
        upthrust = fake_up and uwr[t] >= 0.35 and catr >= 0.5
        mspring = mfake_dn and lwr[t] >= 0.35 and catr >= 0.5
        mupthrust = mfake_up and uwr[t] >= 0.35 and catr >= 0.5
        bull_rej = lwr[t] >= 0.5 and body_ratio <= 0.3 and c[t] > o[t]
        bear_rej = uwr[t] >= 0.5 and body_ratio <= 0.3 and c[t] < o[t]
        climax = catr >= 1.8 and ((avg_vol[t] is not None and avg_vol[t] > 0
                                   and v[t] >= 1.8 * avg_vol[t]) if real else True)
        evr_falling = nz_evr(t, 0) < nz_evr(t, 1) < nz_evr(t, 2) < nz_evr(t, 3)
        absorption = real and evr_falling and rel_vol > 1.2

        bars_since_test += 1
        test_candle = False
        gap_sup = gap_res = False
        ob_sup = ob_res = False
        ob_sup_sc = ob_res_sc = 0.0
        if a is not None:
            if climax:
                clx_state = 1
                clx_dir = 1 if c[t] < o[t] else (-1 if c[t] > o[t] else 0)
                clx = (c[t], h[t], lo[t], rng[t], v[t], t)
                clx_test_bar = None
            if clx is not None:
                cc, ch, cl, cr, cv, cb = clx
                active = clx_state >= 1 and t - cb <= 20
                if active and clx_state == 1:
                    if clx_dir == 1 and c[t] > cc:
                        clx_state = 2
                    if clx_dir == -1 and c[t] < cc:
                        clx_state = 2
                if active and clx_state == 2:
                    back = False
                    if clx_dir == 1 and a > 0:
                        back = c[t] < cc + 1.5 * a and c[t] > cl - 0.5 * a
                    if clx_dir == -1 and a > 0:
                        back = c[t] > cc - 1.5 * a and c[t] < ch + 0.5 * a
                    if back and t - cb >= 2:
                        clx_state = 3
                if active and clx_state == 3:
                    test = rng[t] <= cr * 0.6 and rng[t] > 0 and (v[t] <= cv * 0.7 if real else True)
                    in_zone = a > 0 and abs(c[t] - cc) <= 1.5 * a
                    if test and in_zone:
                        clx_state = 4
                        clx_test_bar = t
                if active and clx_state == 4:
                    if clx_dir == 1 and (micro_bull or major_bull):
                        clx_state = 5
                    if clx_dir == -1 and (micro_bear or major_bear):
                        clx_state = 5
            test_candle = clx_state == 4 and clx_test_bar == t and bars_since_test >= 3
            if test_candle:
                bars_since_test = 0

            gap_min = a * 0.3
            if t >= 2 and lo[t] > h[t - 2] and lo[t] - h[t - 2] >= gap_min:
                gaps.append({"upper": lo[t], "lower": h[t - 2], "dir": 1, "bar": t})
                if len(gaps) > 10:
                    gaps.pop(0)
            if t >= 2 and h[t] < lo[t - 2] and lo[t - 2] - h[t] >= gap_min:
                gaps.append({"upper": lo[t - 2], "lower": h[t], "dir": -1, "bar": t})
                if len(gaps) > 10:
                    gaps.pop(0)
            for i in range(len(gaps) - 1, -1, -1):
                g = gaps[i]
                if t - g["bar"] > 20:
                    gaps.pop(i)
                    continue
                if (g["dir"] == 1 and lo[t] <= g["lower"]) or (g["dir"] == -1 and h[t] >= g["upper"]):
                    gaps.pop(i)
                    continue
                if g["dir"] == 1 and g["lower"] < c[t] < g["upper"] + a:
                    gap_sup = True
                if g["dir"] == -1 and g["lower"] - a < c[t] < g["upper"]:
                    gap_res = True

            for bull, bos, obs in ((True, micro_bull or major_bull, bull_obs),
                                   (False, micro_bear or major_bear, bear_obs)):
                if not bos:
                    continue
                k = None
                for lb in range(1, 11):
                    if t - lb >= 0 and ((c[t - lb] < o[t - lb]) if bull else (c[t - lb] > o[t - lb])):
                        k = lb
                        break
                if k is None:
                    continue
                up_, lw_ = h[t - k], lo[t - k]
                d = 1 if bull else -1
                overlap = any(g["dir"] == d and g["upper"] >= lw_ and g["lower"] <= up_ for g in gaps)
                score = (30.0 if catr >= 1.5 else 0.0) + 25.0 + (20.0 if overlap else 0.0) \
                    + (15.0 if rel_vol >= 1.3 else 0.0)
                obs.append({"upper": up_, "lower": lw_, "mit": False, "score": score})
                if len(obs) > 5:
                    obs.pop(0)
            for obs, is_sup in ((bull_obs, True), (bear_obs, False)):
                for ob in reversed(obs):
                    if ob["mit"]:
                        continue
                    if lo[t] <= ob["upper"] and h[t] >= ob["lower"]:
                        ob["mit"] = True
                        continue
                    dist = abs(c[t] - (ob["upper"] + ob["lower"]) / 2.0) / a if a > 0 else 999.0
                    if dist < 2.0:
                        if is_sup:
                            ob_sup, ob_sup_sc = True, max(ob_sup_sc, ob["score"])
                        else:
                            ob_res, ob_res_sc = True, max(ob_res_sc, ob["score"])

            def add(kind, upper, lower, d, q):
                zones.append({"kind": kind, "upper": upper, "lower": lower, "mid": (upper + lower) / 2.0,
                              "dir": d, "q": q, "bar": t, "status": 0, "touch": 0, "good": 0,
                              "weak": 0, "maxb": 0.0, "lastb": 0.0, "deep": 0, "vb": False})

            half = a * 0.3 / 2.0
            if mph is not None:
                add(2, mph + half, mph - half, -1, 2)
            if mpl is not None:
                add(1, mpl + half, mpl - half, 1, 2)
            abs_spring = absorption and (spring or mspring)
            abs_ut = absorption and (upthrust or mupthrust)
            if mspring and msl is not None:
                q = 3 if lwr[t] >= 0.55 and catr >= 0.8 else 2
                q = min(3, q + 1) if abs_spring else q
                add(3, max(lo[t], msl), min(lo[t], msl), 1, q)
            elif spring and sl is not None:
                q = 3 if lwr[t] >= 0.55 and catr >= 0.8 else (2 if lwr[t] >= 0.35 else 1)
                q = min(3, q + 1) if abs_spring else q
                add(3, max(lo[t], sl), min(lo[t], sl), 1, q)
            if mupthrust and msh is not None:
                q = 3 if uwr[t] >= 0.55 and catr >= 0.8 else 2
                q = min(3, q + 1) if abs_ut else q
                add(4, max(h[t], msh), min(h[t], msh), -1, q)
            elif upthrust and sh is not None:
                q = 3 if uwr[t] >= 0.55 and catr >= 0.8 else (2 if uwr[t] >= 0.35 else 1)
                q = min(3, q + 1) if abs_ut else q
                add(4, max(h[t], sh), min(h[t], sh), -1, q)
            if major_bull and msh is not None:
                add(5, msh + half, msh - half, 1, 2)
            if major_bear and msl is not None:
                add(6, msl + half, msl - half, -1, 2)
            if climax:
                d = 1 if c[t] < o[t] else -1
                add(7 if d == 1 else 8, h[t], lo[t], d, 3 if rel_vol >= 1.8 * 1.5 else 2)

            for i in range(len(zones) - 1, -1, -1):
                z = zones[i]
                age = t - z["bar"]
                if age > 200:
                    zones.pop(i)
                    continue
                in_z = h[t] >= z["lower"] and lo[t] <= z["upper"]
                above = c[t] > z["upper"]
                below = c[t] < z["lower"]
                if age > 0:
                    prev_in = h[t - 1] >= z["lower"] and lo[t - 1] <= z["upper"]
                    if in_z and not prev_in:
                        z["touch"] += 1
                        z["vb"] = False
                        if z["status"] == 0:
                            z["status"] = 1
                    can = z["status"] in (1, 2) and in_z and not z["vb"]
                    if can and z["dir"] == 1 and c[t] > z["mid"] and c[t] > o[t]:
                        z["status"] = 2
                        z["vb"] = True
                        b = (c[t] - z["lower"]) / a if a > 0 else 0.0
                        z["lastb"] = b
                        z["maxb"] = max(z["maxb"], b)
                        if b >= 0.5:
                            z["good"] += 1
                        else:
                            z["weak"] += 1
                    if can and z["dir"] == -1 and c[t] < z["mid"] and c[t] < o[t]:
                        z["status"] = 2
                        z["vb"] = True
                        b = (z["upper"] - c[t]) / a if a > 0 else 0.0
                        z["lastb"] = b
                        z["maxb"] = max(z["maxb"], b)
                        if b >= 0.5:
                            z["good"] += 1
                        else:
                            z["weak"] += 1
                    if z["dir"] == 1 and in_z and c[t] < z["mid"]:
                        z["deep"] += 1
                    if z["dir"] == -1 and in_z and c[t] > z["mid"]:
                        z["deep"] += 1
                    broke = False
                    if z["dir"] == 1 and below and c[t - 1] >= z["lower"]:
                        z["status"] = 3
                        broke = True
                    if z["dir"] == -1 and above and c[t - 1] <= z["upper"]:
                        z["status"] = 3
                        broke = True
                    if z["status"] == 3 and not broke and z["dir"] == 1 and in_z and c[t] < z["mid"]:
                        z["status"] = 4
                        z["dir"] = -1
                    elif z["status"] == 3 and not broke and z["dir"] == -1 and in_z and c[t] > z["mid"]:
                        z["status"] = 4
                        z["dir"] = 1

        code = 0
        for hit, bit in ((ph is not None, 1), (pl is not None, 2), (micro_bull, 4), (micro_bear, 8),
                         (major_bull, 16), (major_bear, 32), (spring, 64), (upthrust, 128),
                         (mspring, 256), (mupthrust, 512), (climax, 1024), (test_candle, 2048),
                         (bull_rej, 4096), (bear_rej, 8192), (eqh is not None, 16384),
                         (eql is not None, 32768), (bull_choch, 65536), (bear_choch, 131072)):
            if hit:
                code += bit

        if a is None or rsi[t] is None or approach_avg[t] is None:
            continue
        if real and (mfi[t] is None or avg_vol[t] is None):
            continue

        ns = nr = -1
        nsd = nrd = 999.0
        for i, z in enumerate(zones):
            if z["status"] == 3:
                continue
            dist = abs(c[t] - z["mid"]) / a if a > 0 else 999.0
            if dist > 3.0:
                continue
            if z["dir"] == 1 and z["mid"] < c[t] and dist < nsd:
                nsd, ns = dist, i
            if z["dir"] == -1 and z["mid"] > c[t] and dist < nrd:
                nrd, nr = dist, i

        row = {}
        flags = [False] * 6
        for side, idx, dist in ((0, ns, nsd), (1, nr, nrd)):
            if idx < 0:
                continue
            z = zones[idx]
            d = z["dir"]
            age = t - z["bar"]
            s = 30.0 if z["kind"] in (3, 4) else (25.0 if z["kind"] in (5, 6, 7, 8) else 20.0)
            s += 20.0 if z["q"] == 3 else (10.0 if z["q"] == 2 else 4.0)
            s += 20.0 if z["status"] == 2 else (10.0 if z["status"] == 1 else (6.0 if z["status"] == 0 else -25.0))
            s += 10.0 if age < 30 else (5.0 if age < 80 else (-5.0 if age > 150 else 0.0))
            structural = max(0.0, min(100.0, s))
            r = 30.0
            r += min(30.0, z["good"] * 12.0)
            r += 15.0 if z["maxb"] >= 1.0 else (8.0 if z["maxb"] >= 0.5 else 0.0)
            r -= min(20.0, z["deep"] * 5.0)
            r -= 10.0 if z["weak"] > z["good"] else 0.0
            r -= (z["touch"] - 3) * 5.0 if z["touch"] > 3 else 0.0
            if z["good"] >= 2 and z["lastb"] < z["maxb"] * 0.5:
                r -= 10.0
            respect = max(0.0, min(100.0, r))
            aq = 0.0
            av = avg_vol[t]
            if d == 1:
                aq += 15.0 if rng[t] < approach_avg[t] * 0.8 else 0.0
                aq += 10.0 if real and v[t] < av * 0.9 and c[t] < c[t - 1] else 0.0
                aq += 10.0 if lwr[t] > 0.3 and lwr[t] > lwr[t - 1] else 0.0
                aq += 10.0 if rsi[t] < 35 else 0.0
                aq += 8.0 if real and mfi[t] < 30 else 0.0
                aq -= 15.0 if rng[t] > approach_avg[t] * 1.2 and c[t] < o[t] else 0.0
                aq -= 12.0 if real and v[t] > av * 1.3 and c[t] < o[t] else 0.0
                aq -= 18.0 if major_bear else 0.0
            else:
                aq += 15.0 if rng[t] < approach_avg[t] * 0.8 else 0.0
                aq += 10.0 if real and v[t] < av * 0.9 and c[t] > c[t - 1] else 0.0
                aq += 10.0 if uwr[t] > 0.3 and uwr[t] > uwr[t - 1] else 0.0
                aq += 10.0 if rsi[t] > 65 else 0.0
                aq += 8.0 if real and mfi[t] > 70 else 0.0
                aq -= 15.0 if rng[t] > approach_avg[t] * 1.2 and c[t] > o[t] else 0.0
                aq -= 12.0 if real and v[t] > av * 1.3 and c[t] > o[t] else 0.0
                aq -= 18.0 if major_bull else 0.0
            approach = max(-100.0, min(100.0, aq))
            if d == 1:
                ca = 15.0 if is_rng else (-20.0 if t_dn else 5.0)
            else:
                ca = 15.0 if is_rng else (-20.0 if t_up else 5.0)
            ca += 10.0 if er < 0.25 else 0.0
            context = max(-100.0, min(100.0, ca))
            reach = 75.0 if dist <= 0.5 else (60.0 if dist <= 1.0 else (40.0 if dist <= 2.0 else 20.0))
            if d == 1 and c[t] < c[t - 1] < c[t - 2]:
                reach += 15.0
            if d == -1 and c[t] > c[t - 1] > c[t - 2]:
                reach += 15.0
            if d == 1 and c[t] > c[t - 1] > c[t - 2]:
                reach -= 15.0
            if d == -1 and c[t] < c[t - 1] < c[t - 2]:
                reach -= 15.0
            reach = max(0.0, min(100.0, reach))
            hold = max(0.0, min(100.0, 0.30 * structural + 0.25 * respect + 0.30 * (approach + 100.0) / 2.0
                                + 0.15 * (context + 100.0) / 2.0))
            if (gap_sup if side == 0 else gap_res):
                hold = min(100.0, hold + 5.0)
            if (ob_sup if side == 0 else ob_res):
                hold = min(100.0, hold + (ob_sup_sc if side == 0 else ob_res_sc) * 0.1)
            if poc is not None and a > 0 and abs(z["mid"] - poc) / a < 1.0:
                hold = min(100.0, hold + 7.0)
            elif va_lo is not None and va_hi is not None and va_lo <= z["mid"] <= va_hi:
                hold = min(100.0, hold + 3.0)
            br = 100.0 - hold
            br += 10.0 if approach < -30 else 0.0
            br += z["touch"] * 3.0 if z["touch"] > 3 else 0.0
            br += 10.0 if z["weak"] > z["good"] else 0.0
            if z["good"] >= 2 and z["lastb"] < z["maxb"] * 0.4:
                br += 12.0
            br = max(0.0, min(100.0, br))
            if br >= 65 and approach < -20:
                state = "BREAK LIKELY"
            elif hold >= 65:
                state = "REACTION LIKELY"
            elif hold >= 50 and br >= 50:
                state = "UNDER PRESSURE"
            elif z["touch"] > 3 and z["lastb"] < z["maxb"] * 0.5:
                state = "WEAKENING"
            elif hold >= 50:
                state = "HOLD POSSIBLE"
            else:
                state = "STABLE"
            name = "support" if side == 0 else "resistance"
            for k, val in (("mid", z["mid"]), ("upper", z["upper"]), ("lower", z["lower"]),
                           ("type", float(z["kind"])), ("status", float(z["status"])),
                           ("distance_atr", dist), ("reach", reach), ("hold", hold), ("break", br),
                           ("structural", structural), ("respect", respect), ("approach", approach),
                           ("context", context), ("state", float(STATE[state]))):
                row[f"{name}_{k}"] = val
            near = dist <= 1.0
            flags[side] = near and reach >= 60
            flags[2 + side] = near and hold >= 60 and br < 45
            flags[4 + side] = near and br >= 60 and hold < 45
        for i, k in enumerate(EVENTS):
            if flags[i] and not prev_flags[i]:
                out["events"][k].append(t)
        prev_flags = flags
        row.update({"zones": float(len(zones)), "micro_trend": float(micro_trend),
                    "major_trend": float(major_trend), "clx_state": float(clx_state),
                    "clx_direction": float(clx_dir), "event_code": float(code)})
        if poc is not None:
            row.update({"poc": poc, "va_high": va_hi, "va_low": va_lo})
        out["reported"][t] = True
        out["rows"][t] = row
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Structure Zones",
    "",
    *provenance("structure_zones"),
    "",
    "Formula: the original definition with",
    "its defaults. Conventions the original leaves open: EMAs seeded with the first close, ATR",
    "= Wilder average of the true range (first bar high - low) seeded with an SMA, RSI = Wilder",
    "averages of the close changes seeded with an SMA, MFI over the last 14 changes of hlc3 (50",
    "without flow, 100 without negative flow), pivots strict on both sides, real volume =",
    "cumulative volume so far positive, a first major pivot counts as new for the volume profile.",
    "Swings, BOS, swing trends, volume profile and effort-vs-result run from the first bar; zones,",
    "gaps, order blocks and the climax sequence from the first bar with an ATR; output once RSI",
    "(with real volume also MFI and the volume average) exists. Alerts fire on entry.",
    "",
    "Series: 900 bars, prev = previous close (100 before the first bar); close = prev +",
    "0.3 sin(i/70) + 0.8 sin(i/5.3) + 0.4 cos(i/1.9); open = prev + 0.3 sin(i/2.4); high =",
    "max(open, close) + 0.2 + 0.3 |sin(i/3.1)|; low = min(open, close) - 0.2 - 0.3 |cos(i/2.6)|;",
    "volume = 1000 + 600 sin(i/4.5)^2. When i mod 29 = 13: low - 1.6, volume x 2; when i mod 31 =",
    "21: high + 1.6, volume x 2; when i mod 47 = 33 (replacing the bar): close = prev + 3.2, open =",
    "prev - 0.2, high = close + 0.2, low = open - 0.2, volume x 2.8. Block 'v' with that volume,",
    "block 'nv' with volume 0 everywhere.",
    "",
    "<v|nv>_first is the first reporting bar; _cp<bar>_* the outputs at that bar (zone keys only",
    "where that zone exists); _ev_<kind>_n the number of alerts of that kind and _ev_<kind>_<j> the",
    "bar of the j-th (up to 8).",
]


def _block(prefix, out):
    case = {"first": float(next(t for t in range(N_BARS) if out["reported"][t]))}
    for cp in CHECKPOINTS:
        for k, val in out["rows"][cp].items():
            case[f"cp{cp}_{k}"] = val
    for k in EVENTS:
        bars = out["events"][k]
        case[f"ev_{k}_n"] = float(len(bars))
        for j, b in enumerate(bars[:8]):
            case[f"ev_{k}_{j}"] = float(b)
    return (prefix, None, case)


def build():
    blocks = [
        _block("v", compute(_series(True))),
        _block("nv", compute(_series(False))),
        (None, None, {"vsz_tolerance": 1e-9}),
    ]
    return HEADER, blocks
