//! PDF encoding of an already paginated, immutable print scene.
use super::{
    Drawable, FontId, LinkDestination, MAX_ASSET_BYTES, MAX_OUTPUT_BYTES, MAX_PAGES,
    PaginatedDocument, TextStyle,
};
use krilla::{
    Document,
    action::LinkAction,
    annotation::{LinkAnnotation, Target},
    destination::XyzDestination,
    geom::{PathBuilder, Point, Rect, Size, Transform},
    image::{BitsPerComponent, CustomImage, Image, ImageColorspace},
    page::PageSettings,
    paint::{Fill, LineJoin, Stroke},
    text::{Font, GlyphId, KrillaGlyph},
};
use std::{
    fmt,
    hash::{Hash, Hasher},
    sync::Arc,
};
const MAX_GLYPH_REPLAY: usize = 1_000_000;
const MAX_URI_BYTES: usize = MAX_OUTPUT_BYTES / 8;
const MAX_CLUSTER_REPLAY_BYTES: usize = MAX_OUTPUT_BYTES / 8;
#[derive(Clone)]
struct PdfImage {
    index: usize,
    width: u32,
    height: u32,
    rgb: Arc<[u8]>,
    alpha: Option<Arc<[u8]>>,
}
impl Hash for PdfImage {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.index.hash(state);
        self.width.hash(state);
        self.height.hash(state);
    }
}
impl CustomImage for PdfImage {
    fn color_channel(&self) -> &[u8] {
        &self.rgb
    }
    fn alpha_channel(&self) -> Option<&[u8]> {
        self.alpha.as_deref()
    }
    fn bits_per_component(&self) -> BitsPerComponent {
        BitsPerComponent::Eight
    }
    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
    fn icc_profile(&self) -> Option<&[u8]> {
        None
    }
    fn color_space(&self) -> ImageColorspace {
        ImageColorspace::Rgb
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PdfError {
    Cancelled,
    InvalidScene(&'static str),
    Limit(&'static str),
    Encoding(String),
}
impl fmt::Display for PdfError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Cancelled => f.write_str("PDF encoding cancelled"),
            Self::InvalidScene(s) | Self::Limit(s) => f.write_str(s),
            Self::Encoding(s) => write!(f, "PDF encoding failed: {s}"),
        }
    }
}
impl std::error::Error for PdfError {}
fn rect(r: super::Rect) -> Result<Rect, PdfError> {
    if ![r.x, r.y, r.width, r.height].iter().all(|v| v.is_finite()) {
        return Err(PdfError::InvalidScene("Invalid PDF rectangle."));
    }
    Rect::from_xywh(r.x, r.y, r.width, r.height)
        .ok_or(PdfError::InvalidScene("Invalid PDF rectangle."))
}
fn point(p: super::Point) -> Result<Point, PdfError> {
    if !p.x.is_finite() || !p.y.is_finite() {
        return Err(PdfError::InvalidScene("Invalid PDF point."));
    }
    Ok(Point::from_xy(p.x, p.y))
}
/// Encode only; caller owns explicit publication. Cancellation never returns a partial artifact.
pub fn encode_pdf(
    scene: &PaginatedDocument,
    mut cancelled: impl FnMut() -> bool,
) -> Result<Vec<u8>, PdfError> {
    if scene.pages.is_empty() || scene.pages.len() > MAX_PAGES {
        return Err(PdfError::Limit("Invalid PDF page count."));
    }
    let mut assets = Vec::with_capacity(scene.assets.len());
    let mut asset_bytes = 0usize;
    for (asset_index, asset) in scene.assets.iter().enumerate() {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let bytes = (asset.width as usize)
            .checked_mul(asset.height as usize)
            .and_then(|v| v.checked_mul(4))
            .ok_or(PdfError::Limit("PDF image size overflow."))?;
        asset_bytes = asset_bytes
            .checked_add(bytes)
            .filter(|v| *v <= MAX_ASSET_BYTES)
            .ok_or(PdfError::Limit("PDF image budget exceeded."))?;
        if asset.width == 0 || asset.height == 0 || bytes != asset.rgba.len() {
            return Err(PdfError::InvalidScene("Invalid PDF image."));
        }
        let mut rgb = Vec::with_capacity(bytes / 4 * 3);
        let mut alpha = Vec::with_capacity(bytes / 4);
        let mut translucent = false;
        for (index, pixel) in asset.rgba.as_chunks::<4>().0.iter().enumerate() {
            if index % 4096 == 0 && cancelled() {
                return Err(PdfError::Cancelled);
            }
            rgb.extend_from_slice(&pixel[..3]);
            alpha.push(pixel[3]);
            translucent |= pixel[3] != 255;
        }
        let image = PdfImage {
            index: asset_index,
            width: asset.width,
            height: asset.height,
            rgb: rgb.into(),
            alpha: translucent.then(|| alpha.into()),
        };
        assets.push(Image::from_custom(image, false).map_err(PdfError::Encoding)?);
    }
    let fonts: Vec<Font> = [FontId::Body, FontId::Code, FontId::Emoji]
        .into_iter()
        .map(|id| {
            Font::new(id.bytes().into(), 0)
                .ok_or(PdfError::InvalidScene("Embedded font is invalid."))
        })
        .collect::<Result<_, _>>()?;
    let mut document = Document::new();
    let mut work = 0usize;
    let mut glyph_work = 0usize;
    let mut cluster_bytes = 0usize;
    let mut uri_bytes = 0usize;
    for page in &scene.pages {
        if cancelled() {
            return Err(PdfError::Cancelled);
        }
        let settings = PageSettings::from_wh(page.geometry.width, page.geometry.height)
            .ok_or(PdfError::InvalidScene("Invalid PDF page dimensions."))?;
        let mut output_page = document.start_page_with(settings);
        {
            let mut surface = output_page.surface();
            for item in &page.drawables {
                work = work
                    .checked_add(1)
                    .filter(|v| *v <= 120_000)
                    .ok_or(PdfError::Limit("PDF drawing limit exceeded."))?;
                if cancelled() {
                    return Err(PdfError::Cancelled);
                }
                match item {
                    Drawable::Text {
                        text,
                        shape,
                        baseline,
                        style,
                    } => {
                        point(*baseline)?;
                        if let Some(ink) = style.styled_ink(shape) {
                            let left = baseline.x + ink.x;
                            let right = left + ink.width;
                            let top = baseline.y - ink.y - ink.height;
                            let bottom = baseline.y - ink.y;
                            if ![left, right, top, bottom].iter().all(|v| v.is_finite())
                                || left < -0.001
                                || top < -0.001
                                || right > page.geometry.width + 0.001
                                || bottom > page.geometry.height + 0.001
                            {
                                return Err(PdfError::InvalidScene(
                                    "Styled text would extend outside its PDF page.",
                                ));
                            }
                        }
                        let mut prepared = Vec::new();
                        for run in &shape.runs {
                            if !run.size.is_finite()
                                || run.size <= 0.0
                                || !run.origin.x.is_finite()
                                || !run.origin.y.is_finite()
                            {
                                return Err(PdfError::InvalidScene("Invalid font metrics."));
                            }
                            glyph_work = glyph_work
                                .checked_add(run.glyphs.len())
                                .filter(|v| *v <= MAX_GLYPH_REPLAY)
                                .ok_or(PdfError::Limit("PDF glyph replay limit exceeded."))?;
                            let font = fonts[match run.font {
                                FontId::Body => 0,
                                FontId::Code => 1,
                                FontId::Emoji => 2,
                            }]
                            .clone();
                            let mut pen = 0.0;
                            let mut glyphs = Vec::with_capacity(run.glyphs.len());
                            for (index, g) in run.glyphs.iter().enumerate() {
                                if index % 1024 == 0 && cancelled() {
                                    return Err(PdfError::Cancelled);
                                }
                                if text.get(g.cluster.clone()).is_none()
                                    || g.cluster.is_empty()
                                    || ![g.x, g.y, g.advance].iter().all(|v| v.is_finite())
                                {
                                    return Err(PdfError::InvalidScene(
                                        "Invalid PDF text cluster.",
                                    ));
                                }
                                cluster_bytes = cluster_bytes
                                    .checked_add(g.cluster.len())
                                    .filter(|v| *v <= MAX_CLUSTER_REPLAY_BYTES)
                                    .ok_or(PdfError::Limit(
                                        "PDF text replay byte limit exceeded.",
                                    ))?;
                                glyphs.push(KrillaGlyph::new(
                                    GlyphId::new(g.id as u32),
                                    g.advance / run.size,
                                    (g.x - pen) / run.size,
                                    g.y / run.size,
                                    0.0,
                                    g.cluster.clone(),
                                    None,
                                ));
                                pen += g.advance;
                            }
                            prepared.push((
                                Point::from_xy(
                                    baseline.x + run.origin.x,
                                    baseline.y - run.origin.y,
                                ),
                                glyphs,
                                font,
                                run.size,
                            ));
                        }
                        surface.set_fill(Some(Fill::default()));
                        surface.set_stroke(if style.bold {
                            Some(Stroke {
                                width: TextStyle::BOLD_STROKE,
                                line_join: LineJoin::Round,
                                ..Stroke::default()
                            })
                        } else {
                            None
                        });
                        if style.italic {
                            surface.push_transform(&Transform::from_row(
                                1.0,
                                0.0,
                                -TextStyle::ITALIC_SHEAR,
                                1.0,
                                TextStyle::ITALIC_SHEAR * baseline.y,
                                0.0,
                            ));
                        }
                        for (origin, glyphs, font, size) in prepared {
                            surface.draw_glyphs(origin, &glyphs, font, text, size, false);
                        }
                        if style.italic {
                            surface.pop();
                        }
                        surface.set_stroke(None);
                    }
                    Drawable::Line { from, to, width } => {
                        point(*from)?;
                        point(*to)?;
                        if !width.is_finite() || *width <= 0.0 || *width > 48.0 {
                            return Err(PdfError::InvalidScene("Invalid PDF line width."));
                        }
                        let mut path = PathBuilder::new();
                        path.move_to(from.x, from.y);
                        path.line_to(to.x, to.y);
                        if let Some(path) = path.finish() {
                            surface.set_fill(None);
                            surface.set_stroke(Some(Stroke {
                                width: *width,
                                ..Stroke::default()
                            }));
                            surface.draw_path(&path);
                            surface.set_stroke(None);
                        }
                    }
                    Drawable::Image { asset, bounds } => {
                        rect(*bounds)?;
                        let image = assets
                            .get(*asset)
                            .ok_or(PdfError::InvalidScene("Missing PDF image."))?;
                        let size = Size::from_wh(bounds.width, bounds.height)
                            .ok_or(PdfError::InvalidScene("Invalid PDF image size."))?;
                        surface.push_transform(&Transform::from_translate(bounds.x, bounds.y));
                        surface.draw_image(image.clone(), size);
                        surface.pop();
                    }
                    Drawable::Link { .. } => {}
                }
            }
            surface.finish();
        }
        for item in &page.drawables {
            if let Drawable::Link {
                bounds,
                destination,
            } = item
            {
                if cancelled() {
                    return Err(PdfError::Cancelled);
                }
                let target = match destination {
                    LinkDestination::External(uri) => {
                        uri_bytes = uri_bytes
                            .checked_add(uri.len())
                            .filter(|v| *v <= MAX_URI_BYTES)
                            .ok_or(PdfError::Limit("PDF annotation text limit exceeded."))?;
                        let checked = crate::notes_markdown::html::safe_link_destination(uri)
                            .ok_or(PdfError::InvalidScene("Unsafe PDF link."))?;
                        if checked != uri.as_ref()
                            || !(uri.starts_with("http://")
                                || uri.starts_with("https://")
                                || uri.starts_with("mailto:"))
                        {
                            return Err(PdfError::InvalidScene("Unsafe PDF link."));
                        }
                        Target::Action(LinkAction::new(uri.to_string()).into())
                    }
                    LinkDestination::Internal {
                        page,
                        point: destination,
                    } => {
                        if *page >= scene.pages.len() {
                            return Err(PdfError::InvalidScene("Invalid PDF destination page."));
                        }
                        Target::Destination(XyzDestination::new(*page, point(*destination)?).into())
                    }
                };
                output_page.add_annotation(LinkAnnotation::new(rect(*bounds)?, target).into());
            }
        }
        output_page.finish();
    }
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    let bytes = document
        .finish()
        .map_err(|error| PdfError::Encoding(error.to_string()))?;
    if cancelled() {
        return Err(PdfError::Cancelled);
    }
    if bytes.len() > MAX_OUTPUT_BYTES {
        return Err(PdfError::Limit("PDF output exceeds its byte limit."));
    }
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes_markdown::print_layout::{PrintSettings, paginate_with_assets};
    use crate::notes_markdown::{LocalRasterAsset, LocalRasterFormat, LocalRasterResolver};
    struct FixtureImage {
        tall: Vec<u8>,
        square: Vec<u8>,
    }
    impl LocalRasterResolver for FixtureImage {
        fn resolve<'a>(&'a self, destination: &str) -> Option<LocalRasterAsset<'a>> {
            let bytes = match destination {
                "assets/tall.png" => &self.tall,
                "assets/square.png" => &self.square,
                _ => return None,
            };
            Some(LocalRasterAsset {
                format: LocalRasterFormat::Png,
                bytes,
            })
        }
    }
    fn png(width: u32, height: u32) -> Vec<u8> {
        let pixels: Vec<u8> = (0..16)
            .flat_map(|i| [i * 11, 120, 200, if i == 0 { 96 } else { 255 }])
            .collect();
        let image = image_025::RgbaImage::from_raw(width, height, pixels).unwrap();
        let mut buffer = std::io::Cursor::new(Vec::new());
        image_025::DynamicImage::ImageRgba8(image)
            .write_to(&mut buffer, image_025::ImageFormat::Png)
            .unwrap();
        buffer.into_inner()
    }
    #[test]
    fn pdf_encodes_selectable_scene_and_reports_cancellation() {
        use std::io::Write;
        let image = FixtureImage {
            tall: png(2, 8),
            square: png(4, 4),
        };
        let mut settings = PrintSettings::default();
        settings.title = "Café".into();
        settings.header = "{title}".into();
        settings.footer = "Page {page} of {pages}".into();
        let mut source = String::from(
            "# Café 😀\n\n**Bold** and *italic* `let x = 42` [web](https://example.com)\n\n[go](#café)\n\n![Tall](assets/tall.png) ![Square](assets/square.png) ![Again](assets/tall.png)\n\n| A | B |\n|---|---|\n",
        );
        for i in 0..100 {
            source.push_str(&format!("| Row {i} | Accént |\n"));
        }
        let scene =
            paginate_with_assets(&source, &settings, "2026-11-18", Some(&image), || false).unwrap();
        assert!(scene.pages.len() > 1 && scene.assets.len() == 2);
        let pdf = encode_pdf(&scene, || false).unwrap();
        assert!(pdf.starts_with(b"%PDF-"));
        assert_eq!(
            encode_pdf(&scene, || true).unwrap_err(),
            PdfError::Cancelled
        );
        if let Some(path) = std::env::var_os("MULTI_LAUNCHER_TEST_PDF_PATH") {
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(path)
                .unwrap();
            file.write_all(&pdf).unwrap();
        }
    }
    #[test]
    fn pdf_zero_margin_style_and_italic_cancellation() {
        let mut settings = PrintSettings::default();
        settings.margins.left = 0.0;
        let scene = crate::notes_markdown::print_layout::paginate(
            "**j** and *italic*",
            &settings,
            "",
            || false,
        )
        .unwrap();
        assert!(encode_pdf(&scene, || false).is_ok());
        let italic =
            crate::notes_markdown::print_layout::paginate("*italic*", &settings, "", || false)
                .unwrap();
        let mut checks = 0usize;
        let result = encode_pdf(&italic, || {
            checks += 1;
            checks >= 3
        });
        assert_eq!(result.unwrap_err(), PdfError::Cancelled);
    }
    #[test]
    fn expanded_uri_is_warned_without_breaking_pdf() {
        let source = format!("[link](https://example.com/{})", "é".repeat(3000));
        let scene = crate::notes_markdown::print_layout::paginate(
            &source,
            &PrintSettings::default(),
            "",
            || false,
        )
        .unwrap();
        assert!(!scene.warnings.is_empty());
        assert!(encode_pdf(&scene, || false).is_ok());
    }
}
