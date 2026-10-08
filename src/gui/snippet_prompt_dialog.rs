use std::collections::HashMap;

use crate::actions::Action;
use crate::commands::{ActivationSource, SnippetPromptIntent};
use crate::plugins::snippet_template::{
    RenderErrorKind, RenderIssueKind, RenderedPreview, TemplateRenderError, render_for_copy,
    render_preview,
};
use crate::plugins::snippets::{
    PreparedSnippetTemplate, SnippetEntry, SnippetInputKind, SnippetRunMode,
};
use crate::universal_actions::RootLauncherPolicy;
use eframe::egui;

#[derive(Clone, PartialEq, Eq)]
enum PromptFocusTarget {
    Field(String),
    Copy,
    Cancel,
}

pub(crate) enum SnippetPromptUiAction {
    Submit,
    Cancel,
}

/// Transient, in-memory state for one prompted snippet invocation or editor preview.
/// It intentionally has no `Debug` implementation because it owns user-entered values.
#[derive(Default)]
pub(crate) struct SnippetPromptDialog {
    session: Option<SnippetPromptSession>,
    generation: u64,
    open: bool,
    feedback: Option<SnippetPromptError>,
    focused_generation: Option<u64>,
    pending_focus: Option<PromptFocusTarget>,
    copy_widget_id: Option<egui::Id>,
    cancel_widget_id: Option<egui::Id>,
    #[cfg(test)]
    pub(crate) last_layout: Option<SnippetPromptLayout>,
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct SnippetPromptWidgetLayout {
    pub(crate) id: egui::Id,
    pub(crate) rect: egui::Rect,
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct SnippetPromptLayout {
    pub(crate) window_rect: egui::Rect,
    pub(crate) body_rect: egui::Rect,
    pub(crate) body_content_size: egui::Vec2,
    pub(crate) body_scroll_offset: egui::Vec2,
    pub(crate) field_ids: Vec<egui::Id>,
    pub(crate) copy: Option<SnippetPromptWidgetLayout>,
    pub(crate) cancel: SnippetPromptWidgetLayout,
}

pub(crate) struct SnippetPromptSession {
    pub(crate) generation: u64,
    pub(crate) alias: String,
    pub(crate) prepared: PreparedSnippetTemplate,
    pub(crate) values: HashMap<String, String>,
    pub(crate) initial_focus: Option<String>,
    mode: SnippetPromptMode,
}

enum SnippetPromptMode {
    Execute {
        entry_snapshot: SnippetEntry,
        safe_action: Action,
        source: ActivationSource,
        history_query: String,
        root_policy: RootLauncherPolicy,
    },
    PreviewOnly {
        draft: SnippetEntry,
    },
}

/// Safe submit failures contain no field values, rendered output, or template excerpts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SnippetPromptError {
    NoActiveSession,
    PreviewOnly,
    InvalidConfiguration,
    MissingValue,
    RequiredValueEmpty,
    StaleTemplate,
    ClipboardUnavailable,
}

impl SnippetPromptError {
    pub(crate) fn message(self) -> &'static str {
        match self {
            Self::NoActiveSession => "The snippet prompt is no longer open.",
            Self::PreviewOnly => "Preview mode cannot copy to the clipboard.",
            Self::InvalidConfiguration => "The snippet field configuration is invalid.",
            Self::MissingValue => "A snippet field value is missing.",
            Self::RequiredValueEmpty => "Fill in the required snippet fields before copying.",
            Self::StaleTemplate => {
                "This snippet changed while the prompt was open. Reopen it before copying."
            }
            Self::ClipboardUnavailable => "The snippet could not be copied to the clipboard.",
        }
    }
}

#[derive(Debug, PartialEq)]
pub(crate) struct SnippetPromptCompletion {
    pub(crate) alias: String,
    pub(crate) safe_action: Action,
    pub(crate) source: ActivationSource,
    pub(crate) history_query: String,
    pub(crate) root_policy: RootLauncherPolicy,
}

impl SnippetPromptDialog {
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn ensure_open(&mut self) {
        self.open = true;
    }

    pub(crate) fn session(&self) -> Option<&SnippetPromptSession> {
        self.session.as_ref()
    }

    pub(crate) fn is_preview_only(&self) -> bool {
        self.session
            .as_ref()
            .is_some_and(|session| matches!(&session.mode, SnippetPromptMode::PreviewOnly { .. }))
    }

    pub(crate) fn feedback(&self) -> Option<SnippetPromptError> {
        self.feedback
    }

