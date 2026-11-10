"""golden_applicability_pairs: the verdict of every (two-series indicator, series class, reference
class) triple.

Written from the documented pair rules (`src/applicability.rs`: `PairRequirements`,
`check_pair_applicability`), not from the Rust code. A two-series indicator compares two series;
the verdict here is about what the two must have in common, not about either series alone.

Line format: `indicator<TAB>class<TAB>reference class<TAB>verdict<TAB>explanations joined by
" || "`, with the verdicts and the ten series classes of `golden_applicability`.
"""

from .applicability import CLASSES, render

NAME = "golden_applicability_pairs"

# The session of each series class (`golden_applicability` does not carry it: no single-series
# rule reads it): regular trading hours, extended hours, or continuous trading.
SESSION = {
    "index-cash": "regular",
    "index-future": "extended",
    "future-single": "extended",
    "future-continuous": "extended",
    "cfd-broker": "continuous",
    "fx-spot": "continuous",
    "stock-adjusted": "regular",
    "stock-thin": "regular",
    "commodity-chained": "extended",
    "synthetic": "continuous",
}

# Both indicators compare the returns of two close series bar by bar: they need the same price
# adjustment and the same session on both sides.
INDICATORS = {
    "correlation": {"adjustment": True, "session": True},
    "relative_strength": {"adjustment": True, "session": True},
}

TEXT_ADJUSTMENT = (
    "Die beiden Reihen sind verschieden bereinigt: Ein Split oder eine Dividende steht nur in "
    "einer von beiden als Kurssprung, in der anderen ist er herausgerechnet; der Vergleich "
    "enthält diesen Unterschied als Kursbewegung")
TEXT_SESSION = (
    "Die beiden Reihen haben verschiedene Handelszeiten: Bars mit gleichem Zeitstempel umfassen "
    "verschiedene Zeiträume, und Zeitstempel, die nur eine Reihe führt, entfallen bei der "
    "Ausrichtung; die verglichenen Schlusskurse stammen nicht vom selben Zeitpunkt")


def verdict(indicator, cls, reference):
    """(verdict, [explanations]) of one triple."""
    need = INDICATORS[indicator]
    adjustment, reference_adjustment = CLASSES[cls][2], CLASSES[reference][2]
    degraded = []
    if need["adjustment"] and adjustment != reference_adjustment:
        degraded.append(TEXT_ADJUSTMENT)
    if need["session"] and SESSION[cls] != SESSION[reference]:
        degraded.append(TEXT_SESSION)
    return ("limited", degraded) if degraded else ("applicable", [])


def build():
    assert set(SESSION) == set(CLASSES)
    header = [
        "golden_applicability_pairs: verdict of every (two-series indicator, series class,",
        "reference class) triple, 2 x 10 x 10.",
        "",
        "Derived by reference/kestrel_reference/fixtures/applicability_pairs.py from the documented",
        "pair rules, not from the Rust code. Line:",
        "indicator<TAB>class<TAB>reference class<TAB>verdict<TAB>explanations.",
    ]
    rows = []
    for indicator in INDICATORS:
        for cls in CLASSES:
            for reference in CLASSES:
                kind, reasons = verdict(indicator, cls, reference)
                rows.append(f"{indicator}\t{cls}\t{reference}\t{kind}\t{' || '.join(reasons)}")
    assert verdict("correlation", "stock-thin", "index-cash") == ("applicable", [])
    assert verdict("relative_strength", "stock-adjusted", "stock-thin") == (
        "limited", [TEXT_ADJUSTMENT])
    assert verdict("relative_strength", "stock-adjusted", "index-future") == (
        "limited", [TEXT_ADJUSTMENT, TEXT_SESSION])
    return header, rows


RENDER = render
