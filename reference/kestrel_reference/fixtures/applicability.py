"""golden_applicability: the applicability verdict of every (indicator, series class) pair.

Written from the documented rules (`src/applicability.rs`, module and rule docs), not from the
Rust code: each series class is a set of declared properties, each indicator a set of declared
requirements, and a verdict follows from which rule conditions hold.

Two files come out of this module:

- `golden_applicability.txt`: the complete matrix (20 indicators x 10 series classes).
- `golden_applicability_v0_35.txt`: the part of the matrix that existed in 0.35.0 (17 indicators x
  9 series classes, every indicator except those with a named structure method), computed with
  the eight generic rules only. It was checked against the output of the 0.35.0 tag once; the Rust test
  `unchanged_indicators_keep_their_v0_35_rows` keeps it from changing silently.

Line format: `indicator<TAB>class<TAB>verdict<TAB>explanations joined by " || "`, where the verdict
is `applicable`, `limited` (degraded) or `misleading` (unsuitable) and the explanations are listed
in rule order, unsuitable rules first.
"""

NAME = "golden_applicability"
NAME_FROZEN = "golden_applicability_v0_35"

# --- series classes: declared properties --------------------------------------------------------
# volume, continuity, price_adjustment, provenance, liquidity_tier, session. (trade_direction is
# false for all of them; `session` is read by the gap rules only.)
CLASSES = {
    "index-cash": ("none", "spot", "raw", "exchange", "deep", "regular"),
    "index-future": ("real", "single", "raw", "exchange", "deep", "extended"),
    "future-single": ("real", "single", "raw", "exchange", "normal", "extended"),
    "future-continuous": ("real", "back_adjusted", "raw", "exchange", "deep", "extended"),
    "cfd-broker": ("tick", "spot", "raw", "broker", "unknown", "continuous"),
    "fx-spot": ("tick", "spot", "raw", "broker", "deep", "continuous"),
    "stock-adjusted": ("real", "spot", "split_and_dividend", "exchange", "deep", "regular"),
    "stock-thin": ("real", "spot", "raw", "exchange", "thin", "regular"),
    "commodity-chained": ("real", "unadjusted", "raw", "exchange", "normal", "extended"),
    "synthetic": ("real", "spot", "raw", "synthetic", "normal", "continuous"),
}
FROZEN_CLASSES = [c for c in CLASSES if c != "future-single"]

# --- indicators: declared requirements ----------------------------------------------------------
# volume = needs real turnover; session/roll/adjustment = the generic sensitivities;
# method = a named structure method with rules of its own.
EXTREMES = {"session": True, "roll": True, "adjustment": True}
INDICATORS = {
    "rsi": {},
    "macd": {},
    "atr": {},
    "adx": {},
    "bollinger": {},
    "volume_profile": {"volume": True, "roll": True},
    "vwap": {"volume": True, "session": True},
    "anchored_vwap": {"volume": True, "session": True},
    "mfi": {"volume": True},
    "cvd": {"volume": True},
    "buy_sell_pressure": {},
    "klinger": {"volume": True},
    "elder_ray": {},
    "pivots_structure": EXTREMES,
    "swing_structure": EXTREMES,
    "zigzag": EXTREMES,
    "elliott": {"method": "elliott"},
    "wyckoff": {"volume": True, "method": "wyckoff"},
    "gap": {"method": "gap"},
    "pivot_sets": EXTREMES,
}
FROZEN_INDICATORS = [i for i in INDICATORS if "method" not in INDICATORS[i]]

# --- the generic rules, in evaluation order ------------------------------------------------------
TEXT_TICK = "zeigt Update-Häufigkeit, nicht Umsatz"
TEXT_NO_VOLUME = ("die Serie hat kein Volumen; ein Index ist ein berechneter Wert — "
                  "Volumen-Referenz wäre Future oder ETF")
TEXT_BACK_ADJUSTED = "historische Niveaus sind nicht die gehandelten"
TEXT_ROLL_JUMPS = "Roll-Sprünge in der Reihe"
TEXT_ADJUSTED = "historische Marken liegen in der bereinigten Reihe an anderen Zahlen"
TEXT_EXTREME = "Extreme sind anbieterspezifisch, eine Auszeichnung ist nicht übertragbar"

