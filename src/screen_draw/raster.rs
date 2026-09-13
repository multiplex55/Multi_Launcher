//! Deterministic raster composition for Screen Draw rebuilds and exports.
//!
//! Geometry remains in signed desktop coordinates until it is translated into
//! the caller's small output buffer. On Windows, permanent Pen strokes are
//! rasterized into an offscreen GDI DIB by the exact segment helper used by the
//! Mouse Gesture trail. Other tools use the shared software annotation kernel.

use image::{Rgba, RgbaImage};

use crate::annotation::raster::{self as software, Color, Point, Rect};

use super::{
    AnnotationDocument, AnnotationKind, AnnotationObject, CanvasBackground, DesktopPoint,
    DesktopRect, RgbaColor, Stroke,
};

#[derive(Debug, Clone, Copy)]
pub enum RasterBackground<'a> {
    Transparent,
    Solid(RgbaColor),
    Frozen {
        image: &'a RgbaImage,
        bounds: DesktopRect,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterError {
    FrozenImageSizeMismatch,
    DimensionsTooLarge,
    #[cfg(windows)]
    GdiAllocationFailed,
}

/// Resolves a document background selection against its immutable snapshot.
pub fn selected_background<'a>(
    selection: CanvasBackground,
    frozen_image: &'a RgbaImage,
    frozen_bounds: DesktopRect,
) -> RasterBackground<'a> {
    match selection {
        CanvasBackground::FrozenDesktop => RasterBackground::Frozen {
            image: frozen_image,
            bounds: frozen_bounds,
        },
        CanvasBackground::White => RasterBackground::Solid(RgbaColor::WHITE),
        CanvasBackground::Black => RasterBackground::Solid(RgbaColor::BLACK),
        CanvasBackground::Solid(color) => RasterBackground::Solid(color),
    }
}

/// Rebuilds the requested output in-place without allocating a full-desktop copy.
///
/// `origin` is the signed desktop coordinate corresponding to output pixel
/// `(0, 0)`. Fading/active strokes may be supplied separately and are composited
/// after permanent objects. They intentionally use the software path; the GDI
/// parity requirement applies to committed and rebuilt normal Pen ink.
pub fn render_document_into(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    background: RasterBackground<'_>,
    document: &AnnotationDocument,
    transient_strokes: &[Stroke],
) -> Result<(), RasterError> {
    render_annotations_into(
        output,
        origin,
        background,
        document.objects(),
        document.annotations_visible(),
        transient_strokes,
    )
}

pub(crate) fn render_annotations_into(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    background: RasterBackground<'_>,
    objects: &[AnnotationObject],
    annotations_visible: bool,
    transient_strokes: &[Stroke],
) -> Result<(), RasterError> {
    paint_background(output, origin, background)?;
    let font = software::default_font_arc();
    if annotations_visible {
        for object in objects {
            render_kind(output, origin, &object.kind, font.as_ref())?;
        }
        for stroke in transient_strokes {
            render_software_stroke(output, origin, stroke, stroke.color);
        }
    }
    Ok(())
}

fn paint_background(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    background: RasterBackground<'_>,
) -> Result<(), RasterError> {
    match background {
        RasterBackground::Transparent => {
            for pixel in output.pixels_mut() {
                *pixel = Rgba([0, 0, 0, 0]);
            }
        }
        RasterBackground::Solid(color) => {
            for pixel in output.pixels_mut() {
                *pixel = Rgba(color.channels());
            }
        }
        RasterBackground::Frozen { image, bounds } => {
            if image.dimensions() != (bounds.width, bounds.height) {
                return Err(RasterError::FrozenImageSizeMismatch);
            }
            for pixel in output.pixels_mut() {
                *pixel = Rgba([0, 0, 0, 0]);
            }
            let output_bounds =
                DesktopRect::new(origin.x, origin.y, output.width(), output.height());
            let Some(overlap) = output_bounds.intersection(bounds) else {
                return Ok(());
            };
            let output_offset = output_bounds
                .desktop_to_local(overlap.origin())
                .expect("intersection begins inside output");
            let source_offset = bounds
                .desktop_to_local(overlap.origin())
                .expect("intersection begins inside source");
            for y in 0..overlap.height {
                for x in 0..overlap.width {
                    output.put_pixel(
                        output_offset.x + x,
                        output_offset.y + y,
                        *image.get_pixel(source_offset.x + x, source_offset.y + y),
                    );
                }
            }
        }
    }
    Ok(())
}

