"""contract_volume: complete per-bar contracts of the volume family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code. Where the golden module `volume.py` already holds an
independent implementation of a formula, it is imported and evaluated bar by bar (on the prefix
or window that bar sees) instead of being written a second time.

Conventions the type documentation leaves open, assumed here (each also named at its case):

- Alert kinds and constant strengths. Several types document the rule of an alert but not its
  kind or strength; the kind names and constants below are the ones the catalog publishes and are
  pinned by these cases: `volume` `high_volume_spike` 0.8; `cmf` `bull_bias`/`bear_bias` 0.8;
  `hires_volume_flow` `volume_absorption` 0.7; `money_flow_profile` bias flips 1.0.
- Alert rules that are not documented on the type at all, pinned to the catalog's behaviour and
  marked as such in the coverage matrix: `rvol` `extreme_rvol` 0.9 at RVOL >= 2.5;
  `volume_profile` `price_at_poc` 0.85 when |close - POC| / close <= 0.003, otherwise
  `price_above_vah` 0.8 for close > VAH, otherwise `price_below_val` 0.8 for close < VAL;
  `persistent_volume_profile` one `bin_expired` 0.3 when a bar leaving the window takes the last
  touch away from a bin.
- `klinger` publishes `extra["kvo"]` (equal to `value`) beside the documented `signal`, `hist`
  and `volume_force`.
- `hires_volume_flow`: the window of both robust bands holds the last `window_len` values, the
  current bar's included; the median of an even count is the mean of the two middle values; the
  MAD scale is 1 / Phi^-1(3/4). No band (and no absorption) before the window is full.
- `rvat`: the contract bars are 900 s apart, so `bar_seconds` is set to 900 in every case - with
  the catalog default 60 the declared warmup (1441 bars) cannot equal the first output of a
  15-minute series. A comparison day is a calendar day among the previous `days` days.
- `money_flow_profile`: crosses use the crate's crossing tolerance (`contract_pilot`).
- `market_facilitation` publishes its state (`green`, `fade`, `fake`, `squat`, `neutral`) in the
  `state` column of the table; every volume oscillator comes from `kestrel_reference.volume_oscillators`.
- Unknown volume (`NaN`) is used only where the type documents it (`rvol`, `obv`, `cmf`).
"""

import math
from statistics import NormalDist

from ..fixture import provenance, render_contract
from ..indicators import clamp
from ..smoothing import ema_first_sample
from ..tpo_profile import compute as tpo_compute
from ..volume_oscillators import compute as oscillator_compute
from .contract_pilot import case_block, crossed_over, crossed_under
from .volume import (_acc_dist, _cmf, _cvd, _force_index, _hires_flow, _klinger,
                     _money_flow_profile, _persistent_poc, _price_volume_trend, _profile,
                     _relative_volume_at_time, _volume_index)

NAME = "contract_volume"
RENDER = render_contract

# Relative to max(1, |expected|): the second implementation repeats the documented arithmetic, so
# differences are rounding only (running sums over a few hundred bars included).
TOLERANCE = 1e-9

BAR_SECONDS = 900  # spacing of the contract bars (tests/common/contract.rs)
MAD_SCALE = 1.0 / NormalDist().inv_cdf(0.75)
NAN = float("nan")


# --- Input series ----------------------------------------------------------------------------

def shaped(n, seed, start=100.0, step=1.0, wick=0.8, base=500, spread=20):
    """Bars with the close anywhere in its range, open at the previous close, prices on a cent
    grid and volumes in hundreds, from a small linear congruential generator (deterministic)."""
    state = seed

    def rnd():
        nonlocal state
        state = (state * 1103515245 + 12345) % 2 ** 31
        return state / 2 ** 31

    bars, close = [], start
    for _ in range(n):
        open_ = close
        close = round(open_ + (rnd() - 0.5) * 2.0 * step, 2)
        high = round(max(open_, close) + 0.05 + rnd() * wick, 2)
        low = round(min(open_, close) - 0.05 - rnd() * wick, 2)
        bars.append((open_, high, low, close, float(base + 100 * int(rnd() * spread))))
    return bars


def with_volume(bars, changes):
    """`bars` with the volume of the 0-based bars in `changes` replaced."""
    out = list(bars)
    for index, volume in changes.items():
        o, h, l, c, _ = out[index]
        out[index] = (o, h, l, c, volume)
    return out


def with_close(bars, changes):
    """`bars` with the close of the given bars replaced (high/low widened to contain it)."""
    out = list(bars)
    for index, close in changes.items():
        o, h, l, _, v = out[index]
        out[index] = (o, max(h, close, o), min(l, close, o), close, v)
    return out


def known(volume):
    return math.isfinite(volume)


# --- Volume, RVOL, OBV, CMF, A/D -------------------------------------------------------------

