//! Saved Regex presets and their serialized canonical store.
use super::regex_tester::RegexWorkspace;
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::regex_tester::{
    PresetContent, PresetId, PresetInput, PresetStore, RegexPreset,
};
use std::path::PathBuf;

pub(super) const WAIT_RESTART: &str = "Wait for Regex Tester presets to finish before restarting.";
pub(super) const WAIT_QUIT: &str = "Wait for Regex Tester presets to finish before quitting.";
pub(super) const WAIT_CLOSE: &str = "Wait for Regex Tester presets to finish before closing.";
fn is_wait(message: &str) -> bool {
    matches!(message, WAIT_RESTART | WAIT_QUIT | WAIT_CLOSE)
}

#[derive(Clone)]
struct Confirmation {
    generation: u64,
    token: u64,
    mount: utility_mount::MountReceipt,
    reviewed: Arc<RegexPreset>,
}

pub(super) struct PresetLane {
    path: Option<PathBuf>,
    store: Arc<Mutex<Option<PresetStore>>>,
    busy: bool,
    initialized: bool,
    generation: u64,
    entries: Arc<Vec<RegexPreset>>,
    writable: bool,
    diagnostic: Option<String>,
    expanded: bool,
    selected: Option<PresetId>,
    pub(super) name: String,
    include_sample: bool,
    include_replacement: bool,
    confirmation: Option<Confirmation>,
    confirmation_revision: u64,
}
impl PresetLane {
    pub(super) fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            store: Arc::new(Mutex::new(None)),
            busy: false,
            initialized: false,
            generation: 0,
            entries: Arc::new(Vec::new()),
            writable: false,
            diagnostic: None,
            expanded: false,
            selected: None,
            name: String::new(),
            include_sample: false,
            include_replacement: false,
            confirmation: None,
            confirmation_revision: 0,
        }
    }
    pub(super) fn busy(&self) -> bool {
        self.busy
    }
    pub(super) fn has_confirmation(&self) -> bool {
        self.confirmation.is_some()
    }
    pub(super) fn can_edit_name(&self) -> bool {
        self.expanded && !self.busy && self.confirmation.is_none()
    }
    pub(super) fn blocks_host(&self) -> bool {
        self.busy || self.confirmation.is_some()
    }
    pub(super) fn cancel_confirmation(&mut self) -> bool {
        let removed = self.confirmation.take().is_some();
        if removed {
            self.confirmation_revision = self.confirmation_revision.wrapping_add(1);
        }
        removed
    }
}

enum Operation {
    Inspect,
    Reload,
    Create(PresetInput),
    Rename(PresetId, String),
    Update(PresetId, PresetContent),
    Delete(RegexPreset),
}
fn perform(
    store: &mut PresetStore,
    operation: Operation,
) -> (
    Vec<RegexPreset>,
    bool,
    Option<String>,
    bool,
    Option<PresetId>,
) {
    let reloading = matches!(&operation, Operation::Reload);
    let result = match operation {
        Operation::Inspect => Ok(None),
        Operation::Reload => store.reload().map(|()| None),
        Operation::Create(input) => store.create(input).map(Some),
        Operation::Rename(id, name) => store.rename(id, &name).map(|()| None),
        Operation::Update(id, content) => store.update_content(id, content).map(|()| None),
        Operation::Delete(reviewed) => store.delete_checked(&reviewed).map(|()| None),
    };
    let reloaded = reloading && result.is_ok();
    let (created, error) = match result {
        Ok(created) => (created, None),
        Err(error) => (None, Some(error.to_string())),
    };
    let diagnostic = store
        .diagnostic()
        .map(|diagnostic| diagnostic.message.clone())
        .or(error);
    (
        store.entries().to_vec(),
        store.is_writable(),
        diagnostic,
        reloaded,
        created,
    )
}

