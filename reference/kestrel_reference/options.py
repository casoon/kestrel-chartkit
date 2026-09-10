"""European option pricing: Black-Scholes-Merton and Black-76."""

import math

from .normal import cdf, pdf


def black_scholes_merton(spot, strike, t, r, q, vol, is_call):
    """Price and Greeks of a European option on an underlying with continuous dividend yield `q`.

        d1 = (ln(S/K) + (r - q + vol^2 / 2) t) / (vol sqrt t),   d2 = d1 - vol sqrt t
        call = S e^(-qt) N(d1) - K e^(-rt) N(d2)
        put  = K e^(-rt) N(-d2) - S e^(-qt) N(-d1)

    Greeks are per unit change of their variable (vega per 1.0 of volatility, rho per 1.0 of
    rate); theta is the change in price per year of calendar time.
    """
    sqrt_t = math.sqrt(t)
    spread = vol * sqrt_t
    d1 = (math.log(spot / strike) + (r - q + 0.5 * vol * vol) * t) / spread
    d2 = d1 - spread
    carry = math.exp(-q * t)
    discount = math.exp(-r * t)
    density = pdf(d1)

    gamma = carry * density / (spot * spread)
    vega = spot * carry * density * sqrt_t
    time_decay = -spot * carry * density * vol / (2.0 * sqrt_t)

    if is_call:
        price = spot * carry * cdf(d1) - strike * discount * cdf(d2)
        delta = carry * cdf(d1)
        theta = time_decay - r * strike * discount * cdf(d2) + q * spot * carry * cdf(d1)
        rho = strike * t * discount * cdf(d2)
    else:
        price = strike * discount * cdf(-d2) - spot * carry * cdf(-d1)
        delta = -carry * cdf(-d1)
        theta = time_decay + r * strike * discount * cdf(-d2) - q * spot * carry * cdf(-d1)
        rho = -strike * t * discount * cdf(-d2)

    return {
        "price": price,
        "delta": delta,
        "gamma": gamma,
        "vega": vega,
        "theta_annual": theta,
        "rho": rho,
    }


def black_76(forward, strike, t, r, vol, is_call):
    """Black-76: a forward and a discount factor e^(-rt), no spot/carry decomposition.

        d1 = (ln(F/K) + vol^2 t / 2) / (vol sqrt t),   d2 = d1 - vol sqrt t
        call = e^(-rt) (F N(d1) - K N(d2)),   put = e^(-rt) (K N(-d2) - F N(-d1))
    """
    discount = math.exp(-r * t)
    spread = vol * math.sqrt(t)
    d1 = (math.log(forward / strike) + 0.5 * spread * spread) / spread
    d2 = d1 - spread
    if is_call:
        return discount * (forward * cdf(d1) - strike * cdf(d2))
    return discount * (strike * cdf(-d2) - forward * cdf(-d1))