# --- the rules of the named structure methods ----------------------------------------------------
# (method, series property) -> explanation. Properties: roll_jumps = unadjusted stitched series,
# back_adjusted = back-adjusted stitched series, adjusted = any split/dividend adjustment,
# broker = broker provenance, finite = single contract, continuous = no session boundary.
METHOD_TEXT = {
    ("elliott", "roll_jumps"): (
        "Roll-Sprünge zwischen den Kontrakten stehen als Kursbewegung in der Reihe und würden "
        "als Wellenbewegung gezählt"),
    ("elliott", "back_adjusted"): (
        "Die Rückwärtsbereinigung verschiebt frühere Kurse additiv: Längenvergleiche der "
        "Regelprüfung (Differenzen) bleiben erhalten, prozentuale Schwellen und Verhältnisse "
        "zum Kursniveau nicht; Wellenpunkte liegen auf nie gehandelten Preisen, und Niveaus "
        "können negativ werden"),
    ("elliott", "adjusted"): (
        "Die Bereinigung skaliert frühere Kurse proportional: prozentuale Verhältnisse bleiben "
        "erhalten, absolute Differenzen vor und nach dem Stichtag nicht — Längenvergleiche wie "
        "„Welle 3 nie die kürzeste“ vergleichen über den Stichtag hinweg verschiedene Maßstäbe"),
    ("elliott", "broker"): (
        "Hoch und Tief der Wellenpunkte sind Quotes dieses Anbieters und gelten nicht am "
        "gehandelten Markt; bei einem anderen Anbieter ergeben sich andere Extreme und damit "
        "andere Überlappungen"),
    ("elliott", "finite"): (
        "Die Reihe umfasst nur die Laufzeit eines Kontrakts und endet mit dem Verfall; die Zahl "
        "der Swings, aus denen höhere Wellengrade gebildet werden, ist dadurch begrenzt"),
    ("wyckoff", "roll_jumps"): (
        "Ein Roll-Sprung verschiebt die Kurse gegen die bereits gesetzten Spannengrenzen und "
        "weitet die ATR, in der jede Distanz gemessen wird, und der Umsatz wechselt mit dem "
        "Kontrakt: Climax-Test und Sign of Strength messen an einer Umsatzschwelle aus Bars "
        "verschiedener Kontrakte"),
    ("wyckoff", "back_adjusted"): (
        "Die Preise sind über den Kontraktwechsel angeglichen, der Umsatz nicht: Climax-Test, "
        "Secondary Test und Sign of Strength vergleichen an der Roll-Grenze Umsatz verschiedener "
        "Kontrakte, und die Spannengrenzen liegen vor dem Wechsel auf nie gehandelten Preisen"),
    ("wyckoff", "broker"): (
        "Spannengrenzen, Spring und UTAD hängen an einzelnen Hochs und Tiefs (Extrem der "
        "Climax-Bar und der Gegenbewegung, Durchstoß einer einzelnen Bar); diese Extreme sind "
        "Quotes des Anbieters und gelten nicht am gehandelten Markt"),
    ("gap", "roll_jumps"): (
        "Ein Kontraktwechsel steht als Kurssprung zwischen zwei Bars in der Reihe und wird als "
        "Kurslücke gemeldet, ohne dass sich der Markt bewegt hat"),
    ("gap", "back_adjusted"): (
        "Die Rückwärtsbereinigung verschiebt frühere Kurse additiv: Abstände und damit die Größe "
        "früherer Lücken bleiben erhalten, ihre Ränder liegen auf nie gehandelten Preisen; ob an "
        "der Roll-Grenze eine Lücke steht, hängt daran, welche Kurse der beiden Kontrakte die "
        "Bereinigung angleicht"),
    ("gap", "adjusted"): (
        "Die Bereinigung skaliert frühere Kurse proportional: Ränder und absolute Größe früherer "
        "Lücken stehen an anderen Zahlen als gehandelt, und der Kurssprung am Stichtag, den die "
        "Bereinigung herausnimmt, steht nicht als Lücke in der Reihe"),
    ("gap", "broker"): (
        "Eröffnung, Hoch und Tief sind Quotes des Anbieters: ob zwischen zwei Bars eine Lücke "
        "steht und wie groß sie ist, hängt an dessen Kursstellung und Handelszeiten und gilt "
        "nicht am gehandelten Markt"),
    ("gap", "continuous"): (
        "Die Reihe hat keine Sitzungsgrenze: zwischen Schluss und nächster Eröffnung liegt keine "
        "Handelspause, und eine Lücke steht nur dort, wo die Reihe unterbrochen ist (Wochenende, "
        "Ausfall der Kursstellung)"),
}
# Which properties each method reads (Wyckoff has no rule for adjusted prices or finite life;
# only the gaps read the session).
METHOD_PROPERTIES = {
    "elliott": ["roll_jumps", "back_adjusted", "adjusted", "broker", "finite"],
    "wyckoff": ["roll_jumps", "back_adjusted", "broker"],
    "gap": ["roll_jumps", "back_adjusted", "adjusted", "broker", "continuous"],
}


