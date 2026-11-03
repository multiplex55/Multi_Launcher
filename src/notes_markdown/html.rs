//! Bounded CommonMark HTML export with conservative URL and asset handling.

use std::{fmt, io, io::Write, ops::Range};

use pulldown_cmark::{CowStr, Event, Options, Parser, Tag, TagEnd, html as cmark_html};
use url::Url;

use super::{
    authoring::validate_source,
    preview::{MarkdownPreview, MermaidFencePreview, MermaidFenceStatus},
};

pub const MAX_MARKDOWN_HTML_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_LOCAL_RASTER_BYTES: usize = 2 * 1024 * 1024;
pub const MAX_LOCAL_RASTER_AGGREGATE_BYTES: usize = 4 * 1024 * 1024;
const MAX_LOCAL_RASTER_DIMENSION: u32 = 8_192;
const MAX_LOCAL_RASTER_PIXELS: u64 = 16_777_216;
const MAX_MARKDOWN_HTML_EVENTS: usize = 1_000_000;
const MAX_MARKDOWN_HTML_IMAGES: usize = 512;
const MAX_MERMAID_SVG_AGGREGATE_BYTES: usize = 2 * 1024 * 1024;
const BASE_URL: &str = "https://markdown.invalid/";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkdownExportFormat {
    Markdown,
    Html,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MarkdownExportWarnings {
    unsafe_links: usize,
    unavailable_images: usize,
    blocked_remote_images: usize,
    unavailable_mermaid_previews: usize,
}

impl MarkdownExportWarnings {
    pub fn unsafe_links(self) -> usize {
        self.unsafe_links
    }

    pub fn unavailable_images(self) -> usize {
        self.unavailable_images
    }

    pub fn blocked_remote_images(self) -> usize {
        self.blocked_remote_images
    }

    pub fn unavailable_mermaid_previews(self) -> usize {
        self.unavailable_mermaid_previews
    }
}

/// Immutable export bytes captured from one exact Markdown source snapshot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MarkdownExportArtifact {
    format: MarkdownExportFormat,
    bytes: Vec<u8>,
    warnings: MarkdownExportWarnings,
}

impl MarkdownExportArtifact {
    pub fn format(&self) -> MarkdownExportFormat {
        self.format
    }

    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    pub fn warnings(&self) -> MarkdownExportWarnings {
        self.warnings
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LocalRasterFormat {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl LocalRasterFormat {
    fn mime_type(self) -> &'static str {
        match self {
            Self::Png => "image/png",
            Self::Jpeg => "image/jpeg",
            Self::Gif => "image/gif",
            Self::WebP => "image/webp",
        }
    }

    fn signature_matches(self, bytes: &[u8]) -> bool {
        match self {
            Self::Png => bytes.starts_with(b"\x89PNG\r\n\x1a\n"),
            Self::Jpeg => bytes.starts_with(&[0xff, 0xd8, 0xff]),
            Self::Gif => bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a"),
            Self::WebP => bytes.len() >= 12 && &bytes[..4] == b"RIFF" && &bytes[8..12] == b"WEBP",
        }
    }
}

/// Borrowed local raster bytes. The resolver owns path policy and must bound
/// its own lookup; export validates format and byte limits before encoding.
#[derive(Clone, Copy, Debug)]
pub struct LocalRasterAsset<'a> {
    pub format: LocalRasterFormat,
    pub bytes: &'a [u8],
}

/// Optional caller-supplied local-only resolver. This interface performs no IO.
pub trait LocalRasterResolver {
    fn resolve<'a>(&'a self, relative_destination: &str) -> Option<LocalRasterAsset<'a>>;
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MarkdownHtmlError {
    SourceTooLarge { max_bytes: usize },
    EventLimitExceeded,
    OutputLimitExceeded { max_bytes: usize },
}

impl fmt::Display for MarkdownHtmlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::SourceTooLarge { max_bytes } => {
                write!(
                    f,
                    "Markdown source exceeds the {max_bytes}-byte export limit"
                )
            }
            Self::EventLimitExceeded => f.write_str("Markdown HTML event limit exceeded"),
            Self::OutputLimitExceeded { max_bytes } => {
                write!(f, "HTML output exceeds the {max_bytes}-byte limit")
            }
        }
    }
}

impl std::error::Error for MarkdownHtmlError {}

