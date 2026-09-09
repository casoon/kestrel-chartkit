//! Renderer-neutral scene model: panes, axes, z-ordered/opacity-tagged objects (polylines with
//! line styles, bounded boxes, area fills, text/tooltips, tables), and identity-keyed dynamic
//! object updates. Pure geometry-and-style data — no SVG/Canvas/WebGL specifics — complementing
//! [`crate::viz::ChartRenderData`]/[`crate::viz::render_chart_svg`]'s single-pane, price-only DTO
//! and its static (non-updatable) SVG string output.

use crate::artifact::Artifact;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

/// Which of a pane's two axes an [`Axis`] describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AxisKind {
    /// Horizontal axis. **Measured in Unix timestamps, seconds, UTC** — see [`Axis`].
    X,
    /// Vertical axis, measured in the pane's value unit (price for a price pane).
    Y,
}

/// The value range one axis of a pane covers.
///
/// # Coordinate domain
///
/// **The X axis is time: Unix timestamps in seconds, UTC.** Not bar indices, not pixels.
/// Object coordinates are given in the same domain, and a renderer maps them to the screen.
///
/// The reason is that a bar index only exists once a bar set is fixed, and the bar set
/// belongs to the renderer: it culls, it may resample to another timeframe, and it may show
/// two instruments with different trading calendars side by side. In all three cases the same
/// fact would carry different indices, while its timestamp stays what it is.
///
/// A renderer that wants a gap-free display (no empty weekends) maps time to its own bar
/// index — that mapping is a table on its side, whereas the reverse would be a guess about
/// data this crate cannot see. How trading pauses are shown is therefore deliberately not
/// part of this model.
///
/// A pane **without** axes leaves the domain undeclared; [`crate::viz::render_scene_svg`]
/// then treats coordinates as pixels for backwards compatibility. New scenes should declare
/// their axes.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Axis {
    pub kind: AxisKind,
    pub label: String,
    pub min: f64,
    pub max: f64,
}

impl Axis {
    /// An X axis over a Unix-second range (UTC).
    pub fn time(label: impl Into<String>, from_ts: i64, to_ts: i64) -> Self {
        Self {
            kind: AxisKind::X,
            label: label.into(),
            min: from_ts as f64,
            max: to_ts as f64,
        }
    }

    /// A Y axis over a value range — price, ratio, percent, whatever the pane shows.
    pub fn value(label: impl Into<String>, min: f64, max: f64) -> Self {
        Self {
            kind: AxisKind::Y,
            label: label.into(),
            min,
            max,
        }
    }

    /// Width of the range; `0.0` when it is degenerate.
    pub fn span(&self) -> f64 {
        let span = self.max - self.min;
        if span.is_finite() && span > 0.0 {
            span
        } else {
            0.0
        }
    }
}

