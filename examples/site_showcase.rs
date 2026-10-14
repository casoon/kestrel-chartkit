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
    aggregate_subscores, build_checked, catalog, classify_regime, score_indicator, Bar, Indicator,
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

/// A calm price chart of a fictitious instrument as a chartlet specification (JSON), the way
/// Kestrel draws its own: candles, two EMAs, the nearest support and resistance zones, the last
/// price, the latest RSI signals, and volume and RSI panes below. Every number is computed by
/// this crate; chartlet only draws (the site renders the spec at build time, light and dark).
fn demo_chart() -> Result<String, Box<dyn Error>> {
    // Up, pullback, up, sell-off, recovery — 4h bars from 5 January 2026, a fictitious price.
    let raw = legs(
        11,
        &[(32, 0.28), (18, -0.22), (26, 0.32), (16, -0.45), (18, 0.18)],
    );
    let average_range = raw.iter().map(|b| b.high - b.low).sum::<f64>() / raw.len() as f64;
    let bars: Vec<Bar> = raw
        .iter()
        .enumerate()
        .map(|(i, b)| {
            // Volume follows the bar's range, so the volume pane has something to say.
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
        .collect();

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
        r#"{{"mark":"ohlc","name":"DEMO","data":[{}]}}"#,
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
        r#"{{"schemaVersion":1,"type":"time","title":"DEMO · fictitious instrument · 4h","description":"Synthetic 4-hour candles of a made-up instrument with EMA 20 and EMA 50, the nearest support and resistance zones from the confirmed pivots, the last price and the latest RSI signals, all computed by kestrel-chartkit. Volume and RSI(14) below.","width":640,"height":520,"timeAxis":{{"gaps":"collapse"}},"panes":[{{"heightRatio":8,"valueAxis":{{"title":"Price","decimals":2}},"layers":[{}]}},{{"heightRatio":2,"valueAxis":{{"title":"Volume"}},"layers":[{{"mark":"area","name":"Volume","stroke":"thin","points":[{}]}}]}},{{"heightRatio":2,"valueAxis":{{"title":"RSI","decimals":0}},"layers":[{},{{"mark":"annotation","value":70,"label":"70"}},{{"mark":"annotation","value":30,"label":"30"}}]}}]}}"#,
        price.join(","),
        volume.join(","),
        line("RSI 14", &rsi_line),
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

    let entries = catalog();
    let names: String = entries.iter().map(|e| format!("{}\n", e.name)).collect();
    fs::write(dir.join("catalog.txt"), names)?;
    println!("catalog: {} indicators", entries.len());
    Ok(())
}
