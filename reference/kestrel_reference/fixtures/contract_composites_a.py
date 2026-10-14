"""contract_composites_a: complete per-bar contracts of the composites a family (plan 62, step 4).

Same format as `contract_pilot`: a case fixes bars and parameters and, per bar, the output, every
extra field and the complete set of alerts. Each expected value follows the formula documented
on the indicator's type, not the Rust code.

Most indicators of this family already have an independent second implementation in their golden
fixture module (`fixtures/<name>.py`). Those modules fix their parameters as module constants;
`constants` sets them for one case and restores them afterwards, so the golden implementation is
reused as it is instead of being copied. What a golden module does not compute (fields the golden
fixture does not check, alert strengths) is added here from the type documentation.

Conventions assumed where the documentation leaves them open are stated per indicator below.
"""

import contextlib
import math

from .. import pine_series as ps
from ..fixture import provenance
from . import coil_release as cr_ref
from . import compression_zones as cz_ref
from . import flow_exhaustion as fe_ref
from . import market_state_chain as msc_ref
from . import overextension_risk as oer_ref
from . import price_average_relation as par_ref
from . import reaction_speed as rs_ref
from . import roc_context as roc_ref
from . import rsi_cycle_stats as rcs_ref
from . import tradability_filter as tf_ref
from . import smoothed_supertrend as sst_ref
from .contract_pilot import case_block

NAME = "contract_composites_a"


@contextlib.contextmanager
def constants(module, **values):
    """Sets module-level constants (or functions) for the duration of the block."""
    saved = {key: getattr(module, key) for key in values}
    for key, value in values.items():
        setattr(module, key, value)
    try:
        yield
    finally:
        for key, value in saved.items():
            setattr(module, key, value)


def block(name, comment, params, bars, expected):
    """`case_block` with an optional `secondary`: an output is `(value, extra)` or
    `(value, extra, secondary)`; the secondary key follows the value key."""
    plain = [(None if o is None else (o[0], o[1]), a) for o, a in expected]
    prefix, comment, case = case_block(name, comment, params, bars, plain)
    ordered = {}
    for key, value in case.items():
        ordered[key] = value
        if key.startswith("out") and key.endswith("_value"):
            index = int(key[3:-6])
            output = expected[index - 1][0]
            if len(output) > 2 and output[2] is not None:
                ordered[f"out{index}_secondary"] = output[2]
    return prefix, comment, ordered


def ohlcv(bars):
    """Bars as (open, high, low, close, volume) tuples of floats."""
    return [tuple(float(x) for x in b) for b in bars]


def events_at(events, n):
    """Per bar the list of event kinds of an `{kind: [bar, ...]}` map."""
    per_bar = [[] for _ in range(n)]
    for kind, bars in events.items():
        for t in bars:
            per_bar[t].append(kind)
    return per_bar


def flat_series(n, price=100.0, volume=1000.0):
    """A bar that never moves: open = high = low = close."""
    return [(price, price, price, price, volume)] * n


def mirrored(bars, axis=300.0):
    """Every price reflected at `axis`: high and low swap roles, volume is kept. A long/short
    symmetric indicator turns each bullish event into its bearish counterpart and back."""
    return [(axis - o, axis - l, axis - h, axis - c, v) for o, h, l, c, v in bars]


# --- ROC Context -----------------------------------------------------------------------------
#
# Values and events from `roc_context.compute` (the golden implementation) with the case's
# parameters; `hist = line - signal`, secondary = signal. `ma_kind` 1 (SMA) and 2 (RMA) replace
# the EMA of line, signal and context by the strict SMA / the SMA-seeded Wilder average.
# Alert strength (assumed convention; the type documents `quality` for stretch crosses only): a
# stretch cross carries quality / 100 clamped to 0.05..1; zero crosses and stalls have no graded
# quality and carry the fixed strength 0.5.

ROC_MA = {0: ps.ema, 1: ps.sma, 2: ps.rma}
ROC_GRADED = {"bull_cross", "bull_cross_weak", "bear_cross", "bear_cross_weak"}


def roc_context_case(bars, roc_len=12, avg_len=3, sig_len=9, ma_kind=0, ob_level=5.0,
                     os_level=-5.0, ctx_len=50, stall_len=5):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(roc_ref, ROC_LEN=roc_len, AVG_LEN=avg_len, SIG_LEN=sig_len, OB=ob_level,
                   OS=os_level, CTX_LEN=ctx_len, STALL_LEN=stall_len), \
            constants(ps, ema=ROC_MA[ma_kind]):
        out = roc_ref.compute(bars)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        if out["line"][t] is None:
            expected.append((None, {}))
            continue
        extra = {"hist": out["line"][t] - out["signal"][t], "context": out["ctx"][t],
                 "sentiment": out["sentiment"][t], "stall": out["stall"][t],
                 "divergence": out["divergence"][t]}
        if out["quality"][t] is not None:
            extra["quality"] = out["quality"][t]
        alerts = {}
        for kind in per_bar[t]:
            if kind in ROC_GRADED:
                alerts[kind] = ps.clamp(out["quality"][t] / 100.0, 0.05, 1.0)
            else:
                alerts[kind] = 0.5
        expected.append(((out["line"][t], extra, out["signal"][t]), alerts))
    params = {"roc_len": float(roc_len), "avg_len": float(avg_len), "sig_len": float(sig_len),
              "ma_kind": float(ma_kind), "ob_level": ob_level, "os_level": os_level,
              "ctx_len": float(ctx_len), "stall_len": float(stall_len)}
    return params, bars, expected


def roc_context_blocks():
    return [
        block("roc_context_defaults", "ROC Context, catalog defaults, wave series",
              *roc_context_case(ps.wave_series(320))),
        block("roc_context_short", "ROC Context with short lengths and narrow levels",
              *roc_context_case(ps.wave_series(160), roc_len=5, avg_len=2, sig_len=4,
                                ob_level=2.0, os_level=-2.0, ctx_len=20, stall_len=3)),
        block("roc_context_sma", "ROC Context, ma_kind 1 (SMA)",
              *roc_context_case(ps.wave_series(160), roc_len=6, avg_len=3, sig_len=5, ma_kind=1,
                                ob_level=2.0, os_level=-2.0, ctx_len=20, stall_len=4)),
        block("roc_context_rma", "ROC Context, ma_kind 2 (RMA)",
              *roc_context_case(ps.wave_series(160), roc_len=6, avg_len=3, sig_len=5, ma_kind=2,
                                ob_level=2.0, os_level=-2.0, ctx_len=20, stall_len=4)),
        block("roc_context_flat", "ROC Context on a flat series: every ROC is 0, nothing fires",
              *roc_context_case(flat_series(40), roc_len=4, ctx_len=10)),
    ]


