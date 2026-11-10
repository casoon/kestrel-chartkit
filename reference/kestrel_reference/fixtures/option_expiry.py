"""golden_option_expiry: the result of an option position at expiry (call or put, bought or sold,
premium, contract size), its break-even, largest profit and largest loss, and the result over a
price grid, from the definitions documented on `option_expiry_result`, `option_expiry_profile` and
`option_expiry_series`."""

from fractions import Fraction

from ..fixture import provenance

NAME = "golden_option_expiry"

# (call?, bought?, strike, premium, contract size, prices of the underlying at expiry,
#  grid as (from, to, points))
POSITIONS = [
    # The example worked by hand: strike 100, premium 4, per unit. At 108 the bought call makes
    # 108 - 100 - 4 = 4 and the sold call -4; at 95 the call expires worthless (-4 / +4) and the
    # bought put makes 100 - 95 - 4 = 1; break-even 104 (call) and 96 (put); the sold put loses
    # at most 100 - 4 = 96, with the underlying at 0.
    (True, True, 100.0, 4.0, 1.0, [0.0, 88.0, 95.0, 100.0, 104.0, 108.0, 112.0], (88.0, 112.0, 25)),
    (True, False, 100.0, 4.0, 1.0, [0.0, 88.0, 95.0, 100.0, 104.0, 108.0, 112.0], (88.0, 112.0, 25)),
    (False, True, 100.0, 4.0, 1.0, [0.0, 88.0, 95.0, 96.0, 100.0, 108.0, 112.0], (88.0, 112.0, 25)),
    (False, False, 100.0, 4.0, 1.0, [0.0, 88.0, 95.0, 96.0, 100.0, 108.0, 112.0], (88.0, 112.0, 25)),
    # Per contract of 100 units.
    (True, True, 100.0, 4.0, 100.0, [95.0, 100.0, 104.0, 108.0], (80.0, 120.0, 9)),
    (False, False, 100.0, 4.0, 100.0, [0.0, 95.0, 96.0, 100.0], (80.0, 120.0, 9)),
    # Prices and premiums that are no whole numbers, and a grid the strike is not on.
    (True, False, 52.5, 2.35, 10.0, [40.0, 52.5, 54.85, 61.2, 250.0], (40.5, 64.5, 7)),
    # (the second grid has a step of 25 / 7, which no double holds exactly).
    (False, True, 52.5, 2.35, 10.0, [0.0, 47.3, 50.15, 52.5, 61.2], (40.0, 65.0, 8)),
    (True, True, 0.75, 0.0625, 1000.0, [0.5, 0.75, 0.8125, 1.1], (0.0, 1.5, 13)),
    # Premium 0: nothing to lose for the buyer, nothing to gain for the seller.
    (True, True, 20.0, 0.0, 1.0, [0.0, 20.0, 25.0], (10.0, 30.0, 5)),
    (False, False, 20.0, 0.0, 1.0, [0.0, 20.0, 25.0], (10.0, 30.0, 5)),
    # A put at a premium equal to its strike, the most a put can pay.
    (False, True, 20.0, 20.0, 1.0, [0.0, 10.0, 20.0, 30.0], (0.0, 40.0, 5)),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: option result at expiry",
    "",
    *provenance("option_expiry"),
    "",
    "With S the price of the underlying at expiry, K the strike, P the premium per unit and c the",
    "contract size, the option is worth max(S - K, 0) (call) or max(K - S, 0) (put); the buyer's",
    "result is (worth - P) c, the seller's is its negative. The break-even is the S at which the",
    "result is 0. Largest profit and largest loss are taken over all S >= 0; a bound that does not",
    "exist is flagged (_max_profit_unlimited, _max_loss_unlimited = 1) and has no value. The largest",
    "loss is a positive amount.",
    "",
    "The reference does not use the closed forms of the crate's documentation. In exact rational",
    "arithmetic on the doubles of the inputs (fractions.Fraction) it values the option as",
    "max(S, K) - K or K - min(S, K), takes the seller's result as the negative of the buyer's,",
    "finds the break-even by solving the linear piece on the in-the-money side and checks that the",
    "result there is 0, and finds the extremes from the two linear pieces: the results at S = 0 and",
    "S = K and the slope beyond K. Positions 0 to 3 are also checked against the example worked by",
    "hand (see the module's case comments).",
    "",
    "Grid: S_i = from + (to - from) i / (points - 1), i = 0 .. points - 1.",
    "",
    "pos<i>_call and _long are 1 or 0; _s<j>/_result<j> the single prices; _grid_s<j>/",
    "_grid_result<j> the grid. option_expiry_tolerance is the comparison tolerance the tests apply",
    "(relative to the larger of 1 and the reference value), stated rather than derived.",
]