def volume_case(ma_period, bars):
    """value = volume; from the ma_period-th bar `avg_volume` (mean of the last ma_period volumes,
    this one included) and `volume_ratio` (1 for a zero average); `high_volume_spike` 0.8 when the
    volume exceeds twice a positive average."""
    expected = []
    for i, bar in enumerate(bars):
        volume = bar[4]
        if i + 1 < ma_period:
            expected.append(((volume, {}), {}))
            continue
        average = sum(b[4] for b in bars[i + 1 - ma_period:i + 1]) / ma_period
        ratio = volume / average if average > 0.0 else 1.0
        alerts = {"high_volume_spike": 0.8} if average > 0.0 and volume > 2.0 * average else {}
        expected.append(((volume, {"avg_volume": average, "volume_ratio": ratio}), alerts))
    return {"ma_period": float(ma_period)}, bars, expected


def rvol_case(period, bars):
    """volume over the mean of the last `period` known volumes (this one included), 1 for a zero
    mean; a NaN-volume bar is skipped. `extreme_rvol` 0.9 at RVOL >= 2.5 (undocumented rule)."""
    seen, expected = [], []
    for bar in bars:
        if not known(bar[4]):
            expected.append((None, {}))
            continue
        seen.append(bar[4])
        if len(seen) < period:
            expected.append((None, {}))
            continue
        average = sum(seen[-period:]) / period
        rvol = bar[4] / average if average > 0.0 else 1.0
        expected.append(((rvol, {}), {"extreme_rvol": 0.9} if rvol >= 2.5 else {}))
    return {"period": float(period)}, bars, expected


def obv_case(bars):
    """Running total from 0: + volume on a higher close, - volume on a lower one, nothing on an
    equal close or the first bar. A NaN-volume bar adds nothing and has no output, but its close
    is the next bar's previous close."""
    total, previous, expected = 0.0, None, []
    for _, _, _, close, volume in bars:
        if previous is not None and known(volume):
            total += volume * ((close > previous) - (close < previous))
        previous = close
        expected.append(((total, {}), {}) if known(volume) else (None, {}))
    return {}, bars, expected


def cmf_case(period, bars):
    """CMF over the last `period` known-volume bars (`volume._cmf`, clamped to -1..1);
    `bull_bias` above 0.20, `bear_bias` below -0.20, both 0.8. A NaN-volume bar stays out."""
    seen, expected = [], []
    for bar in bars:
        if not known(bar[4]):
            expected.append((None, {}))
            continue
        seen.append(bar)
        if len(seen) < period:
            expected.append((None, {}))
            continue
        value = _cmf(seen, period)
        alerts = {"bull_bias": 0.8} if value > 0.20 else {"bear_bias": 0.8} if value < -0.20 else {}
        expected.append(((value, {}), alerts))
    return {"period": float(period)}, bars, expected


def acc_dist_case(bars):
    """Running total of MFM * volume (`volume._acc_dist` over the prefix)."""
    expected = [((_acc_dist(bars[:i + 1]), {}), {}) for i in range(len(bars))]
    return {}, bars, expected


# --- CVD and the high-resolution flow --------------------------------------------------------

def cvd_case(bars):
    """share = (close - low) / (high - low), 0.5 for a bar without range as documented; buy =
    volume * share, sell = volume * (1 - share), delta = buy - sell; value = running total
    (`volume._cvd` over the prefix)."""
    expected = []
    for i, (_, h, l, c, v) in enumerate(bars):
        share = (c - l) / (h - l) if h - l > 0.0 else 0.5
        buy, sell = v * share, v * (1.0 - share)
        expected.append(((_cvd(bars[:i + 1]), {"delta": buy - sell, "buy_volume": buy,
                                               "sell_volume": sell}), {}))
    return {}, bars, expected


def _median(values):
    ordered = sorted(values)
    mid = len(ordered) // 2
    return ordered[mid] if len(ordered) % 2 else (ordered[mid - 1] + ordered[mid]) / 2.0


def _robust_band(values):
    """median, median + 2.5 * scaled MAD."""
    median = _median(values)
    mad = _median([abs(v - median) for v in values]) * MAD_SCALE
    return median, median + 2.5 * mad