/// Geometry and style of a scene object.
///
/// Every `color` field follows the contract in [`crate::viz::sanitize_color`]:
/// `#rgb`, `#rrggbb`, `#rrggbbaa` or `var(--name)`. Renderers should pass colors
/// through that function instead of inventing their own parsing.
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SceneObjectKind {
    Polyline {
        points: Vec<(f64, f64)>,
        color: String,
        style: LineStyle,
        width: f64,
    },
    /// A bounded box (e.g. a zone, order block, or pattern annotation).
    BoundedBox {
        x0: f64,
        y0: f64,
        x1: f64,
        y1: f64,
        fill_color: Option<String>,
        border_color: Option<String>,
    },
    /// An area fill between an arbitrary polygon's vertices (e.g. the region between two lines).
    Fill {
        points: Vec<(f64, f64)>,
        color: String,
    },
    Text {
        x: f64,
        y: f64,
        content: String,
        color: String,
    },
    Tooltip {
        x: f64,
        y: f64,
        content: String,
    },
    Table {
        x: f64,
        y: f64,
        rows: Vec<Vec<String>>,
    },
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct SceneObject {
    pub id: String,
    /// Higher draws on top. Ties broken by insertion order.
    pub z_order: i32,
    /// `0.0` (fully transparent) ..= `1.0` (fully opaque).
    pub opacity: f64,
    pub kind: SceneObjectKind,
}

impl SceneObject {
    pub fn new(id: impl Into<String>, z_order: i32, opacity: f64, kind: SceneObjectKind) -> Self {
        Self {
            id: id.into(),
            z_order,
            opacity: opacity.clamp(0.0, 1.0),
            kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Pane {
    pub id: String,
    /// This pane's share of total vertical space, relative to sibling panes (e.g. a price pane
    /// at `3.0` and a volume pane at `1.0` split 75%/25%).
    ///
    /// **A suggestion, not a command.** A renderer that owns the surrounding layout — a
    /// chart with its own indicator panes, say — may lay the scene out differently or draw
    /// it as a single overlay. The ratio then says only how the scene *would* divide space
    /// if it were alone. Consumers should document which of the two they do.
    pub height_ratio: f64,
    pub axes: Vec<Axis>,
    objects: Vec<SceneObject>,
}

impl Pane {
    pub fn new(id: impl Into<String>, height_ratio: f64) -> Self {
        Self {
            id: id.into(),
            height_ratio,
            axes: Vec::new(),
            objects: Vec::new(),
        }
    }

    /// Inserts `object`, or replaces the existing object with the same `id` — the "dynamische
    /// Objekt-Updates" this scene model provides: callers re-`upsert_object` the same ID every
    /// tick instead of clearing and rebuilding the whole pane.
    pub fn upsert_object(&mut self, object: SceneObject) {
        match self.objects.iter_mut().find(|o| o.id == object.id) {
            Some(existing) => *existing = object,
            None => self.objects.push(object),
        }
    }

    pub fn remove_object(&mut self, id: &str) -> bool {
        let before = self.objects.len();
        self.objects.retain(|o| o.id != id);
        self.objects.len() != before
    }

    pub fn objects(&self) -> &[SceneObject] {
        &self.objects
    }

    /// Objects in draw order (ascending `z_order`, ties in insertion order).
    pub fn objects_z_ordered(&self) -> Vec<&SceneObject> {
        let mut ordered: Vec<&SceneObject> = self.objects.iter().collect();
        ordered.sort_by_key(|o| o.z_order);
        ordered
    }
}

/// A renderer-neutral description of what to draw.
///
/// The scene says *what* and *where in data space*, never *how large on screen*: panes
/// carry relative ratios (see [`Pane::height_ratio`]), objects carry data coordinates.
/// Mapping to pixels — and deciding whether the scene owns the layout or overlays an
/// existing one — belongs to the renderer.
///
/// Build one from indicator results with [`scene_from_artifacts`].
#[derive(Debug, Clone, PartialEq, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Scene {
    panes: Vec<Pane>,
}

impl Scene {
    pub fn new() -> Self {
        Self::default()
    }

    /// Inserts `pane`, or replaces the existing pane with the same `id`.
    pub fn upsert_pane(&mut self, pane: Pane) {
        match self.panes.iter_mut().find(|p| p.id == pane.id) {
            Some(existing) => *existing = pane,
            None => self.panes.push(pane),
        }
    }

    pub fn pane_mut(&mut self, id: &str) -> Option<&mut Pane> {
        self.panes.iter_mut().find(|p| p.id == id)
    }

    pub fn pane(&self, id: &str) -> Option<&Pane> {
        self.panes.iter().find(|p| p.id == id)
    }

    pub fn panes(&self) -> &[Pane] {
        &self.panes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_upsert_pane_replaces_by_id() {
        let mut scene = Scene::new();
        scene.upsert_pane(Pane::new("price", 3.0));
        scene.upsert_pane(Pane::new("price", 5.0));
        assert_eq!(scene.panes().len(), 1);
        assert_eq!(scene.pane("price").unwrap().height_ratio, 5.0);
    }

    #[test]
    fn test_upsert_object_updates_in_place() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "sma20",
            1,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(0.0, 100.0)],
                color: "#fff".to_string(),
                style: LineStyle::Solid,
                width: 1.0,
            },
        ));
        pane.upsert_object(SceneObject::new(
            "sma20",
            1,
            1.0,
            SceneObjectKind::Polyline {
                points: vec![(0.0, 100.0), (1.0, 101.0)],
                color: "#fff".to_string(),
                style: LineStyle::Solid,
                width: 1.0,
            },
        ));

        assert_eq!(
            pane.objects().len(),
            1,
            "same id must update, not duplicate"
        );
        match &pane.objects()[0].kind {
            SceneObjectKind::Polyline { points, .. } => assert_eq!(points.len(), 2),
            _ => panic!("expected Polyline"),
        }
    }

    #[test]
    fn test_objects_z_ordered_sorts_ascending() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "top",
            10,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: "top".to_string(),
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
                content: "bottom".to_string(),
                color: "#fff".to_string(),
            },
        ));

        let ordered = pane.objects_z_ordered();
        assert_eq!(ordered[0].id, "bottom");
        assert_eq!(ordered[1].id, "top");
    }

    #[test]
    fn test_opacity_is_clamped() {
        let object = SceneObject::new(
            "x",
            0,
            1.5,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: String::new(),
                color: "#fff".to_string(),
            },
        );
        assert_eq!(object.opacity, 1.0);
    }

    #[test]
    fn test_remove_object() {
        let mut pane = Pane::new("price", 1.0);
        pane.upsert_object(SceneObject::new(
            "x",
            0,
            1.0,
            SceneObjectKind::Text {
                x: 0.0,
                y: 0.0,
                content: String::new(),
                color: "#fff".to_string(),
            },
        ));
        assert!(pane.remove_object("x"));
        assert!(pane.objects().is_empty());
        assert!(
            !pane.remove_object("x"),
            "removing again must be a no-op returning false"
        );
    }
}

