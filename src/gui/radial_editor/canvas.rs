//! Pure canvas state for the compact Radial Designer.
//!
//! The renderer owns the egui paint objects, while this module owns the
//! coordinate and authoring invariants.  Keeping the transform and projected
//! identity independent of egui makes hit testing deterministic at arbitrary
//! zoom, pan, and DPI values and prevents dynamic preview rows from becoming
//! persisted document entities.

use crate::radial::authoring::DraftGeneration;
use crate::radial::dynamic::SourceFingerprint;
use crate::radial::model::{CellId, DynamicSource, MenuId, RingId};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum DesignerMode {
    #[default]
    Design,
    PreviewTest,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct CanvasPoint {
    pub(crate) x: f32,
    pub(crate) y: f32,
}

impl CanvasPoint {
    pub(crate) const fn new(x: f32, y: f32) -> Self {
        Self { x, y }
    }
}

/// The best work-area report available to the egui layer.  The origin is not
/// assumed to be `(0, 0)`: Windows topologies commonly place a monitor to the
/// left or above the primary display.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowWorkArea {
    pub(crate) origin: CanvasPoint,
    pub(crate) size: CanvasPoint,
}

impl WindowWorkArea {
    pub(crate) fn normalized(self) -> Self {
        Self {
            origin: CanvasPoint::new(
                if self.origin.x.is_finite() {
                    self.origin.x
                } else {
                    0.0
                },
                if self.origin.y.is_finite() {
                    self.origin.y
                } else {
                    0.0
                },
            ),
            size: CanvasPoint::new(
                if self.size.x.is_finite() && self.size.x > 0.0 {
                    self.size.x
                } else {
                    1.0
                },
                if self.size.y.is_finite() && self.size.y > 0.0 {
                    self.size.y
                } else {
                    1.0
                },
            ),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct WindowGeometry {
    pub(crate) position: Option<CanvasPoint>,
    pub(crate) size: CanvasPoint,
}

/// Normalize restored Designer geometry against the currently reported work
/// area.  Clamping uses the work-area origin and therefore remains correct
/// when monitor topology changes or the usable display is negative.
pub(crate) fn clamp_window_geometry(
    geometry: WindowGeometry,
    work_area: WindowWorkArea,
) -> WindowGeometry {
    let work_area = work_area.normalized();
    let size = CanvasPoint::new(
        if geometry.size.x.is_finite() && geometry.size.x > 0.0 {
            geometry.size.x.min(work_area.size.x)
        } else {
            work_area.size.x.min(900.0).max(1.0)
        },
        if geometry.size.y.is_finite() && geometry.size.y > 0.0 {
            geometry.size.y.min(work_area.size.y)
        } else {
            work_area.size.y.min(650.0).max(1.0)
        },
    );
    let position = geometry.position.map(|position| {
        let max_x = work_area.origin.x + (work_area.size.x - size.x).max(0.0);
        let max_y = work_area.origin.y + (work_area.size.y - size.y).max(0.0);
        CanvasPoint::new(
            if position.x.is_finite() {
                position.x.clamp(work_area.origin.x, max_x)
            } else {
                work_area.origin.x
            },
            if position.y.is_finite() {
                position.y.clamp(work_area.origin.y, max_y)
            } else {
                work_area.origin.y
            },
        )
    });
    WindowGeometry { position, size }
}

/// Clamp only the restored size when the platform cannot report a desktop
/// work-area origin.  The saved position is deliberately discarded: without
/// a trustworthy desktop origin, asking the OS to place the viewport is safer
/// than restoring an off-screen coordinate or treating an egui-local `(0, 0)`
/// as the desktop origin on a multi-monitor topology.
pub(crate) fn clamp_window_size(
    geometry: WindowGeometry,
    available: CanvasPoint,
) -> WindowGeometry {
    let available = CanvasPoint::new(
        if available.x.is_finite() && available.x > 0.0 {
            available.x
        } else {
            1.0
        },
        if available.y.is_finite() && available.y > 0.0 {
            available.y
        } else {
            1.0
        },
    );
    let size = CanvasPoint::new(
        if geometry.size.x.is_finite() && geometry.size.x > 0.0 {
            geometry.size.x.min(available.x)
        } else {
            available.x.min(900.0).max(1.0)
        },
        if geometry.size.y.is_finite() && geometry.size.y > 0.0 {
            geometry.size.y.min(available.y)
        } else {
            available.y.min(650.0).max(1.0)
        },
    );
    WindowGeometry {
        position: None,
        size,
    }
}

/// Debounce preference writes caused by continuous pan/resize interactions.
/// The state can still update every frame, but persistence is eligible only
/// once the user has been idle for the delay (or explicitly flushes on close).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PreferenceDebounce {
    deadline: Option<Instant>,
    delay: Duration,
}

impl Default for PreferenceDebounce {
    fn default() -> Self {
        Self {
            deadline: None,
            delay: Duration::from_millis(250),
        }
    }
}

impl PreferenceDebounce {
    pub(crate) fn mark_changed(&mut self, now: Instant) {
        self.deadline = Some(now + self.delay);
    }

    pub(crate) fn ready(&self, now: Instant) -> bool {
        self.deadline.is_some_and(|deadline| now >= deadline)
    }

    pub(crate) fn flush(&mut self) {
        self.deadline = Some(Instant::now());
    }

    pub(crate) fn clear(&mut self) {
        self.deadline = None;
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct CanvasTransform {
    /// Top-left corner of the clipped canvas in screen logical units.
    pub(crate) canvas_origin: CanvasPoint,
    /// Size of the clipped canvas in screen logical units.
    pub(crate) canvas_size: CanvasPoint,
    /// Logical world bounds emitted by the production radial layout.
    pub(crate) world_min: CanvasPoint,
    pub(crate) world_max: CanvasPoint,
    /// User pan in screen logical units.
    pub(crate) pan: CanvasPoint,
    /// User zoom, clamped to a finite positive range by [`Self::normalized`].
    pub(crate) zoom: f32,
    /// egui logical-to-physical scale used by the prepared frame.
    pub(crate) dpi: f32,
}

impl CanvasTransform {
    pub(crate) fn normalized(mut self) -> Self {
        if !self.canvas_origin.x.is_finite() {
            self.canvas_origin.x = 0.0;
        }
        if !self.canvas_origin.y.is_finite() {
            self.canvas_origin.y = 0.0;
        }
        if !self.canvas_size.x.is_finite() || self.canvas_size.x <= 0.0 {
            self.canvas_size.x = 1.0;
        }
        if !self.canvas_size.y.is_finite() || self.canvas_size.y <= 0.0 {
            self.canvas_size.y = 1.0;
        }
        if !self.world_min.x.is_finite()
            || !self.world_min.y.is_finite()
            || !self.world_max.x.is_finite()
            || !self.world_max.y.is_finite()
            || self.world_max.x <= self.world_min.x
            || self.world_max.y <= self.world_min.y
        {
            self.world_min = CanvasPoint::new(-1.0, -1.0);
            self.world_max = CanvasPoint::new(1.0, 1.0);
        }
        if !self.pan.x.is_finite() {
            self.pan.x = 0.0;
        }
        if !self.pan.y.is_finite() {
            self.pan.y = 0.0;
        }
        if !self.zoom.is_finite() {
            self.zoom = 1.0;
        }
        self.zoom = self.zoom.clamp(0.1, 8.0);
        if !self.dpi.is_finite() || self.dpi <= 0.0 {
            self.dpi = 1.0;
        }
        self.dpi = self.dpi.clamp(0.25, 8.0);
        self
    }

    fn base_scale(&self) -> CanvasPoint {
        let world_width = (self.world_max.x - self.world_min.x).max(f32::EPSILON);
        let world_height = (self.world_max.y - self.world_min.y).max(f32::EPSILON);
        // Layout coordinates are logical while the compositor texture is
        // rasterized in physical pixels.  Fit in physical space, then divide
        // back to egui logical units at the final mapping boundary.
        let physical_width = self.canvas_size.x * self.dpi;
        let physical_height = self.canvas_size.y * self.dpi;
        let scale = (physical_width / world_width).min(physical_height / world_height);
        CanvasPoint::new(scale, scale)
    }

    fn content_offset(&self, scale: CanvasPoint) -> CanvasPoint {
        // `canvas_origin`/`canvas_size` are egui logical units, just like
        // `LayoutSnapshot` world coordinates.  The compositor raster is
        // physical pixels, so the fit is computed in physical space and then
        // mapped back to logical coordinates by both directions below.  Keep
        // this factor explicit so the inverse uses the exact same transform.
        let factor = self.zoom;
        CanvasPoint::new(
            ((self.canvas_size.x * self.dpi
                - (self.world_max.x - self.world_min.x) * scale.x * factor)
                .max(0.0))
                * 0.5,
            ((self.canvas_size.y * self.dpi
                - (self.world_max.y - self.world_min.y) * scale.y * factor)
                .max(0.0))
                * 0.5,
        )
    }

    /// Convert a production-layout world point to the exact screen transform
    /// used by the designer image and hit test.
    pub(crate) fn world_to_screen(self, world: CanvasPoint) -> CanvasPoint {
        let this = self.normalized();
        let scale = this.base_scale();
        let offset = this.content_offset(scale);
        CanvasPoint::new(
            this.canvas_origin.x
                + (offset.x
                    + (world.x - this.world_min.x) * scale.x * this.zoom
                    + this.pan.x * this.dpi)
                    / this.dpi,
            this.canvas_origin.y
                + (offset.y
                    + (world.y - this.world_min.y) * scale.y * this.zoom
                    + this.pan.y * this.dpi)
                    / this.dpi,
        )
    }

    /// Inverse of [`Self::world_to_screen`].  Callers should use the same
    /// transform instance for painting and hit testing; no second fit or
    /// content-driven recentering is performed here.
    pub(crate) fn screen_to_world(self, screen: CanvasPoint) -> CanvasPoint {
        let this = self.normalized();
        let scale = this.base_scale();
        let offset = this.content_offset(scale);
        CanvasPoint::new(
            this.world_min.x
                + ((screen.x - this.canvas_origin.x) * this.dpi - offset.x - this.pan.x * this.dpi)
                    / (scale.x * this.zoom),
            this.world_min.y
                + ((screen.y - this.canvas_origin.y) * this.dpi - offset.y - this.pan.y * this.dpi)
                    / (scale.y * this.zoom),
        )
    }

    pub(crate) fn contains_screen(self, screen: CanvasPoint) -> bool {
        let this = self.normalized();
        screen.x >= this.canvas_origin.x
            && screen.y >= this.canvas_origin.y
            && screen.x <= this.canvas_origin.x + this.canvas_size.x
            && screen.y <= this.canvas_origin.y + this.canvas_size.y
    }

    pub(crate) fn fit(
        canvas_origin: CanvasPoint,
        canvas_size: CanvasPoint,
        world_min: CanvasPoint,
        world_max: CanvasPoint,
        dpi: f32,
    ) -> Self {
        Self {
            canvas_origin,
            canvas_size,
            world_min,
            world_max,
            pan: CanvasPoint::default(),
            zoom: 1.0,
            dpi,
        }
        .normalized()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ProjectedCellProvenance {
    Authored {
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
    },
    Dynamic {
        menu_id: MenuId,
        ring_id: RingId,
        source_cell_id: CellId,
        source: DynamicSource,
        result_index: usize,
        fingerprint: SourceFingerprint,
    },
    Center,
    Background,
    Control,
}

impl ProjectedCellProvenance {
    pub(crate) fn is_authored(&self) -> bool {
        matches!(self, Self::Authored { .. })
    }

    pub(crate) fn authored_ids(&self) -> Option<(&MenuId, &RingId, &CellId)> {
        match self {
            Self::Authored {
                menu_id,
                ring_id,
                cell_id,
            } => Some((menu_id, ring_id, cell_id)),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ProjectedSelection {
    pub(crate) label: String,
    pub(crate) provenance: ProjectedCellProvenance,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DragPayload {
    pub(crate) source: ProjectedCellProvenance,
    pub(crate) generation: DraftGeneration,
}

/// A center-plus placement stays a draft until the inspector assigns valid
/// authored content to the chosen stable slot.  It is deliberately separate
/// from [`DragPayload`]: cancelling the draft never mutates the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct PlacementDraft {
    pub(crate) menu_id: MenuId,
    pub(crate) ring_id: RingId,
    pub(crate) cell_id: CellId,
    pub(crate) generation: DraftGeneration,
}

impl PlacementDraft {
    pub(crate) fn new(
        menu_id: MenuId,
        ring_id: RingId,
        cell_id: CellId,
        generation: DraftGeneration,
    ) -> Self {
        Self {
            menu_id,
            ring_id,
            cell_id,
            generation,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VisitedMenuPath {
    menus: Vec<MenuId>,
}

impl VisitedMenuPath {
    pub(crate) fn new(root: MenuId) -> Self {
        Self { menus: vec![root] }
    }

    pub(crate) fn current(&self) -> Option<&MenuId> {
        self.menus.last()
    }

    pub(crate) fn as_slice(&self) -> &[MenuId] {
        &self.menus
    }

    pub(crate) fn enter(&mut self, menu_id: MenuId) -> bool {
        if self.menus.contains(&menu_id) {
            return false;
        }
        self.menus.push(menu_id);
        true
    }

    pub(crate) fn select_direct(&mut self, menu_id: MenuId) {
        self.menus.clear();
        self.menus.push(menu_id);
    }

    pub(crate) fn ensure_root(&mut self, root: MenuId, existing: impl Fn(&MenuId) -> bool) {
        // A direct tree selection is a valid path root even when it differs
        // from the document's configured default.  Only an empty path or a
        // deleted/invalid root should be repaired to the default.
        if self.menus.is_empty() {
            self.menus.clear();
            self.menus.push(root);
        } else if self.menus.first().is_none_or(|id| !existing(id)) {
            self.menus.clear();
            self.menus.push(root);
        }
        self.replace_invalid_tail(existing);
    }

    /// Select a menu from the tree/inspector as a fresh path root.  A tree
    /// selection does not prove that the menu was reached through a submenu
    /// edge, so retaining the configured default root would invent a
    /// breadcrumb for unreferenced (or multiply referenced) menus.  Real
    /// canvas traversal uses [`Self::enter`] and is preserved by the early
    /// current-menu return.
    pub(crate) fn select_menu(&mut self, _root: MenuId, menu_id: MenuId) {
        if self.current() == Some(&menu_id) {
            return;
        }
        self.menus.clear();
        self.menus.push(menu_id);
    }

    pub(crate) fn back(&mut self) -> Option<MenuId> {
        (self.menus.len() > 1).then(|| self.menus.pop().expect("path has parent"))
    }

    pub(crate) fn replace_invalid_tail(&mut self, existing: impl Fn(&MenuId) -> bool) {
        while self.menus.len() > 1 {
            if self.current().is_some_and(&existing) {
                break;
            }
            self.menus.pop();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn transform_round_trips_zoom_pan_and_dpi() {
        let transform = CanvasTransform::fit(
            CanvasPoint::new(14.0, 22.0),
            CanvasPoint::new(640.0, 420.0),
            CanvasPoint::new(-120.0, -80.0),
            CanvasPoint::new(520.0, 360.0),
            1.5,
        );
        let transform = CanvasTransform {
            zoom: 1.7,
            pan: CanvasPoint::new(-31.0, 18.0),
            ..transform
        };
        for point in [
            CanvasPoint::new(-120.0, -80.0),
            CanvasPoint::new(0.0, 0.0),
            CanvasPoint::new(240.0, 140.0),
            CanvasPoint::new(520.0, 360.0),
        ] {
            let screen = transform.world_to_screen(point);
            let round_trip = transform.screen_to_world(screen);
            assert!((round_trip.x - point.x).abs() < 0.001);
            assert!((round_trip.y - point.y).abs() < 0.001);
        }
    }

    #[test]
    fn visited_path_is_the_navigation_source_of_truth_and_rejects_cycles() {
        let root = MenuId::new("root");
        let child = MenuId::new("child");
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter(child.clone()));
        assert!(!path.enter(root));
        assert_eq!(path.current(), Some(&child));
        assert!(path.back().is_some());
        assert_eq!(path.as_slice(), &[MenuId::new("root")]);
    }

    #[test]
    fn selecting_from_the_tree_establishes_a_fresh_path_root() {
        let root = MenuId::new("root");
        let first = MenuId::new("first");
        let second = MenuId::new("second");
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter(first));
        path.select_menu(root.clone(), second.clone());
        assert_eq!(path.as_slice(), &[second]);
    }

    #[test]
    fn selection_mirroring_does_not_collapse_canvas_navigation() {
        let root = MenuId::new("root");
        let child = MenuId::new("child");
        let grandchild = MenuId::new("grandchild");
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter(child));
        assert!(path.enter(grandchild.clone()));
        path.select_menu(root, grandchild);
        assert_eq!(path.as_slice().len(), 3);
    }

    #[test]
    fn direct_selection_does_not_invent_parent_for_unreferenced_menu() {
        let root = MenuId::new("root");
        let reused = MenuId::new("reused");
        let child = MenuId::new("child");
        let mut path = VisitedMenuPath::new(root);
        path.select_menu(MenuId::new("root"), reused.clone());
        assert_eq!(path.as_slice(), &[reused.clone()]);
        assert!(path.enter(child.clone()));
        assert_eq!(path.back(), Some(child));
        assert_eq!(path.as_slice(), &[reused]);
    }

    #[test]
    fn next_frame_normalization_preserves_direct_root_and_repairs_deleted_root() {
        let default_root = MenuId::new("default");
        let selected = MenuId::new("selected");
        let child = MenuId::new("child");
        let mut path = VisitedMenuPath::new(default_root.clone());
        path.select_direct(selected.clone());
        assert!(path.enter(child.clone()));

        let existing = |id: &MenuId| id == &default_root || id == &selected || id == &child;
        path.ensure_root(default_root.clone(), existing);
        assert_eq!(path.as_slice(), &[selected.clone(), child]);
        assert_eq!(path.back(), Some(MenuId::new("child")));
        assert_eq!(path.as_slice(), &[selected.clone()]);

        path.select_direct(selected);
        path.ensure_root(default_root.clone(), |id| id == &default_root);
        assert_eq!(path.as_slice(), &[default_root]);
    }

    #[test]
    fn dynamic_projection_never_reports_authored_ids() {
        let provenance = ProjectedCellProvenance::Dynamic {
            menu_id: MenuId::new("menu"),
            ring_id: RingId::new("ring"),
            source_cell_id: CellId::new("source"),
            source: DynamicSource::Favorites,
            result_index: 3,
            fingerprint: SourceFingerprint {
                generation: 1,
                source: "favorites".into(),
                query: None,
            },
        };
        assert!(!provenance.is_authored());
        assert!(provenance.authored_ids().is_none());
    }

    #[test]
    fn restored_geometry_clamps_to_negative_work_area_without_losing_visibility() {
        let restored = clamp_window_geometry(
            WindowGeometry {
                position: Some(CanvasPoint::new(900.0, 900.0)),
                size: CanvasPoint::new(1_200.0, 800.0),
            },
            WindowWorkArea {
                origin: CanvasPoint::new(-1_920.0, -200.0),
                size: CanvasPoint::new(1_600.0, 900.0),
            },
        );
        assert_eq!(restored.size, CanvasPoint::new(1_200.0, 800.0));
        assert_eq!(restored.position, Some(CanvasPoint::new(-1_520.0, -100.0)));
    }

    #[test]
    fn preference_debounce_waits_for_idle_or_explicit_flush() {
        let start = Instant::now();
        let mut debounce = PreferenceDebounce::default();
        debounce.mark_changed(start);
        assert!(!debounce.ready(start + Duration::from_millis(249)));
        assert!(debounce.ready(start + Duration::from_millis(250)));
        debounce.mark_changed(start);
        debounce.flush();
        assert!(debounce.ready(Instant::now()));
        debounce.clear();
        assert!(!debounce.ready(Instant::now()));
    }
}
