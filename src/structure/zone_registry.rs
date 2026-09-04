use crate::model::{Bar, SupportResistanceZone, ZoneKind};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Lifecycle state of a market structure zone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum ZoneState {
    #[default]
    Active,
    Touched,
    Reacted,
    Broken,
    Flipped,
}

/// Tracked Zone record with lifecycle metadata and confluence rating.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ManagedZone {
    pub id: u64,
    pub zone: SupportResistanceZone,
    pub state: ZoneState,
    pub confluence_score: f64,
    pub touch_count: u32,
    pub age_bars: u32,
    pub created_at: i64,
    /// Which detector(s) produced this zone's evidence (e.g. `"order_block"`,
    /// `"fair_value_gap"`, `"sr_pivot"`, `"volume_profile_hvn"`). A merged zone accumulates every
    /// contributing source instead of losing provenance to whichever zone happened to survive.
    pub sources: Vec<String>,
    /// IDs of zones merged into this one (this zone's own ID is never included) — merge lineage
    /// that survives across repeated merges, rather than being discarded.
    pub merged_from: Vec<u64>,
    /// Strongest observed reaction magnitude, in ATR units: the price distance moved away from
    /// the zone edge at the moment `state` most recently transitioned to `Reacted`. `0.0` if the
    /// zone has never reacted.
    pub reaction_strength: f64,
}

/// Central registry managing support/resistance, order block, and FVG zones.
#[derive(Debug, Clone, Default)]
pub struct ZoneRegistry {
    next_id: u64,
    zones: Vec<ManagedZone>,
}

impl ZoneRegistry {
    pub fn new() -> Self {
        Self {
            next_id: 1,
            zones: Vec::new(),
        }
    }

    /// Registers a new zone into the registry, assigning a unique ID and initial confluence score.
    /// Source provenance defaults to `"unspecified"`; prefer [`ZoneRegistry::register_with_source`]
    /// when the producing detector is known.
    pub fn register(&mut self, zone: SupportResistanceZone, created_at: i64) -> u64 {
        self.register_with_source(zone, created_at, "unspecified")
    }

    /// Like [`ZoneRegistry::register`], tagging the zone with which detector produced it.
    pub fn register_with_source(
        &mut self,
        zone: SupportResistanceZone,
        created_at: i64,
        source: impl Into<String>,
    ) -> u64 {
        let id = self.next_id;
        self.next_id += 1;

        let touch_score = (f64::from(zone.touches) / 5.0).min(1.0);
        let confluence_score = (touch_score * 0.3 + zone.strength * 0.7).clamp(0.0, 1.0);

        self.zones.push(ManagedZone {
            id,
            zone,
            state: ZoneState::Active,
            confluence_score,
            touch_count: 0,
            age_bars: 0,
            created_at,
            sources: vec![source.into()],
            merged_from: Vec::new(),
            reaction_strength: 0.0,
        });

        id
    }

    /// Updates zone states based on incoming price action (touch, break, flip, aging).
    pub fn update(&mut self, bar: &Bar, atr: f64) {
        for mz in &mut self.zones {
            mz.age_bars += 1;

            if mz.state == ZoneState::Flipped {
                continue;
            }

            let price_in_zone = bar.low <= mz.zone.price_top && bar.high >= mz.zone.price_bottom;

            if mz.state == ZoneState::Broken {
                let is_flip = match mz.zone.kind {
                    ZoneKind::Support => price_in_zone && bar.close < mz.zone.price_bottom,
                    ZoneKind::Resistance => price_in_zone && bar.close > mz.zone.price_top,
                };
                if is_flip {
                    mz.state = ZoneState::Flipped;
                    mz.touch_count += 1;
                }
                continue;
            }

            let reacted = match mz.zone.kind {
                ZoneKind::Support => {
                    mz.state == ZoneState::Touched && bar.close > mz.zone.price_top
                }
                ZoneKind::Resistance => {
                    mz.state == ZoneState::Touched && bar.close < mz.zone.price_bottom
                }
            };
            if reacted {
                mz.state = ZoneState::Reacted;
                let edge = match mz.zone.kind {
                    ZoneKind::Support => mz.zone.price_top,
                    ZoneKind::Resistance => mz.zone.price_bottom,
                };
                if atr > 0.0 {
                    let magnitude = (bar.close - edge).abs() / atr;
                    mz.reaction_strength = mz.reaction_strength.max(magnitude);
                }
            }

            if price_in_zone && !reacted {
                mz.touch_count += 1;
                mz.state = ZoneState::Touched;
            }

            // Check for structural break
            match mz.zone.kind {
                ZoneKind::Support => {
                    if bar.close < mz.zone.price_bottom - (atr * 0.2) {
                        mz.state = ZoneState::Broken;
                    }
                }
                ZoneKind::Resistance => {
                    if bar.close > mz.zone.price_top + (atr * 0.2) {
                        mz.state = ZoneState::Broken;
                    }
                }
            }
        }
    }

