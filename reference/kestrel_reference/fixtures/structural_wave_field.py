"""golden_structural_wave_field: Structural Wave Field, computation without the visual field.

Written from the published Pine Script (WavesUnchained, v1.0.2), not from the Rust code, with the
Balanced preset and Relative Volume participation. Series are full arrays (`x[k]` read as
`series[t - k]`, None where not available); the `var` state is followed bar by bar from the first
bar on which the slow horizon's persistence exists.
"""

import math

from ..fixture import provenance

NAME = "golden_structural_wave_field"

STRUCT_BASE = 24
LEN_FAST, LEN_MED, LEN_SLOW = 24, 48, 96
ENTRY, HOLD, FLIP = 0.34, 0.14, 0.42
DISP_TARGET = 3.0
WAVE_LEN, WAVE_SMOOTH, VEL_LAG, ACC_LAG, QUALITY_LEN = 10, 5, 2, 2, 12
W_MOM, W_VEL, W_ACC = 0.55, 0.28, 0.17
WAVE_START, WAVE_END, MIN_WAVE_BARS = 0.18, 0.08, 2
RECOVERY_LEN = 8
CONTEXT_MIN = 0.44
RS_MIN_BARS, RS_MIN_PEAK, RS_MIN_RET, RS_MIN_QUALITY = 3, 0.24, 0.10, 0.38
RS_COOLDOWN = 15
RS_CONFIRM = 0.16
RS_MIN_STRUCT, RS_MIN_CONF = 0.22, 0.35
MEMORY = 30
LEG_ARCHIVE = 30
MAT_W = (0.20, 0.25, 0.15, 0.20, 0.10, 0.10)
RS_W = (0.30, 0.30, 0.20, 0.20)
EXH_MATURITY, FAILED_PEAK, FAILED_DISP = 0.58, 0.62, 0.70
VOLUME_LEN = 20
PART_WEIGHT = 0.10
PHASE_CONFIRM = 2
MINTICK = 1e-12

N_BARS = 1400
CHECKPOINTS = [200, 333, 480, 615, 777, 902, 1050, 1222, 1399]
EVENTS = [
    "bull_flow_initiated", "bear_flow_initiated", "flow_transition", "bull_flow_flip",
    "bear_flow_flip", "bull_expansion", "bear_expansion", "correction_start", "compression",
    "bull_early_resync", "bear_early_resync", "bull_resync", "bear_resync", "late_expansion",
    "exhaustion", "failed_expansion", "bull_dominant_expansion", "bear_dominant_expansion",
    "weakening_waves",
]
KEYS = ["drive", "aligned", "direction", "strength", "confidence", "internal_wave", "wave_quality",
        "counter_pressure", "integrity", "retracement", "maturity", "compression",
        "resync_quality", "resync_stage", "exhaustion", "deterioration", "wave_decay",
        "impulse_context", "correction_context", "wave_context", "phase", "phase_confidence"]


# (drift amplitude, drift period, wiggle period) per block; block 'v' with volume, 'nv' without.
VARIANTS = {"v": (0.8, 110.0, 11.0), "nv": (0.45, 60.0, 6.3)}


