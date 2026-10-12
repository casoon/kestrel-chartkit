"""golden_market_motion_dna: Market Motion DNA, live leg.

Written from the published Pine Script (WavesUnchained, v1.5.2), not from the Rust code: the pivot
engine and `f_calculateLeg` are followed literally, `x[k]` read as `series[t - k]`. Only the live
leg and its class state are checked; the completed-leg drawings do not feed them.
"""

import math

from ..fixture import provenance

NAME = "golden_market_motion_dna"

PIV_L = 5
PIV_R = 5
MAX_LEG = 350
MIN_LEG_ATR = 0.50
ATR_LEN = 14
MATURE = 8
RET_CEIL = 0.80
MOM_LO, MOM_HI = 0.60, 1.40
TR_LO, TR_HI = 0.70, 1.30
MA_LEN = 34
SUB_W = 2
W = [20.0, 15.0, 12.5, 10.0, 7.5, 10.0, 5.0, 7.5, 7.5, 5.0]
IMP_T = 62.0
COR_T = 62.0
MIN_SEP = 18.0
BUFFER = 5.0
MINTICK = 1e-12

N_BARS = 700
CHECKPOINTS = [60, 177, 290, 404, 515, 699]
FEATURES = ["efficiency", "overlap", "retracement", "commitment", "wick_clean",
            "momentum_expansion", "tr_expansion", "ma_support", "cleanliness", "acceptance"]
EVENTS = ["bull_impulse", "bear_impulse", "correction_started", "character_lost"]


