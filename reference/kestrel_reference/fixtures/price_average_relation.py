"""golden_price_average_relation: Price-Average Relation.

Written from the original definition, not from the Rust code: every
series is a full array, every step propagates "not available" (None) strictly, and a bar reports
only when everything it reads is available.
"""

import math

from ..fixture import provenance

NAME = "golden_price_average_relation"

# Default parameters (EMA family).
MA_LEN = 50
ATR_LEN = 14
SLOPE_LEN = 10
FULL_SLOPE = 0.50
PERSIST = 20
EFF_LEN = 14
STATS = 100
EXT_NORMAL = 0.75
EXT_EXTREME = 2.50
PROXIMITY = 0.35
RESPECT_LOOKBACK = 80
RESPECT_RESPONSE = 4
COMP_LEN = 20
COMP_BAND = 0.45
ACC_LEN = 5
EXH_LEN = 8
COOLDOWN = 8
MIN_TREND = 55.0
MIN_RESPECT = 55.0
PB_PRIOR_EXT = 0.70
PB_MAX_EXH = 65.0
ACC_THRESHOLD = 60.0
COMP_THRESHOLD = 65.0
CHASE_EXT = 75.0
EXH_THRESHOLD = 68.0
BREAK_CONFIRM = 3
PIVOT_LEFT = 5
PIVOT_RIGHT = 5
TURN_LOOKBACK = 30
TURN_LEVEL = 55.0
TURN_CONFIRM = 3

N_BARS = 900
CHECKPOINTS = [200, 333, 450, 617, 780, 899]
EVENTS = [
    "bull_pullback", "bear_pullback", "bull_momentum_release", "bear_momentum_release",
    "do_not_chase_up", "do_not_chase_down", "bear_exhaustion", "bull_exhaustion",
    "bear_relationship_break", "bull_relationship_break", "bull_divergence", "bear_divergence",
    "bull_turn", "bear_turn",
]


def _series():
    """Deterministic OHLC: two trends, a range, a crash and a recovery, with a fast wiggle."""
    bars = []
    prev_close = 100.0
    for i in range(N_BARS):
        x = float(i)
        drift = 0.18 if i < 220 else (-0.02 if i < 380 else (-0.30 if i < 470 else 0.12))
        amp = 1.0
        if 560 <= i < 650:
            drift, amp = 0.0, 0.08
        elif 650 <= i < 690:
            drift = 0.45
        base = prev_close + drift + amp * (0.9 * math.sin(x / 3.1) + 0.6 * math.sin(x / 11.0))
        close = base + amp * 0.35 * math.cos(x / 1.7)
        open_ = prev_close + 0.2 * math.sin(x / 2.3)
        high = max(open_, close) + 0.4 + 0.3 * abs(math.sin(x / 4.7))
        low = min(open_, close) - 0.4 - 0.3 * abs(math.cos(x / 5.3))
        bars.append((open_, high, low, close))
        prev_close = close
    return bars


def _ok(*vals):
    return all(v is not None for v in vals)


def _clamp(v, lo, hi):
    return max(lo, min(hi, v))


def _safe_div(n, d, fallback=0.0):
    return n / d if abs(d) > 1e-10 else fallback


def _norm(v, lo, hi):
    return _clamp((v - lo) / (hi - lo), 0.0, 1.0) if hi > lo else 0.0


def _sign(v, dead=0.0):
    return 1.0 if v > dead else (-1.0 if v < -dead else 0.0)


def _at(s, t, k):
    """s[t - k] with Pine's history operator: None before the series start."""
    j = t - k
    return s[j] if j >= 0 else None


def _sma(s, n):
    out = [None] * len(s)
    for t in range(len(s)):
        if t >= n - 1:
            w = s[t - n + 1:t + 1]
            if _ok(*w):
                out[t] = sum(w) / n
    return out


def _stdev(s, n):
    """Population standard deviation (Pine ta.stdev, biased)."""
    out = [None] * len(s)
    for t in range(len(s)):
        if t >= n - 1:
            w = s[t - n + 1:t + 1]
            if _ok(*w):
                m = sum(w) / n
                out[t] = math.sqrt(sum((v - m) ** 2 for v in w) / n)
    return out