# --- Smoothed Supertrend ---------------------------------------------------------------------
#
# Values and events from `smoothed_supertrend.compute`; `adx` = ADX(14, 14) (`pine_series.adx`)
# once it exists; `strength_rank` and `distance_rank` once their 100-value windows exist;
# `overextension` = +1 when the distance rank exceeds 85 with the close above the baseline, -1
# below it, 0 otherwise (also 0 before the distance rank exists).
# Alert strengths (assumed convention; the type documents none): fixed per kind, flips 0.7, plain
# rejections 0.5, strong rejections 0.8.

SST_STRENGTH = {"bull_flip": 0.7, "bear_flip": 0.7, "bull_rejection": 0.5, "bear_rejection": 0.5,
                "bull_rejection_strong": 0.8, "bear_rejection_strong": 0.8}


def smoothed_supertrend_case(bars, factor=12.0, atr_len=90, wma_len=40, ema_len=14,
                             adx_threshold=25.0, slope_lookback=3, slope_threshold=0.05,
                             touches=2):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(sst_ref, FACTOR=factor, ATR_LEN=atr_len, WMA_LEN=wma_len, EMA_LEN=ema_len,
                   ADX_THRESHOLD=adx_threshold, SLOPE_LB=slope_lookback,
                   SLOPE_TH=slope_threshold, TOUCHES=touches):
        out = sst_ref.compute(bars)
        overext_th = sst_ref.OVEREXT_TH
    adx = ps.adx([b[1] for b in bars], [b[2] for b in bars], [b[3] for b in bars], 14, 14)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t, bar in enumerate(bars):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        base = out["baseline"][t]
        extra = {"trend": float(out["trend"][t]), "st_lower": out["lower"][t],
                 "st_upper": out["upper"][t], "overextension": 0.0}
        if adx[t] is not None:
            extra["adx"] = adx[t]
        if out["strength"][t] is not None:
            extra["strength_rank"] = out["strength"][t]
        rank = out["dist_rank"][t]
        if rank is not None:
            extra["distance_rank"] = rank
            if rank > overext_th and bar[3] > base:
                extra["overextension"] = 1.0
            elif rank > overext_th and bar[3] < base:
                extra["overextension"] = -1.0
        expected.append(((base, extra), {k: SST_STRENGTH[k] for k in per_bar[t]}))
    params = {"factor": factor, "atr_len": float(atr_len), "wma_len": float(wma_len),
              "ema_len": float(ema_len), "adx_threshold": adx_threshold,
              "slope_lookback": float(slope_lookback), "slope_threshold": slope_threshold,
              "touches": float(touches)}
    return params, bars, expected


SST_SHORT = {"factor": 3.0, "atr_len": 14, "wma_len": 10, "ema_len": 5, "adx_threshold": 20.0,
             "slope_threshold": 0.1, "touches": 1}


def smoothed_supertrend_blocks():
    return [
        block("smoothed_supertrend_defaults",
              "Smoothed Supertrend, catalog defaults, wave series: flips, rejections, strong "
              "bear rejections",
              *smoothed_supertrend_case(ps.wave_series(400))),
        block("smoothed_supertrend_mirrored",
              "Smoothed Supertrend, defaults, the same wave reflected: strong bull rejections",
              *smoothed_supertrend_case(mirrored(ps.wave_series(400)))),
        block("smoothed_supertrend_short", "Smoothed Supertrend with short lengths, wave series",
              *smoothed_supertrend_case(ps.wave_series(260), **SST_SHORT)),
        block("smoothed_supertrend_lcg",
              "Smoothed Supertrend with short lengths and slope 5, pseudo-random walk",
              *smoothed_supertrend_case(ps.lcg_series(260), factor=5.0, atr_len=14, wma_len=10,
                                        ema_len=5, slope_lookback=5, touches=3)),
    ]


# --- Reaction Speed --------------------------------------------------------------------------
#
# Values from `reaction_speed.compute`. While an event is tracked and on its closing bar the
# output carries `event_dir` and `event_type` (1 BOS, 2 sweep, 3 OB), and `reaction`, `speed`,
# `validation` once a reaction candle has set them; the closing bar adds `final_score`. One event
# is tracked at a time, so the type of a tracked bar is the type of the next closed event; the
# cases end with no event open. No alerts.

RS_TYPE = {"bos": 1.0, "sweep": 2.0, "ob": 3.0}


def reaction_speed_case(bars, atr_len=14, atr_base=50, swing_len=10, react_bars=8,
                        valid_bars=20, atr_move=1.6):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(rs_ref, ATR_LEN=atr_len, ATR_BASE=atr_base, SWING=swing_len,
                   REACT_BARS=react_bars, VALID_BARS=valid_bars, ATR_MOVE=atr_move):
        out = rs_ref.compute(bars)
    finals = out["finals"]
    expected = []
    for t in range(len(bars)):
        if out["activity"][t] is None:
            expected.append((None, {}))
            continue
        extra = {}
        if out["event_dir"][t] is not None:
            closing = next(f for f in finals if f[0] >= t)
            extra["event_dir"] = out["event_dir"][t]
            extra["event_type"] = RS_TYPE[closing[1]]
            if out["reaction"][t] is not None:
                extra.update(reaction=out["reaction"][t], speed=out["speed"][t],
                             validation=out["validation"][t])
            if closing[0] == t:
                extra["final_score"] = closing[3]
        expected.append(((out["activity"][t], extra), {}))
    params = {"atr_len": float(atr_len), "atr_base": float(atr_base),
              "swing_len": float(swing_len), "react_bars": float(react_bars),
              "valid_bars": float(valid_bars), "atr_move": atr_move}
    return params, bars, expected


