//! SVG renderer for the [`crate::viz::scene::Scene`] model. Complements
//! [`crate::viz::render_chart_svg`] (single-pane `ChartRenderData` DTO) with a renderer for the
//! generic, multi-pane `Scene` IR — additive, not a replacement.

use super::scene::{AxisKind, LineStyle, Pane, Scene, SceneObject, SceneObjectKind};
use super::{escape_xml, sanitize_color};

const TABLE_ROW_HEIGHT: f64 = 14.0;
const TABLE_COL_WIDTH: f64 = 80.0;
const TOOLTIP_HIT_SIZE: f64 = 10.0;

/// Central style source for [`render_scene_svg`], mirroring the color values otherwise
/// hardcoded in [`crate::viz::render_chart_svg`].
#[derive(Debug, Clone, PartialEq)]
pub struct Theme {
    pub background: String,
    pub text: String,
    pub candle_bull: String,
    pub candle_bear: String,
    pub marker_buy: String,
    pub marker_sell: String,
    pub marker_exit: String,
    pub marker_hold: String,
    pub fallback: String,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            background: "#141824".to_string(),
            text: "#e0e6ed".to_string(),
            candle_bull: "#26a69a".to_string(),
            candle_bear: "#ef5350".to_string(),
            marker_buy: "#00e676".to_string(),
            marker_sell: "#ff1744".to_string(),
            marker_exit: "#ff9100".to_string(),
            marker_hold: "#29b6f6".to_string(),
            fallback: "#29b6f6".to_string(),
        }
    }
}

/// Maps a pane's data coordinates to pixels inside its own rectangle.
///
/// Built from the pane's axes (see [`crate::viz::scene::Axis`]: x is time in Unix seconds).
/// A pane that declares no usable axes gets [`Projection::identity`], which passes
/// coordinates through as pixels — the behaviour of scenes written before the domain was
/// declared.
#[derive(Debug, Clone, Copy)]
struct Projection {
    x_min: f64,
    x_span: f64,
    y_min: f64,
    y_span: f64,
    width: f64,
    height: f64,
}

impl Projection {
    fn identity() -> Self {
        Self {
            x_min: 0.0,
            x_span: 0.0,
            y_min: 0.0,
            y_span: 0.0,
            width: 0.0,
            height: 0.0,
        }
    }

    fn for_pane(pane: &Pane, width: f64, height: f64) -> Self {
        let axis = |kind: AxisKind| pane.axes.iter().find(|a| a.kind == kind);
        match (axis(AxisKind::X), axis(AxisKind::Y)) {
            (Some(x), Some(y)) if x.span() > 0.0 && y.span() > 0.0 => Self {
                x_min: x.min,
                x_span: x.span(),
                y_min: y.min,
                y_span: y.span(),
                width,
                height,
            },
            _ => Self::identity(),
        }
    }

    fn is_identity(&self) -> bool {
        self.x_span == 0.0 || self.y_span == 0.0
    }

    fn x(&self, value: f64) -> f64 {
        if self.is_identity() {
            return value;
        }
        (value - self.x_min) / self.x_span * self.width
    }

    /// Y is inverted: a higher value sits higher on screen, i.e. at a smaller pixel row.
    fn y(&self, value: f64) -> f64 {
        if self.is_identity() {
            return value;
        }
        self.height - (value - self.y_min) / self.y_span * self.height
    }
}

