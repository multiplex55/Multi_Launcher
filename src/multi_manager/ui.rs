use crate::gui::LauncherApp;
use crate::gui::theme::widgets::{self, ButtonEmphasis, StatusTone};
use crate::multi_manager::bindings;
use crate::multi_manager::model::{
    MmBindingStatus, MmHotkey, MmHotkeyValidation, MmRect, MmWindow, MmWorkspace,
    PendingCaptureAction, new_workspace_id,
};
use crate::multi_manager::win;
use eframe::egui;
use std::sync::atomic::Ordering;
use std::time::Instant;

#[derive(Debug, Default)]
pub struct MultiManagerDialog {
    pub open: bool,
    all_expanded: bool,
    expand_all_signal: Option<bool>,
    rename: WorkspaceRenameState,
    hotkey_editor: HotkeyEditorState,
    hotkey_editor_needs_focus: bool,
    confirm: DeleteConfirmState,
    virtual_desktops: VirtualDesktopSnapshotState,
}

#[derive(Debug, Default)]
pub struct MultiManagerSettingsDialog {
    pub open: bool,
}

#[derive(Debug, Default)]
pub struct WorkspaceRenameState {
    workspace_id: String,
    value: String,
}

#[derive(Debug, Default)]
pub struct HotkeyEditorState {
    workspace_id: String,
    key: String,
    ctrl: bool,
    shift: bool,
    alt: bool,
    win: bool,
}

#[derive(Debug, Default)]
struct DeleteConfirmState {
    workspace_id: Option<String>,
    window: Option<(String, usize)>,
    reload: bool,
}

#[derive(Debug, Default)]
enum VirtualDesktopSnapshotState {
    #[default]
    Unverified,
    Ready(Vec<crate::virtual_desktop::VirtualDesktopInfo>),
    Failed(String),
}

