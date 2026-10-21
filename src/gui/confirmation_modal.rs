use eframe::egui;

use crate::commands::{
    ActivationSource, BrowserTabCommand, ClipboardCommand, Command, NoteCommand, StorageCommand,
    TodoCommand, TodoCompatibilityKind,
};
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfirmationResult {
    None,
    Confirmed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DestructiveAction {
    ClearClipboard,
    ClearHistory,
    ClearTodos,
    DeleteTodo,
    DeleteNote,
    DeleteGesture,
    ClearTempfiles,
    ClearBrowserTabCache,
    EmptyRecycleBin,
    ResetWidgetSettings,
    DeleteMacro,
    RemoveFolder,
    RemoveBookmark,
    CancelTimer,
    StopStopwatch,
    RemoveSnippet,
    DeleteTempfile,
    RemoveClipboardEntry,
    CloseWindow,
}

impl DestructiveAction {
    pub fn from_command(command: &Command) -> Option<Self> {
        match command {
            Command::Clipboard(ClipboardCommand::Clear) => Some(Self::ClearClipboard),
            Command::Storage(StorageCommand::HistoryClear) => Some(Self::ClearHistory),
            Command::Todo(TodoCommand::Clear) => Some(Self::ClearTodos),
            Command::Storage(StorageCommand::TempfileClear) => Some(Self::ClearTempfiles),
            Command::BrowserTab(BrowserTabCommand::Clear) => Some(Self::ClearBrowserTabCache),
            Command::Storage(StorageCommand::RecycleClean) => Some(Self::EmptyRecycleBin),
            Command::Todo(
                TodoCommand::Remove { .. }
                | TodoCommand::Compatibility {
                    kind: TodoCompatibilityKind::Remove,
                },
            ) => Some(Self::DeleteTodo),
            Command::Note(NoteCommand::Remove { .. }) => Some(Self::DeleteNote),
            _ => None,
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::ClearClipboard => "Clear clipboard history",
            Self::ClearHistory => "Clear search history",
            Self::ClearTodos => "Clear completed todos",
            Self::DeleteTodo => "Delete todo",
            Self::DeleteNote => "Delete note",
            Self::DeleteGesture => "Delete gesture",
            Self::ClearTempfiles => "Clear temp files",
            Self::ClearBrowserTabCache => "Clear browser tab cache",
            Self::EmptyRecycleBin => "Empty recycle bin",
            Self::ResetWidgetSettings => "Reset widget settings",
            Self::DeleteMacro => "Delete macro",
            Self::RemoveFolder => "Remove folder",
            Self::RemoveBookmark => "Remove bookmark",
            Self::CancelTimer => "Remove timer",
            Self::StopStopwatch => "Stop stopwatch",
            Self::RemoveSnippet => "Remove snippet",
            Self::DeleteTempfile => "Delete temporary file",
            Self::RemoveClipboardEntry => "Remove clipboard entry",
            Self::CloseWindow => "Close window",
        }
    }

    pub fn warning(self) -> &'static str {
        "This action cannot be undone."
    }
}

impl DestructiveAction {
    pub fn from_universal_action(
        action: &crate::universal_actions::UniversalAction,
    ) -> Option<Self> {
        use crate::universal_actions::action_ids;

        match action.id.as_str() {
            value if value == action_ids::FOLDER_REMOVE.as_str() => Some(Self::RemoveFolder),
            value if value == action_ids::BOOKMARK_REMOVE.as_str() => Some(Self::RemoveBookmark),
            value if value == action_ids::TIMER_CANCEL.as_str() => Some(Self::CancelTimer),
            value if value == action_ids::STOPWATCH_STOP.as_str() => Some(Self::StopStopwatch),
            value if value == action_ids::SNIPPET_REMOVE.as_str() => Some(Self::RemoveSnippet),
            value if value == action_ids::TEMPFILE_DELETE.as_str() => Some(Self::DeleteTempfile),
            value if value == action_ids::NOTE_REMOVE.as_str() => Some(Self::DeleteNote),
            value if value == action_ids::CLIPBOARD_REMOVE.as_str() => {
                Some(Self::RemoveClipboardEntry)
            }
            value if value == action_ids::WINDOW_CLOSE.as_str() => Some(Self::CloseWindow),
            _ => None,
        }
    }

