//! Price-unit paper accounting. No contract multiplier, FX, or intrabar fill assumptions.

/// Direction of a position or directional signal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceDirection {
    Long,
    Short,
}
impl PriceDirection {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "long" | "bull" => Some(Self::Long),
            "short" | "bear" => Some(Self::Short),
            _ => None,
        }
    }
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Long => "long",
            Self::Short => "short",
        }
    }
    pub fn pnl(self, entry: f64, exit: f64) -> f64 {
        match self {
            Self::Long => exit - entry,
            Self::Short => entry - exit,
        }
    }
    pub fn favorable(self, entry: f64, high: f64, low: f64) -> f64 {
        match self {
            Self::Long => high - entry,
            Self::Short => entry - low,
        }
    }
    pub fn adverse(self, entry: f64, high: f64, low: f64) -> f64 {
        match self {
            Self::Long => entry - low,
            Self::Short => high - entry,
        }
    }
    /// Extend non-negative running excursions with one completed bar.
    pub fn update_excursions(
        self,
        entry: f64,
        bar: PriceObservation,
        mfe: f64,
        mae: f64,
    ) -> (f64, f64) {
        (
            mfe.max(self.favorable(entry, bar.high, bar.low).max(0.0)),
            mae.max(self.adverse(entry, bar.high, bar.low).max(0.0)),
        )
    }
}

/// OHLC range relevant to price-unit evaluation; timestamps/order are the caller's responsibility.
#[derive(Debug, Clone, Copy)]
pub struct PriceObservation {
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// Recomputed from bars strictly after entry. Excursion times are one-based;
/// ties select the first observation, even when every raw excursion is negative.
#[derive(Debug, Clone, Default)]
pub struct ForwardPriceOutcome {
    pub returns: Vec<f64>,
    pub mfe: f64,
    pub mae: f64,
    pub bars_to_mfe: usize,
    pub bars_to_mae: usize,
}
impl ForwardPriceOutcome {
    pub fn compute(
        direction: PriceDirection,
        entry: f64,
        bars: &[PriceObservation],
        horizon: usize,
    ) -> Self {
        let mut out = Self::default();
        let (mut best, mut worst) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
        for (i, b) in bars.iter().take(horizon).enumerate() {
            out.returns.push(direction.pnl(entry, b.close));
            let fav = direction.favorable(entry, b.high, b.low);
            let adv = direction.adverse(entry, b.high, b.low);
            if fav > best {
                best = fav;
                out.bars_to_mfe = i + 1;
            }
            if adv > worst {
                worst = adv;
                out.bars_to_mae = i + 1;
            }
        }
        out.mfe = best.max(0.0);
        out.mae = worst.max(0.0);
        out
    }
    pub fn return_at(&self, horizon: usize) -> Option<f64> {
        horizon
            .checked_sub(1)
            .and_then(|i| self.returns.get(i).copied())
    }
}

/// Summary of price deltas within ONE instrument (or another explicitly common unit).
#[derive(Debug, Clone, Copy, Default)]
pub struct PriceStats {
    pub closed_count: i64,
    pub win_rate: f64,
    pub avg_pnl: f64,
    pub total_pnl: f64,
}
impl PriceStats {
    /// NULL outcomes count as closed, but do not enter the mean, matching stored legacy rows.
    pub fn compute(values: impl IntoIterator<Item = Option<f64>>) -> Self {
        let (mut n, mut present, mut wins, mut total) = (0, 0, 0, 0.0);
        for p in values {
            n += 1;
            if let Some(p) = p {
                present += 1;
                total += p;
                if p > 0.0 {
                    wins += 1;
                }
            }
        }
        Self {
            closed_count: n,
            win_rate: if n > 0 { wins as f64 / n as f64 } else { 0.0 },
            avg_pnl: if present > 0 {
                total / present as f64
            } else {
                0.0
            },
            total_pnl: total,
        }
    }
    /// Merge disjoint summaries in the same price unit.
    pub fn merge(values: impl IntoIterator<Item = Self>) -> Self {
        let (mut n, mut wins, mut total) = (0, 0.0, 0.0);
        for v in values {
            n += v.closed_count;
            wins += v.win_rate * v.closed_count as f64;
            total += v.total_pnl;
        }
        if n == 0 {
            return Self::default();
        }
        Self {
            closed_count: n,
            win_rate: wins / n as f64,
            avg_pnl: total / n as f64,
            total_pnl: total,
        }
    }
}

/// A completed directional outcome, normalized only during aggregation.
#[derive(Debug, Clone, Copy)]
pub struct PriceOutcomeSample {
    pub entry: f64,
    pub return_value: Option<f64>,
    pub atr: Option<f64>,
    pub mfe: f64,
    pub mae: f64,
    pub strength: f64,
}
#[derive(Debug, Clone, Default)]
pub struct PriceOutcomeStats {
    pub n: i64,
    pub hit_rate: f64,
    pub avg_ret_pct: Option<f64>,
    pub avg_ret_atr: Option<f64>,
    pub avg_mfe_pct: f64,
    pub avg_mae_pct: f64,
    pub avg_strength: f64,
}
impl PriceOutcomeStats {
    /// Entries must be positive; caller selects completed outcomes and cohorts.
    pub fn compute(values: &[PriceOutcomeSample]) -> Self {
        if values.is_empty() {
            return Self::default();
        }
        let n = values.len() as f64;
        let returns: Vec<_> = values
            .iter()
            .filter_map(|v| v.return_value.map(|r| r / v.entry * 100.0))
            .collect();
        let atr: Vec<_> = values
            .iter()
            .filter_map(|v| {
                v.return_value
                    .zip(v.atr.filter(|a| *a > 0.0))
                    .map(|(r, a)| r / a)
            })
            .collect();
        Self {
            n: values.len() as i64,
            hit_rate: values
                .iter()
                .filter(|v| v.return_value.is_some_and(|r| r > 0.0))
                .count() as f64
                / n,
            avg_ret_pct: (!returns.is_empty())
                .then(|| returns.iter().sum::<f64>() / returns.len() as f64),
            avg_ret_atr: (!atr.is_empty()).then(|| atr.iter().sum::<f64>() / atr.len() as f64),
            avg_mfe_pct: values.iter().map(|v| v.mfe / v.entry * 100.0).sum::<f64>() / n,
            avg_mae_pct: values.iter().map(|v| v.mae / v.entry * 100.0).sum::<f64>() / n,
            avg_strength: values.iter().map(|v| v.strength).sum::<f64>() / n,
        }
    }
}
