//! Bounded, source-faithful syntax projection for the Rust regex parser AST.
//!
//! This is a structural view of accepted regex syntax, not an execution trace.

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::num::NonZeroU64;
use std::ops::Range;
use std::sync::Arc;

use regex_syntax::ast::{self, Ast, ClassSet, ClassSetItem, GroupKind};

use super::explanation::{Explanation, SyntaxAnalysis, analyze_syntax};
use super::model::{ByteSpan, RegexFlags, RegexValidationError};
use super::policy::MAX_PATTERN_BYTES;

pub const MAX_SYNTAX_NODES: usize = 1024;
pub const MAX_SYNTAX_DEPTH: usize = 128;
const MAX_DISPLAY_LABEL_BYTES: usize = 72;

/// One source-level construct represented in the regex AST.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RegexSyntaxKind {
    Empty,
    Flags,
    ScopedFlags,
    Literal,
    Dot,
    Assertion,
    PerlClass,
    UnicodeClass,
    AsciiClass,
    CharacterClass,
    CharacterRange,
    ClassUnion,
    ClassSetOperation,
    CapturingGroup,
    NamedGroup,
    NonCapturingGroup,
    Alternation,
    Concatenation,
    Repetition,
}

impl RegexSyntaxKind {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Empty => "Empty expression",
            Self::Flags => "Flags",
            Self::ScopedFlags => "Scoped flags",
            Self::Literal => "Literal",
            Self::Dot => "Dot",
            Self::Assertion => "Assertion",
            Self::PerlClass => "Character class",
            Self::UnicodeClass => "Unicode class",
            Self::AsciiClass => "ASCII class",
            Self::CharacterClass => "Character set",
            Self::CharacterRange => "Character range",
            Self::ClassUnion => "Character-set union",
            Self::ClassSetOperation => "Character-set operation",
            Self::CapturingGroup => "Capturing group",
            Self::NamedGroup => "Named group",
            Self::NonCapturingGroup => "Non-capturing group",
            Self::Alternation => "Alternation",
            Self::Concatenation => "Concatenation",
            Self::Repetition => "Repetition",
        }
    }
}

/// Opaque identity for a node in one accepted syntax projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct RegexSyntaxNodeId(NonZeroU64);

impl RegexSyntaxNodeId {
    pub const fn get(self) -> u64 {
        self.0.get()
    }
}

/// A node identity bound to one projection lineage and syntax generation.
#[derive(Debug, Clone)]
pub struct RegexSyntaxNodeRef {
    id: RegexSyntaxNodeId,
    generation: u64,
    lineage: Arc<ProjectionIdentity>,
}

impl PartialEq for RegexSyntaxNodeRef {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
            && self.generation == other.generation
            && Arc::ptr_eq(&self.lineage, &other.lineage)
    }
}

impl Eq for RegexSyntaxNodeRef {}

impl Hash for RegexSyntaxNodeRef {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
        self.generation.hash(state);
        (Arc::as_ptr(&self.lineage) as usize).hash(state);
    }
}

impl RegexSyntaxNodeRef {
    pub const fn id(&self) -> RegexSyntaxNodeId {
        self.id
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }
}

/// Immutable AST node with both its full structural range and its focused
/// source range. Explanations are referenced by index in the accepted
/// `ExplanationResult` from the same generation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexSyntaxNode {
    id: RegexSyntaxNodeId,
    kind: RegexSyntaxKind,
    span: ByteSpan,
    focus_span: ByteSpan,
    parent: Option<RegexSyntaxNodeId>,
    children: Vec<RegexSyntaxNodeId>,
    explanation_indices: Vec<usize>,
    display_label: String,
    depth: usize,
}

impl RegexSyntaxNode {
    pub const fn id(&self) -> RegexSyntaxNodeId {
        self.id
    }

    pub const fn kind(&self) -> RegexSyntaxKind {
        self.kind
    }

    pub const fn span(&self) -> ByteSpan {
        self.span
    }

    pub const fn focus_span(&self) -> ByteSpan {
        self.focus_span
    }

    pub const fn parent(&self) -> Option<RegexSyntaxNodeId> {
        self.parent
    }

    pub fn children(&self) -> &[RegexSyntaxNodeId] {
        &self.children
    }

    pub fn explanation_indices(&self) -> &[usize] {
        &self.explanation_indices
    }

    pub fn display_label(&self) -> &str {
        &self.display_label
    }

    pub const fn depth(&self) -> usize {
        self.depth
    }
}

/// Complete projection of one accepted pattern and flag set.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexSyntaxTree {
    source: Arc<str>,
    flags: RegexFlags,
    generation: u64,
    lineage: Arc<ProjectionIdentity>,
    nodes: Vec<RegexSyntaxNode>,
    roots: Vec<RegexSyntaxNodeId>,
    by_id: HashMap<RegexSyntaxNodeId, usize>,
}

#[derive(Debug, PartialEq, Eq)]
struct ProjectionIdentity;

impl RegexSyntaxTree {
    pub fn source(&self) -> &str {
        &self.source
    }

    pub const fn flags(&self) -> RegexFlags {
        self.flags
    }