def _series():
    """Impulsive pushes and choppy pullbacks of varying length."""
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        phase = (i // 35) % 4
        drift = [1.0, -0.12, -1.0, 0.10][phase]
        chop = [0.3, 0.9, 0.3, 1.0][phase]
        close = prev + drift + chop * math.sin(x * 1.3) + 0.2 * math.cos(x / 3.7)
        open_ = prev + 0.3 * chop * math.sin(x / 1.9)
        high = max(open_, close) + 0.15 + 0.2 * abs(math.sin(x / 2.3))
        low = min(open_, close) - 0.15 - 0.2 * abs(math.cos(x / 3.1))
        bars.append((open_, high, low, close))
        prev = close
    return bars


def _clamp(v, lo, hi):
    return max(lo, min(hi, v))


def _safe_div(n, d, fb):
    return n / d if d != 0.0 else fb


def _map01(v, lo, hi):
    return _clamp((v - lo) / (hi - lo), 0.0, 1.0) if hi > lo else 0.5


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


def compute(bars, max_leg=MAX_LEG):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    tr_s = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                             for t in range(1, n)]
    atr = _rma(tr_s, ATR_LEN)
    ma = _ema(c, MA_LEN)
    sub_high = [_pivot(h, t, SUB_W, SUB_W, True) for t in range(n)]
    sub_low = [_pivot(lo, t, SUB_W, SUB_W, False) for t in range(n)]

    def calc_leg(bi, start_bar, start_price, end_bar, end_price, direction):
        raw = max(end_bar - start_bar, 1)
        ev = min(raw, max_leg)
        eff_start = end_bar - ev
        eff_price = start_price if eff_start == start_bar else c[eff_start]
        net = max(abs(end_price - eff_price), MINTICK)
        path = ov_sum = com_sum = wick_sum = acc_sum = ma_sum = 0.0
        ov_cnt = feat = changes = prev_sign = 0
        fav = eff_price
        max_adv = 0.0
        fb = sb = ft = st = 0.0
        fc = sc = 0
        last_sub_type = 0
        last_sub_price = None
        swing_count = 0
        swing_sum = 0.0
        sizes = []
        for step in range(1, ev + 1):
            ab = eff_start + step
            off = bi - ab
            poff = off + 1
            cc, pc = c[bi - off], c[bi - poff]
            co, ch, cl = o[bi - off], h[bi - off], lo[bi - off]
            ph, pl = h[bi - poff], lo[bi - poff]
            cma, pma = ma[bi - off], ma[bi - poff]
            cr = max(ch - cl, MINTICK)
            pr = max(ph - pl, MINTICK)
            delta = cc - pc
            ds = 1 if delta > 0 else (-1 if delta < 0 else 0)
            dd = direction * delta
            dbody = max(direction * (cc - co), 0.0)
            ctr = max(ch - cl, max(abs(ch - pc), abs(cl - pc)))
            path += ctr
            amt = max(0.0, min(ch, ph) - max(cl, pl))
            ov_sum += _safe_div(amt, min(cr, pr), 0.0)
            ov_cnt += 1
            loc = (cc - cl) / cr if direction == 1 else (ch - cc) / cr
            com_sum += _clamp(loc, 0.0, 1.0)
            wick_sum += 1.0 - _clamp((cr - abs(cc - co)) / cr, 0.0, 1.0)
            acc_sum += 1.0 if dd > 0 else (0.0 if dd < 0 else 0.5)
            p_sup = cc >= cma if direction == 1 else cc <= cma
            s_sup = cma >= pma if direction == 1 else cma <= pma
            ma_sum += (0.6 if p_sup else 0.0) + (0.4 if s_sup else 0.0)
            if ds != 0:
                if prev_sign != 0 and ds != prev_sign:
                    changes += 1
                prev_sign = ds
            if direction == 1:
                fav = max(fav, ch)
                max_adv = max(max_adv, fav - cl)
            else:
                fav = min(fav, cl)
                max_adv = max(max_adv, ch - fav)
            if step <= int(math.ceil(ev * 0.5)):
                fb += dbody
                ft += ctr
                fc += 1
            else:
                sb += dbody
                st += ctr
                sc += 1
            confirm = off - SUB_W
            shv = sub_high[bi - confirm] if confirm >= 0 else None
            slv = sub_low[bi - confirm] if confirm >= 0 else None
            ctype, cprice = 0, None
            if shv is not None and slv is not None:
                ctype = 1 if last_sub_type <= 0 else -1
                cprice = shv if ctype == 1 else slv
            elif shv is not None:
                ctype, cprice = 1, shv
            elif slv is not None:
                ctype, cprice = -1, slv
            if ctype != 0:
                if last_sub_type == 0:
                    last_sub_type, last_sub_price = ctype, cprice
                    swing_count += 1
                elif ctype == last_sub_type:
                    if (ctype == 1 and cprice > last_sub_price) or (ctype == -1 and cprice < last_sub_price):
                        last_sub_price = cprice
                else:
                    size = abs(cprice - last_sub_price)
                    swing_sum += size
                    sizes.append(size)
                    swing_count += 1
                    last_sub_type, last_sub_price = ctype, cprice
            feat += 1
        eff = _clamp(_safe_div(net, max(path, net), 0.0), 0.0, 1.0)
        ov = _clamp(ov_sum / ov_cnt, 0.0, 1.0) if ov_cnt > 0 else 0.5
        ret = _clamp(_safe_div(max_adv, net, 0.0) / RET_CEIL, 0.0, 1.0)
        com = _clamp(com_sum / feat, 0.0, 1.0) if feat > 0 else 0.5
        wick = _clamp(wick_sum / feat, 0.0, 1.0) if feat > 0 else 0.5
        acc = _clamp(acc_sum / feat, 0.0, 1.0) if feat > 0 else 0.5
        mas = _clamp(ma_sum / feat, 0.0, 1.0) if feat > 0 else 0.5
        clean = 1.0 - _clamp(changes / max(ev - 1.0, 1.0), 0.0, 1.0) if ev > 1 else 0.5
        a_end = atr[end_bar] if atr[end_bar] is not None else 0.0
        afb = fb / fc if fc > 0 else 0.0
        asb = sb / sc if sc > 0 else afb
        base = max(afb, a_end * 0.25)
        mom = _map01(asb / base if base > MINTICK else 1.0, MOM_LO, MOM_HI)
        aft = ft / fc if fc > 0 else 0.0
        ast = st / sc if sc > 0 else aft
        trx = _map01(ast / aft if aft > MINTICK else 1.0, TR_LO, TR_HI)
        f = [eff, ov, ret, com, wick, mom, trx, mas, clean, acc]
        wsum = max(sum(W), 0.0001)
        raw_imp = (eff * W[0] + (1 - ov) * W[1] + (1 - ret) * W[2] + com * W[3] + wick * W[4]
                   + mom * W[5] + trx * W[6] + mas * W[7] + clean * W[8] + acc * W[9]) / wsum * 100.0
        raw_cor = ((1 - eff) * W[0] + ov * W[1] + ret * W[2] + (1 - com) * W[3] + (1 - wick) * W[4]
                   + (1 - mom) * W[5] + (1 - trx) * W[6] + (1 - mas) * W[7] + (1 - clean) * W[8]
                   + (1 - acc) * W[9]) / wsum * 100.0
        mat = _clamp(ev / float(MATURE), 0.0, 1.0)
        imp = 50.0 + (raw_imp - 50.0) * mat
        cor = 50.0 + (raw_cor - 50.0) * mat
        k = len(sizes)
        avg_sw = _safe_div(swing_sum / k, a_end, 0.0) if k > 0 else 0.0
        prog = _clamp(_safe_div(net, swing_sum, 1.0), 0.0, 1.0)
        half = int(math.ceil(k * 0.5))
        s1 = sum(sizes[:half])
        s2 = sum(sizes[half:])
        a1 = s1 / half if half > 0 else 0.0
        a2 = s2 / (k - half) if k - half > 0 else a1
        growth = a2 / a1 if a1 > MINTICK else 1.0
        return {"impulse": imp, "correction": cor, "confidence": abs(imp - cor) * mat,
                "maturity": mat, "bars": float(ev), "features": f,
                "internal_swings": float(swing_count), "avg_internal_swing_atr": avg_sw,
                "progression_ratio": prog, "sub_leg_growth": growth}

    def classify(leg):
        sep = abs(leg["impulse"] - leg["correction"])
        if leg["maturity"] >= 0.999 and sep >= MIN_SEP:
            if leg["impulse"] >= IMP_T and leg["impulse"] > leg["correction"]:
                return 1
            if leg["correction"] >= COR_T and leg["correction"] > leg["impulse"]:
                return -1
        return 0

    piv_bars, piv_prices, piv_types = [], [], []
    live_class = 0
    out = {"live": [None] * n, "class": [None] * n, "direction": [None] * n,
           "start": [None] * n, "events": {k: [] for k in EVENTS}}
    for t in range(n):
        ch_ = _pivot(h, t, PIV_L, PIV_R, True)
        cl_ = _pivot(lo, t, PIV_L, PIV_R, False)
        cand_bar = t - PIV_R
        ctype, cprice = 0, None
        if ch_ is not None and cl_ is not None:
            if not piv_types:
                ctype = 1 if abs(ch_ - c[t - PIV_R]) >= abs(c[t - PIV_R] - cl_) else -1
            else:
                ctype = -piv_types[-1]
            cprice = ch_ if ctype == 1 else cl_
        elif ch_ is not None:
            ctype, cprice = 1, ch_
        elif cl_ is not None:
            ctype, cprice = -1, cl_
        if ctype != 0:
            if not piv_bars:
                piv_bars.append(cand_bar)
                piv_prices.append(cprice)
                piv_types.append(ctype)
            elif ctype == piv_types[-1]:
                if (ctype == 1 and cprice > piv_prices[-1]) or (ctype == -1 and cprice < piv_prices[-1]):
                    piv_bars[-1], piv_prices[-1] = cand_bar, cprice
            else:
                a_piv = atr[t - PIV_R]
                large = MIN_LEG_ATR <= 0.0 or (a_piv is not None
                                               and abs(cprice - piv_prices[-1]) >= a_piv * MIN_LEG_ATR)
                if large and cand_bar > piv_bars[-1]:
                    piv_bars.append(cand_bar)
                    piv_prices.append(cprice)
                    piv_types.append(ctype)
        if not piv_bars:
            continue
        sb, sp, lt = piv_bars[-1], piv_prices[-1], piv_types[-1]
        direction = 1 if lt == -1 else -1
        end_price = h[t] if direction == 1 else lo[t]
        leg = calc_leg(t, sb, sp, t, end_price, direction)
        raw_class = classify(leg)
        prev_class = live_class
        if live_class == 1:
            keep = leg["impulse"] >= IMP_T - BUFFER and leg["impulse"] >= leg["correction"]
            live_class = 1 if keep else raw_class
        elif live_class == -1:
            keep = leg["correction"] >= COR_T - BUFFER and leg["correction"] >= leg["impulse"]
            live_class = -1 if keep else raw_class
        else:
            live_class = raw_class
        if live_class == 1 and prev_class != 1:
            out["events"]["bull_impulse" if direction == 1 else "bear_impulse"].append(t)
        if live_class == -1 and prev_class != -1:
            out["events"]["correction_started"].append(t)
        if live_class == 0 and prev_class != 0:
            out["events"]["character_lost"].append(t)
        out["live"][t] = leg
        out["class"][t] = float(live_class)
        out["direction"][t] = float(direction)
        out["start"][t] = sp
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Market Motion DNA (live leg)",
    "",
    *provenance("market_motion_dna"),
    "",
    "Formula: the published Pine Script 'Market Motion DNA' (WavesUnchained, v1.5.2) with its",
    "defaults (pivots 5/5, 350 bars per leg, minimum leg 0.5 ATR, ATR 14, maturity 8 bars,",
    "retracement ceiling 0.8, momentum 0.6..1.4, true range 0.7..1.3, EMA 34, sub-pivots 2,",
    "weights 20/15/12.5/10/7.5/10/5/7.5/7.5/5, thresholds 62/62, separation 18, buffer 5).",
    "Conventions where Pine leaves them open: ATR = Wilder average of the true range (first bar",
    "high - low) seeded with an SMA, read as 0 where missing (nz) and blocking a new pivot when",
    "missing at the pivot bar; EMA seeded with the first close; mintick = 1e-12; pivots strict on",
    "both sides.",
    "",
    "Series: 700 bars in phases of 35 bars cycling through (drift, chop) = (1.0, 0.3),",
    "(-0.12, 0.9), (-1.0, 0.3), (0.10, 1.0); close = prev + drift + chop sin(1.3 i) +",
    "0.2 cos(i/3.7) (prev = previous close, 100 before the first bar); open = prev + 0.3 chop",
    "sin(i/1.9); high = max(open, close) + 0.15 + 0.2 |sin(i/2.3)|; low = min(open, close) -",
    "0.15 - 0.2 |cos(i/3.1)|.",
    "",
    "first_reported is the first bar with a live leg; cp<bar>_* the live outputs at that bar;",
    "ev_<kind>_n the number of alerts of that kind and ev_<kind>_<j> the bar of the j-th (up to 10).",
    "Block 'short': the same series with at most 20 bars evaluated per leg (cp<bar>_* as above).",
]


