//! Bounded, in-memory rendering for general Mermaid source.

use mermaid_rs_renderer::{
    DiagramKind, LayoutConfig, Theme, compute_layout, measure_svg_dimensions, parse_mermaid_strict,
    render_svg,
};
use resvg::{tiny_skia, usvg};
use std::collections::{HashMap, VecDeque};
use std::fmt;
use std::hash::{Hash, Hasher};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};

const MAX_RENDER_SOURCE_BYTES: usize = super::MAX_MERMAID_SOURCE_BYTES;
const MAX_RENDER_SOURCE_LINES: usize = super::MAX_MERMAID_SOURCE_LINES;
const MAX_RENDER_LINE_BYTES: usize = super::MAX_MERMAID_LINE_BYTES;
const MAX_RENDER_NESTING: usize = 64;
const MAX_PARSED_ITEMS: usize = 16_384;
const MAX_RENDER_SIDE: u32 = super::MAX_EXPORT_SIDE;
const MAX_RENDER_PIXELS: u64 = super::MAX_EXPORT_PIXELS;
const MAX_RENDER_SVG_BYTES: usize = super::MAX_SVG_BYTES;
const MAX_CACHED_ARTIFACTS: usize = 4;
const MAX_CACHED_RASTER_VARIANTS_PER_ARTIFACT: usize = 3;
const MAX_CACHED_RASTER_PIXELS: u64 = MAX_RENDER_PIXELS;
const SOURCE_EDIT_DEBOUNCE: Duration = Duration::from_millis(180);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramFamily {
    Flowchart,
    Mindmap,
    Sequence,
    Class,
    State,
    EntityRelationship,
    Gantt,
    Timeline,
    UserJourney,
    Pie,
    GitGraph,
    Other,
}

/// Selects one of the bounded source-rendered Mermaid families. The strict
/// parser and family admission checks still decide whether its source renders.
pub(crate) fn generic_preview_family(source: &str) -> Option<DiagramFamily> {
    let header = source.lines().find_map(|raw_line| {
        let line = strip_trailing_mermaid_comment(raw_line).trim();
        (!line.is_empty() && !line.starts_with("%%")).then_some(line)
    })?;
    let token = header.split_whitespace().next()?.to_ascii_lowercase();
    match token.as_str() {
        "mindmap" => Some(DiagramFamily::Mindmap),
        "sequencediagram" => Some(DiagramFamily::Sequence),
        "classdiagram" => Some(DiagramFamily::Class),
        "statediagram" | "statediagram-v2" => Some(DiagramFamily::State),
        "erdiagram" => Some(DiagramFamily::EntityRelationship),
        "gantt" => Some(DiagramFamily::Gantt),
        "timeline" => Some(DiagramFamily::Timeline),
        "journey" => Some(DiagramFamily::UserJourney),
        "pie" => Some(DiagramFamily::Pie),
        "gitgraph" => Some(DiagramFamily::GitGraph),
        _ => None,
    }
}

/// Native flowcharts remain on the graph-session path; the ten additional
/// families use the source-rendered adapter.
pub(crate) fn is_generic_preview_candidate(source: &str) -> bool {
    generic_preview_family(source).is_some()
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum RenderTheme {
    #[default]
    Modern,
    MermaidDefault,
}

/// Renderer-affecting options included in the cache key.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct RenderOptions {
    pub theme: RenderTheme,
}

/// Runtime document identity. It is intentionally opaque and is never saved.
#[derive(Clone)]
pub struct RenderDocumentIdentity(Arc<()>);

impl RenderDocumentIdentity {
    pub fn fresh() -> Self {
        Self(Arc::new(()))
    }
}

impl fmt::Debug for RenderDocumentIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("RenderDocumentIdentity")
    }
}

