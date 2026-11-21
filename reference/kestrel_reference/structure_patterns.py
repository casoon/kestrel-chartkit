"""Count and range patterns: setup_countdown, narrow_range, williams_fractals.

Written from the rules (DeMark 1994, Crabel 1990, Williams 1995), independently of the Rust code.
Python standard library only.

`compute(name, params, bars)` runs one indicator over `bars` (tuples `(open, high, low, close,
volume)`) and returns one row per bar: `(None, {})` where there is no output, otherwise
`((value, extras), alerts)` with `alerts` mapping each alert kind to its strength (always 1).
Invalid parameters raise `ValueError`.
"""

NAMES = ("setup_countdown", "narrow_range", "williams_fractals")


def _check_params(name, params, allowed):
    params = dict(params or {})
    for key in params:
        if key not in allowed:
            raise ValueError(f"unknown parameter {key!r} for {name}")
    return params


def _get_int(params, key, default, lo):
    v = params.get(key, default)
    if isinstance(v, bool):
        raise ValueError(f"{key} must be an integer")
    if isinstance(v, float):
        if v != v or v != int(v):
            raise ValueError(f"{key} must be an integer")
        v = int(v)
    if not isinstance(v, int):
        raise ValueError(f"{key} must be an integer")
    if v < lo:
        raise ValueError(f"{key} must be at least {lo}")
    return v


def _alerts(kinds):
    return {kind: 1.0 for kind in kinds}


def _setup_countdown(bars):
    n = len(bars)
    C = [b[3] for b in bars]
    H = [b[1] for b in bars]
    L = [b[2] for b in bars]
    out = [(None, {})] * n
    sdir, scount = 0, 0          # +1 sell, -1 buy, 0 none
    cdir, ccount, cc8 = 0, 0, 0.0
    for i in range(5, n):
        alerts = []
        buy = C[i] < C[i - 4]
        sell = C[i] > C[i - 4]
        completed = 0
        if buy:
            if sdir == -1:
                scount += 1
            elif C[i - 1] > C[i - 5]:
                sdir, scount = -1, 1
            else:
                sdir, scount = 0, 0
        elif sell:
            if sdir == 1:
                scount += 1
            elif C[i - 1] < C[i - 5]:
                sdir, scount = 1, 1
            else:
                sdir, scount = 0, 0
        else:
            sdir, scount = 0, 0
        if sdir != 0 and scount == 9:
            completed = sdir
            alerts.append("buy_setup" if sdir == -1 else "sell_setup")
        # A completed setup starts the countdown of its side; an opposite one cancels the running one.
        if completed != 0:
            if cdir == -completed:
                cdir = 0
            if cdir != completed:
                cdir, ccount = completed, 0
        if cdir == -1:
            if C[i] <= L[i - 2]:
                if ccount < 12:
                    ccount += 1
                    if ccount == 8:
                        cc8 = C[i]
                elif ccount == 12 and L[i] <= cc8:
                    ccount = 13
                    alerts.append("buy_countdown")
        elif cdir == 1:
            if C[i] >= H[i - 2]:
                if ccount < 12:
                    ccount += 1
                    if ccount == 8:
                        cc8 = C[i]
                elif ccount == 12 and H[i] >= cc8:
                    ccount = 13
                    alerts.append("sell_countdown")
        cd_out = cdir * ccount if cdir != 0 else 0
        out[i] = ((float(sdir * scount if sdir != 0 else 0), {"countdown": float(cd_out)}),
                  _alerts(alerts))
        if ccount == 13:
            cdir, ccount = 0, 0
    return out


def _narrow_range(params, bars):
    period = _get_int(params, "period", 7, 2)
    n = len(bars)
    R = [b[1] - b[2] for b in bars]
    out = [(None, {})] * n
    for i in range(period - 1, n):
        v = 1 if all(R[i] < R[j] for j in range(i - period + 1, i)) else 0
        inside = 0
        if i >= 1 and bars[i][1] <= bars[i - 1][1] and bars[i][2] >= bars[i - 1][2]:
            inside = 1
        out[i] = ((float(v), {"inside": float(inside)}),
                  _alerts(["narrow_range"] if v == 1 else []))
    return out


def _williams_fractals(params, bars):
    span = _get_int(params, "span", 2, 1)
    hs = [b[1] for b in bars]
    ls = [b[2] for b in bars]
    up = down = None
    out = []
    for i in range(len(bars)):
        alerts = []
        j = i - span
        if j >= span:
            window = [k for k in range(j - span, j + span + 1) if k != j]
            if all(hs[j] > hs[k] for k in window):
                up = hs[j]
                alerts.append("up_fractal")
            if all(ls[j] < ls[k] for k in window):
                down = ls[j]
                alerts.append("down_fractal")
        if up is None or down is None:
            out.append((None, {}))
        else:
            out.append(((float(up), {"down_level": float(down)}), _alerts(alerts)))
    return out


def compute(name, params, bars):
    if name == "setup_countdown":
        _check_params(name, params, ())
        return _setup_countdown(bars)
    if name == "narrow_range":
        return _narrow_range(_check_params(name, params, ("period",)), bars)
    if name == "williams_fractals":
        return _williams_fractals(_check_params(name, params, ("span",)), bars)
    raise ValueError(f"unknown indicator {name!r}")
