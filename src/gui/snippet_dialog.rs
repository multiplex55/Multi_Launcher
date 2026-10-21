use super::{LauncherApp, push_toast};
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::plugins::snippet_template::{TemplateError, parse_template};
use crate::plugins::snippets::{
    MAX_SNIPPET_TAG_CHARS, MAX_SNIPPET_TAGS, SNIPPETS_FILE, SnippetEntry, SnippetFieldDefinition,
    SnippetInputKind, SnippetPreparationError, load_snippets, normalize_snippet_tags,
    prepare_snippet_text, snippet_preview_text, update_snippets,
};
use eframe::egui;
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Clone, Debug, PartialEq, Eq)]
struct PendingRemoval {
    index: usize,
    confirmation_instance: u64,
    expected_entry: SnippetEntry,
    expected_entries: Vec<SnippetEntry>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct AliasValidation {
    edited_index: Option<usize>,
    alias: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum CommitRejection {
    StaleSnapshot,
    DuplicateAlias,
    InvalidEditorState,
    InvalidTemplate,
    InvalidTags,
}

#[derive(Debug)]
enum EditorCandidateError {
    InvalidState(&'static str),
    InvalidTags(&'static str),
    PromptedTemplate(SnippetPreparationError),
}

#[derive(Debug)]
enum CommitFailure {
    Rejected(CommitRejection),
    Persistence(anyhow::Error),
}

#[derive(Default)]
pub struct SnippetDialog {
    pub open: bool,
    entries: Vec<SnippetEntry>,
    snapshot_generation: u64,
    view_revision: u64,
    removal_instance: u64,
    selected_entry_index: Option<usize>,
    edit_idx: Option<usize>,
    body_edit_session: u64,
    alias: String,
    text: String,
    hide_contents: bool,
    body_revealed: bool,
    prompt_for_fields: bool,
    fields: Vec<SnippetFieldDefinition>,
    tags: Vec<String>,
    tag_input: String,
    tag_error: Option<String>,
    discovered_field_keys: Vec<String>,
    template_error: Option<TemplateError>,
    filter: String,
    selected_tag_filters: Vec<String>,
    load_error: Option<String>,
    inline_error: Option<String>,
    pending_removal: Option<PendingRemoval>,
    #[cfg(test)]
    persistence_path_override: Option<String>,
    #[cfg(test)]
    body_editor_widget_id: Option<egui::Id>,
    #[cfg(test)]
    contents_scroll_id: Option<egui::Id>,
}

fn matches_snippet_filter(entry: &SnippetEntry, filter: &str) -> bool {
    let filter = filter.trim();
    if filter.is_empty() {
        return true;
    }

    let lowered_filter = filter.to_lowercase();
    entry.alias.to_lowercase().contains(&lowered_filter)
        || entry.text.to_lowercase().contains(&lowered_filter)
        || entry
            .tags
            .iter()
            .any(|tag| tag.to_lowercase().contains(&lowered_filter))
}

#[cfg(test)]
fn matching_snippet_indices(entries: &[SnippetEntry], filter: &str) -> Vec<usize> {
    matching_snippet_indices_with_tags(entries, filter, &[])
}

fn matching_snippet_indices_with_tags(
    entries: &[SnippetEntry],
    filter: &str,
    selected_tags: &[String],
) -> Vec<usize> {
    let selected_tags = selected_tags
        .iter()
        .map(|tag| tag.to_lowercase())
        .collect::<Vec<_>>();
    entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| {
            let matches_tags = selected_tags
                .iter()
                .all(|selected| entry.tags.iter().any(|tag| tag.to_lowercase() == *selected));
            (matches_snippet_filter(entry, filter) && matches_tags).then_some(index)
        })
        .collect()
}

fn available_snippet_tags(entries: &[SnippetEntry], selected: &[String]) -> Vec<String> {
    let mut tags = Vec::new();
    for tag in entries
        .iter()
        .flat_map(|entry| entry.tags.iter())
        .chain(selected.iter())
    {
        if !tags
            .iter()
            .any(|known: &String| known.to_lowercase() == tag.to_lowercase())
        {
            tags.push(tag.clone());
        }
    }
    tags.sort_by_key(|tag| tag.to_lowercase());
    tags
}

fn body_editor_id_source(session: u64) -> (&'static str, u64) {
    ("snippet_body_editor", session)
}

fn register_snippet_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: &'static str,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) -> Option<crate::gui::jump_mode::ui::FocusDirective> {
    crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    )
}

fn apply_select_focus(
    response: &egui::Response,
    directive: Option<crate::gui::jump_mode::ui::FocusDirective>,
) {
    if directive == Some(crate::gui::jump_mode::ui::FocusDirective::Select) {
        response.request_focus();
    }
}

fn show_field_definition_row(
    ui: &mut egui::Ui,
    session: u64,
    field_index: usize,
    key: &str,
    field: &mut SnippetFieldDefinition,
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    visual_order: &mut u64,
    authorized_owner_pass: bool,
) -> bool {
    let mut field_shadow;
    let field = if authorized_owner_pass {
        field
    } else {
        field_shadow = field.clone();
        &mut field_shadow
    };
    ui.label(key);
    let label_response = ui.add_sized(
        [130.0, ui.spacing().interact_size.y],
        egui::TextEdit::singleline(&mut field.label)
            .id_salt(("snippet_prompt_field_label", session, key))
            .hint_text(key),
    );
    apply_select_focus(
        &label_response,
        register_snippet_target(
            scope,
            ui,
            &label_response,
            format!("author/prompt-field/{field_index}/label"),
            "Prompt field label",
            crate::gui::jump_mode::TargetKind::TextEntry,
            visual_order,
        ),
    );
    let default_response = ui.add_sized(
        [170.0, 46.0],
        egui::TextEdit::multiline(&mut field.default_value)
            .id_salt(("snippet_prompt_field_default", session, key))
            .desired_rows(2)
            .hint_text("Default value"),
    );
    apply_select_focus(
        &default_response,
        register_snippet_target(
            scope,
            ui,
            &default_response,
            format!("author/prompt-field/{field_index}/default"),
            "Prompt field default",
            crate::gui::jump_mode::TargetKind::TextEntry,
            visual_order,
        ),
    );
    let required_response = ui.checkbox(&mut field.required, "Required");
    apply_select_focus(
        &required_response,
        register_snippet_target(
            scope,
            ui,
            &required_response,
            format!("author/prompt-field/{field_index}/required"),
            "Prompt field required",
            crate::gui::jump_mode::TargetKind::Checkbox,
            visual_order,
        ),
    );
    let input_kind = match field.input_kind {
        SnippetInputKind::SingleLine => "Single line",
        SnippetInputKind::Multiline => "Multiline",
    };
    let combo_identity = format!("author/prompt-field/{field_index}/input-kind");
    let popup_scope = scope.map(|scope| scope.combo_popup_scope(&combo_identity));
    let mut kind_changed = false;
    let combo = egui::ComboBox::from_id_salt(("snippet_prompt_field_kind", session, key))
        .selected_text(input_kind)
        .show_ui(ui, |ui| {
            let single_line = ui.selectable_value(
                &mut field.input_kind,
                SnippetInputKind::SingleLine,
                "Single line",
            );
            kind_changed |= single_line.changed();
            apply_select_focus(
                &single_line,
                register_snippet_target(
                    popup_scope.as_ref(),
                    ui,
                    &single_line,
                    "option/single-line",
                    "Single line input",
                    crate::gui::jump_mode::TargetKind::Selectable,
                    visual_order,
                ),
            );
            let multiline = ui.selectable_value(
                &mut field.input_kind,
                SnippetInputKind::Multiline,
                "Multiline",
            );
            kind_changed |= multiline.changed();
            apply_select_focus(
                &multiline,
                register_snippet_target(
                    popup_scope.as_ref(),
                    ui,
                    &multiline,
                    "option/multiline",
                    "Multiline input",
                    crate::gui::jump_mode::TargetKind::Selectable,
                    visual_order,
                ),
            );
        });
    apply_select_focus(
        &combo.response,
        register_snippet_target(
            scope,
            ui,
            &combo.response,
            combo_identity,
            "Prompt field input type",
            crate::gui::jump_mode::TargetKind::ComboBox,
            visual_order,
        ),
    );
    ui.end_row();
    label_response.changed()
        || default_response.changed()
        || required_response.changed()
        || kind_changed
}

fn single_line_alias(alias: &str) -> String {
    let mut display = String::with_capacity(alias.len());
    let mut space_pending = false;
    for character in alias.chars() {
        if character.is_whitespace() || character.is_control() {
            space_pending = !display.is_empty();
        } else {
            if space_pending {
                display.push(' ');
                space_pending = false;
            }
            display.push(character);
        }
    }
    display
}

fn alias_is_duplicate(snippets: &[SnippetEntry], validation: &AliasValidation) -> bool {
    if validation.edited_index.is_some_and(|index| {
        snippets
            .get(index)
            .is_some_and(|entry| entry.alias == validation.alias)
    }) {
        return false;
    }

    snippets.iter().enumerate().any(|(index, entry)| {
        Some(index) != validation.edited_index && entry.alias == validation.alias
    })
}

fn update_snapshot(
    path: &str,
    expected: &[SnippetEntry],
    candidate: Vec<SnippetEntry>,
    alias_validation: Option<AliasValidation>,
) -> Result<Vec<SnippetEntry>, CommitFailure> {
    let mut rejection = None;
    let committed = update_snippets(path, |current| {
        if current.as_slice() != expected {
            rejection = Some(CommitRejection::StaleSnapshot);
            return Ok(false);
        }
        if let Some(validation) = &alias_validation
            && alias_is_duplicate(current, validation)
        {
            rejection = Some(CommitRejection::DuplicateAlias);
            return Ok(false);
        }
        if current.as_slice() == candidate.as_slice() {
            return Ok(false);
        }

        *current = candidate;
        Ok(true)
    })
    .map_err(CommitFailure::Persistence)?;

    if let Some(rejection) = rejection {
        Err(CommitFailure::Rejected(rejection))
    } else {
        Ok(committed)
    }
}

fn is_load_failure(error: &anyhow::Error) -> bool {
    matches!(
        error.downcast_ref::<crate::common::persistence::PersistenceError>(),
        Some(
            crate::common::persistence::PersistenceError::Read { .. }
                | crate::common::persistence::PersistenceError::MalformedJson { .. }
        )
    )
}

impl SnippetDialog {
    pub fn open(&mut self) {
        self.open_from(SNIPPETS_FILE);
    }

    fn open_from(&mut self, path: &str) {
        let _ = self.load_from(path);
        self.open = true;
        self.reset_editor();
        self.filter.clear();
        self.selected_tag_filters.clear();
        self.inline_error = None;
        self.pending_removal = None;
    }

    pub fn ensure_open(&mut self) {
        self.ensure_open_from(SNIPPETS_FILE);
    }

    fn ensure_open_from(&mut self, path: &str) {
        if !self.open {
            self.open_from(path);
        }
    }

    pub fn end_session(&mut self) {
        if self.open {
            self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
            self.advance_view_revision();
            self.selected_entry_index = None;
        }
        self.open = false;
        self.reset_editor();
        self.filter.clear();
        self.selected_tag_filters.clear();
        self.load_error = None;
        self.inline_error = None;
        self.pending_removal = None;
    }

    pub fn open_edit(&mut self, alias: &str) {
        self.open_edit_from(SNIPPETS_FILE, alias);
    }

    fn open_edit_from(&mut self, path: &str, alias: &str) {
        self.pending_removal = None;
        self.inline_error = None;
        if self.load_from(path).is_err() {
            self.reset_editor();
            self.filter.clear();
            self.selected_tag_filters.clear();
            self.open = true;
            return;
        }
        self.filter.clear();
        self.selected_tag_filters.clear();
        if let Some(pos) = self.entries.iter().position(|e| e.alias == alias) {
            self.begin_existing(pos);
        } else {
            self.begin_new(alias);
        }
    }

    fn begin_existing(&mut self, index: usize) -> bool {
        let Some(entry) = self.entries.get(index).cloned() else {
            return false;
        };
        self.body_edit_session = self.body_edit_session.wrapping_add(1);
        self.advance_view_revision();
        self.pending_removal = None;
        self.inline_error = None;
        self.edit_idx = Some(index);
        self.alias = entry.alias;
        self.text = entry.text;
        self.hide_contents = entry.hide_contents;
        self.body_revealed = !entry.hide_contents;
        self.prompt_for_fields = entry.prompt_for_fields;
        self.fields = entry.fields;
        self.tags = entry.tags;
        self.tag_input.clear();
        self.tag_error = None;
        self.refresh_prompted_fields();
        self.open = true;
        true
    }

    fn begin_new(&mut self, alias: &str) {
        self.body_edit_session = self.body_edit_session.wrapping_add(1);
        self.advance_view_revision();
        self.pending_removal = None;
        self.inline_error = None;
        self.edit_idx = Some(self.entries.len());
        self.alias = alias.to_owned();
        self.text.clear();
        self.hide_contents = false;
        self.body_revealed = true;
        self.prompt_for_fields = false;
        self.fields.clear();
        self.tags.clear();
        self.tag_input.clear();
        self.tag_error = None;
        self.discovered_field_keys.clear();
        self.template_error = None;
        self.open = true;
    }

    fn reset_editor(&mut self) {
        if self.edit_idx.is_some() {
            self.advance_view_revision();
        }
        self.edit_idx = None;
        self.alias.clear();
        self.text.clear();
        self.hide_contents = false;
        self.body_revealed = false;
        self.prompt_for_fields = false;
        self.fields.clear();
        self.tags.clear();
        self.tag_input.clear();
        self.tag_error = None;
        self.discovered_field_keys.clear();
        self.template_error = None;
    }

    fn add_editor_tag(&mut self) {
        if self.tag_input.trim().is_empty() {
            self.tag_input.clear();
            self.tag_error = None;
            return;
        }
        let tag = self.tag_input.clone();
        match self.add_editor_tag_value(&tag) {
            Ok(()) => self.tag_input.clear(),
            Err(error) => self.tag_error = Some(error.to_owned()),
        }
    }

    fn add_editor_tag_value(&mut self, tag: &str) -> Result<(), &'static str> {
        let mut candidate = self.tags.clone();
        candidate.push(tag.to_owned());
        match normalize_snippet_tags(candidate) {
            Ok(tags) => {
                if self.tags != tags {
                    self.tags = tags;
                    self.advance_view_revision();
                }
                self.tag_error = None;
                self.inline_error = None;
                Ok(())
            }
            Err(error) => Err(error),
        }
    }

