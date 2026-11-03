//! Cached, presentation-only CommonMark and Mermaid previews for Markdown Studio.

use crate::diagram::{DiagramImageSnapshot, DiagramSession, PreviewState, SceneMetrics};
use crate::notes_markdown::{
    LocalRasterFormat, LocalRasterResolver, MAX_LOCAL_RASTER_BYTES, MAX_MARKDOWN_PREVIEW_LINES,
    MarkdownAnalysis, MarkdownPreview, MarkdownPreviewLimit, MarkdownPreviewStatus,
    MermaidFencePreview, MermaidFenceStatus,
};
use eframe::egui;
use eframe::egui::load::ImageLoader;
use egui_commonmark::{CommonMarkCache, CommonMarkViewer};
use pulldown_cmark::{Event, Options, Parser, Tag, TagEnd};
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicU64, Ordering},
};

const MAX_CACHED_MERMAID_PIXELS: u64 = 8 * 1024 * 1024;
const MAX_PREVIEW_LOCAL_IMAGES: usize = 128;
const MAX_PREVIEW_LOCAL_DIMENSION: u32 = 8_192;
const MAX_PREVIEW_LOCAL_PIXELS: u64 = 16_777_216;
const MAX_PREVIEW_LOCAL_AGGREGATE_PIXELS: u64 = 16_777_216;
const MAX_IMAGE_DIAGNOSTICS: usize = 16;
const MAX_PREVIEW_PARSER_EVENTS: usize = 1_000_000;
static NEXT_IMAGE_LOADER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone)]
pub(super) struct MermaidCard {
    raw_source: String,
    status: MermaidFenceStatus,
    texture: Option<egui::TextureHandle>,
}

impl MermaidCard {
    pub(super) fn raw_source(&self) -> &str {
        &self.raw_source
    }

    pub(super) fn status(&self) -> &MermaidFenceStatus {
        &self.status
    }
}

/// A card-only bounded analysis for the existing Notes preview. It shares the
/// exact fence parser and accepted Diagram rasterizer without replacing the
/// Notes-specific CommonMark rendering path.
pub(super) struct MermaidCardsCache {
    revision: u64,
    source: Option<String>,
    status: Result<MarkdownPreviewStatus, String>,
    fences: Vec<MermaidFencePreview>,
    cards: Vec<MermaidCard>,
}

impl MermaidCardsCache {
    pub(super) fn new(ctx: &egui::Context, revision: u64, source: &str) -> Self {
        match MarkdownPreview::analyze(source) {
            Ok(preview) => {
                let fences = preview.mermaid_fences().to_vec();
                let cards = build_mermaid_cards(&fences, |fence, remaining_pixels| {
                    render_mermaid_fence(ctx, fence, remaining_pixels)
                });
                Self {
                    revision,
                    source: Some(source.to_owned()),
                    status: Ok(preview.status().clone()),
                    fences,
                    cards,
                }
            }
            Err(error) => Self {
                revision,
                // Oversized Notes remain fully owned by the legacy Note
                // renderer; do not duplicate an unbounded document merely to
                // cache a bounded Mermaid-preview failure.
                source: None,
                status: Err(error.to_string()),
                fences: Vec::new(),
                cards: Vec::new(),
            },
        }
    }

    pub(super) fn matches(&self, revision: u64, source: &str) -> bool {
        self.revision == revision && self.source.as_deref().is_none_or(|cached| cached == source)
    }

    pub(super) fn status(&self) -> Result<&MarkdownPreviewStatus, &str> {
        self.status.as_ref().map_err(String::as_str)
    }

    pub(super) fn fences(&self) -> &[MermaidFencePreview] {
        &self.fences
    }

    pub(super) fn fence(&self, index: usize) -> Option<&MermaidFencePreview> {
        self.fences.get(index)
    }

    pub(super) fn card(&self, index: usize) -> Option<&MermaidCard> {
        self.cards.get(index)
    }
}

/// A bounded in-memory image loader used only for already-validated local
/// images. Markdown destinations are rewritten to opaque per-preview URIs, so
/// CommonMarkViewer cannot pass remote or arbitrary file destinations to the
/// application's image loaders.
pub(super) struct MarkdownAssetImageLoader {
    id: String,
    nonce: u64,
    images: Mutex<HashMap<String, Arc<egui::ColorImage>>>,
}

