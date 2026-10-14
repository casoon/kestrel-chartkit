"""contract_composites_b: complete per-bar contracts of the composites b family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code. Where a golden module already holds an independent
implementation of an indicator, it is imported and its per-bar state mapped onto the contract
fields here.
"""

import math
from contextlib import contextmanager

from .. import exact
from ..fixture import provenance
from ..indicators import adx, efficiency_ratio, keltner, rsi_lines
from ..smoothing import ema_first_sample
from . import leg_conviction as golden_leg_conviction
from . import sweep_reversal_score as golden_sweep
from . import trend_break_risk as golden_break_risk
from . import equal_level_liquidity as golden_equal_levels
from . import accumulation_phase as golden_accumulation
from . import leg_character as golden_leg_character
from . import reversal_setup_score as golden_reversal_setup
from . import structure_zones as golden_structure_zones
from . import structural_flow as golden_structural_flow
from .contract_pilot import case_block

NAME = "contract_composites_b"


def clamp(x, low, high):
    return min(max(x, low), high)


@contextmanager
def constants(module, **values):
    """Runs a golden module's `compute` with other parameters: the golden modules read their
    parameters from module constants at call time, so they are set for the duration and restored
    afterwards. The implementation itself is reused unchanged."""
    saved = {key: getattr(module, key) for key in values}
    for key, value in values.items():
        assert key in saved
        setattr(module, key, value)
    try:
        yield module
    finally:
        for key, value in saved.items():
            setattr(module, key, value)


def block(name, comment, params, bars, expected):
    """`case_block` with an optional third element of an output, its `secondary`:
    (value, extra) or (value, extra, secondary)."""
    plain = [((out[0], out[1]), alerts) if out is not None else (None, alerts)
             for out, alerts in expected]
    name, comment, case = case_block(name, comment, params, bars, plain)
    for i, (out, _) in enumerate(expected, start=1):
        if out is not None and len(out) == 3:
            case[f"out{i}_secondary"] = out[2]
    return name, comment, case


# --- Series ----------------------------------------------------------------------------------

def swing_series(n, start=100.0, drift=0.15, amp=1.0, period=9.0, volume=True, phase=0.0):
    """Deterministic OHLCV: a drifting wave with a second, slower wave on top, open = previous
    close plus a small offset, asymmetric wicks and a volume that varies with the bar."""
    bars, prev = [], start
    for i in range(n):
        x = float(i) + phase
        close = prev + drift + amp * (math.sin(x / period * 2.0 * math.pi) * 0.9
                                      + 0.4 * math.sin(x / 23.0)) * 0.5 + 0.2 * math.cos(x / 1.7)
        open_ = prev + 0.12 * math.sin(x / 2.3)
        high = max(open_, close) + 0.25 + 0.2 * abs(math.sin(x / 3.1))
        low = min(open_, close) - 0.2 - 0.3 * abs(math.cos(x / 5.3))
        vol = (1000.0 + 500.0 * math.sin(x / 4.0) ** 2 + 250.0 * math.cos(x / 11.0)) if volume else 0.0
        bars.append((open_, high, low, close, vol))
        prev = close
    return bars


# --- Buy/sell pressure -----------------------------------------------------------------------

def bar_pressure(bar):
    """Per the type: range = max(high - low, 1e-8); location = 2 (close - low) / range - 1, 0 for a
    bar without range; upper wick = high - max(open, close), lower wick = min(open, close) - low,
    a body outside the range clipped to it; raw = (0.6 location + 0.4 wick balance) 100."""
    o, high, low, close, _ = bar
    span = max(high - low, 1e-8)
    location = 2.0 * (close - low) / span - 1.0 if high > low else 0.0
    upper = high - min(high, max(o, close))
    lower = max(low, min(o, close)) - low
    wick_balance = (lower - upper) / span
    return location, wick_balance, (0.6 * location + 0.4 * wick_balance) * 100.0


def pressure_series(bars, period):
    """value = clamp(first-sample EMA(period) of raw, -100, 100), from the first bar on."""
    parts = [bar_pressure(b) for b in bars]
    smoothed = ema_first_sample([raw for _, _, raw in parts], period)
    return [(clamp(s, -100.0, 100.0), loc, wick) for s, (loc, wick, _) in zip(smoothed, parts)]


def buy_sell_pressure_case(period, bars):
    expected = [((v, {"location": loc, "wick_balance": wick}), {})
                for v, loc, wick in pressure_series(bars, period)]
    return {"period": float(period)}, bars, expected


def pressure_edge_bars():
    """A doji without range (location 0, wick balance 0), a bar closing at its high with the open
    at its low, a hammer, a shooting star, a body poking out of its range (clipped) and a bar
    closing exactly in the middle."""
    return [
        (100.0, 100.0, 100.0, 100.0, 1000.0),
        (100.0, 102.0, 100.0, 102.0, 1000.0),
        (101.0, 101.2, 97.0, 101.1, 1000.0),
        (101.0, 104.0, 100.9, 101.1, 1000.0),
        (103.0, 102.0, 100.0, 99.0, 1000.0),
        (100.0, 102.0, 98.0, 100.0, 0.0),
        (100.0, 101.0, 99.0, 100.5, 1000.0),
    ]


