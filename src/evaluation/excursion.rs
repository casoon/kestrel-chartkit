//! Forward excursion summaries and an explicitly uncalibrated MFE >= MAE proxy.
//! Input timestamps are epoch milliseconds, preserving subsecond alignment.

#[cfg(feature = "serde")]
use serde::Serialize;

/// A bar with an explicit epoch-millisecond close timestamp.
#[derive(Debug, Clone, Copy)]
pub struct TimedPriceObservation {
    pub ts: i64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
}

/// A fired signal reduced to what the outcome math needs: when it fired and
/// which way it pointed (`+1` bull / long, `-1` bear / short).
#[derive(Debug, Clone, Copy)]
pub struct SignalRef {
    pub ts: i64,
    pub direction: i8,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct HourBucket {
    /// Hour of day (0..23, UTC) the signal fired in.
    pub hour: u8,
    pub count: i64,
    pub avg_mfe_pct: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct OutcomeSummary {
    /// Signals with enough forward bars to be evaluated (`horizon` bars).
    pub count: i64,
    pub avg_mfe_pct: f64,
    pub avg_mae_pct: f64,
    /// Proxy win rate: fraction of settled signals whose favorable excursion
    /// exceeded its adverse one (mfe ≥ mae). Not a calibrated success metric
    /// — see the module documentation.
    pub win_rate: f64,
    /// How many forward bars each signal was measured over.
    pub horizon: usize,
    pub by_hour: Vec<HourBucket>,
}

/// Evaluates every `signal` against `bars` (oldest first, same timeframe),
/// looking `horizon` bars forward from the first bar at or after the signal.
/// Entry is that bar's close; excursions are measured on the following bars'
/// highs/lows, expressed as a percentage of entry.
pub fn evaluate_outcomes(
    signals: &[SignalRef],
    bars: &[TimedPriceObservation],
    horizon: usize,
) -> OutcomeSummary {
    let mut count = 0_i64;
    let mut sum_mfe = 0.0;
    let mut sum_mae = 0.0;
    let mut wins = 0_i64;
    // hour -> (count, sum_mfe_pct)
    let mut hours: [(i64, f64); 24] = [(0, 0.0); 24];

    for sig in signals {
        let Some(idx) = entry_index(bars, sig.ts) else {
            continue;
        };
        // Need `horizon` bars strictly after the entry bar to settle.
        if idx + horizon >= bars.len() {
            continue;
        }
        let entry = bars[idx].close;
        if entry <= 0.0 {
            continue;
        }
        let forward = &bars[idx + 1..=idx + horizon];
        let samples: Vec<_> = forward
            .iter()
            .map(|b| super::price::PriceObservation {
                high: b.high,
                low: b.low,
                close: b.close,
            })
            .collect();
        let direction = if sig.direction >= 0 {
            super::price::PriceDirection::Long
        } else {
            super::price::PriceDirection::Short
        };
        let outcome =
            super::price::ForwardPriceOutcome::compute(direction, entry, &samples, horizon);
        let mfe_pct = 100.0 * outcome.mfe / entry;
        let mae_pct = 100.0 * outcome.mae / entry;

        count += 1;
        sum_mfe += mfe_pct;
        sum_mae += mae_pct;
        if mfe_pct >= mae_pct {
            wins += 1;
        }
        let hour = hour_of(sig.ts);
        hours[hour as usize].0 += 1;
        hours[hour as usize].1 += mfe_pct;
    }

    let by_hour = hours
        .iter()
        .enumerate()
        .filter(|(_, (c, _))| *c > 0)
        .map(|(hour, (c, sum))| HourBucket {
            hour: hour as u8,
            count: *c,
            avg_mfe_pct: sum / *c as f64,
        })
        .collect();

    OutcomeSummary {
        count,
        avg_mfe_pct: if count > 0 {
            sum_mfe / count as f64
        } else {
            0.0
        },
        avg_mae_pct: if count > 0 {
            sum_mae / count as f64
        } else {
            0.0
        },
        win_rate: if count > 0 {
            wins as f64 / count as f64
        } else {
            0.0
        },
        horizon,
        by_hour,
    }
}

/// First bar index whose `ts` is ≥ the signal's `ts` (bars sorted ascending).
fn entry_index(bars: &[TimedPriceObservation], ts: i64) -> Option<usize> {
    let pos = bars.partition_point(|b| b.ts < ts);
    (pos < bars.len()).then_some(pos)
}

fn hour_of(ts_ms: i64) -> u8 {
    let secs = ts_ms.div_euclid(1000);
    (secs.div_euclid(3600).rem_euclid(24)) as u8
}