impl MarkdownAssetImageLoader {
    pub(super) fn new() -> Arc<Self> {
        let nonce = NEXT_IMAGE_LOADER_ID.fetch_add(1, Ordering::Relaxed);
        Arc::new(Self {
            id: format!("{}::local-images::{nonce}", module_path!()),
            nonce,
            images: Mutex::new(HashMap::new()),
        })
    }

    fn replace_images(&self, ctx: &egui::Context, images: HashMap<String, Arc<egui::ColorImage>>) {
        let old_uris = self
            .images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for uri in old_uris {
            ctx.forget_image(&uri);
        }
        *self
            .images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = images;
    }

    fn uri(&self, generation: u64, index: usize) -> String {
        format!(
            "markdown-local://assets/{}/{generation}/{index}",
            self.nonce
        )
    }
}

impl egui::load::ImageLoader for MarkdownAssetImageLoader {
    fn id(&self) -> &str {
        &self.id
    }

    fn load(
        &self,
        _ctx: &egui::Context,
        uri: &str,
        _size_hint: egui::SizeHint,
    ) -> egui::load::ImageLoadResult {
        let images = self
            .images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if !uri.starts_with(&format!("markdown-local://assets/{}/", self.nonce)) {
            return Err(egui::load::LoadError::NotSupported);
        }
        images
            .get(uri)
            .cloned()
            .map(|image| egui::load::ImagePoll::Ready { image })
            .ok_or_else(|| {
                egui::load::LoadError::Loading(
                    "validated Markdown image is no longer cached".into(),
                )
            })
    }

    fn forget(&self, uri: &str) {
        self.images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .remove(uri);
    }

    fn forget_all(&self) {
        self.images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .clear();
    }

    fn byte_size(&self) -> usize {
        self.images
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .values()
            .map(|image| {
                image
                    .pixels
                    .len()
                    .saturating_mul(std::mem::size_of::<egui::Color32>())
            })
            .fold(0usize, usize::saturating_add)
    }
}

/// Expensive parsing, local-asset decoding, and Diagram rasterization happen
/// once for a document revision. The full CommonMark source is rendered by the
/// shared viewer; only image destinations are replaced in its display copy.
pub(super) struct MarkdownPreviewCache {
    revision: u64,
    source: String,
    base_directory: Option<std::path::PathBuf>,
    display_source: Option<String>,
    analysis: Option<MarkdownAnalysis>,
    status: MarkdownPreviewStatus,
    fences: Vec<MermaidFencePreview>,
    cards: Vec<MermaidCard>,
    image_diagnostics: Vec<String>,
}

impl MarkdownPreviewCache {
    pub(super) fn matches(
        &self,
        revision: u64,
        source: &str,
        base_directory: Option<&Path>,
    ) -> bool {
        self.revision == revision
            && self.source == source
            && self.base_directory.as_deref() == base_directory
    }

    pub(super) fn new(
        ctx: &egui::Context,
        revision: u64,
        generation: u64,
        source: &str,
        base_directory: Option<&Path>,
        local_assets: Option<&dyn LocalRasterResolver>,
        image_loader: &MarkdownAssetImageLoader,
    ) -> Result<Self, String> {
        let preview = MarkdownPreview::analyze(source).map_err(|error| error.to_string())?;
        let analysis = preview.analysis().cloned();
        let status = preview.status().clone();
        let fences = preview.mermaid_fences().to_vec();
        let mut image_diagnostics = Vec::new();
        let mut image_cache = HashMap::new();

        // The shared viewer also parses the source. Do not invoke it for a
        // source that the authoritative bounded analysis declined to parse.
        let display_source = if allows_commonmark_display(&status) {
            Some(prepare_safe_display_source(
                source,
                local_assets,
                image_loader,
                generation,
                &mut image_cache,
                &mut image_diagnostics,
            )?)
        } else {
            None
        };

        image_loader.replace_images(ctx, image_cache);

        let cards = build_mermaid_cards(&fences, |fence, remaining_pixels| {
            render_mermaid_fence(ctx, fence, remaining_pixels)
        });

        Ok(Self {
            revision,
            source: source.to_owned(),
            base_directory: base_directory.map(Path::to_path_buf),
            display_source,
            analysis,
            status,
            fences,
            cards,
            image_diagnostics,
        })
    }

