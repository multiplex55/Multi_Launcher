#![expect(clippy::mem_forget)]

use ecolor::Color32;
use emath::{GuiRounding as _, OrderedFloat, Vec2, vec2};
use self_cell::self_cell;
use skrifa::{GlyphId, MetadataProvider as _};
use std::{collections::BTreeMap, sync::Arc};
use vello_cpu::{color, kurbo};

use crate::{
    TextOptions, TextureAtlas,
    text::{
        FontTweak, VariationCoords,
        fonts::{Blob, CachedFamily, FontFaceKey},
    },
};

// ----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Deserialize, serde::Serialize))]
pub struct UvRect {
    /// X/Y offset for nice rendering (unit: points).
    pub offset: Vec2,

    /// Screen size (in points) of this glyph.
    /// Note that the height is different from the font height.
    pub size: Vec2,

    /// Top left corner UV in texture.
    pub min: [u16; 2],

    /// Bottom right corner (exclusive).
    pub max: [u16; 2],
}

impl UvRect {
    pub fn is_nothing(&self) -> bool {
        self.min == self.max
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct GlyphInfo {
    /// Doesn't need to be unique.
    ///
    /// Is `None` for a special "invisible" glyph.
    pub(crate) id: Option<GlyphId>,

    /// In [`skrifa`]s "unscaled" coordinate system.
    pub advance_width_unscaled: OrderedFloat<f32>,
}

impl GlyphInfo {
    /// A valid, but invisible, glyph of zero-width.
    pub const INVISIBLE: Self = Self {
        id: None,
        advance_width_unscaled: OrderedFloat(0.0),
    };
}

/// Result of resolving a `char` to a [`GlyphId`] within a single [`FontFace`].
///
/// Location-independent: only depends on the font's charmap and `FontTweak`,
/// not on variable-font variation coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum GlyphIdResolution {
    /// A real, visible glyph.
    Glyph(GlyphId),

    /// A valid char, but rendered as zero-width (control chars, joiners, …).
    Invisible,
}

/// A precomputed hash of a [`skrifa::instance::Location`].
///
/// Used as a cache key so that we don't have to re-hash the coordinate list
/// for every glyph lookup. Compute once per text run and reuse for every glyph
/// in the run.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct LocationHash(u64);

impl nohash_hasher::IsEnabled for LocationHash {}

impl LocationHash {
    #[inline]
    pub fn new(location: &skrifa::instance::Location) -> Self {
        if location.coords().is_empty() {
            // Fast path for the (common) default-coords case.
            Self(0)
        } else {
            Self(crate::util::hash(location))
        }
    }
}

// Subpixel binning, taken from cosmic-text:
// https://github.com/pop-os/cosmic-text/blob/974ddaed96b334f560b606ebe5d2ca2d2f9f23ef/src/glyph_cache.rs

/// Bin for subpixel positioning of glyphs.
///
/// For accurate glyph positioning, we want to render each glyph at a subpixel coordinate. However, we also want to
/// cache each glyph's bitmap. As a compromise, we bin each subpixel offset into one of four fractional values. This
/// means one glyph can have up to four subpixel-positioned bitmaps in the cache.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub(super) enum SubpixelBin {
    #[default]
    Zero,
    One,
    Two,
    Three,
}

impl SubpixelBin {
    /// Bin the given position and return the new integral coordinate.
    fn new(pos: f32) -> (i32, Self) {
        let trunc = pos as i32;
        let fract = pos - trunc as f32;

        if pos.is_sign_negative() {
            if fract > -0.125 {
                (trunc, Self::Zero)
            } else if fract > -0.375 {
                (trunc - 1, Self::Three)
            } else if fract > -0.625 {
                (trunc - 1, Self::Two)
            } else if fract > -0.875 {
                (trunc - 1, Self::One)
            } else {
                (trunc - 1, Self::Zero)
            }
        } else {
            if fract < 0.125 {
                (trunc, Self::Zero)
            } else if fract < 0.375 {
                (trunc, Self::One)
            } else if fract < 0.625 {
                (trunc, Self::Two)
            } else if fract < 0.875 {
                (trunc, Self::Three)
            } else {
                (trunc + 1, Self::Zero)
            }
        }
    }

    pub fn as_float(&self) -> f32 {
        match self {
            Self::Zero => 0.0,
            Self::One => 0.25,
            Self::Two => 0.5,
            Self::Three => 0.75,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GlyphAllocation {
    /// UV rectangle for drawing.
    pub uv_rect: UvRect,
}

#[derive(Hash, PartialEq, Eq)]
struct GlyphCacheKey(u64);

impl nohash_hasher::IsEnabled for GlyphCacheKey {}

impl GlyphCacheKey {
    #[inline]
    fn new(glyph_id: GlyphId, metrics: &StyledMetrics, bin: SubpixelBin) -> Self {
        let StyledMetrics {
            pixels_per_point,
            px_scale_factor,
            location_hash,
            ..
        } = *metrics;
        debug_assert!(
            0.0 < pixels_per_point && pixels_per_point.is_finite(),
            "Bad pixels_per_point {pixels_per_point}"
        );
        debug_assert!(
            0.0 < px_scale_factor && px_scale_factor.is_finite(),
            "Bad px_scale_factor: {px_scale_factor}"
        );
        Self(crate::util::hash((
            glyph_id,
            pixels_per_point.to_bits(),
            px_scale_factor.to_bits(),
            bin,
            location_hash,
        )))
    }
}

// ----------------------------------------------------------------------------

struct DependentFontData<'a> {
    skrifa: skrifa::FontRef<'a>,
    charmap: skrifa::charmap::Charmap<'a>,
    outline_glyphs: skrifa::outline::OutlineGlyphCollection<'a>,
    metrics: skrifa::metrics::Metrics,
    hinting_instance: Option<skrifa::outline::HintingInstance>,
}

self_cell! {
    struct FontCell {
        owner: Blob,

        #[covariant]
        dependent: DependentFontData,
    }
}

impl FontCell {
    fn px_scale_factor(&self, scale: f32) -> f32 {
        let units_per_em = self.borrow_dependent().metrics.units_per_em as f32;
        scale / units_per_em
    }

    fn allocate_glyph_uncached(
        &mut self,
        atlas: &mut TextureAtlas,
        metrics: &StyledMetrics,
        glyph_id: GlyphId,
        bin: SubpixelBin,
        location: skrifa::instance::LocationRef<'_>,
        hinting_target: skrifa::outline::Target,
    ) -> Option<GlyphAllocation> {
        debug_assert!(
            glyph_id != skrifa::GlyphId::NOTDEF,
            "Can't allocate glyph for id 0"
        );

        let mut path = kurbo::BezPath::new();
        let mut pen = VelloPen {
            path: &mut path,
            x_offset: bin.as_float() as f64,
        };

        self.with_dependent_mut(|_, font_data| {
            let outline = font_data.outline_glyphs.get(glyph_id)?;

            if let Some(hinting_instance) = &mut font_data.hinting_instance {
                let size = skrifa::instance::Size::new(metrics.scale);
                if hinting_instance.size() != size
                    || hinting_instance.location().coords() != location.coords()
                    || hinting_instance.target() != hinting_target
                {
                    hinting_instance
                        .reconfigure(&font_data.outline_glyphs, size, location, hinting_target)
                        .ok()?;
                }
                let draw_settings = skrifa::outline::DrawSettings::hinted(hinting_instance, false);
                outline.draw(draw_settings, &mut pen).ok()?;
            } else {
                let draw_settings = skrifa::outline::DrawSettings::unhinted(
                    skrifa::instance::Size::new(metrics.scale),
                    location,
                );
                outline.draw(draw_settings, &mut pen).ok()?;
            }

            Some(())
        })?;

        let bounds = path.control_box().expand();
        let width = bounds.width() as u16;
        let height = bounds.height() as u16;

        let uv_rect = if width == 0 || height == 0 {
            UvRect::default()
        } else {
            let mut ctx = vello_cpu::RenderContext::new(width, height);
            ctx.set_transform(kurbo::Affine::translate((-bounds.x0, -bounds.y0)));
            ctx.set_paint(color::OpaqueColor::<color::Srgb>::WHITE);
            ctx.fill_path(&path);
            let mut dest = vello_cpu::Pixmap::new(width, height);
            let mut resources = vello_cpu::Resources::new();
            ctx.render(&mut dest, &mut resources);

            let glyph_pos = {
                let color_transfer_function = atlas.options().color_transfer_function;
                let (glyph_pos, image) = atlas.allocate((width as usize, height as usize));
                let pixels = dest.data_as_u8_slice();
                for y in 0..height as usize {
                    for x in 0..width as usize {
                        let pixel_offset = 4 * ((y * width as usize) + x);
                        image[(x + glyph_pos.0, y + glyph_pos.1)] = color_transfer_function
                            .to_atlas_color(Color32::from_rgba_premultiplied(
                                pixels[pixel_offset],
                                pixels[pixel_offset + 1],
                                pixels[pixel_offset + 2],
                                pixels[pixel_offset + 3],
                            ));
                    }
                }
                glyph_pos
            };
            let offset_in_pixels = vec2(bounds.x0 as f32, bounds.y0 as f32);
            let offset =
                offset_in_pixels / metrics.pixels_per_point + metrics.y_offset_in_points * Vec2::Y;
            UvRect {
                offset,
                size: vec2(width as f32, height as f32) / metrics.pixels_per_point,
                min: [glyph_pos.0 as u16, glyph_pos.1 as u16],
                max: [
                    (glyph_pos.0 + width as usize) as u16,
                    (glyph_pos.1 + height as usize) as u16,
                ],
            }
        };

        Some(GlyphAllocation { uv_rect })
    }
}

struct VelloPen<'a> {
    path: &'a mut kurbo::BezPath,
    x_offset: f64,
}

impl skrifa::outline::OutlinePen for VelloPen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.path.move_to((x as f64 + self.x_offset, -y as f64));
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.path.line_to((x as f64 + self.x_offset, -y as f64));
    }

    fn quad_to(&mut self, cx0: f32, cy0: f32, x: f32, y: f32) {
        self.path.quad_to(
            (cx0 as f64 + self.x_offset, -cy0 as f64),
            (x as f64 + self.x_offset, -y as f64),
        );
    }

    fn curve_to(&mut self, cx0: f32, cy0: f32, cx1: f32, cy1: f32, x: f32, y: f32) {
        self.path.curve_to(
            (cx0 as f64 + self.x_offset, -cy0 as f64),
            (cx1 as f64 + self.x_offset, -cy1 as f64),
            (x as f64 + self.x_offset, -y as f64),
        );
    }

    fn close(&mut self) {
        self.path.close_path();
    }
}

