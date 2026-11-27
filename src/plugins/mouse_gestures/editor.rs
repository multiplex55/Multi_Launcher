//! Checked, profile-bound editor transactions for the selected GPUI owner.
use super::{GpuiGestureOwner, SERVICE};
use crate::mouse_gestures::db::{BindingEntry, BindingKind, GestureDb, GestureEntry};
use crate::mouse_gestures::engine::DirMode;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::Ordering;

#[derive(Clone, Debug)]
pub struct GestureEditorSnapshot {
    pub database: GestureDb,
    pub revision: u64,
    pub enabled: bool,
    pub practice_mode: bool,
    generation: u64,
    bytes: Option<Vec<u8>>,
}

#[derive(Clone, Debug)]
pub enum GestureEdit {
    Add,
    Rename { index: usize, expected: GestureEntry, label: String },
    Tokens { index: usize, expected: GestureEntry, tokens: String },
    SetQuery { index: usize, expected: GestureEntry, binding: Option<usize>, label: String, query: String },
    Enabled { index: usize, expected: GestureEntry, enabled: bool },
}

fn read_exact(path: &Path) -> anyhow::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(Some(bytes)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error.into()),
    }
}

fn decode(path: &Path, bytes: Option<&[u8]>) -> anyhow::Result<GestureDb> {
    match bytes {
        None => Ok(GestureDb::default()),
        Some(bytes) if bytes.iter().all(u8::is_ascii_whitespace) => Ok(GestureDb::default()),
        Some(bytes) => crate::mouse_gestures::db::decode_gestures(path, bytes),
    }
}

fn apply_edit(db: &mut GestureDb, edit: GestureEdit) -> anyhow::Result<()> {
    if matches!(edit, GestureEdit::Add) {
        db.gestures.push(GestureEntry { label: format!("Gesture {}", db.gestures.len() + 1), tokens: String::new(), dir_mode: DirMode::Four, stroke: Vec::new(), enabled: true, bindings: Vec::new() });
        return Ok(());
    }
    let (index, expected) = match &edit {
        GestureEdit::Rename { index, expected, .. } | GestureEdit::Tokens { index, expected, .. } | GestureEdit::SetQuery { index, expected, .. } | GestureEdit::Enabled { index, expected, .. } => (*index, expected.clone()),
        GestureEdit::Add => unreachable!(),
    };
    anyhow::ensure!(db.gestures.get(index) == Some(&expected), "Gesture changed; reload before saving");
    match edit {
        GestureEdit::Rename { label, .. } => {
            anyhow::ensure!(!label.trim().is_empty(), "Gesture name cannot be empty");
            db.gestures[index].label = label;
        }
        GestureEdit::Tokens { tokens, .. } => {
            anyhow::ensure!(expected.dir_mode == DirMode::Four, "Only Four direction gestures can be edited here");
            let tokens = tokens.trim().to_ascii_uppercase();
            anyhow::ensure!(tokens.chars().all(|c| matches!(c, 'U' | 'D' | 'L' | 'R')), "Use only U, D, L and R directions");
            anyhow::ensure!(tokens.is_empty() || !expected.enabled || !db.gestures.iter().enumerate().any(|(i, row)| i != index && row.enabled && row.dir_mode == DirMode::Four && row.tokens == tokens), "An enabled gesture already uses those directions");
            if db.gestures[index].tokens != tokens {
                db.gestures[index].tokens = tokens;
                db.gestures[index].stroke.clear();
            }
        }
        GestureEdit::SetQuery { binding, label, query, .. } => {
            anyhow::ensure!(expected.dir_mode == DirMode::Four, "Only Four direction gestures can be edited here");
            anyhow::ensure!(!label.trim().is_empty(), "Binding name cannot be empty");
            anyhow::ensure!(!query.trim().is_empty() && !query.contains(['\r', '\n']), "Query must be a nonempty single line");
            let row = &mut db.gestures[index];
            if let Some(binding) = binding {
                anyhow::ensure!(row.bindings.get(binding).is_some_and(|entry| entry.kind == BindingKind::SetQuery), "Binding changed or is not a Set Query binding");
                row.bindings[binding].label = label;
                row.bindings[binding].action = query;
            } else { row.bindings.push(BindingEntry { label, kind: BindingKind::SetQuery, action: query, args: None, enabled: true }); }
        }
        GestureEdit::Enabled { enabled, .. } => {
            anyhow::ensure!(expected.dir_mode == DirMode::Four, "Only Four direction gestures can be edited here");
            anyhow::ensure!(!enabled || expected.enabled || expected.tokens.is_empty() || !db.gestures.iter().enumerate().any(|(i, row)| i != index && row.enabled && row.dir_mode == DirMode::Four && row.tokens == expected.tokens), "An enabled gesture already uses those directions");
            db.gestures[index].enabled = enabled;
        }
        GestureEdit::Add => unreachable!(),
    }
    Ok(())
}

