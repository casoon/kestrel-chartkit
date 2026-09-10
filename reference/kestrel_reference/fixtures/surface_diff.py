"""golden_surface_diff: bilinear volatility grids and the total-variance contrast."""

from ..fixture import provenance
from ..surfaces import bilinear, total_variance_volatility

NAME = "golden_surface_diff"

HEADER = [
    "kestrel-chartkit golden reference fixture: volatility surface difference tests",
    "",
    *provenance("surface_diff"),
    "",
    "Convention under test: bilinear interpolation of the quoted volatilities themselves - linear",
    "in maturity, linear in strike - with Actual/365Fixed and maturities on whole days, so both",
    "sides sit on the same time axis. That is a decision, not the only possible one, which is why",
    "the contrast is carried in the fixture rather than argued about in prose:",
    "",
    "query<j>_vol is the value under the convention this crate implements, and the value the",
    "comparison holds to machine precision. query<j>_variance_vol is the value under the other",
    "established convention, linear interpolation of total variance sigma^2*t along the maturity",
    "axis, which is what rules out calendar-spread arbitrage between the quoted slices. The two",
    "agree on every quoted grid point and differ between them; the test records the size of that",
    "difference so the choice stays visible.",
    "",
    "query<j>_inside marks whether the query lies inside the quoted grid. Outside it query<j>_vol",
    "continues the slope of the edge segment - the documented alternative - while this crate holds",
    "the edge value flat; those queries are not comparable and are marked 0. Outside the grid",
    "query<j>_variance_vol holds the edge volatility beyond the last maturity and uses the edge",
    "strike beyond the strike range; it is only compared inside.",
]

SURFACES = [
    ("aufwaerts geneigte Laufzeitstruktur mit Smile",
     [30, 91, 182, 365, 730],
     [80.0, 90.0, 100.0, 110.0, 120.0],
     [[0.320, 0.268, 0.240, 0.255, 0.295],
      [0.305, 0.262, 0.238, 0.251, 0.288],
      [0.296, 0.258, 0.240, 0.250, 0.282],
      [0.288, 0.256, 0.243, 0.251, 0.276],
      [0.281, 0.255, 0.246, 0.253, 0.271]]),
    ("fallende Laufzeitstruktur, ausgepraegter Skew",
     [45, 120, 300],
     [70.0, 100.0, 130.0],
     [[0.520, 0.400, 0.360],
      [0.470, 0.370, 0.345],
      [0.430, 0.350, 0.335]]),
    ("flache Flaeche - jede Konvention muss dieselbe Zahl liefern",
     [60, 240],
     [95.0, 105.0],
     [[0.220, 0.220],
      [0.220, 0.220]]),
]


def _queries(times, strikes):
    """Quoted grid points, midpoints in each axis, a quarter point, and a margin just outside."""
    queries = [(times[0], strikes[0]), (times[-1], strikes[-1])]
    for t in times:
        queries.append((t, strikes[len(strikes) // 2]))
    for strike in strikes:
        queries.append((times[len(times) // 2], strike))

    for row in range(len(times) - 1):
        mid_time = 0.5 * (times[row] + times[row + 1])
        queries.append((mid_time, strikes[0]))
        queries.append((mid_time, strikes[-1]))
        for column in range(len(strikes) - 1):
            queries.append((mid_time, 0.5 * (strikes[column] + strikes[column + 1])))
        # A quarter point too, so the comparison is not only made at midpoints, where many
        # schemes happen to agree.
        queries.append((times[row] + 0.25 * (times[row + 1] - times[row]),
                        strikes[0] + 0.75 * (strikes[-1] - strikes[0])))

    width = strikes[-1] - strikes[0]
    queries.append((times[0] * 0.5, strikes[0] - 0.05 * width))
    queries.append((times[-1] + 0.1, strikes[-1] + 0.05 * width))
    return queries


def build():
    blocks = [("meta", "case count", {"surface_case_count": float(len(SURFACES))})]
    for index, (comment, maturity_days, strikes, vols) in enumerate(SURFACES, start=1):
        times = [days / 365.0 for days in maturity_days]
        case = {"maturity_count": float(len(times)), "strike_count": float(len(strikes))}
        for row, t in enumerate(times):
            case[f"maturity{row}_t"] = t
            for column, strike in enumerate(strikes):
                case[f"strike{column}"] = strike
                case[f"vol{row}_{column}"] = vols[row][column]

        queries = _queries(times, strikes)
        case["query_count"] = float(len(queries))
        for query, (t, strike) in enumerate(queries):
            inside = times[0] <= t <= times[-1] and strikes[0] <= strike <= strikes[-1]
            case[f"query{query}_t"] = t
            case[f"query{query}_k"] = strike
            case[f"query{query}_inside"] = 1.0 if inside else 0.0
            case[f"query{query}_vol"] = bilinear(times, strikes, vols, t, strike)
            case[f"query{query}_variance_vol"] = total_variance_volatility(
                times, strikes, vols, t, strike
            )
        blocks.append((f"surface{index}", comment, case))
    return HEADER, blocks
