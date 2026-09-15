"""golden_cross_asset_alignment: timestamp alignment, aligned percentage returns and market
breadth over aligned closes, from the definitions documented on align_closes, aligned_returns and
market_breadth_from_closes."""

from ..fixture import provenance

NAME = "golden_cross_asset_alignment"

SERIES_ABC = [
    ("A", [(0, 100.0), (1, 102.0), (2, 101.0), (3, 104.0), (4, 103.0), (5, 107.0), (6, 108.0)]),
    ("B", [(0, 50.0), (1, 49.0), (2, 51.0), (4, 52.5), (5, 51.0), (6, 50.0)]),
    ("C", [(1, 200.0), (2, 209.0), (3, 203.0), (4, 206.0), (5, 210.0), (6, 209.0)]),
]

# (series as (symbol, [(timestamp, close), ...]), lookback)
CASES = [
    (SERIES_ABC, 3),
    (SERIES_ABC, 100),
    ([("X", [(10, 20.0), (20, 0.0), (30, 22.0), (40, 23.0), (50, 21.0)]),
      ("Y", [(10, 5.0), (20, 5.5), (30, 5.2), (40, 5.1), (50, 5.6)])], 10),
    ([("P", [(1, 10.0), (2, 11.0), (2, 11.5), (3, 12.0)]),
      ("Q", [(1, 30.0), (2, 29.0), (3, 28.0)])], 5),
    ([("M", [(1, 1.0), (2, 2.0)]),
      ("N", [(2, 3.0), (3, 4.0)])], 5),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: cross-asset alignment",
    "",
    *provenance("cross_asset_alignment"),
    "",
    "Shared timestamps: those present in every series, ascending; a timestamp a series holds",
    "twice takes its later close. Returns: 100 (c_t - c_(t-1)) / c_(t-1) over consecutive shared",
    "timestamps, a period dropped for all series where any base close is zero, then the last",
    "`lookback` of them. Breadth: window = last min(shared, lookback + 1) shared closes, at least",
    "two; per series the change from first to last close of the window as above, the last close",
    "and the mean of the window as moving-average reference, a zero first close leaving the",
    "series out; advancing above +1e-9, declining below -1e-9, unchanged otherwise, ratio",
    "advancing / max(1, declining), net (advancing - declining) / total, share above the mean.",
    "",
    "case<i>_s<k>_ts<j>/_close<j> are the inputs; _shared<j>, _return_ts<j>, _s<k>_return<j> and",
    "the breadth keys the expected result. _breadth is 1 when breadth is defined, 0 otherwise.",
    "",
    "cross_asset_alignment_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def _align(series):
    maps = [dict(samples) for _, samples in series]
    shared = sorted(set(maps[0]).intersection(*maps[1:])) if maps else []
    return shared, [[m[t] for t in shared] for m in maps]


def _returns(shared, closes, lookback):
    stamps, rows = [], []
    for t in range(1, len(shared)):
        if any(c[t - 1] == 0.0 for c in closes):
            continue
        stamps.append(shared[t])
        rows.append([100.0 * (c[t] - c[t - 1]) / c[t - 1] for c in closes])
    keep = max(0, len(stamps) - lookback)
    return stamps[keep:], rows[keep:]


def _breadth(shared, closes, lookback):
    window = min(len(shared), lookback + 1)
    if window < 2:
        return None
    members = []
    for c in closes:
        recent = c[-window:]
        first, last = recent[0], recent[-1]
        if first == 0.0:
            continue
        members.append((100.0 * (last - first) / first, last, sum(recent) / window))
    if not members:
        return None
    advancing = sum(1 for r, _, _ in members if r > 1e-9)
    declining = sum(1 for r, _, _ in members if r < -1e-9)
    unchanged = len(members) - advancing - declining
    above = sum(1 for _, price, ma in members if price > ma)
    return {
        "total": float(len(members)),
        "advancing": float(advancing),
        "declining": float(declining),
        "unchanged": float(unchanged),
        "ad_ratio": advancing / max(1, declining),
        "net_pct": (advancing - declining) / len(members),
        "above_ma": above / len(members),
    }


def build():
    blocks = [(None, "case count", {"meta_case_count": float(len(CASES))})]
    for i, (series, lookback) in enumerate(CASES):
        case = {"series": float(len(series)), "lookback": float(lookback)}
        for k, (_, samples) in enumerate(series):
            case[f"s{k}_len"] = float(len(samples))
            for j, (ts, close) in enumerate(samples):
                case[f"s{k}_ts{j}"] = float(ts)
                case[f"s{k}_close{j}"] = close
        shared, closes = _align(series)
        case["shared_count"] = float(len(shared))
        for j, ts in enumerate(shared):
            case[f"shared{j}"] = float(ts)
        stamps, rows = _returns(shared, closes, lookback)
        case["return_count"] = float(len(stamps))
        for j, ts in enumerate(stamps):
            case[f"return_ts{j}"] = float(ts)
            for k, value in enumerate(rows[j]):
                case[f"s{k}_return{j}"] = value
        breadth = _breadth(shared, closes, lookback)
        case["breadth"] = 0.0 if breadth is None else 1.0
        case.update(breadth or {})
        symbols = " ".join(symbol for symbol, _ in series)
        blocks.append((f"case{i}", f"{symbols}, lookback {lookback}", case))
    blocks.append((None, None, {"cross_asset_alignment_tolerance": 1e-12}))
    return HEADER, blocks
