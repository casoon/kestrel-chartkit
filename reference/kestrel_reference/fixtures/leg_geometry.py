"""golden_leg_geometry: window leg, retracement, largest pullback, swing sequence, strict pivots,
threshold zigzag, path efficiency and swing overlap, from their definitions.

Written from the documentation of `structure::legs`, not from the Rust code. The zigzag is
written as a state machine over "the extreme so far" with explicit rescans; the window leg as
two scans picking the earliest lowest low and the latest highest high.
"""

from .. import pine_series as ps
from ..fixture import provenance

NAME = "golden_leg_geometry"

N_BARS = 400
WINDOW_CASES = [(399, 30), (250, 30), (380, 60), (120, 10), (399, 5)]
RETRACE_CASES = [
    (100.0, 200.0, 210.0), (100.0, 200.0, 170.0), (100.0, 200.0, 138.2), (100.0, 200.0, 130.0),
    (100.0, 200.0, 100.0), (100.0, 200.0, 90.0), (200.0, 100.0, 90.0), (200.0, 100.0, 130.0),
    (200.0, 100.0, 170.0), (200.0, 100.0, 205.0), (3.2688, 3.5653, 3.465),
]
DEEP = 0.618
PULLBACK_CASES = [(260, True), (330, False), (150, True), (399, False)]
PIVOT_WIDTHS = [2, 5]
ZIGZAG_CONST = [2.0, 4.0]
SEQ_CASES = [
    ([1.0, 2.0], [0.5, 0.7]), ([2.0, 1.0], [0.7, 0.5]), ([1.0, 2.0], [0.7, 0.5]),
    ([1.0, 1.0], [0.5, 0.7]), ([1.0], [0.5, 0.7]), ([3.0, 1.0, 2.0], [0.1, 0.9, 0.5]),
]
EFF_CASES = [[1.0, 2.0, 3.0], [1.0, 3.0, 2.0, 4.0], [5.0, 5.0, 5.0], [2.0, 1.0], [4.0]]
OVERLAP_CASES = [
    (100.0, [(110.0, True), (105.0, False), (120.0, True), (108.0, False)], True),
    (100.0, [(110.0, True), (105.0, False), (120.0, True), (112.0, False)], True),
    (200.0, [(190.0, False), (195.0, True), (180.0, False), (192.0, True)], False),
    (100.0, [(110.0, True)], True),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: leg geometry (structure::legs)",
    "",
    *provenance("leg_geometry"),
    "",
    "Series: pine_series.wave_series(400), bars (open, high, low, close).",
    "window<i>: the last `len` bars up to and including bar `end` (the slice bars[..=end]); the",
    "leg runs from the earlier to the later of (earliest lowest low, latest highest high).",
    "retr<i>: share = (to - price)/(to - from); zone 0 extension (share < 0), 1 correction",
    "(< 0.618), 2 deep correction (<= 1), 3 reversal.",
    "pull<i>: largest pullback before bar i against the extreme max(high[i-20..i]) (up) resp.",
    "min(low[i-20..i]) (down), from the last bar before i that reached it.",
    "seq<i>: 0 up, 1 down, 2 mixed, 3 unknown from the last two highs and lows.",
    "piv_w<w>: indices of strict pivot lows / highs (strictly beyond every other bar within w).",
    "zz_c<c>: threshold zigzag with a constant threshold c on every bar from bar 10 on (none",
    "before); pivots as index, price, is_high, confirming index.",
    "eff<i>: |last-first| / sum |steps|, 0 when nothing moved; none for fewer than two points.",
    "ovl<i>: overlapping pullbacks / pullbacks with two extremes before them; none without.",
]


def _window_leg(bars, len_):
    start = len(bars) - len_
    lo_i = start
    for i in range(start, len(bars)):
        if bars[i][2] < bars[lo_i][2]:
            lo_i = i
    hi_i = start
    best = bars[start][1]
    for i in range(start, len(bars)):
        if bars[i][1] >= best:
            best = bars[i][1]
            hi_i = i
    if hi_i > lo_i:
        return (bars[lo_i][2], bars[hi_i][1], lo_i, hi_i, 1.0)
    return (bars[hi_i][1], bars[lo_i][2], hi_i, lo_i, 0.0)


def _retracement(frm, to, price):
    share = (to - price) / (to - frm)
    if share < 0.0:
        zone = 0
    elif share < DEEP:
        zone = 1
    elif share <= 1.0:
        zone = 2
    else:
        zone = 3
    return share, zone


def _pullback(bars, i, up):
    window = bars[i - 20:i]
    extreme = max(b[1] for b in window) if up else min(b[2] for b in window)
    k = 0
    for j in range(i - 1, -1, -1):
        if (up and bars[j][1] >= extreme) or (not up and bars[j][2] <= extreme):
            k = j
            break
    span = bars[k:i]
    if up:
        return extreme, extreme - min(b[2] for b in span)
    return extreme, max(b[1] for b in span) - extreme


def _sequence(highs, lows):
    if len(highs) < 2 or len(lows) < 2:
        return 3
    h0, h1 = highs[-2], highs[-1]
    l0, l1 = lows[-2], lows[-1]
    if h1 > h0 and l1 > l0:
        return 0
    if h1 < h0 and l1 < l0:
        return 1
    return 2


def _strict(bars, w):
    lows, highs = [], []
    for j in range(w, len(bars) - w):
        others = [bars[k] for k in range(j - w, j + w + 1) if k != j]
        if all(o[2] > bars[j][2] for o in others):
            lows.append(j)
        if all(o[1] < bars[j][1] for o in others):
            highs.append(j)
    return lows, highs