def _ema(s, n):
    """alpha = 2/(n+1), seeded with the first available value (Pine ta.ema)."""
    a = 2.0 / (n + 1.0)
    out = [None] * len(s)
    prev = None
    for t, v in enumerate(s):
        if v is None:
            continue
        prev = v if prev is None else a * v + (1.0 - a) * prev
        out[t] = prev
    return out


def _rma(s, n):
    """Wilder average seeded with the SMA of the first n values (Pine ta.rma)."""
    out = [None] * len(s)
    prev = None
    for t in range(len(s)):
        if prev is None:
            if t >= n - 1:
                prev = sum(s[t - n + 1:t + 1]) / n
                out[t] = prev
        else:
            prev = (s[t] + (n - 1) * prev) / n
            out[t] = prev
    return out


def _map(f, *series):
    return [f(*vals) if _ok(*vals) else None for vals in zip(*series)]


def compute(bars):
    n = len(bars)
    o = [b[0] for b in bars]
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    idx = range(n)

    ma = _ema(c, MA_LEN)
    tr = [h[0] - lo[0]] + [max(h[t] - lo[t], abs(h[t] - c[t - 1]), abs(lo[t] - c[t - 1]))
                           for t in range(1, n)]
    atr = _rma(tr, ATR_LEN)

    sd = _map(lambda cc, m, a: _safe_div(cc - m, a), c, ma, atr)
    ad = _map(abs, sd)
    slope = [(_safe_div(ma[t] - _at(ma, t, SLOPE_LEN), atr[t])
              if _ok(_at(ma, t, SLOPE_LEN), atr[t]) else None) for t in idx]
    slope_strength = _map(lambda s: 100.0 * _norm(abs(s), 0.0, FULL_SLOPE), slope)
    ma_dir = _map(lambda s: _sign(s, FULL_SLOPE * 0.05), slope)
    side = _map(lambda cc, m, a: _sign(cc - m, a * 0.03), c, ma, atr)

    bull_side = _sma([1.0 if c[t] > ma[t] else 0.0 for t in idx], PERSIST)
    bear_side = _sma([1.0 if c[t] < ma[t] else 0.0 for t in idx], PERSIST)
    side_persist = _map(lambda s, b, r: b if s > 0 else (r if s < 0 else 0.5),
                        side, bull_side, bear_side)
    up = [None] + [1.0 if ma[t] > ma[t - 1] else 0.0 for t in range(1, n)]
    down = [None] + [1.0 if ma[t] < ma[t - 1] else 0.0 for t in range(1, n)]
    bull_slope = _sma(up, PERSIST)
    bear_slope = _sma(down, PERSIST)
    slope_persist = _map(lambda d, b, r: b if d > 0 else (r if d < 0 else 0.5),
                         ma_dir, bull_slope, bear_slope)

    moves = [None] + [abs(c[t] - c[t - 1]) for t in range(1, n)]
    path = _map(lambda v: v * EFF_LEN, _sma(moves, EFF_LEN))
    net = [abs(c[t] - c[t - EFF_LEN]) if t >= EFF_LEN else None for t in idx]
    eff = _map(lambda nt, p: _clamp(_safe_div(nt, p), 0.0, 1.0), net, path)

    def agreement(s, d):
        if s != 0 and d != 0 and s == d:
            return 1.0
        if s != 0 and d == 0:
            return 0.45
        if s == 0:
            return 0.25
        return 0.0

    agree = _map(agreement, side, ma_dir)
    trend = _map(lambda ss, sp, slp, e, ag: 100.0 * _clamp(
        0.30 * ss / 100.0 + 0.25 * sp + 0.20 * slp + 0.15 * e + 0.10 * ag, 0.0, 1.0),
        slope_strength, side_persist, slope_persist, eff, agree)
    direction = _map(lambda ag, s, d: s if ag >= 1.0 else (d if d != 0 else s), agree, side, ma_dir)
    signed_trend = _map(lambda tq, d: tq * d, trend, direction)

    # Respect: every touch episode in the lookback, judged RESPECT_RESPONSE bars later.
    respect_raw = [None] * n
    samples_out = [None] * n
    hit_out = [None] * n
    for t in idx:
        oldest = t - (RESPECT_LOOKBACK + 1)
        if oldest < 0 or atr[oldest] is None:
            continue
        succ = fail = 0.0
        weighted = 0.0
        for i in range(RESPECT_RESPONSE, RESPECT_LOOKBACK + 1):
            k = t - i
            touched = abs(c[k] - ma[k]) <= atr[k] * PROXIMITY
            before = abs(c[k - 1] - ma[k - 1]) <= atr[k - 1] * PROXIMITY
            if touched and not before:
                trend_side = 1.0 if ma[k] - ma[k - 1] >= 0 else -1.0
                q = k + RESPECT_RESPONSE
                progress = _safe_div((c[q] - c[k]) * trend_side, atr[q])
                held = (c[q] - ma[q]) * trend_side > 0
                ok = held and progress > 0
                succ += 1.0 if ok else 0.0
                fail += 0.0 if ok else 1.0
                weighted += _clamp(progress, -1.0, 1.0)
        samples = succ + fail
        hit = succ / samples if samples > 0 else 0.5
        quality = _norm(weighted / samples, -0.35, 0.50) if samples > 0 else 0.5
        conf = _norm(samples, 2.0, 10.0)
        raw = 100.0 * (0.70 * hit + 0.30 * quality)
        respect_raw[t] = _clamp(50.0 + (raw - 50.0) * conf, 0.0, 100.0)
        samples_out[t] = samples
        hit_out[t] = 100.0 * hit
    crosses = [None] + [1.0 if ((c[t] > ma[t] and c[t - 1] <= ma[t - 1])
                                or (c[t] < ma[t] and c[t - 1] >= ma[t - 1])) else 0.0
                        for t in range(1, n)]
    flip = _sma(crosses, PERSIST)
    respect = _map(lambda r, f: _clamp(0.80 * r + 0.20 * 100.0 * (1.0 - _clamp(f * 4.0, 0.0, 1.0)),
                                       0.0, 100.0), respect_raw, flip)

    ext_atr = _map(lambda a: 100.0 * _norm(a, EXT_NORMAL, EXT_EXTREME), ad)
    mean = _sma(ad, STATS)
    dev = _stdev(ad, STATS)
    extension = _map(lambda ea, a, m, d: _clamp(
        0.70 * ea + 0.30 * 100.0 * _norm(_safe_div(a - m, d), 0.0, 2.5), 0.0, 100.0),
        ext_atr, ad, mean, dev)

    near = _sma(_map(lambda a: 1.0 if a <= COMP_BAND else 0.0, ad), COMP_LEN)
    avg_dist = _sma(ad, COMP_LEN)
    atr_base = _sma(atr, STATS)
    compression = _map(lambda nr, av, a, b: 100.0 * _clamp(
        0.50 * nr
        + 0.30 * (1.0 - _norm(av, COMP_BAND * 0.40, COMP_BAND * 1.80))
        + 0.20 * (1.0 - _norm(_safe_div(a, b, 1.0), 0.70, 1.20)), 0.0, 1.0),
        near, avg_dist, atr, atr_base)

    directed = _map(lambda s, d: s * d, sd, direction)
    acceleration = [None] * n
    for t in idx:
        need = (directed[t], _at(directed, t, ACC_LEN), slope[t], direction[t],
                _at(ma, t, ACC_LEN), _at(ma, t, ACC_LEN + SLOPE_LEN), _at(atr, t, ACC_LEN),
                atr[t], _at(c, t, 2 * ACC_LEN))
        if not _ok(*need):
            continue
        d = direction[t]
        sep_vel = (directed[t] - directed[t - ACC_LEN]) / ACC_LEN
        cur_sv = slope[t] / SLOPE_LEN * d
        prior_slope = _safe_div(ma[t - ACC_LEN] - ma[t - ACC_LEN - SLOPE_LEN], atr[t - ACC_LEN])
        slope_acc = cur_sv - prior_slope / SLOPE_LEN * d
        recent = _safe_div(c[t] - c[t - ACC_LEN], atr[t]) * d
        prior = _safe_div(c[t - ACC_LEN] - c[t - 2 * ACC_LEN], atr[t - ACC_LEN]) * d
        acceleration[t] = _clamp(
            0.50 * 100.0 * _norm(sep_vel, 0.0, 0.25)
            + 0.20 * 100.0 * _norm(slope_acc, 0.0, 0.04)
            + 0.30 * 100.0 * _norm(recent - prior, 0.0, 1.50), 0.0, 100.0)

    def opposing(t):
        d = direction[t]
        if d is None:
            return None
        rng = max(h[t] - lo[t], 1e-12)
        if d > 0:
            return (h[t] - max(o[t], c[t])) / rng
        if d < 0:
            return (min(o[t], c[t]) - lo[t]) / rng
        return 0.0

    wick = _sma([opposing(t) for t in idx], EXH_LEN)
    exhaustion = [None] * n
    for t in idx:
        need = (eff[t], _at(eff, t, EXH_LEN), wick[t], direction[t], atr[t],
                _at(c, t, EXH_LEN), acceleration[t], _at(acceleration, t, EXH_LEN), extension[t])
        if not _ok(*need):
            continue
        decay = _norm(eff[t - EXH_LEN] - eff[t], 0.0, 0.35)
        progress = _safe_div((c[t] - c[t - EXH_LEN]) * direction[t], atr[t])
        fail = 1.0 - _norm(progress, 0.0, 1.50)
        loss = _norm(acceleration[t - EXH_LEN] - acceleration[t], 0.0, 50.0)
        exhaustion[t] = 100.0 * _clamp(
            extension[t] / 100.0 * (0.35 * decay + 0.25 * wick[t] + 0.25 * fail + 0.15 * loss),
            0.0, 1.0)

    raw_rel = _map(lambda st, e, x: st * (1.0 - _clamp(
        0.18 * _norm(e, CHASE_EXT, 100.0) + 0.32 * _norm(x, EXH_THRESHOLD, 100.0), 0.0, 0.50)),
        signed_trend, extension, exhaustion)
    rel = _ema(raw_rel, 3)
    rel_signal = _ema(rel, 5)

    # Events. A condition is evaluated only where all its inputs exist; a transition (x and not
    # x[1]) needs the previous bar's condition too.
    prior_ext_len = max(2, ACC_LEN + 1)

    def cond_pullback(t, bull):
        need = (_at(direction, t, 1), _at(trend, t, 1), direction[t], trend[t], respect[t],
                exhaustion[t], ma[t], atr[t])
        win = [_at(sd, t, k) for k in range(1, prior_ext_len + 1)]
        if not _ok(*need, *win):
            return None
        s = 1.0 if bull else -1.0
        prior = _at(direction, t, 1) * s > 0 and _at(trend, t, 1) >= MIN_TREND
        prior_ext = max(win) if bull else -min(win)
        if bull:
            rej = (lo[t] <= ma[t] + atr[t] * PROXIMITY and c[t] > ma[t] and c[t] > o[t]
                   and c[t] >= (h[t] + lo[t]) * 0.5)
        else:
            rej = (h[t] >= ma[t] - atr[t] * PROXIMITY and c[t] < ma[t] and c[t] < o[t]
                   and c[t] <= (h[t] + lo[t]) * 0.5)
        return (prior and direction[t] * s > 0 and trend[t] >= MIN_TREND
                and respect[t] >= MIN_RESPECT and prior_ext >= PB_PRIOR_EXT and rej
                and exhaustion[t] <= PB_MAX_EXH)

    def cond_momentum(t, bull):
        win = [_at(compression, t, k) for k in range(1, COMP_LEN + 1)]
        need = (sd[t], _at(sd, t, 1), slope[t], acceleration[t], eff[t], exhaustion[t])
        if not _ok(*need, *win):
            return None
        was = max(win) >= COMP_THRESHOLD
        if bull:
            cross = sd[t] > COMP_BAND and sd[t - 1] <= COMP_BAND
            slope_ok = slope[t] > 0
        else:
            cross = sd[t] < -COMP_BAND and sd[t - 1] >= -COMP_BAND
            slope_ok = slope[t] < 0
        return (was and cross and slope_ok and acceleration[t] >= ACC_THRESHOLD
                and eff[t] >= 0.35 and exhaustion[t] < EXH_THRESHOLD)

    def cond_chase(t, bull):
        need = (direction[t], trend[t], extension[t], _at(extension, t, 1))
        if not _ok(*need):
            return None
        s = 1.0 if bull else -1.0
        return (direction[t] * s > 0 and trend[t] >= MIN_TREND
                and extension[t] > CHASE_EXT and extension[t - 1] <= CHASE_EXT)

    def cond_exhaustion(t, bull):
        need = (direction[t], exhaustion[t], _at(exhaustion, t, 1))
        if not _ok(*need):
            return None
        s = 1.0 if bull else -1.0
        return (direction[t] * s > 0
                and exhaustion[t] > EXH_THRESHOLD and exhaustion[t - 1] <= EXH_THRESHOLD)

    other_side = _sma([1.0 if c[t] < ma[t] else 0.0 for t in idx], BREAK_CONFIRM)
    this_side = _sma([1.0 if c[t] > ma[t] else 0.0 for t in idx], BREAK_CONFIRM)

    def broken_raw(t, was_up):
        need = (_at(direction, t, BREAK_CONFIRM), _at(trend, t, BREAK_CONFIRM),
                other_side[t], this_side[t], rel[t], slope[t])
        if not _ok(*need):
            return None
        s = 1.0 if was_up else -1.0
        past = (_at(direction, t, BREAK_CONFIRM) * s > 0
                and _at(trend, t, BREAK_CONFIRM) >= MIN_TREND)
        if was_up:
            return (past and other_side[t] >= 0.80 and rel[t] < 10.0
                    and slope[t] <= FULL_SLOPE * 0.10)
        return (past and this_side[t] >= 0.80 and rel[t] > -10.0
                and slope[t] >= -FULL_SLOPE * 0.10)

    def turn_raw(t, bull):
        win = [_at(rel, t, k) for k in range(TURN_LOOKBACK)]
        prev = [_at(rel, t, k) for k in range(1, TURN_CONFIRM + 1)]
        if not _ok(*win, *prev):
            return None
        if bull:
            return min(win) <= -TURN_LEVEL and all(rel[t] > p for p in prev)
        return max(win) >= TURN_LEVEL and all(rel[t] < p for p in prev)

    def transition(f, t, flag):
        now = f(t, flag)
        before = f(t - 1, flag) if t >= 1 else None
        if now is None or before is None:
            return None
        return now and not before

    def pivot(t, low_side):
        """Strict pivot at t - PIVOT_RIGHT: below (above) every bar within PIVOT_LEFT before and
        PIVOT_RIGHT after it."""
        k = t - PIVOT_RIGHT
        if k - PIVOT_LEFT < 0:
            return None
        src = lo if low_side else h
        centre = src[k]
        others = src[k - PIVOT_LEFT:k] + src[k + 1:t + 1]
        if low_side:
            return centre if all(centre < v for v in others) else None
        return centre if all(centre > v for v in others) else None

    families = {
        "pullback": None, "momentum": None, "chase": None, "exhaustion": None,
        "broken": None, "divergence": None, "turn": None,
    }
    last_pl = last_pl_rel = last_ph = last_ph_rel = None
    events = {name: [] for name in EVENTS}
    reported = [False] * n

    for t in idx:
        core = (rel[t], rel_signal[t], trend[t], respect[t], extension[t], compression[t],
                acceleration[t], exhaustion[t])
        reported[t] = _ok(*core)

        pl = pivot(t, True)
        ph = pivot(t, False)
        rel_at_pivot = _at(rel, t, PIVOT_RIGHT)
        bull_div = bear_div = None
        if reported[t]:
            bull_div = (pl is not None and last_pl is not None and rel_at_pivot is not None
                        and pl < last_pl and rel_at_pivot > last_pl_rel)
            bear_div = (ph is not None and last_ph is not None and rel_at_pivot is not None
                        and ph > last_ph and rel_at_pivot < last_ph_rel)
        if pl is not None and rel_at_pivot is not None:
            last_pl, last_pl_rel = pl, rel_at_pivot
        if ph is not None and rel_at_pivot is not None:
            last_ph, last_ph_rel = ph, rel_at_pivot

        if not reported[t]:
            continue

        candidates = [
            ("pullback", "bull_pullback", cond_pullback(t, True)),
            ("pullback", "bear_pullback", cond_pullback(t, False)),
            ("momentum", "bull_momentum_release", cond_momentum(t, True)),
            ("momentum", "bear_momentum_release", cond_momentum(t, False)),
            ("chase", "do_not_chase_up", cond_chase(t, True)),
            ("chase", "do_not_chase_down", cond_chase(t, False)),
            ("exhaustion", "bear_exhaustion", cond_exhaustion(t, True)),
            ("exhaustion", "bull_exhaustion", cond_exhaustion(t, False)),
            ("broken", "bear_relationship_break", transition(broken_raw, t, True)),
            ("broken", "bull_relationship_break", transition(broken_raw, t, False)),
            ("divergence", "bull_divergence", bull_div),
            ("divergence", "bear_divergence", bear_div),
            ("turn", "bull_turn", transition(turn_raw, t, True)),
            ("turn", "bear_turn", transition(turn_raw, t, False)),
        ]
        allowed = {f: (last is None or t - last - 1 > COOLDOWN) for f, last in families.items()}
        fired = set()
        for family, name, cond in candidates:
            if cond and allowed[family]:
                events[name].append(t)
                fired.add(family)
        for family in fired:
            families[family] = t

    return {
        "reported": reported, "relationship": rel, "signal": rel_signal, "trend": trend,
        "respect": respect, "extension": extension, "compression": compression,
        "acceleration": acceleration, "exhaustion": exhaustion, "ma": ma,
        "distance_atr": sd, "respect_samples": samples_out, "events": events,
    }