    pub(super) fn analysis(&self) -> Option<&MarkdownAnalysis> {
        self.analysis.as_ref()
    }

    pub(super) fn fences(&self) -> &[MermaidFencePreview] {
        &self.fences
    }

    pub(super) fn selected_fence_source(&self, index: usize) -> Option<&str> {
        self.fences.get(index).map(MermaidFencePreview::raw_source)
    }

    pub(super) fn draw(
        &self,
        ui: &mut egui::Ui,
        viewer_cache: &mut CommonMarkCache,
        image_loader: &Arc<MarkdownAssetImageLoader>,
    ) {
        if let MarkdownPreviewStatus::LimitExceeded(limit) = &self.status {
            ui.colored_label(
                ui.visuals().warn_fg_color,
                format!("Preview is limited ({limit:?}); the original source remains editable."),
            );
        }
        if !allows_commonmark_display(&self.status) {
            return;
        }

        if let Some(display_source) = &self.display_source {
            let ctx = ui.ctx().clone();
            if !ctx.is_loader_installed(image_loader.id()) {
                ctx.add_image_loader(image_loader.clone());
            }
            let first_new_command = ctx.output(|output| output.commands.len());
            CommonMarkViewer::new()
                .default_implicit_uri_scheme("markdown-blocked://")
                .show(ui, viewer_cache, display_source);
            let blocked_links = filter_unsafe_open_urls(&ctx, first_new_command);
            if blocked_links > 0 {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Blocked {blocked_links} unsafe Markdown link destination(s)."),
                );
            }
        }

        for diagnostic in &self.image_diagnostics {
            ui.colored_label(ui.visuals().warn_fg_color, diagnostic);
        }

        if !self.cards.is_empty() {
            ui.separator();
            ui.heading("Mermaid previews");
            for card in &self.cards {
                draw_mermaid_card(ui, card);
            }
        } else if self.fences.is_empty() && self.source.trim().is_empty() {
            ui.weak("Preview is empty.");
        }
    }
}

fn build_mermaid_cards(
    fences: &[MermaidFencePreview],
    mut render: impl FnMut(&MermaidFencePreview, u64) -> Result<(egui::TextureHandle, u64), String>,
) -> Vec<MermaidCard> {
    let mut cached_pixels = 0u64;
    fences
        .iter()
        .map(|fence| {
            let mut card = MermaidCard {
                raw_source: fence.raw_source().to_owned(),
                status: fence.status().clone(),
                texture: None,
            };
            if matches!(fence.status(), MermaidFenceStatus::Current) {
                let remaining_pixels = MAX_CACHED_MERMAID_PIXELS.saturating_sub(cached_pixels);
                if remaining_pixels == 0 {
                    card.status = MermaidFenceStatus::PreviewEncodingFailed(
                        "combined Mermaid preview images exceed the 8-megapixel cache limit".into(),
                    );
                    return card;
                }
                match render(fence, remaining_pixels) {
                    Ok((_texture, 0)) => {
                        card.status = MermaidFenceStatus::PreviewEncodingFailed(
                            "Diagram preview has empty pixel dimensions".into(),
                        );
                    }
                    Ok((texture, pixels)) if pixels <= remaining_pixels => {
                        cached_pixels = cached_pixels.saturating_add(pixels);
                        card.texture = Some(texture);
                    }
                    Ok((_texture, _pixels)) => {
                        card.status = MermaidFenceStatus::PreviewEncodingFailed(
                            "combined Mermaid preview images exceed the 8-megapixel cache limit"
                                .into(),
                        );
                    }
                    Err(error) => {
                        card.status = MermaidFenceStatus::PreviewEncodingFailed(error);
                    }
                }
            }
            card
        })
        .collect()
}

fn allows_commonmark_display(status: &MarkdownPreviewStatus) -> bool {
    !matches!(
        status,
        MarkdownPreviewStatus::LimitExceeded(
            MarkdownPreviewLimit::SourceBytes
                | MarkdownPreviewLimit::SourceLines
                | MarkdownPreviewLimit::ParserEvents
        )
    )
}

