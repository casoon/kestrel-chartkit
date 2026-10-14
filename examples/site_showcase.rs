//! Renders the SVG charts shown on the project website (`site/`) with this crate's own
//! indicators and SVG renderers. The bars are synthetic (`kestrel_chartkit::synthetic`, fixed
//! seeds), so every run writes the same files.
//!
//! Regenerate after changing an indicator or a renderer:
//!
//! ```bash
//! cargo run --example site_showcase
//! ```
//!
//! Writes `examples/site/*.svg`, `examples/site/catalog.txt` and `examples/site/demo-chart.json`
//! — the last one a chartlet specification of a calm price chart (see `demo_chart`), drawn by
//! the site at build time.

use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use kestrel_chartkit::model::{MarketRegime, ZoneKind};
use kestrel_chartkit::signal::TriggerAction;
use kestrel_chartkit::structure::find_sr_zones;
use kestrel_chartkit::synthetic::{random_walk_bars, trending_bars};
use kestrel_chartkit::viz::{
    render_chart_svg, render_scene_svg, Axis, ChartBarData, ChartMarkerData, ChartRenderData,
    ChartSeries, ChartZoneData, LineStyle, Pane, Scene, SceneObject, SceneObjectKind, Theme,
};
use kestrel_chartkit::{
    aggregate_subscores, build_checked, catalog, classify_regime, output_unit, score_indicator,
    Bar, Indicator, IndicatorUnit,
};

type Chart = Result<String, Box<dyn Error>>;

fn params(pairs: &[(&str, f64)]) -> HashMap<String, f64> {
    pairs.iter().map(|&(k, v)| (k.to_string(), v)).collect()
}

/// A random walk around 100, one bar per minute.
fn walk(seed: u64, count: usize) -> Vec<Bar> {
    random_walk_bars(seed, count, 100.0, 0.0, 0.8, 1_000.0)
        .into_iter()
        .map(|q| q.bar)
        .collect()
}

/// Consecutive trending legs `(bars, drift per bar)`, joined into one continuous series.
fn legs(seed: u64, legs: &[(usize, f64)]) -> Vec<Bar> {
    let mut bars: Vec<Bar> = Vec::new();
    let mut price = 100.0;
    for (i, &(count, drift)) in legs.iter().enumerate() {
        for q in trending_bars(seed + i as u64, count, price, drift, 0.6, 1_000.0) {
            let b = q.bar;
            let timestamp = bars.len() as i64 * 60;
            price = b.close;
            bars.push(Bar::new(
                timestamp, b.open, b.high, b.low, b.close, b.volume,
            ));
        }
    }
    bars
}

/// Up, sideways, down, sideways.
fn market_legs() -> Vec<Bar> {
    legs(3, &[(60, 0.35), (50, 0.0), (60, -0.4), (40, 0.0)])
}

fn line(name: &str, color: &str, points: Vec<(i64, f64)>) -> ChartSeries {
    ChartSeries {
        name: name.to_string(),
        color: color.to_string(),
        points,
    }
}

fn candles(bars: &[Bar]) -> Vec<ChartBarData> {
    bars.iter().map(ChartBarData::from).collect()
}

// showcase:start bollinger-bands
fn bollinger_bands() -> Chart {
    let bars = walk(7, 120);
    let mut bb = build_checked("bollinger", &params(&[("len", 20.0), ("mult", 2.0)]))?;

    let (mut upper, mut basis, mut lower) = (Vec::new(), Vec::new(), Vec::new());
    for bar in &bars {
        if let Some(out) = bb.on_bar(bar) {
            upper.push((bar.timestamp, out.extra["upper"]));
            basis.push((bar.timestamp, out.extra["basis"]));
            lower.push((bar.timestamp, out.extra["lower"]));
        }
    }

    let data = ChartRenderData {
        title: "Bollinger Bands (20, 2)".to_string(),
        bars: candles(&bars),
        series: vec![
            line("Upper band", "#90caf9", upper),
            line("Basis", "#ffb74d", basis),
            line("Lower band", "#90caf9", lower),
        ],
        zones: Vec::new(),
        markers: Vec::new(),
    };
    Ok(render_chart_svg(&data, 800, 400))
}
// showcase:end