# --- Trend quality ---------------------------------------------------------------------------

def trend_quality_series(bars, period):
    """Per the type, all four inputs over `period`:
    - direction: +1 if the first-sample EMA of the close rose against the previous bar's, else -1;
      a flat EMA counts as -1, and so does the first output (documented: no previous value);
    - efficiency: Kaufman ER over the last period + 1 closes;
    - strength: clamp(ADX / 50, 0, 1), Wilder DI and ADX smoothing both over `period`;
    - participation: clamp(RVOL / 2, 0.2, 1), RVOL = volume / mean of the last `period` volumes
      including the current one, 1 for a zero mean.
    value = clamp(product * 100, -100, 100). First output with bar 2 period (the ADX).
    Returns per bar None or (value, direction, efficiency, strength, participation)."""
    closes = [b[3] for b in bars]
    ema = ema_first_sample(closes, period)
    adx_line = adx([b[:4] for b in bars], period, period)
    first = 2 * period - 1  # 0-based bar of the first ADX
    out = [None] * len(bars)
    for t in range(first, len(bars)):
        direction = 1.0 if t > first and ema[t] > ema[t - 1] else -1.0
        efficiency = efficiency_ratio(closes[:t + 1], period)
        strength = clamp(adx_line[t - first] / 50.0, 0.0, 1.0)
        volumes = [b[4] for b in bars[t + 1 - period:t + 1]]
        mean = sum(volumes) / period
        rvol = bars[t][4] / mean if mean > 0.0 else 1.0
        participation = clamp(rvol / 2.0, 0.2, 1.0)
        value = clamp(direction * efficiency * strength * participation * 100.0, -100.0, 100.0)
        out[t] = (value, direction, efficiency, strength, participation)
    return out


def trend_quality_case(period, bars):
    expected = []
    for row in trend_quality_series(bars, period):
        if row is None:
            expected.append((None, {}))
            continue
        value, direction, efficiency, strength, participation = row
        expected.append(((value, {"direction": direction, "efficiency": efficiency,
                                  "strength": strength, "participation": participation}), {}))
    return {"period": float(period)}, bars, expected


# --- Multi-factor ----------------------------------------------------------------------------

def regime_state(bars, period):
    """Volatility regime (period, 2, 1.5) on the last bar of `bars`: Bollinger over `period`
    closes, population deviation, against a Keltner channel (first-sample EMA(period) basis, plain
    mean of the last 10 true ranges); -1 squeeze when both bands lie inside the channel, else 1
    when the Bollinger width exceeds 1.3 channel widths (at least 1e-8), else 0."""
    bands = exact.bollinger([b[3] for b in bars[-period:]], 2.0)
    bb_upper, bb_lower = float(bands["upper"]), float(bands["lower"])
    _, kc_upper, kc_lower = keltner([b[:4] for b in bars], period, 10, 1.5)
    if bb_upper <= kc_upper and bb_lower >= kc_lower:
        return -1.0
    if bb_upper - bb_lower > max(kc_upper - kc_lower, 1e-8) * 1.3:
        return 1.0
    return 0.0


def multi_factor_case(period, bars):
    """Per the type: trend_factor = trend quality / 100, rsi_factor = (RSI line - 50) / 50 with the
    RSI line = EMA(3) over Wilder's RSI(period), pressure_factor = buy/sell pressure / 100,
    volatility_factor = regime state (period, 2, 1.5); raw = 0.35 trend + 0.25 rsi + 0.40
    pressure, halved in a squeeze, clamped to -1..1. First output once all four have one."""
    trend = trend_quality_series(bars, period)
    line = rsi_lines([b[3] for b in bars], period, 3, 3)[0]
    pressure = pressure_series(bars, period)
    first_regime = max(period, 10) - 1
    expected = []
    for t in range(len(bars)):
        if trend[t] is None or t < period or t < first_regime:
            expected.append((None, {}))
            continue
        trend_factor = trend[t][0] / 100.0
        rsi_factor = (line[t - period] - 50.0) / 50.0
        pressure_factor = pressure[t][0] / 100.0
        state = regime_state(bars[:t + 1], period)
        raw = 0.35 * trend_factor + 0.25 * rsi_factor + 0.40 * pressure_factor
        value = clamp(raw * 0.5 if state < 0.0 else raw, -1.0, 1.0)
        expected.append(((value, {"trend_factor": trend_factor, "rsi_factor": rsi_factor,
                                  "pressure_factor": pressure_factor,
                                  "volatility_factor": state}), {}))
    return {"period": float(period)}, bars, expected


# --- Leg conviction ------------------------------------------------------------------------