    /// Resolve confirmation metadata for a radial operation that may have
    /// arrived through a generic primary result rather than a curated action
    /// id. Keep this fallback radial-only so legacy primary activation retains
    /// its established confirmation path.
    pub fn from_radial_operation(
        action: &crate::universal_actions::UniversalAction,
    ) -> Option<Self> {
        Self::from_universal_action(action).or_else(|| {
            let command = match &action.operation {
                crate::universal_actions::UniversalActionOperation::Command { command, .. } => {
                    return Self::from_command(command);
                }
                crate::universal_actions::UniversalActionOperation::InvokePrimary(primary) => {
                    crate::commands::parse_action(primary).ok()?
                }
                crate::universal_actions::UniversalActionOperation::UiIntent(_) => return None,
            };
            Self::from_command(&command)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{ConfirmationModal, DestructiveAction};
    use crate::actions::Action;
    use crate::commands::{ActivationSource, Command, NoteCommand, parse_action};
    use crate::universal_actions::{
        ActionAvailability, ActionPresentation, ActionSafety, ActionTarget, UniversalAction,
        UniversalActionOperation, action_ids,
    };
    use eframe::egui;

    fn command(raw: &str) -> Command {
        parse_action(&Action {
            label: raw.into(),
            desc: "test".into(),
            action: raw.into(),
            args: None,
        })
        .unwrap()
    }

    #[test]
    fn from_command_maps_note_remove() {
        let command = command("note:remove:project-idea");
        assert_eq!(
            DestructiveAction::from_command(&command),
            Some(DestructiveAction::DeleteNote)
        );
        assert!(matches!(command, Command::Note(NoteCommand::Remove { .. })));
    }

    #[test]
    fn typed_destructive_metadata_maps_all_eight_existing_protocols() {
        for (raw, expected) in [
            ("clipboard:clear", DestructiveAction::ClearClipboard),
            ("history:clear", DestructiveAction::ClearHistory),
            ("todo:clear", DestructiveAction::ClearTodos),
            ("tempfile:clear", DestructiveAction::ClearTempfiles),
            ("tab:clear", DestructiveAction::ClearBrowserTabCache),
            ("recycle:clean", DestructiveAction::EmptyRecycleBin),
            ("todo:remove:2", DestructiveAction::DeleteTodo),
            ("todo:remove:not-an-index", DestructiveAction::DeleteTodo),
            ("note:remove:alpha", DestructiveAction::DeleteNote),
        ] {
            assert_eq!(
                DestructiveAction::from_command(&command(raw)),
                Some(expected),
                "{raw}"
            );
        }
    }

    #[test]
    fn macro_source_is_rendered_in_confirmation_text() {
        let mut modal = ConfirmationModal::default();
        modal.open_for_source(
            DestructiveAction::ClearHistory,
            Some(ActivationSource::Macro),
        );
        assert_eq!(modal.source_label.as_deref(), Some("Triggered by macro"));
    }

    #[test]
    fn long_confirmation_keeps_actions_visible_and_window_close_cancels_once() {
        fn button_bounds(output: &egui::FullOutput, label: &str) -> Option<egui::Rect> {
            output
                .platform_output
                .accesskit_update
                .as_ref()?
                .nodes
                .iter()
                .filter(|(_, node)| {
                    node.role() == egui::accesskit::Role::Button && node.label() == Some(label)
                })
                .find_map(|(_, node)| {
                    let bounds = node.bounds()?;
                    Some(egui::Rect::from_min_max(
                        egui::pos2(bounds.x0 as f32, bounds.y0 as f32),
                        egui::pos2(bounds.x1 as f32, bounds.y1 as f32),
                    ))
                })
        }

        fn input(screen_rect: egui::Rect, events: Vec<egui::Event>) -> egui::RawInput {
            egui::RawInput {
                screen_rect: Some(screen_rect),
                focused: true,
                events,
                ..Default::default()
            }
        }

        let ctx = egui::Context::default();
        ctx.enable_accesskit();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(480.0, 280.0));
        let mut modal = ConfirmationModal::default();
        modal.open = true;
        modal.description = "Remove the selected item and its saved data. ".repeat(60);
        modal.source_label =
            Some("Triggered by a macro from a long source description. ".repeat(28));
        modal.warning =
            "This action cannot be undone; current data may not be recoverable. ".repeat(32);

        let mut final_bounds = None;
        for _ in 0..3 {
            let mut result = super::ConfirmationResult::None;
            let output = ctx.run_ui(input(screen_rect, Vec::new()), |ctx| {
                result = modal.ui(ctx);
            });
            let bounds = (
                button_bounds(&output, "Confirm"),
                button_bounds(&output, "Cancel"),
                button_bounds(&output, "Close window"),
            );
            if bounds.0.is_some() && bounds.1.is_some() && bounds.2.is_some() {
                final_bounds = Some(bounds);
            }
            output.drop_without_applying_deltas();
            assert_eq!(result, super::ConfirmationResult::None);
        }
        assert!(modal.is_open());
        let (confirm_bounds, cancel_bounds, close_bounds) =
            final_bounds.expect("the settled confirmation frame has accessibility bounds");
        let confirm_bounds = confirm_bounds.expect("Confirm button is exposed");
        let cancel_bounds = cancel_bounds.expect("Cancel button is exposed");
        let close_bounds = close_bounds.expect("the window close button is exposed");
        assert!(screen_rect.contains_rect(confirm_bounds));
        assert!(screen_rect.contains_rect(cancel_bounds));
        assert!(screen_rect.contains_rect(close_bounds));
        assert!(!confirm_bounds.intersects(cancel_bounds));

        let close_point = close_bounds.center();
        let mut press_result = super::ConfirmationResult::None;
        let press_output = ctx.run_ui(
            input(
                screen_rect,
                vec![
                    egui::Event::PointerMoved(close_point),
                    egui::Event::PointerButton {
                        pos: close_point,
                        button: egui::PointerButton::Primary,
                        pressed: true,
                        modifiers: egui::Modifiers::default(),
                    },
                ],
            ),
            |ctx| press_result = modal.ui(ctx),
        );
        press_output.drop_without_applying_deltas();
        assert_eq!(press_result, super::ConfirmationResult::None);
        assert!(modal.is_open());

        let mut release_result = super::ConfirmationResult::None;
        let release_output = ctx.run_ui(
            input(
                screen_rect,
                vec![egui::Event::PointerButton {
                    pos: close_point,
                    button: egui::PointerButton::Primary,
                    pressed: false,
                    modifiers: egui::Modifiers::default(),
                }],
            ),
            |ctx| release_result = modal.ui(ctx),
        );
        release_output.drop_without_applying_deltas();
        assert_eq!(release_result, super::ConfirmationResult::Cancelled);
        assert!(!modal.is_open());

        let mut after_close_result = super::ConfirmationResult::None;
        let after_close_output = ctx.run_ui(input(screen_rect, Vec::new()), |ctx| {
            after_close_result = modal.ui(ctx);
        });
        after_close_output.drop_without_applying_deltas();
        assert_eq!(after_close_result, super::ConfirmationResult::None);
    }

    #[test]
    fn every_universal_destructive_action_has_central_confirmation_metadata() {
        let legacy = Action {
            label: "target".into(),
            desc: "test".into(),
            action: "noop".into(),
            args: None,
        };
        for id in [
            action_ids::FOLDER_REMOVE,
            action_ids::BOOKMARK_REMOVE,
            action_ids::TIMER_CANCEL,
            action_ids::STOPWATCH_STOP,
            action_ids::SNIPPET_REMOVE,
            action_ids::TEMPFILE_DELETE,
            action_ids::NOTE_REMOVE,
            action_ids::CLIPBOARD_REMOVE,
            action_ids::WINDOW_CLOSE,
        ] {
            let action = UniversalAction {
                id: id.clone(),
                target: ActionTarget::Generic {
                    action: legacy.clone(),
                },
                presentation: ActionPresentation::new("Remove"),
                availability: ActionAvailability::Available,
                safety: ActionSafety::Destructive,
                operation: UniversalActionOperation::InvokePrimary(legacy.clone()),
            };
            assert!(
                DestructiveAction::from_universal_action(&action).is_some(),
                "missing metadata for {id}"
            );
        }
    }
}
#[derive(Debug, Clone)]
pub struct ConfirmationModal {
    open: bool,
    title: String,
    description: String,
    warning: String,
    confirm_label: String,
    cancel_label: String,
    source_label: Option<String>,
}

impl Default for ConfirmationModal {
    fn default() -> Self {
        Self {
            open: false,
            title: "Confirm destructive action".into(),
            description: String::new(),
            warning: "This action cannot be undone.".into(),
            confirm_label: "Confirm".into(),
            cancel_label: "Cancel".into(),
            source_label: None,
        }
    }
}

impl ConfirmationModal {
    pub(crate) fn close(&mut self) {
        self.open = false;
    }
    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    #[cfg(test)]
    pub(crate) fn source_copy(&self) -> Option<&str> {
        self.source_label.as_deref()
    }
    pub fn open_custom(&mut self, description: impl Into<String>, warning: impl Into<String>) {
        self.title = "Confirm destructive action".into();
        self.description = description.into();
        self.warning = warning.into();
        self.confirm_label = "Confirm".into();
        self.cancel_label = "Cancel".into();
        self.source_label = None;
        self.open = true;
    }
    pub fn open_for(&mut self, kind: DestructiveAction) {
        self.open_for_source(kind, None);
    }

    pub fn open_for_source(&mut self, kind: DestructiveAction, source: Option<ActivationSource>) {
        self.title = "Confirm destructive action".into();
        self.description = kind.label().into();
        self.warning = kind.warning().into();
        self.confirm_label = "Confirm".into();
        self.cancel_label = "Cancel".into();
        self.source_label = source.map(|source| format!("Triggered by {}", source.label()));
        self.open = true;
    }

    pub fn ui(&mut self, ctx: &egui::Context) -> ConfirmationResult {
        self.ui_inner(ctx, None, None, true, true)
    }

    /// Render this confirmation on a caller-owned transient layer and collect
    /// only the current Confirm/Cancel responses through its optional scope.
    pub(crate) fn ui_with_jump_mode(
        &mut self,
        ctx: &egui::Context,
        window_id: egui::Id,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        authorized_owner_pass: bool,
    ) -> ConfirmationResult {
        self.ui_inner(
            ctx,
            Some(window_id),
            scope,
            native_ui_owner,
            authorized_owner_pass,
        )
    }

    fn ui_inner(
        &mut self,
        ctx: &egui::Context,
        window_id: Option<egui::Id>,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        native_ui_owner: bool,
        authorized_owner_pass: bool,
    ) -> ConfirmationResult {
        if !self.open {
            return ConfirmationResult::None;
        }
        let mut result = ConfirmationResult::None;
        let mut open = true;
        let mut visual_order = 0;
        let mut window = egui::Window::new(self.title.clone());
        if let Some(window_id) = window_id {
            window = window.id(window_id);
        }
        let mut content = egui::ScrollArea::vertical();
        content = if let Some(window_id) = window_id {
            content.id_salt(("destructive_confirmation_content", window_id))
        } else {
            content.id_salt("destructive_confirmation_content")
        };
        window
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_height = confirmation_footer_reserved_height(
                    ui,
                    &[&self.confirm_label, &self.cancel_label],
                );
                content
                    .min_scrolled_height(0.0)
                    .max_height((ui.available_height() - footer_height).max(0.0))
                    .show(ui, |ui| {
                        if !self.description.is_empty() {
                            ui.label(&self.description);
                        }
                        if let Some(label) = &self.source_label {
                            widgets::secondary_help(ui, label);
                        }
                        widgets::semantic_status(ui, StatusTone::Warning, &self.warning, None);
                    });
                ui.separator();
                ui.add_enabled_ui(native_ui_owner, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        let confirm = ui.button(&self.confirm_label);
                        let confirm =
                            widgets::with_button_emphasis(ui, confirm, ButtonEmphasis::Destructive);
                        let _ = crate::gui::jump_mode::ui::register_and_apply_target(
                            scope,
                            ui,
                            &confirm,
                            "confirmation/confirm",
                            "Confirm",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        );
                        if confirm.clicked() {
                            result = ConfirmationResult::Confirmed;
                        }
                        let cancel = ui.button(&self.cancel_label);
                        let _ = crate::gui::jump_mode::ui::register_and_apply_target(
                            scope,
                            ui,
                            &cancel,
                            "confirmation/cancel",
                            "Cancel",
                            crate::gui::jump_mode::TargetKind::Button,
                            &mut visual_order,
                        );
                        if cancel.clicked() {
                            result = ConfirmationResult::Cancelled;
                        }
                    });
                });
            });

        if !authorized_owner_pass {
            return ConfirmationResult::None;
        }
        if result != ConfirmationResult::None || !open {
            self.open = false;
            if !open && result == ConfirmationResult::None {
                result = ConfirmationResult::Cancelled;
            }
        }
        result
    }
}