HEADER = [
    "kestrel-chartkit golden reference fixture: Price-Average Relation",
    "",
    *provenance("price_average_relation"),
    "",
    "Formula: the original definition",
    "with its defaults, EMA family, length 50. Conventions the original leaves open:",
    "EMA seeded with the first value, ATR = Wilder average of the true range (first bar high -",
    "low) seeded with an SMA, population standard deviation, mintick floor replaced by 1e-12,",
    "ta.rising = greater than each of the previous n values, pivots strict on both sides. Every",
    "step propagates 'not available'; a bar reports when relationship, signal and all six scores",
    "exist; events are evaluated only on reported bars and need every input they read, the",
    "previous bar's condition included for transitions. Cooldown 8 bars per event family,",
    "counted from the last fired event of that family.",
    "",
    "Series: 900 bars, prev = previous close (100 before the first bar), drift 0.18 up to bar",
    "219, -0.02 to 379, -0.30 to 469, 0.12 after, except 0 with amp 0.08 for bars 560-649 and",
    "0.45 for 650-689 (amp 1 elsewhere); close = prev + drift + amp (0.9 sin(i/3.1) +",
    "0.6 sin(i/11) + 0.35 cos(i/1.7)); open = prev + 0.2 sin(i/2.3); high = max(open, close) +",
    "0.4 + 0.3 |sin(i/4.7)|; low = min(open, close) - 0.4 - 0.3 |cos(i/5.3)|.",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6).",
]


def build():
    out = compute(_series())
    first = next(t for t, r in enumerate(out["reported"]) if r)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {k: out[k][cp] for k in (
            "relationship", "signal", "trend", "respect", "extension", "compression",
            "acceleration", "exhaustion", "ma", "distance_atr", "respect_samples")}
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    ev = {}
    for name in EVENTS:
        bars = out["events"][name]
        ev[f"{name}_n"] = float(len(bars))
        for j, b in enumerate(bars[:6]):
            ev[f"{name}_{j}"] = float(b)
    blocks.append(("ev", "events", ev))
    blocks.append((None, None, {"relation_tolerance": 1e-9}))
    return HEADER, blocks