def leg_conviction_case(bars, pivot_left=5, pivot_right=5, max_leg=100, atr_len=14,
                        speed_ref=1.0, w_speed=0.4, w_clean=0.4, w_volume=0.2):
    """Legs, step lines and divergences from the golden module (`leg_conviction.compute`, written
    from the original definition), mapped per bar onto the documented output:
    - first output on the bar after the first full pivot window (0-based bar L + R + 1);
    - until a scored up-leg and a scored down-leg both exist: value 0, secondary 0, no extra;
    - then value = last up-leg conviction, secondary = minus the last down-leg conviction, extra
      bull_last, bear_last, bull_prev / bear_prev once they exist, and pivot_price (the pivot of
      the divergence; the low when both sides fire on one bar, as the low is processed second)
      on a bar with an alert.
    Alert strength: not documented on the type. Assumed: the drop in conviction against the
    previous leg of the same side, (previous - current) / 100, clamped to 0.05..1."""
    with constants(golden_leg_conviction, PIV_L=pivot_left, PIV_R=pivot_right, MAX_LEG=max_leg,
                   ATR_LEN=atr_len, SPEED_REF=speed_ref, W_SPEED=w_speed, W_CLEAN=w_clean,
                   W_VOL=w_volume) as golden:
        out = golden.compute(bars)
    high_conv = {t: conv for t, d, conv in out["legs"] if d > 0}
    low_conv = {t: conv for t, d, conv in out["legs"] if d < 0}

    def drop(convs, t):
        earlier = [convs[k] for k in sorted(convs) if k < t]
        return clamp((earlier[-1] - convs[t]) / 100.0, 0.05, 1.0)

    expected = []
    for t in range(len(bars)):
        if t < pivot_left + pivot_right + 1:
            expected.append((None, {}))
            continue
        alerts, extra = {}, {}
        if t in out["bear_div"]:
            alerts["bear_conviction_divergence"] = drop(high_conv, t)
            extra["pivot_price"] = bars[t - pivot_right][1]
        if t in out["bull_div"]:
            alerts["bull_conviction_divergence"] = drop(low_conv, t)
            extra["pivot_price"] = bars[t - pivot_right][2]
        high_step, low_step = out["high_step"][t], out["low_step"][t]
        if high_step is None or low_step is None:
            assert not alerts
            expected.append(((0.0, {}, 0.0), {}))
            continue
        extra["bull_last"], extra["bear_last"] = high_step, -low_step
        if out["bull_prev"][t] is not None:
            extra["bull_prev"] = out["bull_prev"][t]
        if out["bear_prev"][t] is not None:
            extra["bear_prev"] = out["bear_prev"][t]
        expected.append(((high_step, extra, low_step), alerts))
    params = {"pivot_left": float(pivot_left), "pivot_right": float(pivot_right),
              "max_leg": float(max_leg), "atr_len": float(atr_len), "speed_ref": speed_ref,
              "w_speed": w_speed, "w_clean": w_clean, "w_volume": w_volume}
    return params, bars, expected


# --- Sweep reversal score --------------------------------------------------------------------

def sweep_case(bars, swing_len=10, atr_len=14, atr_base_len=50, atr_low_factor=0.6, rsi_len=14,
               pivot_len=5, min_quality_long=2, min_quality_short=2, cooldown=5):
    """Signals and readings from the golden module (`sweep_reversal_score.compute`), mapped per
    bar onto the documented output: output from the first bar with ATR base, RSI and sweep window;
    value 1 on a long, -1 on a short, 0 otherwise (1 when both); extra long_quality,
    short_quality, rsi, atr, atr_base, swing_low, swing_high, and on a signal bar entry/stop/take
    (long_*/short_* when both fire). Alert strength: not documented on the type. Assumed: the
    signal's quality out of its maximum 2, quality / 2."""
    with constants(golden_sweep, SWING_LEN=swing_len, ATR_LEN=atr_len, ATR_BASE_LEN=atr_base_len,
                   ATR_LOW_FACTOR=atr_low_factor, RSI_LEN=rsi_len, PIV_LEN=pivot_len,
                   MIN_Q_LONG=min_quality_long, MIN_Q_SHORT=min_quality_short,
                   COOLDOWN=cooldown) as golden:
        out = golden.compute([b[:4] for b in bars])
    longs = {row[0]: row[1:] for row in out["longs"]}
    shorts = {row[0]: row[1:] for row in out["shorts"]}
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        extra = {"long_quality": out["long_q"][t], "short_quality": out["short_q"][t],
                 "rsi": out["rsi"][t], "atr": out["atr"][t], "atr_base": out["atr_base"][t],
                 "swing_low": out["swing_low"][t], "swing_high": out["swing_high"][t]}
        alerts = {}
        both = t in longs and t in shorts
        for rows, prefix, kind in ((longs, "long_", "bull_sweep_reversal"),
                                   (shorts, "short_", "bear_sweep_reversal")):
            if t in rows:
                quality, entry, stop, take = rows[t]
                pre = prefix if both else ""
                extra[pre + "entry"], extra[pre + "stop"], extra[pre + "take"] = entry, stop, take
                alerts[kind] = quality / 2.0
        value = 1.0 if t in longs else (-1.0 if t in shorts else 0.0)
        expected.append(((value, extra), alerts))
    params = {"swing_len": float(swing_len), "atr_len": float(atr_len),
              "atr_base_len": float(atr_base_len), "atr_low_factor": atr_low_factor,
              "rsi_len": float(rsi_len), "pivot_len": float(pivot_len),
              "min_quality_long": float(min_quality_long),
              "min_quality_short": float(min_quality_short), "cooldown": float(cooldown)}
    return params, bars, expected