/// Preserve Markdown exactly. No parsing or newline normalization is performed.
pub fn export_markdown(source: &str) -> Result<MarkdownExportArtifact, MarkdownHtmlError> {
    validate_source(source).map_err(|_| MarkdownHtmlError::SourceTooLarge {
        max_bytes: super::authoring::MAX_MARKDOWN_SOURCE_BYTES,
    })?;
    Ok(MarkdownExportArtifact {
        format: MarkdownExportFormat::Markdown,
        bytes: source.as_bytes().to_vec(),
        warnings: MarkdownExportWarnings::default(),
    })
}

/// Produce a static, script-free HTML document from the accepted full source.
/// Mermaid figures use only exact-range, current Diagram previews. The optional
/// resolver can provide local raster bytes, but this function never accesses a
/// filesystem or network.
pub fn export_html(
    preview: &MarkdownPreview,
    local_assets: Option<&dyn LocalRasterResolver>,
) -> Result<MarkdownExportArtifact, MarkdownHtmlError> {
    let source = preview.source();
    validate_source(source).map_err(|_| MarkdownHtmlError::SourceTooLarge {
        max_bytes: super::authoring::MAX_MARKDOWN_SOURCE_BYTES,
    })?;
    let mut output = BoundedWriter::new(MAX_MARKDOWN_HTML_BYTES);
    output.write_all(HTML_PREFIX.as_bytes()).map_err(|_| {
        MarkdownHtmlError::OutputLimitExceeded {
            max_bytes: MAX_MARKDOWN_HTML_BYTES,
        }
    })?;

    let fences = preview.mermaid_fences();
    let mut warnings = MarkdownExportWarnings::default();
    let mut events = Vec::new();
    let mut source_events = Parser::new_ext(source, html_options()).into_offset_iter();
    let mut event_count = 0usize;
    let mut aggregate_assets = 0usize;
    let mut aggregate_svg = 0usize;
    let mut image_count = 0usize;
    let mut skipped_code_block = false;
    let mut skipped_links = Vec::<bool>::new();
    let mut skipped_images = Vec::<bool>::new();

    while let Some((event, range)) = source_events.next() {
        event_count = event_count.saturating_add(1);
        if event_count > MAX_MARKDOWN_HTML_EVENTS {
            return Err(MarkdownHtmlError::EventLimitExceeded);
        }
        if skipped_code_block {
            if matches!(event, Event::End(TagEnd::CodeBlock)) {
                skipped_code_block = false;
            }
            continue;
        }
        match event {
            Event::Start(Tag::CodeBlock(kind)) if is_mermaid_codeblock(&kind) => {
                let fence = fences.iter().find(|fence| fence.full_range() == range);
                let figure = match fence {
                    Some(fence) => render_mermaid_fence(fence, &mut warnings, &mut aggregate_svg),
                    None => {
                        warnings.unavailable_mermaid_previews =
                            warnings.unavailable_mermaid_previews.saturating_add(1);
                        escaped_fence_fallback(
                            source,
                            range.clone(),
                            "Mermaid preview is unavailable",
                        )
                    }
                };
                events.push(Event::Html(CowStr::from(figure)));
                skipped_code_block = true;
            }
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                if let Some(destination) = safe_link_destination(&dest_url) {
                    skipped_links.push(false);
                    events.push(Event::Start(Tag::Link {
                        link_type,
                        dest_url: CowStr::from(destination),
                        title,
                        id,
                    }));
                } else {
                    warnings.unsafe_links = warnings.unsafe_links.saturating_add(1);
                    skipped_links.push(true);
                }
            }
            Event::End(TagEnd::Link) => {
                if !skipped_links.pop().unwrap_or(false) {
                    events.push(Event::End(TagEnd::Link));
                }
            }
            Event::Start(Tag::Image {
                link_type,
                dest_url,
                title,
                id,
            }) => {
                image_count = image_count.saturating_add(1);
                let resolved = resolve_local_asset(
                    &dest_url,
                    if image_count <= MAX_MARKDOWN_HTML_IMAGES {
                        local_assets
                    } else {
                        None
                    },
                    &mut aggregate_assets,
                );
                if let Some(data_url) = resolved {
                    skipped_images.push(false);
                    events.push(Event::Start(Tag::Image {
                        link_type,
                        dest_url: CowStr::from(data_url),
                        title,
                        id,
                    }));
                } else {
                    if is_remote_destination(&dest_url) {
                        warnings.blocked_remote_images =
                            warnings.blocked_remote_images.saturating_add(1);
                    } else {
                        warnings.unavailable_images = warnings.unavailable_images.saturating_add(1);
                    }
                    skipped_images.push(true);
                }
            }
            Event::End(TagEnd::Image) => {
                if !skipped_images.pop().unwrap_or(false) {
                    events.push(Event::End(TagEnd::Image));
                }
            }
            Event::Html(raw) | Event::InlineHtml(raw) => {
                events.push(Event::Text(raw));
            }
            other => events.push(other),
        }
    }

    cmark_html::write_html_io(&mut output, events.into_iter()).map_err(|_| {
        MarkdownHtmlError::OutputLimitExceeded {
            max_bytes: MAX_MARKDOWN_HTML_BYTES,
        }
    })?;
    output.write_all(HTML_SUFFIX.as_bytes()).map_err(|_| {
        MarkdownHtmlError::OutputLimitExceeded {
            max_bytes: MAX_MARKDOWN_HTML_BYTES,
        }
    })?;
    if output.exceeded {
        return Err(MarkdownHtmlError::OutputLimitExceeded {
            max_bytes: MAX_MARKDOWN_HTML_BYTES,
        });
    }
    Ok(MarkdownExportArtifact {
        format: MarkdownExportFormat::Html,
        bytes: output.bytes,
        warnings,
    })
}