    pub const fn generation(&self) -> u64 {
        self.generation
    }

    pub fn nodes(&self) -> &[RegexSyntaxNode] {
        &self.nodes
    }

    pub fn roots(&self) -> &[RegexSyntaxNodeId] {
        &self.roots
    }

    pub fn node(&self, id: RegexSyntaxNodeId) -> Option<&RegexSyntaxNode> {
        self.by_id.get(&id).and_then(|index| self.nodes.get(*index))
    }

    pub fn node_ref(&self, id: RegexSyntaxNodeId) -> Option<RegexSyntaxNodeRef> {
        self.node(id).map(|_| RegexSyntaxNodeRef {
            id,
            generation: self.generation,
            lineage: self.lineage.clone(),
        })
    }

    /// Resolve only references created for this tree generation. A retained
    /// ID from an older edit cannot select a coincidentally reused node.
    pub fn resolve(&self, reference: &RegexSyntaxNodeRef) -> Option<&RegexSyntaxNode> {
        (reference.generation == self.generation && Arc::ptr_eq(&reference.lineage, &self.lineage))
            .then(|| self.node(reference.id))
            .flatten()
    }

    pub fn source_for(&self, reference: &RegexSyntaxNodeRef) -> Option<&str> {
        let node = self.resolve(reference)?;
        self.source.get(node.span.as_range())
    }

    pub fn focus_source_for(&self, reference: &RegexSyntaxNodeRef) -> Option<&str> {
        let node = self.resolve(reference)?;
        self.source.get(node.focus_span.as_range())
    }