impl GpuiGestureOwner {
    /// Read off the UI thread; malformed or unpublished data remains read-only.
    pub fn editor_snapshot(&self) -> anyhow::Result<GestureEditorSnapshot> {
        let runtime = SERVICE.get().ok_or_else(|| anyhow::anyhow!("Mouse Gestures runtime unavailable"))?.lock().map_err(|_| anyhow::anyhow!("Mouse Gestures runtime unavailable"))?;
        anyhow::ensure!(self.active.load(Ordering::Acquire) && runtime.gpui.as_ref().is_some_and(|owner| std::ptr::eq(owner.as_ref(), self)), "Mouse Gestures owner retired");
        let _permit = crate::common::repair_watch::require_path_read(&runtime.path)?;
        let _transaction = crate::mouse_gestures::db::gestures_transaction_guard();
        let bytes = read_exact(&runtime.path)?;
        let database = decode(&runtime.path, bytes.as_deref())?;
        anyhow::ensure!(*runtime.db.lock().map_err(|_| anyhow::anyhow!("Mouse Gestures database unavailable"))? == database, "Mouse Gestures changed; wait for the watcher");
        Ok(GestureEditorSnapshot { database, revision: runtime.directory_revision, enabled: runtime.settings.enabled, practice_mode: runtime.settings.practice_mode, generation: self.editor_revision.load(Ordering::Acquire), bytes })
    }

