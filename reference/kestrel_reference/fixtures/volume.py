"""golden_volume: the volume-based indicators, each value from the definition documented on its
type."""

import math

from ..fixture import provenance
from ..smoothing import ema_first_sample, ema_published

NAME = "golden_volume"
TOLERANCES = {
    "vwap_tolerance": 1e-12,
    "volume_profile_tolerance": 1e-12,
    "volume_tolerance": 1e-9,
    "efi_tolerance": 1e-12,
    "rvat_tolerance": 1e-12,
    "pvt_tolerance": 1e-12,
}

# The shared series: open = close = p, high = p + 0.5, low = p - 0.5, volume (i + 1) * 100.
VOL_PRICES = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84, 46.08, 45.89, 46.03,
              45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]
BARS = [(p, p + 0.5, p - 0.5, p, (i + 1) * 100.0) for i, p in enumerate(VOL_PRICES)]
# The close away from mid-range and volume both rising and falling, so the money-flow and volume
# index formulas actually act.
SHAPED = [(10.0, 10.8, 9.6, 10.6, 1200.0), (10.6, 11.2, 10.2, 10.3, 900.0),
          (10.3, 10.9, 10.0, 10.8, 1500.0), (10.8, 11.0, 10.1, 10.2, 700.0),
          (10.2, 10.7, 9.9, 10.6, 1100.0), (10.6, 11.4, 10.5, 11.3, 1600.0),
          (11.3, 11.5, 10.9, 11.0, 800.0), (11.0, 11.6, 10.8, 11.5, 1300.0)]

HEADER = [
    "kestrel-chartkit golden reference fixture: volume-based indicators",
    "",
    *provenance("volume"),
    "",
    "Base values over the shared 20-bar series: open = close = p, high = p + 0.5, low = p - 0.5,",
    "volume (i + 1) * 100, with p = [44.34, 44.09, 44.15, 43.61, 44.33, 44.83, 45.10, 45.42, 45.84,",
    "46.08, 45.89, 46.03, 45.61, 46.28, 46.28, 46.00, 46.03, 46.41, 46.22, 45.64]. Each from the",
    "definition on its type: A/D, Anchored VWAP (default whole-day session from 00:00 UTC, so one",
    "anchor for the whole series), CMF(5), CVD,",
    "EOM(5, divisor 10000), Extended Volume Profile (last 5 bars, 10 bins), HiRes Volume Flow,",
    "Klinger(3, 5, 3), NVI, OBV, Persistent Volume Profile (last 5 bars, bin width 1), PVI,",
    "RVOL(5), and the volume with its 5-bar average. VWAP over two bars and the Volume Profile",
    "over three equal bars (10 bins) use their own small inputs, given in the test.",
    "",
    "On the shared series the close sits exactly mid-range and volume only rises, so A/D, CMF, CVD",
    "and the HiRes flow are identically zero there and NVI stays at its start value 1000: those keys",
    "pin that invariance. The *_shaped keys come from an 8-bar series with the close away from",
    "mid-range and volume rising and falling, where the formulas actually act.",
    "",
    "Elder's Force Index(3): raw = (close - previous close) * volume, main line an EMA over that raw",
    "series (alpha = 2/(N+1), seeded with the first raw value, published from the N-th change on).",
    "Closes 100, 102, 101, 101, 104, 103 with volumes 1000, 1500, 800, 0, 1200, 900 - the fourth",
    "bar is zero-volume and unchanged, so its raw force is 0 and enters as an ordinary observation.",
    "",
    "Relative Volume at Time (package 31), days = 5, day start at UTC midnight. Three days with three",
    "slots each (00:00, 01:00, 02:00): day 0: 100, 200, 300; day 1: 150, -, 350 (01:00 missing);",
    "day 2: 300, 100, 200. Regular ratio against the same slot on previous days that had it,",
    "cumulative ratio against what those days had accumulated by that slot.",
    "",
    "Price Volume Trend (package 37) over closes [100, 102, 101, 101, 104] with volumes",
    "[1000, 1500, 800, 1200, 900]: running sum of volume * (close / previous close - 1), from the",
    "first bar that has a predecessor.",
    "",
    "The *_tolerance keys are the comparison tolerances the tests apply, stated rather than derived.",
]

SECONDS_PER_DAY = 86_400


def _force_index(closes, volumes, length):
    """raw = (close - previous close) * volume; line = EMA(length) over raw, seeded with the first
    raw value and published from the length-th change on."""
    raw = [(closes[t] - closes[t - 1]) * volumes[t] for t in range(1, len(closes))]
    line = ema_published(raw, length)
    return [(r, v) for r, v in zip(raw, line) if v is not None]