# --- Trend break risk ------------------------------------------------------------------------

BREAK_RISK_STRENGTH = {"break_pressure": None, "break_critical": None, "structure_break": 1.0,
                       "divergence": 0.6}


def trend_break_risk_case(bars, bias_len=50, pivot_len=5, near_atr=1.5, rsi_len=14, hold_bars=10,
                          smooth_len=3):
    """Per-bar state and events from the golden module (`trend_break_risk.compute`), mapped onto
    the documented output: output from the first bar with ATR and RSI; value = smoothed risk;
    extra risk_raw, trend_dir, the four held sensors, rsi, ema, and break_level / distance_atr
    while a break level exists. Alert strengths are not documented on the type. Assumed: the
    smoothed risk / 100 for the pressure and critical crosses, 1 for a confirmed structure break,
    0.6 for a divergence (a fixed weight: the sensor is binary)."""
    with constants(golden_break_risk, BIAS_LEN=bias_len, PIV_LEN=pivot_len, NEAR_ATR=near_atr,
                   RSI_LEN=rsi_len, HOLD_BARS=hold_bars, SMOOTH_LEN=smooth_len) as golden:
        out = golden.compute([b[:4] for b in bars])
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, []).append(kind)
    expected = []
    for t in range(len(bars)):
        if out["risk"][t] is None:
            expected.append((None, {}))
            continue
        extra = {"risk_raw": out["risk_raw"][t], "trend_dir": out["trend"][t],
                 "near": out["near"][t], "sfp": out["sfp"][t], "structure": out["structure"][t],
                 "divergence": out["divergence"][t], "rsi": out["rsi"][t], "ema": out["ema"][t]}
        if out["break_level"][t] is not None:
            extra["break_level"] = out["break_level"][t]
        if out["distance"][t] is not None:
            extra["distance_atr"] = out["distance"][t]
        alerts = {}
        for kind in fired.get(t, []):
            strength = BREAK_RISK_STRENGTH[kind.split("_", 1)[1]]
            alerts[kind] = out["risk"][t] / 100.0 if strength is None else strength
        expected.append(((out["risk"][t], extra), alerts))
    params = {"bias_len": float(bias_len), "pivot_len": float(pivot_len), "near_atr": near_atr,
              "rsi_len": float(rsi_len), "hold_bars": float(hold_bars),
              "smooth_len": float(smooth_len)}
    return params, bars, expected


# --- Equal-level liquidity -------------------------------------------------------------------

EQUAL_LEVEL_STRENGTH = {"bear_bsl_sweep": 0.6, "bull_bsl_breakout": 0.6, "bear_bsl_stop_hunt": 0.6,
                        "bull_ssl_sweep": 0.6, "bear_ssl_breakout": 0.6, "bull_ssl_stop_hunt": 0.6,
                        "primary_bsl_near": 0.4, "primary_ssl_near": 0.4}


def equal_level_liquidity_case(bars, pivot_len=8, eq_tolerance_atr=0.12, min_touches=2,
                               max_age=400, max_distance_atr=8.0, reclaim_bars=3,
                               min_volume_ratio=1.2, alert_distance_atr=1.0):
    """Levels, events and reclaim scores from the golden module (`equal_level_liquidity.compute`),
    mapped per bar onto the documented output: output from the first bar with an ATR; value = the
    bias; extra primary_/secondary_bsl/ssl with _score (primary also _touches) while they exist,
    bsl_count, ssl_count, recent_bull_score, recent_bear_score, volume_ratio, and
    bear_/bull_reclaim_score on a reclaim bar. The golden module lists reclaim scores per bar in
    the order bear, bull. Alert strengths are not documented on the type. Assumed: the reclaim
    score / 100 for a reclaim and a premium reclaim, a fixed 0.6 for sweeps, breakouts and stop
    hunts, 0.4 for the proximity warnings."""
    with constants(golden_equal_levels, PIVOT_LEN=pivot_len, EQ_TOL_ATR=eq_tolerance_atr,
                   MIN_TOUCHES=min_touches, MAX_AGE=max_age, MAX_DIST_ATR=max_distance_atr,
                   RECLAIM_BARS=reclaim_bars, MIN_VOLUME_RATIO=min_volume_ratio,
                   ALERT_DIST_ATR=alert_distance_atr) as golden:
        out = golden.compute(bars)
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    scores = {}
    for t, score in out["scores"]:
        scores.setdefault(t, []).append(score)
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        extra = {}
        names = ("primary_bsl", "secondary_bsl", "primary_ssl", "secondary_ssl")
        for name, level in zip(names, out["levels"][t]):
            if level is not None:
                extra[name], extra[name + "_score"] = level[0], level[1]
                if name.startswith("primary"):
                    extra[name + "_touches"] = float(level[2])
        extra["bsl_count"], extra["ssl_count"] = out["counts"][t]
        extra["recent_bull_score"], extra["recent_bear_score"] = out["recent"][t]
        extra["volume_ratio"] = out["volume_ratio"][t]
        kinds = fired.get(t, set())
        reclaims = list(scores.get(t, []))
        alerts = {k: EQUAL_LEVEL_STRENGTH[k] for k in kinds if k in EQUAL_LEVEL_STRENGTH}
        for side in ("bear", "bull"):
            if f"{side}_reclaim" in kinds:
                score = reclaims.pop(0)
                extra[f"{side}_reclaim_score"] = score
                alerts[f"{side}_reclaim"] = score / 100.0
                if f"{side}_premium_reclaim" in kinds:
                    alerts[f"{side}_premium_reclaim"] = score / 100.0
        assert not reclaims
        expected.append(((out["bias"][t], extra), alerts))
    params = {"pivot_len": float(pivot_len), "eq_tolerance_atr": eq_tolerance_atr,
              "min_touches": float(min_touches), "max_age": float(max_age),
              "max_distance_atr": max_distance_atr, "reclaim_bars": float(reclaim_bars),
              "min_volume_ratio": min_volume_ratio, "alert_distance_atr": alert_distance_atr}
    return params, bars, expected


