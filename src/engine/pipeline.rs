use crate::indicator::swing_structure::{SwingStructureEngine, SwingStructureOutput};
use crate::indicator::IndicatorOutput;
use crate::model::{Bar, MarketRegime};

use super::balance_migration::{build_balance_migration, BalanceMigrationOutput};
use super::liquidity_path::{build_free_space_score, FreeSpaceScore};
use super::market_context::{
    build_market_context, classify_volume_node, AcceptanceLevel, AuctionPhase, MarketContextOutput,
};
use super::market_state::{
    derive_playbook, ExpectedPlaybook, MarketStateOutput, OpeningType, VolatilityRegime,
};
use super::structural_stop::StructuralTrailingStop;
use super::vwap_regime::{VwapRegimeOutput, VwapRegimeTracker};

/// M2.1: Rechnet ATR-Output (%-Wert) in Rohwert um und speist ihn in `SwingStructureEngine` ein.
pub fn update_swing_structure(
    engine: &mut SwingStructureEngine,
    bar: &Bar,
    atr_output: &IndicatorOutput,
) -> Option<SwingStructureOutput> {
    let atr_raw = (atr_output.value / 100.0) * bar.close;
    engine.update(bar, atr_raw)
}

/// M2.2: Anbindung von `build_market_context` an `VolumeProfileEngine`-Output.
///
/// `atr_val` ist immer der ATR-Prozentwert (`100 * ATR / close`), wie ihn
/// [`crate::indicator::atr::Atr`] liefert — analog zu `atr_output.value` in
/// [`update_swing_structure`].
pub fn build_market_context_from_profile(
    regime: MarketRegime,
    bar: &Bar,
    atr_val: f64,
    vp_output: &IndicatorOutput,
    previous_acceptance: AcceptanceLevel,
    auction_phase: AuctionPhase,
) -> MarketContextOutput {
    let vpoc = vp_output
        .extra
        .get("vpoc")
        .copied()
        .unwrap_or(vp_output.value);
    let density = vp_output
        .extra
        .get("current_density")
        .copied()
        .unwrap_or(0.05);
    let node = classify_volume_node(density, 0.10, 0.02);

    let atr_raw = (atr_val / 100.0) * bar.close;

    build_market_context(
        regime,
        bar.close,
        vpoc,
        atr_raw,
        node,
        previous_acceptance,
        auction_phase,
    )
}

/// M2.3: `AuctionPhase`-Übergänge aus `MarketStructureBreaksEngine` und `OrderBlockEngine` ableiten.
pub fn derive_auction_phase(
    msb_output: Option<&IndicatorOutput>,
    ob_output: Option<&IndicatorOutput>,
    current_phase: AuctionPhase,
) -> AuctionPhase {
    if let Some(msb) = msb_output {
        if msb.value.abs() >= 1.0 {
            return AuctionPhase::Breakout;
        }
    }

    if let Some(ob) = ob_output {
        if let Some(&duration) = ob.extra.get("active_ob_duration") {
            if duration > 0.0 {
                return AuctionPhase::Acceptance {
                    duration_bars: duration as u32,
                };
            }
        }
    }

    current_phase
}

/// M2.4 & M2.5: Erzeugt `MarketStateOutput` und leitet das `ExpectedPlaybook` ab.
///
/// `atr_val` ist der ATR-Prozentwert (`100 * ATR / close`), wie ihn
/// [`crate::indicator::atr::Atr`] liefert. [`crate::regime::classify_regime`]
/// erwartet ATR dagegen als Bruch (`ATR / close`), daher die Division um 100
/// vor der Weitergabe.
pub fn build_market_state_and_playbook(
    bars: &[Bar],
    adx_val: f64,
    atr_val: f64,
    trend_stability: f64,
    opening_type: OpeningType,
) -> (MarketStateOutput, ExpectedPlaybook) {
    let regime = crate::regime::classify_regime(bars, adx_val, atr_val / 100.0);
    let playbook = derive_playbook(regime, trend_stability);

    let vol_regime = if atr_val > 2.5 {
        VolatilityRegime::High
    } else if atr_val < 0.8 {
        VolatilityRegime::Low
    } else {
        VolatilityRegime::Normal
    };

    let state_out = MarketStateOutput {
        regime,
        volatility_regime: vol_regime,
        trend_stability,
        // Placeholder heuristic, not a measured probability: no actual balance-persistence
        // statistics feed into this yet, only the current `regime`. See the field doc on
        // `MarketStateOutput::balance_probability`.
        balance_probability: if regime == MarketRegime::Consolidation {
            0.8
        } else {
            0.2
        },
        // Placeholder heuristic, not derived from session history yet. See the field doc on
        // `MarketStateOutput::opening_range_percentile`.
        opening_range_percentile: 0.5,
        opening_type,
    };

    (state_out, playbook)
}