def _relative_volume_at_time(bars, days):
    """Regular: this bar's volume over the mean volume in the same slot on the previous `days`
    days that had a bar there. Cumulative: today's volume up to and including this slot over the
    mean of what those same days had accumulated by that slot. Only earlier days count; a missing
    slot lowers the sample count and is never filled in; no output without a comparison."""
    history, out = {}, []
    for timestamp, volume in bars:
        day, slot = divmod(timestamp, SECONDS_PER_DAY)
        comparison = [d for d in range(day - days, day) if slot in history.get(d, {})]
        today = history.setdefault(day, {})
        cumulative_today = sum(v for s, v in today.items() if s < slot) + volume
        today[slot] = volume
        if not comparison:
            continue
        regular_base = sum(history[d][slot] for d in comparison) / len(comparison)
        cumulative_base = sum(sum(v for s, v in history[d].items() if s <= slot)
                              for d in comparison) / len(comparison)
        if regular_base == 0:
            continue
        out.append((timestamp, volume / regular_base, cumulative_today / cumulative_base,
                    len(comparison)))
    return out


def _price_volume_trend(closes, volumes):
    """Running sum of volume * (close / previous close - 1), from the first bar that has a
    predecessor."""
    total, out = 0.0, []
    for t in range(1, len(closes)):
        total += volumes[t] * (closes[t] / closes[t - 1] - 1.0)
        out.append(total)
    return out


def _money_flow_multiplier(high, low, close):
    """((close - low) - (high - close)) / (high - low), 0 for a range below 1e-8."""
    span = high - low
    return ((close - low) - (high - close)) / span if span > 1e-8 else 0.0


def _acc_dist(bars):
    return sum(_money_flow_multiplier(h, l, c) * v for _, h, l, c, v in bars)


def _cmf(bars, length):
    window = bars[-length:]
    total = sum(v for *_, v in window)
    flow = sum(_money_flow_multiplier(h, l, c) * v for _, h, l, c, v in window)
    return max(-1.0, min(1.0, flow / total if total > 0.0 else 0.0))


def _cvd(bars):
    """Running sum of volume * (2 share - 1), share = (close - low) / (high - low), range floored
    at 1e-8."""
    return sum(v * (2.0 * (c - l) / max(h - l, 1e-8) - 1.0) for _, h, l, c, v in bars)


def _hires_flow(bars):
    """As CVD, with the share clamped to 0..=1."""
    return sum(v * (2.0 * min(max((c - l) / max(h - l, 1e-8), 0.0), 1.0) - 1.0)
               for _, h, l, c, v in bars)


def _profile(bars, bins):
    """Volume by price over the window's own range split into `bins` equal bins. Each bar's volume
    (its range when it has none) is spread evenly over the bins from floor((low - min) / step) to
    floor((high - min) / step), both clamped to the last bin. POC is the first bin with the
    largest volume, priced at its centre; the value area grows from the POC one bin at a time
    towards the larger neighbour (up on a tie) until it holds 70 % of the volume. VAH is the upper
    edge of its top bin, VAL the lower edge of its bottom bin."""
    low = min(b[2] for b in bars)
    high = max(b[1] for b in bars)
    step = (high - low) / bins
    volumes = [0.0] * bins
    total = 0.0
    for _, h, l, _, v in bars:
        weight = v if v > 0.0 else h - l
        total += weight
        start = min(max(math.floor((l - low) / step), 0), bins - 1)
        end = max(min(max(math.floor((h - low) / step), 0), bins - 1), start)
        share = weight / (end - start + 1)
        for index in range(start, end + 1):
            volumes[index] += share
    poc, best = 0, 0.0
    for index, volume in enumerate(volumes):
        if volume > best:
            poc, best = index, volume
    target = total * 0.70
    accumulated, bottom, top = volumes[poc], poc, poc
    while accumulated < target and (bottom > 0 or top < bins - 1):
        down = volumes[bottom - 1] if bottom > 0 else -1.0
        up = volumes[top + 1] if top < bins - 1 else -1.0
        if up >= down and top < bins - 1:
            top += 1
            accumulated += volumes[top]
        elif bottom > 0:
            bottom -= 1
            accumulated += volumes[bottom]
        else:
            top += 1
            accumulated += volumes[top]
    return low + (poc + 0.5) * step, low + (top + 1.0) * step, low + bottom * step


