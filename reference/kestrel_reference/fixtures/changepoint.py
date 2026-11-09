"""golden_changepoint: the two-sided CUSUM (E. S. Page 1954), from its definition."""

from fractions import Fraction

from ..fixture import provenance

NAME = "golden_changepoint"

# (values, mu0, k, h): decimals as strings, evaluated in exact rational arithmetic so a sum that
# lands exactly on h is decided exactly.
CASES = [
    # Twelve standardized returns; the mean steps from about 0 to about 1.2 after the fifth value.
    # By hand (mu0 = 0, k = 0.5, h = 4): S = 0 0 0 0 0.1 0.9 1.5 2.6 3.1 4.0 4.7 5.1; the alarm
    # comes with the 11th value (index 10), because S = 4.0 is not above h.
    (["0.3", "-0.8", "0.5", "-0.2", "0.6", "1.3", "1.1", "1.6", "1.0", "1.4", "1.2", "0.9"],
     "0", "0.5", "4"),
    # One outlier of 4.2 < k + h = 4.5: S peaks at 3.8 and decays to 0, no alarm.
    (["0.3", "-0.8", "0.5", "-0.2", "0.6", "4.2", "-0.4", "0.1", "-0.6", "0.3", "-0.5", "0.2"],
     "0", "0.5", "4"),
    # The first case negated value by value: upper and lower swap roles.
    (["-0.3", "0.8", "-0.5", "0.2", "-0.6", "-1.3", "-1.1", "-1.6", "-1.0", "-1.4", "-1.2", "-0.9"],
     "0", "0.5", "4"),
    # A reference value other than 0 and a smaller tolerance; both sides move.
    (["2.4", "1.9", "2.1", "3.0", "3.4", "3.1", "2.2", "1.0", "0.6", "0.4", "0.9", "1.1"],
     "2", "0.25", "1.5"),
    # S reaches h exactly (1, 2, 3 against h = 3) and never exceeds it: no alarm.
    (["1.5", "1.5", "1.5"], "0", "0.5", "3"),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: two-sided CUSUM",
    "",
    *provenance("changepoint"),
    "",
    "Definition: S+[t] = max(0, S+[t-1] + x[t] - mu0 - k) and S-[t] = max(0, S-[t-1] - x[t] + mu0 - k),",
    "S[-1] = 0; an alarm is the first t with S[t] > h (strict, the sums are never reset).",
    "",
    "The reference does not run that recursion. It uses the equivalent closed form",
    "S+[t] = max(0, max over s <= t of sum_{i=s..t} (x[i] - mu0 - k)) and S-[t] likewise with",
    "(mu0 - x[i] - k), in exact rational arithmetic (fractions.Fraction from the decimal text of the",
    "inputs), then rounds each result to the nearest double. Cases 0 and 1 are also checked here",
    "against the values worked out by hand (see the case comments).",
    "",
    "cusum<i>_n values cusum<i>_x<j> with cusum<i>_mu0/_k/_h; _upper<j> and _lower<j> are the sums",
    "after each value; _alarm_upper/_alarm_lower the 0-based index of the first alarm, -1 for none.",
]


def _closed_form(terms):
    """For each t the largest suffix sum of `terms` ending at t, floored at 0."""
    out = []
    for t in range(len(terms)):
        best = Fraction(0)
        total = Fraction(0)
        for s in range(t, -1, -1):
            total += terms[s]
            best = max(best, total)
        out.append(best)
    return out


def _first_alarm(sums, h):
    return next((i for i, s in enumerate(sums) if s > h), -1)


def build():
    blocks = [(None, "case count", {"meta_case_count": float(len(CASES))})]
    for i, (texts, mu0_text, k_text, h_text) in enumerate(CASES):
        x = [Fraction(v) for v in texts]
        mu0, k, h = Fraction(mu0_text), Fraction(k_text), Fraction(h_text)
        upper = _closed_form([v - mu0 - k for v in x])
        lower = _closed_form([mu0 - v - k for v in x])
        if i == 0:
            assert [float(s) for s in upper] == [0, 0, 0, 0, 0.1, 0.9, 1.5, 2.6, 3.1, 4.0, 4.7, 5.1]
            assert _first_alarm(upper, h) == 10
        if i == 1:
            assert float(max(upper)) == 3.8 and _first_alarm(upper, h) == -1
        case = {"n": float(len(x)), "mu0": float(mu0), "k": float(k), "h": float(h)}
        case.update({f"x{j}": float(v) for j, v in enumerate(x)})
        case.update({f"upper{j}": float(s) for j, s in enumerate(upper)})
        case.update({f"lower{j}": float(s) for j, s in enumerate(lower)})
        case["alarm_upper"] = float(_first_alarm(upper, h))
        case["alarm_lower"] = float(_first_alarm(lower, h))
        blocks.append((f"cusum{i}", None, case))
    blocks.append((None, None, {"changepoint_tolerance": 1e-12}))
    return HEADER, blocks
