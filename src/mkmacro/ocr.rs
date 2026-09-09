//! Platform-neutral OCR document representation, logical-text normalization,
//! and text matching. Platform adapters convert recognizer-specific values into
//! these types; runtime capture and polling remain separate concerns.

use crate::mkmacro::{
    CapturedRegion, DiagnosticKind, ExecResult, ExecutionDiagnostic, MkOcrLanguage, MkOcrMatchMode,
    MkOcrOccurrence, MkPoint, ScreenCaptureBackend, ScreenRect, SearchRegion, cancelled_error,
};
use image::{RgbaImage, imageops};
use regex::{Regex, RegexBuilder};
use std::{cmp::Ordering, fmt, ops::Range};

/// Tile overlap reduces the chance that a word crossing an engine input edge
/// is truncated. Effective overlap is reduced for unusually small backends so
/// every tile step remains nonzero.
pub const OCR_TILE_OVERLAP_PX: u32 = 96;

pub trait OcrBackend: Send + Sync {
    fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>>;
    fn max_image_dimension(&self) -> ExecResult<u32>;
    fn recognize(
        &self,
        image: &RgbaImage,
        language: &MkOcrLanguage,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecResult<OcrDocument>;
}

#[derive(Debug, Default)]
pub struct UnsupportedOcrBackend;

impl OcrBackend for UnsupportedOcrBackend {
    fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
        Err(unsupported_ocr())
    }

    fn max_image_dimension(&self) -> ExecResult<u32> {
        Err(unsupported_ocr())
    }

    fn recognize(
        &self,
        _: &RgbaImage,
        _: &MkOcrLanguage,
        cancelled: &dyn Fn() -> bool,
    ) -> ExecResult<OcrDocument> {
        if cancelled() {
            Err(cancelled_error())
        } else {
            Err(unsupported_ocr())
        }
    }
}

fn unsupported_ocr() -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "OCR is unavailable on this platform",
    )
    .context("backend", "ocr")
    .context("platform", std::env::consts::OS)
}