impl MultiManagerDialog {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        app.multi_manager_poll_capture(ctx);
        if !self.open {
            return;
        }
        refresh_live_titles_if_due(app, ctx, false);
        let mut open = self.open;
        egui::Window::new("MultiManager")
            .open(&mut open)
            .vscroll(true)
            .show(ctx, |ui| {
                self.capture_banner(ui, app);
                ui.add_enabled_ui(app.multi_manager.pending_capture.is_none(), |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Add Workspace").clicked() {
                            add_workspace(app);
                        }
                        let save = ui.button("Save");
                        if widgets::with_button_emphasis(ui, save, ButtonEmphasis::Primary)
                            .clicked()
                        {
                            app.multi_manager_save();
                        }
                        if ui.button("Reload").clicked() {
                            self.confirm.reload = true;
                        }
                        if ui.button("Save HWND Snapshot").clicked() {
                            app.multi_manager_save_bindings();
                        }
                        if ui.button("Restore HWND Snapshot").clicked() {
                            app.multi_manager_restore_bindings();
                        }
                        let reconnect_active = app
                            .multi_manager
                            .reconnect_in_progress
                            .load(Ordering::Acquire);
                        if ui
                            .add_enabled(
                                reconnect_button_enabled(reconnect_active),
                                egui::Button::new(reconnect_button_text(reconnect_active)),
                            )
                            .clicked()
                        {
                            app.multi_manager_start_manual_reconnect();
                        }
                        if ui.button("Refresh Titles").clicked() {
                            app.multi_manager_refresh_titles();
                        }
                        if ui.button("Refresh Desktops").clicked() {
                            self.refresh_virtual_desktops();
                        }
                        if ui.button("Send All Home").clicked() {
                            send_all(app, true);
                        }
                        if ui.button("Recapture All").clicked() {
                            app.multi_manager_start_recapture_all();
                        }
                        if ui
                            .button(expand_all_button_label(self.all_expanded))
                            .clicked()
                        {
                            toggle_expand_all(&mut self.all_expanded, &mut self.expand_all_signal);
                        }
                        if app.multi_manager.dirty {
                            widgets::semantic_status(ui, StatusTone::Warning, "● dirty", None);
                        } else {
                            widgets::semantic_status(ui, StatusTone::Success, "saved", None);
                        }
                        let last = app
                            .multi_manager
                            .last_hotkey_info
                            .lock()
                            .ok()
                            .and_then(|g| g.clone())
                            .map(|(s, _)| s)
                            .unwrap_or_else(|| "none".into());
                        widgets::semantic_status(ui, StatusTone::Info, "last hotkey", Some(&last));
                    });
                    if self.confirm.reload {
                        ui.horizontal_wrapped(|ui| {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Warning,
                                "Reload and discard unsaved changes?",
                                None,
                            );
                            let confirm = ui.button("Confirm");
                            if widgets::with_button_emphasis(
                                ui,
                                confirm,
                                ButtonEmphasis::Destructive,
                            )
                            .clicked()
                            {
                                app.multi_manager_reload();
                                self.confirm.reload = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm.reload = false;
                            }
                        });
                    }
                    widgets::compact_section_heading(
                        ui,
                        "Workspaces",
                        Some("Capture windows, review bindings, and manage positions."),
                    );
                    let ids = app
                        .multi_manager
                        .workspaces
                        .lock()
                        .map(|w| w.iter().map(|x| x.id.clone()).collect::<Vec<_>>())
                        .unwrap_or_default();
                    for id in ids {
                        self.workspace_card(ui, app, &id);
                    }
                    self.expand_all_signal = None;
                });
            });
        self.open = open;
    }

    fn capture_banner(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        if let Some(action) = &app.multi_manager.pending_capture {
            widgets::semantic_status(ui, StatusTone::Info, capture_banner_text(action), None);
            match action {
                PendingCaptureAction::CaptureOneWindow { workspace_id } => {
                    ui.label(format!("Capturing one window for workspace {workspace_id}"));
                }
                PendingCaptureAction::CaptureMultipleWindows { workspace_id } => {
                    ui.label(format!(
                        "Capturing multiple windows for workspace {workspace_id}; press Escape when done"
                    ));
                }
                PendingCaptureAction::RecaptureWindow {
                    workspace_id,
                    window_index,
                } => {
                    let workspace_label = workspace_display_label(app, workspace_id);
                    ui.label(format!(
                        "Recapturing workspace {workspace_label}, window {}",
                        window_index + 1
                    ));
                }
            }
        }
    }

    fn workspace_card(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str) {
        let workspace = get_ws(app, id).unwrap_or_else(|| MmWorkspace {
            id: id.into(),
            ..Default::default()
        });
        let label = workspace_header_label(&workspace);
        let tone = workspace_header_tone(&workspace);
        let mut state = egui::collapsing_header::CollapsingState::load_with_default_open(
            ui.ctx(),
            egui::Id::new(("multi_manager_workspace_header", id)),
            self.all_expanded,
        );
        if let Some(expand) = self.expand_all_signal {
            state.set_open(expand);
        }
        state
            .show_header(ui, |ui| {
                widgets::semantic_status(ui, tone, &label, None);
            })
            .body(|ui| {
                ui.push_id(("multi_manager_workspace", id), |ui| {
                    widgets::compact_section_heading(ui, "Workspace", None);
                    ui.horizontal_wrapped(|ui| {
                        if self.rename.workspace_id == id {
                            widgets::labeled_form_row(ui, "Name", |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut self.rename.value)
                                        .id_salt(("mm_workspace_rename", id)),
                                )
                            });
                            if ui.button("Apply").clicked() {
                                let val = self.rename.value.clone();
                                app.multi_manager.with_workspace_mut(id, |w| w.name = val);
                                self.rename = Default::default();
                            }
                        } else if ui.button("Rename").clicked() {
                            self.rename.workspace_id = id.into();
                            self.rename.value = app
                                .multi_manager
                                .workspaces
                                .lock()
                                .ok()
                                .and_then(|w| w.iter().find(|x| x.id == id).map(|x| x.name.clone()))
                                .unwrap_or_default();
                        }
                        let mut disabled = get_ws(app, id).map(|w| w.disabled).unwrap_or(false);
                        if ui.checkbox(&mut disabled, "Disabled").changed() {
                            app.multi_manager
                                .with_workspace_mut(id, |w| w.disabled = disabled);
                        }
                        let mut rotate = get_ws(app, id).map(|w| w.rotate).unwrap_or(false);
                        if ui.checkbox(&mut rotate, "Rotate").changed() {
                            app.multi_manager
                                .with_workspace_mut(id, |w| w.rotate = rotate);
                        }
                        if ui.button("↑").clicked() {
                            reorder_workspace(app, id, -1);
                        }
                        if ui.button("↓").clicked() {
                            reorder_workspace(app, id, 1);
                        }
                        if ui.button("Delete").clicked() {
                            self.confirm.workspace_id = Some(id.into());
                        }
                    });
                    self.hotkey_ui(ui, app, id);
                    self.virtual_desktop_ui(ui, app, id, &workspace);
                    if self.confirm.workspace_id.as_deref() == Some(id) {
                        ui.horizontal_wrapped(|ui| {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Warning,
                                "Confirm delete workspace?",
                                None,
                            );
                            let delete = ui.button("Delete");
                            if widgets::with_button_emphasis(
                                ui,
                                delete,
                                ButtonEmphasis::Destructive,
                            )
                            .clicked()
                            {
                                delete_workspace(app, id);
                                self.confirm.workspace_id = None;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm.workspace_id = None;
                            }
                        });
                    }
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Capture Active Window").clicked() {
                            app.multi_manager_start_capture_one(id, ui.ctx());
                        }
                        if ui.button("Capture Multiple Windows").clicked() {
                            app.multi_manager_start_capture_multiple(id, ui.ctx());
                        }
                        if ui.button("Send Home").clicked() {
                            app.multi_manager_send_home(id);
                        }
                        if ui.button("Move Target").clicked() {
                            app.multi_manager_send_target(id);
                        }
                    });
                    let count = get_ws(app, id).map(|w| w.windows.len()).unwrap_or(0);
                    for i in 0..count {
                        self.window_card(ui, app, id, i);
                    }
                });
            });
    }

    fn refresh_virtual_desktops(&mut self) {
        match crate::virtual_desktop::VirtualDesktopService.snapshot() {
            Ok(snapshot) => {
                self.virtual_desktops = VirtualDesktopSnapshotState::Ready(snapshot.desktops);
            }
            Err(error) => {
                self.virtual_desktops = VirtualDesktopSnapshotState::Failed(error.to_string())
            }
        }
    }

    fn virtual_desktop_ui(
        &mut self,
        ui: &mut egui::Ui,
        app: &mut LauncherApp,
        id: &str,
        workspace: &MmWorkspace,
    ) {
        ui.horizontal_wrapped(|ui| {
            widgets::compact_section_heading(ui, "Virtual desktop", None);
            match &workspace.virtual_desktop {
                Some(binding) => {
                    let cached = binding
                        .cached_name
                        .clone()
                        .unwrap_or_else(|| binding.id.to_string());
                    match &self.virtual_desktops {
                        VirtualDesktopSnapshotState::Unverified => {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Info,
                                &format!("{cached} (not refreshed)"),
                                None,
                            );
                        }
                        VirtualDesktopSnapshotState::Failed(_) => {
                            widgets::semantic_status(
                                ui,
                                StatusTone::Warning,
                                &format!("{cached} (refresh failed; status unknown)"),
                                None,
                            );
                        }
                        VirtualDesktopSnapshotState::Ready(desktops) => {
                            if let Some(desktop) =
                                desktops.iter().find(|desktop| desktop.id == binding.id)
                            {
                                widgets::semantic_status(
                                    ui,
                                    StatusTone::Info,
                                    &desktop.display_name(),
                                    None,
                                );
                            } else {
                                widgets::semantic_status(
                                    ui,
                                    StatusTone::Warning,
                                    &format!("{cached} (stale: {})", binding.id),
                                    None,
                                );
                            }
                        }
                    }
                    if ui.button("Clear").clicked() {
                        app.multi_manager.unbind_virtual_desktop(id);
                    }
                }
                None => {
                    widgets::semantic_status(ui, StatusTone::Info, "unbound", None);
                }
            }
            egui::ComboBox::from_id_salt(("mm_virtual_desktop", id))
                .selected_text("Bind / rebind…")
                .show_ui(ui, |ui| {
                    let VirtualDesktopSnapshotState::Ready(desktops) = &self.virtual_desktops
                    else {
                        ui.label("Refresh desktops to load choices");
                        return;
                    };
                    for desktop in desktops {
                        if ui
                            .selectable_label(
                                false,
                                format!("{} — Desktop {}", desktop.display_name(), desktop.index),
                            )
                            .clicked()
                        {
                            app.multi_manager.bind_virtual_desktop(
                                id,
                                crate::virtual_desktop::VirtualDesktopBinding {
                                    id: desktop.id.clone(),
                                    cached_name: desktop.name.clone(),
                                },
                            );
                        }
                    }
                });
        });
        if let VirtualDesktopSnapshotState::Failed(error) = &self.virtual_desktops {
            widgets::semantic_status(ui, StatusTone::Error, error, None);
        }
    }

    fn hotkey_ui(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str) {
        if self.hotkey_editor.workspace_id != id {
            if widgets::labeled_form_row(ui, "Hotkey", |ui| ui.button("Edit hotkey")).clicked() {
                begin_hotkey_edit(
                    &mut self.hotkey_editor,
                    id,
                    get_ws(app, id)
                        .and_then(|workspace| workspace.hotkey)
                        .as_ref(),
                );
                self.hotkey_editor_needs_focus = true;
            }
            return;
        }

        let escape_pressed = ui.input(|input| input.key_pressed(egui::Key::Escape));
        if escape_pressed {
            cancel_hotkey_edit(&mut self.hotkey_editor);
            self.hotkey_editor_needs_focus = false;
            return;
        }

        widgets::labeled_form_row(ui, "Hotkey", |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.checkbox(&mut self.hotkey_editor.ctrl, "Ctrl");
                ui.checkbox(&mut self.hotkey_editor.shift, "Shift");
                ui.checkbox(&mut self.hotkey_editor.alt, "Alt");
                ui.checkbox(&mut self.hotkey_editor.win, "Win");
                let response = ui.add(
                    egui::TextEdit::singleline(&mut self.hotkey_editor.key)
                        .id_salt(("mm_workspace_hotkey_key", id)),
                );
                if self.hotkey_editor_needs_focus {
                    response.request_focus();
                    self.hotkey_editor_needs_focus = false;
                }

                let candidate = hotkey_from_editor(&self.hotkey_editor);
                match candidate.validate() {
                    MmHotkeyValidation::Valid => {
                        widgets::semantic_status(
                            ui,
                            StatusTone::Success,
                            MmHotkeyValidation::Valid.label(),
                            None,
                        );
                    }
                    validation if hotkey_editor_is_empty(&self.hotkey_editor) => {
                        widgets::semantic_status(ui, StatusTone::Info, "no hotkey set", None);
                        let _ = validation;
                    }
                    validation => {
                        widgets::semantic_status(ui, StatusTone::Error, validation.label(), None);
                    }
                }

                if ui
                    .add_enabled(
                        hotkey_set_enabled(&self.hotkey_editor),
                        egui::Button::new("Set"),
                    )
                    .clicked()
                {
                    app.multi_manager
                        .with_workspace_mut(id, |w| w.hotkey = Some(candidate));
                    self.hotkey_editor = Default::default();
                }
                if ui.button("Reset").clicked() {
                    app.multi_manager
                        .with_workspace_mut(id, reset_workspace_hotkey);
                    self.hotkey_editor = Default::default();
                }
                let cancel = ui.button("Cancel");
                if cancel.clicked() {
                    cancel_hotkey_edit(&mut self.hotkey_editor);
                }
                cancel
            })
            .inner
        });
    }

    fn window_card(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str, index: usize) {
        ui.push_id(("multi_manager_window", id, index), |ui| {
            ui.group(|ui| {
                let win = get_ws(app, id)
                    .and_then(|w| w.windows.get(index).cloned())
                    .unwrap_or_default();
                widgets::compact_section_heading(ui, &format!("Window {}", index + 1), None);
                let mut alias = win.alias.clone();
                if widgets::labeled_form_row(ui, "Alias", |ui| {
                    ui.add(egui::TextEdit::singleline(&mut alias).id_salt((
                        "mm_window_alias",
                        id,
                        index,
                    )))
                })
                .changed()
                {
                    app.multi_manager.with_workspace_mut(id, |w| {
                        if let Some(x) = w.windows.get_mut(index) {
                            x.alias = alias
                        }
                    });
                }
                widgets::labeled_form_row(ui, "HWND", |ui| ui.label(win.hwnd.to_string()));
                window_status_ui(ui, &win);
                if is_duplicate_hwnd(app, win.hwnd, id, index) {
                    widgets::semantic_status(ui, StatusTone::Warning, "⚠ duplicate HWND", None);
                }
                window_metadata_ui(ui, &win);
                rect_ui(ui, ("home_rect", id, index), "Home", win.home_rect, |r| {
                    app.multi_manager.with_workspace_mut(id, |w| {
                        if let Some(x) = w.windows.get_mut(index) {
                            x.home_rect = Some(r);
                        }
                    })
                });
                rect_ui(
                    ui,
                    ("target_rect", id, index),
                    "Target",
                    win.target_rect,
                    |r| {
                        app.multi_manager.with_workspace_mut(id, |w| {
                            if let Some(x) = w.windows.get_mut(index) {
                                x.target_rect = Some(r);
                            }
                        })
                    },
                );
                ui.horizontal_wrapped(|ui| {
                    if ui.button("Capture Home").clicked() {
                        capture_rect(app, id, index, true);
                    }
                    if ui.button("Capture Target").clicked() {
                        capture_rect(app, id, index, false);
                    }
                    if ui.button("Move Home").clicked() {
                        move_window(app, &win, true);
                    }
                    if ui.button("Move Target").clicked() {
                        move_window(app, &win, false);
                    }
                    if ui.button("Recapture").clicked() {
                        app.multi_manager_start_recapture_window(id, index, ui.ctx());
                    }
                    if ui.button("Swap Home/Target").clicked() {
                        app.multi_manager.with_workspace_mut(id, |w| {
                            if let Some(x) = w.windows.get_mut(index) {
                                std::mem::swap(&mut x.home_rect, &mut x.target_rect);
                            }
                        });
                    }
                    if ui.button("↑").clicked() {
                        reorder_window(app, id, index, -1);
                    }
                    if ui.button("↓").clicked() {
                        reorder_window(app, id, index, 1);
                    }
                    if ui.button("Delete").clicked() {
                        self.confirm.window = Some((id.into(), index));
                    }
                });
                if self
                    .confirm
                    .window
                    .as_ref()
                    .is_some_and(|(wid, i)| wid == id && *i == index)
                {
                    ui.horizontal_wrapped(|ui| {
                        widgets::semantic_status(
                            ui,
                            StatusTone::Warning,
                            "Confirm delete window?",
                            None,
                        );
                        let delete = ui.button("Delete");
                        if widgets::with_button_emphasis(ui, delete, ButtonEmphasis::Destructive)
                            .clicked()
                        {
                            app.multi_manager.with_workspace_mut(id, |w| {
                                if index < w.windows.len() {
                                    w.windows.remove(index);
                                }
                            });
                            self.confirm.window = None;
                        }
                        if ui.button("Cancel").clicked() {
                            self.confirm.window = None;
                        }
                    });
                }
            });
        });
    }
}

