//! Price levels from an OHLC window: support/resistance, pivots, retracements and RSI targets.
//! Pivots use the supplied window, not an implicitly selected exchange session.

#[cfg(feature = "serde")]
use serde::Serialize;

use crate::Bar;

const DEFAULT_WINDOW: usize = 120;
const RSI_LEN: usize = 14;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CheatSheetLevelKind {
    Support,
    Resistance,
    Pivot,
    Fibonacci,
    MovingAverage,
    RsiTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum CheatSheetLevelSide {
    Below,
    At,
    Above,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CheatSheetLevel {
    pub kind: CheatSheetLevelKind,
    pub label: String,
    pub price: f64,
    pub distance_pct: f64,
    pub side: CheatSheetLevelSide,
    pub strength: f64,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct CheatSheetReading {
    pub last: f64,
    pub window_high: f64,
    pub window_low: f64,
    pub levels: Vec<CheatSheetLevel>,
}

pub fn cheat_sheet(bars: &[Bar], window: usize) -> Option<CheatSheetReading> {
    if bars.len() < 2 {
        return None;
    }
    let requested = if window == 0 { DEFAULT_WINDOW } else { window };
    let n = requested.max(2).min(bars.len());
    let bars = &bars[bars.len() - n..];
    let last = bars.last()?.close;
    let window_high = bars
        .iter()
        .map(|b| b.high)
        .fold(f64::NEG_INFINITY, f64::max);
    let window_low = bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min);
    let span = window_high - window_low;
    if span <= 0.0 || last <= 0.0 {
        return None;
    }

    let mut levels = Vec::new();
    add_support_resistance(&mut levels, bars, last, span);
    add_pivots(&mut levels, last, window_high, window_low);
    add_fibonacci(&mut levels, last, window_high, window_low);
    add_moving_averages(&mut levels, bars, last, span);
    add_rsi_targets(&mut levels, bars, last);

    levels.sort_by(|a, b| {
        a.distance_pct
            .abs()
            .total_cmp(&b.distance_pct.abs())
            .then_with(|| a.label.cmp(&b.label))
    });

    Some(CheatSheetReading {
        last,
        window_high,
        window_low,
        levels,
    })
}

fn add_level(
    levels: &mut Vec<CheatSheetLevel>,
    kind: CheatSheetLevelKind,
    label: impl Into<String>,
    price: f64,
    last: f64,
    strength: f64,
) {
    if !price.is_finite() || price <= 0.0 || last <= 0.0 {
        return;
    }
    let distance_pct = 100.0 * (price / last - 1.0);
    let side = if distance_pct > 0.05 {
        CheatSheetLevelSide::Above
    } else if distance_pct < -0.05 {
        CheatSheetLevelSide::Below
    } else {
        CheatSheetLevelSide::At
    };
    levels.push(CheatSheetLevel {
        kind,
        label: label.into(),
        price,
        distance_pct,
        side,
        strength: strength.clamp(0.0, 1.0),
    });
}

fn add_support_resistance(levels: &mut Vec<CheatSheetLevel>, bars: &[Bar], last: f64, span: f64) {
    let tolerance = (span * 0.015).max(last * 0.001);
    let mut supports = Vec::new();
    let mut resistances = Vec::new();

    for i in 2..bars.len().saturating_sub(2) {
        let low = bars[i].low;
        if low <= bars[i - 1].low
            && low <= bars[i - 2].low
            && low <= bars[i + 1].low
            && low <= bars[i + 2].low
        {
            supports.push((low, touches(bars, low, tolerance)));
        }
        let high = bars[i].high;
        if high >= bars[i - 1].high
            && high >= bars[i - 2].high
            && high >= bars[i + 1].high
            && high >= bars[i + 2].high
        {
            resistances.push((high, touches(bars, high, tolerance)));
        }
    }

    supports.push((bars.iter().map(|b| b.low).fold(f64::INFINITY, f64::min), 1));
    resistances.push((
        bars.iter()
            .map(|b| b.high)
            .fold(f64::NEG_INFINITY, f64::max),
        1,
    ));

    supports.sort_by(|a, b| (last - b.0).abs().total_cmp(&(last - a.0).abs()));
    supports.retain(|(price, _)| *price <= last);
    supports.sort_by(|a, b| (last - a.0).total_cmp(&(last - b.0)));
    supports.dedup_by(|a, b| (a.0 - b.0).abs() <= tolerance);

    resistances.retain(|(price, _)| *price >= last);
    resistances.sort_by(|a, b| (a.0 - last).total_cmp(&(b.0 - last)));
    resistances.dedup_by(|a, b| (a.0 - b.0).abs() <= tolerance);

    for (idx, (price, count)) in supports.into_iter().take(3).enumerate() {
        add_level(
            levels,
            CheatSheetLevelKind::Support,
            format!("S{}", idx + 1),
            price,
            last,
            count as f64 / 6.0,
        );
    }
    for (idx, (price, count)) in resistances.into_iter().take(3).enumerate() {
        add_level(
            levels,
            CheatSheetLevelKind::Resistance,
            format!("R{}", idx + 1),
            price,
            last,
            count as f64 / 6.0,
        );
    }
}

fn touches(bars: &[Bar], level: f64, tolerance: f64) -> usize {
    bars.iter()
        .filter(|b| (b.high - level).abs() <= tolerance || (b.low - level).abs() <= tolerance)
        .count()
}

fn add_pivots(levels: &mut Vec<CheatSheetLevel>, last: f64, high: f64, low: f64) {
    let pivot = (high + low + last) / 3.0;
    add_level(
        levels,
        CheatSheetLevelKind::Pivot,
        "Pivot",
        pivot,
        last,
        0.8,
    );
    add_level(
        levels,
        CheatSheetLevelKind::Pivot,
        "Pivot S1",
        2.0 * pivot - high,
        last,
        0.65,
    );
    add_level(
        levels,
        CheatSheetLevelKind::Pivot,
        "Pivot R1",
        2.0 * pivot - low,
        last,
        0.65,
    );
    add_level(
        levels,
        CheatSheetLevelKind::Pivot,
        "Pivot S2",
        pivot - (high - low),
        last,
        0.45,
    );
    add_level(
        levels,
        CheatSheetLevelKind::Pivot,
        "Pivot R2",
        pivot + (high - low),
        last,
        0.45,
    );
}

fn add_fibonacci(levels: &mut Vec<CheatSheetLevel>, last: f64, high: f64, low: f64) {
    for ratio in [0.236, 0.382, 0.5, 0.618, 0.786] {
        let price = high - (high - low) * ratio;
        add_level(
            levels,
            CheatSheetLevelKind::Fibonacci,
            format!("Fib {:.1}%", ratio * 100.0),
            price,
            last,
            0.5,
        );
    }
}

fn add_moving_averages(levels: &mut Vec<CheatSheetLevel>, bars: &[Bar], last: f64, span: f64) {
    let ma20 = sma(bars, 20);
    let ma50 = sma(bars, 50);
    let ma200 = sma(bars, 200);
    for (label, value) in [("SMA20", ma20), ("SMA50", ma50), ("SMA200", ma200)] {
        if let Some(price) = value {
            add_level(
                levels,
                CheatSheetLevelKind::MovingAverage,
                label,
                price,
                last,
                0.55,
            );
        }
    }
    if let (Some(a), Some(b)) = (ma20, ma50) {
        let spread = (a - b).abs();
        if spread <= (span * 0.025).max(last * 0.0025) {
            add_level(
                levels,
                CheatSheetLevelKind::MovingAverage,
                "SMA20/50 Stall",
                (a + b) / 2.0,
                last,
                0.8,
            );
        }
    }
}

fn sma(bars: &[Bar], len: usize) -> Option<f64> {
    if bars.len() < len {
        return None;
    }
    let window = &bars[bars.len() - len..];
    Some(window.iter().map(|b| b.close).sum::<f64>() / len as f64)
}

fn add_rsi_targets(levels: &mut Vec<CheatSheetLevel>, bars: &[Bar], last: f64) {
    let Some((avg_gain, avg_loss)) = wilder_gain_loss(bars, RSI_LEN) else {
        return;
    };
    for target in [30.0, 50.0, 70.0] {
        if let Some(price) = rsi_target_price(last, avg_gain, avg_loss, RSI_LEN, target) {
            add_level(
                levels,
                CheatSheetLevelKind::RsiTarget,
                format!("RSI {:.0}", target),
                price,
                last,
                0.5,
            );
        }
    }
}

fn wilder_gain_loss(bars: &[Bar], len: usize) -> Option<(f64, f64)> {
    if bars.len() < len + 1 {
        return None;
    }
    let mut gains = Vec::with_capacity(len);
    let mut losses = Vec::with_capacity(len);
    for pair in bars[..=len].windows(2) {
        let change = pair[1].close - pair[0].close;
        gains.push(change.max(0.0));
        losses.push((-change).max(0.0));
    }
    let mut avg_gain = gains.iter().sum::<f64>() / len as f64;
    let mut avg_loss = losses.iter().sum::<f64>() / len as f64;
    for pair in bars[len..].windows(2) {
        let change = pair[1].close - pair[0].close;
        avg_gain = (avg_gain * (len as f64 - 1.0) + change.max(0.0)) / len as f64;
        avg_loss = (avg_loss * (len as f64 - 1.0) + (-change).max(0.0)) / len as f64;
    }
    Some((avg_gain, avg_loss))
}

fn rsi_target_price(
    last: f64,
    avg_gain: f64,
    avg_loss: f64,
    len: usize,
    target: f64,
) -> Option<f64> {
    if !(0.0..100.0).contains(&target) {
        return None;
    }
    let rs = target / (100.0 - target);
    let k = len as f64 - 1.0;
    let up_delta = rs * avg_loss * k - avg_gain * k;
    let down_delta = avg_loss * k - avg_gain * k / rs;
    let delta = if up_delta >= 0.0 {
        up_delta
    } else if down_delta <= 0.0 {
        down_delta
    } else {
        up_delta
    };
    let price = last + delta;
    (price > 0.0).then_some(price)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(c: f64) -> Bar {
        Bar {
            timestamp: 0,
            open: c,
            high: c + 1.0,
            low: c - 1.0,
            close: c,
            volume: 1.0,
        }
    }

    #[test]
    fn creates_core_level_groups() {
        let bars: Vec<Bar> = (0..140).map(|i| bar(100.0 + (i % 20) as f64)).collect();
        let r = cheat_sheet(&bars, 120).expect("reading");
        assert!(r
            .levels
            .iter()
            .any(|l| l.kind == CheatSheetLevelKind::Pivot));
        assert!(r
            .levels
            .iter()
            .any(|l| l.kind == CheatSheetLevelKind::Fibonacci));
        assert!(r
            .levels
            .iter()
            .any(|l| l.kind == CheatSheetLevelKind::MovingAverage));
        assert!(r
            .levels
            .iter()
            .any(|l| l.kind == CheatSheetLevelKind::RsiTarget));
    }
}