    /// Commit the exact snapshot, then publish it through the existing runtime owner.
    pub fn commit_editor(&self, snapshot: &GestureEditorSnapshot, edit: GestureEdit, current: impl Fn() -> bool) -> anyhow::Result<GestureEditorSnapshot> {
        let mut runtime = SERVICE.get().ok_or_else(|| anyhow::anyhow!("Mouse Gestures runtime unavailable"))?.lock().map_err(|_| anyhow::anyhow!("Mouse Gestures runtime unavailable"))?;
        anyhow::ensure!(self.active.load(Ordering::Acquire) && current() && runtime.gpui.as_ref().is_some_and(|owner| std::ptr::eq(owner.as_ref(), self)) && runtime.directory_revision == snapshot.revision, "Mouse Gestures editor is no longer current");
        let _repair_permit = crate::common::repair_watch::require_path_read(&runtime.path)?;
        let transaction = crate::mouse_gestures::db::gestures_transaction_guard();
        if read_exact(&runtime.path).ok().as_ref() != Some(&snapshot.bytes) {
            self.close_admission();
            self.set_status(Some("Mouse Gestures changed or could not be read; waiting for a valid watcher publication.".into()));
            runtime.apply();
            anyhow::bail!("Mouse Gestures changed; reload before saving");
        }
        anyhow::ensure!(*runtime.db.lock().map_err(|_| anyhow::anyhow!("Mouse Gestures database unavailable"))? == snapshot.database, "Mouse Gestures changed; reload before saving");
        let mut database = snapshot.database.clone();
        apply_edit(&mut database, edit)?;
        let bytes = serde_json::to_vec_pretty(&database)?;
        let was_ready = self.ready.load(Ordering::Acquire);
        self.close_admission();
        let result = (|| -> anyhow::Result<()> {
            if snapshot.bytes.is_some() {
                crate::common::atomic_file::save_atomic_if(&runtime.path, &bytes, || read_exact(&runtime.path).ok().as_ref() == Some(&snapshot.bytes) && self.active.load(Ordering::Acquire) && current())?;
            } else {
                let parent = runtime.path.parent().ok_or_else(|| anyhow::anyhow!("Mouse Gestures directory unavailable"))?;
                let mut staged = tempfile::NamedTempFile::new_in(parent)?;
                staged.write_all(&bytes)?;
                staged.flush()?;
                staged.as_file().sync_all()?;
                anyhow::ensure!(current() && self.active.load(Ordering::Acquire), "Mouse Gestures editor is no longer current");
                staged.persist_noclobber(&runtime.path).map_err(|error| error.error)?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            if was_ready && self.active.load(Ordering::Acquire) && read_exact(&runtime.path).ok().as_ref() == Some(&snapshot.bytes) {
                self.open_admission();
            } else if self.active.load(Ordering::Acquire) {
                self.set_status(Some("Mouse Gestures changed or could not be read; waiting for a valid watcher publication.".into()));
                runtime.apply();
            }
            return Err(error);
        }
        *runtime.db.lock().map_err(|_| anyhow::anyhow!("Mouse Gestures database unavailable"))? = database.clone();
        runtime.bump_directory_revision();
        if was_ready && self.active.load(Ordering::Acquire) { self.open_admission(); }
        let revision = runtime.directory_revision;
        drop(transaction);
        runtime.apply();
        (self.wake)();
        Ok(GestureEditorSnapshot { database, revision, enabled: runtime.settings.enabled, practice_mode: runtime.settings.practice_mode, generation: self.editor_revision.load(Ordering::Acquire), bytes: Some(bytes) })
    }

    pub fn editor_snapshot_is_current(&self, snapshot: &GestureEditorSnapshot) -> bool {
        self.active.load(Ordering::Acquire) && self.editor_revision.load(Ordering::Acquire) == snapshot.generation
    }
}

#[cfg(test)]
mod tests {
    use super::{apply_edit, GestureEdit};
    use crate::mouse_gestures::db::{BindingEntry, BindingKind, GestureDb};
    use crate::mouse_gestures::engine::DirMode;

    #[test]
    fn checked_enabled_transition_preserves_rows_and_refuses_stale_or_ambiguous_enable() {
        let mut db = GestureDb::default();
        apply_edit(&mut db, GestureEdit::Add).unwrap();
        db.gestures[0].tokens = "LR".into();
        db.gestures[0].stroke = vec![[1, 2]];
        db.gestures[0].bindings.push(BindingEntry { label: "query".into(), kind: BindingKind::SetQuery, action: "hello".into(), args: Some("keep".into()), enabled: false });
        let original = db.gestures[0].clone();
        let mut conflicting = original.clone();
        conflicting.label = "other".into();
        db.gestures.push(conflicting.clone());
        apply_edit(&mut db, GestureEdit::Enabled { index: 0, expected: original.clone(), enabled: false }).unwrap();
        let mut disabled = original.clone();
        disabled.enabled = false;
        assert_eq!(db.gestures, vec![disabled.clone(), conflicting.clone()]);
        assert!(apply_edit(&mut db, GestureEdit::Enabled { index: 0, expected: original.clone(), enabled: true }).is_err());
        assert!(apply_edit(&mut db, GestureEdit::Enabled { index: 0, expected: disabled.clone(), enabled: true }).is_err());
        assert_eq!(db.gestures, vec![disabled.clone(), conflicting.clone()]);
        apply_edit(&mut db, GestureEdit::Enabled { index: 1, expected: conflicting.clone(), enabled: false }).unwrap();
        apply_edit(&mut db, GestureEdit::Enabled { index: 0, expected: disabled, enabled: true }).unwrap();
        assert_eq!(db.gestures[0], original);
        conflicting.enabled = false;
        assert_eq!(db.gestures[1], conflicting);

        db.gestures[1].dir_mode = DirMode::Eight;
        let eight = db.gestures[1].clone();
        assert!(apply_edit(&mut db, GestureEdit::Enabled { index: 1, expected: eight.clone(), enabled: true }).is_err());
        assert_eq!(db.gestures[1], eight);
    }

    #[test]
    fn add_rename_and_supported_binding_preserve_other_records() {
        let mut db = GestureDb::default();
        apply_edit(&mut db, GestureEdit::Add).unwrap();
        let original = db.gestures[0].clone();
        assert_eq!(original.label, "Gesture 1");
        assert!(original.enabled && original.tokens.is_empty());
        db.gestures[0].bindings.push(BindingEntry { label: "other".into(), kind: BindingKind::Execute, action: "x".into(), args: None, enabled: true });
        let expected = db.gestures[0].clone();
        assert!(apply_edit(&mut db, GestureEdit::SetQuery { index: 0, expected: expected.clone(), binding: Some(0), label: "x".into(), query: "q".into() }).is_err());
        apply_edit(&mut db, GestureEdit::SetQuery { index: 0, expected, binding: None, label: "query".into(), query: "hello".into() }).unwrap();
        assert_eq!(db.gestures[0].bindings[0].kind, BindingKind::Execute);
        assert_eq!(db.gestures[0].bindings[1].kind, BindingKind::SetQuery);
    }

    #[test]
    fn edits_preserve_unowned_binding_and_unchanged_stroke() {
        let mut db = GestureDb::default();
        apply_edit(&mut db, GestureEdit::Add).unwrap();
        db.gestures[0].tokens = "LR".into();
        db.gestures[0].stroke = vec![[1, 2]];
        db.gestures[0].bindings.push(BindingEntry { label: "old".into(), kind: BindingKind::SetQuery, action: "old".into(), args: Some("kept".into()), enabled: false });
        let row = db.gestures[0].clone();
        apply_edit(&mut db, GestureEdit::Tokens { index: 0, expected: row, tokens: "lr".into() }).unwrap();
        assert_eq!(db.gestures[0].stroke, vec![[1, 2]]);
        let row = db.gestures[0].clone();
        assert!(apply_edit(&mut db, GestureEdit::Rename { index: 0, expected: row.clone(), label: "  ".into() }).is_err());
        assert!(apply_edit(&mut db, GestureEdit::SetQuery { index: 0, expected: row.clone(), binding: Some(0), label: "new".into(), query: "a\nb".into() }).is_err());
        apply_edit(&mut db, GestureEdit::SetQuery { index: 0, expected: row, binding: Some(0), label: "new".into(), query: "value".into() }).unwrap();
        assert!(!db.gestures[0].bindings[0].enabled);
        assert_eq!(db.gestures[0].bindings[0].args.as_deref(), Some("kept"));
    }
}