def hires_case(window_len, bars):
    """value = running total of volume * (2 * share - 1), share clamped to 0..1
    (`volume._hires_flow`); `delta` this bar's, `is_estimated` 1. `volume_absorption` 0.7 when the
    volume is above the upper robust band of the last window_len volumes and the range at or below
    the median of the last window_len ranges. Every bar here has a range (zero range is not
    documented on this type)."""
    expected = []
    for i, (_, h, l, c, v) in enumerate(bars):
        assert h > l
        delta = v * (2.0 * clamp((c - l) / (h - l), 0.0, 1.0) - 1.0)
        alerts = {}
        if i + 1 >= window_len:
            window = bars[i + 1 - window_len:i + 1]
            _, upper = _robust_band([b[4] for b in window])
            range_median, _ = _robust_band([b[1] - b[2] for b in window])
            if v > upper and h - l <= range_median:
                alerts["volume_absorption"] = 0.7
        expected.append(((_hires_flow(bars[:i + 1]), {"delta": delta, "is_estimated": 1.0}),
                         alerts))
    return {"window_len": float(window_len)}, bars, expected


def absorb_bars():
    """Shaped bars with two volume spikes on tight bars (absorption once the window is full) and
    one spike on a wide bar (no absorption)."""
    out = []
    for k, (o, h, l, c, v) in enumerate(shaped(30, 17)):
        if k in (3, 12, 22):
            mid = round((h + l) / 2.0, 2)
            o, h, l, c = mid, round(mid + 0.06, 2), round(mid - 0.06, 2), round(mid + 0.02, 2)
            v = 12000.0
        if k == 17:
            h, v = round(h + 3.0, 2), 15000.0
        out.append((o, h, l, c, v))
    return out


# --- Klinger, Chaikin oscillator, EOM, PVT, EFI, NVI/PVI ------------------------------------

def _volume_force(bars):
    """vf per bar as documented on the Klinger type (trend from high + low + close, cm restarted
    from the previous dm when the trend flips, ratio 0 for cm ~ 0)."""
    forces, previous = [], None
    for _, h, l, c, v in bars:
        total, dm = h + l + c, h - l
        if previous is None:
            trend, cm = 1.0, dm
        else:
            p_total, p_trend, p_dm, p_cm = previous
            trend = 1.0 if total > p_total else -1.0 if total < p_total else p_trend
            cm = p_cm + dm if trend == p_trend else p_dm + dm
        ratio = dm / cm if abs(cm) > 2.220446049250313e-16 else 0.0
        forces.append(v * abs(2.0 * ratio - 1.0) * trend * 100.0)
        previous = (total, trend, dm, cm)
    return forces


def klinger_case(fast, slow, signal, bars):
    """KVO = EMA(fast) - EMA(slow) of vf, both first-sample seeded from the first bar, published
    from the slow-th bar; signal = first-sample EMA(signal) over the published KVO; hist = KVO -
    signal; `kvo` repeats the value. The KVO/signal pair is `volume._klinger`'s."""
    forces = _volume_force(bars)
    kvo, sig = _klinger(bars, fast, slow, signal)
    fast_line, slow_line = ema_first_sample(forces, fast), ema_first_sample(forces, slow)
    assert all(abs(k - (f - s)) <= 1e-12 * max(1.0, abs(k))
               for k, f, s in zip(kvo, fast_line[slow - 1:], slow_line[slow - 1:]))
    expected = [(None, {}) for _ in range(slow - 1)]
    for k, (line, s) in enumerate(zip(kvo, sig)):
        expected.append(((line, {"kvo": line, "signal": s, "hist": line - s,
                                 "volume_force": forces[slow - 1 + k]}), {}))
    params = {"fast_len": float(fast), "slow_len": float(slow), "signal_len": float(signal)}
    return params, bars, expected


def chaikin_case(fast, slow, bars):
    """EMA(fast) - EMA(slow) of the A/D line, both first-sample seeded from the first bar,
    published from the slow-th bar."""
    line = [_acc_dist(bars[:i + 1]) for i in range(len(bars))]
    fast_line, slow_line = ema_first_sample(line, fast), ema_first_sample(line, slow)
    expected = [((f - s, {}), {}) if i >= slow - 1 else (None, {})
                for i, (f, s) in enumerate(zip(fast_line, slow_line))]
    return {"fast_len": float(fast), "slow_len": float(slow)}, bars, expected


def eom_case(period, divisor, bars):
    """raw = (hl2 - previous hl2) / ((volume / divisor) / max(high - low, 1e-8)), 0 for a box
    <= 1e-8; value = SMA(period) of raw, first with the period + 1-th bar."""
    raws, expected = [], [(None, {})]
    for previous, bar in zip(bars, bars[1:]):
        distance = (bar[1] + bar[2]) / 2.0 - (previous[1] + previous[2]) / 2.0
        box = (bar[4] / divisor) / max(bar[1] - bar[2], 1e-8)
        raws.append(distance / box if box > 1e-8 else 0.0)
        expected.append(((sum(raws[-period:]) / period, {}), {}) if len(raws) >= period
                        else (None, {}))
    return {"period": float(period), "volume_divisor": divisor}, bars, expected