    pub fn explanations_for<'a>(
        &self,
        reference: &RegexSyntaxNodeRef,
        explanations: &'a [Explanation],
    ) -> Option<Vec<&'a Explanation>> {
        let node = self.resolve(reference)?;
        node.explanation_indices
            .iter()
            .map(|index| explanations.get(*index))
            .collect()
    }

    /// Finds the smallest focused syntax range containing a UTF-8 caret. If
    /// no focus range contains it, structural ranges provide a deliberate
    /// fallback for ignored x-mode whitespace/comments and group punctuation.
    /// Zero-width focus ranges match only at their exact byte offset.
    pub fn best_node_at_caret(&self, byte_offset: usize) -> Option<RegexSyntaxNodeRef> {
        if byte_offset > self.source.len() || !self.source.is_char_boundary(byte_offset) {
            return None;
        }
        self.best_node_for_query(true, |span| caret_contains(span, byte_offset))
            .or_else(|| {
                self.best_node_for_query(false, |span| {
                    if span.is_empty() {
                        span.start_byte() == byte_offset
                    } else {
                        span.start_byte() <= byte_offset && byte_offset <= span.end_byte()
                    }
                })
            })
    }

    /// Finds the smallest focused syntax range containing a UTF-8 selection,
    /// then falls back to structural containment. Empty selections use caret
    /// semantics; malformed or non-boundary ranges are rejected.
    pub fn best_node_for_selection(&self, selection: Range<usize>) -> Option<RegexSyntaxNodeRef> {
        if selection.start > selection.end
            || selection.end > self.source.len()
            || !self.source.is_char_boundary(selection.start)
            || !self.source.is_char_boundary(selection.end)
        {
            return None;
        }
        if selection.is_empty() {
            return self.best_node_at_caret(selection.start);
        }
        self.best_node_for_query(true, |span| {
            span.start_byte() <= selection.start && selection.end <= span.end_byte()
        })
        .or_else(|| {
            self.best_node_for_query(false, |span| {
                span.start_byte() <= selection.start && selection.end <= span.end_byte()
            })
        })
    }

    fn best_node_for_query(
        &self,
        focused_span: bool,
        mut contains: impl FnMut(ByteSpan) -> bool,
    ) -> Option<RegexSyntaxNodeRef> {
        let queried_span = |node: &RegexSyntaxNode| {
            if focused_span {
                node.focus_span
            } else {
                node.span
            }
        };
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| contains(queried_span(node)))
            .min_by(|(left_order, left), (right_order, right)| {
                queried_span(left)
                    .len_bytes()
                    .cmp(&queried_span(right).len_bytes())
                    .then_with(|| right.depth.cmp(&left.depth))
                    .then_with(|| left.span.len_bytes().cmp(&right.span.len_bytes()))
                    .then_with(|| left_order.cmp(right_order))
            })
            .map(|(_, node)| RegexSyntaxNodeRef {
                id: node.id,
                generation: self.generation,
                lineage: self.lineage.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VisualizationUnavailable {
    InvalidPattern(RegexValidationError),
    PatternLimit { limit: usize },
    NodeLimit { limit: usize },
    DepthLimit { limit: usize },
    InvalidAstSpan,
    IdentitySpaceExhausted,
}

/// A projection is either complete or explicitly unavailable. Complexity
/// limits never expose a partial tree as though it were complete.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegexSyntaxVisualizationResult {
    Available(RegexSyntaxTree),
    Unavailable(VisualizationUnavailable),
}

impl RegexSyntaxVisualizationResult {
    pub fn tree(&self) -> Option<&RegexSyntaxTree> {
        match self {
            Self::Available(tree) => Some(tree),
            Self::Unavailable(_) => None,
        }
    }

    pub fn unavailable_reason(&self) -> Option<&VisualizationUnavailable> {
        match self {
            Self::Available(_) => None,
            Self::Unavailable(reason) => Some(reason),
        }
    }
}

/// Parse and project one pattern using the same compiler/parser boundary as
/// explanations. This standalone convenience has no cross-edit ID reuse.
pub fn visualize(pattern: &str, flags: &RegexFlags) -> RegexSyntaxVisualizationResult {
    if pattern.len() > MAX_PATTERN_BYTES {
        return RegexSyntaxVisualizationResult::Unavailable(
            VisualizationUnavailable::PatternLimit {
                limit: MAX_PATTERN_BYTES,
            },
        );
    }
    let analysis = match analyze_syntax(pattern, flags) {
        Ok(analysis) => analysis,
        Err(error) => {
            return RegexSyntaxVisualizationResult::Unavailable(
                VisualizationUnavailable::InvalidPattern(error),
            );
        }
    };
    let mut next_id = 1;
    project_analysis(pattern, *flags, &analysis, None, 1, &mut next_id)
}

pub(crate) fn project_analysis(
    pattern: &str,
    flags: RegexFlags,
    analysis: &SyntaxAnalysis,
    previous: Option<&RegexSyntaxTree>,
    generation: u64,
    next_id: &mut u64,
) -> RegexSyntaxVisualizationResult {
    let mut builder = ProjectionBuilder {
        source: pattern,
        drafts: Vec::new(),
    };
    if let Err(error) = builder.visit_ast(&analysis.ast, None, 0) {
        return RegexSyntaxVisualizationResult::Unavailable(error);
    }
    attach_explanations(&mut builder.drafts, &analysis.explanations);
    let source: Arc<str> = Arc::from(pattern);
    let preserved = previous
        .filter(|tree| tree.flags == flags && tree.source.as_ref() != pattern)
        .map(|tree| reconcile_ids(tree, pattern, &builder.drafts))
        .unwrap_or_else(|| vec![None; builder.drafts.len()]);
    let mut ids = Vec::with_capacity(builder.drafts.len());
    for preserved_id in preserved {
        if let Some(id) = preserved_id {
            ids.push(id);
        } else {
            let Some(raw) = NonZeroU64::new(*next_id) else {
                return RegexSyntaxVisualizationResult::Unavailable(
                    VisualizationUnavailable::IdentitySpaceExhausted,
                );
            };
            let Some(next) = next_id.checked_add(1) else {
                return RegexSyntaxVisualizationResult::Unavailable(
                    VisualizationUnavailable::IdentitySpaceExhausted,
                );
            };
            *next_id = next;
            ids.push(RegexSyntaxNodeId(raw));
        }
    }

    let mut nodes = Vec::with_capacity(builder.drafts.len());
    let mut by_id = HashMap::with_capacity(builder.drafts.len());
    for (index, draft) in builder.drafts.into_iter().enumerate() {
        let id = ids[index];
        let parent = draft.parent.map(|parent| ids[parent]);
        let children = draft.children.iter().map(|child| ids[*child]).collect();
        by_id.insert(id, index);
        nodes.push(RegexSyntaxNode {
            id,
            kind: draft.kind,
            span: draft.span,
            focus_span: draft.focus_span,
            parent,
            children,
            explanation_indices: draft.explanation_indices,
            display_label: display_label(pattern, draft.kind, draft.span, draft.focus_span),
            depth: draft.depth,
        });
    }
    let roots = nodes
        .iter()
        .filter(|node| node.parent.is_none())
        .map(|node| node.id)
        .collect();
    RegexSyntaxVisualizationResult::Available(RegexSyntaxTree {
        source,
        flags,
        generation,
        lineage: Arc::new(ProjectionIdentity),
        nodes,
        roots,
        by_id,
    })
}

#[derive(Debug)]
struct NodeDraft {
    kind: RegexSyntaxKind,
    span: ByteSpan,
    focus_span: ByteSpan,
    parent: Option<usize>,
    children: Vec<usize>,
    explanation_indices: Vec<usize>,
    depth: usize,
}

struct ProjectionBuilder<'a> {
    source: &'a str,
    drafts: Vec<NodeDraft>,
}

