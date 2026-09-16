//! Preparation-time font selection and bounded text layout metadata.
//!
//! The native renderer may turn these snapshots into glyph runs later. This
//! module deliberately performs family discovery once and never from paint.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedGlyph {
    pub glyph_id: u16,
    pub family: Arc<str>,
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
    pub alpha: Arc<[u8]>,
}

pub const MAX_DISCOVERED_FONTS: usize = 512;
pub const MAX_LAYOUT_CACHE_ENTRIES: usize = 1_024;
pub const MAX_LAYOUT_DIAGNOSTICS: usize = 512;
pub const MAX_LABEL_GRAPHEMES: usize = 256;
pub const MAX_TOOLTIP_SOURCE_GRAPHEMES: usize = 4_096;
pub const MAX_TOOLTIP_LINES: usize = 24;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ScriptClass {
    Latin,
    Cyrillic,
    Cjk,
    Emoji,
    Mixed,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontLayoutPurpose {
    CellLabel,
    Tooltip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontWrapPolicy {
    Ellipsis,
    Word,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontAlignment {
    Center,
    Left,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FontRequest {
    pub family: Option<String>,
    pub size_milli: u32,
    pub bold: bool,
    pub italic: bool,
    pub dpi_milli: u32,
    pub max_width_milli: u32,
    pub max_height_milli: u32,
    pub max_lines: u16,
    pub purpose: FontLayoutPurpose,
    pub wrap: FontWrapPolicy,
    pub alignment: FontAlignment,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FontDiagnostic {
    RequestedFamilyMissing(String),
    FallbackFamilyMissing,
    LabelTruncated,
    TooltipViewLimited,
    MissingGlyph(char),
    FontReadFailed(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PreparedTextLayout {
    /// Complete immutable source supplied by the model or dynamic projection.
    pub source_text: Arc<str>,
    /// Bounded string rendered by the compositor.
    pub text: Arc<str>,
    pub selected_family: Arc<str>,
    pub script: ScriptClass,
    pub estimated_width_milli: u32,
    /// Measured multiline block dimensions in desktop-logical milli-pixels.
    pub measured_width_milli: u32,
    pub measured_height_milli: u32,
    pub line_count: u16,
    /// Preparation-time glyph coverage. Paint never opens or scans fonts.
    pub glyphs: Vec<PreparedGlyph>,
    pub diagnostics: Vec<FontDiagnostic>,
}

pub trait FontCatalog: Send + Sync {
    fn has_family(&self, family: &str) -> bool;

    fn load_family(&self, family: &str) -> Option<ab_glyph::FontArc> {
        self.has_family(family)
            .then(|| crate::annotation::raster::default_font_arc().map(|(font, _)| font))
            .flatten()
    }
}

/// A bounded filename-based catalog captured once during service creation.
/// It avoids repeated system font directory scans and never opens font bytes.
#[derive(Clone, Debug, Default)]
pub struct SystemFontCatalog {
    families: BTreeSet<String>,
    files: BTreeMap<String, PathBuf>,
}

impl SystemFontCatalog {
    pub fn discover() -> Self {
        let mut families = BTreeSet::new();
        let mut files = BTreeMap::new();
        if let Some(directory) = windows_fonts_directory() {
            if let Ok(entries) = std::fs::read_dir(directory) {
                for entry in entries.flatten().take(MAX_DISCOVERED_FONTS) {
                    let path = entry.path();
                    if path
                        .extension()
                        .and_then(|value| value.to_str())
                        .is_some_and(|value| {
                            matches!(value.to_ascii_lowercase().as_str(), "ttf" | "otf" | "ttc")
                        })
                    {
                        if let Some(stem) = path.file_stem().and_then(|value| value.to_str()) {
                            register_font_path(&mut families, &mut files, stem, &path);
                            for alias in aliases_for_font_file(&path) {
                                register_font_path(&mut families, &mut files, alias, &path);
                            }
                        }
                    }
                }
            }
        }
        Self { families, files }
    }

    pub fn family_names(&self) -> Vec<String> {
        self.families.iter().cloned().collect()
    }
}

impl FontCatalog for SystemFontCatalog {
    fn has_family(&self, family: &str) -> bool {
        self.families.contains(&normalize_family(family))
    }

    fn load_family(&self, family: &str) -> Option<ab_glyph::FontArc> {
        let bytes = std::fs::read(self.files.get(&normalize_family(family))?).ok()?;
        for index in 0..32 {
            if let Ok(font) = ab_glyph::FontVec::try_from_vec_and_index(bytes.clone(), index) {
                return Some(font.into());
            }
        }
        None
    }
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct LayoutKey {
    request: FontRequest,
    text: String,
}

struct LayoutEntry {
    layout: Arc<PreparedTextLayout>,
    touched: u64,
}

pub struct FontLayoutService<C = SystemFontCatalog> {
    catalog: C,
    cache: BTreeMap<LayoutKey, LayoutEntry>,
    fonts: BTreeMap<String, Option<ab_glyph::FontArc>>,
    clock: u64,
    max_entries: usize,
}

impl FontLayoutService<SystemFontCatalog> {
    pub fn discover() -> Self {
        Self::with_catalog(SystemFontCatalog::discover(), MAX_LAYOUT_CACHE_ENTRIES)
    }
}

impl<C: FontCatalog> FontLayoutService<C> {
    pub fn with_catalog(catalog: C, max_entries: usize) -> Self {
        Self {
            catalog,
            cache: BTreeMap::new(),
            fonts: BTreeMap::new(),
            clock: 0,
            max_entries: max_entries.max(1),
        }
    }

    pub fn prepare(&mut self, text: &str, request: FontRequest) -> Arc<PreparedTextLayout> {
        let key = LayoutKey {
            request: request.clone(),
            text: text.into(),
        };
        if let Some(entry) = self.cache.get_mut(&key) {
            self.clock = self.clock.wrapping_add(1);
            entry.touched = self.clock;
            return Arc::clone(&entry.layout);
        }
        let layout = Arc::new(prepare_layout(
            &self.catalog,
            &mut self.fonts,
            text,
            &request,
        ));
        self.clock = self.clock.wrapping_add(1);
        self.cache.insert(
            key,
            LayoutEntry {
                layout: Arc::clone(&layout),
                touched: self.clock,
            },
        );
        while self.cache.len() > self.max_entries {
            let evict = self
                .cache
                .iter()
                .min_by_key(|(key, entry)| (entry.touched, *key))
                .map(|(key, _)| key.clone())
                .expect("cache is non-empty");
            self.cache.remove(&evict);
        }
        layout
    }

    pub fn len(&self) -> usize {
        self.cache.len()
    }
}

fn prepare_layout(
    catalog: &impl FontCatalog,
    font_cache: &mut BTreeMap<String, Option<ab_glyph::FontArc>>,
    text: &str,
    request: &FontRequest,
) -> PreparedTextLayout {
    let script = classify_script(text);
    let mut diagnostics = Vec::new();
    let fallback = fallback_families(script);
    let mut candidates = Vec::<String>::new();
    if let Some(requested) = &request.family {
        if catalog.has_family(requested) {
            candidates.push(requested.clone());
        } else {
            diagnostics.push(FontDiagnostic::RequestedFamilyMissing(requested.clone()));
        }
    }
    for family in fallback
        .iter()
        .chain(fallback_families(ScriptClass::Cjk))
        .chain(fallback_families(ScriptClass::Emoji))
    {
        if catalog.has_family(family)
            && !candidates
                .iter()
                .any(|candidate| candidate.eq_ignore_ascii_case(family))
        {
            candidates.push((*family).to_string());
        }
    }
    let mut loaded = Vec::new();
    for family in candidates {
        let key = normalize_family(&family);
        let font = font_cache
            .entry(key)
            .or_insert_with(|| catalog.load_family(&family))
            .clone();
        if let Some(font) = font {
            loaded.push((family, font));
        } else {
            diagnostics.push(FontDiagnostic::FontReadFailed(family));
        }
    }
    if loaded.is_empty() {
        diagnostics.push(FontDiagnostic::FallbackFamilyMissing);
        if let Some((font, _)) = crate::annotation::raster::default_font_arc() {
            loaded.push((fallback[0].to_owned(), font));
        }
    }
    let selected = loaded
        .first()
        .map(|(family, _)| family.clone())
        .unwrap_or_else(|| fallback[0].to_owned());

    let source_grapheme_limit = match request.purpose {
        FontLayoutPurpose::CellLabel => MAX_LABEL_GRAPHEMES,
        FontLayoutPurpose::Tooltip => MAX_TOOLTIP_SOURCE_GRAPHEMES,
    };
    let graphemes = UnicodeSegmentation::graphemes(text, true)
        .take(source_grapheme_limit.saturating_add(1))
        .collect::<Vec<_>>();
    let source_limited = graphemes.len() > source_grapheme_limit;
    let width_per_grapheme = (request.size_milli as u64 * request.dpi_milli as u64 / 1_000)
        .saturating_mul(if matches!(script, ScriptClass::Cjk | ScriptClass::Emoji) {
            10
        } else {
            6
        })
        / 10;
    let dpi_milli = request.dpi_milli.max(1) as u64;
    let width_limit_physical =
        (request.max_width_milli.max(1) as u64).saturating_mul(dpi_milli) / 1_000;
    let fit = if width_per_grapheme == 0 {
        MAX_TOOLTIP_SOURCE_GRAPHEMES
    } else {
        (width_limit_physical / width_per_grapheme) as usize
    }
    .max(1);
    let (display, line_count, widest_graphemes) = match request.wrap {
        FontWrapPolicy::Ellipsis => {
            let visible_count = graphemes.len().min(source_grapheme_limit);
            let fit = fit.min(MAX_LABEL_GRAPHEMES);
            if source_limited || visible_count > fit {
                diagnostics.push(FontDiagnostic::LabelTruncated);
                let retained = fit.saturating_sub(1).min(visible_count);
                (
                    format!("{}…", graphemes[..retained].concat()),
                    1,
                    retained.saturating_add(1),
                )
            } else {
                (graphemes.concat(), 1, visible_count)
            }
        }
        FontWrapPolicy::Word => {
            let bounded = graphemes[..graphemes.len().min(source_grapheme_limit)].concat();
            let max_lines = usize::from(request.max_lines.max(1)).min(MAX_TOOLTIP_LINES);
            let height_line_limit = if request.max_height_milli == 0 {
                max_lines
            } else {
                (u64::from(request.max_height_milli) * 1_000
                    / (u64::from(request.size_milli.max(1)) * 1_200))
                    .max(1) as usize
            }
            .min(max_lines);
            let (mut rendered, lines, width, line_limited) =
                wrap_tooltip(&bounded, fit, height_line_limit);
            if source_limited || line_limited {
                diagnostics.push(FontDiagnostic::TooltipViewLimited);
                if !rendered.ends_with('…') {
                    rendered.push('…');
                }
            }
            (rendered, lines, width)
        }
    };
    let glyphs = rasterize_glyphs(&display, request, &loaded, &mut diagnostics);
    let mut unique_diagnostics = BTreeSet::new();
    diagnostics.retain(|diagnostic| unique_diagnostics.insert(diagnostic.clone()));
    diagnostics.truncate(MAX_LAYOUT_DIAGNOSTICS);
    let measured_width_milli = width_per_grapheme
        .saturating_mul(widest_graphemes as u64)
        .saturating_mul(1_000)
        .checked_div(dpi_milli)
        .unwrap_or(u64::MAX)
        .min(u32::MAX as u64) as u32;
    let measured_height_milli = (request.size_milli as u64)
        .saturating_mul(1_200)
        .saturating_mul(line_count.max(1) as u64)
        .min(u32::MAX as u64) as u32;
    PreparedTextLayout {
        source_text: text.into(),
        text: display.into(),
        selected_family: selected.into(),
        script,
        estimated_width_milli: width_per_grapheme
            .saturating_mul(widest_graphemes as u64)
            .min(u32::MAX as u64) as u32,
        measured_width_milli,
        measured_height_milli,
        line_count: line_count.min(u16::MAX as usize) as u16,
        glyphs,
        diagnostics,
    }
}

fn wrap_tooltip(
    text: &str,
    max_graphemes_per_line: usize,
    max_lines: usize,
) -> (String, usize, usize, bool) {
    let mut lines = vec![String::new()];
    let mut limited = false;
    let mut paragraphs = text.split('\n').peekable();
    while let Some(paragraph) = paragraphs.next() {
        for word in paragraph.split_whitespace() {
            let current_len = |line: &str| UnicodeSegmentation::graphemes(line, true).count();
            let line_len = current_len(lines.last().map(String::as_str).unwrap_or_default());
            if line_len > 0 {
                if line_len + 1 + UnicodeSegmentation::graphemes(word, true).count()
                    <= max_graphemes_per_line
                {
                    lines.last_mut().expect("one line exists").push(' ');
                } else {
                    if lines.len() >= max_lines {
                        limited = true;
                        break;
                    }
                    lines.push(String::new());
                }
            }
            for grapheme in UnicodeSegmentation::graphemes(word, true) {
                if current_len(lines.last().map(String::as_str).unwrap_or_default())
                    >= max_graphemes_per_line
                {
                    if lines.len() >= max_lines {
                        limited = true;
                        break;
                    }
                    lines.push(String::new());
                }
                lines
                    .last_mut()
                    .expect("one line exists")
                    .push_str(grapheme);
            }
            if limited {
                break;
            }
        }
        if limited {
            break;
        }
        if paragraphs.peek().is_some() {
            if lines.len() >= max_lines {
                limited = true;
                break;
            }
            lines.push(String::new());
        }
    }
    let widest = lines
        .iter()
        .map(|line| UnicodeSegmentation::graphemes(line.as_str(), true).count())
        .max()
        .unwrap_or_default();
    (lines.join("\n"), lines.len(), widest, limited)
}

fn rasterize_glyphs(
    text: &str,
    request: &FontRequest,
    fonts: &[(String, ab_glyph::FontArc)],
    diagnostics: &mut Vec<FontDiagnostic>,
) -> Vec<PreparedGlyph> {
    use ab_glyph::{Font, ScaleFont, point};
    let Some((_, first)) = fonts.first() else {
        return Vec::new();
    };
    let (scale_tweak, y_offset) = crate::annotation::raster::default_font_arc()
        .map(|(_, tweak)| (tweak.scale, tweak.y_offset))
        .unwrap_or((1.0, 0.0));
    let pixel_size = request.size_milli as f32 / 1_000.0 * request.dpi_milli.max(1) as f32
        / 1_000.0
        * scale_tweak;
    let ascent = first.as_scaled(pixel_size.max(1.0)).ascent() + y_offset * pixel_size;
    let line_height = pixel_size * 1.2;
    let mut output = Vec::new();
    for (line_index, line) in text.split('\n').enumerate() {
        let mut caret = point(0.0, ascent + line_index as f32 * line_height);
        let mut pending = Vec::new();
        let mut run_width = 0.0_f32;
        for character in line.chars() {
            let Some((family, font)) = fonts
                .iter()
                .find(|(_, font)| font.glyph_id(character).0 != 0)
            else {
                let diagnostic = FontDiagnostic::MissingGlyph(character);
                if !diagnostics.contains(&diagnostic) {
                    diagnostics.push(diagnostic);
                }
                continue;
            };
            let scaled = font.as_scaled(pixel_size.max(1.0));
            let mut glyph = scaled.scaled_glyph(character);
            let glyph_id = glyph.id.0;
            glyph.position = caret;
            caret.x += scaled.h_advance(glyph.id);
            run_width = run_width.max(caret.x);
            if let Some(outlined) = scaled.outline_glyph(glyph) {
                let bounds = outlined.px_bounds();
                let width = bounds.width().ceil().max(0.0) as u32;
                let height = bounds.height().ceil().max(0.0) as u32;
                let Some(len) = (width as usize).checked_mul(height as usize) else {
                    continue;
                };
                let mut alpha = vec![0_u8; len];
                outlined.draw(|x, y, coverage| {
                    if let Some(slot) = alpha.get_mut(y as usize * width as usize + x as usize) {
                        *slot = (coverage * 255.0).round().clamp(0.0, 255.0) as u8;
                    }
                });
                pending.push((
                    bounds.min.x.floor() as i32,
                    bounds.min.y.floor() as i32,
                    width,
                    height,
                    alpha,
                    glyph_id,
                    family.clone(),
                ));
            }
        }
        let x_offset = match request.alignment {
            FontAlignment::Center => -(run_width * 0.5).round() as i32,
            FontAlignment::Left => 0,
        };
        output.extend(
            pending
                .into_iter()
                .map(
                    |(x, y, width, height, alpha, glyph_id, family)| PreparedGlyph {
                        glyph_id,
                        family: family.into(),
                        x: x + x_offset,
                        y,
                        width,
                        height,
                        alpha: alpha.into(),
                    },
                ),
        );
    }
    output
}

fn register_font_path(
    families: &mut BTreeSet<String>,
    files: &mut BTreeMap<String, PathBuf>,
    family: &str,
    path: &Path,
) {
    let key = normalize_family(family);
    families.insert(key.clone());
    files.entry(key).or_insert_with(|| path.to_owned());
}

fn aliases_for_font_file(path: &Path) -> &'static [&'static str] {
    match path
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "segoeui.ttf" => &["Segoe UI"],
        "seguiemj.ttf" => &["Segoe UI Emoji"],
        "seguisym.ttf" => &["Segoe UI Symbol"],
        "arial.ttf" => &["Arial"],
        "times.ttf" => &["Times New Roman"],
        "cour.ttf" => &["Courier New"],
        "tahoma.ttf" => &["Tahoma"],
        "verdana.ttf" => &["Verdana"],
        "georgia.ttf" => &["Georgia"],
        "consola.ttf" => &["Consolas"],
        "msyh.ttc" | "msyh.ttf" => &["Microsoft YaHei UI"],
        "yugothr.ttc" | "yugothm.ttc" => &["Yu Gothic UI"],
        _ => &[],
    }
}

fn normalize_family(value: &str) -> String {
    value.trim().to_lowercase()
}

fn fallback_families(script: ScriptClass) -> &'static [&'static str] {
    match script {
        ScriptClass::Emoji => &["Segoe UI Emoji", "Segoe UI Symbol", "Segoe UI"],
        ScriptClass::Cjk => &["Microsoft YaHei UI", "Yu Gothic UI", "Segoe UI"],
        _ => &["Segoe UI", "Arial"],
    }
}

fn classify_script(text: &str) -> ScriptClass {
    let mut classes = BTreeSet::new();
    for character in text
        .chars()
        .filter(|value| !value.is_whitespace() && !value.is_ascii_punctuation())
    {
        classes.insert(if character.is_ascii() {
            ScriptClass::Latin
        } else if ('\u{0400}'..='\u{052f}').contains(&character) {
            ScriptClass::Cyrillic
        } else if ('\u{3400}'..='\u{9fff}').contains(&character)
            || ('\u{3040}'..='\u{30ff}').contains(&character)
            || ('\u{ac00}'..='\u{d7af}').contains(&character)
        {
            ScriptClass::Cjk
        } else if character as u32 >= 0x1f000 {
            ScriptClass::Emoji
        } else {
            ScriptClass::Latin
        });
    }
    match classes.len() {
        0 | 1 => classes.into_iter().next().unwrap_or(ScriptClass::Latin),
        _ => ScriptClass::Mixed,
    }
}

fn windows_fonts_directory() -> Option<PathBuf> {
    std::env::var_os("WINDIR")
        .map(PathBuf::from)
        .map(|path| path.join("Fonts"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Catalog {
        families: BTreeSet<String>,
        lookups: AtomicUsize,
        loads: AtomicUsize,
    }
    impl FontCatalog for Catalog {
        fn has_family(&self, family: &str) -> bool {
            self.lookups.fetch_add(1, Ordering::Relaxed);
            self.families.contains(&normalize_family(family))
        }
        fn load_family(&self, family: &str) -> Option<ab_glyph::FontArc> {
            self.loads.fetch_add(1, Ordering::Relaxed);
            self.has_family(family)
                .then(|| crate::annotation::raster::default_font_arc().map(|(font, _)| font))
                .flatten()
        }
    }

    fn request(family: Option<&str>, width: u32) -> FontRequest {
        FontRequest {
            family: family.map(str::to_owned),
            size_milli: 12_000,
            bold: false,
            italic: false,
            dpi_milli: 1_000,
            max_width_milli: width,
            max_height_milli: 0,
            max_lines: 1,
            purpose: FontLayoutPurpose::CellLabel,
            wrap: FontWrapPolicy::Ellipsis,
            alignment: FontAlignment::Center,
        }
    }

    #[test]
    fn missing_family_uses_script_fallback_and_cached_layout_avoids_new_lookup() {
        let catalog = Catalog {
            families: [
                normalize_family("Segoe UI Emoji"),
                normalize_family("Segoe UI"),
            ]
            .into_iter()
            .collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 4);
        let first = service.prepare("Open 🚀", request(Some("Absent"), 200_000));
        let lookups = service.catalog.lookups.load(Ordering::Relaxed);
        let second = service.prepare("Open 🚀", request(Some("Absent"), 200_000));
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(service.catalog.lookups.load(Ordering::Relaxed), lookups);
        assert_eq!(first.script, ScriptClass::Mixed);
        assert_eq!(service.catalog.loads.load(Ordering::Relaxed), 2);
        let loads = service.catalog.loads.load(Ordering::Relaxed);
        service.prepare("different text", request(None, 200_000));
        assert_eq!(service.catalog.loads.load(Ordering::Relaxed), loads);
        assert!(
            first
                .diagnostics
                .contains(&FontDiagnostic::RequestedFamilyMissing("Absent".into()))
        );
    }

    #[test]
    fn unicode_graphemes_are_preserved_and_long_labels_are_bounded() {
        let catalog = Catalog {
            families: [normalize_family("Segoe UI")].into_iter().collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 1);
        let layout = service.prepare(
            "e\u{301} 日本語 abcdefghijklmnopqrstuvwxyz",
            request(None, 40_000),
        );
        assert!(layout.text.ends_with('…'));
        assert!(layout.diagnostics.contains(&FontDiagnostic::LabelTruncated));
        assert_eq!(
            &*layout.source_text,
            "e\u{301} 日本語 abcdefghijklmnopqrstuvwxyz"
        );
        assert_eq!(layout.script, ScriptClass::Mixed);
        service.prepare("replacement", request(None, 40_000));
        assert_eq!(service.len(), 1);
    }

    #[test]
    fn tooltip_layout_wraps_complete_unicode_and_dynamic_text_separately_from_labels() {
        let catalog = Catalog {
            families: [normalize_family("Segoe UI")].into_iter().collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 8);
        let source = "日本語 dynamic result with several words and e\u{301} accents";
        let label_request = request(None, 60_000);
        let mut tooltip_request = label_request.clone();
        tooltip_request.max_width_milli = 120_000;
        tooltip_request.max_height_milli = 240_000;
        tooltip_request.max_lines = 12;
        tooltip_request.purpose = FontLayoutPurpose::Tooltip;
        tooltip_request.wrap = FontWrapPolicy::Word;
        tooltip_request.alignment = FontAlignment::Left;
        let label = service.prepare(source, label_request);
        let tooltip = service.prepare(source, tooltip_request);
        assert_ne!(label.text, tooltip.text);
        assert_eq!(&*tooltip.source_text, source);
        assert!(tooltip.text.contains("日本語"));
        assert!(tooltip.text.contains("e\u{301}"));
        assert!(tooltip.line_count > 1);
        assert!(tooltip.measured_height_milli > 12_000);
        assert!(
            !tooltip
                .diagnostics
                .contains(&FontDiagnostic::TooltipViewLimited)
        );
        assert!(!Arc::ptr_eq(&label, &tooltip));
    }

    #[test]
    fn tooltip_view_limit_is_typed_and_does_not_rewrite_source() {
        let catalog = Catalog {
            families: [normalize_family("Segoe UI")].into_iter().collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 4);
        let source = "word ".repeat(MAX_TOOLTIP_SOURCE_GRAPHEMES + 10);
        let mut tooltip_request = request(None, 200_000);
        tooltip_request.purpose = FontLayoutPurpose::Tooltip;
        tooltip_request.wrap = FontWrapPolicy::Word;
        tooltip_request.alignment = FontAlignment::Left;
        tooltip_request.max_lines = 2;
        tooltip_request.max_height_milli = 30_000;
        let tooltip = service.prepare(&source, tooltip_request);
        assert_eq!(&*tooltip.source_text, source);
        assert!(tooltip.text.ends_with('…'));
        assert!(tooltip.line_count <= 2);
        assert!(
            tooltip
                .diagnostics
                .contains(&FontDiagnostic::TooltipViewLimited)
        );
    }

    #[test]
    fn cache_key_separates_purpose_wrap_constraints_width_and_dpi() {
        let catalog = Catalog {
            families: [normalize_family("Segoe UI")].into_iter().collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 8);
        let source = "same text";
        let label_request = request(None, 80_000);
        let mut tooltip_request = label_request.clone();
        tooltip_request.purpose = FontLayoutPurpose::Tooltip;
        tooltip_request.wrap = FontWrapPolicy::Word;
        tooltip_request.max_lines = 8;
        tooltip_request.max_height_milli = 100_000;
        let label = service.prepare(source, label_request.clone());
        let tooltip = service.prepare(source, tooltip_request.clone());
        assert!(!Arc::ptr_eq(&label, &tooltip));
        tooltip_request.max_width_milli = 120_000;
        let wider = service.prepare(source, tooltip_request.clone());
        assert!(!Arc::ptr_eq(&tooltip, &wider));
        tooltip_request.dpi_milli = 1_500;
        let high_dpi = service.prepare(source, tooltip_request);
        assert!(!Arc::ptr_eq(&wider, &high_dpi));
    }

    #[test]
    fn absent_glyph_is_diagnosed_instead_of_rasterizing_tofu() {
        let catalog = Catalog {
            families: [normalize_family("Segoe UI")].into_iter().collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 2);
        let missing = char::from_u32(0x10_fffd).unwrap();
        let layout = service.prepare(&missing.to_string(), request(None, 100_000));
        assert!(
            layout
                .diagnostics
                .contains(&FontDiagnostic::MissingGlyph(missing))
        );
        assert!(layout.glyphs.is_empty());

        let repeated = service.prepare(&format!("{missing}{missing}"), request(None, 100_000));
        assert_eq!(
            repeated
                .diagnostics
                .iter()
                .filter(|diagnostic| matches!(diagnostic, &&FontDiagnostic::MissingGlyph(_)))
                .count(),
            1,
            "repeated missing glyphs are retained as one bounded diagnostic"
        );
    }

    #[test]
    fn script_specific_selection_does_not_hide_missing_glyphs_in_a_synthetic_catalog() {
        let catalog = Catalog {
            families: [
                normalize_family("Segoe UI Emoji"),
                normalize_family("Microsoft YaHei UI"),
            ]
            .into_iter()
            .collect(),
            lookups: AtomicUsize::new(0),
            loads: AtomicUsize::new(0),
        };
        let mut service = FontLayoutService::with_catalog(catalog, 4);
        let emoji = service.prepare("🚀", request(None, 100_000));
        assert_eq!(emoji.script, ScriptClass::Emoji);
        assert_eq!(&*emoji.selected_family, "Segoe UI Emoji");
        assert!(emoji.glyphs.is_empty());
        assert!(
            emoji
                .diagnostics
                .contains(&FontDiagnostic::MissingGlyph('🚀'))
        );
        let cjk = service.prepare("日本語", request(None, 100_000));
        assert_eq!(cjk.script, ScriptClass::Cjk);
        assert_eq!(&*cjk.selected_family, "Microsoft YaHei UI");
        assert!(
            cjk.diagnostics
                .iter()
                .any(|diagnostic| matches!(diagnostic, FontDiagnostic::MissingGlyph(_)))
        );
    }

    #[cfg(windows)]
    #[test]
    fn installed_family_bytes_change_real_glyph_raster_and_are_cached() {
        let catalog = SystemFontCatalog::discover();
        assert!(catalog.has_family("Segoe UI"));
        assert!(catalog.has_family("Arial"));
        let mut service = FontLayoutService::with_catalog(catalog, 8);
        let segoe = service.prepare("Ag", request(Some("Segoe UI"), 100_000));
        let arial = service.prepare("Ag", request(Some("Arial"), 100_000));
        assert_eq!(&*segoe.selected_family, "Segoe UI");
        assert_eq!(&*arial.selected_family, "Arial");
        assert!(segoe.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
        assert!(arial.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
        assert_ne!(segoe.glyphs, arial.glyphs);
        let loaded = service.fonts.len();
        let again = service.prepare("Ag", request(Some("Segoe UI"), 100_000));
        assert!(Arc::ptr_eq(&segoe, &again));
        assert_eq!(service.fonts.len(), loaded);
    }

    #[cfg(windows)]
    #[test]
    fn mixed_script_uses_per_glyph_installed_fallback_without_tofu() {
        let mut service = FontLayoutService::discover();
        let layout = service.prepare("A日🚀", request(Some("Arial"), 200_000));
        assert!(layout.glyphs.iter().all(|glyph| glyph.glyph_id != 0));
        assert!(
            !layout
                .diagnostics
                .iter()
                .any(|diagnostic| { matches!(diagnostic, FontDiagnostic::MissingGlyph(_)) })
        );
        assert!(layout.glyphs.iter().any(|glyph| &*glyph.family == "Arial"));
        assert!(layout.glyphs.iter().any(|glyph| &*glyph.family != "Arial"));
    }
}
