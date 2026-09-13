use super::geometry::{HitShape, LayoutSnapshot, LogicalPoint, LogicalRect};
use super::model::CellId;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgba(pub u8, pub u8, pub u8, pub u8);

#[derive(Clone, Debug, PartialEq)]
pub enum VectorPrimitive {
    FilledCircle {
        center: LogicalPoint,
        radius: f32,
        color: Rgba,
    },
    FilledWedge {
        center: LogicalPoint,
        inner_radius: f32,
        outer_radius: f32,
        start_angle: f32,
        end_angle: f32,
        color: Rgba,
    },
    Text {
        origin: LogicalPoint,
        text: String,
        color: Rgba,
    },
}

/// Immutable original-vector scene. Native backends may rasterize it, but the
/// retained source never becomes a scaled copy of a previous bitmap.
#[derive(Clone, Debug, PartialEq)]
pub struct VectorScene {
    pub bounds: LogicalRect,
    pub generation: u64,
    pub primitives: Vec<VectorPrimitive>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InputOwner {
    Actionable(CellId),
    Protective,
    Exterior,
}

pub fn build_scene(layout: &LayoutSnapshot, generation: u64) -> VectorScene {
    let mut primitives = vec![VectorPrimitive::FilledCircle {
        center: layout.center,
        radius: layout.center_radius,
        color: Rgba(30, 33, 39, 238),
    }];
    for cell in &layout.cells {
        let color = if cell.actionable {
            Rgba(54, 61, 72, 244)
        } else {
            Rgba(42, 46, 54, 220)
        };
        match cell.shape {
            HitShape::Circle { center, radius } => primitives.push(VectorPrimitive::FilledCircle {
                center,
                radius,
                color,
            }),
            HitShape::Wedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
            } => primitives.push(VectorPrimitive::FilledWedge {
                center,
                inner_radius,
                outer_radius,
                start_angle,
                end_angle,
                color,
            }),
        }
        let origin = match cell.shape {
            HitShape::Circle { center, .. } | HitShape::Wedge { center, .. } => center,
        };
        primitives.push(VectorPrimitive::Text {
            origin,
            text: cell.label.clone(),
            color: Rgba(245, 245, 245, 255),
        });
    }
    VectorScene {
        bounds: layout.visual_extent,
        generation,
        primitives,
    }
}

/// Input ownership is geometric and deliberately independent from rendered alpha.
pub fn input_owner(layout: &LayoutSnapshot, point: LogicalPoint, ancestor: bool) -> InputOwner {
    if !inside_rect(layout.input_extent, point) || !inside_owned_background(layout, point) {
        return InputOwner::Exterior;
    }
    if ancestor {
        return InputOwner::Protective;
    }
    if let Some(cell) = layout
        .cells
        .iter()
        .find(|cell| shape_contains(&cell.shape, point))
    {
        return if cell.actionable {
            InputOwner::Actionable(cell.cell_id.clone())
        } else {
            InputOwner::Protective
        };
    }
    InputOwner::Protective
}

fn inside_owned_background(layout: &LayoutSnapshot, p: LogicalPoint) -> bool {
    // The host region is the circular wheel/tree background, not the rectangular
    // visual surface. Internal holes remain owned so clicks cannot leak through.
    let radius = (layout.input_extent.max.x - layout.input_extent.min.x)
        .max(layout.input_extent.max.y - layout.input_extent.min.y)
        * 0.5;
    let dx = p.x - layout.center.x;
    let dy = p.y - layout.center.y;
    dx * dx + dy * dy <= radius * radius
}

fn inside_rect(rect: LogicalRect, p: LogicalPoint) -> bool {
    p.x >= rect.min.x && p.x <= rect.max.x && p.y >= rect.min.y && p.y <= rect.max.y
}

fn shape_contains(shape: &HitShape, p: LogicalPoint) -> bool {
    match *shape {
        HitShape::Circle { center, radius } => {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            dx * dx + dy * dy <= radius * radius
        }
        HitShape::Wedge {
            center,
            inner_radius,
            outer_radius,
            start_angle,
            end_angle,
        } => {
            let dx = p.x - center.x;
            let dy = p.y - center.y;
            let radius = (dx * dx + dy * dy).sqrt();
            if radius < inner_radius || radius > outer_radius {
                return false;
            }
            let angle = dy.atan2(dx).rem_euclid(std::f32::consts::TAU);
            let start = start_angle.rem_euclid(std::f32::consts::TAU);
            let span = (end_angle - start_angle).rem_euclid(std::f32::consts::TAU);
            span == 0.0 || (angle - start).rem_euclid(std::f32::consts::TAU) < span
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::radial::geometry::{PhysicalPoint, PhysicalRect, ScaleFactor, layout_menu};
    use crate::radial::model::RadialDocument;

    fn layout() -> LayoutSnapshot {
        layout_menu(
            &RadialDocument::starter().menus[0],
            PhysicalPoint { x: 300.0, y: 300.0 },
            PhysicalRect {
                min: PhysicalPoint { x: 0.0, y: 0.0 },
                max: PhysicalPoint { x: 600.0, y: 600.0 },
            },
            ScaleFactor::new(1.0).unwrap(),
            0.5,
        )
        .unwrap()
    }
    #[test]
    fn transparent_pixels_inside_a_circle_still_belong_to_the_cell() {
        let l = layout();
        let c = &l.cells[0];
        let HitShape::Circle { center, radius } = c.shape else {
            panic!()
        };
        assert_eq!(
            input_owner(
                &l,
                LogicalPoint {
                    x: center.x + radius * 0.9,
                    y: center.y
                },
                false
            ),
            InputOwner::Actionable(c.cell_id.clone())
        );
    }
    #[test]
    fn internal_gap_is_protective_and_true_exterior_is_not_owned() {
        let l = layout();
        assert_eq!(input_owner(&l, l.center, false), InputOwner::Protective);
        assert_eq!(
            input_owner(
                &l,
                LogicalPoint {
                    x: l.input_extent.max.x + 1.0,
                    y: l.center.y
                },
                false
            ),
            InputOwner::Exterior
        );
    }
    #[test]
    fn ancestor_is_inert_but_protective() {
        let l = layout();
        let HitShape::Circle { center, .. } = l.cells[0].shape else {
            panic!()
        };
        assert_eq!(input_owner(&l, center, true), InputOwner::Protective);
    }
}
