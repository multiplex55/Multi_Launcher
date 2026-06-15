use crate::gui::LauncherApp;
use crate::multi_manager::bindings;
use crate::multi_manager::model::{
    new_workspace_id, MmHotkey, MmRect, MmWindow, MmWorkspace, PendingCaptureAction,
};
use crate::multi_manager::win;
use eframe::egui;
use std::sync::atomic::Ordering;

#[derive(Debug, Default)]
pub struct MultiManagerDialog {
    pub open: bool,
    expanded_all: bool,
    rename: WorkspaceRenameState,
    hotkey_editor: HotkeyEditorState,
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
                    if ui.button("Save Bindings").clicked() {
                        app.multi_manager_save_bindings();
                    }
                    if ui.button("Restore Bindings").clicked() {
                        app.multi_manager_restore_bindings();
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
                        .button(if self.expanded_all {
                            "Collapse All"
                        } else {
                            "Expand All"
                        })
                        .clicked()
                    {
                        self.expanded_all = !self.expanded_all;
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
            });
        self.open = open;
    }

    fn capture_banner(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp) {
        if let Some(action) = &app.multi_manager.pending_capture {
            ui.colored_label(egui::Color32::LIGHT_BLUE, "Capture mode active: press Enter to capture active window, S to skip current item, or Escape to cancel remaining queue.");
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
                    ui.label(format!(
                        "Recapturing workspace {workspace_id}, window {}",
                        window_index + 1
                    ));
                }
            }
        }
    }

    fn workspace_card(&mut self, ui: &mut egui::Ui, app: &mut LauncherApp, id: &str) {
        let name = app
            .multi_manager
            .workspaces
            .lock()
            .ok()
            .and_then(|w| {
                w.iter().find(|x| x.id == id).map(|x| {
                    if x.name.is_empty() {
                        x.id.clone()
                    } else {
                        x.name.clone()
                    }
                })
            })
            .unwrap_or_else(|| id.into());
        egui::CollapsingHeader::new(name)
            .id_source(("multi_manager_workspace_header", id))
            .default_open(self.expanded_all)
            .show(ui, |ui| {
                ui.push_id(("multi_manager_workspace", id), |ui| {
                    ui.horizontal(|ui| {
                        if self.rename.workspace_id == id {
                            if ui
                                .add(
                                    egui::TextEdit::singleline(&mut self.rename.value)
                                        .id_source(("mm_workspace_rename", id)),
                                )
                                .changed()
                            {}
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
                self.hotkey_editor.workspace_id = id.into();
                if let Some(h) = get_ws(app, id).and_then(|w| w.hotkey) {
                    self.hotkey_editor.key = h.key;
                    self.hotkey_editor.ctrl = h.ctrl;
                    self.hotkey_editor.shift = h.shift;
                    self.hotkey_editor.alt = h.alt;
                    self.hotkey_editor.win = h.win;
                }
            }
            return;
        }
        ui.horizontal(|ui| {
            ui.checkbox(&mut self.hotkey_editor.ctrl, "Ctrl");
            ui.checkbox(&mut self.hotkey_editor.shift, "Shift");
            ui.checkbox(&mut self.hotkey_editor.alt, "Alt");
            ui.checkbox(&mut self.hotkey_editor.win, "Win");
            ui.add(
                egui::TextEdit::singleline(&mut self.hotkey_editor.key)
                    .id_source(("mm_workspace_hotkey_key", id)),
            );
            let valid = !self.hotkey_editor.key.trim().is_empty();
            ui.label(if valid { "valid" } else { "invalid" });
            if ui.button("Set").clicked() && valid {
                let h = MmHotkey {
                    key: self.hotkey_editor.key.clone(),
                    ctrl: self.hotkey_editor.ctrl,
                    shift: self.hotkey_editor.shift,
                    alt: self.hotkey_editor.alt,
                    win: self.hotkey_editor.win,
                };
                app.multi_manager
                    .with_workspace_mut(id, |w| w.hotkey = Some(h));
                self.hotkey_editor = Default::default();
            }
            if ui.button("Reset").clicked() {
                app.multi_manager
                    .with_workspace_mut(id, |w| w.hotkey = None);
                self.hotkey_editor = Default::default();
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
                ui.label(format!("Original title: {}", win.title));
                ui.label(format!("HWND: {}", win.hwnd));
                if is_duplicate_hwnd(app, win.hwnd, id, index) {
                    ui.colored_label(egui::Color32::YELLOW, "⚠ duplicate HWND");
                }
                ui.label(if win.valid { "valid" } else { "invalid" });
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
                        move_window(&win, true);
                    }
                    if ui.button("Move Target").clicked() {
                        move_window(&win, false);
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
                    app.multi_manager
                        .runtime
                        .control
                        .enabled
                        .store(s.enabled, Ordering::Relaxed);
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
    if let Ok(mut w) = app.multi_manager.workspaces.lock() {
        if let Some(i) = w.iter().position(|x| x.id == id) {
            let j = (i as isize + delta).clamp(0, w.len().saturating_sub(1) as isize) as usize;
            w.swap(i, j);
        }
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
    if let Some(c) = win::active_window() {
        app.multi_manager.with_workspace_mut(id, |w| {
            if let Some(x) = w.windows.get_mut(index) {
                if home {
                    x.home_rect = Some(c.rect)
                } else {
                    x.target_rect = Some(c.rect)
                };
                x.hwnd = c.hwnd;
                x.title = c.title;
            }
        });
    }
}
fn move_window(w: &MmWindow, home: bool) {
    if let Some(r) = if home { w.home_rect } else { w.target_rect } {
        let _ = win::move_window_to_rect(w.hwnd, r);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_dialog_state_is_closed() {
        assert!(!MultiManagerDialog::default().open);
        assert!(!MultiManagerSettingsDialog::default().open);
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
