"""golden_validation_probability: the log-loss of the calibration test, from its documented
definition."""

import math

from ..fixture import provenance

NAME = "golden_validation_probability"
PREDICTIONS = [0.8, 0.7, 0.4, 0.1]
OUTCOMES = [True, True, False, False]

HEADER = [
    "kestrel-chartkit golden reference fixture: validation and probability calibration",
    "",
    *provenance("validation_probability"),
    "",
    "Predictions [0.8, 0.7, 0.4, 0.1] for the outcomes [win, win, loss, loss]. Log-loss is the",
    "mean of -ln p for a win and -ln(1 - p) for a loss, p clamped to [1e-15, 1 - 1e-15]:",
    "-(ln 0.8 + ln 0.7 + ln 0.6 + ln 0.9) / 4.",
    "",
    "validation_probability_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def build():
    total = 0.0
    for raw, won in zip(PREDICTIONS, OUTCOMES):
        p = min(max(raw, 1e-15), 1.0 - 1e-15)
        total -= math.log(p) if won else math.log(1.0 - p)
    case = {"log_loss": total / len(PREDICTIONS)}
    return HEADER, [(None, "calibration sample", case),
                    (None, None, {"validation_probability_tolerance": 1e-12})]
