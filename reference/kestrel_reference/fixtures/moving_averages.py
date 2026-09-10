"""golden_moving_averages — partly derived so far.

Derived here: the base SMA/EMA/WMA/LSMA over the 10-bar series, the EMA seed contract
(package 19), the LSMA fit outputs (package 23), VIDYA with the CMO (package 27) and Tillson T3
(package 28). Every definition below is the one stated in the fixture header.
"""

from ..smoothing import ema_first_sample, ema_sma_seed, least_squares, sma, wma

NAME = "golden_moving_averages"
PARTIAL = True
CONSTANTS = {"ma_tolerance": 1e-9}

CLOSES = [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0]
EMA_SEED_CLOSES = [22.0, 22.5, 23.25, 22.75, 23.5, 24.0, 23.0, 22.25, 23.75, 24.5, 25.0, 24.25]
LSMA_WINDOWS = {"linear": [100.0, 102.5, 105.0, 107.5, 110.0], "noisy": [10.0, 12.0, 11.0, 14.0, 13.0]}
VIDYA_CLOSES = [100.0, 101.0, 100.5, 102.0, 101.0, 103.0, 104.0, 103.5, 105.0, 104.0, 106.0,
                105.5, 107.0, 106.5, 108.0]
T3_CLOSES = [100.0, 101.0, 102.0, 101.5, 103.0, 104.0, 103.5, 105.0, 106.0, 105.5, 107.0, 108.0,
             107.5, 109.0, 110.0]


def _cmo(changes):
    """Chande Momentum Oscillator over a window of price changes: 100 (up - down) / (up + down)."""
    up = sum(c for c in changes if c > 0)
    down = -sum(c for c in changes if c < 0)
    return 100.0 * (up - down) / (up + down) if up + down else 0.0


def _vidya(closes, cmo_len, ema_len):
    """VIDYA: the first value is the close of the bar the CMO first becomes defined for (after
    cmo_len changes); from then on an exponential step with alpha = 2/(ema_len+1) * |CMO|/100.
    Returns the published values and the last bar's CMO and alpha."""
    values, cmo, alpha = [], None, None
    for index in range(cmo_len, len(closes)):
        changes = [closes[i] - closes[i - 1] for i in range(index - cmo_len + 1, index + 1)]
        cmo = _cmo(changes)
        if not values:
            values.append(closes[index])
            continue
        alpha = 2.0 / (ema_len + 1) * abs(cmo) / 100.0
        values.append(values[-1] + alpha * (closes[index] - values[-1]))
    return values, cmo, alpha


def _t3(closes, period, v):
    """Tillson T3: six chained first-sample-seeded EMAs of `period`, combined as
    c1 e6 + c2 e5 + c3 e4 + c4 e3 with c1 = -v^3, c2 = 3v^2 + 3v^3, c3 = -6v^2 - 3v - 3v^3,
    c4 = 1 + 3v + v^3 + 3v^2; published from the period-th bar on."""
    stages = [closes]
    for _ in range(6):
        stages.append(ema_first_sample(stages[-1], period))
    e3, e4, e5, e6 = stages[3], stages[4], stages[5], stages[6]
    c1 = -v ** 3
    c2 = 3 * v ** 2 + 3 * v ** 3
    c3 = -6 * v ** 2 - 3 * v - 3 * v ** 3
    c4 = 1 + 3 * v + v ** 3 + 3 * v ** 2
    combined = [c1 * a + c2 * b + c3 * c + c4 * d for a, b, c, d in zip(e6, e5, e4, e3)]
    return combined[period - 1:]


def derive():
    keys = {
        "sma5_last": sma(CLOSES[-5:]),
        "ema5_last": ema_first_sample(CLOSES, 5)[-1],
        "wma5_last": wma(CLOSES[-5:]),
        "lsma5_last": float(least_squares(CLOSES[-5:])[3]),
    }

    sma_seeded = [x for x in ema_sma_seed(EMA_SEED_CLOSES, 5) if x is not None]
    keys["ema5_seed_sma_first_output"] = sma_seeded[0]
    keys["ema5_seed_sma_last"] = sma_seeded[-1]
    keys["ema5_seed_first_sample_last"] = ema_first_sample(EMA_SEED_CLOSES, 5)[-1]

    for label, window in LSMA_WINDOWS.items():
        slope, intercept, r2, value = least_squares(window)
        keys[f"lsma5_{label}_slope"] = float(slope)
        keys[f"lsma5_{label}_intercept"] = float(intercept)
        keys[f"lsma5_{label}_r2"] = float(r2)
        keys[f"lsma5_{label}_value"] = float(value)

    vidya, cmo, alpha = _vidya(VIDYA_CLOSES, 4, 5)
    keys["vidya4_5_output_count"] = float(len(vidya))
    keys["vidya4_5_seed"] = vidya[0]
    keys["vidya4_5_second"] = vidya[1]
    keys["vidya4_5_last"] = vidya[-1]
    keys["vidya4_5_last_cmo"] = cmo
    keys["vidya4_5_last_alpha"] = alpha

    t3 = _t3(T3_CLOSES, 3, 0.7)
    keys["t3_3_v07_output_count"] = float(len(t3))
    keys["t3_3_v07_first"] = t3[0]
    keys["t3_3_v07_last"] = t3[-1]
    keys["t3_3_v00_last"] = _t3(T3_CLOSES, 3, 0.0)[-1]
    keys["t3_3_v10_last"] = _t3(T3_CLOSES, 3, 1.0)[-1]
    return keys
