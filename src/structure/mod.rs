pub mod zone_registry;

pub use zone_registry::*;

use crate::model::{Bar, SupportResistanceZone, ZoneKind};

/// How wide a zone is drawn around its pivot, and how close two pivots may sit
/// before they count as one — both in **price points**, not percent.
///
/// Exists because the width used to be hard-wired to 0.25 % of the current
/// price. A fixed percentage is the wrong unit for this: in a quiet market it
/// spans several sessions' worth of range, in a volatile one it is thinner than
/// a single bar. A caller that knows the instrument's volatility (ATR) can say
/// so instead; [`ZoneTolerance::relative_to_price`] keeps the old rule for
/// those that do not.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ZoneTolerance {
    /// Half the height of the band around the pivot.
    pub half_width: f64,
    /// Two pivots closer than this count as the same level.
    pub dedup_distance: f64,
}

impl ZoneTolerance {
    /// The previous, hard-wired rule: 0.25 % of the price as half-width, 0.5 %
    /// as dedup distance. Kept so [`find_sr_zones`] behaves exactly as before.
    pub fn relative_to_price(price: f64) -> Self {
        Self {
            half_width: price * 0.0025,
            dedup_distance: price * 0.005,
        }
    }

    /// Replaces an unusable value with the price-relative one. A zero or NaN
    /// ATR would otherwise collapse every zone to a line, which reads as a
    /// precision the data does not have.
    fn sanitized(self, price: f64) -> Self {
        let fallback = Self::relative_to_price(price);
        Self {
            half_width: if self.half_width.is_finite() && self.half_width > 0.0 {
                self.half_width
            } else {
                fallback.half_width
            },
            dedup_distance: if self.dedup_distance.is_finite() && self.dedup_distance > 0.0 {
                self.dedup_distance
            } else {
                fallback.dedup_distance
            },
        }
    }
}

/// Pivot zones with the historical, price-relative tolerance.
pub fn find_sr_zones(bars: &[Bar], pivot_len: usize) -> Vec<SupportResistanceZone> {
    let price = bars.last().map(|b| b.close).unwrap_or(1.0);
    find_sr_zones_with_tolerance(bars, pivot_len, ZoneTolerance::relative_to_price(price))
}

/// Pivot zones whose width and dedup distance the caller sets — typically from
/// the instrument's ATR, so that a zone is as wide as the market's own noise
/// rather than a fixed share of the price.
pub fn find_sr_zones_with_tolerance(
    bars: &[Bar],
    pivot_len: usize,
    tolerance: ZoneTolerance,
) -> Vec<SupportResistanceZone> {
    if bars.len() < pivot_len * 2 + 1 {
        return Vec::new();
    }

    // Pivots als `(Preis, Zeitstempel)`: die Zeit ist die **Ereigniszeit** der
    // Zone — die Bar, auf der der Wendepunkt lag. Ohne sie wäre nur die
    // Wissenszeit (wann die Zone gefunden wurde) belegbar.
    let mut supports: Vec<(f64, i64)> = Vec::new();
    let mut resistances: Vec<(f64, i64)> = Vec::new();
    let current_price = bars.last().map(|b| b.close).unwrap_or(1.0);
    let ZoneTolerance {
        half_width,
        dedup_distance,
    } = tolerance.sanitized(current_price);

    for i in pivot_len..(bars.len() - pivot_len) {
        let candidate_high = bars[i].high;
        let is_pivot_high = bars[i - pivot_len..=i + pivot_len]
            .iter()
            .all(|b| b.high <= candidate_high);

        if is_pivot_high {
            resistances.push((candidate_high, bars[i].timestamp));
        }

        let candidate_low = bars[i].low;
        let is_pivot_low = bars[i - pivot_len..=i + pivot_len]
            .iter()
            .all(|b| b.low >= candidate_low);

        if is_pivot_low {
            supports.push((candidate_low, bars[i].timestamp));
        }
    }

    let mut zones = Vec::new();

    // Sort supports descending (closest to price first). Beim Zusammenlegen
    // dicht beieinanderliegender Niveaus bleibt der **früheste** Pivot-Zeitpunkt
    // erhalten: die Zone entstand, als der Bereich zuerst drehte, nicht beim
    // letzten Test.
    supports.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap());
    let supports = dedup_pivots(supports, dedup_distance);

    for (sup, pivot_ts) in supports
        .into_iter()
        .filter(|(s, _)| *s < current_price)
        .take(2)
    {
        let dist = (sup - current_price) / current_price * 100.0;
        zones.push(SupportResistanceZone {
            kind: ZoneKind::Support,
            price: sup,
            price_top: sup + half_width,
            price_bottom: sup - half_width,
            strength: 0.8,
            distance_pct: dist,
            touches: 1,
            pivot_ts,
        });
    }

    // Sort resistances ascending (closest to price first)
    resistances.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
    let resistances = dedup_pivots(resistances, dedup_distance);

    for (res, pivot_ts) in resistances
        .into_iter()
        .filter(|(r, _)| *r > current_price)
        .take(2)
    {
        let dist = (res - current_price) / current_price * 100.0;
        zones.push(SupportResistanceZone {
            kind: ZoneKind::Resistance,
            price: res,
            price_top: res + half_width,
            price_bottom: res - half_width,
            strength: 0.8,
            distance_pct: dist,
            touches: 1,
            pivot_ts,
        });
    }

    zones
}

