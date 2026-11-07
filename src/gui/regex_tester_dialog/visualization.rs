//! Interactive presentation of the accepted Regex Tester syntax projection.

use std::collections::HashSet;
use std::ops::Range;

use crate::regex_tester::{
    EvaluationResult, RegexFlags, RegexSession, RegexSyntaxNodeId, RegexSyntaxNodeRef,
    RegexSyntaxTree, RegexSyntaxVisualizationResult, VisualizationUnavailable,
};
use eframe::egui;

use super::JumpTargets;

#[derive(Clone, Default)]
pub(super) struct VisualizationState {
    accepted_generation: Option<u64>,
    selected: Option<RegexSyntaxNodeRef>,
    pending_source_focus: Option<RegexSyntaxNodeRef>,
    expanded: HashSet<RegexSyntaxNodeId>,
    #[cfg(test)]
    node_rects: std::collections::HashMap<RegexSyntaxNodeId, egui::Rect>,
    #[cfg(test)]
    node_widget_ids: std::collections::HashMap<RegexSyntaxNodeId, egui::Id>,
    #[cfg(test)]
    disclosure_rects: std::collections::HashMap<RegexSyntaxNodeId, egui::Rect>,
    #[cfg(test)]
    constructor_rects: std::collections::HashMap<&'static str, egui::Rect>,
    #[cfg(test)]
    row_indices: std::collections::HashMap<RegexSyntaxNodeId, usize>,
    #[cfg(test)]
    tree_scroll_geometry: Option<(egui::Rect, egui::Rect, egui::Vec2, egui::Vec2, f32)>,
}

#[derive(Clone, Debug)]
pub(super) struct SourceSelection {
    pub pattern: String,
    pub flags: RegexFlags,
    pub bytes: Range<usize>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Constructor {
    EscapeSelection,
    DigitClass,
    WordClass,
    WhitespaceClass,
    Dot,
    StartAnchor,
    EndAnchor,
    Group,
    Alternation,
    OneOrMore,
    ZeroOrMore,
}

#[derive(Clone, Debug)]
pub(super) struct ConstructorRequest {
    pub selection: SourceSelection,
    constructor: Constructor,
}

#[derive(Clone, Debug)]
pub(super) enum Action {
    Constructor(ConstructorRequest),
    SelectNode(RegexSyntaxNodeRef),
    OpenExplanation,
}

#[derive(Clone, Copy)]
struct VisibleNode {
    id: RegexSyntaxNodeId,
    depth: usize,
    last_sibling: bool,
    ancestor_continuations: u128,
}

fn syntax_row_metrics(ui: &egui::Ui) -> (f32, f32) {
    let row_height = ui.spacing().interact_size.y.max(22.0);
    let row_step = row_height + ui.spacing().item_spacing.y;
    (row_height, row_step)
}

fn scroll_margin_height(ui: &egui::Ui) -> f32 {
    let margin = ui.spacing().scroll.content_margin;
    margin.topf() + margin.bottomf()
}

pub(super) fn minimum_viewport_height(ui: &egui::Ui) -> f32 {
    let (_, row_step) = syntax_row_metrics(ui);
    row_step + scroll_margin_height(ui) * 2.0 + ui.spacing().item_spacing.y
}

impl VisualizationState {
    /// Rebind only an existing selected ID that the accepted projection kept.
    /// The projection owns ID reconciliation; this UI state never treats a
    /// stale reference as a current node.
    pub(super) fn accept_tree(&mut self, tree: Option<&RegexSyntaxTree>) {
        let Some(tree) = tree else {
            self.pending_source_focus = None;
            return;
        };
        if self.accepted_generation == Some(tree.generation()) {
            return;
        }

        self.accepted_generation = Some(tree.generation());
        self.selected = self
            .selected
            .take()
            .and_then(|reference| tree.node_ref(reference.id()));
        self.pending_source_focus = None;
        self.expanded = tree
            .nodes()
            .iter()
            .filter(|node| !node.children().is_empty())
            .map(|node| node.id())
            .collect();
    }

    pub(super) fn clear_pending_source_focus(&mut self) {
        self.pending_source_focus = None;
    }

    /// Query accepted syntax using the editor's exact UTF-8 byte selection.
    pub(super) fn reference_from_editor(
        &self,
        tree: &RegexSyntaxTree,
        selection: Range<usize>,
    ) -> Option<RegexSyntaxNodeRef> {
        if selection.is_empty() {
            tree.best_node_at_caret(selection.start)
        } else {
            tree.best_node_for_selection(selection)
        }
    }