// showcase:start supertrend
fn supertrend() -> Chart {
    let bars = market_legs();
    let mut st = build_checked(
        "supertrend",
        &params(&[("period", 10.0), ("multiplier", 3.0)]),
    )?;

    // One line segment per trend run, a marker wherever the trend flips.
    let mut segments: Vec<ChartSeries> = Vec::new();
    let mut markers = Vec::new();
    let mut prev_trend = 0.0;
    for bar in &bars {
        let Some(out) = st.on_bar(bar) else { continue };
        let trend = out.extra["trend"];
        if trend != prev_trend {
            let (name, color, label, action) = if trend > 0.0 {
                ("Supertrend long", "#26a69a", "flip up", TriggerAction::Buy)
            } else {
                (
                    "Supertrend short",
                    "#ef5350",
                    "flip down",
                    TriggerAction::Sell,
                )
            };
            if prev_trend != 0.0 {
                markers.push(ChartMarkerData {
                    timestamp: bar.timestamp,
                    price: bar.close,
                    label: label.to_string(),
                    action,
                });
            }
            segments.push(line(name, color, Vec::new()));
            prev_trend = trend;
        }
        if let Some(segment) = segments.last_mut() {
            segment.points.push((bar.timestamp, out.value));
        }
    }

    let data = ChartRenderData {
        title: "Supertrend (10, 3)".to_string(),
        bars: candles(&bars),
        series: segments,
        zones: Vec::new(),
        markers,
    };
    Ok(render_chart_svg(&data, 800, 400))
}
// showcase:end

// showcase:start volume-profile
fn volume_profile() -> Chart {
    let bars = walk(23, 120);
    let lookback = 70;
    let mut vp = build_checked(
        "volume_profile",
        &params(&[("lookback", lookback as f64), ("num_bins", 30.0)]),
    )?;

    let mut last = None;
    for bar in &bars {
        if let Some(out) = vp.on_bar(bar) {
            last = Some(out);
        }
    }
    let out = last.ok_or("volume profile produced no output")?;
    let (poc, vah, val) = (out.extra["vpoc"], out.extra["vah"], out.extra["val"]);

    let from = bars[bars.len() - lookback].timestamp;
    let to = bars[bars.len() - 1].timestamp;
    let data = ChartRenderData {
        title: "Volume profile, last 70 bars".to_string(),
        bars: candles(&bars),
        series: vec![line(
            "Point of control",
            "#ffb74d",
            vec![(from, poc), (to, poc)],
        )],
        zones: vec![ChartZoneData {
            name: format!("Value area {val:.2} to {vah:.2}"),
            price_top: vah,
            price_bottom: val,
            color: "#29b6f6".to_string(),
        }],
        markers: Vec::new(),
    };
    Ok(render_chart_svg(&data, 800, 400))
}
// showcase:end

// showcase:start market-regime
fn regime_color(regime: MarketRegime) -> &'static str {
    match regime {
        MarketRegime::BullishExpansion => "#26a69a",
        MarketRegime::BearishExpansion => "#ef5350",
        MarketRegime::Consolidation => "#78909c",
        MarketRegime::Transition => "#ffb74d",
    }
}

