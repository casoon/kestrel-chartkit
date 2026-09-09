use std::collections::HashMap;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::SignalDirection as Direction;

/// One active directional statement with a caller-defined timeframe key.
/// Strength must be finite and non-negative; the caller chooses active statements.
#[derive(Debug, Clone, PartialEq)]
pub struct DirectionalStatement<T> {
    pub tf: T,
    pub direction: Direction,
    pub strength: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Agreement {
    pub direction: Direction,
    /// 0..1 — how one-sided the aggregated statements were.
    ///
    /// Named `agreement` until 2026-09-08. The name promised what the
    /// number does not deliver: consumers rendered it as "90 % confidence",
    /// which reads as a win probability. It is a share of agreeing
    /// statements — five agreeing indicators can be wrong together, and
    /// this value says nothing about how often that happens. For the
    /// quantity that does, see `evaluation::calibration`, where
    /// `confidence` is used in its proper statistical sense.
    pub agreement: f64,
    /// True when `direction` is `Neutral` because opposing statements were
    /// evenly matched — even after the timeframe-hierarchy tie-break — as
    /// opposed to plain "no active statements". The dashboard shows this as
    /// an explicit "Widerspruch" rather than folding it into "Neutral".
    pub conflict: bool,
}

/// Ordered aggregation stages: filters apply in order, the last tally stage wins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "snake_case")
)]
pub enum AgreementStrategy {
    /// Tally: each active statement counts once, direction with the most
    /// votes wins; agreement = winning share of all non-neutral votes.
    #[default]
    Majority,
    /// Tally: each active statement counts `strength` instead of 1;
    /// agreement = winning direction's share of total strength.
    WeightedByStrength,
    /// Pre-filter: the highest-priority timeframe (first entry of
    /// `tf_order` that has any active statement) sets the bias direction.
    /// Statements from other timeframes are dropped unless they agree —
    /// "erst 4H-Bias, dann 1H/15M nur als Bestätigung".
    TimeframeTopDown,
}

/// Applies `strategies` in order to `statements`, returning the resulting
/// `Agreement`. `tf_order` is highest-priority timeframe first (e.g.
/// `[Hour4, Hour1, Minute15]`); also used to break tally ties.
///
/// Empty `statements` (nothing active) yields `Neutral`/`0.0`, not an
/// error — "no signal" is a valid, common state, not a degenerate one.
pub fn aggregate_agreement<T: Copy + Eq>(
    mut statements: Vec<DirectionalStatement<T>>,
    strategies: &[AgreementStrategy],
    tf_order: &[T],
) -> Agreement {
    let mut tally_by_strength = false;

    for strategy in strategies {
        match strategy {
            AgreementStrategy::TimeframeTopDown => {
                statements = apply_top_down(statements, tf_order);
            }
            AgreementStrategy::Majority => tally_by_strength = false,
            AgreementStrategy::WeightedByStrength => tally_by_strength = true,
        }
    }

    let result = tally(&statements, tally_by_strength);
    if result.direction != Direction::Neutral {
        return result;
    }

    // A tie isn't automatically a dead end — the timeframe hierarchy (4H >
    // 1H > 15M, `tf_order`) can still break it even if the configured
    // strategies didn't already apply it. Only once that also fails to
    // produce a direction do we call it an unresolved conflict rather than
    // plain "no signal".
    let tie_broken = tally(
        &apply_top_down(statements.clone(), tf_order),
        tally_by_strength,
    );
    if tie_broken.direction != Direction::Neutral {
        return tie_broken;
    }

    Agreement {
        conflict: has_opposing_statements(&statements),
        ..result
    }
}

fn has_opposing_statements<T>(statements: &[DirectionalStatement<T>]) -> bool {
    let (mut bull, mut bear) = (false, false);
    for s in statements {
        match s.direction {
            Direction::Bullish => bull = true,
            Direction::Bearish => bear = true,
            Direction::Neutral => {}
        }
    }
    bull && bear
}

/// Finds the highest-priority timeframe (per `tf_order`) that has at least
/// one non-neutral statement, takes ITS majority direction as the bias, and
/// drops every statement (on any timeframe) that disagrees with that bias.
/// Once a bias is found, all opposing statements are removed, including
/// those whose timeframe is absent from `tf_order`.
fn apply_top_down<T: Copy + Eq>(
    statements: Vec<DirectionalStatement<T>>,
    tf_order: &[T],
) -> Vec<DirectionalStatement<T>> {
    let bias = tf_order.iter().find_map(|&tf| {
        let on_tf: Vec<&DirectionalStatement<T>> =
            statements.iter().filter(|s| s.tf == tf).collect();
        if on_tf.is_empty() {
            return None;
        }
        match tally_direction(on_tf.into_iter().cloned(), false) {
            Direction::Neutral => None,
            dir => Some(dir),
        }
    });

    match bias {
        None => statements,
        Some(bias) => statements
            .into_iter()
            .filter(|s| s.direction == bias)
            .collect(),
    }
}

fn tally<T: Clone>(statements: &[DirectionalStatement<T>], by_strength: bool) -> Agreement {
    let direction = tally_direction(statements.iter().cloned(), by_strength);

    let (bull, bear): (f64, f64) = statements.iter().fold((0.0, 0.0), |(b, r), s| {
        let weight = if by_strength { s.strength } else { 1.0 };
        match s.direction {
            Direction::Bullish => (b + weight, r),
            Direction::Bearish => (b, r + weight),
            Direction::Neutral => (b, r),
        }
    });
    let total = bull + bear;
    let agreement = if total > 0.0 {
        (bull.max(bear)) / total
    } else {
        0.0
    };

    Agreement {
        direction,
        agreement,
        conflict: false,
    }
}

fn tally_direction<T>(
    statements: impl Iterator<Item = DirectionalStatement<T>>,
    by_strength: bool,
) -> Direction {
    let mut weight: HashMap<Direction, f64> = HashMap::new();
    for s in statements {
        if s.direction == Direction::Neutral {
            continue;
        }
        let w = if by_strength { s.strength } else { 1.0 };
        *weight.entry(s.direction).or_insert(0.0) += w;
    }
    let bull = weight.get(&Direction::Bullish).copied().unwrap_or(0.0);
    let bear = weight.get(&Direction::Bearish).copied().unwrap_or(0.0);
    if bull == 0.0 && bear == 0.0 {
        Direction::Neutral
    } else if bull > bear {
        Direction::Bullish
    } else if bear > bull {
        Direction::Bearish
    } else {
        Direction::Neutral // exact tie — no basis to prefer either side
    }
}