fn html_options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_FOOTNOTES
}

fn is_mermaid_codeblock(kind: &pulldown_cmark::CodeBlockKind<'_>) -> bool {
    matches!(kind, pulldown_cmark::CodeBlockKind::Fenced(info)
        if info.split_ascii_whitespace().next().is_some_and(|language| language.eq_ignore_ascii_case("mermaid")))
}

fn render_mermaid_fence(
    fence: &MermaidFencePreview,
    warnings: &mut MarkdownExportWarnings,
    aggregate_svg_bytes: &mut usize,
) -> String {
    if let (MermaidFenceStatus::Current, Some(svg)) = (fence.status(), fence.trusted_svg()) {
        if aggregate_svg_bytes.saturating_add(svg.len()) <= MAX_MERMAID_SVG_AGGREGATE_BYTES {
            *aggregate_svg_bytes += svg.len();
            let svg_data = encode_base64(svg.as_bytes());
            return format!(
                "<figure class=\"markdown-mermaid\"><img alt=\"Mermaid diagram\" src=\"data:image/svg+xml;base64,{svg_data}\"></figure>\n"
            );
        }
        warnings.unavailable_mermaid_previews =
            warnings.unavailable_mermaid_previews.saturating_add(1);
        return format!(
            "<figure class=\"markdown-mermaid-unavailable\"><pre><code>{}</code></pre><figcaption>Mermaid SVG aggregate exceeds the {}-byte limit</figcaption></figure>\n",
            escape_html(fence.raw_source()),
            MAX_MERMAID_SVG_AGGREGATE_BYTES
        );
    }
    warnings.unavailable_mermaid_previews = warnings.unavailable_mermaid_previews.saturating_add(1);
    let message = match fence.status() {
        MermaidFenceStatus::Invalid(diagnostic) | MermaidFenceStatus::Unsupported(diagnostic) => {
            diagnostic.to_string()
        }
        MermaidFenceStatus::Unclosed => "Mermaid fence is not closed".to_owned(),
        MermaidFenceStatus::TooLarge { max_bytes } => {
            format!("Mermaid source exceeds the {max_bytes}-byte limit")
        }
        MermaidFenceStatus::AggregateLimitExceeded { max_bytes } => {
            format!("Mermaid preview aggregate exceeds the {max_bytes}-byte limit")
        }
        MermaidFenceStatus::SvgAggregateLimitExceeded { max_bytes } => {
            format!("Mermaid SVG preview aggregate exceeds the {max_bytes}-byte limit")
        }
        MermaidFenceStatus::PreviewEncodingFailed(reason) => reason.clone(),
        MermaidFenceStatus::Current => "Mermaid preview is unavailable".to_owned(),
    };
    format!(
        "<figure class=\"markdown-mermaid-unavailable\"><pre><code>{}</code></pre><figcaption>Mermaid preview unavailable: {}</figcaption></figure>\n",
        escape_html(fence.raw_source()),
        escape_html(&message)
    )
}

