use std::collections::{HashMap, VecDeque};

use crate::model::Bar;

use super::{Indicator, IndicatorOutput};

const SECONDS_PER_DAY: i64 = 86_400;

/// Relative Volume at Time: this bar's volume against the volume at the *same time of day* on
/// previous days.
///
/// The rolling [`super::volume_indicators::RvolEngine`] answers a different question — how this
/// bar compares to the recent ones, whatever hour they fell in. At 09:31 that comparison is
/// dominated by the opening bar and says little. Here the comparison runs down the same column of
/// the clock instead: 09:31 today against 09:31 on each of the previous `days` days.
///
/// Two ratios come out of it, and they are not interchangeable:
/// - `value`: **regular** — this bar's volume divided by the average volume in the same slot on
///   previous days.
/// - `extra["cumulative"]`: **cumulative** — the volume accumulated since the start of today
///   divided by the average of what had accumulated by the same slot on previous days. This is
///   the one that answers "is today busy so far", and it is far less jumpy than the regular one.
///
/// The day starts at `day_start_offset` seconds after midnight UTC, so a market whose session
/// does not align with UTC midnight can be placed correctly. A slot is the number of seconds into
/// that day, which makes bars of any spacing line up as long as they arrive at consistent times.
///
/// **Only days strictly before today count as comparison.** Today's own bars never enter their
/// own reference, so no value depends on data that did not exist when the bar closed.
///
/// How many comparison days actually contributed is published rather than assumed:
/// - `extra["samples"]`: number of previous days that had a bar in this slot.
/// - `extra["complete"]`: `1.0` when that number equals `days`, `0.0` otherwise.
///
/// A missing slot lowers `samples`; it is never filled in from a neighbouring slot or an earlier
/// day. A shortened trading day therefore shows up as an incomplete comparison instead of a
/// quietly wrong ratio.
///
/// Unit: a ratio, where `1.0` means "as much as usual at this time". Zero average volume in the
/// comparison slots produces no output — a ratio against nothing has no meaning.
///
/// First output: the first bar whose slot has at least one previous day to compare against —
/// one day of bars. How many bars that is depends on the timeframe, which this indicator cannot
/// know from a timestamp alone, so `bar_seconds` states it. That parameter is *declarative only*:
/// it makes [`Indicator::warmup_period`] an honest number of bars and has no influence on the
/// calculation, which always derives slots from timestamps. A wrong value misstates the declared
/// warmup, never a ratio.
///
/// Memory is bounded by pruning slots untouched for more than `days` days.
#[derive(Debug, Clone)]
pub struct RelativeVolumeAtTime {
    days: usize,
    day_start_offset: i64,
    bar_seconds: i64,
    /// Per slot, the most recent days' `(day index, bar volume, cumulative volume by that slot)`.
    history: HashMap<i64, VecDeque<(i64, f64, f64)>>,
    current_day: Option<i64>,
    cumulative_today: f64,
}

impl RelativeVolumeAtTime {
    pub fn new(days: usize, day_start_offset: i64, bar_seconds: i64) -> Self {
        Self {
            days: days.max(1),
            day_start_offset,
            bar_seconds: bar_seconds.clamp(1, SECONDS_PER_DAY),
            history: HashMap::new(),
            current_day: None,
            cumulative_today: 0.0,
        }
    }

    pub fn with_defaults() -> Self {
        Self::new(10, 0, 60)
    }

    fn day_and_slot(&self, timestamp: i64) -> (i64, i64) {
        let shifted = timestamp - self.day_start_offset;
        (
            shifted.div_euclid(SECONDS_PER_DAY),
            shifted.rem_euclid(SECONDS_PER_DAY),
        )
    }

    /// Drops slots whose newest entry is older than the comparison window; without this the map
    /// would keep one entry per distinct time of day ever seen.
    fn prune(&mut self, current_day: i64) {
        let horizon = current_day - self.days as i64;
        self.history.retain(|_, entries| {
            entries.retain(|(day, _, _)| *day >= horizon);
            !entries.is_empty()
        });
    }
}

impl Indicator for RelativeVolumeAtTime {
    fn name(&self) -> &str {
        "rvat"
    }

    fn warmup_period(&self) -> usize {
        // One day of bars: a slot cannot repeat sooner than that. Expressed in bars via the
        // declared `bar_seconds`, since the trait speaks in bars and this indicator thinks in
        // days.
        (SECONDS_PER_DAY / self.bar_seconds) as usize
    }

    fn on_bar(&mut self, bar: &Bar) -> Option<IndicatorOutput> {
        if !bar.volume.is_finite() || bar.volume < 0.0 {
            return None;
        }

        let (day, slot) = self.day_and_slot(bar.timestamp);
        if self.current_day != Some(day) {
            self.current_day = Some(day);
            self.cumulative_today = 0.0;
            self.prune(day);
        }
        self.cumulative_today += bar.volume;

        let entries = self.history.entry(slot).or_default();
        // Only previous days are comparison material; today's own entry is appended afterwards.
        let previous: Vec<(f64, f64)> = entries
            .iter()
            .filter(|(entry_day, _, _)| *entry_day < day)
            .map(|(_, volume, cumulative)| (*volume, *cumulative))
            .collect();

        entries.push_back((day, bar.volume, self.cumulative_today));
        while entries.len() > self.days + 1 {
            entries.pop_front();
        }

        let samples = previous.len();
        if samples == 0 {
            return None;
        }

        let average_volume = previous.iter().map(|(v, _)| v).sum::<f64>() / samples as f64;
        let average_cumulative = previous.iter().map(|(_, c)| c).sum::<f64>() / samples as f64;
        if average_volume <= 0.0 || average_cumulative <= 0.0 {
            return None;
        }

        let mut extra = HashMap::new();
        extra.insert(
            "cumulative".to_string(),
            self.cumulative_today / average_cumulative,
        );
        extra.insert("samples".to_string(), samples as f64);
        extra.insert(
            "complete".to_string(),
            if samples >= self.days { 1.0 } else { 0.0 },
        );

        Some(IndicatorOutput::with_extra(
            bar.volume / average_volume,
            extra,
        ))
    }

    fn reset(&mut self) {
        self.history.clear();
        self.current_day = None;
        self.cumulative_today = 0.0;
    }
}