fn render_kind(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    kind: &AnnotationKind,
    font: Option<&(ab_glyph::FontArc, eframe::egui::FontTweak)>,
) -> Result<(), RasterError> {
    match kind {
        AnnotationKind::Pen(stroke) => render_pen_stroke(output, origin, stroke)?,
        AnnotationKind::Highlighter(stroke) => {
            render_software_stroke(output, origin, stroke, stroke.color)
        }
        AnnotationKind::Line(line) => software::draw_line(
            output,
            local_point(line.from, origin),
            local_point(line.to, origin),
            color(line.style.color),
            line.style.thickness,
        ),
        AnnotationKind::Arrow(arrow) => software::draw_arrow(
            output,
            local_point(arrow.from, origin),
            local_point(arrow.to, origin),
            color(arrow.style.color),
            arrow.style.thickness,
        ),
        AnnotationKind::Rectangle(shape) => software::draw_rect_outline(
            output,
            Rect::from_corners(
                local_point(shape.from, origin),
                local_point(shape.to, origin),
            ),
            color(shape.style.color),
            shape.style.thickness,
        ),
        AnnotationKind::Ellipse(shape) => software::draw_ellipse(
            output,
            Rect::from_corners(
                local_point(shape.from, origin),
                local_point(shape.to, origin),
            ),
            color(shape.style.color),
            shape.style.thickness,
        ),
        AnnotationKind::Text(text) => {
            if let Some((font, tweak)) = font {
                software::draw_text(
                    output,
                    font,
                    *tweak,
                    Point::new(
                        text.bounds.x as f32 - origin.x as f32,
                        text.bounds.y as f32 - origin.y as f32,
                    ),
                    &text.text,
                    color(text.color),
                    text.font_size,
                );
            }
        }
    }
    Ok(())
}

/// Renders one ephemeral authoring preview without inserting it into document
/// history. Callers normally use a small dirty-region image.
pub(crate) fn render_preview_into(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    preview: &AnnotationKind,
) -> Result<(), RasterError> {
    let font = software::default_font_arc();
    render_kind(output, origin, preview, font.as_ref())
}

/// Measures text with the same bundled proportional font and scale used by
/// [`render_document_into`]. The fallback remains deterministic if the bundled
/// font cannot be loaded.
pub(crate) fn measure_text(text: &str, size: f32) -> (u32, u32) {
    use ab_glyph::{Font, ScaleFont};

    let size = size.max(1.0);
    let lines: Vec<&str> = text.split('\n').collect();
    if let Some((font, tweak)) = software::default_font_arc() {
        let scaled = font.as_scaled(size * tweak.scale);
        let width = lines
            .iter()
            .map(|line| {
                line.chars()
                    .map(|character| scaled.h_advance(scaled.glyph_id(character)))
                    .sum::<f32>()
            })
            .fold(0.0_f32, f32::max)
            .ceil() as u32;
        let height = (lines.len().max(1) as f32 * scaled.height()).ceil() as u32;
        return (width.max(1), height.max(1));
    }
    let columns = lines
        .iter()
        .map(|line| line.chars().count())
        .max()
        .unwrap_or(0)
        .max(1);
    (
        (columns as f32 * size * 0.65).ceil().max(1.0) as u32,
        (lines.len().max(1) as f32 * size * 1.25).ceil().max(1.0) as u32,
    )
}

