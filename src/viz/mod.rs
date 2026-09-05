#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use crate::model::Bar;
use crate::signal::TriggerAction;

/// Renderer-neutral scene model: panes, axes, z-ordered/opacity-tagged objects, and dynamic
/// (identity-keyed) object updates.
pub mod scene;
pub use scene::{Axis, AxisKind, LineStyle, Pane, Scene, SceneObject, SceneObjectKind};

/// SVG renderer for the [`Scene`] model.
mod scene_svg;
pub use scene_svg::{render_scene_svg, Theme};

/// Interactive Render DTO for charting frontend (Web, Canvas, Tauri, Terminal GUI).
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChartRenderData {
    pub title: String,
    pub bars: Vec<ChartBarData>,
    pub series: Vec<ChartSeries>,
    pub zones: Vec<ChartZoneData>,
    pub markers: Vec<ChartMarkerData>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChartBarData {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl From<&Bar> for ChartBarData {
    fn from(b: &Bar) -> Self {
        Self {
            timestamp: b.timestamp,
            open: b.open,
            high: b.high,
            low: b.low,
            close: b.close,
            volume: b.volume,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChartSeries {
    pub name: String,
    pub color: String,
    pub points: Vec<(i64, f64)>,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChartZoneData {
    pub name: String,
    pub price_top: f64,
    pub price_bottom: f64,
    pub color: String,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct ChartMarkerData {
    pub timestamp: i64,
    pub price: f64,
    pub label: String,
    pub action: TriggerAction,
}

pub(crate) fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

/// Accepts a hex literal or a CSS custom-property reference, and nothing else.
///
/// The hex case is the obvious one. The `var(--name)` case exists because a fixed colour cannot
/// serve two themes: a chart exported once has to render in both a light and a dark page, and only
/// the page knows which is current. Emitting a variable reference lets the consumer map the
/// library's colour decisions onto its own token system without the library knowing anything about
/// themes — which is the vendor-neutral way round.
///
/// A fallback (`var(--x, #fff)`) is deliberately not accepted: it would put a second colour
/// decision inside a string this function cannot check, and an unresolvable variable should show
/// up as an obviously wrong chart rather than quietly render in a colour nobody chose.
pub(crate) fn sanitize_color(c: &str) -> String {
    let trimmed = c.trim();
    if is_hex_literal(trimmed) || is_css_variable(trimmed) {
        trimmed.to_string()
    } else {
        "#29b6f6".to_string()
    }
}

fn is_hex_literal(c: &str) -> bool {
    (c.len() == 4 || c.len() == 7)
        && c.starts_with('#')
        && c[1..].chars().all(|ch| ch.is_ascii_hexdigit())
}

/// `var(--name)` with a conservative name charset — letters, digits, hyphen, underscore.
fn is_css_variable(c: &str) -> bool {
    let Some(inner) = c.strip_prefix("var(").and_then(|r| r.strip_suffix(')')) else {
        return false;
    };
    let name = inner.trim();
    name.starts_with("--")
        && name.len() > 2
        && name[2..]
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
}

/// Static SVG Renderer producing clean SVG string charts for CLI previews or reports.
pub fn render_chart_svg(data: &ChartRenderData, width: u32, height: u32) -> String {
    let safe_title = escape_xml(&data.title);

    if data.bars.is_empty() {
        return format!(
            r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}"><text x="10" y="20" fill="red">{}</text></svg>"##,
            width, height, safe_title
        );
    }

    let min_price = data.bars.iter().map(|b| b.low).fold(f64::MAX, f64::min);
    let max_price = data.bars.iter().map(|b| b.high).fold(f64::MIN, f64::max);
    let price_range = (max_price - min_price).max(1e-8);

    let margin_top = 40.0;
    let margin_bottom = 30.0;
    let margin_left = 20.0;
    let margin_right = 60.0;

    let chart_w = width as f64 - margin_left - margin_right;
    let chart_h = height as f64 - margin_top - margin_bottom;

    let to_y = |p: f64| -> f64 { margin_top + (1.0 - (p - min_price) / price_range) * chart_h };

    let n = data.bars.len();
    let bar_w = (chart_w / n as f64).max(1.0);

    // Map timestamps to X coordinates
    let min_ts = data.bars.first().map(|b| b.timestamp).unwrap_or(0);
    let max_ts = data.bars.last().map(|b| b.timestamp).unwrap_or(1);
    let ts_range = (max_ts - min_ts).max(1) as f64;

    let get_x_for_ts = |ts: i64| -> f64 {
        if n == 1 {
            return margin_left + chart_w / 2.0;
        }
        let ratio = (ts - min_ts) as f64 / ts_range;
        margin_left + ratio * chart_w
    };

    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" style="background-color:#141824;font-family:sans-serif;">"##,
        width, height
    ));

    // Title
    svg.push_str(&format!(
        r##"<text x="20" y="25" fill="#e0e6ed" font-size="16" font-weight="bold">{}</text>"##,
        safe_title
    ));

    // Render Zones
    for zone in &data.zones {
        let y_top = to_y(zone.price_top);
        let y_bot = to_y(zone.price_bottom);
        let zone_h = (y_bot - y_top).abs().max(1.0);
        let min_y = y_top.min(y_bot);
        let color = sanitize_color(&zone.color);
        let safe_name = escape_xml(&zone.name);

        svg.push_str(&format!(
            r##"<rect x="{}" y="{}" width="{}" height="{}" fill="{}" opacity="0.2"/>"##,
            margin_left, min_y, chart_w, zone_h, color
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" fill="{}" font-size="10" opacity="0.8">{}</text>"##,
            margin_left + 5.0,
            min_y + 12.0,
            color,
            safe_name
        ));
    }

    // Render Candlesticks
    for (i, b) in data.bars.iter().enumerate() {
        let x = margin_left + i as f64 * bar_w + bar_w / 2.0;
        let y_high = to_y(b.high);
        let y_low = to_y(b.low);
        let y_open = to_y(b.open);
        let y_close = to_y(b.close);

        let is_bull = b.close >= b.open;
        let color = if is_bull { "#26a69a" } else { "#ef5350" };

        // Wick
        svg.push_str(&format!(
            r##"<line x1="{:.1}" y1="{:.1}" x2="{:.1}" y2="{:.1}" stroke="{}" stroke-width="1"/>"##,
            x, y_high, x, y_low, color
        ));

        // Body
        let body_top = y_open.min(y_close);
        let body_h = (y_open - y_close).abs().max(1.0);
        let body_w = (bar_w * 0.7).max(1.0);
        svg.push_str(&format!(
            r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" fill="{}"/>"##,
            x - body_w / 2.0,
            body_top,
            body_w,
            body_h,
            color
        ));
    }

    // Render Series Lines
    for s in &data.series {
        if s.points.is_empty() {
            continue;
        }
        let color = sanitize_color(&s.color);
        let points_str: Vec<String> = s
            .points
            .iter()
            .map(|&(ts, val)| format!("{:.1},{:.1}", get_x_for_ts(ts), to_y(val)))
            .collect();

        svg.push_str(&format!(
            r##"<polyline points="{}" fill="none" stroke="{}" stroke-width="1.5"/>"##,
            points_str.join(" "),
            color
        ));
    }

    // Render Markers positioned by timestamp
    for m in &data.markers {
        let x = get_x_for_ts(m.timestamp);
        let y = to_y(m.price);
        let m_color = match m.action {
            TriggerAction::Buy => "#00e676",
            TriggerAction::Sell => "#ff1744",
            TriggerAction::Exit => "#ff9100",
            TriggerAction::Hold => "#29b6f6",
        };
        let safe_label = escape_xml(&m.label);

        svg.push_str(&format!(
            r##"<circle cx="{:.1}" cy="{:.1}" r="5" fill="{}"/>"##,
            x, y, m_color
        ));
        svg.push_str(&format!(
            r##"<text x="{:.1}" y="{:.1}" fill="{}" font-size="9" font-weight="bold">{}</text>"##,
            x + 7.0,
            y + 3.0,
            m_color,
            safe_label
        ));
    }

    svg.push_str("</svg>");
    svg
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_svg_xml_escaping_and_color_sanitization() {
        let bars = vec![
            ChartBarData {
                timestamp: 1000,
                open: 100.0,
                high: 105.0,
                low: 95.0,
                close: 104.0,
                volume: 1000.0,
            },
            ChartBarData {
                timestamp: 2000,
                open: 104.0,
                high: 108.0,
                low: 102.0,
                close: 107.0,
                volume: 1500.0,
            },
        ];
        let data = ChartRenderData {
            title: "<Chart> & \"Test\"".to_string(),
            bars,
            zones: vec![ChartZoneData {
                name: "Zone <A>".to_string(),
                price_top: 108.0,
                price_bottom: 105.0,
                color: "invalid_color_script".to_string(),
            }],
            series: vec![ChartSeries {
                name: "SMA 20".to_string(),
                points: vec![(1000, 98.0), (2000, 103.0)],
                color: "#00ff00".to_string(),
            }],
            markers: vec![
                ChartMarkerData {
                    timestamp: 1000,
                    price: 100.0,
                    label: "Buy & Hold <NOW>".to_string(),
                    action: TriggerAction::Buy,
                },
                ChartMarkerData {
                    timestamp: 2000,
                    price: 107.0,
                    label: "Exit".to_string(),
                    action: TriggerAction::Exit,
                },
            ],
        };

        let svg = render_chart_svg(&data, 800, 400);

        // Escaped text
        assert!(svg.contains("&lt;Chart&gt; &amp; &quot;Test&quot;"));
        assert!(svg.contains("Zone &lt;A&gt;"));
        assert!(svg.contains("Buy &amp; Hold &lt;NOW&gt;"));

        // Sanitized color (fallback to #29b6f6)
        assert!(svg.contains("#29b6f6"));

        // Series polyline
        assert!(svg.contains("<polyline points="));

        // Markers at the first/last timestamp receive distinct X coordinates.
        assert!(svg.contains("<circle cx=\"20.0\""));
        assert!(svg.contains("<circle cx=\"740.0\""));
    }
}

#[cfg(test)]
mod color_tests {
    use super::sanitize_color;

    #[test]
    fn accepts_hex_literals() {
        assert_eq!(sanitize_color("#abc"), "#abc");
        assert_eq!(sanitize_color("#1a2B3c"), "#1a2B3c");
        assert_eq!(sanitize_color("  #ffffff  "), "#ffffff");
    }

    #[test]
    fn accepts_css_variables() {
        assert_eq!(
            sanitize_color("var(--chart-bullish)"),
            "var(--chart-bullish)"
        );
        assert_eq!(sanitize_color("var(--x_1)"), "var(--x_1)");
    }

    #[test]
    fn rejects_everything_else() {
        // Anything that could carry a second colour decision, an expression, or markup.
        for hostile in [
            "red",
            "rgb(1,2,3)",
            "var(--x, #fff)",
            "var(--x);fill:url(#y)",
            "var(--x)\"onload=\"alert(1)",
            "url(#gradient)",
            "#12345",
            "var(--)",
            "var(x)",
        ] {
            assert_eq!(
                sanitize_color(hostile),
                "#29b6f6",
                "must reject {hostile:?}"
            );
        }
    }
}
