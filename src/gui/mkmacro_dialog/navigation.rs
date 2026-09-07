//! One derived row representation and one stable-ID navigation owner.
use super::{MkMacroDialog, action_catalog};
use crate::mkmacro::{MkAction, MkMacroDocument, StructureAnalysis, authoring_fields};
use std::{cell::RefCell, collections::HashMap, sync::Arc};

#[derive(Default)]
pub(super) struct NavigationState {
    cache: RefCell<RowCache>,
    pub search: super::search::SearchState,
    pub outline: super::outline::OutlineState,
    pub focus_table: bool,
}

pub(super) struct SearchRow {
    pub macro_id: u64,
    pub step_id: u64,
    pub row: usize,
    pub action_type: &'static str,
    pub details: String,
    pub label: String,
    pub comment: String,
    pub bookmarked: bool,
    pub call_target: Option<String>,
    pub fields: Vec<authoring_fields::AuthoringField>,
    pub depth: usize,
    pub outline_node: bool,
    pub caption: String,
    searchable: String,
}

impl SearchRow {
    pub fn selectable(
        &self,
        ui: &eframe::egui::Ui,
        selected: bool,
    ) -> eframe::egui::SelectableLabel {
        let mut text = eframe::egui::text::LayoutJob::simple(
            self.caption.clone(),
            eframe::egui::TextStyle::Button.resolve(ui.style()),
            ui.visuals().text_color(),
            (ui.available_width() - 2.0 * ui.spacing().button_padding.x).max(1.0),
        );
        text.wrap.max_rows = 1;
        text.wrap.break_anywhere = true;
        eframe::egui::SelectableLabel::new(selected, text)
    }

    pub fn hover(&self, ui: &mut eframe::egui::Ui) {
        ui.strong(format!("Step {} · {}", self.row, self.action_type));
        if self.bookmarked {
            ui.label("★ Bookmarked");
        }
        if let Some(target) = &self.call_target {
            ui.label(format!("Call target: {target}"));
        }
        ui.label(&self.details);
        for field in &self.fields {
            if !field.value.is_empty() {
                ui.label(format!("{}: {}", field.path, field.value));
            }
        }
    }
}

#[derive(Default)]
struct RowCache {
    key: Option<(u64, u64)>,
    rows: Arc<Vec<SearchRow>>,
}

impl RowCache {
    fn rows(
        &mut self,
        document: &MkMacroDocument,
        revision: u64,
        macro_id: u64,
        structure: &StructureAnalysis,
    ) -> Arc<Vec<SearchRow>> {
        if self.key == Some((revision, macro_id)) {
            return self.rows.clone();
        }
        self.key = Some((revision, macro_id));
        let names: HashMap<_, _> = document
            .macros
            .iter()
            .map(|m| (m.id, m.name.as_str()))
            .collect();
        self.rows = Arc::new(
            document
                .macros
                .iter()
                .find(|m| m.id == macro_id)
                .into_iter()
                .flat_map(|m| m.steps.iter())
                .enumerate()
                .map(|(index, step)| {
                    let action_type = action_catalog::action_name(&step.action);
                    let details = action_catalog::action_details(&step.action);
                    let call_target = match &step.action {
                        MkAction::CallMacro(call) => Some(names.get(&call.macro_id).map_or_else(
                            || format!("Missing macro #{}", call.macro_id),
                            |name| (*name).to_owned(),
                        )),
                        _ => None,
                    };
                    let fields = authoring_fields::step_fields(step);
                    let mut searchable = format!(
                        "{} {} {} {} {}",
                        index + 1,
                        action_type,
                        details,
                        call_target.as_deref().unwrap_or_default(),
                        if step.metadata.bookmarked {
                            "bookmark bookmarked"
                        } else {
                            ""
                        }
                    );
                    for field in &fields {
                        searchable.push('\n');
                        searchable.push_str(&field.value);
                    }
                    let mut caption = format!(
                        "{}  {}{}",
                        index + 1,
                        if step.metadata.bookmarked { "★ " } else { "" },
                        action_type
                    );
                    if !step.metadata.label.is_empty() {
                        caption.push_str(&format!(" · {}", step.metadata.label));
                    }
                    if let Some(target) = &call_target {
                        caption.push_str(&format!(" → {target}"));
                    }
                    if !step.metadata.comment.is_empty() {
                        let excerpt: String = step
                            .metadata
                            .comment
                            .chars()
                            .take(70)
                            .map(|c| if c.is_whitespace() { ' ' } else { c })
                            .collect();
                        caption.push_str(&format!(" — {excerpt}"));
                    }
                    SearchRow {
                        macro_id,
                        step_id: step.id,
                        row: index + 1,
                        action_type,
                        details,
                        label: step.metadata.label.clone(),
                        comment: step.metadata.comment.clone(),
                        bookmarked: step.metadata.bookmarked,
                        call_target,
                        fields,
                        depth: structure.steps.get(index).map_or(0, |s| s.depth),
                        outline_node: step.action.block_marker().is_some()
                            || matches!(
                                step.action,
                                MkAction::CallMacro(_)
                                    | MkAction::Return(_)
                                    | MkAction::Break
                                    | MkAction::Continue
                            )
                            || step.metadata.bookmarked
                            || !step.metadata.label.is_empty(),
                        caption,
                        searchable: searchable.to_lowercase(),
                    }
                })
                .collect(),
        );
        self.rows.clone()
    }
}