const RAW_SHAPE_CACHE_MAX_ENTRIES: usize = 128;
const RAW_SHAPE_CACHE_MAX_PAYLOAD_BYTES: usize = 1024 * 1024;
const RAW_SHAPE_CACHE_MAX_TEXT_BYTES: usize = 4 * 1024;
const RAW_SHAPE_CACHE_MAX_ENTRY_BYTES: usize = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShapeBufferProvenance {
    /// `layout_section` uses only a new buffer or one returned by `clear`.
    LayoutOwnedClean,
    /// Callers without the layout lifecycle proof must bypass raw-result reuse.
    Unknown,
}

#[derive(Clone, Copy)]
struct RawShapeCacheKeyRef<'a> {
    text: &'a str,
    script: harfrust::Script,
    direction: harfrust::Direction,
    language: Option<&'a harfrust::Language>,
    flags: u32,
}

struct RawShapeCacheEntry {
    text: String,
    script: harfrust::Script,
    direction: harfrust::Direction,
    language: Option<harfrust::Language>,
    flags: u32,
    glyph_infos: Arc<[harfrust::GlyphInfo]>,
    glyph_positions: Arc<[harfrust::GlyphPosition]>,
    payload_bytes: usize,
}

impl RawShapeCacheKeyRef<'_> {
    fn matches(self, entry: &RawShapeCacheEntry) -> bool {
        self.text == entry.text
            && self.script == entry.script
            && self.direction == entry.direction
            && self.language == entry.language.as_ref()
            && self.flags == entry.flags
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RawShapeCacheBypass {
    TextLimit,
    EntryPayloadLimit,
    ArithmeticOverflow,
    UnsupportedBuffer,
}

#[derive(Default)]
struct RawShapeCache {
    /// The least-recently used entry is first. Entries are owned by one face.
    entries: Vec<RawShapeCacheEntry>,
    payload_bytes: usize,
}

impl RawShapeCache {
    fn lookup(
        &mut self,
        key: RawShapeCacheKeyRef<'_>,
    ) -> Option<(Arc<[harfrust::GlyphInfo]>, Arc<[harfrust::GlyphPosition]>)> {
        let index = self.entries.iter().position(|entry| key.matches(entry))?;
        let entry = self.entries.remove(index);
        let result = (
            Arc::clone(&entry.glyph_infos),
            Arc::clone(&entry.glyph_positions),
        );
        self.entries.push(entry);
        Some(result)
    }

    fn insert(
        &mut self,
        key: RawShapeCacheKeyRef<'_>,
        glyph_infos: &[harfrust::GlyphInfo],
        glyph_positions: &[harfrust::GlyphPosition],
    ) -> Result<usize, RawShapeCacheBypass> {
        if key.text.len() > RAW_SHAPE_CACHE_MAX_TEXT_BYTES {
            return Err(RawShapeCacheBypass::TextLimit);
        }
        if glyph_infos.len() != glyph_positions.len() {
            return Err(RawShapeCacheBypass::UnsupportedBuffer);
        }

        let language_bytes = key.language.map_or(0, |language| language.as_bytes().len());
        let payload_bytes = raw_shape_payload_bytes(
            key.text.len(),
            language_bytes,
            glyph_infos.len(),
            glyph_positions.len(),
        )
        .ok_or(RawShapeCacheBypass::ArithmeticOverflow)?;
        if payload_bytes > RAW_SHAPE_CACHE_MAX_ENTRY_BYTES {
            return Err(RawShapeCacheBypass::EntryPayloadLimit);
        }
        self.payload_bytes
            .checked_add(payload_bytes)
            .ok_or(RawShapeCacheBypass::ArithmeticOverflow)?;

        let mut evictions = 0;
        while self.entries.len() >= RAW_SHAPE_CACHE_MAX_ENTRIES
            || self.payload_bytes > RAW_SHAPE_CACHE_MAX_PAYLOAD_BYTES - payload_bytes
        {
            let oldest = self.entries.remove(0);
            debug_assert!(self.payload_bytes >= oldest.payload_bytes);
            self.payload_bytes -= oldest.payload_bytes;
            evictions += 1;
        }

        self.entries.push(RawShapeCacheEntry {
            text: key.text.to_owned(),
            script: key.script,
            direction: key.direction,
            language: key.language.cloned(),
            flags: key.flags,
            glyph_infos: Arc::from(glyph_infos),
            glyph_positions: Arc::from(glyph_positions),
            payload_bytes,
        });
        self.payload_bytes += payload_bytes;
        Ok(evictions)
    }
}

fn raw_shape_payload_bytes(
    text_bytes: usize,
    language_bytes: usize,
    glyph_info_count: usize,
    glyph_position_count: usize,
) -> Option<usize> {
    text_bytes
        .checked_add(language_bytes)?
        .checked_add(glyph_info_count.checked_mul(std::mem::size_of::<harfrust::GlyphInfo>())?)?
        .checked_add(
            glyph_position_count.checked_mul(std::mem::size_of::<harfrust::GlyphPosition>())?,
        )
}

pub(crate) enum LayoutShapeBuffer {
    GlyphBuffer(harfrust::GlyphBuffer),
    Cached {
        glyph_infos: Arc<[harfrust::GlyphInfo]>,
        glyph_positions: Arc<[harfrust::GlyphPosition]>,
        buffer: harfrust::UnicodeBuffer,
    },
}

impl LayoutShapeBuffer {
    pub(crate) fn glyph_infos(&self) -> &[harfrust::GlyphInfo] {
        match self {
            Self::GlyphBuffer(buffer) => buffer.glyph_infos(),
            Self::Cached { glyph_infos, .. } => glyph_infos,
        }
    }

    pub(crate) fn glyph_positions(&self) -> &[harfrust::GlyphPosition] {
        match self {
            Self::GlyphBuffer(buffer) => buffer.glyph_positions(),
            Self::Cached {
                glyph_positions, ..
            } => glyph_positions,
        }
    }

    pub(crate) fn clear(self) -> harfrust::UnicodeBuffer {
        match self {
            Self::GlyphBuffer(buffer) => buffer.clear(),
            Self::Cached { mut buffer, .. } => {
                buffer.clear();
                buffer
            }
        }
    }
}

/// A specific font face.
/// The interface uses points as the unit for everything.
pub struct FontFace {
    name: String,
    font: FontCell,
    tweak: FontTweak,
    subpixel_binning: bool,

    /// Cached `harfrust` shaper data (parsed GSUB/GPOS tables).
    /// `ShaperData` is `Copy` — lives outside the `self_cell`.
    shaper_data: harfrust::ShaperData,

    /// Bounded FIFO cache of shaping plans for this face's actual shaping inputs.
    shape_plans: Vec<harfrust::ShapePlan>,

    /// Bounded LRU cache of raw shaping results for clean layout-owned buffers.
    raw_shape_cache: RawShapeCache,

    /// Location-independent: `char → GlyphId | Invisible`.
    ///
    /// Only depends on the font's charmap + `FontTweak`. A miss means the char
    /// is not in this face's repertoire and the fallback chain should be tried.
    glyph_id_cache: ahash::HashMap<char, GlyphIdResolution>,

    /// Location-dependent: `(char, LocationHash) → unscaled advance width`.
    ///
    /// Variable fonts can vary advance widths per axis (HVAR table), so this
    /// must be re-keyed per resolved [`skrifa::instance::Location`].
    advance_width_cache: ahash::HashMap<(char, LocationHash), OrderedFloat<f32>>,

    glyph_alloc_cache: ahash::HashMap<GlyphCacheKey, GlyphAllocation>,
}

const SHAPE_PLAN_CACHE_CAPACITY: usize = 8;

fn shape_plan_index(
    plans: &[harfrust::ShapePlan],
    key: &harfrust::ShapePlanKey<'_>,
) -> Option<usize> {
    plans.iter().position(|plan| key.matches(plan))
}

/// Insert a plan in FIFO order, returning whether the oldest plan was evicted.
fn insert_shape_plan(plans: &mut Vec<harfrust::ShapePlan>, plan: harfrust::ShapePlan) -> bool {
    debug_assert!(plans.len() <= SHAPE_PLAN_CACHE_CAPACITY);
    let evicted = plans.len() == SHAPE_PLAN_CACHE_CAPACITY;
    if evicted {
        plans.remove(0);
    }
    plans.push(plan);
    evicted
}

impl FontFace {
    pub fn new(
        options: TextOptions,
        name: String,
        font_data: Blob,
        index: u32,
        tweak: FontTweak,
    ) -> Result<Self, Box<dyn core::error::Error>> {
        let font = FontCell::try_new(font_data, |font_data| {
            let skrifa_font =
                skrifa::FontRef::from_index(AsRef::<[u8]>::as_ref(font_data.as_ref()), index)?;

            let charmap = skrifa_font.charmap();
            let glyphs = skrifa_font.outline_glyphs();

            // Note: We use default location here during initialization because
            // the actual weight will be applied via the stored location during rendering.
            // The metrics won't be significantly different at this unscaled size.
            // TODO(emilk): heed location for vertical metrics too (HVAR/MVAR).
            let metrics = skrifa_font.metrics(
                skrifa::instance::Size::unscaled(),
                skrifa::instance::LocationRef::default(),
            );

            let hinting_enabled = tweak.hinting.unwrap_or(options.font_hinting);
            let hinting_instance = hinting_enabled
                .then(|| {
                    // It doesn't really matter what we put here for options. Since the size is `unscaled()`, we will
                    // always reconfigure this hinting instance with the real options when rendering for the first time.
                    skrifa::outline::HintingInstance::new(
                        &glyphs,
                        skrifa::instance::Size::unscaled(),
                        skrifa::instance::LocationRef::default(),
                        skrifa::outline::Target::default(),
                    )
                    .ok()
                })
                .flatten();

            Ok::<DependentFontData<'_>, Box<dyn core::error::Error>>(DependentFontData {
                skrifa: skrifa_font,
                charmap,
                outline_glyphs: glyphs,
                metrics,
                hinting_instance,
            })
        })?;

        let shaper_data = harfrust::ShaperData::new(&font.borrow_dependent().skrifa);

        let subpixel_binning = tweak.subpixel_binning.unwrap_or(options.subpixel_binning);

        Ok(Self {
            name,
            font,
            tweak,
            subpixel_binning,
            shaper_data,
            shape_plans: Vec::new(),
            raw_shape_cache: RawShapeCache::default(),
            glyph_id_cache: Default::default(),
            advance_width_cache: Default::default(),
            glyph_alloc_cache: Default::default(),
        })
    }

    /// Code points that will always be replaced by the replacement character.
    ///
    /// See also [`invisible_char`].
    fn ignore_character(&self, chr: char) -> bool {
        use crate::text::FontDefinitions;

        if !FontDefinitions::builtin_font_names().contains(&self.name.as_str()) {
            return false;
        }

        matches!(
            chr,
            // Strip out a religious symbol with secondary nefarious interpretation:
            '\u{534d}' | '\u{5350}' |

            // Ignore ubuntu-specific stuff in `Ubuntu-Light.ttf`:
            '\u{E0FF}' | '\u{EFFD}' | '\u{F0FF}' | '\u{F200}'
        )
    }

    /// An un-ordered iterator over all supported characters.
    fn characters(&self) -> impl Iterator<Item = char> + '_ {
        self.font
            .borrow_dependent()
            .charmap
            .mappings()
            .filter_map(|(chr, _)| char::from_u32(chr).filter(|c| !self.ignore_character(*c)))
    }

    /// Resolve a `char` to a [`GlyphId`] within this face.
    ///
    /// Location-independent. Returns `None` when this face cannot represent
    /// the char (the caller should try the fallback chain).
    ///
    /// `\t` and thin spaces share `' '`s glyph id (they just have a custom advance).
    pub(super) fn glyph_id_resolution(&mut self, c: char) -> Option<GlyphIdResolution> {
        if let Some(resolution) = self.glyph_id_cache.get(&c) {
            return Some(*resolution);
        }

        if self.ignore_character(c) {
            return None; // these will result in the replacement character when rendering
        }

        let resolution = if c == '\t' || c == '\u{2009}' || c == '\u{202F}' {
            // `\t` and thin spaces are rendered as a space glyph with a custom advance.
            self.glyph_id_resolution(' ')?
        } else if invisible_char(c) {
            GlyphIdResolution::Invisible
        } else {
            let glyph_id = self
                .font
                .borrow_dependent()
                .charmap
                .map(c)
                .filter(|id| *id != GlyphId::NOTDEF)?;
            GlyphIdResolution::Glyph(glyph_id)
        };

        self.glyph_id_cache.insert(c, resolution);
        Some(resolution)
    }

    /// Unscaled advance width for `c` at the given variation location.
    ///
    /// Location-dependent (variable fonts can vary advances via HVAR).
    /// Cached per `(char, LocationHash)`.
    fn advance_width_unscaled(&mut self, c: char, metrics: &StyledMetrics) -> f32 {
        let cache_key = (c, metrics.location_hash);
        if let Some(advance) = self.advance_width_cache.get(&cache_key) {
            return advance.0;
        }

        let advance = match c {
            '\t' => self.tweak.tab_size * self.advance_width_unscaled(' ', metrics),
            '\u{2009}' | '\u{202F}' => {
                // Thin space (U+2009) and narrow no-break space (U+202F),
                // often used as thousands separator.
                self.tweak.thin_space_width * self.advance_width_unscaled(' ', metrics)
            }
            _ => {
                let Some(GlyphIdResolution::Glyph(glyph_id)) = self.glyph_id_resolution(c) else {
                    return 0.0;
                };
                let font_data = self.font.borrow_dependent();
                let glyph_metrics = font_data
                    .skrifa
                    .glyph_metrics(skrifa::instance::Size::unscaled(), &metrics.location);
                glyph_metrics.advance_width(glyph_id).unwrap_or_default()
            }
        };

        self.advance_width_cache.insert(cache_key, advance.into());
        advance
    }

    /// `\n` will result in `None`.
    ///
    /// Caller must pass [`StyledMetrics`] resolved against *this* face so that
    /// variable-font advance widths are looked up at the correct location.
    pub(super) fn glyph_info(&mut self, c: char, metrics: &StyledMetrics) -> Option<GlyphInfo> {
        let resolution = self.glyph_id_resolution(c)?;
        let glyph_info = match resolution {
            GlyphIdResolution::Invisible => GlyphInfo::INVISIBLE,
            GlyphIdResolution::Glyph(glyph_id) => GlyphInfo {
                id: Some(glyph_id),
                advance_width_unscaled: self.advance_width_unscaled(c, metrics).into(),
            },
        };
        Some(glyph_info)
    }

    #[inline(always)]
    pub fn styled_metrics(
        &self,
        pixels_per_point: f32,
        font_size: f32,
        coords: &VariationCoords,
    ) -> StyledMetrics {
        let pt_scale_factor = self.font.px_scale_factor(font_size * self.tweak.scale);
        let font_data = self.font.borrow_dependent();
        let ascent = (font_data.metrics.ascent * pt_scale_factor).round_ui();
        let descent = (font_data.metrics.descent * pt_scale_factor).round_ui();
        let line_gap = (font_data.metrics.leading * pt_scale_factor).round_ui();

        let scale = font_size * self.tweak.scale * pixels_per_point;
        let px_scale_factor = self.font.px_scale_factor(scale);

        let y_offset_in_points = ((font_size * self.tweak.scale * self.tweak.y_offset_factor)
            + self.tweak.y_offset)
            .round_ui();

        let axes = font_data.skrifa.axes();
        // Override the default coordinates with ones specified via FontTweak, then the ones specified directly via the
        // argument (probably from TextFormat).
        let settings = core::iter::chain(self.tweak.coords.as_ref(), coords.as_ref());
        let location = axes.location(settings);
        let location_hash = LocationHash::new(&location);

        StyledMetrics {
            pixels_per_point,
            px_scale_factor,
            scale,
            y_offset_in_points,
            ascent,
            row_height: ascent - descent + line_gap,
            location,
            location_hash,
        }
    }

    pub(crate) fn tweak(&self) -> &FontTweak {
        &self.tweak
    }

    /// Shape one already-segmented run with face-owned plan and raw-result caches.
    /// The returned carrier releases all face borrows before glyph emission.
    pub(crate) fn shape_for_layout(
        &mut self,
        text: &str,
        coords: &VariationCoords,
        mut buffer: harfrust::UnicodeBuffer,
        flags: harfrust::BufferFlags,
        provenance: ShapeBufferProvenance,
    ) -> LayoutShapeBuffer {
        let Self {
            font,
            tweak,
            shaper_data,
            shape_plans,
            raw_shape_cache,
            ..
        } = self;
        let font_data = font.borrow_dependent();
        let font_ref = &font_data.skrifa;

        // Preserve the face-tweak then text-format variation ordering used by
        // layout. The key must describe the exact instance passed to HarfRust.
        let variations: Vec<harfrust::Variation> =
            core::iter::chain(tweak.coords.as_ref(), coords.as_ref())
                .map(|&(tag, value)| harfrust::Variation { tag, value })
                .collect();

        let instance = if variations.is_empty() {
            None
        } else {
            Some(harfrust::ShaperInstance::from_variations(
                font_ref, variations,
            ))
        };

        // Eligibility is decided before changing the input buffer. Hidden
        // context and replacement policy are trusted only on the proven
        // new-or-cleared layout path.
        let raw_cache_eligible = provenance == ShapeBufferProvenance::LayoutOwnedClean
            && buffer.is_empty()
            && buffer.cluster_level() == harfrust::BufferClusterLevel::default()
            && instance.is_none()
            && text.len() <= RAW_SHAPE_CACHE_MAX_TEXT_BYTES;

        buffer.set_flags(flags);
        buffer.push_str(text);
        buffer.guess_segment_properties();

        let direction = buffer.direction();
        let script = buffer.script();
        let language = buffer.language();
        let raw_cache_key = raw_cache_eligible.then_some(RawShapeCacheKeyRef {
            text,
            script,
            direction,
            language: language.as_ref(),
            flags: flags.bits(),
        });

        if let Some(key) = raw_cache_key
            && let Some((glyph_infos, glyph_positions)) = raw_shape_cache.lookup(key)
        {
            buffer.clear();
            return LayoutShapeBuffer::Cached {
                glyph_infos,
                glyph_positions,
                buffer,
            };
        }

        // A raw hit needs neither a shaper nor a retained ShapePlan. Keep
        // those objects on the native-shaping path only.
        let shaper = shaper_data
            .shaper(font_ref)
            .instance(instance.as_ref())
            .build();

        let key = harfrust::ShapePlanKey::new(Some(script), direction)
            .language(language.as_ref())
            .instance(instance.as_ref())
            .features(&[]);

        let plan_index = if let Some(index) = shape_plan_index(shape_plans, &key) {
            index
        } else {
            let plan =
                harfrust::ShapePlan::new(&shaper, direction, Some(script), language.as_ref(), &[]);
            insert_shape_plan(shape_plans, plan);
            shape_plans.len() - 1
        };

        let plan = &shape_plans[plan_index];
        let glyph_buffer = shaper.shape(buffer, harfrust::ShapeOptions::new().plan(Some(plan)));
        if let Some(key) = raw_cache_key {
            let _ = raw_shape_cache.insert(
                key,
                glyph_buffer.glyph_infos(),
                glyph_buffer.glyph_positions(),
            );
        }
        LayoutShapeBuffer::GlyphBuffer(glyph_buffer)
    }

    pub fn allocate_glyph(
        &mut self,
        atlas: &mut TextureAtlas,
        metrics: &StyledMetrics,
        shaped: &ShapedGlyph,
    ) -> (GlyphAllocation, i32) {
        let ShapedGlyph {
            glyph_id,
            h_pos,
            is_cjk,
        } = *shaped;

        if glyph_id == GlyphId::NOTDEF {
            // invisible
            return (GlyphAllocation::default(), h_pos.round() as i32);
        }

        let (h_pos_round, bin) = if self.subpixel_binning && !is_cjk {
            SubpixelBin::new(h_pos)
        } else {
            // CJK scripts contain a lot of characters and could hog the glyph atlas
            // if we stored 4 subpixel offsets per glyph.
            (h_pos.round() as i32, SubpixelBin::Zero)
        };

        let cache_key = GlyphCacheKey::new(glyph_id, metrics, bin);

        let hinting_target = self.tweak.hinting_target.into();
        let alloc = *self.glyph_alloc_cache.entry(cache_key).or_insert_with(|| {
            self.font
                .allocate_glyph_uncached(
                    atlas,
                    metrics,
                    glyph_id,
                    bin,
                    (&metrics.location).into(),
                    hinting_target,
                )
                .unwrap_or_default()
        });

        (alloc, h_pos_round)
    }
}

