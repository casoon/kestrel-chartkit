"""golden_option_pricing: the at-the-money Black-Scholes-Merton case of the option tests, price and
Greeks from the documented formulas."""

from ..fixture import provenance
from ..options import black_scholes_merton

NAME = "golden_option_pricing"

HEADER = [
    "kestrel-chartkit golden reference fixture: option pricing",
    "",
    *provenance("option_pricing"),
    "",
    "At the money: S = K = 100, T = 1 year, r = 0.05, q = 0, sigma = 0.20. Black-Scholes-Merton",
    "with N(x) = erfc(-x / sqrt 2) / 2: d1 = (ln(S/K) + (r - q + sigma^2 / 2) T) / (sigma sqrt T)",
    "= 0.35, d2 = d1 - sigma sqrt T = 0.15; call = S N(d1) - K e^(-rT) N(d2),",
    "put = K e^(-rT) N(-d2) - S N(-d1); delta N(d1) and N(d1) - 1; gamma phi(d1) / (S sigma sqrt T);",
    "vega S sqrt(T) phi(d1), per 1.0 of volatility.",
    "",
    "The tests compare with an absolute floor plus a relative share of the reference value,",
    "option_pricing_abs_floor + option_pricing_rel |reference|, stated rather than derived.",
]


def build():
    call = black_scholes_merton(100.0, 100.0, 1.0, 0.05, 0.0, 0.20, True)
    put = black_scholes_merton(100.0, 100.0, 1.0, 0.05, 0.0, 0.20, False)
    case = {
        "atm_call_price": call["price"],
        "atm_put_price": put["price"],
        "atm_call_delta": call["delta"],
        "atm_put_delta": put["delta"],
        "atm_gamma": call["gamma"],
        "atm_vega": call["vega"],
    }
    tolerances = {"option_pricing_abs_floor": 1e-10, "option_pricing_rel": 1e-11}
    return HEADER, [(None, "at the money", case), (None, None, tolerances)]
