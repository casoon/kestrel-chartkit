"""golden_flow_exhaustion: Flow Exhaustion (MFI + StochRSI).

Written from the original definition, not from the Rust code. MFI oscillator mode only, no
higher-timeframe filter, manual divergence parameters (the timeframe presets need the chart
timeframe).
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_flow_exhaustion"

MFI_LEN = 14
ZONE_LOOKBACK = 200
UPPER_FRAC = 0.85
LOWER_FRAC = 0.15
RSI_LEN = 14
STOCH_LEN = 14
K_SMOOTH = 3
STOCH_OS = 20.0
STOCH_OB = 80.0
CONFIRM = 1
JMA_LEN = 20
JMA_PHASE = 0.0
JMA_POWER = 2.0
PIV_LEFT = 7
PIV_RIGHT = 7
MIN_DELTA = 15.0
BUFFER = 0.10
ATR_LEN = 14
MIN_ATR_MOVE = 0.75
PRICE_EPS = 0.0010
MFI_EPS = 1.0
REACT_BARS = 10
REACT_ATR = 0.35

W_DELTA = 0.18 / 0.82
W_ATR = 0.16 / 0.82
W_TIME = 0.18 / 0.82
W_REACT = 0.18 / 0.82
W_SEQ = 0.12 / 0.82

N_BARS = 900
CHECKPOINTS = [150, 260, 333, 450, 617, 780, 899]
EVENTS = ["bull_exhaustion", "bear_exhaustion", "bull_divergence", "bear_divergence"]


def compute(bars):
    n = len(bars)
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    v = [b[4] for b in bars]
    hlc3 = [(h[t] + lo[t] + c[t]) / 3.0 for t in range(n)]

    mfi_raw = ps.mfi(hlc3, v, MFI_LEN)
    mfi = ps.jma(mfi_raw, JMA_LEN, JMA_PHASE, JMA_POWER)
    hi = ps.highest(mfi, ZONE_LOOKBACK)
    lw = ps.lowest(mfi, ZONE_LOOKBACK)
    upper = ps.smap(lambda a, b: b + (a - b) * UPPER_FRAC, hi, lw)
    lower = ps.smap(lambda a, b: b + (a - b) * LOWER_FRAC, hi, lw)

    rsi = ps.rsi(c, RSI_LEN)
    r_lo = ps.lowest(rsi, STOCH_LEN)
    r_hi = ps.highest(rsi, STOCH_LEN)
    stoch = ps.smap(lambda r, a, b: 100.0 * (r - a) / max(b - a, 1e-10), rsi, r_lo, r_hi)
    k = ps.jma(ps.sma(stoch, K_SMOOTH), JMA_LEN, JMA_PHASE, JMA_POWER)
    atr = ps.atr(h, lo, c, ATR_LEN)

    def is_os(t):
        return ps.ok(mfi[t], lower[t]) and mfi[t] < lower[t]

    def is_ob(t):
        return ps.ok(mfi[t], upper[t]) and mfi[t] > upper[t]

    def confirmed(f, t):
        return all(t - i >= 0 and f(t - i) for i in range(CONFIRM + 1))

    def in_os(m, t):
        if ps.ok(upper[t], lower[t]):
            band = max(upper[t] - lower[t], 1.0)
            return m <= lower[t] + band * BUFFER
        return m <= 25.0

    def in_ob(m, t):
        if ps.ok(upper[t], lower[t]):
            band = max(upper[t] - lower[t], 1.0)
            return m >= upper[t] - band * BUFFER
        return m >= 75.0

    events = {e: [] for e in EVENTS}
    reported = [ps.ok(mfi[t], k[t]) for t in range(n)]
    last = {True: None, False: None}  # side -> (price, osc, bar)
    seq = {True: 0, False: 0}
    pending = []  # (pivot bar, bull, base01)
    div_scores = []

    for t in range(n):
        for bull in (True, False):
            p2 = ps.pivot(lo if bull else h, t, PIV_LEFT, PIV_RIGHT, bull)
            if p2 is None:
                continue
            x2 = t - PIV_RIGHT
            m2 = mfi[x2]
            is_div = False
            prev = last[bull]
            if prev is not None and prev[1] is not None and m2 is not None:
                p1, m1, x1 = prev
                if bull:
                    area = in_os(m1, t) or in_os(m2, t)
                    d = m2 - m1
                    price_ok = p2 <= p1 * (1.0 + PRICE_EPS)
                    osc_ok = m2 >= m1 - MFI_EPS
                else:
                    area = in_ob(m1, t) or in_ob(m2, t)
                    d = m1 - m2
                    price_ok = p2 >= p1 * (1.0 - PRICE_EPS)
                    osc_ok = m2 <= m1 + MFI_EPS
                swing = atr[t] is not None and abs(p1 - p2) >= atr[t] * MIN_ATR_MOVE
                is_div = price_ok and osc_ok and area and d >= MIN_DELTA and swing
                if is_div:
                    seq[bull] += 1
                    seq[not bull] = 0
                    dt = max(1, x2 - x1)
                    base = (W_DELTA * ps.clamp01(d / (MIN_DELTA * 2.0))
                            + W_ATR * ps.clamp01(abs(p1 - p2) / max(atr[t] * MIN_ATR_MOVE, 1e-10)
                                                 / 2.0)
                            + W_TIME * ps.clamp01(dt / 40.0)
                            + W_SEQ * ps.clamp01(seq[bull] / 3.0))
                    pending.append((x2, bull, base))
            if not is_div:
                seq[bull] = 0
            last[bull] = (p2, m2, x2)

        lag = max(PIV_RIGHT - REACT_BARS, 0)
        while pending and t - pending[0][0] >= REACT_BARS + lag:
            x2, bull, base = pending.pop(0)
            piv_close = c[t - REACT_BARS - lag]
            end = t - lag
            if bull:
                move = max(h[end - REACT_BARS + 1:end + 1]) - piv_close
            else:
                move = piv_close - min(lo[end - REACT_BARS + 1:end + 1])
            react = ps.clamp01(move / max(atr[t] * REACT_ATR, 1e-10))
            score = ps.pine_round(ps.clamp01(base + W_REACT * react) * 100.0)
            if reported[t]:
                events["bull_divergence" if bull else "bear_divergence"].append(t)
                div_scores.append(float(score) if bull else -float(score))

        if not reported[t]:
            continue
        long_ctx = confirmed(is_os, t) or bool(ps.crossover(mfi, lower, t))
        short_ctx = confirmed(is_ob, t) or bool(ps.crossunder(mfi, upper, t))
        os_line = [STOCH_OS] * n
        ob_line = [STOCH_OB] * n
        if long_ctx and ps.crossover(k, os_line, t):
            events["bull_exhaustion"].append(t)
        if short_ctx and ps.crossunder(k, ob_line, t):
            events["bear_exhaustion"].append(t)

    return {"reported": reported, "mfi": mfi, "k": k, "upper": upper, "lower": lower,
            "events": events, "div_scores": div_scores}


HEADER = [
    "kestrel-chartkit golden reference fixture: Flow Exhaustion (MFI + StochRSI)",
    "",
    *provenance("flow_exhaustion"),
    "",
    "Formula: the original definition, MFI mode,",
    "dynamic zones, JMA on MFI and StochRSI K, no HTF filter, manual divergence parameters",
    "(pivots 7/7, min delta 15, buffer 0.10, swing 0.75 ATR, eps 0.001 / 1.0). Conventions the",
    "original leaves open: MFI flow counts from the second bar and is na while its window has no",
    "volume; JMA seeded with its first value (e0 = jma = value) instead of a zero state; RSI and",
    "ATR Wilder averages seeded with an SMA; ATR first true range = high - low; pivots strict on",
    "both sides; rolling highest/lowest need every value of the window. A bar reports when MFI and",
    "K exist; signals and divergence alerts are emitted only on reported bars. A divergence is",
    "emitted when its reaction window has passed (pivot + 10 bars) with its final score.",
    "",
    "Series: pine_series.wave_series(900) (see reference/kestrel_reference/pine_series.py).",
    "",
    "first_reported is the first reporting bar; cp<bar>_* the outputs at that bar; ev_<kind>_n",
    "the number of events of that kind and ev_<kind>_<j> the bar of the j-th (up to 6); score_<j>",
    "the signed score of the j-th divergence event in bar order (up to 8).",
]


def build():
    out = compute(ps.wave_series(N_BARS))
    first = next(t for t, r in enumerate(out["reported"]) if r)
    blocks = [(None, "warm-up", {"first_reported": float(first)})]
    for cp in CHECKPOINTS:
        case = {"mfi": out["mfi"][cp], "k": out["k"][cp]}
        if out["upper"][cp] is not None:
            case["upper"] = out["upper"][cp]
            case["lower"] = out["lower"][cp]
        blocks.append((f"cp{cp}", f"bar {cp}", case))
    blocks.append(("ev", "events", ps.events_block(out["events"], EVENTS)))
    scores = out["div_scores"][:8]
    blocks.append(("score", "divergence scores", {str(j): s for j, s in enumerate(scores)}))
    blocks.append((None, None, {"tolerance": 1e-9}))
    return HEADER, blocks