    pub(super) fn apply_editor_selection(
        &mut self,
        tree: &RegexSyntaxTree,
        selection: Option<RegexSyntaxNodeRef>,
    ) -> bool {
        let selected = selection
            .and_then(|reference| tree.resolve(&reference).map(|node| node.id()))
            .and_then(|id| tree.node_ref(id));
        if self.selected == selected {
            return false;
        }
        self.selected = selected;
        true
    }

    pub(super) fn select_node(
        &mut self,
        tree: &RegexSyntaxTree,
        reference: &RegexSyntaxNodeRef,
    ) -> bool {
        let Some(node) = tree.resolve(reference) else {
            return false;
        };
        let Some(current) = tree.node_ref(node.id()) else {
            return false;
        };
        self.selected = Some(current.clone());
        true
    }

    pub(super) fn queue_source_focus(
        &mut self,
        tree: &RegexSyntaxTree,
        reference: &RegexSyntaxNodeRef,
    ) -> bool {
        let Some(node) = tree.resolve(reference) else {
            return false;
        };
        self.pending_source_focus = tree.node_ref(node.id());
        self.pending_source_focus.is_some()
    }

    pub(super) fn selected_id(&self) -> Option<RegexSyntaxNodeId> {
        self.selected.as_ref().map(RegexSyntaxNodeRef::id)
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(super) fn observation_identity(&self) -> u64 {
        use std::hash::{Hash, Hasher};

        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        self.accepted_generation.hash(&mut hasher);
        self.selected
            .as_ref()
            .map(RegexSyntaxNodeRef::id)
            .hash(&mut hasher);
        let mut expanded = self.expanded.iter().copied().collect::<Vec<_>>();
        expanded.sort_unstable_by_key(|id| id.get());
        expanded.hash(&mut hasher);
        hasher.finish()
    }

    pub(super) fn selection_matches(&self, candidate: &Option<RegexSyntaxNodeRef>) -> bool {
        self.selected == *candidate
    }

    pub(super) fn take_source_focus_span(
        &mut self,
        tree: &RegexSyntaxTree,
    ) -> Option<crate::regex_tester::ByteSpan> {
        let reference = self.pending_source_focus.take()?;
        tree.resolve(&reference).map(|node| node.focus_span())
    }

    #[cfg(test)]
    pub(super) fn test_node_rect(&self, id: RegexSyntaxNodeId) -> Option<egui::Rect> {
        self.node_rects.get(&id).copied()
    }

    #[cfg(test)]
    pub(super) fn test_node_widget_id(&self, id: RegexSyntaxNodeId) -> Option<egui::Id> {
        self.node_widget_ids.get(&id).copied()
    }

    #[cfg(test)]
    pub(super) fn test_disclosure_rect(&self, id: RegexSyntaxNodeId) -> Option<egui::Rect> {
        self.disclosure_rects.get(&id).copied()
    }

    #[cfg(test)]
    pub(super) fn test_selected_id(&self) -> Option<RegexSyntaxNodeId> {
        self.selected_id()
    }

    #[cfg(test)]
    pub(super) fn test_has_pending_source_focus(&self) -> bool {
        self.pending_source_focus.is_some()
    }

    #[cfg(test)]
    pub(super) fn test_constructor_rect(&self, label: &'static str) -> Option<egui::Rect> {
        self.constructor_rects.get(label).copied()
    }

    #[cfg(test)]
    pub(super) fn test_row_index(&self, id: RegexSyntaxNodeId) -> Option<usize> {
        self.row_indices.get(&id).copied()
    }

    #[cfg(test)]
    pub(super) fn test_tree_scroll_geometry(
        &self,
    ) -> Option<(egui::Rect, egui::Rect, egui::Vec2, egui::Vec2, f32)> {
        self.tree_scroll_geometry
    }

    #[cfg(test)]
    pub(super) fn translate_test_geometry(&mut self, delta: egui::Vec2) {
        for rect in self.node_rects.values_mut() {
            *rect = rect.translate(delta);
        }
        for rect in self.disclosure_rects.values_mut() {
            *rect = rect.translate(delta);
        }
        for rect in self.constructor_rects.values_mut() {
            *rect = rect.translate(delta);
        }
        if let Some((viewport, clipped, content, offset, step)) = self.tree_scroll_geometry {
            self.tree_scroll_geometry = Some((
                viewport.translate(delta),
                clipped.translate(delta),
                content,
                offset,
                step,
            ));
        }
    }
}

pub(super) fn show(
    ui: &mut egui::Ui,
    state: &mut VisualizationState,
    session: &RegexSession,
    source_selection: Option<&SourceSelection>,
    targets: &mut JumpTargets<'_>,
    committing: bool,
) -> Option<Action> {
    #[cfg(test)]
    {
        state.node_rects.clear();
        state.node_widget_ids.clear();
        state.disclosure_rects.clear();
        state.constructor_rects.clear();
        state.row_indices.clear();
        state.tree_scroll_geometry = None;
    }
    let mut action = show_constructors(ui, state, source_selection, targets, committing);

    match session.visualization() {
        Some(RegexSyntaxVisualizationResult::Available(tree)) => {
            state.accept_tree(Some(tree));
            show_tree(ui, state, session, tree, targets, committing, &mut action);
        }
        Some(RegexSyntaxVisualizationResult::Unavailable(reason)) => {
            state.accept_tree(None);
            show_unavailable(ui, reason);
        }
        None => {
            state.accept_tree(None);
            match session.result() {
                Some(EvaluationResult::InvalidPattern(error)) => {
                    ui.colored_label(
                        ui.visuals().error_fg_color,
                        format!("Current pattern is invalid: {}", error.message),
                    );
                }
                Some(EvaluationResult::Suspended(reason)) => {
                    ui.weak(format!(
                        "Syntax is unavailable while evaluation is suspended: {}",
                        super::presentation::suspension(reason)
                    ));
                }
                _ => {
                    ui.weak(
                        "Syntax appears after the current pattern and flags finish evaluation. Older syntax is not shown while edits are pending.",
                    );
                }
            };
        }
    }
    action
}

fn show_unavailable(ui: &mut egui::Ui, reason: &VisualizationUnavailable) {
    let message = match reason {
        VisualizationUnavailable::InvalidPattern(error) => {
            format!("Current pattern is invalid: {}", error.message)
        }
        VisualizationUnavailable::PatternLimit { limit } => {
            format!("Syntax projection is unavailable above {limit} pattern bytes.")
        }
        VisualizationUnavailable::NodeLimit { limit } => {
            format!("Syntax projection exceeds its {limit}-node display bound.")
        }
        VisualizationUnavailable::DepthLimit { limit } => {
            format!("Syntax projection exceeds its nesting-depth bound of {limit}.")
        }
        VisualizationUnavailable::InvalidAstSpan => {
            "The parser returned a syntax span that cannot be mapped to the source.".to_owned()
        }
        VisualizationUnavailable::IdentitySpaceExhausted => {
            "Syntax node identities are unavailable for this pattern.".to_owned()
        }
    };
    ui.colored_label(ui.visuals().warn_fg_color, message);
}

fn show_tree(
    ui: &mut egui::Ui,
    state: &mut VisualizationState,
    session: &RegexSession,
    tree: &RegexSyntaxTree,
    targets: &mut JumpTargets<'_>,
    committing: bool,
    action: &mut Option<Action>,
) {
    ui.label(format!(
        "{} syntax nodes · flags {}",
        tree.nodes().len(),
        tree.flags().suffix()
    ));
    let rows = visible_nodes(tree, &state.expanded);
    let (row_height, row_step) = syntax_row_metrics(ui);
    #[cfg(test)]
    {
        state.node_rects.clear();
        state.node_widget_ids.clear();
        state.disclosure_rects.clear();
        state.row_indices.clear();
        for (index, row) in rows.iter().enumerate() {
            state.row_indices.insert(row.id, index);
        }
    }
    let _parent_clip = ui.clip_rect();
    let _output = egui::ScrollArea::vertical()
        .id_salt("regex_tester_syntax_tree")
        .auto_shrink([false, false])
        .min_scrolled_height(row_step + scroll_margin_height(ui))
        .max_height(ui.available_height().min(260.0).max(row_height))
        .show_rows(ui, row_height, rows.len(), |ui, visible_range| {
            for row_index in visible_range {
                let Some(row) = rows.get(row_index).copied() else {
                    continue;
                };
                let Some(node) = tree.node(row.id) else {
                    continue;
                };
                let row_ui_id = ui.id().with(("regex-syntax-node", row.id.get()));
                ui.scope_builder(egui::UiBuilder::new().id(row_ui_id), |ui| {
                    ui.horizontal(|ui| {
                        let row_left = ui.cursor().min.x;
                        let row_top = ui.cursor().min.y;
                        let row_height = ui.spacing().interact_size.y.max(22.0);
                        let indent_step = 14.0;
                        let depth = row.depth.min(9);
                        let ancestor_width = (depth as f32 * indent_step).min(128.0);
                        ui.add_space(ancestor_width);
                        let slot_width = ui
                            .spacing()
                            .interact_size
                            .x
                            .max(ui.spacing().icon_width)
                            .max(indent_step);
                        let expected_slot_rect = egui::Rect::from_min_size(
                            ui.cursor().min,
                            egui::vec2(slot_width, row_height),
                        );
                        let expanded = state.expanded.contains(&row.id);
                        let disclosure = if !node.children().is_empty() {
                            let accessible_label = if expanded {
                                "Collapse syntax children"
                            } else {
                                "Expand syntax children"
                            };
                            let response = ui
                                .add_sized(
                                    [slot_width, row_height],
                                    egui::Button::new("").frame(false),
                                )
                                .on_hover_text(accessible_label);
                            response.widget_info(|| {
                                egui::WidgetInfo::labeled(
                                    egui::WidgetType::Button,
                                    ui.is_enabled(),
                                    accessible_label,
                                )
                            });
                            Some(response)
                        } else {
                            ui.add_space(slot_width);
                            None
                        };
                        let slot_rect = disclosure
                            .as_ref()
                            .map_or(expected_slot_rect, |response| response.rect);
                        let label = format!("{} · {}", node.kind().label(), node.display_label());
                        let selected = state.selected.as_ref().is_some_and(|reference| {
                            tree.resolve(reference).is_some_and(|n| n.id() == row.id)
                        });
                        let token = tree
                            .source()
                            .get(node.span().as_range())
                            .unwrap_or_default();
                        let row_width = ui.available_width();
                        let response = ui
                            .add_sized(
                                [row_width, row_height],
                                egui::Button::selectable(selected, label).truncate(),
                            )
                            .on_hover_text(format!(
                                "{}\nBytes {}..{}",
                                token,
                                node.span().start_byte(),
                                node.span().end_byte()
                            ));

                        let row_mid = row_top + row_height * 0.5;
                        let stroke_color = ui.visuals().widgets.inactive.fg_stroke.color;
                        let stroke = egui::Stroke::new(1.2, stroke_color);
                        let painter = ui.painter();
                        let branch_x = slot_rect.center().x;
                        let branch_axis_offset = branch_x - row_left - ancestor_width;
                        for ancestor_depth in 0..depth {
                            let continues = 1_u128
                                .checked_shl(ancestor_depth as u32)
                                .is_some_and(|bit| row.ancestor_continuations & bit != 0);
                            if continues {
                                let x = row_left
                                    + ancestor_depth as f32 * indent_step
                                    + branch_axis_offset;
                                painter.line_segment(
                                    [egui::pos2(x, row_top), egui::pos2(x, row_top + row_height)],
                                    stroke,
                                );
                            }
                        }
                        let icon_clearance = (slot_width * 0.28).min(5.0);
                        painter.line_segment(
                            [
                                egui::pos2(branch_x, row_top),
                                egui::pos2(branch_x, row_mid - icon_clearance),
                            ],
                            stroke,
                        );
                        if !row.last_sibling {
                            painter.line_segment(
                                [
                                    egui::pos2(branch_x, row_mid + icon_clearance),
                                    egui::pos2(branch_x, row_top + row_height),
                                ],
                                stroke,
                            );
                        }
                        painter.line_segment(
                            [
                                egui::pos2(slot_rect.right(), row_mid),
                                egui::pos2(response.rect.left(), row_mid),
                            ],
                            stroke,
                        );
                        if let Some(disclosure) = &disclosure {
                            let icon_color = if disclosure.hovered() || disclosure.has_focus() {
                                ui.visuals().widgets.active.fg_stroke.color
                            } else {
                                stroke_color
                            };
                            let center = disclosure.rect.center();
                            let reach = (slot_width * 0.2).min(3.5);
                            let points = if expanded {
                                [
                                    egui::pos2(center.x - reach, center.y - reach * 0.5),
                                    egui::pos2(center.x, center.y + reach * 0.5),
                                    egui::pos2(center.x + reach, center.y - reach * 0.5),
                                ]
                            } else {
                                [
                                    egui::pos2(center.x - reach * 0.5, center.y - reach),
                                    egui::pos2(center.x + reach * 0.5, center.y),
                                    egui::pos2(center.x - reach * 0.5, center.y + reach),
                                ]
                            };
                            painter.line_segment(
                                [points[0], points[1]],
                                egui::Stroke::new(1.5, icon_color),
                            );
                            painter.line_segment(
                                [points[1], points[2]],
                                egui::Stroke::new(1.5, icon_color),
                            );
                            #[cfg(test)]
                            if disclosure.rect.intersects(ui.clip_rect()) {
                                state.disclosure_rects.insert(row.id, disclosure.rect);
                            }
                            if disclosure.rect.intersects(ui.clip_rect()) {
                                targets.button(
                                    ui,
                                    disclosure,
                                    format!("syntax-expand/{}", row.id.get()),
                                    "Expand or collapse syntax children",
                                );
                            }
                            if committing && disclosure.clicked() {
                                if expanded {
                                    state.expanded.remove(&row.id);
                                } else {
                                    state.expanded.insert(row.id);
                                }
                            }
                        } else {
                            painter.circle_filled(egui::pos2(branch_x, row_mid), 1.8, stroke_color);
                        }
                        if response.rect.intersects(ui.clip_rect()) {
                            targets.selectable(
                                ui,
                                &response,
                                format!("syntax-node/{}", row.id.get()),
                                "Regex syntax node",
                            );
                        }
                        #[cfg(test)]
                        if response.rect.intersects(ui.clip_rect()) {
                            state.node_rects.insert(row.id, response.rect);
                            state.node_widget_ids.insert(row.id, response.id);
                        }
                        if committing
                            && response.clicked()
                            && action.is_none()
                            && let Some(reference) = tree.node_ref(row.id)
                        {
                            *action = Some(Action::SelectNode(reference));
                        }
                    });
                });
            }
        });
    #[cfg(test)]
    {
        state.tree_scroll_geometry = Some((
            _output.inner_rect,
            _output.inner_rect.intersect(_parent_clip),
            _output.content_size,
            _output.state.offset,
            row_step,
        ));
    }

    ui.separator();
    let selected = state
        .selected
        .as_ref()
        .and_then(|reference| tree.resolve(reference));
    let Some(node) = selected else {
        ui.weak("Select a syntax node to inspect its exact source and explanation.");
        return;
    };
    ui.strong(node.kind().label());
    ui.label(format!(
        "Source bytes {}..{} (end exclusive)",
        node.span().start_byte(),
        node.span().end_byte()
    ));
    if node.focus_span() != node.span() {
        ui.label(format!(
            "Focused bytes {}..{}",
            node.focus_span().start_byte(),
            node.focus_span().end_byte()
        ));
    }
    let token = tree
        .node_ref(node.id())
        .and_then(|reference| tree.source_for(&reference))
        .unwrap_or_default();
    ui.add(
        egui::Label::new(egui::RichText::new(token).monospace())
            .wrap_mode(egui::TextWrapMode::Wrap),
    );

    let explanations = session.explanation().and_then(|result| match result {
        crate::regex_tester::ExplanationResult::Success { explanations } => tree
            .node_ref(node.id())
            .and_then(|reference| tree.explanations_for(&reference, explanations)),
        crate::regex_tester::ExplanationResult::InvalidPattern(_) => None,
    });
    if let Some(explanations) = explanations.filter(|items| !items.is_empty()) {
        for (index, explanation) in explanations.into_iter().enumerate() {
            ui.horizontal_wrapped(|ui| {
                ui.weak(format!(
                    "{} · bytes {}..{}",
                    explanation.label,
                    explanation.span.start_byte(),
                    explanation.span.end_byte()
                ));
                if index == 0 {
                    let link = ui.link("Open explanation list");
                    if link.rect.intersects(ui.clip_rect()) {
                        targets.button(
                            ui,
                            &link,
                            "syntax-open-explanation",
                            "Open explanation list",
                        );
                    }
                    if committing && link.clicked() && action.is_none() {
                        *action = Some(Action::OpenExplanation);
                    }
                }
            });
            ui.label(&explanation.description);
        }
    } else {
        ui.weak(format!(
            "Structural {} node; the compiler explanation has no more specific source entry.",
            node.kind().label().to_lowercase()
        ));
    }
}

fn visible_nodes(
    tree: &RegexSyntaxTree,
    expanded: &HashSet<RegexSyntaxNodeId>,
) -> Vec<VisibleNode> {
    let mut rows = Vec::with_capacity(tree.nodes().len());
    let mut pending = Vec::with_capacity(tree.nodes().len());
    for (index, root) in tree.roots().iter().copied().enumerate().rev() {
        pending.push((root, 0, index + 1 == tree.roots().len(), 0_u128));
    }
    while let Some((id, depth, last_sibling, ancestor_continuations)) = pending.pop() {
        let Some(node) = tree.node(id) else {
            continue;
        };
        rows.push(VisibleNode {
            id,
            depth,
            last_sibling,
            ancestor_continuations,
        });
        if expanded.contains(&id) {
            let current_continuation = if last_sibling {
                0
            } else {
                1_u128.checked_shl(depth as u32).unwrap_or(0)
            };
            let child_continuations = ancestor_continuations | current_continuation;
            for (index, child) in node.children().iter().copied().enumerate().rev() {
                pending.push((
                    child,
                    depth.saturating_add(1),
                    index + 1 == node.children().len(),
                    child_continuations,
                ));
            }
        }
    }
    rows
}

fn show_constructors(
    ui: &mut egui::Ui,
    _state: &mut VisualizationState,
    selection: Option<&SourceSelection>,
    targets: &mut JumpTargets<'_>,
    committing: bool,
) -> Option<Action> {
    ui.strong("Construct");
    ui.weak("Insert at the editor selection or caret. Selected expressions are grouped before repetition.");
    let usable = selection.is_some_and(|selection| {
        selection.pattern.len() <= crate::regex_tester::policy::MAX_PATTERN_BYTES
            && selection.bytes.start <= selection.bytes.end
            && selection.bytes.end <= selection.pattern.len()
            && selection.pattern.is_char_boundary(selection.bytes.start)
            && selection.pattern.is_char_boundary(selection.bytes.end)
    });
    let mut selected_action = None;
    #[cfg(test)]
    _state.constructor_rects.clear();
    ui.horizontal_wrapped(|ui| {
        let constructors = [
            (
                Constructor::EscapeSelection,
                "Escape selection",
                "Insert selected text as a literal",
            ),
            (Constructor::DigitClass, "\\d", "Digit class"),
            (Constructor::WordClass, "\\w", "Word character class"),
            (Constructor::WhitespaceClass, "\\s", "Whitespace class"),
            (Constructor::Dot, ".", "Any character"),
            (Constructor::StartAnchor, "^", "Start anchor"),
            (Constructor::EndAnchor, "$", "End anchor"),
            (
                Constructor::Group,
                "Group",
                "Wrap selected text in a non-capturing group",
            ),
            (Constructor::Alternation, "|", "Alternation operator"),
            (
                Constructor::OneOrMore,
                "+",
                "Repeat selected expression one or more times",
            ),
            (
                Constructor::ZeroOrMore,
                "*",
                "Repeat selected expression zero or more times",
            ),
        ];
        for (constructor, label, hint) in constructors {
            let can_escape = constructor != Constructor::EscapeSelection
                || selection.is_some_and(|selection| !selection.bytes.is_empty());
            let response = ui
                .add_enabled(usable && can_escape, egui::Button::new(label))
                .on_hover_text(hint);
            #[cfg(test)]
            _state.constructor_rects.insert(label, response.rect);
            if response.rect.intersects(ui.clip_rect()) {
                targets.button(
                    ui,
                    &response,
                    format!("syntax-constructor/{constructor:?}"),
                    "Regex constructor",
                );
            }
            if committing && response.clicked() && selected_action.is_none() {
                selected_action = selection.cloned().map(|selection| {
                    Action::Constructor(ConstructorRequest {
                        selection,
                        constructor,
                    })
                });
            }
        }
    });
    selected_action
}

/// Build a complete replacement for one captured source selection. The caller
/// revalidates the pattern, flags, editor selection and byte limit immediately
/// before applying it.
pub(super) fn constructor_replacement(
    request: &ConstructorRequest,
) -> Option<(String, Range<usize>)> {
    let source = &request.selection.pattern;
    let range = request.selection.bytes.clone();
    let selected = source.get(range.clone())?;
    let (inserted, caret_offset) = match request.constructor {
        Constructor::EscapeSelection => (regex::escape(selected), None),
        Constructor::DigitClass => (r"\d".to_owned(), None),
        Constructor::WordClass => (r"\w".to_owned(), None),
        Constructor::WhitespaceClass => (r"\s".to_owned(), None),
        Constructor::Dot => (".".to_owned(), None),
        Constructor::StartAnchor => ("^".to_owned(), None),
        Constructor::EndAnchor => ("$".to_owned(), None),
        Constructor::Alternation => ("|".to_owned(), None),
        Constructor::Group => {
            if selected.is_empty() {
                ("(?:)".to_owned(), Some(3))
            } else {
                (format!("(?:{selected})"), None)
            }
        }
        Constructor::OneOrMore | Constructor::ZeroOrMore => {
            let quantifier = if request.constructor == Constructor::OneOrMore {
                "+"
            } else {
                "*"
            };
            if selected.is_empty() {
                (format!("(?:){quantifier}"), Some(3))
            } else {
                (format!("(?:{selected}){quantifier}"), None)
            }
        }
    };
    let mut replacement = String::with_capacity(source.len() + inserted.len());
    replacement.push_str(&source[..range.start]);
    replacement.push_str(&inserted);
    let inserted_start = range.start;
    replacement.push_str(&source[range.end..]);
    let cursor_byte = inserted_start + caret_offset.unwrap_or(inserted.len());
    Some((replacement, cursor_byte..cursor_byte))
}

pub(super) fn pattern_char_range(source: &str, bytes: Range<usize>) -> Option<Range<usize>> {
    if bytes.start > bytes.end
        || bytes.end > source.len()
        || !source.is_char_boundary(bytes.start)
        || !source.is_char_boundary(bytes.end)
    {
        return None;
    }
    Some(source[..bytes.start].chars().count()..source[..bytes.end].chars().count())
}

pub(super) fn byte_range_from_char_range(
    source: &str,
    chars: Range<usize>,
) -> Option<Range<usize>> {
    if chars.start > chars.end {
        return None;
    }
    let byte_at = |char_index: usize| {
        if char_index == source.chars().count() {
            Some(source.len())
        } else {
            source.char_indices().nth(char_index).map(|(byte, _)| byte)
        }
    };
    Some(byte_at(chars.start)?..byte_at(chars.end)?)
}

pub(super) fn pattern_id() -> egui::Id {
    egui::Id::new("regex_tester_pattern")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn request(source: &str, bytes: Range<usize>, constructor: Constructor) -> ConstructorRequest {
        ConstructorRequest {
            selection: SourceSelection {
                pattern: source.to_owned(),
                flags: RegexFlags::default(),
                bytes,
            },
            constructor,
        }
    }

    #[test]
    fn regex_visualization_constructors_escape_unicode_wrap_and_attach_quantifiers() {
        let escaped =
            constructor_replacement(&request("a.b", 1..2, Constructor::EscapeSelection)).unwrap();
        assert_eq!(escaped.0, r"a\.b");
        let grouped = constructor_replacement(&request("é+", 0..3, Constructor::Group)).unwrap();
        assert_eq!(grouped.0, "(?:é+)");
        let repeated =
            constructor_replacement(&request("猫", 0..3, Constructor::OneOrMore)).unwrap();
        assert_eq!(repeated.0, "(?:猫)+");
        let empty_repetition =
            constructor_replacement(&request("", 0..0, Constructor::ZeroOrMore)).unwrap();
        assert_eq!(empty_repetition.0, "(?:)*");
        assert_eq!(empty_repetition.1, 3..3);
    }

    #[test]
    fn regex_visualization_cursor_conversion_respects_utf8_scalar_boundaries() {
        assert_eq!(pattern_char_range("é猫x", 2..5), Some(1..2));
        assert_eq!(byte_range_from_char_range("é猫x", 1..2), Some(2..5));
        assert_eq!(pattern_char_range("é", 1..2), None);
        assert_eq!(byte_range_from_char_range("é", 0..3), None);
    }
}
