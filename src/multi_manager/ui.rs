use crate::gui::LauncherApp;
use crate::multi_manager::bindings;
use crate::multi_manager::model::{
    MmHotkey, MmHotkeyValidation, MmRect, MmWindow, MmWorkspace, PendingCaptureAction,
    new_workspace_id,
};
use crate::multi_manager::win;
use eframe::egui;
use std::sync::atomic::Ordering;

#[derive(Debug, Default)]
pub struct MultiManagerDialog {
    pub open: bool,
    all_expanded: bool,
    expand_all_signal: Option<bool>,
    rename: WorkspaceRenameState,
    hotkey_editor: HotkeyEditorState,
    hotkey_editor_needs_focus: bool,
    confirm: DeleteConfirmState,
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

impl MultiManagerDialog {
    pub fn ui(&mut self, ctx: &egui::Context, app: &mut LauncherApp) {
        app.multi_manager_poll_capture(ctx);
        if !self.open {
            return;
        }
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
                        if ui.button("Save").clicked() {
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
                        if ui.button("Reconnect Windows").clicked() {
                            app.multi_manager_reconnect_windows();
                        }
                        if ui.button("Refresh Titles").clicked() {
                            app.multi_manager_refresh_titles();
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
                        ui.label(if app.multi_manager.dirty {
                            "● dirty"
                        } else {
                            "saved"
                        });
                        let last = app
                            .multi_manager
                            .last_hotkey_info
                            .lock()
                            .ok()
                            .and_then(|g| g.clone())
                            .map(|(s, _)| s)
                            .unwrap_or_else(|| "none".into());
                        ui.label(format!("last hotkey: {last}"));
                    });
                    if self.confirm.reload {
                        ui.horizontal(|ui| {
                            ui.colored_label(
                                egui::Color32::YELLOW,
                                "Reload and discard unsaved changes?",
                            );
                            if ui.button("Confirm").clicked() {
                                app.multi_manager_reload();
                                self.confirm.reload = false;
                            }
                            if ui.button("Cancel").clicked() {
                                self.confirm.reload = false;
                            }
                        });
                    }
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
            ui.colored_label(egui::Color32::LIGHT_BLUE, capture_banner_text(action));
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
        let color = workspace_header_color(&workspace);
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
                ui.label(egui::RichText::new(label).color(color));
            })
            .body(|ui| {
                ui.push_id(("multi_manager_workspace", id), |ui| {
                    ui.horizontal(|ui| {
                        if self.rename.workspace_id == id {
                            ui.add(
                                egui::TextEdit::singleline(&mut self.rename.value)
                                    .id_source(("mm_workspace_rename", id)),
                            )
                            .changed();
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
                    if self.confirm.workspace_id.as_deref() == Some(id) {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::RED, "Confirm delete workspace?");
                            if ui.button("Delete").clicked() {
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

    fn hotkey_ui(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str) {
        if self.hotkey_editor.workspace_id != id {
            if ui.button("Edit hotkey").clicked() {
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

        ui.horizontal(|ui| {
            ui.checkbox(&mut self.hotkey_editor.ctrl, "Ctrl");
            ui.checkbox(&mut self.hotkey_editor.shift, "Shift");
            ui.checkbox(&mut self.hotkey_editor.alt, "Alt");
            ui.checkbox(&mut self.hotkey_editor.win, "Win");
            let response = ui.add(
                egui::TextEdit::singleline(&mut self.hotkey_editor.key)
                    .id_source(("mm_workspace_hotkey_key", id)),
            );
            if self.hotkey_editor_needs_focus {
                response.request_focus();
                self.hotkey_editor_needs_focus = false;
            }

            let candidate = hotkey_from_editor(&self.hotkey_editor);
            match candidate.validate() {
                MmHotkeyValidation::Valid => {
                    ui.colored_label(egui::Color32::GREEN, MmHotkeyValidation::Valid.label());
                }
                validation if hotkey_editor_is_empty(&self.hotkey_editor) => {
                    ui.colored_label(egui::Color32::GRAY, "no hotkey set");
                    let _ = validation;
                }
                validation => {
                    ui.colored_label(egui::Color32::RED, validation.label());
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
            if ui.button("Cancel").clicked() {
                cancel_hotkey_edit(&mut self.hotkey_editor);
            }
        });
    }

    fn window_card(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str, index: usize) {
        ui.push_id(("multi_manager_window", id, index), |ui| {
            ui.group(|ui| {
                let win = get_ws(app, id)
                    .and_then(|w| w.windows.get(index).cloned())
                    .unwrap_or_default();
                ui.label(format!("Window {}", index + 1));
                let mut alias = win.alias.clone();
                if ui
                    .add(egui::TextEdit::singleline(&mut alias).id_source((
                        "mm_window_alias",
                        id,
                        index,
                    )))
                    .changed()
                {
                    app.multi_manager.with_workspace_mut(id, |w| {
                        if let Some(x) = w.windows.get_mut(index) {
                            x.alias = alias
                        }
                    });
                }
                ui.label(format!("HWND: {}", win.hwnd));
                let (status_text, status_color) = window_status_text_color(
                    &win,
                    win.hwnd != 0 && crate::multi_manager::win::is_valid_window(win.hwnd),
                    false,
                );
                ui.colored_label(status_color, status_text);
                if is_duplicate_hwnd(app, win.hwnd, id, index) {
                    ui.colored_label(egui::Color32::YELLOW, "⚠ duplicate HWND");
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
                    ui.horizontal(|ui| {
                        ui.colored_label(egui::Color32::RED, "Confirm delete window?");
                        if ui.button("Delete").clicked() {
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

fn workspace_header_color(workspace: &MmWorkspace) -> egui::Color32 {
    if workspace.disabled {
        egui::Color32::YELLOW
    } else if workspace.valid
        && workspace
            .hotkey
            .as_ref()
            .is_some_and(|hotkey| hotkey.is_valid())
    {
        egui::Color32::GREEN
    } else {
        egui::Color32::RED
    }
}

fn window_status_text_color(
    win: &MmWindow,
    hwnd_is_currently_valid: bool,
    ambiguous: bool,
) -> (&'static str, egui::Color32) {
    if ambiguous {
        ("ambiguous", egui::Color32::YELLOW)
    } else if win.hwnd == 0 {
        ("missing HWND", egui::Color32::RED)
    } else if !win.valid {
        ("invalid", egui::Color32::RED)
    } else if !hwnd_is_currently_valid {
        ("stale HWND", egui::Color32::YELLOW)
    } else {
        ("valid", egui::Color32::GREEN)
    }
}

fn window_metadata_ui(ui: &mut egui::Ui, win: &MmWindow) {
    ui.label(format!("Title: {}", win.current_display_title()));
    if !win.live_title.trim().is_empty() && win.live_title.trim() != win.captured_title.trim() {
        ui.label(format!("Captured title: {}", win.fallback_title()));
    }
    ui.label(format!("Class: {}", win.class_name));
    ui.label(format!("Executable: {}", win.executable));
    if !win.process_path.trim().is_empty() {
        ui.collapsing("Process", |ui| {
            ui.label(format!("Process: {}", win.process_path));
        });
    }
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
                let mut changed = false;
                changed |= ui.checkbox(&mut s.enabled, "Enabled").changed();
                ui.label("Workspace file path");
                changed |= ui.text_edit_singleline(&mut s.workspaces_path).changed();
                ui.label("Bindings file path");
                changed |= ui.text_edit_singleline(&mut s.bindings_path).changed();
                changed |= ui.checkbox(&mut s.auto_save, "Auto-save").changed();
                changed |= ui
                    .checkbox(&mut s.save_on_exit, "Save on launcher exit")
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.show_force_recapture_prompt,
                        "Show force recapture prompt",
                    )
                    .changed();
                changed |= ui
                    .checkbox(&mut s.developer_debugging, "Developer debugging")
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut s.hotkey_poll_ms)
                            .clamp_range(10..=5000)
                            .prefix("Hotkey poll interval ")
                            .suffix(" ms"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.auto_reconnect_on_load,
                        "Auto-reconnect windows on load",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.auto_reconnect_missing_windows,
                        "Auto-reconnect missing windows while running",
                    )
                    .changed();
                changed |= ui
                    .add(
                        egui::DragValue::new(&mut s.auto_reconnect_interval_ms)
                            .clamp_range(500..=60000)
                            .prefix("Auto-reconnect interval ")
                            .suffix(" ms"),
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.ignore_launcher_window_on_capture,
                        "Ignore launcher window on capture",
                    )
                    .changed();
                changed |= ui
                    .checkbox(
                        &mut s.hide_launcher_before_toggle,
                        "Hide launcher before toggle",
                    )
                    .changed();
                if changed {
                    app.multi_manager_settings = s.clone();
                    app.multi_manager.auto_save = s.auto_save;
                    app.multi_manager.auto_reconnect_on_load = s.auto_reconnect_on_load;
                    app.multi_manager.auto_reconnect_missing_windows =
                        s.auto_reconnect_missing_windows;
                    app.multi_manager.reconnect_interval =
                        std::time::Duration::from_millis(s.auto_reconnect_interval_ms);
                    let control = &app.multi_manager.runtime.control;
                    control.enabled.store(s.enabled, Ordering::Relaxed);
                    control
                        .auto_reconnect_missing_windows
                        .store(s.auto_reconnect_missing_windows, Ordering::Relaxed);
                    control
                        .auto_reconnect_interval_ms
                        .store(s.auto_reconnect_interval_ms, Ordering::Relaxed);
                }
                ui.separator();
                if ui.button("Save MultiManager Settings").clicked() {
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
    id_source: impl std::hash::Hash,
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
        ui.horizontal(|ui| {
            ui.label(label);
            let mut changed = false;
            changed |= ui.add(egui::DragValue::new(&mut r.x)).changed();
            changed |= ui.add(egui::DragValue::new(&mut r.y)).changed();
            changed |= ui
                .add(egui::DragValue::new(&mut r.w).clamp_range(1..=10000))
                .changed();
            changed |= ui
                .add(egui::DragValue::new(&mut r.h).clamp_range(1..=10000))
                .changed();
            if changed {
                let _ = set(r);
            }
        });
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
    fn window_status_reports_missing_hwnd() {
        let window = MmWindow {
            hwnd: 0,
            valid: true,
            ..Default::default()
        };

        assert_eq!(
            window_status_text_color(&window, false, false),
            ("missing HWND", egui::Color32::RED)
        );
    }

    #[test]
    fn window_status_reports_invalid() {
        let window = MmWindow {
            hwnd: 42,
            valid: false,
            ..Default::default()
        };

        assert_eq!(
            window_status_text_color(&window, true, false),
            ("invalid", egui::Color32::RED)
        );
    }

    #[test]
    fn window_status_reports_stale_hwnd() {
        let window = MmWindow {
            hwnd: 42,
            valid: true,
            ..Default::default()
        };

        assert_eq!(
            window_status_text_color(&window, false, false),
            ("stale HWND", egui::Color32::YELLOW)
        );
    }

    #[test]
    fn window_status_reports_valid() {
        let window = MmWindow {
            hwnd: 42,
            valid: true,
            ..Default::default()
        };

        assert_eq!(
            window_status_text_color(&window, true, false),
            ("valid", egui::Color32::GREEN)
        );
    }

    #[test]
    fn window_status_reports_ambiguous_when_recorded() {
        let window = MmWindow {
            hwnd: 42,
            valid: true,
            ..Default::default()
        };

        assert_eq!(
            window_status_text_color(&window, true, true),
            ("ambiguous", egui::Color32::YELLOW)
        );
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