/// Positioning info for a single glyph, ready for atlas allocation.
#[derive(Clone, Copy, Debug)]
pub(crate) struct ShapedGlyph {
    pub glyph_id: GlyphId,

    /// Horizontal position of the glyph origin, in physical pixels.
    pub h_pos: f32,

    /// CJK glyphs skip subpixel positioning to save atlas space.
    pub is_cjk: bool,
}

// TODO(emilk): rename?
/// Wrapper over multiple [`FontFace`] (e.g. a primary + fallbacks for emojis)
pub struct Font<'a> {
    pub(super) fonts_by_id: &'a mut nohash_hasher::IntMap<FontFaceKey, FontFace>,
    pub(super) cached_family: &'a mut CachedFamily,
    pub(super) atlas: &'a mut TextureAtlas,
}

impl Font<'_> {
    pub fn preload_characters(&mut self, s: &str) {
        for c in s.chars() {
            self.resolve_face(c);
        }
    }

    /// All supported characters, and in which font they are available in.
    pub fn characters(&mut self) -> &BTreeMap<char, Vec<String>> {
        self.cached_family.characters.get_or_insert_with(|| {
            let mut characters: BTreeMap<char, Vec<String>> = Default::default();
            for font_id in &self.cached_family.fonts {
                let font = self.fonts_by_id.get(font_id).expect("Nonexistent font ID");
                for chr in font.characters() {
                    characters.entry(chr).or_default().push(font.name.clone());
                }
            }
            characters
        })
    }

    pub fn styled_metrics(
        &self,
        pixels_per_point: f32,
        font_size: f32,
        coords: &VariationCoords,
    ) -> StyledMetrics {
        self.cached_family
            .fonts
            .first()
            .and_then(|key| self.fonts_by_id.get(key))
            .map(|font_face| font_face.styled_metrics(pixels_per_point, font_size, coords))
            .unwrap_or_default()
    }

    /// Width of this character in points, at the font's default variation location.
    pub fn glyph_width(&mut self, c: char, font_size: f32) -> f32 {
        let face_key = self.resolve_face(c);
        let Some(font_face) = self.fonts_by_id.get_mut(&face_key) else {
            return 0.0;
        };
        let metrics = font_face.styled_metrics(1.0, font_size, &VariationCoords::default());
        let Some(glyph_info) = font_face.glyph_info(c, &metrics) else {
            return 0.0;
        };
        glyph_info.advance_width_unscaled.0 * font_face.font.px_scale_factor(font_size)
    }

    /// Can we display this glyph?
    pub fn has_glyph(&mut self, c: char) -> bool {
        // TODO(emilk): this is a false negative if the user asks about the replacement character itself 🤦‍♂️
        self.resolve_face(c) != self.cached_family.replacement_face_key
    }

    /// Can we display all the glyphs in this text?
    pub fn has_glyphs(&mut self, s: &str) -> bool {
        s.chars().all(|c| self.has_glyph(c))
    }

    /// Find which face in the fallback chain owns `c`.
    ///
    /// Location-independent — fallback choice depends only on charmap support.
    /// Falls back to the replacement-glyph face when no fallback face has `c`.
    #[inline]
    pub(crate) fn resolve_face(&mut self, c: char) -> FontFaceKey {
        if let Some(font_key) = self.cached_family.face_cache.get(&c) {
            return *font_key;
        }
        self.resolve_face_slow(c)
    }

    #[cold]
    fn resolve_face_slow(&mut self, c: char) -> FontFaceKey {
        let font_key = self
            .cached_family
            .find_face_for_char(c, self.fonts_by_id)
            .unwrap_or(self.cached_family.replacement_face_key);
        self.cached_family.face_cache.insert(c, font_key);
        font_key
    }

    /// Resolve `c` to its (face, [`GlyphInfo`]) at the given face's location.
    ///
    /// `\n` will (intentionally) show up as the replacement character.
    ///
    /// `metrics` must be the resolved [`StyledMetrics`] for the face that ends
    /// up owning `c`. Most callers pass the metrics of their text run's primary
    /// face — that is correct as long as `c` is in that face. For correct
    /// fallback-face advances, resolve the face first with [`Self::resolve_face`]
    /// and build metrics for that face.
    pub(crate) fn glyph_info(
        &mut self,
        c: char,
        metrics: &StyledMetrics,
    ) -> (FontFaceKey, GlyphInfo) {
        let face_key = self.resolve_face(c);
        let Some(face) = self.fonts_by_id.get_mut(&face_key) else {
            return (face_key, GlyphInfo::INVISIBLE);
        };
        let glyph_info = face.glyph_info(c, metrics).unwrap_or_else(|| {
            // `c` is in no face — render the replacement character instead.
            face.glyph_info(self.cached_family.replacement_char, metrics)
                .unwrap_or(GlyphInfo::INVISIBLE)
        });
        (face_key, glyph_info)
    }
}