def build():
    out = compute(_series())
    first = next(t for t in range(N_BARS) if out["live"][t] is not None)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        leg = out["live"][cp]
        case = {"impulse": leg["impulse"], "correction": leg["correction"],
                "confidence": leg["confidence"], "maturity": leg["maturity"], "bars": leg["bars"],
                "direction": out["direction"][cp], "class": out["class"][cp],
                "leg_start": out["start"][cp]}
        for name, v in zip(FEATURES, leg["features"]):
            case[name] = v
        for k in ("internal_swings", "avg_internal_swing_atr", "progression_ratio",
                  "sub_leg_growth"):
            case[k] = leg[k]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    ev = {}
    for name in EVENTS:
        bars = out["events"][name]
        ev[f"{name}_n"] = float(len(bars))
        for j, b in enumerate(bars[:10]):
            ev[f"{name}_{j}"] = float(b)
    blocks.append(("ev", "events", ev))
    short = compute(_series(), max_leg=20)
    case = {}
    for cp in CHECKPOINTS:
        leg = short["live"][cp]
        case[f"cp{cp}_impulse"] = leg["impulse"]
        case[f"cp{cp}_bars"] = leg["bars"]
        case[f"cp{cp}_class"] = short["class"][cp]
        for name, v in zip(FEATURES, leg["features"]):
            case[f"cp{cp}_{name}"] = v
    blocks.append(("short", "at most 20 bars per leg", case))
    blocks.append((None, None, {"mmd_tolerance": 1e-9}))
    return HEADER, blocks