fn refresh_live_titles_if_due(app: &mut LauncherApp, ctx: &egui::Context, force: bool) -> bool {
    let now = Instant::now();
    let Ok(mut workspaces) = app.multi_manager.workspaces.lock() else {
        return false;
    };
    let changed = refresh_live_titles_throttled_with(
        &mut workspaces,
        &mut app.multi_manager.last_live_title_refresh,
        now,
        force,
        crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL,
        win::is_valid_window,
        win::window_title,
    );
    if changed {
        ctx.request_repaint();
    }
    changed
}

pub(crate) fn refresh_live_titles_throttled_with(
    workspaces: &mut [MmWorkspace],
    last_refresh: &mut Option<Instant>,
    now: Instant,
    force: bool,
    interval: std::time::Duration,
    is_valid: impl FnMut(usize) -> bool,
    window_title: impl FnMut(usize) -> Option<String>,
) -> bool {
    if !force
        && last_refresh.is_some_and(|last_refresh| now.duration_since(last_refresh) < interval)
    {
        return false;
    }
    *last_refresh = Some(now);
    bindings::refresh_live_titles_with(workspaces, is_valid, window_title)
}

fn reconnect_button_text(active: bool) -> &'static str {
    if active {
        "Reconnecting…"
    } else {
        "Reconnect Windows"
    }
}

fn reconnect_button_enabled(active: bool) -> bool {
    !active
}

