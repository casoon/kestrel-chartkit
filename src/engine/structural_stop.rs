/// Live CRV recompute for structural trailing-stop management (plan Anhang G.2):
/// `RR_remaining = (NextTarget - CurrentPrice) / (CurrentPrice - ProtectiveStop)`. When this
/// shrinks towards zero, holding further becomes mathematically unattractive even without a
/// fixed profit target being hit — a trade can become structurally "fertig".
///
/// Sign convention: pass `next_target`/`current_price`/`protective_stop` so that
/// `current_price - protective_stop` is positive for a long and negative for a short (i.e.
/// keep numerator and denominator consistent with the trade direction).
pub fn remaining_rr(next_target: f64, current_price: f64, protective_stop: f64) -> f64 {
    let denom = current_price - protective_stop;
    if denom.abs() < f64::EPSILON {
        return 0.0;
    }
    (next_target - current_price) / denom
}

/// Tracks a structural (non-ATR, non-break-even) trailing stop: the stop only advances when
/// the market has printed new confirmed structure, and only ever tightens risk — never
/// loosens it (plan Anhang G.2: "Stop wird nur nachgezogen, wenn der Markt neue Struktur
/// erzeugt hat").
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralTrailingStop {
    pub current_stop: f64,
    pub is_long: bool,
}

impl StructuralTrailingStop {
    pub fn new(initial_stop: f64, is_long: bool) -> Self {
        Self {
            current_stop: initial_stop,
            is_long,
        }
    }

    /// Advance the stop to a newly confirmed structural level (e.g. a new accepted balance
    /// boundary), ignored if it would loosen risk.
    pub fn advance(&mut self, new_structural_level: f64) {
        if self.is_long {
            if new_structural_level > self.current_stop {
                self.current_stop = new_structural_level;
            }
        } else if new_structural_level < self.current_stop {
            self.current_stop = new_structural_level;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn remaining_rr_for_a_short_setup() {
        // Short: target below price, stop above price → both numerator and denominator
        // negative, ratio stays positive.
        let rr = remaining_rr(29_180.0, 29_690.0, 29_746.0);
        assert!((rr - 9.107142857142858).abs() < 1e-6);
    }

    #[test]
    fn remaining_rr_shrinks_as_price_approaches_target() {
        let far = remaining_rr(110.0, 100.0, 95.0);
        let near = remaining_rr(110.0, 108.0, 95.0);
        assert!(
            near < far,
            "RR_remaining should shrink as price nears the target"
        );
    }

    #[test]
    fn long_stop_only_advances_upward() {
        let mut stop = StructuralTrailingStop::new(100.0, true);
        stop.advance(105.0);
        assert_eq!(stop.current_stop, 105.0);
        stop.advance(102.0); // would loosen risk, must be ignored
        assert_eq!(stop.current_stop, 105.0);
    }

    #[test]
    fn short_stop_only_advances_downward() {
        let mut stop = StructuralTrailingStop::new(100.0, false);
        stop.advance(95.0);
        assert_eq!(stop.current_stop, 95.0);
        stop.advance(98.0); // would loosen risk, must be ignored
        assert_eq!(stop.current_stop, 95.0);
    }
}
