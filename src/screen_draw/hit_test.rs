use super::{
    AnnotationKind, AnnotationObject, ArrowAnnotation, DesktopPoint, DesktopRect, LineAnnotation,
    ShapeAnnotation, Stroke, TextAnnotation,
};

/// Returns whether an eraser sample intersects an annotation. `tolerance` is
/// the eraser radius in physical desktop pixels; the object's own stroke width
/// is included automatically.
pub fn annotation_hit_test(object: &AnnotationObject, point: DesktopPoint, tolerance: f32) -> bool {
    kind_hit_test(&object.kind, point, tolerance)
}

pub fn kind_hit_test(kind: &AnnotationKind, point: DesktopPoint, tolerance: f32) -> bool {
    match kind {
        AnnotationKind::Pen(stroke) | AnnotationKind::Highlighter(stroke) => {
            stroke_hit_test(stroke, point, tolerance)
        }
        AnnotationKind::Line(line) => line_hit_test(line, point, tolerance),
        AnnotationKind::Arrow(arrow) => arrow_hit_test(arrow, point, tolerance),
        AnnotationKind::Rectangle(shape) => rectangle_hit_test(shape, point, tolerance),
        AnnotationKind::Ellipse(shape) => ellipse_hit_test(shape, point, tolerance),
        AnnotationKind::Text(text) => text_hit_test(text, point, tolerance),
    }
}

pub fn stroke_hit_test(stroke: &Stroke, point: DesktopPoint, tolerance: f32) -> bool {
    let radius = hit_radius(stroke.thickness, tolerance);
    match stroke.points.as_slice() {
        [] => false,
        [only] => point_distance_squared(only.position, point) <= radius * radius,
        points => points.windows(2).any(|pair| {
            point_segment_distance_squared(point, pair[0].position, pair[1].position)
                <= radius * radius
        }),
    }
}

fn line_hit_test(line: &LineAnnotation, point: DesktopPoint, tolerance: f32) -> bool {
    let radius = hit_radius(line.style.thickness, tolerance);
    point_segment_distance_squared(point, line.from, line.to) <= radius * radius
}

fn arrow_hit_test(arrow: &ArrowAnnotation, point: DesktopPoint, tolerance: f32) -> bool {
    if line_hit_test(
        &LineAnnotation {
            from: arrow.from,
            to: arrow.to,
            style: arrow.style,
        },
        point,
        tolerance,
    ) {
        return true;
    }

    let dx = f64::from(arrow.to.x) - f64::from(arrow.from.x);
    let dy = f64::from(arrow.to.y) - f64::from(arrow.from.y);
    let length = dx.hypot(dy);
    if length == 0.0 {
        return false;
    }
    let head_length = (f64::from(arrow.style.thickness.max(0.0)) * 4.0).clamp(8.0, 24.0);
    let unit_x = dx / length;
    let unit_y = dy / length;
    let back_x = f64::from(arrow.to.x) - unit_x * head_length;
    let back_y = f64::from(arrow.to.y) - unit_y * head_length;
    let wing = head_length * 0.55;
    let left = (back_x - unit_y * wing, back_y + unit_x * wing);
    let right = (back_x + unit_y * wing, back_y - unit_x * wing);
    let radius = hit_radius(arrow.style.thickness, tolerance);

    point_segment_distance_squared_f64(point, arrow.to, left) <= radius * radius
        || point_segment_distance_squared_f64(point, arrow.to, right) <= radius * radius
}

fn rectangle_hit_test(shape: &ShapeAnnotation, point: DesktopPoint, tolerance: f32) -> bool {
    let (left, top, right, bottom) = shape_edges(shape);
    let px = f64::from(point.x);
    let py = f64::from(point.y);
    let radius = hit_radius(shape.style.thickness, tolerance);
    // Treat the interior as selectable too. This makes object erasure reliable
    // for both outline-only rectangles and later filled rectangle rendering.
    px >= left - radius && px <= right + radius && py >= top - radius && py <= bottom + radius
}

fn ellipse_hit_test(shape: &ShapeAnnotation, point: DesktopPoint, tolerance: f32) -> bool {
    let (left, top, right, bottom) = shape_edges(shape);
    let rx = (right - left) / 2.0;
    let ry = (bottom - top) / 2.0;
    let radius = hit_radius(shape.style.thickness, tolerance);
    if rx == 0.0 || ry == 0.0 {
        return point_segment_distance_squared(point, shape.from, shape.to) <= radius * radius;
    }
    let cx = left + rx;
    let cy = top + ry;
    let dx = f64::from(point.x) - cx;
    let dy = f64::from(point.y) - cy;
    let normalized = (dx / rx).powi(2) + (dy / ry).powi(2);
    if normalized <= 1.0 {
        return true;
    }

    // Convert the radial tolerance into a conservative normalized expansion.
    let expanded_rx = rx + radius;
    let expanded_ry = ry + radius;
    (dx / expanded_rx).powi(2) + (dy / expanded_ry).powi(2) <= 1.0
}

fn text_hit_test(text: &TextAnnotation, point: DesktopPoint, tolerance: f32) -> bool {
    expanded_rect_contains(text.bounds, point, tolerance.max(0.0) as f64)
}

fn expanded_rect_contains(rect: DesktopRect, point: DesktopPoint, amount: f64) -> bool {
    let x = f64::from(point.x);
    let y = f64::from(point.y);
    x >= f64::from(rect.x) - amount
        && x <= rect.right() as f64 + amount
        && y >= f64::from(rect.y) - amount
        && y <= rect.bottom() as f64 + amount
}