def _klinger(bars, fast, slow, signal):
    """Volume force vf = volume |2 dm/cm - 1| trend 100 with dm = high - low, trend from the sum
    high + low + close against the previous bar (kept when equal, +1 on the first bar), and cm
    accumulating dm while the trend holds and restarting from the previous dm plus this one when it
    flips. KVO = EMA(fast) - EMA(slow) over vf, both first-sample-seeded from the first bar, published
    from the slow-th bar on; signal = EMA(signal) over the published KVO values."""
    previous_sum, previous_trend, previous_dm, cm = None, 1.0, 0.0, 0.0
    forces = []
    for _, high, low, close, volume in bars:
        total = high + low + close
        if previous_sum is None:
            trend = 1.0
        elif total > previous_sum:
            trend = 1.0
        elif total < previous_sum:
            trend = -1.0
        else:
            trend = previous_trend
        dm = high - low
        if previous_sum is None:
            cm = dm
        elif trend == previous_trend:
            cm = cm + dm
        else:
            cm = previous_dm + dm
        ratio = dm / cm if abs(cm) > 2.220446049250313e-16 else 0.0
        forces.append(volume * abs(2.0 * ratio - 1.0) * trend * 100.0)
        previous_sum, previous_trend, previous_dm = total, trend, dm
    kvo = [f - s for f, s in zip(ema_first_sample(forces, fast), ema_first_sample(forces, slow))]
    kvo = kvo[slow - 1:]
    return kvo, ema_first_sample(kvo, signal)


def _volume_index(bars, rises):
    """NVI (rises = False) or PVI (rises = True): starts at 1000 and moves by the close's relative
    change only on bars whose volume fell (NVI) respectively rose (PVI) against the previous bar."""
    index = 1000.0
    for previous, bar in zip(bars, bars[1:]):
        moved = bar[4] > previous[4] if rises else bar[4] < previous[4]
        if moved:
            change = (bar[3] - previous[3]) / previous[3] if previous[3] > 0.0 else 0.0
            index += index * change
    return index


def _persistent_poc(bars, bin_width):
    """Bins on a fixed price grid, key = floor(price / bin_width). Each bar's volume (its range
    when it has none) is spread evenly over the keys from floor(low / w) to floor(high / w); the
    window holds the last bars only. POC is the centre of the first (lowest-priced) bin with the
    largest volume."""
    volumes = {}
    for _, high, low, _, volume in bars:
        weight = volume if volume > 0.0 else high - low
        start = math.floor(low / bin_width)
        end = max(math.floor(high / bin_width), start)
        for key in range(start, end + 1):
            volumes[key] = volumes.get(key, 0.0) + weight / (end - start + 1)
    best_key, best = None, -math.inf
    for key in sorted(volumes):
        if volumes[key] > best:
            best_key, best = key, volumes[key]
    return (best_key * bin_width + (best_key + 1) * bin_width) / 2.0


