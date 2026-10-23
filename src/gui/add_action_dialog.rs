use crate::actions::Action;
#[cfg(test)]
use crate::actions_editor::ScrollLayoutObservation;
use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{
    ButtonEmphasis, StatusTone, compact_section_heading, labeled_form_row, secondary_help,
    semantic_status, with_button_emphasis,
};
use crate::target_repair::{FilesystemTargetKind, RepairCompletion, SelectedTarget};
use eframe::egui;
use rfd::FileDialog;
use std::sync::atomic::{AtomicU64, Ordering};

#[cfg(target_os = "windows")]
struct RepairDialogParent(std::num::NonZeroIsize);

#[cfg(target_os = "windows")]
impl raw_window_handle::HasWindowHandle for RepairDialogParent {
    fn window_handle(
        &self,
    ) -> Result<raw_window_handle::WindowHandle<'_>, raw_window_handle::HandleError> {
        let handle = raw_window_handle::Win32WindowHandle::new(self.0);
        // SAFETY: this adapter is created only from the live root HWND published
        // by the current LauncherApp frame and is used synchronously by rfd.
        Ok(unsafe { raw_window_handle::WindowHandle::borrow_raw(handle.into()) })
    }
}

#[cfg(target_os = "windows")]
impl raw_window_handle::HasDisplayHandle for RepairDialogParent {
    fn display_handle(
        &self,
    ) -> Result<raw_window_handle::DisplayHandle<'_>, raw_window_handle::HandleError> {
        Ok(raw_window_handle::DisplayHandle::windows())
    }
}

#[cfg(feature = "qa-framebuffer-capture")]
use serde::Serialize;

static NEXT_REPAIR_PRESENTATION_TOKEN: AtomicU64 = AtomicU64::new(1);

/// Dialog state used when adding a new user defined command.
///
/// The struct holds the text the user has entered as well as an `open`
/// flag indicating if the dialog should currently be visible.
pub struct AddActionDialog {
    /// `true` while the dialog window is displayed.
    pub open: bool,
    /// Command label being edited.
    label: String,
    /// Command description being edited.
    desc: String,
    /// Path to the executable or file to launch.
    path: String,
    /// Whether the arguments field is visible.
    show_args: bool,
    /// Additional arguments to pass when launching.
    args: String,
    /// Current dialog mode (add or edit existing command).
    mode: DialogMode,
    /// Inline feedback from validation or failed publication.
    feedback: Option<String>,
    #[cfg(test)]
    pub(crate) test_form_layout: Option<AddActionLayoutObservation>,
    #[cfg(test)]
    test_picker_result: Option<Option<std::path::PathBuf>>,
    #[cfg(test)]
    test_picker_kinds: Vec<FilesystemTargetKind>,
    #[cfg(feature = "qa-framebuffer-capture")]
    qa_repair_paint: Option<TargetRepairDialogPaint>,
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Debug, Serialize)]
pub(crate) struct TargetRepairDialogPaint {
    pub(crate) token: u64,
    pub(crate) target_owner: &'static str,
    pub(crate) record_index: usize,
    pub(crate) owner_revision: u64,
    pub(crate) replacement_kind: &'static str,
    pub(crate) pending: bool,
    pub(crate) feedback_present: bool,
    pub(crate) window_visible: bool,
    pub(crate) sizing_pass: bool,
    pub(crate) egui_frame_nr: u64,
    pub(crate) egui_pass_index: usize,
    #[serde(skip)]
    pub(crate) window_layer_id: Option<egui::LayerId>,
    pub(crate) replacement_is_empty: bool,
    pub(crate) controls: Vec<TargetRepairControlPaint>,
}

#[cfg(feature = "qa-framebuffer-capture")]
#[derive(Clone, Debug, Serialize)]
pub(crate) struct TargetRepairControlPaint {
    pub(crate) name: &'static str,
    pub(crate) response_id: String,
    pub(crate) rect: [f32; 4],
    pub(crate) enabled: bool,
    pub(crate) painted: bool,
}

#[cfg(feature = "qa-framebuffer-capture")]
fn target_repair_control_paint(
    ui: &egui::Ui,
    name: &'static str,
    response: &egui::Response,
) -> TargetRepairControlPaint {
    let rect = super::qa_framebuffer::qa_clipped_response_rect(response.rect, ui.clip_rect());
    TargetRepairControlPaint {
        name,
        response_id: format!("{:?}", response.id),
        rect: [rect.min.x, rect.min.y, rect.max.x, rect.max.y],
        enabled: response.enabled() && ui.is_enabled(),
        painted: ui.is_visible() && !ui.is_sizing_pass() && rect.is_positive(),
    }
}

/// Mode of the action dialog.
#[derive(Default)]
enum DialogMode {
    /// Add a new command when confirmed.
    #[default]
    Add,
    /// Edit the command at the given index when confirmed.
    Edit(usize),
    Repair {
        selected: SelectedTarget,
        replacement_kind: FilesystemTargetKind,
        token: u64,
        pending: bool,
    },
}

