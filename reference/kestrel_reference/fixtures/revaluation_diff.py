"""golden_revaluation_diff: a curve-discounted bond and an option under market scenarios."""

from datetime import date

from ..bonds import value_on_curve
from ..curves import ZeroCurve
from ..dates import add_days, is_end_of_month
from ..fixture import date_keys, provenance
from ..options import black_scholes_merton
from ..schedules import accrual_dates

NAME = "golden_revaluation_diff"

HEADER = [
    "kestrel-chartkit golden reference fixture: integrated revaluation under market scenarios",
    "",
    *provenance("revaluation_diff"),
    "",
    "This closes the one gap the per-instrument fixtures leave open: a bond discounted on a zero",
    "curve rather than at a single yield, an option priced off the same curve, and both of them",
    "again after the market data is shifted. The sensitivities follow from those two valuations as",
    "one-sided differences - scenario minus base - which is what the crate defines them to be.",
    "",
    "Deliberately not covered here: summing positions and converting currency. A second",
    "implementation of an addition and a multiplication by a quoted rate would prove nothing; the",
    "portfolio fixture checks those against the single-instrument valuations instead.",
    "",
    "Conventions: continuously compounded zero rates, linear in time between the nodes,",
    "Actual/365Fixed, unadjusted schedule, no settlement delay. The rate shift is applied to every",
    "zero rate in absolute units; the underlying shock scales the spot; the volatility shift is in",
    "volatility points. All three are applied to the market data and everything is then recomputed",
    "from scratch - no result is scaled.",
    "",
    "Per scenario: scen<i>_bond_dirty / _bond_clean / _bond_accrued, scen<i>_option_price and the",
    "rate that produced it as scen<i>_option_rate, plus scen<i>_bond_delta / _option_delta, the",
    "difference against the base scenario.",
]

REFERENCE = date(2026, 6, 15)
NODES = [(183, 0.021), (365, 0.025), (1095, 0.031), (1825, 0.034), (3650, 0.038)]
BOND = dict(face=1000.0, coupon=0.045, frequency=2,
            issue=date(2026, 2, 10), maturity=date(2031, 2, 10))
OPTION = dict(spot=100.0, strike=105.0, days=274, volatility=0.28, dividend_yield=0.01,
              is_call=True)
SCENARIOS = [
    ("Basis - keine Verschiebung", 0.0, 0.0, 0.0),
    ("Zins plus ein Basispunkt", 0.0001, 0.0, 0.0),
    ("Zins plus 100 Basispunkte", 0.01, 0.0, 0.0),
    ("Zins minus 50 Basispunkte", -0.005, 0.0, 0.0),
    ("Basiswert plus ein Prozent", 0.0, 0.01, 0.0),
    ("Basiswert minus zehn Prozent", 0.0, -0.10, 0.0),
    ("Volatilitaet plus einen Punkt", 0.0, 0.0, 0.01),
    ("Volatilitaet plus fuenf Punkte", 0.0, 0.0, 0.05),
    ("Alles zugleich - Einbruch mit Vola- und Zinsanstieg", 0.005, -0.10, 0.05),
]


def _revalued(rate_shift, spot_shock, vol_shift):
    """Bond and option under one shifted market state, both recomputed from the shifted data."""
    curve = ZeroCurve([(days / 365.0, rate) for days, rate in NODES], shift=rate_shift)
    dates = accrual_dates(BOND["issue"], BOND["maturity"], 12 // BOND["frequency"],
                          month_end_rule=is_end_of_month(BOND["maturity"]))
    bond = value_on_curve(BOND["face"], BOND["coupon"], dates, REFERENCE, curve)

    t = OPTION["days"] / 365.0
    rate = curve.zero(t)
    option = black_scholes_merton(
        OPTION["spot"] * (1.0 + spot_shock),
        OPTION["strike"],
        t,
        rate,
        OPTION["dividend_yield"],
        max(OPTION["volatility"] + vol_shift, 0.0),
        OPTION["is_call"],
    )
    return {
        "bond_dirty": bond["dirty"],
        "bond_clean": bond["clean"],
        "bond_accrued": bond["accrued"],
        "option_rate": rate,
        "option_price": option["price"],
    }


def build():
    market = {"node_count": float(len(NODES)), "scenario_count": float(len(SCENARIOS))}
    for index, (days, rate) in enumerate(NODES):
        market[f"node{index}_t"] = days / 365.0
        market[f"node{index}_rate"] = rate
    market["face"] = BOND["face"]
    market["coupon"] = BOND["coupon"]
    market["frequency"] = float(BOND["frequency"])
    market.update(date_keys("issue", BOND["issue"]))
    market.update(date_keys("maturity", BOND["maturity"]))
    market["spot"] = OPTION["spot"]
    market["strike"] = OPTION["strike"]
    market["expiry_t"] = OPTION["days"] / 365.0
    market.update(date_keys("expiry", add_days(REFERENCE, OPTION["days"])))
    market["volatility"] = OPTION["volatility"]
    market["dividend_yield"] = OPTION["dividend_yield"]
    market["option_type"] = 1.0 if OPTION["is_call"] else -1.0

    blocks = [("market", "curve, bond and option that every scenario starts from", market)]
    base = None
    for index, (comment, rate_shift, spot_shock, vol_shift) in enumerate(SCENARIOS):
        values = _revalued(rate_shift, spot_shock, vol_shift)
        if base is None:
            base = values
        case = {
            "rate_shift": rate_shift,
            "underlying_shock": spot_shock,
            "volatility_shift": vol_shift,
        }
        case.update(values)
        case["bond_delta"] = values["bond_dirty"] - base["bond_dirty"]
        case["option_delta"] = values["option_price"] - base["option_price"]
        blocks.append((f"scen{index}", comment, case))
    return HEADER, blocks