# --- Accumulation phase ----------------------------------------------------------------------

ACCUMULATION_FIELDS = (
    "accum_trend_loss", "accum_range", "accum_compression", "accum_liquidity", "accum_structure",
    "dist_trend_loss", "dist_range", "dist_compression", "dist_liquidity", "dist_structure",
    "accum_bars", "dist_bars", "accum_state", "dist_state", "efficiency", "bbw_ratio",
    "atr_ratio", "range_atr")


def accumulation_phase_case(bars, atr_len=14, lookback=20, min_phase_bars=10, pivot_left=3,
                            pivot_right=3, er_range=0.3, bbw_compress=0.7, atr_compress=0.8):
    """Scores, components, phases and state entries from the golden module
    (`accumulation_phase.compute`), mapped per bar onto the documented output: value = the
    accumulation score, secondary = the distribution score, the eighteen documented extra fields.
    Alert strength is not documented on the type. Assumed: rising with the state entered,
    0.4 developing, 0.6 mature, 0.8 breakout/breakdown ready."""
    with constants(golden_accumulation, ATR_LEN=atr_len, LOOKBACK=lookback,
                   MIN_PHASE=min_phase_bars, PIV_L=pivot_left, PIV_R=pivot_right,
                   ER_RANGE=er_range, BBW_COMP=bbw_compress, ATR_COMP=atr_compress) as golden:
        out = golden.compute([b[:4] for b in bars])
        kinds = golden.EVENTS
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        row = out["rows"][t]
        extra = {key: row[key] for key in ACCUMULATION_FIELDS}
        alerts = {k: 0.4 + 0.2 * (kinds.index(k) % 3) for k in fired.get(t, set())}
        expected.append(((row["accum"], extra, row["dist"]), alerts))
    params = {"atr_len": float(atr_len), "lookback": float(lookback),
              "min_phase_bars": float(min_phase_bars), "pivot_left": float(pivot_left),
              "pivot_right": float(pivot_right), "er_range": er_range,
              "bbw_compress": bbw_compress, "atr_compress": atr_compress}
    return params, bars, expected


# --- Leg character ---------------------------------------------------------------------------

def leg_character_case(bars, pivot_left=5, pivot_right=5, max_leg_bars=350, min_leg_atr=0.5,
                       mature_bars=8, ma_len=34, impulse_threshold=62.0,
                       correction_threshold=62.0, change_buffer=5.0):
    """Live leg, class and class changes from the golden module (`leg_character.compute`, the
    original's pivot engine and leg calculation), mapped per bar onto the documented output:
    output from the first pivot on; value = impulse; extra correction, confidence, maturity, bars,
    direction, class, leg_start, the ten features and the four sub-swing diagnostics.
    The first output is path dependent while `warmup_period()` states the earliest possible bar
    (L + R + 1); the case series are cut so that the first pivot is confirmed on that bar
    (asserted below). Alert strength is not documented on the type. Assumed: the impulse score
    / 100 for bull_/bear_impulse, the correction score / 100 for correction_started, a fixed 0.3
    for character_lost."""
    with constants(golden_leg_character, PIV_L=pivot_left, PIV_R=pivot_right,
                   MIN_LEG_ATR=min_leg_atr, MATURE=mature_bars, MA_LEN=ma_len,
                   IMP_T=impulse_threshold, COR_T=correction_threshold,
                   BUFFER=change_buffer) as golden:
        out = golden.compute([b[:4] for b in bars], max_leg=max_leg_bars)
        features = golden.FEATURES
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    expected = []
    for t in range(len(bars)):
        leg = out["live"][t]
        if leg is None:
            expected.append((None, {}))
            continue
        extra = {"correction": leg["correction"], "confidence": leg["confidence"],
                 "maturity": leg["maturity"], "bars": leg["bars"],
                 "direction": out["direction"][t], "class": out["class"][t],
                 "leg_start": out["start"][t]}
        extra.update(zip(features, leg["features"]))
        for key in ("internal_swings", "avg_internal_swing_atr", "progression_ratio",
                    "sub_leg_growth"):
            extra[key] = leg[key]
        alerts = {}
        for kind in fired.get(t, set()):
            if kind == "character_lost":
                alerts[kind] = 0.3
            elif kind == "correction_started":
                alerts[kind] = leg["correction"] / 100.0
            else:
                alerts[kind] = leg["impulse"] / 100.0
        expected.append(((leg["impulse"], extra), alerts))
    assert next(i for i, e in enumerate(expected) if e[0] is not None) == pivot_left + pivot_right
    params = {"pivot_left": float(pivot_left), "pivot_right": float(pivot_right),
              "max_leg_bars": float(max_leg_bars), "min_leg_atr": min_leg_atr,
              "mature_bars": float(mature_bars), "ma_len": float(ma_len),
              "impulse_threshold": impulse_threshold,
              "correction_threshold": correction_threshold, "change_buffer": change_buffer}
    return params, bars, expected


