"""golden_analytics_components: the measures the analytics read-outs combine, each from the
definition documented on its function."""

from .. import analytics as an
from ..fixture import provenance
from ..indicators import efficiency_ratio

NAME = "golden_analytics_components"
OFFSETS = [0.0, 1.5, -1.0, 0.8, -0.6]


def shaped(step, wick):
    """120 bars around a line of slope `step` with a five-bar rebound pattern; open = previous
    close, wicks beyond the body."""
    bars, previous = [], 100.0
    for i in range(120):
        close = 100.0 + step * i + OFFSETS[i % 5]
        high = max(previous, close) + wick + 0.1 * (i % 3)
        low = min(previous, close) - wick - 0.1 * (i % 2)
        bars.append((previous, high, low, close, 1.0))
        previous = close
    return bars


RAMP = [(100.0 + i * 0.5, 100.0 + i * 0.5 + 0.2, 100.0 + i * 0.5 - 0.2, 100.0 + i * 0.5, 1.0)
        for i in range(120)]
NOISY = shaped(0.2, 0.3)
DRIFT = shaped(0.05, 0.2)
PRICE = [(c, c + 1.0, c - 1.0, c, 1.0)
         for c in [10.0, 11.0, 12.0, 11.0, 13.0, 14.0, 13.0, 15.0, 16.0, 15.0]]
FEAR = [(100.0, 101.0, 99.0, 100.0, 1.0)] * 40 + [(100.0, 101.0, 80.0, 100.0, 1.0)]
VOLUME = [(100.0, 101.0, 99.0, 100.0, v) for v in (1.0, 4.0, 2.0, 3.0)]

HEADER = [
    "kestrel-chartkit golden reference fixture: analytics components",
    "",
    *provenance("analytics_components"),
    "",
    "Each value from the definition documented on its function:",
    "  classify_trend_regime(bars, 14, 20)  adx: ADX(14, 14) fed from the first bar; chop:",
    "        100 log10(sum TR / (highest high - lowest low)) / log10(20) over the last 20 bars,",
    "        the first TR against the close before them; efficiency: ratio over the last 21",
    "        closes; votes: adx >= 25, chop <= 38.2, efficiency >= 0.5.",
    "  trend_persistence_reading  r2_score = 100 corr(last 34 closes, index)^2; er_score = 100 ER",
    "        over the last 35 closes; fdi_score = 100 (1 - norm(1 + ln(path / range) / ln 34,",
    "        1.20, 1.65)), norm clamped to 0..1.",
    "  LegEfficiencyEngine(34), SMA(5)  the ratio over 35 closes; the last average and the one",
    "        5 published values earlier.",
    "  price_summary(bars, atr_len, 1.5)  Wilder ATR of the true range, seeded with the mean of",
    "        the first atr_len values; atr_percentile = share of the published ATR series at or",
    "        below the last value; stop = 1.5 ATR; change and range position of the last close.",
    "  fear_gauge_reading  wvf = 100 (hc - low) / hc, bwvf = 100 (high - lc) / lc, hc/lc the",
    "        highest/lowest close of the last 22 bars.",
    "  activity_reading  share of the last 4 volumes at or below the last one.",
    "",
    "Series (open, high, low, close, volume):",
    "  ramp    120 bars, open = close = 100 + 0.5 i, high/low = close +/- 0.2, volume 1.",
    "  noisy   120 bars, close = 100 + 0.2 i + [0, 1.5, -1, 0.8, -0.6][i mod 5], open = previous",
    "          close (100 at first), high = max(open, close) + 0.3 + 0.1 (i mod 3),",
    "          low = min(open, close) - 0.3 - 0.1 (i mod 2), volume 1.",
    "  drift   as noisy, with slope 0.05 instead of 0.2 and wick 0.2 instead of 0.3.",
    "  price   closes [10, 11, 12, 11, 13, 14, 13, 15, 16, 15], open = close,",
    "          high/low = close +/- 1; atr_len 3.",
    "  fear    (100, 101, 99, 100, 1) 40 times, then (100, 101, 80, 100, 1): a fear spike on an",
    "          unchanged close, which the gauge flags as absorbed.",
    "  volume  (100, 101, 99, 100, v) for v = 1, 4, 2, 3.",
    "",
    "On the ramp every trend measure sits at its bound (ADX 100, efficiency 1, R^2 and fractal",
    "scores 100). noisy and drift move them off it: noisy trends with one regime vote; drift",
    "barely moves, casts no vote and has R^2 and efficiency scores below 20.",
    "drift_atr_percentile is from price_summary(drift, 14, 1.5).",
    "",
    "analytics_components_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def _trend_measures(bars, prefix):
    regime = an.regime(bars, 14, 20)
    persistence = an.trend_persistence(bars)
    return {
        f"{prefix}_adx": regime["adx"],
        f"{prefix}_chop": regime["choppiness"],
        f"{prefix}_efficiency": regime["efficiency"],
        f"{prefix}_votes": float(regime["votes"]),
        f"{prefix}_r2_score": persistence["r2_score"],
        f"{prefix}_er_score": persistence["er_score"],
        f"{prefix}_fdi_score": persistence["fdi_score"],
    }


def derive():
    closes = [b[3] for b in RAMP]
    averages = [sum(closes[i - 4:i + 1]) / 5.0 for i in range(4, len(closes))]
    regime = an.regime(RAMP, 14, 20)
    persistence = an.trend_persistence(RAMP)
    price = an.price_summary(PRICE, 3, 1.5)
    fear = an.fear_gauge(FEAR)
    assert fear["state"] == "fear_spike" and fear["absorbed"], fear
    keys = {
        "ramp_adx": regime["adx"],
        "ramp_er": efficiency_ratio(closes, 34),
        "ramp_r2_score": persistence["r2_score"],
        "ramp_fdi_score": persistence["fdi_score"],
        "ramp_chop": regime["choppiness"],
        "ramp_sma5_last": averages[-1],
        "ramp_sma5_prior": averages[-6],
        "price_atr": price["atr"],
        "price_atr_pct": price["atr_pct"],
        "price_atr_rank": price["atr_percentile"],
        "price_stop": price["stop_distance"],
        "price_change_pct": price["change_pct"],
        "price_range_position": price["range_position"],
        "fear_wvf": fear["wvf"],
        "fear_bwvf": fear["bwvf"],
        "volume_rank": an.volume_percentile(VOLUME, 4),
    }
    keys.update(_trend_measures(NOISY, "noisy"))
    keys.update(_trend_measures(DRIFT, "drift"))
    keys["drift_atr_percentile"] = an.price_summary(DRIFT, 14, 1.5)["atr_percentile"]
    return keys


def build():
    values = {**derive(), "analytics_components_tolerance": 1e-9}
    groups = [("ramp", "ramp_"), ("price", "price_"), ("fear", "fear_"), ("volume", "volume_"),
              ("noisy", "noisy_"), ("drift", "drift_")]
    blocks = [(None, comment, {k: v for k, v in values.items() if k.startswith(stem)})
              for comment, stem in groups]
    blocks.append((None, None, {"analytics_components_tolerance": 1e-9}))
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