impl RegexWorkspace {
    fn preset_operation(&mut self, operation: Operation, cx: &mut Context<Self>) {
        if self.presets.busy || !self.open || self.rehosting || self.admission.is_fenced() {
            return;
        }
        let Some(path) = self.presets.path.clone() else {
            return;
        };
        let Some(permit) = self.admission.admit() else {
            return;
        };
        self.presets.busy = true;
        let store = self.presets.store.clone();
        let task = cx.background_executor().spawn(async move {
            let _permit = permit;
            let mut store = store
                .lock()
                .map_err(|_| "Preset store is unavailable.".to_owned())?;
            let store = store.get_or_insert_with(|| PresetStore::open(path));
            Ok::<_, String>(perform(store, operation))
        });
        let owner = cx.entity().downgrade();
        cx.spawn(async move |_, cx| {
            let result = task.await;
            let _ = owner.update(cx, |view, cx| {
                view.presets.busy = false;
                match result {
                    Ok((entries, writable, diagnostic, reloaded, created)) => {
                        let changed = !view.presets.initialized
                            || reloaded
                            || view.presets.entries.as_ref() != &entries;
                        if changed {
                            view.presets.generation = view.presets.generation.wrapping_add(1);
                            view.presets.cancel_confirmation();
                        }
                        view.presets.initialized = true;
                        view.presets.entries = Arc::new(entries);
                        view.presets.writable = writable;
                        view.presets.diagnostic = diagnostic;
                        if let Some(id) = created {
                            view.presets.selected = Some(id);
                        }
                        if view.presets.selected.is_some_and(|id| {
                            !view.presets.entries.iter().any(|entry| entry.id == id)
                        }) {
                            view.presets.selected = None;
                        }
                    }
                    Err(error) => view.presets.diagnostic = Some(error),
                }
                let _ = view.root.update(cx, |root, cx| {
                    if root.restart_status.as_deref().is_some_and(is_wait) {
                        root.restart_status = None;
                        cx.notify();
                    }
                });
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }
    fn preset_content(&self) -> PresetContent {
        PresetContent {
            pattern: self.session.draft.pattern.clone(),
            flags: self.session.draft.flags,
            sample_text: self
                .presets
                .include_sample
                .then(|| self.session.draft.test_text.clone()),
            replacement: self
                .presets
                .include_replacement
                .then(|| self.session.draft.replacement.clone()),
            test_cases: self.session.draft.test_cases.clone(),
        }
    }
    fn load_preset(&mut self, preset: &RegexPreset, window: &mut Window, cx: &mut Context<Self>) {
        if self.session.draft.pattern != preset.pattern {
            self.clipboard_epochs.pattern.fetch_add(1, Ordering::AcqRel);
        }
        self.clipboard_epochs.main_changed(true);
        if preset.sample_text.is_some() {
            self.clipboard_epochs.source.fetch_add(1, Ordering::AcqRel);
        }
        if preset.replacement.is_some() {
            self.clipboard_epochs
                .replacement
                .fetch_add(1, Ordering::AcqRel);
        }
        self.session.draft.pattern = preset.pattern.clone();
        self.session.draft.flags = preset.flags;
        if let Some(text) = &preset.sample_text {
            self.session.draft.test_text = text.clone();
        }
        if let Some(text) = &preset.replacement {
            self.session.draft.replacement = text.clone();
        }
        self.session.draft.test_cases = preset.test_cases.clone();
        self.mount.case_receipt = self.mount.case_owner.attach();
        self.mount.cases.clear();
        self.session
            .mark_test_cases_changed(std::time::Instant::now());
        self.mount.pattern.focus_handle(cx).focus(window, cx);
        self.finish_prepared_change(cx);
        self.mount.pattern.update(cx, |input, cx| {
            input.set_value(preset.pattern.clone(), window, cx)
        });
        if let Some(text) = &preset.sample_text {
            self.mount
                .text
                .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
        }
        if let Some(text) = &preset.replacement {
            self.mount
                .replacement
                .update(cx, |input, cx| input.set_value(text.clone(), window, cx));
        }
        super::regex_cases::sync_editors(self, window, cx);
    }
}

fn base_current(
    view: &RegexWorkspace,
    receipt: &utility_mount::MountReceipt,
    generation: u64,
) -> bool {
    receipt.is_current()
        && view.open
        && !view.rehosting
        && !view.admission.is_fenced()
        && view.presets.generation == generation
        && !view.presets.busy
}
fn current(view: &RegexWorkspace, receipt: &utility_mount::MountReceipt, generation: u64) -> bool {
    base_current(view, receipt, generation) && view.presets.expanded
}
pub(super) fn render(view: &mut RegexWorkspace, cx: &mut Context<RegexWorkspace>) -> AnyElement {
    let receipt = view.mount.receipt.clone();
    let generation = view.presets.generation;
    let enabled = view.open && !view.rehosting && !view.admission.is_fenced() && !view.presets.busy;
    let expanded = view.presets.expanded;
    let toggle = receipt.clone();
    let mut panel = div().id("regex-presets").flex().flex_col().gap_2().child(
        Button::new("regex-presets-disclosure")
            .label(if expanded {
                "▾ Presets"
            } else {
                "▸ Presets"
            })
            .disabled(!enabled || view.presets.has_confirmation())
            .on_click(cx.listener(move |view, _, _, cx| {
                if base_current(view, &toggle, generation) && !view.presets.has_confirmation() {
                    view.presets.expanded = !view.presets.expanded;
                    view.presets.generation = view.presets.generation.wrapping_add(1);
                    if view.presets.expanded && !view.presets.initialized {
                        view.preset_operation(Operation::Inspect, cx);
                    }
                    cx.notify();
                }
            })),
    );
    if !expanded {
        return panel.into_any_element();
    }
    let reload = receipt.clone();
    panel = panel.child(
        Button::new("regex-presets-reload")
            .label("Reload presets")
            .disabled(!enabled || view.presets.path.is_none() || view.presets.has_confirmation())
            .on_click(cx.listener(move |view, _, _, cx| {
                if current(view, &reload, generation) && !view.presets.has_confirmation() {
                    view.preset_operation(Operation::Reload, cx);
                }
            })),
    );
    if let Some(error) = &view.presets.diagnostic {
        panel = panel.child(Label::new(format!("Presets: {error}")).id("regex-presets-error"));
    }
    if view.presets.initialized && !view.presets.writable {
        panel = panel.child(
            Label::new("Preset writes are blocked. Repair the file and reload.")
                .id("regex-presets-blocked"),
        );
    }
    let mut rows = div()
        .id("regex-preset-list")
        .flex()
        .flex_col()
        .max_h(px(180.0))
        .overflow_y_scroll();
    for preset in view.presets.entries.iter() {
        let id = preset.id;
        let row_receipt = receipt.clone();
        rows = rows.child(
            Button::new(SharedString::from(format!(
                "regex-preset-{generation}-{}",
                id.value()
            )))
            .label(preset.name.clone())
            .selected(view.presets.selected == Some(id))
            .disabled(!enabled || view.presets.confirmation.is_some())
            .on_click(cx.listener(move |view, _, window, cx| {
                if current(view, &row_receipt, generation) && !view.presets.has_confirmation() {
                    let Some(name) = view
                        .presets
                        .entries
                        .iter()
                        .find(|item| item.id == id)
                        .map(|item| item.name.clone())
                    else {
                        return;
                    };
                    view.presets.selected = Some(id);
                    view.presets.cancel_confirmation();
                    view.presets.name = name.clone();
                    view.mount
                        .preset_name
                        .update(cx, |input, cx| input.set_value(name, window, cx));
                    cx.notify();
                }
            })),
        );
    }
    panel = panel.child(rows).child(
        Input::new(&view.mount.preset_name)
            .disabled(!enabled || view.presets.confirmation.is_some())
            .aria_label("Preset name"),
    );
    for (key, label, selected) in [
        ("sample", "Include sample text", view.presets.include_sample),
        (
            "replacement",
            "Include replacement",
            view.presets.include_replacement,
        ),
    ] {
        let receipt = receipt.clone();
        let owner = cx.entity().downgrade();
        panel = panel.child(
            Checkbox::new(SharedString::from(format!("regex-preset-{key}")))
                .checked(selected)
                .disabled(!enabled || view.presets.confirmation.is_some())
                .accessibility_label(label)
                .on_change(move |state, _, _, app| {
                    let _ = owner.update(app, |view, cx| {
                        if current(view, &receipt, generation)
                            && view.presets.confirmation.is_none()
                        {
                            if key == "sample" {
                                view.presets.include_sample = state == CheckboxState::Checked;
                            } else {
                                view.presets.include_replacement = state == CheckboxState::Checked;
                            }
                            cx.notify();
                        }
                    });
                })
                .child(
                    div()
                        .flex()
                        .gap_1()
                        .child(
                            CheckboxIndicator::new()
                                .checked(selected)
                                .size(px(16.0))
                                .border_1()
                                .child(if selected { "✓" } else { "" }),
                        )
                        .child(
                            Label::new(label)
                                .id(SharedString::from(format!("regex-preset-label-{key}"))),
                        ),
                ),
        );
    }
    let selected = view
        .presets
        .selected
        .filter(|id| view.presets.entries.iter().any(|entry| entry.id == *id));
    let writable = enabled
        && view.presets.initialized
        && view.presets.writable
        && view.presets.confirmation.is_none();
    let save = receipt.clone();
    panel = panel.child(
        Button::new("regex-preset-save")
            .label("Save new preset")
            .disabled(!writable)
            .on_click(cx.listener(move |view, _, _, cx| {
                if current(view, &save, generation)
                    && view.presets.writable
                    && view.presets.confirmation.is_none()
                {
                    let content = view.preset_content();
                    view.preset_operation(
                        Operation::Create(PresetInput {
                            name: view.presets.name.clone(),
                            pattern: content.pattern,
                            flags: content.flags,
                            sample_text: content.sample_text,
                            replacement: content.replacement,
                            test_cases: content.test_cases,
                        }),
                        cx,
                    );
                }
            })),
    );
    if let Some(id) = selected {
        let load = receipt.clone();
        panel = panel.child(
            Button::new("regex-preset-load")
                .label("Load selected preset")
                .disabled(!enabled || view.presets.confirmation.is_some())
                .on_click(cx.listener(move |view, _, window, cx| {
                    if current(view, &load, generation)
                        && view.presets.selected == Some(id)
                        && view.presets.confirmation.is_none()
                    {
                        let Some(preset) = view
                            .presets
                            .entries
                            .iter()
                            .find(|entry| entry.id == id)
                            .cloned()
                        else {
                            return;
                        };
                        view.load_preset(&preset, window, cx);
                    }
                })),
        );
        let rename = receipt.clone();
        panel = panel.child(
            Button::new("regex-preset-rename")
                .label("Rename selected preset")
                .disabled(!writable)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if current(view, &rename, generation)
                        && view.presets.writable
                        && view.presets.confirmation.is_none()
                        && view.presets.selected == Some(id)
                    {
                        view.preset_operation(Operation::Rename(id, view.presets.name.clone()), cx);
                    }
                })),
        );
        let update = receipt.clone();
        panel = panel.child(
            Button::new("regex-preset-update")
                .label("Update selected preset")
                .disabled(!writable)
                .on_click(cx.listener(move |view, _, _, cx| {
                    if current(view, &update, generation)
                        && view.presets.writable
                        && view.presets.confirmation.is_none()
                        && view.presets.selected == Some(id)
                    {
                        view.preset_operation(Operation::Update(id, view.preset_content()), cx);
                    }
                })),
        );
        let delete = receipt.clone();
        panel = panel.child(
            Button::new("regex-preset-delete")
                .label("Delete selected preset…")
                .disabled(!writable)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if current(view, &delete, generation)
                        && view.presets.writable
                        && view.presets.confirmation.is_none()
                        && view.presets.selected == Some(id)
                    {
                        let Some(reviewed) = view
                            .presets
                            .entries
                            .iter()
                            .find(|entry| entry.id == id)
                            .cloned()
                        else {
                            return;
                        };
                        view.presets.confirmation_revision =
                            view.presets.confirmation_revision.wrapping_add(1);
                        view.presets.confirmation = Some(Confirmation {
                            generation,
                            token: view.presets.confirmation_revision,
                            mount: delete.clone(),
                            reviewed: Arc::new(reviewed),
                        });
                        view.mount.preset_confirmation_focus.focus(window, cx);
                        cx.notify();
                    }
                })),
        );
    }
    if let Some(confirmation) = view.presets.confirmation.clone() {
        let confirm = confirmation.clone();
        let cancel = confirmation.clone();
        panel = panel.child(
            div()
                .id("regex-preset-confirmation-scope")
                .track_focus(&view.mount.preset_confirmation_focus)
                .flex()
                .flex_col()
                .gap_1()
                .child(
                    Label::new(format!("Delete {:?}?", confirmation.reviewed.name))
                        .id("regex-preset-confirmation"),
                )
                .child(
                    Button::new("regex-preset-confirm-delete")
                        .label("Confirm delete")
                        .disabled(!enabled || !view.presets.writable)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if current(view, &confirm.mount, confirm.generation)
                                && view.presets.writable
                                && view.presets.confirmation.as_ref().is_some_and(|item| {
                                    item.token == confirm.token
                                        && item.generation == confirm.generation
                                        && item.reviewed == confirm.reviewed
                                })
                                && view.presets.selected == Some(confirm.reviewed.id)
                            {
                                view.focus_anchor(window, cx);
                                view.presets.cancel_confirmation();
                                view.preset_operation(
                                    Operation::Delete((*confirm.reviewed).clone()),
                                    cx,
                                );
                            }
                        })),
                )
                .child(
                    Button::new("regex-preset-cancel-delete")
                        .label("Cancel")
                        .disabled(!enabled)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if current(view, &cancel.mount, cancel.generation)
                                && view.presets.confirmation.as_ref().is_some_and(|item| {
                                    item.token == cancel.token
                                        && item.generation == cancel.generation
                                        && item.reviewed == cancel.reviewed
                                })
                            {
                                view.focus_anchor(window, cx);
                                view.presets.cancel_confirmation();
                                cx.notify();
                            }
                        })),
                ),
        );
    }
    panel.into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Operation, perform};
    use multi_launcher::regex_tester::{PresetInput, PresetStore, RegexFlags, RegexTestCases};

    #[test]
    fn failed_reload_preserves_published_rows_and_corrupt_bytes_until_repaired() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("regex_presets.json");
        let mut store = PresetStore::open(path.clone());
        let (rows, writable, diagnostic, _, _) = perform(
            &mut store,
            Operation::Create(PresetInput {
                name: "sample".into(),
                pattern: "abc".into(),
                flags: RegexFlags::default(),
                sample_text: Some(String::new()),
                replacement: None,
                test_cases: RegexTestCases::default(),
            }),
        );
        assert!(writable && diagnostic.is_none());
        let good_bytes = std::fs::read(&path).unwrap();
        let corrupt = b"{broken";
        std::fs::write(&path, corrupt).unwrap();
        let (after, writable, diagnostic, reloaded, _) = perform(&mut store, Operation::Reload);
        assert_eq!(after, rows);
        assert!(!writable && diagnostic.is_some() && !reloaded);
        assert_eq!(std::fs::read(&path).unwrap(), corrupt);
        std::fs::write(&path, good_bytes).unwrap();
        let (restored, writable, diagnostic, reloaded, _) = perform(&mut store, Operation::Reload);
        assert_eq!(restored, rows);
        assert!(writable && diagnostic.is_none() && reloaded);
        assert_eq!(restored[0].sample_text.as_deref(), Some(""));
        assert_eq!(restored[0].replacement, None);
    }
}