pub(super) fn rows(d: &MkMacroDialog) -> Arc<Vec<SearchRow>> {
    let Some(mid) = d.selected_macro_id else {
        return Arc::default();
    };
    let Some(structure) = d.cached_structure(mid) else {
        return Arc::default();
    };
    d.navigation
        .cache
        .borrow_mut()
        .rows(&d.draft, d.draft_revision(), mid, &structure)
}

/// Query work is shared by all three consumers and repeated only on a changed
/// row generation, query, or explicit empty-query policy.
#[derive(Default)]
pub(super) struct RowFilter {
    rows: Option<Arc<Vec<SearchRow>>>,
    query: String,
    empty_matches: bool,
    matches: Arc<Vec<usize>>,
}

impl RowFilter {
    pub fn matches(
        &mut self,
        rows: &Arc<Vec<SearchRow>>,
        query: &str,
        empty_matches: bool,
    ) -> Arc<Vec<usize>> {
        if self.rows.as_ref().is_some_and(|old| Arc::ptr_eq(old, rows))
            && self.query == query
            && self.empty_matches == empty_matches
        {
            return self.matches.clone();
        }
        let normalized = query.trim().to_lowercase();
        self.matches = Arc::new(
            rows.iter()
                .enumerate()
                .filter(|(_, row)| {
                    if normalized.is_empty() {
                        empty_matches
                    } else {
                        row.searchable.contains(&normalized)
                    }
                })
                .map(|(i, _)| i)
                .collect(),
        );
        self.rows = Some(rows.clone());
        self.query = query.to_owned();
        self.empty_matches = empty_matches;
        self.matches.clone()
    }
}

pub(super) fn table_id(macro_id: u64) -> eframe::egui::Id {
    eframe::egui::Id::new(("mkmacro_table_focus", macro_id))
}