fn market_regime(width: u32, height: u32) -> Chart {
    let bars = market_legs();
    let mut adx = build_checked("adx", &HashMap::new())?;
    let mut atr = build_checked("atr", &HashMap::new())?;

    let mut regimes = Vec::new();
    let mut adx_line = Vec::new();
    for (i, bar) in bars.iter().enumerate() {
        let (Some(a), Some(t)) = (adx.on_bar(bar), atr.on_bar(bar)) else {
            continue;
        };
        // The ATR indicator reports 100 * ATR / close; classify_regime expects the fraction.
        let regime = classify_regime(&bars[..=i], a.value, t.value / 100.0);
        regimes.push((bar.timestamp, regime));
        adx_line.push((bar.timestamp as f64, a.value));
    }

    let (first, last) = (bars[0].timestamp, bars[bars.len() - 1].timestamp);
    let min = bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let max = bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    // Headroom above the highest high for the title.
    let (low, high) = (min - 1.0, max + (max - min) * 0.2);

    // Price pane: one shaded box per run of equal regimes, the close on top.
    let mut price = Pane::new("price", 3.0);
    price.axes = vec![
        Axis::time("time", first, last),
        Axis::value("price", low, high),
    ];
    let mut start = 0;
    for i in 1..=regimes.len() {
        if i == regimes.len() || regimes[i].1 != regimes[start].1 {
            let x1 = regimes.get(i).map_or(last, |r| r.0);
            price.upsert_object(SceneObject::new(
                format!("regime-{start}"),
                0,
                0.3,
                SceneObjectKind::BoundedBox {
                    x0: regimes[start].0 as f64,
                    y0: low,
                    x1: x1 as f64,
                    y1: high,
                    fill_color: Some(regime_color(regimes[start].1).to_string()),
                    border_color: None,
                },
            ));
            start = i;
        }
    }
    price.upsert_object(SceneObject::new(
        "close",
        1,
        1.0,
        SceneObjectKind::Polyline {
            points: bars.iter().map(|b| (b.timestamp as f64, b.close)).collect(),
            color: "#e0e6ed".to_string(),
            style: LineStyle::Solid,
            width: 1.5,
        },
    ));
    price.upsert_object(SceneObject::new(
        "title",
        2,
        1.0,
        SceneObjectKind::Text {
            x: first as f64 + 120.0,
            y: max + (max - min) * 0.07,
            content: "Market regime per bar".to_string(),
            color: "#e0e6ed".to_string(),
        },
    ));

    // ADX pane with the trend threshold classify_regime uses.
    let adx_max = adx_line.iter().map(|p| p.1).fold(0.0, f64::max) * 1.15;
    let mut trend = Pane::new("adx", 1.0);
    trend.axes = vec![
        Axis::time("time", first, last),
        Axis::value("adx", 0.0, adx_max),
    ];
    trend.upsert_object(SceneObject::new(
        "adx",
        1,
        1.0,
        SceneObjectKind::Polyline {
            points: adx_line,
            color: "#90caf9".to_string(),
            style: LineStyle::Solid,
            width: 1.5,
        },
    ));
    trend.upsert_object(SceneObject::new(
        "threshold",
        0,
        1.0,
        SceneObjectKind::Polyline {
            points: vec![(first as f64, 20.0), (last as f64, 20.0)],
            color: "#ffb74d".to_string(),
            style: LineStyle::Dashed,
            width: 1.0,
        },
    ));

    let mut scene = Scene::new();
    scene.upsert_pane(price);
    scene.upsert_pane(trend);
    Ok(render_scene_svg(&scene, width, height, &Theme::default()))
}
// showcase:end

// showcase:start composite-signal
fn composite_signal() -> Chart {
    let bars = market_legs();
    let names = ["macd", "vortex", "trend_quality"];
    let mut indicators = names
        .iter()
        .map(|name| build_checked(name, &HashMap::new()))
        .collect::<Result<Vec<_>, _>>()?;
    let mut adx = build_checked("adx", &HashMap::new())?;
    let mut atr = build_checked("atr", &HashMap::new())?;

    let mut markers = Vec::new();
    let mut prev = TriggerAction::Hold;
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
        // Mark the bar where a trigger starts, not every bar it persists.
        if signal.trigger != prev && signal.trigger != TriggerAction::Hold {
            markers.push(ChartMarkerData {
                timestamp: bar.timestamp,
                price: bar.close,
                label: format!("{:?} {:+.2}", signal.trigger, signal.score),
                action: signal.trigger,
            });
        }
        prev = signal.trigger;
    }

    let data = ChartRenderData {
        title: "Composite signal triggers".to_string(),
        bars: candles(&bars),
        series: Vec::new(),
        zones: Vec::new(),
        markers,
    };
    Ok(render_chart_svg(&data, 800, 400))
}
// showcase:end

