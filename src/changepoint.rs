//! Change-point detection primitives: the one-sided CUSUM of E. S. Page (1954).
//!
//! A pure function over a plain `&[f64]`; it makes no assumption about where the values come from
//! (returns, standardized returns, any other series). The streaming `cusum` indicator
//! ([`crate::indicator::cusum`]) standardizes close-to-close returns and feeds them through the same
//! [`CusumState`] step.

/// The running state of a two-sided CUSUM: the accumulated upward and downward deviation, both
/// `>= 0`.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CusumState {
    /// `S⁺`: accumulated deviation above `mu0 + k`.
    pub upper: f64,
    /// `S⁻`: accumulated deviation below `mu0 - k`.
    pub lower: f64,
}

impl CusumState {
    /// Advances both sums by one value `x`:
    /// `S⁺ ← max(0, S⁺ + x - mu0 - k)`, `S⁻ ← max(0, S⁻ - x + mu0 - k)`.
    ///
    /// A non-finite `x` leaves the state unchanged, so one missing value neither raises an alarm
    /// nor erases accumulated deviation.
    pub fn step(&mut self, x: f64, mu0: f64, k: f64) {
        if !x.is_finite() {
            return;
        }
        self.upper = (self.upper + x - mu0 - k).max(0.0);
        self.lower = (self.lower - x + mu0 - k).max(0.0);
    }
}

/// Result of [`cusum`]: both sums after every value and the first alarm of each side.
#[derive(Debug, Clone, PartialEq)]
pub struct CusumOutput {
    /// `S⁺` after each value; same length as the input.
    pub upper: Vec<f64>,
    /// `S⁻` after each value; same length as the input.
    pub lower: Vec<f64>,
    /// Index (0-based) of the first value after which `S⁺ > h`.
    pub first_alarm_upper: Option<usize>,
    /// Index (0-based) of the first value after which `S⁻ > h`.
    pub first_alarm_lower: Option<usize>,
}

/// Two-sided CUSUM with reference value `mu0`, tolerance `k` and threshold `h`, starting from
/// `S⁺ = S⁻ = 0`:
///
/// ```text
/// S⁺[t] = max(0, S⁺[t-1] + x[t] - mu0 - k)      alarm up   at the first t with S⁺[t] > h
/// S⁻[t] = max(0, S⁻[t-1] - x[t] + mu0 - k)      alarm down at the first t with S⁻[t] > h
/// ```
///
/// The comparison is strict (`S = h` raises no alarm) and the sums are never reset after an alarm,
/// so `upper`/`lower` keep describing the accumulated deviation. A single value alarms on its own
/// only if it exceeds `mu0 + k + h`. Non-finite values leave the sums unchanged
/// ([`CusumState::step`]).
pub fn cusum(values: &[f64], mu0: f64, k: f64, h: f64) -> CusumOutput {
    let mut state = CusumState::default();
    let mut out = CusumOutput {
        upper: Vec::with_capacity(values.len()),
        lower: Vec::with_capacity(values.len()),
        first_alarm_upper: None,
        first_alarm_lower: None,
    };
    for (t, &x) in values.iter().enumerate() {
        state.step(x, mu0, k);
        out.upper.push(state.upper);
        out.lower.push(state.lower);
        if out.first_alarm_upper.is_none() && state.upper > h {
            out.first_alarm_upper = Some(t);
        }
        if out.first_alarm_lower.is_none() && state.lower > h {
            out.first_alarm_lower = Some(t);
        }
    }
    out
}