    fn remove_editor_tag(&mut self, tag: &str) {
        let previous_len = self.tags.len();
        self.tags
            .retain(|existing| existing.to_lowercase() != tag.to_lowercase());
        if self.tags.len() != previous_len {
            self.advance_view_revision();
        }
        self.tag_error = None;
        self.inline_error = None;
    }

    fn toggle_tag_filter(&mut self, tag: &str) {
        if let Some(index) = self
            .selected_tag_filters
            .iter()
            .position(|selected| selected.to_lowercase() == tag.to_lowercase())
        {
            self.selected_tag_filters.remove(index);
        } else {
            self.selected_tag_filters.push(tag.to_owned());
        }
        self.filter_changed();
    }

    fn clear_filter_state(&mut self) {
        self.filter.clear();
        self.selected_tag_filters.clear();
        self.filter_changed();
    }

    fn set_prompt_for_fields(&mut self, enabled: bool) {
        if self.prompt_for_fields == enabled {
            return;
        }
        self.prompt_for_fields = enabled;
        self.advance_view_revision();
        self.inline_error = None;
        if enabled {
            self.refresh_prompted_fields();
        } else {
            self.discovered_field_keys.clear();
            self.template_error = None;
        }
    }

    /// Refresh the active discovery view without dropping detached settings for
    /// keys that temporarily disappeared from the text.
    fn refresh_prompted_fields(&mut self) {
        let previous_keys = self.discovered_field_keys.clone();
        self.discovered_field_keys.clear();
        self.template_error = None;
        if !self.prompt_for_fields {
            return;
        }

        match parse_template(&self.text) {
            Ok(parsed) => {
                self.discovered_field_keys = parsed.field_keys;
                for key in &self.discovered_field_keys {
                    if !self.fields.iter().any(|field| field.name == *key) {
                        self.fields.push(SnippetFieldDefinition::new(key.clone()));
                    }
                }
            }
            Err(error) => self.template_error = Some(error),
        }
        if self.discovered_field_keys != previous_keys {
            self.advance_view_revision();
        }
    }

    fn advance_view_revision(&mut self) {
        self.view_revision = self.view_revision.wrapping_add(1);
    }

    pub(crate) fn jump_mode_target_generation(&self) -> crate::gui::jump_mode::TargetGeneration {
        crate::gui::jump_mode::TargetGeneration(self.snapshot_generation)
    }

    pub(crate) fn jump_mode_pending_removal_identity(&self) -> Option<(u64, usize)> {
        self.pending_removal
            .as_ref()
            .map(|pending| (pending.confirmation_instance, pending.index))
    }

    #[cfg(test)]
    pub(crate) fn test_open_from(&mut self, path: &str) {
        self.open_from(path);
    }

    #[cfg(test)]
    pub(crate) fn test_request_removal(&mut self, index: usize) {
        self.request_removal(index);
    }

    #[cfg(test)]
    pub(crate) fn test_cancel_removal(&mut self) {
        self.cancel_removal();
    }

    #[cfg(test)]
    pub(crate) fn test_use_persistence_path(&mut self, path: &str) {
        self.persistence_path_override = Some(path.to_owned());
    }

    #[cfg(test)]
    pub(crate) fn test_selected_entry_index(&self) -> Option<usize> {
        self.selected_entry_index
    }

    #[cfg(test)]
    pub(crate) fn test_entries(&self) -> &[SnippetEntry] {
        &self.entries
    }