/// A captured frame plus the globally reconstructed OCR document. Word bounds
/// in `document` use signed virtual-desktop coordinates; `capture` retains the
/// immutable source pixels for authoring previews without another capture.
#[derive(Debug, Clone)]
pub struct OcrRegionDocument {
    pub capture: CapturedRegion,
    pub document: OcrDocument,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OcrTile {
    /// Tile bounds in capture-local coordinates.
    pub rect: ScreenRect,
}

/// Captures a region exactly once, recognizes one or more immutable tiles, and
/// returns a document whose word geometry is in virtual-desktop coordinates.
pub fn recognize_region(
    capture_backend: &dyn ScreenCaptureBackend,
    ocr_backend: &dyn OcrBackend,
    region: &SearchRegion,
    language: &MkOcrLanguage,
    cancelled: &dyn Fn() -> bool,
) -> ExecResult<OcrRegionDocument> {
    check_cancelled(cancelled)?;
    let capture = capture_backend
        .capture(region, cancelled)
        .map_err(|error| error.context("ocr_pipeline_operation", "capture region"))?;
    check_cancelled(cancelled)?;
    let max_dimension = ocr_backend.max_image_dimension().map_err(|error| {
        error
            .context("ocr_pipeline_operation", "query maximum image dimension")
            .context("image_width", capture.image.width().to_string())
            .context("image_height", capture.image.height().to_string())
    })?;
    let tiles = plan_ocr_tiles(capture.image.width(), capture.image.height(), max_dimension)?;

    let tiled = tiles.len() > 1;
    let mut recognized = Vec::new();
    for (tile_index, tile) in tiles.into_iter().enumerate() {
        check_cancelled(cancelled)?;
        let cropped = if tiled {
            Some(
                imageops::crop_imm(
                    &capture.image,
                    u32::try_from(tile.rect.x)
                        .map_err(|_| invalid_geometry("negative OCR tile X"))?,
                    u32::try_from(tile.rect.y)
                        .map_err(|_| invalid_geometry("negative OCR tile Y"))?,
                    tile.rect.width,
                    tile.rect.height,
                )
                .to_image(),
            )
        } else {
            None
        };
        let image = cropped.as_ref().unwrap_or(&capture.image);
        let document = ocr_backend
            .recognize(image, language, cancelled)
            .map_err(|error| {
                error
                    .context("ocr_pipeline_operation", "recognize tile")
                    .context("tile_index", tile_index.to_string())
                    .context(
                        "tile",
                        format!(
                            "{},{},{}x{}",
                            tile.rect.x, tile.rect.y, tile.rect.width, tile.rect.height
                        ),
                    )
            })?;
        check_cancelled(cancelled)?;
        recognized.push((tile_index, tile, document));
    }

    let document = if tiled {
        merge_tiled_documents(&recognized, capture.origin, capture.image.dimensions())?
    } else {
        let (_, tile, document) = recognized
            .pop()
            .ok_or_else(|| invalid_geometry("OCR produced no tile"))?;
        translate_document(
            document,
            tile.rect,
            capture.origin,
            capture.image.dimensions(),
        )?
    };
    check_cancelled(cancelled)?;
    Ok(OcrRegionDocument { capture, document })
}

fn check_cancelled(cancelled: &dyn Fn() -> bool) -> ExecResult {
    if cancelled() {
        Err(cancelled_error())
    } else {
        Ok(())
    }
}

fn invalid_geometry(message: impl Into<String>) -> ExecutionDiagnostic {
    ExecutionDiagnostic::new(DiagnosticKind::InvalidTarget, message).context("backend", "ocr")
}

pub fn plan_ocr_tiles(width: u32, height: u32, max_dimension: u32) -> ExecResult<Vec<OcrTile>> {
    if width == 0 || height == 0 {
        return Err(invalid_geometry("OCR source image is empty"));
    }
    if max_dimension == 0 {
        return Err(invalid_geometry(
            "OCR backend reported a zero maximum image dimension",
        ));
    }
    let overlap = OCR_TILE_OVERLAP_PX
        .min(max_dimension / 4)
        .min(max_dimension.saturating_sub(1));
    let xs = tile_axis(width, max_dimension, overlap)?;
    let ys = tile_axis(height, max_dimension, overlap)?;
    let capacity = xs
        .len()
        .checked_mul(ys.len())
        .ok_or_else(|| invalid_geometry("OCR tile count overflow"))?;
    let mut tiles = Vec::with_capacity(capacity);
    for y in ys {
        for &x in &xs {
            tiles.push(OcrTile {
                rect: ScreenRect::new(
                    i32::try_from(x)
                        .map_err(|_| invalid_geometry("OCR tile X exceeds signed coordinates"))?,
                    i32::try_from(y)
                        .map_err(|_| invalid_geometry("OCR tile Y exceeds signed coordinates"))?,
                    max_dimension.min(width - x),
                    max_dimension.min(height - y),
                ),
            });
        }
    }
    Ok(tiles)
}

fn tile_axis(length: u32, maximum: u32, overlap: u32) -> ExecResult<Vec<u32>> {
    if length <= maximum {
        return Ok(vec![0]);
    }
    let step = maximum
        .checked_sub(overlap)
        .filter(|step| *step > 0)
        .ok_or_else(|| invalid_geometry("OCR tile step is zero"))?;
    let mut starts: Vec<u32> = vec![0];
    while starts
        .last()
        .copied()
        .unwrap_or(0)
        .checked_add(maximum)
        .is_some_and(|end| end < length)
    {
        let next = starts
            .last()
            .copied()
            .unwrap_or(0)
            .checked_add(step)
            .ok_or_else(|| invalid_geometry("OCR tile position overflow"))?;
        if next >= length {
            return Err(invalid_geometry("OCR tile start exceeds source axis"));
        }
        if starts.last().copied() == Some(next) {
            return Err(invalid_geometry("OCR tile planning did not advance"));
        }
        starts.push(next);
    }
    Ok(starts)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrLanguageInfo {
    pub tag: String,
    pub display_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrWord {
    pub text: String,
    /// Bounds in the coordinate space of the owning document.
    pub bounds: ScreenRect,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OcrLine {
    /// Recognizer-provided line text. When empty, [`OcrLine::recognized_text`]
    /// reconstructs it deterministically from the words.
    pub text: String,
    pub words: Vec<OcrWord>,
}

impl OcrLine {
    pub fn recognized_text(&self) -> String {
        if self.text.is_empty() {
            self.words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" ")
        } else {
            self.text.clone()
        }
    }

    pub fn bounds(&self) -> Option<ScreenRect> {
        union_bounds(self.words.iter().map(|word| word.bounds))
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OcrDocument {
    pub recognized_language: Option<String>,
    pub image_width: u32,
    pub image_height: u32,
    pub lines: Vec<OcrLine>,
}

impl OcrDocument {
    /// Human-readable OCR text for Read Text. Logical line ordering and Unicode
    /// are preserved; only meaningless whitespace outside the document is removed.
    pub fn recognized_text(&self) -> String {
        self.lines
            .iter()
            .map(OcrLine::recognized_text)
            .collect::<Vec<_>>()
            .join("\n")
            .trim()
            .to_owned()
    }

    pub fn normalized_text(&self) -> NormalizedOcrText {
        NormalizedOcrText::from_document(self)
    }
}

fn translate_document(
    mut document: OcrDocument,
    tile: ScreenRect,
    desktop_origin: (i32, i32),
    full_dimensions: (u32, u32),
) -> ExecResult<OcrDocument> {
    if (document.image_width != 0 || document.image_height != 0)
        && (document.image_width, document.image_height) != (tile.width, tile.height)
    {
        return Err(invalid_geometry(format!(
            "OCR backend returned document dimensions {}x{} for {}x{} tile",
            document.image_width, document.image_height, tile.width, tile.height
        )));
    }
    for line in &mut document.lines {
        for word in &mut line.words {
            word.bounds = translate_word_bounds(word.bounds, tile, desktop_origin)?;
        }
    }
    document.image_width = full_dimensions.0;
    document.image_height = full_dimensions.1;
    Ok(document)
}

fn translate_word_bounds(
    local: ScreenRect,
    tile: ScreenRect,
    desktop_origin: (i32, i32),
) -> ExecResult<ScreenRect> {
    if local.is_empty()
        || local.x < 0
        || local.y < 0
        || local.right() > i64::from(tile.width)
        || local.bottom() > i64::from(tile.height)
    {
        return Err(invalid_geometry(format!(
            "OCR word bounds ({},{},{}x{}) exceed tile {}x{}",
            local.x, local.y, local.width, local.height, tile.width, tile.height
        )));
    }
    let x = i64::from(desktop_origin.0)
        .checked_add(i64::from(tile.x))
        .and_then(|value| value.checked_add(i64::from(local.x)))
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid_geometry("OCR desktop X translation overflow"))?;
    let y = i64::from(desktop_origin.1)
        .checked_add(i64::from(tile.y))
        .and_then(|value| value.checked_add(i64::from(local.y)))
        .and_then(|value| i32::try_from(value).ok())
        .ok_or_else(|| invalid_geometry("OCR desktop Y translation overflow"))?;
    let translated = ScreenRect::new(x, y, local.width, local.height);
    if translated.right() > i64::from(i32::MAX) + 1 || translated.bottom() > i64::from(i32::MAX) + 1
    {
        return Err(invalid_geometry("OCR desktop rectangle endpoint overflow"));
    }
    Ok(translated)
}

#[derive(Debug)]
struct TiledWord {
    tile_index: usize,
    word: OcrWord,
}

fn merge_tiled_documents(
    documents: &[(usize, OcrTile, OcrDocument)],
    desktop_origin: (i32, i32),
    full_dimensions: (u32, u32),
) -> ExecResult<OcrDocument> {
    let mut recognized_language = None;
    let mut words: Vec<TiledWord> = Vec::new();
    for (tile_index, tile, document) in documents {
        if (document.image_width != 0 || document.image_height != 0)
            && (document.image_width, document.image_height) != (tile.rect.width, tile.rect.height)
        {
            return Err(invalid_geometry(format!(
                "OCR backend returned document dimensions {}x{} for {}x{} tile",
                document.image_width, document.image_height, tile.rect.width, tile.rect.height
            )));
        }
        if recognized_language.is_none() {
            recognized_language.clone_from(&document.recognized_language);
        }
        for line in &document.lines {
            for word in &line.words {
                let translated = OcrWord {
                    text: word.text.clone(),
                    bounds: translate_word_bounds(word.bounds, tile.rect, desktop_origin)?,
                };
                let duplicate = words.iter().any(|existing| {
                    existing.tile_index != *tile_index
                        && ocr_words_are_duplicates(&existing.word, &translated)
                });
                if !duplicate {
                    words.push(TiledWord {
                        tile_index: *tile_index,
                        word: translated,
                    });
                }
            }
        }
    }
    Ok(OcrDocument {
        recognized_language,
        image_width: full_dimensions.0,
        image_height: full_dimensions.1,
        lines: reconstruct_ocr_lines(words.into_iter().map(|word| word.word).collect()),
    })
}

/// Duplicate recognition requires both exact recognized text and spatially
/// equivalent geometry. Text equality alone would incorrectly remove distinct
/// controls carrying the same label.
pub fn ocr_words_are_duplicates(left: &OcrWord, right: &OcrWord) -> bool {
    if left.text != right.text || left.bounds.is_empty() || right.bounds.is_empty() {
        return false;
    }
    let intersection_width = (left.bounds.right().min(right.bounds.right())
        - i64::from(left.bounds.x.max(right.bounds.x)))
    .max(0) as u64;
    let intersection_height = (left.bounds.bottom().min(right.bounds.bottom())
        - i64::from(left.bounds.y.max(right.bounds.y)))
    .max(0) as u64;
    let intersection_area = intersection_width.saturating_mul(intersection_height);
    let smaller_area = u64::from(left.bounds.width)
        .saturating_mul(u64::from(left.bounds.height))
        .min(u64::from(right.bounds.width).saturating_mul(u64::from(right.bounds.height)));
    let substantial_overlap = intersection_area.saturating_mul(2) >= smaller_area;
    let centers_are_near = rect_center(left.bounds)
        .zip(rect_center(right.bounds))
        .is_some_and(|(left, right)| {
            left.x.abs_diff(right.x) <= 3 && left.y.abs_diff(right.y) <= 3
        });
    substantial_overlap || centers_are_near
}

/// Reconstructs deterministic global lines from spatial OCR words. Vertical
/// overlap establishes line membership; final lines and words are explicitly
/// sorted so no hash/container iteration order can affect results.
pub fn reconstruct_ocr_lines(mut words: Vec<OcrWord>) -> Vec<OcrLine> {
    words.retain(|word| !word.text.is_empty() && !word.bounds.is_empty());
    words.sort_by(compare_words_spatially);
    let mut lines: Vec<Vec<OcrWord>> = Vec::new();
    for word in words {
        let destination = lines
            .iter()
            .enumerate()
            .filter_map(|(index, line)| {
                let bounds = union_bounds(line.iter().map(|word| word.bounds))?;
                let overlap = vertical_overlap(bounds, word.bounds);
                let minimum_height = bounds.height.min(word.bounds.height);
                (overlap.saturating_mul(2) >= minimum_height).then_some((index, overlap))
            })
            .max_by_key(|(index, overlap)| (*overlap, std::cmp::Reverse(*index)))
            .map(|(index, _)| index);
        if let Some(index) = destination {
            lines[index].push(word);
        } else {
            lines.push(vec![word]);
        }
    }
    for line in &mut lines {
        line.sort_by(compare_words_in_line);
    }
    lines.sort_by(|left, right| compare_lines(left, right));
    lines
        .into_iter()
        .map(|words| OcrLine {
            text: words
                .iter()
                .map(|word| word.text.as_str())
                .collect::<Vec<_>>()
                .join(" "),
            words,
        })
        .collect()
}

fn vertical_overlap(left: ScreenRect, right: ScreenRect) -> u32 {
    let top = i64::from(left.y.max(right.y));
    let bottom = left.bottom().min(right.bottom());
    u32::try_from((bottom - top).max(0)).unwrap_or(u32::MAX)
}

fn compare_words_spatially(left: &OcrWord, right: &OcrWord) -> Ordering {
    left.bounds
        .y
        .cmp(&right.bounds.y)
        .then(left.bounds.x.cmp(&right.bounds.x))
        .then(left.bounds.height.cmp(&right.bounds.height))
        .then(left.bounds.width.cmp(&right.bounds.width))
        .then(left.text.cmp(&right.text))
}

fn compare_words_in_line(left: &OcrWord, right: &OcrWord) -> Ordering {
    left.bounds
        .x
        .cmp(&right.bounds.x)
        .then(left.bounds.y.cmp(&right.bounds.y))
        .then(left.text.cmp(&right.text))
}

fn compare_lines(left: &[OcrWord], right: &[OcrWord]) -> Ordering {
    let left = union_bounds(left.iter().map(|word| word.bounds));
    let right = union_bounds(right.iter().map(|word| word.bounds));
    match (left, right) {
        (Some(left), Some(right)) => left.y.cmp(&right.y).then(left.x.cmp(&right.x)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrTextSpan {
    pub range: Range<usize>,
    pub line_index: usize,
    pub word_index: usize,
    pub bounds: ScreenRect,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NormalizedOcrText {
    pub text: String,
    pub spans: Vec<OcrTextSpan>,
}

impl NormalizedOcrText {
    fn from_document(document: &OcrDocument) -> Self {
        let mut normalized = Self::default();
        for (line_index, line) in document.lines.iter().enumerate() {
            for (word_index, word) in line.words.iter().enumerate() {
                for token in word.text.split_whitespace() {
                    if token.is_empty() {
                        continue;
                    }
                    if !normalized.text.is_empty() {
                        normalized.text.push(' ');
                    }
                    let start = normalized.text.len();
                    normalized.text.push_str(token);
                    normalized.spans.push(OcrTextSpan {
                        range: start..normalized.text.len(),
                        line_index,
                        word_index,
                        bounds: word.bounds,
                    });
                }
            }
        }
        normalized
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OcrMatch {
    /// Text actually recognized for the match, retaining OCR case and punctuation.
    pub text: String,
    pub bounds: ScreenRect,
    pub center: MkPoint,
    /// One-based occurrence in deterministic document reading order.
    pub occurrence: usize,
    pub normalized_range: Range<usize>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OcrSearchResult {
    pub selected: Option<OcrMatch>,
    pub match_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OcrMatchError {
    InvalidRegex(String),
}

impl fmt::Display for OcrMatchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidRegex(error) => {
                write!(formatter, "invalid OCR regular expression: {error}")
            }
        }
    }
}

impl std::error::Error for OcrMatchError {}

/// Searches normalized logical OCR text and maps each match back to the exact
/// recognized words supplying its geometry. Matches without any participating
/// OCR word (including every zero-width regex match) cannot invent coordinates
/// and are omitted.
pub fn search_document(
    document: &OcrDocument,
    query: &str,
    mode: MkOcrMatchMode,
    case_sensitive: bool,
    occurrence: MkOcrOccurrence,
) -> Result<OcrSearchResult, OcrMatchError> {
    let normalized = document.normalized_text();
    let regex = match mode {
        MkOcrMatchMode::Contains | MkOcrMatchMode::WholeWordPhrase => {
            let query = normalize_whitespace(query);
            if query.is_empty() {
                return Ok(OcrSearchResult::default());
            }
            build_regex(&regex::escape(&query), case_sensitive)?
        }
        MkOcrMatchMode::Regex => build_regex(query, case_sensitive)?,
    };

    let matches = regex
        .find_iter(&normalized.text)
        .filter(|matched| {
            mode != MkOcrMatchMode::WholeWordPhrase
                || has_word_boundaries(&normalized.text, matched.range())
        })
        .filter_map(|matched| mapped_match(&normalized, matched.range()))
        .enumerate()
        .map(|(index, mut matched)| {
            matched.occurrence = index + 1;
            matched
        })
        .collect::<Vec<_>>();
    let selected = occurrence
        .selected_index()
        .and_then(|index| matches.get(index).cloned());
    Ok(OcrSearchResult {
        selected,
        match_count: matches.len(),
    })
}

fn build_regex(pattern: &str, case_sensitive: bool) -> Result<Regex, OcrMatchError> {
    RegexBuilder::new(pattern)
        .case_insensitive(!case_sensitive)
        .build()
        .map_err(|error| OcrMatchError::InvalidRegex(error.to_string()))
}

fn normalize_whitespace(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn has_word_boundaries(text: &str, range: Range<usize>) -> bool {
    let starts_with_word = text[range.clone()]
        .chars()
        .next()
        .is_some_and(is_word_character);
    let ends_with_word = text[range.clone()]
        .chars()
        .next_back()
        .is_some_and(is_word_character);
    let before_is_word = text[..range.start]
        .chars()
        .next_back()
        .is_some_and(is_word_character);
    let after_is_word = text[range.end..]
        .chars()
        .next()
        .is_some_and(is_word_character);
    (!starts_with_word || !before_is_word) && (!ends_with_word || !after_is_word)
}

fn is_word_character(character: char) -> bool {
    character == '_' || character.is_alphanumeric()
}

fn mapped_match(normalized: &NormalizedOcrText, range: Range<usize>) -> Option<OcrMatch> {
    if range.is_empty() {
        return None;
    }
    let bounds = union_bounds(
        normalized
            .spans
            .iter()
            .filter(|span| span.range.start < range.end && range.start < span.range.end)
            .map(|span| span.bounds),
    )?;
    let center = rect_center(bounds)?;
    Some(OcrMatch {
        text: normalized.text[range.clone()].to_owned(),
        bounds,
        center,
        occurrence: 0,
        normalized_range: range,
    })
}

fn union_bounds(bounds: impl IntoIterator<Item = ScreenRect>) -> Option<ScreenRect> {
    let mut bounds = bounds.into_iter().filter(|bounds| !bounds.is_empty());
    let first = bounds.next()?;
    let (mut left, mut top, mut right, mut bottom) = (
        i64::from(first.x),
        i64::from(first.y),
        first.right(),
        first.bottom(),
    );
    for bounds in bounds {
        left = left.min(i64::from(bounds.x));
        top = top.min(i64::from(bounds.y));
        right = right.max(bounds.right());
        bottom = bottom.max(bounds.bottom());
    }
    Some(ScreenRect::new(
        i32::try_from(left).ok()?,
        i32::try_from(top).ok()?,
        u32::try_from(right.checked_sub(left)?).ok()?,
        u32::try_from(bottom.checked_sub(top)?).ok()?,
    ))
}

fn rect_center(bounds: ScreenRect) -> Option<MkPoint> {
    Some(MkPoint {
        x: i32::try_from(i64::from(bounds.x) + i64::from(bounds.width / 2)).ok()?,
        y: i32::try_from(i64::from(bounds.y) + i64::from(bounds.height / 2)).ok()?,
    })
}

/// Converts a recognizer's floating-point image rectangle using floor/ceil
/// coverage and clamps it to the source image. Invalid, non-finite, or empty
/// rectangles are ignored.
pub fn rounded_clamped_ocr_bounds(
    x: f32,
    y: f32,
    width: f32,
    height: f32,
    image_width: u32,
    image_height: u32,
) -> Option<ScreenRect> {
    if !x.is_finite()
        || !y.is_finite()
        || !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || image_width == 0
        || image_height == 0
    {
        return None;
    }
    let right = x + width;
    let bottom = y + height;
    if !right.is_finite() || !bottom.is_finite() {
        return None;
    }
    let left = x.floor().clamp(0.0, image_width as f32) as u32;
    let top = y.floor().clamp(0.0, image_height as f32) as u32;
    let right = right.ceil().clamp(0.0, image_width as f32) as u32;
    let bottom = bottom.ceil().clamp(0.0, image_height as f32) as u32;
    let width = right.checked_sub(left)?;
    let height = bottom.checked_sub(top)?;
    let bounds = ScreenRect::new(
        i32::try_from(left).ok()?,
        i32::try_from(top).ok()?,
        width,
        height,
    );
    (!bounds.is_empty()).then_some(bounds)
}

/// Windows `SoftwareBitmap` consumes BGRA8 while screen capture is RGBA8.
/// Conversion is explicit and leaves the source frame immutable.
pub fn rgba_to_bgra_bytes(image: &RgbaImage) -> Vec<u8> {
    let mut converted = Vec::with_capacity(image.as_raw().len());
    for pixel in image.pixels() {
        converted.extend_from_slice(&[pixel[2], pixel[1], pixel[0], pixel[3]]);
    }
    converted
}

fn normalized_ocr_engine_cache_key(language_tag: &str) -> Option<String> {
    let language_tag = language_tag.trim();
    (!language_tag.is_empty()).then(|| format!("tag:{}", language_tag.to_ascii_lowercase()))
}

fn ocr_profile_language_signature(tags: &[String]) -> String {
    tags.iter()
        .map(|tag| format!("{}:{}", tag.len(), tag.to_ascii_lowercase()))
        .collect::<Vec<_>>()
        .join("|")
}

fn record_auto_profile_resolution(
    state: &mut Option<(String, String)>,
    signature: String,
    resolved_key: String,
) {
    *state = Some((signature, resolved_key));
}

#[cfg(windows)]
mod windows_backend {
    use super::*;
    use std::{collections::VecDeque, sync::Mutex};
    use windows::{
        Globalization::Language,
        Graphics::Imaging::{BitmapAlphaMode, BitmapPixelFormat, SoftwareBitmap},
        Media::Ocr::{OcrEngine, OcrResult},
        Security::Cryptography::CryptographicBuffer,
        System::UserProfile::GlobalizationPreferences,
        Win32::System::WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
        core::HSTRING,
    };

    const MAX_CACHED_OCR_ENGINES: usize = 4;

    /// Native OCR adapter. Projected `OcrEngine` values are Send + Sync; a small
    /// mutex-protected LRU therefore safely reuses lazily created language
    /// engines across executor and authoring worker threads.
    pub struct WindowsOcrBackend {
        engines: Mutex<VecDeque<(String, OcrEngine)>>,
        auto_profile: Mutex<Option<(String, String)>>,
    }

    impl Default for WindowsOcrBackend {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WindowsOcrBackend {
        pub fn new() -> Self {
            Self {
                engines: Mutex::new(VecDeque::new()),
                auto_profile: Mutex::new(None),
            }
        }

        fn cached_engine(
            &self,
            key: &str,
            requested: &MkOcrLanguage,
        ) -> ExecResult<Option<OcrEngine>> {
            let mut engines = self.engines.lock().map_err(|_| {
                diagnostic(
                    "lock engine cache",
                    requested,
                    "OCR engine cache was poisoned",
                )
            })?;
            if let Some(index) = engines.iter().position(|(cached, _)| cached == key) {
                let entry = engines.remove(index).ok_or_else(|| {
                    diagnostic("read engine cache", requested, "OCR engine cache changed")
                })?;
                let cached = entry.1.clone();
                engines.push_back(entry);
                return Ok(Some(cached));
            }
            Ok(None)
        }

        fn cache_engine(
            &self,
            key: String,
            engine: OcrEngine,
            requested: &MkOcrLanguage,
        ) -> ExecResult<OcrEngine> {
            let mut engines = self.engines.lock().map_err(|_| {
                diagnostic(
                    "lock engine cache",
                    requested,
                    "OCR engine cache was poisoned",
                )
            })?;
            // A racing thread may have populated the same resolved language.
            // Prefer that entry and keep the LRU free of duplicates.
            if let Some(index) = engines.iter().position(|(cached, _)| cached == &key) {
                let entry = engines.remove(index).ok_or_else(|| {
                    diagnostic("read engine cache", requested, "OCR engine cache changed")
                })?;
                let cached = entry.1.clone();
                engines.push_back(entry);
                return Ok(cached);
            }
            while engines.len() >= MAX_CACHED_OCR_ENGINES {
                engines.pop_front();
            }
            engines.push_back((key, engine.clone()));
            Ok(engine)
        }

        fn engine(&self, language: &MkOcrLanguage) -> ExecResult<OcrEngine> {
            match language {
                MkOcrLanguage::Auto => {
                    // A cheap ordered profile-language signature invalidates
                    // Auto without constructing a recognizer for every tile or
                    // poll. Engines remain keyed by the resolved canonical tag.
                    let signature = user_profile_language_signature(language)?;
                    let cached_key = self
                        .auto_profile
                        .lock()
                        .map_err(|_| {
                            diagnostic(
                                "lock profile cache",
                                language,
                                "OCR profile cache was poisoned",
                            )
                        })?
                        .as_ref()
                        .and_then(|(cached_signature, key)| {
                            (cached_signature == &signature).then(|| key.clone())
                        });
                    if let Some(key) = cached_key
                        && let Some(cached) = self.cached_engine(&key, language)?
                    {
                        return Ok(cached);
                    }
                    let engine =
                        OcrEngine::TryCreateFromUserProfileLanguages().map_err(|error| {
                            diagnostic("resolve user-profile language", language, error)
                        })?;
                    let resolved_tag = engine
                        .RecognizerLanguage()
                        .and_then(|resolved| resolved.LanguageTag())
                        .map_err(|error| diagnostic("read resolved language", language, error))?
                        .to_string();
                    let key = normalized_ocr_engine_cache_key(&resolved_tag).ok_or_else(|| {
                        diagnostic(
                            "read resolved language",
                            language,
                            "OCR recognizer returned an empty language tag",
                        )
                    })?;
                    let engine = match self.cached_engine(&key, language)? {
                        Some(cached) => cached,
                        None => self.cache_engine(key.clone(), engine, language)?,
                    };
                    let mut profile = self.auto_profile.lock().map_err(|_| {
                        diagnostic(
                            "lock profile cache",
                            language,
                            "OCR profile cache was poisoned",
                        )
                    })?;
                    record_auto_profile_resolution(&mut profile, signature, key);
                    Ok(engine)
                }
                MkOcrLanguage::LanguageTag(tag) => {
                    let language_value = Language::CreateLanguage(&HSTRING::from(tag))
                        .map_err(|error| diagnostic("parse language tag", language, error))?;
                    let supported = OcrEngine::IsLanguageSupported(&language_value)
                        .map_err(|error| diagnostic("check language support", language, error))?;
                    if !supported {
                        return Err(ExecutionDiagnostic::new(
                            DiagnosticKind::TargetNotFound,
                            format!("OCR language '{tag}' is not installed or supported"),
                        )
                        .context("backend", "windows.media.ocr")
                        .context("operation", "resolve language")
                        .context("language", tag));
                    }
                    let canonical_tag = language_value
                        .LanguageTag()
                        .map_err(|error| diagnostic("read language tag", language, error))?
                        .to_string();
                    let key = normalized_ocr_engine_cache_key(&canonical_tag).ok_or_else(|| {
                        diagnostic(
                            "read language tag",
                            language,
                            "OCR language resolved to an empty tag",
                        )
                    })?;
                    if let Some(cached) = self.cached_engine(&key, language)? {
                        return Ok(cached);
                    }
                    let engine = OcrEngine::TryCreateFromLanguage(&language_value)
                        .map_err(|error| diagnostic("create language engine", language, error))?;
                    self.cache_engine(key, engine, language)
                }
            }
        }
    }

    fn user_profile_language_signature(language: &MkOcrLanguage) -> ExecResult<String> {
        let languages = GlobalizationPreferences::Languages()
            .map_err(|error| diagnostic("read profile languages", language, error))?;
        let count = languages
            .Size()
            .map_err(|error| diagnostic("count profile languages", language, error))?;
        let mut tags = Vec::with_capacity(count as usize);
        for index in 0..count {
            tags.push(
                languages
                    .GetAt(index)
                    .map_err(|error| diagnostic("read profile language", language, error))?
                    .to_string(),
            );
        }
        Ok(ocr_profile_language_signature(&tags))
    }

    impl OcrBackend for WindowsOcrBackend {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            let _winrt = initialize_winrt(&MkOcrLanguage::Auto, "enumerate languages")?;
            let languages = OcrEngine::AvailableRecognizerLanguages()
                .map_err(|error| diagnostic("enumerate languages", &MkOcrLanguage::Auto, error))?;
            let language_count = languages
                .Size()
                .map_err(|error| diagnostic("count languages", &MkOcrLanguage::Auto, error))?;
            let mut result = Vec::with_capacity(language_count as usize);
            for index in 0..language_count {
                let language = languages.GetAt(index).map_err(|error| {
                    diagnostic("read language", &MkOcrLanguage::Auto, error)
                        .context("language_index", index.to_string())
                })?;
                result.push(OcrLanguageInfo {
                    tag: language
                        .LanguageTag()
                        .map_err(|error| {
                            diagnostic("read language tag", &MkOcrLanguage::Auto, error)
                        })?
                        .to_string(),
                    display_name: language
                        .DisplayName()
                        .map_err(|error| {
                            diagnostic("read language name", &MkOcrLanguage::Auto, error)
                        })?
                        .to_string(),
                });
            }
            result.sort_by(|left, right| {
                left.tag
                    .to_ascii_lowercase()
                    .cmp(&right.tag.to_ascii_lowercase())
                    .then(left.tag.cmp(&right.tag))
            });
            Ok(result)
        }

        fn max_image_dimension(&self) -> ExecResult<u32> {
            let _winrt = initialize_winrt(&MkOcrLanguage::Auto, "maximum image dimension")?;
            OcrEngine::MaxImageDimension()
                .map_err(|error| diagnostic("maximum image dimension", &MkOcrLanguage::Auto, error))
        }

        fn recognize(
            &self,
            image: &RgbaImage,
            language: &MkOcrLanguage,
            cancelled: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            check_cancelled(cancelled)?;
            if image.width() == 0 || image.height() == 0 {
                return Err(diagnostic("validate image", language, "OCR image is empty"));
            }
            let width = i32::try_from(image.width()).map_err(|_| {
                diagnostic(
                    "convert image width",
                    language,
                    "OCR image width exceeds i32",
                )
            })?;
            let height = i32::try_from(image.height()).map_err(|_| {
                diagnostic(
                    "convert image height",
                    language,
                    "OCR image height exceeds i32",
                )
            })?;
            let _winrt = initialize_winrt(language, "recognize")?;
            let engine = self.engine(language)?;
            let recognized_language = engine
                .RecognizerLanguage()
                .and_then(|language| language.LanguageTag())
                .map_err(|error| diagnostic("read recognizer language", language, error))?
                .to_string();
            let pixels = rgba_to_bgra_bytes(image);
            u32::try_from(pixels.len()).map_err(|_| {
                diagnostic(
                    "create pixel buffer",
                    language,
                    "OCR BGRA buffer exceeds the WinRT buffer length limit",
                )
            })?;
            let buffer = CryptographicBuffer::CreateFromByteArray(&pixels)
                .map_err(|error| diagnostic("create pixel buffer", language, error))?;
            let bitmap = SoftwareBitmap::CreateCopyWithAlphaFromBuffer(
                &buffer,
                BitmapPixelFormat::Bgra8,
                width,
                height,
                BitmapAlphaMode::Ignore,
            )
            .map_err(|error| diagnostic("create software bitmap", language, error))?;
            check_cancelled(cancelled)?;
            let recognized = engine
                .RecognizeAsync(&bitmap)
                .and_then(|operation| operation.get());
            let _ = bitmap.Close();
            let recognized = recognized.map_err(|error| {
                diagnostic("recognize image", language, error)
                    .context("image_width", image.width().to_string())
                    .context("image_height", image.height().to_string())
            })?;
            check_cancelled(cancelled)?;
            map_result(
                recognized,
                recognized_language,
                image.dimensions(),
                language,
            )
        }
    }

    fn map_result(
        result: OcrResult,
        recognized_language: String,
        dimensions: (u32, u32),
        requested_language: &MkOcrLanguage,
    ) -> ExecResult<OcrDocument> {
        let native_lines = result
            .Lines()
            .map_err(|error| diagnostic("read OCR lines", requested_language, error))?;
        let line_count = native_lines
            .Size()
            .map_err(|error| diagnostic("count OCR lines", requested_language, error))?;
        let mut lines = Vec::with_capacity(line_count as usize);
        for line_index in 0..line_count {
            let native_line = native_lines.GetAt(line_index).map_err(|error| {
                diagnostic("read OCR line", requested_language, error)
                    .context("line_index", line_index.to_string())
            })?;
            let line_text = native_line
                .Text()
                .map_err(|error| diagnostic("read OCR line text", requested_language, error))?
                .to_string();
            let native_words = native_line
                .Words()
                .map_err(|error| diagnostic("read OCR words", requested_language, error))?;
            let word_count = native_words
                .Size()
                .map_err(|error| diagnostic("count OCR words", requested_language, error))?;
            let mut words = Vec::with_capacity(word_count as usize);
            for word_index in 0..word_count {
                let native_word = native_words.GetAt(word_index).map_err(|error| {
                    diagnostic("read OCR word", requested_language, error)
                        .context("line_index", line_index.to_string())
                        .context("word_index", word_index.to_string())
                })?;
                let bounds = native_word.BoundingRect().map_err(|error| {
                    diagnostic("read OCR word bounds", requested_language, error)
                })?;
                let Some(bounds) = rounded_clamped_ocr_bounds(
                    bounds.X,
                    bounds.Y,
                    bounds.Width,
                    bounds.Height,
                    dimensions.0,
                    dimensions.1,
                ) else {
                    continue;
                };
                let text = native_word
                    .Text()
                    .map_err(|error| diagnostic("read OCR word text", requested_language, error))?
                    .to_string();
                if !text.is_empty() {
                    words.push(OcrWord { text, bounds });
                }
            }
            lines.push(OcrLine {
                text: line_text,
                words,
            });
        }
        Ok(OcrDocument {
            recognized_language: Some(recognized_language),
            image_width: dimensions.0,
            image_height: dimensions.1,
            lines,
        })
    }

    fn diagnostic(
        operation: &'static str,
        language: &MkOcrLanguage,
        error: impl fmt::Display,
    ) -> ExecutionDiagnostic {
        ExecutionDiagnostic::new(
            DiagnosticKind::Backend,
            format!("Windows OCR {operation} failed: {error}"),
        )
        .context("backend", "windows.media.ocr")
        .context("operation", operation)
        .context("language", language_label(language))
    }

    fn language_label(language: &MkOcrLanguage) -> String {
        match language {
            MkOcrLanguage::Auto => "auto".into(),
            MkOcrLanguage::LanguageTag(tag) => tag.clone(),
        }
    }

    struct WinRtGuard;
    impl Drop for WinRtGuard {
        fn drop(&mut self) {
            unsafe { RoUninitialize() };
        }
    }

    fn initialize_winrt(
        language: &MkOcrLanguage,
        operation: &'static str,
    ) -> ExecResult<WinRtGuard> {
        // Initialization is per-thread. Every successful call, including an
        // already-initialized MTA, is balanced by the guard.
        unsafe { RoInitialize(RO_INIT_MULTITHREADED) }
            .map_err(|error| diagnostic(operation, language, error))?;
        Ok(WinRtGuard)
    }
}

#[cfg(windows)]
pub use windows_backend::WindowsOcrBackend;

#[cfg(test)]
mod tests {
    use super::*;
    use image::Rgba;
    use std::{
        collections::VecDeque,
        sync::{
            Arc, Mutex,
            atomic::{AtomicBool, AtomicUsize, Ordering as AtomicOrdering},
        },
    };

    fn word(text: &str, x: i32, y: i32, width: u32) -> OcrWord {
        OcrWord {
            text: text.into(),
            bounds: ScreenRect::new(x, y, width, 10),
        }
    }

    fn document() -> OcrDocument {
        OcrDocument {
            recognized_language: Some("en-US".into()),
            image_width: 300,
            image_height: 100,
            lines: vec![
                OcrLine {
                    text: "Build   completed".into(),
                    words: vec![word("Build", 10, 4, 30), word("completed", 50, 4, 60)],
                },
                OcrLine {
                    text: "successfully — Καλημέρα cat concatenate CAT.".into(),
                    words: vec![
                        word("successfully", 10, 24, 70),
                        word("—", 85, 24, 5),
                        word("Καλημέρα", 95, 24, 55),
                        word("cat", 155, 24, 18),
                        word("concatenate", 178, 24, 65),
                        word("CAT.", 248, 24, 30),
                    ],
                },
            ],
        }
    }

    #[test]
    fn document_text_preserves_lines_unicode_and_only_trims_outer_space() {
        let mut document = document();
        document.lines[0].text = "  Build completed  ".into();
        assert_eq!(
            document.recognized_text(),
            "Build completed  \nsuccessfully — Καλημέρα cat concatenate CAT."
        );
    }

    #[test]
    fn normalization_maps_unicode_words_across_lines() {
        let normalized = document().normalized_text();
        assert_eq!(
            normalized.text,
            "Build completed successfully — Καλημέρα cat concatenate CAT."
        );
        assert_eq!(normalized.spans.len(), 8);
        assert_eq!(
            &normalized.text[normalized.spans[4].range.clone()],
            "Καλημέρα"
        );
        assert_eq!(normalized.spans[4].line_index, 1);
        assert_eq!(normalized.spans[4].word_index, 2);
    }

    #[test]
    fn contains_is_case_aware_preserves_recognized_text_and_unions_bounds() {
        let insensitive = search_document(
            &document(),
            "build completed successfully",
            MkOcrMatchMode::Contains,
            false,
            MkOcrOccurrence::First,
        )
        .unwrap();
        let matched = insensitive.selected.unwrap();
        assert_eq!(matched.text, "Build completed successfully");
        assert_eq!(matched.bounds, ScreenRect::new(10, 4, 100, 30));
        assert_eq!(matched.center, MkPoint { x: 60, y: 19 });
        assert!(
            search_document(
                &document(),
                "build",
                MkOcrMatchMode::Contains,
                true,
                MkOcrOccurrence::First,
            )
            .unwrap()
            .selected
            .is_none()
        );
    }

    #[test]
    fn whole_word_phrase_rejects_substrings_and_accepts_punctuation() {
        let result = search_document(
            &document(),
            "cat",
            MkOcrMatchMode::WholeWordPhrase,
            false,
            MkOcrOccurrence::First,
        )
        .unwrap();
        assert_eq!(result.match_count, 2);
        assert_eq!(result.selected.unwrap().text, "cat");
    }

    #[test]
    fn regex_handles_unicode_case_occurrences_and_invalid_patterns() {
        let result = search_document(
            &document(),
            r"cat\.?(?: concatenate)?",
            MkOcrMatchMode::Regex,
            false,
            MkOcrOccurrence::Nth(2),
        )
        .unwrap();
        assert_eq!(result.match_count, 2);
        assert_eq!(result.selected.unwrap().text, "CAT.");
        assert!(matches!(
            search_document(
                &document(),
                "(",
                MkOcrMatchMode::Regex,
                true,
                MkOcrOccurrence::First,
            ),
            Err(OcrMatchError::InvalidRegex(_))
        ));
    }

    #[test]
    fn nth_beyond_count_preserves_total_and_zero_width_regex_has_no_coordinates() {
        let beyond = search_document(
            &document(),
            "cat",
            MkOcrMatchMode::Contains,
            false,
            MkOcrOccurrence::Nth(5),
        )
        .unwrap();
        assert_eq!(beyond.match_count, 3);
        assert!(beyond.selected.is_none());

        let zero_width = search_document(
            &document(),
            r"^|$",
            MkOcrMatchMode::Regex,
            true,
            MkOcrOccurrence::First,
        )
        .unwrap();
        assert_eq!(zero_width, OcrSearchResult::default());
    }

    #[test]
    fn empty_documents_and_queries_have_no_matches() {
        assert_eq!(
            search_document(
                &OcrDocument::default(),
                "anything",
                MkOcrMatchMode::Contains,
                false,
                MkOcrOccurrence::First,
            )
            .unwrap(),
            OcrSearchResult::default()
        );
        assert_eq!(
            search_document(
                &document(),
                " \t\n",
                MkOcrMatchMode::Contains,
                false,
                MkOcrOccurrence::First,
            )
            .unwrap(),
            OcrSearchResult::default()
        );
    }

    #[test]
    fn tile_plan_covers_every_pixel_and_clamps_final_tiles() {
        assert_eq!(
            plan_ocr_tiles(100, 100, 100).unwrap(),
            vec![OcrTile {
                rect: ScreenRect::new(0, 0, 100, 100)
            }]
        );
        let tiles = plan_ocr_tiles(250, 210, 100).unwrap();
        assert_eq!(tiles.len(), 9);
        assert_eq!(tiles[0].rect, ScreenRect::new(0, 0, 100, 100));
        assert_eq!(tiles[8].rect, ScreenRect::new(150, 150, 100, 60));
        assert!(tiles.iter().all(|tile| {
            !tile.rect.is_empty() && tile.rect.width <= 100 && tile.rect.height <= 100
        }));
        let mut covered = vec![false; 250 * 210];
        for tile in &tiles {
            for y in tile.rect.y as usize..tile.rect.bottom() as usize {
                for x in tile.rect.x as usize..tile.rect.right() as usize {
                    covered[y * 250 + x] = true;
                }
            }
        }
        assert!(covered.into_iter().all(|covered| covered));
        assert_eq!(plan_ocr_tiles(101, 100, 100).unwrap().len(), 2);
        assert_eq!(plan_ocr_tiles(100, 101, 100).unwrap().len(), 2);
        assert_eq!(plan_ocr_tiles(101, 101, 100).unwrap().len(), 4);
        assert!(plan_ocr_tiles(0, 1, 100).is_err());
        assert!(plan_ocr_tiles(1, 1, 0).is_err());
    }

    #[test]
    fn float_bounds_use_floor_ceil_clamping_and_reject_invalid_rectangles() {
        assert_eq!(
            rounded_clamped_ocr_bounds(1.8, 2.2, 3.3, 4.1, 10, 10),
            Some(ScreenRect::new(1, 2, 5, 5))
        );
        assert_eq!(
            rounded_clamped_ocr_bounds(-2.0, -1.0, 5.2, 3.2, 10, 10),
            Some(ScreenRect::new(0, 0, 4, 3))
        );
        for invalid in [
            rounded_clamped_ocr_bounds(f32::NAN, 0.0, 1.0, 1.0, 10, 10),
            rounded_clamped_ocr_bounds(0.0, 0.0, 0.0, 1.0, 10, 10),
            rounded_clamped_ocr_bounds(20.0, 20.0, 2.0, 2.0, 10, 10),
        ] {
            assert_eq!(invalid, None);
        }
    }

    #[test]
    fn rgba_to_bgra_conversion_is_exact_and_non_mutating() {
        let image = RgbaImage::from_vec(2, 1, vec![1, 2, 3, 4, 10, 20, 30, 40]).unwrap();
        assert_eq!(rgba_to_bgra_bytes(&image), vec![3, 2, 1, 4, 30, 20, 10, 40]);
        assert_eq!(image.as_raw(), &[1, 2, 3, 4, 10, 20, 30, 40]);
    }

    #[test]
    fn engine_cache_keys_use_validated_resolved_language_tags() {
        assert_eq!(
            normalized_ocr_engine_cache_key(" en-US "),
            Some("tag:en-us".into())
        );
        assert_eq!(
            normalized_ocr_engine_cache_key("EN-us"),
            normalized_ocr_engine_cache_key("en-US")
        );
        assert_eq!(normalized_ocr_engine_cache_key("  "), None);
        assert_eq!(
            ocr_profile_language_signature(&["en-US".into(), "fr-FR".into()]),
            "5:en-us|5:fr-fr"
        );
        assert_ne!(
            ocr_profile_language_signature(&["en-US".into(), "fr-FR".into()]),
            ocr_profile_language_signature(&["fr-FR".into(), "en-US".into()])
        );
        let mut profile = Some(("old-profile".into(), "tag:fr-fr".into()));
        record_auto_profile_resolution(&mut profile, "new-profile".into(), "tag:en-us".into());
        assert_eq!(profile, Some(("new-profile".into(), "tag:en-us".into())));
    }

    #[test]
    fn duplicate_detection_is_spatial_and_reconstruction_is_deterministic() {
        let first = word("Delete", -100, 20, 40);
        let overlapping = word("Delete", -98, 21, 40);
        let distinct = word("Delete", 100, 20, 40);
        let other_text = word("Keep", -100, 20, 40);
        assert!(ocr_words_are_duplicates(&first, &overlapping));
        assert!(!ocr_words_are_duplicates(&first, &distinct));
        assert!(!ocr_words_are_duplicates(&first, &other_text));

        let lines = reconstruct_ocr_lines(vec![
            word("second", 40, 32, 30),
            word("line", 10, 32, 20),
            word("right", 50, 4, 30),
            word("left", 10, 5, 25),
        ]);
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].recognized_text(), "left right");
        assert_eq!(lines[1].recognized_text(), "line second");
    }

    struct FakeCapture {
        image: RgbaImage,
        origin: (i32, i32),
        captures: AtomicUsize,
    }

    impl ScreenCaptureBackend for FakeCapture {
        fn virtual_desktop(&self) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(-1_000, -1_000, 2_000, 2_000))
        }

        fn region_bounds(&self, _: &SearchRegion) -> ExecResult<ScreenRect> {
            Ok(ScreenRect::new(
                self.origin.0,
                self.origin.1,
                self.image.width(),
                self.image.height(),
            ))
        }

        fn capture_rect(&self, _: ScreenRect, _: &dyn Fn() -> bool) -> ExecResult<RgbaImage> {
            self.captures.fetch_add(1, AtomicOrdering::SeqCst);
            Ok(self.image.clone())
        }
    }

    struct FakeOcr {
        maximum: u32,
        documents: Mutex<VecDeque<OcrDocument>>,
        calls: Mutex<Vec<((u32, u32), MkOcrLanguage)>>,
        cancel_after_recognition: Option<Arc<AtomicBool>>,
    }

    impl OcrBackend for FakeOcr {
        fn available_languages(&self) -> ExecResult<Vec<OcrLanguageInfo>> {
            Ok(vec![OcrLanguageInfo {
                tag: "en-US".into(),
                display_name: "English (United States)".into(),
            }])
        }

        fn max_image_dimension(&self) -> ExecResult<u32> {
            Ok(self.maximum)
        }

        fn recognize(
            &self,
            image: &RgbaImage,
            language: &MkOcrLanguage,
            _: &dyn Fn() -> bool,
        ) -> ExecResult<OcrDocument> {
            self.calls
                .lock()
                .unwrap()
                .push((image.dimensions(), language.clone()));
            let document = self
                .documents
                .lock()
                .unwrap()
                .pop_front()
                .unwrap_or_default();
            if let Some(cancelled) = &self.cancel_after_recognition {
                cancelled.store(true, AtomicOrdering::SeqCst);
            }
            Ok(document)
        }
    }

    fn tile_document(width: u32, words: Vec<OcrWord>) -> OcrDocument {
        OcrDocument {
            recognized_language: Some("en-US".into()),
            image_width: width,
            image_height: 30,
            lines: vec![OcrLine {
                text: String::new(),
                words,
            }],
        }
    }

    #[test]
    fn recognize_region_captures_once_translates_deduplicates_and_orders() {
        let capture = FakeCapture {
            image: RgbaImage::from_pixel(175, 30, Rgba([1, 2, 3, 255])),
            origin: (-200, -50),
            captures: AtomicUsize::new(0),
        };
        let language = MkOcrLanguage::LanguageTag("en-US".into());
        let backend = FakeOcr {
            maximum: 100,
            documents: Mutex::new(VecDeque::from([
                tile_document(100, vec![word("Left", 10, 5, 30), word("Same", 80, 5, 15)]),
                tile_document(100, vec![word("Same", 5, 5, 15), word("Same", 70, 5, 20)]),
            ])),
            calls: Mutex::new(vec![]),
            cancel_after_recognition: None,
        };

        let result = recognize_region(
            &capture,
            &backend,
            &SearchRegion::Desktop,
            &language,
            &|| false,
        )
        .unwrap();
        assert_eq!(capture.captures.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(result.capture.origin, (-200, -50));
        assert_eq!(result.document.recognized_text(), "Left Same Same");
        assert_eq!(
            result.document.lines[0]
                .words
                .iter()
                .map(|word| (word.text.as_str(), word.bounds.x, word.bounds.y))
                .collect::<Vec<_>>(),
            vec![("Left", -190, -45), ("Same", -120, -45), ("Same", -55, -45)]
        );
        let calls = backend.calls.lock().unwrap();
        assert_eq!(calls.len(), 2);
        assert!(
            calls
                .iter()
                .all(|(dimensions, requested)| *dimensions == (100, 30) && requested == &language)
        );
        let matches = search_document(
            &result.document,
            "Same",
            MkOcrMatchMode::Contains,
            true,
            MkOcrOccurrence::Nth(2),
        )
        .unwrap();
        assert_eq!(matches.match_count, 2);
        assert_eq!(matches.selected.unwrap().center.x, -45);
    }

    #[test]
    fn cancellation_after_backend_call_prevents_further_tiles() {
        let cancelled = Arc::new(AtomicBool::new(false));
        let capture = FakeCapture {
            image: RgbaImage::new(175, 30),
            origin: (0, 0),
            captures: AtomicUsize::new(0),
        };
        let backend = FakeOcr {
            maximum: 100,
            documents: Mutex::new(VecDeque::from([tile_document(100, vec![])])),
            calls: Mutex::new(vec![]),
            cancel_after_recognition: Some(cancelled.clone()),
        };
        let result = recognize_region(
            &capture,
            &backend,
            &SearchRegion::Desktop,
            &MkOcrLanguage::Auto,
            &|| cancelled.load(AtomicOrdering::SeqCst),
        );
        assert_eq!(result.unwrap_err().kind, DiagnosticKind::Cancelled);
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
        assert_eq!(capture.captures.load(AtomicOrdering::SeqCst), 1);
    }

    #[test]
    fn unsupported_backend_is_explicit_and_cancellation_wins() {
        let backend = UnsupportedOcrBackend;
        let unavailable = backend.available_languages().unwrap_err();
        assert_eq!(unavailable.kind, DiagnosticKind::UnsupportedOperation);
        assert_eq!(unavailable.context.get("backend").unwrap(), "ocr");
        let cancelled = backend
            .recognize(&RgbaImage::new(1, 1), &MkOcrLanguage::Auto, &|| true)
            .unwrap_err();
        assert_eq!(cancelled.kind, DiagnosticKind::Cancelled);
    }

    #[test]
    fn fake_backend_exposes_languages_maximum_calls_and_preflight_cancellation() {
        let backend = FakeOcr {
            maximum: 321,
            documents: Mutex::new(VecDeque::from([OcrDocument::default()])),
            calls: Mutex::new(vec![]),
            cancel_after_recognition: None,
        };
        assert_eq!(
            backend.available_languages().unwrap(),
            vec![OcrLanguageInfo {
                tag: "en-US".into(),
                display_name: "English (United States)".into(),
            }]
        );
        assert_eq!(backend.max_image_dimension().unwrap(), 321);
        backend
            .recognize(
                &RgbaImage::new(7, 9),
                &MkOcrLanguage::LanguageTag("el-GR".into()),
                &|| false,
            )
            .unwrap();
        assert_eq!(
            *backend.calls.lock().unwrap(),
            vec![((7, 9), MkOcrLanguage::LanguageTag("el-GR".into()))]
        );

        let capture = FakeCapture {
            image: RgbaImage::new(10, 10),
            origin: (-10, -20),
            captures: AtomicUsize::new(0),
        };
        let error = recognize_region(
            &capture,
            &backend,
            &SearchRegion::Desktop,
            &MkOcrLanguage::Auto,
            &|| true,
        )
        .unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::Cancelled);
        assert_eq!(capture.captures.load(AtomicOrdering::SeqCst), 0);
        assert_eq!(backend.calls.lock().unwrap().len(), 1);
    }

    #[test]
    fn matching_reports_exact_unicode_ranges_multiline_geometry_and_all_occurrences() {
        let document = document();
        let greek = search_document(
            &document,
            "καλημέρα CAT",
            MkOcrMatchMode::Contains,
            false,
            MkOcrOccurrence::First,
        )
        .unwrap();
        let selected = greek.selected.unwrap();
        assert_eq!(selected.text, "Καλημέρα cat");
        assert_eq!(selected.normalized_range, 33..53);
        assert_eq!(selected.bounds, ScreenRect::new(95, 24, 78, 10));

        let multiline = search_document(
            &document,
            r"completed\s+successfully",
            MkOcrMatchMode::Regex,
            true,
            MkOcrOccurrence::First,
        )
        .unwrap();
        assert_eq!(multiline.match_count, 1);
        assert_eq!(
            multiline.selected.unwrap().bounds,
            ScreenRect::new(10, 4, 100, 30)
        );

        for (occurrence, expected_text) in [
            (MkOcrOccurrence::First, "cat"),
            (MkOcrOccurrence::Nth(2), "cat"),
            (MkOcrOccurrence::Nth(3), "CAT"),
        ] {
            let result = search_document(
                &document,
                "cat",
                MkOcrMatchMode::Contains,
                false,
                occurrence,
            )
            .unwrap();
            assert_eq!(result.match_count, 3);
            assert_eq!(result.selected.unwrap().text, expected_text);
        }
    }

    #[test]
    fn tile_plans_have_required_overlap_and_partial_edges_on_each_axis() {
        for (width, height, expected) in [(175, 30, 2), (30, 175, 2), (175, 175, 4)] {
            let tiles = plan_ocr_tiles(width, height, 100).unwrap();
            assert_eq!(tiles.len(), expected);
            assert_eq!(tiles[0].rect.x, 0);
            assert_eq!(tiles[0].rect.y, 0);
            assert!(
                tiles
                    .iter()
                    .any(|tile| tile.rect.right() == i64::from(width))
            );
            assert!(
                tiles
                    .iter()
                    .any(|tile| tile.rect.bottom() == i64::from(height))
            );
            for pair in tiles.windows(2) {
                if pair[0].rect.y == pair[1].rect.y {
                    assert!(pair[0].rect.right() > i64::from(pair[1].rect.x));
                }
            }
        }
    }

    #[test]
    fn reconstructed_cross_tile_words_support_phrase_matching_in_reading_order() {
        let lines = reconstruct_ocr_lines(vec![
            word("second", -20, -30, 30),
            word("first", -70, -30, 35),
            word("below", -70, -5, 35),
        ]);
        let document = OcrDocument {
            image_width: 200,
            image_height: 100,
            lines,
            ..Default::default()
        };
        assert_eq!(document.recognized_text(), "first second\nbelow");
        let result = search_document(
            &document,
            "first second below",
            MkOcrMatchMode::WholeWordPhrase,
            true,
            MkOcrOccurrence::First,
        )
        .unwrap();
        assert_eq!(result.match_count, 1);
        assert_eq!(
            result.selected.unwrap().bounds,
            ScreenRect::new(-70, -30, 80, 35)
        );
    }
}