impl ProjectionBuilder<'_> {
    fn visit_ast(
        &mut self,
        ast: &Ast,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<usize, VisualizationUnavailable> {
        if depth > MAX_SYNTAX_DEPTH {
            return Err(VisualizationUnavailable::DepthLimit {
                limit: MAX_SYNTAX_DEPTH,
            });
        }
        let span = span_from_ast(self.source, ast.span())?;
        let (kind, focus_span) = match ast {
            Ast::Empty(_) => (RegexSyntaxKind::Empty, span),
            Ast::Flags(flags) => (
                RegexSyntaxKind::Flags,
                span_from_ast(self.source, &flags.span)?,
            ),
            Ast::Literal(literal) => (
                RegexSyntaxKind::Literal,
                span_from_ast(self.source, &literal.span)?,
            ),
            Ast::Dot(dot) => (RegexSyntaxKind::Dot, span_from_ast(self.source, dot)?),
            Ast::Assertion(assertion) => (
                RegexSyntaxKind::Assertion,
                span_from_ast(self.source, &assertion.span)?,
            ),
            Ast::ClassUnicode(class) => (
                RegexSyntaxKind::UnicodeClass,
                span_from_ast(self.source, &class.span)?,
            ),
            Ast::ClassPerl(class) => (
                RegexSyntaxKind::PerlClass,
                span_from_ast(self.source, &class.span)?,
            ),
            Ast::ClassBracketed(class) => (
                RegexSyntaxKind::CharacterClass,
                span_from_ast(self.source, &class.span)?,
            ),
            Ast::Repetition(repetition) => (
                RegexSyntaxKind::Repetition,
                span_from_ast(self.source, &repetition.op.span)?,
            ),
            Ast::Group(group) => (
                match &group.kind {
                    GroupKind::CaptureIndex(_) => RegexSyntaxKind::CapturingGroup,
                    GroupKind::CaptureName { .. } => RegexSyntaxKind::NamedGroup,
                    GroupKind::NonCapturing(_) => RegexSyntaxKind::NonCapturingGroup,
                },
                span,
            ),
            Ast::Alternation(alternation) => (
                RegexSyntaxKind::Alternation,
                span_from_ast(self.source, &alternation.span)?,
            ),
            Ast::Concat(concat) => (
                RegexSyntaxKind::Concatenation,
                span_from_ast(self.source, &concat.span)?,
            ),
        };
        let index = self.push(kind, span, focus_span, parent, depth)?;
        match ast {
            Ast::Repetition(repetition) => {
                let child = self.visit_ast(&repetition.ast, Some(index), depth + 1)?;
                self.drafts[index].children.push(child);
            }
            Ast::Group(group) => {
                if let GroupKind::NonCapturing(flags) = &group.kind
                    && !flags.items.is_empty()
                {
                    let flag_span = span_from_ast(self.source, &flags.span)?;
                    let flag_node = self.push(
                        RegexSyntaxKind::ScopedFlags,
                        flag_span,
                        flag_span,
                        Some(index),
                        depth + 1,
                    )?;
                    self.drafts[index].children.push(flag_node);
                }
                let child = self.visit_ast(&group.ast, Some(index), depth + 1)?;
                self.drafts[index].children.push(child);
            }
            Ast::Alternation(alternation) => {
                for alternative in &alternation.asts {
                    let child = self.visit_ast(alternative, Some(index), depth + 1)?;
                    self.drafts[index].children.push(child);
                }
            }
            Ast::Concat(concat) => {
                for component in &concat.asts {
                    let child = self.visit_ast(component, Some(index), depth + 1)?;
                    self.drafts[index].children.push(child);
                }
            }
            Ast::ClassBracketed(class) => {
                let child = self.visit_class_set(&class.kind, Some(index), depth + 1)?;
                self.drafts[index].children.push(child);
            }
            _ => {}
        }
        Ok(index)
    }

    fn visit_class_set(
        &mut self,
        set: &ClassSet,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<usize, VisualizationUnavailable> {
        match set {
            ClassSet::Item(item) => self.visit_class_item(item, parent, depth),
            ClassSet::BinaryOp(operation) => {
                if depth > MAX_SYNTAX_DEPTH {
                    return Err(VisualizationUnavailable::DepthLimit {
                        limit: MAX_SYNTAX_DEPTH,
                    });
                }
                let span = span_from_ast(self.source, &operation.span)?;
                let index = self.push(
                    RegexSyntaxKind::ClassSetOperation,
                    span,
                    span,
                    parent,
                    depth,
                )?;
                let lhs = self.visit_class_set(&operation.lhs, Some(index), depth + 1)?;
                let rhs = self.visit_class_set(&operation.rhs, Some(index), depth + 1)?;
                self.drafts[index].children.extend([lhs, rhs]);
                Ok(index)
            }
        }
    }

    fn visit_class_item(
        &mut self,
        item: &ClassSetItem,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<usize, VisualizationUnavailable> {
        if depth > MAX_SYNTAX_DEPTH {
            return Err(VisualizationUnavailable::DepthLimit {
                limit: MAX_SYNTAX_DEPTH,
            });
        }
        match item {
            ClassSetItem::Empty(empty) => {
                let span = span_from_ast(self.source, empty)?;
                self.push(RegexSyntaxKind::Empty, span, span, parent, depth)
            }
            ClassSetItem::Literal(literal) => {
                let span = span_from_ast(self.source, &literal.span)?;
                self.push(RegexSyntaxKind::Literal, span, span, parent, depth)
            }
            ClassSetItem::Range(range) => {
                let span = span_from_ast(self.source, &range.span)?;
                self.push(RegexSyntaxKind::CharacterRange, span, span, parent, depth)
            }
            ClassSetItem::Ascii(class) => {
                let span = span_from_ast(self.source, &class.span)?;
                self.push(RegexSyntaxKind::AsciiClass, span, span, parent, depth)
            }
            ClassSetItem::Unicode(class) => {
                let span = span_from_ast(self.source, &class.span)?;
                self.push(RegexSyntaxKind::UnicodeClass, span, span, parent, depth)
            }
            ClassSetItem::Perl(class) => {
                let span = span_from_ast(self.source, &class.span)?;
                self.push(RegexSyntaxKind::PerlClass, span, span, parent, depth)
            }
            ClassSetItem::Bracketed(class) => {
                let span = span_from_ast(self.source, &class.span)?;
                let index =
                    self.push(RegexSyntaxKind::CharacterClass, span, span, parent, depth)?;
                let child = self.visit_class_set(&class.kind, Some(index), depth + 1)?;
                self.drafts[index].children.push(child);
                Ok(index)
            }
            ClassSetItem::Union(union) => {
                let span = span_from_ast(self.source, &union.span)?;
                let index = self.push(RegexSyntaxKind::ClassUnion, span, span, parent, depth)?;
                for item in &union.items {
                    let child = self.visit_class_item(item, Some(index), depth + 1)?;
                    self.drafts[index].children.push(child);
                }
                Ok(index)
            }
        }
    }

    fn push(
        &mut self,
        kind: RegexSyntaxKind,
        span: ByteSpan,
        focus_span: ByteSpan,
        parent: Option<usize>,
        depth: usize,
    ) -> Result<usize, VisualizationUnavailable> {
        if depth > MAX_SYNTAX_DEPTH {
            return Err(VisualizationUnavailable::DepthLimit {
                limit: MAX_SYNTAX_DEPTH,
            });
        }
        if self.drafts.len() >= MAX_SYNTAX_NODES {
            return Err(VisualizationUnavailable::NodeLimit {
                limit: MAX_SYNTAX_NODES,
            });
        }
        if self.source.get(span.as_range()).is_none()
            || self.source.get(focus_span.as_range()).is_none()
            || parent.is_some_and(|index| index >= self.drafts.len())
        {
            return Err(VisualizationUnavailable::InvalidAstSpan);
        }
        let index = self.drafts.len();
        self.drafts.push(NodeDraft {
            kind,
            span,
            focus_span,
            parent,
            children: Vec::new(),
            explanation_indices: Vec::new(),
            depth,
        });
        Ok(index)
    }
}

fn span_from_ast(source: &str, span: &ast::Span) -> Result<ByteSpan, VisualizationUnavailable> {
    let span = ByteSpan::new(span.start.offset, span.end.offset)
        .ok_or(VisualizationUnavailable::InvalidAstSpan)?;
    source
        .get(span.as_range())
        .ok_or(VisualizationUnavailable::InvalidAstSpan)?;
    Ok(span)
}

fn attach_explanations(drafts: &mut [NodeDraft], explanations: &[Explanation]) {
    let mut by_span: HashMap<(usize, usize), Vec<usize>> = HashMap::new();
    for (index, explanation) in explanations.iter().enumerate() {
        by_span
            .entry((explanation.span.start_byte(), explanation.span.end_byte()))
            .or_default()
            .push(index);
    }
    for draft in drafts {
        let mut linked = Vec::new();
        for span in [draft.focus_span, draft.span] {
            if let Some(indices) = by_span.get(&(span.start_byte(), span.end_byte())) {
                for index in indices {
                    if !linked.contains(index) {
                        linked.push(*index);
                    }
                }
            }
        }
        draft.explanation_indices = linked;
    }
}

fn display_label(
    source: &str,
    kind: RegexSyntaxKind,
    structural: ByteSpan,
    focus: ByteSpan,
) -> String {
    let token = source
        .get(focus.as_range())
        .filter(|token| !token.is_empty())
        .or_else(|| source.get(structural.as_range()))
        .unwrap_or_default();
    let mut label = String::with_capacity(MAX_DISPLAY_LABEL_BYTES);
    label.push_str(kind.label());
    label.push_str(": ");
    let mut characters = token.chars().peekable();
    while let Some(character) = characters.next() {
        let rendered = if character.is_control() {
            ' '
        } else {
            character
        };
        let marker_reserve = if characters.peek().is_some() {
            '…'.len_utf8()
        } else {
            0
        };
        if label.len() + rendered.len_utf8() + marker_reserve > MAX_DISPLAY_LABEL_BYTES {
            if label.len() + '…'.len_utf8() <= MAX_DISPLAY_LABEL_BYTES {
                label.push('…');
            }
            break;
        }
        label.push(rendered);
    }
    label
}

fn caret_contains(span: ByteSpan, offset: usize) -> bool {
    if span.is_empty() {
        span.start_byte() == offset
    } else {
        span.start_byte() <= offset && offset < span.end_byte()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct IdentityKey {
    kind: RegexSyntaxKind,
    token: String,
    parent_kind: Option<RegexSyntaxKind>,
    parent_token: Option<String>,
}

#[derive(Clone, Copy)]
struct UnchangedRegions {
    prefix: usize,
    old_suffix_start: usize,
    new_suffix_start: usize,
}

impl UnchangedRegions {
    fn between(old: &str, new: &str) -> Self {
        let mut prefix = old
            .bytes()
            .zip(new.bytes())
            .take_while(|(left, right)| left == right)
            .count();
        while prefix > 0 && (!old.is_char_boundary(prefix) || !new.is_char_boundary(prefix)) {
            prefix -= 1;
        }
        let mut suffix = old[prefix..]
            .bytes()
            .rev()
            .zip(new[prefix..].bytes().rev())
            .take_while(|(left, right)| left == right)
            .count();
        while suffix > 0
            && (!old.is_char_boundary(old.len() - suffix)
                || !new.is_char_boundary(new.len() - suffix))
        {
            suffix -= 1;
        }
        Self {
            prefix,
            old_suffix_start: old.len() - suffix,
            new_suffix_start: new.len() - suffix,
        }
    }

    fn map_old(self, span: ByteSpan) -> Option<ByteSpan> {
        if span.end_byte() <= self.prefix {
            return Some(span);
        }
        if span.start_byte() >= self.old_suffix_start {
            let start = self.new_suffix_start + (span.start_byte() - self.old_suffix_start);
            let end = self.new_suffix_start + (span.end_byte() - self.old_suffix_start);
            return ByteSpan::new(start, end);
        }
        None
    }

    fn contains_new(self, span: ByteSpan) -> bool {
        span.end_byte() <= self.prefix || span.start_byte() >= self.new_suffix_start
    }
}

fn reconcile_ids(
    previous: &RegexSyntaxTree,
    source: &str,
    drafts: &[NodeDraft],
) -> Vec<Option<RegexSyntaxNodeId>> {
    let regions = UnchangedRegions::between(&previous.source, source);
    let previous_nodes = previous
        .nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<HashMap<_, _>>();

    let mut old_by_key: HashMap<IdentityKey, Vec<(RegexSyntaxNodeId, ByteSpan)>> = HashMap::new();
    for node in &previous.nodes {
        let Some(mapped_span) = regions.map_old(node.span) else {
            continue;
        };
        let key = old_identity_key(&previous.source, node, &previous_nodes);
        old_by_key
            .entry(key)
            .or_default()
            .push((node.id, mapped_span));
    }

    let mut new_by_key: HashMap<IdentityKey, Vec<(usize, ByteSpan)>> = HashMap::new();
    for (index, node) in drafts.iter().enumerate() {
        if !regions.contains_new(node.span) {
            continue;
        }
        let key = draft_identity_key(source, node, drafts);
        new_by_key.entry(key).or_default().push((index, node.span));
    }

    let mut result = vec![None; drafts.len()];
    for (key, old_candidates) in old_by_key {
        let Some(new_candidates) = new_by_key.get(&key) else {
            continue;
        };
        // Matching only a unique source/kind/context signature avoids
        // assigning an old ID to one of several identical repeated nodes.
        if old_candidates.len() == 1 && new_candidates.len() == 1 {
            let (id, mapped_span) = old_candidates[0];
            let (new_index, new_span) = new_candidates[0];
            if mapped_span == new_span {
                result[new_index] = Some(id);
            }
        }
    }
    result
}

fn old_identity_key(
    source: &str,
    node: &RegexSyntaxNode,
    by_id: &HashMap<RegexSyntaxNodeId, &RegexSyntaxNode>,
) -> IdentityKey {
    let (parent_kind, parent_token) = node
        .parent
        .and_then(|parent| by_id.get(&parent).copied())
        .map_or((None, None), |parent| {
            let token = (parent.kind != RegexSyntaxKind::Concatenation).then(|| {
                source
                    .get(parent.span.as_range())
                    .unwrap_or_default()
                    .to_owned()
            });
            (Some(parent.kind), token)
        });
    IdentityKey {
        kind: node.kind,
        token: source
            .get(node.span.as_range())
            .unwrap_or_default()
            .to_owned(),
        parent_kind,
        parent_token,
    }
}

fn draft_identity_key(source: &str, node: &NodeDraft, drafts: &[NodeDraft]) -> IdentityKey {
    let (parent_kind, parent_token) = node.parent.map_or((None, None), |parent_index| {
        let parent = &drafts[parent_index];
        let token = (parent.kind != RegexSyntaxKind::Concatenation).then(|| {
            source
                .get(parent.span.as_range())
                .unwrap_or_default()
                .to_owned()
        });
        (Some(parent.kind), token)
    });
    IdentityKey {
        kind: node.kind,
        token: source
            .get(node.span.as_range())
            .unwrap_or_default()
            .to_owned(),
        parent_kind,
        parent_token,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tree(pattern: &str, flags: &RegexFlags) -> RegexSyntaxTree {
        match visualize(pattern, flags) {
            RegexSyntaxVisualizationResult::Available(tree) => tree,
            RegexSyntaxVisualizationResult::Unavailable(reason) => {
                panic!("expected complete syntax projection, got {reason:?}")
            }
        }
    }

    #[test]
    fn regex_visualization_projects_nested_groups_alternation_concat_and_repetition() {
        let pattern = r"(?P<name>a|b+)(?:c){2}";
        let tree = tree(pattern, &RegexFlags::default());
        let kinds = tree
            .nodes()
            .iter()
            .map(RegexSyntaxNode::kind)
            .collect::<Vec<_>>();
        for kind in [
            RegexSyntaxKind::NamedGroup,
            RegexSyntaxKind::Alternation,
            RegexSyntaxKind::Concatenation,
            RegexSyntaxKind::Repetition,
            RegexSyntaxKind::NonCapturingGroup,
        ] {
            assert!(kinds.contains(&kind), "missing {kind:?}");
        }
        let alternatives = tree
            .nodes()
            .iter()
            .find(|node| node.kind() == RegexSyntaxKind::Alternation)
            .unwrap();
        assert_eq!(alternatives.children().len(), 2);
        let first = tree.node(alternatives.children()[0]).unwrap();
        let second = tree.node(alternatives.children()[1]).unwrap();
        assert_eq!(
            tree.source_for(&tree.node_ref(first.id()).unwrap()),
            Some("a")
        );
        assert_eq!(
            tree.source_for(&tree.node_ref(second.id()).unwrap()),
            Some("b+")
        );

        let repeat = tree
            .nodes()
            .iter()
            .find(|node| {
                node.kind() == RegexSyntaxKind::Repetition
                    && tree.focus_source_for(&tree.node_ref(node.id()).unwrap()) == Some("{2}")
            })
            .unwrap();
        let repeat_ref = tree.node_ref(repeat.id()).unwrap();
        assert_eq!(tree.source_for(&repeat_ref), Some("(?:c){2}"));
        assert_eq!(tree.focus_source_for(&repeat_ref), Some("{2}"));
        assert!(!repeat.explanation_indices().is_empty());
        let super::super::explanation::ExplanationResult::Success { explanations } =
            super::super::explanation::explain(pattern, &RegexFlags::default())
        else {
            panic!("accepted AST should retain its existing explanations");
        };
        let linked = tree.explanations_for(&repeat_ref, &explanations).unwrap();
        assert!(linked.iter().any(|entry| {
            entry.kind == super::super::explanation::ExplanationKind::Repetition
                && entry.token == "{2}"
        }));
    }

    #[test]
    fn regex_visualization_exposes_class_unions_and_binary_set_structure() {
        let pattern = r"[a-z&&[^aeiou]~~[:digit:]]";
        let tree = tree(pattern, &RegexFlags::default());
        let operations = tree
            .nodes()
            .iter()
            .filter(|node| node.kind() == RegexSyntaxKind::ClassSetOperation)
            .collect::<Vec<_>>();
        assert_eq!(operations.len(), 2);
        assert!(operations.iter().all(|node| node.children().len() == 2));
        assert!(
            tree.nodes()
                .iter()
                .any(|node| node.kind() == RegexSyntaxKind::ClassUnion)
        );
        assert!(
            tree.nodes()
                .iter()
                .any(|node| node.kind() == RegexSyntaxKind::CharacterRange)
        );
        for node in tree.nodes() {
            assert!(tree.source().get(node.span().as_range()).is_some());
            assert!(tree.source().get(node.focus_span().as_range()).is_some());
        }
    }

    #[test]
    fn regex_visualization_uses_original_unicode_escape_and_extended_mode_offsets() {
        let pattern = "é # 🧭 ignored\n(?-u:\\x61|🦀)";
        let flags = RegexFlags {
            ignore_whitespace: true,
            ..RegexFlags::default()
        };
        let tree = tree(pattern, &flags);
        assert!(tree.nodes().iter().any(|node| {
            node.kind() == RegexSyntaxKind::Literal
                && tree.source_for(&tree.node_ref(node.id()).unwrap()) == Some("é")
                && node.span() == ByteSpan::new(0, 2).unwrap()
        }));
        assert!(tree.nodes().iter().any(|node| {
            node.kind() == RegexSyntaxKind::Literal
                && tree.source_for(&tree.node_ref(node.id()).unwrap()) == Some(r"\x61")
        }));
        assert!(tree.nodes().iter().any(|node| {
            node.kind() == RegexSyntaxKind::ScopedFlags
                && tree.focus_source_for(&tree.node_ref(node.id()).unwrap()) == Some("-u")
        }));
        assert!(
            !tree
                .nodes()
                .iter()
                .any(|node| tree.source_for(&tree.node_ref(node.id()).unwrap()) == Some("🧭"))
        );
        let ignored_caret = pattern.find("ignored").unwrap() + 2;
        let fallback = tree.best_node_at_caret(ignored_caret).unwrap();
        assert_eq!(
            tree.resolve(&fallback).unwrap().kind(),
            RegexSyntaxKind::Concatenation
        );
        assert!(
            tree.best_node_at_caret(1).is_none(),
            "inside é is not a UTF-8 boundary"
        );
        assert!(tree.best_node_for_selection(1..2).is_none());
        assert!(tree.best_node_for_selection(0..1).is_none());
    }

    #[test]
    fn regex_visualization_queries_choose_smallest_focus_then_structural_fallback() {
        let pattern = r"(?P<item>a+)|";
        let tree = tree(pattern, &RegexFlags::default());
        let plus = pattern.find('+').unwrap();
        let selected = tree.best_node_at_caret(plus).unwrap();
        let node = tree.resolve(&selected).unwrap();
        assert_eq!(node.kind(), RegexSyntaxKind::Repetition);
        assert_eq!(tree.focus_source_for(&selected), Some("+"));
        assert_eq!(
            tree.best_node_at_caret(plus),
            tree.best_node_at_caret(plus),
            "caret tie-breaking is deterministic"
        );

        let empty_offset = pattern.len();
        let empty = tree.best_node_at_caret(empty_offset).unwrap();
        assert_eq!(tree.resolve(&empty).unwrap().kind(), RegexSyntaxKind::Empty);
        assert_eq!(tree.source_for(&empty), Some(""));
        assert!(
            tree.best_node_for_selection(0..pattern.len())
                .is_some_and(|reference| tree.resolve(&reference).is_some())
        );
        assert!(tree.best_node_for_selection(1..2).is_some());
        assert!(tree.best_node_for_selection(2..1).is_none());
        let quantified = match visualize("a+", &RegexFlags::default()) {
            RegexSyntaxVisualizationResult::Available(tree) => tree,
            RegexSyntaxVisualizationResult::Unavailable(reason) => {
                panic!("expected quantified pattern projection, got {reason:?}")
            }
        };
        let spanning_selection = quantified.best_node_for_selection(0..2).unwrap();
        assert_eq!(
            quantified.resolve(&spanning_selection).unwrap().kind(),
            RegexSyntaxKind::Repetition,
            "a selection crossing a literal and quantifier falls back to structure"
        );
    }

    #[test]
    fn regex_visualization_reports_unsupported_compiler_syntax_and_complexity_limits() {
        for pattern in ["(?=a)", r"(a)\1", "(?q:a)"] {
            assert!(matches!(
                visualize(pattern, &RegexFlags::default()),
                RegexSyntaxVisualizationResult::Unavailable(
                    VisualizationUnavailable::InvalidPattern(_)
                )
            ));
        }

        let oversized = "a".repeat(MAX_PATTERN_BYTES + 1);
        assert_eq!(
            visualize(&oversized, &RegexFlags::default()).unavailable_reason(),
            Some(&VisualizationUnavailable::PatternLimit {
                limit: MAX_PATTERN_BYTES
            })
        );

        let broad = "a".repeat(MAX_SYNTAX_NODES + 8);
        assert_eq!(
            visualize(&broad, &RegexFlags::default()).unavailable_reason(),
            Some(&VisualizationUnavailable::NodeLimit {
                limit: MAX_SYNTAX_NODES
            })
        );
        assert!(matches!(
            super::super::engine::evaluate(&broad, &RegexFlags::default(), "a"),
            super::super::model::EvaluationResult::Success { .. }
        ));
        let deep = format!(
            "{}a{}",
            "(?:".repeat(MAX_SYNTAX_DEPTH + 1),
            ")".repeat(MAX_SYNTAX_DEPTH + 1)
        );
        assert_eq!(
            visualize(&deep, &RegexFlags::default()).unavailable_reason(),
            Some(&VisualizationUnavailable::DepthLimit {
                limit: MAX_SYNTAX_DEPTH
            })
        );
        assert!(matches!(
            super::super::engine::evaluate(&deep, &RegexFlags::default(), "a"),
            super::super::model::EvaluationResult::Success { .. }
        ));
    }

    #[test]
    fn regex_visualization_allocates_a_complete_tree_without_a_previous_projection() {
        let tree = tree("a(?i:b)c", &RegexFlags::default());
        let ids = tree
            .nodes()
            .iter()
            .map(RegexSyntaxNode::id)
            .collect::<Vec<_>>();
        assert!(!ids.is_empty());
        assert!(ids.iter().all(|id| id.get() > 0));
        let mut unique = ids.clone();
        unique.sort_by_key(|id| id.get());
        unique.dedup();
        assert_eq!(unique.len(), ids.len());
    }

    #[test]
    fn source_derived_node_labels_are_bounded_without_changing_source_spans() {
        let body = format!("{}a", "a|".repeat(40));
        let pattern = format!("({body})");
        let tree = tree(&pattern, &RegexFlags::default());
        let group = tree
            .nodes()
            .iter()
            .find(|node| node.kind() == RegexSyntaxKind::CapturingGroup)
            .unwrap();
        assert!(group.display_label().len() <= MAX_DISPLAY_LABEL_BYTES);
        assert!(group.display_label().ends_with('…'));
        assert_eq!(
            tree.source_for(&tree.node_ref(group.id()).unwrap()),
            Some(pattern.as_str())
        );
    }

    #[test]
    fn syntax_refs_are_bound_to_their_projection_even_when_ids_match() {
        let flags = RegexFlags::default();
        let first = tree("a", &flags);
        let first_node = first.nodes().first().unwrap();
        let stale = first.node_ref(first_node.id()).unwrap();
        let second = tree("b", &flags);
        assert_eq!(second.nodes().first().unwrap().id(), stale.id());
        assert_eq!(second.generation(), stale.generation());
        assert!(first.resolve(&stale).is_some());
        assert!(second.resolve(&stale).is_none());
    }
}