def property_holds(prop, cls):
    volume, continuity, adjustment, provenance, tier, session = CLASSES[cls]
    return {
        "roll_jumps": continuity == "unadjusted",
        "back_adjusted": continuity == "back_adjusted",
        "adjusted": adjustment != "raw",
        "broker": provenance == "broker",
        "finite": continuity == "single",
        "continuous": session == "continuous",
    }[prop]


def verdict(indicator, cls):
    """(verdict, [explanations]) of one pair."""
    need = INDICATORS[indicator]
    volume, continuity, adjustment, provenance, tier, session = CLASSES[cls]
    unsuitable, degraded = [], []
    if need.get("volume") and volume == "tick":
        unsuitable.append(TEXT_TICK)
    if need.get("volume") and volume == "none":
        unsuitable.append(TEXT_NO_VOLUME)
    if need.get("roll") and continuity == "back_adjusted":
        degraded.append(TEXT_BACK_ADJUSTED)
    if need.get("roll") and continuity == "unadjusted":
        degraded.append(TEXT_ROLL_JUMPS)
    if need.get("adjustment") and adjustment != "raw":
        degraded.append(TEXT_ADJUSTED)
    if (need.get("session") or need.get("roll")) and provenance == "broker":
        degraded.append(TEXT_EXTREME)
    method = need.get("method")
    if method:
        for prop in METHOD_PROPERTIES[method]:
            if property_holds(prop, cls):
                degraded.append(METHOD_TEXT[(method, prop)])
    if unsuitable:
        return "misleading", unsuitable + degraded
    if degraded:
        return "limited", degraded
    return "applicable", []


def lines(indicators, classes):
    out = []
    for indicator in indicators:
        for cls in classes:
            kind, reasons = verdict(indicator, cls)
            out.append(f"{indicator}\t{cls}\t{kind}\t{' || '.join(reasons)}")
    return out


def render(header, rows):
    return "\n".join([f"# {line}".rstrip() for line in header] + rows) + "\n"


def build():
    header = [
        "golden_applicability: verdict of every (indicator, series class) pair, all 20 x 10.",
        "",
        "Derived by reference/kestrel_reference/fixtures/applicability.py from the documented",
        "rules, not from the Rust code. Line: indicator<TAB>class<TAB>verdict<TAB>explanations.",
    ]
    return header, lines(INDICATORS, CLASSES)


def build_frozen():
    header = [
        "golden_applicability_v0_35: the matrix as kestrel-chartkit 0.35.0 produced it",
        "(17 indicators x 9 series classes, generic rules only). Checked against the output of",
        "the v0.35.0 tag; the test keeps these rows from changing silently.",
        "",
        "Derived by reference/kestrel_reference/fixtures/applicability.py. Line:",
        "indicator<TAB>class<TAB>verdict<TAB>explanations.",
    ]
    return header, lines(FROZEN_INDICATORS, FROZEN_CLASSES)


RENDER = render
