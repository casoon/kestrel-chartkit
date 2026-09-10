"""golden_option_diff: standard normal CDF, Black-Scholes-Merton with Greeks, Black-76."""

from ..fixture import provenance
from ..normal import cdf
from ..options import black_76, black_scholes_merton

NAME = "golden_option_diff"

HEADER = [
    "kestrel-chartkit golden reference fixture: option pricing difference tests",
    "",
    *provenance("option_diff"),
    "Every case carries its own inputs, so there is no second copy of the parameter grid on the",
    "Rust side.",
    "",
    "Conventions: European exercise, flat interest and dividend rates, Actual/365Fixed, no",
    "calendar adjustment. Maturities are whole days, so the year fraction is exactly days/365 - the",
    "same number the crate takes as time_to_expiry_years. Greeks are per unit change of their",
    "variable; theta is annual.",
    "",
    "Normal CDF cases: x, value, computed as erfc(-x / sqrt 2) / 2.",
    "Black-Scholes cases: spot, strike, t (years), r, q, vol, type (1 = call, -1 = put), price and",
    "Greeks.",
    "Black-76 cases: forward, strike, t, r, vol, type, price - a forward and a discount factor,",
    "without a spot/carry decomposition.",
]

CDF_GRID = [
    -8.0, -6.0, -4.0, -3.0, -2.5, -2.0, -1.5, -1.0, -0.5, -0.35, -0.15, -0.05,
    0.0, 0.05, 0.15, 0.35, 0.5, 1.0, 1.5, 2.0, 2.5, 3.0, 4.0, 6.0, 8.0,
    0.123456789, -0.987654321, 7.07, -7.07, 1.959963984540054,
]


def _option_grid():
    grid = []
    # Moneyness x maturity x volatility x call/put, then negative rates and a zero-dividend leg.
    for days, maturity_label in [(7, "one week"), (91, "one quarter"), (365, "one year"),
                                 (1825, "five years")]:
        for strike, moneyness_label in [(80.0, "ITM call"), (100.0, "at the money"),
                                        (125.0, "OTM call")]:
            for vol, vol_label in [(0.01, "near-zero vol"), (0.2, "typical vol"),
                                   (0.6, "high vol")]:
                for is_call in (True, False):
                    grid.append((100.0, strike, days, 0.05, 0.02, vol, is_call,
                                 f"{maturity_label}, {moneyness_label}, {vol_label}"))
    grid.append((100.0, 100.0, 365, -0.005, 0.0, 0.2, True, "negative rate, no dividend"))
    grid.append((100.0, 100.0, 365, -0.005, 0.0, 0.2, False, "negative rate, no dividend"))
    grid.append((100.0, 100.0, 365, 0.05, 0.08, 0.2, True, "dividend yield above the rate"))
    grid.append((100.0, 100.0, 365, 0.05, 0.08, 0.2, False, "dividend yield above the rate"))
    grid.append((5000.0, 5000.0, 365, 0.03, 0.01, 0.15, True, "large price level"))
    grid.append((0.5, 0.5, 365, 0.03, 0.01, 0.15, True, "small price level"))
    return grid


def _black76_grid():
    grid = []
    for t in (0.0192, 0.25, 1.0, 5.0):
        for strike in (80.0, 100.0, 125.0):
            for vol in (0.01, 0.25, 0.8):
                for is_call in (True, False):
                    grid.append((100.0, strike, t, 0.04, vol, is_call))
    grid.append((100.0, 100.0, 1.0, -0.01, 0.25, True))
    grid.append((100.0, 100.0, 1.0, -0.01, 0.25, False))
    return grid


def build():
    blocks = [("meta", "case count", {"cdf_case_count": float(len(CDF_GRID))})]
    for index, x in enumerate(CDF_GRID, start=1):
        blocks.append((f"cdf{index}", None, {"x": x, "value": cdf(x)}))

    options = _option_grid()
    blocks.append(("meta", "case count", {"option_case_count": float(len(options))}))
    for index, (spot, strike, days, r, q, vol, is_call, comment) in enumerate(options, start=1):
        t = days / 365.0
        case = {
            "spot": spot, "strike": strike, "t": t, "r": r, "q": q, "vol": vol,
            "type": 1.0 if is_call else -1.0,
        }
        case.update(black_scholes_merton(spot, strike, t, r, q, vol, is_call))
        blocks.append((f"opt{index}", comment, case))

    black76 = _black76_grid()
    blocks.append(("meta", "case count", {"black76_case_count": float(len(black76))}))
    for index, (forward, strike, t, r, vol, is_call) in enumerate(black76, start=1):
        blocks.append((f"b76_{index}", None, {
            "forward": forward, "strike": strike, "t": t, "r": r, "vol": vol,
            "type": 1.0 if is_call else -1.0,
            "price": black_76(forward, strike, t, r, vol, is_call),
        }))
    return HEADER, blocks