fn escaped_fence_fallback(source: &str, range: Range<usize>, message: &str) -> String {
    let raw = source.get(range).unwrap_or_default();
    format!(
        "<figure class=\"markdown-mermaid-unavailable\"><pre><code>{}</code></pre><figcaption>{}</figcaption></figure>\n",
        escape_html(raw),
        escape_html(message)
    )
}

fn safe_link_destination(destination: &str) -> Option<String> {
    if destination.is_empty()
        || destination.len() > 8192
        || destination
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || destination.contains('\\')
    {
        return None;
    }
    if let Ok(absolute) = Url::parse(destination) {
        return match absolute.scheme() {
            "http" | "https"
                if absolute.host_str().is_some()
                    && absolute.username().is_empty()
                    && absolute.password().is_none() =>
            {
                Some(absolute.to_string())
            }
            "mailto" if absolute.path().contains('@') => Some(absolute.to_string()),
            _ => None,
        };
    }
    let base = Url::parse(BASE_URL).ok()?;
    let joined = base.join(destination).ok()?;
    if joined.scheme() == "https" && joined.host_str() == base.host_str() {
        Some(destination.to_owned())
    } else {
        None
    }
}

fn is_remote_destination(destination: &str) -> bool {
    Url::parse(destination).is_ok()
        || destination.starts_with("//")
        || destination.starts_with("\\\\")
}

fn resolve_local_asset(
    destination: &str,
    resolver: Option<&dyn LocalRasterResolver>,
    aggregate_bytes: &mut usize,
) -> Option<String> {
    if !is_local_relative_destination(destination) {
        return None;
    }
    let asset = resolver?.resolve(destination)?;
    if asset.bytes.is_empty()
        || asset.bytes.len() > MAX_LOCAL_RASTER_BYTES
        || aggregate_bytes.saturating_add(asset.bytes.len()) > MAX_LOCAL_RASTER_AGGREGATE_BYTES
        || !asset.format.signature_matches(asset.bytes)
        || !raster_dimensions_within_limits(asset.format, asset.bytes)
    {
        return None;
    }
    *aggregate_bytes += asset.bytes.len();
    let encoded = encode_base64(asset.bytes);
    Some(format!(
        "data:{};base64,{encoded}",
        asset.format.mime_type()
    ))
}

fn raster_dimensions_within_limits(format: LocalRasterFormat, bytes: &[u8]) -> bool {
    let image_format = match format {
        LocalRasterFormat::Png => image_025::ImageFormat::Png,
        LocalRasterFormat::Jpeg => image_025::ImageFormat::Jpeg,
        LocalRasterFormat::Gif => image_025::ImageFormat::Gif,
        LocalRasterFormat::WebP => image_025::ImageFormat::WebP,
    };
    let Ok((width, height)) =
        image_025::ImageReader::with_format(std::io::Cursor::new(bytes), image_format)
            .into_dimensions()
    else {
        return false;
    };
    width > 0
        && height > 0
        && width <= MAX_LOCAL_RASTER_DIMENSION
        && height <= MAX_LOCAL_RASTER_DIMENSION
        && u64::from(width).saturating_mul(u64::from(height)) <= MAX_LOCAL_RASTER_PIXELS
}

fn is_local_relative_destination(destination: &str) -> bool {
    if destination.is_empty()
        || destination.len() > 8192
        || destination.starts_with(['/', '\\'])
        || destination
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || destination.contains('\\')
    {
        return false;
    }
    if Url::parse(destination).is_ok() {
        return false;
    }
    Url::parse(BASE_URL)
        .and_then(|base| base.join(destination))
        .is_ok_and(|joined| {
            joined.scheme() == "https" && joined.host_str() == Some("markdown.invalid")
        })
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut output = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let first = chunk[0];
        let second = *chunk.get(1).unwrap_or(&0);
        let third = *chunk.get(2).unwrap_or(&0);
        output.push(TABLE[(first >> 2) as usize] as char);
        output.push(TABLE[(((first & 0x03) << 4) | (second >> 4)) as usize] as char);
        if chunk.len() > 1 {
            output.push(TABLE[(((second & 0x0f) << 2) | (third >> 6)) as usize] as char);
        } else {
            output.push('=');
        }
        if chunk.len() > 2 {
            output.push(TABLE[(third & 0x3f) as usize] as char);
        } else {
            output.push('=');
        }
    }
    output
}

