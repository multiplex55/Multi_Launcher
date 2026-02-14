use crate::draw::model::{CanvasModel, Color, DrawObject, Geometry};
use crate::draw::overlay::OverlayWindow;

pub fn render_canvas_into_overlay(window: &mut OverlayWindow, canvas: &CanvasModel) {
    window.with_bitmap_mut(|pixels, width, height| {
        render_canvas_to_pixels(canvas, pixels, width, height);
    });
}

pub fn render_canvas_to_pixels(canvas: &CanvasModel, pixels: &mut [u8], width: u32, height: u32) {
    clear_pixels(pixels);
    for object in &canvas.objects {
        render_draw_object(object, pixels, width, height);
    }
}

fn clear_pixels(pixels: &mut [u8]) {
    for px in pixels.chunks_exact_mut(4) {
        px.copy_from_slice(&[0, 0, 0, 0]);
    }
}

fn render_draw_object(object: &DrawObject, pixels: &mut [u8], width: u32, height: u32) {
    let color = match object.geometry {
        Geometry::Eraser { .. } => Color::rgba(0, 0, 0, 0),
        _ => object.style.stroke.color,
    };
    let stroke_width = object.style.stroke.width.max(1);

    match &object.geometry {
        Geometry::Pen { points } | Geometry::Eraser { points } => {
            draw_polyline(points, color, stroke_width, pixels, width, height)
        }
        Geometry::Line { start, end } => {
            draw_segment(*start, *end, color, stroke_width, pixels, width, height)
        }
        Geometry::Rect { start, end } => {
            draw_rect(*start, *end, color, stroke_width, pixels, width, height)
        }
        Geometry::Ellipse { start, end } => {
            draw_ellipse(*start, *end, color, stroke_width, pixels, width, height)
        }
    }
}

fn draw_polyline(
    points: &[(i32, i32)],
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    if points.is_empty() {
        return;
    }
    if points.len() == 1 {
        draw_brush(points[0], color, stroke_width, pixels, width, height);
        return;
    }

    for segment in points.windows(2) {
        draw_segment(
            segment[0],
            segment[1],
            color,
            stroke_width,
            pixels,
            width,
            height,
        );
    }
}

fn draw_rect(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    let (x0, x1) = if start.0 <= end.0 {
        (start.0, end.0)
    } else {
        (end.0, start.0)
    };
    let (y0, y1) = if start.1 <= end.1 {
        (start.1, end.1)
    } else {
        (end.1, start.1)
    };

    draw_segment(
        (x0, y0),
        (x1, y0),
        color,
        stroke_width,
        pixels,
        width,
        height,
    );
    draw_segment(
        (x1, y0),
        (x1, y1),
        color,
        stroke_width,
        pixels,
        width,
        height,
    );
    draw_segment(
        (x1, y1),
        (x0, y1),
        color,
        stroke_width,
        pixels,
        width,
        height,
    );
    draw_segment(
        (x0, y1),
        (x0, y0),
        color,
        stroke_width,
        pixels,
        width,
        height,
    );
}

fn draw_ellipse(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    let min_x = start.0.min(end.0);
    let max_x = start.0.max(end.0);
    let min_y = start.1.min(end.1);
    let max_y = start.1.max(end.1);

    let rx = ((max_x - min_x).max(1) as f32) * 0.5;
    let ry = ((max_y - min_y).max(1) as f32) * 0.5;
    let cx = (min_x + max_x) as f32 * 0.5;
    let cy = (min_y + max_y) as f32 * 0.5;

    let circumference = std::f32::consts::TAU * rx.max(ry);
    let steps = circumference.max(12.0) as usize;

    for step in 0..=steps {
        let t = (step as f32 / steps as f32) * std::f32::consts::TAU;
        let x = (cx + rx * t.cos()).round() as i32;
        let y = (cy + ry * t.sin()).round() as i32;
        draw_brush((x, y), color, stroke_width, pixels, width, height);
    }
}

fn draw_segment(
    start: (i32, i32),
    end: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    let mut x0 = start.0;
    let mut y0 = start.1;
    let x1 = end.0;
    let y1 = end.1;

    let dx = (x1 - x0).abs();
    let sx = if x0 < x1 { 1 } else { -1 };
    let dy = -(y1 - y0).abs();
    let sy = if y0 < y1 { 1 } else { -1 };
    let mut err = dx + dy;

    loop {
        draw_brush((x0, y0), color, stroke_width, pixels, width, height);
        if x0 == x1 && y0 == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x0 += sx;
        }
        if e2 <= dx {
            err += dx;
            y0 += sy;
        }
    }
}

fn draw_brush(
    center: (i32, i32),
    color: Color,
    stroke_width: u32,
    pixels: &mut [u8],
    width: u32,
    height: u32,
) {
    let radius = (stroke_width.saturating_sub(1) / 2) as i32;
    for y in (center.1 - radius)..=(center.1 + radius) {
        for x in (center.0 - radius)..=(center.0 + radius) {
            let dx = x - center.0;
            let dy = y - center.1;
            if dx * dx + dy * dy <= radius * radius {
                set_pixel(pixels, width, height, x, y, color);
            }
        }
    }
}