/// Metrics for a font at a specific screen-space scale.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct StyledMetrics {
    /// The DPI part of the screen-space scale.
    pub pixels_per_point: f32,

    /// Scale factor, relative to the font's units per em (so, probably much less than 1).
    ///
    /// Translates "unscaled" units to physical (screen) pixels.
    pub px_scale_factor: f32,

    /// Absolute scale in screen pixels, for skrifa.
    pub scale: f32,

    /// Vertical offset, in UI points (not screen-space).
    pub y_offset_in_points: f32,

    /// This is the distance from the top to the baseline.
    ///
    /// Unit: points.
    pub ascent: f32,

    /// Height of one row of text in points.
    ///
    /// Returns a value rounded to [`emath::GUI_ROUNDING`].
    pub row_height: f32,

    /// Resolved variation coordinates.
    pub location: skrifa::instance::Location,

    /// Precomputed hash of [`Self::location`].
    ///
    /// Hashed once per run of text so per-glyph cache lookups don't have to
    /// re-hash the full coordinate list.
    pub(crate) location_hash: LocationHash,
}

/// Code points that will always be invisible (zero width).
///
/// See also [`FontFace::ignore_character`].
#[inline]
fn invisible_char(c: char) -> bool {
    if c == '\r' {
        // A character most vile and pernicious. Don't display it.
        return true;
    }

    // See https://github.com/emilk/egui/issues/336

    // From https://www.fileformat.info/info/unicode/category/Cf/list.htm

    // TODO(emilk): heed bidi characters

    matches!(
        c,
        '\u{200B}' // ZERO WIDTH SPACE
            | '\u{200C}' // ZERO WIDTH NON-JOINER
            | '\u{200D}' // ZERO WIDTH JOINER
            | '\u{200E}' // LEFT-TO-RIGHT MARK
            | '\u{200F}' // RIGHT-TO-LEFT MARK
            | '\u{202A}' // LEFT-TO-RIGHT EMBEDDING
            | '\u{202B}' // RIGHT-TO-LEFT EMBEDDING
            | '\u{202C}' // POP DIRECTIONAL FORMATTING
            | '\u{202D}' // LEFT-TO-RIGHT OVERRIDE
            | '\u{202E}' // RIGHT-TO-LEFT OVERRIDE
            | '\u{2060}' // WORD JOINER
            | '\u{2061}' // FUNCTION APPLICATION
            | '\u{2062}' // INVISIBLE TIMES
            | '\u{2063}' // INVISIBLE SEPARATOR
            | '\u{2064}' // INVISIBLE PLUS
            | '\u{2066}' // LEFT-TO-RIGHT ISOLATE
            | '\u{2067}' // RIGHT-TO-LEFT ISOLATE
            | '\u{2068}' // FIRST STRONG ISOLATE
            | '\u{2069}' // POP DIRECTIONAL ISOLATE
            | '\u{206A}' // INHIBIT SYMMETRIC SWAPPING
            | '\u{206B}' // ACTIVATE SYMMETRIC SWAPPING
            | '\u{206C}' // INHIBIT ARABIC FORM SHAPING
            | '\u{206D}' // ACTIVATE ARABIC FORM SHAPING
            | '\u{206E}' // NATIONAL DIGIT SHAPES
            | '\u{206F}' // NOMINAL DIGIT SHAPES
            | '\u{FEFF}' // ZERO WIDTH NO-BREAK SPACE
    )
}

#[inline]
pub(super) fn is_cjk_ideograph(c: char) -> bool {
    ('\u{4E00}' <= c && c <= '\u{9FFF}')
        || ('\u{3400}' <= c && c <= '\u{4DBF}')
        || ('\u{2B740}' <= c && c <= '\u{2B81F}')
}

#[inline]
pub(super) fn is_kana(c: char) -> bool {
    ('\u{3040}' <= c && c <= '\u{309F}') // Hiragana block
        || ('\u{30A0}' <= c && c <= '\u{30FF}') // Katakana block
}