def _zigzag(bars, thr):
    pivots = []
    hi = lo = 0
    state = None  # None before the first pivot, "up" or "down" afterwards

    def lowest(a, b):
        best = a
        for k in range(a, b + 1):
            if bars[k][2] < bars[best][2]:
                best = k
        return best

    def highest(a, b):
        best = a
        for k in range(a, b + 1):
            if bars[k][1] >= bars[best][1]:
                best = k
        return best

    for i, b in enumerate(bars):
        close = b[3]
        if state is None or state == "up":
            if b[1] > bars[hi][1]:
                hi = i
        if state is None or state == "down":
            if b[2] < bars[lo][2]:
                lo = i
        t = thr[i]
        if t is None:
            continue
        if state in (None, "up") and hi < i and close <= bars[hi][1] - t:
            pivots.append((hi, bars[hi][1], 1.0, i))
            lo = lowest(hi + 1, i)
            state = "down"
            continue
        if state in (None, "down") and lo < i and close >= bars[lo][2] + t:
            pivots.append((lo, bars[lo][2], 0.0, i))
            hi = highest(lo + 1, i)
            state = "up"
    return pivots


def _efficiency(points):
    if len(points) < 2:
        return None
    path = 0.0
    for a, b in zip(points, points[1:]):
        path += abs(b - a)
    return abs(points[-1] - points[0]) / path if path > 0.0 else 0.0


def _overlap(anchor, swings, up):
    extremes = [anchor]
    cases = hits = 0
    for price, is_high in swings:
        if is_high == up:
            extremes.append(price)
        elif len(extremes) >= 2:
            cases += 1
            ref = extremes[-2]
            if (up and price < ref) or (not up and price > ref):
                hits += 1
    return hits / cases if cases else None


def build():
    bars = [b[:4] for b in ps.wave_series(N_BARS)]
    blocks = [(None, "case counts", {
        "meta_window_n": float(len(WINDOW_CASES)), "meta_retr_n": float(len(RETRACE_CASES)),
        "meta_pull_n": float(len(PULLBACK_CASES)), "meta_seq_n": float(len(SEQ_CASES)),
        "meta_eff_n": float(len(EFF_CASES)), "meta_ovl_n": float(len(OVERLAP_CASES)),
        "meta_deep": DEEP})]
    for i, (end, len_) in enumerate(WINDOW_CASES):
        frm, to, fi, ti, up = _window_leg(bars[:end + 1], len_)
        blocks.append((f"window{i}", None, {"end": float(end), "len": float(len_), "from": frm,
                                            "to": to, "from_index": float(fi),
                                            "to_index": float(ti), "up": up}))
    for i, (frm, to, price) in enumerate(RETRACE_CASES):
        share, zone = _retracement(frm, to, price)
        blocks.append((f"retr{i}", None, {"from": frm, "to": to, "price": price, "share": share,
                                          "zone": float(zone)}))
    for i, (at, up) in enumerate(PULLBACK_CASES):
        extreme, pull = _pullback(bars, at, up)
        blocks.append((f"pull{i}", None, {"at": float(at), "up": 1.0 if up else 0.0,
                                          "extreme": extreme, "pullback": pull}))
    for i, (highs, lows) in enumerate(SEQ_CASES):
        case = {"nh": float(len(highs)), "nl": float(len(lows))}
        case.update({f"h{j}": v for j, v in enumerate(highs)})
        case.update({f"l{j}": v for j, v in enumerate(lows)})
        case["result"] = float(_sequence(highs, lows))
        blocks.append((f"seq{i}", None, case))
    for w in PIVOT_WIDTHS:
        lows, highs = _strict(bars, w)
        case = {"lows_n": float(len(lows)), "highs_n": float(len(highs))}
        case.update({f"low{j}": float(v) for j, v in enumerate(lows)})
        case.update({f"high{j}": float(v) for j, v in enumerate(highs)})
        blocks.append((f"piv_w{w}", f"strict pivots, width {w}", case))
    for c in ZIGZAG_CONST:
        thr = [None if i < 10 else c for i in range(N_BARS)]
        piv = _zigzag(bars, thr)
        case = {"n": float(len(piv))}
        for j, (idx, price, is_high, conf) in enumerate(piv):
            case[f"p{j}_index"] = float(idx)
            case[f"p{j}_price"] = price
            case[f"p{j}_high"] = is_high
            case[f"p{j}_confirmed"] = float(conf)
        blocks.append((f"zz_c{int(c)}", f"zigzag, threshold {c}", case))
    for i, pts in enumerate(EFF_CASES):
        case = {"n": float(len(pts))}
        case.update({f"v{j}": v for j, v in enumerate(pts)})
        e = _efficiency(pts)
        if e is None:
            case["none"] = 1.0
        else:
            case["result"] = e
        blocks.append((f"eff{i}", None, case))
    for i, (anchor, swings, up) in enumerate(OVERLAP_CASES):
        case = {"anchor": anchor, "up": 1.0 if up else 0.0, "n": float(len(swings))}
        for j, (p, h) in enumerate(swings):
            case[f"s{j}_price"] = p
            case[f"s{j}_high"] = 1.0 if h else 0.0
        o = _overlap(anchor, swings, up)
        if o is None:
            case["none"] = 1.0
        else:
            case["result"] = o
        blocks.append((f"ovl{i}", None, case))
    blocks.append((None, None, {"leg_geometry_tolerance": 1e-12}))
    return HEADER, blocks