/// Renders `scene` as a self-contained SVG string. Panes are stacked vertically, each taking a
/// share of `height` proportional to its `height_ratio` among sibling panes; within a pane,
/// objects are emitted in `objects_z_ordered()` order.
///
/// Object coordinates are mapped through the pane's axes (x = Unix seconds, y = value). Panes
/// without axes are drawn in pixel coordinates, as before.
pub fn render_scene_svg(scene: &Scene, width: u32, height: u32, theme: &Theme) -> String {
    let mut svg = String::new();
    svg.push_str(&format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" style="background-color:{};font-family:sans-serif;">"##,
        width,
        height,
        sanitize_color(&theme.background)
    ));

    let total_ratio: f64 = scene.panes().iter().map(|p| p.height_ratio).sum();
    let mut y_offset = 0.0;
    for pane in scene.panes() {
        let pane_height = if total_ratio > 0.0 {
            height as f64 * pane.height_ratio / total_ratio
        } else {
            0.0
        };

        svg.push_str(&format!(
            r##"<g transform="translate(0,{:.1})">"##,
            y_offset
        ));
        let projection = Projection::for_pane(pane, width as f64, pane_height);
        for object in pane.objects_z_ordered() {
            svg.push_str(&render_object(object, theme, &projection));
        }
        svg.push_str("</g>");

        y_offset += pane_height;
    }

    svg.push_str("</svg>");
    svg
}

