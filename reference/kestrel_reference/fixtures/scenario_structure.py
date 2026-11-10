"""scenario_structure: pivot structure, liquidity pools and the ZigZags over the hand-built
sequences of the structure scenarios, from the definitions documented on their types."""

from ..fixture import provenance
from ..structure import (advanced_zigzag, liquidity_pools, pivot_sequence_error,
                         pivot_structure_scores)
from .contract_structure import zigzag as simple_zigzag

NAME = "scenario_structure"

# (open, high, low, close), exactly as in tests/scenario_reference_structure.rs.
POOLS = [
    (100.0, 102.0, 98.0, 101.0), (102.0, 106.0, 101.0, 105.0), (105.0, 110.0, 104.0, 108.0),
    (107.0, 108.0, 102.0, 104.0), (103.0, 105.0, 100.0, 102.0), (102.0, 107.0, 101.0, 106.0),
    (106.0, 110.2, 105.0, 109.0), (108.0, 108.0, 103.0, 104.0), (103.0, 105.0, 101.0, 102.0),
    (103.0, 111.5, 102.0, 108.0),
]
PIVOTS = [
    (100.0, 102.0, 98.0, 101.0), (102.0, 105.0, 101.0, 104.0), (105.0, 110.0, 104.0, 108.0),
    (106.0, 107.0, 102.0, 103.0), (103.0, 104.0, 100.0, 102.0), (102.0, 108.0, 101.0, 107.0),
    (107.0, 115.0, 106.0, 114.0), (114.0, 120.0, 113.0, 118.0), (116.0, 117.0, 110.0, 112.0),
    (112.0, 113.0, 108.0, 110.0),
]
ZIGZAG_CLOSES = [100.0, 105.0, 115.0, 110.0, 105.0, 98.0, 92.0, 90.0, 95.0, 102.0, 110.0, 118.0,
                 125.0, 120.0, 115.0]
ZIGZAG = [(c, c + 0.5, c - 0.5, c) for c in ZIGZAG_CLOSES]

# (high, low) of the gap scenario, exactly as in tests/scenario_reference_structure.rs; open and
# close are the middle of the bar.
ZIGZAG_GAP_SPANS = (
    # Rise to a high of 120, decline to a low of 100.
    [(106.0 + 2.0 * k, 104.0 + 2.0 * k) for k in range(8)]
    + [(120.0 - 4.0 * k, 118.0 - 3.6 * k) for k in range(1, 6)]
    # Four inside bars: no pivot, the low of 100 stays the running node.
    + [(102.0 - 0.2 * k, 100.0 + 0.2 * k) for k in range(1, 5)]
    # Gap down. The highs rise to 83 and fall again, the lows fall throughout.
    + [(81.0, 79.5), (81.5, 79.3), (82.0, 79.2), (83.0, 79.0), (82.5, 78.9), (82.0, 78.8),
       (81.5, 78.7), (81.0, 78.6), (80.5, 78.5), (80.0, 78.4)]
    # The lows turn at 78.4; a rise to 82.4 and three lower highs.
    + [(80.6, 78.9), (81.2, 79.4), (81.8, 80.0), (82.4, 80.6), (81.9, 80.2), (81.4, 79.8),
       (80.9, 79.4)]
)
# Bars up to the last one of the falling lows: the pivot low of 78.4 is not yet known.
ZIGZAG_GAP_CUT = 27


def gap_bars(mirrored):
    """The gap scenario as (open, high, low, close); mirrored: every price negated, so highs and
    lows change places."""
    spans = [(-low, -high) for high, low in ZIGZAG_GAP_SPANS] if mirrored else ZIGZAG_GAP_SPANS
    return [((high + low) / 2.0, high, low, (high + low) / 2.0) for high, low in spans]


def node_keys(prefix, nodes):
    """Count and, per node, price, kind, confirmation and bar index (0-based)."""
    keys = {f"{prefix}_nodes": float(len(nodes))}
    for i, (price, is_high, confirmed, bar) in enumerate(nodes, 1):
        keys[f"{prefix}_{i}_price"] = price
        keys[f"{prefix}_{i}_is_high"] = 1.0 if is_high else 0.0
        keys[f"{prefix}_{i}_confirmed"] = 1.0 if confirmed else 0.0
        keys[f"{prefix}_{i}_bar"] = float(bar)
    return keys


def gap_scenario():
    """The nodes of the advanced ZigZag over the gap scenario and its mirror image, in percent and
    in ATR mode, at the cut and at the end; the simple ZigZag's newest node at both."""
    keys = {}
    for mirrored, side in ((False, "zigzag_gap"), (True, "zigzag_gap_mirror")):
        bars = gap_bars(mirrored)
        for mode, arguments in (("pct", {"deviation_pct": 2.0}),
                                ("atr", {"atr_mult": 1.0, "atr_len": 14})):
            for cut, moment in ((ZIGZAG_GAP_CUT, "cut"), (len(bars), "end")):
                _, _, nodes = advanced_zigzag(bars[:cut], 3, 1, **arguments)
                sequence = [(bar, price, is_high) for price, is_high, _, bar in nodes]
                assert pivot_sequence_error(sequence) is None, (side, mode, moment)
                keys.update(node_keys(f"{side}_{mode}_{moment}", nodes))
        _, _, expected = simple_zigzag(3, 2.0, bars)
        for cut, moment in ((ZIGZAG_GAP_CUT, "cut"), (len(bars), "end")):
            value, extra = expected[cut - 1][0]
            keys[f"{side}_simple_{moment}_price"] = value
            keys[f"{side}_simple_{moment}_direction"] = extra["direction"]
    return keys