def _result(call, bought, strike, premium, size, price):
    worth = max(price, strike) - strike if call else strike - min(price, strike)
    buyer = (worth - premium) * size
    return buyer if bought else -buyer


def _profile(call, bought, strike, premium, size):
    """(break-even, largest profit or None, largest loss or None), from the piecewise-linear form."""
    def result(price):
        return _result(call, bought, strike, premium, size, price)

    # In the money the buyer's result per unit is S - K - P (call) or K - S - P (put).
    break_even = strike + premium if call else strike - premium
    assert result(break_even) == 0
    at_zero, at_strike = result(Fraction(0)), result(strike)
    slope_beyond = result(strike + 1) - at_strike
    profit = None if slope_beyond > 0 else max(at_zero, at_strike)
    loss = None if slope_beyond < 0 else -min(at_zero, at_strike)
    return break_even, profit, loss


def build():
    blocks = [(None, "position count", {"meta_position_count": float(len(POSITIONS))})]
    by_hand = {
        0: ({108.0: 4.0, 95.0: -4.0}, 104.0, None, 4.0),
        1: ({108.0: -4.0, 95.0: 4.0}, 104.0, 4.0, None),
        2: ({95.0: 1.0, 108.0: -4.0}, 96.0, 96.0, 4.0),
        3: ({95.0: -1.0, 0.0: -96.0}, 96.0, 4.0, 96.0),
    }
    for i, (call, bought, strike, premium, size, prices, grid) in enumerate(POSITIONS):
        k, p, c = Fraction(strike), Fraction(premium), Fraction(size)
        break_even, profit, loss = _profile(call, bought, k, p, c)
        case = {
            "call": 1.0 if call else 0.0,
            "long": 1.0 if bought else 0.0,
            "strike": strike,
            "premium": premium,
            "contract_size": size,
            "break_even": float(break_even),
            "max_profit_unlimited": 1.0 if profit is None else 0.0,
        }
        if profit is not None:
            case["max_profit"] = float(profit)
        case["max_loss_unlimited"] = 1.0 if loss is None else 0.0
        if loss is not None:
            case["max_loss"] = float(loss)
        results = [float(_result(call, bought, k, p, c, Fraction(s))) for s in prices]
        if i in by_hand:
            expected, hand_break_even, hand_profit, hand_loss = by_hand[i]
            for price, value in expected.items():
                assert results[prices.index(price)] == value, (i, price)
            assert (float(break_even), profit, loss) == (hand_break_even, hand_profit, hand_loss)
        case["n"] = float(len(prices))
        for j, (price, value) in enumerate(zip(prices, results)):
            case[f"s{j}"] = price
            case[f"result{j}"] = value
        start, end, points = grid
        case.update({"grid_from": start, "grid_to": end, "grid_points": float(points)})
        for j in range(points):
            price = Fraction(start) + (Fraction(end) - Fraction(start)) * j / (points - 1)
            case[f"grid_s{j}"] = float(price)
            case[f"grid_result{j}"] = float(_result(call, bought, k, p, c, price))
        side = "bought" if bought else "sold"
        kind = "call" if call else "put"
        blocks.append((f"pos{i}", f"{kind} {side}, strike {strike}, premium {premium}, x {size}",
                       case))
    blocks.append((None, None, {"option_expiry_tolerance": 1e-12}))
    return HEADER, blocks