    /// Merges overlapping zones of the same kind within `atr_margin` distance.
    pub fn merge_overlapping(&mut self, atr_margin: f64) {
        if self.zones.len() < 2 {
            return;
        }

        let candidates = std::mem::take(&mut self.zones);
        self.zones = merge_zone_candidates(candidates, atr_margin, true);
    }

    /// Like [`ZoneRegistry::merge_overlapping`], but also merges same-kind zones across
    /// *different* sources/states (not just identical-source, identical-state duplicates), and
    /// resolves which zone "wins" as the merge target by an explicit priority order — higher
    /// confluence score first, then more touches, then newer — instead of whichever was iterated
    /// first silently dominating.
    pub fn merge_overlapping_with_priority(&mut self, price_margin: f64) {
        if self.zones.len() < 2 {
            return;
        }

        let mut candidates = std::mem::take(&mut self.zones);
        candidates.sort_by(|a, b| {
            b.confluence_score
                .total_cmp(&a.confluence_score)
                .then(b.touch_count.cmp(&a.touch_count))
                .then(b.created_at.cmp(&a.created_at))
        });

        self.zones = merge_zone_candidates(candidates, price_margin, false);
    }

    /// Recomputes every active zone's confluence score from its own touches/strength (as at
    /// registration) plus a cross-source overlap bonus: how many *distinct other sources* have a
    /// zone overlapping this one's price range within `price_tolerance`. Reflects genuine
    /// multi-detector agreement rather than one detector's self-reported touch count — the
    /// "belastbarer Confluence-Score" a single-source score cannot provide. Call after
    /// registering zones from more than one detector.
    pub fn recompute_confluence(&mut self, price_tolerance: f64) {
        let snapshot: Vec<(u64, f64, f64, Vec<String>)> = self
            .zones
            .iter()
            .map(|mz| {
                (
                    mz.id,
                    mz.zone.price_bottom,
                    mz.zone.price_top,
                    mz.sources.clone(),
                )
            })
            .collect();

        for mz in &mut self.zones {
            let touch_score = (f64::from(mz.zone.touches) / 5.0).min(1.0);
            let base_score = (touch_score * 0.3 + mz.zone.strength * 0.7).clamp(0.0, 1.0);

            let distinct_other_sources: std::collections::HashSet<&String> = snapshot
                .iter()
                .filter(|(id, bottom, top, _)| {
                    *id != mz.id
                        && *bottom <= mz.zone.price_top + price_tolerance
                        && *top >= mz.zone.price_bottom - price_tolerance
                })
                .flat_map(|(_, _, _, sources)| sources.iter())
                .filter(|s| !mz.sources.contains(s))
                .collect();

            let confluence_bonus = (distinct_other_sources.len() as f64 * 0.15).min(0.4);
            mz.confluence_score = (base_score + confluence_bonus).clamp(0.0, 1.0);
        }
    }

    /// Returns up to `top_n` active zones within `max_distance` of `current_price`, ranked by
    /// confluence score descending — the zones a consumer should actually act on, rather than the
    /// full unranked registry.
    pub fn relevant_zones(
        &self,
        current_price: f64,
        max_distance: f64,
        top_n: usize,
    ) -> Vec<&ManagedZone> {
        let mut candidates: Vec<&ManagedZone> = self
            .active_zones()
            .filter(|mz| {
                let distance = if current_price < mz.zone.price_bottom {
                    mz.zone.price_bottom - current_price
                } else if current_price > mz.zone.price_top {
                    current_price - mz.zone.price_top
                } else {
                    0.0
                };
                distance <= max_distance
            })
            .collect();
        candidates.sort_by(|a, b| b.confluence_score.total_cmp(&a.confluence_score));
        candidates.truncate(top_n);
        candidates
    }