# --- Reversal setup score --------------------------------------------------------------------

# Alert strengths are not documented on the type. Assumed: a fixed weight per kind, the side's
# quality / 100 where the alert is about a ranked opportunity (action, candidate).
REVERSAL_SETUP_STRENGTH = {
    "bull_review": 0.5, "bear_review": 0.5, "bull_action": "bull_quality",
    "bear_action": "bear_quality", "long_signal_invalidated": 0.6,
    "short_signal_invalidated": 0.6, "bull_candidate": "bull_quality",
    "bear_candidate": "bear_quality", "conflict": 0.4, "bull_spring": 0.5, "bear_upthrust": 0.5,
    "bull_major_bos": 0.6, "bear_major_bos": 0.6, "bear_failed_bull_reversal": 0.5,
    "bull_failed_bear_reversal": 0.5, "bull_choch": 0.5, "bear_choch": 0.5,
}


def reversal_setup_score_case(bars):
    """Scores, statuses and events from the golden module (`reversal_setup_score.compute`, the
    original with its defaults and momentum mix A), mapped per bar onto the documented output:
    value = bull score, secondary = bear score, extra = the sixteen documented fields."""
    out = golden_reversal_setup.compute(bars)
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        row = out["rows"][t]
        extra = {k: v for k, v in row.items() if k not in ("bull_score", "bear_score")}
        alerts = {}
        for kind in fired.get(t, set()):
            strength = REVERSAL_SETUP_STRENGTH[kind]
            alerts[kind] = row[strength] / 100.0 if isinstance(strength, str) else strength
        expected.append(((row["bull_score"], extra, row["bear_score"]), alerts))
    return {}, bars, expected


# --- Structure zones -------------------------------------------------------------------------

def late_volume(bars, quiet=20):
    """The bars with volume 0 on the first `quiet` bars. With real volume from the first bar the
    first output waits for the MFI and the 20-bar volume average (bar 20) while
    `warmup_period()` states the earliest possible bar (15, without volume); volume that starts
    after the warm-up keeps the first output on bar 15 and still runs the volume-dependent parts
    (MFI, absorption, the volume profile) from bar 21 on."""
    return [b[:4] + (0.0 if i < quiet else b[4],) for i, b in enumerate(bars)]


def structure_zones_case(bars):
    """Zones, assessment and events from the golden module (`structure_zones.compute`, the
    original with its defaults), mapped per bar onto the documented output: value = number of
    tracked zones, extra = the support_/resistance_ fields of the assessed zones, micro_trend,
    major_trend, clx_state, clx_direction, event_code, and poc/va_high/va_low once a profile
    exists. Alert strength is not documented on the type. Assumed: a fixed 0.5 for every kind."""
    out = golden_structure_zones.compute(bars)
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        row = out["rows"][t]
        extra = {k: v for k, v in row.items() if k != "zones"}
        expected.append(((row["zones"], extra), {k: 0.5 for k in fired.get(t, set())}))
    return {}, bars, expected


# --- Structural flow -------------------------------------------------------------------------

def structural_flow_case(bars):
    """Flow, wave, Re-Sync, phases and events from the golden module (`structural_flow.compute`,
    the original's Balanced preset with relative-volume participation), mapped per bar onto the
    documented output: value = structural drive, secondary = aligned wave, extra = the twenty
    documented fields. Alert strength is not documented on the type. Assumed: the flow strength
    of the bar, clamped to 0.05..1, for every kind."""
    out = golden_structural_flow.compute(bars)
    fired = {}
    for kind, at in out["events"].items():
        for t in at:
            fired.setdefault(t, set()).add(kind)
    expected = []
    for t in range(len(bars)):
        row = out["rows"][t]
        if row is None:
            expected.append((None, {}))
            continue
        extra = {k: v for k, v in row.items() if k not in ("drive", "aligned")}
        strength = clamp(row["strength"], 0.05, 1.0)
        expected.append(((row["drive"], extra, row["aligned"]),
                         {k: strength for k in fired.get(t, set())}))
    return {}, bars, expected