impl PartialEq for RenderDocumentIdentity {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl Eq for RenderDocumentIdentity {}

impl Hash for RenderDocumentIdentity {
    fn hash<H: Hasher>(&self, state: &mut H) {
        std::ptr::hash(Arc::as_ptr(&self.0), state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RenderKey {
    pub document: RenderDocumentIdentity,
    pub accepted_revision: u64,
    pub options: RenderOptions,
}

impl RenderKey {
    pub fn new(
        document: RenderDocumentIdentity,
        accepted_revision: u64,
        options: RenderOptions,
    ) -> Self {
        Self {
            document,
            accepted_revision,
            options,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiagramRenderError {
    SourceTooLarge,
    TooManyLines,
    LineTooLarge,
    ExcessiveNesting,
    InvalidSource,
    UnqualifiedFamily,
    WorkerPanicked,
    UnsupportedLayout,
    WorkerUnavailable,
    DiagramTooComplex,
    SvgTooLarge,
    InvalidDimensions,
    RasterLimitExceeded,
    RasterizationFailed,
}

impl fmt::Display for DiagramRenderError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::SourceTooLarge => "Mermaid source exceeds the rendering size limit",
            Self::TooManyLines => "Mermaid source exceeds the rendering line limit",
            Self::LineTooLarge => "A Mermaid source line exceeds the rendering limit",
            Self::ExcessiveNesting => "Mermaid source exceeds the rendering nesting limit",
            Self::InvalidSource => "Mermaid source could not be parsed completely",
            Self::UnqualifiedFamily => {
                "This Mermaid diagram family is not qualified for preview yet"
            }
            Self::WorkerPanicked => "Mermaid preview rendering stopped unexpectedly",
            Self::UnsupportedLayout => "Mermaid source could not be laid out",
            Self::WorkerUnavailable => "The Mermaid preview worker could not be started",
            Self::DiagramTooComplex => "Mermaid diagram exceeds the rendering complexity limit",
            Self::SvgTooLarge => "Rendered SVG exceeds the size limit",
            Self::InvalidDimensions => "Rendered diagram has invalid dimensions",
            Self::RasterLimitExceeded => "Requested raster size exceeds the rendering limit",
            Self::RasterizationFailed => "Rendered SVG could not be rasterized",
        })
    }
}

impl std::error::Error for DiagramRenderError {}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RasterSize {
    width: u32,
    height: u32,
}

impl RasterSize {
    pub fn new(width: u32, height: u32) -> Result<Self, DiagramRenderError> {
        let pixels = u64::from(width)
            .checked_mul(u64::from(height))
            .ok_or(DiagramRenderError::RasterLimitExceeded)?;
        if width == 0
            || height == 0
            || width > MAX_RENDER_SIDE
            || height > MAX_RENDER_SIDE
            || pixels > MAX_RENDER_PIXELS
        {
            return Err(DiagramRenderError::RasterLimitExceeded);
        }
        Ok(Self { width, height })
    }

    pub fn width(self) -> u32 {
        self.width
    }

    pub fn height(self) -> u32 {
        self.height
    }
}

#[derive(Clone, Debug)]
pub struct RasterImage {
    pub width: u32,
    pub height: u32,
    /// Straight-alpha RGBA8 pixels, stored row-major.
    pub rgba: Arc<[u8]>,
}

#[derive(Clone, Debug)]
pub struct DiagramRenderOutput {
    pub family: DiagramFamily,
    pub svg: Arc<str>,
    pub natural_width: f32,
    pub natural_height: f32,
    pub raster: Arc<RasterImage>,
}

/// Parse, lay out, render and rasterize one exact Mermaid source string.
///
/// The caller should run this on a background worker. The function preserves
/// the source as received and rejects input/output that exceeds repository
/// limits before it can be published as a current preview.
pub fn render_mermaid_preview(
    source: &str,
    raster_size: RasterSize,
) -> Result<DiagramRenderOutput, DiagramRenderError> {
    render_mermaid_preview_with_options(source, raster_size, RenderOptions::default())
}

pub fn render_mermaid_preview_with_options(
    source: &str,
    raster_size: RasterSize,
    options: RenderOptions,
) -> Result<DiagramRenderOutput, DiagramRenderError> {
    RasterSize::new(raster_size.width, raster_size.height)?;
    let counters = RenderCounters::default();
    let artifact = build_render_artifact(source, options, Some(&counters))?;
    let raster = rasterize_svg(&artifact.svg, raster_size, Some(&counters))?;
    Ok(make_render_output(artifact, Arc::new(raster)))
}

fn build_render_artifact(
    source: &str,
    options: RenderOptions,
    counters: Option<&RenderCounters>,
) -> Result<Arc<RenderArtifact>, DiagramRenderError> {
    preflight_source(source)?;
    if let Some(counters) = counters {
        counters.parse_count.fetch_add(1, Ordering::Relaxed);
    }
    let parsed = parse_mermaid_strict(source).map_err(|_| DiagramRenderError::InvalidSource)?;
    if parsed_item_count(&parsed.graph) > MAX_PARSED_ITEMS {
        return Err(DiagramRenderError::DiagramTooComplex);
    }
    let family = match parsed.graph.kind {
        DiagramKind::Flowchart => {
            super::parse_mermaid(source).map_err(|_| DiagramRenderError::InvalidSource)?;
            DiagramFamily::Flowchart
        }
        DiagramKind::Mindmap => {
            validate_mindmap_coverage(source, parsed.graph.mindmap.nodes.len())?;
            DiagramFamily::Mindmap
        }
        DiagramKind::Sequence => {
            validate_sequence_capability(source, &parsed.graph)?;
            DiagramFamily::Sequence
        }
        DiagramKind::Class => {
            validate_class_capability(source, &parsed.graph)?;
            DiagramFamily::Class
        }
        DiagramKind::State => {
            validate_state_capability(source, &parsed.graph)?;
            DiagramFamily::State
        }
        DiagramKind::Er => {
            validate_entity_relationship_capability(source, &parsed.graph)?;
            DiagramFamily::EntityRelationship
        }
        DiagramKind::Gantt => {
            validate_gantt_capability(source, &parsed.graph)?;
            DiagramFamily::Gantt
        }
        DiagramKind::Timeline => {
            validate_timeline_capability(source, &parsed.graph)?;
            DiagramFamily::Timeline
        }
        DiagramKind::Journey => {
            validate_user_journey_capability(source, &parsed.graph)?;
            DiagramFamily::UserJourney
        }
        DiagramKind::Pie => {
            validate_pie_capability(source, &parsed.graph)?;
            DiagramFamily::Pie
        }
        DiagramKind::GitGraph => {
            validate_gitgraph_capability(source, &parsed.graph)?;
            DiagramFamily::GitGraph
        }
        _ => return Err(DiagramRenderError::UnqualifiedFamily),
    };

    let theme = match options.theme {
        RenderTheme::Modern => Theme::modern(),
        RenderTheme::MermaidDefault => Theme::mermaid_default(),
    };
    let config = LayoutConfig::default();
    if let Some(counters) = counters {
        counters.layout_count.fetch_add(1, Ordering::Relaxed);
    }
    let layout = compute_layout(&parsed.graph, &theme, &config);
    if matches!(
        layout.diagram,
        mermaid_rs_renderer::layout::DiagramData::Error(_)
    ) {
        return Err(DiagramRenderError::UnsupportedLayout);
    }
    let dimensions = measure_svg_dimensions(&layout, &config, None);
    validate_svg_dimensions(dimensions.width, dimensions.height)?;

    let svg: Arc<str> = Arc::from(render_svg(&layout, &theme, &config));
    if svg.is_empty() || svg.len() > MAX_RENDER_SVG_BYTES {
        return Err(DiagramRenderError::SvgTooLarge);
    }
    Ok(Arc::new(RenderArtifact {
        family,
        svg,
        natural_width: dimensions.width,
        natural_height: dimensions.height,
    }))
}

fn make_render_output(
    artifact: Arc<RenderArtifact>,
    raster: Arc<RasterImage>,
) -> DiagramRenderOutput {
    DiagramRenderOutput {
        family: artifact.family,
        svg: Arc::clone(&artifact.svg),
        natural_width: artifact.natural_width,
        natural_height: artifact.natural_height,
        raster,
    }
}

fn validate_mindmap_coverage(source: &str, parsed_nodes: usize) -> Result<(), DiagramRenderError> {
    let mut header_seen = false;
    let mut expected_nodes = 0usize;
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("%%") {
            if line.starts_with("%%{") {
                return Err(DiagramRenderError::InvalidSource);
            }
            continue;
        }
        if line == "---" || line.starts_with("--- ") {
            return Err(DiagramRenderError::InvalidSource);
        }
        if !header_seen && line.eq_ignore_ascii_case("mindmap") {
            header_seen = true;
            continue;
        }
        if !header_seen {
            return Err(DiagramRenderError::InvalidSource);
        }
        expected_nodes = expected_nodes.saturating_add(1);
    }
    if header_seen && expected_nodes == parsed_nodes {
        Ok(())
    } else {
        Err(DiagramRenderError::InvalidSource)
    }
}

/// M2-B qualifies the renderer's simple participant-and-message sequence
/// subset. Other sequence constructs remain unsupported until their behavior
/// has dedicated fixtures; this checks parser accounting rather than trying
/// to duplicate the renderer's grammar.
fn validate_sequence_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let mut header_seen = false;
    let mut participant_lines = 0usize;
    let mut message_lines = 0usize;
    for raw_line in source.lines() {
        let line = strip_trailing_mermaid_comment(raw_line).trim();
        if line.is_empty() {
            if raw_line.trim_start().starts_with("%%{") {
                return Err(DiagramRenderError::InvalidSource);
            }
            continue;
        }
        if !header_seen && line.eq_ignore_ascii_case("sequencediagram") {
            header_seen = true;
            continue;
        }
        if !header_seen {
            return Err(DiagramRenderError::InvalidSource);
        }
        let lower = line.to_ascii_lowercase();
        if lower.starts_with("participant ") || lower.starts_with("actor ") {
            if line.split_whitespace().nth(1).is_none() {
                return Err(DiagramRenderError::InvalidSource);
            }
            participant_lines = participant_lines.saturating_add(1);
        } else if is_sequence_message_statement(line) {
            message_lines = message_lines.saturating_add(1);
        } else {
            return Err(DiagramRenderError::InvalidSource);
        }
    }
    if !header_seen
        || participant_lines > graph.sequence_participants.len()
        || message_lines != graph.edges.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn family_statements<'a>(
    source: &'a str,
    headers: &[&str],
) -> Result<Vec<&'a str>, DiagramRenderError> {
    let mut header_seen = false;
    let mut statements = Vec::new();
    for raw_line in source.lines() {
        if raw_line.trim_start().starts_with("%%{") {
            return Err(DiagramRenderError::InvalidSource);
        }
        let line = strip_trailing_mermaid_comment(raw_line).trim();
        if line.is_empty() || line.starts_with("%%") {
            if line.starts_with("%%{") {
                return Err(DiagramRenderError::InvalidSource);
            }
            continue;
        }
        if line == "---" {
            return Err(DiagramRenderError::InvalidSource);
        }
        if !header_seen {
            let token = line.split_whitespace().next().unwrap_or_default();
            if headers
                .iter()
                .any(|header| token.eq_ignore_ascii_case(header))
            {
                header_seen = true;
                continue;
            }
            return Err(DiagramRenderError::InvalidSource);
        }
        statements.push(line);
    }
    if header_seen {
        Ok(statements)
    } else {
        Err(DiagramRenderError::InvalidSource)
    }
}