fn escape_html(text: &str) -> String {
    let mut escaped = String::with_capacity(text.len());
    for character in text.chars() {
        match character {
            '&' => escaped.push_str("&amp;"),
            '<' => escaped.push_str("&lt;"),
            '>' => escaped.push_str("&gt;"),
            '"' => escaped.push_str("&quot;"),
            '\'' => escaped.push_str("&#39;"),
            _ => escaped.push(character),
        }
    }
    escaped
}

#[derive(Debug)]
struct BoundedWriter {
    bytes: Vec<u8>,
    max_bytes: usize,
    exceeded: bool,
}

impl BoundedWriter {
    fn new(max_bytes: usize) -> Self {
        Self {
            bytes: Vec::new(),
            max_bytes,
            exceeded: false,
        }
    }
}

impl Write for BoundedWriter {
    fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
        let Some(new_len) = self.bytes.len().checked_add(buffer.len()) else {
            self.exceeded = true;
            return Err(io::Error::other("bounded HTML output overflow"));
        };
        if new_len > self.max_bytes {
            self.exceeded = true;
            return Err(io::Error::other("bounded HTML output limit exceeded"));
        }
        self.bytes.extend_from_slice(buffer);
        Ok(buffer.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

const HTML_PREFIX: &str = "<!doctype html>\n<html lang=\"en\"><head><meta charset=\"utf-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><meta http-equiv=\"Content-Security-Policy\" content=\"default-src 'none'; img-src data:; style-src 'unsafe-inline'; base-uri 'none'; form-action 'none'; object-src 'none'\"><title>Markdown export</title><style>\n:root{color-scheme:light dark;font-family:system-ui,sans-serif;line-height:1.55}body{max-width:76rem;margin:2rem auto;padding:0 1.25rem}pre{overflow:auto;padding:1rem;border-radius:.5rem;background:color-mix(in srgb,CanvasText 8%,Canvas)}code{font-family:ui-monospace,monospace}table{border-collapse:collapse}th,td{border:1px solid color-mix(in srgb,CanvasText 35%,Canvas);padding:.4rem .65rem}blockquote{margin-inline:0;padding:.25rem 1rem;border-inline-start:.25rem solid #6688aa}img,svg{max-width:100%;height:auto}.markdown-mermaid{overflow:auto}.markdown-mermaid-unavailable{border-inline-start:.25rem solid #c58b25;padding-inline-start:1rem}a{overflow-wrap:anywhere}</style></head><body>\n";
const HTML_SUFFIX: &str = "\n</body></html>\n";

#[cfg(test)]
mod tests {
    use super::*;

    struct TestAssets {
        bytes: Vec<u8>,
    }

    impl TestAssets {
        fn png(width: u32, height: u32) -> Self {
            let image = image_025::DynamicImage::ImageRgba8(image_025::ImageBuffer::from_pixel(
                width,
                height,
                image_025::Rgba([20, 40, 60, 255]),
            ));
            let mut bytes = std::io::Cursor::new(Vec::new());
            image
                .write_to(&mut bytes, image_025::ImageFormat::Png)
                .expect("PNG fixture should encode");
            Self {
                bytes: bytes.into_inner(),
            }
        }

        fn malformed_png() -> Self {
            Self {
                bytes: b"\x89PNG\r\n\x1a\nsmall test".to_vec(),
            }
        }
    }

    impl LocalRasterResolver for TestAssets {
        fn resolve<'a>(&'a self, destination: &str) -> Option<LocalRasterAsset<'a>> {
            (destination == "images/pixel.png").then_some(LocalRasterAsset {
                format: LocalRasterFormat::Png,
                bytes: &self.bytes,
            })
        }
    }