    #[cfg(test)]
    pub(crate) fn test_is_editing(&self) -> bool {
        self.edit_idx.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_edit_index(&self) -> Option<usize> {
        self.edit_idx
    }

    #[cfg(test)]
    pub(crate) fn test_alias(&self) -> &str {
        &self.alias
    }

    #[cfg(test)]
    pub(crate) fn test_filter(&self) -> &str {
        &self.filter
    }

    #[cfg(test)]
    pub(crate) fn test_text(&self) -> &str {
        &self.text
    }

    #[cfg(test)]
    pub(crate) fn test_body_revealed(&self) -> bool {
        self.body_revealed
    }

    #[cfg(test)]
    pub(crate) fn test_contents_scroll_id(&self) -> Option<egui::Id> {
        self.contents_scroll_id
    }

    #[cfg(test)]
    pub(crate) fn test_body_editor_widget_id(&self) -> Option<egui::Id> {
        self.body_editor_widget_id
    }

    fn field_mut(&mut self, key: &str) -> Option<&mut SnippetFieldDefinition> {
        self.fields.iter_mut().find(|field| field.name == key)
    }

    fn show_prompt_field_settings(
        &mut self,
        ui: &mut egui::Ui,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        visual_order: &mut u64,
        authorized_owner_pass: bool,
    ) {
        if !self.prompt_for_fields {
            if !self.fields.is_empty() {
                secondary_help(ui, "Field settings are retained while prompting is off.");
            }
            return;
        }

        compact_section_heading(
            ui,
            "Prompt fields",
            Some("Configure the inputs requested by this template."),
        );
        if !self.body_revealed {
            secondary_help(ui, "Field settings are hidden until contents are revealed.");
            return;
        }
        if let Some(error) = &self.template_error {
            semantic_status(
                ui,
                StatusTone::Error,
                "Template error",
                Some(&error.to_string()),
            );
            secondary_help(
                ui,
                "Existing field settings remain in this draft until the template is valid.",
            );
            return;
        }
        if self.discovered_field_keys.is_empty() {
            secondary_help(
                ui,
                "Add at least one placeholder, for example {{name}}, to enable prompting.",
            );
            if !self.fields.is_empty() {
                secondary_help(
                    ui,
                    "Previously configured field settings are retained until Save.",
                );
            }
            return;
        }

        secondary_help(ui, "Fields follow their first appearance in the text.");
        let body_edit_session = self.body_edit_session;
        egui::Grid::new(("snippet_prompt_field_grid", body_edit_session))
            .striped(true)
            .show(ui, |ui| {
                ui.strong("Key");
                ui.strong("Label");
                ui.strong("Default");
                ui.strong("Required");
                ui.strong("Input");
                ui.end_row();

                let mut changed = false;
                for (field_index, key) in self.discovered_field_keys.clone().into_iter().enumerate()
                {
                    let Some(field) = self.field_mut(&key) else {
                        ui.label(&key);
                        ui.label("Field settings unavailable");
                        ui.end_row();
                        continue;
                    };
                    changed |= show_field_definition_row(
                        ui,
                        body_edit_session,
                        field_index,
                        &key,
                        field,
                        scope,
                        visual_order,
                        authorized_owner_pass,
                    );
                }
                if changed && authorized_owner_pass {
                    self.inline_error = None;
                }
            });
        if self
            .fields
            .iter()
            .any(|field| !self.discovered_field_keys.contains(&field.name))
        {
            secondary_help(
                ui,
                "Settings for removed keys stay in this draft if you re-add them; Save prunes absent keys.",
            );
        }
    }

    fn apply_window_open(&mut self, open: bool) {
        if open {
            self.open = true;
        } else {
            self.end_session();
        }
    }

    fn cancel_editor(&mut self) {
        self.reset_editor();
        self.inline_error = None;
        self.pending_removal = None;
    }

    fn reveal_contents(&mut self) {
        if self.edit_idx.is_some() {
            if !self.body_revealed {
                self.advance_view_revision();
            }
            self.body_revealed = true;
        }
    }

    fn set_hide_contents(&mut self, hide_contents: bool) {
        if self.hide_contents == hide_contents {
            return;
        }
        self.hide_contents = hide_contents;
        self.advance_view_revision();
        let editing_existing = self
            .edit_idx
            .is_some_and(|index| index < self.entries.len());
        if editing_existing && hide_contents {
            self.body_revealed = false;
        }
    }

    fn editor_candidate(
        &self,
    ) -> Result<(Vec<SnippetEntry>, Vec<SnippetEntry>, AliasValidation), EditorCandidateError> {
        let Some(index) = self.edit_idx else {
            return Err(EditorCandidateError::InvalidState(
                "No snippet is being edited.",
            ));
        };
        let expected = self.entries.clone();
        let mut candidate = expected.clone();
        let mut draft = if index == candidate.len() {
            SnippetEntry {
                alias: self.alias.clone(),
                text: self.text.clone(),
                hide_contents: self.hide_contents,
                prompt_for_fields: self.prompt_for_fields,
                fields: self.fields.clone(),
                tags: self.tags.clone(),
            }
        } else if let Some(entry) = candidate.get(index) {
            let mut draft = entry.clone();
            draft.alias = self.alias.clone();
            draft.text = self.text.clone();
            draft.hide_contents = self.hide_contents;
            draft.prompt_for_fields = self.prompt_for_fields;
            draft.fields = self.fields.clone();
            draft.tags = self.tags.clone();
            draft
        } else {
            return Err(EditorCandidateError::InvalidState(
                "The edited snippet no longer exists.",
            ));
        };
        draft.tags = normalize_snippet_tags(std::mem::take(&mut draft.tags))
            .map_err(EditorCandidateError::InvalidTags)?;
        let mut prepared = prepare_snippet_text(&draft, &self.text)
            .map_err(EditorCandidateError::PromptedTemplate)?;
        prepared.alias = self.alias.clone();
        prepared.hide_contents = self.hide_contents;
        let alias_validation = if index == candidate.len() {
            candidate.push(prepared);
            AliasValidation {
                edited_index: None,
                alias: self.alias.clone(),
            }
        } else if index < candidate.len() {
            candidate[index] = prepared;
            AliasValidation {
                edited_index: Some(index),
                alias: self.alias.clone(),
            }
        } else {
            return Err(EditorCandidateError::InvalidState(
                "The edited snippet no longer exists.",
            ));
        };
        Ok((expected, candidate, alias_validation))
    }

    fn can_preview_editor_draft(&self) -> bool {
        if !self.body_revealed {
            return false;
        }
        if self.prompt_for_fields {
            return true;
        }

        let Ok((expected, candidate, alias_validation)) = self.editor_candidate() else {
            return false;
        };
        let candidate_index = alias_validation.edited_index.unwrap_or(expected.len());
        let Some(draft) = candidate.get(candidate_index) else {
            return false;
        };
        match crate::plugins::snippets::prepare_snippet_run(draft) {
            Ok(crate::plugins::snippets::SnippetRunMode::SystemOnly(_)) => true,
            Err(_) => {
                crate::plugins::snippet_template::parse_plain_system_template(&draft.text).is_err()
            }
            Ok(
                crate::plugins::snippets::SnippetRunMode::Plain
                | crate::plugins::snippets::SnippetRunMode::Prompted(_),
            ) => false,
        }
    }

    fn preview_editor_draft(&mut self) -> Option<SnippetEntry> {
        if !self.body_revealed {
            self.inline_error = Some("Reveal contents before previewing this snippet.".to_owned());
            return None;
        }
        if self.alias.trim().is_empty() {
            self.inline_error = Some("An alias is required before previewing.".to_owned());
            return None;
        }

        let (expected, candidate, alias_validation) = match self.editor_candidate() {
            Ok(candidate) => candidate,
            Err(EditorCandidateError::InvalidState(error)) => {
                self.inline_error = Some(error.to_owned());
                return None;
            }
            Err(EditorCandidateError::InvalidTags(error)) => {
                self.inline_error = Some(error.to_owned());
                return None;
            }
            Err(EditorCandidateError::PromptedTemplate(error)) => {
                self.inline_error = Some(error.to_string());
                return None;
            }
        };
        if alias_is_duplicate(&expected, &alias_validation) {
            self.inline_error = Some("An entry with this exact alias already exists.".to_owned());
            return None;
        }

        let candidate_index = alias_validation.edited_index.unwrap_or(expected.len());
        let Some(draft) = candidate.get(candidate_index).cloned() else {
            self.inline_error = Some("The editor is no longer available.".to_owned());
            return None;
        };
        match crate::plugins::snippets::prepare_snippet_run(&draft) {
            Ok(
                crate::plugins::snippets::SnippetRunMode::SystemOnly(_)
                | crate::plugins::snippets::SnippetRunMode::Prompted(_),
            ) => {}
            Ok(crate::plugins::snippets::SnippetRunMode::Plain) => {
                self.inline_error = Some(
                    "Plain snippets are literal; add an automatic value to preview one.".into(),
                );
                return None;
            }
            Err(error) => {
                self.inline_error = Some(error.to_string());
                return None;
            }
        }
        self.inline_error = None;
        Some(draft)
    }

    pub(crate) fn report_preview_start_failure(
        &mut self,
        error: crate::gui::snippet_prompt_dialog::SnippetPromptError,
    ) {
        self.inline_error = Some(error.message().to_owned());
    }

    fn save_editor(&mut self, path: &str) -> Result<(), CommitFailure> {
        let (expected, candidate, alias_validation) = match self.editor_candidate() {
            Ok(candidate) => candidate,
            Err(EditorCandidateError::InvalidState(error)) => {
                self.inline_error = Some(error.to_owned());
                return Err(CommitFailure::Rejected(CommitRejection::InvalidEditorState));
            }
            Err(EditorCandidateError::InvalidTags(error)) => {
                self.inline_error = Some(error.to_owned());
                return Err(CommitFailure::Rejected(CommitRejection::InvalidTags));
            }
            Err(EditorCandidateError::PromptedTemplate(error)) => {
                self.inline_error = Some(error.to_string());
                return Err(CommitFailure::Rejected(CommitRejection::InvalidTemplate));
            }
        };
        self.commit_candidate(path, &expected, candidate, Some(alias_validation))?;
        self.reset_editor();
        Ok(())
    }

    fn load_from(&mut self, path: &str) -> anyhow::Result<()> {
        self.pending_removal = None;
        self.inline_error = None;
        self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
        self.advance_view_revision();
        self.selected_entry_index = None;
        match load_snippets(path) {
            Ok(entries) => {
                self.entries = entries;
                self.load_error = None;
                Ok(())
            }
            Err(error) => {
                self.load_error = Some(error.to_string());
                Err(error)
            }
        }
    }

    fn commit_candidate(
        &mut self,
        path: &str,
        expected: &[SnippetEntry],
        candidate: Vec<SnippetEntry>,
        alias_validation: Option<AliasValidation>,
    ) -> Result<(), CommitFailure> {
        match update_snapshot(path, expected, candidate, alias_validation) {
            Ok(committed) => {
                self.entries = committed;
                self.snapshot_generation = self.snapshot_generation.wrapping_add(1);
                self.advance_view_revision();
                self.selected_entry_index = None;
                self.load_error = None;
                self.inline_error = None;
                Ok(())
            }
            Err(CommitFailure::Rejected(rejection)) => {
                self.inline_error = Some(match &rejection {
                    CommitRejection::StaleSnapshot => {
                        "Snippets changed since this view was loaded. Reload and try again."
                            .to_owned()
                    }
                    CommitRejection::DuplicateAlias => {
                        "An entry with this exact alias already exists.".to_owned()
                    }
                    CommitRejection::InvalidEditorState => {
                        "The editor is no longer available.".to_owned()
                    }
                    CommitRejection::InvalidTemplate => {
                        "The prompted snippet template is invalid.".to_owned()
                    }
                    CommitRejection::InvalidTags => "Snippet tags are invalid.".to_owned(),
                });
                Err(CommitFailure::Rejected(rejection))
            }
            Err(CommitFailure::Persistence(error)) => {
                let error_message = error.to_string();
                if is_load_failure(&error) {
                    self.load_error = Some(error_message.clone());
                }
                self.inline_error = Some(format!("Failed to save snippets: {error_message}"));
                Err(CommitFailure::Persistence(error))
            }
        }
    }

    fn request_removal(&mut self, index: usize) {
        let Some(expected_entry) = self.entries.get(index).cloned() else {
            return;
        };
        self.removal_instance = self.removal_instance.wrapping_add(1).max(1);
        self.pending_removal = Some(PendingRemoval {
            index,
            confirmation_instance: self.removal_instance,
            expected_entry,
            expected_entries: self.entries.clone(),
        });
        self.advance_view_revision();
        self.inline_error = None;
    }

    fn cancel_removal(&mut self) {
        if self.pending_removal.is_some() {
            self.advance_view_revision();
        }
        self.pending_removal = None;
        self.inline_error = None;
    }

    fn filter_changed(&mut self) {
        self.advance_view_revision();
        self.pending_removal = None;
        self.inline_error = None;
    }

    fn confirm_removal(&mut self, path: &str) -> Result<bool, CommitFailure> {
        let Some(pending) = self.pending_removal.clone() else {
            return Ok(false);
        };
        if pending.expected_entries.get(pending.index) != Some(&pending.expected_entry) {
            self.pending_removal = None;
            let rejection = CommitRejection::StaleSnapshot;
            self.inline_error = Some(
                "Snippets changed since this view was loaded. Reload and try again.".to_owned(),
            );
            return Err(CommitFailure::Rejected(rejection));
        }

        let mut candidate = pending.expected_entries.clone();
        candidate.remove(pending.index);
        match self.commit_candidate(path, &pending.expected_entries, candidate, None) {
            Ok(()) => {
                self.pending_removal = None;
                Ok(true)
            }
            Err(error @ CommitFailure::Rejected(CommitRejection::StaleSnapshot)) => {
                self.pending_removal = None;
                Err(error)
            }
            Err(error) => Err(error),
        }
    }

    fn finish_success(app: &mut LauncherApp, message: &str) {
        if app.enable_toasts {
            push_toast(
                &mut app.toasts,
                Toast {
                    text: message.into(),
                    kind: ToastKind::Success,
                    options: ToastOptions::default().duration_in_seconds(app.toast_duration as f64),

                    style: Default::default(),
                },
            );
        }
        app.search();
        app.focus_input();
    }

    fn report_commit_failure(app: &mut LauncherApp, failure: &CommitFailure) {
        if let CommitFailure::Persistence(error) = failure {
            app.report_error_message("ui operation", format!("Failed to save snippets: {error}"));
        }
    }

    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) -> Option<SnippetEntry> {
        self.ui_with_jump_mode(ctx, app, None, true, true)
    }

    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        app: &mut LauncherApp,
        jump_mode_scope: Option<crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        first_ui_pass: bool,
    ) -> Option<SnippetEntry> {
        if !self.open {
            self.end_session();
            return None;
        }
        #[cfg(test)]
        let persistence_path_override = self.persistence_path_override.clone();
        #[cfg(test)]
        let persistence_path = persistence_path_override
            .as_deref()
            .unwrap_or(SNIPPETS_FILE);
        #[cfg(not(test))]
        let persistence_path = SNIPPETS_FILE;
        let authorized_owner_pass = native_ui_owner && first_ui_pass;
        let (window_open, save_request, confirm_removal, preview_request) = self
            .show_window_with_jump_mode(
                ctx,
                jump_mode_scope.as_ref(),
                native_ui_owner,
                authorized_owner_pass,
                || app.report_error_message("ui operation", "Both fields required"),
            );

        if save_request && authorized_owner_pass {
            match self.save_editor(persistence_path) {
                Ok(()) => Self::finish_success(app, "Saved snippet"),
                Err(failure) => Self::report_commit_failure(app, &failure),
            }
        }
        if confirm_removal && authorized_owner_pass {
            match self.confirm_removal(persistence_path) {
                Ok(true) => Self::finish_success(app, "Removed snippet"),
                Ok(false) => {}
                Err(failure) => Self::report_commit_failure(app, &failure),
            }
        }
        self.apply_window_open(if native_ui_owner {
            if authorized_owner_pass {
                window_open
            } else {
                self.open
            }
        } else {
            self.open
        });
        if preview_request && authorized_owner_pass {
            self.preview_editor_draft()
        } else {
            None
        }
    }

    fn show_window(
        &mut self,
        ctx: &egui::Context,
        mut report_required_fields: impl FnMut(),
    ) -> (bool, bool, bool, bool) {
        self.show_window_with_jump_mode(ctx, None, true, true, &mut report_required_fields)
    }

    fn show_window_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        jump_mode_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        authorized_owner_pass: bool,
        mut report_required_fields: impl FnMut(),
    ) -> (bool, bool, bool, bool) {
        let mut window_open = self.open;
        let mut close = false;
        let mut save_request = false;
        let mut confirm_removal = false;
        let mut preview_request = false;
        let initial_view_revision = self.view_revision;
        let scoped_scope = jump_mode_scope
            .map(|scope| scope.child_namespace(format!("view/{}", self.view_revision)));
        let removal_scope = self.pending_removal.as_ref().and(scoped_scope.as_ref());
        let scope = self
            .pending_removal
            .is_none()
            .then_some(scoped_scope.as_ref())
            .flatten();
        let mut visual_order = 0_u64;
        egui::Window::new("Snippets")
            .default_size((600.0, 500.0))
            .min_width(160.0)
            .min_height(160.0)
            .resizable(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.add_enabled_ui(native_ui_owner, |ui| {
                // Resize follows content in egui. Bound both axes here so overflow
                // scrolls inside the user's chosen size instead of enlarging it.
                let _contents = egui::ScrollArea::both()
                    .id_salt("snippet_dialog_contents")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                // Only controls set the minimum scrollable width, never the body.
                ui.set_min_width(280.0);
                if let Some(error) = &self.load_error {
                    compact_section_heading(ui, "Saved snippets", None);
                    semantic_status(
                        ui,
                        StatusTone::Error,
                        "Snippets are read-only because loading failed",
                        Some(error),
                    );
                    let close_response = ui.button("Close");
                    apply_select_focus(
                        &close_response,
                        register_snippet_target(
                            scope,
                            ui,
                            &close_response,
                            "load-error/close",
                            "Close snippets",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        ),
                    );
                    if with_button_emphasis(ui, close_response, ButtonEmphasis::Primary).clicked()
                        && authorized_owner_pass
                    {
                        close = true;
                    }
                    return;
                }
                if self.edit_idx.is_some() {
                    compact_section_heading(
                        ui,
                        if self.edit_idx == Some(self.entries.len()) {
                            "New snippet"
                        } else {
                            "Edit snippet"
                        },
                        Some("Aliases are matched exactly, including case and surrounding spaces."),
                    );
                    let mut alias_shadow;
                    let alias_value = if authorized_owner_pass {
                        &mut self.alias
                    } else {
                        alias_shadow = self.alias.clone();
                        &mut alias_shadow
                    };
                    let alias = labeled_form_row(ui, "Alias", |ui| {
                        let width = ui.available_width().max(80.0);
                        ui.add_sized(
                            [width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(alias_value)
                                .desired_width(f32::INFINITY),
                        )
                    });
                    apply_select_focus(
                        &alias,
                        register_snippet_target(
                            scope,
                            ui,
                            &alias,
                            "author/alias",
                            "Snippet alias",
                            crate::gui::jump_mode::TargetKind::TextEntry,
                            &mut visual_order,
                        ),
                    );
                    if alias.changed() && authorized_owner_pass {
                        self.inline_error = None;
                    }
                    let mut hide_contents = self.hide_contents;
                    let hide_response = ui
                        .checkbox(&mut hide_contents, "Hide contents")
                        .on_hover_text(
                            "Hides previews only; saved text and copied clipboard/history remain plaintext.",
                        );
                    apply_select_focus(
                        &hide_response,
                        register_snippet_target(
                            scope,
                            ui,
                            &hide_response,
                            "author/hide-contents",
                            "Hide snippet contents",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        ),
                    );
                    if hide_response.changed() && authorized_owner_pass {
                        self.set_hide_contents(hide_contents);
                    }
                    secondary_help(
                        ui,
                        "Hiding replaces previews only; saved text and copied clipboard/history remain plaintext. This is not encryption.",
                    );
                    let mut prompt_for_fields = self.prompt_for_fields;
                    let prompt_response = ui
                        .checkbox(&mut prompt_for_fields, "Prompt for fields")
                        .on_hover_text(
                            "When enabled, {{field}} placeholders request values before copying.",
                        );
                    apply_select_focus(
                        &prompt_response,
                        register_snippet_target(
                            scope,
                            ui,
                            &prompt_response,
                            "author/prompt-for-fields",
                            "Prompt for fields",
                            crate::gui::jump_mode::TargetKind::Checkbox,
                            &mut visual_order,
                        ),
                    );
                    if prompt_response.changed() && authorized_owner_pass {
                        self.set_prompt_for_fields(prompt_for_fields);
                    }
                    ui.small(r"Use {{field}} for an input; prefix with \{{...}} to keep braces literal.");
                    ui.small(r"Example: Hello {{name}}. {{date}} is an ordinary input, not a computed date.");
                    compact_section_heading(
                        ui,
                        &format!("Tags ({}/{MAX_SNIPPET_TAGS})", self.tags.len()),
                        Some("Tags are searchable labels; adding or removing them never changes snippet text."),
                    );
                    let mut add_tag = false;
                    let mut tag_input_changed = false;
                    let mut tag_input_response = None;
                    let mut add_tag_response = None;
                    let mut tag_input_shadow;
                    let tag_input_value = if authorized_owner_pass {
                        &mut self.tag_input
                    } else {
                        tag_input_shadow = self.tag_input.clone();
                        &mut tag_input_shadow
                    };
                    ui.horizontal(|ui| {
                        let button_width = 64.0;
                        let spacing = ui.spacing().item_spacing.x;
                        let input_width = (ui.available_width() - button_width - spacing).max(80.0);
                        let input = ui.add_sized(
                            [input_width, ui.spacing().interact_size.y],
                            egui::TextEdit::singleline(tag_input_value)
                                .hint_text(format!("Add a tag (up to {MAX_SNIPPET_TAG_CHARS} characters)")),
                        );
                        tag_input_response = Some(input.clone());
                        tag_input_changed = input.changed();
                        add_tag |= input.lost_focus()
                            && ui.input(|events| events.key_pressed(egui::Key::Enter))
                            && authorized_owner_pass;
                        let add = ui.add_sized(
                            [button_width, ui.spacing().interact_size.y],
                            egui::Button::new("Add tag"),
                        );
                        add_tag |= add.clicked() && authorized_owner_pass;
                        add_tag_response = Some(add);
                    });
                    if let Some(response) = tag_input_response.as_ref() {
                        apply_select_focus(
                            response,
                            register_snippet_target(
                                scope,
                                ui,
                                response,
                                "author/tag-input",
                                "Snippet tag",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            ),
                        );
                    }
                    if let Some(response) = add_tag_response.as_ref() {
                        apply_select_focus(
                            response,
                            register_snippet_target(
                                scope,
                                ui,
                                response,
                                "author/add-tag",
                                "Add snippet tag",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                    }
                    if tag_input_changed && authorized_owner_pass {
                        self.tag_error = None;
                    }
                    if add_tag {
                        self.add_editor_tag();
                    }
                    if self.tags.is_empty() {
                        secondary_help(ui, "No tags yet. Add up to 16 labels; spaces within a tag are normalized.");
                    } else {
                        ui.horizontal_wrapped(|ui| {
                            for (tag_index, tag) in self.tags.clone().into_iter().enumerate() {
                                let response = ui
                                    .selectable_label(true, format!("{tag}  ×"))
                                    .on_hover_text("Remove this tag from the draft.")
                                    ;
                                apply_select_focus(
                                    &response,
                                    register_snippet_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("author/tag/{tag_index}/remove"),
                                        "Remove snippet tag",
                                        crate::gui::jump_mode::TargetKind::Button,
                                        &mut visual_order,
                                    ),
                                );
                                if response.clicked() && authorized_owner_pass {
                                    self.remove_editor_tag(&tag);
                                }
                            }
                        });
                    }
                    let suggested_tags = available_snippet_tags(&self.entries, &[])
                        .into_iter()
                        .filter(|tag| {
                            !self
                                .tags
                                .iter()
                                .any(|current| current.to_lowercase() == tag.to_lowercase())
                        })
                        .collect::<Vec<_>>();
                    let mut selected_tags_to_add = Vec::new();
                    if !suggested_tags.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.small("Add existing:");
                            for (suggested_index, tag) in suggested_tags.into_iter().enumerate() {
                                let response = ui.selectable_label(false, tag.as_str());
                                apply_select_focus(
                                    &response,
                                    register_snippet_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("author/suggested-tag/{suggested_index}"),
                                        "Add existing snippet tag",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    ),
                                );
                                if response.clicked() && authorized_owner_pass {
                                    selected_tags_to_add.push(tag);
                                }
                            }
                        });
                    }
                    for tag in selected_tags_to_add {
                        if let Err(error) = self.add_editor_tag_value(&tag) {
                            self.tag_error = Some(error.to_owned());
                        }
                    }
                    if let Some(error) = &self.tag_error {
                        semantic_status(ui, StatusTone::Error, "Tag could not be added", Some(error));
                    }
                    secondary_help(
                        ui,
                        "Automatic values: {{sys.date}}, {{sys.time}}, {{sys.clipboard}}, {{sys.window_title}}. Date/time default to %Y-%m-%d and %H:%M:%S; add :<format> to customize. Unavailable clipboard/title values are empty. {{date}} remains an ordinary prompted field.",
                    );
                    compact_section_heading(ui, "Text", None);
                    let action_height = ui.spacing().interact_size.y
                        + ui.spacing().button_padding.y * 2.0
                        + ui.spacing().item_spacing.y;
                    let editor_height = (ui.available_height() - action_height).max(120.0);
                    let font_id = egui::TextStyle::Body.resolve(ui.style());
                    let line_height = ui.ctx().fonts_mut(|fonts| fonts.row_height(&font_id));
                    let desired_rows =
                        (editor_height / line_height).floor().clamp(4.0, 48.0) as usize;
                    let mut body_changed = false;
                    if self.body_revealed {
                        let mut body_shadow;
                        let body_value = if authorized_owner_pass {
                            &mut self.text
                        } else {
                            body_shadow = self.text.clone();
                            &mut body_shadow
                        };
                        egui::ScrollArea::both()
                            .id_salt("snippet_dialog_body")
                            .auto_shrink([false, false])
                            .max_height(editor_height)
                            .show(ui, |ui| {
                                let response = ui.add(
                                    egui::TextEdit::multiline(body_value)
                                        .id_salt(body_editor_id_source(
                                            self.body_edit_session,
                                        ))
                                        .desired_width(f32::INFINITY)
                                        .desired_rows(desired_rows),
                                );
                                #[cfg(test)]
                                {
                                    self.body_editor_widget_id = Some(response.id);
                                }
                                apply_select_focus(
                                    &response,
                                    register_snippet_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("author/body/{}", self.body_edit_session),
                                        "Snippet text",
                                        crate::gui::jump_mode::TargetKind::TextEntry,
                                        &mut visual_order,
                                    ),
                                );
                                body_changed = response.changed();
                            });
                    } else {
                        ui.label("Contents hidden");
                        let reveal = ui.button("Reveal to Edit");
                        apply_select_focus(
                            &reveal,
                            register_snippet_target(
                                scope,
                                ui,
                                &reveal,
                                "author/reveal-contents",
                                "Reveal snippet contents",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                        if reveal.clicked() && authorized_owner_pass {
                            self.reveal_contents();
                        }
                    }
                    if body_changed && authorized_owner_pass {
                        self.inline_error = None;
                        self.refresh_prompted_fields();
                    }
                    self.show_prompt_field_settings(
                        ui,
                        scope,
                        &mut visual_order,
                        authorized_owner_pass,
                    );
                    ui.horizontal(|ui| {
                        let save = ui.button("Save");
                        apply_select_focus(
                            &save,
                            register_snippet_target(
                                scope,
                                ui,
                                &save,
                                "author/save",
                                "Save snippet",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                        if with_button_emphasis(ui, save, ButtonEmphasis::Primary).clicked()
                            && authorized_owner_pass
                        {
                            if self.alias.trim().is_empty()
                                || (!self.prompt_for_fields && self.text.trim().is_empty())
                            {
                                report_required_fields();
                            } else {
                                save_request = true;
                            }
                        }
                        let preview = ui.add_enabled(
                            self.can_preview_editor_draft(),
                            egui::Button::new("Test / Preview"),
                        );
                        apply_select_focus(
                            &preview,
                            register_snippet_target(
                                scope,
                                ui,
                                &preview,
                                "author/test-preview",
                                "Test snippet preview",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                        if with_button_emphasis(ui, preview, ButtonEmphasis::Primary).clicked()
                            && authorized_owner_pass
                        {
                            preview_request = true;
                        }
                        let cancel = ui.button("Cancel");
                        apply_select_focus(
                            &cancel,
                            register_snippet_target(
                                scope,
                                ui,
                                &cancel,
                                "author/cancel",
                                "Cancel snippet edit",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                        if cancel.clicked() && authorized_owner_pass {
                            self.cancel_editor();
                        }
                    });
                    if !self.prompt_for_fields {
                        secondary_help(
                            ui,
                            "Plain text stays literal; include an automatic value to preview a plain snippet. Test / Preview resolves a draft without saving or copying.",
                        );
                    } else if !self.body_revealed {
                        secondary_help(ui, "Reveal contents before testing this prompted snippet.");
                    }
                    if let Some(error) = &self.inline_error {
                        semantic_status(ui, StatusTone::Error, "Snippet could not be saved", Some(error));
                    }
                } else {
                    compact_section_heading(
                        ui,
                        "Saved snippets",
                        Some("Review, edit, or test your saved snippets."),
                    );
                    let mut filter_shadow;
                    let clear_filters_enabled =
                        !self.filter.is_empty() || !self.selected_tag_filters.is_empty();
                    let filter_value = if authorized_owner_pass {
                        &mut self.filter
                    } else {
                        filter_shadow = self.filter.clone();
                        &mut filter_shadow
                    };
                    let mut filter_changed = false;
                    let mut clear_filters_requested = false;
                    ui.horizontal(|ui| {
                        ui.label("Filter");
                        let clear_filter_width = 116.0;
                        let filter_spacing = ui.spacing().item_spacing.x;
                        let filter_width =
                            (ui.available_width() - clear_filter_width - filter_spacing).max(80.0);
                        let filter_response = ui.add_sized(
                                [filter_width, ui.spacing().interact_size.y],
                                egui::TextEdit::singleline(filter_value)
                                    .desired_width(f32::INFINITY),
                            );
                        apply_select_focus(
                            &filter_response,
                            register_snippet_target(
                                scope,
                                ui,
                                &filter_response,
                                "list/filter",
                                "Filter snippets",
                                crate::gui::jump_mode::TargetKind::TextEntry,
                                &mut visual_order,
                            ),
                        );
                        filter_changed = filter_response.changed();
                        let clear_filters = ui.add_enabled(
                                clear_filters_enabled,
                                egui::Button::new("Clear Filters").min_size(egui::vec2(
                                    clear_filter_width,
                                    ui.spacing().interact_size.y
                                        + ui.spacing().button_padding.y * 2.0,
                                )),
                            );
                        apply_select_focus(
                            &clear_filters,
                            register_snippet_target(
                                scope,
                                ui,
                                &clear_filters,
                                "list/clear-filters",
                                "Clear snippet filters",
                                crate::gui::jump_mode::TargetKind::Button,
                                &mut visual_order,
                            ),
                        );
                        clear_filters_requested = clear_filters.clicked();
                    });
                    if authorized_owner_pass {
                        if clear_filters_requested {
                            self.clear_filter_state();
                        } else if filter_changed {
                            self.filter_changed();
                        }
                    }
                    let tag_choices = available_snippet_tags(&self.entries, &self.selected_tag_filters);
                    let mut toggled_tags = Vec::new();
                    if !tag_choices.is_empty() {
                        ui.horizontal_wrapped(|ui| {
                            ui.label("Tags:");
                            for (tag_index, tag) in tag_choices.into_iter().enumerate() {
                                let selected = self
                                    .selected_tag_filters
                                    .iter()
                                    .any(|current| current.to_lowercase() == tag.to_lowercase());
                                let response = ui.selectable_label(selected, tag.as_str());
                                apply_select_focus(
                                    &response,
                                    register_snippet_target(
                                        scope,
                                        ui,
                                        &response,
                                        format!("list/tag-filter/{tag_index}"),
                                        "Filter by snippet tag",
                                        crate::gui::jump_mode::TargetKind::Selectable,
                                        &mut visual_order,
                                    ),
                                );
                                if response.clicked() && authorized_owner_pass {
                                    toggled_tags.push(tag);
                                }
                            }
                        });
                    }
                    for tag in toggled_tags {
                        self.toggle_tag_filter(&tag);
                    }
                    let matching_indices = matching_snippet_indices_with_tags(
                        &self.entries,
                        &self.filter,
                        &self.selected_tag_filters,
                    );
                    if authorized_owner_pass
                        && self
                            .selected_entry_index
                            .is_some_and(|selected| !matching_indices.contains(&selected))
                    {
                        self.selected_entry_index = None;
                    }
                    let count_label = if self.selected_tag_filters.is_empty() {
                        format!("{} of {} snippets", matching_indices.len(), self.entries.len())
                    } else {
                        format!(
                            "{} of {} snippets · all selected tags required",
                            matching_indices.len(),
                            self.entries.len()
                        )
                    };
                    secondary_help(ui, &count_label);
                    if let Some(error) = &self.inline_error {
                        semantic_status(ui, StatusTone::Error, "Snippet could not be updated", Some(error));
                    }
                    if matching_indices.is_empty() {
                        ui.label("No snippets match the current filters");
                    } else {
                        let footer_height = (ui.spacing().interact_size.y
                            + ui.spacing().button_padding.y * 2.0
                            + ui.spacing().item_spacing.y)
                            * 2.0;
                        let list_height = (ui.available_height() - footer_height).max(100.0);
                        egui::ScrollArea::both()
                            .id_salt("snippet_dialog_list")
                            .auto_shrink([false, false])
                            .max_height(list_height)
                            .show(ui, |ui| {
                                for idx in matching_indices {
                                    let entry = self.entries[idx].clone();
                                    ui.push_id(("snippet-row", self.snapshot_generation, idx), |ui| {
                                        ui.horizontal(|ui| {
                                            if entry.hide_contents {
                                                ui.small("Hidden")
                                                    .on_hover_text("Contents are hidden in previews.");
                                            }
                                            let spacing = ui.spacing().item_spacing.x;
                                            let row_height = ui.spacing().interact_size.y
                                                + ui.spacing().button_padding.y * 2.0;
                                            let edit_width = 80.0;
                                            let remove_width = 72.0;
                                            let preview_width = (ui.available_width()
                                                - edit_width
                                                - remove_width
                                                - spacing * 2.0)
                                                .max(0.0);
                                            let preview = format!(
                                                "{}: {}",
                                                single_line_alias(&entry.alias),
                                                snippet_preview_text(&entry)
                                            );
                                            let (preview_rect, preview_response) = ui.allocate_exact_size(
                                                egui::vec2(preview_width, row_height),
                                                egui::Sense::click(),
                                            );
                                            if self.selected_entry_index == Some(idx) {
                                                ui.painter().rect_filled(
                                                    preview_rect,
                                                    ui.visuals().widgets.inactive.corner_radius,
                                                    ui.visuals().selection.bg_fill,
                                                );
                                            }
                                            let text_color = ui.visuals().text_color();
                                            let mut layout_job = egui::text::LayoutJob::simple_singleline(
                                                preview,
                                                egui::TextStyle::Body.resolve(ui.style()),
                                                text_color,
                                            );
                                            layout_job.wrap.max_width = preview_width;
                                            layout_job.wrap.max_rows = 1;
                                            layout_job.wrap.break_anywhere = true;
                                            let galley = ui.ctx().fonts_mut(|fonts| fonts.layout_job(layout_job));
                                            let text_position = egui::pos2(
                                                preview_rect.left(),
                                                preview_rect.center().y - galley.size().y / 2.0,
                                            );
                                            ui.painter().with_clip_rect(preview_rect).galley(
                                                text_position,
                                                galley,
                                                text_color,
                                            );

                                            let preview_identity = format!("entry/{idx}/preview");
                                            let preview_target_id = scope
                                                .map(|scope| scope.target_id(&preview_identity));
                                            let directive = register_snippet_target(
                                                scope,
                                                ui,
                                                &preview_response,
                                                &preview_identity,
                                                "Snippet entry",
                                                crate::gui::jump_mode::TargetKind::Action,
                                                &mut visual_order,
                                            );
                                            if preview_response.clicked() && authorized_owner_pass {
                                                self.selected_entry_index = Some(idx);
                                                preview_response.request_focus();
                                            }
                                            if directive == Some(crate::gui::jump_mode::ui::FocusDirective::Select)
                                                && authorized_owner_pass
                                            {
                                                self.selected_entry_index = Some(idx);
                                                preview_response.request_focus();
                                                if let (Some(scope), Some(target_id)) =
                                                    (scope, preview_target_id.as_ref())
                                                {
                                                    scope.record_select_focus_owner(
                                                        target_id,
                                                        preview_response.id,
                                                    );
                                                }
                                            }

                                            let confirmation_instance = self
                                                .pending_removal
                                                .as_ref()
                                                .filter(|pending| pending.index == idx)
                                                .map(|pending| pending.confirmation_instance);
                                            if let Some(confirmation_instance) = confirmation_instance {
                                                let confirm = ui
                                                    .push_id(
                                                        (
                                                            "snippet-removal-confirm",
                                                            confirmation_instance,
                                                        ),
                                                        |ui| {
                                                            ui.add_sized(
                                                                egui::vec2(edit_width, row_height),
                                                                egui::Button::new("Confirm"),
                                                            )
                                                        },
                                                    )
                                                    .inner;
                                                apply_select_focus(
                                                    &confirm,
                                                    register_snippet_target(
                                                        removal_scope,
                                                        ui,
                                                        &confirm,
                                                        "removal/confirm",
                                                        "Confirm snippet removal",
                                                        crate::gui::jump_mode::TargetKind::Button,
                                                        &mut visual_order,
                                                    ),
                                                );
                                                if with_button_emphasis(
                                                    ui,
                                                    confirm,
                                                    ButtonEmphasis::Destructive,
                                                )
                                                .clicked()
                                                    && authorized_owner_pass
                                                {
                                                    confirm_removal = true;
                                                }
                                                let cancel = ui
                                                    .push_id(
                                                        (
                                                            "snippet-removal-cancel",
                                                            confirmation_instance,
                                                        ),
                                                        |ui| {
                                                            ui.add_sized(
                                                                egui::vec2(remove_width, row_height),
                                                                egui::Button::new("Cancel"),
                                                            )
                                                        },
                                                    )
                                                    .inner;
                                                apply_select_focus(
                                                    &cancel,
                                                    register_snippet_target(
                                                        removal_scope,
                                                        ui,
                                                        &cancel,
                                                        "removal/cancel",
                                                        "Cancel snippet removal",
                                                        crate::gui::jump_mode::TargetKind::Button,
                                                        &mut visual_order,
                                                    ),
                                                );
                                                if cancel.clicked() && authorized_owner_pass {
                                                    self.cancel_removal();
                                                }
                                            } else {
                                                let edit = ui.add_sized(
                                                    egui::vec2(edit_width, row_height),
                                                    egui::Button::new("Edit"),
                                                );
                                                apply_select_focus(
                                                    &edit,
                                                    register_snippet_target(
                                                        scope,
                                                        ui,
                                                        &edit,
                                                        format!("entry/{idx}/edit"),
                                                        "Edit snippet",
                                                        crate::gui::jump_mode::TargetKind::Button,
                                                        &mut visual_order,
                                                    ),
                                                );
                                                if edit.clicked() && authorized_owner_pass {
                                                    self.begin_existing(idx);
                                                }
                                                let remove = ui.add_sized(
                                                    egui::vec2(remove_width, row_height),
                                                    egui::Button::new("Remove"),
                                                );
                                                apply_select_focus(
                                                    &remove,
                                                    register_snippet_target(
                                                        scope,
                                                        ui,
                                                        &remove,
                                                        format!("entry/{idx}/remove"),
                                                        "Remove snippet",
                                                        crate::gui::jump_mode::TargetKind::Button,
                                                        &mut visual_order,
                                                    ),
                                                );
                                                if remove.clicked() && authorized_owner_pass {
                                                    self.request_removal(idx);
                                                }
                                            }
                                        });
                                        if !entry.tags.is_empty() {
                                            ui.horizontal_wrapped(|ui| {
                                                ui.add_space(4.0);
                                                for tag in &entry.tags {
                                                    ui.small(format!("#{tag}"));
                                                }
                                            });
                                        }
                                    });
                                }
                            });
                    }
                    let add = ui.button("Add Snippet");
                    apply_select_focus(
                        &add,
                        register_snippet_target(
                            scope,
                            ui,
                            &add,
                            "list/add",
                            "Add snippet",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        ),
                    );
                    if with_button_emphasis(ui, add, ButtonEmphasis::Primary).clicked()
                        && authorized_owner_pass
                    {
                        self.begin_new("");
                    }
                    let close_response = ui.button("Close");
                    apply_select_focus(
                        &close_response,
                        register_snippet_target(
                            scope,
                            ui,
                            &close_response,
                            "list/close",
                            "Close snippets",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        ),
                    );
                    if close_response.clicked() && authorized_owner_pass {
                        close = true;
                    }
                    }
                });
                #[cfg(test)]
                {
                    self.contents_scroll_id = Some(_contents.id);
                }
                });
            });
        if self.view_revision != initial_view_revision
            && let Some(scope) = jump_mode_scope
        {
            scope.discard_registered_targets();
        }
        if close {
            window_open = false;
        }
        (window_open, save_request, confirm_removal, preview_request)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AliasValidation, CommitFailure, CommitRejection, SnippetDialog, body_editor_id_source,
        matches_snippet_filter, matching_snippet_indices, matching_snippet_indices_with_tags,
        single_line_alias,
    };
    use crate::plugins::snippets::{
        SnippetEntry, SnippetFieldDefinition, SnippetInputKind, load_snippets, save_snippets,
        snippets_version,
    };
    use eframe::egui;
    use std::sync::{Arc, atomic::AtomicBool};

    fn snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.to_string(),
            text: text.to_string(),
            hide_contents: false,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: Vec::new(),
        }
    }

    fn prompted_snippet(alias: &str, text: &str) -> SnippetEntry {
        SnippetEntry {
            alias: alias.into(),
            text: text.into(),
            hide_contents: true,
            prompt_for_fields: true,
            fields: vec![SnippetFieldDefinition {
                name: "name".into(),
                label: "Preferred name".into(),
                default_value: "Ada".into(),
                required: false,
                input_kind: SnippetInputKind::Multiline,
            }],
            tags: Vec::new(),
        }
    }

    fn preview_app(ctx: &egui::Context, directory: &tempfile::TempDir) -> crate::gui::LauncherApp {
        let path = |name: &str| directory.path().join(name).to_string_lossy().into_owned();
        let mut settings = crate::settings::Settings::default();
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        crate::gui::LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            crate::plugin::PluginManager::new(),
            path("actions.json"),
            path("settings.json"),
            settings,
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn dialog_frame(
        ctx: &egui::Context,
        dialog: &mut SnippetDialog,
        events: Vec<egui::Event>,
    ) -> egui::FullOutput {
        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                events,
                ..Default::default()
            },
            |ctx| {
                dialog.show_window(ctx, || panic!("unexpected field validation"));
            },
        )
    }

    #[test]
    fn jump_mode_snippets_lifecycle_nonauthorized_render_pass_keeps_draft_and_store_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = snippet("draft", "Original λ\n{{name}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();

        let ctx = egui::Context::default();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        let draft_before = (
            dialog.alias.clone(),
            dialog.text.clone(),
            dialog.entries.clone(),
        );
        dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
        let body_id = dialog
            .test_body_editor_widget_id()
            .expect("the actual revealed body response has an egui ID");
        ctx.memory_mut(|memory| memory.request_focus(body_id));
        let selected_before = dialog.test_selected_entry_index();

        ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(1200.0, 900.0),
                )),
                focused: true,
                events: vec![egui::Event::Text("discarded draft text".into())],
                ..Default::default()
            },
            |ctx| {
                dialog.show_window_with_jump_mode(ctx, None, true, false, || {
                    panic!("unexpected required-field action")
                });
            },
        )
        .drop_without_applying_deltas();

        assert_eq!(
            (
                dialog.alias.clone(),
                dialog.text.clone(),
                dialog.entries.clone()
            ),
            draft_before,
            "the noncommitting pass edits only its local widget shadow"
        );
        assert_eq!(dialog.test_selected_entry_index(), selected_before);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert!(dialog.test_is_editing());
    }

    #[test]
    fn preview_uses_unsaved_draft_without_mutating_editor_or_persisted_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("saved", "Saved {{name}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();

        let ctx = egui::Context::default();
        let mut app = preview_app(&ctx, &directory);
        app.test_skip_history_persistence = true;
        app.snippet_dialog
            .open_edit_from(path.to_str().unwrap(), "saved");
        app.focus_panel(crate::gui::Panel::SnippetDialog);
        app.update_panel_stack();
        assert!(!app.snippet_dialog.body_revealed);
        app.snippet_dialog.reveal_contents();
        app.snippet_dialog.alias = "draft alias".into();
        app.snippet_dialog.text = "Draft {{ticket}}\r\nHello {{name}}".into();
        app.snippet_dialog.fields[0].label = "Draft recipient".into();
        app.snippet_dialog.fields[0].default_value = "Draft Ada".into();
        app.snippet_dialog.fields[0].required = true;
        let mut orphan = SnippetFieldDefinition::new("orphan");
        orphan.label = "Retained orphan".into();
        orphan.default_value = "Do not display".into();
        app.snippet_dialog.fields.push(orphan.clone());
        app.snippet_dialog.refresh_prompted_fields();
        let ticket = app.snippet_dialog.field_mut("ticket").unwrap();
        ticket.label = "Ticket details".into();
        ticket.default_value = "T-42".into();
        ticket.required = false;
        ticket.input_kind = SnippetInputKind::Multiline;

        dialog_frame(&ctx, &mut app.snippet_dialog, Vec::new()).drop_without_applying_deltas();
        let editor_id = app
            .snippet_dialog
            .body_editor_widget_id
            .expect("the revealed authoring editor has a rendered widget id");
        let mut editor_widget_state = egui::text_edit::TextEditState::load(&ctx, editor_id)
            .expect("the revealed authoring editor has rendered widget state");
        editor_widget_state
            .cursor
            .set_char_range(Some(egui::text::CCursorRange::one(
                egui::text::CCursor::new(6),
            )));
        let editor_cursor_before = editor_widget_state.cursor.char_range().unwrap();
        editor_widget_state.store(&ctx, editor_id);

        let editor_state = |dialog: &SnippetDialog| {
            (
                dialog.edit_idx,
                dialog.body_edit_session,
                dialog.alias.clone(),
                dialog.text.clone(),
                dialog.hide_contents,
                dialog.body_revealed,
                dialog.prompt_for_fields,
                dialog.fields.clone(),
                dialog.discovered_field_keys.clone(),
            )
        };
        let editor_before = editor_state(&app.snippet_dialog);
        let usage_before = app.usage.clone();
        let history_before = app.test_recorded_history_queries.clone();
        let draft = app.snippet_dialog.preview_editor_draft().unwrap();
        assert_eq!(draft.alias, "draft alias");
        assert_eq!(draft.text, "Draft {{ticket}}\r\nHello {{name}}");
        assert_eq!(
            draft
                .fields
                .iter()
                .map(|field| field.name.as_str())
                .collect::<Vec<_>>(),
            ["ticket", "name"]
        );
        assert!(!draft.fields.iter().any(|field| field.name == orphan.name));
        assert_eq!(app.snippet_dialog.fields, editor_before.7);

        app.begin_snippet_preview(draft.clone()).unwrap();
        app.update_panel_stack();
        assert_eq!(
            egui::text_edit::TextEditState::load(&ctx, editor_id)
                .expect("preview must retain authoring editor state")
                .cursor
                .char_range(),
            Some(editor_cursor_before)
        );
        assert!(app.snippet_prompt_dialog.is_preview_only());
        assert!(app.is_panel_open(crate::gui::Panel::SnippetPromptDialog));
        let first_generation = app.snippet_prompt_dialog.session().unwrap().generation;
        assert_eq!(
            app.snippet_prompt_dialog
                .session()
                .unwrap()
                .values
                .get("name")
                .map(String::as_str),
            Some("Draft Ada")
        );
        assert_eq!(
            app.snippet_prompt_dialog
                .session()
                .unwrap()
                .values
                .get("ticket")
                .map(String::as_str),
            Some("T-42")
        );
        app.snippet_prompt_dialog
            .set_value("name", "Trial recipient".into());
        app.snippet_prompt_dialog
            .set_value("ticket", "Trial line one\nline two".into());
        assert_eq!(
            app.snippet_prompt_dialog.preview().unwrap().text,
            "Draft Trial line one\nline two\r\nHello Trial recipient"
        );
        assert_eq!(editor_state(&app.snippet_dialog), editor_before);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_recorded_history_queries, history_before);

        assert_eq!(app.cancel_snippet_prompt(), Some(draft.clone()));
        app.update_panel_stack();
        assert!(app.snippet_prompt_dialog.session().is_none());
        assert_eq!(
            egui::text_edit::TextEditState::load(&ctx, editor_id)
                .expect("returning from preview must retain authoring editor state")
                .cursor
                .char_range(),
            Some(editor_cursor_before)
        );
        assert_eq!(
            app.panel_stack.last(),
            Some(&crate::gui::Panel::SnippetDialog)
        );
        assert_eq!(editor_state(&app.snippet_dialog), editor_before);

        let reopened_draft = app.snippet_dialog.preview_editor_draft().unwrap();
        app.begin_snippet_preview(reopened_draft).unwrap();
        let reopened = app.snippet_prompt_dialog.session().unwrap();
        assert_ne!(reopened.generation, first_generation);
        assert_eq!(
            reopened.values.get("name").map(String::as_str),
            Some("Draft Ada")
        );
        assert_eq!(
            reopened.values.get("ticket").map(String::as_str),
            Some("T-42")
        );
        assert!(
            !reopened
                .values
                .values()
                .any(|value| value.contains("Trial"))
        );
        assert_eq!(app.cancel_snippet_prompt(), Some(draft));
        app.update_panel_stack();

        assert_eq!(editor_state(&app.snippet_dialog), editor_before);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert_eq!(app.usage, usage_before);
        assert_eq!(app.test_recorded_history_queries, history_before);
    }

    #[test]
    fn plain_system_variable_editor_preview_is_read_only_and_reveal_gated() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = SnippetEntry {
            alias: "saved".into(),
            text: "Saved {{sys.date}}".into(),
            hide_contents: true,
            prompt_for_fields: false,
            fields: Vec::new(),
            tags: vec!["automation".into()],
        };
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();

        let ctx = egui::Context::default();
        let mut app = preview_app(&ctx, &directory);
        app.snippet_dialog
            .open_edit_from(path.to_str().unwrap(), "saved");
        assert!(!app.snippet_dialog.body_revealed);
        app.snippet_dialog.set_prompt_for_fields(false);
        app.snippet_dialog.text = "Draft {{sys.date}}".into();
        assert!(!app.snippet_dialog.can_preview_editor_draft());
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        assert!(
            app.snippet_dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("Reveal contents")
        );
        assert!(!app.snippet_prompt_dialog.is_open());

        app.snippet_dialog.reveal_contents();
        app.snippet_dialog.alias = "draft alias".into();
        app.snippet_dialog.tags.push("draft".into());
        assert!(app.snippet_dialog.can_preview_editor_draft());
        let draft = app.snippet_dialog.preview_editor_draft().unwrap();
        assert_eq!(draft.text, "Draft {{sys.date}}");
        assert_eq!(
            draft.tags,
            vec!["automation".to_owned(), "draft".to_owned()]
        );
        app.begin_snippet_preview(draft.clone()).unwrap();
        let session = app.snippet_prompt_dialog.session().unwrap();
        assert!(app.snippet_prompt_dialog.is_preview_only());
        assert!(session.prepared.fields.is_empty());
        assert!(
            app.snippet_prompt_dialog
                .preview()
                .unwrap()
                .text
                .starts_with("Draft ")
        );

        let copied = std::cell::Cell::new(false);
        assert_eq!(
            app.submit_snippet_prompt_with_test_backends(
                |_| Err(()),
                |_| {
                    copied.set(true);
                    Ok(())
                },
            ),
            Err(crate::gui::snippet_prompt_dialog::SnippetPromptError::PreviewOnly)
        );
        assert!(!copied.get());
        assert_eq!(app.cancel_snippet_prompt(), Some(draft));
        assert_eq!(app.snippet_dialog.text, "Draft {{sys.date}}");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);

        app.snippet_dialog.alias = "draft alias".into();
        app.snippet_dialog.text = "Unknown {{sys.future}}".into();
        assert!(app.snippet_dialog.can_preview_editor_draft());
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        assert!(
            app.snippet_dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("unknown system variable")
        );
        assert!(!app.snippet_prompt_dialog.is_open());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
    }

    #[test]
    fn preview_rejects_hidden_invalid_duplicate_and_plain_drafts_in_the_editor() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("saved", "Saved {{name}}");
        let duplicate = prompted_snippet("duplicate", "Other {{name}}");
        save_snippets(path.to_str().unwrap(), &[original.clone(), duplicate]).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();

        let ctx = egui::Context::default();
        let mut app = preview_app(&ctx, &directory);
        app.snippet_dialog
            .open_edit_from(path.to_str().unwrap(), "saved");
        assert!(!app.snippet_dialog.body_revealed);
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        assert!(
            app.snippet_dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("Reveal contents")
        );

        app.snippet_dialog.reveal_contents();
        app.snippet_dialog.alias = "draft alias".into();
        app.snippet_dialog.text = "Private malformed {{unfinished".into();
        app.snippet_dialog.refresh_prompted_fields();
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        let error = app.snippet_dialog.inline_error.as_deref().unwrap();
        assert!(error.contains("invalid template"));
        assert!(!error.contains("Private malformed"));
        assert_eq!(app.snippet_dialog.text, "Private malformed {{unfinished");

        app.snippet_dialog.text = "Hello {{name}}".into();
        app.snippet_dialog.refresh_prompted_fields();
        app.snippet_dialog.alias = "duplicate".into();
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        assert!(
            app.snippet_dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("exact alias already exists")
        );

        app.snippet_dialog.alias = "draft alias".into();
        app.snippet_dialog.set_prompt_for_fields(false);
        app.snippet_dialog.text = "Literal {{unfinished".into();
        assert!(app.snippet_dialog.preview_editor_draft().is_none());
        assert!(
            app.snippet_dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("Plain snippets are literal")
        );
        assert!(!app.snippet_prompt_dialog.is_open());
        assert!(
            !app.panel_stack
                .contains(&crate::gui::Panel::SnippetPromptDialog)
        );
        assert_eq!(app.snippet_dialog.alias, "draft alias");
        assert_eq!(app.snippet_dialog.text, "Literal {{unfinished");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert!(app.test_recorded_history_queries.is_empty());
    }

    fn dialog_rect(ctx: &egui::Context) -> egui::Rect {
        ctx.memory(|memory| memory.area_rect(egui::Id::new(Some("Snippets"))).unwrap())
    }

    #[test]
    fn snippet_dialog_size_is_independent_of_contents_and_filter() {
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut dialog = SnippetDialog {
            open: true,
            entries: vec![snippet("short", "body")],
            ..Default::default()
        };
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
        }
        let size = dialog_rect(&ctx).size();
        assert!(size.x < 650.0 && size.y < 550.0, "{size:?}");

        dialog.entries = vec![snippet(&"alias".repeat(2000), &"body λ\n".repeat(2000))];
        for _ in 0..20 {
            let output = dialog_frame(&ctx, &mut dialog, Vec::new());
            assert_eq!(dialog_rect(&ctx).size(), size);
            for shape in &output.shapes {
                if let egui::epaint::Shape::Text(text) = &shape.shape
                    && matches!(text.galley.job.text.as_str(), "Edit" | "Remove")
                {
                    assert!(text.pos.x + text.galley.size().x < dialog_rect(&ctx).right());
                }
            }
            assert!(output.viewport_output.values().all(|viewport| {
                !viewport
                    .commands
                    .iter()
                    .any(|command| matches!(command, egui::ViewportCommand::InnerSize(_)))
            }));
            output.drop_without_applying_deltas();
        }
        dialog.filter = "no match".into();
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
        dialog.filter.clear();
        dialog.entries[0].hide_contents = true;
        let output = dialog_frame(&ctx, &mut dialog, Vec::new());
        assert_eq!(dialog_rect(&ctx).size(), size);
        assert!(output.shapes.iter().all(|shape| {
            !matches!(&shape.shape, egui::epaint::Shape::Text(text) if text.galley.job.text.contains("body λ"))
        }));
        dialog.begin_existing(0);
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
        dialog.reveal_contents();
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
            assert_eq!(dialog_rect(&ctx).size(), size);
        }
        output.drop_without_applying_deltas();
    }

    #[test]
    fn snippet_dialog_user_resize_and_scrolling_preserve_window_bounds() {
        let ctx = egui::Context::default();
        ctx.style_mut_of(ctx.theme(), |style| style.animation_time = 0.0);
        let mut dialog = SnippetDialog {
            open: true,
            entries: (0..80)
                .map(|i| snippet(&format!("row-{i}"), "body"))
                .collect(),
            ..Default::default()
        };
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
        }
        let original = dialog_rect(&ctx);
        let corner = original.max - egui::vec2(2.0, 2.0);
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(corner),
                egui::Event::PointerButton {
                    pos: corner,
                    button: egui::PointerButton::Primary,
                    pressed: true,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .drop_without_applying_deltas();
        let resized_corner = original.min + egui::vec2(240.0, 260.0);
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![egui::Event::PointerMoved(resized_corner)],
        )
        .drop_without_applying_deltas();
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![egui::Event::PointerButton {
                pos: resized_corner,
                button: egui::PointerButton::Primary,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        )
        .drop_without_applying_deltas();
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
        }
        let resized = dialog_rect(&ctx);
        assert!(
            resized.width() < 280.0 && resized.height() < 300.0,
            "{resized:?}"
        );

        let text_position = |output: &egui::FullOutput, prefix: &str| {
            output
                .shapes
                .iter()
                .find_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) if text.galley.job.text.starts_with(prefix) => {
                        Some(text.pos)
                    }
                    _ => None,
                })
                .unwrap_or_else(|| panic!("no visible text starting with {prefix}"))
        };
        let before = dialog_frame(&ctx, &mut dialog, Vec::new());
        let filter = text_position(&before, "Filter");
        let count = text_position(&before, "80 of 80 snippets");
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(filter),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(-100.0, 0.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .drop_without_applying_deltas();
        let after = dialog_frame(&ctx, &mut dialog, Vec::new());
        assert!(text_position(&after, "80 of 80 snippets").x < count.x);
        assert_eq!(dialog_rect(&ctx), resized);

        before.drop_without_applying_deltas();
        let before = dialog_frame(&ctx, &mut dialog, Vec::new());
        let first_visible_row = |output: &egui::FullOutput| {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => text
                        .galley
                        .job
                        .text
                        .strip_prefix("row-")
                        .and_then(|text| text.split_once(':'))
                        .and_then(|(index, _)| index.parse::<usize>().ok()),
                    _ => None,
                })
                .min()
                .expect("at least one list row remains visible")
        };
        assert_eq!(first_visible_row(&before), 0);
        let first_row_position = text_position(&before, "row-0:");
        dialog_frame(
            &ctx,
            &mut dialog,
            vec![
                egui::Event::PointerMoved(resized.center()),
                egui::Event::MouseWheel {
                    unit: egui::MouseWheelUnit::Point,
                    delta: egui::vec2(0.0, -100.0),
                    phase: egui::TouchPhase::Move,
                    modifiers: egui::Modifiers::NONE,
                },
            ],
        )
        .drop_without_applying_deltas();
        after.drop_without_applying_deltas();
        let after = dialog_frame(&ctx, &mut dialog, Vec::new());
        // Smooth scrolling can move part of a row before it leaves the clip rect.
        assert!(
            first_visible_row(&after) > 0
                || text_position(&after, "row-0:").y < first_row_position.y,
            "list did not scroll vertically"
        );
        assert_eq!(dialog_rect(&ctx), resized);
        dialog.end_session();
        dialog.open = true;
        for _ in 0..4 {
            dialog_frame(&ctx, &mut dialog, Vec::new()).drop_without_applying_deltas();
        }
        assert_eq!(dialog_rect(&ctx), resized);
        after.drop_without_applying_deltas();
        before.drop_without_applying_deltas();
    }

    #[test]
    fn empty_filter_matches_all() {
        let entry = snippet("greet", "hello world");
        assert!(matches_snippet_filter(&entry, ""));
        assert!(matches_snippet_filter(&entry, "   "));
    }

    #[test]
    fn alias_only_match() {
        let entry = snippet("git-status", "show status");
        assert!(matches_snippet_filter(&entry, "status"));
    }

    #[test]
    fn text_only_match() {
        let entry = snippet("gs", "Git Status Output");
        assert!(matches_snippet_filter(&entry, "output"));
    }

    #[test]
    fn case_insensitive_matching() {
        let entry = snippet("DockerUp", "Start Containers");
        assert!(matches_snippet_filter(&entry, "docker"));
        assert!(matches_snippet_filter(&entry, "CONTAINERS"));
    }

    #[test]
    fn non_match_behavior() {
        let entry = snippet("deploy", "release production");
        assert!(!matches_snippet_filter(&entry, "staging"));
    }

    #[test]
    fn matching_count_uses_alias_and_hidden_body_filter_predicate() {
        let mut hidden = snippet("private", "launch-token payload");
        hidden.hide_contents = true;
        let entries = vec![snippet("Deploy", "release"), hidden];

        assert_eq!(matching_snippet_indices(&entries, "").len(), 2);
        assert_eq!(matching_snippet_indices(&entries, "DEPLOY"), vec![0]);
        assert_eq!(matching_snippet_indices(&entries, "launch-token"), vec![1]);
        assert!(matching_snippet_indices(&entries, "no match").is_empty());
    }

    #[test]
    fn selected_tag_filters_match_case_insensitively_with_and_semantics() {
        let mut release = snippet("release", "deploy build");
        release.tags = vec!["Work".into(), "High Priority".into()];
        let mut work_only = snippet("work-only", "deploy build");
        work_only.tags = vec!["work".into()];
        let mut priority_only = snippet("priority-only", "deploy build");
        priority_only.tags = vec!["high priority".into()];
        let entries = vec![release, work_only, priority_only];
        let selected = vec!["work".to_owned(), "HIGH PRIORITY".to_owned()];

        assert_eq!(
            matching_snippet_indices_with_tags(&entries, "deploy", &selected),
            vec![0]
        );
        assert_eq!(
            matching_snippet_indices_with_tags(&entries, "WORK", &[]),
            vec![0, 1]
        );
        assert_eq!(
            matching_snippet_indices_with_tags(&entries, "high", &[]),
            vec![0, 2]
        );
        assert!(matching_snippet_indices_with_tags(&entries, "missing", &selected).is_empty());

        let mut dialog = SnippetDialog {
            entries,
            selected_tag_filters: selected,
            ..SnippetDialog::default()
        };
        dialog.request_removal(0);
        dialog.toggle_tag_filter("work");
        assert_eq!(
            dialog.selected_tag_filters,
            vec!["HIGH PRIORITY".to_owned()]
        );
        assert!(dialog.pending_removal.is_none());

        dialog.filter = "deploy".into();
        dialog.clear_filter_state();
        assert!(dialog.filter.is_empty());
        assert!(dialog.selected_tag_filters.is_empty());
    }

    #[test]
    fn saved_list_shows_tag_chips_without_revealing_hidden_body_text() {
        fn painted_text(output: &egui::FullOutput) -> String {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        let mut hidden = snippet("masked", "private body that must stay concealed");
        hidden.hide_contents = true;
        hidden.tags = vec!["release".into(), "high priority".into()];
        let mut dialog = SnippetDialog {
            open: true,
            entries: vec![hidden],
            ..SnippetDialog::default()
        };
        let ctx = egui::Context::default();
        let mut output = dialog_frame(&ctx, &mut dialog, Vec::new());
        for _ in 0..3 {
            output.drop_without_applying_deltas();
            output = dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let painted = painted_text(&output);
        output.drop_without_applying_deltas();

        assert!(painted.contains("release"));
        assert!(painted.contains("high priority"));
        assert!(!painted.contains("private body that must stay concealed"));
    }

    #[test]
    fn alias_preview_normalizes_control_whitespace_without_changing_alias() {
        let entry = snippet("private\nalias", "body");

        assert_eq!(single_line_alias(&entry.alias), "private alias");
        assert_eq!(entry.alias, "private\nalias");
    }

    #[test]
    fn editor_initializes_saved_privacy_and_new_drafts_safely() {
        let hidden = prompted_snippet("masked", "private {{name}}\nline");
        let dialog_entries = vec![hidden.clone(), snippet("visible", "public")];
        let mut dialog = SnippetDialog {
            entries: dialog_entries,
            ..SnippetDialog::default()
        };

        assert!(dialog.begin_existing(0));
        assert!(dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.prompt_for_fields);
        assert_eq!(dialog.text, hidden.text);

        assert!(dialog.begin_existing(1));
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);

        dialog.begin_new("new");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert!(!dialog.prompt_for_fields);
        assert!(dialog.fields.is_empty());
        assert!(dialog.text.is_empty());

        dialog.set_hide_contents(true);
        assert!(dialog.hide_contents);
        assert!(
            dialog.body_revealed,
            "new drafts remain editable while masked"
        );
    }

    #[test]
    fn prompted_field_controls_follow_opt_in_and_masking_state() {
        fn painted_text(output: &egui::FullOutput) -> String {
            output
                .shapes
                .iter()
                .filter_map(|shape| match &shape.shape {
                    egui::epaint::Shape::Text(text) => Some(text.galley.job.text.as_str()),
                    _ => None,
                })
                .collect::<Vec<_>>()
                .join("\n")
        }

        fn field_settings_frame(
            ctx: &egui::Context,
            dialog: &mut SnippetDialog,
        ) -> egui::FullOutput {
            ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(
                        egui::Pos2::ZERO,
                        egui::vec2(1200.0, 900.0),
                    )),
                    ..Default::default()
                },
                |ctx| {
                    egui::CentralPanel::default().show(ctx, |ui| {
                        let mut visual_order = 0;
                        dialog.show_prompt_field_settings(ui, None, &mut visual_order, true);
                    });
                },
            )
        }

        let mut hidden = prompted_snippet("masked", "Private {{secret_key}}");
        hidden.tags = vec!["safe-label".into()];
        hidden.fields[0].name = "secret_key".into();
        hidden.fields[0].label = "PRIVATE LABEL".into();
        hidden.fields[0].default_value = "PRIVATE DEFAULT".into();
        let mut removed = SnippetFieldDefinition::new("removed_key");
        removed.label = "REMOVED LABEL".into();
        removed.default_value = "REMOVED DEFAULT".into();
        hidden.fields.push(removed);
        let mut dialog = SnippetDialog {
            open: true,
            entries: vec![hidden],
            ..Default::default()
        };
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);
        assert!(dialog.open);
        assert!(dialog.prompt_for_fields);

        let ctx = egui::Context::default();
        let mut output = dialog_frame(&ctx, &mut dialog, Vec::new());
        for _ in 0..3 {
            output.drop_without_applying_deltas();
            output = dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let painted = painted_text(&output);
        assert!(painted.contains("Prompt for fields"), "{painted}");
        assert!(painted.contains("Field settings are hidden until contents are revealed."));
        assert!(!painted.contains("secret_key"));
        assert!(!painted.contains("PRIVATE LABEL"));
        assert!(!painted.contains("PRIVATE DEFAULT"));
        assert!(painted.contains("safe-label"));
        assert!(!painted.contains("Private {{secret_key}}"));
        assert!(painted.contains("Tags (1/16)"));
        assert!(painted.contains("Add tag"));

        dialog.reveal_contents();
        for _ in 0..4 {
            output.drop_without_applying_deltas();
            output = dialog_frame(&ctx, &mut dialog, Vec::new());
        }
        let editor_painted = painted_text(&output);
        assert!(editor_painted.contains("Use {{field}}"));
        assert!(editor_painted.contains("\\{{...}}"));
        assert!(editor_painted.contains("{{date}} is an ordinary input, not a computed date."));
        assert!(editor_painted.contains("sys.clipboard"));
        assert!(editor_painted.contains("%Y-%m-%d"));
        assert!(editor_painted.contains("Unavailable clipboard/title values are empty."));

        let field_context = egui::Context::default();
        assert!(dialog.body_revealed);
        assert_eq!(dialog.discovered_field_keys, vec!["secret_key"]);
        assert!(dialog.template_error.is_none());
        field_settings_frame(&field_context, &mut dialog).drop_without_applying_deltas();
        output.drop_without_applying_deltas();
        let output = field_settings_frame(&field_context, &mut dialog);
        let fields_painted = painted_text(&output);
        assert!(fields_painted.contains("secret_key"), "{fields_painted:?}");
        assert!(fields_painted.contains("PRIVATE LABEL"));
        assert!(fields_painted.contains("PRIVATE DEFAULT"));
        assert!(!fields_painted.contains("removed_key"));
        assert!(!fields_painted.contains("REMOVED LABEL"));
        assert!(!fields_painted.contains("REMOVED DEFAULT"));
        assert!(
            dialog
                .fields
                .iter()
                .any(|field| field.name == "removed_key")
        );
        output.drop_without_applying_deltas();
    }

    #[test]
    fn command_edit_path_uses_concealed_state_for_existing_masked_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut hidden = snippet("masked", "private λ\nbody");
        hidden.hide_contents = true;
        save_snippets(
            path.to_str().unwrap(),
            &[hidden.clone(), snippet("visible", "public")],
        )
        .unwrap();
        let mut dialog = SnippetDialog::default();

        dialog.open_edit_from(path.to_str().unwrap(), "masked");
        assert!(dialog.open);
        assert!(dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, hidden.text);

        dialog.open_edit_from(path.to_str().unwrap(), "visible");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert_eq!(dialog.text, "public");

        dialog.open_edit_from(path.to_str().unwrap(), "new alias");
        assert_eq!(dialog.alias, "new alias");
        assert!(!dialog.hide_contents);
        assert!(dialog.body_revealed);
        assert!(dialog.text.is_empty());
    }

    #[test]
    fn body_editor_undo_state_is_isolated_between_edit_sessions() {
        use std::cell::Cell;

        let mut masked = snippet("private", "confidential λ\nsecond line");
        masked.hide_contents = true;
        let mut dialog = SnippetDialog {
            entries: vec![masked, snippet("public", "public body")],
            ..SnippetDialog::default()
        };
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);
        dialog.reveal_contents();

        let context = egui::Context::default();
        let editor_bounds = Cell::new(egui::Rect::NOTHING);
        let frame = |text: &mut String, session: u64, time: f64, events: Vec<egui::Event>| {
            let mut input = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(640.0, 480.0),
                )),
                time: Some(time),
                events,
                ..Default::default()
            };
            input
                .viewports
                .get_mut(&egui::ViewportId::ROOT)
                .expect("root viewport is present in RawInput")
                .inner_rect = input.screen_rect;
            context.run_ui(input, |ctx| {
                egui::CentralPanel::default().show(ctx, |ui| {
                    let response = ui.add(
                        egui::TextEdit::multiline(text)
                            .id_salt(body_editor_id_source(session))
                            .desired_width(300.0)
                            .desired_rows(4),
                    );
                    editor_bounds.set(response.rect);
                });
            })
        };

        let first_session = dialog.body_edit_session;
        frame(&mut dialog.text, first_session, 0.0, Vec::new()).drop_without_applying_deltas();
        let point = editor_bounds.get().left_top() + egui::vec2(12.0, 12.0);
        let pointer_button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut dialog.text,
            first_session,
            0.1,
            vec![egui::Event::PointerMoved(point)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            first_session,
            0.2,
            vec![pointer_button(true)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            first_session,
            0.3,
            vec![pointer_button(false)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            first_session,
            0.4,
            vec![egui::Event::Text(" edited".into())],
        )
        .drop_without_applying_deltas();
        assert_ne!(dialog.text, "confidential λ\nsecond line");

        dialog.set_hide_contents(false);
        dialog.set_hide_contents(true);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.body_edit_session, first_session);
        dialog.reveal_contents();
        assert_eq!(dialog.body_edit_session, first_session);
        dialog.cancel_editor();
        assert!(dialog.begin_existing(1));
        let second_session = dialog.body_edit_session;
        assert_ne!(second_session, first_session);
        assert_eq!(dialog.text, "public body");

        frame(&mut dialog.text, second_session, 0.5, Vec::new()).drop_without_applying_deltas();
        let point = editor_bounds.get().left_top() + egui::vec2(12.0, 12.0);
        let pointer_button = |pressed| egui::Event::PointerButton {
            pos: point,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(
            &mut dialog.text,
            second_session,
            0.6,
            vec![egui::Event::PointerMoved(point)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            second_session,
            0.7,
            vec![pointer_button(true)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            second_session,
            0.8,
            vec![pointer_button(false)],
        )
        .drop_without_applying_deltas();
        frame(
            &mut dialog.text,
            second_session,
            0.9,
            vec![egui::Event::Key {
                key: egui::Key::Z,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
            }],
        )
        .drop_without_applying_deltas();
        assert_eq!(dialog.text, "public body");
    }

    #[test]
    fn new_masked_draft_persists_plaintext_and_resets_after_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        save_snippets(path.to_str().unwrap(), &[]).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        dialog.begin_new("masked");
        let body = "λ first line\n  second 中文 🚀\n";
        dialog.text = body.to_owned();
        dialog.set_hide_contents(true);
        assert!(dialog.body_revealed);

        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].text, body);
        assert!(candidate[0].hide_contents);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap()[0].text, body);
        assert!(load_snippets(path.to_str().unwrap()).unwrap()[0].hide_contents);
        assert!(dialog.text.is_empty());
        assert!(!dialog.hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn alias_and_flag_only_save_preserves_masked_multiline_unicode_body() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let body = "original λ\n  multiline 中文 🚀\n";
        let mut original = snippet("old alias", body);
        original.hide_contents = true;
        let initial = vec![original];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, body);

        dialog.alias = "new alias".to_owned();
        dialog.set_hide_contents(false);
        assert!(
            !dialog.body_revealed,
            "unmasking does not reveal the current session"
        );
        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].text, body);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].alias, "new alias");
        assert_eq!(saved[0].text, body);
        assert!(!saved[0].hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
    }

    #[test]
    fn prompted_editor_save_preserves_configuration_on_alias_and_text_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("saved", "Hello {{name}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.alias = "renamed".into();
        dialog.text = "Updated {{name}}".into();
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let mut expected = original;
        expected.alias = "renamed".into();
        expected.text = "Updated {{name}}".into();
        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            vec![expected]
        );
    }

    #[test]
    fn prompted_authoring_is_explicit_and_metadata_only_save_is_atomic() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = snippet("ordinary", "Hello {{name}} / {{ticket_id}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        assert!(!dialog.prompt_for_fields);
        assert!(dialog.fields.is_empty());

        dialog.set_prompt_for_fields(true);
        assert_eq!(
            dialog.discovered_field_keys,
            vec!["name".to_owned(), "ticket_id".to_owned()]
        );
        assert_eq!(dialog.fields[0], SnippetFieldDefinition::new("name"));
        assert_eq!(dialog.fields[1], SnippetFieldDefinition::new("ticket_id"));
        dialog.fields[0].label = "Recipient".into();
        dialog.fields[0].default_value = "Ada".into();
        dialog.fields[0].required = false;
        dialog.fields[0].input_kind = SnippetInputKind::Multiline;
        dialog.fields[1].label = "Ticket".into();
        dialog.fields[1].default_value = "T-42".into();

        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();
        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].text, original.text);
        assert!(candidate[0].prompt_for_fields);
        assert_eq!(candidate[0].fields[0].label, "Recipient");
        assert_eq!(candidate[0].fields[0].default_value, "Ada");
        assert!(!candidate[0].fields[0].required);
        assert_eq!(
            candidate[0].fields[0].input_kind,
            SnippetInputKind::Multiline
        );
        assert_eq!(candidate[0].fields[1].label, "Ticket");
        assert_eq!(candidate[0].fields[1].default_value, "T-42");
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);

        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved, candidate);
        assert_eq!(snippets_version(), version + 1);
        let saved_bytes = std::fs::read(&path).unwrap();

        assert!(dialog.begin_existing(0));
        let no_op_version = snippets_version();
        dialog.save_editor(path.to_str().unwrap()).unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
        assert_eq!(snippets_version(), no_op_version);
    }

    #[test]
    fn new_snippets_start_plain_and_can_be_saved_with_authored_field_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        save_snippets(path.to_str().unwrap(), &[]).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        dialog.begin_new("greeting");

        assert!(!dialog.prompt_for_fields);
        assert!(dialog.fields.is_empty());
        dialog.text = "Hello {{name}}".into();
        dialog.set_prompt_for_fields(true);
        assert_eq!(dialog.fields, vec![SnippetFieldDefinition::new("name")]);
        dialog.fields[0].label = "Name".into();
        dialog.fields[0].default_value = "Ada".into();
        dialog.fields[0].required = true;
        dialog.fields[0].input_kind = SnippetInputKind::SingleLine;

        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].prompt_for_fields);
        assert_eq!(saved[0].fields[0].name, "name");
        assert_eq!(saved[0].fields[0].label, "Name");
        assert_eq!(saved[0].fields[0].default_value, "Ada");
        assert!(saved[0].fields[0].required);
        assert_eq!(saved[0].fields[0].input_kind, SnippetInputKind::SingleLine);
    }

    #[test]
    fn prompt_off_saves_malformed_braces_literally_and_retains_field_metadata() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = prompted_snippet("saved", "Hello {{name}}");
        original.fields[0].label = "Preferred name".into();
        original.fields[0].default_value = "Ada".into();
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.set_prompt_for_fields(false);
        dialog.text = r"Literal {{unfinished and \{{still literal}}".into();
        dialog.refresh_prompted_fields();
        assert!(dialog.template_error.is_none());
        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert!(!candidate[0].prompt_for_fields);
        assert_eq!(candidate[0].text, dialog.text);
        assert_eq!(candidate[0].fields, original.fields);

        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert!(!saved[0].prompt_for_fields);
        assert_eq!(
            saved[0].text,
            r"Literal {{unfinished and \{{still literal}}"
        );
        assert_eq!(saved[0].fields, original.fields);
    }

    #[test]
    fn prompted_editor_reconciles_fields_in_candidate_and_commits_them_only_on_save() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = prompted_snippet("saved", "Hello {{name}} and {{removed}}");
        original.fields[0].label = "Recipient".into();
        original.fields[0].default_value = "Ada".into();
        original.fields[0].required = false;
        let mut configured_removed = SnippetFieldDefinition::new("removed");
        configured_removed.label = "Old ticket".into();
        configured_removed.default_value = "T-42".into();
        configured_removed.input_kind = SnippetInputKind::Multiline;
        original.fields.push(configured_removed.clone());
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        dialog.text = "{{new_key}} then {{name}}".into();
        dialog.refresh_prompted_fields();

        assert_eq!(
            dialog.discovered_field_keys,
            vec!["new_key".to_owned(), "name".to_owned()]
        );
        assert_eq!(dialog.fields[0], original.fields[0]);
        assert_eq!(dialog.fields[1], configured_removed);
        assert_eq!(dialog.fields[2], SnippetFieldDefinition::new("new_key"));

        dialog.text = "{{removed}} then {{new_key}} then {{name}}".into();
        dialog.refresh_prompted_fields();
        assert_eq!(
            dialog.discovered_field_keys,
            vec![
                "removed".to_owned(),
                "new_key".to_owned(),
                "name".to_owned()
            ]
        );
        assert_eq!(dialog.field_mut("removed").unwrap().default_value, "T-42");

        dialog.text = "{{new_key}} then {{name}}".into();
        dialog.refresh_prompted_fields();

        let (_, candidate, _) = dialog.editor_candidate().unwrap();

        assert_eq!(
            candidate[0].fields,
            vec![
                SnippetFieldDefinition::new("new_key"),
                original.fields[0].clone()
            ]
        );
        assert!(dialog.fields.iter().any(|field| field.name == "removed"));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(dialog.entries, vec![original.clone()]);
        assert_eq!(snippets_version(), version);

        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].text, "{{new_key}} then {{name}}");
        assert_eq!(saved[0].fields, candidate[0].fields);
        assert!(!saved[0].fields.iter().any(|field| field.name == "removed"));
        assert_eq!(snippets_version(), version + 1);
    }

    #[test]
    fn prompted_editor_invalid_text_keeps_inline_error_draft_and_snapshot() {
        for (invalid_text, private_text) in [
            ("PRIVATE {{unfinished", "PRIVATE"),
            ("literal only", "literal only"),
        ] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("snippets.json");
            let original = prompted_snippet("saved", "Hello {{name}}");
            save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
            let original_bytes = std::fs::read(&path).unwrap();
            let version = snippets_version();
            let mut dialog = SnippetDialog::default();
            dialog.load_from(path.to_str().unwrap()).unwrap();
            assert!(dialog.begin_existing(0));
            dialog.alias = "draft alias".into();
            dialog.text = invalid_text.into();
            dialog.refresh_prompted_fields();
            assert!(dialog.discovered_field_keys.is_empty());
            if invalid_text.contains("unfinished") {
                assert!(dialog.template_error.is_some());
            } else {
                assert!(dialog.template_error.is_none());
            }

            assert!(matches!(
                dialog.save_editor(path.to_str().unwrap()),
                Err(CommitFailure::Rejected(CommitRejection::InvalidTemplate))
            ));

            assert_eq!(dialog.alias, "draft alias");
            assert_eq!(dialog.text, invalid_text);
            assert!(dialog.fields.iter().any(|field| field.name == "name"));
            assert_eq!(dialog.entries, vec![original.clone()]);
            assert_eq!(dialog.edit_idx, Some(0));
            let inline_error = dialog.inline_error.as_deref().unwrap();
            assert!(!inline_error.contains(private_text));
            assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
            assert_eq!(
                load_snippets(path.to_str().unwrap()).unwrap(),
                vec![original]
            );
            assert_eq!(snippets_version(), version);
        }
    }

    #[test]
    fn prompted_editor_cancel_discards_alias_and_text_draft_without_writing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("saved", "Hello {{name}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.alias = "unsaved alias".into();
        dialog.text = "Unsaved {{name}}".into();
        dialog.fields[0].label = "Draft label".into();
        dialog.fields[0].default_value = "Draft default".into();
        dialog.set_prompt_for_fields(false);
        dialog.cancel_editor();

        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            vec![original.clone()]
        );
        assert_eq!(dialog.entries, vec![original]);
        assert!(dialog.edit_idx.is_none());
        assert!(!dialog.prompt_for_fields);
        assert!(dialog.fields.is_empty());
        assert!(dialog.discovered_field_keys.is_empty());
    }

    #[test]
    fn tag_editor_normalizes_drafts_and_preserves_save_cancel_reopen_state() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = prompted_snippet("saved", "Hello {{name}}");
        original.tags = vec!["Work".into(), "high priority".into()];
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        assert_eq!(dialog.tags, original.tags);

        dialog.tag_input = "discarded tag".into();
        dialog.add_editor_tag();
        assert_eq!(
            dialog.tags,
            vec![
                "Work".to_owned(),
                "high priority".to_owned(),
                "discarded tag".to_owned()
            ]
        );
        dialog.cancel_editor();
        assert!(dialog.tags.is_empty());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);

        assert!(dialog.begin_existing(0));
        assert_eq!(dialog.tags, original.tags);
        let existing_tags = dialog.tags.clone();
        dialog.tag_input = "x".repeat(65);
        dialog.add_editor_tag();
        assert_eq!(dialog.tags, existing_tags);
        assert!(
            dialog
                .tag_error
                .as_deref()
                .unwrap()
                .contains("64 characters")
        );
        assert_eq!(dialog.tag_input, "x".repeat(65));
        dialog.tag_input.clear();
        dialog.tag_error = None;
        dialog.tag_input = " release   note ".into();
        dialog.add_editor_tag();
        dialog.tag_input = "WORK".into();
        dialog.add_editor_tag();
        assert_eq!(
            dialog.tags,
            vec![
                "Work".to_owned(),
                "high priority".to_owned(),
                "release note".to_owned()
            ]
        );
        let (_, candidate, _) = dialog.editor_candidate().unwrap();
        assert_eq!(candidate[0].tags, dialog.tags);
        assert_eq!(candidate[0].text, original.text);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(
            saved[0].tags,
            vec![
                "Work".to_owned(),
                "high priority".to_owned(),
                "release note".to_owned()
            ]
        );
        assert_eq!(saved[0].text, original.text);
        assert_eq!(saved[0].fields, original.fields);
        assert!(dialog.begin_existing(0));
        assert_eq!(dialog.tags, saved[0].tags);

        let saved_bytes = std::fs::read(&path).unwrap();
        let saved_version = snippets_version();
        dialog.tags.push("x".repeat(65));
        assert!(matches!(
            dialog.save_editor(path.to_str().unwrap()),
            Err(CommitFailure::Rejected(CommitRejection::InvalidTags))
        ));
        assert!(dialog.edit_idx.is_some());
        assert!(
            dialog
                .inline_error
                .as_deref()
                .unwrap()
                .contains("64 characters")
        );
        assert_eq!(std::fs::read(&path).unwrap(), saved_bytes);
        assert_eq!(snippets_version(), saved_version);

        dialog.cancel_editor();
        dialog.begin_new("fresh");
        dialog.text = "fresh body".into();
        dialog.tag_input = "New Tag".into();
        dialog.add_editor_tag();
        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[1].tags, vec!["New Tag".to_owned()]);
        assert_eq!(saved[1].text, "fresh body");
    }

    #[test]
    fn hidden_metadata_only_edit_preserves_body_and_tags_without_reveal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = snippet("masked", "private body that remains hidden");
        original.hide_contents = true;
        original.tags = vec!["team".into(), "release".into()];
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));
        assert!(!dialog.body_revealed);

        dialog.alias = "renamed masked".into();
        dialog.set_hide_contents(false);
        assert!(!dialog.body_revealed);
        dialog.save_editor(path.to_str().unwrap()).unwrap();

        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].alias, "renamed masked");
        assert_eq!(saved[0].text, original.text);
        assert_eq!(saved[0].tags, original.tags);
        assert!(!saved[0].hide_contents);
    }

    #[test]
    fn prompted_editor_no_op_save_preserves_bytes_and_version() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("saved", "Hello {{name}}");
        save_snippets(path.to_str().unwrap(), std::slice::from_ref(&original)).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let version = snippets_version();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.save_editor(path.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(snippets_version(), version);
        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            vec![original]
        );
    }

    #[test]
    fn hiding_existing_draft_conceals_but_preserves_edits_until_reveal() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("visible", "original body")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        assert!(dialog.begin_existing(0));

        dialog.text = "draft λ\nbody".to_owned();
        dialog.set_hide_contents(true);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, "draft λ\nbody");
        dialog.set_hide_contents(false);
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.text, "draft λ\nbody");

        dialog.reveal_contents();
        assert!(dialog.body_revealed);
        dialog.text = "revealed replacement 中文".to_owned();
        dialog.save_editor(path.to_str().unwrap()).unwrap();
        let saved = load_snippets(path.to_str().unwrap()).unwrap();
        assert_eq!(saved[0].text, "revealed replacement 中文");
        assert!(!saved[0].hide_contents);
        assert!(!dialog.body_revealed);
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn cancel_and_close_clear_revealed_draft_immediately() {
        let mut dialog = SnippetDialog {
            entries: vec![snippet("private", "body")],
            ..SnippetDialog::default()
        };
        dialog.reveal_contents();
        assert!(!dialog.body_revealed, "reveal requires an active editor");
        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "unsaved reveal".to_owned();
        dialog.cancel_editor();
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
        assert!(dialog.edit_idx.is_none());

        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "another unsaved reveal".to_owned();
        dialog.apply_window_open(false);
        assert!(!dialog.open);
        assert!(!dialog.body_revealed);
        assert!(dialog.text.is_empty());
        assert!(dialog.edit_idx.is_none());
    }

    #[test]
    fn failed_editor_save_preserves_revealed_session_draft() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let original = prompted_snippet("masked", "Original {{name}}");
        save_snippets(path.to_str().unwrap(), &[original]).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.open_edit_from(path.to_str().unwrap(), "masked");
        dialog.reveal_contents();
        dialog.alias = "changed alias".to_owned();
        dialog.text = "unsaved revised {{name}} λ\nbody".to_owned();
        dialog.refresh_prompted_fields();
        dialog.fields[0].label = "Draft label".into();
        dialog.fields[0].default_value = "Draft default".into();
        dialog.set_hide_contents(false);
        let draft = dialog.text.clone();
        let invalid = b"external malformed update";
        std::fs::write(&path, invalid).unwrap();

        assert!(matches!(
            dialog.save_editor(path.to_str().unwrap()),
            Err(CommitFailure::Persistence(_))
        ));
        assert_eq!(dialog.edit_idx, Some(0));
        assert_eq!(dialog.alias, "changed alias");
        assert_eq!(dialog.text, draft);
        assert!(dialog.body_revealed);
        assert!(!dialog.hide_contents);
        assert!(dialog.prompt_for_fields);
        assert_eq!(dialog.fields[0].label, "Draft label");
        assert_eq!(dialog.fields[0].default_value, "Draft default");
        assert!(dialog.load_error.is_some());
        assert_eq!(std::fs::read(path).unwrap(), invalid);
    }

    #[test]
    fn ensure_open_is_idempotent_and_reopen_reloads_after_session_end() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let mut original = snippet("masked", "original body");
        original.hide_contents = true;
        save_snippets(path.to_str().unwrap(), &[original]).unwrap();
        let mut dialog = SnippetDialog::default();

        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.begin_existing(0));
        dialog.reveal_contents();
        dialog.text = "unsaved draft".to_owned();
        dialog.filter = "original".into();
        dialog.selected_tag_filters = vec!["work".into()];
        let external = vec![snippet("masked", "external update")];
        save_snippets(path.to_str().unwrap(), &external).unwrap();

        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.open);
        assert_eq!(dialog.text, "unsaved draft");
        assert_eq!(dialog.filter, "original");
        assert_eq!(dialog.selected_tag_filters, vec!["work".to_owned()]);
        assert!(dialog.body_revealed);
        assert_eq!(dialog.entries[0].text, "original body");

        dialog.end_session();
        assert!(dialog.text.is_empty());
        assert!(dialog.filter.is_empty());
        assert!(dialog.selected_tag_filters.is_empty());
        assert!(!dialog.body_revealed);
        dialog.ensure_open_from(path.to_str().unwrap());
        assert!(dialog.open);
        assert!(dialog.edit_idx.is_none());
        assert!(dialog.text.is_empty());
        assert!(!dialog.body_revealed);
        assert_eq!(dialog.entries, external);
    }

    #[test]
    fn remove_request_and_cancel_do_not_write() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let entries = vec![snippet("first", "value")];
        save_snippets(path.to_str().unwrap(), &entries).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        dialog.request_removal(0);
        assert_eq!(dialog.pending_removal.as_ref().unwrap().index, 0);
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);

        dialog.cancel_removal();
        assert!(dialog.pending_removal.is_none());
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), entries);
    }

    #[test]
    fn confirm_removes_the_selected_duplicate_row_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let entries = vec![
            snippet("same", "first body"),
            snippet("same", "second body"),
        ];
        save_snippets(path.to_str().unwrap(), &entries).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        dialog.request_removal(1);
        assert!(dialog.confirm_removal(path.to_str().unwrap()).unwrap());

        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            vec![entries[0].clone()]
        );
        assert!(dialog.pending_removal.is_none());
    }

    #[test]
    fn filter_changes_and_new_targets_invalidate_pending_confirmation() {
        let mut dialog = SnippetDialog {
            entries: vec![snippet("first", "one"), snippet("second", "two")],
            ..SnippetDialog::default()
        };

        dialog.request_removal(0);
        dialog.filter_changed();
        assert!(dialog.pending_removal.is_none());

        dialog.request_removal(0);
        dialog.request_removal(1);
        let pending = dialog.pending_removal.as_ref().unwrap();
        assert_eq!(pending.index, 1);
        assert_eq!(pending.expected_entry.alias, "second");
    }

    #[test]
    fn stale_removal_confirmation_keeps_external_update_unchanged() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("first", "one")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        dialog.request_removal(0);

        let mut externally_updated = initial.clone();
        externally_updated.push(snippet("external", "append"));
        save_snippets(path.to_str().unwrap(), &externally_updated).unwrap();
        let external_bytes = std::fs::read(&path).unwrap();

        assert!(matches!(
            dialog.confirm_removal(path.to_str().unwrap()),
            Err(CommitFailure::Rejected(CommitRejection::StaleSnapshot))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), external_bytes);
        assert_eq!(
            load_snippets(path.to_str().unwrap()).unwrap(),
            externally_updated
        );
        assert!(dialog.pending_removal.is_none());
        assert!(dialog.inline_error.is_some());
    }

    #[test]
    fn exact_alias_validation_rejects_create_and_rename_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("existing", "one"), snippet("other", "two")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let original_bytes = std::fs::read(&path).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut created = initial.clone();
        created.push(snippet("existing", "new"));
        assert!(matches!(
            dialog.commit_candidate(
                path.to_str().unwrap(),
                &initial,
                created,
                Some(AliasValidation {
                    edited_index: None,
                    alias: "existing".to_owned(),
                }),
            ),
            Err(CommitFailure::Rejected(CommitRejection::DuplicateAlias))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert!(dialog.load_error.is_none());
        assert!(dialog.inline_error.is_some());

        let mut renamed = initial.clone();
        renamed[1].alias = "existing".to_owned();
        assert!(matches!(
            dialog.commit_candidate(
                path.to_str().unwrap(),
                &initial,
                renamed,
                Some(AliasValidation {
                    edited_index: Some(1),
                    alias: "existing".to_owned(),
                }),
            ),
            Err(CommitFailure::Rejected(CommitRejection::DuplicateAlias))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original_bytes);
        assert_eq!(dialog.entries, initial);
    }

    #[test]
    fn alias_validation_is_case_sensitive_and_preserves_untrimmed_aliases() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("Foo", "one")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut candidate = initial.clone();
        candidate.push(snippet("foo", "lowercase"));
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &initial,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: None,
                    alias: "foo".to_owned(),
                }),
            )
            .unwrap();

        let expected = candidate;
        let mut candidate = expected.clone();
        candidate.push(snippet(" Foo ", "spaced"));
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &expected,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: None,
                    alias: " Foo ".to_owned(),
                }),
            )
            .unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), candidate);
    }

    #[test]
    fn unchanged_alias_is_allowed_for_historical_duplicates() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![
            snippet("duplicate", "first"),
            snippet("duplicate", "second"),
        ];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();

        let mut candidate = initial.clone();
        candidate[0].text = "updated first".to_owned();
        dialog
            .commit_candidate(
                path.to_str().unwrap(),
                &initial,
                candidate.clone(),
                Some(AliasValidation {
                    edited_index: Some(0),
                    alias: "duplicate".to_owned(),
                }),
            )
            .unwrap();

        assert_eq!(load_snippets(path.to_str().unwrap()).unwrap(), candidate);
    }

    #[test]
    fn external_append_body_and_privacy_changes_reject_stale_snapshot() {
        for external_change in ["append", "body", "privacy"] {
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("snippets.json");
            let initial = vec![snippet("saved", "value")];
            save_snippets(path.to_str().unwrap(), &initial).unwrap();
            let mut dialog = SnippetDialog::default();
            dialog.load_from(path.to_str().unwrap()).unwrap();

            let mut externally_updated = initial.clone();
            match external_change {
                "append" => externally_updated.push(snippet("external", "append")),
                "body" => externally_updated[0].text = "external body".to_owned(),
                "privacy" => externally_updated[0].hide_contents = true,
                _ => unreachable!(),
            }
            save_snippets(path.to_str().unwrap(), &externally_updated).unwrap();
            let external_bytes = std::fs::read(&path).unwrap();
            let mut candidate = initial.clone();
            candidate[0].text = "draft update".to_owned();

            assert!(matches!(
                dialog.commit_candidate(path.to_str().unwrap(), &initial, candidate, None),
                Err(CommitFailure::Rejected(CommitRejection::StaleSnapshot))
            ));
            assert_eq!(
                std::fs::read(&path).unwrap(),
                external_bytes,
                "{external_change}"
            );
            assert_eq!(
                load_snippets(path.to_str().unwrap()).unwrap(),
                externally_updated,
                "{external_change}"
            );
            assert_eq!(dialog.entries, initial, "{external_change}");
            assert!(dialog.load_error.is_none(), "{external_change}");
        }
    }

    #[test]
    fn malformed_reload_and_commit_keep_dialog_last_good_and_read_only() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("snippets.json");
        let initial = vec![snippet("saved", "value")];
        save_snippets(path.to_str().unwrap(), &initial).unwrap();
        let mut dialog = SnippetDialog::default();
        dialog.load_from(path.to_str().unwrap()).unwrap();
        let invalid = b"invalid snippets";
        std::fs::write(&path, invalid).unwrap();

        assert!(
            dialog
                .commit_candidate(
                    path.to_str().unwrap(),
                    &initial,
                    vec![snippet("lost", "value")],
                    Some(AliasValidation {
                        edited_index: None,
                        alias: "lost".to_owned(),
                    }),
                )
                .is_err()
        );
        assert_eq!(dialog.entries, initial);
        assert_eq!(std::fs::read(&path).unwrap(), invalid);
        assert!(dialog.load_error.is_some());
        assert!(dialog.load_from(path.to_str().unwrap()).is_err());
        assert_eq!(dialog.entries, initial);
        assert!(dialog.load_error.is_some());
    }
}