pub(super) fn navigate(d: &mut MkMacroDialog, macro_id: u64, step_id: u64) -> bool {
    let Some(structure) = d.cached_structure(macro_id) else {
        return false;
    };
    if structure.step(step_id).is_none() {
        return false;
    }
    d.set_selected_macro(Some(macro_id));
    d.editor_state
        .folds
        .expand_ancestors(macro_id, step_id, &structure);
    d.selection.replace([step_id]);
    d.editor_state.scroll_to = Some((macro_id, step_id));
    d.navigation.focus_table = true;
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::*;

    fn document() -> MkMacroDocument {
        let actions = vec![
            MkAction::If(MkCondition::All { conditions: vec![] }),
            MkAction::RepeatStart { count: 2 },
            MkAction::Text(MkTextPayload {
                text: "secret ${token}".into(),
                mode: MkTextMode::Type,
            }),
            MkAction::RepeatEnd,
            MkAction::Else,
            MkAction::CallMacro(MkCallMacroPayload {
                macro_id: 2,
                ..Default::default()
            }),
            MkAction::Return(Default::default()),
            MkAction::EndIf,
            MkAction::Delay(Default::default()),
            MkAction::Delay(Default::default()),
        ];
        let mut root = MkMacro {
            id: 1,
            name: "Root".into(),
            description: String::new(),
            enabled: true,
            hotkey: None,
            hotkey_scope: Default::default(),
            folder_id: None,
            playback: Default::default(),
            signature: Default::default(),
            steps: actions
                .into_iter()
                .enumerate()
                .map(|(i, action)| MkStep {
                    id: (i as u64 + 1) * 10,
                    enabled: true,
                    breakpoint: false,
                    repeat: 1,
                    delay_after_ms: 0,
                    on_error: Default::default(),
                    metadata: Default::default(),
                    action,
                })
                .collect(),
        };
        root.steps[9].metadata = MkStepMetadata {
            label: "Checkpoint".into(),
            comment: "Review the result".into(),
            bookmarked: true,
            ..Default::default()
        };
        let mut callee = root.clone();
        callee.id = 2;
        callee.name = "Dependency name".into();
        callee.steps.clear();
        MkMacroDocument {
            macros: vec![root, callee],
            ..Default::default()
        }
    }

    #[test]
    fn shared_search_rows_cover_fields_outline_hierarchy_and_revision_cache() {
        let mut doc = document();
        let structure = analyze_structure(&doc.macros[0].steps);
        let mut cache = RowCache::default();
        let rows = cache.rows(&doc, 1, 1, &structure);
        assert!(Arc::ptr_eq(&rows, &cache.rows(&doc, 1, 1, &structure)));
        assert_eq!(
            rows.iter().map(|r| r.depth).collect::<Vec<_>>(),
            [0, 1, 2, 1, 0, 1, 1, 0, 0, 0]
        );
        assert!(!rows[2].outline_node);
        assert!(!rows[8].outline_node);
        assert!(
            rows[4].outline_node
                && rows[5].outline_node
                && rows[6].outline_node
                && rows[9].outline_node
        );
        let mut filter = RowFilter::default();
        for (query, expected) in [
            ("secret", 2),
            ("token", 2),
            ("type 15 characters", 2),
            ("Dependency NAME", 5),
            ("checkpoint", 9),
            ("bookmarked", 9),
            ("review the result", 9),
        ] {
            assert_eq!(
                filter.matches(&rows, query, false).as_slice(),
                [expected],
                "{query}"
            );
        }
        let matched = filter.matches(&rows, "token", false);
        assert!(Arc::ptr_eq(
            &matched,
            &filter.matches(&rows, "token", false)
        ));
        assert!(filter.matches(&rows, "not here", false).is_empty());
        assert!(filter.matches(&rows, "", false).is_empty());
        assert_eq!(filter.matches(&rows, "", true).len(), rows.len());
        doc.macros[1].name = "Renamed dependency".into();
        let updated = cache.rows(&doc, 2, 1, &structure);
        assert!(!Arc::ptr_eq(&rows, &updated));
        assert_eq!(
            updated[5].call_target.as_deref(),
            Some("Renamed dependency")
        );
        assert!(
            filter
                .matches(&updated, "Dependency name", false)
                .is_empty()
        );
        assert_eq!(
            filter
                .matches(&updated, "Renamed dependency", false)
                .as_slice(),
            [5]
        );
    }

    #[test]
    fn navigation_expands_hidden_targets_and_never_dirties_the_draft() {
        let directory = tempfile::tempdir().unwrap();
        let (store, _) = MkMacroStore::open(directory.path()).unwrap();
        let mut d = MkMacroDialog::new(Arc::new(store));
        d.draft = document();
        d.mark_dirty();
        d.dirty = false;
        d.set_selected_macro(Some(1));
        let revision = d.draft_revision();
        let original = d.draft.clone();
        let structure = d.cached_structure(1).unwrap();
        d.editor_state.folds.toggle(1, 10, &structure);
        d.editor_state.folds.toggle(1, 20, &structure);
        assert_eq!(d.editor_state.folds.visible_rows(1, &structure), [0, 8, 9]);
        assert!(navigate(&mut d, 1, 30));
        assert_eq!(d.selection.ids, std::collections::BTreeSet::from([30]));
        assert_eq!(d.selection.primary, Some(30));
        assert_eq!(d.editor_state.scroll_to, Some((1, 30)));
        assert!(d.navigation.focus_table);
        assert_eq!(
            d.editor_state.folds.visible_rows(1, &structure),
            (0..10).collect::<Vec<_>>()
        );
        assert!(!navigate(&mut d, 2, 30));
        assert_eq!(d.selected_macro_id, Some(1));
        assert_eq!(d.draft_revision(), revision);
        assert_eq!(d.draft, original);
        assert!(!d.dirty);
    }
}
