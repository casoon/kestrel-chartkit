"""golden_validation_probability: the log-loss of the calibration test and the isotonic
calibration curve with tied scores, from their documented definitions."""

import math
from bisect import bisect_left
from itertools import groupby

from ..fixture import provenance

NAME = "golden_validation_probability"
PREDICTIONS = [0.8, 0.7, 0.4, 0.1]
OUTCOMES = [True, True, False, False]

# (samples as (score, won), query scores). Ties are the point: signal strengths repeat, many
# indicators report exactly 1.0.
ISOTONIC_CASES = [
    ([(0.2, False), (0.5, False), (0.5, True), (0.9, True)],
     [0.1, 0.2, 0.35, 0.5, 0.7, 0.9, 1.0]),
    ([(1.0, True), (1.0, False), (1.0, True), (0.6, True), (0.6, False), (0.3, False), (1.0, True)],
     [0.3, 0.45, 0.6, 0.8, 1.0]),
    ([(0.4, True), (0.4, True), (0.7, False), (0.7, False), (0.7, True), (0.9, True)],
     [0.4, 0.55, 0.7, 0.8, 0.9]),
    ([(0.8, True), (0.8, False), (0.8, False)],
     [0.5, 0.8, 0.9]),
    ([(10.0, False), (20.0, True), (30.0, False), (40.0, True), (50.0, False), (60.0, True)],
     [0.0, 15.0, 25.0, 35.0, 45.0, 55.0, 75.0]),
]

HEADER = [
    "kestrel-chartkit golden reference fixture: validation and probability calibration",
    "",
    *provenance("validation_probability"),
    "",
    "Predictions [0.8, 0.7, 0.4, 0.1] for the outcomes [win, win, loss, loss]. Log-loss is the",
    "mean of -ln p for a win and -ln(1 - p) for a loss, p clamped to [1e-15, 1 - 1e-15]:",
    "-(ln 0.8 + ln 0.7 + ln 0.6 + ln 0.9) / 4.",
    "",
    "Isotonic calibration (IsotonicCalibrator). Samples with the same score form one block whose",
    "value is their win rate. Adjacent blocks are pooled while a block's value is below its",
    "predecessor's; a pooled block takes the weighted mean of its members. Every block puts its",
    "lowest score and, if different, its highest score on the curve, both with the block's value,",
    "so a score inside a pooled block maps to that block's value. Pooled here with a stack, not",
    "in place. Prediction: at or below the first threshold the first probability, at or above the",
    "last the last one, on a threshold its probability, in between linear between the",
    "neighbouring thresholds. iso<i>_score<j>/_won<j> are the samples, _blocks the number of",
    "blocks, _points/_threshold<k>/_probability<k> the curve, _query<m>/_predicted<m> predictions",
    "from it.",
    "Without pooling the ties first, iso0 would keep two blocks at score 0.5 (0.0 and 1.0).",
    "",
    "validation_probability_tolerance is the comparison tolerance the tests apply, stated rather",
    "than derived.",
]


def _isotonic(samples):
    ordered = sorted(samples, key=lambda sample: sample[0])
    blocks = []
    for score, group in groupby(ordered, key=lambda sample: sample[0]):
        outcomes = [1.0 if won else 0.0 for _, won in group]
        blocks.append((score, score, float(len(outcomes)), sum(outcomes) / len(outcomes)))
    pooled = []
    for block in blocks:
        pooled.append(block)
        while len(pooled) > 1 and pooled[-1][3] < pooled[-2][3]:
            high = pooled.pop()
            low = pooled.pop()
            weight = low[2] + high[2]
            pooled.append((low[0], high[1], weight, (low[2] * low[3] + high[2] * high[3]) / weight))
    thresholds, probabilities = [], []
    for lowest, highest, _, value in pooled:
        thresholds.append(lowest)
        probabilities.append(value)
        if highest > lowest:
            thresholds.append(highest)
            probabilities.append(value)
    return len(pooled), thresholds, probabilities


def _predict(thresholds, probabilities, score):
    if score <= thresholds[0]:
        return probabilities[0]
    if score >= thresholds[-1]:
        return probabilities[-1]
    i = bisect_left(thresholds, score)
    if thresholds[i] == score:
        return probabilities[i]
    x0, x1 = thresholds[i - 1], thresholds[i]
    y0, y1 = probabilities[i - 1], probabilities[i]
    return y0 + (score - x0) / (x1 - x0) * (y1 - y0)


def build():
    total = 0.0
    for raw, won in zip(PREDICTIONS, OUTCOMES):
        p = min(max(raw, 1e-15), 1.0 - 1e-15)
        total -= math.log(p) if won else math.log(1.0 - p)
    blocks = [(None, "calibration sample", {"log_loss": total / len(PREDICTIONS)}),
              (None, "isotonic cases", {"meta_isotonic_case_count": float(len(ISOTONIC_CASES))})]
    for i, (samples, queries) in enumerate(ISOTONIC_CASES):
        block_count, thresholds, probabilities = _isotonic(samples)
        case = {"n": float(len(samples))}
        for j, (score, won) in enumerate(samples):
            case[f"score{j}"] = score
            case[f"won{j}"] = 1.0 if won else 0.0
        case["blocks"] = float(block_count)
        case["points"] = float(len(thresholds))
        for k, (threshold, probability) in enumerate(zip(thresholds, probabilities)):
            case[f"threshold{k}"] = threshold
            case[f"probability{k}"] = probability
        case["queries"] = float(len(queries))
        for m, query in enumerate(queries):
            case[f"query{m}"] = query
            case[f"predicted{m}"] = _predict(thresholds, probabilities, query)
        blocks.append((f"iso{i}", None, case))
    blocks.append((None, None, {"validation_probability_tolerance": 1e-12}))
    return HEADER, blocks
