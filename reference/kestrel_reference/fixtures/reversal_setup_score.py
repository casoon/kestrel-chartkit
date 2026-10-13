"""golden_reversal_setup_score: Reversal Setup Score (momentum mix A).

Written from the original definition, not from the Rust code, with the
inputs at their defaults. Series are full arrays (`x[k]` read as `series[t - k]`); the `var` state
(swing trends, climax sequence, events) is followed bar by bar.
"""

import math

from ..fixture import provenance

NAME = "golden_reversal_setup_score"

N_BARS = 1000
CHECKPOINTS = [90, 233, 377, 512, 640, 777, 901, 999]
EVENTS = [
    "bull_review", "bear_review", "bull_action", "bear_action", "long_signal_invalidated",
    "short_signal_invalidated", "bull_candidate", "bear_candidate", "conflict", "bull_spring",
    "bear_upthrust", "bull_major_bos", "bear_major_bos", "bear_failed_bull_reversal",
    "bull_failed_bear_reversal", "bull_choch", "bear_choch",
]
KEYS = ["bull_setup", "bull_conf", "bull_conf_decayed", "bull_quality", "bull_status",
        "bear_setup", "bear_conf", "bear_conf_decayed", "bear_quality", "bear_status",
        "conflict_count", "clx_state", "clx_direction", "price_zone", "efficiency_ratio", "rsi",
        "bull_score", "bear_score"]
STATUS = {"—": 0, "WATCH": 1, "SETUP": 2, "ALERT": 3, "PENDING": 4, "CANDIDATE": 5,
          "CONFLICTED": 6, "FAILED": 7}


def _series(with_volume):
    """Swings of changing size and direction with occasional wide bars and wick spikes."""
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.25 * math.sin(x / 61.0) + 0.1 * math.sin(x / 23.0)
        close = prev + drift + 0.9 * math.sin(x / 4.1) + 0.5 * math.cos(x / 1.7)
        open_ = prev + 0.35 * math.sin(x / 2.2)
        high = max(open_, close) + 0.25 + 0.3 * abs(math.sin(x / 3.4))
        low = min(open_, close) - 0.25 - 0.3 * abs(math.cos(x / 2.9))
        vol = 1000.0 + 500.0 * math.sin(x / 5.5) ** 2
        if i % 37 == 20:
            low -= 2.0
            vol *= 2.5
        if i % 41 == 30:
            high += 2.0
            vol *= 2.5
        if i % 53 == 44:
            close = prev - 3.5
            open_ = prev + 0.5
            high = open_ + 0.3
            low = close - 0.3
            vol *= 3.0
        if not with_volume:
            vol = 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