#[inline]
pub(super) fn is_cjk(c: char) -> bool {
    // TODO(bigfarts): Add support for Korean Hangul.
    is_cjk_ideograph(c) || is_kana(c)
}

#[inline]
pub(super) fn is_cjk_break_allowed(c: char) -> bool {
    // See: https://en.wikipedia.org/wiki/Line_breaking_rules_in_East_Asian_languages#Characters_not_permitted_on_the_start_of_a_line.
    !")]｝〕〉》」』】〙〗〟'\"｠»ヽヾーァィゥェォッャュョヮヵヶぁぃぅぇぉっゃゅょゎゕゖㇰㇱㇲㇳㇴㇵㇶㇷㇸㇹㇺㇻㇼㇽㇾㇿ々〻‐゠–〜?!‼⁇⁈⁉・、:;,。.".contains(c)
}

#[cfg(all(test, feature = "default_fonts"))]
mod shape_plan_cache_tests {
    use super::*;
    use crate::text::{FontData, FontDefinitions};
    use harfrust::{Direction, Feature, Language, Script, ShapePlan, ShapePlanKey, ShaperInstance};
    use std::{path::Path, str::FromStr};

    fn face_from_data(name: &str, data: &FontData) -> FontFace {
        let bytes = data.font.clone().into_owned();
        let blob: Blob = std::sync::Arc::new(bytes);
        FontFace::new(
            TextOptions::default(),
            name.to_owned(),
            blob,
            data.index,
            data.tweak.clone(),
        )
        .expect("test font data should load")
    }

    fn default_face(name: &str) -> FontFace {
        let definitions = FontDefinitions::default();
        let data = definitions
            .font_data
            .get(name)
            .expect("requested default test font exists");
        face_from_data(name, data)
    }

    pub(super) fn plan_for(
        face: &FontFace,
        script: Script,
        direction: Direction,
        language: Option<&Language>,
        instance: Option<&ShaperInstance>,
        features: &[Feature],
    ) -> ShapePlan {
        let font_data = face.font.borrow_dependent();
        let font_ref = &font_data.skrifa;
        let shaper = face.shaper_data.shaper(font_ref).instance(instance).build();
        ShapePlan::new(&shaper, direction, Some(script), language, features)
    }

    fn key<'a>(
        script: Script,
        direction: Direction,
        language: Option<&'a Language>,
        instance: Option<&'a ShaperInstance>,
        features: &'a [Feature],
    ) -> ShapePlanKey<'a> {
        ShapePlanKey::new(Some(script), direction)
            .language(language)
            .instance(instance)
            .features(features)
    }

    fn flags() -> harfrust::BufferFlags {
        harfrust::BufferFlags::BEGINNING_OF_TEXT | harfrust::BufferFlags::END_OF_TEXT
    }

    fn empty_buffer(language: Option<Language>) -> harfrust::UnicodeBuffer {
        let mut buffer = harfrust::UnicodeBuffer::new();
        if let Some(language) = language {
            buffer.set_language(language);
        }
        buffer
    }

    fn implicit_shape(
        face: &FontFace,
        text: &str,
        coords: &VariationCoords,
        language: Option<Language>,
    ) -> harfrust::GlyphBuffer {
        let font_data = face.font.borrow_dependent();
        let font_ref = &font_data.skrifa;
        let variations: Vec<harfrust::Variation> =
            core::iter::chain(face.tweak.coords.as_ref(), coords.as_ref())
                .map(|&(tag, value)| harfrust::Variation { tag, value })
                .collect();
        let instance = if variations.is_empty() {
            None
        } else {
            Some(harfrust::ShaperInstance::from_variations(
                font_ref, variations,
            ))
        };
        let shaper = face
            .shaper_data
            .shaper(font_ref)
            .instance(instance.as_ref())
            .build();

        let mut buffer = empty_buffer(language);
        buffer.set_flags(flags());
        buffer.push_str(text);
        buffer.guess_segment_properties();
        shaper.shape(buffer, harfrust::ShapeOptions::new())
    }

    fn cached_plan_index(
        face: &FontFace,
        text: &str,
        coords: &VariationCoords,
        language: Option<Language>,
    ) -> Option<usize> {
        let font_data = face.font.borrow_dependent();
        let font_ref = &font_data.skrifa;
        let variations: Vec<harfrust::Variation> =
            core::iter::chain(face.tweak.coords.as_ref(), coords.as_ref())
                .map(|&(tag, value)| harfrust::Variation { tag, value })
                .collect();
        let instance = if variations.is_empty() {
            None
        } else {
            Some(harfrust::ShaperInstance::from_variations(
                font_ref, variations,
            ))
        };

        let mut buffer = empty_buffer(language);
        buffer.set_flags(flags());
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let language = buffer.language();
        let plan_key = ShapePlanKey::new(Some(buffer.script()), buffer.direction())
            .language(language.as_ref())
            .instance(instance.as_ref())
            .features(&[]);
        shape_plan_index(&face.shape_plans, &plan_key)
    }

    fn assert_glyph_output_equal(
        expected: &harfrust::GlyphBuffer,
        actual: &LayoutShapeBuffer,
        context: &str,
    ) {
        let expected_infos: Vec<_> = expected
            .glyph_infos()
            .iter()
            .map(|glyph| format!("{glyph:?}"))
            .collect();
        let actual_infos: Vec<_> = actual
            .glyph_infos()
            .iter()
            .map(|glyph| format!("{glyph:?}"))
            .collect();
        assert_eq!(
            expected_infos, actual_infos,
            "glyph infos differ: {context}"
        );

        let expected_positions: Vec<_> = expected
            .glyph_positions()
            .iter()
            .map(|position| format!("{position:?}"))
            .collect();
        let actual_positions: Vec<_> = actual
            .glyph_positions()
            .iter()
            .map(|position| format!("{position:?}"))
            .collect();
        assert_eq!(
            expected_positions, actual_positions,
            "glyph positions differ: {context}"
        );
    }

    fn assert_reuse_matches(
        face: &mut FontFace,
        text: &str,
        coords: &VariationCoords,
        language: Option<Language>,
    ) {
        let expected = implicit_shape(face, text, coords, language.clone());
        let actual = face.shape_for_layout(
            text,
            coords,
            empty_buffer(language.clone()),
            flags(),
            ShapeBufferProvenance::Unknown,
        );
        assert_glyph_output_equal(&expected, &actual, text);
        let matching_plan_index = cached_plan_index(face, text, coords, language.clone())
            .expect("actual shaping inputs should match a cached plan");

        let plan_count = face.shape_plans.len();
        let expected_hit = implicit_shape(face, text, coords, language.clone());
        let actual_hit = face.shape_for_layout(
            text,
            coords,
            empty_buffer(language.clone()),
            flags(),
            ShapeBufferProvenance::Unknown,
        );
        assert_glyph_output_equal(&expected_hit, &actual_hit, text);
        assert_eq!(
            face.shape_plans.len(),
            plan_count,
            "matching plan should be reused"
        );
        assert_eq!(
            cached_plan_index(face, text, coords, language),
            Some(matching_plan_index),
            "a cache hit must preserve the matching plan's FIFO position"
        );
    }

    #[test]
    fn shape_plan_key_checks_script_direction_language_and_features() {
        let face = default_face("Hack");
        let language: Language = "en".parse().unwrap();
        let other_language: Language = "fr".parse().unwrap();
        let features = [Feature::from_str("kern=1").unwrap()];
        let disabled_features = [Feature::from_str("kern=0").unwrap()];
        let plan = plan_for(
            &face,
            harfrust::script::LATIN,
            Direction::LeftToRight,
            Some(&language),
            None,
            &features,
        );

        assert!(
            key(
                harfrust::script::LATIN,
                Direction::LeftToRight,
                Some(&language),
                None,
                &features,
            )
            .matches(&plan)
        );
        assert!(
            !key(
                harfrust::script::CYRILLIC,
                Direction::LeftToRight,
                Some(&language),
                None,
                &features,
            )
            .matches(&plan)
        );
        assert!(
            !key(
                harfrust::script::LATIN,
                Direction::RightToLeft,
                Some(&language),
                None,
                &features,
            )
            .matches(&plan)
        );
        assert!(
            !key(
                harfrust::script::LATIN,
                Direction::LeftToRight,
                Some(&other_language),
                None,
                &features,
            )
            .matches(&plan)
        );
        assert!(
            !key(
                harfrust::script::LATIN,
                Direction::LeftToRight,
                Some(&language),
                None,
                &disabled_features,
            )
            .matches(&plan)
        );
    }

    #[test]
    fn plans_are_face_scoped_fifo_bounded_and_recreated_with_the_face() {
        let mut face = default_face("Hack");
        let coords = VariationCoords::default();
        let text = "A repeated Latin run";
        let mut languages = Vec::new();

        for index in 1..=9 {
            let language: Language = format!("en-{index:03}").parse().unwrap();
            assert_reuse_matches(&mut face, text, &coords, Some(language.clone()));
            languages.push(language);
            assert!(face.shape_plans.len() <= SHAPE_PLAN_CACHE_CAPACITY);
        }

        assert_eq!(face.shape_plans.len(), SHAPE_PLAN_CACHE_CAPACITY);
        for (offset, language) in languages.iter().enumerate() {
            assert_eq!(
                cached_plan_index(&face, text, &coords, Some(language.clone())).is_some(),
                offset != 0,
                "FIFO retention for language key {language:?}"
            );
        }

        let retained_indices: Vec<_> = languages[1..]
            .iter()
            .map(|language| {
                cached_plan_index(&face, text, &coords, Some(language.clone()))
                    .expect("retained FIFO plan should match")
            })
            .collect();
        assert_reuse_matches(&mut face, text, &coords, Some(languages[4].clone()));
        assert_eq!(
            languages[1..]
                .iter()
                .map(|language| {
                    cached_plan_index(&face, text, &coords, Some(language.clone()))
                        .expect("cache hit must not evict or reorder plans")
                })
                .collect::<Vec<_>>(),
            retained_indices,
            "cache hits must leave FIFO order unchanged"
        );

        let other_face = default_face("Hack");
        assert!(other_face.shape_plans.is_empty());
        let replacement_face = default_face("Hack");
        assert!(replacement_face.shape_plans.is_empty());
        assert_eq!(face.shape_plans.len(), SHAPE_PLAN_CACHE_CAPACITY);
    }

    #[test]
    fn multilingual_default_font_runs_match_implicit_glyph_output() {
        let definitions = FontDefinitions::default();
        let texts = [
            "Latin text: café 123",
            "Ελληνικά κείμενο",
            "Привет, мир",
            "שלום עולם",
            "مرحبا بالعالم",
            "नमस्ते दुनिया",
            "日本語の文章",
            "👩🏽‍💻 🧑🏻‍🚀",
        ];
        let coords = VariationCoords::default();

        for name in [
            "Hack",
            "Ubuntu-Light",
            "NotoEmoji-Regular",
            "emoji-icon-font",
        ] {
            let data = definitions
                .font_data
                .get(name)
                .expect("default font fixture should exist");
            let mut face = face_from_data(name, data);
            for text in texts {
                assert_reuse_matches(&mut face, text, &coords, None);
            }
            assert!(face.shape_plans.len() <= SHAPE_PLAN_CACHE_CAPACITY);
        }
    }

    #[test]
    fn variable_font_supported_weights_match_implicit_glyph_output() {
        let path = Path::new(r"C:\Windows\Fonts\Bahnschrift.ttf");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!(
                "M2A_VARIABLE_GLYPH_PARITY status=unavailable reason=Bahnschrift_not_found path={}",
                path.display()
            );
            return;
        };

        let data = FontData::from_owned(bytes);
        let axes = data.variation_axes();
        let Some(axis) = axes
            .iter()
            .find(|axis| axis.tag == crate::text::Tag::new(b"wght"))
        else {
            eprintln!(
                "M2A_VARIABLE_GLYPH_PARITY status=unavailable reason=Bahnschrift_wght_axis_not_found path={}",
                path.display()
            );
            return;
        };
        assert!(axis.range.min < axis.range.max);
        let weights = [axis.range.min, axis.range.max];
        let mut face = face_from_data("local-Bahnschrift-test", &data);
        let text = "Variable weight 0123456789";
        let mut plan_indices = Vec::new();

        for weight in weights {
            let coords = VariationCoords::new([(axis.tag, weight)]);
            assert_reuse_matches(&mut face, text, &coords, None);
            plan_indices.push(
                cached_plan_index(&face, text, &coords, None)
                    .expect("actual variation instance should match its cached plan"),
            );
        }

        assert!(
            plan_indices
                .iter()
                .all(|index| *index < face.shape_plans.len()),
            "each supported variation instance must resolve to a compatible retained plan"
        );
        assert!(
            (1..=weights.len()).contains(&face.shape_plans.len()),
            "the exercised variation instances retain at least one plan and no more than one each"
        );
        assert!(face.shape_plans.len() <= SHAPE_PLAN_CACHE_CAPACITY);
        eprintln!(
            "M2A_VARIABLE_GLYPH_PARITY status=passed path={} axis=wght weights={}..{} plan_indices={:?} complete_glyph_info_and_position_debug_parity=passed",
            path.display(),
            weights[0],
            weights[1],
            plan_indices
        );
    }
}