def pvt_case(bars):
    """`volume._price_volume_trend`: no output on the first bar, then the running total."""
    totals = _price_volume_trend([b[3] for b in bars], [b[4] for b in bars])
    return {}, bars, [(None, {})] + [((t, {}), {}) for t in totals]


def efi_case(ema_len, bars):
    """`volume._force_index`: raw = change * volume, line = first-sample EMA from the first change,
    published from the ema_len-th change (bar ema_len + 1)."""
    published = _force_index([b[3] for b in bars], [b[4] for b in bars], ema_len)
    expected = [(None, {}) for _ in range(len(bars) - len(published))]
    expected += [((line, {"raw": raw}), {}) for raw, line in published]
    return {"ema_len": float(ema_len)}, bars, expected


def volume_index_case(rises, bars):
    """NVI (rises False) / PVI (rises True) per `volume._volume_index` over the prefix."""
    return {}, bars, [((_volume_index(bars[:i + 1], rises), {}), {}) for i in range(len(bars))]


# --- RVAT ------------------------------------------------------------------------------------

def rvat_case(days, offset, volumes):
    """`volume._relative_volume_at_time` on the contract timestamps (bar i at i * 900 s) shifted
    by the day start; `samples` and `complete` (samples >= days) from its sample count."""
    shifted = [(i * BAR_SECONDS - offset, v) for i, v in enumerate(volumes, start=1)]
    outputs = {t: (r, c, s) for t, r, c, s in _relative_volume_at_time(shifted, days)}
    bars = [(10.0, 10.5, 9.5, 10.0, v) for v in volumes]
    expected = []
    for t, _ in shifted:
        if t in outputs:
            regular, cumulative, samples = outputs[t]
            expected.append(((regular, {"cumulative": cumulative, "samples": float(samples),
                                        "complete": 1.0 if samples >= days else 0.0}), {}))
        else:
            expected.append((None, {}))
    params = {"days": float(days), "day_start_offset": float(offset),
              "bar_seconds": float(BAR_SECONDS)}
    return params, bars, expected