fn filter_unsafe_open_urls(ctx: &egui::Context, first_new_command: usize) -> usize {
    ctx.output_mut(|output| {
        let first = first_new_command.min(output.commands.len());
        let mut new_commands = output.commands.split_off(first);
        let original_len = new_commands.len();
        new_commands.retain(|command| {
            !matches!(
                command,
                egui::OutputCommand::OpenUrl(open_url)
                    if crate::notes_markdown::html::safe_link_destination(&open_url.url).is_none()
            )
        });
        let blocked = original_len.saturating_sub(new_commands.len());
        output.commands.extend(new_commands);
        blocked
    })
}

fn render_mermaid_fence(
    ctx: &egui::Context,
    fence: &MermaidFencePreview,
    remaining_pixels: u64,
) -> Result<(egui::TextureHandle, u64), String> {
    let session =
        DiagramSession::from_fresh_source(fence.raw_source()).map_err(|error| error.to_string())?;
    if session.status().preview != PreviewState::Current {
        return Err("Diagram preview could not be prepared".into());
    }
    let snapshot = DiagramImageSnapshot::capture(&session, SceneMetrics::default())
        .map_err(|error| error.to_string())?;
    let artifact = snapshot
        .encode_png(1.0)
        .map_err(|error| error.to_string())?;
    let (width, height) = artifact
        .dimensions()
        .ok_or_else(|| "Diagram preview has no pixel dimensions".to_owned())?;
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0 || height == 0 || pixels > remaining_pixels {
        return Err("Diagram preview exceeds the bounded image dimensions".into());
    }
    let decoded = image::load_from_memory(artifact.bytes())
        .map_err(|error| format!("could not decode Diagram preview: {error}"))?
        .to_rgba8();
    let color = egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        decoded.as_raw(),
    );
    let texture = ctx.load_texture(
        format!("markdown-mermaid-{}", fence.full_range().start),
        color,
        egui::TextureOptions::LINEAR,
    );
    Ok((texture, pixels))
}

pub(super) fn draw_mermaid_card(ui: &mut egui::Ui, card: &MermaidCard) {
    ui.group(|ui| {
        draw_mermaid_card_contents(ui, card);
    });
}

pub(super) fn draw_mermaid_card_contents(ui: &mut egui::Ui, card: &MermaidCard) {
    ui.horizontal(|ui| {
        ui.strong("Mermaid diagram");
        match &card.status {
            MermaidFenceStatus::Current if card.texture.is_some() => {
                ui.colored_label(ui.visuals().warn_fg_color, "Current");
            }
            MermaidFenceStatus::Invalid(diagnostic)
            | MermaidFenceStatus::Unsupported(diagnostic) => {
                ui.colored_label(ui.visuals().error_fg_color, diagnostic.to_string());
            }
            MermaidFenceStatus::Unclosed => {
                ui.colored_label(ui.visuals().warn_fg_color, "Fence is not closed");
            }
            MermaidFenceStatus::PreviewEncodingFailed(error) => {
                ui.colored_label(ui.visuals().warn_fg_color, error);
            }
            other => {
                ui.colored_label(
                    ui.visuals().warn_fg_color,
                    format!("Preview unavailable: {other:?}"),
                );
            }
        }
    });
    if let Some(texture) = &card.texture {
        let size = texture.size_vec2();
        let scale = (ui.available_width() / size.x.max(1.0)).min(1.0);
        ui.image((texture.id(), size * scale));
    } else {
        ui.monospace(&card.raw_source);
    }
}

