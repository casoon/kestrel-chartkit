"""golden_volume — partly derived so far.

Derived here: Elder's Force Index, Relative Volume at Time (package 31) and the Price Volume
Trend (37), each from the definition documented on its type. Not yet: the base volume
indicators from the 0.2.0 starting point.
"""

from ..smoothing import ema_published

NAME = "golden_volume"
PARTIAL = True
CONSTANTS = {"efi_tolerance": 1e-12, "rvat_tolerance": 1e-12, "pvt_tolerance": 1e-12}

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


def derive():
    keys = {}
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