HEADER = [
    "kestrel-chartkit scenario reference fixture: structure detectors",
    "",
    *provenance("scenario_structure"),
    "",
    "The bar sequences are the hand-built ones in tests/scenario_reference_structure.rs:",
    "  liquidity_pools   10 bars, pivot_len 2, tolerance_pct 0.5 (pivot highs 110.0 at bar 2 and",
    "                    110.2 at bar 6, a pivot low 100.0 at bar 4).",
    "  pivots_structure  10 bars, left_bars 2, right_bars 2, score_window 3.",
    "  zigzag_advanced   15 closes [100, 105, 115, 110, 105, 98, 92, 90, 95, 102, 110, 118, 125,",
    "                    120, 115], high/low = close +/- 0.5; depth 2, backstep 1, deviation 2 %.",
    "",
    "Each detector as documented on its type: liquidity pools from non-strict pivots merged within",
    "the tolerance, checked against every bar (stop hunt: pierced and closed back); the structure",
    "score from strict pivots, +2/-1 per high and +1/-2 per low against the previous one,",
    "100 sum / (2 score_window); the ZigZag's running leg confirmed by an opposite pivot beyond the",
    "deviation and the backstep. Flags are 1 for yes and 0 for no.",
    "",
    "zigzag_atr*: the same ZigZag bars in ATR mode, atr_len 3, multiples 3 and 6: the price",
    "distance from the running node must reach the multiple of a Wilder ATR of the true range as",
    "of the current bar. At 3 both swings (26 and 36 points) confirm, at 6 neither does.",
    "",
    "zigzag_gap*: a gap behind which a pivot forms on the wrong side of the running node.",
    "34 bars given by high and low (open = close = their middle); depth 3, backstep 1, deviation",
    "2 % (pct) or 1 ATR(14) (atr). Bar indices are 0-based.",
    "  bars  0- 7  highs 106, 108 .. 120, lows 2 below",
    "  bars  8-12  highs 116, 112 .. 100, lows 114.4, 110.8 .. 100.0",
    "  bars 13-16  inside bars, highs 101.8 .. 101.2, lows 100.2 .. 100.8",
    "  bars 17-26  gap down: highs 81, 81.5, 82, 83, 82.5, 82, 81.5, 81, 80.5, 80;",
    "              lows 79.5, 79.3, 79.2, 79.0, 78.9 .. 78.4, falling throughout",
    "  bars 27-33  highs 80.6, 81.2, 81.8, 82.4, 81.9, 81.4, 80.9; lows 78.9, 79.4, 80.0, 80.6,",
    "              80.2, 79.8, 79.4",
    "By hand: the pivot high 120 (bar 7) starts the first node, the pivot low 100 (bar 12, 16.7 %",
    "below) confirms it and becomes the running node. Bar 20 is a pivot high at 83, below the",
    "running low: it lies on the wrong side and confirms nothing, so at the cut (the first 27",
    "bars) the nodes are 120 (confirmed) and 100 (running). Bar 26 is a pivot low at 78.4 and",
    "replaces the running low; the pivot high 82.4 (bar 30, 5.1 % above) confirms it. At the end:",
    "120, 78.4 (both confirmed), 82.4 (running). A rule that measured the distance as an amount",
    "would confirm 100 at bar 20 and add a high of 83 below it.",
    "zigzag_gap_mirror*: every price negated, highs and lows exchanged; the nodes are the mirror",
    "image. *_simple_*: price and direction of the newest node of the plain `zigzag` (depth 3,",
    "2 %) on the same bars.",
]


def build():
    counts, events = liquidity_pools(POOLS, 2, 0.5)
    hunts = [price for _, _, event, price in events if event == "stop_hunted"]
    pools = {"liquidity_pools_stop_hunt_count": float(len(hunts))}
    pools.update({f"liquidity_pools_stop_hunt_{i}_price": p for i, p in enumerate(hunts, 1)})
    pools["liquidity_pools_active_last"] = float(counts[-1])

    scores = pivot_structure_scores(PIVOTS, 2, 2, 3)
    pivots = {"pivots_structure_output_count": float(len(scores))}
    pivots.update({f"pivots_structure_score_{i}": s for i, s in enumerate(scores, 1)})

    outputs, confirmations, _ = advanced_zigzag(ZIGZAG, 2, 1, 2.0)
    value, running = outputs[-1]
    zigzag = {
        "zigzag_advanced_value_last": value,
        "zigzag_advanced_running_last": 1.0 if running else 0.0,
        "zigzag_advanced_confirmations": float(len(confirmations)),
        "zigzag_advanced_last_confirms_low": 1.0 if confirmations[-1] == "low" else 0.0,
    }
    atr = {}
    for mult in (3, 6):
        _, _, nodes = advanced_zigzag(ZIGZAG, 2, 1, atr_mult=float(mult), atr_len=3)
        confirmed = [price for price, _, done, _ in nodes if done]
        atr[f"zigzag_atr{mult}_confirmed_count"] = float(len(confirmed))
        atr.update({f"zigzag_atr{mult}_confirmed_{i}_price": p for i, p in enumerate(confirmed, 1)})
        atr[f"zigzag_atr{mult}_running_price"] = nodes[-1][0]
    return HEADER, [(None, "liquidity pools", pools), (None, "pivot structure", pivots),
                    (None, "advanced ZigZag", zigzag), (None, "advanced ZigZag, ATR mode", atr),
                    (None, "ZigZag across a gap: the nodes stay a pivot sequence", gap_scenario()),
                    (None, None, {"scenario_structure_tolerance": 1e-9})]