fn prepare_safe_display_source(
    source: &str,
    local_assets: Option<&dyn LocalRasterResolver>,
    image_loader: &MarkdownAssetImageLoader,
    generation: u64,
    image_cache: &mut HashMap<String, Arc<egui::ColorImage>>,
    diagnostics: &mut Vec<String>,
) -> Result<String, String> {
    let mut output = String::with_capacity(source.len());
    let mut output_cursor = 0usize;
    let mut parser_events = 0usize;
    let mut image_count = 0usize;
    let mut aggregate_pixels = 0u64;
    let mut active: Option<ImageAltBuilder> = None;
    let mut nested_image_depth = 0usize;

    for (event, range) in Parser::new_ext(source, viewer_parser_options()).into_offset_iter() {
        parser_events = parser_events.saturating_add(1);
        if parser_events > MAX_PREVIEW_PARSER_EVENTS {
            return Err("Markdown preview parser event limit was exceeded".into());
        }

        match event {
            Event::Start(Tag::Image { dest_url, .. }) => {
                if active.is_some() {
                    nested_image_depth = nested_image_depth.saturating_add(1);
                    continue;
                }
                if !valid_utf8_range(source, &range) || range.start < output_cursor {
                    return Err("Markdown image source range was invalid".into());
                }
                output.push_str(&source[output_cursor..range.start]);
                output_cursor = range.end;
                active = Some(ImageAltBuilder {
                    destination: dest_url.to_string(),
                    alt: String::new(),
                });
                nested_image_depth = 1;
            }
            Event::End(TagEnd::Image) if active.is_some() => {
                nested_image_depth = nested_image_depth.saturating_sub(1);
                if nested_image_depth == 0 {
                    let image = active.take().expect("active image was checked");
                    image_count = image_count.saturating_add(1);
                    let replacement = if image_count <= MAX_PREVIEW_LOCAL_IMAGES {
                        let decoded = match local_assets {
                            Some(resolver) => decode_local_asset(
                                resolver,
                                &image.destination,
                                &mut aggregate_pixels,
                            ),
                            None => {
                                Err(format!("No local image resolver for {}", image.destination))
                            }
                        };
                        match decoded {
                            Ok(color_image) => {
                                let index = image_cache.len();
                                let uri = image_loader.uri(generation, index);
                                image_cache.insert(uri.clone(), Arc::new(color_image));
                                format!("![{}](<{uri}>)", escape_markdown_text(&image.alt))
                            }
                            Err(reason) => {
                                push_image_diagnostic(diagnostics, reason);
                                replacement_alt(&image.alt)
                            }
                        }
                    } else {
                        push_image_diagnostic(
                            diagnostics,
                            format!(
                                "Only the first {MAX_PREVIEW_LOCAL_IMAGES} images are previewed."
                            ),
                        );
                        replacement_alt(&image.alt)
                    };
                    output.push_str(&replacement);
                }
            }
            Event::Text(text) | Event::Code(text) | Event::Html(text) | Event::InlineHtml(text)
                if let Some(image) = active.as_mut() =>
            {
                image.alt.push_str(&text);
            }
            Event::SoftBreak | Event::HardBreak if active.is_some() => {
                if let Some(image) = active.as_mut() {
                    image.alt.push(' ');
                }
            }
            Event::TaskListMarker(checked) if active.is_some() => {
                if let Some(image) = active.as_mut() {
                    image
                        .alt
                        .push_str(if checked { "checked" } else { "unchecked" });
                }
            }
            _ => {}
        }
    }

    if active.is_some() || output_cursor > source.len() {
        return Err("Markdown image source range was incomplete".into());
    }
    output.push_str(&source[output_cursor..]);
    Ok(output)
}

struct ImageAltBuilder {
    destination: String,
    alt: String,
}

fn decode_local_asset(
    resolver: &dyn LocalRasterResolver,
    destination: &str,
    aggregate_pixels: &mut u64,
) -> Result<egui::ColorImage, String> {
    if !is_safe_relative_destination(destination) {
        return Err(if is_remote_destination(destination) {
            format!("Remote image blocked: {destination}")
        } else {
            format!("Only document-relative local images are previewed: {destination}")
        });
    }
    let asset = resolver
        .resolve(destination)
        .ok_or_else(|| format!("Local image is missing or blocked: {destination}"))?;
    if asset.bytes.is_empty()
        || asset.bytes.len() > MAX_LOCAL_RASTER_BYTES
        || !local_signature_matches(asset.format, asset.bytes)
    {
        return Err(format!(
            "Local image is invalid or too large: {destination}"
        ));
    }

    let format = image_format(asset.format);
    let (width, height) =
        image_025::ImageReader::with_format(std::io::Cursor::new(asset.bytes), format)
            .into_dimensions()
            .map_err(|_| format!("Could not read local image dimensions: {destination}"))?;
    let pixels = u64::from(width).saturating_mul(u64::from(height));
    if width == 0
        || height == 0
        || width > MAX_PREVIEW_LOCAL_DIMENSION
        || height > MAX_PREVIEW_LOCAL_DIMENSION
        || pixels > MAX_PREVIEW_LOCAL_PIXELS
        || aggregate_pixels.saturating_add(pixels) > MAX_PREVIEW_LOCAL_AGGREGATE_PIXELS
    {
        return Err(format!(
            "Local image dimensions exceed the preview limit: {destination}"
        ));
    }
    let decoded = image_025::load_from_memory(asset.bytes)
        .map_err(|_| format!("Could not decode local image: {destination}"))?
        .to_rgba8();
    *aggregate_pixels = aggregate_pixels.saturating_add(pixels);
    Ok(egui::ColorImage::from_rgba_unmultiplied(
        [width as usize, height as usize],
        decoded.as_raw(),
    ))
}