def _series(prefix):
    """Trending legs with pullbacks of changing depth and a one-bar shock every 300 bars."""
    amp, period, wiggle = VARIANTS[prefix]
    with_volume = prefix == "v"
    bars = []
    prev = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = amp * math.sin(x / period) + 0.15 * math.sin(x / 37.0)
        close = prev + drift + 0.7 * math.sin(x / wiggle) + 0.35 * math.cos(x / 2.1)
        if i % 300 == 250:
            close = prev - 25.0 if (i // 300) % 2 == 0 else prev + 25.0
        open_ = prev + 0.25 * math.sin(x / 1.8)
        high = max(open_, close) + 0.2 + 0.25 * abs(math.sin(x / 3.7))
        low = min(open_, close) - 0.2 - 0.25 * abs(math.cos(x / 4.3))
        vol = (1000.0 + 700.0 * math.sin(x / 7.0) ** 2 + 300.0 * math.cos(x / 19.0)) if with_volume else 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


def _clamp(x, lo, hi):
    return max(lo, min(hi, x))


def _safe_div(num, den, fallback=0.0):
    return num / den if abs(den) > 1e-10 else fallback


def _squash(x):
    return x / (1.0 + abs(x))


def _sign(x):
    return 1.0 if x > 0 else (-1.0 if x < 0 else 0.0)


def _decay(cur, ref, floor):
    return _clamp(1.0 - _safe_div(cur, max(ref, floor), 1.0), 0.0, 1.0)


def _median(a):
    s = sorted(a)
    n = len(s)
    return s[n // 2] if n % 2 == 1 else (s[n // 2 - 1] + s[n // 2]) / 2.0


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
    for t in range(len(src)):
        if prev is None:
            if t >= n - 1:
                prev = sum(src[t - n + 1:t + 1]) / n
                out[t] = prev
        else:
            prev = (src[t] + (n - 1) * prev) / n
            out[t] = prev
    return out


def _sum(src, n):
    out = [None] * len(src)
    for t in range(n - 1, len(src)):
        w = src[t - n + 1:t + 1]
        if None not in w:
            out[t] = sum(w)
    return out


def _lag(src, t, k):
    return src[t - k] if t - k >= 0 else None


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = [None if x is None else max(x, MINTICK) for x in _rma(tr, 14)]
    change = [None] + [c[t] - c[t - 1] for t in range(1, n)]
    abs_change = [None if x is None else abs(x) for x in change]

    def horizon(length):
        path = _sum(abs_change, length)
        ema_ref = _ema(c, length)
        dvals = [c[t] - c[t - length] if t >= length else None for t in range(n)]
        terms = [max(change[t] * _sign(dvals[t]), 0.0) if dvals[t] is not None and change[t] is not None
                 else None for t in range(n)]
        dir_path = _sum(terms, length)
        out = [None] * n
        for t in range(n):
            d = dvals[t]
            if None in (d, path[t], dir_path[t], atr[t]):
                continue
            eff = _safe_div(abs(d), path[t], 0.0)
            disp = _squash(_safe_div(d, atr[t] * DISP_TARGET, 0.0))
            pers = _safe_div(dir_path[t], path[t], 0.0) * _sign(d)
            loc = _squash(_safe_div(c[t] - ema_ref[t], atr[t], 0.0))
            out[t] = 0.35 * disp + 0.30 * _sign(d) * eff + 0.20 * pers + 0.15 * loc
        return out

    h_fast, h_med, h_slow = horizon(LEN_FAST), horizon(LEN_MED), horizon(LEN_SLOW)

    ema_wave = _ema(c, WAVE_LEN)
    body_ema = _ema([c[t] - o[t] for t in range(n)], max(2, round(WAVE_LEN * 0.5)))
    raw_mom = [None] * n
    for t in range(n):
        if atr[t] is None or t < WAVE_LEN or t < 1:
            continue
        pm = _squash(_safe_div(c[t] - c[t - WAVE_LEN], atr[t] * 1.75, 0.0))
        es = _squash(_safe_div((ema_wave[t] - ema_wave[t - 1]) * WAVE_LEN, atr[t] * 1.25, 0.0))
        ci = _squash(_safe_div(body_ema[t], atr[t] * 0.65, 0.0))
        raw_mom[t] = 0.52 * pm + 0.30 * es + 0.18 * ci
    core = _ema(raw_mom, WAVE_SMOOTH)
    vel = [None] * n
    for t in range(n):
        m0, m2 = core[t], _lag(core, t, VEL_LAG)
        if m0 is not None and m2 is not None:
            vel[t] = _squash((m0 - m2) * 3.0)
    acc = [None] * n
    for t in range(n):
        v0, v2 = vel[t], _lag(vel, t, ACC_LAG)
        if v0 is not None and v2 is not None:
            acc[t] = _squash((v0 - v2) * 3.0)
    wsum = max(W_MOM + W_VEL + W_ACC, 0.001)
    internal = [None if None in (core[t], vel[t], acc[t]) else
                _clamp((W_MOM * core[t] + W_VEL * vel[t] + W_ACC * acc[t]) / wsum, -1.0, 1.0)
                for t in range(n)]
    energy = [None if x is None else abs(x) for x in internal]
    q_path = _sum(abs_change, QUALITY_LEN)
    q_eff = [None if q_path[t] is None or t < QUALITY_LEN else
             _clamp(_safe_div(abs(c[t] - c[t - QUALITY_LEN]), q_path[t], 0.0), 0.0, 1.0)
             for t in range(n)]
    noise = [None] * n
    for t in range(QUALITY_LEN, n):
        w = change[t - QUALITY_LEN + 1:t + 1]
        if None not in w:
            m = sum(w) / QUALITY_LEN
            noise[t] = math.sqrt(sum((x - m) ** 2 for x in w) / QUALITY_LEN)
    eff_ema = _ema(q_eff, 5)
    vol_base = [None] * n
    for t in range(VOLUME_LEN - 1, n):
        vol_base[t] = sum(v[t - VOLUME_LEN + 1:t + 1]) / VOLUME_LEN
    rel_vol = [(_safe_div(v[t], vol_base[t], 1.0) if vol_base[t] is not None else 1.0) for t in range(n)]
    participation = [_clamp((r - 0.50) / 1.50, 0.0, 1.0) for r in rel_vol]
    part_ema = _ema(participation, 5)
    rec_ema = _ema(c, RECOVERY_LEN)

    # First bar on which every input exists.
    t0 = next(t for t in range(n) if None not in (h_fast[t], h_med[t], h_slow[t], atr[t], internal[t],
                                                    q_eff[t], noise[t], eff_ema[t], _lag(eff_ema, t, 1)))

    cum = 0.0
    signed = [None] * n         # flow-dependent series, 0 before t0 where defined
    counter_hist = [None] * n
    ppe_vals = [None] * n
    for t in range(t0):
        cum += v[t]
        if change[t] is not None:
            signed[t] = 0.0
        if q_eff[t] is not None and atr[t] is not None:
            counter_hist[t] = 0.0
        if change[t] is not None and atr[t] is not None:
            ppe_vals[t] = 0.0

    struct_dir = 0
    leg_dur, leg_disp, leg_cyc = [], [], []
    leg_start_bar = leg_start_price = leg_extreme = None
    leg_atr_sum, leg_atr_bars, leg_aligned, leg_corr = 0.0, 0, 0, 0
    waves = []  # dicts: pol, dur, disp, peak, integ, qual, eff, part, ret
    seg = None
    serial = 0
    rs = {"seen": False, "stage": 0, "hold": 99, "bars": 0, "start": None, "pp": 0.0, "pr": 0.0,
          "last": None, "pend": None, "cons": None, "pend_end": None}
    last_imp_serial = last_corr_serial = None
    phase = pending_phase = pending_bars = 0
    prev_strength = None
    prev_phase, prev_stage, prev_weak = 0, 0, False
    ppe_ema_prev = None
    ppe_hist = []
    for t in range(t0):
        if ppe_vals[t] is not None:
            ppe_hist.append(ppe_vals[t])
    ppe_ema_val = _ema(ppe_hist, 5)[-1] if ppe_hist else None
    out = {"rows": [None] * n, "events": {k: [] for k in EVENTS}, "first": t0}

    def nth(pol, k):
        found = 0
        for w in reversed(waves):
            if w["pol"] == pol:
                if found == k:
                    return w
                found += 1
        return None

    for t in range(t0, n):
        cum += v[t]
        real = cum > 0
        a = atr[t]
        raw_drive = 0.20 * h_fast[t] + 0.35 * h_med[t] + 0.45 * h_slow[t]

        def s(x):
            return 1 if x > 0.05 else (-1 if x < -0.05 else 0)

        sf, sm, ss = s(h_fast[t]), s(h_med[t]), s(h_slow[t])
        agree = 1.0 if (sf == sm and sm == ss) else (0.68 if (sf == sm or sf == ss or sm == ss) else 0.34)
        spread = (abs(h_fast[t] - h_med[t]) + abs(h_med[t] - h_slow[t]) + abs(h_fast[t] - h_slow[t])) / 3.0
        confidence = _clamp(agree * (1.0 - 0.65 * spread), 0.0, 1.0)
        drive = raw_drive * (0.72 + 0.28 * agree)
        strength = _clamp(abs(drive), 0.0, 1.0)
        prev_dir = struct_dir
        if struct_dir == 0:
            if drive > ENTRY:
                struct_dir = 1
            elif drive < -ENTRY:
                struct_dir = -1
        elif struct_dir == 1:
            if drive < -FLIP:
                struct_dir = -1
            elif drive < HOLD:
                struct_dir = 0
        else:
            if drive > FLIP:
                struct_dir = 1
            elif drive > -HOLD:
                struct_dir = 0
        d = struct_dir
        flip = prev_dir != 0 and d != 0 and d != prev_dir
        exit_ = prev_dir != 0 and d == 0
        initiated = prev_dir == 0 and d != 0
        reset = flip or exit_

        if initiated or (d != 0 and leg_start_bar is None):
            leg_start_bar, leg_start_price, leg_extreme = t, c[t], c[t]
            leg_atr_sum, leg_atr_bars, leg_aligned, leg_corr = 0.0, 0, 0, 0
        if reset and leg_start_bar is not None and leg_start_price is not None and leg_extreme is not None:
            old_dur = max(1.0, t - leg_start_bar)
            old_atr = _safe_div(leg_atr_sum, max(float(leg_atr_bars), 1.0), a)
            leg_dur.append(old_dur)
            leg_disp.append(abs(leg_extreme - leg_start_price) / max(old_atr, MINTICK))
            leg_cyc.append(max(1.0, leg_aligned + 0.5 * leg_corr))
            for arr in (leg_dur, leg_disp, leg_cyc):
                if len(arr) > LEG_ARCHIVE:
                    arr.pop(0)
            leg_start_bar = t if flip else None
            leg_start_price = c[t] if flip else None
            leg_extreme = c[t] if flip else None
            leg_atr_sum, leg_atr_bars, leg_aligned, leg_corr = 0.0, 0, 0, 0
        if d == 1:
            leg_extreme = h[t] if leg_extreme is None else max(leg_extreme, h[t])
        elif d == -1:
            leg_extreme = lo[t] if leg_extreme is None else min(leg_extreme, lo[t])
        if d != 0 and leg_start_bar is not None:
            leg_atr_sum += a
            leg_atr_bars += 1
        leg_range = abs(leg_extreme - leg_start_price) if leg_extreme is not None and leg_start_price is not None else None
        if d == 1:
            ret = _safe_div(leg_extreme - c[t], max(leg_range, a * 0.25), 0.0)
        elif d == -1:
            ret = _safe_div(c[t] - leg_extreme, max(leg_range, a * 0.25), 0.0)
        else:
            ret = 0.0
        ret = _clamp(ret, 0.0, 1.5)

        aligned = 0.0 if d == 0 else internal[t] * d
        aligned_acc = acc[t] * d
        signed[t] = change[t] * d
        window = signed[t - QUALITY_LEN + 1:t + 1]
        aligned_path = sum(max(x, 0.0) for x in window)
        counter_path = sum(max(-x, 0.0) for x in window)
        path_total = max(aligned_path + counter_path, MINTICK)
        q_pers = _clamp(max(aligned_path, counter_path) / path_total, 0.0, 1.0)
        q_smooth = _clamp(1.0 - _safe_div(noise[t], a * 0.90, 1.0), 0.0, 1.0)
        quality = _clamp(0.45 * q_eff[t] + 0.30 * q_pers + 0.25 * q_smooth, 0.0, 1.0)
        counter_energy = max(-aligned, 0.0)
        counter_pers = _clamp(counter_path / path_total, 0.0, 1.0)
        counter = _clamp(counter_energy * (0.42 + 0.22 * quality + 0.18 * _clamp(ret, 0.0, 1.0)
                                           + 0.18 * counter_pers), 0.0, 1.0)
        stress = _clamp(max(-drive * d, 0.0) * 2.0, 0.0, 1.0)
        integrity = _clamp(1.0 - (0.52 * counter + 0.30 * _clamp(ret, 0.0, 1.0) + 0.18 * stress), 0.0, 1.0)
        counter_hist[t] = counter
        part_active = real
        part = participation[t]
        part_loss = _clamp(part_ema[t - 1] - part_ema[t], 0.0, 1.0) if part_active else 0.0

        prior = nth(1, 0)
        prior_peak = prior["peak"] if prior else None
        prior_disp = prior["disp"] if prior else None
        desired = 1 if aligned >= WAVE_START else (-1 if aligned <= -WAVE_START else 0)
        if reset:
            seg = None
            waves.clear()
        en = energy[t]

        def start(pol):
            return {"pol": pol, "bar": t, "price": c[t], "peak": en, "integ": en, "q": quality,
                    "e": q_eff[t], "p": part, "ret": ret, "move": 0.0, "atr": a}

        ended = None
        if seg is None:
            if desired != 0:
                seg = start(desired)
        else:
            opposite = desired == -seg["pol"]
            died = en <= WAVE_END
            if opposite or died:
                dur = max(1, t - seg["bar"])
                if dur >= MIN_WAVE_BARS:
                    ended = {"pol": seg["pol"], "dur": float(dur),
                             "disp": seg["move"] / max(seg["atr"] if seg["atr"] is not None else a, MINTICK),
                             "peak": seg["peak"], "integ": seg["integ"],
                             "qual": _safe_div(seg["q"], dur, quality), "eff": _safe_div(seg["e"], dur, q_eff[t]),
                             "part": _safe_div(seg["p"], dur, part), "ret": seg["ret"]}
                    waves.append(ended)
                    if len(waves) > MEMORY:
                        waves.pop(0)
                seg = start(desired) if opposite else None
            else:
                sd = d * seg["pol"]
                seg["peak"] = max(seg["peak"], en)
                seg["integ"] += en
                seg["q"] += quality
                seg["e"] += q_eff[t]
                seg["p"] += part
                seg["ret"] = max(seg["ret"], ret)
                seg["move"] = max(seg["move"], max((c[t] - seg["price"]) * sd, 0.0))
                seg["atr"] = a
        if ended is not None:
            if ended["pol"] == 1:
                leg_aligned += 1
            elif ended["pol"] == -1:
                leg_corr += 1
        if reset:
            serial = 0
        elif ended is not None:
            serial += 1

        last, prevw = nth(1, 0), nth(1, 1)
        active = seg is not None and seg["pol"] == 1
        live_bars = max(1.0, t - seg["bar"] + 1) if active else (last["dur"] if last else 1.0)
        live_disp = seg["move"] / a if active else (last["disp"] if last else 0.0)
        live_integ = seg["integ"] if active else (last["integ"] if last else 0.0)
        live_q = _safe_div(seg["q"], live_bars, quality) if active else (last["qual"] if last else quality)
        live_e = _safe_div(seg["e"], live_bars, q_eff[t]) if active else (last["eff"] if last else q_eff[t])
        live_p = _safe_div(seg["p"], live_bars, part) if active else (last["part"] if last else part)
        cur_peak = seg["peak"] if active else (last["peak"] if last else max(aligned, 0.0))

        def ref(key, live, floor):
            if active:
                return last[key] if last else max(live, floor)
            if prevw:
                return prevw[key]
            return last[key] if last else max(live, floor)

        r_peak = ref("peak", cur_peak, 0.01)
        r_disp = ref("disp", live_disp, 0.01)
        r_integ = ref("integ", live_integ, 0.01)
        r_dur = ref("dur", live_bars, 1.0)
        r_q = ref("qual", live_q, 0.05)
        r_e = ref("eff", live_e, 0.05)
        r_p = ref("part", live_p, 0.05)
        p_decay = _decay(live_p, r_p, 0.05) if part_active else 0.0
        p_w = 0.05 if part_active else 0.0
        wave_decay = _clamp((0.35 * _decay(cur_peak, r_peak, 0.05) + 0.18 * _decay(live_disp, r_disp, 0.05)
                             + 0.17 * _decay(live_integ, r_integ, 0.05) + 0.08 * _decay(live_bars, r_dur, 1.0)
                             + 0.10 * _decay(live_q, r_q, 0.05) + 0.07 * _decay(live_e, r_e, 0.05)
                             + p_w * p_decay) / (0.95 + p_w), 0.0, 1.0)
        failed = (ended is not None and ended["pol"] == 1 and prior_peak is not None
                  and ended["peak"] < prior_peak * FAILED_PEAK
                  and (prior_disp is None or ended["disp"] < prior_disp * FAILED_DISP))
        dominant = (ended is not None and ended["pol"] == 1 and prior_peak is not None
                    and ended["peak"] > prior_peak * 1.20)

        hist_bars = _median(leg_dur) if leg_dur else STRUCT_BASE * 3.0
        hist_disp = _median(leg_disp) if leg_disp else 4.0
        hist_cyc = _median(leg_cyc) if leg_cyc else 3.0
        leg_bars = 0.0 if leg_start_bar is None else max(1.0, t - leg_start_bar + 1)
        extension = 0.0 if d == 0 else _safe_div(max((leg_extreme - leg_start_price) * d, 0.0), a, 0.0)
        progress = 0.0 if d == 0 else _safe_div(max((c[t] - leg_start_price) * d, 0.0), a, 0.0)
        cycles = leg_aligned + 0.5 * leg_corr + (0.5 if seg is not None and seg["pol"] == 1 else
                                                 (0.25 if seg is not None and seg["pol"] == -1 else 0.0))
        age_score = _clamp(_safe_div(leg_bars, max(hist_bars, 1.0), 0.0), 0.0, 1.0)
        ext_score = _clamp(_safe_div(extension, max(hist_disp, 0.5), 0.0), 0.0, 1.0)
        cyc_score = _clamp(_safe_div(cycles, max(hist_cyc, 1.0), 0.0), 0.0, 1.0)
        eff_loss = _clamp((eff_ema[t - 1] - eff_ema[t]) * 4.0, 0.0, 1.0)
        price_prog = _clamp(_safe_div(progress, max(hist_disp, 0.5), 0.0), 0.0, 1.5)
        peaks = [w["peak"] for w in waves if w["pol"] == 1]
        max_peak = max(peaks) if peaks else cur_peak
        press_prog = _clamp(_safe_div(cur_peak, max(max_peak, 0.05), 1.0), 0.0, 1.5)
        mismatch = _clamp(price_prog - press_prog, 0.0, 1.0)
        mw = max(sum(MAT_W), 0.001)
        maturity = _clamp((MAT_W[0] * age_score + MAT_W[1] * ext_score + MAT_W[2] * cyc_score
                           + MAT_W[3] * wave_decay + MAT_W[4] * eff_loss + MAT_W[5] * mismatch) / mw, 0.0, 1.0)
        seg_bars = max(1.0, t - seg["bar"] + 1) if seg is not None and seg["pol"] == -1 else 0.0
        corr_norm = _clamp(_safe_div(seg_bars, max(hist_bars * 0.25, 2.0), 0.0), 0.0, 1.0)
        compression = (_clamp(corr_norm * strength * (1.0 - _clamp(ret, 0.0, 1.0)) * (1.0 - counter), 0.0, 1.0)
                       if seg is not None and seg["pol"] == -1 else 0.0)
        ppe = _clamp(_safe_div(max(signed[t], 0.0) / a, max(max(aligned, 0.0), 0.05), 0.0) / 2.0, 0.0, 1.0)
        prev_ppe_ema = ppe_ema_val
        ppe_ema_val = ppe if ppe_ema_val is None else (2.0 / 6.0) * ppe + (1.0 - 2.0 / 6.0) * ppe_ema_val
        ppe_loss = _clamp(prev_ppe_ema - ppe_ema_val, 0.0, 1.0) if prev_ppe_ema is not None else 0.0

        if reset:
            rs = {"seen": False, "stage": 0, "hold": 99, "bars": 0, "start": None, "pp": 0.0, "pr": 0.0,
                  "last": None, "pend": None, "cons": None, "pend_end": None}

        def clear_ctx():
            rs["seen"], rs["bars"], rs["start"], rs["pp"], rs["pr"] = False, 0, None, 0.0, 0.0

        if not rs["seen"]:
            rs["stage"] = 0
        counter_active = aligned <= -WAVE_END or counter > 0.18
        if counter_active and rs["pend"] is not None and serial > rs["pend"]:
            clear_ctx()
            rs["pend"] = None
            rs["pend_end"] = None
        if counter_active:
            if not rs["seen"]:
                rs["hold"], rs["bars"], rs["start"], rs["pp"], rs["pr"] = 99, 0, t, 0.0, 0.0
            rs["seen"] = True
            rs["bars"] += 1
            rs["pp"] = max(rs["pp"], counter)
            rs["pr"] = max(rs["pr"], ret)
        fresh = rs["seen"] and rs["start"] is not None and (counter_active or t - rs["start"] <= WAVE_LEN * 3)
        if rs["seen"] and not fresh:
            clear_ctx()
        if ended is not None and ended["pol"] == -1:
            if rs["bars"] >= RS_MIN_BARS and (rs["pp"] >= RS_MIN_PEAK or rs["pr"] >= RS_MIN_RET):
                rs["pend"], rs["pend_end"] = serial, t
            else:
                clear_ctx()
        pend_fresh = rs["pend"] is not None and rs["pend_end"] is not None and t - rs["pend_end"] <= WAVE_LEN * 2
        if rs["pend"] is not None and not pend_fresh:
            rs["pend"], rs["pend_end"] = None, None
            clear_ctx()
        cp2 = counter_hist[t - VEL_LAG] if t - VEL_LAG >= 0 and counter_hist[t - VEL_LAG] is not None else 0.0
        counter_decay = _clamp((cp2 - counter) * 3.5, 0.0, 1.0)
        accel_trend = _clamp(aligned_acc * 1.8, 0.0, 1.0)
        vel_rec = _clamp(vel[t] * d * 2.0, 0.0, 1.0)
        rec_loc = (1.0 if c[t] > rec_ema[t] else 0.0) if d == 1 else ((1.0 if c[t] < rec_ema[t] else 0.0) if d == -1 else 0.0)
        rec_disp = 0.0 if d == 0 else _clamp(_squash(_safe_div((c[t] - c[t - RECOVERY_LEN]) * d, a, 0.0)) * 1.5, 0.0, 1.0)
        rec_score = 0.45 * rec_loc + 0.55 * rec_disp
        rec_ev = 0.55 * rec_score + 0.45 * vel_rec
        rsw = max(sum(RS_W), 0.001)
        rs_raw = (RS_W[0] * counter_decay + RS_W[1] * accel_trend + RS_W[2] * rec_ev + RS_W[3] * integrity) / rsw \
            if rs["seen"] else 0.0
        rs_quality = _clamp(rs_raw * (0.72 + 0.28 * strength) * (1.0 + 0.20 * compression), 0.0, 1.0)
        owned = pend_fresh and (rs["cons"] is None or rs["pend"] != rs["cons"])
        cooldown = rs["last"] is None or t - rs["last"] >= RS_COOLDOWN
        confirmed = (not reset and owned and cooldown and aligned > RS_CONFIRM and vel_rec > 0.08
                     and integrity > 0.50 and strength > RS_MIN_STRUCT and confidence > RS_MIN_CONF
                     and rs_quality >= RS_MIN_QUALITY)
        if not rs["seen"]:
            rs["stage"] = 0
        else:
            det = 1 if counter_decay > 0.20 else 0
            developed = rs["bars"] >= max(1, RS_MIN_BARS - 1) and (rs["pp"] >= RS_MIN_PEAK * 0.75
                                                                  or rs["pr"] >= RS_MIN_RET * 0.75)
            if not developed:
                det = 0
            if developed and abs(aligned) < WAVE_END and counter_decay > 0.15:
                det = 2
            if developed and aligned_acc > 0.08 and (vel_rec > 0.05 or rec_score > 0.25):
                det = 3
            if confirmed:
                det = 4
            rs["stage"] = max(rs["stage"], det)
        if confirmed:
            rs["hold"] = 0
            rs["last"] = t
            rs["cons"] = rs["pend"]
            rs["pend"], rs["pend_end"] = None, None
            clear_ctx()
        elif not reset:
            rs["hold"] = min(rs["hold"] + 1, 99)
        glow = 1.0 - rs["hold"] / 6.0 if rs["hold"] <= 5 else 0.0
        stage = rs["stage"]

        pols = [w["pol"] for w in waves]
        imp_pat = len(pols) >= 5 and pols[-5:] == [1, -1, 1, -1, 1]
        corr_pat = len(pols) >= 3 and pols[-3:] == [-1, 1, -1]
        a1, a2, a3 = nth(1, 2), nth(1, 1), nth(1, 0)

        def hier(key):
            if a1 and a2 and a3:
                x1, x2, x3 = a1[key], a2[key], a3[key]
                return _clamp((x2 / max(x1, 0.05) - 0.8) * 0.8 + (1.0 - x3 / max(x2, 0.05)) * 0.6, 0.0, 1.0)
            return None

        exp_h = hier("peak")
        exp_h = 0.0 if exp_h is None else exp_h
        disp_h = hier("disp")
        disp_h = exp_h if disp_h is None else disp_h
        int_h = hier("integ")
        int_h = exp_h if int_h is None else int_h
        seq_h = _clamp(0.50 * exp_h + 0.25 * disp_h + 0.25 * int_h, 0.0, 1.0)
        three = a1 and a2 and a3
        q_mean = (a1["qual"] + a2["qual"] + a3["qual"]) / 3.0 if three else quality
        e_mean = (a1["eff"] + a2["eff"] + a3["eff"]) / 3.0 if three else q_eff[t]
        p_mean = (a1["part"] + a2["part"] + a3["part"]) / 3.0 if three else part
        seq_pw = 0.10 if part_active else 0.0
        seq_q = _clamp((0.72 * q_mean + 0.28 * e_mean + seq_pw * p_mean) / (1.0 + seq_pw), 0.0, 1.0)
        lc = nth(-1, 0)
        last_corr_ret = lc["ret"] if lc else None
        corr_control = integrity if last_corr_ret is None else _clamp(1.0 - last_corr_ret, 0.0, 1.0)
        imp_seq = _clamp(0.30 * seq_h + 0.30 * seq_q + 0.20 * corr_control + 0.20 * integrity, 0.0, 1.0) if imp_pat else 0.0

        def recent(k, key, fb):
            return sum(w[key] for w in waves[-k:]) / k if len(waves) >= k else fb

        imp_ctx = _clamp(0.55 * imp_seq + 0.25 * recent(5, "qual", quality) + 0.20 * recent(5, "eff", q_eff[t]),
                         0.0, 1.0) if imp_pat else 0.0
        imp_ok = imp_pat and imp_ctx >= CONTEXT_MIN
        depth = counter if last_corr_ret is None else _clamp(last_corr_ret / 0.35, 0.0, 1.0)
        corr_ctx = _clamp(0.40 * recent(3, "qual", quality) + 0.25 * recent(3, "eff", q_eff[t])
                          + 0.20 * integrity + 0.15 * depth, 0.0, 1.0) if corr_pat else 0.0
        corr_ok = corr_pat and corr_ctx >= CONTEXT_MIN
        if reset:
            last_imp_serial = last_corr_serial = None
        late_imp = _clamp(maturity * (0.45 * wave_decay + 0.30 * seq_h + 0.25 * imp_ctx), 0.0, 1.0) if imp_ok else 0.0
        if imp_ok:
            wave_ctx = 5.0 if late_imp > 0.55 else (4.0 if seq_h > 0.45 else 3.0)
        elif corr_ok:
            wave_ctx = 2.0
        elif leg_aligned >= 2:
            wave_ctx = 1.0
        else:
            wave_ctx = 0.0

        dec_ev = _clamp((0.42 * wave_decay + 0.28 * eff_loss + 0.20 * ppe_loss + PART_WEIGHT * part_loss)
                        / max(0.90 + PART_WEIGHT, 0.01), 0.0, 1.0)
        cont_fail = _clamp(0.53 * mismatch + 0.47 * (1.0 if failed else 0.0), 0.0, 1.0)
        exhaustion = _clamp(maturity * (0.55 * dec_ev + 0.45 * cont_fail) * 1.35, 0.0, 1.0) \
            if maturity >= EXH_MATURITY else 0.0
        deterioration = _clamp(0.45 * wave_decay + 0.30 * mismatch + 0.25 * cont_fail, 0.0, 1.0)
        weak_state = a2 is not None and a3 is not None and a3["peak"] < a2["peak"] * 0.80 and maturity > 0.45
        weakening = weak_state and not prev_weak

        transition = d == 0 or strength < 0.16 or confidence < 0.18
        initiation = (not transition and leg_bars <= max(4.0, hist_bars * 0.18)
                      and prev_strength is not None and strength > prev_strength)
        accel_ph = (not transition and aligned > 0.08 and aligned_acc > 0.10
                    and energy[t - 1] is not None and en > energy[t - 1])
        corr_ph = not transition and ((seg is not None and seg["pol"] == -1) or counter > 0.22) and integrity > 0.28
        comp_ph = corr_ph and compression > 0.34
        rs_ph = stage >= 2 or glow > 0.0
        late_ph = not transition and aligned > 0.06 and maturity > 0.56 and (wave_decay > 0.18 or late_imp > 0.35)
        exh_ph = not transition and exhaustion > 0.42
        exp_ph = not transition and aligned > 0.12 and quality > 0.25 and counter < 0.28
        if flip:
            cand = 9
        elif transition:
            cand = 0
        elif exh_ph:
            cand = 8
        elif rs_ph:
            cand = 6
        elif comp_ph:
            cand = 5
        elif corr_ph:
            cand = 4
        elif late_ph:
            cand = 7
        elif exp_ph:
            cand = 3
        elif accel_ph:
            cand = 2
        elif initiation:
            cand = 1
        else:
            cand = 0
        if flip:
            phase, pending_phase, pending_bars = 9, 9, 0
        elif exit_ or d == 0:
            phase, pending_phase, pending_bars = 0, 0, 0
        elif phase == 9:
            phase, pending_phase, pending_bars = cand, cand, 0
        elif cand == phase:
            pending_phase, pending_bars = phase, 0
        else:
            if cand != pending_phase:
                pending_phase, pending_bars = cand, 1
            else:
                pending_bars += 1
            if pending_bars >= PHASE_CONFIRM:
                phase, pending_bars = pending_phase, 0
        if phase == 1:
            pconf = _clamp(0.5 * strength + 0.5 * (1.0 - age_score), 0.0, 1.0)
        elif phase == 2:
            pconf = _clamp(0.45 * max(aligned, 0.0) + 0.35 * accel_trend + 0.20 * quality, 0.0, 1.0)
        elif phase == 3:
            pconf = _clamp(0.35 * strength + 0.35 * max(aligned, 0.0) + 0.30 * quality, 0.0, 1.0)
        elif phase == 4:
            pconf = _clamp(0.55 * counter + 0.45 * integrity, 0.0, 1.0)
        elif phase == 5:
            pconf = compression
        elif phase == 6:
            pconf = max(rs_quality, glow)
        elif phase == 7:
            pconf = _clamp(0.55 * maturity + 0.45 * max(aligned, 0.0), 0.0, 1.0)
        elif phase == 8:
            pconf = exhaustion
        elif phase == 9:
            pconf = 1.0
        else:
            pconf = _clamp(1.0 - confidence, 0.0, 1.0)

        if ended is not None and imp_ok and (last_imp_serial is None or serial - last_imp_serial >= 5):
            last_imp_serial = serial
        if ended is not None and corr_ok and (last_corr_serial is None or serial - last_corr_serial >= 3):
            last_corr_serial = serial

        side = "bull" if d == 1 else "bear"
        fired = []
        if initiated:
            fired.append(f"{side}_flow_initiated")
        if exit_:
            fired.append("flow_transition")
        if flip:
            fired.append(f"{side}_flow_flip")
        if phase == 3 and prev_phase != 3:
            fired.append(f"{side}_expansion")
        if phase == 4 and prev_phase != 4:
            fired.append("correction_start")
        if phase == 5 and prev_phase != 5:
            fired.append("compression")
        if stage == 1 and prev_stage == 0:
            fired.append(f"{side}_early_resync")
        if confirmed:
            fired.append(f"{side}_resync")
        if phase == 7 and prev_phase != 7:
            fired.append("late_expansion")
        if phase == 8 and prev_phase != 8:
            fired.append("exhaustion")
        if failed:
            fired.append("failed_expansion")
        if dominant:
            fired.append(f"{side}_dominant_expansion")
        if weakening:
            fired.append("weakening_waves")
        for k in fired:
            out["events"][k].append(t)
        prev_strength, prev_phase, prev_stage, prev_weak = strength, phase, stage, weak_state

        out["rows"][t] = {
            "drive": drive, "aligned": aligned, "direction": float(d), "strength": strength,
            "confidence": confidence, "internal_wave": internal[t], "wave_quality": quality,
            "counter_pressure": counter, "integrity": integrity, "retracement": ret,
            "maturity": maturity, "compression": compression, "resync_quality": rs_quality,
            "resync_stage": float(stage), "exhaustion": exhaustion, "deterioration": deterioration,
            "wave_decay": wave_decay, "impulse_context": imp_ctx, "correction_context": corr_ctx,
            "wave_context": wave_ctx, "phase": float(phase), "phase_confidence": pconf,
        }
    return out


HEADER = [
    "kestrel-chartkit golden reference fixture: Structural Wave Field",
    "",
    *provenance("structural_wave_field"),
    "",
    "Formula: the published Pine Script 'Structural Wave Field' (WavesUnchained, v1.0.2), Balanced",
    "preset, Relative Volume participation; the visual field is not part of it. Conventions where",
    "Pine leaves them open: EMAs seeded with the first available value, ATR = Wilder average of the",
    "true range (first bar high - low) seeded with an SMA and floored at 1e-12 (also for mintick),",
    "population standard deviation, Pine's array.median, relative volume 1 while the volume average",
    "is missing or 0. Every series starts once its inputs exist; the state machines and output",
    "start on the first bar on which every input exists (the slow horizon's persistence), with",
    "resting previous values (phase 0, Re-Sync stage 0, counter pressure 0) and no previous",
    "strength.",
    "",
    "Series: 1400 bars, prev = previous close (100 before the first bar); close = prev + A sin(i/P)",
    "+ 0.15 sin(i/37) + 0.7 sin(i/W) + 0.35 cos(i/2.1), replaced by prev - 25 when i mod 300 = 250",
    "and floor(i/300) is even, prev + 25 when it is odd; open = prev + 0.25 sin(i/1.8); high =",
    "max(open, close) + 0.2 + 0.25 |sin(i/3.7)|; low = min(open, close) - 0.2 - 0.25 |cos(i/4.3)|.",
    "Block 'v': A = 0.8, P = 110, W = 11, volume = 1000 + 700 sin(i/7)^2 + 300 cos(i/19); block",
    "'nv': A = 0.45, P = 60, W = 6.3, volume 0.",
    "",
    "<v|nv>_first is the first reporting bar; _cp<bar>_* the outputs at that bar; _ev_<kind>_n the",
    "number of alerts of that kind and _ev_<kind>_<j> the bar of the j-th (up to 8).",
]


def _block(prefix, out):
    case = {"first": float(out["first"])}
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
        _block("v", compute(_series("v"))),
        _block("nv", compute(_series("nv"))),
        (None, None, {"swf_tolerance": 1e-9}),
    ]
    return HEADER, blocks