fn validate_class_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["classDiagram"])?;
    let mut declarations = 0usize;
    let mut relations = 0usize;
    let mut open_class = false;
    let mut members = Vec::new();
    for line in statements {
        if open_class {
            if line == "}" {
                open_class = false;
            } else if line.len() > 1 && matches!(line.as_bytes()[0], b'+' | b'-' | b'#' | b'~') {
                members.push(line);
            } else {
                return Err(DiagramRenderError::InvalidSource);
            }
            continue;
        }
        if let Some(declaration) = line.strip_prefix("class ") {
            let declaration = declaration.trim();
            let (identifier, has_body) = declaration
                .strip_suffix('{')
                .map_or((declaration, false), |identifier| (identifier.trim(), true));
            if !is_simple_identifier(identifier) {
                return Err(DiagramRenderError::InvalidSource);
            }
            declarations = declarations.saturating_add(1);
            open_class = has_body;
        } else if has_class_relation_operator(line) {
            relations = relations.saturating_add(1);
        } else {
            return Err(DiagramRenderError::InvalidSource);
        }
    }
    if open_class
        || declarations == 0
        || graph.nodes.len() < declarations
        || graph.edges.len() != relations
        || members.iter().any(|member| {
            !graph
                .nodes
                .values()
                .any(|node| node.label.contains(member.trim()))
        })
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn has_class_relation_operator(line: &str) -> bool {
    [
        "<|--", "--|>", "*--", "--*", "o--", "--o", "<|..", "..|>", "<..", "..>", "<--", "-->",
        "..", "--",
    ]
    .iter()
    .any(|operator| line.contains(operator))
}

fn is_simple_identifier(value: &str) -> bool {
    let mut bytes = value.bytes();
    bytes
        .next()
        .is_some_and(|first| first.is_ascii_alphabetic() || first == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn validate_state_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["stateDiagram", "stateDiagram-v2"])?;
    if statements.is_empty()
        || statements.iter().any(|line| !line.contains("-->"))
        || graph.edges.len() != statements.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_entity_relationship_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["erDiagram"])?;
    if statements.is_empty()
        || statements
            .iter()
            .any(|line| !line.contains("--") || !line.contains(':'))
        || graph.edges.len() != statements.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_gantt_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["gantt"])?;
    let mut title_count = 0usize;
    let mut section_count = 0usize;
    let mut task_count = 0usize;
    for line in statements {
        let lower = line.to_ascii_lowercase();
        if let Some(title) = lower.strip_prefix("title ") {
            if title.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            title_count = title_count.saturating_add(1);
        } else if let Some(section) = lower.strip_prefix("section ") {
            if section.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            section_count = section_count.saturating_add(1);
        } else if let Some(format) = lower.strip_prefix("dateformat ") {
            if format.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
        } else if let Some((label, details)) = line.split_once(':') {
            if label.trim().is_empty() || details.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            task_count = task_count.saturating_add(1);
        } else {
            return Err(DiagramRenderError::InvalidSource);
        }
    }
    if title_count > 1
        || (title_count == 1) != graph.gantt_title.is_some()
        || section_count != graph.gantt_sections.len()
        || task_count != graph.gantt_tasks.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_timeline_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["timeline"])?;
    let mut title_count = 0usize;
    let mut section_count = 0usize;
    let mut event_count = 0usize;
    for line in statements {
        let lower = line.to_ascii_lowercase();
        if let Some(title) = lower.strip_prefix("title ") {
            if title.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            title_count = title_count.saturating_add(1);
        } else if let Some(section) = lower.strip_prefix("section ") {
            if section.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            section_count = section_count.saturating_add(1);
        } else if let Some((time, event)) = line.split_once(':') {
            if time.trim().is_empty() || event.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            event_count = event_count.saturating_add(1);
        } else {
            return Err(DiagramRenderError::InvalidSource);
        }
    }
    if title_count > 1
        || (title_count == 1) != graph.timeline.title.is_some()
        || section_count != graph.timeline.sections.len()
        || event_count != graph.timeline.events.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_user_journey_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["journey"])?;
    let mut title_count = 0usize;
    let mut section_count = 0usize;
    let mut task_count = 0usize;
    for line in statements {
        let lower = line.to_ascii_lowercase();
        if let Some(title) = lower.strip_prefix("title ") {
            if title.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            title_count = title_count.saturating_add(1);
        } else if let Some(section) = lower.strip_prefix("section ") {
            if section.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            section_count = section_count.saturating_add(1);
        } else {
            let mut parts = line.split(':').map(str::trim);
            let label = parts.next().unwrap_or_default();
            let score = parts.next().and_then(|value| value.parse::<f32>().ok());
            if label.is_empty()
                || !score.is_some_and(|score| score.is_finite() && (0.0..=5.0).contains(&score))
            {
                return Err(DiagramRenderError::InvalidSource);
            }
            task_count = task_count.saturating_add(1);
        }
    }
    if title_count > 1
        || (title_count == 1) != graph.journey_title.is_some()
        || section_count != graph.subgraphs.len()
        || task_count != graph.nodes.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_pie_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["pie"])?;
    let mut title_count = 0usize;
    let mut slice_count = 0usize;
    for line in statements {
        let lower = line.to_ascii_lowercase();
        if lower == "showdata" {
            continue;
        }
        if let Some(title) = lower.strip_prefix("title ") {
            if title.trim().is_empty() {
                return Err(DiagramRenderError::InvalidSource);
            }
            title_count = title_count.saturating_add(1);
            continue;
        }
        let Some((label, value)) = line.split_once(':') else {
            return Err(DiagramRenderError::InvalidSource);
        };
        let value = value.trim().parse::<f32>().ok();
        if label.trim().is_empty() || !value.is_some_and(|value| value.is_finite() && value > 0.0) {
            return Err(DiagramRenderError::InvalidSource);
        }
        slice_count = slice_count.saturating_add(1);
    }
    if title_count > 1
        || (title_count == 1) != graph.pie_title.is_some()
        || slice_count != graph.pie_slices.len()
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn validate_gitgraph_capability(
    source: &str,
    graph: &mermaid_rs_renderer::Graph,
) -> Result<(), DiagramRenderError> {
    let statements = family_statements(source, &["gitgraph"])?;
    let mut commit_count = 0usize;
    let mut branches = std::collections::HashSet::from(["main".to_owned()]);
    for line in statements {
        let mut parts = line.split_whitespace();
        match parts
            .next()
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "commit" => {
                let remainder = line.strip_prefix("commit").unwrap_or_default().trim();
                if !remainder.is_empty()
                    && !(remainder.starts_with("id:") && remainder.contains('"'))
                {
                    return Err(DiagramRenderError::InvalidSource);
                }
                commit_count = commit_count.saturating_add(1);
            }
            "branch" | "checkout" | "switch" => {
                let command = parts.next().unwrap_or_default();
                if !is_simple_identifier(command) || parts.next().is_some() {
                    return Err(DiagramRenderError::InvalidSource);
                }
                branches.insert(command.to_owned());
            }
            "merge" => {
                let branch = parts.next().unwrap_or_default();
                if !is_simple_identifier(branch) || parts.next().is_some() {
                    return Err(DiagramRenderError::InvalidSource);
                }
                commit_count = commit_count.saturating_add(1);
            }
            _ => return Err(DiagramRenderError::InvalidSource),
        }
    }
    let parsed_branches = graph
        .gitgraph
        .branches
        .iter()
        .map(|branch| branch.name.as_str())
        .collect::<std::collections::HashSet<_>>();
    if commit_count == 0
        || commit_count != graph.gitgraph.commits.len()
        || branches.len() != parsed_branches.len()
        || !branches
            .iter()
            .all(|branch| parsed_branches.contains(branch.as_str()))
    {
        return Err(DiagramRenderError::InvalidSource);
    }
    Ok(())
}