def rvat_volumes(n, zero_bars=()):
    """A volume pattern that differs per slot and per day; the listed 0-based bars carry zero."""
    return [0.0 if i in zero_bars else float(100 + (i * 37) % 400 + 50 * ((i // 96) % 3))
            for i in range(n)]


# --- Volume profiles -------------------------------------------------------------------------

def _bins(window, count):
    """Bin volumes as documented on VolumeProfileEngine (the spreading `volume._profile` uses),
    with the total weight."""
    low = min(b[2] for b in window)
    high = max(b[1] for b in window)
    step = (high - low) / count
    volumes, total = [0.0] * count, 0.0
    for _, h, l, _, v in window:
        weight = v if v > 0.0 else h - l
        total += weight
        start = min(max(math.floor((l - low) / step), 0), count - 1)
        end = max(min(max(math.floor((h - low) / step), 0), count - 1), start)
        share = weight / (end - start + 1)
        for index in range(start, end + 1):
            volumes[index] += share
    return low, step, volumes, total


def volume_profile_case(lookback, num_bins, bars):
    """POC/VAH/VAL from `volume._profile` over the last `lookback` bars; total_volume, the POC and
    close-bin densities and lvn_width = 2 step from the same bins. A window without range
    publishes the close alone. Alerts per the undocumented rule in the module docstring."""
    expected = []
    for i in range(len(bars)):
        if i + 1 < lookback:
            expected.append((None, {}))
            continue
        window = bars[i + 1 - lookback:i + 1]
        close = bars[i][3]
        if max(b[1] for b in window) - min(b[2] for b in window) < 1e-8:
            expected.append(((close, {}), {}))
            continue
        poc, vah, val = _profile(window, num_bins)
        low, step, volumes, total = _bins(window, num_bins)
        current = volumes[min(max(math.floor((close - low) / step), 0), num_bins - 1)]
        alerts = {}
        if abs(close - poc) / close <= 0.003:
            alerts["price_at_poc"] = 0.85
        elif close > vah:
            alerts["price_above_vah"] = 0.8
        elif close < val:
            alerts["price_below_val"] = 0.8
        extra = {"vpoc": poc, "vah": vah, "val": val, "total_volume": total,
                 "vpoc_density": max(volumes) / total, "current_density": current / total,
                 "lvn_width": 2.0 * step}
        expected.append(((poc, extra), alerts))
    return {"lookback": float(lookback), "num_bins": float(num_bins)}, bars, expected


def extended_profile_case(lookback, num_bins, bars):
    """value = the POC of `volume._profile` over the last `lookback` bars; no extra, no alerts; a
    window without range has no output."""
    expected = []
    for i in range(len(bars)):
        window = bars[max(0, i + 1 - lookback):i + 1]
        flat = max(b[1] for b in window) - min(b[2] for b in window) < 1e-8
        if i + 1 < lookback or flat:
            expected.append((None, {}))
        else:
            expected.append(((_profile(window, num_bins)[0], {}), {}))
    return {"lookback": float(lookback), "num_bins": float(num_bins)}, bars, expected


def _grid_keys(bar, width):
    start = math.floor(bar[2] / width)
    return set(range(start, max(math.floor(bar[1] / width), start) + 1))


def persistent_profile_case(lookback, width, bars):
    """value = `volume._persistent_poc` over the last `lookback` bars. `bin_expired` 0.3 when the
    bar leaving the window was the last to touch a bin (undocumented alert)."""
    expected = []
    for i in range(len(bars)):
        if i + 1 < lookback:
            expected.append((None, {}))
            continue
        window = bars[i + 1 - lookback:i + 1]
        alerts = {}
        if i >= lookback:
            remaining = set().union(*(_grid_keys(b, width) for b in window))
            if _grid_keys(bars[i - lookback], width) - remaining:
                alerts["bin_expired"] = 0.3
        expected.append(((_persistent_poc(window, width), {}), alerts))
    return {"lookback": float(lookback), "bin_width": width}, bars, expected


def money_flow_profile_case(lookback, rows, va_pct, bars):
    """Profile per window from `volume._money_flow_profile` (its last output is the full window's);
    alerts from the second output on with the crate's crossing tolerance: `bull_value_area_break` /
    `bear_value_area_break` with strength (distance beyond the level) / (VAH - VAL) clamped to 0..1,
    `bull_bias` / `bear_bias` 1.0 on crosses of bull_pct through 50."""
    expected, previous = [], None
    for i in range(len(bars)):
        window = bars[max(0, i + 1 - lookback):i + 1]
        if len(window) < 2:
            expected.append((None, {}))
            continue
        assert all(b[1] > b[2] for b in window)
        p = _money_flow_profile(window, lookback, rows, va_pct)[-1]
        close, vah, val, bull = bars[i][3], p["vah"], p["val"], p["bull_pct"]
        alerts = {}
        if previous is not None:
            p_close, p_vah, p_val, p_bull = previous
            width = vah - val
            if crossed_over(p_close, p_vah, close, vah):
                alerts["bull_value_area_break"] = clamp((close - vah) / width, 0.0, 1.0)
            if crossed_under(p_close, p_val, close, val):
                alerts["bear_value_area_break"] = clamp((val - close) / width, 0.0, 1.0)
            if crossed_over(p_bull, 50.0, bull, 50.0):
                alerts["bull_bias"] = 1.0
            if crossed_under(p_bull, 50.0, bull, 50.0):
                alerts["bear_bias"] = 1.0
        previous = (close, vah, val, bull)
        extra = {"vah": vah, "val": val, "delta_poc": p["delta_poc"], "bull_pct": bull}
        expected.append(((p["poc"], extra), alerts))
    params = {"lookback": float(lookback), "rows": float(rows), "va_pct": va_pct}
    return params, bars, expected


def trending(n, seed, drift):
    """`shaped` with a cyclic drift added, so a short profile window is left behind in both
    directions and breakouts and bias flips happen."""
    out, shift = [], 0.0
    for k, (o, h, l, c, v) in enumerate(shaped(n, seed, step=0.6)):
        shift = round(shift + drift[k % len(drift)], 2)
        out.append((round(o + shift, 2), round(h + shift, 2), round(l + shift, 2),
                    round(c + shift, 2), v))
    return out


# --- Cases -----------------------------------------------------------------------------------

def tpo_case(bars, **params):
    """The time-at-price profile, from `kestrel_reference.tpo_profile`."""
    params = {key: float(value) for key, value in params.items()}
    return params, bars, tpo_compute(params, bars)


def tpo_cases():
    # Two bars of equal weight at the ends of a range 0..4 in four bins: every bin holds one
    # bar, the two middle bins tie at the same distance from the centre of the range, and the
    # close of the last bar decides (3.4 lies nearer the third bin, 0.6 the second).
    tie_high = [(0.5, 1.0, 0.0, 0.8, 1000.0), (3.5, 4.0, 3.0, 3.4, 1000.0)]
    tie_low = [(3.5, 4.0, 3.0, 3.2, 1000.0), (0.5, 1.0, 0.0, 0.6, 1000.0)]
    flat = [(10.0, 10.0, 10.0, 10.0, 500.0)] * 4 + [(10.0, 10.5, 9.5, 10.2, 500.0)] + \
           [(10.0, 10.0, 10.0, 10.0, 500.0)] * 3
    return [
        ("tpo_profile_defaults", "TPO profile with the catalog defaults (48, 30) over 90 bars",
         tpo_case(shaped(90, 103))),
        ("tpo_profile_short", "TPO profile (8, 5) over 40 bars: few bins, a short window",
         tpo_case(shaped(40, 7), lookback=8, num_bins=5)),
        ("tpo_profile_tie_high", "TPO profile (2, 4): the middle bins tie at equal distance from "
         "the range centre, a close in the upper half picks the third bin",
         tpo_case(tie_high, lookback=2, num_bins=4)),
        ("tpo_profile_tie_low", "TPO profile (2, 4): the same tie with a close in the lower half "
         "picks the second bin",
         tpo_case(tie_low, lookback=2, num_bins=4)),
        ("tpo_profile_one_bin", "TPO profile (3, 1): one bin holds every bar",
         tpo_case(shaped(12, 17), lookback=3, num_bins=1)),
        ("tpo_profile_flat", "TPO profile (3, 5): windows without range publish the close for "
         "all three values",
         tpo_case(flat, lookback=3, num_bins=5)),
    ]


def oscillator_case(name, bars, **params):
    """One of the four volume oscillators, from `kestrel_reference.volume_oscillators`. The state
    of `market_facilitation` leaves the extras and is written as the `state` of the output."""
    params = {key: float(value) for key, value in params.items()}
    rows = oscillator_compute(name, params, bars)
    states = {}
    expected = []
    for i, (output, alerts) in enumerate(rows, start=1):
        if output is not None and "state" in output[1]:
            extra = dict(output[1])
            states[i] = extra.pop("state")
            output = (output[0], extra)
        expected.append((output, alerts))
    return params, bars, expected, states


def oscillator_block(name, comment, case):
    params, bars, expected, states = case
    block = case_block(name, comment, params, bars, expected)
    for i, state in states.items():
        block[2][f"out{i}_state"] = state
    return block


def oscillator_cases():
    level = 1e8
    # Quiet bars between active ones: the volume of bars 40..46 is zero, so a period of 3 sees an
    # average volume of zero from bar 44 on.
    stretch = with_volume(shaped(60, 211), {i: 0.0 for i in range(39, 46)})
    stretch_close = with_close(with_volume(shaped(24, 223), {7: 0.0, 8: 0.0, 9: 0.0}),
                               {10: 100.0, 11: 100.0, 12: 100.0})
    equal_volume = with_volume(shaped(40, 229), {10: 700.0, 11: 700.0, 20: 0.0, 21: 0.0, 22: 800.0})
    twin = [(10.0, 11.0, 9.0, 10.0, 500.0)] * 2
    return [
        ("volume_flow_indicator_defaults", "VFI with the catalog defaults (130, 0.2, 2.5, 3) over "
         "300 bars", oscillator_case("volume_flow_indicator", shaped(300, 201))),
        ("volume_flow_indicator_short", "VFI (period 2, smoothing 1): the line is the raw ratio",
         oscillator_case("volume_flow_indicator", shaped(70, 203), period=2, smoothing=1)),
        ("volume_flow_indicator_no_cutoff", "VFI (period 5, coef 0, vcoef 1): no cutoff, volume "
         "capped at the average",
         oscillator_case("volume_flow_indicator", shaped(80, 207), period=5, coef=0, vcoef=1,
                         smoothing=2)),
        ("volume_flow_indicator_zero_volume", "VFI (period 3, smoothing 2): seven bars without "
         "volume leave the raw ratio unpublished while the average volume is zero",
         oscillator_case("volume_flow_indicator", stretch, period=3, smoothing=2)),
        ("volume_flow_indicator_high_level", "VFI (period 5) at a price level around 1e8",
         oscillator_case("volume_flow_indicator", shaped(70, 211, start=level), period=5,
                         smoothing=2)),
        ("volume_zone_oscillator_defaults", "VZO with the catalog default (period 14)",
         oscillator_case("volume_zone_oscillator", shaped(80, 213))),
        ("volume_zone_oscillator_one", "VZO(1): equal closes count as zero, zero volume leaves "
         "no value",
         oscillator_case("volume_zone_oscillator", stretch_close, period=1)),
        ("volume_zone_oscillator_five", "VZO(5) over shaped bars with equal closes",
         oscillator_case("volume_zone_oscillator", stretch_close + shaped(30, 227), period=5)),
        ("volume_price_confirmation_defaults", "VPCI with the catalog defaults (5, 20, 20)",
         oscillator_case("volume_price_confirmation", shaped(120, 231))),
        ("volume_price_confirmation_small", "VPCI (2, 3, 2): short window of two bars",
         oscillator_case("volume_price_confirmation", shaped(40, 233), short=2, long=3,
                         smoothing=2)),
        ("volume_price_confirmation_minimal", "VPCI (1, 2, 1): the signal is the value itself",
         oscillator_case("volume_price_confirmation", shaped(30, 237), short=1, long=2,
                         smoothing=1)),
        ("volume_price_confirmation_zero_volume", "VPCI (2, 4, 3): a stretch without volume leaves "
         "bars unpublished while either window has no volume",
         oscillator_case("volume_price_confirmation",
                         with_volume(shaped(40, 239), {i: 0.0 for i in range(12, 19)}),
                         short=2, long=4, smoothing=3)),
        ("volume_price_confirmation_high_level", "VPCI (3, 8, 4) at a price level around 1e8: the "
         "difference of the two means keeps its precision",
         oscillator_case("volume_price_confirmation", shaped(60, 241, start=level), short=3,
                         long=8, smoothing=4)),
        ("market_facilitation_shaped", "Market facilitation: all four states over shaped bars",
         oscillator_case("market_facilitation", shaped(60, 251))),
        ("market_facilitation_edges", "Market facilitation: equal volume and an equal index are "
         "neutral, zero volume leaves no value and restarts the state",
         oscillator_case("market_facilitation", twin + equal_volume)),
    ]


def cases():
    base = shaped(40, 7)
    spikes = with_volume(base, {24: 9000.0, 31: 6000.0, 35: 4000.0})
    equal_closes = with_close(shaped(16, 11), {5: 100.0, 6: 100.0})
    flat_first = [(100.0, 100.0, 100.0, 100.0, 800.0)] + shaped(10, 13)[1:]

    volume_zero = [(10.0, 10.5, 9.5, 10.0, v)
                   for v in [0.0, 0.0, 0.0, 100.0, 0.0, 300.0, 100.0, 100.0]]
    rvol_nan = with_volume(shaped(14, 19), {5: NAN, 9: NAN, 7: 2000.0})
    # RVOL exactly 2.5: four volumes a, then 4a, over period 5; then zeros (mean 0 -> 1).
    rvol_edge = [(10.0, 10.5, 9.5, 10.0, v) for v in [100.0] * 4 + [400.0] + [0.0] * 6]
    cmf_nan = with_volume(shaped(14, 23), {4: NAN, 10: 0.0})
    cmf_zero = [(10.0, 10.5, 9.5, 10.2, 0.0)] * 4 + [(10.0, 10.0, 10.0, 10.0, 500.0),
                                                     (10.0, 10.6, 9.6, 10.6, 300.0)]
    obv_nan = with_volume(equal_closes, {8: NAN})
    eom_bars = with_volume(flat_first, {4: 0.0})
    efi_bars = with_volume(with_close(shaped(12, 29), {5: 100.0, 6: 100.0}), {8: 0.0})
    pvt_bars = with_close(shaped(15, 31), {7: 100.0, 8: 100.0})
    index_bars = with_volume(shaped(25, 37), {10: 500.0, 11: 500.0})

    vp_small = shaped(14, 41) + [(98.0, 98.0, 98.0, 98.0, 0.0)] * 3
    vp_zero_volume = with_volume(shaped(12, 43), {3: 0.0, 6: 0.0})
    # Profile first, then a window of identical bars without range, then movement again.
    flat_window = (shaped(4, 47, start=50.0) + [(50.0, 50.0, 50.0, 50.0, 100.0)] * 4
                   + shaped(4, 48, start=50.0))
    persistent_bars = shaped(90, 53)
    persistent_small = shaped(20, 59, step=1.5, wick=0.4)

    return [
        ("volume_defaults", "Volume with the catalog default (ma_period 20): spikes alert",
         volume_case(20, spikes)),
        ("volume_short_zero", "Volume, ma_period 3: zero average gives ratio 1 and no alert",
         volume_case(3, volume_zero)),
        ("rvol_defaults", "RVOL with the catalog default (period 20)", rvol_case(20, spikes)),
        ("rvol_nan_volume", "RVOL(4): NaN-volume bars after the first output are skipped, not counted",
         rvol_case(4, rvol_nan)),
        ("rvol_threshold", "RVOL(5): exactly 2.5 alerts, a zero mean gives 1",
         rvol_case(5, rvol_edge)),
        ("obv_equal_closes", "OBV: equal closes leave the total, the first bar adds nothing",
         obv_case(equal_closes)),
        ("obv_nan_volume", "OBV: a NaN-volume bar adds nothing, its close still counts",
         obv_case(obv_nan)),
        ("cmf_defaults", "CMF with the catalog default (period 20)", cmf_case(20, spikes)),
        ("cmf_short", "CMF(5): bias alerts in both directions", cmf_case(5, shaped(30, 61))),
        ("cmf_nan_volume", "CMF(4): NaN volume stays out of both windows, zero volume enters",
         cmf_case(4, cmf_nan)),
        ("cmf_zero_window", "CMF(3): zero volume sum gives 0, a bar without range MFM 0",
         cmf_case(3, cmf_zero)),
        ("acc_dist_shaped", "A/D over shaped bars after a bar without range",
         acc_dist_case(flat_first + shaped(12, 67))),
        ("cvd_shaped", "CVD over shaped bars", cvd_case(shaped(20, 71))),
        ("cvd_zero_range", "CVD: a bar without range splits its volume evenly (share 0.5)",
         cvd_case(flat_first)),
        ("hires_volume_flow_defaults", "HiRes flow with the catalog default (window_len 20)",
         hires_case(20, absorb_bars())),
        ("hires_volume_flow_short", "HiRes flow, window_len 5: absorption on tight spikes",
         hires_case(5, absorb_bars())),
        ("klinger_defaults", "Klinger with the catalog defaults (34, 55, 13)",
         klinger_case(34, 55, 13, shaped(80, 73))),
        ("klinger_short", "Klinger(3, 5, 3), ending with unchanged hlc sums (trend kept)",
         klinger_case(3, 5, 3, shaped(20, 79) + [(100.0, 101.0, 99.0, 100.0, 900.0)] * 3)),
        ("chaikin_oscillator_defaults", "Chaikin oscillator with the catalog defaults (3, 10)",
         chaikin_case(3, 10, shaped(25, 83))),
        ("chaikin_oscillator_short", "Chaikin oscillator (2, 5)",
         chaikin_case(2, 5, shaped(15, 89))),
        ("eom_defaults", "EOM with the catalog defaults (14, divisor 10000)",
         eom_case(14, 10000.0, shaped(25, 97))),
        ("eom_edges", "EOM(3, divisor 1000): a bar without range and a zero-volume bar",
         eom_case(3, 1000.0, eom_bars)),
        ("pvt_shaped", "PVT over shaped bars with an unchanged close", pvt_case(pvt_bars)),
        ("efi_defaults", "EFI with the catalog default (ema_len 13)",
         efi_case(13, shaped(25, 101))),
        ("efi_short", "EFI(3): unchanged close and zero volume give raw 0",
         efi_case(3, efi_bars)),
        ("nvi_shaped", "NVI: moves only on falling volume", volume_index_case(False, index_bars)),
        ("pvi_shaped", "PVI: moves only on rising volume", volume_index_case(True, index_bars)),
        ("rvat_two_days", "RVAT, days 2, day start 00:00 UTC: four days, zero-volume slots",
         rvat_case(2, 0, rvat_volumes(300, zero_bars={10, 20, 106}))),
        ("rvat_offset", "RVAT, catalog days 10, day start 01:00 UTC: a partial day before day 0",
         rvat_case(10, 3600, rvat_volumes(200))),
        ("volume_profile_defaults", "Volume profile with the catalog defaults (70, 30)",
         volume_profile_case(70, 30, shaped(90, 103))),
        ("volume_profile_small", "Volume profile (5, 10): alerts above, below and at the POC",
         volume_profile_case(5, 10, vp_small)),
        ("volume_profile_zero_volume", "Volume profile (4, 6): zero-volume bars weigh their range",
         volume_profile_case(4, 6, vp_zero_volume)),
        ("volume_profile_flat", "Volume profile (3, 5): windows without range publish the close",
         volume_profile_case(3, 5, flat_window)),
        ("extended_volume_profile_defaults", "Extended profile with the catalog defaults (70, 30)",
         extended_profile_case(70, 30, shaped(90, 103))),
        ("extended_volume_profile_flat", "Extended profile (3, 5): no output for windows without range",
         extended_profile_case(3, 5, flat_window)),
        ("persistent_volume_profile_defaults",
         "Persistent profile with the catalog defaults (70, bin width 1)",
         persistent_profile_case(70, 1.0, persistent_bars)),
        ("persistent_volume_profile_small", "Persistent profile (4, bin width 0.5): bins expire",
         persistent_profile_case(4, 0.5, persistent_small)),
        ("money_flow_profile_defaults",
         "Money flow profile with the catalog defaults (200, 25, 0.70), window rolling",
         money_flow_profile_case(200, 25, 0.70, shaped(215, 107))),
        ("money_flow_profile_short", "Money flow profile (3, 10, 0.5) over a drifting series",
         money_flow_profile_case(3, 10, 0.5, trending(40, 109, [0.8, 0.8, -1.5, 0.6, -0.9]))),
    ]


def build():
    header = provenance("contract_volume") + [
        "",
        "Format: see contract_pilot. Assumed conventions: see the module docstring.",
    ]
    blocks = [case_block(name, comment, *case) for name, comment, case in cases() + tpo_cases()]
    blocks += [oscillator_block(name, comment, case) for name, comment, case in oscillator_cases()]
    return header, blocks