/// Confirmation state for a captured Diff copy plan.  The plan is owned by the
/// modal so later tree-selection changes cannot retarget the operation.
#[derive(Debug, Default)]
pub struct DiffCopyConfirmation {
    plan: Option<crate::diff::file_ops::CopyPlan>,
}

impl DiffCopyConfirmation {
    pub fn open(&mut self, plan: crate::diff::file_ops::CopyPlan) {
        self.plan = Some(plan);
    }

    pub fn pending(&self) -> Option<&crate::diff::file_ops::CopyPlan> {
        self.plan.as_ref()
    }

    /// Returns the fixed plan only after an enabled Confirm button is pressed.
    pub fn ui(&mut self, ctx: &egui::Context) -> Option<crate::diff::file_ops::CopyPlan> {
        let plan = self.plan.as_ref()?;
        let mut confirm = false;
        let mut cancel = false;
        let mut open = true;
        egui::Window::new("Confirm file operation")
            .collapsible(false)
            .resizable(true)
            .anchor(egui::Align2::CENTER_CENTER, [0.0, 0.0])
            .open(&mut open)
            .show(ctx, |ui| {
                let footer_height = confirmation_footer_reserved_height(ui, &["Confirm", "Cancel"]);
                egui::ScrollArea::vertical()
                    .id_salt("diff_copy_confirmation_content")
                    .min_scrolled_height(0.0)
                    .max_height((ui.available_height() - footer_height).max(0.0))
                    .show(ui, |ui| {
                        ui.label(format!(
                            "{} files, {} directories",
                            plan.totals.files_copied, plan.totals.directories_created
                        ));
                        ui.label(format!(
                            "{} overwrites, {} conflicts, {} skipped",
                            plan.totals.overwrites, plan.totals.conflicts, plan.totals.skips
                        ));
                        for conflict in &plan.conflicts {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Warning,
                                &format!("{}: {}", conflict.relative.display(), conflict.message),
                                None,
                            );
                        }
                        for error in &plan.errors {
                            widgets::semantic_status(ui, StatusTone::Error, &error.message, None);
                        }
                        if plan.totals.overwrites > 0 {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Warning,
                                "Existing differing files will be overwritten.",
                                None,
                            );
                        }
                    });
                ui.separator();
                ui.horizontal_wrapped(|ui| {
                    let confirm_button = ui.add_enabled(
                        !plan.has_fatal_errors(),
                        egui::Button::new("Confirm").wrap_mode(ui.wrap_mode()),
                    );
                    let confirm_button = widgets::with_button_emphasis(
                        ui,
                        confirm_button,
                        ButtonEmphasis::Destructive,
                    );
                    if confirm_button.clicked() {
                        confirm = true;
                    }
                    if ui.button("Cancel").clicked() {
                        cancel = true;
                    }
                });
            });
        if confirm {
            return self.plan.take();
        }
        if cancel || !open {
            self.plan = None;
        }
        None
    }
}