def reaction_speed_blocks():
    return [
        block("reaction_speed_defaults", "Reaction Speed, catalog defaults, wave series: BOS and "
              "sweep events", *reaction_speed_case(ps.wave_series(153))),
        block("reaction_speed_lcg", "Reaction Speed, catalog defaults, pseudo-random walk: an "
              "order-block tap as well", *reaction_speed_case(ps.lcg_series(159))),
        block("reaction_speed_short", "Reaction Speed with short windows: events expiring "
              "without a reaction (final score 0)",
              *reaction_speed_case(ps.lcg_series(150), atr_len=5, atr_base=10, swing_len=40,
                                   react_bars=2, valid_bars=3, atr_move=1.0)),
    ]


# --- Flow Exhaustion -------------------------------------------------------------------------
#
# Values and events from `flow_exhaustion.compute`: value = smoothed MFI, secondary = K (also
# `stoch_k`), `upper`/`lower` once the zones exist, `divergence_score` (signed) on a bar that
# emits a divergence. `pivot_len` sets both pivot sides. Strengths: a divergence carries its final
# score / 100 (documented), clamped to 0.05..1; an exhaustion alert has no graded quality and
# carries the fixed strength 0.6 (assumed convention).


def flow_exhaustion_case(bars, mfi_len=14, zone_lookback=200, rsi_len=14, stoch_len=14,
                         k_smooth=3, jma_len=20, confirm_bars=1, pivot_len=7, react_bars=10):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(fe_ref, MFI_LEN=mfi_len, ZONE_LOOKBACK=zone_lookback, RSI_LEN=rsi_len,
                   STOCH_LEN=stoch_len, K_SMOOTH=k_smooth, JMA_LEN=jma_len, CONFIRM=confirm_bars,
                   PIV_LEFT=pivot_len, PIV_RIGHT=pivot_len, REACT_BARS=react_bars):
        out = fe_ref.compute(bars)
    # The signed scores are listed in emission order; bull ones (+) belong to the bull events in
    # order, bear ones (-) to the bear events.
    scores = {"bull_divergence": [], "bear_divergence": []}
    for score in out["div_scores"]:
        scores["bull_divergence" if math.copysign(1.0, score) > 0 else "bear_divergence"].append(
            score)
    div_at = {}
    for kind in scores:
        assert len(scores[kind]) == len(out["events"][kind])
        for t, score in zip(out["events"][kind], scores[kind]):
            assert t not in div_at, "two divergences on one bar"
            div_at[t] = (kind, score)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        extra = {"stoch_k": out["k"][t]}
        if out["upper"][t] is not None:
            extra.update(upper=out["upper"][t], lower=out["lower"][t])
        alerts = {}
        for kind in per_bar[t]:
            if kind.endswith("_exhaustion"):
                alerts[kind] = 0.6
            else:
                score = div_at[t][1]
                extra["divergence_score"] = score
                alerts[kind] = ps.clamp(abs(score) / 100.0, 0.05, 1.0)
        expected.append(((out["mfi"][t], extra, out["k"][t]), alerts))
    params = {"mfi_len": float(mfi_len), "zone_lookback": float(zone_lookback),
              "rsi_len": float(rsi_len), "stoch_len": float(stoch_len),
              "k_smooth": float(k_smooth), "jma_len": float(jma_len),
              "confirm_bars": float(confirm_bars), "pivot_len": float(pivot_len),
              "react_bars": float(react_bars)}
    return params, bars, expected


FE_SHORT = {"mfi_len": 10, "zone_lookback": 40, "rsi_len": 8, "stoch_len": 8, "jma_len": 5,
            "pivot_len": 4, "react_bars": 6}


def late_volume(bars, silent):
    """The first `silent` bars without volume."""
    return [b[:4] + (0.0,) if i < silent else b for i, b in enumerate(bars)]


def flow_exhaustion_blocks():
    return [
        block("flow_exhaustion_defaults", "Flow Exhaustion, catalog defaults, wave series: "
              "exhaustion both ways, a bear divergence",
              *flow_exhaustion_case(ps.wave_series(330))),
        block("flow_exhaustion_mirrored", "Flow Exhaustion, defaults, the wave reflected: a bull "
              "divergence", *flow_exhaustion_case(mirrored(ps.wave_series(330)))),
        block("flow_exhaustion_short", "Flow Exhaustion with short lengths, wave series",
              *flow_exhaustion_case(ps.wave_series(200), **FE_SHORT)),
        block("flow_exhaustion_lag", "Flow Exhaustion, pivots 4 against a 3-bar reaction window "
              "(the reaction waits for the pivot), no zone confirmation, pseudo-random walk",
              *flow_exhaustion_case(ps.lcg_series(300, 7), **{**FE_SHORT, "react_bars": 3,
                                                              "confirm_bars": 0})),
        block("flow_exhaustion_late_volume", "Flow Exhaustion, short lengths: no volume on the "
              "first 25 bars, so no MFI until a volume enters its window",
              *flow_exhaustion_case(late_volume(ps.wave_series(90), 25), **FE_SHORT)),
        block("flow_exhaustion_no_volume", "Flow Exhaustion without any volume: no output",
              *flow_exhaustion_case(ps.wave_series(60, with_volume=False), **FE_SHORT)),
    ]


# --- Coil Release ----------------------------------------------------------------------------
#
# Values and events from `coil_release.compute`; `corr_high`/`corr_low` = highest high / lowest
# low of the `break_len` bars before the current one; `was_base` = 1 when at least
# round(break_len * base_share) (half away from zero) of those bars are base bars (complexity
# above complex_high and efficiency below eff_low) — the golden module's reading of "at least
# base_share of the previous bars". The golden module keeps complexity and efficiency of the bars
# before the first report local; a recording wrapper around `pine_series.sma` captures the two
# full series it smooths (its 3-bar means), so the base bars are classified here from them.
# Strength (assumed convention; the type documents none): the setup score clamped to 0.05..1.