def derive():
    keys = {}
    # Rolling VWAP of the typical price over two bars: (100 * 1000 + 110 * 3000) / 4000.
    two = [(100.0, 105.0, 95.0, 100.0, 1000.0), (110.0, 115.0, 105.0, 110.0, 3000.0)]
    keys["vwap_two_bar"] = (sum((h + l + c) / 3.0 * v for _, h, l, c, v in two)
                            / sum(v for *_, v in two))
    three = [(100.0, 102.0, 98.0, 100.0, v) for v in (1000.0, 2000.0, 3000.0)]
    keys["vpoc_three_bar"], keys["vah_three_bar"], keys["val_three_bar"] = _profile(three, 10)

    keys["acc_dist_last"] = _acc_dist(BARS)
    typical = [((h + l + c) / 3.0, v) for _, h, l, c, v in BARS]
    keys["anchored_vwap_last"] = sum(t * v for t, v in typical) / sum(v for _, v in typical)
    keys["cmf5_last"] = _cmf(BARS, 5)
    keys["cvd_last"] = _cvd(BARS)
    raw = [((b[1] + b[2]) / 2.0 - (a[1] + a[2]) / 2.0) / ((b[4] / 10000.0) / max(b[1] - b[2], 1e-8))
           for a, b in zip(BARS, BARS[1:])]
    keys["eom5_last"] = sum(raw[-5:]) / 5.0
    keys["ext_vpoc"], keys["ext_vah"], keys["ext_val"] = _profile(BARS[-5:], 10)
    keys["hires_flow_last"] = _hires_flow(BARS)
    kvo, signal = _klinger(BARS, 3, 5, 3)
    keys["klinger_line"], keys["klinger_signal"] = kvo[-1], signal[-1]
    keys["pers_vpoc"] = _persistent_poc(BARS[-5:], 1.0)
    keys["nvi_last"] = _volume_index(BARS, rises=False)
    keys["pvi_last"] = _volume_index(BARS, rises=True)
    keys["obv_last"] = sum(b[4] * ((b[3] > a[3]) - (b[3] < a[3])) for a, b in zip(BARS, BARS[1:]))
    volumes = [b[4] for b in BARS]
    keys["rvol5_last"] = volumes[-1] / (sum(volumes[-5:]) / 5.0)
    keys["volume_last"] = volumes[-1]
    keys["volume_avg5"] = sum(volumes[-5:]) / 5.0

    keys["acc_dist_shaped_last"] = _acc_dist(SHAPED)
    keys["cmf5_shaped_last"] = _cmf(SHAPED, 5)
    keys["cvd_shaped_last"] = _cvd(SHAPED)
    keys["hires_flow_shaped_last"] = _hires_flow(SHAPED)
    keys["nvi_shaped_last"] = _volume_index(SHAPED, rises=False)
    efi = _force_index([100.0, 102.0, 101.0, 101.0, 104.0, 103.0],
                       [1000.0, 1500.0, 800.0, 0.0, 1200.0, 900.0], 3)
    for (raw, line), label in zip(efi, ["first", "second", "third"]):
        keys[f"efi3_raw_{label}"] = raw
        keys[f"efi3_line_{label}"] = line

    profile = [[100.0, 200.0, 300.0], [150.0, None, 350.0], [300.0, 100.0, 200.0]]
    bars = [(day * SECONDS_PER_DAY + slot, volume)
            for day, volumes in enumerate(profile)
            for slot, volume in zip((0, 3600, 7200), volumes) if volume is not None]
    outputs = _relative_volume_at_time(bars, 5)
    keys["rvat_output_count"] = float(len(outputs))
    for timestamp, regular, cumulative, samples in outputs:
        day, slot = divmod(timestamp, SECONDS_PER_DAY)
        keys[f"rvat_d{day}_s{slot}_regular"] = regular
        keys[f"rvat_d{day}_s{slot}_cumulative"] = cumulative
        if not (day == 1 and slot == 7200):
            keys[f"rvat_d{day}_s{slot}_samples"] = float(samples)

    pvt = _price_volume_trend([100.0, 102.0, 101.0, 101.0, 104.0],
                              [1000.0, 1500.0, 800.0, 1200.0, 900.0])
    keys["pvt_output_count"] = float(len(pvt))
    for value, label in zip(pvt, ["first", "second", "third", "last"]):
        keys[f"pvt_{label}"] = value
    return keys


SECTIONS = [
    ("base values over the shared 20-bar series and the small VWAP/profile inputs",
     ["vwap_two_bar", "vwap_tolerance", "vpoc_three_bar", "vah_three_bar", "val_three_bar",
      "volume_profile_tolerance", "acc_dist_last", "anchored_vwap_last", "cmf5_last", "cvd_last",
      "eom5_last", "ext_vpoc", "ext_vah", "ext_val", "hires_flow_last", "klinger_line",
      "klinger_signal", "nvi_last", "obv_last", "pers_vpoc", "pvi_last", "rvol5_last",
      "volume_last", "volume_avg5", "volume_tolerance"]),
    ("the same money-flow and volume-index definitions on the shaped series",
     ["acc_dist_shaped_last", "cmf5_shaped_last", "cvd_shaped_last", "hires_flow_shaped_last",
      "nvi_shaped_last"]),
    ("Elder's Force Index(3)",
     ["efi3_raw_first", "efi3_line_first", "efi3_raw_second", "efi3_line_second",
      "efi3_raw_third", "efi3_line_third", "efi_tolerance"]),
    ("Relative Volume at Time (package 31)",
     ["rvat_d1_s0_regular", "rvat_d1_s0_cumulative", "rvat_d1_s0_samples",
      "rvat_d1_s7200_regular", "rvat_d1_s7200_cumulative", "rvat_d2_s0_regular",
      "rvat_d2_s0_cumulative", "rvat_d2_s0_samples", "rvat_d2_s3600_regular",
      "rvat_d2_s3600_cumulative", "rvat_d2_s3600_samples", "rvat_d2_s7200_regular",
      "rvat_d2_s7200_cumulative", "rvat_d2_s7200_samples", "rvat_output_count",
      "rvat_tolerance"]),
    ("Price Volume Trend (package 37)",
     ["pvt_first", "pvt_second", "pvt_third", "pvt_last", "pvt_output_count", "pvt_tolerance"]),
]


def build():
    values = {**derive(), **TOLERANCES}
    blocks = [(None, comment, {key: values[key] for key in keys}) for comment, keys in SECTIONS]
    placed = sum(len(case) for _, _, case in blocks)
    assert placed == len(values), f"{placed} placed, {len(values)} values"
    return HEADER, blocks
