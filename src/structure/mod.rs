pub mod zone_registry;

pub use zone_registry::*;

use crate::model::{Bar, SupportResistanceZone, ZoneKind};

pub fn find_sr_zones(bars: &[Bar], pivot_len: usize) -> Vec<SupportResistanceZone> {
    if bars.len() < pivot_len * 2 + 1 {
        return Vec::new();
    }

    let mut supports = Vec::new();
    let mut resistances = Vec::new();
    let current_price = bars.last().map(|b| b.close).unwrap_or(1.0);

    for i in pivot_len..(bars.len() - pivot_len) {
        let candidate_high = bars[i].high;
        let is_pivot_high = bars[i - pivot_len..=i + pivot_len]
            .iter()
            .all(|b| b.high <= candidate_high);

        if is_pivot_high {
            resistances.push(candidate_high);
        }

        let candidate_low = bars[i].low;
        let is_pivot_low = bars[i - pivot_len..=i + pivot_len]
            .iter()
            .all(|b| b.low >= candidate_low);

        if is_pivot_low {
            supports.push(candidate_low);
        }
    }

    let mut zones = Vec::new();

    // Sort supports descending (closest to price first)
    supports.sort_by(|a, b| b.partial_cmp(a).unwrap());
    supports.dedup_by(|a, b| (*a - *b).abs() < current_price * 0.005);

    for sup in supports.into_iter().filter(|s| *s < current_price).take(2) {
        let dist = (sup - current_price) / current_price * 100.0;
        zones.push(SupportResistanceZone {
            kind: ZoneKind::Support,
            price: sup,
            price_top: sup + current_price * 0.0025,
            price_bottom: sup - current_price * 0.0025,
            strength: 0.8,
            distance_pct: dist,
            touches: 1,
        });
    }

    // Sort resistances ascending (closest to price first)
    resistances.sort_by(|a, b| a.partial_cmp(b).unwrap());
    resistances.dedup_by(|a, b| (*a - *b).abs() < current_price * 0.005);

    for res in resistances
        .into_iter()
        .filter(|r| *r > current_price)
        .take(2)
    {
        let dist = (res - current_price) / current_price * 100.0;
        zones.push(SupportResistanceZone {
            kind: ZoneKind::Resistance,
            price: res,
            price_top: res + current_price * 0.0025,
            price_bottom: res - current_price * 0.0025,
            strength: 0.8,
            distance_pct: dist,
            touches: 1,
        });
    }

    zones
}