/// Legt Pivots dicht beieinanderliegender Preise zu einem zusammen und behält
/// den **frühesten** Zeitstempel — die Ereigniszeit der Zone.
fn dedup_pivots(pivots: Vec<(f64, i64)>, dedup_distance: f64) -> Vec<(f64, i64)> {
    let mut out: Vec<(f64, i64)> = Vec::new();
    for (price, ts) in pivots {
        if let Some(last) = out.last_mut() {
            if (last.0 - price).abs() < dedup_distance {
                last.1 = last.1.min(ts);
                continue;
            }
        }
        out.push((price, ts));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(high: f64, low: f64, close: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: close,
            high,
            low,
            close,
            volume: 0.0,
        }
    }

    /// A zigzag with a pronounced high and low on either side of the closing
    /// price. Deliberately asymmetric (high 120, low 80, close 100): a series
    /// mirrored around the close could not show a support/resistance mix-up.
    fn zigzag() -> Vec<Bar> {
        let mut bars = Vec::new();
        for round in 0..6 {
            let drift = f64::from(round) * 0.01;
            bars.push(bar(120.0 - drift, 110.0, 118.0));
            bars.push(bar(112.0, 101.0, 105.0));
            bars.push(bar(101.0, 80.0 + drift, 85.0));
            bars.push(bar(105.0, 95.0, 100.0));
        }
        bars.push(bar(104.0, 96.0, 100.0));
        bars
    }

    /// Die Zone trägt die **Ereigniszeit** ihres Pivots — die Bar, auf der der
    /// Wendepunkt lag. Das ist die Voraussetzung dafür, dass Kestrel
    /// `event_ts` von der Wissenszeit trennen kann.
    #[test]
    fn die_zone_traegt_die_ereigniszeit_ihres_pivots() {
        // Ein ausgeprägtes Tief bei Index 2, sonst flach.
        let bars: Vec<Bar> = (0..7)
            .map(|i| {
                let close = if i == 2 { 90.0 } else { 100.0 };
                Bar {
                    timestamp: i as i64 * 60,
                    open: close,
                    high: close + 1.0,
                    low: close - 1.0,
                    close,
                    volume: 0.0,
                }
            })
            .collect();
        let zones = find_sr_zones_with_tolerance(
            &bars,
            1,
            ZoneTolerance {
                half_width: 1.0,
                dedup_distance: 1.0,
            },
        );
        assert!(
            zones.iter().any(|z| z.pivot_ts == 120),
            "keine Zone trägt die Pivot-Bar (Index 2 = 120): {zones:?}"
        );
    }

    #[test]
    fn a_wider_tolerance_gives_wider_zones() {
        let bars = zigzag();
        let narrow = find_sr_zones_with_tolerance(
            &bars,
            1,
            ZoneTolerance {
                half_width: 0.5,
                dedup_distance: 1.0,
            },
        );
        let wide = find_sr_zones_with_tolerance(
            &bars,
            1,
            ZoneTolerance {
                half_width: 4.0,
                dedup_distance: 1.0,
            },
        );
        assert!(!narrow.is_empty() && !wide.is_empty());
        for zone in &narrow {
            assert!(
                (zone.price_top - zone.price_bottom - 1.0).abs() < 1e-9,
                "{zone:?}"
            );
        }
        for zone in &wide {
            assert!(
                (zone.price_top - zone.price_bottom - 8.0).abs() < 1e-9,
                "{zone:?}"
            );
        }
    }

    /// The pivot itself stays the zone's anchor, whatever the width.
    #[test]
    fn the_pivot_stays_the_centre() {
        let zones = find_sr_zones_with_tolerance(
            &zigzag(),
            1,
            ZoneTolerance {
                half_width: 2.0,
                dedup_distance: 1.0,
            },
        );
        for zone in &zones {
            let centre = (zone.price_top + zone.price_bottom) / 2.0;
            assert!((zone.price - centre).abs() < 1e-9, "{zone:?}");
        }
    }

    /// A dedup distance wide enough to swallow every pivot leaves at most one
    /// level per side — the knob works in the other direction too.
    #[test]
    fn a_wide_dedup_distance_collapses_levels() {
        let bars = zigzag();
        let tight = find_sr_zones_with_tolerance(
            &bars,
            1,
            ZoneTolerance {
                half_width: 0.5,
                dedup_distance: 0.01,
            },
        );
        let coarse = find_sr_zones_with_tolerance(
            &bars,
            1,
            ZoneTolerance {
                half_width: 0.5,
                dedup_distance: 100.0,
            },
        );
        assert!(coarse.len() <= tight.len(), "{coarse:?} vs {tight:?}");
    }

    /// The parameterless entry point must keep its old numbers: 0.25 % of the
    /// price as half-width. Other crates depend on that behaviour.
    #[test]
    fn the_legacy_entry_point_is_unchanged() {
        let bars = zigzag();
        let price = bars.last().unwrap().close;
        for zone in find_sr_zones(&bars, 1) {
            let half = (zone.price_top - zone.price_bottom) / 2.0;
            assert!((half - price * 0.0025).abs() < 1e-9, "{zone:?}");
        }
    }

    /// A zero or NaN ATR must not collapse zones to a line.
    #[test]
    fn an_unusable_tolerance_falls_back_to_the_price_rule() {
        let bars = zigzag();
        let price = bars.last().unwrap().close;
        for tolerance in [
            ZoneTolerance {
                half_width: 0.0,
                dedup_distance: 0.0,
            },
            ZoneTolerance {
                half_width: f64::NAN,
                dedup_distance: f64::NAN,
            },
        ] {
            let zones = find_sr_zones_with_tolerance(&bars, 1, tolerance);
            assert!(!zones.is_empty());
            for zone in &zones {
                let half = (zone.price_top - zone.price_bottom) / 2.0;
                assert!((half - price * 0.0025).abs() < 1e-9, "{zone:?}");
            }
        }
    }
}