fn render_software_stroke(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    stroke: &Stroke,
    stroke_color: RgbaColor,
) {
    for segment in stroke.segments() {
        software::draw_line(
            output,
            local_point(segment.from, origin),
            local_point(segment.to, origin),
            color(stroke_color),
            stroke.thickness,
        );
    }
}

fn local_point(point: DesktopPoint, origin: DesktopPoint) -> Point {
    Point::new(
        point.x as f32 - origin.x as f32,
        point.y as f32 - origin.y as f32,
    )
}

fn color(color: RgbaColor) -> Color {
    Color(color.channels())
}

const PEN_MASK_TILE_SIZE: u32 = 256;

pub(crate) fn stroke_mask_tiles(output: DesktopRect, stroke: &Stroke) -> Vec<DesktopRect> {
    use std::collections::BTreeSet;

    if output.is_empty() || stroke.points.len() < 2 {
        return Vec::new();
    }
    let margin = (stroke.thickness.max(1.0) * 0.5).ceil() as i64 + 2;
    let mut occupied = BTreeSet::new();
    for segment in stroke.segments() {
        let from = segment.from;
        let to = segment.to;
        let dx = i64::from(to.x) - i64::from(from.x);
        let dy = i64::from(to.y) - i64::from(from.y);
        let sample_step = i64::from(PEN_MASK_TILE_SIZE / 2);
        let steps = ((dx.abs().max(dy.abs()) + sample_step - 1) / sample_step).max(1);
        // Consecutive samples are at most `sample_step` apart on either axis.
        // Expanding each sample by half that interval plus the stroke radius
        // conservatively covers the complete segment, including both
        // orthogonal neighbors when it crosses a tile corner. The GDI mask
        // still decides which pixels are actually blended.
        let coverage_margin = margin + (sample_step + 1) / 2;
        for step in 0..=steps {
            let x = i64::from(from.x) + dx * step / steps;
            let y = i64::from(from.y) + dy * step / steps;
            let left = (x - coverage_margin).max(i64::from(output.x));
            let top = (y - coverage_margin).max(i64::from(output.y));
            let right = (x + coverage_margin + 1).min(output.right());
            let bottom = (y + coverage_margin + 1).min(output.bottom());
            if right <= left || bottom <= top {
                continue;
            }
            let local_left = u32::try_from(left - i64::from(output.x)).unwrap_or(0);
            let local_top = u32::try_from(top - i64::from(output.y)).unwrap_or(0);
            let local_right = u32::try_from(right - i64::from(output.x)).unwrap_or(output.width);
            let local_bottom = u32::try_from(bottom - i64::from(output.y)).unwrap_or(output.height);
            for tile_y in local_top / PEN_MASK_TILE_SIZE
                ..=(local_bottom.saturating_sub(1) / PEN_MASK_TILE_SIZE)
            {
                for tile_x in local_left / PEN_MASK_TILE_SIZE
                    ..=(local_right.saturating_sub(1) / PEN_MASK_TILE_SIZE)
                {
                    occupied.insert((tile_x, tile_y));
                }
            }
        }
    }
    occupied
        .into_iter()
        .map(|(tile_x, tile_y)| {
            let local_x = tile_x * PEN_MASK_TILE_SIZE;
            let local_y = tile_y * PEN_MASK_TILE_SIZE;
            DesktopRect::new(
                output.x.saturating_add(local_x as i32),
                output.y.saturating_add(local_y as i32),
                PEN_MASK_TILE_SIZE.min(output.width - local_x),
                PEN_MASK_TILE_SIZE.min(output.height - local_y),
            )
        })
        .collect()
}

#[cfg(windows)]
fn render_pen_stroke(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    stroke: &Stroke,
) -> Result<(), RasterError> {
    let output_bounds = DesktopRect::new(origin.x, origin.y, output.width(), output.height());
    render_pen_stroke_in_tiles(
        output,
        origin,
        stroke,
        &stroke_mask_tiles(output_bounds, stroke),
    )
}