/// Builds a [`Scene`] from indicator-emitted [`Artifact`]s.
///
/// Until now the scene model had no producer: every consumer had to invent its own
/// mapping from artifacts to objects, which meant two renderers would disagree about
/// what an order block looks like. This is that mapping, in one place.
///
/// Placement in time comes from the artifact itself ([`crate::artifact::ZoneArtifact::span`],
/// [`crate::artifact::ProfileArtifact::span`], a pivot's `timestamp`). `fallback_span` is used only for
/// artifacts that carry none; artifacts that end up with neither are **skipped** rather
/// than stretched across an invented range.
///
/// Colors follow the contract in [`crate::viz::sanitize_color`].
pub fn scene_from_artifacts(artifacts: &[Artifact], fallback_span: Option<(i64, i64)>) -> Scene {
    const ZONE_FILL: &str = "#58a6ff";
    const PIVOT_HIGH: &str = "#e5534b";
    const PIVOT_LOW: &str = "#3fb950";
    const PROFILE_FILL: &str = "#8b949e";
    const TEXT: &str = "#c9d3df";

    let mut pane = Pane::new("artifacts", 1.0);

    for (index, artifact) in artifacts.iter().enumerate() {
        match artifact {
            Artifact::Pivot(p) => {
                pane.upsert_object(SceneObject::new(
                    format!("pivot-{index}"),
                    30,
                    if p.confirmed { 1.0 } else { 0.5 },
                    SceneObjectKind::Polyline {
                        points: vec![(p.timestamp as f64, p.price), (p.timestamp as f64, p.price)],
                        color: if p.is_high { PIVOT_HIGH } else { PIVOT_LOW }.to_string(),
                        style: LineStyle::Solid,
                        width: 2.0,
                    },
                ));
            }
            Artifact::Zone(z) => {
                let Some((from, to)) = z.span().or(fallback_span) else {
                    continue;
                };
                pane.upsert_object(SceneObject::new(
                    format!("zone-{index}"),
                    10,
                    (0.15 + z.strength.clamp(0.0, 1.0) * 0.35).min(0.5),
                    SceneObjectKind::BoundedBox {
                        x0: from as f64,
                        y0: z.price_top,
                        x1: to as f64,
                        y1: z.price_bottom,
                        fill_color: Some(ZONE_FILL.to_string()),
                        border_color: None,
                    },
                ));
            }
            Artifact::Profile(p) => {
                let Some((from, to)) = p.span().or(fallback_span) else {
                    continue;
                };
                let max_value = p.bins.iter().map(|b| b.value.abs()).fold(0.0_f64, f64::max);
                if max_value <= 0.0 {
                    continue;
                }
                // Bins grow leftwards from the profile's right edge, scaled by value.
                let span = (to - from) as f64;
                for (bin_index, bin) in p.bins.iter().enumerate() {
                    let width = span * 0.25 * (bin.value.abs() / max_value);
                    pane.upsert_object(SceneObject::new(
                        format!("profile-{index}-{bin_index}"),
                        5,
                        0.35,
                        SceneObjectKind::BoundedBox {
                            x0: to as f64 - width,
                            y0: bin.price_high,
                            x1: to as f64,
                            y1: bin.price_low,
                            fill_color: Some(PROFILE_FILL.to_string()),
                            border_color: None,
                        },
                    ));
                }
            }
            Artifact::Scenario(s) => {
                let Some((from, _)) = fallback_span else {
                    continue;
                };
                pane.upsert_object(SceneObject::new(
                    format!("scenario-{index}"),
                    40,
                    if s.invalidated { 0.4 } else { 1.0 },
                    SceneObjectKind::Text {
                        x: from as f64,
                        y: 0.0,
                        content: format!("{} · {} · {:.0}%", s.name, s.stage, s.progress * 100.0),
                        color: TEXT.to_string(),
                    },
                ));
            }
        }
    }

    // Achsen aus den tatsächlich gezeichneten Objekten ableiten, damit die Szene ihre
    // Domäne mitbringt statt sie dem Renderer zu überlassen.
    let mut x_bounds: Option<(f64, f64)> = None;
    let mut y_bounds: Option<(f64, f64)> = None;
    let widen = |bounds: &mut Option<(f64, f64)>, value: f64| {
        if !value.is_finite() {
            return;
        }
        *bounds = Some(match *bounds {
            None => (value, value),
            Some((lo, hi)) => (lo.min(value), hi.max(value)),
        });
    };
    for object in pane.objects() {
        match &object.kind {
            SceneObjectKind::Polyline { points, .. } | SceneObjectKind::Fill { points, .. } => {
                for (x, y) in points {
                    widen(&mut x_bounds, *x);
                    widen(&mut y_bounds, *y);
                }
            }
            SceneObjectKind::BoundedBox { x0, y0, x1, y1, .. } => {
                widen(&mut x_bounds, *x0);
                widen(&mut x_bounds, *x1);
                widen(&mut y_bounds, *y0);
                widen(&mut y_bounds, *y1);
            }
            SceneObjectKind::Text { x, y, .. } | SceneObjectKind::Tooltip { x, y, .. } => {
                widen(&mut x_bounds, *x);
                widen(&mut y_bounds, *y);
            }
            SceneObjectKind::Table { x, y, .. } => {
                widen(&mut x_bounds, *x);
                widen(&mut y_bounds, *y);
            }
        }
    }
    if let (Some((x_lo, x_hi)), Some((y_lo, y_hi))) = (x_bounds, y_bounds) {
        pane.axes = vec![
            Axis::time("time", x_lo as i64, x_hi as i64),
            Axis::value("value", y_lo, y_hi),
        ];
    }

    let mut scene = Scene::new();
    scene.upsert_pane(pane);
    scene
}