fn image_format(format: LocalRasterFormat) -> image_025::ImageFormat {
    match format {
        LocalRasterFormat::Png => image_025::ImageFormat::Png,
        LocalRasterFormat::Jpeg => image_025::ImageFormat::Jpeg,
        LocalRasterFormat::Gif => image_025::ImageFormat::Gif,
        LocalRasterFormat::WebP => image_025::ImageFormat::WebP,
    }
}

fn local_signature_matches(format: LocalRasterFormat, bytes: &[u8]) -> bool {
    match format {
        LocalRasterFormat::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
        LocalRasterFormat::Jpeg => bytes.starts_with(&[0xff, 0xd8, 0xff]),
        LocalRasterFormat::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
        LocalRasterFormat::WebP => {
            bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP"
        }
    }
}

fn is_safe_relative_destination(destination: &str) -> bool {
    !destination.is_empty()
        && destination.len() <= 8_192
        && !destination.starts_with(['/', '\\'])
        && !destination.contains('\\')
        && !destination.contains(':')
        && !destination
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        && Path::new(destination)
            .components()
            .all(|component| matches!(component, std::path::Component::Normal(_)))
}

fn is_remote_destination(destination: &str) -> bool {
    destination.starts_with("//")
        || destination.starts_with("\\\\")
        || destination.split_once(':').is_some_and(|(scheme, _)| {
            !scheme.is_empty()
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        })
}

fn replacement_alt(alt: &str) -> String {
    if alt.is_empty() {
        "[image unavailable]".into()
    } else {
        escape_markdown_text(alt)
    }
}

fn escape_markdown_text(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        if "\\`*_{}[]<>#+-.!|~".contains(character) {
            escaped.push('\\');
        }
        escaped.push(character);
    }
    escaped
}

fn push_image_diagnostic(diagnostics: &mut Vec<String>, message: String) {
    if diagnostics.len() < MAX_IMAGE_DIAGNOSTICS {
        diagnostics.push(message);
    } else if diagnostics.len() == MAX_IMAGE_DIAGNOSTICS {
        diagnostics.push("Additional unavailable images were omitted from diagnostics.".into());
    }
}

fn valid_utf8_range(source: &str, range: &Range<usize>) -> bool {
    range.start <= range.end
        && range.end <= source.len()
        && source.is_char_boundary(range.start)
        && source.is_char_boundary(range.end)
}

