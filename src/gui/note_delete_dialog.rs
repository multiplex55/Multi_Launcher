use super::{push_toast, LauncherApp};
use crate::plugins::note::{load_notes, remove_note};
use eframe::egui::{self, Context};
use egui_toast::{Toast, ToastKind, ToastOptions};

#[derive(Default)]
pub struct NoteDeleteDialog {
    pub open: bool,
    slug: String,
}

impl NoteDeleteDialog {
    pub fn open(&mut self, slug: String) {
        self.open = true;
        self.slug = slug;
    }

    pub fn ui(&mut self, ctx: &Context, app: &mut LauncherApp) {
        if !self.open {
            return;
        }
        let mut keep_open = true;
        let slug = self.slug.clone();
        egui::Window::new("Delete note?")
            .open(&mut keep_open)
            .collapsible(false)
            .resizable(false)
            .show(ctx, |ui| {
                ui.label(format!("Delete note '{slug}'?"));
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        if let Ok(notes) = load_notes() {
                            if let Some((idx, note)) =
                                notes.into_iter().enumerate().find(|(_, n)| n.slug == slug)
                            {
                                let word_count = note.content.split_whitespace().count();
                                if let Err(e) = remove_note(idx) {
                                    app.set_error(format!("Failed to remove note: {e}"));
                                } else {
                                    if app.enable_toasts {
                                        push_toast(
                                            &mut app.toasts,
                                            Toast {
                                                text: format!(
                                                    "Removed note {} ({} words)",
                                                    note.title, word_count
                                                )
                                                .into(),
                                                kind: ToastKind::Success,
                                                options: ToastOptions::default()
                                                    .duration_in_seconds(app.toast_duration as f64),
                                            },
                                        );
                                    }
                                    app.notes_dialog.open();
                                    app.search();
                                    app.focus_input();
                                }
                            }
                        }
                        self.open = false;
                    }
                    if ui.button("Cancel").clicked() {
                        self.open = false;
                    }
                });
            });
        if !keep_open {
            self.open = false;
        }
    }
}
