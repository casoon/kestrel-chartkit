//! Renderer-neutral scene model: panes, axes, z-ordered/opacity-tagged objects (polylines with
//! line styles, bounded boxes, area fills, text/tooltips, tables), and identity-keyed dynamic
//! object updates. Pure geometry-and-style data — no SVG/Canvas/WebGL specifics — complementing
//! [`crate::viz::ChartRenderData`]/[`crate::viz::render_chart_svg`]'s single-pane, price-only DTO
//! and its static (non-updatable) SVG string output.

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum LineStyle {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum AxisKind {
    X,
    Y,
}

#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Axis {
    pub kind: AxisKind,
    pub label: String,
    pub min: f64,
    pub max: f64,
}

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
