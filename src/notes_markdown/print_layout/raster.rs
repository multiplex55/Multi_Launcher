//! Bounded rendering of the shared print scene to opaque top-down RGBA pixels.
use super::{Drawable, FontId, MAX_ASSET_BYTES, PaginatedDocument, TextStyle};
use resvg::tiny_skia as sk;
use rustybuzz::ttf_parser::{Face, GlyphId, OutlineBuilder};
use std::{collections::HashMap, fmt};

const MAX_SIDE: u32 = 4096;
const MAX_PIXELS: u64 = 16_777_216;
const MAX_PIXEL_VISITS: u64 = 128_000_000;
const MAX_GLYPHS: usize = 1_000_000;
const MAX_DRAWABLES: usize = 120_000;
const MAX_CACHED_PATHS: usize = 4096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RasterError {
    Cancelled,
    InvalidScene(&'static str),
    Limit(&'static str),
}
impl fmt::Display for RasterError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("Page rasterization cancelled."),
            Self::InvalidScene(s) | Self::Limit(s) => f.write_str(s),
        }
    }
}
impl std::error::Error for RasterError {}
#[derive(Debug, Clone)]
pub struct RasterPage {
    pub width: u32,
    pub height: u32,
    pub scale: f32,
    pub rgba: Vec<u8>,
}
struct Outline {
    path: sk::PathBuilder,
    units: f32,
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to(x / self.units, -y / self.units);
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to(x / self.units, -y / self.units);
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        self.path.quad_to(
            x1 / self.units,
            -y1 / self.units,
            x / self.units,
            -y / self.units,
        );
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        self.path.cubic_to(
            x1 / self.units,
            -y1 / self.units,
            x2 / self.units,
            -y2 / self.units,
            x / self.units,
            -y / self.units,
        );
    }
    fn close(&mut self) {
        self.path.close();
    }
}
fn font_index(id: FontId) -> usize {
    match id {
        FontId::Body => 0,
        FontId::Code => 1,
        FontId::Emoji => 2,
    }
}
fn valid(values: &[f32]) -> bool {
    values
        .iter()
        .all(|v| v.is_finite() && v.abs() <= 1_000_000.0)
}
fn charge(
    work: &mut u64,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    w: u32,
    h: u32,
) -> Result<(), RasterError> {
    if !valid(&[x0, y0, x1, y1]) {
        return Err(RasterError::InvalidScene("Invalid raster bounds."));
    }
    let width = (x1.ceil().min(w as f32) - x0.floor().max(0.0)).max(0.0) as u64;
    let height = (y1.ceil().min(h as f32) - y0.floor().max(0.0)).max(0.0) as u64;
    *work = work
        .checked_add(width.saturating_mul(height))
        .filter(|n| *n <= MAX_PIXEL_VISITS)
        .ok_or(RasterError::Limit("Page raster work limit exceeded."))?;
    Ok(())
}
/// Rasterize one scene page at the requested pixels per point, capped to 4096
/// pixels per side. `scale` is the actual common point-to-pixel scale.
pub fn raster_page(
    scene: &PaginatedDocument,
    page_index: usize,
    requested_scale: f32,
    mut cancelled: impl FnMut() -> bool,
) -> Result<RasterPage, RasterError> {
    if cancelled() {
        return Err(RasterError::Cancelled);
    }
    let page = scene
        .pages
        .get(page_index)
        .ok_or(RasterError::InvalidScene("Invalid page index."))?;
    let (width, height) = (page.geometry.width, page.geometry.height);
    if !valid(&[width, height])
        || !requested_scale.is_finite()
        || width <= 0.0
        || height <= 0.0
        || requested_scale <= 0.0
    {
        return Err(RasterError::InvalidScene("Invalid page size or scale."));
    }
    let target = f64::from(requested_scale)
        .min(f64::from(MAX_SIDE) / f64::from(width))
        .min(f64::from(MAX_SIDE) / f64::from(height));
    let mut scale = target as f32;
    if f64::from(scale) > target {
        scale = f32::from_bits(scale.to_bits() - 1);
    }
    if !scale.is_finite() || scale <= 0.0 {
        return Err(RasterError::InvalidScene("Invalid raster scale."));
    }
    let w = (f64::from(width) * f64::from(scale)).ceil() as u32;
    let h = (f64::from(height) * f64::from(scale)).ceil() as u32;
    if w == 0 || h == 0 || w > MAX_SIDE || h > MAX_SIDE || u64::from(w) * u64::from(h) > MAX_PIXELS
    {
        return Err(RasterError::Limit("Page raster dimensions exceed limits."));
    }
    if page.drawables.len() > MAX_DRAWABLES {
        return Err(RasterError::Limit("Page drawable limit exceeded."));
    }
    let mut output =
        sk::Pixmap::new(w, h).ok_or(RasterError::Limit("Page raster allocation failed."))?;
    output.fill(sk::Color::WHITE);
    let faces = [FontId::Body, FontId::Code, FontId::Emoji].map(|id| Face::parse(id.bytes(), 0));
    let faces = [
        faces[0]
            .as_ref()
            .map_err(|_| RasterError::InvalidScene("Invalid font."))?,
        faces[1]
            .as_ref()
            .map_err(|_| RasterError::InvalidScene("Invalid font."))?,
        faces[2]
            .as_ref()
            .map_err(|_| RasterError::InvalidScene("Invalid font."))?,
    ];
    let mut paths: HashMap<(usize, u16), Option<sk::Path>> = HashMap::new();
    let mut images: HashMap<usize, sk::Pixmap> = HashMap::new();
    let mut glyph_count = 0usize;
    let mut run_count = 0usize;
    let mut asset_bytes = 0usize;
    let mut work = u64::from(w) * u64::from(h);
    let mut paint = sk::Paint::default();
    paint.set_color_rgba8(0, 0, 0, 255);
    paint.anti_alias = true;
    for item in &page.drawables {
        if cancelled() {
            return Err(RasterError::Cancelled);
        }
        match item {
            Drawable::Text {
                text,
                shape,
                baseline,
                style,
            } => {
                if !valid(&[baseline.x, baseline.y, shape.width]) {
                    return Err(RasterError::InvalidScene("Invalid text position."));
                }
                for run in &shape.runs {
                    if cancelled() {
                        return Err(RasterError::Cancelled);
                    }
                    run_count = run_count
                        .checked_add(1)
                        .filter(|n| *n <= MAX_DRAWABLES)
                        .ok_or(RasterError::Limit("Page text run limit exceeded."))?;
                    if !valid(&[run.size, run.origin.x, run.origin.y])
                        || run.size <= 0.0
                        || run.size > 48.0
                    {
                        return Err(RasterError::InvalidScene("Invalid font metrics."));
                    }
                    glyph_count = glyph_count
                        .checked_add(run.glyphs.len())
                        .filter(|n| *n <= MAX_GLYPHS)
                        .ok_or(RasterError::Limit("Page glyph limit exceeded."))?;
                    let idx = font_index(run.font);
                    let face = faces[idx];
                    for g in &run.glyphs {
                        if cancelled() {
                            return Err(RasterError::Cancelled);
                        }
                        if !valid(&[g.x, g.y, g.advance])
                            || text.get(g.cluster.clone()).is_none()
                            || g.cluster.is_empty()
                            || g.id >= face.number_of_glyphs()
                        {
                            return Err(RasterError::InvalidScene("Invalid glyph."));
                        }
                        let key = (idx, g.id);
                        let path = if let Some(path) = paths.get(&key) {
                            path.clone()
                        } else {
                            let mut outline = Outline {
                                path: sk::PathBuilder::new(),
                                units: f32::from(face.units_per_em()),
                            };
                            let path = face
                                .outline_glyph(GlyphId(g.id), &mut outline)
                                .and_then(|_| outline.path.finish());
                            if paths.len() < MAX_CACHED_PATHS {
                                paths.insert(key, path.clone());
                            }
                            path
                        };
                        if let Some(path) = path {
                            let shear = if style.italic {
                                TextStyle::ITALIC_SHEAR
                            } else {
                                0.0
                            };
                            let gx = baseline.x + run.origin.x + g.x;
                            let gy = baseline.y - run.origin.y - g.y;
                            let sx = scale * run.size;
                            let tx = scale * (gx + shear * (baseline.y - gy));
                            let ty = scale * gy;
                            if !valid(&[sx, tx, ty]) {
                                return Err(RasterError::InvalidScene("Invalid glyph transform."));
                            }
                            let t = sk::Transform::from_row(sx, 0.0, -shear * sx, sx, tx, ty);
                            let b = path.bounds();
                            let x0 = tx + sx * b.left() - shear * sx * b.bottom();
                            let x1 = tx + sx * b.right() - shear * sx * b.top();
                            let y0 = ty + sx * b.top();
                            let y1 = ty + sx * b.bottom();
                            let pad = if style.bold {
                                scale * TextStyle::BOLD_STROKE
                            } else {
                                0.0
                            } + 2.0;
                            charge(&mut work, x0 - pad, y0 - pad, x1 + pad, y1 + pad, w, h)?;
                            output.fill_path(&path, &paint, sk::FillRule::Winding, t, None);
                            if style.bold {
                                charge(&mut work, x0 - pad, y0 - pad, x1 + pad, y1 + pad, w, h)?;
                                let stroke = sk::Stroke {
                                    width: TextStyle::BOLD_STROKE / run.size,
                                    line_join: sk::LineJoin::Round,
                                    ..Default::default()
                                };
                                output.stroke_path(&path, &paint, &stroke, t, None);
                            }
                        }
                    }
                }
            }
            Drawable::Line { from, to, width } => {
                if !valid(&[from.x, from.y, to.x, to.y, *width]) || *width <= 0.0 || *width > 48.0 {
                    return Err(RasterError::InvalidScene("Invalid line."));
                }
                let (x0, y0, x1, y1) = (from.x * scale, from.y * scale, to.x * scale, to.y * scale);
                let pad = width * scale / 2.0 + 2.0;
                charge(
                    &mut work,
                    x0.min(x1) - pad,
                    y0.min(y1) - pad,
                    x0.max(x1) + pad,
                    y0.max(y1) + pad,
                    w,
                    h,
                )?;
                let mut path = sk::PathBuilder::new();
                path.move_to(x0, y0);
                path.line_to(x1, y1);
                if let Some(path) = path.finish() {
                    let stroke = sk::Stroke {
                        width: width * scale,
                        ..Default::default()
                    };
                    output.stroke_path(&path, &paint, &stroke, sk::Transform::identity(), None);
                }
            }
            Drawable::Image { asset, bounds } => {
                if !valid(&[bounds.x, bounds.y, bounds.width, bounds.height])
                    || bounds.width <= 0.0
                    || bounds.height <= 0.0
                {
                    return Err(RasterError::InvalidScene("Invalid image bounds."));
                }
                if !images.contains_key(asset) {
                    let image = scene
                        .assets
                        .get(*asset)
                        .ok_or(RasterError::InvalidScene("Missing image."))?;
                    let bytes = (image.width as usize)
                        .checked_mul(image.height as usize)
                        .and_then(|n| n.checked_mul(4))
                        .ok_or(RasterError::Limit("Image size overflow."))?;
                    asset_bytes = asset_bytes
                        .checked_add(bytes)
                        .filter(|n| *n <= MAX_ASSET_BYTES)
                        .ok_or(RasterError::Limit("Image cache limit exceeded."))?;
                    if image.width == 0 || image.height == 0 || image.rgba.len() != bytes {
                        return Err(RasterError::InvalidScene("Invalid image data."));
                    }
                    work = work
                        .checked_add((bytes / 4) as u64)
                        .filter(|n| *n <= MAX_PIXEL_VISITS)
                        .ok_or(RasterError::Limit("Page raster work limit exceeded."))?;
                    let mut rgba = Vec::with_capacity(bytes);
                    for chunk in image.rgba.chunks(16384) {
                        if cancelled() {
                            return Err(RasterError::Cancelled);
                        }
                        for pixel in chunk.chunks_exact(4) {
                            let a = u16::from(pixel[3]);
                            rgba.extend(
                                pixel[..3]
                                    .iter()
                                    .map(|c| ((u16::from(*c) * a + 127) / 255) as u8),
                            );
                            rgba.push(pixel[3]);
                        }
                    }
                    let size = sk::IntSize::from_wh(image.width, image.height)
                        .ok_or(RasterError::InvalidScene("Invalid image dimensions."))?;
                    images.insert(
                        *asset,
                        sk::Pixmap::from_vec(rgba, size)
                            .ok_or(RasterError::InvalidScene("Invalid image pixels."))?,
                    );
                }
                let image = &images[asset];
                let x = bounds.x * scale;
                let y = bounds.y * scale;
                let dw = bounds.width * scale;
                let dh = bounds.height * scale;
                charge(&mut work, x, y, x + dw, y + dh, w, h)?;
                let t = sk::Transform::from_row(
                    dw / image.width() as f32,
                    0.0,
                    0.0,
                    dh / image.height() as f32,
                    x,
                    y,
                );
                output.draw_pixmap(0, 0, image.as_ref(), &sk::PixmapPaint::default(), t, None);
            }
            Drawable::Link { .. } => {}
        }
    }
    if cancelled() {
        return Err(RasterError::Cancelled);
    }
    Ok(RasterPage {
        width: w,
        height: h,
        scale,
        rgba: output.take(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_markdown::print_layout::{PrintSettings, paginate};
    #[test]
    fn raster_is_opaque_bounded_and_shows_text() {
        let scene = paginate(
            "# Café\n\n**j** and *italic*",
            &PrintSettings::default(),
            "",
            || false,
        )
        .unwrap();
        let page = raster_page(&scene, 0, 1.0, || false).unwrap();
        assert_eq!((page.width, page.height), (612, 792));
        assert!(page.rgba.chunks_exact(4).all(|p| p[3] == 255));
        assert!(page.rgba.chunks_exact(4).any(|p| p[0] < 200));
        let capped = raster_page(&scene, 0, 100.0, || false).unwrap();
        assert!(capped.width <= 4096 && capped.height <= 4096 && capped.scale < 100.0);
        assert!(matches!(
            raster_page(&scene, 0, f32::NAN, || false),
            Err(RasterError::InvalidScene(_))
        ));
        assert!(matches!(
            raster_page(&scene, 0, 1.0, || true),
            Err(RasterError::Cancelled)
        ));
    }
    #[test]
    fn raster_image_orientation_alpha_and_invalid_data() {
        use crate::notes_markdown::print_layout::{PrintRaster, Rect};
        use std::sync::Arc;
        let mut scene = paginate("", &PrintSettings::default(), "", || false).unwrap();
        scene.assets.push(PrintRaster {
            width: 2,
            height: 2,
            rgba: Arc::from([255, 0, 0, 255, 0, 255, 0, 128, 0, 0, 255, 255, 0, 0, 0, 0]),
        });
        scene.pages[0].drawables.push(Drawable::Image {
            asset: 0,
            bounds: Rect {
                x: 10.0,
                y: 10.0,
                width: 20.0,
                height: 20.0,
            },
        });
        let page = raster_page(&scene, 0, 1.0, || false).unwrap();
        let pixel = |x: usize, y: usize| &page.rgba[(y * page.width as usize + x) * 4..][..4];
        assert_eq!(pixel(12, 12), &[255, 0, 0, 255]);
        assert_eq!(pixel(22, 12), &[127, 255, 127, 255]);
        assert_eq!(pixel(12, 22), &[0, 0, 255, 255]);
        assert_eq!(pixel(22, 22), &[255, 255, 255, 255]);
        scene.assets[0].width = 3;
        assert!(matches!(
            raster_page(&scene, 0, 1.0, || false),
            Err(RasterError::InvalidScene(_))
        ));
    }
    #[test]
    fn raster_counter_and_mid_render_cancellation() {
        let mut settings = PrintSettings::default();
        settings.body_size = 48.0;
        let scene = paginate("O", &settings, "", || false).unwrap();
        let (baseline, ink) = scene.pages[0]
            .drawables
            .iter()
            .find_map(|d| match d {
                Drawable::Text {
                    baseline, shape, ..
                } => shape.ink.map(|ink| (*baseline, ink)),
                _ => None,
            })
            .unwrap();
        let page = raster_page(&scene, 0, 2.0, || false).unwrap();
        let y = ((baseline.y - ink.y - ink.height / 2.0) * page.scale) as usize;
        let left = ((baseline.x + ink.x) * page.scale).floor().max(0.0) as usize;
        let right = ((baseline.x + ink.x + ink.width) * page.scale).ceil() as usize;
        let row: Vec<u8> = (left..right)
            .map(|x| page.rgba[(y * page.width as usize + x) * 4])
            .collect();
        assert!(row[..row.len() / 3].iter().any(|v| *v < 180));
        assert!(
            row[row.len() / 3..row.len() * 2 / 3]
                .iter()
                .any(|v| *v == 255)
        );
        assert!(row[row.len() * 2 / 3..].iter().any(|v| *v < 180));
        let mut calls = 0usize;
        assert!(matches!(
            raster_page(&scene, 0, 1.0, || {
                calls += 1;
                calls >= 4
            }),
            Err(RasterError::Cancelled)
        ));
    }
}