impl Default for AddActionDialog {
    fn default() -> Self {
        Self {
            open: false,
            label: String::new(),
            desc: String::new(),
            path: String::new(),
            show_args: false,
            args: String::new(),
            mode: DialogMode::Add,
            feedback: None,
            #[cfg(test)]
            test_form_layout: None,
            #[cfg(test)]
            test_picker_result: None,
            #[cfg(test)]
            test_picker_kinds: Vec::new(),
            #[cfg(feature = "qa-framebuffer-capture")]
            qa_repair_paint: None,
        }
    }
}

impl AddActionDialog {
    /// Start the dialog for creating a new command.
    pub fn open_add(&mut self) {
        self.open_add_with_path("");
    }

    /// Start the dialog for creating a new command using `path` as the
    /// pre-filled executable.
    pub fn open_add_with_path(&mut self, path: &str) {
        self.open = true;
        self.label.clear();
        self.desc.clear();
        self.path = path.into();
        self.args.clear();
        self.show_args = false;
        self.feedback = None;
        self.mode = DialogMode::Add;
    }

    /// Start editing an existing command at `idx` with the given values.
    pub fn open_edit(&mut self, idx: usize, act: &Action) {
        self.open = true;
        self.label = act.label.clone();
        self.desc = act.desc.clone();
        self.path = act.action.clone();
        self.args = act.args.clone().unwrap_or_default();
        self.show_args = act.args.is_some();
        self.feedback = None;
        self.mode = DialogMode::Edit(idx);
    }

    pub fn open_repair(&mut self, selected: SelectedTarget) {
        self.open = true;
        self.path.clear();
        self.show_args = false;
        self.args.clear();
        self.feedback = None;
        let replacement_kind = if selected.is_folder_entry() {
            FilesystemTargetKind::Folder
        } else {
            FilesystemTargetKind::File
        };
        self.mode = DialogMode::Repair {
            selected,
            replacement_kind,
            token: NEXT_REPAIR_PRESENTATION_TOKEN
                .fetch_add(1, Ordering::Relaxed)
                .max(1),
            pending: false,
        };
    }

    pub(crate) fn retire_repair_presentation(&mut self) {
        if matches!(&self.mode, DialogMode::Repair { .. }) {
            self.open = false;
            self.path.clear();
            self.feedback = None;
            self.mode = DialogMode::Add;
            #[cfg(feature = "qa-framebuffer-capture")]
            {
                self.qa_repair_paint = None;
            }
        }
    }

    pub(crate) fn is_repair_presentation_open(&self) -> bool {
        self.open && matches!(&self.mode, DialogMode::Repair { .. })
    }

    #[cfg(test)]
    pub(crate) fn set_test_picker_result(&mut self, result: Option<std::path::PathBuf>) {
        self.test_picker_result = Some(result);
    }

    fn pick_repair_path(
        &mut self,
        kind: FilesystemTargetKind,
        root_hwnd: Option<usize>,
    ) -> Result<Option<std::path::PathBuf>, String> {
        #[cfg(test)]
        self.test_picker_kinds.push(kind);
        #[cfg(test)]
        if let Some(result) = self.test_picker_result.take() {
            return Ok(result);
        }
        #[cfg(target_os = "windows")]
        let dialog = {
            let hwnd = root_hwnd.ok_or_else(|| {
                "The launcher window is not ready to own the repair picker.".to_owned()
            })?;
            let hwnd = std::num::NonZeroIsize::new(hwnd as isize)
                .ok_or_else(|| "The launcher window cannot own the repair picker.".to_owned())?;
            FileDialog::new().set_parent(&RepairDialogParent(hwnd))
        };
        #[cfg(not(target_os = "windows"))]
        let dialog = {
            let _ = root_hwnd;
            FileDialog::new()
        };
        Ok(match kind {
            FilesystemTargetKind::File => dialog.pick_file(),
            FilesystemTargetKind::Folder => dialog.pick_folder(),
        })
    }

    pub(crate) fn complete_target_repair(&mut self, completion: &RepairCompletion) -> bool {
        let DialogMode::Repair {
            selected,
            token,
            pending,
            ..
        } = &mut self.mode
        else {
            return false;
        };
        if !self.open
            || !*pending
            || !selected.same_capture(&completion.request.selected)
            || std::path::Path::new(&self.path) != completion.request.replacement
            || completion.request.presentation_token() != Some(*token)
        {
            return false;
        }
        *pending = false;
        match &completion.result {
            Ok(_) => {
                self.open = false;
                self.feedback = None;
            }
            Err(error) => self.feedback = Some(error.message.to_owned()),
        }
        true
    }

    #[cfg(test)]
    pub(crate) fn test_draft_fields(&self) -> (&str, &str, &str, bool, &str) {
        (
            &self.label,
            &self.desc,
            &self.path,
            self.show_args,
            &self.args,
        )
    }