    pub(crate) fn begin_execution(
        &mut self,
        ctx: &egui::Context,
        intent: SnippetPromptIntent,
    ) -> u64 {
        let session = SnippetPromptSession::new(
            intent.alias,
            intent.prepared,
            SnippetPromptMode::Execute {
                entry_snapshot: intent.entry_snapshot,
                safe_action: intent.safe_action,
                source: intent.source,
                history_query: intent.history_query,
                root_policy: intent.root_policy,
            },
        );
        self.replace_session(ctx, session)
    }

    pub(crate) fn begin_preview(
        &mut self,
        ctx: &egui::Context,
        draft: SnippetEntry,
    ) -> Result<u64, SnippetPromptError> {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&draft) {
            Ok(SnippetRunMode::Prompted(prepared)) => prepared,
            Ok(SnippetRunMode::Plain) | Err(_) => {
                self.feedback = Some(SnippetPromptError::InvalidConfiguration);
                return Err(SnippetPromptError::InvalidConfiguration);
            }
        };
        let session = SnippetPromptSession::new(
            draft.alias.clone(),
            prepared,
            SnippetPromptMode::PreviewOnly { draft },
        );
        Ok(self.replace_session(ctx, session))
    }

    fn replace_session(&mut self, ctx: &egui::Context, mut session: SnippetPromptSession) -> u64 {
        self.clear_widget_state(ctx);
        self.generation = self.generation.wrapping_add(1).max(1);
        session.generation = self.generation;
        self.session = Some(session);
        self.open = true;
        self.feedback = None;
        self.focused_generation = None;
        self.pending_focus = None;
        self.copy_widget_id = None;
        self.cancel_widget_id = None;
        self.generation
    }

    pub(crate) fn queue_tab_focus(&mut self, ctx: &egui::Context, backwards: bool) {
        let Some(session) = self.session.as_ref() else {
            return;
        };

        let mut targets = session
            .prepared
            .fields
            .iter()
            .map(|field| PromptFocusTarget::Field(field.name.clone()))
            .collect::<Vec<_>>();
        if !self.is_preview_only() {
            targets.push(PromptFocusTarget::Copy);
        }
        targets.push(PromptFocusTarget::Cancel);

        let current = targets.iter().position(|target| match target {
            PromptFocusTarget::Field(key) => {
                ctx.memory(|memory| memory.has_focus(field_widget_id(session.generation, key)))
            }
            PromptFocusTarget::Copy => self
                .copy_widget_id
                .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id))),
            PromptFocusTarget::Cancel => self
                .cancel_widget_id
                .is_some_and(|id| ctx.memory(|memory| memory.has_focus(id))),
        });
        let current = current.or_else(|| {
            let first = session.initial_focus.as_ref()?;
            targets
                .iter()
                .position(|target| matches!(target, PromptFocusTarget::Field(key) if key == first))
        });
        let target_index = match (current, backwards) {
            (Some(index), true) => index.checked_sub(1).unwrap_or(targets.len() - 1),
            (Some(index), false) => (index + 1) % targets.len(),
            (None, true) => targets.len() - 1,
            (None, false) => 0,
        };
        self.pending_focus = targets.get(target_index).cloned();
    }

    pub(crate) fn show(&mut self, ctx: &egui::Context) -> Option<SnippetPromptUiAction> {
        if !self.open {
            return None;
        }
        let Some(session) = self.session.as_ref() else {
            self.open = false;
            return Some(SnippetPromptUiAction::Cancel);
        };

        let generation = session.generation;
        let initial_focus = session.initial_focus.clone();
        let preview_only = matches!(&session.mode, SnippetPromptMode::PreviewOnly { .. });
        let mut focus_target = self.pending_focus.take();
        if self.focused_generation != Some(generation) {
            self.focused_generation = Some(generation);
            if focus_target.is_none() {
                focus_target = initial_focus.map(PromptFocusTarget::Field);
            }
        }

        let title = format!(
            "{} — {}",
            if preview_only {
                "Preview Snippet"
            } else {
                "Fill Snippet"
            },
            session.alias
        );
        let available = ctx.available_rect().size();
        let style = ctx.style();
        let mut window_frame = egui::Frame::window(&style);
        window_frame.inner_margin += window_frame.stroke.width / 2.0;
        let title_bar_height = ctx
            .fonts(|fonts| fonts.row_height(&style.text_styles[&egui::TextStyle::Heading]))
            + style.spacing.window_margin.sum().y;
        let window_chrome = window_frame.outer_margin.sum()
            + window_frame.inner_margin.sum()
            + egui::vec2(0.0, title_bar_height);
        let max_size = (available - window_chrome).max(egui::vec2(1.0, 1.0));
        let default_size = egui::vec2(max_size.x.min(620.0), max_size.y.min(700.0));
        let min_size = egui::vec2(max_size.x.min(280.0), max_size.y.min(180.0));
        let mut opened = self.open;
        let feedback = self.feedback;
        let mut action = None;
        let mut value_changed = false;
        #[cfg(test)]
        let mut body_layout = None;
        #[cfg(test)]
        let mut copy_layout = None;
        #[cfg(test)]
        let mut cancel_layout = None;
        #[cfg(test)]
        let field_ids = session
            .prepared
            .fields
            .iter()
            .map(|field| field_widget_id(generation, &field.name))
            .collect();
        let mut focus_scroll_rect = None;

        let window_id = egui::Id::new(("snippet_prompt_window", generation));
        let _window_output = egui::Window::new(title)
            .id(window_id)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .collapsible(false)
            .resizable(true)
            .default_size(default_size)
            .min_size(min_size)
            .max_size(max_size)
            .open(&mut opened)
            .show(ctx, |ui| {
                let Some(session) = self.session.as_mut() else {
                    return;
                };
                if preview_only {
                    ui.colored_label(
                        egui::Color32::YELLOW,
                        "Preview mode — copying is disabled.",
                    );
                }

                let footer_height = ui.spacing().interact_size.y + 12.0;
                let feedback_height = if feedback.is_some() { 22.0 } else { 0.0 };
                let body_height =
                    (ui.available_height() - footer_height - feedback_height).max(24.0);
                let _body_output = egui::ScrollArea::vertical()
                    .id_source(("snippet_prompt_body", generation))
                    .auto_shrink([false, false])
                    .max_height(body_height)
                    .show(ui, |ui| {
                        for field in &session.prepared.fields {
                            ui.horizontal_wrapped(|ui| {
                                ui.strong(field.display_label().as_ref());
                                if field.required {
                                    ui.colored_label(egui::Color32::LIGHT_RED, "* required");
                                }
                            });

                            let value = session.values.entry(field.name.clone()).or_default();
                            let id = field_widget_id(generation, &field.name);
                            let response = match field.input_kind {
                                SnippetInputKind::SingleLine => ui.add(
                                    egui::TextEdit::singleline(value)
                                        .id(id)
                                        .desired_width(f32::INFINITY),
                                ),
                                SnippetInputKind::Multiline => ui.add_sized(
                                    [ui.available_width(), 82.0],
                                    egui::TextEdit::multiline(value)
                                        .id(id)
                                        .desired_rows(3)
                                        .desired_width(f32::INFINITY),
                                ),
                            };
                            value_changed |= response.changed();
                            if matches!(&focus_target, Some(PromptFocusTarget::Field(key)) if key == &field.name)
                            {
                                response.request_focus();
                                focus_scroll_rect = Some(response.rect);
                            }

                            ui.add_space(4.0);
                        }

                        ui.separator();
                        ui.strong("Preview");
                        match render_preview(
                            &session.prepared.parsed,
                            &session.prepared.fields,
                            &session.values,
                        ) {
                            Ok(preview) => {
                                for issue in &preview.validation_errors {
                                    if issue.kind == RenderIssueKind::RequiredValueEmpty
                                        && let Some(field) = session
                                            .prepared
                                            .fields
                                            .iter()
                                            .find(|field| field.name == issue.key)
                                    {
                                        ui.colored_label(
                                            egui::Color32::LIGHT_RED,
                                            format!(
                                                "{}: enter a value for this required field.",
                                                field.display_label()
                                            ),
                                        );
                                    }
                                }
                                egui::ScrollArea::vertical()
                                    .id_source(("snippet_prompt_preview", generation))
                                    .auto_shrink([false, false])
                                    .max_height(ui.available_height().min(150.0).max(44.0))
                                    .show(ui, |ui| {
                                        let mut text = preview.text;
                                        ui.add(
                                            egui::TextEdit::multiline(&mut text)
                                                .id(preview_widget_id(generation))
                                                .desired_rows(3)
                                                .desired_width(f32::INFINITY)
                                                .interactive(false),
                                        );
                                    });
                            }
                            Err(error) => {
                                ui.colored_label(egui::Color32::LIGHT_RED, error.to_string());
                            }
                        }

                        // The nested preview ScrollArea shares egui's frame-level
                        // scroll target. Queue the field target after it renders so
                        // the outer form ScrollArea consumes it.
                        if let Some(rect) = focus_scroll_rect {
                            ui.scroll_to_rect(rect, Some(egui::Align::Center));
                        }
                    });
                #[cfg(test)]
                {
                    body_layout = Some((
                        _body_output.inner_rect,
                        _body_output.content_size,
                        _body_output.state.offset,
                    ));
                }

                if let Some(error) = feedback {
                    ui.colored_label(egui::Color32::LIGHT_RED, error.message());
                }
                ui.horizontal(|ui| {
                    if !preview_only {
                        let response = ui
                            .push_id(button_widget_id(generation, "copy"), |ui| ui.button("Copy"))
                            .inner;
                        #[cfg(test)]
                        {
                            copy_layout = Some(SnippetPromptWidgetLayout {
                                id: response.id,
                                rect: response.rect,
                            });
                        }
                        self.copy_widget_id = Some(response.id);
                        if matches!(&focus_target, Some(PromptFocusTarget::Copy)) {
                            response.request_focus();
                        }
                        if response.clicked() && action.is_none() {
                            action = Some(SnippetPromptUiAction::Submit);
                        }
                    }

                    let response = ui
                        .push_id(button_widget_id(generation, "cancel"), |ui| {
                            ui.button(if preview_only {
                                "Return to Editor"
                            } else {
                                "Cancel"
                            })
                        })
                        .inner;
                    #[cfg(test)]
                    {
                        cancel_layout = Some(SnippetPromptWidgetLayout {
                            id: response.id,
                            rect: response.rect,
                        });
                    }
                    self.cancel_widget_id = Some(response.id);
                    if matches!(&focus_target, Some(PromptFocusTarget::Cancel)) {
                        response.request_focus();
                    }
                    if response.clicked() {
                        action = Some(SnippetPromptUiAction::Cancel);
                    }
                });
            });

        #[cfg(test)]
        {
            self.last_layout = match (_window_output, body_layout, cancel_layout) {
                (
                    Some(window),
                    Some((body_rect, body_content_size, body_scroll_offset)),
                    Some(cancel),
                ) => Some(SnippetPromptLayout {
                    window_rect: ctx
                        .memory(|memory| memory.area_rect(window_id))
                        .unwrap_or(window.response.rect),
                    body_rect,
                    body_content_size,
                    body_scroll_offset,
                    field_ids,
                    copy: copy_layout,
                    cancel,
                }),
                _ => None,
            };
        }

        self.open = opened;
        if value_changed {
            self.feedback = None;
        }
        if !opened {
            Some(SnippetPromptUiAction::Cancel)
        } else {
            action
        }
    }

    /// Set only a configured field. The entered text remains in this session and is
    /// never copied into attribution or error state.
    pub(crate) fn set_value(&mut self, key: &str, value: String) -> bool {
        let Some(session) = self.session.as_mut() else {
            return false;
        };
        if !session
            .prepared
            .fields
            .iter()
            .any(|field| field.name == key)
        {
            return false;
        }
        session.values.insert(key.to_owned(), value);
        self.feedback = None;
        true
    }

    pub(crate) fn preview(&self) -> Result<RenderedPreview, TemplateRenderError> {
        let Some(session) = &self.session else {
            return Err(TemplateRenderError {
                kind: RenderErrorKind::InvalidConfiguration,
                issues: Vec::new(),
            });
        };
        render_preview(
            &session.prepared.parsed,
            &session.prepared.fields,
            &session.values,
        )
    }

    /// Try a final copy using injected resolution and clipboard operations. Both
    /// callbacks receive only the alias or rendered output; failures are replaced
    /// with safe local errors. Preview-only sessions are rejected before either runs.
    pub(crate) fn submit_with(
        &mut self,
        ctx: &egui::Context,
        resolve_current: impl FnOnce(&str) -> Result<SnippetEntry, ()>,
        copy_text: impl FnOnce(&str) -> Result<(), ()>,
    ) -> Result<SnippetPromptCompletion, SnippetPromptError> {
        let result = (|| {
            let session = self
                .session
                .as_ref()
                .ok_or(SnippetPromptError::NoActiveSession)?;
            let SnippetPromptMode::Execute {
                entry_snapshot,
                safe_action,
                source,
                history_query,
                root_policy,
            } = &session.mode
            else {
                return Err(SnippetPromptError::PreviewOnly);
            };

            let rendered = render_for_copy(
                &session.prepared.parsed,
                &session.prepared.fields,
                &session.values,
            )
            .map_err(map_render_error)?;
            let current =
                resolve_current(&session.alias).map_err(|()| SnippetPromptError::StaleTemplate)?;
            if current != *entry_snapshot {
                return Err(SnippetPromptError::StaleTemplate);
            }

            copy_text(&rendered).map_err(|()| SnippetPromptError::ClipboardUnavailable)?;
            Ok(SnippetPromptCompletion {
                alias: session.alias.clone(),
                safe_action: safe_action.clone(),
                source: *source,
                history_query: history_query.clone(),
                root_policy: *root_policy,
            })
        })();

        match result {
            Ok(completion) => {
                self.clear_widget_state(ctx);
                self.session = None;
                self.open = false;
                self.feedback = None;
                self.pending_focus = None;
                Ok(completion)
            }
            Err(error) => {
                self.feedback = Some(error);
                Err(error)
            }
        }
    }

    /// Cancel and drop entered values. Preview mode returns the exact unsaved draft
    /// so the editor can resume with its pre-preview state.
    pub(crate) fn cancel(&mut self, ctx: &egui::Context) -> Option<SnippetEntry> {
        self.clear_widget_state(ctx);
        self.open = false;
        self.feedback = None;
        self.pending_focus = None;
        match self.session.take().map(|session| session.mode) {
            Some(SnippetPromptMode::PreviewOnly { draft }) => Some(draft),
            Some(SnippetPromptMode::Execute { .. }) | None => None,
        }
    }

    pub(crate) fn shutdown(&mut self, ctx: &egui::Context) {
        let _ = self.cancel(ctx);
    }

    fn clear_widget_state(&self, ctx: &egui::Context) {
        let Some(session) = self.session.as_ref() else {
            return;
        };

        let mut ids = session
            .prepared
            .fields
            .iter()
            .map(|field| field_widget_id(session.generation, &field.name))
            .collect::<Vec<_>>();
        ids.push(preview_widget_id(session.generation));
        ids.extend(self.copy_widget_id);
        ids.extend(self.cancel_widget_id);

        let focused = ctx.memory(|memory| {
            ids.iter()
                .copied()
                .filter(|id| memory.has_focus(*id))
                .collect::<Vec<_>>()
        });
        if !focused.is_empty() {
            ctx.memory_mut(|memory| {
                for id in focused {
                    memory.surrender_focus(id);
                }
            });
        }
        ctx.data_mut(|data| {
            for id in ids {
                data.remove::<egui::text_edit::TextEditState>(id);
            }
        });
    }
}

