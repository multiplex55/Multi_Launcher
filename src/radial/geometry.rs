use super::model::{CellContent, CellId, Control, LayoutKind, MenuDefinition, Override, RingId};
use std::f32::consts::TAU;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalPoint {
    pub x: f64,
    pub y: f64,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LogicalPoint {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LogicalRect {
    pub min: LogicalPoint,
    pub max: LogicalPoint,
}
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PhysicalRect {
    pub min: PhysicalPoint,
    pub max: PhysicalPoint,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ScaleFactor(f64);
impl ScaleFactor {
    pub fn new(value: f64) -> Option<Self> {
        (value.is_finite() && value > 0.0).then_some(Self(value))
    }
    pub fn get(self) -> f64 {
        self.0
    }
    pub fn physical_to_logical(self, point: PhysicalPoint) -> LogicalPoint {
        LogicalPoint {
            x: (point.x / self.0) as f32,
            y: (point.y / self.0) as f32,
        }
    }
    pub fn logical_to_physical(self, point: LogicalPoint) -> PhysicalPoint {
        PhysicalPoint {
            x: point.x as f64 * self.0,
            y: point.y as f64 * self.0,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum HitShape {
    Circle {
        center: LogicalPoint,
        radius: f32,
    },
    Wedge {
        center: LogicalPoint,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        end_angle: f32,
    },
}

#[derive(Clone, Debug, PartialEq)]
pub struct CellLayout {
    pub cell_id: CellId,
    pub ring_id: RingId,
    pub label: String,
    pub icon: Override<String>,
    pub control: Option<Control>,
    pub shape: HitShape,
    pub actionable: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LayoutSnapshot {
    pub requested_anchor: PhysicalPoint,
    pub origin: PhysicalPoint,
    pub scale_factor: ScaleFactor,
    pub scale: f32,
    pub center: LogicalPoint,
    pub center_radius: f32,
    pub background_extent: LogicalRect,
    pub rim_extent: LogicalRect,
    pub visual_extent: LogicalRect,
    pub input_extent: LogicalRect,
    pub cells: Vec<CellLayout>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum LayoutError {
    InvalidScale,
    InvalidWorkArea,
    Oversized {
        required_scale: f32,
        minimum_scale: f32,
    },
}

pub fn layout_menu(
    menu: &MenuDefinition,
    requested_anchor: PhysicalPoint,
    work_area: PhysicalRect,
    scale_factor: ScaleFactor,
    minimum_scale: f32,
) -> Result<LayoutSnapshot, LayoutError> {
    if !minimum_scale.is_finite() || minimum_scale <= 0.0 || minimum_scale > 1.0 {
        return Err(LayoutError::InvalidScale);
    }
    let width = ((work_area.max.x - work_area.min.x) / scale_factor.get()) as f32;
    let height = ((work_area.max.y - work_area.min.y) / scale_factor.get()) as f32;
    if width <= 0.0 || height <= 0.0 || !width.is_finite() || !height.is_finite() {
        return Err(LayoutError::InvalidWorkArea);
    }
    let nominal = menu
        .rings
        .iter()
        .map(|ring| ring.radius + ring.cell_radius)
        .fold(menu.center_radius, f32::max);
    // Reserve deterministic room for labels, glow, and the outer rim. Later
    // renderers may use less, but may not draw outside this snapshot extent.
    let visual_padding = 12.0;
    let required_scale = (width.min(height) / ((nominal + visual_padding) * 2.0)).min(1.0);
    if required_scale < minimum_scale {
        return Err(LayoutError::Oversized {
            required_scale,
            minimum_scale,
        });
    }
    let scale = required_scale;
    let input_margin = nominal * scale;
    let visual_margin = (nominal + visual_padding) * scale;
    let requested_logical = scale_factor.physical_to_logical(requested_anchor);
    let work_min = scale_factor.physical_to_logical(work_area.min);
    let work_max = scale_factor.physical_to_logical(work_area.max);
    let center = LogicalPoint {
        x: requested_logical
            .x
            .clamp(work_min.x + visual_margin, work_max.x - visual_margin),
        y: requested_logical
            .y
            .clamp(work_min.y + visual_margin, work_max.y - visual_margin),
    };
    let origin = scale_factor.logical_to_physical(center);
    let mut cells = Vec::new();
    for ring in &menu.rings {
        let n = ring.cells.len();
        if n == 0 {
            continue;
        }
        let start = ring.rotation_degrees.to_radians();
        for (index, cell) in ring.cells.iter().enumerate() {
            let shape = match menu.layout {
                LayoutKind::CircularCells => {
                    let angle = if n == 1 {
                        start
                    } else {
                        start + TAU * index as f32 / n as f32
                    };
                    HitShape::Circle {
                        center: LogicalPoint {
                            x: center.x + ring.radius * scale * angle.cos(),
                            y: center.y + ring.radius * scale * angle.sin(),
                        },
                        radius: ring.cell_radius * scale,
                    }
                }
                LayoutKind::Wedges => {
                    let width = TAU / n as f32;
                    let half_gap = if n <= 1 {
                        0.0
                    } else if ring.radius > 0.0 {
                        (ring.gap / ring.radius).min(width * 0.8) * 0.5
                    } else {
                        0.0
                    };
                    HitShape::Wedge {
                        center,
                        inner_radius: (ring.radius - ring.cell_radius).max(menu.center_radius)
                            * scale,
                        outer_radius: (ring.radius + ring.cell_radius) * scale,
                        start_angle: start + width * index as f32 + half_gap,
                        end_angle: start + width * (index + 1) as f32 - half_gap,
                    }
                }
            };
            cells.push(CellLayout {
                cell_id: cell.id.clone(),
                ring_id: ring.id.clone(),
                label: cell.label.clone(),
                icon: cell.icon.clone(),
                control: match &cell.content {
                    CellContent::Control { control } => Some(*control),
                    _ => None,
                },
                shape,
                actionable: !matches!(cell.content, super::model::CellContent::Spacer),
            });
        }
    }
    let input_extent = LogicalRect {
        min: LogicalPoint {
            x: center.x - input_margin,
            y: center.y - input_margin,
        },
        max: LogicalPoint {
            x: center.x + input_margin,
            y: center.y + input_margin,
        },
    };
    let visual_extent = LogicalRect {
        min: LogicalPoint {
            x: center.x - visual_margin,
            y: center.y - visual_margin,
        },
        max: LogicalPoint {
            x: center.x + visual_margin,
            y: center.y + visual_margin,
        },
    };
    Ok(LayoutSnapshot {
        requested_anchor,
        origin,
        scale_factor,
        scale,
        center,
        center_radius: menu.center_radius * scale,
        background_extent: input_extent,
        rim_extent: visual_extent,
        visual_extent,
        input_extent,
        cells,
    })
}

impl LayoutSnapshot {
    /// Later entries are topmost. Half-open wedge edges ensure a boundary has one owner.
    pub fn hit_test(&self, point: LogicalPoint) -> Option<&CellLayout> {
        self.cells
            .iter()
            .rev()
            .find(|cell| cell.actionable && contains(&cell.shape, point))
    }
}

fn contains(shape: &HitShape, point: LogicalPoint) -> bool {
    match shape {
        HitShape::Circle { center, radius } => distance2(*center, point) <= radius * radius,
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let dx = point.x - center.x;
            let dy = point.y - center.y;
            let radius2 = dx * dx + dy * dy;
            if radius2 < inner_radius * inner_radius || radius2 > outer_radius * outer_radius {
                return false;
            }
            let angle = dy.atan2(dx);
            angle_in_interval(angle, *start_angle, *end_angle)
        }
    }
}
fn distance2(a: LogicalPoint, b: LogicalPoint) -> f32 {
    let dx = a.x - b.x;
    let dy = a.y - b.y;
    dx * dx + dy * dy
}
fn normalize(angle: f32) -> f32 {
    angle.rem_euclid(TAU)
}
fn angle_in_interval(angle: f32, start: f32, end: f32) -> bool {
    let span = end - start;
    if span.abs() >= TAU - 1e-5 {
        return true;
    }
    let width = span.rem_euclid(TAU);
    let offset = (normalize(angle) - normalize(start)).rem_euclid(TAU);
    // Every sector owns its start and excludes its end. At the wrap boundary,
    // the first sector therefore owns the point and the last does not.
    offset < width
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::model::*;
    fn menu(layout: LayoutKind) -> MenuDefinition {
        let mut m = RadialDocument::starter().menus.remove(0);
        m.layout = layout;
        m
    }
    fn work() -> PhysicalRect {
        PhysicalRect {
            min: PhysicalPoint {
                x: -1920.0,
                y: -200.0,
            },
            max: PhysicalPoint { x: 0.0, y: 880.0 },
        }
    }

    #[test]
    fn conversions_preserve_negative_coordinates_at_fractional_dpi() {
        let s = ScaleFactor::new(1.25).unwrap();
        let p = PhysicalPoint {
            x: -1234.5,
            y: 78.75,
        };
        let round = s.logical_to_physical(s.physical_to_logical(p));
        assert!((round.x - p.x).abs() < 0.001);
        assert!((round.y - p.y).abs() < 0.001);
    }
    #[test]
    fn requested_anchor_is_retained_while_center_is_clamped() {
        let l = layout_menu(
            &menu(LayoutKind::CircularCells),
            PhysicalPoint {
                x: -2500.0,
                y: -500.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(l.requested_anchor.x, -2500.0);
        assert!(l.origin.x >= -1920.0);
        assert!(l.visual_extent.min.x >= -1920.0);
    }
    #[test]
    fn circular_spacer_reserves_geometry_but_cannot_hit() {
        let mut m = menu(LayoutKind::CircularCells);
        m.rings[0].cells[0].content = CellContent::Spacer;
        let l = layout_menu(
            &m,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let p = match l.cells[0].shape {
            HitShape::Circle { center, .. } => center,
            _ => unreachable!(),
        };
        assert!(l.hit_test(p).is_none());
    }
    #[test]
    fn wedge_gap_and_center_are_non_actionable_and_edges_have_one_owner() {
        let l = layout_menu(
            &menu(LayoutKind::Wedges),
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert!(l.hit_test(l.center).is_none());
        let first = &l.cells[0];
        if let HitShape::Wedge {
            center,
            outer_radius,
            end_angle,
            ..
        } = first.shape
        {
            let p = LogicalPoint {
                x: center.x + outer_radius * 0.8 * end_angle.cos(),
                y: center.y + outer_radius * 0.8 * end_angle.sin(),
            };
            let owners = l
                .cells
                .iter()
                .filter(|cell| contains(&cell.shape, p))
                .count();
            assert!(owners <= 1);
        } else {
            unreachable!()
        }
    }

    #[test]
    fn one_cell_wedge_owns_the_full_annulus() {
        let mut menu = menu(LayoutKind::Wedges);
        menu.rings[0].cells.truncate(1);
        let layout = layout_menu(
            &menu,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        for angle in [0.0_f32, 1.0, 3.0, 5.5] {
            let point = LogicalPoint {
                x: layout.center.x + 90.0 * angle.cos(),
                y: layout.center.y + 90.0 * angle.sin(),
            };
            assert_eq!(
                layout.hit_test(point).map(|cell| &cell.cell_id),
                Some(&layout.cells[0].cell_id)
            );
        }
    }

    #[test]
    fn gapless_wedge_wrap_boundary_has_exactly_one_owner() {
        let mut menu = menu(LayoutKind::Wedges);
        menu.rings[0].gap = 0.0;
        let layout = layout_menu(
            &menu,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        let start = menu.rings[0].rotation_degrees.to_radians();
        let point = LogicalPoint {
            x: layout.center.x + 90.0 * start.cos(),
            y: layout.center.y + 90.0 * start.sin(),
        };
        assert_eq!(
            layout
                .cells
                .iter()
                .filter(|cell| contains(&cell.shape, point))
                .count(),
            1
        );
    }
    #[test]
    fn zero_and_one_cell_rings_are_explicit() {
        let mut m = menu(LayoutKind::CircularCells);
        m.rings.push(RingDefinition {
            id: RingId::new("empty"),
            radius: 150.0,
            cell_radius: 20.0,
            rotation_degrees: 0.0,
            gap: 0.0,
            cells: vec![],
        });
        m.rings[0].cells.truncate(1);
        let l = layout_menu(
            &m,
            PhysicalPoint {
                x: -800.0,
                y: 300.0,
            },
            work(),
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap();
        assert_eq!(l.cells.len(), 1);
    }
    #[test]
    fn refuses_wheel_below_usable_minimum() {
        let tiny = PhysicalRect {
            min: PhysicalPoint { x: 0.0, y: 0.0 },
            max: PhysicalPoint { x: 20.0, y: 20.0 },
        };
        assert!(matches!(
            layout_menu(
                &menu(LayoutKind::CircularCells),
                PhysicalPoint::default(),
                tiny,
                ScaleFactor::new(1.0).unwrap(),
                0.5
            ),
            Err(LayoutError::Oversized { .. })
        ));
    }
}