def _ema(src, n):
    a = 2.0 / (n + 1.0)
    out, prev = [], None
    for v in src:
        if v is None:
            out.append(None)
            continue
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
        w = src[t - n + 1:t + 1]
        if None not in w:
            out[t] = sum(w) / n
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

    ema20 = _ema(c, 20)
    ema50 = _ema(c, 50)
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, 14)
    up = [None] + [max(c[t] - c[t - 1], 0.0) for t in range(1, n)]
    dn = [None] + [max(c[t - 1] - c[t], 0.0) for t in range(1, n)]
    au, ad = _rma(up, 14), _rma(dn, 14)
    rsi = [None] * n
    for t in range(n):
        if au[t] is not None:
            rsi[t] = 50.0 if au[t] == 0 and ad[t] == 0 else (
                100.0 if ad[t] == 0 else 100.0 - 100.0 / (1.0 + au[t] / ad[t]))
    e12, e26 = _ema(c, 12), _ema(c, 26)
    macd = [e12[t] - e26[t] for t in range(n)]
    sig = _ema(macd, 9)
    hist = [macd[t] - sig[t] for t in range(n)]
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
    basis = _sma(c, 20)
    bb_dev = [None] * n
    for t in range(19, n):
        m = basis[t]
        bb_dev[t] = math.sqrt(sum((c[j] - m) ** 2 for j in range(t - 19, t + 1)) / 20.0) * 2.0
    bb_width = [2.0 * d if d is not None else None for d in bb_dev]
    bb_width_avg = _sma(bb_width, 50)
    eff = [None] + [(abs(c[t] - c[t - 1]) / (h[t] - lo[t]) if h[t] - lo[t] > 0 else 0.0)
                    for t in range(1, n)]
    eff_ema = _ema(eff, 5)

    cum = 0.0
    micro_sh = micro_sl = major_sh = major_sl = None
    micro_trend = major_trend = 0
    clx_state = clx_dir = 0
    clx_close = clx_high = clx_low = clx_range = clx_vol = None
    clx_bar = None
    prev_confirmed = False
    side = {s: {"trigger": None, "bar": None, "conf": 0.0, "ft": False, "failed": False,
                "was_cand": False, "prev_failed": False, "prev_trap": False, "prev_status": 0}
            for s in ("bull", "bear")}
    prev_conflict = False
    out = {"reported": [False] * n, "rows": [None] * n, "events": {k: [] for k in EVENTS}}

    for t in range(n):
        cum += v[t]
        real = cum > 0
        ph3, pl3 = _pivot(h, t, 3, True), _pivot(lo, t, 3, False)
        ph7, pl7 = _pivot(h, t, 7, True), _pivot(lo, t, 7, False)
        if ph3 is not None:
            micro_sh = ph3
        if pl3 is not None:
            micro_sl = pl3
        if ph7 is not None:
            major_sh = ph7
        if pl7 is not None:
            major_sl = pl7
        c1 = c[t - 1] if t >= 1 else None
        micro_bull_bos = micro_sh is not None and c1 is not None and c[t] > micro_sh and c1 <= micro_sh
        micro_bear_bos = micro_sl is not None and c1 is not None and c[t] < micro_sl and c1 >= micro_sl
        major_bull_bos = major_sh is not None and c1 is not None and c[t] > major_sh and c1 <= major_sh
        major_bear_bos = major_sl is not None and c1 is not None and c[t] < major_sl and c1 >= major_sl
        micro_bull_choch = micro_bull_bos and micro_trend == -1
        micro_bear_choch = micro_bear_bos and micro_trend == 1
        major_bull_choch = major_bull_bos and major_trend == -1
        major_bear_choch = major_bear_bos and major_trend == 1
        if micro_bull_bos:
            micro_trend = 1
        if micro_bear_bos:
            micro_trend = -1
        if major_bull_bos:
            major_trend = 1
        if major_bear_bos:
            major_trend = -1

        need = [atr[t], rsi[t], rsi[t - 1] if t >= 1 else None, hist[t - 1] if t >= 1 else None,
                basis[t], bb_width_avg[t], avg_vol[t], eff_ema[t]]
        if real:
            need.append(mfi[t])
        if None in need or t < 10 or any(eff_ema[t - k] is None for k in (1, 2, 3)):
            continue

        a = atr[t]
        rel_vol = v[t] / avg_vol[t] if real and avg_vol[t] > 0 else 1.0
        p2e = (c[t] - ema20[t]) / a if a > 0 else 0.0
        rng = h[t] - lo[t]
        uwr = (h[t] - max(o[t], c[t])) / rng if rng > 0 else 0.0
        lwr = (min(o[t], c[t]) - lo[t]) / rng if rng > 0 else 0.0
        catr = rng / a if a > 0 else 0.0
        falling = all(eff_ema[t] < eff_ema[t - k] for k in (1, 2, 3))
        shock = catr > 2.5 and rel_vol > 2.0
        bb_up, bb_lo = basis[t] + bb_dev[t], basis[t] - bb_dev[t]
        wratio = (bb_up - bb_lo) / bb_width_avg[t] if bb_width_avg[t] > 0 else 1.0
        path = sum(abs(c[j] - c[j - 1]) for j in range(t - 9, t + 1))
        er = abs(c[t] - c[t - 10]) / path if path > 0 else 0.0
        t_up = ema20[t] > ema50[t] and er > 0.35
        t_dn = ema20[t] < ema50[t] and er > 0.35
        is_rng = er <= 0.35
        setup_mult = 1.2 if is_rng else (0.8 if t_up or t_dn else 1.0)
        conf_mult = 1.3 if t_up or t_dn else 1.0
        vol_bonus = 1.2 if wratio > 1.3 else (0.8 if wratio < 0.7 else 1.0)
        hh, ll = max(h[t - 19:t + 1]), min(lo[t - 19:t + 1])
        span = hh - ll
        near_hi = ((hh - c[t]) / span if span > 0 else 0.5) < 0.15
        near_lo = ((c[t] - ll) / span if span > 0 else 0.5) < 0.15
        zone = 0
        if major_sh is not None and major_sl is not None and major_sh - major_sl > 0:
            r = major_sh - major_sl
            prem, disc = major_sl + r * 0.75, major_sl + r * 0.25
            zone = 1 if c[t] >= prem else (-1 if c[t] <= disc else 0)
        fake_up = micro_sh is not None and h[t] > micro_sh and c[t] <= micro_sh
        fake_dn = micro_sl is not None and lo[t] < micro_sl and c[t] >= micro_sl
        mfake_up = major_sh is not None and h[t] > major_sh and c[t] <= major_sh
        mfake_dn = major_sl is not None and lo[t] < major_sl and c[t] >= major_sl
        spring = fake_dn and lwr >= 0.35 and catr >= 0.5
        upthrust = fake_up and uwr >= 0.35 and catr >= 0.5
        mspring = mfake_dn and lwr >= 0.35 and catr >= 0.5
        mupthrust = mfake_up and uwr >= 0.35 and catr >= 0.5

        climax = catr >= 1.8 and ((avg_vol[t] > 0 and v[t] >= 1.8 * avg_vol[t]) if real else True)
        if climax:
            clx_state = 1
            clx_dir = 1 if c[t] < o[t] else (-1 if c[t] > o[t] else 0)
            clx_close, clx_high, clx_low, clx_range, clx_vol, clx_bar = c[t], h[t], lo[t], rng, v[t], t
        if clx_state >= 1 and clx_bar is not None and t - clx_bar <= 20:
            if clx_state == 1:
                if clx_dir == 1 and c[t] > clx_close:
                    clx_state = 2
                if clx_dir == -1 and c[t] < clx_close:
                    clx_state = 2
            elif clx_state == 2:
                back = False
                if clx_dir == 1 and a > 0:
                    back = c[t] < clx_close + 1.5 * a and c[t] > clx_low - 0.5 * a
                if clx_dir == -1 and a > 0:
                    back = c[t] > clx_close - 1.5 * a and c[t] < clx_high + 0.5 * a
                if back and t - clx_bar >= 2:
                    clx_state = 3
            elif clx_state == 3:
                test = rng <= clx_range * 0.6 and rng > 0 and (v[t] <= clx_vol * 0.7 if real else True)
                in_zone = a > 0 and abs(c[t] - clx_close) <= 1.5 * a
                if test and in_zone:
                    clx_state = 4
            elif clx_state == 4:
                if clx_dir == 1 and (micro_bull_bos or major_bull_bos):
                    clx_state = 5
                if clx_dir == -1 and (micro_bear_bos or major_bear_bos):
                    clx_state = 5
        if clx_state >= 1 and clx_bar is not None and t - clx_bar > 20:
            clx_state = 0
        test_valid = clx_state in (4, 5)
        confirmed = clx_state == 5
        just_conf = confirmed and not prev_confirmed
        prev_confirmed = confirmed

        r0, r1 = rsi[t], rsi[t - 1]
        m0 = mfi[t] if real else None
        m1 = mfi[t - 1] if real else None
        bs = 0.0
        if r0 < 35:
            bs += 1.0
        if r0 < 45 and r0 > r1:
            bs += 0.5
        if real and m0 < 30:
            bs += 1.0
        if real and m1 is not None and m0 > m1 and m1 < 40:
            bs += 0.5
        if not real and r0 < 35:
            bs += 1.0
        if not real and r0 < 45 and r0 > r1:
            bs += 0.5
        if hist[t] > hist[t - 1] and hist[t - 1] < 0:
            bs += 0.5
        if p2e < -1.5:
            bs += 1.0
        if p2e < -2.25:
            bs += 0.5
        if lo[t] <= bb_lo:
            bs += 1.0
        if lwr >= 0.4:
            bs += 1.0
        if falling and c[t] < ema20[t]:
            bs += 1.5
        if rel_vol < 0.6:
            bs -= 0.5
        if shock:
            bs -= 2.0
        bull_setup = max(0.0, bs) * setup_mult

        ss = 0.0
        if r0 > 65:
            ss += 1.0
        if r0 > 55 and r0 < r1:
            ss += 0.5
        if real and m0 > 70:
            ss += 1.0
        if real and m1 is not None and m0 < m1 and m1 > 60:
            ss += 0.5
        if not real and r0 > 65:
            ss += 1.0
        if not real and r0 > 55 and r0 < r1:
            ss += 0.5
        if hist[t] < hist[t - 1] and hist[t - 1] > 0:
            ss += 0.5
        if p2e > 1.5:
            ss += 1.0
        if p2e > 2.25:
            ss += 0.5
        if h[t] >= bb_up:
            ss += 1.0
        if uwr >= 0.4:
            ss += 1.0
        if falling and c[t] > ema20[t]:
            ss += 1.5
        if rel_vol < 0.6:
            ss -= 0.5
        if shock:
            ss -= 2.0
        bear_setup = max(0.0, ss) * setup_mult

        def spring_sc(wick, sweep, major):
            f = (wick >= 0.55) + (catr >= 0.8) + (rel_vol >= 1.3) + (sweep >= 0.15)
            base = 3.0 if major else 1.5
            return base + 1.5 if f >= 3 else (base + 0.5 if f >= 1 else base)

        def bos_sc(sweep, major):
            f = (sweep >= 0.1) + (sweep >= 0.3) + (rel_vol >= 1.3)
            base = 2.0 if major else 0.5
            return base + 1.5 if f >= 2 else (base + 0.5 if f >= 1 else base)

        bc = 0.0
        if mspring:
            bc += spring_sc(lwr, (major_sl - lo[t]) / a if a > 0 else 0.0, True)
        elif spring:
            bc += spring_sc(lwr, (micro_sl - lo[t]) / a if a > 0 else 0.0, False)
        if major_bull_bos:
            bc += bos_sc((c[t] - major_sh) / a if a > 0 else 0.0, True)
            if major_bull_choch:
                bc += 1.5
        elif micro_bull_bos:
            bc += bos_sc((c[t] - micro_sh) / a if a > 0 else 0.0, False)
            if micro_bull_choch:
                bc += 0.75
        if fake_dn and not spring and not mspring:
            bc += 0.5
        if mfake_dn and not mspring:
            bc += 1.0
        if climax and c[t] < ema20[t]:
            bc += 1.0
        if test_valid and clx_dir == 1:
            bc += 1.5
        if confirmed and clx_dir == 1:
            bc += 2.5
        if zone == -1:
            bc += 1.0
        if zone == 1:
            bc -= 0.5
        bull_conf = max(0.0, bc) * conf_mult * vol_bonus

        sc = 0.0
        if mupthrust:
            sc += spring_sc(uwr, (h[t] - major_sh) / a if a > 0 else 0.0, True)
        elif upthrust:
            sc += spring_sc(uwr, (h[t] - micro_sh) / a if a > 0 else 0.0, False)
        if major_bear_bos:
            sc += bos_sc((major_sl - c[t]) / a if a > 0 else 0.0, True)
            if major_bear_choch:
                sc += 1.5
        elif micro_bear_bos:
            sc += bos_sc((micro_sl - c[t]) / a if a > 0 else 0.0, False)
            if micro_bear_choch:
                sc += 0.75
        if fake_up and not upthrust and not mupthrust:
            sc += 0.5
        if mfake_up and not mupthrust:
            sc += 1.0
        if climax and c[t] > ema20[t]:
            sc += 1.0
        if test_valid and clx_dir == -1:
            sc += 1.5
        if confirmed and clx_dir == -1:
            sc += 2.5
        if zone == 1:
            sc += 1.0
        if zone == -1:
            sc -= 0.5
        bear_conf = max(0.0, sc) * conf_mult * vol_bonus

        new = {"bull": spring or mspring or micro_bull_bos or major_bull_bos or (just_conf and clx_dir == 1),
               "bear": upthrust or mupthrust or micro_bear_bos or major_bear_bos or (just_conf and clx_dir == -1)}
        conf = {"bull": bull_conf, "bear": bear_conf}
        counter = {"bull": micro_bear_bos or major_bear_bos, "bear": micro_bull_bos or major_bull_bos}
        ev = {}
        for s in ("bull", "bear"):
            st = side[s]
            if new[s]:
                st["trigger"], st["bar"] = c[t], t
            age = t - st["bar"] if st["bar"] is not None else 999
            dec = 1.0 if age <= 3 else (1.0 - 0.8 * (age - 3) / (6 - 3) if age <= 6 else 0.0)
            if new[s]:
                st["conf"] = conf[s]
            cd = max(conf[s], st["conf"] * dec)
            active = 1 <= age <= 4 and st["trigger"] is not None
            if s == "bull":
                follow = active and c[t] > c[t - 1] and c[t] > st["trigger"] and not counter[s]
            else:
                follow = active and c[t] < c[t - 1] and c[t] < st["trigger"] and not counter[s]
            if new[s]:
                st["ft"] = False
            if follow:
                st["ft"] = True
            expired = age > 6 and not st["ft"] and st["trigger"] is not None
            if s == "bull":
                lost = st["trigger"] is not None and 1 <= age <= 12 and c[t] < st["trigger"] and st["ft"]
            else:
                lost = st["trigger"] is not None and 1 <= age <= 12 and c[t] > st["trigger"] and st["ft"]
            if new[s]:
                st["failed"] = False
            if expired or lost:
                st["failed"] = True
            ev[s] = {"age": age, "decay": dec, "cd": cd}

        b_ready, b_strong, b_conf = bull_setup >= 3.0, bull_setup >= 5.0, ev["bull"]["cd"] >= 2.0
        s_ready, s_strong, s_conf = bear_setup >= 3.0, bear_setup >= 5.0, ev["bear"]["cd"] >= 2.0
        setup_conflict = b_ready and s_ready
        conf_conflict = b_conf and s_conf
        b_trend_c = b_ready and t_dn and er > 0.45
        s_trend_c = s_ready and t_up and er > 0.45
        b_opp = b_ready and b_conf and s_conf
        s_opp = s_ready and s_conf and b_conf
        is_conflict = setup_conflict or conf_conflict or b_trend_c or s_trend_c or b_opp or s_opp
        n_conf = setup_conflict + conf_conflict + b_trend_c + s_trend_c

        def status(st, ready, strong, confd, opp, tc):
            if st["failed"]:
                return "FAILED"
            if ready and confd and st["ft"]:
                return "CONFLICTED" if opp else ("CONFLICTED" if tc else "CANDIDATE")
            if ready and confd:
                return "CONFLICTED" if opp else ("CONFLICTED" if tc else "PENDING")
            if strong:
                return "CONFLICTED" if tc else "SETUP"
            if confd and not ready:
                return "ALERT"
            if ready:
                return "WATCH"
            return "—"

        b_status = status(side["bull"], b_ready, b_strong, b_conf, b_opp, b_trend_c)
        s_status = status(side["bear"], s_ready, s_strong, s_conf, s_opp, s_trend_c)
        for s, stt in (("bull", b_status), ("bear", s_status)):
            if new[s]:
                side[s]["was_cand"] = False
            if stt == "CANDIDATE":
                side[s]["was_cand"] = True
        bull_score = 1.0 * bull_setup + 1.3 * bull_conf
        bear_score = 1.0 * bear_setup + 1.3 * bear_conf
        absorption = real and rel_vol >= 1.5 and catr < 0.7

        bq = 50.0
        if falling and (near_lo or c[t] < ema20[t]):
            bq += 15.0
        if lwr > 0.4:
            bq += 10.0
        if absorption:
            bq += 15.0
        if shock:
            bq -= 20.0
        if bull_setup >= 5.0 and bull_conf < 2.0:
            bq -= 15.0
        if ev["bull"]["decay"] < 0.3 and ev["bull"]["age"] < 999:
            bq -= 10.0
        if micro_bull_choch or major_bull_choch:
            bq += 10.0
        if zone == -1:
            bq += 5.0
        if zone == 1:
            bq -= 5.0
        sq = 50.0
        if falling and (near_hi or c[t] > ema20[t]):
            sq += 15.0
        if uwr > 0.4:
            sq += 10.0
        if absorption:
            sq += 15.0
        if shock:
            sq -= 20.0
        if bear_setup >= 5.0 and bear_conf < 2.0:
            sq -= 15.0
        if ev["bear"]["decay"] < 0.3 and ev["bear"]["age"] < 999:
            sq -= 10.0
        if micro_bear_choch or major_bear_choch:
            sq += 10.0
        if zone == 1:
            sq += 5.0
        if zone == -1:
            sq -= 5.0
        bq = max(0.0, min(100.0, bq))
        sq = max(0.0, min(100.0, sq))

        fired = []
        if bull_setup >= 4.0 and ev["bull"]["cd"] >= 1.5 and n_conf < 2 and (new["bull"] or ev["bull"]["age"] <= 2):
            fired.append("bull_review")
        if bear_setup >= 4.0 and ev["bear"]["cd"] >= 1.5 and n_conf < 2 and (new["bear"] or ev["bear"]["age"] <= 2):
            fired.append("bear_review")
        bt = side["bull"]["trigger"] if side["bull"]["trigger"] is not None else 0.0
        st_ = side["bear"]["trigger"] if side["bear"]["trigger"] is not None else 0.0
        if bull_setup >= 5.0 and ev["bull"]["cd"] >= 3.0 and side["bull"]["ft"] and not b_opp \
                and not b_trend_c and c[t] > bt and bq >= 50:
            fired.append("bull_action")
        if bear_setup >= 5.0 and ev["bear"]["cd"] >= 3.0 and side["bear"]["ft"] and not s_opp \
                and not s_trend_c and c[t] < st_ and sq >= 50:
            fired.append("bear_action")
        if side["bull"]["failed"] and not side["bull"]["prev_failed"] and side["bull"]["was_cand"]:
            fired.append("long_signal_invalidated")
        if side["bear"]["failed"] and not side["bear"]["prev_failed"] and side["bear"]["was_cand"]:
            fired.append("short_signal_invalidated")
        if b_status == "CANDIDATE" and side["bull"]["prev_status"] != 5:
            fired.append("bull_candidate")
        if s_status == "CANDIDATE" and side["bear"]["prev_status"] != 5:
            fired.append("bear_candidate")
        conflict_alert = is_conflict and n_conf >= 2
        if conflict_alert and not prev_conflict:
            fired.append("conflict")
        if spring or mspring:
            fired.append("bull_spring")
        if upthrust or mupthrust:
            fired.append("bear_upthrust")
        if major_bull_bos:
            fired.append("bull_major_bos")
        if major_bear_bos:
            fired.append("bear_major_bos")
        b_trap = side["bull"]["failed"] and bear_setup > 0
        s_trap = side["bear"]["failed"] and bull_setup > 0
        if b_trap and not side["bull"]["prev_trap"]:
            fired.append("bear_failed_bull_reversal")
        if s_trap and not side["bear"]["prev_trap"]:
            fired.append("bull_failed_bear_reversal")
        if micro_bull_choch or major_bull_choch:
            fired.append("bull_choch")
        if micro_bear_choch or major_bear_choch:
            fired.append("bear_choch")
        for k in fired:
            out["events"][k].append(t)
        prev_conflict = conflict_alert
        side["bull"]["prev_failed"] = side["bull"]["failed"]
        side["bear"]["prev_failed"] = side["bear"]["failed"]
        side["bull"]["prev_trap"] = b_trap
        side["bear"]["prev_trap"] = s_trap
        side["bull"]["prev_status"] = STATUS[b_status]
        side["bear"]["prev_status"] = STATUS[s_status]

        out["reported"][t] = True
        out["rows"][t] = {
            "bull_setup": bull_setup, "bull_conf": bull_conf, "bull_conf_decayed": ev["bull"]["cd"],
            "bull_quality": bq, "bull_status": float(STATUS[b_status]),
            "bear_setup": bear_setup, "bear_conf": bear_conf, "bear_conf_decayed": ev["bear"]["cd"],
            "bear_quality": sq, "bear_status": float(STATUS[s_status]),
            "conflict_count": float(n_conf), "clx_state": float(clx_state),
            "clx_direction": float(clx_dir), "price_zone": float(zone), "efficiency_ratio": er,
            "rsi": r0, "bull_score": bull_score, "bear_score": bear_score,
        }
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Reversal Setup Score",
    "",
    *provenance("reversal_setup_score"),
    "",
    "Formula: the original definition with its",
    "defaults and momentum mix 'A: RSI+MFI+MACD'. Conventions the original leaves open: EMAs",
    "(also in MACD 12/26/9) seeded with the first value, ATR = Wilder average of the true range",
    "(first bar high - low) seeded with an SMA, RSI = Wilder averages of the close changes seeded",
    "with an SMA, MFI over the last 14 changes of hlc3 (50 without flow, 100 without negative",
    "flow), population standard deviation, ta.falling = below each of the previous 3 values,",
    "pivots strict on both sides, real volume = cumulative volume so far positive. Swings, BOS and",
    "the swing trends run from the first bar; scores, climax sequence, events and output from the",
    "first bar on which every input exists. Candidate and conflict alerts fire on entry.",
    "",
    "Series: 1000 bars, prev = previous close (100 before the first bar); drift = 0.25 sin(i/61)",
    "+ 0.1 sin(i/23); close = prev + drift + 0.9 sin(i/4.1) + 0.5 cos(i/1.7); open = prev +",
    "0.35 sin(i/2.2); high = max(open, close) + 0.25 + 0.3 |sin(i/3.4)|; low = min(open, close) -",
    "0.25 - 0.3 |cos(i/2.9)|; volume = 1000 + 500 sin(i/5.5)^2. When i mod 37 = 20: low - 2,",
    "volume x 2.5; when i mod 41 = 30: high + 2, volume x 2.5; when i mod 53 = 44 (replacing the",
    "bar): close = prev - 3.5, open = prev + 0.5, high = open + 0.3, low = close - 0.3, volume x 3.",
    "Block 'v' with that volume, block 'nv' with volume 0 everywhere.",
    "",
    "<v|nv>_first is the first reporting bar; _cp<bar>_* the outputs at that bar; _ev_<kind>_n the",
    "number of alerts of that kind and _ev_<kind>_<j> the bar of the j-th (up to 8).",
]


def _block(prefix, out):
    case = {"first": float(next(t for t in range(N_BARS) if out["reported"][t]))}
    for cp in CHECKPOINTS:
        for k in KEYS:
            case[f"cp{cp}_{k}"] = out["rows"][cp][k]
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
        (None, None, {"vrs_tolerance": 1e-9}),
    ]
    return HEADER, blocks