fn confirmation_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let native_button_style = style.button_style(
        &egui::widget_style::Classes::default(),
        egui::widget_style::WidgetState::Inactive,
    );
    let button_font = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| native_button_style.text_style.font_id.clone());
    let button_frame = native_button_style.frame;
    let frame_margin = if ui.visuals().button_frame {
        button_frame.total_margin().sum()
    } else {
        egui::Vec2::ZERO
    };
    let available_width = ui.available_width().max(1.0);
    let text_width = (available_width - frame_margin.x).max(1.0);
    let wrap_mode = ui.wrap_mode();
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let mut row_width = 0.0;
    let mut row_height = 0.0f32;
    let mut actions_height = 0.0;

    for label in labels {
        let text_size = egui::WidgetText::from(*label)
            .into_galley(ui, Some(wrap_mode), text_width, button_font.clone())
            .size();
        let button_width = (text_size.x + frame_margin.x)
            .max(spacing.interact_size.x)
            .min(available_width);
        let button_height = (text_size.y + spacing.extra_text_line_spacing + frame_margin.y)
            .max(spacing.interact_size.y);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            actions_height += row_height + spacing.item_spacing.y;
            row_width = button_width;
            row_height = button_height;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
            row_height = row_height.max(button_height);
        }
    }
    actions_height += row_height.max(spacing.interact_size.y);
    let separator_spacing = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let footer_height = actions_height + 2.0 * spacing.item_spacing.y + separator_spacing;
    (footer_height * pixels_per_point).ceil() / pixels_per_point
}
