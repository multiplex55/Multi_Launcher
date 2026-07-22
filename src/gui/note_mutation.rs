use crate::gui::LauncherApp;
use crate::plugins::note::{load_notes, save_note};
use anyhow::{Context, anyhow};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum NoteMutationOutcome<R> {
    Changed(R),
    Unchanged(R),
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct NoteMutationResult {
    pub wrapped_links: usize,
    pub skipped_existing_links: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NoteMutationOutput {
    pub content: String,
    pub result: NoteMutationResult,
}

impl NoteMutationOutput {
    pub(crate) fn changed(content: impl Into<String>, result: NoteMutationResult) -> Self {
        Self {
            content: content.into(),
            result,
        }
    }

    pub(crate) fn unchanged(content: impl Into<String>, result: NoteMutationResult) -> Self {
        Self {
            content: content.into(),
            result,
        }
    }

    fn into_outcome(self, source: &str) -> NoteMutationOutcome<Self> {
        if self.content == source {
            NoteMutationOutcome::Unchanged(self)
        } else {
            NoteMutationOutcome::Changed(self)
        }
    }
}

impl LauncherApp {
    pub(crate) fn mutate_note_by_slug(
        &mut self,
        slug: &str,
        mutation: impl FnOnce(&str) -> NoteMutationOutput,
    ) -> anyhow::Result<NoteMutationOutcome<NoteMutationResult>> {
        match self.try_mutate_note_by_slug(slug, mutation) {
            Ok(outcome) => Ok(outcome),
            Err(err) => {
                self.report_error_message("note.mutation", err.to_string());
                Err(err)
            }
        }
    }

    fn try_mutate_note_by_slug(
        &mut self,
        slug: &str,
        mutation: impl FnOnce(&str) -> NoteMutationOutput,
    ) -> anyhow::Result<NoteMutationOutcome<NoteMutationResult>> {
        if let Some(index) = self
            .note_panels
            .iter()
            .position(|panel| panel.note_slug() == slug)
        {
            let mut panel = self.note_panels.remove(index);
            let source = panel.note_content().to_owned();
            let outcome = mutation(&source).into_outcome(&source);
            let result = match outcome {
                NoteMutationOutcome::Unchanged(output) => {
                    NoteMutationOutcome::Unchanged(output.result)
                }
                NoteMutationOutcome::Changed(output) => {
                    let content = output.content;
                    let mut note = panel.note_content_clone_for_mutation();
                    note.content = content.clone();
                    let save_result = save_note(&mut note, true).context("save mutated open note");
                    if let Err(err) = save_result {
                        self.note_panels.insert(index, panel);
                        return Err(err);
                    }
                    panel.replace_content_from_mutation(content, 0.0);
                    let result = output.result;
                    self.note_panels.insert(index, panel);
                    self.refresh_after_note_mutation();
                    return Ok(NoteMutationOutcome::Changed(result));
                }
            };
            self.note_panels.insert(index, panel);
            return Ok(result);
        }

        let mut note = load_notes()
            .context("load notes for mutation")?
            .into_iter()
            .find(|note| note.slug == slug)
            .ok_or_else(|| anyhow!("Note not found: {slug}"))?;
        let source = note.content.clone();
        match mutation(&source).into_outcome(&source) {
            NoteMutationOutcome::Unchanged(output) => {
                Ok(NoteMutationOutcome::Unchanged(output.result))
            }
            NoteMutationOutcome::Changed(output) => {
                note.content = output.content;
                save_note(&mut note, true).context("save mutated closed note")?;
                self.refresh_after_note_mutation();
                Ok(NoteMutationOutcome::Changed(output.result))
            }
        }
    }

    fn refresh_after_note_mutation(&mut self) {
        #[cfg(test)]
        {
            self.note_mutation_refresh_count += 1;
        }
        #[cfg(test)]
        {
            self.note_mutation_cache_refresh_count += 1;
        }
        self.dashboard_data_cache.refresh_notes();
        if self.notes_dialog.open {
            #[cfg(test)]
            {
                self.note_mutation_quick_notes_refresh_count += 1;
            }
            self.notes_dialog.refresh_entries_from_notes();
        }
        for panel in &mut self.note_panels {
            #[cfg(test)]
            {
                self.note_mutation_panel_invalidation_count += 1;
            }
            panel.invalidate_note_derived_data_after_external_mutation();
        }
        self.last_results_valid = false;
        #[cfg(test)]
        {
            self.note_mutation_search_count += 1;
        }
        self.search();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gui::NotePanel;
    use crate::plugins::note::{Note, load_notes, save_note, save_notes};
    use crate::{plugin::PluginManager, settings::Settings};
    use eframe::egui;
    use once_cell::sync::Lazy;
    use std::sync::{Arc, Mutex, atomic::AtomicBool};
    use tempfile::tempdir;

    static TEST_MUTEX: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

    fn new_app(ctx: &egui::Context) -> LauncherApp {
        LauncherApp::new(
            ctx,
            Arc::new(Vec::new()),
            0,
            PluginManager::new(),
            "actions.json".into(),
            "settings.json".into(),
            Settings::default(),
            None,
            None,
            None,
            None,
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
            Arc::new(AtomicBool::new(false)),
        )
    }

    fn note(title: &str, slug: &str, content: &str) -> Note {
        Note {
            title: title.into(),
            path: Default::default(),
            content: content.into(),
            tags: Vec::new(),
            links: Vec::new(),
            slug: slug.into(),
            alias: None,
            aliases: Vec::new(),
            entity_refs: Vec::new(),
        }
    }

    fn setup() -> (tempfile::TempDir, egui::Context, LauncherApp) {
        let dir = tempdir().unwrap();
        let notes_dir = dir.path().join("notes");
        std::fs::create_dir_all(&notes_dir).unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &notes_dir) };
        unsafe { std::env::set_var("HOME", dir.path()) };
        save_notes(&[]).unwrap();
        let ctx = egui::Context::default();
        let app = new_app(&ctx);
        (dir, ctx, app)
    }

    #[test]
    fn open_note_mutation_uses_unsaved_panel_content_and_saves_it() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        let mut disk_note = note("Alpha", "alpha", "disk");
        save_note(&mut disk_note, true).unwrap();
        let mut panel = NotePanel::from_note(disk_note);
        panel.replace_content_after_external_mutation("unsaved".into());
        app.note_panels.push(panel);

        let outcome = app
            .mutate_note_by_slug("alpha", |content| {
                assert_eq!(content, "unsaved");
                NoteMutationOutput::changed(
                    format!("{content} changed"),
                    NoteMutationResult {
                        wrapped_links: 1,
                        skipped_existing_links: 0,
                    },
                )
            })
            .unwrap();

        assert_eq!(
            outcome,
            NoteMutationOutcome::Changed(NoteMutationResult {
                wrapped_links: 1,
                skipped_existing_links: 0,
            })
        );

        assert_eq!(app.note_panels[0].note_content(), "unsaved changed");
        let saved = load_notes().unwrap().remove(0);
        assert_eq!(saved.content, "# Alpha\n\nunsaved changed");
    }

    #[test]
    fn closed_note_mutation_uses_persisted_content() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        let mut disk_note = note("Alpha", "alpha", "persisted");
        save_note(&mut disk_note, true).unwrap();

        let outcome = app
            .mutate_note_by_slug("alpha", |content| {
                assert_eq!(content, "# Alpha\n\npersisted");
                NoteMutationOutput::changed(
                    format!("{content} updated"),
                    NoteMutationResult::default(),
                )
            })
            .unwrap();

        assert_eq!(
            outcome,
            NoteMutationOutcome::Changed(NoteMutationResult::default())
        );

        assert_eq!(
            load_notes().unwrap()[0].content,
            "# Alpha\n\npersisted updated"
        );
    }

    #[test]
    fn unchanged_mutation_does_not_rewrite_file_or_refresh() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        let mut disk_note = note("Alpha", "alpha", "same");
        save_note(&mut disk_note, true).unwrap();
        let path = load_notes().unwrap()[0].path.clone();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();

        let outcome = app
            .mutate_note_by_slug("alpha", |content| {
                NoteMutationOutput::unchanged(content, NoteMutationResult::default())
            })
            .unwrap();

        assert_eq!(
            outcome,
            NoteMutationOutcome::Unchanged(NoteMutationResult::default())
        );

        let after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after);
        assert_eq!(app.note_mutation_refresh_count, 0);
        assert_eq!(app.note_mutation_cache_refresh_count, 0);
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 0);
        assert_eq!(app.note_mutation_panel_invalidation_count, 0);
        assert_eq!(app.note_mutation_search_count, 0);
    }

    #[test]
    fn unchanged_open_note_mutation_leaves_panel_content_untouched() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        let mut disk_note = note("Alpha", "alpha", "disk");
        save_note(&mut disk_note, true).unwrap();
        let path = load_notes().unwrap()[0].path.clone();
        let before = std::fs::metadata(&path).unwrap().modified().unwrap();
        let mut panel = NotePanel::from_note(disk_note);
        panel.replace_content_after_external_mutation("unsaved".into());
        app.note_panels.push(panel);

        let outcome = app
            .mutate_note_by_slug("alpha", |content| {
                assert_eq!(content, "unsaved");
                NoteMutationOutput::unchanged(
                    content,
                    NoteMutationResult {
                        wrapped_links: 2,
                        skipped_existing_links: 3,
                    },
                )
            })
            .unwrap();

        assert_eq!(
            outcome,
            NoteMutationOutcome::Unchanged(NoteMutationResult {
                wrapped_links: 2,
                skipped_existing_links: 3,
            })
        );
        assert_eq!(app.note_panels[0].note_content(), "unsaved");
        let after = std::fs::metadata(&path).unwrap().modified().unwrap();
        assert_eq!(before, after);
        assert_eq!(app.note_mutation_refresh_count, 0);
        assert_eq!(app.note_mutation_cache_refresh_count, 0);
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 0);
        assert_eq!(app.note_mutation_panel_invalidation_count, 0);
        assert_eq!(app.note_mutation_search_count, 0);
    }

    #[test]
    fn changed_mutation_refreshes_note_derived_state_once() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        app.notes_dialog.open();
        let mut disk_note = note("Alpha", "alpha", "old");
        save_note(&mut disk_note, true).unwrap();

        let outcome = app
            .mutate_note_by_slug("alpha", |_| {
                NoteMutationOutput::changed(
                    "new",
                    NoteMutationResult {
                        wrapped_links: 4,
                        skipped_existing_links: 5,
                    },
                )
            })
            .unwrap();

        assert_eq!(
            outcome,
            NoteMutationOutcome::Changed(NoteMutationResult {
                wrapped_links: 4,
                skipped_existing_links: 5,
            })
        );
        assert_eq!(app.note_mutation_refresh_count, 1);
        assert_eq!(app.note_mutation_cache_refresh_count, 1);
        assert_eq!(app.note_mutation_quick_notes_refresh_count, 1);
        assert_eq!(app.note_mutation_panel_invalidation_count, 0);
        assert_eq!(app.note_mutation_search_count, 1);
    }

    #[test]
    fn failed_open_note_save_reports_error_and_preserves_unsaved_edits() {
        let _lock = TEST_MUTEX.lock().unwrap();
        let (_dir, _ctx, mut app) = setup();
        let mut disk_note = note("Alpha", "alpha", "disk");
        save_note(&mut disk_note, true).unwrap();
        let mut panel = NotePanel::from_note(disk_note);
        panel.replace_content_after_external_mutation("unsaved".into());
        app.note_panels.push(panel);
        let invalid_notes_dir = _dir.path().join("not-a-dir.md");
        std::fs::write(&invalid_notes_dir, "not a directory").unwrap();
        unsafe { std::env::set_var("ML_NOTES_DIR", &invalid_notes_dir) };

        let err = app
            .mutate_note_by_slug("alpha", |_| {
                NoteMutationOutput::changed("changed", NoteMutationResult::default())
            })
            .unwrap_err();

        assert!(err.to_string().contains("save mutated open note"));
        assert!(
            app.error
                .as_ref()
                .unwrap()
                .contains("save mutated open note")
        );
        assert_eq!(app.note_panels[0].note_content(), "unsaved");
    }
}