fn field_widget_id(generation: u64, key: &str) -> egui::Id {
    egui::Id::new(("snippet_prompt_field", generation, key))
}

fn preview_widget_id(generation: u64) -> egui::Id {
    egui::Id::new(("snippet_prompt_preview_text", generation))
}

fn button_widget_id(generation: u64, name: &str) -> egui::Id {
    egui::Id::new(("snippet_prompt_button", generation, name))
}

impl SnippetPromptSession {
    fn new(alias: String, prepared: PreparedSnippetTemplate, mode: SnippetPromptMode) -> Self {
        let values = prepared
            .fields
            .iter()
            .map(|field| (field.name.clone(), field.default_value.clone()))
            .collect();
        let initial_focus = prepared
            .fields
            .iter()
            .find(|field| field.required)
            .or_else(|| prepared.fields.first())
            .map(|field| field.name.clone());
        Self {
            generation: 0,
            alias,
            prepared,
            values,
            initial_focus,
            mode,
        }
    }
}

fn map_render_error(error: TemplateRenderError) -> SnippetPromptError {
    match error.kind {
        RenderErrorKind::InvalidConfiguration => SnippetPromptError::InvalidConfiguration,
        RenderErrorKind::MissingValues => SnippetPromptError::MissingValue,
        RenderErrorKind::RequiredValues => SnippetPromptError::RequiredValueEmpty,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugins::snippets::{SnippetFieldDefinition, SnippetInputKind};

    fn entry() -> SnippetEntry {
        SnippetEntry {
            alias: "ticketreply".into(),
            text: "Hello {{name}}, {{ticket}}".into(),
            hide_contents: false,
            prompt_for_fields: true,
            fields: vec![
                SnippetFieldDefinition {
                    name: "name".into(),
                    label: "Recipient".into(),
                    default_value: "Ada".into(),
                    required: true,
                    input_kind: SnippetInputKind::SingleLine,
                },
                SnippetFieldDefinition {
                    name: "ticket".into(),
                    label: "Ticket".into(),
                    default_value: String::new(),
                    required: false,
                    input_kind: SnippetInputKind::Multiline,
                },
            ],
        }
    }

    fn intent(entry: SnippetEntry) -> SnippetPromptIntent {
        let prepared = match crate::plugins::snippets::prepare_snippet_run(&entry).unwrap() {
            SnippetRunMode::Prompted(prepared) => prepared,
            SnippetRunMode::Plain => unreachable!(),
        };
        SnippetPromptIntent {
            alias: entry.alias.clone(),
            entry_snapshot: entry,
            prepared,
            safe_action: Action {
                label: "ticketreply".into(),
                desc: "Snippet".into(),
                action: crate::plugins::snippets::snippet_run_action("ticketreply"),
                args: None,
            },
            source: ActivationSource::Dashboard,
            history_query: "cs ticketreply".into(),
            root_policy: RootLauncherPolicy::PreserveOrdinaryState,
        }
    }

    fn render_frame(
        ctx: &egui::Context,
        dialog: &mut SnippetPromptDialog,
        events: Vec<egui::Event>,
    ) {
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(
                    egui::Pos2::ZERO,
                    egui::vec2(720.0, 520.0),
                )),
                focused: true,
                events,
                ..Default::default()
            },
            |ctx| {
                let _ = dialog.show(ctx);
            },
        );
    }

    fn type_into_rendered_field(
        ctx: &egui::Context,
        dialog: &mut SnippetPromptDialog,
        generation: u64,
        key: &str,
        value: &str,
    ) {
        let id = field_widget_id(generation, key);
        ctx.memory_mut(|memory| memory.request_focus(id));
        render_frame(
            ctx,
            dialog,
            vec![
                egui::Event::Key {
                    key: egui::Key::A,
                    physical_key: None,
                    pressed: true,
                    repeat: false,
                    modifiers: egui::Modifiers::CTRL | egui::Modifiers::COMMAND,
                },
                egui::Event::Text(value.to_owned()),
            ],
        );
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.values.get(key))
                .map(String::as_str),
            Some(value)
        );
        assert!(egui::text_edit::TextEditState::load(ctx, id).is_some());
    }

    #[test]
    fn execution_session_uses_defaults_and_focuses_first_required_field() {
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(entry()));

        let session = dialog.session().unwrap();
        assert_eq!(session.values.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(session.values.get("ticket").map(String::as_str), Some(""));
        assert_eq!(session.initial_focus.as_deref(), Some("name"));
        assert_eq!(session.prepared.fields[0].name, "name");
        assert_eq!(session.prepared.fields[1].name, "ticket");
        assert!(dialog.is_open());
    }

    #[test]
    fn optional_only_form_focuses_first_available_field() {
        let ctx = egui::Context::default();
        let mut saved = entry();
        for field in &mut saved.fields {
            field.required = false;
        }
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(saved));
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.initial_focus.as_deref()),
            Some("name")
        );
    }

    #[test]
    fn replacement_starts_fresh_generation_with_configured_defaults() {
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        let first = dialog.begin_execution(&ctx, intent(entry()));
        render_frame(&ctx, &mut dialog, Vec::new());
        let first_field = field_widget_id(first, "name");
        let first_preview = preview_widget_id(first);
        type_into_rendered_field(&ctx, &mut dialog, first, "name", "replacement sentinel");
        assert!(egui::text_edit::TextEditState::load(&ctx, first_preview).is_some());
        assert!(ctx.memory(|memory| memory.has_focus(first_field)));

        let second = dialog.begin_execution(&ctx, intent(entry()));
        assert!(egui::text_edit::TextEditState::load(&ctx, first_field).is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, first_preview).is_none());
        assert!(!ctx.memory(|memory| memory.has_focus(first_field)));
        render_frame(&ctx, &mut dialog, Vec::new());
        assert!(
            egui::text_edit::TextEditState::load(&ctx, field_widget_id(second, "name")).is_some()
        );

        let session = dialog.session().unwrap();
        assert_ne!(first, second);
        assert_eq!(session.generation, second);
        assert_eq!(session.values.get("name").map(String::as_str), Some("Ada"));
        assert_eq!(session.values.get("ticket").map(String::as_str), Some(""));
    }

    #[test]
    fn successful_submit_copies_once_and_returns_only_captured_safe_attribution() {
        let entry = entry();
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(entry.clone()));
        let generation = dialog.session().unwrap().generation;
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(&ctx, &mut dialog, generation, "ticket", "42");
        let name_id = field_widget_id(generation, "name");
        let ticket_id = field_widget_id(generation, "ticket");
        let preview_id = preview_widget_id(generation);
        let mut copied = Vec::new();
        let completion = dialog
            .submit_with(
                &ctx,
                |_| Ok(entry.clone()),
                |text| {
                    copied.push(text.to_owned());
                    Ok(())
                },
            )
            .unwrap();

        assert_eq!(copied, ["Hello Ada, 42"]);
        assert_eq!(completion.alias, "ticketreply");
        assert_eq!(completion.safe_action.action, "snippet:run:ticketreply");
        assert_eq!(completion.safe_action.args, None);
        assert_eq!(completion.source, ActivationSource::Dashboard);
        assert_eq!(completion.history_query, "cs ticketreply");
        assert_eq!(
            completion.root_policy,
            RootLauncherPolicy::PreserveOrdinaryState
        );
        assert!(!dialog.is_open());
        assert!(dialog.session().is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, name_id).is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, ticket_id).is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, preview_id).is_none());

        let second = dialog.submit_with(&ctx, |_| Ok(entry), |_| panic!("must not copy twice"));
        assert_eq!(second, Err(SnippetPromptError::NoActiveSession));
    }

    #[test]
    fn required_and_missing_values_fail_without_copy_and_keep_the_session() {
        let ctx = egui::Context::default();
        let mut saved = entry();
        saved.fields[1].required = true;
        let mut dialog = SnippetPromptDialog::default();
        let generation = dialog.begin_execution(&ctx, intent(saved.clone()));
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(&ctx, &mut dialog, generation, "name", "retained sentinel");
        let name_id = field_widget_id(generation, "name");
        let mut writes = 0;
        let result = dialog.submit_with(
            &ctx,
            |_| Ok(saved.clone()),
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::RequiredValueEmpty));
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
        assert!(egui::text_edit::TextEditState::load(&ctx, name_id).is_some());
        assert_eq!(
            dialog.feedback(),
            Some(SnippetPromptError::RequiredValueEmpty)
        );

        dialog.session.as_mut().unwrap().values.remove("ticket");
        let result = dialog.submit_with(
            &ctx,
            |_| Ok(saved),
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::MissingValue));
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
        assert!(egui::text_edit::TextEditState::load(&ctx, name_id).is_some());
    }

    #[test]
    fn clipboard_failure_is_safe_and_retains_filled_values() {
        let entry = entry();
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(entry.clone()));
        dialog.set_value("name", "private form sentinel".into());
        let error = dialog
            .submit_with(&ctx, |_| Ok(entry), |_| Err(()))
            .unwrap_err();

        assert_eq!(error, SnippetPromptError::ClipboardUnavailable);
        assert!(!error.message().contains("private form sentinel"));
        assert!(dialog.is_open());
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.values.get("name"))
                .map(String::as_str),
            Some("private form sentinel")
        );
    }

    #[test]
    fn invalid_preview_does_not_replace_or_clear_active_execution_state() {
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        let generation = dialog.begin_execution(&ctx, intent(entry()));
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(&ctx, &mut dialog, generation, "name", "active sentinel");
        let field_id = field_widget_id(generation, "name");
        let preview_id = preview_widget_id(generation);

        let mut invalid = entry();
        invalid.prompt_for_fields = false;
        assert_eq!(
            dialog.begin_preview(&ctx, invalid),
            Err(SnippetPromptError::InvalidConfiguration)
        );
        assert!(dialog.is_open());
        assert_eq!(dialog.session().unwrap().generation, generation);
        assert_eq!(
            dialog
                .session()
                .unwrap()
                .values
                .get("name")
                .map(String::as_str),
            Some("active sentinel")
        );
        assert!(egui::text_edit::TextEditState::load(&ctx, field_id).is_some());
        assert!(egui::text_edit::TextEditState::load(&ctx, preview_id).is_some());
    }

    #[test]
    fn changed_deleted_or_ambiguous_current_entries_are_stale() {
        for current in [
            Some({
                let mut changed = entry();
                changed.text.push_str(" changed");
                changed
            }),
            None,
            None,
        ] {
            let snapshot = entry();
            let ctx = egui::Context::default();
            let mut dialog = SnippetPromptDialog::default();
            dialog.begin_execution(&ctx, intent(snapshot));
            let mut writes = 0;
            let result = dialog.submit_with(
                &ctx,
                |_| current.clone().ok_or(()),
                |_| {
                    writes += 1;
                    Ok(())
                },
            );
            assert_eq!(result, Err(SnippetPromptError::StaleTemplate));
            assert_eq!(writes, 0);
            assert!(dialog.is_open());
        }
    }

    #[test]
    fn unchanged_persisted_entry_with_orphan_metadata_is_not_false_stale() {
        let ctx = egui::Context::default();
        let mut saved = entry();
        let mut orphan = SnippetFieldDefinition::new("orphan");
        orphan.default_value = "persisted but unused".into();
        saved.fields.push(orphan);
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(saved.clone()));
        dialog.set_value("ticket", "42".into());

        let mut writes = Vec::new();
        let result = dialog.submit_with(
            &ctx,
            |_| Ok(saved.clone()),
            |text| {
                writes.push(text.to_owned());
                Ok(())
            },
        );
        assert!(result.is_ok());
        assert_eq!(writes, ["Hello Ada, 42"]);
    }

    #[test]
    fn preview_cannot_resolve_or_copy_and_cancel_returns_exact_draft() {
        let mut draft = entry();
        draft.alias = "unsaved alias".into();
        draft.text = "Draft: {{name}} / {{ticket}}\r\n".into();
        draft.fields[0].default_value = "Draft default".into();
        let expected_draft = draft.clone();
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        let generation = dialog.begin_preview(&ctx, draft).unwrap();
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(&ctx, &mut dialog, generation, "name", "preview only");
        let name_id = field_widget_id(generation, "name");
        let ticket_id = field_widget_id(generation, "ticket");
        let preview_id = preview_widget_id(generation);
        assert!(egui::text_edit::TextEditState::load(&ctx, preview_id).is_some());

        let mut resolutions = 0;
        let mut writes = 0;
        let result = dialog.submit_with(
            &ctx,
            |_| {
                resolutions += 1;
                Err(())
            },
            |_| {
                writes += 1;
                Ok(())
            },
        );
        assert_eq!(result, Err(SnippetPromptError::PreviewOnly));
        assert_eq!(resolutions, 0);
        assert_eq!(writes, 0);
        assert!(dialog.is_open());
        assert_eq!(dialog.preview().unwrap().text, "Draft: preview only / \r\n");

        assert_eq!(dialog.cancel(&ctx), Some(expected_draft));
        assert!(!dialog.is_open());
        assert!(dialog.session().is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, name_id).is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, ticket_id).is_none());
        assert!(egui::text_edit::TextEditState::load(&ctx, preview_id).is_none());
    }

    #[test]
    fn cancel_and_shutdown_clear_values_and_safe_feedback() {
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        let cancel_generation = dialog.begin_execution(&ctx, intent(entry()));
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(
            &ctx,
            &mut dialog,
            cancel_generation,
            "name",
            "private sentinel",
        );
        let cancel_field = field_widget_id(cancel_generation, "name");
        assert!(ctx.memory(|memory| memory.has_focus(cancel_field)));
        dialog.cancel(&ctx);
        assert!(dialog.session().is_none());
        assert_eq!(dialog.feedback(), None);
        assert!(egui::text_edit::TextEditState::load(&ctx, cancel_field).is_none());
        assert!(!ctx.memory(|memory| memory.has_focus(cancel_field)));

        let shutdown_generation = dialog.begin_execution(&ctx, intent(entry()));
        render_frame(&ctx, &mut dialog, Vec::new());
        type_into_rendered_field(
            &ctx,
            &mut dialog,
            shutdown_generation,
            "name",
            "private sentinel",
        );
        let shutdown_field = field_widget_id(shutdown_generation, "name");
        dialog.shutdown(&ctx);
        assert!(dialog.session().is_none());
        assert!(!dialog.is_open());
        assert!(egui::text_edit::TextEditState::load(&ctx, shutdown_field).is_none());
    }

    #[test]
    fn session_and_attribution_do_not_format_entered_values() {
        let entry = entry();
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        dialog.begin_execution(&ctx, intent(entry.clone()));
        dialog.set_value("name", "private form sentinel".into());
        let completion = dialog.submit_with(&ctx, |_| Ok(entry), |_| Ok(())).unwrap();
        assert_eq!(completion.safe_action.args, None);
        assert_eq!(completion.safe_action.action, "snippet:run:ticketreply");
        assert_eq!(completion.history_query, "cs ticketreply");
    }

    #[test]
    fn presentation_focuses_required_field_and_scopes_widgets_to_each_session() {
        let ctx = egui::Context::default();
        let mut dialog = SnippetPromptDialog::default();
        let first_generation = dialog.begin_execution(&ctx, intent(entry()));
        let first_field_id = field_widget_id(first_generation, "name");
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(400.0, 220.0));

        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                assert!(dialog.show(ctx).is_none());
            },
        );
        assert!(ctx.memory(|memory| memory.has_focus(first_field_id)));

        dialog.set_value("name", "old transient value".into());
        let second_generation = dialog.begin_execution(&ctx, intent(entry()));
        let second_field_id = field_widget_id(second_generation, "name");
        assert_ne!(first_field_id, second_field_id);
        assert_ne!(
            egui::Id::new(("snippet_prompt_window", first_generation)),
            egui::Id::new(("snippet_prompt_window", second_generation))
        );
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                assert!(dialog.show(ctx).is_none());
            },
        );
        assert!(ctx.memory(|memory| memory.has_focus(second_field_id)));
        assert!(!ctx.memory(|memory| memory.has_focus(first_field_id)));
        assert_eq!(
            dialog
                .session()
                .and_then(|session| session.values.get("name"))
                .map(String::as_str),
            Some("Ada")
        );

        let mut draft = entry();
        draft.alias = "draft".into();
        dialog.begin_preview(&ctx, draft).unwrap();
        let _ = ctx.run(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ctx| {
                assert!(dialog.show(ctx).is_none());
            },
        );
        assert!(dialog.copy_widget_id.is_none());
        assert!(dialog.cancel_widget_id.is_some());
    }
}
