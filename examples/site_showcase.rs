//! Writes the chartlet specifications behind the charts of the project website (`site/`). Every
//! number in them is computed by this crate; chartlet (`@casoon/chartlet`) only draws them, at
//! the site's build time. The bars are synthetic — a fictitious CASOON share with invented prices,
//! fixed seeds — so every run writes the same files.
//!
//! ```bash
//! cargo run --example site_showcase
//! ```
//!
//! Writes into `site/.generated/` (not committed; the Pages workflow runs this example first):
//! `hero.json`, `regime.json`, `composite.json`, one `indicators/<name>.json` per catalog
//! indicator, and `indicators.json`, the list the showcase is built from.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use kestrel_chartkit::indicator::definition::families;
use kestrel_chartkit::model::{MarketRegime, ZoneKind};
use kestrel_chartkit::signal::TriggerAction;
use kestrel_chartkit::structure::find_sr_zones;
use kestrel_chartkit::synthetic::trending_bars;
use kestrel_chartkit::{
    aggregate_subscores, build_checked, catalog, classify_regime, output_unit, score_indicator,
    Bar, IndicatorUnit,
};

fn params(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
    pairs.iter().map(|&(k, v)| (k.to_string(), v)).collect()
}

/// Text for a JSON string literal.
fn esc(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Consecutive trending legs `(bars, drift per bar)`, joined into one continuous series.
fn legs(seed: u64, legs: &[(usize, f64)]) -> Vec<Bar> {
    let mut bars: Vec<Bar> = Vec::new();
    let mut price = 100.0;
    for (i, &(count, drift)) in legs.iter().enumerate() {
        for q in trending_bars(seed + i as u64, count, price, drift, 0.6, 1_000.0) {
            let b = q.bar;
            price = b.close;
            bars.push(Bar::new(0, b.open, b.high, b.low, b.close, b.volume));
        }
    }
    bars
}

/// Trending legs as 4-hour bars of a fictitious CASOON share (invented prices, no real quotes)
/// from 5 January 2026. The volume follows each bar's range, so a volume pane has something to
/// say.
fn demo_bars(seed: u64, pattern: &[(usize, f64)]) -> Vec<Bar> {
    let raw = legs(seed, pattern);
    let average_range = raw.iter().map(|b| b.high - b.low).sum::<f64>() / raw.len() as f64;
    raw.iter()
        .enumerate()
        .map(|(i, b)| {
            let volume = (800.0 + 2_400.0 * (b.high - b.low) / average_range).round();
            Bar::new(
                1_767_571_200 + i as i64 * 14_400,
                b.open,
                b.high,
                b.low,
                b.close,
                volume,
            )
        })
        .collect()
}

fn candles(bars: &[Bar]) -> String {
    let candles: Vec<String> = bars
        .iter()
        .map(|b| {
            format!(
                r#"{{"time":{},"open":{:.4},"high":{:.4},"low":{:.4},"close":{:.4}}}"#,
                b.timestamp, b.open, b.high, b.low, b.close
            )
        })
        .collect();
    format!(
        r#"{{"mark":"ohlc","name":"CASOON (fictitious)","data":[{}]}}"#,
        candles.join(",")
    )
}

fn hero_chart() -> Result<String, Box<dyn Error>> {
    // Up, pullback, up, sell-off, recovery.
    let bars = demo_bars(
        11,
        &[(32, 0.28), (18, -0.22), (26, 0.32), (16, -0.45), (18, 0.18)],
    );
    let mut ema_fast = build_checked("ema", &params(&[("period", 20.0)]))?;
    let mut ema_slow = build_checked("ema", &params(&[("period", 50.0)]))?;
    let mut rsi = build_checked("rsi", &HashMap::new())?;
    let (mut fast, mut slow, mut rsi_line) = (Vec::new(), Vec::new(), Vec::new());
    let mut signals: Vec<(usize, bool, &'static str)> = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        if let Some(o) = ema_fast.on_bar(bar) {
            fast.push((bar.timestamp, o.value));
        }
        if let Some(o) = ema_slow.on_bar(bar) {
            slow.push((bar.timestamp, o.value));
        }
        if let Some(o) = rsi.on_bar(bar) {
            rsi_line.push((bar.timestamp, o.value));
            for alert in rsi.alerts() {
                match alert.kind.as_str() {
                    "bull_extreme" => signals.push((i, true, "RSI up")),
                    "bear_extreme" => signals.push((i, false, "RSI down")),
                    _ => {}
                }
            }
        }
    }
    let last = bars.last().ok_or("no bars")?;

    let point = |(t, v): &(i64, f64)| format!(r#"{{"time":{t},"value":{v:.4}}}"#);
    let line = |name: &str, points: &[(i64, f64)]| {
        let points: Vec<String> = points.iter().map(point).collect();
        format!(
            r#"{{"mark":"line","name":"{name}","markers":false,"points":[{}]}}"#,
            points.join(",")
        )
    };
    let candles: Vec<String> = bars
        .iter()
        .map(|b| {
            format!(
                r#"{{"time":{},"open":{:.4},"high":{:.4},"low":{:.4},"close":{:.4}}}"#,
                b.timestamp, b.open, b.high, b.low, b.close
            )
        })
        .collect();
    let mut price = vec![format!(
        r#"{{"mark":"ohlc","name":"CASOON (fictitious)","data":[{}]}}"#,
        candles.join(",")
    )];
    price.push(line("EMA 20", &fast));
    price.push(line("EMA 50", &slow));
    // The nearest pivot zone on either side of the last close (`find_sr_zones`, pivots of 5).
    let zones = find_sr_zones(&bars, 5);
    for (kind, label) in [
        (ZoneKind::Support, "Support"),
        (ZoneKind::Resistance, "Resistance"),
    ] {
        let nearest = zones
            .iter()
            .filter(|z| z.kind == kind)
            .min_by(|a, b| a.distance_pct.abs().total_cmp(&b.distance_pct.abs()));
        if let Some(z) = nearest {
            price.push(format!(
                r#"{{"mark":"band","label":"{label}","bottom":{:.4},"top":{:.4}}}"#,
                z.price_bottom, z.price_top
            ));
        }
    }
    price.push(format!(
        r#"{{"mark":"annotation","value":{:.4},"label":"Last {:.2}"}}"#,
        last.close, last.close
    ));
    // The three latest signals, under the low of a turn up, over the high of a turn down.
    for &(i, up, label) in signals.iter().rev().take(3) {
        let bar = &bars[i];
        price.push(format!(
            r#"{{"mark":"annotation","time":{},"value":{:.4},"label":"{label}","shape":"{}"}}"#,
            bar.timestamp,
            if up { bar.low } else { bar.high },
            if up { "triangle-up" } else { "triangle-down" }
        ));
    }
    let volume: Vec<String> = bars
        .iter()
        .map(|b| point(&(b.timestamp, b.volume)))
        .collect();

    Ok(format!(
        r#"{{"schemaVersion":1,"type":"time","title":"CASOON share (fictitious prices) · 4h","description":"Synthetic 4-hour candles of a fictitious CASOON share — invented prices, no real quotes — with EMA 20 and EMA 50, the nearest support and resistance zones from the confirmed pivots, the last price and the latest RSI signals, all computed by kestrel-chartkit. Volume and RSI(14) below.","width":640,"height":560,"timeAxis":{{"gaps":"collapse"}},"panes":[{{"heightRatio":8,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}},{{"heightRatio":2,"valueAxis":{{"title":"Volume"}},"layers":[{{"mark":"area","name":"Volume","stroke":"thin","points":[{}]}}]}},{{"heightRatio":2,"valueAxis":{{"title":"RSI","decimals":0}},"layers":[{},{{"mark":"annotation","value":70,"label":"70"}},{{"mark":"annotation","value":30,"label":"30"}}]}}]}}"#,
        price.join(","),
        volume.join(","),
        line("RSI 14", &rsi_line),
    ))
}

/// The market regime per bar (`classify_regime` with ADX(14) and ATR(14)) as a band under the
/// candles, one colour per regime, and the ADX with its trend threshold below.
fn regime_chart() -> Result<String, Box<dyn Error>> {
    let bars = demo_bars(3, &[(42, 0.45), (36, 0.0), (42, -0.5), (26, 0.0)]);
    let mut adx = build_checked("adx", &HashMap::new())?;
    let mut atr = build_checked("atr", &HashMap::new())?;
    let mut regimes: Vec<(i64, Option<MarketRegime>)> = Vec::new();
    let mut adx_line = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let (Some(a), Some(t)) = (adx.on_bar(bar), atr.on_bar(bar)) else {
            regimes.push((bar.timestamp, None));
            continue;
        };
        // The ATR indicator reports 100 * ATR / close; classify_regime expects the fraction.
        let regime = classify_regime(&bars[..=i], a.value, t.value / 100.0);
        regimes.push((bar.timestamp, Some(regime)));
        adx_line.push(format!(
            r#"{{"time":{},"value":{:.4}}}"#,
            bar.timestamp, a.value
        ));
    }
    // Mid tones that read on a light and a dark page alike.
    let kinds = [
        (
            MarketRegime::BullishExpansion,
            "Bullish expansion",
            "#2f9e5b",
        ),
        (
            MarketRegime::BearishExpansion,
            "Bearish expansion",
            "#d0454f",
        ),
        (MarketRegime::Consolidation, "Consolidation", "#8a94a6"),
        (MarketRegime::Transition, "Transition", "#c99a2e"),
    ];
    let mut bands = Vec::new();
    for (kind, label, color) in kinds {
        if !regimes.iter().any(|(_, r)| *r == Some(kind)) {
            continue;
        }
        // A regime holds until the next bar; at a change it ends on the first bar of the new one,
        // so the band has no hole.
        let points: Vec<String> = regimes
            .iter()
            .enumerate()
            .map(|(i, (t, r))| {
                let here = *r == Some(kind);
                let ends_here = r.is_some() && i > 0 && regimes[i - 1].1 == Some(kind);
                let value = if here || ends_here { "1" } else { "null" };
                format!(r#"{{"time":{t},"value":{value}}}"#)
            })
            .collect();
        bands.push(format!(
            r#"{{"mark":"area","name":"{label}","color":"{color}","curve":"step","markers":false,"points":[{}]}}"#,
            points.join(",")
        ));
    }
    Ok(format!(
        r#"{{"schemaVersion":1,"type":"time","title":"Market regime per bar · CASOON (fictitious) 4h","description":"Synthetic 4-hour candles of a fictitious CASOON share in four legs: up, sideways, down, sideways. classify_regime runs on every bar with ADX(14) and ATR(14); the band below the candles shows the regime, the lowest pane the ADX with the trend threshold of 20. The first bars have no regime yet: ADX and ATR are still warming up.","width":640,"height":560,"timeAxis":{{"gaps":"collapse"}},"panes":[{{"heightRatio":4,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}},{{"heightRatio":1,"valueAxis":{{"title":"Regime","min":0,"max":1,"exact":true,"step":1}},"layers":[{}]}},{{"heightRatio":2,"valueAxis":{{"title":"ADX","decimals":0}},"layers":[{{"mark":"line","name":"ADX 14","markers":false,"points":[{}]}},{{"mark":"annotation","value":20,"label":"trend threshold 20"}}]}}]}}"#,
        candles(&bars),
        bands.join(","),
        adx_line.join(",")
    ))
}

/// MACD, Vortex and Trend Quality scored on every bar and aggregated with the regime: the
/// composite score below the candles, and a marker where a Buy or Sell trigger starts.
fn composite_chart() -> Result<String, Box<dyn Error>> {
    let bars = demo_bars(3, &[(42, 0.45), (36, 0.0), (42, -0.5), (26, 0.0)]);
    let names = ["macd", "vortex", "trend_quality"];
    let mut indicators = names
        .iter()
        .map(|name| build_checked(name, &HashMap::new()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut adx = build_checked("adx", &HashMap::new())?;
    let mut atr = build_checked("atr", &HashMap::new())?;
    let mut scores = Vec::new();
    let mut triggers = Vec::new();
    let mut previous = TriggerAction::Hold;
    for (i, bar) in bars.iter().enumerate() {
        let mut subscores = Vec::new();
        for (name, indicator) in names.iter().zip(indicators.iter_mut()) {
            if let Some(out) = indicator.on_bar(bar) {
                subscores.push(score_indicator(name, &out, &indicator.alerts()));
            }
        }
        let (Some(a), Some(t)) = (adx.on_bar(bar), atr.on_bar(bar)) else {
            continue;
        };
        if subscores.len() < names.len() {
            continue;
        }
        let regime = classify_regime(&bars[..=i], a.value, t.value / 100.0);
        let signal = aggregate_subscores(
            subscores,
            None,
            regime,
            Vec::new(),
            Some(bar),
            t.extra["raw"],
        );
        scores.push(format!(
            r#"{{"time":{},"value":{:.4}}}"#,
            bar.timestamp, signal.score
        ));
        // Mark the bar where a trigger starts, not every bar it persists.
        if signal.trigger != previous && signal.trigger != TriggerAction::Hold {
            triggers.push((i, signal.trigger, signal.score));
        }
        previous = signal.trigger;
    }
    let mut price = vec![candles(&bars)];
    // The latest five, none in the last eight bars: a label at the right edge has no room.
    let shown: Vec<_> = triggers.iter().filter(|t| t.0 + 8 < bars.len()).collect();
    for &&(i, action, score) in shown.iter().rev().take(5).rev() {
        let bar = &bars[i];
        let buy = action == TriggerAction::Buy;
        price.push(format!(
            r#"{{"mark":"annotation","time":{},"value":{:.4},"label":"{:?} {:+.2}","shape":"{}"}}"#,
            bar.timestamp,
            if buy { bar.low } else { bar.high },
            action,
            score,
            if buy { "triangle-up" } else { "triangle-down" }
        ));
    }
    Ok(format!(
        r#"{{"schemaVersion":1,"type":"time","title":"Composite signal · CASOON (fictitious) 4h","description":"MACD, Vortex and Trend Quality scored on every bar and weighed against the market regime, on synthetic 4-hour candles of a fictitious CASOON share. The lower pane shows the composite score; a marker shows where a Buy or Sell trigger starts, labelled with the score. Only a ClearToTrade grade produces a trigger, so a direction against the regime stays silent.","width":640,"height":420,"timeAxis":{{"gaps":"collapse"}},"panes":[{{"heightRatio":5,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}},{{"heightRatio":2,"valueAxis":{{"title":"Score","decimals":2}},"layers":[{{"mark":"line","name":"composite score","markers":false,"points":[{}]}},{{"mark":"annotation","value":0,"label":"0"}}]}}]}}"#,
        price.join(","),
        scores.join(",")
    ))
}

/// Bars shown per indicator chart; the indicators run over the whole demo series first, so long
/// warmups are complete before the window starts.
const INDICATOR_WINDOW: usize = 150;

/// One chartlet specification per catalog indicator (default parameters, the same fictitious
/// series): candles with the indicator over them when its values are prices, below them in a
/// pane of its own otherwise; `secondary` and extras as further lines; the latest alerts as
/// markers. `None` when the indicator produces too few values in the window.
fn indicator_chart(name: &str, description: &str, bars: &[Bar]) -> Option<String> {
    let entry = catalog().into_iter().find(|e| e.name == name)?;
    let mut params = HashMap::new();
    if entry.default_params.contains_key("bar_seconds") {
        params.insert("bar_seconds".to_string(), 14_400.0);
    }
    let mut indicator = build_checked(name, &params).ok()?;
    let start = bars.len().saturating_sub(INDICATOR_WINDOW);
    let window = &bars[start..];
    let mut outputs = Vec::new();
    let mut alerts: Vec<(usize, String)> = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let output = indicator.on_bar(bar);
        if i >= start {
            for alert in indicator.alerts() {
                alerts.push((i - start, alert.kind));
            }
            outputs.push(output);
        }
    }
    // A series over the window, `None` where the field is missing: the line breaks there instead
    // of joining two distant values. Leading gaps are dropped.
    type Series = Vec<(i64, Option<f64>)>;
    let series = |pick: &dyn Fn(&kestrel_chartkit::indicator::IndicatorOutput) -> Option<f64>| {
        let points: Series = window
            .iter()
            .zip(&outputs)
            .map(|(b, o)| {
                let v = o.as_ref().and_then(pick).filter(|v| v.is_finite());
                (b.timestamp, v)
            })
            .skip_while(|(_, v)| v.is_none())
            .collect();
        points
    };
    let defined = |points: &Series| points.iter().filter(|(_, v)| v.is_some()).count();
    let values = series(&|o| Some(o.value));
    if defined(&values) < 5 {
        return None;
    }
    let point = |(t, v): &(i64, Option<f64>)| match v {
        Some(v) => format!(r#"{{"time":{t},"value":{v:.6}}}"#),
        None => format!(r#"{{"time":{t},"value":null}}"#),
    };
    let line = |label: &str, points: &Series| {
        let points: Vec<String> = points.iter().map(point).collect();
        format!(
            r#"{{"mark":"line","name":"{label}","markers":false,"points":[{}]}}"#,
            points.join(",")
        )
    };

    // Lines besides `value`: secondary, then extras — `signal` first, the rest by name.
    let mut extra_keys: Vec<String> = outputs
        .iter()
        .flatten()
        .flat_map(|o| o.extra.keys().cloned())
        .collect();
    extra_keys.sort();
    extra_keys.dedup();
    extra_keys.sort_by_key(|k| k != "signal");
    let low = window.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let high = window.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let margin = (high - low) * 0.25;
    let on_price_scale = |points: &Series| {
        let defined: Vec<f64> = points.iter().filter_map(|(_, v)| *v).collect();
        defined.len() >= 5
            && defined
                .iter()
                .filter(|v| **v >= low - margin && **v <= high + margin)
                .count()
                * 10
                >= defined.len() * 9
    };
    let price_unit = output_unit(name) == IndicatorUnit::Price;

    let candles: Vec<String> = window
        .iter()
        .map(|b| {
            format!(
                r#"{{"time":{},"open":{:.4},"high":{:.4},"low":{:.4},"close":{:.4}}}"#,
                b.timestamp, b.open, b.high, b.low, b.close
            )
        })
        .collect();
    let mut price = vec![format!(
        r#"{{"mark":"ohlc","name":"CASOON (fictitious)","data":[{}]}}"#,
        candles.join(",")
    )];
    let mut own: Vec<String> = Vec::new();
    // Each line goes where its values live: over the candles when they stay on the price scale
    // (and the indicator does not declare a non-price unit for `value`), in the pane below
    // otherwise. ichimoku's `value` is the cloud width although its unit is Price.
    let mut place = |label: &str, points: &Series, is_value: bool| {
        let over_candles = on_price_scale(points) && (!is_value || price_unit);
        let target = if over_candles { &mut price } else { &mut own };
        if target.len() < 5 {
            target.push(line(label, points));
            true
        } else {
            false
        }
    };
    place("value", &values, true);
    let mut extra_lines = 0;
    let secondary = series(&|o| o.secondary);
    let mut candidates = vec![("secondary".to_string(), secondary)];
    for key in &extra_keys {
        candidates.push((key.clone(), series(&|o| o.extra.get(key).copied())));
    }
    for (label, points) in candidates {
        if defined(&points) >= 5 && extra_lines < 3 && place(&label, &points, false) {
            extra_lines += 1;
        }
    }

    // The latest alerts, at least eight bars apart so their labels stay readable, at most three:
    // under the low for a bullish kind, over the high for a bearish one, at the close otherwise.
    let mut shown: Vec<&(usize, String)> = Vec::new();
    // Alerts of the last eight bars are left out: a label at the right edge has no room.
    for alert in alerts.iter().rev().filter(|a| a.0 + 8 < window.len()) {
        if shown.len() == 3 {
            break;
        }
        if shown.iter().all(|s| s.0.abs_diff(alert.0) >= 8) {
            shown.push(alert);
        }
    }
    for (i, kind) in shown.iter().rev().map(|a| (&a.0, &a.1)) {
        let bar = &window[*i];
        let bullish = kind.starts_with("bull") || kind.contains("bullish") || kind.ends_with("_up");
        let bearish =
            kind.starts_with("bear") || kind.contains("bearish") || kind.ends_with("_down");
        let (value, shape) = if bullish {
            (bar.low, "triangle-up")
        } else if bearish {
            (bar.high, "triangle-down")
        } else {
            (bar.close, "circle")
        };
        price.push(format!(
            r#"{{"mark":"annotation","time":{},"value":{value:.4},"label":"{}","shape":"{shape}"}}"#,
            bar.timestamp,
            kind.strip_prefix(&format!("{name}_")).unwrap_or(kind)
        ));
    }

    let mut panes = vec![format!(
        r#"{{"heightRatio":3,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}}"#,
        price.join(",")
    )];
    if !own.is_empty() {
        panes.push(format!(
            r#"{{"heightRatio":2,"valueAxis":{{"title":"{name}"}},"layers":[{}]}}"#,
            own.join(",")
        ));
    }
    // The catalog description up to its first sentence; chartlet takes at most 1000 characters.
    let first = description.split(". ").next().unwrap_or(description);
    let described = esc(&first.chars().take(400).collect::<String>());
    Some(format!(
        r#"{{"schemaVersion":1,"type":"time","title":"{name} · CASOON (fictitious) 4h","description":"{described}. Default parameters on synthetic 4-hour bars of a fictitious CASOON share (invented prices), the last {INDICATOR_WINDOW} bars; {} alerts in view, the latest as markers.","width":640,"height":{},"timeAxis":{{"gaps":"collapse"}},"panes":[{}]}}"#,
        alerts.len(),
        if own.is_empty() { 320 } else { 420 },
        panes.join(",")
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("site/.generated");
    let indicators = out.join("indicators");
    fs::create_dir_all(&indicators)?;
    for (name, spec) in [
        ("hero", hero_chart()?),
        ("regime", regime_chart()?),
        ("composite", composite_chart()?),
    ] {
        fs::write(out.join(format!("{name}.json")), format!("{spec}\n"))?;
    }

    // One chart per indicator, and the list the showcase is built from: name, family,
    // description, unit and default parameters, in the order of the families.
    let long = demo_bars(
        21,
        &[
            (40, 0.25),
            (30, -0.15),
            (35, 0.3),
            (25, -0.4),
            (30, 0.05),
            (40, 0.2),
            (30, -0.3),
            (40, 0.15),
            (30, -0.1),
            (20, 0.2),
        ],
    );
    let catalog = catalog();
    let mut entries = Vec::new();
    for (family, list) in families() {
        for definition in list {
            let entry = catalog
                .iter()
                .find(|e| e.name == definition.name)
                .ok_or("definition without catalog entry")?;
            let chart = indicator_chart(entry.name, entry.description, &long);
            if let Some(spec) = &chart {
                fs::write(
                    indicators.join(format!("{}.json", entry.name)),
                    format!("{spec}\n"),
                )?;
            }
            let mut defaults: Vec<(&String, &f64)> = entry.default_params.iter().collect();
            defaults.sort_by(|a, b| a.0.cmp(b.0));
            let defaults: Vec<String> = defaults
                .iter()
                .map(|(k, v)| format!(r#""{}":{}"#, esc(k), v))
                .collect();
            entries.push(format!(
                r#"{{"name":"{}","family":"{}","description":"{}","unit":"{:?}","chart":{},"defaults":{{{}}}}}"#,
                entry.name,
                esc(family),
                esc(entry.description),
                output_unit(entry.name),
                chart.is_some(),
                defaults.join(",")
            ));
        }
    }
    fs::write(
        out.join("indicators.json"),
        format!("[\n{}\n]\n", entries.join(",\n")),
    )?;
    println!("wrote {} ({} indicators)", out.display(), entries.len());
    Ok(())
}
