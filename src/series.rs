use std::collections::VecDeque;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Sliding historical lookback buffer for streaming time series values.
/// Supports 0-indexed reverse access where `0` is the current bar, `1` is the previous bar, etc.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Series<T> {
    capacity: usize,
    buffer: VecDeque<T>,
}

impl<T> Series<T> {
    /// Creates a new `Series` with a maximum lookback history capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            buffer: VecDeque::with_capacity(capacity.max(1)),
        }
    }

    /// Pushes a new value onto the series, evicting the oldest value if capacity is reached.
    pub fn push(&mut self, value: T) {
        if self.buffer.len() >= self.capacity {
            self.buffer.pop_back();
        }
        self.buffer.push_front(value);
    }

    /// Returns a reference to the value at `offset` bars ago (0 = current, 1 = previous, etc.).
    pub fn get(&self, offset: usize) -> Option<&T> {
        self.buffer.get(offset)
    }

    /// Returns a reference to the latest value (offset 0).
    pub fn latest(&self) -> Option<&T> {
        self.buffer.front()
    }

    /// Returns the number of items currently in history.
    pub fn len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns true if the series is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Clears the series history.
    pub fn reset(&mut self) {
        self.buffer.clear();
    }

    /// Returns the number of bars since `predicate` evaluated to true.
    pub fn barssince<F>(&self, predicate: F) -> Option<usize>
    where
        F: Fn(&T) -> bool,
    {
        for (i, val) in self.buffer.iter().enumerate() {
            if predicate(val) {
                return Some(i);
            }
        }
        None
    }

    /// Returns the value when `predicate` evaluated to true `occurrence` times ago (0-indexed).
    pub fn valuewhen<F>(&self, predicate: F, occurrence: usize) -> Option<&T>
    where
        F: Fn(&T) -> bool,
    {
        let mut count = 0;
        for val in self.buffer.iter() {
            if predicate(val) {
                if count == occurrence {
                    return Some(val);
                }
                count += 1;
            }
        }
        None
    }
}

impl Series<f64> {
    /// Returns the maximum value in the last `n` bars.
    pub fn highest(&self, n: usize) -> Option<f64> {
        if n == 0 || self.buffer.is_empty() {
            return None;
        }
        self.buffer
            .iter()
            .take(n)
            .copied()
            .filter(|v| v.is_finite())
            .fold(None, |max, val| match max {
                None => Some(val),
                Some(m) => Some(m.max(val)),
            })
    }

    /// Returns the minimum value in the last `n` bars.
    pub fn lowest(&self, n: usize) -> Option<f64> {
        if n == 0 || self.buffer.is_empty() {
            return None;
        }
        self.buffer
            .iter()
            .take(n)
            .copied()
            .filter(|v| v.is_finite())
            .fold(None, |min, val| match min {
                None => Some(val),
                Some(m) => Some(m.min(val)),
            })
    }

    /// Returns the offset (0..n-1) of the highest value in the last `n` bars.
    pub fn highestbars(&self, n: usize) -> Option<usize> {
        if n == 0 || self.buffer.is_empty() {
            return None;
        }
        let mut max_val = f64::NEG_INFINITY;
        let mut max_idx = None;

        for (i, &val) in self.buffer.iter().take(n).enumerate() {
            if val.is_finite() && val > max_val {
                max_val = val;
                max_idx = Some(i);
            }
        }
        max_idx
    }

    /// Returns the offset (0..n-1) of the lowest value in the last `n` bars.
    pub fn lowestbars(&self, n: usize) -> Option<usize> {
        if n == 0 || self.buffer.is_empty() {
            return None;
        }
        let mut min_val = f64::INFINITY;
        let mut min_idx = None;

        for (i, &val) in self.buffer.iter().take(n).enumerate() {
            if val.is_finite() && val < min_val {
                min_val = val;
                min_idx = Some(i);
            }
        }
        min_idx
    }
}

/// Helper functions for time series calculations and crossovers.
pub struct SeriesEvents;

impl SeriesEvents {
    /// Returns difference `series[0] - series[1]`.
    pub fn change(series: &Series<f64>) -> Option<f64> {
        if let (Some(&curr), Some(&prev)) = (series.get(0), series.get(1)) {
            Some(curr - prev)
        } else {
            None
        }
    }