fn render_object(object: &SceneObject, theme: &Theme, p: &Projection) -> String {
    let opacity = object.opacity;
    match &object.kind {
        SceneObjectKind::Polyline {
            points,
            color,
            style,
            width,
        } => {
            let points_str: Vec<String> = points
                .iter()
                .map(|&(x, y)| format!("{:.1},{:.1}", p.x(x), p.y(y)))
                .collect();
            let dash = match style {
                LineStyle::Solid => "",
                LineStyle::Dashed => r##" stroke-dasharray="6,4""##,
                LineStyle::Dotted => r##" stroke-dasharray="2,2""##,
            };
            format!(
                r##"<polyline points="{}" fill="none" stroke="{}" stroke-width="{}" opacity="{}"{}/>"##,
                points_str.join(" "),
                sanitize_color(color),
                width,
                opacity,
                dash
            )
        }
        SceneObjectKind::BoundedBox {
            x0,
            y0,
            x1,
            y1,
            fill_color,
            border_color,
        } => {
            let (px0, px1) = (p.x(*x0), p.x(*x1));
            let (py0, py1) = (p.y(*y0), p.y(*y1));
            let x = px0.min(px1);
            let y = py0.min(py1);
            let w = (px1 - px0).abs();
            let h = (py1 - py0).abs();
            let mut attrs = String::new();
            if let Some(fc) = fill_color {
                attrs.push_str(&format!(r##" fill="{}""##, sanitize_color(fc)));
            }
            if let Some(bc) = border_color {
                attrs.push_str(&format!(r##" stroke="{}""##, sanitize_color(bc)));
            }
            format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{:.1}" height="{:.1}" opacity="{}"{}/>"##,
                x, y, w, h, opacity, attrs
            )
        }
        SceneObjectKind::Fill { points, color } => {
            let points_str: Vec<String> = points
                .iter()
                .map(|&(x, y)| format!("{:.1},{:.1}", p.x(x), p.y(y)))
                .collect();
            format!(
                r##"<polygon points="{}" fill="{}" opacity="{}"/>"##,
                points_str.join(" "),
                sanitize_color(color),
                opacity
            )
        }
        SceneObjectKind::Text {
            x,
            y,
            content,
            color,
        } => {
            format!(
                r##"<text x="{:.1}" y="{:.1}" fill="{}" opacity="{}">{}</text>"##,
                p.x(*x),
                p.y(*y),
                sanitize_color(color),
                opacity,
                escape_xml(content)
            )
        }
        SceneObjectKind::Tooltip { x, y, content } => {
            format!(
                r##"<rect x="{:.1}" y="{:.1}" width="{}" height="{}" fill="transparent" opacity="{}"><title>{}</title></rect>"##,
                p.x(*x),
                p.y(*y),
                TOOLTIP_HIT_SIZE,
                TOOLTIP_HIT_SIZE,
                opacity,
                escape_xml(content)
            )
        }
        SceneObjectKind::Table { x, y, rows } => {
            // Der Ankerpunkt liegt in Datenkoordinaten, Zeilenhöhe und Spaltenbreite
            // sind Pixelmaße: eine Tabelle ist ein Bedienelement, keine Geometrie.
            let anchor_x = p.x(*x);
            let anchor_y = p.y(*y);
            let mut s = String::new();
            for (row_idx, row) in rows.iter().enumerate() {
                let row_y = anchor_y + row_idx as f64 * TABLE_ROW_HEIGHT;
                for (col_idx, cell) in row.iter().enumerate() {
                    let cell_x = anchor_x + col_idx as f64 * TABLE_COL_WIDTH;
                    s.push_str(&format!(
                        r##"<text x="{:.1}" y="{:.1}" fill="{}" text-anchor="start" opacity="{}">{}</text>"##,
                        cell_x,
                        row_y,
                        sanitize_color(&theme.text),
                        opacity,
                        escape_xml(cell)
                    ));
                }
            }
            s
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::viz::scene::Pane;

    #[test]
    fn test_empty_scene_renders_valid_empty_svg() {
        let scene = Scene::new();
        let svg = render_scene_svg(&scene, 800, 400, &Theme::default());
        assert!(svg
            .starts_with(r##"<svg xmlns="http://www.w3.org/2000/svg" width="800" height="400""##));
        assert!(svg.ends_with("</svg>"));
        assert!(!svg.contains("<g "));
    }

    /// A pane that declares axes gets its coordinates mapped: the time range spans the
    /// full width, and the value range fills the height with y inverted.
    #[test]
    fn test_declared_axes_map_data_coordinates_to_pixels() {
        use super::super::scene::Axis;

        let mut pane = Pane::new("price", 1.0);
        pane.axes = vec![
            Axis::time("time", 1_000, 2_000),
            Axis::value("price", 100.0, 200.0),
        ];
        pane.upsert_object(SceneObject::new(
            "line",
            0,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(1_000.0, 100.0), (2_000.0, 200.0)],
                color: "#ffffff".to_string(),
                style: LineStyle::Solid,
                width: 1.0,
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 400, 200, &Theme::default());

        // Start: left edge, bottom (lowest price). End: right edge, top (highest price).
        assert!(
            svg.contains(r##"points="0.0,200.0 400.0,0.0""##),
            "unexpected mapping: {svg}"
        );
    }

    /// A pane without axes keeps the old behaviour: coordinates are pixels.
    #[test]
    fn test_a_pane_without_axes_passes_coordinates_through() {
        let mut pane = Pane::new("legacy", 1.0);
        pane.upsert_object(SceneObject::new(
            "line",
            0,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(10.0, 20.0), (30.0, 40.0)],
                color: "#ffffff".to_string(),
                style: LineStyle::Solid,
                width: 1.0,
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 400, 200, &Theme::default());
        assert!(svg.contains(r##"points="10.0,20.0 30.0,40.0""##));
    }

    /// A degenerate range must not divide by zero — it falls back to pass-through.
    #[test]
    fn test_a_degenerate_axis_range_falls_back_to_pixels() {
        use super::super::scene::Axis;

        let mut pane = Pane::new("flat", 1.0);
        pane.axes = vec![
            Axis::time("time", 1_000, 1_000),
            Axis::value("price", 100.0, 100.0),
        ];
        pane.upsert_object(SceneObject::new(
            "line",
            0,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(10.0, 20.0), (30.0, 40.0)],
                color: "#ffffff".to_string(),
                style: LineStyle::Solid,
                width: 1.0,
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 400, 200, &Theme::default());
        assert!(svg.contains(r##"points="10.0,20.0 30.0,40.0""##));
    }

    #[test]
    fn test_xml_escaping_in_text() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "t",
            0,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "<A> & \"B\"".to_string(),
                color: "#fff".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(svg.contains("&lt;A&gt; &amp; &quot;B&quot;"));
    }

    #[test]
    fn test_color_sanitization_fallback() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "t",
            0,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "x".to_string(),
                color: "not-a-color".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(svg.contains("#29b6f6"));
    }

    #[test]
    fn test_z_order_reflected_in_output_order() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "top",
            10,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "TOPMARK".to_string(),
                color: "#fff".to_string(),
            },
        ));
        pane.upsert_object(SceneObject::new(
            "bottom",
            -5,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "BOTTOMMARK".to_string(),
                color: "#fff".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        let bottom_pos = svg.find("BOTTOMMARK").unwrap();
        let top_pos = svg.find("TOPMARK").unwrap();
        assert!(bottom_pos < top_pos, "lower z_order must render first");
    }

    #[test]
    fn test_opacity_attribute_present() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "t",
            0,
            0.5,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "x".to_string(),
                color: "#fff".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(svg.contains(r##"opacity="0.5""##));
    }

    #[test]
    fn test_dashed_and_dotted_lines_produce_stroke_dasharray() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "dashed",
            0,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                color: "#fff".to_string(),
                style: LineStyle::Dashed,
                width: 1.0,
            },
        ));
        pane.upsert_object(SceneObject::new(
            "dotted",
            1,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(0.0, 0.0), (1.0, 1.0)],
                color: "#fff".to_string(),
                style: LineStyle::Dotted,
                width: 1.0,
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(svg.contains(r##"stroke-dasharray="6,4""##));
        assert!(svg.contains(r##"stroke-dasharray="2,2""##));
    }

    #[test]
    fn test_bounded_box_omits_unset_fill_and_stroke() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "box",
            0,
            1.0,
            SceneObjectKind::BoundedBox {
                x0: 0.0,
                y0: 0.0,
                x1: 10.0,
                y1: 10.0,
                fill_color: None,
                border_color: None,
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(!svg.contains("fill="));
        assert!(!svg.contains("stroke="));
    }

    #[test]
    fn test_table_grid_layout() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "table",
            0,
            1.0,
            SceneObjectKind::Table {
                x: 0.0,
                y: 0.0,
                rows: vec![
                    vec!["a".to_string(), "b".to_string()],
                    vec!["c".to_string(), "d".to_string()],
                ],
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert_eq!(svg.matches("<text").count(), 4);
        assert!(svg.contains(r##"x="0.0" y="0.0""##));
        assert!(svg.contains(&format!(r##"x="{:.1}" y="0.0""##, TABLE_COL_WIDTH)));
        assert!(svg.contains(&format!(r##"x="0.0" y="{:.1}""##, TABLE_ROW_HEIGHT)));
    }

    #[test]
    fn test_tooltip_renders_nested_title() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "tip",
            0,
            1.0,
            SceneObjectKind::Tooltip {
                x: 5.0,
                y: 5.0,
                content: "hover <me>".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(pane);

        let svg = render_scene_svg(&scene, 100, 100, &Theme::default());
        assert!(svg.contains("<title>hover &lt;me&gt;</title>"));
        assert!(svg.contains("fill=\"transparent\""));
    }

    #[test]
    fn test_multi_pane_height_split_by_ratio() {
        let mut price = Pane::new("price", 3.0);
        price.upsert_object(SceneObject::new(
            "p",
            0,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "P".to_string(),
                color: "#fff".to_string(),
            },
        ));
        let mut volume = Pane::new("volume", 1.0);
        volume.upsert_object(SceneObject::new(
            "v",
            0,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "V".to_string(),
                color: "#fff".to_string(),
            },
        ));
        let mut scene = Scene::new();
        scene.upsert_pane(price);
        scene.upsert_pane(volume);

        let svg = render_scene_svg(&scene, 100, 400, &Theme::default());
        assert!(svg.contains(r##"<g transform="translate(0,0.0)">"##));
        assert!(svg.contains(r##"<g transform="translate(0,300.0)">"##));
    }
}