fn shape_edges(shape: &ShapeAnnotation) -> (f64, f64, f64, f64) {
    (
        f64::from(shape.from.x.min(shape.to.x)),
        f64::from(shape.from.y.min(shape.to.y)),
        f64::from(shape.from.x.max(shape.to.x)),
        f64::from(shape.from.y.max(shape.to.y)),
    )
}

fn hit_radius(thickness: f32, tolerance: f32) -> f64 {
    f64::from(thickness.max(0.0) / 2.0 + tolerance.max(0.0))
}

fn point_distance_squared(a: DesktopPoint, b: DesktopPoint) -> f64 {
    let dx = f64::from(a.x) - f64::from(b.x);
    let dy = f64::from(a.y) - f64::from(b.y);
    dx * dx + dy * dy
}

fn point_segment_distance_squared(
    point: DesktopPoint,
    from: DesktopPoint,
    to: DesktopPoint,
) -> f64 {
    point_segment_distance_squared_f64(point, from, (f64::from(to.x), f64::from(to.y)))
}

fn point_segment_distance_squared_f64(
    point: DesktopPoint,
    from: DesktopPoint,
    to: (f64, f64),
) -> f64 {
    let from_x = f64::from(from.x);
    let from_y = f64::from(from.y);
    let dx = to.0 - from_x;
    let dy = to.1 - from_y;
    if dx == 0.0 && dy == 0.0 {
        return point_distance_squared(point, from);
    }
    let t = (((f64::from(point.x) - from_x) * dx + (f64::from(point.y) - from_y) * dy)
        / (dx * dx + dy * dy))
        .clamp(0.0, 1.0);
    let nearest_x = from_x + t * dx;
    let nearest_y = from_y + t * dy;
    let point_dx = f64::from(point.x) - nearest_x;
    let point_dy = f64::from(point.y) - nearest_y;
    point_dx * point_dx + point_dy * point_dy
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_draw::{AnnotationId, RgbaColor, ShapeStyle, StrokePoint};

    fn style(thickness: f32) -> ShapeStyle {
        ShapeStyle {
            color: RgbaColor::RED,
            thickness,
        }
    }

    fn object(kind: AnnotationKind) -> AnnotationObject {
        AnnotationObject {
            id: AnnotationId(1),
            kind,
        }
    }

    #[test]
    fn thick_and_single_point_strokes_are_hit_in_signed_space() {
        let stroke = Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(-100, 10)),
                StrokePoint::mouse(DesktopPoint::new(-50, 10)),
            ],
            color: RgbaColor::RED,
            thickness: 10.0,
        };
        assert!(annotation_hit_test(
            &object(AnnotationKind::Pen(stroke)),
            DesktopPoint::new(-75, 15),
            0.0
        ));

        let dot = Stroke {
            points: vec![StrokePoint::mouse(DesktopPoint::new(-20, -20))],
            color: RgbaColor::RED,
            thickness: 6.0,
        };
        assert!(stroke_hit_test(&dot, DesktopPoint::new(-17, -20), 0.0));
        assert!(!stroke_hit_test(&dot, DesktopPoint::new(-10, -20), 0.0));
    }

    #[test]
    fn lines_arrows_and_reversed_shapes_are_hit() {
        let line = object(AnnotationKind::Line(LineAnnotation {
            from: DesktopPoint::new(-20, 0),
            to: DesktopPoint::new(20, 0),
            style: style(2.0),
        }));
        assert!(annotation_hit_test(&line, DesktopPoint::new(0, 3), 2.0));

        let arrow = object(AnnotationKind::Arrow(ArrowAnnotation {
            from: DesktopPoint::new(0, 0),
            to: DesktopPoint::new(30, 0),
            style: style(2.0),
        }));
        assert!(annotation_hit_test(&arrow, DesktopPoint::new(24, 5), 1.0));

        let rectangle = object(AnnotationKind::Rectangle(ShapeAnnotation {
            from: DesktopPoint::new(20, 20),
            to: DesktopPoint::new(-20, -10),
            style: style(2.0),
        }));
        assert!(annotation_hit_test(
            &rectangle,
            DesktopPoint::new(0, 0),
            0.0
        ));

        let ellipse = object(AnnotationKind::Ellipse(ShapeAnnotation {
            from: DesktopPoint::new(-20, -10),
            to: DesktopPoint::new(20, 10),
            style: style(2.0),
        }));
        assert!(annotation_hit_test(&ellipse, DesktopPoint::new(0, 0), 0.0));
        assert!(!annotation_hit_test(
            &ellipse,
            DesktopPoint::new(30, 20),
            0.0
        ));
    }

    #[test]
    fn text_uses_layout_bounds_and_highlighter_uses_stroke_width() {
        let text = object(AnnotationKind::Text(TextAnnotation {
            text: "two\nlines".into(),
            bounds: DesktopRect::new(-200, 50, 100, 40),
            color: RgbaColor::WHITE,
            font_size: 16.0,
        }));
        assert!(annotation_hit_test(&text, DesktopPoint::new(-150, 70), 0.0));

        let highlighter = object(AnnotationKind::Highlighter(Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(0, 0)),
                StrokePoint::mouse(DesktopPoint::new(100, 0)),
            ],
            color: RgbaColor::rgba(255, 255, 0, 90),
            thickness: 20.0,
        }));
        assert!(annotation_hit_test(
            &highlighter,
            DesktopPoint::new(50, 10),
            0.0
        ));
    }
}