    #[test]
    fn markdown_export_preserves_exact_source_and_html_escapes_raw_html() {
        let source = "# 日記\r\n\r\n<script>alert(1)</script>\r\n";
        assert_eq!(export_markdown(source).unwrap().bytes(), source.as_bytes());
        let preview = MarkdownPreview::analyze(source).unwrap();
        let html = export_html(&preview, None).unwrap();
        let output = std::str::from_utf8(html.bytes()).unwrap();
        assert!(!output.contains("<script>"));
        assert!(output.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(output.contains("Content-Security-Policy"));
        assert!(!output.contains("<script"));
    }

    #[test]
    fn markdown_html_rejects_executable_urls_and_does_not_load_remote_images() {
        let source = "[bad](<javascript:alert(1)>) [good](https://example.com/x) ![remote](https://example.com/x.png)";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let html = export_html(&preview, None).unwrap();
        let output = std::str::from_utf8(html.bytes()).unwrap();
        assert!(!output.contains("javascript:"));
        assert!(output.contains("href=\"https://example.com/x\""));
        assert!(!output.contains("src=\"https://example.com/x.png\""));
        assert!(output.contains("remote"));
        assert_eq!(html.warnings().unsafe_links(), 1);
        assert_eq!(html.warnings().blocked_remote_images(), 1);
    }

    #[test]
    fn markdown_html_embeds_local_rasters_and_accepted_mermaid_only() {
        let source = "![pixel](images/pixel.png)\n\n```mermaid\nflowchart TD\nA --> B\n```\n\n```mermaid\nflowchart TD\nA -->\n```\n";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let html = export_html(&preview, Some(&TestAssets::png(1, 1))).unwrap();
        let output = std::str::from_utf8(html.bytes()).unwrap();
        assert!(output.contains("data:image/png;base64,"));
        assert!(output.contains("data:image/svg+xml;base64,"));
        assert!(output.contains("Mermaid preview unavailable"));
        assert!(!output.contains("flowchart TD\nA --&gt; B"));
        assert_eq!(html.warnings().unavailable_mermaid_previews(), 1);
    }

    #[test]
    fn markdown_html_isolates_multiple_mermaid_svg_documents_and_bounds_local_rasters() {
        let source = "```mermaid\nflowchart TD\nA --> B\n```\n\n```mermaid\nflowchart LR\nC --> D\n```\n\n![large](images/pixel.png)";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let large = TestAssets::png(MAX_LOCAL_RASTER_DIMENSION + 1, 1);
        let html = export_html(&preview, Some(&large)).unwrap();
        let output = std::str::from_utf8(html.bytes()).unwrap();
        let embedded_svgs = output
            .split("data:image/svg+xml;base64,")
            .skip(1)
            .map(|rest| rest.split('"').next().unwrap_or_default())
            .collect::<Vec<_>>();
        assert_eq!(embedded_svgs.len(), 2);
        assert_ne!(embedded_svgs[0], embedded_svgs[1]);
        assert!(!output.contains("data:image/png;base64,"));
        assert_eq!(html.warnings().unavailable_images(), 1);

        let malformed = TestAssets::malformed_png();
        let malformed_html = export_html(
            &MarkdownPreview::analyze("![broken](images/pixel.png)").unwrap(),
            Some(&malformed),
        )
        .unwrap();
        assert!(
            !std::str::from_utf8(malformed_html.bytes())
                .unwrap()
                .contains("data:image/png;base64,")
        );
        assert_eq!(malformed_html.warnings().unavailable_images(), 1);
    }

    #[test]
    fn markdown_html_uses_shared_analysis_for_wiki_tasks_callouts_and_code() {
        let source = "# Café 日記\n\n[[linked-note]]\n\n- [ ] TODO <!-- ml:todo:task-1 -->\n\n> [!WARNING] 注意\n> body\n\n`inline` and ![missing](images/missing.png)\n";
        let preview = MarkdownPreview::analyze(source).unwrap();
        let analysis = preview.analysis().unwrap();
        assert_eq!(analysis.headings[0].title, "Café 日記");
        assert_eq!(analysis.task_items.len(), 1);
        assert_eq!(analysis.callouts[0].kind, "warning");
        let html = export_html(&preview, None).unwrap();
        let output = std::str::from_utf8(html.bytes()).unwrap();
        assert!(output.contains("linked-note"));
        assert!(output.contains("<code>inline</code>"));
        assert!(output.contains("missing"));
        assert_eq!(html.warnings().unavailable_images(), 1);
    }

    #[test]
    fn markdown_html_requires_exact_preview_source_and_bounds_output() {
        let preview = MarkdownPreview::analyze("# one\n").unwrap();
        assert_eq!(
            export_html(&preview, None).unwrap().format(),
            MarkdownExportFormat::Html
        );
        assert!(matches!(
            export_markdown(&"x".repeat(super::super::authoring::MAX_MARKDOWN_SOURCE_BYTES + 1)),
            Err(MarkdownHtmlError::SourceTooLarge { .. })
        ));
        let source = format!(
            "```mermaid\nflowchart TD\nA --> B\n```\n{}",
            "x".repeat(128)
        );
        let preview = MarkdownPreview::analyze(&source).unwrap();
        let html = export_html(&preview, None).unwrap();
        assert!(html.bytes().len() <= MAX_MARKDOWN_HTML_BYTES);
    }
}