    #[cfg(test)]
    pub(crate) fn test_feedback(&self) -> Option<&str> {
        self.feedback.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_repair_path(&self) -> Option<&str> {
        matches!(&self.mode, DialogMode::Repair { .. }).then_some(self.path.as_str())
    }

    #[cfg(test)]
    pub(crate) fn test_repair_token(&self) -> Option<u64> {
        match &self.mode {
            DialogMode::Repair { token, .. } => Some(*token),
            _ => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn test_set_repair_pending(&mut self, pending: bool) {
        if let DialogMode::Repair {
            pending: current, ..
        } = &mut self.mode
        {
            *current = pending;
        }
    }

    #[cfg(test)]
    pub(crate) fn test_repair_capture(&self) -> Option<SelectedTarget> {
        match &self.mode {
            DialogMode::Repair { selected, .. } => Some(selected.clone()),
            _ => None,
        }
    }

    /// Draw the "Add App" dialog and update `app` with any new action.
    ///
    /// * `ctx` - Egui context used to render the window.
    /// * `app` - Application state that will receive the new action if the
    ///   user confirms the dialog. The actions list is persisted on success.
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        #[cfg(feature = "qa-framebuffer-capture")]
        {
            self.qa_repair_paint = None;
        }
        if !self.open {
            return;
        }
        let mut should_close = false;
        let title = match &self.mode {
            DialogMode::Add => "Add App",
            DialogMode::Edit(_) => "Edit App",
            DialogMode::Repair { .. } => "Repair Target",
        };
        let button_label = match &self.mode {
            DialogMode::Add => "Add",
            DialogMode::Edit(_) => "Save",
            DialogMode::Repair { .. } => "Confirm repair",
        };
        let repair_pending = matches!(&self.mode, DialogMode::Repair { pending: true, .. });
        #[cfg(test)]
        let mut test_layout = None;
        #[cfg(test)]
        let mut test_visible = false;
        #[cfg(test)]
        let mut test_sizing_pass = false;
        #[cfg(test)]
        let mut browse_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut file_kind_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut folder_kind_rect = egui::Rect::NOTHING;
        #[cfg(test)]
        let mut browse_enabled = false;
        #[cfg(feature = "qa-framebuffer-capture")]
        let mut qa_controls = Vec::new();
        #[cfg(feature = "qa-framebuffer-capture")]
        let mut qa_window_visible = false;
        #[cfg(feature = "qa-framebuffer-capture")]
        let mut qa_sizing_pass = false;
        let mut repair_kind_changed = false;
        let mut window_open = self.open;
        let window_output = egui::Window::new(title)
            .open(&mut window_open)
            .resizable(true)
            .max_width(ctx.content_rect().width().max(1.0))
            .max_height(ctx.content_rect().height().max(1.0))
            .show(ctx, |ui| {
                #[cfg(test)]
                {
                    test_visible = ui.is_visible();
                    test_sizing_pass = ui.is_sizing_pass();
                }
                #[cfg(feature = "qa-framebuffer-capture")]
                {
                    qa_window_visible = ui.is_visible();
                    qa_sizing_pass = ui.is_sizing_pass();
                }
                let repair_mode = matches!(&self.mode, DialogMode::Repair { .. });
                compact_section_heading(
                    ui,
                    if repair_mode { "Repair saved target" } else { "App details" },
                    None,
                );
                let mut repair_kind = match &self.mode {
                    DialogMode::Repair {
                        replacement_kind, ..
                    } => Some(*replacement_kind),
                    _ => None,
                };
                let footer_height = action_footer_reserved_height(ui, &[button_label, "Cancel"]);
                let form_max_rect = ui.max_rect();
                let form_clip_rect = ui.clip_rect();
                let body_height = (ui.available_height() - footer_height).max(0.0);
                let form_output = egui::ScrollArea::vertical()
                    .id_salt("add_action_form")
                    .auto_shrink([false, false])
                    .min_scrolled_height(0.0)
                    .max_height(body_height)
                    .show(ui, |ui| {
                        let mut changed = false;
                        if let DialogMode::Repair { selected, pending, .. } = &self.mode {
                            compact_section_heading(ui, "Saved target", None);
                            match selected {
                                SelectedTarget::Action { original, .. } => {
                                    ui.label(&original.label);
                                    if !original.desc.is_empty() {
                                        secondary_help(ui, &original.desc);
                                    }
                                    if let Some(args) = &original.args {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.weak("Arguments");
                                            ui.monospace(args);
                                        });
                                    }
                                }
                                SelectedTarget::Folder { original, .. } => {
                                    ui.label(&original.label);
                                    if let Some(alias) = &original.alias {
                                        ui.horizontal_wrapped(|ui| {
                                            ui.weak("Alias");
                                            ui.label(alias);
                                        });
                                    }
                                }
                            }
                            ui.label(selected.original_path());
                            secondary_help(
                                ui,
                                "Choose an existing replacement. Repair updates the saved target and its supported references; it never launches it.",
                            );
                            if !selected.is_folder_entry() {
                                ui.add_enabled_ui(!*pending, |ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.label("Replacement type");
                                        let file = ui.selectable_value(
                                            repair_kind.as_mut().expect("repair mode has a kind"),
                                            FilesystemTargetKind::File,
                                            "File or application",
                                        );
                                        let folder = ui.selectable_value(
                                            repair_kind.as_mut().expect("repair mode has a kind"),
                                            FilesystemTargetKind::Folder,
                                            "Folder",
                                        );
                                        repair_kind_changed = file.changed() || folder.changed();
                                        #[cfg(feature = "qa-framebuffer-capture")]
                                        {
                                            qa_controls.push(target_repair_control_paint(ui, "file_type", &file));
                                            qa_controls.push(target_repair_control_paint(ui, "folder_type", &folder));
                                        }
                                        #[cfg(test)]
                                        {
                                            file_kind_rect = file.rect;
                                            folder_kind_rect = folder.rect;
                                        }
                                    });
                                });
                            }
                            if *pending {
                                semantic_status(
                                    ui,
                                    StatusTone::Info,
                                    "Repair in progress",
                                    Some("The saved entry will update only after the repair commits. Closing this window does not stop the submitted repair."),
                                );
                            }
                        }
                        if let Some(feedback) = &self.feedback {
                            let heading = if repair_mode {
                                "Unable to repair target"
                            } else {
                                "Unable to save app"
                            };
                            semantic_status(ui, StatusTone::Error, heading, Some(feedback));
                        }
                        if !repair_mode {
                            changed |= labeled_form_row(ui, "Label", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.label)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            })
                            .changed();
                            changed |= labeled_form_row(ui, "Description", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.desc)
                                        .desired_width(ui.available_width().max(72.0)),
                                )
                            })
                            .changed();
                        }
                        let mut path_browsed = false;
                        let path = labeled_form_row(ui, "Path", |ui| {
                        let path = ui.add_enabled(
                            !repair_pending,
                                egui::TextEdit::singleline(&mut self.path)
                                    .desired_width(ui.available_width().max(72.0)),
                            );
                            if let Some(replacement_kind) = repair_kind {
                                let label = match replacement_kind {
                                    FilesystemTargetKind::File => "Choose file…",
                                    FilesystemTargetKind::Folder => "Choose folder…",
                                };
                                let browse = ui.add_enabled(!repair_pending, egui::Button::new(label));
                                #[cfg(feature = "qa-framebuffer-capture")]
                                qa_controls.push(target_repair_control_paint(ui, "browse", &browse));
                                #[cfg(test)]
                                {
                                    browse_rect = browse.rect;
                                    browse_enabled = browse.enabled();
                                }
                                if browse.clicked() {
                                    match self.pick_repair_path(replacement_kind, app.launcher_hwnd) {
                                        Ok(Some(selected)) => match selected.to_str() {
                                            Some(path) => {
                                                self.path = path.to_owned();
                                                path_browsed = true;
                                            }
                                            None => self.feedback = Some("The selected path cannot be represented as text.".into()),
                                        },
                                        Ok(None) => {},
                                        Err(error) => self.feedback = Some(error),
                                    }
                                }
                            } else {
                                let browse = ui.add_enabled(!repair_pending, egui::Button::new("Browse"));
                                #[cfg(feature = "qa-framebuffer-capture")]
                                qa_controls.push(target_repair_control_paint(ui, "browse", &browse));
                                #[cfg(test)]
                                {
                                    browse_rect = browse.rect;
                                    browse_enabled = browse.enabled();
                                }
                                if browse.clicked()
                                    && let Some(file) = FileDialog::new().pick_file()
                                {
                                    if let Some(path) = file.to_str() {
                                        self.path = path.to_owned();
                                    } else {
                                        self.path = file.display().to_string();
                                    }
                                    path_browsed = true;
                                }
                            }
                            #[cfg(feature = "qa-framebuffer-capture")]
                            qa_controls.push(target_repair_control_paint(ui, "path", &path));
                            path
                        });
                        changed |= path.changed() | path_browsed;

                        if !repair_mode {
                            ui.horizontal_wrapped(|ui| {
                                changed |= ui.checkbox(&mut self.show_args, "Add arguments").changed();
                                if self.show_args {
                                    changed |= ui
                                        .add(
                                            egui::TextEdit::singleline(&mut self.args)
                                                .desired_width(ui.available_width().max(72.0)),
                                        )
                                        .changed();
                                }
                            });
                        }
                        if changed {
                            self.feedback = None;
                        }
                        if let (Some(kind), DialogMode::Repair { replacement_kind, .. }) =
                            (repair_kind, &mut self.mode)
                        {
                            *replacement_kind = kind;
                        }
                    });

                ui.separator();
                let mut primary_rect = egui::Rect::NOTHING;
                let mut cancel_rect = egui::Rect::NOTHING;
                ui.horizontal_wrapped(|ui| {
                    let primary = ui.add_enabled(!repair_pending, egui::Button::new(button_label));
                    let primary = with_button_emphasis(ui, primary, ButtonEmphasis::Primary);
                    primary_rect = primary.rect;
                    #[cfg(feature = "qa-framebuffer-capture")]
                    qa_controls.push(target_repair_control_paint(ui, "confirm", &primary));
                    if primary.clicked() && !repair_pending {
                        use std::path::Path;
                        if matches!(&self.mode, DialogMode::Repair { .. }) {
                            let selected = match &self.mode {
                                DialogMode::Repair { selected, .. } => selected.clone(),
                                _ => unreachable!(),
                            };
                            let replacement = std::path::PathBuf::from(&self.path);
                            let kind = match &self.mode {
                                DialogMode::Repair { replacement_kind, .. } => *replacement_kind,
                                _ => unreachable!(),
                            };
                            let token = match &self.mode {
                                DialogMode::Repair { token, .. } => *token,
                                _ => unreachable!(),
                            };
                            match app.submit_target_repair(selected, replacement, kind, token) {
                                Ok(()) => {
                                    if let DialogMode::Repair { pending, .. } = &mut self.mode {
                                        *pending = true;
                                    }
                                    self.feedback = None;
                                }
                                Err(error) => self.feedback = Some(error.message.to_owned()),
                            }
                        } else if self.path.is_empty() || !Path::new(&self.path).exists() {
                            let message = "Path does not exist".to_owned();
                            self.feedback = Some(message.clone());
                            app.report_error_message("ui operation", message);
                        } else {
                            let candidate = Action {
                                label: self.label.clone(),
                                desc: self.desc.clone(),
                                action: self.path.clone(),
                                args: if self.show_args && !self.args.trim().is_empty() {
                                    Some(self.args.clone())
                                } else {
                                    None
                                },
                            };
                            let result = match &self.mode {
                                DialogMode::Add => app.update_custom_actions(move |actions| {
                                    actions.push(candidate);
                                    Ok(())
                                }),
                                DialogMode::Edit(idx) => {
                                    let idx = *idx;
                                    app.update_custom_actions(move |actions| {
                                        let action = actions.get_mut(idx).ok_or_else(|| {
                                            anyhow::anyhow!("custom action no longer exists")
                                        })?;
                                        *action = candidate;
                                        Ok(())
                                    })
                                }
                                DialogMode::Repair { .. } => unreachable!(),
                            };
                            match result {
                                Ok(()) => {
                                    self.label.clear();
                                    self.desc.clear();
                                    self.path.clear();
                                    self.args.clear();
                                    self.show_args = false;
                                    self.feedback = None;
                                    should_close = true;
                                }
                                Err(error) => {
                                    let message = format!("Failed to save: {error}");
                                    self.feedback = Some(message.clone());
                                    app.report_error_message("ui operation", message);
                                }
                            }
                        }
                    }

                    let cancel = ui.add_enabled(!repair_pending, egui::Button::new("Cancel"));
                    cancel_rect = cancel.rect;
                    #[cfg(feature = "qa-framebuffer-capture")]
                    qa_controls.push(target_repair_control_paint(ui, "cancel", &cancel));
                    if cancel.clicked() {
                        if matches!(&self.mode, DialogMode::Repair { .. }) {
                            self.mode = DialogMode::Add;
                        }
                        should_close = true;
                    }
                });

                #[cfg(test)]
                {
                    test_layout = Some(AddActionLayoutObservation {
                        scroll: ScrollLayoutObservation {
                            id: form_output.id,
                            visible: test_visible,
                            sizing_pass: test_sizing_pass,
                            parent_max_rect: form_max_rect,
                            parent_clip_rect: form_clip_rect,
                            inner_rect: form_output.inner_rect,
                            content_size: form_output.content_size,
                            offset: form_output.state.offset,
                        },
                        visible: test_visible,
                        sizing_pass: test_sizing_pass,
                        primary_rect,
                        cancel_rect,
                        window_rect: egui::Rect::NOTHING,
                        browse_rect,
                        file_kind_rect,
                        folder_kind_rect,
                        browse_enabled,
                    });
                }
                #[cfg(not(test))]
                let _ = (
                    form_max_rect,
                    form_clip_rect,
                    form_output,
                    primary_rect,
                    cancel_rect,
                );
            });

        #[cfg(feature = "qa-framebuffer-capture")]
        let qa_window_layer_id = window_output
            .as_ref()
            .map(|window| window.response.layer_id);

        #[cfg(test)]
        {
            if let Some(mut layout) = test_layout {
                layout.window_rect = window_output
                    .as_ref()
                    .map_or(egui::Rect::NOTHING, |window| window.response.rect);
                self.test_form_layout = Some(layout);
            }
        }
        #[cfg(not(test))]
        let _ = window_output;

        self.open = window_open;
        if repair_kind_changed {
            self.feedback = None;
        }

        if should_close {
            self.open = false;
        }
        #[cfg(feature = "qa-framebuffer-capture")]
        if self.open
            && let DialogMode::Repair {
                selected,
                replacement_kind,
                token,
                pending,
            } = &self.mode
        {
            let (target_owner, record_index, owner_revision) = match selected {
                SelectedTarget::Action {
                    index,
                    actions_version,
                    ..
                } => ("custom_action", *index, *actions_version),
                SelectedTarget::Folder {
                    index,
                    folders_version,
                    ..
                } => ("folder", *index, *folders_version),
            };
            let replacement_kind = match replacement_kind {
                FilesystemTargetKind::File => "file",
                FilesystemTargetKind::Folder => "folder",
            };
            self.qa_repair_paint = Some(TargetRepairDialogPaint {
                token: *token,
                target_owner,
                record_index,
                owner_revision,
                replacement_kind,
                pending: *pending,
                feedback_present: self.feedback.is_some(),
                window_visible: qa_window_visible,
                sizing_pass: qa_sizing_pass,
                egui_frame_nr: ctx.cumulative_frame_nr(),
                egui_pass_index: ctx.current_pass_index(),
                window_layer_id: qa_window_layer_id,
                replacement_is_empty: self.path.is_empty(),
                controls: qa_controls,
            });
        }
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_repair_paint_for_current_pass(
        &self,
        ctx: &egui::Context,
    ) -> Option<TargetRepairDialogPaint> {
        let paint = self.qa_repair_paint.as_ref()?;
        (paint.egui_frame_nr == ctx.cumulative_frame_nr()
            && paint.egui_pass_index == ctx.current_pass_index())
        .then(|| paint.clone())
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_target_repair_path_matches(&self, expected: &str) -> bool {
        self.open
            && matches!(&self.mode, DialogMode::Repair { .. })
            && std::path::Path::new(&self.path) == std::path::Path::new(expected)
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_target_repair_path_is_empty(&self) -> bool {
        self.open && matches!(&self.mode, DialogMode::Repair { .. }) && self.path.is_empty()
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_target_repair_presentation_matches(
        &self,
        token: u64,
        target_owner: &str,
        record_index: usize,
        owner_revision: u64,
        replacement_kind: &str,
        pending: bool,
        feedback_present: bool,
    ) -> bool {
        let DialogMode::Repair {
            selected,
            replacement_kind: current_kind,
            token: current_token,
            pending: current_pending,
        } = &self.mode
        else {
            return false;
        };
        let (current_owner, current_index, current_revision) = match selected {
            SelectedTarget::Action {
                index,
                actions_version,
                ..
            } => ("custom_action", *index, *actions_version),
            SelectedTarget::Folder {
                index,
                folders_version,
                ..
            } => ("folder", *index, *folders_version),
        };
        let current_kind = match current_kind {
            FilesystemTargetKind::File => "file",
            FilesystemTargetKind::Folder => "folder",
        };

        self.open
            && current_token == &token
            && current_owner == target_owner
            && current_index == record_index
            && current_revision == owner_revision
            && current_kind == replacement_kind
            && *current_pending == pending
            && self.feedback.is_some() == feedback_present
    }

    #[cfg(feature = "qa-framebuffer-capture")]
    pub(crate) fn qa_target_repair_paint_matches_current_state(
        &self,
        token: u64,
        target_owner: &str,
        record_index: usize,
        owner_revision: u64,
        replacement_kind: &str,
        pending: bool,
        feedback_present: bool,
        window_layer_id: egui::LayerId,
        egui_frame_nr: u64,
        egui_pass_index: usize,
    ) -> bool {
        self.qa_target_repair_presentation_matches(
            token,
            target_owner,
            record_index,
            owner_revision,
            replacement_kind,
            pending,
            feedback_present,
        ) && self.qa_repair_paint.as_ref().is_some_and(|paint| {
            paint.token == token
                && paint.target_owner == target_owner
                && paint.record_index == record_index
                && paint.owner_revision == owner_revision
                && paint.replacement_kind == replacement_kind
                && paint.pending == pending
                && paint.feedback_present == feedback_present
                && paint.window_visible
                && !paint.sizing_pass
                && paint.window_layer_id == Some(window_layer_id)
                && paint.egui_frame_nr == egui_frame_nr
                && paint.egui_pass_index == egui_pass_index
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plugin::PluginManager;
    use crate::settings::Settings;
    use std::sync::{Arc, atomic::AtomicBool};
    use tempfile::tempdir;

    fn test_app(ctx: &egui::Context, root: &std::path::Path) -> LauncherApp {
        let mut settings = Settings::default();
        settings.hotkey = None;
        settings.quit_hotkey = None;
        settings.help_hotkey = None;
        settings.paste_sequence.next_hotkey = None;
        settings.paste_sequence.cancel_hotkey = None;
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new_inert_for_test(),
            root.join("actions.json").to_string_lossy().into_owned(),
            root.join("settings.json").to_string_lossy().into_owned(),
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

    fn run_frame(
        ctx: &egui::Context,
        dialog: &mut AddActionDialog,
        app: &mut LauncherApp,
        frame: &mut usize,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(760.0, 620.0),
            )),
            time: Some(1.0 + *frame as f64 / 60.0),
            focused: true,
            events,
            ..Default::default()
        };
        *frame += 1;
        let output = ctx.run_ui(input, |ctx| dialog.ui(ctx, app));
        output.drop_without_applying_deltas();
    }

    fn click_at(
        ctx: &egui::Context,
        dialog: &mut AddActionDialog,
        app: &mut LauncherApp,
        frame: &mut usize,
        point: egui::Pos2,
    ) {
        run_frame(
            ctx,
            dialog,
            app,
            frame,
            vec![egui::Event::PointerMoved(point)],
        );
        for pressed in [true, false] {
            run_frame(
                ctx,
                dialog,
                app,
                frame,
                vec![egui::Event::PointerButton {
                    pos: point,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }

    fn current_layout(dialog: &AddActionDialog) -> AddActionLayoutObservation {
        dialog
            .test_form_layout
            .filter(|layout| layout.visible && !layout.sizing_pass)
            .expect("the actual repair dialog responses were rendered")
    }

    #[test]
    fn target_repair_ui_uses_explicit_typed_picker_and_confirm() {
        let root = tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, root.path());
        let original = Action {
            label: "Saved app".into(),
            desc: "Keep metadata".into(),
            action: root
                .path()
                .join("missing.exe")
                .to_string_lossy()
                .into_owned(),
            args: Some("--profile=blue".into()),
        };
        app.actions = Arc::new(vec![original.clone()]);
        app.custom_len = 1;
        let actions_before = Arc::clone(&app.actions);
        let action_file = std::path::PathBuf::from(&app.actions_path);
        let replacement_folder = root.path().join("replacement-folder");
        std::fs::create_dir(&replacement_folder).unwrap();
        let replacement_file = root.path().join("replacement.exe");
        std::fs::write(&replacement_file, b"owned fixture").unwrap();

        let mut dialog = AddActionDialog::default();
        dialog.open_repair(SelectedTarget::action_with_kind(
            original,
            0,
            crate::target_repair::ActionTargetKind::FileOrFolder,
        ));
        let mut frame = 0;
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new()); // Window sizing pass.
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new());
        let layout = current_layout(&dialog);
        assert!(layout.browse_enabled && layout.browse_rect.is_positive());
        assert!(layout.file_kind_rect.is_positive() && layout.folder_kind_rect.is_positive());

        dialog.set_test_picker_result(None);
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            layout.browse_rect.center(),
        );
        assert_eq!(
            dialog.test_draft_fields().2,
            "",
            "picker cancellation keeps the draft unchanged"
        );
        assert_eq!(dialog.test_picker_kinds, [FilesystemTargetKind::File]);
        assert!(
            dialog.test_picker_result.is_none(),
            "the cancelled chooser result is consumed"
        );

        let layout = current_layout(&dialog);
        dialog.set_test_picker_result(Some(replacement_file.clone()));
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            layout.browse_rect.center(),
        );
        assert_eq!(
            dialog.test_picker_kinds,
            [FilesystemTargetKind::File, FilesystemTargetKind::File]
        );
        assert_eq!(
            dialog.test_draft_fields().2,
            replacement_file.to_str().unwrap()
        );

        let folder_kind = current_layout(&dialog).folder_kind_rect;
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            folder_kind.center(),
        );
        let layout = current_layout(&dialog);
        dialog.set_test_picker_result(Some(replacement_folder.clone()));
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            layout.browse_rect.center(),
        );
        assert_eq!(
            dialog.test_picker_kinds,
            [
                FilesystemTargetKind::File,
                FilesystemTargetKind::File,
                FilesystemTargetKind::Folder
            ]
        );
        assert_eq!(
            dialog.test_draft_fields().2,
            replacement_folder.to_str().unwrap()
        );
        assert!(Arc::ptr_eq(&app.actions, &actions_before));
        assert!(
            !action_file.exists(),
            "choosing a replacement does not persist or run it"
        );

        let confirm = current_layout(&dialog).primary_rect;
        click_at(&ctx, &mut dialog, &mut app, &mut frame, confirm.center());
        assert!(
            dialog.open,
            "a failed explicit submission retains the repair draft"
        );
        assert_eq!(
            dialog.test_draft_fields().2,
            replacement_folder.to_str().unwrap()
        );
        assert_eq!(
            dialog.test_feedback(),
            Some("Target repair is unavailable.")
        );
        assert!(Arc::ptr_eq(&app.actions, &actions_before));
        assert!(!action_file.exists());

        let retry_as_file = current_layout(&dialog).file_kind_rect;
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            retry_as_file.center(),
        );
        assert_eq!(
            dialog.test_feedback(),
            None,
            "changing replacement type clears stale feedback"
        );
        assert_eq!(
            dialog.test_draft_fields().2,
            replacement_folder.to_str().unwrap()
        );
        assert!(Arc::ptr_eq(&app.actions, &actions_before));

        let cancel = current_layout(&dialog).cancel_rect;
        click_at(&ctx, &mut dialog, &mut app, &mut frame, cancel.center());
        assert!(!dialog.open);
        assert!(Arc::ptr_eq(&app.actions, &actions_before));
        assert!(!action_file.exists());
    }

    #[test]
    fn target_repair_ui_folder_rows_only_offer_folder_picker_and_ignore_stale_completion() {
        let root = tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, root.path());
        let original = crate::plugins::folders::FolderEntry {
            label: "Saved folder".into(),
            path: root
                .path()
                .join("missing-folder")
                .to_string_lossy()
                .into_owned(),
            alias: Some("Projects".into()),
        };
        let selected = SelectedTarget::folder(original, 0);
        let replacement = root.path().join("replacement-folder");
        std::fs::create_dir(&replacement).unwrap();
        let mut dialog = AddActionDialog::default();
        dialog.open_repair(selected.clone());
        let mut frame = 0;
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new());
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new());
        let layout = current_layout(&dialog);
        assert!(layout.file_kind_rect.is_negative() && layout.folder_kind_rect.is_negative());
        dialog.set_test_picker_result(Some(replacement.clone()));
        click_at(
            &ctx,
            &mut dialog,
            &mut app,
            &mut frame,
            layout.browse_rect.center(),
        );
        assert_eq!(dialog.test_picker_kinds, [FilesystemTargetKind::Folder]);
        assert_eq!(dialog.test_draft_fields().2, replacement.to_str().unwrap());

        let stale_token = dialog.test_repair_token().unwrap();
        let stale_request =
            crate::target_repair::RepairRequest::new(selected.clone(), replacement.clone())
                .with_expected_replacement_kind(FilesystemTargetKind::Folder)
                .with_presentation_token(stale_token);
        if let DialogMode::Repair { pending, .. } = &mut dialog.mode {
            *pending = true;
        }
        dialog.open = false;
        dialog.open_repair(selected.clone());
        let current_token = dialog.test_repair_token().unwrap();
        assert_ne!(current_token, stale_token);
        dialog.path = replacement.to_string_lossy().into_owned();
        if let DialogMode::Repair { pending, .. } = &mut dialog.mode {
            *pending = true;
        }
        let stale_completion = RepairCompletion {
            request: stale_request,
            result: Err(crate::target_repair::RepairFailure {
                kind: crate::target_repair::RepairFailureKind::WorkerUnavailable,
                message: "old completion",
            }),
        };
        assert!(!dialog.complete_target_repair(&stale_completion));
        assert!(dialog.open);
        assert_eq!(dialog.test_feedback(), None);
        assert_eq!(dialog.test_draft_fields().2, replacement.to_str().unwrap());
    }

    #[test]
    fn target_repair_ui_long_saved_metadata_keeps_confirm_and_cancel_reachable() {
        let root = tempdir().unwrap();
        let ctx = egui::Context::default();
        let mut app = test_app(&ctx, root.path());
        let original = Action {
            label: "Saved application ".repeat(18),
            desc: "A long saved description that must remain available in the scrollable repair preview. ".repeat(24),
            action: root.path().join("missing ".to_string() + &"path segment ".repeat(24) + "app.exe").to_string_lossy().into_owned(),
            args: Some("--metadata-argument ".repeat(28)),
        };
        let mut dialog = AddActionDialog::default();
        dialog.open_repair(SelectedTarget::action_with_kind(
            original,
            0,
            crate::target_repair::ActionTargetKind::FileOrFolder,
        ));
        let mut frame = 0;
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new());
        run_frame(&ctx, &mut dialog, &mut app, &mut frame, Vec::new());
        let layout = current_layout(&dialog);
        assert!(layout.scroll.content_size.y > layout.scroll.inner_rect.height());
        assert!(layout.primary_rect.is_positive() && layout.cancel_rect.is_positive());
        assert!(layout.window_rect.contains_rect(layout.primary_rect));
        assert!(layout.window_rect.contains_rect(layout.cancel_rect));
        assert!(layout.primary_rect.top() >= layout.scroll.inner_rect.bottom() - 1.0);
        assert!(layout.cancel_rect.top() >= layout.scroll.inner_rect.bottom() - 1.0);
        assert!(layout.primary_rect.intersects(layout.window_rect));
        assert!(layout.cancel_rect.intersects(layout.window_rect));
    }
}