fn viewer_parser_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_DEFINITION_LIST
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAssets(Vec<(String, LocalRasterFormat, Vec<u8>)>);

    impl LocalRasterResolver for TestAssets {
        fn resolve<'a>(
            &'a self,
            destination: &str,
        ) -> Option<crate::notes_markdown::LocalRasterAsset<'a>> {
            self.0
                .iter()
                .find(|(name, _, _)| name == destination)
                .map(
                    |(_, format, bytes)| crate::notes_markdown::LocalRasterAsset {
                        format: *format,
                        bytes,
                    },
                )
        }
    }

    fn events(source: &str) -> Vec<Event<'_>> {
        Parser::new_ext(source, viewer_parser_options()).collect()
    }

    #[test]
    fn markdown_studio_preview_preserves_commonmark_quotes_lists_and_inline_formatting() {
        let source = "> 2. ordered **bold** item\n>    - nested *emphasis*\n>\n> 3. next item\n";
        let assets = TestAssets(Vec::new());
        let loader = MarkdownAssetImageLoader::new();
        let mut images = HashMap::new();
        let mut diagnostics = Vec::new();
        let display = prepare_safe_display_source(
            source,
            Some(&assets),
            &loader,
            1,
            &mut images,
            &mut diagnostics,
        )
        .unwrap();
        assert_eq!(display, source);
        let parsed = events(&display);
        assert!(
            parsed
                .iter()
                .any(|event| matches!(event, Event::Start(Tag::BlockQuote(_))))
        );
        assert!(
            parsed
                .iter()
                .any(|event| matches!(event, Event::Start(Tag::List(Some(2)))))
        );
        assert!(
            parsed
                .iter()
                .any(|event| matches!(event, Event::Start(Tag::List(None))))
        );
        assert!(
            parsed
                .iter()
                .any(|event| matches!(event, Event::Start(Tag::Strong)))
        );
        assert!(
            parsed
                .iter()
                .any(|event| matches!(event, Event::Start(Tag::Emphasis)))
        );
    }

    #[test]
    fn markdown_studio_preview_rewrites_valid_local_images_and_blocks_remote_destinations() {
        let directory = tempfile::tempdir().unwrap();
        let rgba = image_025::RgbaImage::from_pixel(1, 1, image_025::Rgba([20, 30, 40, 255]));
        let local_path = directory.path().join("local.png");
        rgba.save(&local_path).unwrap();
        let assets = TestAssets(vec![(
            "local.png".into(),
            LocalRasterFormat::Png,
            std::fs::read(local_path).unwrap(),
        )]);
        let source = "![local](local.png) and ![remote](https://example.invalid/remote.png) and ![missing](missing.png)";
        let loader = MarkdownAssetImageLoader::new();
        let mut images = HashMap::new();
        let mut diagnostics = Vec::new();
        let display = prepare_safe_display_source(
            source,
            Some(&assets),
            &loader,
            8,
            &mut images,
            &mut diagnostics,
        )
        .unwrap();
        assert!(display.contains("markdown-local://"));
        assert!(!display.contains("https://example.invalid/remote.png"));
        assert!(display.contains("remote"));
        assert!(display.contains("missing"));
        assert_eq!(images.len(), 1);
        assert_eq!(diagnostics.len(), 2);
        assert!(diagnostics[0].contains("Remote image blocked"));
    }

    #[test]
    fn markdown_studio_preview_keeps_healthy_mermaid_cards_when_one_card_fails() {
        let preview = MarkdownPreview::analyze(
            "```mermaid\nflowchart TD\n  A --> B\n```\n\n```mermaid\nflowchart TD\n  C --> D\n```\n",
        )
        .unwrap();
        let fences = preview.mermaid_fences();
        assert_eq!(fences.len(), 2);
        assert!(
            fences
                .iter()
                .all(|fence| matches!(fence.status(), MermaidFenceStatus::Current))
        );
        let ctx = egui::Context::default();
        let cards = build_mermaid_cards(fences, |fence, _remaining_pixels| {
            if fence.raw_source().contains("A --> B") {
                Err("test-local raster failure".into())
            } else {
                let image = egui::ColorImage::new([1, 1], vec![egui::Color32::WHITE]);
                Ok((
                    ctx.load_texture("healthy-card", image, egui::TextureOptions::LINEAR),
                    1,
                ))
            }
        });
        assert_eq!(cards.len(), 2);
        assert!(matches!(
            &cards[0].status,
            MermaidFenceStatus::PreviewEncodingFailed(_)
        ));
        assert!(cards[0].texture.is_none());
        assert!(matches!(&cards[1].status, MermaidFenceStatus::Current));
        assert!(cards[1].texture.is_some());
    }

    #[test]
    fn markdown_studio_preview_obeys_authoritative_global_preview_limits() {
        let source = "line\n".repeat(MAX_MARKDOWN_PREVIEW_LINES + 1);
        let preview = MarkdownPreview::analyze(&source).unwrap();
        assert_eq!(
            preview.status(),
            &MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::SourceLines)
        );
        let ctx = egui::Context::default();
        let loader = MarkdownAssetImageLoader::new();
        let cache = MarkdownPreviewCache::new(&ctx, 1, 1, &source, None, None, &loader).unwrap();
        assert!(cache.display_source.is_none());
        assert_eq!(cache.cards.len(), 0);
        assert!(!allows_commonmark_display(&cache.status));
    }

    #[test]
    fn markdown_studio_preview_cache_tracks_the_relative_asset_base_directory() {
        let first = tempfile::tempdir().unwrap();
        let second = tempfile::tempdir().unwrap();
        let source = "![asset](asset.png)";
        let ctx = egui::Context::default();
        let loader = MarkdownAssetImageLoader::new();
        let cache =
            MarkdownPreviewCache::new(&ctx, 4, 1, source, Some(first.path()), None, &loader)
                .unwrap();
        assert!(cache.matches(4, source, Some(first.path())));
        assert!(!cache.matches(4, source, Some(second.path())));
    }

    #[test]
    fn markdown_studio_filters_unsafe_explicit_link_commands_from_preview() {
        let ctx = egui::Context::default();
        let mut unsafe_rect = egui::Rect::NOTHING;
        let mut safe_rect = egui::Rect::NOTHING;
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            unsafe_rect = ui.hyperlink_to("unsafe link", "javascript:alert(1)").rect;
            safe_rect = ui.hyperlink_to("safe link", "https://example.com").rect;
        });
        output.drop_without_applying_deltas();

        let input = |point, pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        let mut blocked = 0usize;
        let press_output =
            ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(unsafe_rect.center()),
                        input(unsafe_rect.center(), true),
                    ],
                    ..Default::default()
                },
                |ui| {
                    ui.ctx().output_mut(|output| {
                        output.commands.push(egui::OutputCommand::OpenUrl(
                            egui::OpenUrl::same_tab("https://example.com"),
                        ));
                    });
                    let first_new_command = ui.ctx().output(|output| output.commands.len());
                    ui.hyperlink_to("unsafe link", "javascript:alert(1)");
                    ui.hyperlink_to("safe link", "https://example.com");
                    blocked = filter_unsafe_open_urls(ui.ctx(), first_new_command);
                },
            );
        press_output.drop_without_applying_deltas();
        let second_output =
            ctx.run_ui(
                egui::RawInput {
                    events: vec![
                        egui::Event::PointerMoved(unsafe_rect.center()),
                        input(unsafe_rect.center(), false),
                    ],
                    ..Default::default()
                },
                |ui| {
                    ui.ctx().output_mut(|output| {
                        output.commands.push(egui::OutputCommand::OpenUrl(
                            egui::OpenUrl::same_tab("https://example.com"),
                        ));
                    });
                    let first_new_command = ui.ctx().output(|output| output.commands.len());
                    ui.hyperlink_to("unsafe link", "javascript:alert(1)");
                    ui.hyperlink_to("safe link", "https://example.com");
                    blocked = filter_unsafe_open_urls(ui.ctx(), first_new_command);
                },
            );
        assert!(unsafe_rect.is_positive() && safe_rect.is_positive());
        assert_eq!(blocked, 1);
        assert!(second_output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::OpenUrl(url) if url.url == "https://example.com")));
        assert!(!second_output
            .platform_output
            .commands
            .iter()
            .any(|command| matches!(command, egui::OutputCommand::OpenUrl(url) if url.url.starts_with("javascript:"))));
        second_output.drop_without_applying_deltas();
    }

    #[test]
    fn markdown_studio_aggregate_mermaid_limit_keeps_commonmark_and_cached_cards_visible() {
        let source = "# Still visible\n\n```mermaid\nflowchart TD\nA --> B\n```\n";
        let ctx = egui::Context::default();
        let loader = MarkdownAssetImageLoader::new();
        let mut cache = MarkdownPreviewCache::new(&ctx, 1, 1, source, None, None, &loader).unwrap();
        assert!(cache.display_source.is_some());
        assert_eq!(cache.cards.len(), 1);
        assert!(cache.cards[0].texture.is_some());

        // The pure analyzer emits these aggregate statuses while retaining
        // its ordinary source analysis and any earlier healthy Mermaid cards.
        // Force the status here so this test exercises the cache's rendering
        // policy without making a large fixture that would invoke many real
        // rasterizers.
        cache.status =
            MarkdownPreviewStatus::LimitExceeded(MarkdownPreviewLimit::AggregateDiagramBytes);
        let output = ctx.run_ui(egui::RawInput::default(), |ui| {
            cache.draw(ui, &mut CommonMarkCache::default(), &loader);
        });
        assert!(output.shapes.iter().any(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(text) => {
                text.galley.job.text.contains("Still visible")
            }
            _ => false,
        }));
        assert!(output.shapes.iter().any(|clipped| match &clipped.shape {
            egui::epaint::Shape::Text(text) => {
                text.galley.job.text.contains("Mermaid diagram")
            }
            _ => false,
        }));
        output.drop_without_applying_deltas();
        assert!(matches!(
            &cache.cards[0].status,
            MermaidFenceStatus::Current
        ));
    }
}