fn capture_banner_text(action: &PendingCaptureAction) -> &'static str {
    match action {
        PendingCaptureAction::CaptureOneWindow { .. } => {
            "Capture mode active: focus the target window, then press Enter. Escape cancels."
        }
        PendingCaptureAction::CaptureMultipleWindows { .. } => {
            "Multi-capture active: focus a target window and press Enter. Repeat for each window. Escape finishes/cancels."
        }
        PendingCaptureAction::RecaptureWindow { .. } => {
            "Recapture active: focus the replacement window and press Enter. Press S to skip this item. Escape cancels the queue."
        }
    }
}

fn expand_all_button_label(all_expanded: bool) -> &'static str {
    if all_expanded {
        "Collapse All"
    } else {
        "Expand All"
    }
}

fn toggle_expand_all(all_expanded: &mut bool, expand_all_signal: &mut Option<bool>) {
    *all_expanded = !*all_expanded;
    *expand_all_signal = Some(*all_expanded);
}

fn workspace_display_label(app: &LauncherApp, id: &str) -> String {
    get_ws(app, id)
        .and_then(|workspace| {
            if workspace.name.is_empty() {
                None
            } else {
                Some(format!("{id} ({})", workspace.name))
            }
        })
        .unwrap_or_else(|| id.into())
}

fn workspace_header_label(workspace: &MmWorkspace) -> String {
    let mut label = if workspace.name.trim().is_empty() {
        workspace.id.clone()
    } else {
        workspace.name.clone()
    };

    if let Some(sequence) = workspace
        .hotkey
        .as_ref()
        .and_then(|hotkey| hotkey.sequence())
    {
        label.push_str(" - ");
        label.push_str(&sequence);
    }

    label
}

fn workspace_header_tone(workspace: &MmWorkspace) -> StatusTone {
    if workspace.disabled {
        StatusTone::Warning
    } else if workspace.valid
        && workspace
            .hotkey
            .as_ref()
            .is_some_and(|hotkey| hotkey.is_valid())
    {
        StatusTone::Success
    } else {
        StatusTone::Error
    }
}

fn binding_status_presentation(status: MmBindingStatus) -> (&'static str, StatusTone) {
    match status {
        MmBindingStatus::Bound => ("Bound", StatusTone::Success),
        MmBindingStatus::Reconnected => ("Reconnected", StatusTone::Success),
        MmBindingStatus::Missing => ("Missing", StatusTone::Error),
        MmBindingStatus::Closed => ("Closed", StatusTone::Warning),
        MmBindingStatus::Ambiguous => ("Ambiguous", StatusTone::Warning),
        MmBindingStatus::MetadataMismatch => ("Metadata mismatch", StatusTone::Error),
    }
}

fn window_status_presentation(win: &MmWindow) -> (&'static str, StatusTone) {
    binding_status_presentation(win.binding_status)
}

fn window_status_ui(ui: &mut egui::Ui, win: &MmWindow) {
    let (label, tone) = window_status_presentation(win);
    widgets::semantic_status(ui, tone, label, None);
}

fn window_metadata_ui(ui: &mut egui::Ui, win: &MmWindow) {
    widgets::compact_section_heading(ui, "Window metadata", None);
    widgets::labeled_form_row(ui, "Title", |ui| ui.label(win.current_display_title()));
    if !win.live_title.trim().is_empty() && win.live_title.trim() != win.captured_title.trim() {
        widgets::labeled_form_row(ui, "Captured title", |ui| ui.label(win.fallback_title()));
    }
    widgets::labeled_form_row(ui, "Class", |ui| ui.label(&win.class_name));
    widgets::labeled_form_row(ui, "Executable", |ui| ui.label(&win.executable));
    if !win.process_path.trim().is_empty() {
        ui.collapsing("Process", |ui| {
            widgets::labeled_form_row(ui, "Process", |ui| ui.label(&win.process_path));
        });
    }
}

#[derive(Debug, Clone, Copy)]
struct SettingsFormLayout {
    #[cfg(test)]
    scroll_id: egui::Id,
    #[cfg(test)]
    scroll_offset: egui::Vec2,
    #[cfg(test)]
    content_size: egui::Vec2,
    #[cfg(test)]
    viewport: egui::Rect,
    #[cfg(test)]
    save_rect: egui::Rect,
    changed: bool,
    save_clicked: bool,
}

fn show_settings_form(
    ui: &mut egui::Ui,
    settings: &mut crate::settings::MultiManagerSettings,
) -> SettingsFormLayout {
    let max_height = ui.available_height().max(0.0);
    let output = egui::ScrollArea::vertical()
        .id_salt("multi_manager_settings_scroll")
        .max_height(max_height)
        .min_scrolled_height(0.0)
        .auto_shrink([true, true])
        .show(ui, |ui| settings_form_contents(ui, settings));
    let (changed, save) = output.inner;
    SettingsFormLayout {
        #[cfg(test)]
        scroll_id: output.id,
        #[cfg(test)]
        scroll_offset: output.state.offset,
        #[cfg(test)]
        content_size: output.content_size,
        #[cfg(test)]
        viewport: output.inner_rect,
        #[cfg(test)]
        save_rect: save.rect,
        changed,
        save_clicked: save.clicked(),
    }
}

fn settings_form_contents(
    ui: &mut egui::Ui,
    settings: &mut crate::settings::MultiManagerSettings,
) -> (bool, egui::Response) {
    widgets::compact_section_heading(ui, "Settings", Some("Changes apply immediately."));
    let mut changed = false;
    changed |= ui.checkbox(&mut settings.enabled, "Enabled").changed();
    changed |= widgets::labeled_form_row(ui, "Workspace file path", |ui| {
        ui.text_edit_singleline(&mut settings.workspaces_path)
    })
    .changed();
    changed |= widgets::labeled_form_row(ui, "Bindings file path", |ui| {
        ui.text_edit_singleline(&mut settings.bindings_path)
    })
    .changed();
    changed |= ui.checkbox(&mut settings.auto_save, "Auto-save").changed();
    changed |= ui
        .checkbox(&mut settings.save_on_exit, "Save on launcher exit")
        .changed();
    changed |= ui
        .checkbox(
            &mut settings.show_force_recapture_prompt,
            "Show force recapture prompt",
        )
        .changed();
    changed |= ui
        .checkbox(&mut settings.developer_debugging, "Developer debugging")
        .changed();
    changed |= widgets::labeled_form_row(ui, "Hotkey poll interval", |ui| {
        ui.add(
            egui::DragValue::new(&mut settings.hotkey_poll_ms)
                .range(10..=5000)
                .suffix(" ms"),
        )
    })
    .changed();
    changed |= ui
        .checkbox(
            &mut settings.auto_reconnect_on_load,
            "Auto-reconnect windows on load",
        )
        .changed();
    changed |= ui
        .checkbox(
            &mut settings.ignore_launcher_window_on_capture,
            "Ignore launcher window on capture",
        )
        .changed();
    changed |= ui
        .checkbox(
            &mut settings.hide_launcher_before_toggle,
            "Hide launcher before toggle",
        )
        .changed();
    ui.separator();
    let save = ui.button("Save MultiManager Settings");
    (
        changed,
        widgets::with_button_emphasis(ui, save, ButtonEmphasis::Primary),
    )
}

