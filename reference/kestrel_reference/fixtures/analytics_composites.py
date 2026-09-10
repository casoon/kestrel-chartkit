"""golden_analytics_composites: the analytics read-outs that combine the measures of
golden_analytics_components, each from the combination documented on its function."""

from .. import analytics as an
from ..fixture import provenance
from .analytics_components import DRIFT, FEAR, NOISY, RAMP, VOLUME

NAME = "golden_analytics_composites"
GREED = [(109.0, 110.0, 100.0, 109.0, 10.0)] * 40
LEVELS = [(float(c), c + 1.0, c - 1.0, float(c), 1.0) for c in range(100, 120)]

HEADER = [
    "kestrel-chartkit golden reference fixture: analytics composites",
    "",
    *provenance("analytics_composites"),
    "",
    "Each value from the combination documented on its function, over the series and measures",
    "of golden_analytics_components:",
    "  trend_persistence_reading  raw = (40 r2 + 25 er + 20 adx + 15 fdi) / 100 for each of the",
    "        last 5 bars, adx = 100 (0.7 norm(ADX, 12, 35) + 0.3 norm(slope, -1, 1.5)) with slope",
    "        the first-sample EMA(3) of the ADX changes, damped by 0.35 when r2 and er are both",
    "        below 20; score = first-sample EMA(5) of the five raws;",
    "        risk = 0.7 (100 - score) + 0.3 (100 - er).",
    "  trend_reading  SMA(5) kernel: 100 (s_last / s_(last-5) - 1).",
    "  activity_reading  mean of the given ATR percentile 0.5 and the volume percentile.",
    "  fear_greed_reading  0.30 fear + 0.20 volatility + 0.20 flow + 0.20 persistence",
    "        + 0.10 regime, a missing component 50; fear 15 in a fear spike, pulled halfway to 50",
    "        when absorbed; volatility 100 (1 - atr_percentile); flow 50 (m + 1), m the",
    "        volume-weighted close location value over the last 34 bars; regime",
    "        55 + 25 votes / 3 when trending, 45 + 15 votes / 3 when ranging.",
    "  cheat_sheet(bars, 20)  P = (H + L + last) / 3 and the pivots 2P - H, 2P - L, P - (H - L),",
    "        P + (H - L); Fib 50 % = H - (H - L) / 2; SMA20; RSI 30/50/70: the next close that",
    "        moves Wilder's RSI(14) to that level.",
    "",
    "Cases:",
    "  persistence_*, trend_sma_slope  the ramp.",
    "  activity_score                  the volume series.",
    "  fear_greed_neutral_flow         40 bars (109, 110, 100, 109, 10), no other reading.",
    "  fear_greed_absorbed_flow        the same with the fear gauge's reading of the fear series.",
    "  level_*                         20 bars, close = 100 .. 119, high/low = close +/- 1.",
    "  noisy_*, drift_*                persistence over those series; drift is damped.",
    "  fear_greed_drift                the drift bars with their regime, price summary",
    "                                  (atr_len 14) and persistence readings, no fear gauge.",
    "",
    "analytics_composites_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def _persistence(bars, prefix):
    reading = an.trend_persistence(bars)
    return {
        f"{prefix}_adx_score": reading["adx_score"],
        f"{prefix}_score": reading["score"],
        f"{prefix}_risk": reading["transition_risk"],
    }


def derive():
    keys = _persistence(RAMP, "persistence")
    keys["trend_sma_slope"] = an.sma_slope(RAMP, 5)
    keys["activity_score"] = (0.5 + an.volume_percentile(VOLUME, 4)) / 2.0
    keys["fear_greed_neutral_flow"] = an.fear_greed(GREED)
    keys["fear_greed_absorbed_flow"] = an.fear_greed(GREED, fear=an.fear_gauge(FEAR))
    levels = an.levels(LEVELS, 20)
    for label, key in [("Pivot", "level_pivot"), ("Pivot S1", "level_pivot_s1"),
                       ("Pivot R1", "level_pivot_r1"), ("Pivot S2", "level_pivot_s2"),
                       ("Pivot R2", "level_pivot_r2"), ("Fib 50.0%", "level_fib50"),
                       ("SMA20", "level_sma20"), ("RSI 30", "level_rsi30"),
                       ("RSI 50", "level_rsi50"), ("RSI 70", "level_rsi70")]:
        keys[key] = levels[label]
    keys.update(_persistence(NOISY, "noisy_persistence"))
    keys.update(_persistence(DRIFT, "drift_persistence"))
    keys["fear_greed_drift"] = an.fear_greed(
        DRIFT, regime_reading=an.regime(DRIFT, 14, 20),
        atr_percentile=an.price_summary(DRIFT, 14, 1.5)["atr_percentile"],
        persistence=keys["drift_persistence_score"])
    return keys


def build():
    values = {**derive(), "analytics_composites_tolerance": 1e-9}
    groups = [("ramp", ("persistence_", "trend_")), ("volume", ("activity_",)),
              ("sentiment", ("fear_greed_neutral", "fear_greed_absorbed")),
              ("levels", ("level_",)), ("noisy and drift", ("noisy_", "drift_")),
              (None, ("fear_greed_drift",)), (None, ("analytics_composites_tolerance",))]
    blocks = [(None, comment, {k: v for k, v in values.items() if k.startswith(stems)})
              for comment, stems in groups]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
