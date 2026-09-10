"""golden_curve_diff: interpolated zero rates and discount factors."""

from ..curves import ZeroCurve
from ..fixture import provenance

NAME = "golden_curve_diff"

HEADER = [
    "kestrel-chartkit golden reference fixture: yield curve difference tests",
    "",
    *provenance("curve_diff"),
    "",
    "Conventions: continuously compounded zero rates, linear interpolation in time between the",
    "nodes, the first rate held flat before the first node, Actual/365Fixed. Discount factors are",
    "exp(-z(t)*t). Node and query times are whole days over 365.",
    "",
    "Each curve carries its nodes as node<i>_t / node<i>_rate and its queries as query<j>_t /",
    "query<j>_zero / query<j>_discount.",
    "",
    "query<j>_inside marks whether the query lies within the quoted node range. Beyond the last",
    "node this generator holds the instantaneous forward rate at that node flat - the established",
    "alternative, z(t) = (z_n t_n + f_n (t - t_n)) / t - while this crate holds the edge zero rate",
    "flat; those queries are therefore not comparable and are marked 0.",
]

CURVES = [
    ("rising term structure",
     [(183, 0.020), (730, 0.028), (1825, 0.033), (3650, 0.036)],
     [37, 183, 365, 730, 1278, 1825, 2738, 3650, 4380, 10950]),
    ("inverted, partly negative",
     [(91, -0.006), (365, -0.004), (1095, 0.001), (2555, 0.0)],
     [18, 91, 219, 365, 730, 1095, 1825, 2555, 3285]),
    ("single node, flat",
     [(365, 0.025)],
     [0, 183, 365, 1460, 9125]),
]


def build():
    blocks = [("meta", "case count", {"curve_case_count": float(len(CURVES))})]
    for index, (comment, nodes, queries) in enumerate(CURVES, start=1):
        node_times = [(days / 365.0, rate) for days, rate in nodes]
        curve = ZeroCurve(node_times)
        case = {"node_count": float(len(nodes)), "query_count": float(len(queries))}
        for node_index, (t, rate) in enumerate(node_times):
            case[f"node{node_index}_t"] = t
            case[f"node{node_index}_rate"] = rate
        last_node = node_times[-1][0]
        for query_index, days in enumerate(queries):
            t = days / 365.0
            case[f"query{query_index}_t"] = t
            case[f"query{query_index}_zero"] = curve.zero(t)
            case[f"query{query_index}_discount"] = curve.discount(t)
            case[f"query{query_index}_inside"] = 1.0 if t <= last_node else 0.0
        blocks.append((f"curve{index}", comment, case))
    return HEADER, blocks