fn set_pixel(pixels: &mut [u8], width: u32, height: u32, x: i32, y: i32, color: Color) {
    if x < 0 || y < 0 || x >= width as i32 || y >= height as i32 {
        return;
    }

    let idx = ((y as u32 * width + x as u32) * 4) as usize;
    if idx + 3 >= pixels.len() {
        return;
    }

    // Windows DIB uses BGRA byte order.
    pixels[idx] = color.b;
    pixels[idx + 1] = color.g;
    pixels[idx + 2] = color.r;
    pixels[idx + 3] = color.a;
}

#[cfg(test)]
mod tests {
    use super::render_canvas_to_pixels;
    use crate::draw::{
        input::{DrawInputState, PointerModifiers},
        model::{CanvasModel, Color, DrawObject, Geometry, ObjectStyle, StrokeStyle, Tool},
    };

    fn object(tool: Tool, geometry: Geometry) -> DrawObject {
        DrawObject {
            tool,
            style: ObjectStyle {
                stroke: StrokeStyle {
                    width: 3,
                    color: Color::rgba(255, 255, 255, 255),
                },
                fill: None,
            },
            geometry,
        }
    }

    fn changed_pixels(canvas: CanvasModel) -> usize {
        let mut pixels = vec![0u8; 64 * 64 * 4];
        render_canvas_to_pixels(&canvas, &mut pixels, 64, 64);
        pixels.chunks_exact(4).filter(|px| px[3] != 0).count()
    }

    #[test]
    fn rasterizes_primitives_to_non_empty_pixels() {
        let pen_canvas = CanvasModel {
            objects: vec![object(
                Tool::Pen,
                Geometry::Pen {
                    points: vec![(2, 2), (8, 8), (14, 3)],
                },
            )],
        };
        assert!(changed_pixels(pen_canvas) > 0);

        let line_canvas = CanvasModel {
            objects: vec![object(
                Tool::Line,
                Geometry::Line {
                    start: (4, 4),
                    end: (30, 10),
                },
            )],
        };
        assert!(changed_pixels(line_canvas) > 0);

        let rect_canvas = CanvasModel {
            objects: vec![object(
                Tool::Rect,
                Geometry::Rect {
                    start: (8, 8),
                    end: (25, 22),
                },
            )],
        };
        assert!(changed_pixels(rect_canvas) > 0);

        let ellipse_canvas = CanvasModel {
            objects: vec![object(
                Tool::Ellipse,
                Geometry::Ellipse {
                    start: (12, 12),
                    end: (36, 28),
                },
            )],
        };
        assert!(changed_pixels(ellipse_canvas) > 0);
    }

    #[test]
    fn eraser_removes_pixels() {
        let base = object(
            Tool::Pen,
            Geometry::Pen {
                points: vec![(2, 2), (30, 30)],
            },
        );
        let eraser = object(
            Tool::Eraser,
            Geometry::Eraser {
                points: vec![(16, 16), (20, 20)],
            },
        );

        let without_eraser = changed_pixels(CanvasModel {
            objects: vec![base.clone()],
        });
        let with_eraser = changed_pixels(CanvasModel {
            objects: vec![base, eraser],
        });

        assert!(with_eraser < without_eraser);
    }

    #[test]
    fn undo_redo_rerender_is_deterministic() {
        let mut input = DrawInputState::new(Tool::Line, ObjectStyle::default());
        let _ = input.handle_left_down((5, 5), PointerModifiers::default());
        input.handle_left_up((18, 18));
        let _ = input.handle_left_down((10, 30), PointerModifiers::default());
        input.handle_left_up((42, 30));

        let mut before = vec![0u8; 64 * 64 * 4];
        render_canvas_to_pixels(&input.history().canvas(), &mut before, 64, 64);

        let _ = input.handle_key_event(crate::draw::keyboard_hook::KeyEvent {
            key: crate::draw::keyboard_hook::KeyCode::U,
            modifiers: Default::default(),
        });
        let _ = input.handle_key_event(crate::draw::keyboard_hook::KeyEvent {
            key: crate::draw::keyboard_hook::KeyCode::R,
            modifiers: crate::draw::keyboard_hook::KeyModifiers {
                ctrl: true,
                shift: false,
            },
        });

        let mut after = vec![0u8; 64 * 64 * 4];
        render_canvas_to_pixels(&input.history().canvas(), &mut after, 64, 64);

        assert_eq!(before, after);
    }

    #[test]
    fn rendering_is_bounds_safe_on_monitor_edges() {
        let canvas = CanvasModel {
            objects: vec![
                object(
                    Tool::Line,
                    Geometry::Line {
                        start: (-1000, -1000),
                        end: (1000, 1000),
                    },
                ),
                object(
                    Tool::Rect,
                    Geometry::Rect {
                        start: (-500, 2),
                        end: (4, 500),
                    },
                ),
                object(
                    Tool::Ellipse,
                    Geometry::Ellipse {
                        start: (5, -300),
                        end: (500, 5),
                    },
                ),
            ],
        };

        let mut pixels = vec![0u8; 8 * 8 * 4];
        render_canvas_to_pixels(&canvas, &mut pixels, 8, 8);

        assert_eq!(pixels.len(), 8 * 8 * 4);
    }
}