/// A calm price chart of a fictitious CASOON share as a chartlet specification (JSON), the way
/// Kestrel draws its own: candles, two EMAs, the nearest support and resistance zones, the last
/// price, the latest RSI signals, and volume and RSI panes below. Every number is computed by
/// this crate; chartlet only draws (the site renders the spec at build time, light and dark).
/// Trending legs as 4-hour bars of a fictitious CASOON share (invented prices, no real quotes) from 5 January 2026. The volume
/// follows each bar's range, so a volume pane has something to say.
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

fn demo_chart() -> Result<String, Box<dyn Error>> {
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
        r#"{{"schemaVersion":1,"type":"time","title":"CASOON share (fictitious prices) · 4h","description":"Synthetic 4-hour candles of a fictitious CASOON share — invented prices, no real quotes — with EMA 20 and EMA 50, the nearest support and resistance zones from the confirmed pivots, the last price and the latest RSI signals, all computed by kestrel-chartkit. Volume and RSI(14) below.","width":640,"height":520,"timeAxis":{{"gaps":"collapse"}},"panes":[{{"heightRatio":8,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}},{{"heightRatio":2,"valueAxis":{{"title":"Volume"}},"layers":[{{"mark":"area","name":"Volume","stroke":"thin","points":[{}]}}]}},{{"heightRatio":2,"valueAxis":{{"title":"RSI","decimals":0}},"layers":[{},{{"mark":"annotation","value":70,"label":"70"}},{{"mark":"annotation","value":30,"label":"30"}}]}}]}}"#,
        price.join(","),
        volume.join(","),
        line("RSI 14", &rsi_line),
    ))
}

/// Bars shown per indicator chart; the indicators run over the whole demo series first, so long
/// warmups are complete before the window starts.
const INDICATOR_WINDOW: usize = 150;

/// One chartlet specification per catalog indicator (default parameters, the same fictitious
/// 4h series): candles with the indicator over them when its output is a price, below them in a
/// pane of its own otherwise; `secondary`, `signal` and price-level extras as further lines; the
/// latest alerts as markers. `None` when the indicator produces too few values in the window.
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
    let described: String = first.replace('"', "'").chars().take(400).collect();
    Some(format!(
        r#"{{"schemaVersion":1,"type":"time","title":"{name} · CASOON (fictitious) 4h","description":"{described}. Default parameters on synthetic 4-hour bars of a fictitious CASOON share (invented prices), the last {INDICATOR_WINDOW} bars; {} alerts in view, the latest as markers.","width":640,"height":{},"timeAxis":{{"gaps":"collapse"}},"panes":[{}]}}"#,
        alerts.len(),
        if own.is_empty() { 320 } else { 420 },
        panes.join(",")
    ))
}

fn main() -> Result<(), Box<dyn Error>> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples/site");
    fs::create_dir_all(&dir)?;

    let charts = [
        ("bollinger-bands", bollinger_bands()?),
        ("supertrend", supertrend()?),
        ("volume-profile", volume_profile()?),
        ("market-regime", market_regime(800, 440)?),
        ("composite-signal", composite_signal()?),
    ];
    for (name, svg) in &charts {
        let path = dir.join(format!("{name}.svg"));
        fs::write(&path, format!("{svg}\n"))?;
        println!("wrote {}", path.display());
    }

    let spec = dir.join("demo-chart.json");
    fs::write(&spec, format!("{}\n", demo_chart()?))?;
    println!("wrote {}", spec.display());

    // Per-indicator charts for the indicator reference: generated, not committed
    // (site/.generated/ is ignored; the Pages workflow runs this example before the site build).
    let generated = Path::new(env!("CARGO_MANIFEST_DIR")).join("site/.generated/indicators");
    fs::create_dir_all(&generated)?;
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
    let mut charted = 0;
    for entry in catalog() {
        if let Some(spec) = indicator_chart(entry.name, entry.description, &long) {
            fs::write(
                generated.join(format!("{}.json", entry.name)),
                format!("{spec}\n"),
            )?;
            charted += 1;
        }
    }
    println!("indicator charts: {charted} of {}", catalog().len());

    let entries = catalog();
    let names: String = entries.iter().map(|e| format!("{}\n", e.name)).collect();
    fs::write(dir.join("catalog.txt"), names)?;
    println!("catalog: {} indicators", entries.len());
    Ok(())
}