#[cfg(test)]
#[derive(Clone, Copy)]
pub(crate) struct AddActionLayoutObservation {
    pub(crate) scroll: ScrollLayoutObservation,
    pub(crate) visible: bool,
    pub(crate) sizing_pass: bool,
    pub(crate) primary_rect: egui::Rect,
    pub(crate) cancel_rect: egui::Rect,
    pub(crate) window_rect: egui::Rect,
    pub(crate) browse_rect: egui::Rect,
    pub(crate) file_kind_rect: egui::Rect,
    pub(crate) folder_kind_rect: egui::Rect,
    pub(crate) browse_enabled: bool,
}

fn action_footer_reserved_height(ui: &mut egui::Ui, labels: &[&str]) -> f32 {
    let spacing = ui.spacing().clone();
    let style = ui.style().clone();
    let font_id = style
        .override_font_id
        .clone()
        .or_else(|| {
            style
                .override_text_style
                .as_ref()
                .map(|text_style| text_style.resolve(&style))
        })
        .unwrap_or_else(|| egui::TextStyle::Body.resolve(&style));
    let button_frame = style
        .button_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Inactive,
        )
        .frame;
    let button_height = (ui.fonts_mut(|fonts| fonts.row_height(&font_id))
        + spacing.extra_text_line_spacing
        + button_frame.total_margin().sum().y)
        .max(spacing.interact_size.y);
    let text_color = ui.visuals().text_color();
    let available_width = ui.available_width().max(1.0);
    let pixels_per_point = ui.ctx().pixels_per_point().max(f32::EPSILON);
    let mut row_width = 0.0;
    let mut row_count = 1_usize;
    for label in labels {
        let text_width = ui.fonts_mut(|fonts| {
            fonts
                .layout_no_wrap((*label).to_owned(), font_id.clone(), text_color)
                .size()
                .x
        });
        let button_width =
            (text_width + button_frame.total_margin().sum().x).max(spacing.interact_size.x);
        if row_width > 0.0 && row_width + spacing.item_spacing.x + button_width > available_width {
            row_count += 1;
            row_width = button_width;
        } else {
            if row_width > 0.0 {
                row_width += spacing.item_spacing.x;
            }
            row_width += button_width;
        }
    }
    let separator_height = style
        .separator_style(
            &egui::widget_style::Classes::default(),
            egui::widget_style::WidgetState::Noninteractive,
        )
        .spacing;
    let height = row_count as f32 * button_height
        + row_count.saturating_sub(1) as f32 * spacing.item_spacing.y
        + 2.0 * spacing.item_spacing.y
        + separator_height;
    (height * pixels_per_point).ceil() / pixels_per_point
}
