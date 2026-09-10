"""scenario_structure: pivot structure, liquidity pools and the advanced ZigZag over the hand-built
sequences of the structure scenarios, from the definitions documented on their types."""

from ..fixture import provenance
from ..structure import advanced_zigzag, liquidity_pools, pivot_structure_scores

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
        confirmed = [price for price, _, done in nodes if done]
        atr[f"zigzag_atr{mult}_confirmed_count"] = float(len(confirmed))
        atr.update({f"zigzag_atr{mult}_confirmed_{i}_price": p for i, p in enumerate(confirmed, 1)})
        atr[f"zigzag_atr{mult}_running_price"] = nodes[-1][0]
    return HEADER, [(None, "liquidity pools", pools), (None, "pivot structure", pivots),
                    (None, "advanced ZigZag", zigzag), (None, "advanced ZigZag, ATR mode", atr),
                    (None, None, {"scenario_structure_tolerance": 1e-9})]