def coil_release_case(bars, length=34, break_len=12, ema_len=55, slope_len=5, thr_cont=0.45,
                      thr_base=0.55, cooldown=8, chop_veto=61.8):
    bars = [tuple(float(x) for x in b) for b in bars]
    smoothed = []
    plain_sma = ps.sma

    def recording_sma(series, n):
        smoothed.append(series)
        return plain_sma(series, n)

    with constants(cr_ref, LEN=length, BREAK_LEN=break_len, EMA_LEN=ema_len, SLOPE_LEN=slope_len,
                   THR_CONT=thr_cont, THR_BASE=thr_base, COOLDOWN=cooldown, CHOP_VETO=chop_veto), \
            constants(ps, sma=recording_sma):
        out = cr_ref.compute(bars)
        complex_high, eff_low, share = cr_ref.COMPLEX_HIGH, cr_ref.EFF_LOW, cr_ref.BASE_SHARE
    complexity, efficiency = smoothed
    base = [cm is not None and ef is not None and cm > complex_high and ef < eff_low
            for cm, ef in zip(complexity, efficiency)]
    highs = ps.highest([b[1] for b in bars], break_len)
    lows = ps.lowest([b[2] for b in bars], break_len)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        setup = out["setup"][t]
        if setup is None:
            expected.append((None, {}))
            continue
        base_bars = sum(1 for k in range(1, break_len + 1) if base[t - k])
        extra = {"complexity": out["complexity"][t], "fractal_dim": out["fd"][t],
                 "efficiency": out["eff"][t], "chop": out["chop"][t], "coil": out["coil"][t],
                 "coil_score": out["coil_score"][t], "release_dyn": out["release_dyn"][t],
                 "trend": out["trend"][t], "corr_high": highs[t - 1], "corr_low": lows[t - 1],
                 "was_base": 1.0 if base_bars >= ps.pine_round(break_len * share) else 0.0}
        alerts = {kind: ps.clamp(setup, 0.05, 1.0) for kind in per_bar[t]}
        expected.append(((setup, extra), alerts))
    params = {"len": float(length), "break_len": float(break_len), "ema_len": float(ema_len),
              "slope_len": float(slope_len), "thr_cont": thr_cont, "thr_base": thr_base,
              "cooldown": float(cooldown), "chop_veto": chop_veto}
    return params, bars, expected


def coil_release_blocks():
    return [
        block("coil_release_defaults", "Coil Release, catalog defaults, pseudo-random walk (seed "
              "3): bear releases, base breaks both ways",
              *coil_release_case(ps.lcg_series(400, 3))),
        block("coil_release_mirrored", "Coil Release, defaults, the same walk reflected: bull "
              "releases", *coil_release_case(mirrored(ps.lcg_series(400, 3)))),
        block("coil_release_short", "Coil Release with short windows, low thresholds and no "
              "cooldown, wave series",
              *coil_release_case(ps.wave_series(200), length=16, break_len=6, ema_len=20,
                                 slope_len=3, thr_cont=0.3, thr_base=0.35, cooldown=0,
                                 chop_veto=70.0)),
    ]


# --- Compression Zones -----------------------------------------------------------------------
#
# Values, every extra and the events from `compression_zones.compute` (`range_atr` only while
# the ATR is positive, `zone_high`/`zone_low` only in a zone). Strength (assumed convention; the
# type documents none): a breakout carries the fixed strength 0.7.


def compression_zones_case(bars, lookback=16, min_zone_bars=5, range_atr=4.2, adx_max=27.0,
                           min_score=35.0, min_touches=2, watch_bars=20):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(cz_ref, LOOKBACK=lookback, MIN_ZONE_BARS=min_zone_bars, RANGE_ATR=range_atr,
                   ADX_MAX=adx_max, MIN_SCORE=min_score, MIN_TOUCHES=min_touches,
                   WATCH=watch_bars):
        out = cz_ref.compute(bars)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        if out["score"][t] is None:
            expected.append((None, {}))
            continue
        extra = {k: out[k][t] for k in ("in_zone", "upper_touches", "lower_touches", "read",
                                        "armed", "hist_reuse")}
        if out["range_atr"][t] is not None:
            extra["range_atr"] = out["range_atr"][t]
        if out["in_zone"][t] == 1.0:
            extra.update(zone_high=out["zone_high"][t], zone_low=out["zone_low"][t])
        expected.append(((out["score"][t], extra), {k: 0.7 for k in per_bar[t]}))
    params = {"lookback": float(lookback), "min_zone_bars": float(min_zone_bars),
              "range_atr": range_atr, "adx_max": adx_max, "min_score": min_score,
              "min_touches": float(min_touches), "watch_bars": float(watch_bars)}
    return params, bars, expected


def without_volume(bars):
    return [b[:4] + (0.0,) for b in bars]


def compression_zones_blocks():
    short = {"lookback": 10, "min_zone_bars": 3, "range_atr": 3.0, "adx_max": 30.0,
             "min_score": 30.0, "min_touches": 3, "watch_bars": 8}
    return [
        block("compression_zones_defaults", "Compression Zones, catalog defaults, pseudo-random "
              "walk (seed 1): zones and breakouts both ways",
              *compression_zones_case(ps.lcg_series(300, 1))),
        block("compression_zones_short", "Compression Zones with a short window, three touches "
              "and a short watch, wave series", *compression_zones_case(ps.wave_series(250),
                                                                       **short)),
        block("compression_zones_no_volume", "Compression Zones, defaults, the walk without "
              "volume: the dry-up module counts, the volume check is skipped",
              *compression_zones_case(without_volume(ps.lcg_series(300, 1)))),
    ]


# --- Price-Average Relation ------------------------------------------------------------------
#
# Values and events from `price_average_relation.compute`: value = relationship, secondary =
# its signal (also `signal`), the six scores, `ma`, `distance_atr`, `respect_samples`. Two fields
# the golden module computes but does not return are taken as follows:
# - `direction`: the golden module's own series, recorded from its `_map` call whose function
#   takes (agreement, side, ma direction) — identified by the parameter names `ag, s, d`;
# - `respect_hit_rate`: 100 x the share of successful touch episodes among those counted in
#   `respect_samples` (50 without episodes): an episode starts on a close within proximity x ATR
#   of the MA after a bar outside it, between `respect_response` and `respect_lookback` bars back,
#   and succeeds when `respect_response` bars later the close is on the MA side of the MA slope at
#   the touch and has progressed in that direction. Recomputed here, as in the golden module.
# `ma_kind` stays 0 (EMA): the golden module reads the MA from the first bar on, which only the
# first-sample EMA provides, so the SMA/RMA/ALMA families are not specified here.
# Strength (assumed convention; the type documents none): |relationship| / 100 clamped to
# 0.05..1 for every kind.


