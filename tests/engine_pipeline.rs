mod common;

use common::*;
use kestrel_chartkit::engine::pipeline::*;
use kestrel_chartkit::engine::*;
use kestrel_chartkit::indicator::atr::Atr;
use kestrel_chartkit::indicator::swing_structure::SwingStructureEngine;
use kestrel_chartkit::indicator::volume_profile::VolumeProfileEngine;
use kestrel_chartkit::indicator::vwap::Vwap;
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::model::MarketRegime;

#[test]
fn test_m2_1_swing_structure_with_atr() {
    let mut atr = Atr::with_defaults();
    let mut swing_engine = SwingStructureEngine::with_defaults();
    let bars = generate_sine_bars(250, 100.0, 15.0, 30.0, 1000.0);

    let mut outputs = Vec::new();
    for bar in &bars {
        let atr_out = atr.on_bar(bar);
        if let Some(atr_out) = atr_out {
            let swing_out = update_swing_structure(&mut swing_engine, bar, &atr_out);
            if let Some(out) = swing_out {
                outputs.push(out);
            }
        }
    }

    assert!(
        !outputs.is_empty(),
        "Swing structure engine connected to ATR should produce valid outputs"
    );
}

#[test]
fn test_m2_2_market_context_with_volume_profile() {
    let mut vp = VolumeProfileEngine::new(50, 20);
    let bars = generate_sine_bars(100, 100.0, 5.0, 20.0, 1000.0);

    for bar in &bars {
        let vp_out = vp.on_bar(bar);
        if let Some(vp_out) = vp_out {
            let ctx = build_market_context_from_profile(
                MarketRegime::BullishExpansion,
                bar,
                2.0,
                &vp_out,
                AcceptanceLevel::High,
                AuctionPhase::InsideBalance,
            );
            assert!(ctx.distance_to_vpoc_atr >= 0.0);
            assert_eq!(ctx.regime, MarketRegime::BullishExpansion);
        }
    }
}

#[test]
fn test_m2_3_auction_phase_derivation() {
    let mut phase = AuctionPhase::InsideBalance;

    // 1. MSB (Breakout) triggert Breakout Phase
    let msb_breakout = kestrel_chartkit::indicator::IndicatorOutput::new(1.0);
    phase = derive_auction_phase(Some(&msb_breakout), None, phase);
    assert_eq!(phase, AuctionPhase::Breakout);

    // 2. Bestätigter OrderBlock mit Haltedauer triggert Acceptance Phase
    let mut ob_extra = std::collections::HashMap::new();
    ob_extra.insert("active_ob_duration".to_string(), 5.0);
    let ob_acceptance = kestrel_chartkit::indicator::IndicatorOutput::with_extra(1.0, ob_extra);

    phase = derive_auction_phase(None, Some(&ob_acceptance), phase);
    assert_eq!(phase, AuctionPhase::Acceptance { duration_bars: 5 });
}

#[test]
fn test_m2_4_and_5_market_state_and_playbook_pipeline() {
    let bars = generate_trend_bars(100, 100.0, 1.5, 1000.0);
    let (state, playbook) = build_market_state_and_playbook(
        &bars,
        25.0, // ADX > 20 -> Trending
        0.01,
        0.8, // High trend stability
        OpeningType::InsideValue,
    );

    assert_eq!(state.regime, MarketRegime::BullishExpansion);
    assert_eq!(playbook.continuation_long, PlaybookState::Preferred);
    assert_eq!(playbook.continuation_short, PlaybookState::NotAllowed);
}

#[test]
fn test_m2_4_atr_percent_unit_reaches_consolidation_regime() {
    // Regression: `atr_val` from a real `Atr` indicator is a percent value (e.g. ~0.3 for 0.3%
    // ATR). `classify_regime`'s non-trending branch expects ATR as a fraction (threshold 0.02 =
    // 2%). Feeding the percent value through unconverted always exceeds 0.02 for realistic
    // markets, so low-volatility, non-trending bars would incorrectly never classify as
    // Consolidation.
    let mut atr = Atr::with_defaults();
    let bars = generate_flat_spread_bars(60, 100.0, 0.5, 1000.0);

    let mut last_atr = None;
    for bar in &bars {
        if let Some(out) = atr.on_bar(bar) {
            last_atr = Some(out);
        }
    }
    let atr_out = last_atr.expect("ATR should produce output after warmup");
    assert!(
        atr_out.value < 2.0,
        "expected a realistic low ATR percent value, got {}",
        atr_out.value
    );

    let (state, _playbook) = build_market_state_and_playbook(
        &bars,
        10.0, // ADX < 20 -> non-trending
        atr_out.value,
        0.5,
        OpeningType::InsideValue,
    );

    assert_eq!(state.regime, MarketRegime::Consolidation);
}

#[test]
fn test_m2_6_balance_migration_from_volume_profile() {
    let mut vp1 = VolumeProfileEngine::new(30, 10);
    let mut vp2 = VolumeProfileEngine::new(30, 10);

    let bars1 = generate_flat_spread_bars(40, 100.0, 2.0, 1000.0);
    let bars2 = generate_flat_spread_bars(40, 120.0, 2.0, 1000.0); // Shifted higher

    let mut out1 = None;
    for b in &bars1 {
        out1 = vp1.on_bar(b);
    }

    let mut out2 = None;
    for b in &bars2 {
        out2 = vp2.on_bar(b);
    }

    let migration = build_balance_migration_from_profiles(&out1.unwrap(), &out2.unwrap(), 10, 5);

    assert_eq!(migration.direction, MigrationDirection::Bullish);
    assert!(migration.migration > 10.0);
    assert_eq!(migration.acceptance_strength, 1.0);
}

#[test]
fn test_m2_7_structural_trailing_stop_advancement() {
    let mut stop = StructuralTrailingStop::new(90.0, true);
    let advanced_1 = advance_structural_stop(&mut stop, 95.0);
    assert_eq!(advanced_1, Some(95.0));

    // For Long, stop should only advance upward, not downward
    let advanced_2 = advance_structural_stop(&mut stop, 92.0);
    assert_eq!(advanced_2, None);
}

#[test]
fn test_m2_8_free_space_score_from_volume_profile() {
    let mut vp = VolumeProfileEngine::new(50, 20);
    let bars = generate_sine_bars(100, 100.0, 5.0, 20.0, 1000.0);

    for bar in &bars {
        if let Some(vp_out) = vp.on_bar(bar) {
            let free_space = build_free_space_from_profile(&vp_out, 2.0, bar.close);
            assert!((0.0..=100.0).contains(&free_space.score));
        }
    }
}

#[test]
fn test_m2_9_vwap_regime_tracker_with_vwap_indicator() {
    let mut vwap = Vwap::new(50, 10);
    let mut tracker = VwapRegimeTracker::new(20);
    let bars = generate_trend_bars(60, 100.0, 1.0, 1000.0);

    let mut regime_outputs = Vec::new();
    for bar in &bars {
        if let Some(vwap_out) = vwap.on_bar(bar) {
            let regime_out = update_vwap_regime(&mut tracker, &vwap_out, bar.close, 1.5, 0.1, 0.5);
            regime_outputs.push(regime_out);
        }
    }

    assert!(!regime_outputs.is_empty());
    let last = regime_outputs.last().unwrap();
    assert!(last.price_persistence > 0.5);
}
