//! Deterministic synthetic market data walkthrough:
//! 1. Seed-based Random Walk and Trending series with explicit `BarQuality` metadata.
//! 2. Calibrated Wyckoff schematic (Accumulation) driving a state machine to Phase E.
//! 3. Swing pivot generation and Break of Structure / Change of Character (BOS/CHoCH) detection.
//!
//! Run with:
//! ```bash
//! cargo run --example synthetic_patterns
//! ```

use kestrel_chartkit::indicator::bos_choch::BosChochEngine;
use kestrel_chartkit::indicator::wyckoff::{WyckoffBias, WyckoffPhase, WyckoffStateMachine};
use kestrel_chartkit::indicator::Indicator;
use kestrel_chartkit::synthetic::{
    bos_choch_swing_bars, random_walk_bars, trending_bars, wyckoff_schematic_bars, SwingDirection,
    WyckoffGeneratorConfig,
};
use kestrel_chartkit::{build_checked, QualifiedBar};
use std::collections::HashMap;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("=== 1. Synthetic Random Walk & Trending Bars ===");

    // Generate 10 bars of a random walk starting at 100.0 with drift and volatility
    let seed = 42;
    let walk = random_walk_bars(seed, 10, 100.0, 0.1, 0.5, 1000.0);
    println!("Generated {} random walk bars (seed={seed}):", walk.len());
    for (i, QualifiedBar { bar, quality }) in walk.iter().enumerate().take(5) {
        println!(
            "  bar {i:>2}: O={:.2} H={:.2} L={:.2} C={:.2} V={:.0} (synthetic={}, vol_avail={})",
            bar.open,
            bar.high,
            bar.low,
            bar.close,
            bar.volume,
            quality.is_synthetic,
            quality.volume_available
        );
    }
    println!("  ... [{} more bars]", walk.len() - 5);

    // Generate a trending bar series (uptrend: +0.75 per bar + noise)
    let trend = trending_bars(123, 8, 50.0, 0.75, 0.2, 500.0);
    let start_c = trend.first().unwrap().bar.close;
    let end_c = trend.last().unwrap().bar.close;
    println!(
        "\nGenerated 8 trending bars: start close={start_c:.2} -> end close={end_c:.2} (+{:.2})\n",
        end_c - start_c
    );

    // Feed trending bars into a streaming RSI indicator from the registry
    let mut rsi_params = HashMap::new();
    rsi_params.insert("rsi_len".to_string(), 5.0);
    let mut rsi = build_checked("rsi", &rsi_params)?;

    println!("Streaming trending bars through RSI(5):");
    for (i, qb) in trend.iter().enumerate() {
        match rsi.on_bar(&qb.bar) {
            Some(out) => println!("  bar {i}: close={:.2}  RSI={:.2}", qb.bar.close, out.value),
            None => println!("  bar {i}: close={:.2}  RSI=<warming up>", qb.bar.close),
        }
    }

    println!("\n=== 2. Calibrated Wyckoff Schematic Sequence ===");
    // Generate a calibrated Wyckoff accumulation sequence
    let wyckoff_config = WyckoffGeneratorConfig {
        center_price: 100.0,
        range_lookback: 20,
        spread: 2.0,
        base_volume: 100.0,
    };
    let wyckoff_bars = wyckoff_schematic_bars(777, WyckoffBias::Accumulation, wyckoff_config);
    println!(
        "Generated {} bars for Wyckoff Accumulation schematic",
        wyckoff_bars.len()
    );

    let mut wyckoff_machine = WyckoffStateMachine::new(20, 5.0, 3);
    let mut prev_phase = WyckoffPhase::Undefined;

    for (i, qb) in wyckoff_bars.iter().enumerate() {
        wyckoff_machine.on_bar(&qb.bar);
        let curr_phase = wyckoff_machine.phase();

        // Report state transitions and emitted alerts
        if curr_phase != prev_phase && curr_phase != WyckoffPhase::Undefined {
            println!(
                "  bar {i:>2}: Transition to Phase {:?} (close={:.2}, vol={:.0})",
                curr_phase, qb.bar.close, qb.bar.volume
            );
            for alert in wyckoff_machine.alerts() {
                println!("          Alert: [{}] {}", alert.kind, alert.note);
            }
            prev_phase = curr_phase;
        }
    }

    let score = wyckoff_machine.score();
    println!(
        "Wyckoff Final: Bias={:?}, Phase={:?}, Cause Score={:.2}, Sequence Quality={:.2}\n",
        wyckoff_machine.bias().unwrap(),
        wyckoff_machine.phase(),
        score.cause_score,
        score.sequence_quality
    );

    println!("=== 3. Market Structure Break (BOS / CHoCH) ===");
    // Generate swing pivot bars with pivot_len = 3
    let pivot_len = 3;
    let swing_bars = bos_choch_swing_bars(999, SwingDirection::Bearish, pivot_len);
    println!(
        "Generated {} bars for Bearish Swing Breakout (pivot_len={pivot_len}):",
        swing_bars.len()
    );

    let mut bos_engine = BosChochEngine::new(pivot_len);
    for (i, qb) in swing_bars.iter().enumerate() {
        if let Some(out) = bos_engine.on_bar(&qb.bar) {
            let event_desc = if (out.value - 1.0).abs() < 1e-6 {
                "Bullish BOS (+1.0)"
            } else if (out.value - 2.0).abs() < 1e-6 {
                "Bullish CHoCH (+2.0)"
            } else if (out.value - (-1.0)).abs() < 1e-6 {
                "Bearish BOS (-1.0)"
            } else if (out.value - (-2.0)).abs() < 1e-6 {
                "Bearish CHoCH (-2.0)"
            } else {
                "None (0.0)"
            };
            println!(
                "  bar {i:>2}: close={:.2} -> Event Code={:+.1} ({event_desc})",
                qb.bar.close, out.value
            );
            for alert in bos_engine.alerts() {
                println!("          Alert: [{}] {}", alert.kind, alert.note);
            }
        } else {
            println!(
                "  bar {i:>2}: close={:.2} -> <warming up window>",
                qb.bar.close
            );
        }
    }

    println!("\nSynthetic market patterns demonstration completed successfully.");
    Ok(())
}