def respect_hit_rates(bars, ma, atr, lookback, response, proximity):
    c = [b[3] for b in bars]
    out = [None] * len(bars)
    for t in range(len(bars)):
        oldest = t - (lookback + 1)
        if oldest < 0 or atr[oldest] is None:
            continue
        success = episodes = 0
        for i in range(response, lookback + 1):
            k = t - i
            near = abs(c[k] - ma[k]) <= atr[k] * proximity
            near_before = abs(c[k - 1] - ma[k - 1]) <= atr[k - 1] * proximity
            if near and not near_before:
                episodes += 1
                slope_side = 1.0 if ma[k] - ma[k - 1] >= 0 else -1.0
                q = k + response
                progress = par_ref._safe_div((c[q] - c[k]) * slope_side, atr[q])
                if (c[q] - ma[q]) * slope_side > 0 and progress > 0:
                    success += 1
        out[t] = 100.0 * success / episodes if episodes else 50.0
    return out


def price_average_relation_case(bars, ma_len=50, atr_len=14, cooldown=8, turn_level=55.0,
                                turn_lookback=30):
    bars = [tuple(float(x) for x in b) for b in bars]
    plain_map = par_ref._map
    recorded = {}

    def recording_map(f, *series):
        result = plain_map(f, *series)
        code = getattr(f, "__code__", None)
        if code is not None and code.co_varnames[:code.co_argcount] == ("ag", "s", "d"):
            recorded["direction"] = result
        return result

    with constants(par_ref, MA_LEN=ma_len, ATR_LEN=atr_len, COOLDOWN=cooldown,
                   TURN_LEVEL=turn_level, TURN_LOOKBACK=turn_lookback, _map=recording_map):
        out = par_ref.compute([b[:4] for b in bars])
        lookback, response, proximity = (par_ref.RESPECT_LOOKBACK, par_ref.RESPECT_RESPONSE,
                                         par_ref.PROXIMITY)
    atr = par_ref._rma(ps.true_range([b[1] for b in bars], [b[2] for b in bars],
                                     [b[3] for b in bars]), atr_len)
    hit = respect_hit_rates(bars, out["ma"], atr, lookback, response, proximity)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        if not out["reported"][t]:
            expected.append((None, {}))
            continue
        rel = out["relationship"][t]
        extra = {k: out[k][t] for k in ("trend", "respect", "extension", "compression",
                                        "acceleration", "exhaustion", "ma", "distance_atr",
                                        "respect_samples")}
        extra.update(signal=out["signal"][t], direction=recorded["direction"][t],
                     respect_hit_rate=hit[t])
        alerts = {k: ps.clamp(abs(rel) / 100.0, 0.05, 1.0) for k in per_bar[t]}
        expected.append(((rel, extra, out["signal"][t]), alerts))
    params = {"ma_len": float(ma_len), "ma_kind": 0.0, "atr_len": float(atr_len),
              "cooldown": float(cooldown), "turn_level": turn_level,
              "turn_lookback": float(turn_lookback)}
    return params, bars, expected


PAR_SHORT = {"ma_len": 20, "atr_len": 7, "cooldown": 0, "turn_level": 40.0, "turn_lookback": 10}


def price_average_relation_blocks():
    trends = ps.wave_series(760)[420:]
    tiring = ps.wave_series(440)[260:]
    return [
        block("price_average_relation_defaults", "Price-Average Relation, catalog defaults, wave "
              "bars 420-759: range, momentum release, pullbacks, chase, break, divergence, turns",
              *price_average_relation_case(trends)),
        block("price_average_relation_mirrored", "Price-Average Relation, defaults, the same bars "
              "reflected", *price_average_relation_case(mirrored(trends))),
        block("price_average_relation_tiring_down", "Price-Average Relation, defaults, wave bars "
              "260-439: a downtrend tiring (bull exhaustion)",
              *price_average_relation_case(tiring)),
        block("price_average_relation_tiring_up", "Price-Average Relation, defaults, the "
              "same bars reflected (bear exhaustion)",
              *price_average_relation_case(mirrored(tiring))),
        block("price_average_relation_short", "Price-Average Relation, MA 20, ATR 7, no cooldown, "
              "turn level 40 over 10 bars, pseudo-random walk",
              *price_average_relation_case(ps.lcg_series(300), **PAR_SHORT)),
    ]


# --- Overextension Risk ----------------------------------------------------------------------
#
# Values and (with confirm 1) events from `overextension_risk.compute`: value = top score,
# secondary = bottom score, the six role scores, `vol_factor`, the thresholds and `regime`.
# Fields the golden module keeps local:
# - `adx` = ADX(14, 14) (`pine_series.adx`);
# - `money_flow` = 1 when real volume feeds the flow part: cumulative volume above 0 and the
#   30-bar volume SMA existing and positive;
# - `bear_div_active`/`bull_div_active`: the golden module's WaveTrend divergence flags per bar,
#   recorded from its `pine_series.barssince` calls (bear series first, then bull); a divergence
#   stays active for `div_memory` (12) bars after its confirmation while the bar's high (low)
#   stays at or below (above) the pivot price, the high (low) `pivot` (5) bars before it.
# `confirm` 0 is not in the golden module; here a side signals on the first bar whose score is
# at or above its threshold after a bar that was not (a bar without output counts as not), with
# the same gap rule and the same kinds by regime as confirm 1 (assumed reading of "score first
# reaching the threshold").
# Strength (assumed convention; the type documents none): the side's score / 100 clamped to
# 0.05..1.

OER_TOP = {1.0: "do_not_chase_up", -1.0: "bear_trend_exhaustion", 0.0: "bear_range_fade"}
OER_BOT = {-1.0: "do_not_chase_down", 1.0: "bull_trend_exhaustion", 0.0: "bull_range_fade"}


def overextension_signals(out, n, gap):
    """Events of confirm 0 from the per-bar scores and thresholds."""
    events = {kind: [] for kind in list(OER_TOP.values()) + list(OER_BOT.values())}
    for score, thresh, kinds in (("top", "top_thresh", OER_TOP), ("bot", "bot_thresh", OER_BOT)):
        last = None
        for t in range(n):
            if out[score][t] is None:
                continue
            now = out[score][t] >= out[thresh][t]
            before = t > 0 and out[score][t - 1] is not None and \
                out[score][t - 1] >= out[thresh][t - 1]
            if now and not before and (last is None or t - last > gap):
                last = t
                events[kinds[out["regime"][t]]].append(t)
    return events


