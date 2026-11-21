"""Volume oscillators: volume_flow_indicator, volume_zone_oscillator, volume_price_confirmation,
market_facilitation, true_range_money_flow.

Written from the rules (Katsanos 2004 for the flow indicator and the volume cap of the true range
money flow, Khalil and Steckler 2011 for the zone oscillator, Dormeier 2007 for the price
confirmation, Williams 1995 for the facilitation index), independently of the Rust code. Python standard library only.

`compute(name, params, bars)` runs one indicator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)` with `alerts` mapping each alert kind to its strength (always 1).
`market_facilitation` carries its state in `extras["state"]`. Invalid parameters raise
`ValueError`.
"""

import math

NAMES = ("volume_flow_indicator", "volume_zone_oscillator", "volume_price_confirmation",
         "market_facilitation", "true_range_money_flow")

# Relative tolerance of a crossing: a crossing needs the lines to differ by more than this.
CROSS_TOLERANCE_REL = 1e-9


def _get_int(params, key, default, lo):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key} must be an integer")
    if isinstance(v, float):
        if not math.isfinite(v) or v != int(v):
            raise ValueError(f"{key} must be an integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key} must be an integer")
    if v < lo:
        raise ValueError(f"{key} must be at least {lo}")
    return v


def _get_float(params, key, default):
    v = params.get(key, default)
    if isinstance(v, bool) or not isinstance(v, (int, float)) or not math.isfinite(v):
        raise ValueError(f"{key} must be a finite number")
    return float(v)


def _check_keys(name, params, allowed):
    for key in params:
        if key not in allowed:
            raise ValueError(f"unknown parameter {key!r} for {name}")


def _crossings(prev_a, prev_b, a, b):
    """`(above, below)`: `a` crossed over or under `b` between the previous bar and this one."""
    tol = CROSS_TOLERANCE_REL * max(1.0, abs(prev_a), abs(prev_b), abs(a), abs(b))
    return prev_a <= prev_b + tol and a > b + tol, prev_a >= prev_b - tol and a < b - tol


def _ema_step(previous, x, alpha):
    return x if previous is None else alpha * x + (1.0 - alpha) * previous


def _zero_alerts(values):
    """Alerts of a line that crosses zero; `values` holds a number or None per bar."""
    alerts = [{} for _ in values]
    for i in range(1, len(values)):
        if values[i] is None or values[i - 1] is None:
            continue
        above, below = _crossings(values[i - 1], 0.0, values[i], 0.0)
        if above:
            alerts[i]["bull"] = 1.0
        if below:
            alerts[i]["bear"] = 1.0
    return alerts


def _volume_flow_indicator(params, bars):
    _check_keys("volume_flow_indicator", params, ("period", "coef", "vcoef", "smoothing"))
    period = _get_int(params, "period", 130, 2)
    coef = _get_float(params, "coef", 0.2)
    vcoef = _get_float(params, "vcoef", 2.5)
    smoothing = _get_int(params, "smoothing", 3, 1)
    if not 0.0 <= coef <= 10.0:
        raise ValueError("coef must lie within 0..10")
    if not 0.0 < vcoef <= 100.0:
        raise ValueError("vcoef must lie within (0, 100]")
    count = len(bars)
    start = max(30, period)
    alpha = 2.0 / (smoothing + 1)
    typical = [(b[1] + b[2] + b[3]) / 3.0 for b in bars]
    volume = [b[4] for b in bars]
    inter = [None] * count
    for i in range(1, count):
        if typical[i] > 0 and typical[i - 1] > 0:
            inter[i] = math.log(typical[i] / typical[i - 1])
    signed = [None] * count
    for i in range(start, count):
        window = inter[i - 29:i + 1]
        if any(x is None for x in window):
            continue
        mean = sum(window) / 30.0
        vinter = math.sqrt(sum((x - mean) ** 2 for x in window) / 30.0)
        cutoff = coef * vinter * bars[i][3]
        vave = sum(volume[i - period:i]) / period
        capped = min(volume[i], vcoef * vave)
        change = typical[i] - typical[i - 1]
        signed[i] = capped if change > cutoff else -capped if change < -cutoff else 0.0
    values, rows, line = [None] * count, [(None, {})] * count, None
    for i in range(start + period - 1, count):
        window = signed[i - period + 1:i + 1]
        if any(x is None for x in window):
            continue
        vave = sum(volume[i - period:i]) / period
        if not vave > 0:
            continue
        raw = sum(window) / vave
        line = _ema_step(line, raw, alpha)
        values[i] = line
        rows[i] = ((line, {"raw": raw}), {})
    return [(output, alerts) for (output, _), alerts in zip(rows, _zero_alerts(values))]