#[cfg(test)]
mod artifact_scene_tests {
    use super::*;
    use crate::artifact::{PivotArtifact, ProfileArtifact, ProfileBin, ZoneArtifact};

    fn objects(scene: &Scene) -> usize {
        scene.panes().iter().map(|p| p.objects().len()).sum()
    }

    #[test]
    fn a_zone_uses_its_own_span_over_the_fallback() {
        let zone = ZoneArtifact::new("order_block", 105.0, 100.0).spanning(1_000, 2_000);
        let scene = scene_from_artifacts(&[zone.into()], Some((0, 9_999)));

        let object = &scene.panes()[0].objects()[0];
        match &object.kind {
            SceneObjectKind::BoundedBox { x0, x1, .. } => {
                assert_eq!((*x0, *x1), (1_000.0, 2_000.0));
            }
            other => panic!("unexpected object: {other:?}"),
        }
    }

    #[test]
    fn a_zone_without_a_span_falls_back() {
        let zone = ZoneArtifact::new("order_block", 105.0, 100.0);
        let scene = scene_from_artifacts(&[zone.clone().into()], Some((0, 500)));
        match &scene.panes()[0].objects()[0].kind {
            SceneObjectKind::BoundedBox { x0, x1, .. } => {
                assert_eq!((*x0, *x1), (0.0, 500.0));
            }
            other => panic!("unexpected object: {other:?}"),
        }

        // No span and no fallback: skipped rather than placed at an invented range.
        let scene = scene_from_artifacts(&[zone.into()], None);
        assert_eq!(objects(&scene), 0);
    }