#[cfg(windows)]
fn render_pen_stroke_in_tiles(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    stroke: &Stroke,
    tiles: &[DesktopRect],
) -> Result<(), RasterError> {
    use std::{mem, ptr, slice};
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS,
        DeleteDC, DeleteObject, SelectObject,
    };

    if output.width() == 0 || output.height() == 0 || stroke.points.len() < 2 {
        return Ok(());
    }
    for tile in tiles.iter().copied() {
        let width = i32::try_from(tile.width).map_err(|_| RasterError::DimensionsTooLarge)?;
        let height = i32::try_from(tile.height).map_err(|_| RasterError::DimensionsTooLarge)?;
        let byte_len = (tile.width as usize)
            .checked_mul(tile.height as usize)
            .and_then(|pixels| pixels.checked_mul(4))
            .ok_or(RasterError::DimensionsTooLarge)?;
        unsafe {
            let dc = CreateCompatibleDC(None);
            if dc.0.is_null() {
                return Err(RasterError::GdiAllocationFailed);
            }
            let info = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: mem::size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: width,
                    biHeight: -height,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                bmiColors: [Default::default()],
            };
            let mut bits = ptr::null_mut();
            let dib = match CreateDIBSection(dc, &info, DIB_RGB_COLORS, &mut bits, None, 0) {
                Ok(dib) if !bits.is_null() => dib,
                _ => {
                    let _ = DeleteDC(dc);
                    return Err(RasterError::GdiAllocationFailed);
                }
            };
            let old_bitmap = SelectObject(dc, dib);
            ptr::write_bytes(bits as *mut u8, 0, byte_len);
            for segment in stroke.segments() {
                crate::platform::gdi_stroke::draw_solid_segment(
                    dc,
                    (segment.from.x as f32, segment.from.y as f32),
                    (segment.to.x as f32, segment.to.y as f32),
                    (tile.x, tile.y),
                    [255, 255, 255],
                    stroke.thickness,
                );
            }
            let mask = slice::from_raw_parts(bits as *const u8, byte_len);
            let stroke_color = color(stroke.color);
            for (index, pixel) in mask.chunks_exact(4).enumerate() {
                if pixel[0] != 0 || pixel[1] != 0 || pixel[2] != 0 {
                    let x = tile.x - origin.x + (index % tile.width as usize) as i32;
                    let y = tile.y - origin.y + (index / tile.width as usize) as i32;
                    software::blend_pixel(output, x, y, stroke_color);
                }
            }
            let _ = SelectObject(dc, old_bitmap);
            let _ = DeleteObject(dib);
            let _ = DeleteDC(dc);
        }
    }
    Ok(())
}