def _volume_zone_oscillator(params, bars):
    _check_keys("volume_zone_oscillator", params, ("period",))
    period = _get_int(params, "period", 14, 1)
    alpha = 2.0 / (period + 1)
    values = [None] * len(bars)
    signed = total = None
    for i in range(1, len(bars)):
        change = bars[i][3] - bars[i - 1][3]
        direction = 1.0 if change > 0 else -1.0 if change < 0 else 0.0
        signed = _ema_step(signed, direction * bars[i][4], alpha)
        total = _ema_step(total, bars[i][4], alpha)
        if total != 0:
            values[i] = 100.0 * signed / total
    return [(None, a) if v is None else ((v, {}), a) for v, a in zip(values, _zero_alerts(values))]


def _volume_price_confirmation(params, bars):
    _check_keys("volume_price_confirmation", params, ("short", "long", "smoothing"))
    short = _get_int(params, "short", 5, 1)
    long = _get_int(params, "long", 20, 2)
    smoothing = _get_int(params, "smoothing", 20, 1)
    if not short < long:
        raise ValueError("short must be less than long")
    count = len(bars)
    close = [b[3] for b in bars]
    volume = [b[4] for b in bars]
    raw = [None] * count
    for i in range(long - 1, count):
        wide = range(i - long + 1, i + 1)
        near = range(i - short + 1, i + 1)
        volume_long = sum(volume[j] for j in wide)
        volume_short = sum(volume[j] for j in near)
        if not (volume_long > 0 and volume_short > 0):
            continue
        mean_short = sum(close[j] for j in near) / short
        if mean_short == 0:
            continue
        # The difference of the long-window volume-weighted and plain means is taken over
        # deviations from the current close: two means near a high price level would cancel
        # most of their digits.
        reference = close[i]
        weighted = sum((close[j] - reference) * volume[j] for j in wide) / volume_long
        plain = sum(close[j] - reference for j in wide) / long
        ratio = (sum(close[j] * volume[j] for j in near) / volume_short) / mean_short
        volume_ratio = (volume_short / short) / (volume_long / long)
        raw[i] = (weighted - plain) * ratio * volume_ratio
    rows, defined = [(None, {})] * count, []
    for i in range(count):
        if raw[i] is None:
            continue
        defined.append(raw[i])
        if len(defined) < smoothing:
            continue
        signal = sum(defined[-smoothing:]) / smoothing
        alerts = {}
        previous = rows[i - 1][0] if i > 0 else None
        if previous is not None:
            above, below = _crossings(previous[0], previous[1]["signal"], raw[i], signal)
            if above:
                alerts["bull"] = 1.0
            if below:
                alerts["bear"] = 1.0
        rows[i] = ((raw[i], {"signal": signal}), alerts)
    return rows


def _market_facilitation(params, bars):
    _check_keys("market_facilitation", params, ())
    index = [(b[1] - b[2]) / b[4] if b[4] > 0 else None for b in bars]
    rows = []
    for i, value in enumerate(index):
        if value is None:
            rows.append((None, {}))
            continue
        state = "neutral"
        if i > 0 and index[i - 1] is not None:
            index_up, index_down = value > index[i - 1], value < index[i - 1]
            volume_up, volume_down = bars[i][4] > bars[i - 1][4], bars[i][4] < bars[i - 1][4]
            if index_up and volume_up:
                state = "green"
            elif index_down and volume_down:
                state = "fade"
            elif index_up and volume_down:
                state = "fake"
            elif index_down and volume_up:
                state = "squat"
        rows.append(((value, {"state": state}), {"squat": 1.0} if state == "squat" else {}))
    return rows


def _true_range_money_flow(params, bars):
    _check_keys("true_range_money_flow", params, ("period", "volume_cap"))
    period = _get_int(params, "period", 21, 1)
    cap = _get_float(params, "volume_cap", 2.5)
    if period > 10000:
        raise ValueError("period must be at most 10000")
    if not 0.0 <= cap <= 100.0:
        raise ValueError("volume_cap must lie within 0..100")
    values = [None] * len(bars)
    average = money = weight = None
    sum_money = sum_weight = 0.0
    for i, (_, high, low, close, volume) in enumerate(bars):
        if i == 0:
            average = volume
            continue
        top, bottom = max(high, bars[i - 1][3]), min(low, bars[i - 1][3])
        span = top - bottom
        location = ((close - bottom) - (top - close)) / span if span > 0 else 0.0
        counted = min(volume, cap * average) if cap > 0 else volume
        if i <= period:
            sum_money += counted * location
            sum_weight += counted
            if i == period:
                money, weight = sum_money / period, sum_weight / period
        else:
            money += (counted * location - money) / period
            weight += (counted - weight) / period
        average += (volume - average) / period
        if i >= period and weight > 0:
            values[i] = money / weight
    return [(None, a) if v is None else ((v, {}), a) for v, a in zip(values, _zero_alerts(values))]


def compute(name, params, bars):
    params = dict(params or {})
    runner = {
        "volume_flow_indicator": _volume_flow_indicator,
        "volume_zone_oscillator": _volume_zone_oscillator,
        "volume_price_confirmation": _volume_price_confirmation,
        "market_facilitation": _market_facilitation,
        "true_range_money_flow": _true_range_money_flow,
    }.get(name)
    if runner is None:
        raise ValueError(f"unknown indicator {name!r}")
    return runner(params, bars)