def with_volume(bars, volume=1000.0):
    return [b[:4] + (volume,) for b in bars]


# --- Build -----------------------------------------------------------------------------------

def build():
    header = provenance("contract_composites_b") + [
        "",
        "Format: see contract_pilot; out<i>_secondary where the output carries one.",
        "Buy/sell pressure, trend quality and multi-factor follow the formulas documented on their",
        "types. The other indicators reuse the golden modules (written from the original",
        "definitions) and map their per-bar state onto the documented output; alert strengths",
        "their types do not document are assumed conventions, stated per indicator in the module.",
    ]
    swings = swing_series(70)
    blocks = [
        block("buy_sell_pressure_defaults", "Buy/sell pressure, period 14, drifting wave",
                   *buy_sell_pressure_case(14, swings[:40])),
        block("buy_sell_pressure_period3", "Buy/sell pressure, period 3",
                   *buy_sell_pressure_case(3, swings[:25])),
        block("buy_sell_pressure_edges",
                   "Buy/sell pressure, period 2: doji without range, clipped body, middle close",
                   *buy_sell_pressure_case(2, pressure_edge_bars())),
        block("trend_quality_defaults", "Trend quality, period 14, drifting wave",
                   *trend_quality_case(14, swings[:50])),
        block("trend_quality_period5", "Trend quality, period 5, falling wave",
                   *trend_quality_case(5, swing_series(30, drift=-0.2, period=7.0))),
        block("trend_quality_novolume", "Trend quality, period 4, zero volume (RVOL 1)",
                   *trend_quality_case(4, swing_series(20, volume=False))),
        block("trend_quality_flat",
              "Trend quality, period 4, constant bars: the EMA never moves (documented "
              "direction -1), ER 0, ADX 0",
              *trend_quality_case(4, [(100.0, 100.5, 99.5, 100.0, 1000.0)] * 12)),
        block("multi_factor_defaults", "Multi-factor, period 14, drifting wave",
                   *multi_factor_case(14, swings[:55])),
        block("multi_factor_period4", "Multi-factor, period 4 (first output at bar 10)",
                   *multi_factor_case(4, swing_series(40, drift=-0.1, period=6.0))),
        block("leg_conviction_defaults",
                   "Leg conviction, catalog defaults: golden series (with volume), first 175 bars",
                   *leg_conviction_case(golden_leg_conviction._series(True)[:175])),
        block("leg_conviction_short_pivots",
                   "Leg conviction, pivots 3/2, max leg 6, ATR 5, speed ref 0.5, weights .5/.3/.2",
                   *leg_conviction_case(swing_series(90, amp=2.0, period=13.0), pivot_left=3,
                                        pivot_right=2, max_leg=6, atr_len=5, speed_ref=0.5,
                                        w_speed=0.5, w_clean=0.3, w_volume=0.2)),
        block("leg_conviction_novolume",
                   "Leg conviction, pivots 4/3, no volume: the volume weight drops out",
                   *leg_conviction_case(golden_leg_conviction._series(False)[:120], pivot_left=4,
                                        pivot_right=3, atr_len=7)),
        block("sweep_reversal_score_defaults",
                   "Sweep reversal, catalog defaults: golden spike series, first 190 bars",
                   *sweep_case(with_volume(golden_sweep._series()[:190]))),
        block("sweep_reversal_score_short_windows",
                   "Sweep reversal, swing 6, ATR 5 over base 12 at 0.5, RSI 7, pivots 3, "
                   "quality 1/1, cooldown 3",
                   *sweep_case(with_volume(golden_sweep._series()[300:420]), swing_len=6,
                               atr_len=5, atr_base_len=12, atr_low_factor=0.5, rsi_len=7,
                               pivot_len=3, min_quality_long=1, min_quality_short=1, cooldown=3)),
        block("trend_break_risk_defaults",
              "Trend break risk, catalog defaults: golden series bars 150-419 (uptrend, range)",
              *trend_break_risk_case(with_volume(golden_break_risk._series()[150:420]))),
        block("trend_break_risk_fast",
              "Trend break risk, EMA 20, pivots 3, near 2 ATR, RSI 7, hold 4, smoothing 2",
              *trend_break_risk_case(with_volume(golden_break_risk._series()[150:300]),
                                     bias_len=20, pivot_len=3, near_atr=2.0, rsi_len=7,
                                     hold_bars=4, smooth_len=2)),
        block("trend_break_risk_crash",
              "Trend break risk, EMA 20, pivots 3: golden series bars 330-469 (crash, downtrend)",
              *trend_break_risk_case(with_volume(golden_break_risk._series()[330:470]),
                                     bias_len=20, pivot_len=3, near_atr=2.0, rsi_len=7,
                                     hold_bars=4, smooth_len=2)),
        block("equal_level_liquidity_defaults",
              "Equal-level liquidity, catalog defaults: golden range series (with volume), "
              "bars 0-109: stop hunts, premium reclaims, proximity warnings",
              *equal_level_liquidity_case(golden_equal_levels._series(True)[:110])),
        block("equal_level_liquidity_novolume",
              "Equal-level liquidity, catalog defaults, no volume: bars 290-399, a sell-side "
              "breakout reclaimed (score scaled by 147/123)",
              *equal_level_liquidity_case(golden_equal_levels._series(False)[290:400])),
        block("equal_level_liquidity_tuned",
              "Equal-level liquidity, pivots 4, tolerance 0.2, age 80, distance 6, reclaim 2, "
              "volume ratio 1.1, alert distance 1.5: bars 0-299 with a buy-side breakout",
              *equal_level_liquidity_case(golden_equal_levels._series(True)[:300], pivot_len=4,
                                          eq_tolerance_atr=0.2, max_age=80,
                                          max_distance_atr=6.0, reclaim_bars=2,
                                          min_volume_ratio=1.1, alert_distance_atr=1.5)),
        block("accumulation_phase_base",
              "Accumulation phase, catalog defaults: golden series bars 180-319, a base with "
              "dips under its lows (accumulation developing, then mature)",
              *accumulation_phase_case(with_volume(golden_accumulation._series()[180:320]))),
        block("accumulation_phase_top",
              "Accumulation phase, catalog defaults: golden series bars 400-574, rally into a "
              "top with pokes above its highs (distribution developing and mature)",
              *accumulation_phase_case(with_volume(golden_accumulation._series()[400:575]))),
        block("accumulation_phase_fast",
              "Accumulation phase, ATR 10, lookback 15, phase 6, pivots 2/2, ER 0.35, "
              "compression 0.8/0.9: golden series bars 180-339",
              *accumulation_phase_case(with_volume(golden_accumulation._series()[180:340]),
                                       atr_len=10, lookback=15, min_phase_bars=6, pivot_left=2,
                                       pivot_right=2, er_range=0.35, bbw_compress=0.8,
                                       atr_compress=0.9)),
        block("leg_character_defaults",
              "Leg character, catalog defaults: golden push/chop series bars 32-151 (first pivot "
              "on bar 6, confirmed on bar 11): all four class changes",
              *leg_character_case(with_volume(golden_leg_character._series()[32:152]))),
        block("leg_character_short_legs",
              "Leg character, pivots 3/2, at most 20 bars per leg, minimum leg 1 ATR, maturity "
              "5, EMA 10, thresholds 58/66, buffer 3: golden series bars 35-134",
              *leg_character_case(with_volume(golden_leg_character._series()[35:135]),
                                  pivot_left=3, pivot_right=2, max_leg_bars=20, min_leg_atr=1.0,
                                  mature_bars=5, ma_len=10, impulse_threshold=58.0,
                                  correction_threshold=66.0, change_buffer=3.0)),
        block("reversal_setup_score_volume",
              "Reversal setup score (no parameters): golden series with volume, bars 0-219: "
              "reviews, springs, upthrusts, major BOS, CHoCH, traps, conflict",
              *reversal_setup_score_case(golden_reversal_setup._series(True)[:220])),
        block("reversal_setup_score_novolume",
              "Reversal setup score: golden series without volume (RSI instead of MFI), bars "
              "0-159",
              *reversal_setup_score_case(golden_reversal_setup._series(False)[:160])),
        block("reversal_setup_score_candidates",
              "Reversal setup score: golden series with volume, bars 320-519: bull and bear "
              "candidate, short signal invalidated",
              *reversal_setup_score_case(golden_reversal_setup._series(True)[320:520])),
        block("reversal_setup_score_long_invalidated",
              "Reversal setup score: golden series with volume, bars 620-729: bull candidate, "
              "long signal invalidated",
              *reversal_setup_score_case(golden_reversal_setup._series(True)[620:730])),
        block("structure_zones_volume",
              "Structure zones (no parameters): golden series bars 0-199, volume from bar 21: "
              "relevance, resistance hold, support and resistance breaks, profile",
              *structure_zones_case(late_volume(golden_structure_zones._series(True)[:200]))),
        block("structure_zones_holds",
              "Structure zones: golden series bars 200-399, volume from bar 21: support hold and "
              "resistance hold",
              *structure_zones_case(late_volume(golden_structure_zones._series(True)[200:400]))),
        block("structure_zones_novolume",
              "Structure zones: golden series without volume, bars 0-149 (no MFI, no profile)",
              *structure_zones_case(golden_structure_zones._series(False)[:150])),
        block("structural_flow_volume",
              "Structural flow (no parameters): golden series 'v' bars 650-949: initiation, "
              "expansions, correction, early and confirmed Re-Sync, failed and dominant "
              "expansion, weakening waves",
              *structural_flow_case(golden_structural_flow._series("v")[650:950])),
        block("structural_flow_novolume",
              "Structural flow: golden series 'nv' bars 0-329: bear flow, Re-Sync, late "
              "expansion, exhaustion, dominant expansion, weakening waves",
              *structural_flow_case(golden_structural_flow._series("nv")[:330])),
        block("structural_flow_transitions",
              "Structural flow: golden series 'nv' bars 200-529: initiations both ways, "
              "transitions, compression, bull Re-Sync",
              *structural_flow_case(golden_structural_flow._series("nv")[200:530])),
    ]
    return header, blocks