def overextension_risk_case(bars, w_stretch=40.0, w_exhaust=35.0, w_react=25.0,
                            base_threshold=40.0, confirm=1, min_gap=8, ema_len=50,
                            structure_len=80):
    bars = [tuple(float(x) for x in b) for b in bars]
    n = len(bars)
    flags = []
    plain_barssince = ps.barssince

    def recording_barssince(cond, t):
        if not any(cond is f for f in flags):
            flags.append(cond)
        return plain_barssince(cond, t)

    with constants(oer_ref, W_STRETCH=w_stretch, W_EXHAUST=w_exhaust, W_REACT=w_react,
                   BASE_THRESH=base_threshold, GAP=min_gap, EMA_LEN=ema_len,
                   STRUCT_LEN=structure_len), constants(ps, barssince=recording_barssince):
        out = oer_ref.compute(bars)
        memory, pivot = oer_ref.DIV_MEMORY, oer_ref.PIVOT
    events = out["events"] if confirm == 1 else overextension_signals(out, n, min_gap)
    bear_div, bull_div = flags
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    v = [b[4] for b in bars]
    adx = ps.adx(h, lo, [b[3] for b in bars], 14, 14)
    vol_ma = ps.sma(v, 30)

    def active(div, t, holds):
        last = ps.barssince(div, t)
        return last is not None and last <= memory and holds(t - last - pivot, t)

    per_bar = events_at(events, n)
    expected = []
    for t in range(n):
        top = out["top"][t]
        if top is None:
            expected.append((None, {}))
            continue
        extra = {k: out[k][t] for k in ("stretch_top", "stretch_bot", "exhaust_top",
                                        "exhaust_bot", "react_top", "react_bot", "vol_factor",
                                        "top_thresh", "bot_thresh", "regime")}
        flow = sum(v[:t + 1]) > 0.0 and vol_ma[t] is not None and vol_ma[t] > 0.0
        extra.update(
            adx=adx[t], money_flow=1.0 if flow else 0.0,
            bear_div_active=1.0 if active(bear_div, t, lambda k, i: h[i] <= h[k]) else 0.0,
            bull_div_active=1.0 if active(bull_div, t, lambda k, i: lo[i] >= lo[k]) else 0.0)
        alerts = {}
        for kind in per_bar[t]:
            side = out["top"] if kind in OER_TOP.values() else out["bot"]
            alerts[kind] = ps.clamp(side[t] / 100.0, 0.05, 1.0)
        expected.append(((top, extra, out["bot"][t]), alerts))
    params = {"w_stretch": w_stretch, "w_exhaust": w_exhaust, "w_react": w_react,
              "base_threshold": base_threshold, "confirm": float(confirm),
              "min_gap": float(min_gap), "ema_len": float(ema_len),
              "structure_len": float(structure_len)}
    return params, bars, expected


OER_LOW = {"w_stretch": 50.0, "w_exhaust": 30.0, "w_react": 20.0, "base_threshold": 25.0,
           "min_gap": 2, "ema_len": 20, "structure_len": 30}


def overextension_risk_blocks():
    return [
        block("overextension_risk_defaults", "Overextension Risk, catalog defaults, wave series: "
              "chase-up warnings", *overextension_risk_case(ps.wave_series(260))),
        block("overextension_risk_range", "Overextension Risk, catalog defaults, pseudo-random "
              "walk: range fades both ways", *overextension_risk_case(ps.lcg_series(330))),
        block("overextension_risk_low", "Overextension Risk, weights 50/30/20, threshold 25, gap "
              "2, EMA 20, structure 30, pseudo-random walk (seed 5): every kind",
              *overextension_risk_case(ps.lcg_series(300, 5), **OER_LOW)),
        block("overextension_risk_first_touch", "Overextension Risk, the same parameters with "
              "confirm 0 (first bar at the threshold), wave series",
              *overextension_risk_case(ps.wave_series(200), confirm=0, **OER_LOW)),
        block("overextension_risk_no_volume", "Overextension Risk, the same parameters without "
              "volume: no money-flow part",
              *overextension_risk_case(without_volume(ps.lcg_series(200, 5)), **OER_LOW)),
    ]


# --- Tradability Filter ----------------------------------------------------------------------
#
# Values, state codes and events from `tradability_filter.compute`: value = tradability,
# secondary = direction (also `direction`), `balance`, `compression`, `state_code`,
# `candidate_code`. The plain inputs it keeps local are recomputed from their definitions:
# `efficiency` = |close - close[q]| / sum of |close changes| over q = quality_len bars (0 below a
# path of 1e-7), `fast_efficiency` the same over 8 bars, `adx` = ADX(14, 14), `range_high` /
# `range_low` = highest high / lowest low of the `range_len` bars before the current one,
# `range_position` = (close - range_low) / (range_high - range_low) clamped to 0..1 (0.5 for a
# zero width), `atr_ratio` = ATR(14) / EMA(2 quality_len) of the ATR (1 for a zero reference).
# The short window is max(3, quality_len // 4), as in the golden module.
# Strength (assumed convention; the type documents none): the bar's tradability / 100 clamped to
# 0.05..1 (0.05 when there is none).