    /// Returns true if the series has been strictly increasing over the last `length` bars.
    pub fn rising(series: &Series<f64>, length: usize) -> bool {
        if length == 0 || series.len() <= length {
            return false;
        }
        for i in 0..length {
            if let (Some(&curr), Some(&prev)) = (series.get(i), series.get(i + 1)) {
                if curr <= prev {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Returns true if the series has been strictly decreasing over the last `length` bars.
    pub fn falling(series: &Series<f64>, length: usize) -> bool {
        if length == 0 || series.len() <= length {
            return false;
        }
        for i in 0..length {
            if let (Some(&curr), Some(&prev)) = (series.get(i), series.get(i + 1)) {
                if curr >= prev {
                    return false;
                }
            } else {
                return false;
            }
        }
        true
    }

    /// Returns true if series `a` crossed series `b` in either direction on the latest bar.
    pub fn cross(a: &Series<f64>, b: &Series<f64>) -> bool {
        Self::crossover(a, b) || Self::crossunder(a, b)
    }

    /// Returns true if series `a` crossed above series `b` (`a[1] <= b[1]` and `a[0] > b[0]`).
    pub fn crossover(a: &Series<f64>, b: &Series<f64>) -> bool {
        if let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
            (a.get(0), a.get(1), b.get(0), b.get(1))
        {
            a1 <= b1 && a0 > b0
        } else {
            false
        }
    }

    /// Returns true if series `a` crossed below series `b` (`a[1] >= b[1]` and `a[0] < b[0]`).
    pub fn crossunder(a: &Series<f64>, b: &Series<f64>) -> bool {
        if let (Some(&a0), Some(&a1), Some(&b0), Some(&b1)) =
            (a.get(0), a.get(1), b.get(0), b.get(1))
        {
            a1 >= b1 && a0 < b0
        } else {
            false
        }
    }

    /// Returns the sum of all finite values in `series`.
    pub fn cum(series: &Series<f64>) -> f64 {
        (0..series.len())
            .filter_map(|i| series.get(i).copied())
            .filter(|v| v.is_finite())
            .sum()
    }

    /// Returns the source value at the requested occurrence of a separate condition series.
    pub fn value_when<'a, T>(
        condition: &Series<bool>,
        source: &'a Series<T>,
        occurrence: usize,
    ) -> Option<&'a T> {
        let mut seen = 0;
        for offset in 0..condition.len().min(source.len()) {
            if condition.get(offset) == Some(&true) {
                if seen == occurrence {
                    return source.get(offset);
                }
                seen += 1;
            }
        }
        None
    }
}

/// Unbounded streaming cumulative sum, independent of a `Series` lookback capacity.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct CumulativeSum {
    value: f64,
}

impl CumulativeSum {
    pub fn update(&mut self, value: f64) -> f64 {
        if value.is_finite() {
            self.value += value;
        }
        self.value
    }

    pub fn value(&self) -> f64 {
        self.value
    }

    pub fn reset(&mut self) {
        self.value = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_series_basic_lookback() {
        let mut s = Series::new(5);
        s.push(10.0);
        s.push(20.0);
        s.push(30.0);

        assert_eq!(s.get(0), Some(&30.0));
        assert_eq!(s.get(1), Some(&20.0));
        assert_eq!(s.get(2), Some(&10.0));
        assert_eq!(s.get(3), None);

        assert_eq!(s.highest(3), Some(30.0));
        assert_eq!(s.lowest(3), Some(10.0));
        assert_eq!(s.highestbars(3), Some(0));
        assert_eq!(s.lowestbars(3), Some(2));
    }

    #[test]
    fn test_series_events_crossover() {
        let mut a = Series::new(5);
        let mut b = Series::new(5);

        a.push(10.0);
        b.push(15.0);

        a.push(20.0);
        b.push(15.0);

        assert!(SeriesEvents::crossover(&a, &b));
        assert!(!SeriesEvents::crossunder(&a, &b));
    }

    #[test]
    fn value_when_uses_a_separate_condition_series() {
        let mut condition = Series::new(4);
        let mut source = Series::new(4);
        for (matches, value) in [(true, 10), (false, 20), (true, 30)] {
            condition.push(matches);
            source.push(value);
        }
        assert_eq!(SeriesEvents::value_when(&condition, &source, 0), Some(&30));
        assert_eq!(SeriesEvents::value_when(&condition, &source, 1), Some(&10));
    }

    #[test]
    fn cumulative_sum_survives_a_small_lookback_capacity() {
        let mut cumulative = CumulativeSum::default();
        assert_eq!(cumulative.update(1.0), 1.0);
        assert_eq!(cumulative.update(2.0), 3.0);
        assert_eq!(cumulative.update(3.0), 6.0);
    }
}
