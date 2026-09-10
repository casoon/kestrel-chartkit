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


def advanced_zigzag(bars, depth, backstep, deviation_pct):
    """Percent-mode ZigZag: `(value, running)` per output and the kinds of the swings confirmed,
    oldest first ("low" when a pivot high confirms the running low, "high" the other way)."""
    depth = max(depth, 1)
    threshold = deviation_pct / 100.0
    size = 2 * depth + 1
    window, nodes, outputs, confirmed = [], [], [], []
    state = {"direction": 0, "last_confirmed": None}

    def extend(is_high, price, bar_index, backstep_ok):
        same = 1 if is_high else -1
        running = nodes[-1] if nodes and not nodes[-1]["confirmed"] else None
        if state["direction"] in (same, 0):
            if running is not None and running["is_high"] == is_high:
                if (price > running["price"]) if is_high else (price < running["price"]):
                    running["price"] = price
                    state["direction"] = same
            else:
                nodes.append({"price": price, "is_high": is_high, "confirmed": False})
                state["direction"] = same
            return
        last = nodes[-1]["price"] if nodes else None
        change = abs(price - last) / abs(last) if last not in (None, 0.0) else float("inf")
        if change >= threshold and backstep_ok:
            if nodes:
                nodes[-1]["confirmed"] = True
            nodes.append({"price": price, "is_high": is_high, "confirmed": False})
            state["direction"] = same
            state["last_confirmed"] = bar_index
            confirmed.append("low" if is_high else "high")

    for index, bar in enumerate(bars):
        window.append(bar)
        if len(window) > size:
            window.pop(0)
        if len(window) < size:
            continue
        mid = window[depth]
        mid_index = index - depth
        backstep_ok = (state["last_confirmed"] is None
                       or mid_index >= state["last_confirmed"] + backstep)
        if all(i == depth or b[1] <= mid[1] for i, b in enumerate(window)):
            extend(True, mid[1], mid_index, backstep_ok)
        if all(i == depth or b[2] >= mid[2] for i, b in enumerate(window)):
            extend(False, mid[2], mid_index, backstep_ok)
        value = nodes[-1]["price"] if nodes else mid[3]
        outputs.append((value, bool(nodes) and not nodes[-1]["confirmed"]))
    return outputs, confirmed
