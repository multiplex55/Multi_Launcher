//! Pure canvas state for the compact Radial Designer.
//!
//! The renderer owns the egui paint objects, while this module owns the
//! coordinate and authoring invariants.  Keeping the transform and projected
//! identity independent of egui makes hit testing deterministic at arbitrary
//! zoom, pan, and DPI values and prevents dynamic preview rows from becoming
//! persisted document entities.

use crate::radial::authoring::{DraftGeneration, StableSelection};
use crate::radial::dynamic::SourceFingerprint;
use crate::radial::model::{CellContent, CellId, DynamicSource, MenuId, RadialDocument, RingId};
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

const MAX_EDITOR_BACK_LOCATIONS: usize = 64;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SubmenuPathEdge {
    pub(crate) parent_menu: MenuId,
    pub(crate) ring_id: RingId,
    pub(crate) cell_id: CellId,
    pub(crate) child_menu: MenuId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DesignerNavigationIntent {
    Select {
        target: StableSelection,
        control: bool,
        shift: bool,
        force_history: bool,
        open_properties: bool,
    },
    ClearSelection,
    CreateSkin,
    DuplicateSkin(crate::radial::model::SkinId),
    EnterSubmenu {
        edge: SubmenuPathEdge,
        return_selection: StableSelection,
    },
    Back,
    Breadcrumb(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct EditLocation {
    menus: Vec<MenuId>,
    edges: Vec<SubmenuPathEdge>,
    return_selections: Vec<Option<StableSelection>>,
    selection: Option<StableSelection>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct VisitedMenuPath {
    menus: Vec<MenuId>,
    edges: Vec<SubmenuPathEdge>,
    return_selections: Vec<Option<StableSelection>>,
    previous_locations: Vec<EditLocation>,
}

impl VisitedMenuPath {
    pub(crate) fn new(root: MenuId) -> Self {
        Self {
            menus: vec![root],
            ..Self::default()
        }
    }

    pub(crate) fn current(&self) -> Option<&MenuId> {
        self.menus.last()
    }

    pub(crate) fn can_back(&self) -> bool {
        self.menus.len() > 1 || !self.previous_locations.is_empty()
    }

    pub(crate) fn as_slice(&self) -> &[MenuId] {
        &self.menus
    }

    pub(crate) fn acceptance_menu_id_digests(&self) -> Vec<u64> {
        self.menus
            .iter()
            .map(|menu| acceptance_identity_digest(menu.as_str().as_bytes()))
            .collect()
    }

    pub(crate) fn acceptance_edge_digests(&self) -> Vec<u64> {
        self.edges
            .iter()
            .map(|edge| {
                let identity = format!(
                    "{}\0{}\0{}\0{}",
                    edge.parent_menu, edge.ring_id, edge.cell_id, edge.child_menu
                );
                acceptance_identity_digest(identity.as_bytes())
            })
            .collect()
    }

    pub(crate) fn acceptance_digest(&self) -> u64 {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.menus.hash(&mut hasher);
        for edge in &self.edges {
            edge.parent_menu.hash(&mut hasher);
            edge.ring_id.hash(&mut hasher);
            edge.cell_id.hash(&mut hasher);
            edge.child_menu.hash(&mut hasher);
        }
        self.return_selections.hash(&mut hasher);
        for location in &self.previous_locations {
            location.menus.hash(&mut hasher);
            for edge in &location.edges {
                edge.parent_menu.hash(&mut hasher);
                edge.ring_id.hash(&mut hasher);
                edge.cell_id.hash(&mut hasher);
                edge.child_menu.hash(&mut hasher);
            }
            location.return_selections.hash(&mut hasher);
            location.selection.hash(&mut hasher);
        }
        hasher.finish()
    }

    pub(crate) fn enter_submenu(
        &mut self,
        edge: SubmenuPathEdge,
        return_selection: Option<StableSelection>,
    ) -> bool {
        if self.current() != Some(&edge.parent_menu)
            || self.menus.contains(&edge.child_menu)
            || self.menus.len().saturating_sub(1) >= crate::radial::model::limits::MAX_SUBMENU_DEPTH
        {
            return false;
        }
        self.menus.push(edge.child_menu.clone());
        self.edges.push(edge);
        self.return_selections.push(return_selection);
        true
    }

    pub(crate) fn direct_reveal(
        &mut self,
        menu_id: MenuId,
        selection_before: Option<StableSelection>,
        force_history: bool,
    ) {
        let target_is_current = self.current() == Some(&menu_id);
        if !target_is_current || force_history {
            self.previous_locations.push(EditLocation {
                menus: self.menus.clone(),
                edges: self.edges.clone(),
                return_selections: self.return_selections.clone(),
                selection: selection_before,
            });
            if self.previous_locations.len() > MAX_EDITOR_BACK_LOCATIONS {
                self.previous_locations.remove(0);
            }
            self.menus = vec![menu_id];
            self.edges.clear();
            self.return_selections.clear();
        }
    }

    pub(crate) fn back_location(
        &mut self,
        current_selection: Option<StableSelection>,
    ) -> Option<(MenuId, Option<StableSelection>)> {
        if self.menus.len() > 1 {
            self.menus.pop();
            self.edges.pop();
            let returned = self.return_selections.pop().flatten();
            let menu_id = self.current()?.clone();
            return Some((
                menu_id.clone(),
                returned.or_else(|| Some(StableSelection::Menu(menu_id))),
            ));
        }
        let location = self.previous_locations.pop()?;
        self.menus = location.menus;
        self.edges = location.edges;
        self.return_selections = location.return_selections;
        let menu_id = self.current()?.clone();
        let selection = location
            .selection
            .filter(|selection| selection_menu_id(selection) == Some(&menu_id))
            .or_else(|| {
                current_selection.filter(|selection| selection_menu_id(selection) == Some(&menu_id))
            });
        Some((menu_id, selection))
    }

    pub(crate) fn go_to_breadcrumb(
        &mut self,
        index: usize,
    ) -> Option<(MenuId, Option<StableSelection>)> {
        if index >= self.menus.len() {
            return None;
        }
        self.menus.truncate(index + 1);
        self.edges.truncate(index);
        let return_selection = self.return_selections.get(index).cloned().flatten();
        self.return_selections.truncate(index);
        Some((self.current()?.clone(), return_selection))
    }

    pub(crate) fn validate_document_path(&mut self, document: &RadialDocument) {
        let exists = |id: &MenuId| document.menus.iter().any(|menu| &menu.id == id);
        let root = document.default_menu_id.clone();
        if self.menus.is_empty() || self.menus.first().is_none_or(|id| !exists(id)) {
            self.menus = vec![root.clone()];
            self.edges.clear();
            self.return_selections.clear();
        }
        let mut valid_len = 1usize;
        let mut visited = vec![self.menus[0].clone()];
        let max_len = crate::radial::model::limits::MAX_SUBMENU_DEPTH + 1;
        for index in 0..self.menus.len().saturating_sub(1).min(max_len - 1) {
            let Some(edge) = self.edges.get(index) else {
                break;
            };
            if edge.parent_menu != self.menus[index]
                || edge.child_menu != self.menus[index + 1]
                || visited.contains(&edge.child_menu)
            {
                break;
            }
            let linked = document
                .menus
                .iter()
                .find(|menu| menu.id == edge.parent_menu)
                .and_then(|menu| menu.rings.iter().find(|ring| ring.id == edge.ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| cell.id == edge.cell_id))
                .is_some_and(|cell| matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == &edge.child_menu));
            if !linked || !exists(&edge.child_menu) {
                break;
            }
            visited.push(edge.child_menu.clone());
            valid_len += 1;
        }
        self.menus.truncate(valid_len);
        self.edges.truncate(valid_len.saturating_sub(1));
        self.return_selections.truncate(valid_len.saturating_sub(1));
        self.previous_locations.retain_mut(|location| {
            let mut path = Self {
                menus: location.menus.clone(),
                edges: location.edges.clone(),
                return_selections: location.return_selections.clone(),
                previous_locations: Vec::new(),
            };
            path.validate_document_path_without_history(document);
            let Some(menu_id) = path.current() else {
                return false;
            };
            let selection = match location.selection.as_ref() {
                Some(selection) => {
                    crate::radial::authoring::reconcile_stable_selection(selection, document)
                        .filter(|selection| selection_menu_id(selection) == Some(menu_id))
                }
                None => None,
            };
            if location.selection.is_some() && selection.is_none() {
                return false;
            }
            location.menus = path.menus;
            location.edges = path.edges;
            location.return_selections = path.return_selections;
            location.selection = selection;
            true
        });
    }

    fn validate_document_path_without_history(&mut self, document: &RadialDocument) {
        let exists = |id: &MenuId| document.menus.iter().any(|menu| &menu.id == id);
        if self.menus.is_empty() || self.menus.first().is_none_or(|id| !exists(id)) {
            self.menus = vec![document.default_menu_id.clone()];
            self.edges.clear();
            self.return_selections.clear();
        }
        let mut valid_len = 1usize;
        let mut visited = vec![self.menus[0].clone()];
        for index in 0..self
            .menus
            .len()
            .saturating_sub(1)
            .min(crate::radial::model::limits::MAX_SUBMENU_DEPTH)
        {
            let Some(edge) = self.edges.get(index) else {
                break;
            };
            let linked = document
                .menus
                .iter()
                .find(|menu| menu.id == edge.parent_menu)
                .and_then(|menu| menu.rings.iter().find(|ring| ring.id == edge.ring_id))
                .and_then(|ring| ring.cells.iter().find(|cell| cell.id == edge.cell_id))
                .is_some_and(|cell| matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == &edge.child_menu));
            if edge.parent_menu != self.menus[index]
                || edge.child_menu != self.menus[index + 1]
                || visited.contains(&edge.child_menu)
                || !linked
                || !exists(&edge.child_menu)
            {
                break;
            }
            valid_len += 1;
            visited.push(edge.child_menu.clone());
        }
        self.menus.truncate(valid_len);
        self.edges.truncate(valid_len.saturating_sub(1));
        self.return_selections.truncate(valid_len.saturating_sub(1));
    }
}

fn acceptance_identity_digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}

fn selection_menu_id(selection: &StableSelection) -> Option<&MenuId> {
    match selection {
        StableSelection::Menu(menu_id)
        | StableSelection::Ring { menu_id, .. }
        | StableSelection::Cell { menu_id, .. } => Some(menu_id),
        StableSelection::CellSet(cells) => Some(&cells.primary.menu_id),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn navigation_fixture() -> RadialDocument {
        let mut document = RadialDocument::starter();
        let root_id = document.default_menu_id.clone();
        let child_id = MenuId::new("shared-child");
        let second_parent_id = MenuId::new("second-parent");
        let grandchild_id = MenuId::new("grandchild");
        let template = document.menus[0].clone();
        let root_ring = document.menus[0].rings[0].id.clone();
        let root_cell = document.menus[0].rings[0].cells[0].id.clone();
        document.menus[0].rings[0].cells[0].content = CellContent::Submenu {
            menu_id: child_id.clone(),
        };

        let make_menu = |id: MenuId, name: &str, prefix: &str| {
            let mut menu = template.clone();
            menu.id = id;
            menu.name = name.into();
            for (ring_index, ring) in menu.rings.iter_mut().enumerate() {
                ring.id = RingId::new(format!("{prefix}-ring-{ring_index}"));
                for (cell_index, cell) in ring.cells.iter_mut().enumerate() {
                    cell.id = CellId::new(format!("{prefix}-cell-{ring_index}-{cell_index}"));
                    cell.label = "Spacer".into();
                    cell.content = CellContent::Spacer;
                }
            }
            menu
        };
        let mut child = make_menu(child_id.clone(), "Child", "child");
        child.rings[0].cells[0].content = CellContent::Submenu {
            menu_id: grandchild_id.clone(),
        };
        let mut second_parent = make_menu(second_parent_id.clone(), "Second parent", "parent2");
        second_parent.rings[0].cells[0].content = CellContent::Submenu { menu_id: child_id };
        let grandchild = make_menu(grandchild_id, "Grandchild", "grandchild");
        document.menus.extend([child, second_parent, grandchild]);
        // Keep these stable fixtures available to tests without deriving a
        // parent from graph structure.
        let _ = (root_id, root_ring, root_cell);
        document
    }

    fn submenu_edge(
        document: &RadialDocument,
        parent_menu: &MenuId,
        child_menu: &MenuId,
    ) -> SubmenuPathEdge {
        let cell = document
            .menus
            .iter()
            .find(|menu| &menu.id == parent_menu)
            .unwrap()
            .rings
            .iter()
            .flat_map(|ring| ring.cells.iter().map(move |cell| (ring, cell)))
            .find(|(_, cell)| {
                matches!(&cell.content, CellContent::Submenu { menu_id } if menu_id == child_menu)
            })
            .unwrap();
        SubmenuPathEdge {
            parent_menu: parent_menu.clone(),
            ring_id: cell.0.id.clone(),
            cell_id: cell.1.id.clone(),
            child_menu: child_menu.clone(),
        }
    }

    fn cell_selection(
        document: &RadialDocument,
        menu_id: &MenuId,
        index: usize,
    ) -> StableSelection {
        let menu = document
            .menus
            .iter()
            .find(|menu| &menu.id == menu_id)
            .unwrap();
        StableSelection::Cell {
            menu_id: menu_id.clone(),
            ring_id: menu.rings[0].id.clone(),
            cell_id: menu.rings[0].cells[index].id.clone(),
        }
    }

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
    fn visited_path_uses_real_edges_and_enforces_cycle_and_depth_bounds() {
        let document = navigation_fixture();
        let root = document.default_menu_id.clone();
        let child = MenuId::new("shared-child");
        let edge = submenu_edge(&document, &root, &child);
        let return_selection = cell_selection(&document, &root, 0);
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter_submenu(edge, Some(return_selection.clone())));
        path.validate_document_path(&document);
        assert_eq!(path.current(), Some(&child));
        assert!(!path.enter_submenu(
            SubmenuPathEdge {
                parent_menu: child.clone(),
                ring_id: RingId::new("cycle"),
                cell_id: CellId::new("cycle"),
                child_menu: root.clone(),
            },
            None,
        ));
        let (menu, selection) = path
            .back_location(Some(StableSelection::Menu(child.clone())))
            .unwrap();
        assert_eq!(menu, root);
        assert_eq!(selection, Some(return_selection));

        let mut deep = VisitedMenuPath::new(MenuId::new("depth-root"));
        for index in 0..crate::radial::model::limits::MAX_SUBMENU_DEPTH {
            let parent = deep.current().unwrap().clone();
            assert!(deep.enter_submenu(
                SubmenuPathEdge {
                    parent_menu: parent,
                    ring_id: RingId::new(format!("ring-{index}")),
                    cell_id: CellId::new(format!("cell-{index}")),
                    child_menu: MenuId::new(format!("depth-{index}")),
                },
                None,
            ));
        }
        let parent = deep.current().unwrap().clone();
        assert!(!deep.enter_submenu(
            SubmenuPathEdge {
                parent_menu: parent,
                ring_id: RingId::new("too-deep-ring"),
                cell_id: CellId::new("too-deep-cell"),
                child_menu: MenuId::new("too-deep"),
            },
            None,
        ));
    }

    #[test]
    fn direct_reveal_and_shared_submenu_back_restore_actual_edit_locations() {
        let document = navigation_fixture();
        let root = document.default_menu_id.clone();
        let child = MenuId::new("shared-child");
        let second_parent = MenuId::new("second-parent");
        let child_selection = cell_selection(&document, &child, 1);
        let root_cell = cell_selection(&document, &root, 0);
        let second_parent_cell = cell_selection(&document, &second_parent, 0);
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter_submenu(
            submenu_edge(&document, &root, &child),
            Some(root_cell.clone()),
        ));
        path.direct_reveal(second_parent.clone(), Some(child_selection.clone()), false);
        assert_eq!(path.as_slice(), &[second_parent.clone()]);
        assert!(path.enter_submenu(
            submenu_edge(&document, &second_parent, &child),
            Some(second_parent_cell.clone()),
        ));
        path.validate_document_path(&document);
        assert_eq!(path.edges[0].parent_menu, second_parent);
        let (menu, selection) = path
            .back_location(Some(StableSelection::Menu(child.clone())))
            .unwrap();
        assert_eq!(menu, MenuId::new("second-parent"));
        assert_eq!(selection, Some(second_parent_cell));
        let (menu, selection) = path
            .back_location(Some(StableSelection::Menu(menu.clone())))
            .unwrap();
        assert_eq!(menu, child);
        assert_eq!(selection, Some(child_selection));
    }

    #[test]
    fn direct_reveal_preserves_back_location_without_a_prior_selection() {
        let document = navigation_fixture();
        let root = document.default_menu_id.clone();
        let second_parent = MenuId::new("second-parent");
        let mut path = VisitedMenuPath::new(root.clone());

        path.direct_reveal(second_parent.clone(), None, false);
        path.validate_document_path(&document);
        assert_eq!(path.as_slice(), &[second_parent.clone()]);
        assert!(path.can_back());

        let (menu, selection) = path
            .back_location(Some(StableSelection::Menu(second_parent)))
            .unwrap();
        assert_eq!(menu, root);
        assert_eq!(selection, None);
    }

    #[test]
    fn invalid_intermediate_edges_are_repaired_and_deleted_history_is_skipped() {
        let mut document = navigation_fixture();
        let root = document.default_menu_id.clone();
        let child = MenuId::new("shared-child");
        let second_parent = MenuId::new("second-parent");
        let grandchild = MenuId::new("grandchild");
        let mut path = VisitedMenuPath::new(root.clone());
        assert!(path.enter_submenu(
            submenu_edge(&document, &root, &child),
            Some(cell_selection(&document, &root, 0)),
        ));
        assert!(path.enter_submenu(
            submenu_edge(&document, &child, &grandchild),
            Some(cell_selection(&document, &child, 0)),
        ));
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child)
            .unwrap()
            .name = "Renamed".into();
        path.validate_document_path(&document);
        assert_eq!(path.as_slice(), &[root.clone(), child.clone(), grandchild]);
        assert_eq!(
            document
                .menus
                .iter()
                .find(|menu| menu.id == child)
                .unwrap()
                .name,
            "Renamed"
        );
        let second_edge = submenu_edge(&document, &child, &MenuId::new("grandchild"));
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == second_edge.parent_menu)
            .unwrap()
            .rings
            .iter_mut()
            .find(|ring| ring.id == second_edge.ring_id)
            .unwrap()
            .cells
            .iter_mut()
            .find(|cell| cell.id == second_edge.cell_id)
            .unwrap()
            .content = CellContent::Spacer;
        path.validate_document_path(&document);
        assert_eq!(path.as_slice(), &[root.clone(), child.clone()]);

        let mut history_path = VisitedMenuPath::new(root.clone());
        assert!(history_path.enter_submenu(
            submenu_edge(&document, &root, &child),
            Some(cell_selection(&document, &root, 0)),
        ));
        let prior_selection = cell_selection(&document, &child, 1);
        let prior_cell_id = match &prior_selection {
            StableSelection::Cell { cell_id, .. } => cell_id.clone(),
            _ => unreachable!(),
        };
        history_path.direct_reveal(second_parent, Some(prior_selection.clone()), false);
        document
            .menus
            .iter_mut()
            .find(|menu| menu.id == child)
            .unwrap()
            .rings[0]
            .cells
            .retain(|cell| cell.id != prior_cell_id);
        history_path.validate_document_path(&document);
        assert!(!history_path.can_back());
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