#[cfg(all(test, feature = "default_fonts"))]
mod raw_shape_cache_tests {
    use super::*;
    use crate::text::{FontData, FontDefinitions};
    use std::path::Path;

    fn face_from_data(name: &str, data: &FontData, tweak: FontTweak) -> FontFace {
        let bytes = data.font.clone().into_owned();
        let blob: Blob = std::sync::Arc::new(bytes);
        FontFace::new(
            TextOptions::default(),
            name.to_owned(),
            blob,
            data.index,
            tweak,
        )
        .expect("test font data should load")
    }

    fn default_face(name: &str) -> FontFace {
        let definitions = FontDefinitions::default();
        let data = definitions
            .font_data
            .get(name)
            .expect("requested default test font exists");
        face_from_data(name, data, data.tweak.clone())
    }

    fn key<'a>(
        text: &'a str,
        script: harfrust::Script,
        direction: harfrust::Direction,
        language: Option<&'a harfrust::Language>,
        flags: u32,
    ) -> RawShapeCacheKeyRef<'a> {
        RawShapeCacheKeyRef {
            text,
            script,
            direction,
            language,
            flags,
        }
    }

    fn flags() -> harfrust::BufferFlags {
        harfrust::BufferFlags::BEGINNING_OF_TEXT | harfrust::BufferFlags::END_OF_TEXT
    }

    fn shape(
        face: &mut FontFace,
        text: &str,
        coords: &VariationCoords,
        buffer: harfrust::UnicodeBuffer,
        flags: harfrust::BufferFlags,
        provenance: ShapeBufferProvenance,
    ) -> LayoutShapeBuffer {
        face.shape_for_layout(text, coords, buffer, flags, provenance)
    }

    fn reference_shape(
        face: &FontFace,
        text: &str,
        coords: &VariationCoords,
        mut buffer: harfrust::UnicodeBuffer,
        flags: harfrust::BufferFlags,
    ) -> harfrust::GlyphBuffer {
        let font_data = face.font.borrow_dependent();
        let font_ref = &font_data.skrifa;
        let variations: Vec<harfrust::Variation> =
            core::iter::chain(face.tweak.coords.as_ref(), coords.as_ref())
                .map(|&(tag, value)| harfrust::Variation { tag, value })
                .collect();
        let instance = if variations.is_empty() {
            None
        } else {
            Some(harfrust::ShaperInstance::from_variations(
                font_ref, variations,
            ))
        };
        let shaper = face
            .shaper_data
            .shaper(font_ref)
            .instance(instance.as_ref())
            .build();
        buffer.set_flags(flags);
        buffer.push_str(text);
        buffer.guess_segment_properties();
        shaper.shape(buffer, harfrust::ShapeOptions::new())
    }

    fn projection(
        glyph_infos: &[harfrust::GlyphInfo],
        glyph_positions: &[harfrust::GlyphPosition],
    ) -> Vec<(u32, u32, u32, i32, i32, i32, i32)> {
        glyph_infos
            .iter()
            .zip(glyph_positions)
            .map(|(info, position)| {
                (
                    info.glyph_id,
                    info.cluster,
                    info.flags().to_bits(),
                    position.x_advance,
                    position.y_advance,
                    position.x_offset,
                    position.y_offset,
                )
            })
            .collect()
    }

    fn signature(
        glyph_infos: &[harfrust::GlyphInfo],
        glyph_positions: &[harfrust::GlyphPosition],
    ) -> (Vec<(u32, u32, u32, i32, i32, i32, i32)>, String) {
        (
            projection(glyph_infos, glyph_positions),
            format!("{glyph_infos:?}\n{glyph_positions:?}"),
        )
    }

    fn layout_signature(
        output: &LayoutShapeBuffer,
    ) -> (Vec<(u32, u32, u32, i32, i32, i32, i32)>, String) {
        signature(output.glyph_infos(), output.glyph_positions())
    }

    fn assert_matches_reference(
        output: &LayoutShapeBuffer,
        expected: &harfrust::GlyphBuffer,
        context: &str,
    ) {
        assert_eq!(
            layout_signature(output),
            signature(expected.glyph_infos(), expected.glyph_positions()),
            "complete glyph output differs: {context}"
        );
    }

    fn cache_snapshot(face: &FontFace) -> (Vec<String>, usize) {
        let entries = face
            .raw_shape_cache
            .entries
            .iter()
            .map(|entry| {
                format!(
                    "{}|{:?}|{:?}|{:?}|{}|{}",
                    entry.text,
                    entry.script,
                    entry.direction,
                    entry.language,
                    entry.flags,
                    entry.payload_bytes
                )
            })
            .collect();
        (entries, face.raw_shape_cache.payload_bytes)
    }

    #[test]
    fn raw_cache_key_covers_resolved_components_and_is_face_scoped() {
        let language_en: harfrust::Language = "en".parse().unwrap();
        let language_fr: harfrust::Language = "fr".parse().unwrap();
        let base = key(
            "office",
            harfrust::script::LATIN,
            harfrust::Direction::LeftToRight,
            Some(&language_en),
            harfrust::BufferFlags::BEGINNING_OF_TEXT.bits(),
        );
        let glyph_infos = [harfrust::GlyphInfo::default()];
        let glyph_positions = [harfrust::GlyphPosition::default()];
        let mut face = default_face("Hack");
        face.raw_shape_cache
            .insert(base, &glyph_infos, &glyph_positions)
            .expect("small result should be admitted");
        let stored = &face.raw_shape_cache.entries[0];

        assert!(base.matches(stored));
        assert!(
            !key(
                "offices",
                base.script,
                base.direction,
                base.language,
                base.flags,
            )
            .matches(stored)
        );
        assert!(
            !key(
                base.text,
                harfrust::script::CYRILLIC,
                base.direction,
                base.language,
                base.flags,
            )
            .matches(stored)
        );
        assert!(
            !key(
                base.text,
                base.script,
                harfrust::Direction::RightToLeft,
                base.language,
                base.flags,
            )
            .matches(stored)
        );
        assert!(
            !key(
                base.text,
                base.script,
                base.direction,
                Some(&language_fr),
                base.flags,
            )
            .matches(stored)
        );
        assert!(
            !key(
                base.text,
                base.script,
                base.direction,
                base.language,
                harfrust::BufferFlags::END_OF_TEXT.bits(),
            )
            .matches(stored)
        );

        let mut other_face = default_face("Hack");
        let recreated_face = default_face("Hack");
        assert!(other_face.raw_shape_cache.entries.is_empty());
        assert!(recreated_face.raw_shape_cache.entries.is_empty());
        assert!(other_face.raw_shape_cache.lookup(base).is_none());
        assert_eq!(face.raw_shape_cache.entries.len(), 1);
    }

    #[test]
    fn raw_cache_enforces_lru_payload_and_oversize_limits() {
        let mut lru = RawShapeCache::default();
        for index in 0..RAW_SHAPE_CACHE_MAX_ENTRIES {
            let text = format!("entry-{index:03}");
            lru.insert(
                key(
                    &text,
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &[],
                &[],
            )
            .expect("small key should be admitted");
        }
        assert_eq!(lru.entries.len(), RAW_SHAPE_CACHE_MAX_ENTRIES);

        let least_recent = String::from("entry-000");
        assert!(
            lru.lookup(key(
                &least_recent,
                harfrust::script::LATIN,
                harfrust::Direction::LeftToRight,
                None,
                0,
            ))
            .is_some()
        );
        let newest = String::from("entry-128");
        let evictions = lru
            .insert(
                key(
                    &newest,
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &[],
                &[],
            )
            .unwrap();
        assert_eq!(evictions, 1);
        assert_eq!(lru.entries.len(), RAW_SHAPE_CACHE_MAX_ENTRIES);
        assert!(
            lru.lookup(key(
                &least_recent,
                harfrust::script::LATIN,
                harfrust::Direction::LeftToRight,
                None,
                0,
            ))
            .is_some()
        );
        assert!(
            lru.lookup(key(
                "entry-001",
                harfrust::script::LATIN,
                harfrust::Direction::LeftToRight,
                None,
                0,
            ))
            .is_none()
        );
        assert!(lru.entries.iter().all(|entry| entry.text != "entry-001"));

        let mut byte_bounded = RawShapeCache::default();
        let infos = vec![harfrust::GlyphInfo::default(); 1024];
        let positions = vec![harfrust::GlyphPosition::default(); 1024];
        let mut saw_payload_eviction = false;
        for index in 0..64 {
            let text = format!("payload-{index:02}");
            saw_payload_eviction |= byte_bounded
                .insert(
                    key(
                        &text,
                        harfrust::script::LATIN,
                        harfrust::Direction::LeftToRight,
                        None,
                        0,
                    ),
                    &infos,
                    &positions,
                )
                .unwrap()
                > 0;
        }
        assert!(saw_payload_eviction);
        assert!(byte_bounded.payload_bytes <= RAW_SHAPE_CACHE_MAX_PAYLOAD_BYTES);
        assert!(byte_bounded.entries.len() <= RAW_SHAPE_CACHE_MAX_ENTRIES);

        let mut protected = RawShapeCache::default();
        protected
            .insert(
                key(
                    "retained",
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &[],
                &[],
            )
            .unwrap();
        let before = protected
            .entries
            .iter()
            .map(|entry| entry.text.clone())
            .collect::<Vec<_>>();
        let before_payload = protected.payload_bytes;

        let overlong = "x".repeat(RAW_SHAPE_CACHE_MAX_TEXT_BYTES + 1);
        assert_eq!(
            protected.insert(
                key(
                    &overlong,
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &[],
                &[],
            ),
            Err(RawShapeCacheBypass::TextLimit)
        );

        let oversized_infos = vec![harfrust::GlyphInfo::default(); 4096];
        let oversized_positions = vec![harfrust::GlyphPosition::default(); 4096];
        assert_eq!(
            protected.insert(
                key(
                    "oversized-result",
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &oversized_infos,
                &oversized_positions,
            ),
            Err(RawShapeCacheBypass::EntryPayloadLimit)
        );
        assert_eq!(
            protected.insert(
                key(
                    "mismatched-arrays",
                    harfrust::script::LATIN,
                    harfrust::Direction::LeftToRight,
                    None,
                    0,
                ),
                &glyph_infos_for_cache_test(),
                &[],
            ),
            Err(RawShapeCacheBypass::UnsupportedBuffer)
        );
        assert_eq!(
            raw_shape_payload_bytes(usize::MAX, 1, 0, 0),
            None,
            "key payload accounting rejects arithmetic overflow"
        );
        assert_eq!(
            protected
                .entries
                .iter()
                .map(|entry| entry.text.clone())
                .collect::<Vec<_>>(),
            before
        );
        assert_eq!(protected.payload_bytes, before_payload);
        assert!(
            protected
                .entries
                .iter()
                .any(|entry| entry.text == "retained")
        );
    }

    fn glyph_infos_for_cache_test() -> [harfrust::GlyphInfo; 1] {
        [harfrust::GlyphInfo::default()]
    }

    #[test]
    fn raw_cache_hit_miss_recycling_and_unknown_buffer_bypass_match_reference() {
        let texts = [
            "office affine AV",
            "x\u{0301}",
            "مرحبا بالعالم",
            "🇺🇸 family 👨‍👩‍👧‍👦",
        ];
        let coords = VariationCoords::default();
        let mut face = default_face("Ubuntu-Light");
        let reference_face = default_face("Ubuntu-Light");
        let mut recycled = harfrust::UnicodeBuffer::new();
        let mut default_signatures = Vec::new();

        for text in texts {
            let expected = reference_shape(
                &reference_face,
                text,
                &coords,
                harfrust::UnicodeBuffer::new(),
                flags(),
            );
            let miss = shape(
                &mut face,
                text,
                &coords,
                recycled,
                flags(),
                ShapeBufferProvenance::LayoutOwnedClean,
            );
            assert!(matches!(&miss, LayoutShapeBuffer::GlyphBuffer(_)));
            assert_matches_reference(&miss, &expected, text);
            let expected_signature = signature(expected.glyph_infos(), expected.glyph_positions());
            let miss_signature = layout_signature(&miss);
            assert_eq!(miss_signature, expected_signature);
            default_signatures.push(miss_signature);
            recycled = miss.clear();
            assert!(recycled.is_empty());

            let entries_before_hit = face.raw_shape_cache.entries.len();
            let hit = shape(
                &mut face,
                text,
                &coords,
                recycled,
                flags(),
                ShapeBufferProvenance::LayoutOwnedClean,
            );
            assert!(matches!(&hit, LayoutShapeBuffer::Cached { .. }));
            assert_eq!(layout_signature(&hit), expected_signature);
            assert_eq!(face.raw_shape_cache.entries.len(), entries_before_hit);
            recycled = hit.clear();
            assert!(recycled.is_empty());
        }
        assert_eq!(face.raw_shape_cache.entries.len(), texts.len());

        let before_unknown = cache_snapshot(&face);
        let unknown_text = "A\u{FE0F}";
        let fallback_glyph = match face.glyph_id_resolution('B').expect("Ubuntu-Light maps B") {
            GlyphIdResolution::Glyph(glyph_id) => glyph_id.to_u32(),
            GlyphIdResolution::Invisible => panic!("B is visible in Ubuntu-Light"),
        };
        let mut unknown_buffer = harfrust::UnicodeBuffer::new();
        unknown_buffer.set_pre_context("pre");
        unknown_buffer.set_post_context("post");
        unknown_buffer.set_not_found_variation_selector_glyph(fallback_glyph);
        let unknown = shape(
            &mut face,
            unknown_text,
            &coords,
            unknown_buffer,
            flags(),
            ShapeBufferProvenance::Unknown,
        );
        assert!(matches!(&unknown, LayoutShapeBuffer::GlyphBuffer(_)));
        let unknown_signature = layout_signature(&unknown);
        let mut unknown_reference_buffer = harfrust::UnicodeBuffer::new();
        unknown_reference_buffer.set_pre_context("pre");
        unknown_reference_buffer.set_post_context("post");
        unknown_reference_buffer.set_not_found_variation_selector_glyph(fallback_glyph);
        let unknown_reference = reference_shape(
            &reference_face,
            unknown_text,
            &coords,
            unknown_reference_buffer,
            flags(),
        );
        assert_eq!(
            unknown_signature,
            signature(
                unknown_reference.glyph_infos(),
                unknown_reference.glyph_positions()
            )
        );
        let default_unknown_reference = reference_shape(
            &reference_face,
            unknown_text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
        );
        assert_ne!(
            unknown_signature,
            signature(
                default_unknown_reference.glyph_infos(),
                default_unknown_reference.glyph_positions()
            ),
            "the hidden replacement policy must observably affect the reference output"
        );
        assert_eq!(cache_snapshot(&face), before_unknown);
        drop(unknown.clear());

        // A pre-populated input appends text to its existing buffer. Compare the
        // exact same buffer-building sequence; concatenating strings separately
        // would not preserve its cluster and context semantics.
        let make_prepopulated_buffer = || {
            let mut buffer = harfrust::UnicodeBuffer::new();
            buffer.set_pre_context("before");
            buffer.push_str("prefix ");
            buffer.set_post_context("after");
            buffer
        };
        let before_prepopulated = cache_snapshot(&face);
        let appended_text = "office";
        let prepopulated = shape(
            &mut face,
            appended_text,
            &coords,
            make_prepopulated_buffer(),
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(matches!(&prepopulated, LayoutShapeBuffer::GlyphBuffer(_)));
        let prepopulated_reference = reference_shape(
            &reference_face,
            appended_text,
            &coords,
            make_prepopulated_buffer(),
            flags(),
        );
        assert_matches_reference(
            &prepopulated,
            &prepopulated_reference,
            "pre-populated append",
        );
        assert_eq!(cache_snapshot(&face), before_prepopulated);
        drop(prepopulated.clear());

        let custom_text = texts[1];
        let mut custom_buffer = harfrust::UnicodeBuffer::new();
        custom_buffer.set_cluster_level(harfrust::BufferClusterLevel::Characters);
        let before_custom = cache_snapshot(&face);
        let custom = shape(
            &mut face,
            custom_text,
            &coords,
            custom_buffer,
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(matches!(&custom, LayoutShapeBuffer::GlyphBuffer(_)));
        let mut custom_reference_buffer = harfrust::UnicodeBuffer::new();
        custom_reference_buffer.set_cluster_level(harfrust::BufferClusterLevel::Characters);
        let custom_reference = reference_shape(
            &reference_face,
            custom_text,
            &coords,
            custom_reference_buffer,
            flags(),
        );
        assert_matches_reference(&custom, &custom_reference, "custom character clusters");
        assert_ne!(
            layout_signature(&custom),
            default_signatures[1],
            "character clusters observably differ from the default grapheme policy"
        );
        assert_eq!(cache_snapshot(&face), before_custom);
        drop(custom.clear());

        // The first miss leaves ample native glyph-buffer capacity. A cache hit
        // returns the same UnicodeBuffer, and a later miss must reuse its storage.
        let mut reuse_face = default_face("Ubuntu-Light");
        let reuse_reference = default_face("Ubuntu-Light");
        let long_text = "office ".repeat(256);
        let short_text = "office";
        let first_miss = shape(
            &mut reuse_face,
            &long_text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        let (native_allocation, first_glyph_count) = match &first_miss {
            LayoutShapeBuffer::GlyphBuffer(buffer) => {
                (buffer.glyph_infos().as_ptr(), buffer.glyph_infos().len())
            }
            LayoutShapeBuffer::Cached { .. } => panic!("the first long input is a cache miss"),
        };
        assert!(first_glyph_count > 1_000);
        let expected_long = reference_shape(
            &reuse_reference,
            &long_text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
        );
        assert_matches_reference(&first_miss, &expected_long, "long cache miss");
        let recycled = first_miss.clear();
        let hit = shape(
            &mut reuse_face,
            &long_text,
            &coords,
            recycled,
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(matches!(&hit, LayoutShapeBuffer::Cached { .. }));
        assert_eq!(
            layout_signature(&hit),
            signature(expected_long.glyph_infos(), expected_long.glyph_positions())
        );
        let recycled = hit.clear();
        let forced_miss = shape(
            &mut reuse_face,
            short_text,
            &coords,
            recycled,
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        let (reused_allocation, forced_glyph_count) = match &forced_miss {
            LayoutShapeBuffer::GlyphBuffer(buffer) => {
                (buffer.glyph_infos().as_ptr(), buffer.glyph_infos().len())
            }
            LayoutShapeBuffer::Cached { .. } => panic!("different short text is a cache miss"),
        };
        assert!(forced_glyph_count < first_glyph_count);
        assert_eq!(
            reused_allocation, native_allocation,
            "a cache hit followed by a forced miss reuses native glyph allocation"
        );
        let expected_short = reference_shape(
            &reuse_reference,
            short_text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
        );
        assert_matches_reference(&forced_miss, &expected_short, "forced short miss");
        drop(forced_miss.clear());
    }

    #[test]
    fn raw_cache_bypasses_variable_instances_and_preserves_extrema_outputs() {
        let path = Path::new(r"C:\Windows\Fonts\Bahnschrift.ttf");
        let Ok(bytes) = std::fs::read(path) else {
            eprintln!(
                "M2A_RAW_SHAPE_VARIATION_FIXTURE status=unavailable reason=local_Bahnschrift_not_found path={}",
                path.display()
            );
            return;
        };
        let data = FontData::from_owned(bytes);
        let axes = data.variation_axes();
        let axis = axes
            .iter()
            .find(|axis| axis.tag == crate::text::Tag::new(b"wght"))
            .expect("Bahnschrift provides the wght variation axis");
        let weights = [300.0, 700.0];
        assert!(axis.range.min <= weights[0] && weights[1] <= axis.range.max);

        let text = "Variable font: office AV — café";
        let mut format_face = face_from_data("Bahnschrift-format", &data, data.tweak.clone());
        let format_reference =
            face_from_data("Bahnschrift-format-reference", &data, data.tweak.clone());
        for weight in weights {
            let coords = VariationCoords::new([(axis.tag, weight)]);
            let output = shape(
                &mut format_face,
                text,
                &coords,
                harfrust::UnicodeBuffer::new(),
                flags(),
                ShapeBufferProvenance::LayoutOwnedClean,
            );
            assert!(matches!(&output, LayoutShapeBuffer::GlyphBuffer(_)));
            let reference = reference_shape(
                &format_reference,
                text,
                &coords,
                harfrust::UnicodeBuffer::new(),
                flags(),
            );
            assert_matches_reference(&output, &reference, &format!("Bahnschrift wght={weight}"));
            assert!(format_face.raw_shape_cache.entries.is_empty());
            assert_eq!(format_face.raw_shape_cache.payload_bytes, 0);
            drop(output.clear());
        }

        let mut tweak = data.tweak.clone();
        tweak.coords = VariationCoords::new([(axis.tag, weights[0])]);
        let mut tweak_face = face_from_data("Bahnschrift-face-tweak", &data, tweak.clone());
        let tweak_reference = face_from_data("Bahnschrift-tweak-reference", &data, tweak);
        let tweaked = shape(
            &mut tweak_face,
            text,
            &VariationCoords::default(),
            harfrust::UnicodeBuffer::new(),
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(matches!(&tweaked, LayoutShapeBuffer::GlyphBuffer(_)));
        let reference = reference_shape(
            &tweak_reference,
            text,
            &VariationCoords::default(),
            harfrust::UnicodeBuffer::new(),
            flags(),
        );
        assert_matches_reference(&tweaked, &reference, "Bahnschrift face-tweak wght=300");
        assert!(tweak_face.raw_shape_cache.entries.is_empty());
        assert_eq!(tweak_face.raw_shape_cache.payload_bytes, 0);
        eprintln!(
            "M2A_RAW_SHAPE_VARIATION_FIXTURE status=passed path={} axis=wght weights={}..{} format_and_face_tweak_bypass=passed complete_glyph_output_parity=passed",
            path.display(),
            weights[0],
            weights[1]
        );
        drop(tweaked.clear());
    }

    #[test]
    fn raw_hit_returns_before_repopulating_an_evicted_shape_plan() {
        use super::shape_plan_cache_tests::plan_for;

        let text = "office AV";
        let coords = VariationCoords::default();
        let mut face = default_face("Ubuntu-Light");
        let reference_face = default_face("Ubuntu-Light");

        let mut input = harfrust::UnicodeBuffer::new();
        input.set_flags(flags());
        input.push_str(text);
        input.guess_segment_properties();
        let original_language = input.language();
        let original_key = harfrust::ShapePlanKey::new(Some(input.script()), input.direction())
            .language(original_language.as_ref())
            .instance(None)
            .features(&[]);

        let miss = shape(
            &mut face,
            text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(matches!(&miss, LayoutShapeBuffer::GlyphBuffer(_)));
        let original_signature = layout_signature(&miss);
        let expected = reference_shape(
            &reference_face,
            text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
        );
        assert_eq!(
            original_signature,
            signature(expected.glyph_infos(), expected.glyph_positions()),
            "the initial miss matches the complete independent reference output"
        );
        assert_eq!(face.shape_plans.len(), 1);
        assert_eq!(shape_plan_index(&face.shape_plans, &original_key), Some(0));
        let retained_raw_entry = cache_snapshot(&face);
        assert_eq!(retained_raw_entry.0.len(), 1);
        drop(miss.clear());

        let filler_languages: Vec<harfrust::Language> =
            ["fr", "de", "es", "it", "pt", "nl", "sv", "da"]
                .into_iter()
                .map(|language| language.parse().expect("valid distinct language tag"))
                .collect();
        for (index, language) in filler_languages.iter().enumerate() {
            let plan = plan_for(
                &face,
                harfrust::script::LATIN,
                harfrust::Direction::LeftToRight,
                Some(language),
                None,
                &[],
            );
            assert!(
                !original_key.matches(&plan),
                "each inserted public plan key is distinct from the original"
            );
            let inserted_key = harfrust::ShapePlanKey::new(
                Some(harfrust::script::LATIN),
                harfrust::Direction::LeftToRight,
            )
            .language(Some(language))
            .instance(None)
            .features(&[]);
            assert!(
                shape_plan_index(&face.shape_plans, &inserted_key).is_none(),
                "each filler key is genuinely absent before FIFO insertion"
            );
            let evicted = insert_shape_plan(&mut face.shape_plans, plan);
            assert_eq!(
                evicted,
                index == 7,
                "the eighth distinct insertion evicts the original FIFO plan"
            );
        }

        assert_eq!(face.shape_plans.len(), SHAPE_PLAN_CACHE_CAPACITY);
        assert!(
            shape_plan_index(&face.shape_plans, &original_key).is_none(),
            "the original public plan key has been evicted"
        );
        assert_eq!(
            cache_snapshot(&face),
            retained_raw_entry,
            "plan eviction leaves the retained raw result unchanged"
        );
        for (expected_index, language) in filler_languages.iter().enumerate() {
            let inserted_key = harfrust::ShapePlanKey::new(
                Some(harfrust::script::LATIN),
                harfrust::Direction::LeftToRight,
            )
            .language(Some(language))
            .instance(None)
            .features(&[]);
            assert_eq!(
                shape_plan_index(&face.shape_plans, &inserted_key),
                Some(expected_index),
                "the inserted public plans retain FIFO order and content"
            );
        }

        let hit = shape(
            &mut face,
            text,
            &coords,
            harfrust::UnicodeBuffer::new(),
            flags(),
            ShapeBufferProvenance::LayoutOwnedClean,
        );
        assert!(
            matches!(&hit, LayoutShapeBuffer::Cached { .. }),
            "the raw result hits even though its original shape plan was evicted"
        );
        assert_eq!(layout_signature(&hit), original_signature);
        assert_eq!(face.shape_plans.len(), SHAPE_PLAN_CACHE_CAPACITY);
        assert!(shape_plan_index(&face.shape_plans, &original_key).is_none());
        for (expected_index, language) in filler_languages.iter().enumerate() {
            let inserted_key = harfrust::ShapePlanKey::new(
                Some(harfrust::script::LATIN),
                harfrust::Direction::LeftToRight,
            )
            .language(Some(language))
            .instance(None)
            .features(&[]);
            assert_eq!(
                shape_plan_index(&face.shape_plans, &inserted_key),
                Some(expected_index),
                "the cache hit does not repopulate or reorder the FIFO plan cache"
            );
        }
        assert_eq!(
            cache_snapshot(&face),
            retained_raw_entry,
            "the cache hit preserves the same retained raw entry"
        );
        let recycled = hit.clear();
        assert!(
            recycled.is_empty(),
            "the cached carrier releases its buffer"
        );
    }
}