    #[test]
    fn a_pivot_is_placed_at_its_own_timestamp() {
        let pivot = PivotArtifact {
            timestamp: 4_242,
            price: 101.0,
            is_high: true,
            confirmed: true,
        };
        let scene = scene_from_artifacts(&[pivot.into()], None);
        match &scene.panes()[0].objects()[0].kind {
            SceneObjectKind::Polyline { points, .. } => assert_eq!(points[0].0, 4_242.0),
            other => panic!("unexpected object: {other:?}"),
        }
    }

    #[test]
    fn profile_bins_become_boxes_scaled_by_value() {
        let profile = ProfileArtifact {
            kind: "volume_profile".to_string(),
            bins: vec![
                ProfileBin {
                    price_low: 100.0,
                    price_high: 101.0,
                    value: 10.0,
                },
                ProfileBin {
                    price_low: 101.0,
                    price_high: 102.0,
                    value: 5.0,
                },
            ],
            poc: 100.5,
            value_area_high: 102.0,
            value_area_low: 100.0,
            from_ts: None,
            to_ts: None,
        }
        .spanning(0, 1_000);

        let scene = scene_from_artifacts(&[profile.into()], None);
        assert_eq!(objects(&scene), 2);

        let widths: Vec<f64> = scene.panes()[0]
            .objects()
            .iter()
            .filter_map(|o| match &o.kind {
                SceneObjectKind::BoundedBox { x0, x1, .. } => Some(x1 - x0),
                _ => None,
            })
            .collect();
        assert!(
            widths[0] > widths[1],
            "the heavier bin must be wider ({widths:?})"
        );
    }

    #[test]
    fn the_producer_declares_its_axes() {
        let zone = ZoneArtifact::new("order_block", 105.0, 100.0).spanning(1_000, 2_000);
        let scene = scene_from_artifacts(&[zone.into()], None);
        let pane = &scene.panes()[0];

        let x = pane
            .axes
            .iter()
            .find(|a| a.kind == AxisKind::X)
            .expect("x axis declared");
        let y = pane
            .axes
            .iter()
            .find(|a| a.kind == AxisKind::Y)
            .expect("y axis declared");

        assert_eq!((x.min, x.max), (1_000.0, 2_000.0));
        assert_eq!((y.min, y.max), (100.0, 105.0));
    }

    #[test]
    fn an_empty_scene_declares_no_axes() {
        let scene = scene_from_artifacts(&[], None);
        assert!(
            scene.panes()[0].axes.is_empty(),
            "no objects, nothing to bound — better than an invented range"
        );
    }

    #[test]
    fn every_emitted_color_satisfies_the_color_contract() {
        let zone = ZoneArtifact::new("z", 2.0, 1.0).spanning(0, 10);
        let pivot = PivotArtifact {
            timestamp: 5,
            price: 1.5,
            is_high: false,
            confirmed: false,
        };
        let scene = scene_from_artifacts(&[zone.into(), pivot.into()], None);

        for object in scene.panes()[0].objects() {
            let colors: Vec<&str> = match &object.kind {
                SceneObjectKind::Polyline { color, .. } => vec![color],
                SceneObjectKind::BoundedBox {
                    fill_color,
                    border_color,
                    ..
                } => fill_color
                    .iter()
                    .chain(border_color.iter())
                    .map(|c| c.as_str())
                    .collect(),
                SceneObjectKind::Fill { color, .. } => vec![color],
                SceneObjectKind::Text { color, .. } => vec![color],
                _ => Vec::new(),
            };
            for color in colors {
                assert_eq!(
                    crate::viz::sanitize_color(color),
                    color,
                    "emitted color must survive sanitization unchanged"
                );
            }
        }
    }
}