/// Non-Windows builds use a deterministic software fallback solely so domain
/// tests and tooling compile. Windows production rebuild/export never uses it.
#[cfg(not(windows))]
fn render_pen_stroke(
    output: &mut RgbaImage,
    origin: DesktopPoint,
    stroke: &Stroke,
) -> Result<(), RasterError> {
    render_software_stroke(output, origin, stroke, stroke.color);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::screen_draw::{
        ArrowAnnotation, LineAnnotation, ShapeAnnotation, ShapeStyle, StrokePoint, TextAnnotation,
    };

    fn colored_pixels(image: &RgbaImage) -> usize {
        image.pixels().filter(|pixel| pixel.0[3] != 0).count()
    }

    fn stroke(color: RgbaColor, from: DesktopPoint, to: DesktopPoint) -> Stroke {
        Stroke {
            points: vec![StrokePoint::mouse(from), StrokePoint::mouse(to)],
            color,
            thickness: 3.0,
        }
    }

    #[test]
    fn pen_masks_visit_only_occupied_fixed_size_tiles_on_an_8k_target() {
        let diagonal = stroke(
            RgbaColor::RED,
            DesktopPoint::new(-3830, -2150),
            DesktopPoint::new(3829, 2149),
        );
        let output = DesktopRect::new(-3840, -2160, 7680, 4320);
        let tiles = stroke_mask_tiles(output, &diagonal);
        assert!(!tiles.is_empty());
        assert!(
            tiles.iter().all(|tile| {
                tile.width <= PEN_MASK_TILE_SIZE && tile.height <= PEN_MASK_TILE_SIZE
            })
        );
        let visited_pixels: u64 = tiles
            .iter()
            .map(|tile| u64::from(tile.width) * u64::from(tile.height))
            .sum();
        let full_pixels = u64::from(output.width) * u64::from(output.height);
        assert!(visited_pixels < full_pixels / 4);
    }

    #[test]
    fn thick_diagonal_corner_crossing_includes_both_orthogonal_neighbor_tiles() {
        let mut diagonal = stroke(
            RgbaColor::RED,
            DesktopPoint::new(200, 200),
            DesktopPoint::new(312, 312),
        );
        diagonal.thickness = 80.0;
        let tiles = stroke_mask_tiles(DesktopRect::new(0, 0, 512, 512), &diagonal);
        for expected in [
            DesktopPoint::new(0, 0),
            DesktopPoint::new(256, 0),
            DesktopPoint::new(0, 256),
            DesktopPoint::new(256, 256),
        ] {
            assert!(
                tiles.iter().any(|tile| tile.origin() == expected),
                "missing tile at {expected:?}: {tiles:?}"
            );
        }
    }

    #[cfg(windows)]
    #[test]
    fn tiled_gdi_pen_mask_matches_a_single_output_sized_mask() {
        let origin = DesktopPoint::new(-300, -300);
        let bounds = DesktopRect::new(origin.x, origin.y, 600, 600);
        let stroke = Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(-270, -250)),
                StrokePoint::mouse(DesktopPoint::new(-40, 10)),
                StrokePoint::mouse(DesktopPoint::new(270, 260)),
            ],
            color: RgbaColor::rgba(220, 30, 60, 173),
            thickness: 37.0,
        };
        let mut tiled = RgbaImage::new(bounds.width, bounds.height);
        let mut reference = RgbaImage::new(bounds.width, bounds.height);
        let tiles = stroke_mask_tiles(bounds, &stroke);
        render_pen_stroke_in_tiles(&mut tiled, origin, &stroke, &tiles).unwrap();
        render_pen_stroke_in_tiles(&mut reference, origin, &stroke, &[bounds]).unwrap();
        assert_eq!(tiled, reference);
    }

    #[test]
    fn frozen_background_crops_with_negative_origins_and_transparent_gaps() {
        let frozen = RgbaImage::from_fn(4, 3, |x, y| Rgba([x as u8, y as u8, 9, 255]));
        let mut output = RgbaImage::new(4, 3);
        render_document_into(
            &mut output,
            DesktopPoint::new(-12, -6),
            RasterBackground::Frozen {
                image: &frozen,
                bounds: DesktopRect::new(-10, -5, 4, 3),
            },
            &AnnotationDocument::default(),
            &[],
        )
        .unwrap();
        assert_eq!(output.get_pixel(0, 0).0, [0, 0, 0, 0]);
        assert_eq!(output.get_pixel(2, 1).0, [0, 0, 9, 255]);
        assert_eq!(output.get_pixel(3, 2).0, [1, 1, 9, 255]);
    }

    #[test]
    fn highlighter_alpha_blends_and_clips_outside_small_target() {
        let mut document = AnnotationDocument::default();
        document
            .commit(AnnotationKind::Highlighter(stroke(
                RgbaColor::rgba(255, 255, 0, 96),
                DesktopPoint::new(-103, 51),
                DesktopPoint::new(-95, 51),
            )))
            .unwrap();
        let mut output = RgbaImage::new(5, 3);
        render_document_into(
            &mut output,
            DesktopPoint::new(-100, 50),
            RasterBackground::Solid(RgbaColor::BLACK),
            &document,
            &[],
        )
        .unwrap();
        assert!(
            output
                .pixels()
                .any(|pixel| pixel.0[0] > 0 && pixel.0[0] < 255)
        );
        assert!(output.pixels().all(|pixel| pixel.0[3] == 255));
    }

    #[test]
    fn pen_and_transient_strokes_render_without_full_desktop_buffers() {
        let mut document = AnnotationDocument::default();
        document
            .commit(AnnotationKind::Pen(stroke(
                RgbaColor::RED,
                DesktopPoint::new(1000, -2000),
                DesktopPoint::new(1007, -1993),
            )))
            .unwrap();
        let transient = stroke(
            RgbaColor::rgba(0, 255, 0, 128),
            DesktopPoint::new(1000, -1993),
            DesktopPoint::new(1007, -2000),
        );
        let mut output = RgbaImage::new(8, 8);
        render_document_into(
            &mut output,
            DesktopPoint::new(1000, -2000),
            RasterBackground::Transparent,
            &document,
            &[transient],
        )
        .unwrap();
        assert!(colored_pixels(&output) > 8);
        assert!(
            output
                .pixels()
                .any(|pixel| pixel.0[0] > 200 && pixel.0[1] < 40),
            "the normal Pen's GDI footprint must be present independently"
        );
        assert!(
            output
                .pixels()
                .any(|pixel| pixel.0[1] > 40 && pixel.0[0] < 200),
            "the optional transient software stroke must also be present"
        );
    }

    #[test]
    fn every_non_freehand_primitive_renders() {
        let style = ShapeStyle {
            color: RgbaColor::WHITE,
            thickness: 2.0,
        };
        let mut document = AnnotationDocument::default();
        document
            .commit(AnnotationKind::Line(LineAnnotation {
                from: DesktopPoint::new(1, 1),
                to: DesktopPoint::new(8, 1),
                style,
            }))
            .unwrap();
        document
            .commit(AnnotationKind::Arrow(ArrowAnnotation {
                from: DesktopPoint::new(1, 4),
                to: DesktopPoint::new(10, 4),
                style,
            }))
            .unwrap();
        document
            .commit(AnnotationKind::Rectangle(ShapeAnnotation {
                from: DesktopPoint::new(1, 7),
                to: DesktopPoint::new(8, 13),
                style,
            }))
            .unwrap();
        document
            .commit(AnnotationKind::Ellipse(ShapeAnnotation {
                from: DesktopPoint::new(10, 7),
                to: DesktopPoint::new(18, 13),
                style,
            }))
            .unwrap();
        document
            .commit(AnnotationKind::Text(TextAnnotation {
                text: "A\nB".into(),
                bounds: DesktopRect::new(20, 0, 12, 28),
                color: RgbaColor::WHITE,
                font_size: 10.0,
            }))
            .unwrap();
        let mut output = RgbaImage::new(34, 30);
        render_document_into(
            &mut output,
            DesktopPoint::new(0, 0),
            RasterBackground::Transparent,
            &document,
            &[],
        )
        .unwrap();
        assert!(colored_pixels(&output) > 50);
        assert!(
            output
                .enumerate_pixels()
                .any(|(x, y, pixel)| x >= 20 && y >= 10 && pixel.0[3] != 0)
        );
    }

    #[test]
    fn frozen_background_dimension_mismatch_is_rejected() {
        let frozen = RgbaImage::new(2, 2);
        let mut output = RgbaImage::new(1, 1);
        assert_eq!(
            render_document_into(
                &mut output,
                DesktopPoint::new(0, 0),
                RasterBackground::Frozen {
                    image: &frozen,
                    bounds: DesktopRect::new(0, 0, 3, 2)
                },
                &AnnotationDocument::default(),
                &[],
            ),
            Err(RasterError::FrozenImageSizeMismatch)
        );
    }

    #[test]
    fn text_measurement_is_deterministic_and_multiline_height_grows() {
        let first = measure_text("Wide text", 24.0);
        let second = measure_text("Wide text", 24.0);
        let multiline = measure_text("Wide\ntext", 24.0);
        assert_eq!(first, second);
        assert!(first.0 > 1 && first.1 > 1);
        assert!(multiline.1 > first.1);
    }

    #[test]
    fn visibility_hides_permanent_and_transient_annotations_without_deleting_them() {
        let mut document = AnnotationDocument::default();
        document
            .commit(AnnotationKind::Line(crate::screen_draw::LineAnnotation {
                from: DesktopPoint::new(0, 0),
                to: DesktopPoint::new(3, 3),
                style: crate::screen_draw::ShapeStyle {
                    color: RgbaColor::RED,
                    thickness: 2.0,
                },
            }))
            .unwrap();
        document.set_annotations_visible(false);
        let mut transient = Stroke::new(RgbaColor::WHITE, 2.0);
        transient.push_mouse_point(DesktopPoint::new(0, 3));
        transient.push_mouse_point(DesktopPoint::new(3, 0));
        let mut output = RgbaImage::new(4, 4);
        render_document_into(
            &mut output,
            DesktopPoint::new(0, 0),
            RasterBackground::Transparent,
            &document,
            &[transient],
        )
        .unwrap();
        assert!(output.pixels().all(|pixel| pixel.0 == [0, 0, 0, 0]));
        assert_eq!(document.objects().len(), 1);
        assert!(document.can_undo());
    }

    #[test]
    fn software_stroke_raster_does_not_bridge_a_subpath_break() {
        let stroke = Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(5, 10)),
                StrokePoint::mouse(DesktopPoint::new(20, 10)),
                StrokePoint::mouse_break(DesktopPoint::new(80, 10)),
                StrokePoint::mouse(DesktopPoint::new(95, 10)),
            ],
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        let mut output = RgbaImage::new(100, 20);
        render_software_stroke(&mut output, DesktopPoint::new(0, 0), &stroke, stroke.color);
        assert_ne!(output.get_pixel(10, 10).0[3], 0);
        assert_eq!(output.get_pixel(50, 10).0[3], 0);
        assert_ne!(output.get_pixel(90, 10).0[3], 0);
    }

    #[test]
    fn pen_tile_planning_does_not_visit_tiles_across_a_subpath_break() {
        let stroke = Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(20, 20)),
                StrokePoint::mouse(DesktopPoint::new(80, 20)),
                StrokePoint::mouse_break(DesktopPoint::new(1000, 20)),
                StrokePoint::mouse(DesktopPoint::new(1060, 20)),
            ],
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        let tiles = stroke_mask_tiles(DesktopRect::new(0, 0, 1280, 256), &stroke);
        assert!(tiles.iter().any(|tile| tile.x == 0));
        assert!(tiles.iter().any(|tile| tile.x == 768));
        assert!(tiles.iter().all(|tile| tile.x != 256 && tile.x != 512));
    }

    #[cfg(windows)]
    #[test]
    fn gdi_pen_raster_does_not_bridge_a_subpath_break() {
        let stroke = Stroke {
            points: vec![
                StrokePoint::mouse(DesktopPoint::new(5, 10)),
                StrokePoint::mouse(DesktopPoint::new(20, 10)),
                StrokePoint::mouse_break(DesktopPoint::new(80, 10)),
                StrokePoint::mouse(DesktopPoint::new(95, 10)),
            ],
            color: RgbaColor::RED,
            thickness: 2.0,
        };
        let mut output = RgbaImage::new(100, 20);
        render_pen_stroke(&mut output, DesktopPoint::new(0, 0), &stroke).unwrap();
        assert_ne!(output.get_pixel(10, 10).0[3], 0);
        assert_eq!(output.get_pixel(50, 10).0[3], 0);
        assert_ne!(output.get_pixel(90, 10).0[3], 0);
    }
}