def tradability_filter_case(bars, quality_len=20, range_len=30, baseline_len=34, rank_len=150,
                            balance_threshold=58.0, compression_threshold=62.0,
                            max_range_atr=7.0, confirm_bars=2):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(tf_ref, Q=quality_len, RANGE=range_len, BASE=baseline_len, RANK=rank_len,
                   BAL_TH=balance_threshold, COMP_TH=compression_threshold,
                   MAX_RANGE=max_range_atr, CONFIRM=confirm_bars,
                   SHORT=max(3, quality_len // 4)):
        out = tf_ref.compute(bars)
        fast = tf_ref.FAST_EFF
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    atr = ps.atr(h, lo, c, 14)
    atr_ref = ps.ema(atr, 2 * quality_len)
    adx = ps.adx(h, lo, c, 14, 14)

    def efficiency(t, n):
        path = sum(abs(c[k] - c[k - 1]) for k in range(t - n + 1, t + 1))
        return tf_ref.sdiv(abs(c[t] - c[t - n]), path, 0.0)

    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        trad = out["tradability"][t]
        alerts = {k: ps.clamp((trad or 0.0) / 100.0, 0.05, 1.0) for k in per_bar[t]}
        if not out["reported"][t]:
            expected.append((None, alerts))
            continue
        r_hi, r_lo = max(h[t - range_len:t]), min(lo[t - range_len:t])
        extra = {"balance": out["balance"][t], "compression": out["compression"][t],
                 "direction": out["direction"][t], "state_code": out["state"][t],
                 "candidate_code": out["candidate"][t], "efficiency": efficiency(t, quality_len),
                 "fast_efficiency": efficiency(t, fast), "adx": adx[t], "range_high": r_hi,
                 "range_low": r_lo,
                 "range_position": ps.clamp(tf_ref.sdiv(c[t] - r_lo, r_hi - r_lo, 0.5), 0.0, 1.0),
                 "atr_ratio": tf_ref.sdiv(atr[t], atr_ref[t], 1.0)}
        expected.append(((trad, extra, out["direction"][t]), alerts))
    params = {"quality_len": float(quality_len), "range_len": float(range_len),
              "baseline_len": float(baseline_len), "rank_len": float(rank_len),
              "balance_threshold": balance_threshold,
              "compression_threshold": compression_threshold, "max_range_atr": max_range_atr,
              "confirm_bars": float(confirm_bars)}
    return params, bars, expected


TF_SHORT = {"quality_len": 10, "range_len": 12, "baseline_len": 15, "rank_len": 60}


def tradability_filter_blocks():
    return [
        block("tradability_filter_defaults", "Tradability Filter, catalog defaults, "
              "pseudo-random walk (seed 11): breakouts, trends, exhaustion both ways",
              *tradability_filter_case(ps.lcg_series(400, 11))),
        block("tradability_filter_short", "Tradability Filter with short windows (quality 10, "
              "range 12, baseline 15, rank 60), wave series: ready, aftermath",
              *tradability_filter_case(ps.wave_series(300), **TF_SHORT)),
        block("tradability_filter_loose", "Tradability Filter, short windows, balance 50, "
              "compression 55, range 9 ATR, confirm 1, pseudo-random walk (seed 1)",
              *tradability_filter_case(ps.lcg_series(300, 1), balance_threshold=50.0,
                                       compression_threshold=55.0, max_range_atr=9.0,
                                       confirm_bars=1, **TF_SHORT)),
    ]


# --- Market State Chain ----------------------------------------------------------------------
#
# Values and events from `market_state_chain.compute`: value = p_bull - p_bear, `state_code`,
# the three probabilities, `reliability`, `bias`, `exits`, `persistence` and `dwell_ratio` when
# they exist. The classification inputs the golden module keeps local are recomputed from their
# definitions: `atr_rank` / `bb_rank` = min-max position of ATR(14) / Bollinger width
# (100 sd(20) / SMA(20)) over rank_len bars (50 for a flat window), `efficiency` = 100 |net
# move| / path over 34 bars (0 without a path), `adx` = ADX(14, 14); `context_state` = the state,
# except compression split by the context (6 up: close above EMA(ema_len) and its 5-bar slope in
# ATR units above 0.05; 7 down: mirrored; else 2); `bars_in_state` = bars since the last
# committed state change (0 before the first).
# Strength (assumed convention; the type documents none): reliability / 100 clamped to 0.05..1.


def market_state_chain_case(bars, ema_len=55, rank_len=100, lookback=500, min_dwell=2,
                            min_samples=20, conf_th=55.0, rel_th=40.0, bias_margin=8.0):
    bars = [tuple(float(x) for x in b) for b in bars]
    with constants(msc_ref, EMA_LEN=ema_len, RANK_LEN=rank_len, LOOKBACK=lookback,
                   MIN_DWELL=min_dwell, MIN_SAMPLES=min_samples, BIAS_MARGIN=bias_margin):
        out = msc_ref.compute(bars, conf_th, rel_th)
        eff_len, slope_th = msc_ref.EFF_LEN, msc_ref.SLOPE_TH
    h = [b[1] for b in bars]
    lo = [b[2] for b in bars]
    c = [b[3] for b in bars]
    ema = ps.ema(c, ema_len)
    atr = ps.atr(h, lo, c, 14)
    adx = ps.adx(h, lo, c, 14, 14)
    width = ps.smap(lambda d, b: d / b * 100 if b != 0 else 0.0, ps.stdev(c, 20), ps.sma(c, 20))

    def rank(series, t):
        window = series[t - rank_len + 1:t + 1]
        low, high = min(window), max(window)
        return (series[t] - low) / (high - low) * 100 if high != low else 50.0

    per_bar = events_at(out["events"], len(bars))
    last_change = None
    expected = []
    for t in range(len(bars)):
        if "regime_change" in per_bar[t]:
            last_change = t
        if out["state"][t] is None:
            expected.append((None, {}))
            continue
        state = out["state"][t]
        slope = (ema[t] - ema[t - 5]) / atr[t] if atr[t] != 0 else 0.0
        context = state
        if state == 2.0:
            context = 6.0 if c[t] > ema[t] and slope > slope_th else (
                7.0 if c[t] < ema[t] and slope < -slope_th else 2.0)
        path = sum(abs(c[t - i] - c[t - i - 1]) for i in range(eff_len - 1))
        extra = {k: out[k][t] for k in ("p_bull", "p_bear", "p_neutral", "reliability", "bias",
                                        "exits")}
        extra.update(state_code=state, context_state=context,
                     bars_in_state=float(t - last_change if last_change is not None else 0),
                     atr_rank=rank(atr, t), bb_rank=rank(width, t),
                     efficiency=ps.clamp(abs(c[t] - c[t - eff_len + 1]) / path * 100, 0.0, 100.0)
                     if path > 0 else 0.0, adx=adx[t])
        for key in ("persistence", "dwell_ratio"):
            if out[key][t] is not None:
                extra[key] = out[key][t]
        strength = ps.clamp(out["reliability"][t] / 100.0, 0.05, 1.0)
        expected.append(((out["net"][t], extra), {k: strength for k in per_bar[t]}))
    params = {"ema_len": float(ema_len), "rank_len": float(rank_len), "lookback": float(lookback),
              "min_dwell": float(min_dwell), "min_samples": float(min_samples),
              "conf_th": conf_th, "rel_th": rel_th, "bias_margin": bias_margin}
    return params, bars, expected


MSC_SHORT = {"ema_len": 10, "rank_len": 20, "lookback": 50, "min_samples": 5}
MSC_OPEN = {**MSC_SHORT, "conf_th": 0.0, "rel_th": 0.0, "bias_margin": 0.0}


def market_state_chain_blocks():
    return [
        block("market_state_chain_defaults", "Market State Chain, catalog defaults, wave series: "
              "state changes and extended states, no edge",
              *market_state_chain_case(ps.wave_series(300))),
        block("market_state_chain_short", "Market State Chain, EMA 10, rank 20, lookback 50, "
              "5 samples, dwell 3, confidence 30, reliability 0, wave series: breakouts and "
              "reversals both ways",
              *market_state_chain_case(ps.wave_series(300), min_dwell=3, conf_th=30.0,
                                       rel_th=0.0, **MSC_SHORT)),
        block("market_state_chain_open", "Market State Chain, short windows without confidence, "
              "reliability or margin, dwell 1, pseudo-random walk (seed 3): reversals and a "
              "continuation", *market_state_chain_case(ps.lcg_series(300, 3), min_dwell=1,
                                                       **MSC_OPEN)),
        block("market_state_chain_continuations", "Market State Chain, the same parameters "
              "with dwell 1, pseudo-random walk (seed 28): continuations both ways",
              *market_state_chain_case(ps.lcg_series(300, 28), min_dwell=1, **MSC_OPEN)),
    ]


# --- RSI Cycle Statistics --------------------------------------------------------------------
#
# Values and events from `rsi_cycle_stats.compute`: value = RSI, `cycle_dir`, and during a cycle
# `progress`, `cond_n`, `conditional` and `edge` when they exist. The golden module evaluates the
# live cycle's statistics on every bar through its own `statistics(samples, t, direction,
# max_progress, age)`, `base_rate` and `wilson` but returns only part of them; recording wrappers
# around `statistics` and `base_rate` keep each bar's calls, the last of which belong to the live
# cycle after the bar. From those: `max_progress` and `age` (the arguments), `median_duration`
# (when it exists), `base_rate` (when it exists), `ci_low`/`ci_high` = the Wilson interval of the
# conditional sample (when it is not empty).
# Strengths: an opportunity carries the edge it fired with (the call before the live-cycle call
# on that bar) / 100, clamped to 0.05..1; cycle outcomes have no graded quality and carry 0.5.
# Both assumed conventions; the type documents none.


def rsi_cycle_stats_case(bars, rsi_len=14, overbought=70.0, oversold=30.0, max_cycle=150,
                         lookback=1000, min_samples=20, edge_th=15.0, max_ci=40.0):
    bars = [tuple(float(x) for x in b) for b in bars]
    plain_statistics, plain_base_rate = rcs_ref.statistics, rcs_ref.base_rate
    stats_calls, base_calls = {}, {}

    def recording_statistics(samples, t, direction, progress, age):
        result = plain_statistics(samples, t, direction, progress, age)
        stats_calls.setdefault(t, []).append((progress, age, result))
        return result

    def recording_base_rate(samples, t, direction):
        result = plain_base_rate(samples, t, direction)
        base_calls.setdefault(t, []).append(result)
        return result

    with constants(rcs_ref, RSI_LEN=rsi_len, OB=overbought, OS=oversold, MAX_CYCLE=max_cycle,
                   LOOKBACK=lookback, EDGE_TH=edge_th, statistics=recording_statistics,
                   base_rate=recording_base_rate):
        out = rcs_ref.compute(bars, min_samples, max_ci)
    per_bar = events_at(out["events"], len(bars))
    expected = []
    for t in range(len(bars)):
        if out["osc"][t] is None:
            expected.append((None, {}))
            continue
        extra = {"cycle_dir": out["dir"][t]}
        if out["dir"][t] != 0.0:
            max_progress, age, (success, total, median) = stats_calls[t][-1]
            base = base_calls[t][-1]
            extra.update(progress=out["progress"][t], max_progress=max_progress,
                         age=float(age), cond_n=out["cond_n"][t])
            low, high = rcs_ref.wilson(success, total)
            for key, value in (("conditional", out["conditional"][t]), ("base_rate", base),
                               ("edge", out["edge"][t]), ("ci_low", low), ("ci_high", high),
                               ("median_duration", median)):
                if value is not None:
                    extra[key] = value
        alerts = {}
        for kind in per_bar[t]:
            if kind.endswith("_opportunity"):
                _, _, (success, total, _) = stats_calls[t][-2]
                edge = 100.0 * success / total - base_calls[t][-2]
                alerts[kind] = ps.clamp(edge / 100.0, 0.05, 1.0)
            else:
                alerts[kind] = 0.5
        expected.append(((out["osc"][t], extra), alerts))
    params = {"rsi_len": float(rsi_len), "overbought": overbought, "oversold": oversold,
              "max_cycle": float(max_cycle), "lookback": float(lookback),
              "min_samples": float(min_samples), "edge_th": edge_th, "max_ci": max_ci}
    return params, bars, expected


RCS_SHORT = {"rsi_len": 5, "overbought": 60.0, "oversold": 40.0, "max_cycle": 30,
             "lookback": 100, "min_samples": 3, "edge_th": 5.0, "max_ci": 80.0}


def rsi_cycle_stats_blocks():
    return [
        block("rsi_cycle_stats_defaults", "RSI Cycle Statistics, catalog defaults, wave series: "
              "successes and failures, too few samples for an opportunity",
              *rsi_cycle_stats_case(ps.wave_series(250))),
        block("rsi_cycle_stats_short", "RSI Cycle Statistics, RSI 5, zones 60/40, cycles of at "
              "most 30 bars, lookback 100, 3 samples, edge 5, width 80, pseudo-random walk: "
              "opportunities both ways, timeouts",
              *rsi_cycle_stats_case(ps.lcg_series(400, 1), **RCS_SHORT)),
    ]


def build():
    header = provenance("contract_composites_a") + [
        "",
        "Format: see contract_pilot; out<i>_secondary is the expected `secondary` where the",
        "indicator has one.",
    ]
    blocks = (roc_context_blocks() + smoothed_supertrend_blocks()
              + reaction_speed_blocks() + flow_exhaustion_blocks()
              + coil_release_blocks() + compression_zones_blocks()
              + price_average_relation_blocks() + overextension_risk_blocks()
              + tradability_filter_blocks() + market_state_chain_blocks()
              + rsi_cycle_stats_blocks())
    return header, blocks
