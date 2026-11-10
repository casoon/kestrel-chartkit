"""Structure detectors, each following the definition documented on its type in the crate.

Bars are (open, high, low, close) tuples, oldest first.
"""


def pivot_structure_scores(bars, left, right, window):
    """Structure score per output: strict pivots `right` bars back, +2/-1 for a higher/other
    high, +1/-2 for a higher/other low, 100 sum / (2 window) over the last `window` sums."""
    history, sums, outputs = [], [], []
    last_high = last_low = None
    for bar in bars:
        history.append(bar)
        if len(history) > (left + right + 1) * 4:
            history.pop(0)
        if len(history) < left + right + 1:
            continue
        c = len(history) - 1 - right
        high, low = history[c][1], history[c][2]
        others = [history[i] for i in range(c - left, c + right + 1) if i != c]
        score, found = 0.0, False
        if all(b[1] < high for b in others):
            previous, last_high = last_high, high
            if previous is not None:
                score += 2.0 if high > previous else -1.0
                found = True
        if all(b[2] > low for b in others):
            previous, last_low = last_low, low
            if previous is not None:
                score += 1.0 if low > previous else -2.0
                found = True
        if found:
            sums.append(score)
            if len(sums) > window:
                sums.pop(0)
        value = sum(sums) / (window * 2.0) * 100.0 if sums else 0.0
        outputs.append(min(max(value, -100.0), 100.0))
    return outputs


def liquidity_pools(bars, pivot_len, tolerance_pct):
    """Active pool count per output and the pool events `(bar index, kind, event, price)`."""
    pivot_len = max(pivot_len, 2)
    tolerance = max(tolerance_pct, 0.001) / 100.0
    size = 2 * pivot_len + 1
    window, pools, counts, events = [], [], [], []

    def register(kind, price):
        for pool in pools:
            if (pool["kind"] == kind and pool["state"] == "active" and pool["price"] != 0.0
                    and abs(pool["price"] - price) / abs(pool["price"]) <= tolerance):
                pool["touches"] += 1
                pool["price"] = (pool["price"] + price) / 2.0
                return
        pools.append({"kind": kind, "price": price, "touches": 1, "state": "active"})

    for index, bar in enumerate(bars):
        window.append(bar)
        if len(window) > size:
            window.pop(0)
        if len(window) < size:
            continue
        mid = window[pivot_len]
        if all(i == pivot_len or b[1] <= mid[1] for i, b in enumerate(window)):
            register("bsl", mid[1])
        if all(i == pivot_len or b[2] >= mid[2] for i, b in enumerate(window)):
            register("ssl", mid[2])
        _, high, low, close = bar
        for pool in pools:
            kind, state, price = pool["kind"], pool["state"], pool["price"]
            if state == "active" and ((kind == "bsl" and high > price) or
                                      (kind == "ssl" and low < price)):
                back = close < price if kind == "bsl" else close > price
                pool["state"] = "stop_hunted" if back else "broken_through"
                events.append((index, kind, pool["state"], price))
            elif state == "broken_through" and ((kind == "bsl" and close < price) or
                                                (kind == "ssl" and close > price)):
                pool["state"] = "reclaimed"
                events.append((index, kind, "reclaimed", price))
        counts.append(sum(1 for pool in pools if pool["state"] == "active"))
    return counts, events


def pivot_sequence_error(nodes):
    """Why `nodes` — `(timestamp, price, is_high)` — is no pivot sequence, or None if it is one:
    highs and lows alternate, timestamps do not decrease, every high lies above and every low
    below the node before it."""
    for i in range(1, len(nodes)):
        (t0, p0, h0), (t1, p1, h1) = nodes[i - 1], nodes[i]
        if h0 == h1:
            return f"nodes {i - 1} and {i} are of the same kind"
        if t1 < t0:
            return f"node {i} is earlier than node {i - 1}"
        if not (p1 > p0 if h1 else p1 < p0):
            return f"node {i} does not lie beyond node {i - 1}"
    return None