impl MultiManagerSettingsDialog {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        egui::Window::new("MultiManager Settings")
            .open(&mut open)
            .show(ctx, |ui| {
                let mut s = app.multi_manager_settings.clone();
                let form = show_settings_form(ui, &mut s);
                if form.changed {
                    app.multi_manager_settings = s.clone();
                    app.multi_manager.auto_save = s.auto_save;
                    app.multi_manager.auto_reconnect_on_load = s.auto_reconnect_on_load;
                    let control = &app.multi_manager.runtime.control;
                    control.enabled.store(s.enabled, Ordering::Relaxed);
                }
                if form.save_clicked {
                    match crate::multi_manager::settings::save_multi_manager_settings(
                        &app.settings_path,
                        app.multi_manager_settings.clone(),
                    ) {
                        Ok(()) => app.add_success_toast("Saved MultiManager settings"),
                        Err(err) => app
                            .report_error_message("multi_manager.settings.save", format!("{err}")),
                    }
                }
            });
        self.open = open;
    }
}

fn get_ws(app: &LauncherApp, id: &str) -> Option<MmWorkspace> {
    app.multi_manager
        .workspaces
        .lock()
        .ok()?
        .iter()
        .find(|w| w.id == id)
        .cloned()
}
fn add_workspace(app: &mut LauncherApp) {
    if let Ok(mut w) = app.multi_manager.workspaces.lock() {
        w.push(MmWorkspace {
            id: new_workspace_id(),
            name: "New Workspace".into(),
            ..Default::default()
        });
    }
    app.multi_manager.mark_dirty();
}
fn delete_workspace(app: &mut LauncherApp, id: &str) {
    if let Ok(mut w) = app.multi_manager.workspaces.lock() {
        w.retain(|x| x.id != id);
    }
    app.multi_manager.mark_dirty();
}
fn reorder_workspace(app: &mut LauncherApp, id: &str, delta: isize) {
    if let Ok(mut w) = app.multi_manager.workspaces.lock()
        && let Some(i) = w.iter().position(|x| x.id == id)
    {
        let j = (i as isize + delta).clamp(0, w.len().saturating_sub(1) as isize) as usize;
        w.swap(i, j);
    }
    app.multi_manager.mark_dirty();
}
fn reorder_window(app: &mut LauncherApp, id: &str, index: usize, delta: isize) {
    app.multi_manager.with_workspace_mut(id, |w| {
        let j =
            (index as isize + delta).clamp(0, w.windows.len().saturating_sub(1) as isize) as usize;
        if index < w.windows.len() {
            w.windows.swap(index, j);
        }
    });
}
fn send_all(app: &mut LauncherApp, home: bool) {
    let ids = app
        .multi_manager
        .workspaces
        .lock()
        .map(|w| w.iter().map(|x| x.id.clone()).collect::<Vec<_>>())
        .unwrap_or_default();
    for id in ids {
        if home {
            app.multi_manager_send_home(&id);
        } else {
            app.multi_manager_send_target(&id);
        }
    }
}
fn rect_ui(
    ui: &mut egui::Ui,
    id_source: impl std::hash::Hash + std::fmt::Debug,
    label: &str,
    rect: Option<MmRect>,
    mut set: impl FnMut(MmRect) -> Option<()>,
) {
    let mut r = rect.unwrap_or(MmRect {
        x: 0,
        y: 0,
        w: 800,
        h: 600,
    });
    ui.push_id(id_source, |ui| {
        let mut changed = false;
        widgets::labeled_form_row(ui, label, |ui| {
            changed |= ui
                .add(egui::DragValue::new(&mut r.x).prefix("x "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut r.y).prefix("y "))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut r.w).range(1..=10000).prefix("w "))
                .changed();
            let height = ui.add(egui::DragValue::new(&mut r.h).range(1..=10000).prefix("h "));
            changed |= height.changed();
            height
        });
        if changed {
            let _ = set(r);
        }
    });
}
fn capture_rect(app: &mut LauncherApp, id: &str, index: usize, home: bool) {
    let Some(hwnd) = get_ws(app, id)
        .and_then(|workspace| workspace.windows.get(index).map(|window| window.hwnd))
    else {
        app.report_error_message(
            "multi_manager.capture_rect",
            format!("Missing workspace or window row for capture: {id} #{index}"),
        );
        return;
    };

    let Some(rect) = win::window_rect(hwnd) else {
        app.report_error_message(
            "multi_manager.capture_rect",
            format!("Tracked window is missing or invalid: HWND {hwnd}"),
        );
        return;
    };

    let updated = match app.multi_manager.workspaces.lock() {
        Ok(mut workspaces) => set_window_rect_in_workspaces(&mut workspaces, id, index, home, rect),
        Err(_) => RectCaptureMutationResult::LockFailed,
    };
    match updated {
        RectCaptureMutationResult::Applied => app.multi_manager.mark_dirty(),
        RectCaptureMutationResult::MissingWorkspace => app.report_error_message(
            "multi_manager.capture_rect",
            format!("Missing workspace for capture: {id}"),
        ),
        RectCaptureMutationResult::MissingWindow(err) => {
            app.report_error_message("multi_manager.capture_rect", err)
        }
        RectCaptureMutationResult::LockFailed => app.report_error_message(
            "multi_manager.capture_rect",
            "Failed to lock MultiManager workspaces for capture",
        ),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum RectCaptureMutationResult {
    Applied,
    MissingWorkspace,
    MissingWindow(String),
    LockFailed,
}

fn set_window_rect_in_workspaces(
    workspaces: &mut [MmWorkspace],
    id: &str,
    index: usize,
    home: bool,
    rect: MmRect,
) -> RectCaptureMutationResult {
    let Some(workspace) = workspaces.iter_mut().find(|workspace| workspace.id == id) else {
        return RectCaptureMutationResult::MissingWorkspace;
    };

    match set_window_rect(workspace, index, home, rect) {
        Ok(()) => RectCaptureMutationResult::Applied,
        Err(err) => RectCaptureMutationResult::MissingWindow(err),
    }
}

fn set_window_rect(
    workspace: &mut MmWorkspace,
    index: usize,
    home: bool,
    rect: MmRect,
) -> Result<(), String> {
    let Some(window) = workspace.windows.get_mut(index) else {
        return Err(format!(
            "Missing window row for capture: {} #{}",
            workspace.id, index
        ));
    };
    if home {
        window.home_rect = Some(rect);
    } else {
        window.target_rect = Some(rect);
    }
    Ok(())
}
fn move_window(app: &mut LauncherApp, w: &MmWindow, home: bool) {
    if let Some(r) = if home { w.home_rect } else { w.target_rect }
        && let Err(err) = win::move_window_to_rect(w.hwnd, r)
    {
        app.report_error_message("multi_manager.move_window", format!("{err}"));
    }
}

fn begin_hotkey_edit(
    editor: &mut HotkeyEditorState,
    workspace_id: &str,
    hotkey: Option<&MmHotkey>,
) {
    *editor = hotkey
        .map(|hotkey| HotkeyEditorState {
            workspace_id: workspace_id.into(),
            key: hotkey.key.clone(),
            ctrl: hotkey.ctrl,
            shift: hotkey.shift,
            alt: hotkey.alt,
            win: hotkey.win,
        })
        .unwrap_or_else(|| HotkeyEditorState {
            workspace_id: workspace_id.into(),
            ..Default::default()
        });
}

fn cancel_hotkey_edit(editor: &mut HotkeyEditorState) {
    *editor = Default::default();
}

fn reset_workspace_hotkey(workspace: &mut MmWorkspace) {
    workspace.hotkey = None;
}

fn hotkey_from_editor(editor: &HotkeyEditorState) -> MmHotkey {
    MmHotkey {
        key: editor.key.clone(),
        ctrl: editor.ctrl,
        shift: editor.shift,
        alt: editor.alt,
        win: editor.win,
    }
}

fn hotkey_set_enabled(editor: &HotkeyEditorState) -> bool {
    hotkey_from_editor(editor).validate() == MmHotkeyValidation::Valid
}

fn hotkey_editor_is_empty(editor: &HotkeyEditorState) -> bool {
    editor.key.trim().is_empty() && !editor.ctrl && !editor.shift && !editor.alt && !editor.win
}

fn is_duplicate_hwnd(
    app: &LauncherApp,
    hwnd: usize,
    workspace_id: &str,
    window_index: usize,
) -> bool {
    if hwnd == 0 {
        return false;
    }
    app.multi_manager
        .workspaces
        .lock()
        .map(|workspaces| {
            bindings::duplicate_hwnds(&workspaces)
                .into_iter()
                .any(|duplicate| {
                    duplicate.hwnd == hwnd
                        && duplicate
                            .locations
                            .into_iter()
                            .any(|(ws_index, win_index)| {
                                workspaces
                                    .get(ws_index)
                                    .is_some_and(|workspace| workspace.id == workspace_id)
                                    && win_index == window_index
                            })
                })
        })
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dialog_state_is_closed() {
        let dialog = MultiManagerDialog::default();

        assert!(!dialog.open);
        assert!(!dialog.all_expanded);
        assert_eq!(dialog.expand_all_signal, None);
        assert!(!MultiManagerSettingsDialog::default().open);
    }

    #[test]
    fn capture_banner_text_explains_single_capture_controls() {
        let action = PendingCaptureAction::CaptureOneWindow {
            workspace_id: "workspace".into(),
        };

        assert_eq!(
            capture_banner_text(&action),
            "Capture mode active: focus the target window, then press Enter. Escape cancels."
        );
    }

    #[test]
    fn capture_banner_text_explains_multi_capture_controls() {
        let action = PendingCaptureAction::CaptureMultipleWindows {
            workspace_id: "workspace".into(),
        };

        assert_eq!(
            capture_banner_text(&action),
            "Multi-capture active: focus a target window and press Enter. Repeat for each window. Escape finishes/cancels."
        );
    }

    #[test]
    fn capture_banner_text_explains_recapture_controls() {
        let action = PendingCaptureAction::RecaptureWindow {
            workspace_id: "workspace".into(),
            window_index: 0,
        };

        assert_eq!(
            capture_banner_text(&action),
            "Recapture active: focus the replacement window and press Enter. Press S to skip this item. Escape cancels the queue."
        );
    }

    #[test]
    fn reconnect_button_text_reflects_idle_state() {
        assert_eq!(reconnect_button_text(false), "Reconnect Windows");
    }

    #[test]
    fn reconnect_button_text_reflects_active_state() {
        assert_eq!(reconnect_button_text(true), "Reconnecting…");
    }

    #[test]
    fn reconnect_button_is_disabled_while_active() {
        assert!(reconnect_button_enabled(false));
        assert!(!reconnect_button_enabled(true));
    }

    #[test]
    fn expand_all_button_label_reflects_current_global_state() {
        assert_eq!(expand_all_button_label(false), "Expand All");
        assert_eq!(expand_all_button_label(true), "Collapse All");
    }

    #[test]
    fn toggle_expand_all_sets_one_shot_expand_signal() {
        let mut all_expanded = false;
        let mut signal = None;

        toggle_expand_all(&mut all_expanded, &mut signal);

        assert!(all_expanded);
        assert_eq!(signal, Some(true));
    }

    #[test]
    fn toggle_expand_all_sets_one_shot_collapse_signal() {
        let mut all_expanded = true;
        let mut signal = None;

        toggle_expand_all(&mut all_expanded, &mut signal);

        assert!(!all_expanded);
        assert_eq!(signal, Some(false));
    }

    #[test]
    fn every_binding_status_maps_to_expected_label_and_semantic_tone() {
        let cases = [
            (MmBindingStatus::Bound, ("Bound", StatusTone::Success)),
            (
                MmBindingStatus::Reconnected,
                ("Reconnected", StatusTone::Success),
            ),
            (MmBindingStatus::Missing, ("Missing", StatusTone::Error)),
            (MmBindingStatus::Closed, ("Closed", StatusTone::Warning)),
            (
                MmBindingStatus::Ambiguous,
                ("Ambiguous", StatusTone::Warning),
            ),
            (
                MmBindingStatus::MetadataMismatch,
                ("Metadata mismatch", StatusTone::Error),
            ),
        ];

        for (status, expected) in cases {
            assert_eq!(binding_status_presentation(status), expected);
        }
    }

    #[test]
    fn ambiguous_and_metadata_mismatch_keep_window_status_warning_and_error_tones() {
        let mut ambiguous = MmWindow::default();
        ambiguous.mark_ambiguous();
        let mut metadata_mismatch = MmWindow::default();
        metadata_mismatch.mark_metadata_mismatch();

        assert_eq!(
            window_status_presentation(&ambiguous),
            ("Ambiguous", StatusTone::Warning)
        );
        assert_eq!(
            window_status_presentation(&metadata_mismatch),
            ("Metadata mismatch", StatusTone::Error)
        );
    }

    #[test]
    fn status_rendering_uses_recorded_status_without_win32_validation_callbacks() {
        let window = MmWindow {
            hwnd: 0,
            valid: false,
            binding_status: MmBindingStatus::Bound,
            ..Default::default()
        };

        assert_eq!(
            window_status_presentation(&window),
            ("Bound", StatusTone::Success)
        );
    }

    #[test]
    fn settings_form_auto_shrinks_and_scrolls_to_save_without_mutating_settings() {
        let ctx = egui::Context::default();
        let mut settings = crate::settings::MultiManagerSettings::default();
        let original_settings = settings.clone();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(960.0, 720.0));
        let input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..Default::default()
        };
        let render_frame =
            |window_rect: egui::Rect, settings: &mut crate::settings::MultiManagerSettings| {
                let mut sizing_pass = true;
                let mut visible = false;
                let mut rendered_window_rect = egui::Rect::NOTHING;
                let mut layout = None;
                let output = ctx.run_ui(input(), |ctx| {
                    let window_output = egui::Window::new("MultiManager Settings layout contract")
                        .fixed_rect(window_rect)
                        .resizable(false)
                        .show(ctx, |ui| {
                            sizing_pass = ui.is_sizing_pass();
                            visible = ui.is_visible();
                            layout = Some(show_settings_form(ui, settings));
                        });
                    if let Some(window_output) = window_output {
                        rendered_window_rect = window_output.response.rect;
                    }
                });
                let frame = (
                    sizing_pass,
                    visible,
                    rendered_window_rect,
                    layout.expect("settings form should render in its Window"),
                );
                output.drop_without_applying_deltas();
                frame
            };

        let normal_window_rect =
            egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(520.0, 600.0));
        let _ = render_frame(normal_window_rect, &mut settings);
        let (normal_sizing_pass, normal_visible, normal_window, normal) =
            render_frame(normal_window_rect, &mut settings);

        let constrained_window_rect =
            egui::Rect::from_min_size(egui::pos2(20.0, 20.0), egui::vec2(520.0, 180.0));
        let _ = render_frame(constrained_window_rect, &mut settings);
        let (bounded_sizing_pass, bounded_visible, bounded_window, bounded) =
            render_frame(constrained_window_rect, &mut settings);

        let mut scroll_state = egui::scroll_area::State::load(&ctx, bounded.scroll_id)
            .expect("settings form stores its native scroll state");
        let legal_max_scroll_offset = (bounded.content_size.y - bounded.viewport.height()).max(0.0);
        scroll_state.offset.y = legal_max_scroll_offset;
        scroll_state.store(&ctx, bounded.scroll_id);
        let (scrolled_sizing_pass, scrolled_visible, scrolled_window, scrolled) =
            render_frame(constrained_window_rect, &mut settings);

        assert!(!normal_sizing_pass && normal_visible);
        assert!(!bounded_sizing_pass && bounded_visible);
        assert!(!scrolled_sizing_pass && scrolled_visible);
        assert!(screen_rect.contains_rect(normal_window));
        assert!(screen_rect.contains_rect(bounded_window));
        assert!(screen_rect.contains_rect(scrolled_window));
        assert!(normal.content_size.y <= normal.viewport.height() + 1.0);
        assert!(bounded.content_size.y > bounded.viewport.height());
        assert!(!bounded.viewport.contains_rect(bounded.save_rect));
        assert_eq!(bounded.scroll_id, scrolled.scroll_id);
        assert!(scrolled.scroll_offset.y > 0.0);
        assert!(scrolled.scroll_offset.y <= legal_max_scroll_offset + 1.0);
        assert!(scrolled.viewport.contains_rect(scrolled.save_rect));
        assert_eq!(settings, original_settings);
    }

    #[test]
    fn reconnected_windows_are_treated_as_usable() {
        let mut window = MmWindow::default();
        window.mark_reconnected(42);

        assert!(window.can_activate());
    }

    #[test]
    fn workspace_header_label_includes_name_and_hotkey_sequence() {
        let workspace = MmWorkspace {
            id: "workspace-id".into(),
            name: "Workspace Name".into(),
            hotkey: Some(MmHotkey {
                key: "F9".into(),
                ctrl: true,
                alt: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        assert_eq!(
            workspace_header_label(&workspace),
            "Workspace Name - Ctrl+Alt+F9"
        );
    }

    #[test]
    fn desktop_snapshot_states_distinguish_unverified_empty_ready_and_failed() {
        assert!(matches!(
            VirtualDesktopSnapshotState::default(),
            VirtualDesktopSnapshotState::Unverified
        ));
        assert!(matches!(
            VirtualDesktopSnapshotState::Ready(Vec::new()),
            VirtualDesktopSnapshotState::Ready(desktops) if desktops.is_empty()
        ));
        assert!(matches!(
            VirtualDesktopSnapshotState::Failed("offline".into()),
            VirtualDesktopSnapshotState::Failed(error) if error == "offline"
        ));
    }

    #[test]
    fn workspace_header_label_without_hotkey_uses_name_only() {
        let workspace = MmWorkspace {
            id: "workspace-id".into(),
            name: "Workspace Name".into(),
            ..Default::default()
        };

        assert_eq!(workspace_header_label(&workspace), "Workspace Name");
    }

    #[test]
    fn workspace_header_label_blank_name_falls_back_to_workspace_id() {
        let workspace = MmWorkspace {
            id: "workspace-id".into(),
            name: "   ".into(),
            ..Default::default()
        };

        assert_eq!(workspace_header_label(&workspace), "workspace-id");
    }

    #[test]
    fn invalid_hotkey_editor_state_disables_set() {
        let editor = HotkeyEditorState {
            workspace_id: "workspace".into(),
            key: "Ctrl+A".into(),
            ctrl: true,
            ..Default::default()
        };

        assert_eq!(
            hotkey_from_editor(&editor).validate(),
            MmHotkeyValidation::KeyContainsPlus
        );
        assert!(!hotkey_set_enabled(&editor));
    }

    #[test]
    fn valid_hotkey_editor_state_enables_set() {
        let editor = HotkeyEditorState {
            workspace_id: "workspace".into(),
            key: "F9".into(),
            ctrl: true,
            ..Default::default()
        };

        assert_eq!(
            hotkey_from_editor(&editor).validate(),
            MmHotkeyValidation::Valid
        );
        assert!(hotkey_set_enabled(&editor));
    }

    #[test]
    fn begin_hotkey_edit_loads_existing_hotkey() {
        let mut editor = HotkeyEditorState::default();
        let hotkey = MmHotkey {
            key: "F8".into(),
            ctrl: true,
            alt: true,
            ..Default::default()
        };

        begin_hotkey_edit(&mut editor, "workspace", Some(&hotkey));

        assert_eq!(editor.workspace_id, "workspace");
        assert_eq!(editor.key, "F8");
        assert!(editor.ctrl);
        assert!(editor.alt);
        assert!(!editor.shift);
        assert!(!editor.win);
    }

    #[test]
    fn begin_hotkey_edit_resets_editor_when_workspace_has_no_hotkey() {
        let mut editor = HotkeyEditorState {
            workspace_id: "old".into(),
            key: "F8".into(),
            ctrl: true,
            shift: true,
            alt: true,
            win: true,
        };

        begin_hotkey_edit(&mut editor, "workspace", None);

        assert_eq!(editor.workspace_id, "workspace");
        assert!(editor.key.is_empty());
        assert!(!editor.ctrl);
        assert!(!editor.shift);
        assert!(!editor.alt);
        assert!(!editor.win);
        assert!(hotkey_editor_is_empty(&editor));
    }

    #[test]
    fn cancel_hotkey_edit_leaves_stored_workspace_hotkey_unchanged() {
        let stored = MmHotkey {
            key: "F7".into(),
            ctrl: true,
            ..Default::default()
        };
        let mut workspace = MmWorkspace {
            id: "workspace".into(),
            hotkey: Some(stored.clone()),
            ..Default::default()
        };
        let mut editor = HotkeyEditorState {
            workspace_id: "workspace".into(),
            key: "NoSuchKey".into(),
            alt: true,
            ..Default::default()
        };

        cancel_hotkey_edit(&mut editor);

        assert_eq!(workspace.hotkey, Some(stored));
        assert!(editor.workspace_id.is_empty());
        assert!(editor.key.is_empty());
        // Keep the mutable binding meaningful for this helper-level transition test.
        workspace.name = "unchanged hotkey".into();
        assert_eq!(
            workspace.hotkey.as_ref().map(|hotkey| hotkey.key.as_str()),
            Some("F7")
        );
    }

    #[test]
    fn reset_workspace_hotkey_clears_stored_hotkey() {
        let mut workspace = MmWorkspace {
            id: "workspace".into(),
            hotkey: Some(MmHotkey {
                key: "F7".into(),
                ctrl: true,
                ..Default::default()
            }),
            ..Default::default()
        };

        reset_workspace_hotkey(&mut workspace);

        assert_eq!(workspace.hotkey, None);
    }

    #[test]
    fn set_window_rect_home_does_not_alter_target() {
        let target = MmRect {
            x: 10,
            y: 20,
            w: 300,
            h: 400,
        };
        let home = MmRect {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        };
        let mut workspace = workspace_with_window(Some(target), None);

        set_window_rect(&mut workspace, 0, true, home).expect("home rect should be set");

        let window = &workspace.windows[0];
        assert_eq!(window.home_rect, Some(home));
        assert_eq!(window.target_rect, Some(target));
        assert_eq!(window.alias, "Alias");
        assert_eq!(window.captured_title, "Title");
        assert_eq!(window.hwnd, 42);
        assert!(window.valid);
    }

    #[test]
    fn set_window_rect_target_does_not_alter_home() {
        let home = MmRect {
            x: 10,
            y: 20,
            w: 300,
            h: 400,
        };
        let target = MmRect {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        };
        let mut workspace = workspace_with_window(None, Some(home));

        set_window_rect(&mut workspace, 0, false, target).expect("target rect should be set");

        let window = &workspace.windows[0];
        assert_eq!(window.home_rect, Some(home));
        assert_eq!(window.target_rect, Some(target));
        assert_eq!(window.alias, "Alias");
        assert_eq!(window.captured_title, "Title");
        assert_eq!(window.hwnd, 42);
        assert!(window.valid);
    }

    #[test]
    fn set_window_rect_missing_window_returns_controlled_error() {
        let mut workspace = MmWorkspace {
            id: "workspace".into(),
            ..Default::default()
        };
        let rect = MmRect {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        };

        let err = set_window_rect(&mut workspace, 0, true, rect).expect_err("missing row errors");

        assert!(err.contains("Missing window row for capture"));
        assert!(workspace.windows.is_empty());
    }

    #[test]
    fn set_window_rect_in_workspaces_missing_workspace_returns_controlled_result() {
        let mut workspaces = vec![workspace_with_window(None, None)];
        let rect = MmRect {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        };

        let result = set_window_rect_in_workspaces(&mut workspaces, "missing", 0, true, rect);

        assert_eq!(result, RectCaptureMutationResult::MissingWorkspace);
        assert_eq!(workspaces[0].windows[0].home_rect, None);
        assert_eq!(workspaces[0].windows[0].target_rect, None);
    }

    #[test]
    fn set_window_rect_in_workspaces_missing_window_returns_controlled_result() {
        let mut workspaces = vec![MmWorkspace {
            id: "workspace".into(),
            ..Default::default()
        }];
        let rect = MmRect {
            x: 1,
            y: 2,
            w: 30,
            h: 40,
        };

        let result = set_window_rect_in_workspaces(&mut workspaces, "workspace", 0, false, rect);

        assert!(matches!(
            result,
            RectCaptureMutationResult::MissingWindow(ref err)
                if err.contains("Missing window row for capture")
        ));
        assert!(workspaces[0].windows.is_empty());
    }

    #[test]
    fn throttle_prevents_repeated_title_queries() {
        let mut workspaces = vec![workspace_with_window(None, None)];
        let start = Instant::now();
        let mut last = None;
        let mut title_queries = 0;

        assert!(refresh_live_titles_throttled_with(
            &mut workspaces,
            &mut last,
            start,
            false,
            crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL,
            |_| true,
            |_| {
                title_queries += 1;
                Some("Live one".into())
            },
        ));
        assert!(!refresh_live_titles_throttled_with(
            &mut workspaces,
            &mut last,
            start + crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL / 2,
            false,
            crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL,
            |_| true,
            |_| {
                title_queries += 1;
                Some("Live two".into())
            },
        ));

        assert_eq!(title_queries, 1);
        assert_eq!(workspaces[0].windows[0].live_title, "Live one");
        assert_eq!(workspaces[0].windows[0].captured_title, "Title");
    }

    #[test]
    fn live_title_refresh_helper_does_not_mark_workspace_or_bindings_dirty() {
        let dir = tempfile::tempdir().unwrap();
        let settings_path = dir.path().join("settings.json");
        let state = crate::multi_manager::state::MultiManagerState::load_or_default(
            &crate::settings::MultiManagerSettings {
                enabled: false,
                ..Default::default()
            },
            settings_path.to_str().unwrap(),
        );
        state
            .workspaces
            .lock()
            .unwrap()
            .push(workspace_with_window(None, None));
        let mut last = None;

        assert!(refresh_live_titles_throttled_with(
            &mut state.workspaces.lock().unwrap(),
            &mut last,
            Instant::now(),
            false,
            crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL,
            |_| true,
            |_| Some("Runtime only".into()),
        ));

        assert!(!state.dirty);
        assert!(!state.bindings_dirty);
    }

    #[test]
    fn manual_refresh_bypasses_throttle() {
        let mut workspaces = vec![workspace_with_window(None, None)];
        let start = Instant::now();
        let mut last = Some(start);
        let mut title_queries = 0;

        assert!(refresh_live_titles_throttled_with(
            &mut workspaces,
            &mut last,
            start,
            true,
            crate::multi_manager::state::LIVE_TITLE_REFRESH_INTERVAL,
            |_| true,
            |_| {
                title_queries += 1;
                Some("Manual live".into())
            },
        ));

        assert_eq!(title_queries, 1);
        assert_eq!(workspaces[0].windows[0].live_title, "Manual live");
        assert_eq!(workspaces[0].windows[0].captured_title, "Title");
    }

    fn workspace_with_window(
        target_rect: Option<MmRect>,
        home_rect: Option<MmRect>,
    ) -> MmWorkspace {
        MmWorkspace {
            id: "workspace".into(),
            windows: vec![MmWindow {
                alias: "Alias".into(),
                captured_title: "Title".into(),
                home_rect,
                target_rect,
                hwnd: 42,
                valid: true,
                ..Default::default()
            }],
            ..Default::default()
        }
    }
}