/// M2.6: `BalanceMigrationOutput` aus aufeinanderfolgenden `VolumeProfileEngine`-Outputs füllen.
pub fn build_balance_migration_from_profiles(
    prev_vp: &IndicatorOutput,
    new_vp: &IndicatorOutput,
    acceptance_bars: u32,
    min_acceptance_bars: u32,
) -> BalanceMigrationOutput {
    let prev_mid = prev_vp.extra.get("vpoc").copied().unwrap_or(prev_vp.value);
    let new_mid = new_vp.extra.get("vpoc").copied().unwrap_or(new_vp.value);

    build_balance_migration(prev_mid, new_mid, acceptance_bars, min_acceptance_bars)
}

/// M2.7: `StructuralTrailingStop` an neue Pivot-/Struktur-Preise anbinden.
pub fn advance_structural_stop(
    stop: &mut StructuralTrailingStop,
    new_pivot_price: f64,
) -> Option<f64> {
    let old_stop = stop.current_stop;
    stop.advance(new_pivot_price);
    if (stop.current_stop - old_stop).abs() > 1e-9 {
        Some(stop.current_stop)
    } else {
        None
    }
}

/// M2.8: `FreeSpaceScore` aus `VolumeProfileEngine`-Bins und ATR berechnen.
///
/// `atr_val` ist der ATR-Prozentwert (`100 * ATR / close`), wie ihn
/// [`crate::indicator::atr::Atr`] liefert.
pub fn build_free_space_from_profile(
    vp_output: &IndicatorOutput,
    atr_val: f64,
    bar_close: f64,
) -> FreeSpaceScore {
    let vpoc = vp_output
        .extra
        .get("vpoc")
        .copied()
        .unwrap_or(vp_output.value);
    let lvn_width = vp_output.extra.get("lvn_width").copied().unwrap_or(0.0);
    let density = vp_output
        .extra
        .get("current_density")
        .copied()
        .unwrap_or(0.05);

    let atr_raw = (atr_val / 100.0) * bar_close;

    let lvn_width_atr = if atr_raw > 0.0 {
        lvn_width / atr_raw
    } else {
        0.0
    };
    let distance_to_hvn_atr = if atr_raw > 0.0 {
        (bar_close - vpoc).abs() / atr_raw
    } else {
        0.0
    };

    build_free_space_score(lvn_width_atr, distance_to_hvn_atr, density)
}

/// M2.9: `VwapRegimeTracker` mit `indicator::vwap::Vwap` verbinden.
///
/// `atr_val` ist der ATR-Prozentwert (`100 * ATR / close`), wie ihn
/// [`crate::indicator::atr::Atr`] liefert.
pub fn update_vwap_regime(
    tracker: &mut VwapRegimeTracker,
    vwap_output: &IndicatorOutput,
    bar_close: f64,
    atr_val: f64,
    flat_threshold: f64,
    strong_threshold: f64,
) -> VwapRegimeOutput {
    let vwap = vwap_output.value;
    let diff = bar_close - vwap;

    let slope_raw = vwap_output.extra.get("slope").copied().unwrap_or(0.0);
    let sigma = vwap_output
        .extra
        .get("sigma_1_upper")
        .copied()
        .unwrap_or(1.0)
        - vwap;

    let atr_raw = (atr_val / 100.0) * bar_close;

    let slope_atr = if atr_raw > 0.0 {
        slope_raw / atr_raw
    } else {
        0.0
    };

    tracker.update(
        diff,
        atr_raw,
        sigma,
        slope_atr,
        flat_threshold,
        strong_threshold,
    )
}