def advanced_zigzag(bars, depth, backstep, deviation_pct=None, atr_mult=None, atr_len=14):
    """ZigZag in percent mode (`deviation_pct`: the signed relative move against pct / 100) or ATR
    mode (`atr_mult`: the signed price move against atr_mult * Wilder ATR(atr_len), none before
    the ATR exists). The move of a pivot high is its price minus the running node's, that of a
    pivot low the reverse; a pivot on the wrong side of the running node confirms nothing. On a
    bar that is both a pivot high and a pivot low the pivot of the running node's kind comes
    first, and before the first node such a bar starts none.

    Returns `(value, running)` per output, the kinds of the swings confirmed, oldest first ("low"
    when a pivot high confirms the running low, "high" the other way), and the nodes as
    `(price, is_high, confirmed, bar index, index of the confirming bar or None)`."""
    depth = max(depth, 1)
    size = 2 * depth + 1
    nodes, outputs, confirmations = [], [], []
    last_confirmation = None
    seed, atr, prev_close = [], None, None

    for index, (_, high, low, close) in enumerate(bars):
        tr = high - low if prev_close is None else max(high - low, abs(high - prev_close),
                                                        abs(low - prev_close))
        prev_close = close
        if atr is None:
            seed.append(tr)
            if len(seed) == atr_len:
                atr = sum(seed) / atr_len
        else:
            atr = (1.0 / atr_len) * tr + (1.0 - 1.0 / atr_len) * atr
        if index + 1 < size:
            continue
        window = bars[index + 1 - size:index + 1]
        mid = window[depth]
        mid_index = index - depth
        if atr_mult is None:
            threshold = deviation_pct / 100.0
        else:
            threshold = atr_mult * atr if atr is not None and atr > 0.0 else float("inf")
        backstep_ok = last_confirmation is None or mid_index >= last_confirmation + backstep

        kinds = []
        if all(b[1] <= mid[1] for b in window):
            kinds.append(True)
        if all(b[2] >= mid[2] for b in window):
            kinds.append(False)
        if len(kinds) == 2:
            kinds = [] if not nodes else [nodes[-1]["is_high"], not nodes[-1]["is_high"]]

        for is_high in kinds:
            price = mid[1] if is_high else mid[2]
            if not nodes:
                nodes.append({"price": price, "is_high": is_high, "confirmed": False,
                              "bar": mid_index, "at": None})
                continue
            running = nodes[-1]
            if running["is_high"] == is_high:
                if (price > running["price"]) if is_high else (price < running["price"]):
                    running["price"], running["bar"] = price, mid_index
                continue
            move = price - running["price"] if is_high else running["price"] - price
            if move <= 0.0:
                continue
            if atr_mult is None and running["price"] != 0.0:
                move = move / abs(running["price"])
            elif atr_mult is None:
                move = float("inf")
            if move >= threshold and backstep_ok:
                running["confirmed"], running["at"] = True, index
                nodes.append({"price": price, "is_high": is_high, "confirmed": False,
                              "bar": mid_index, "at": None})
                last_confirmation = mid_index
                confirmations.append("low" if is_high else "high")
        value = nodes[-1]["price"] if nodes else mid[3]
        outputs.append((value, bool(nodes) and not nodes[-1]["confirmed"]))
    return outputs, confirmations, [(n["price"], n["is_high"], n["confirmed"], n["bar"], n["at"])
                                    for n in nodes]


def wilder_atr(bars, length):
    """Wilder ATR per bar (None before bar `length`): the mean of the first `length` true ranges,
    then atr + (tr - atr) / length. The true range of the first bar is high - low."""
    values, atr, seed, prev_close = [], None, [], None
    for _, high, low, close in bars:
        tr = high - low if prev_close is None else max(high - low, abs(high - prev_close),
                                                        abs(low - prev_close))
        prev_close = close
        if atr is None:
            seed.append(tr)
            if len(seed) == length:
                atr = sum(seed) / length
        else:
            atr = (1.0 / length) * tr + (1.0 - 1.0 / length) * atr
        values.append(atr)
    return values