fn is_sequence_message_statement(line: &str) -> bool {
    line.contains(':')
        && ["->", "--", "-x", "--x", "-)", "--)"]
            .iter()
            .any(|arrow| line.contains(arrow))
}

fn strip_trailing_mermaid_comment(line: &str) -> &str {
    let mut quote = None;
    let mut escaped = false;
    let chars = line.char_indices().peekable();
    for (index, ch) in chars {
        if escaped {
            escaped = false;
            continue;
        }
        if ch == '\\' {
            escaped = true;
            continue;
        }
        if let Some(open_quote) = quote {
            if ch == open_quote {
                quote = None;
            }
            continue;
        }
        if ch == '"' || ch == '\'' {
            quote = Some(ch);
            continue;
        }
        if ch == '%' && line[index..].starts_with("%%") {
            return &line[..index];
        }
    }
    line
}

fn rasterize_svg(
    svg: &str,
    raster_size: RasterSize,
    counters: Option<&RenderCounters>,
) -> Result<RasterImage, DiagramRenderError> {
    RasterSize::new(raster_size.width, raster_size.height)?;
    if svg.len() > MAX_RENDER_SVG_BYTES {
        return Err(DiagramRenderError::SvgTooLarge);
    }
    let mut options = usvg::Options::default();
    options.resources_dir = None;
    options.fontdb = renderer_font_database();
    options.image_href_resolver = usvg::ImageHrefResolver {
        resolve_data: Box::new(|_, _, _| None),
        resolve_string: Box::new(|_, _| None),
    };
    let tree =
        usvg::Tree::from_str(svg, &options).map_err(|_| DiagramRenderError::RasterizationFailed)?;
    let natural = tree.size();
    validate_svg_dimensions(natural.width(), natural.height())?;

    if let Some(counters) = counters {
        counters
            .raster_count
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
    let mut pixmap = tiny_skia::Pixmap::new(raster_size.width, raster_size.height)
        .ok_or(DiagramRenderError::RasterLimitExceeded)?;
    let scale = (raster_size.width as f32 / natural.width())
        .min(raster_size.height as f32 / natural.height());
    if !scale.is_finite() || scale <= 0.0 {
        return Err(DiagramRenderError::InvalidDimensions);
    }
    resvg::render(
        &tree,
        tiny_skia::Transform::from_scale(scale, scale),
        &mut pixmap.as_mut(),
    );
    let mut rgba = pixmap.take();
    unpremultiply_rgba(&mut rgba);
    Ok(RasterImage {
        width: raster_size.width,
        height: raster_size.height,
        rgba: Arc::from(rgba),
    })
}

pub(crate) fn rasterize_output_svg(
    output: &DiagramRenderOutput,
    raster_size: RasterSize,
) -> Result<RasterImage, DiagramRenderError> {
    rasterize_svg(&output.svg, raster_size, None)
}

fn validate_svg_dimensions(width: f32, height: f32) -> Result<(), DiagramRenderError> {
    if !width.is_finite()
        || !height.is_finite()
        || width <= 0.0
        || height <= 0.0
        || width.ceil() > MAX_RENDER_SIDE as f32
        || height.ceil() > MAX_RENDER_SIDE as f32
        || u64::from(width.ceil() as u32)
            .checked_mul(u64::from(height.ceil() as u32))
            .is_none_or(|pixels| pixels > MAX_RENDER_PIXELS)
    {
        return Err(DiagramRenderError::InvalidDimensions);
    }
    Ok(())
}

fn unpremultiply_rgba(rgba: &mut [u8]) {
    for pixel in rgba.chunks_exact_mut(4) {
        let alpha = u16::from(pixel[3]);
        if alpha == 0 {
            pixel[..3].fill(0);
        } else if alpha < 255 {
            for channel in &mut pixel[..3] {
                *channel = ((u16::from(*channel) * 255 + alpha / 2) / alpha).min(255) as u8;
            }
        }
    }
}

fn renderer_font_database() -> Arc<usvg::fontdb::Database> {
    static FONT_DATABASE: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    FONT_DATABASE
        .get_or_init(|| {
            let mut database = usvg::fontdb::Database::new();
            database.load_system_fonts();
            Arc::new(database)
        })
        .clone()
}

fn preflight_source(source: &str) -> Result<(), DiagramRenderError> {
    if source.len() > MAX_RENDER_SOURCE_BYTES {
        return Err(DiagramRenderError::SourceTooLarge);
    }
    let lines = source.lines().collect::<Vec<_>>();
    if lines.len() > MAX_RENDER_SOURCE_LINES {
        return Err(DiagramRenderError::TooManyLines);
    }
    let mut delimiter_stack = Vec::new();
    let mut subgraph_depth = 0usize;
    let mut mindmap_header_seen = false;
    let mut is_mindmap = false;
    for raw_line in lines {
        let line = raw_line.strip_suffix('\r').unwrap_or(raw_line);
        if line.len() > MAX_RENDER_LINE_BYTES {
            return Err(DiagramRenderError::LineTooLarge);
        }
        if line.trim_start().starts_with("%%") {
            continue;
        }
        let trimmed = line.trim();
        if !mindmap_header_seen && !trimmed.is_empty() {
            mindmap_header_seen = true;
            is_mindmap = trimmed.eq_ignore_ascii_case("mindmap");
        }
        if is_mindmap && !trimmed.is_empty() {
            let indent = line.len().saturating_sub(line.trim_start().len());
            if indent / 2 > MAX_RENDER_NESTING {
                return Err(DiagramRenderError::ExcessiveNesting);
            }
        }
        if trimmed.starts_with("subgraph ") {
            subgraph_depth = subgraph_depth.saturating_add(1);
            if subgraph_depth > MAX_RENDER_NESTING {
                return Err(DiagramRenderError::ExcessiveNesting);
            }
        } else if trimmed == "end" {
            subgraph_depth = subgraph_depth.saturating_sub(1);
        }
        let mut quote = None;
        let mut escaped = false;
        for ch in line.chars() {
            if escaped {
                escaped = false;
                continue;
            }
            if ch == '\\' {
                escaped = true;
                continue;
            }
            if let Some(open_quote) = quote {
                if ch == open_quote {
                    quote = None;
                }
                continue;
            }
            if ch == '"' || ch == '\'' {
                quote = Some(ch);
                continue;
            }
            match ch {
                '[' | '(' | '{' => {
                    delimiter_stack.push(ch);
                    if delimiter_stack.len() > MAX_RENDER_NESTING {
                        return Err(DiagramRenderError::ExcessiveNesting);
                    }
                }
                ']' | ')' | '}' => {
                    if delimiter_stack
                        .last()
                        .is_some_and(|open| delimiters_match(*open, ch))
                    {
                        delimiter_stack.pop();
                    }
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn delimiters_match(open: char, close: char) -> bool {
    matches!((open, close), ('[', ']') | ('(', ')') | ('{', '}'))
}

fn parsed_item_count(graph: &mermaid_rs_renderer::Graph) -> usize {
    [
        graph.nodes.len(),
        graph.node_order.len(),
        graph.edges.len(),
        graph.subgraphs.len(),
        graph.sequence_participants.len(),
        graph.sequence_frames.len(),
        graph.sequence_notes.len(),
        graph.sequence_activations.len(),
        graph.sequence_boxes.len(),
        graph.state_notes.len(),
        graph.pie_slices.len(),
        graph.quadrant.points.len(),
        graph.gantt_tasks.len(),
        graph.gantt_sections.len(),
        graph.gitgraph.commits.len(),
        graph.gitgraph.branches.len(),
        graph.class_defs.len(),
        graph.node_classes.len(),
        graph.node_styles.len(),
        graph.subgraph_styles.len(),
        graph.subgraph_classes.len(),
        graph.node_links.len(),
        graph.edge_styles.len(),
        graph.c4.shapes.len(),
        graph.c4.boundaries.len(),
        graph.c4.rels.len(),
        graph.mindmap.nodes.len(),
        graph.xychart.series.len(),
        graph.xychart.x_axis_categories.len(),
        graph.timeline.events.len(),
        graph.timeline.sections.len(),
        graph.block.as_ref().map_or(0, |block| block.nodes.len()),
        graph.arch_edge_ports.len(),
    ]
    .into_iter()
    .fold(0usize, usize::saturating_add)
}

#[derive(Clone)]
struct RenderArtifact {
    family: DiagramFamily,
    svg: Arc<str>,
    natural_width: f32,
    natural_height: f32,
}

#[derive(Default)]
struct RenderCounters {
    parse_count: AtomicUsize,
    layout_count: AtomicUsize,
    raster_count: AtomicUsize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct RenderCounterSnapshot {
    parses: usize,
    layouts: usize,
    rasters: usize,
}

impl RenderCounters {
    fn snapshot(&self) -> RenderCounterSnapshot {
        RenderCounterSnapshot {
            parses: self.parse_count.load(Ordering::Relaxed),
            layouts: self.layout_count.load(Ordering::Relaxed),
            rasters: self.raster_count.load(Ordering::Relaxed),
        }
    }
}

struct CachedArtifact {
    artifact: Arc<RenderArtifact>,
    rasters: VecDeque<(RasterSize, Arc<RasterImage>)>,
}

#[derive(Default)]
struct RenderCache {
    artifacts: HashMap<RenderKey, CachedArtifact>,
    artifact_lru: VecDeque<RenderKey>,
    raster_lru: VecDeque<(RenderKey, RasterSize)>,
    raster_pixels: u64,
}

impl RenderCache {
    fn render(
        &mut self,
        key: &RenderKey,
        source: &str,
        raster_size: RasterSize,
        counters: &RenderCounters,
    ) -> Result<Arc<DiagramRenderOutput>, DiagramRenderError> {
        RasterSize::new(raster_size.width, raster_size.height)?;
        if let Some(entry) = self.artifacts.get(key) {
            if let Some((_, raster)) = entry
                .rasters
                .iter()
                .find(|(cached_size, _)| *cached_size == raster_size)
            {
                let output = Arc::new(make_render_output(
                    Arc::clone(&entry.artifact),
                    Arc::clone(raster),
                ));
                self.touch_artifact(key);
                return Ok(output);
            }
            let artifact = Arc::clone(&entry.artifact);
            let raster = Arc::new(rasterize_svg(&artifact.svg, raster_size, Some(counters))?);
            self.insert_raster(key, raster_size, Arc::clone(&raster));
            return Ok(Arc::new(make_render_output(artifact, raster)));
        }

        let artifact = build_render_artifact(source, key.options, Some(counters))?;
        let raster = Arc::new(rasterize_svg(&artifact.svg, raster_size, Some(counters))?);
        self.insert_artifact(key.clone(), Arc::clone(&artifact));
        self.insert_raster(key, raster_size, Arc::clone(&raster));
        Ok(Arc::new(make_render_output(artifact, raster)))
    }

    fn touch_artifact(&mut self, key: &RenderKey) {
        if let Some(position) = self.artifact_lru.iter().position(|cached| cached == key) {
            self.artifact_lru.remove(position);
        }
        self.artifact_lru.push_back(key.clone());
    }

    fn insert_artifact(&mut self, key: RenderKey, artifact: Arc<RenderArtifact>) {
        while self.artifacts.len() >= MAX_CACHED_ARTIFACTS {
            let Some(evicted_key) = self.artifact_lru.pop_front() else {
                break;
            };
            if let Some(evicted) = self.artifacts.remove(&evicted_key) {
                for (size, _) in evicted.rasters {
                    self.raster_pixels = self
                        .raster_pixels
                        .saturating_sub(u64::from(size.width) * u64::from(size.height));
                }
                self.raster_lru
                    .retain(|(cached_key, _)| cached_key != &evicted_key);
            }
        }
        self.artifact_lru.push_back(key.clone());
        self.artifacts.insert(
            key,
            CachedArtifact {
                artifact,
                rasters: VecDeque::new(),
            },
        );
    }

    fn insert_raster(&mut self, key: &RenderKey, size: RasterSize, raster: Arc<RasterImage>) {
        let pixels = u64::from(size.width) * u64::from(size.height);
        if pixels > MAX_CACHED_RASTER_PIXELS {
            return;
        }
        while self
            .artifacts
            .get(key)
            .is_some_and(|entry| entry.rasters.len() >= MAX_CACHED_RASTER_VARIANTS_PER_ARTIFACT)
        {
            let Some(position) = self
                .raster_lru
                .iter()
                .position(|(cached_key, _)| cached_key == key)
            else {
                break;
            };
            let (old_key, old_size) = self
                .raster_lru
                .remove(position)
                .expect("located raster cache entry");
            self.remove_raster(&old_key, old_size);
        }
        while self.raster_pixels.saturating_add(pixels) > MAX_CACHED_RASTER_PIXELS {
            let Some((old_key, old_size)) = self.raster_lru.pop_front() else {
                break;
            };
            self.remove_raster(&old_key, old_size);
        }
        if let Some(entry) = self.artifacts.get_mut(key) {
            entry.rasters.push_back((size, raster));
            self.raster_lru.push_back((key.clone(), size));
            self.raster_pixels = self.raster_pixels.saturating_add(pixels);
        }
    }

    fn remove_raster(&mut self, key: &RenderKey, size: RasterSize) {
        if let Some(entry) = self.artifacts.get_mut(key)
            && let Some(position) = entry
                .rasters
                .iter()
                .position(|(cached_size, _)| *cached_size == size)
        {
            entry.rasters.remove(position);
            self.raster_pixels = self
                .raster_pixels
                .saturating_sub(u64::from(size.width) * u64::from(size.height));
        }
    }
}

#[derive(Clone, Debug)]
pub(crate) enum GenericPreviewStatus {
    Idle,
    Pending {
        key: RenderKey,
        raster_size: RasterSize,
    },
    Ready {
        key: RenderKey,
        raster_size: RasterSize,
        output: Arc<DiagramRenderOutput>,
    },
    Failed {
        key: RenderKey,
        raster_size: RasterSize,
        error: DiagramRenderError,
    },
}

impl GenericPreviewStatus {
    fn accepts(&self, completion: &RenderCompletion) -> bool {
        match self {
            Self::Pending { key, raster_size }
            | Self::Ready {
                key, raster_size, ..
            }
            | Self::Failed {
                key, raster_size, ..
            } => key == &completion.key && raster_size == &completion.raster_size,
            Self::Idle => false,
        }
    }

    pub(crate) fn apply(&mut self, completion: RenderCompletion) -> bool {
        if !self.accepts(&completion) {
            return false;
        }
        *self = match completion.result {
            Ok(output) => Self::Ready {
                key: completion.key,
                raster_size: completion.raster_size,
                output,
            },
            Err(error) => Self::Failed {
                key: completion.key,
                raster_size: completion.raster_size,
                error,
            },
        };
        true
    }
}

pub(crate) struct RenderCompletion {
    pub(crate) key: RenderKey,
    pub(crate) raster_size: RasterSize,
    pub(crate) result: Result<Arc<DiagramRenderOutput>, DiagramRenderError>,
}

pub(crate) type RepaintCallback = Arc<dyn Fn() + Send + Sync>;

struct WorkerRequest {
    key: RenderKey,
    source: Arc<str>,
    raster_size: RasterSize,
    not_before: Instant,
    wake: RepaintCallback,
}

struct ActiveWork {
    key: RenderKey,
    raster_size: RasterSize,
    retired: bool,
}

#[derive(Default)]
struct WorkerState {
    pending: Option<WorkerRequest>,
    active: Option<ActiveWork>,
    completion: Option<RenderCompletion>,
    shutdown: bool,
}

impl WorkerState {
    fn enqueue_latest(&mut self, request: WorkerRequest) {
        self.pending = Some(request);
        self.completion = None;
    }

    fn retire(&mut self, document: &RenderDocumentIdentity) {
        if self
            .pending
            .as_ref()
            .is_some_and(|request| &request.key.document == document)
        {
            self.pending = None;
        }
        if let Some(active) = self.active.as_mut()
            && &active.key.document == document
        {
            active.retired = true;
        }
        if self
            .completion
            .as_ref()
            .is_some_and(|completion| &completion.key.document == document)
        {
            self.completion = None;
        }
    }
}

#[derive(Default)]
struct WorkerShared {
    state: Mutex<WorkerState>,
    changed: Condvar,
    counters: RenderCounters,
}

#[derive(Default)]
pub(crate) struct RendererService {
    shared: Arc<WorkerShared>,
    start_state: Mutex<Option<bool>>,
}

static RENDERER_PANIC_HOOK: OnceLock<()> = OnceLock::new();

impl RendererService {
    fn start(&self) -> Result<(), DiagramRenderError> {
        let mut start_state = self
            .start_state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(started) = *start_state {
            return started
                .then_some(())
                .ok_or(DiagramRenderError::WorkerUnavailable);
        }

        RENDERER_PANIC_HOOK.get_or_init(install_renderer_panic_hook);
        let shared = Arc::clone(&self.shared);
        let started = thread::Builder::new()
            .name("diagram-mermaid-renderer".to_owned())
            .spawn(move || renderer_worker(shared))
            .is_ok();
        *start_state = Some(started);
        started
            .then_some(())
            .ok_or(DiagramRenderError::WorkerUnavailable)
    }

    pub(crate) fn submit(
        &self,
        key: RenderKey,
        source: Arc<str>,
        raster_size: RasterSize,
        debounced: bool,
        wake: RepaintCallback,
    ) -> Result<(), DiagramRenderError> {
        RasterSize::new(raster_size.width, raster_size.height)?;
        self.start()?;
        let now = Instant::now();
        let request = WorkerRequest {
            key,
            source,
            raster_size,
            not_before: if debounced {
                now + SOURCE_EDIT_DEBOUNCE
            } else {
                now
            },
            wake,
        };
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if state.active.as_ref().is_some_and(|active| {
            active.key == request.key && active.raster_size == request.raster_size
        }) {
            return Ok(());
        }
        if state.pending.as_ref().is_some_and(|pending| {
            pending.key == request.key
                && pending.raster_size == request.raster_size
                && pending.source == request.source
        }) {
            return Ok(());
        }
        state.enqueue_latest(request);
        self.shared.changed.notify_one();
        Ok(())
    }

    pub(crate) fn retire(&self, document: &RenderDocumentIdentity) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.retire(document);
    }

    pub(crate) fn take_completion(&self) -> Option<RenderCompletion> {
        self.shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .completion
            .take()
    }
}

impl Drop for RendererService {
    fn drop(&mut self) {
        let mut state = self
            .shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.shutdown = true;
        state.pending = None;
        state.completion = None;
        if let Some(active) = state.active.as_mut() {
            active.retired = true;
        }
        self.shared.changed.notify_one();
    }
}

fn renderer_worker(shared: Arc<WorkerShared>) {
    let mut cache = RenderCache::default();
    loop {
        let request = {
            let mut state = shared
                .state
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            loop {
                if state.shutdown {
                    return;
                }
                let Some(request) = state.pending.as_ref() else {
                    state = shared
                        .changed
                        .wait(state)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    continue;
                };
                let delay = request.not_before.saturating_duration_since(Instant::now());
                if !delay.is_zero() {
                    let (next_state, _) = shared
                        .changed
                        .wait_timeout(state, delay)
                        .unwrap_or_else(std::sync::PoisonError::into_inner);
                    state = next_state;
                    continue;
                }
                let request = state.pending.take().expect("pending request was observed");
                state.active = Some(ActiveWork {
                    key: request.key.clone(),
                    raster_size: request.raster_size,
                    retired: false,
                });
                break request;
            }
        };

        let rendered = catch_unwind(AssertUnwindSafe(|| {
            cache.render(
                &request.key,
                &request.source,
                request.raster_size,
                &shared.counters,
            )
        }));
        let result = match rendered {
            Ok(result) => result,
            Err(_) => {
                cache = RenderCache::default();
                Err(DiagramRenderError::WorkerPanicked)
            }
        };
        let mut state = shared
            .state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let publish = state.active.as_ref().is_some_and(|active| !active.retired);
        state.active = None;
        if publish {
            state.completion = Some(RenderCompletion {
                key: request.key,
                raster_size: request.raster_size,
                result,
            });
        }
        let should_stop = state.shutdown;
        drop(state);
        if publish {
            (request.wake)();
        }
        if should_stop {
            return;
        }
    }
}

fn install_renderer_panic_hook() {
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic| {
        if thread::current().name() != Some("diagram-mermaid-renderer") {
            previous_hook(panic);
        }
    }));
}

#[cfg(test)]
mod tests {
    use super::*;

    const FLOWCHART: &str = "flowchart LR\nIntake[\"Customer intake\"] --> Review{\"Review request\"}\nReview -->|Approve| Publish[\"Publish service\"]\nReview -->|Revise| Intake";
    const MINDMAP: &str = "mindmap\n  root((Release plan))\n    Design\n      Research\n    Delivery\n      Ship safely";
    const SEQUENCE: &str = "sequenceDiagram\nparticipant User\nparticipant Studio\nUser->>Studio: Submit diagram\nStudio-->>User: Preview ready";

    #[test]
    fn generic_preview_candidates_include_only_the_source_rendered_families() {
        assert!(!is_generic_preview_candidate(FLOWCHART));
        for source in [
            MINDMAP,
            SEQUENCE,
            "classDiagram\nclass Example",
            "stateDiagram-v2\n[*] --> Ready",
            "erDiagram\nA ||--o{ B : links",
            "gantt\nTask : 1d",
            "timeline\n2026 : Event",
            "journey\nTask: 4: User",
            "pie\n\"A\" : 1",
            "gitGraph\ncommit",
        ] {
            assert!(is_generic_preview_candidate(source), "{source}");
        }
        assert!(!is_generic_preview_candidate("flowchart TD\nA --> B"));
        assert!(!is_generic_preview_candidate(
            "classDiagramExtra\nclass Example"
        ));
    }

    #[test]
    fn production_adapter_renders_flowchart_mindmap_and_sequence_to_rgba() {
        let target = RasterSize::new(640, 420).expect("valid raster size");
        let fixtures = [
            (
                "flowchart",
                FLOWCHART,
                DiagramFamily::Flowchart,
                "Customer intake",
            ),
            ("mindmap", MINDMAP, DiagramFamily::Mindmap, "Release plan"),
            (
                "sequence",
                SEQUENCE,
                DiagramFamily::Sequence,
                "Submit diagram",
            ),
        ];
        let evidence_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("target")
            .join("studio-launcher-usability");
        std::fs::create_dir_all(&evidence_dir).expect("create ignored render evidence directory");

        for (name, source, family, label) in fixtures {
            let output = render_mermaid_preview(source, target).expect("render production fixture");
            assert_eq!(output.family, family);
            assert!(
                output.svg.contains(label),
                "missing readable label in {name} SVG"
            );
            assert!(output.natural_width.is_finite() && output.natural_width > 0.0);
            assert!(output.natural_height.is_finite() && output.natural_height > 0.0);
            assert_eq!((output.raster.width, output.raster.height), (640, 420));
            assert_eq!(output.raster.rgba.len(), 640 * 420 * 4);
            let visible_pixels = output
                .raster
                .rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0)
                .count();
            let dark_pixels = output
                .raster
                .rgba
                .chunks_exact(4)
                .filter(|pixel| pixel[3] > 0 && pixel[0] < 100 && pixel[1] < 100 && pixel[2] < 100)
                .count();
            assert!(
                visible_pixels > 500,
                "no meaningful output pixels in {name}"
            );
            assert!(dark_pixels > 100, "no readable foreground pixels in {name}");

            let rgba = image::RgbaImage::from_raw(
                output.raster.width,
                output.raster.height,
                output.raster.rgba.to_vec(),
            )
            .expect("RGBA buffer dimensions");
            rgba.save(evidence_dir.join(format!("mermaid-{name}.png")))
                .expect("write ignored visual evidence");
        }
    }

    #[test]
    fn production_adapter_rejects_invalid_oversized_nested_and_huge_raster_inputs() {
        let target = RasterSize::new(320, 240).expect("valid raster size");
        assert_eq!(
            render_mermaid_preview("flowchart TD\nA -->", target).unwrap_err(),
            DiagramRenderError::InvalidSource
        );
        assert_eq!(
            render_mermaid_preview(&"x".repeat(MAX_RENDER_SOURCE_BYTES + 1), target).unwrap_err(),
            DiagramRenderError::SourceTooLarge
        );
        let nested = format!("flowchart TD\nA[{}label{}]", "[".repeat(65), "]".repeat(65));
        assert_eq!(
            render_mermaid_preview(&nested, target).unwrap_err(),
            DiagramRenderError::ExcessiveNesting
        );
        assert_eq!(
            RasterSize::new(4_097, 1).unwrap_err(),
            DiagramRenderError::RasterLimitExceeded
        );
        assert!(RasterSize::new(4_096, 4_096).is_ok());
        assert_eq!(
            RasterSize::new(u32::MAX, u32::MAX).unwrap_err(),
            DiagramRenderError::RasterLimitExceeded
        );
        assert_eq!(
            render_mermaid_preview(
                &format!("{FLOWCHART}\nthis line is silently ignored by the upstream parser"),
                target
            )
            .unwrap_err(),
            DiagramRenderError::InvalidSource
        );
        assert_eq!(
            render_mermaid_preview(
                &format!("{SEQUENCE}\nthis sequence statement is unsupported"),
                target
            )
            .unwrap_err(),
            DiagramRenderError::InvalidSource
        );
        assert_eq!(
            render_mermaid_preview("classDiagram\nclass Example\nignored statement", target)
                .unwrap_err(),
            DiagramRenderError::InvalidSource
        );
        for source in [
            "pie\n%%{init: {\"theme\": \"dark\"}}\n\"A\" : 1",
            "pie\n---\ntitle: ignored frontmatter\n---\n\"A\" : 1",
        ] {
            assert_eq!(
                render_mermaid_preview(source, target).unwrap_err(),
                DiagramRenderError::InvalidSource,
                "unsupported directives and frontmatter are never admitted"
            );
        }
    }

    #[test]
    fn cache_skips_parse_layout_for_identical_and_raster_only_requests() {
        let counters = RenderCounters::default();
        let mut cache = RenderCache::default();
        let document = RenderDocumentIdentity::fresh();
        let key = RenderKey::new(document.clone(), 7, RenderOptions::default());
        let first_size = RasterSize::new(640, 420).expect("first raster size");
        let second_size = RasterSize::new(320, 240).expect("second raster size");

        let first = cache
            .render(&key, FLOWCHART, first_size, &counters)
            .expect("render first request");
        assert_eq!(first.family, DiagramFamily::Flowchart);
        assert_eq!(
            counters.snapshot(),
            RenderCounterSnapshot {
                parses: 1,
                layouts: 1,
                rasters: 1,
            }
        );

        let identical = cache
            .render(&key, FLOWCHART, first_size, &counters)
            .expect("reuse identical request");
        assert!(Arc::ptr_eq(&first.raster, &identical.raster));
        assert_eq!(
            counters.snapshot(),
            RenderCounterSnapshot {
                parses: 1,
                layouts: 1,
                rasters: 1,
            }
        );

        cache
            .render(&key, FLOWCHART, second_size, &counters)
            .expect("rasterize cached SVG at another size");
        assert_eq!(
            counters.snapshot(),
            RenderCounterSnapshot {
                parses: 1,
                layouts: 1,
                rasters: 2,
            }
        );

        let other_document = RenderKey::new(
            RenderDocumentIdentity::fresh(),
            key.accepted_revision,
            key.options,
        );
        cache
            .render(&other_document, FLOWCHART, first_size, &counters)
            .expect("same revision in another document must be distinct");
        let other_options = RenderKey::new(
            document,
            key.accepted_revision,
            RenderOptions {
                theme: RenderTheme::MermaidDefault,
            },
        );
        cache
            .render(&other_options, FLOWCHART, first_size, &counters)
            .expect("theme change must be distinct");
        assert_eq!(
            counters.snapshot(),
            RenderCounterSnapshot {
                parses: 3,
                layouts: 3,
                rasters: 4,
            }
        );
    }

    #[test]
    fn worker_admission_replaces_pending_and_retires_active_cooperatively() {
        let size = RasterSize::new(320, 240).expect("valid raster size");
        let first_document = RenderDocumentIdentity::fresh();
        let first_key = RenderKey::new(first_document.clone(), 1, RenderOptions::default());
        let next_key = RenderKey::new(RenderDocumentIdentity::fresh(), 1, RenderOptions::default());
        let latest_key =
            RenderKey::new(RenderDocumentIdentity::fresh(), 2, RenderOptions::default());
        let now = Instant::now();
        let mut state = WorkerState::default();
        state.active = Some(ActiveWork {
            key: first_key.clone(),
            raster_size: size,
            retired: false,
        });
        state.enqueue_latest(test_request(next_key, "second", size, now));
        state.enqueue_latest(test_request(latest_key.clone(), "latest", size, now));

        assert_eq!(
            state.pending.as_ref().map(|request| request.key.clone()),
            Some(latest_key)
        );
        assert_eq!(
            state
                .pending
                .as_ref()
                .map(|request| request.source.as_ref()),
            Some("latest")
        );
        assert!(!state.active.as_ref().expect("active work").retired);

        state.retire(&first_document);
        assert!(
            state
                .active
                .as_ref()
                .expect("active remains in flight")
                .retired
        );
        assert_eq!(
            state
                .pending
                .as_ref()
                .map(|request| request.source.as_ref()),
            Some("latest")
        );
    }

    #[test]
    fn publication_rejects_stale_revision_document_and_options() {
        let size = RasterSize::new(320, 240).expect("valid raster size");
        let document = RenderDocumentIdentity::fresh();
        let base = RenderKey::new(document.clone(), 4, RenderOptions::default());
        let mut publication;
        let stale = || RenderCompletion {
            key: base.clone(),
            raster_size: size,
            result: Err(DiagramRenderError::InvalidSource),
        };

        publication = GenericPreviewStatus::Pending {
            key: RenderKey::new(document.clone(), 5, RenderOptions::default()),
            raster_size: size,
        };
        assert!(!publication.apply(stale()));
        publication = GenericPreviewStatus::Pending {
            key: RenderKey::new(RenderDocumentIdentity::fresh(), 4, RenderOptions::default()),
            raster_size: size,
        };
        assert!(!publication.apply(stale()));
        publication = GenericPreviewStatus::Pending {
            key: RenderKey::new(
                document,
                4,
                RenderOptions {
                    theme: RenderTheme::MermaidDefault,
                },
            ),
            raster_size: size,
        };
        assert!(!publication.apply(stale()));
        publication = GenericPreviewStatus::Pending {
            key: base.clone(),
            raster_size: RasterSize::new(640, 480).expect("different raster size"),
        };
        assert!(!publication.apply(stale()));
    }

    fn test_request(key: RenderKey, source: &str, size: RasterSize, now: Instant) -> WorkerRequest {
        WorkerRequest {
            key,
            source: Arc::from(source),
            raster_size: size,
            not_before: now,
            wake: Arc::new(|| {}),
        }
    }
}