    /// Prunes expired or broken zones older than `max_age_bars`.
    pub fn prune(&mut self, max_age_bars: u32) {
        self.zones
            .retain(|mz| mz.age_bars <= max_age_bars && mz.state != ZoneState::Broken);
    }

    pub fn active_zones(&self) -> impl Iterator<Item = &ManagedZone> {
        self.zones.iter().filter(|mz| mz.state != ZoneState::Broken)
    }

    pub fn zones(&self) -> &[ManagedZone] {
        &self.zones
    }
}

/// Shared merge routine behind [`ZoneRegistry::merge_overlapping`] and
/// [`ZoneRegistry::merge_overlapping_with_priority`]: absorbs each candidate (in the order given
/// by the caller, which encodes either original insertion order or an explicit priority sort)
/// into the first already-merged zone of the same kind (and, if `require_same_state`, the same
/// lifecycle state) whose price range overlaps within `price_margin`.
fn merge_zone_candidates(
    candidates: Vec<ManagedZone>,
    price_margin: f64,
    require_same_state: bool,
) -> Vec<ManagedZone> {
    let mut merged: Vec<ManagedZone> = Vec::new();

    for mz in candidates {
        let mut absorbed = false;
        for existing in &mut merged {
            let same_kind_and_state = existing.zone.kind == mz.zone.kind
                && (!require_same_state || existing.state == mz.state);
            if !same_kind_and_state {
                continue;
            }
            let overlap = (mz.zone.price_bottom <= existing.zone.price_top + price_margin)
                && (mz.zone.price_top >= existing.zone.price_bottom - price_margin);
            if !overlap {
                continue;
            }

            existing.zone.price_top = existing.zone.price_top.max(mz.zone.price_top);
            existing.zone.price_bottom = existing.zone.price_bottom.min(mz.zone.price_bottom);
            existing.zone.strength = existing.zone.strength.max(mz.zone.strength);
            existing.zone.touches = existing.zone.touches.saturating_add(mz.zone.touches);
            existing.confluence_score = existing.confluence_score.max(mz.confluence_score);
            existing.touch_count = existing.touch_count.saturating_add(mz.touch_count);
            existing.age_bars = existing.age_bars.max(mz.age_bars);
            existing.created_at = existing.created_at.min(mz.created_at);
            existing.reaction_strength = existing.reaction_strength.max(mz.reaction_strength);
            for src in &mz.sources {
                if !existing.sources.contains(src) {
                    existing.sources.push(src.clone());
                }
            }
            existing.merged_from.push(mz.id);
            existing.merged_from.extend(mz.merged_from.iter().copied());
            absorbed = true;
            break;
        }
        if !absorbed {
            merged.push(mz);
        }
    }

    merged
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zone_registry_lifecycle() {
        let mut reg = ZoneRegistry::new();
        let s_zone = SupportResistanceZone {
            kind: ZoneKind::Support,
            price: 100.0,
            price_top: 102.0,
            price_bottom: 98.0,
            strength: 0.8,
            distance_pct: 0.0,
            touches: 2,
        };

        let id = reg.register(s_zone, 1000);
        assert_eq!(id, 1);

        // Bar touches support zone
        let bar1 = Bar::new(1000, 101.0, 103.0, 99.0, 100.0, 1000.0);
        reg.update(&bar1, 2.0);
        assert_eq!(reg.zones()[0].state, ZoneState::Touched);

        // Bar breaks support zone below 98 - 0.4 = 97.6
        let bar2 = Bar::new(2000, 99.0, 99.0, 95.0, 96.0, 1000.0);
        reg.update(&bar2, 2.0);
        assert_eq!(reg.zones()[0].state, ZoneState::Broken);

        // A retest from below confirms the former support as flipped resistance.
        let bar3 = Bar::new(3000, 96.0, 99.0, 95.0, 97.0, 1000.0);
        reg.update(&bar3, 2.0);
        assert_eq!(reg.zones()[0].state, ZoneState::Flipped);
    }

    #[test]
    fn touch_can_transition_to_reacted() {
        let mut reg = ZoneRegistry::new();
        reg.register(
            SupportResistanceZone {
                kind: ZoneKind::Support,
                price: 100.0,
                price_top: 102.0,
                price_bottom: 98.0,
                strength: 0.8,
                distance_pct: 0.0,
                touches: 1,
            },
            0,
        );
        reg.update(&Bar::new(1, 103.0, 103.0, 100.0, 101.0, 1.0), 2.0);
        reg.update(&Bar::new(2, 101.0, 104.0, 101.0, 103.0, 1.0), 2.0);
        assert_eq!(reg.zones()[0].state, ZoneState::Reacted);
        // close=103, edge=price_top=102, atr=2.0 -> |103-102|/2 = 0.5 ATR.
        assert!((reg.zones()[0].reaction_strength - 0.5).abs() < 1e-9);
    }

    fn support_zone(
        price_top: f64,
        price_bottom: f64,
        strength: f64,
        touches: u32,
    ) -> SupportResistanceZone {
        SupportResistanceZone {
            kind: ZoneKind::Support,
            price: (price_top + price_bottom) / 2.0,
            price_top,
            price_bottom,
            strength,
            distance_pct: 0.0,
            touches,
        }
    }

    #[test]
    fn test_register_with_source_tags_provenance() {
        let mut reg = ZoneRegistry::new();
        let id = reg.register_with_source(support_zone(102.0, 98.0, 0.5, 1), 0, "order_block");
        let zone = reg.zones().iter().find(|z| z.id == id).unwrap();
        assert_eq!(zone.sources, vec!["order_block".to_string()]);

        let default_id = reg.register(support_zone(50.0, 48.0, 0.5, 1), 0);
        let default_zone = reg.zones().iter().find(|z| z.id == default_id).unwrap();
        assert_eq!(default_zone.sources, vec!["unspecified".to_string()]);
    }

    #[test]
    fn test_recompute_confluence_rewards_multi_source_overlap() {
        let mut reg = ZoneRegistry::new();
        let solo_id = reg.register_with_source(support_zone(60.0, 58.0, 0.3, 0), 0, "sr_pivot");
        let a_id = reg.register_with_source(support_zone(102.0, 98.0, 0.3, 0), 0, "sr_pivot");
        reg.register_with_source(support_zone(101.0, 99.0, 0.3, 0), 0, "order_block");
        reg.register_with_source(support_zone(100.5, 99.5, 0.3, 0), 0, "fair_value_gap");

        reg.recompute_confluence(0.5);

        let solo = reg
            .zones()
            .iter()
            .find(|z| z.id == solo_id)
            .unwrap()
            .confluence_score;
        let confluent = reg
            .zones()
            .iter()
            .find(|z| z.id == a_id)
            .unwrap()
            .confluence_score;
        assert!(
            confluent > solo,
            "a zone corroborated by two other independent sources must score higher than an isolated one"
        );
    }

    #[test]
    fn test_merge_overlapping_with_priority_preserves_lineage_and_sources() {
        let mut reg = ZoneRegistry::new();
        let strong_id =
            reg.register_with_source(support_zone(102.0, 98.0, 0.9, 5), 0, "order_block");
        let weak_id =
            reg.register_with_source(support_zone(101.0, 99.0, 0.2, 0), 5, "fair_value_gap");
        reg.recompute_confluence(0.5);

        reg.merge_overlapping_with_priority(1.0);

        assert_eq!(reg.zones().len(), 1);
        let survivor = &reg.zones()[0];
        assert_eq!(
            survivor.id, strong_id,
            "the higher-confluence zone must be the merge target"
        );
        assert!(survivor.sources.contains(&"order_block".to_string()));
        assert!(survivor.sources.contains(&"fair_value_gap".to_string()));
        assert!(survivor.merged_from.contains(&weak_id));
    }

    #[test]
    fn test_relevant_zones_ranks_by_confluence_within_distance() {
        let mut reg = ZoneRegistry::new();
        reg.register_with_source(support_zone(102.0, 98.0, 0.9, 5), 0, "order_block"); // near, strong
        reg.register_with_source(support_zone(101.0, 99.0, 0.1, 0), 0, "sr_pivot"); // near, weak
        reg.register_with_source(support_zone(1002.0, 998.0, 0.9, 5), 0, "order_block"); // far, strong

        let relevant = reg.relevant_zones(100.0, 5.0, 2);
        assert_eq!(
            relevant.len(),
            2,
            "the far zone must be excluded by max_distance"
        );
        assert!(relevant[0].confluence_score >= relevant[1].confluence_score);
    }
}