def trendlines(bars, depth, backstep, deviation_pct, atr_len, tolerance_atr, atr_mult=None):
    """Support and resistance line through confirmed ZigZag nodes, as documented on
    `indicator::trendlines`. Time is the bar index. Returns per bar `None` or
    `(value, extra, alerts)`.

    Per side a segment of the nodes confirmed since the line last broke, beginning with a node of
    the side's kind (lows for support). A newly confirmed node of the kind fixes the line through
    the oldest node of the kind that no node of the kind in between pierces by more than the
    tolerance, and itself; older nodes leave the segment. The channel is the parallel through the
    node of the other kind furthest beyond the line. From the bar that fixed the line: a touch is
    the bar's low (high) within the tolerance, a break a close below (above) the line, which ends
    it and empties the segment."""
    _, _, nodes = advanced_zigzag(bars, depth, backstep, deviation_pct, atr_mult, atr_len)
    confirmed_on = {}
    for price, is_high, _, bar, at in nodes:
        if at is not None:
            confirmed_on.setdefault(at, []).append({"price": price, "is_high": is_high, "bar": bar})
    atrs = wilder_atr(bars, atr_len)
    state = {"support": {"segment": [], "line": None}, "resistance": {"segment": [], "line": None}}
    outputs = []

    def at_time(line, time):
        a, c = line["a"], line["c"]
        return a["price"] + (c["price"] - a["price"]) / (c["bar"] - a["bar"]) * (time - a["bar"])

    for t, (_, high, low, close) in enumerate(bars):
        atr = atrs[t]
        if atr is None:
            outputs.append(None)
            continue
        tolerance = tolerance_atr * atr
        extra, alerts, count = {"atr": atr, "tolerance": tolerance}, {}, 0
        for name, side in state.items():
            sign = 1.0 if name == "support" else -1.0       # far side of the line: above / below
            wants_high = name == "resistance"
            formed = False
            for node in confirmed_on.get(t, []):
                segment = side["segment"]
                if node["is_high"] != wants_high:
                    if segment:
                        segment.append(node)
                    continue
                segment.append(node)
                kind = [k for k in segment if k["is_high"] == wants_high]
                if len(kind) < 2:
                    continue
                for a in kind[:-1]:
                    line = {"a": a, "c": node}
                    inner = [k for k in kind if a["bar"] < k["bar"] < node["bar"]]
                    if all(sign * (k["price"] - at_time(line, k["bar"])) >= -tolerance
                           for k in inner):
                        break
                del segment[:segment.index(a)]
                kind = [k for k in segment if k["is_high"] == wants_high]
                line.update(nodes=len(kind), touches=0,
                            node_touches=sum(1 for k in kind
                                             if abs(k["price"] - at_time(line, k["bar"])) <= tolerance))
                side["line"] = line
                formed = True
            line = side["line"]
            if line is None:
                continue
            others = [sign * (k["price"] - at_time(line, k["bar"]))
                      for k in side["segment"] if k["is_high"] != wants_high]
            width = max(others) if others and max(others) > 0.0 else None
            value = at_time(line, t)
            extreme = high if wants_high else low
            touch = abs(extreme - value) <= tolerance
            broke = close > value if wants_high else close < value
            if touch:
                line["touches"] += 1
            extra.update({
                f"{name}_value": value,
                f"{name}_start_price": line["a"]["price"],
                f"{name}_start_bars_back": float(t - line["a"]["bar"]),
                f"{name}_end_price": line["c"]["price"],
                f"{name}_end_bars_back": float(t - line["c"]["bar"]),
                f"{name}_nodes": float(line["nodes"]),
                f"{name}_node_touches": float(line["node_touches"]),
                f"{name}_touches": float(line["touches"]),
                f"{name}_touch": 1.0 if touch else 0.0,
                f"{name}_formed": 1.0 if formed else 0.0,
                f"{name}_break": 1.0 if broke else 0.0,
            })
            if width is not None:
                extra[f"{name}_channel_value"] = value + sign * width
                extra[f"{name}_channel_width"] = width
            count += 1
            if broke:
                alerts["bull_resistance_line_break" if wants_high
                       else "bear_support_line_break"] = 0.6
                side["line"], side["segment"] = None, []
        outputs.append((float(count), extra, alerts))
    return outputs
