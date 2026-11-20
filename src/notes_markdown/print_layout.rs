//! Bounded point geometry and reusable, embedded-font shaping for print output.
mod markdown;
mod publication;
pub(crate) use publication::{publish_pdf_with_admission, review_pdf_destination_with_cancel};
mod assets;
mod document;
pub mod pdf;
pub mod raster;
#[cfg(windows)]
pub mod printer;
pub use assets::PrintRaster;
pub use publication::{review_pdf_destination, publish_pdf, PdfDestinationReview, PdfPublishError, PdfPublishOutcome, PdfPublishPermit, PdfTargetKind};
pub use document::{paginate_with_assets, paginate, Drawable, LinkDestination, Page, PaginatedDocument, PrintError, PrintWarning, TextStyle};

use std::ops::Range;
use unicode_bidi::BidiInfo;
use unicode_script::{Script, UnicodeScript};
use unicode_segmentation::UnicodeSegmentation;

pub const MAX_PAGES: usize = 256;
pub const MAX_OUTPUT_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_ASSET_BYTES: usize = 64 * 1024 * 1024;
pub const MAX_TEXT_BYTES: usize = super::authoring::MAX_MARKDOWN_SOURCE_BYTES;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Point {
    pub x: f32,
    pub y: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Paper {
    Letter,
    A4,
    Custom { width: f32, height: f32 },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Orientation {
    Portrait,
    Landscape,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Margins {
    pub top: f32,
    pub right: f32,
    pub bottom: f32,
    pub left: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct PrintSettings {
    pub paper: Paper,
    pub orientation: Orientation,
    pub margins: Margins,
    pub body_size: f32,
    pub code_size: f32,
    pub title: String,
    pub header: String,
    pub footer: String,
}
impl Default for PrintSettings {
    fn default() -> Self {
        Self {
            paper: Paper::Letter,
            orientation: Orientation::Portrait,
            margins: Margins {
                top: 54.0,
                right: 54.0,
                bottom: 54.0,
                left: 54.0,
            },
            body_size: 11.0,
            code_size: 9.0,
            title: String::new(),
            header: String::new(),
            footer: String::new(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageGeometry {
    pub width: f32,
    pub height: f32,
    pub content: Rect,
    pub header_y: Option<f32>,
    pub footer_y: Option<f32>,
}
impl PrintSettings {
    pub fn geometry(&self) -> Result<PageGeometry, &'static str> {
        let (mut width, mut height) = match self.paper {
            Paper::Letter => (612.0, 792.0),
            Paper::A4 => (595.2756, 841.8898),
            Paper::Custom { width, height } => (width, height),
        };
        if !width.is_finite()
            || !height.is_finite()
            || !(72.0..=2880.0).contains(&width)
            || !(72.0..=2880.0).contains(&height)
        {
            return Err("Invalid page dimensions.");
        }
        if matches!(self.orientation, Orientation::Portrait) && width > height
            || matches!(self.orientation, Orientation::Landscape) && width < height
        {
            std::mem::swap(&mut width, &mut height);
        }
        let m = self.margins;
        if [m.top, m.right, m.bottom, m.left]
            .iter()
            .any(|v| !v.is_finite() || *v < 0.0 || *v > 1440.0)
        {
            return Err("Invalid page margins.");
        }
        if !(6.0..=48.0).contains(&self.body_size) || !(6.0..=48.0).contains(&self.code_size) {
            return Err("Font size must be between 6 and 48 points.");
        }
        if self.title.len() > 512 || self.title.chars().any(char::is_control) {
            return Err("Invalid print title.");
        }
        validate_template(&self.header)?;
        validate_template(&self.footer)?;
        let header = if self.header.is_empty() {
            0.0
        } else {
            self.body_size * 1.5
        };
        let footer = if self.footer.is_empty() {
            0.0
        } else {
            self.body_size * 1.5
        };
        let content = Rect {
            x: m.left,
            y: m.top + header,
            width: width - m.left - m.right,
            height: height - m.top - m.bottom - header - footer,
        };
        let minimum_height = line_height(TextRole::Body, self.body_size)?
            .max(line_height(TextRole::Code, self.code_size)?)
            .max(36.0);
        if content.width < 36.0 || content.height < minimum_height {
            return Err("Margins and headers leave insufficient printable space.");
        }
        Ok(PageGeometry {
            width,
            height,
            content,
            header_y: (header > 0.0).then_some(m.top),
            footer_y: (footer > 0.0).then_some(height - m.bottom - footer),
        })
    }
}
fn validate_template(template: &str) -> Result<(), &'static str> {
    if template.len() > 512 || template.chars().any(char::is_control) {
        return Err("Invalid header or footer.");
    }
    let mut rest = template;
    while let Some(i) = rest.find(['{', '}']) {
        if !rest[i..].starts_with('{') {
            return Err("Invalid header or footer token.");
        }
        let close = rest[i..]
            .find('}')
            .ok_or("Invalid header or footer token.")?
            + i;
        if !matches!(&rest[i + 1..close], "title" | "date" | "page" | "pages") {
            return Err("Unknown header or footer token.");
        }
        rest = &rest[close + 1..];
    }
    Ok(())
}
pub fn render_template(
    template: &str,
    title: &str,
    date: &str,
    page: usize,
    pages: usize,
) -> Result<String, &'static str> {
    validate_template(template)?;
    if page == 0
        || pages == 0
        || page > pages
        || pages > MAX_PAGES
        || title.len() > 512
        || date.len() > 128
        || title.chars().chain(date.chars()).any(char::is_control)
    {
        return Err("Invalid page template values.");
    }
    let mut out = String::new();
    let mut rest = template;
    while let Some(i) = rest.find('{') {
        out.push_str(&rest[..i]);
        let end = rest[i..].find('}').unwrap() + i;
        match &rest[i + 1..end] {
            "title" => out.push_str(title),
            "date" => out.push_str(date),
            "page" => out.push_str(&page.to_string()),
            "pages" => out.push_str(&pages.to_string()),
            _ => unreachable!(),
        }
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FontId {
    Body,
    Code,
    Emoji,
}
impl FontId {
    pub fn bytes(self) -> &'static [u8] {
        match self {
            Self::Body => epaint_default_fonts::UBUNTU_LIGHT,
            Self::Code => epaint_default_fonts::HACK_REGULAR,
            Self::Emoji => epaint_default_fonts::NOTO_EMOJI_REGULAR,
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextRole {
    Body,
    Code,
}
#[derive(Clone, Debug, PartialEq)]
/// Clusters identify original UTF-8 bytes; multiple visual glyphs can share a
/// cluster, and visual order can differ from logical order. Consumers must
/// extract each logical cluster only once rather than concatenate glyph ranges.
pub struct ShapedGlyph {
    pub id: u16,
    pub cluster: Range<usize>,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedRun {
    pub font: FontId,
    pub size: f32,
    pub origin: Point,
    pub ascent: f32,
    pub descent: f32,
    pub glyphs: Vec<ShapedGlyph>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ShapedText {
    pub runs: Vec<ShapedRun>,
    /// Pen advance, separate from ink; coordinates use positive y upward from baseline.
    pub width: f32,
    /// Actual glyph ink bounds in the same baseline-relative coordinate system.
    pub ink: Option<Rect>,
    pub unsupported: Vec<Range<usize>>,
}

struct Segment {
    font: FontId,
    script: Script,
    text: String,
    spans: Vec<(Range<usize>, Range<usize>)>,
}
fn face(id: FontId) -> Result<rustybuzz::Face<'static>, &'static str> {
    rustybuzz::Face::from_slice(id.bytes(), 0).ok_or("Embedded font is invalid.")
}
fn ignorable(ch: char) -> bool {
    matches!(ch as u32, 0x00ad | 0x034f | 0x061c | 0x115f..=0x1160 | 0x17b4..=0x17b5 | 0x180b..=0x180f | 0x200b..=0x200f | 0x202a..=0x202e | 0x2060..=0x206f | 0x3164 | 0xfe00..=0xfe0f | 0xfeff | 0xffa0 | 0x1bca0..=0x1bca3 | 0x1d173..=0x1d17a | 0xe0000..=0xe0fff)
}
/// Primary font line height, including a small leading allowance, in points.
pub fn line_height(role: TextRole, size: f32) -> Result<f32, &'static str> {
    if !size.is_finite() || !(6.0..=48.0).contains(&size) {
        return Err("Invalid font size.");
    }
    let font = face(match role {
        TextRole::Body => FontId::Body,
        TextRole::Code => FontId::Code,
    })?;
    let ttf: &rustybuzz::ttf_parser::Face<'_> = font.as_ref();
    Ok(
        (ttf.ascender() as f32 - ttf.descender() as f32 + ttf.line_gap() as f32).max(1.0) * size
            / font.units_per_em() as f32,
    )
}
fn font_supports(font: &rustybuzz::Face<'_>, grapheme: &str, rtl: bool) -> bool {
    if grapheme
        .chars()
        .all(|ch| ignorable(ch) || font.glyph_index(ch).is_some())
    {
        return true;
    }
    let mut buffer = rustybuzz::UnicodeBuffer::new();
    buffer.push_str(grapheme);
    buffer.set_direction(if rtl {
        rustybuzz::Direction::RightToLeft
    } else {
        rustybuzz::Direction::LeftToRight
    });
    buffer.guess_segment_properties();
    let shaped = rustybuzz::shape(font, &[], buffer);
    !shaped.glyph_infos().is_empty() && shaped.glyph_infos().iter().all(|glyph| glyph.glyph_id != 0)
}
/// Shapes one line. Input ranges are UTF-8 byte offsets into the original text;
/// unsupported graphemes are visibly replaced with '?' and reported.
pub fn shape_line(text: &str, role: TextRole, size: f32) -> Result<ShapedText, &'static str> {
    if text.len() > MAX_TEXT_BYTES
        || text.contains(['\n', '\r', '\t'])
        || !size.is_finite()
        || !(6.0..=48.0).contains(&size)
    {
        return Err("Invalid text or font size.");
    }
    if text.is_empty() {
        return Ok(ShapedText {
            runs: Vec::new(),
            width: 0.0,
            ink: None,
            unsupported: Vec::new(),
        });
    }
    let fonts = [
        face(FontId::Body)?,
        face(FontId::Code)?,
        face(FontId::Emoji)?,
    ];
    let primary = match role {
        TextRole::Body => 0,
        TextRole::Code => 1,
    };
    let fallback = if primary == 0 { 1 } else { 0 };
    let bidi = BidiInfo::new(text, None);
    let mut runs = Vec::new();
    let mut unsupported = Vec::new();
    let mut cursor = 0.0;
    let mut ink: Option<Rect> = None;
    for para in &bidi.paragraphs {
        let (levels, visual) = bidi.visual_runs(para, para.range.clone());
        for range in visual {
            let rtl = levels[range.start].is_rtl();
            let mut segments: Vec<Segment> = Vec::new();
            for (offset, grapheme) in text[range.clone()].grapheme_indices(true) {
                let begin = range.start + offset;
                let end = begin + grapheme.len();
                if grapheme.chars().all(ignorable) {
                    continue;
                }
                let selected = [primary, fallback, 2]
                    .into_iter()
                    .find(|&index| font_supports(&fonts[index], grapheme, rtl));
                let index = selected.unwrap_or(primary);
                let font = [FontId::Body, FontId::Code, FontId::Emoji][index];
                let script = grapheme
                    .chars()
                    .map(|ch| ch.script())
                    .find(|s| !matches!(s, Script::Common | Script::Inherited | Script::Unknown))
                    .unwrap_or(Script::Common);
                if segments.last().is_none_or(|s| {
                    s.font != font
                        || (s.script != script
                            && s.script != Script::Common
                            && script != Script::Common)
                }) {
                    segments.push(Segment {
                        font,
                        script,
                        text: String::new(),
                        spans: Vec::new(),
                    });
                }
                let segment = segments.last_mut().unwrap();
                if segment.script == Script::Common {
                    segment.script = script;
                }
                let start = segment.text.len();
                if selected.is_some() {
                    segment.text.push_str(grapheme);
                } else {
                    segment.text.push('?');
                    unsupported.push(begin..end);
                }
                segment.spans.push((start..segment.text.len(), begin..end));
            }
            if rtl {
                segments.reverse();
            }
            for segment in segments {
                let face = &fonts[match segment.font {
                    FontId::Body => 0,
                    FontId::Code => 1,
                    FontId::Emoji => 2,
                }];
                let scale = size / face.units_per_em() as f32;
                let mut buffer = rustybuzz::UnicodeBuffer::new();
                buffer.push_str(&segment.text);
                buffer.set_direction(if rtl {
                    rustybuzz::Direction::RightToLeft
                } else {
                    rustybuzz::Direction::LeftToRight
                });
                buffer.guess_segment_properties();
                let shaped = rustybuzz::shape(face, &[], buffer);
                let mut clusters: Vec<usize> = shaped
                    .glyph_infos()
                    .iter()
                    .map(|info| info.cluster as usize)
                    .collect();
                clusters.sort_unstable();
                clusters.dedup();
                clusters.push(segment.text.len());
                let mut glyphs = Vec::new();
                let mut x = 0.0;
                let mut y = 0.0;
                for (info, pos) in shaped.glyph_infos().iter().zip(shaped.glyph_positions()) {
                    let start = info.cluster as usize;
                    let next = *clusters
                        .get(clusters.partition_point(|&n| n <= start))
                        .unwrap_or(&segment.text.len());
                    let first = segment
                        .spans
                        .get(
                            segment
                                .spans
                                .partition_point(|(local, _)| local.end <= start),
                        )
                        .ok_or("Invalid font cluster.")?;
                    let last_index = segment
                        .spans
                        .partition_point(|(local, _)| local.start < next)
                        .checked_sub(1)
                        .ok_or("Invalid font cluster.")?;
                    let last = &segment.spans[last_index];
                    let gx = x + pos.x_offset as f32 * scale;
                    let gy = y + pos.y_offset as f32 * scale;
                    if let Some(bounds) = face
                        .glyph_bounding_box(rustybuzz::ttf_parser::GlyphId(info.glyph_id as u16))
                    {
                        let left = cursor + gx + bounds.x_min as f32 * scale;
                        let bottom = gy + bounds.y_min as f32 * scale;
                        let right = cursor + gx + bounds.x_max as f32 * scale;
                        let top = gy + bounds.y_max as f32 * scale;
                        ink = Some(match ink {
                            None => Rect {
                                x: left,
                                y: bottom,
                                width: right - left,
                                height: top - bottom,
                            },
                            Some(old) => {
                                let x0 = old.x.min(left);
                                let y0 = old.y.min(bottom);
                                Rect {
                                    x: x0,
                                    y: y0,
                                    width: (old.x + old.width).max(right) - x0,
                                    height: (old.y + old.height).max(top) - y0,
                                }
                            }
                        });
                    }
                    glyphs.push(ShapedGlyph {
                        id: info.glyph_id as u16,
                        cluster: first.1.start..last.1.end,
                        x: gx,
                        y: gy,
                        advance: pos.x_advance as f32 * scale,
                    });
                    x += pos.x_advance as f32 * scale;
                    y += pos.y_advance as f32 * scale;
                }
                let ttf: &rustybuzz::ttf_parser::Face<'_> = face.as_ref();
                runs.push(ShapedRun {
                    font: segment.font,
                    size,
                    origin: Point { x: cursor, y: 0.0 },
                    ascent: ttf.ascender() as f32 * scale,
                    descent: ttf.descender() as f32 * scale,
                    glyphs,
                });
                cursor += x;
            }
        }
    }
    Ok(ShapedText {
        runs,
        width: cursor,
        ink,
        unsupported,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn print_geometry_and_tokens_validate() {
        let mut settings = PrintSettings::default();
        assert_eq!(settings.geometry().unwrap().width, 612.0);
        settings.orientation = Orientation::Landscape;
        settings.header = "{title} {page}/{pages}".into();
        let geometry = settings.geometry().unwrap();
        assert!(geometry.width > geometry.height && geometry.header_y.is_some());
        assert_eq!(
            render_template(&settings.header, "Notes", "2026-01-01", 2, 3).unwrap(),
            "Notes 2/3"
        );
        settings.margins.left = f32::NAN;
        assert!(settings.geometry().is_err());
        assert!(render_template("{wrong}", "", "", 1, 1).is_err());
        assert!(render_template("{page}", "", "", 2, 1).is_err());
    }
    #[test]
    fn font_shapes_clusters_and_reports_missing() {
        let shaped = shape_line("office café 😀 🦄", TextRole::Body, 12.0).unwrap();
        assert!(shaped.width > 0.0 && !shaped.runs.is_empty());
        for run in &shaped.runs {
            assert!(run.ascent > 0.0 && run.descent < 0.0);
            for glyph in &run.glyphs {
                assert!(
                    glyph.cluster.start < glyph.cluster.end
                        && "office café 😀 🦄".get(glyph.cluster.clone()).is_some()
                );
            }
        }
        let missing = shape_line("a\u{10ffff}b", TextRole::Code, 10.0).unwrap();
        assert_eq!(missing.unsupported, vec![1..5]);
        assert!(
            missing
                .runs
                .iter()
                .flat_map(|r| &r.glyphs)
                .all(|g| g.id != 0)
        );
        let composed = shape_line("e\u{301}", TextRole::Body, 12.0).unwrap();
        assert!(composed.unsupported.is_empty());
        assert!(
            composed
                .runs
                .iter()
                .flat_map(|r| &r.glyphs)
                .all(|g| g.cluster == (0..3))
        );
        let controlled = shape_line("a\u{2066}b\u{2069}", TextRole::Body, 12.0).unwrap();
        assert!(controlled.unsupported.is_empty());
        assert!(shape_line("a\tb", TextRole::Code, 12.0).is_err());
        let glyph = shape_line("j", TextRole::Body, 48.0).unwrap();
        assert!(glyph.ink.is_some_and(|ink| ink.x < 0.0));
        assert!(line_height(TextRole::Code, 48.0).unwrap() > 48.0);
    }
}
