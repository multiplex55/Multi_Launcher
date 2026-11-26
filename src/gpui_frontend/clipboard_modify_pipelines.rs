//! Retained, checked Saved Pipeline management.
use super::*;
use multi_launcher::clipboard_modify::{
    model::{SavedPipeline, StageSpec},
    store::{CheckedCatalogEdit, CheckedCatalogRead, CheckedCatalogSave},
};
use std::collections::HashSet;

struct PipelineRow {
    token: u64,
    id: Entity<InputState>,
    label: Entity<InputState>,
    aliases: Entity<InputState>,
    stages: Vec<StageEditor>,
    subscriptions: Vec<Subscription>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftSnapshot(Vec<(String, String, String, Vec<StageSpec>)>);
#[derive(Clone, Copy)]
pub(super) struct DeleteReview {
    token: u64,
    revision: u64,
    serial: u64,
}
pub(super) struct PipelineManagement {
    rows: Vec<PipelineRow>,
    baseline: Option<Vec<SavedPipeline>>,
    revision: u64,
    next_row: u64,
    next_stage: u64,
    dirty: bool,
    duplicate: HashSet<u64>,
    pub review: Option<DeleteReview>,
    review_serial: u64,
    picker: Option<(u64, u64)>,
    focus: FocusHandle,
    cancel_focus: FocusHandle,
    feedback: Option<String>,
}
impl PipelineManagement {
    pub fn new(cx: &mut Context<ClipboardModifyView>) -> Self {
        Self {
            rows: Vec::new(),
            baseline: None,
            revision: 0,
            next_row: 0,
            next_stage: 0,
            dirty: false,
            duplicate: HashSet::new(),
            review: None,
            review_serial: 0,
            picker: None,
            focus: cx.focus_handle(),
            cancel_focus: cx.focus_handle(),
            feedback: None,
        }
    }
    pub fn clear(&mut self) {
        self.rows.clear();
        self.baseline = None;
        self.dirty = false;
        self.duplicate.clear();
        self.review = None;
        self.picker = None;
        self.feedback = None;
        self.revision = self.revision.wrapping_add(1);
    }
    pub fn changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty = true;
        self.feedback = None;
    }
    pub fn stage(&self, row: u64, stage: u64) -> Option<&StageEditor> {
        self.rows
            .iter()
            .find(|item| item.token == row)?
            .stages
            .iter()
            .find(|item| item.id == stage)
    }
    pub fn stage_mut(&mut self, row: u64, stage: u64) -> Option<&mut StageEditor> {
        self.rows
            .iter_mut()
            .find(|item| item.token == row)?
            .stages
            .iter_mut()
            .find(|item| item.id == stage)
    }
    fn snapshot(&self, cx: &App) -> Vec<SavedPipeline> {
        self.rows
            .iter()
            .map(|row| SavedPipeline {
                id: row.id.read(cx).value().to_string(),
                label: row.label.read(cx).value().to_string(),
                aliases: row
                    .aliases
                    .read(cx)
                    .value()
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(str::to_owned)
                    .collect(),
                stages: row.stages.iter().map(|stage| stage.spec.clone()).collect(),
            })
            .collect()
    }
    pub fn restart_draft(&self, cx: &App) -> Option<DraftSnapshot> {
        self.dirty.then(|| {
            DraftSnapshot(
                self.rows
                    .iter()
                    .map(|row| {
                        (
                            row.id.read(cx).value().to_string(),
                            row.label.read(cx).value().to_string(),
                            row.aliases.read(cx).value().to_string(),
                            row.stages.iter().map(|stage| stage.spec.clone()).collect(),
                        )
                    })
                    .collect(),
            )
        })
    }
    pub fn set_readonly(&self, readonly: bool, cx: &mut Context<ClipboardModifyView>) {
        for row in &self.rows {
            for input in [&row.id, &row.label, &row.aliases] {
                input.update(cx, |input, cx| input.set_readonly(readonly, cx));
            }
            for stage in &row.stages {
                for input in [
                    &stage.prefix,
                    &stage.suffix,
                    &stage.name,
                    &stage.language,
                    &stage.separator,
                ] {
                    input.update(cx, |input, cx| input.set_readonly(readonly, cx));
                }
            }
        }
    }
    pub fn input_focused(&self, window: &Window, cx: &App) -> bool {
        self.rows.iter().any(|row| {
            [&row.id, &row.label, &row.aliases]
                .iter()
                .any(|input| input.focus_handle(cx).is_focused(window))
                || row.stages.iter().any(|stage| {
                    [
                        &stage.prefix,
                        &stage.suffix,
                        &stage.name,
                        &stage.language,
                        &stage.separator,
                    ]
                    .iter()
                    .any(|input| input.focus_handle(cx).is_focused(window))
                })
        })
    }
    pub fn focus_review(&self, window: &mut Window, cx: &mut Context<ClipboardModifyView>) {
        self.cancel_focus.focus(window, cx);
    }
    pub fn review_cancel_is_focused(&self, window: &Window) -> bool { self.cancel_focus.is_focused(window) }
}

fn unique_id(base: &str, catalog: &ClipboardModifierCatalog, drafts: &[SavedPipeline]) -> String {
    let normalize = multi_launcher::clipboard_modify::catalog::normalize_name;
    let root = format!("{}-copy", normalize(base));
    let names: HashSet<String> = catalog
        .templates
        .iter()
        .flat_map(|item| std::iter::once(&item.id).chain(&item.aliases))
        .chain(
            catalog
                .pipelines
                .iter()
                .flat_map(|item| std::iter::once(&item.id).chain(&item.aliases)),
        )
        .chain(
            drafts
                .iter()
                .flat_map(|item| std::iter::once(&item.id).chain(&item.aliases)),
        )
        .map(|value| normalize(value))
        .collect();
    if !names.contains(&root) {
        return root;
    }
    for n in 2u64.. {
        let candidate = format!("{root}-{n}");
        if !names.contains(&candidate) {
            return candidate;
        }
    }
    unreachable!()
}

fn valid(view: &ClipboardModifyView, cx: &App) -> Result<(), String> {
    valid_candidate(view, &view.pipelines.snapshot(cx))
}
fn valid_candidate(view: &ClipboardModifyView, draft: &[SavedPipeline]) -> Result<(), String> {
    let Some(catalog) = view.catalog.as_ref() else {
        return Err("Catalog unavailable.".into());
    };
    let model = multi_launcher::clipboard_modify::config::VersionedClipboardModifiersFile {
        schema_version: multi_launcher::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
        templates: catalog.templates.clone(),
        pipelines: draft.to_vec(),
    };
    multi_launcher::clipboard_modify::config::validate_model(&model)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
fn can_auto_save(unapproved_duplicates: bool) -> bool {
    !unapproved_duplicates
}
fn remove_reviewed_candidate<T>(tokens: &[u64], draft: &mut Vec<T>, token: u64) -> bool {
    if tokens.len() != draft.len() {
        return false;
    }
    let Some(index) = tokens.iter().position(|current| *current == token) else {
        return false;
    };
    draft.remove(index);
    true
}
fn changed(view: &mut ClipboardModifyView, cx: &mut Context<ClipboardModifyView>) {
    view.management.authority.fetch_add(1, Ordering::AcqRel);
    view.pipelines.changed();
    view.preview.cancel_and_invalidate();
    cx.notify();
}

fn row_subscriptions(
    row: &PipelineRow,
    receipt: utility_mount::MountReceipt,
    section: u64,
    cx: &mut Context<ClipboardModifyView>,
) -> Vec<Subscription> {
    let token = row.token;
    [&row.id, &row.label, &row.aliases]
        .into_iter()
        .map(|input| {
            let receipt = receipt.clone();
            cx.subscribe(
                input,
                move |view: &mut ClipboardModifyView,
                      input: Entity<InputState>,
                      event: &InputEvent,
                      cx| {
                    if !matches!(event, InputEvent::Change)
                        || !receipt.is_current()
                        || !receipt.matches(&view.mount)
                        || !view.open
                        || view.section != Section::ManagePipelines
                        || view.section_revision != section
                    {
                        return;
                    }
                    if !view.pipelines.rows.iter().any(|row| {
                        row.token == token
                            && [
                                row.id.entity_id(),
                                row.label.entity_id(),
                                row.aliases.entity_id(),
                            ]
                            .contains(&input.entity_id())
                    }) {
                        return;
                    }
                    view.management.authority.fetch_add(1, Ordering::AcqRel);
                    view.pipelines.changed();
                    view.preview.cancel_and_invalidate();
                    cx.notify();
                },
            )
        })
        .collect()
}
fn make_row(
    view: &mut ClipboardModifyView,
    value: &SavedPipeline,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) -> PipelineRow {
    view.pipelines.next_row = view.pipelines.next_row.wrapping_add(1);
    let token = view.pipelines.next_row;
    let id = cx.new(|cx| InputState::new(window, cx));
    let label = cx.new(|cx| InputState::new(window, cx));
    let aliases = cx.new(|cx| InputState::new(window, cx));
    id.update(cx, |input, cx| {
        input.set_value(value.id.clone(), window, cx)
    });
    label.update(cx, |input, cx| {
        input.set_value(value.label.clone(), window, cx)
    });
    aliases.update(cx, |input, cx| {
        input.set_value(value.aliases.join(", "), window, cx)
    });
    let mut stages = Vec::new();
    for spec in &value.stages {
        view.pipelines.next_stage = view.pipelines.next_stage.wrapping_add(1);
        stages.push(view.build_stage_for(
            StageOwner::Pipeline(token),
            view.pipelines.next_stage,
            spec.clone(),
            window,
            cx,
        ));
    }
    let mut row = PipelineRow {
        token,
        id,
        label,
        aliases,
        stages,
        subscriptions: Vec::new(),
    };
    row.subscriptions = row_subscriptions(&row, view.mount.clone(), view.section_revision, cx);
    row
}
pub(super) fn ensure(
    view: &mut ClipboardModifyView,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) {
    if view.pipelines.baseline.is_none() {
        let Some(catalog) = view.catalog.as_ref() else {
            return;
        };
        let pipelines = catalog.pipelines.clone();
        view.pipelines.baseline = Some(pipelines.clone());
        for pipeline in &pipelines {
            let row = make_row(view, pipeline, window, cx);
            view.pipelines.rows.push(row);
        }
    } else {
        for row in &mut view.pipelines.rows {
            row.subscriptions =
                row_subscriptions(row, view.mount.clone(), view.section_revision, cx);
            for stage in &mut row.stages {
                stage._subscriptions = ClipboardModifyView::subscribe_stage(
                    StageOwner::Pipeline(row.token),
                    stage.id,
                    [
                        &stage.prefix,
                        &stage.suffix,
                        &stage.name,
                        &stage.language,
                        &stage.separator,
                    ],
                    view.mount.clone(),
                    view.section_revision,
                    cx,
                );
            }
        }
    }
}

pub(super) fn remount(view: &mut ClipboardModifyView, window: &mut Window, cx: &mut Context<ClipboardModifyView>) {
    let rows: Vec<_> = view.pipelines.rows.iter().map(|row| (
        row.token, row.id.read(cx).value().to_string(), row.label.read(cx).value().to_string(),
        row.aliases.read(cx).value().to_string(),
        row.stages.iter().map(|stage| (stage.id, stage.spec.clone())).collect::<Vec<_>>()
    )).collect();
    view.pipelines.focus = cx.focus_handle();
    view.pipelines.cancel_focus = cx.focus_handle();
    view.pipelines.rows = rows.into_iter().map(|(token, id_value, label_value, aliases_value, specs)| {
        let id = cx.new(|cx| InputState::new(window, cx));
        let label = cx.new(|cx| InputState::new(window, cx));
        let aliases = cx.new(|cx| InputState::new(window, cx));
        id.update(cx, |input, cx| input.set_value(id_value, window, cx));
        label.update(cx, |input, cx| input.set_value(label_value, window, cx));
        aliases.update(cx, |input, cx| input.set_value(aliases_value, window, cx));
        let stages = specs.into_iter().map(|(id, spec)| view.build_stage_for(StageOwner::Pipeline(token), id, spec, window, cx)).collect();
        let mut row = PipelineRow { token, id, label, aliases, stages, subscriptions: Vec::new() };
        row.subscriptions = row_subscriptions(&row, view.mount.clone(), view.section_revision, cx);
        row
    }).collect();
}

fn submit(
    view: &mut ClipboardModifyView,
    reload: bool,
    delete: Option<DeleteReview>,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) {
    let exact_review = delete.is_some_and(|expected| {
        view.pipelines.review.is_some_and(|actual| {
            actual.serial == expected.serial
                && actual.token == expected.token
                && actual.revision == expected.revision
        }) && view.pipelines.revision == expected.revision
    });
    if (view.blocked()
        && !(exact_review
            && view.management.pending.is_none()
            && view.effect_pending.is_none()
            && view.review.is_none()))
        || view.section != Section::ManagePipelines
        || !manage::host_current(view, window, cx)
        || manage::launcher_effect_pending(view, cx)
    {
        return;
    }
    let mut draft = view.pipelines.snapshot(cx);
    if let Some(review) = delete {
        let tokens: Vec<_> = view.pipelines.rows.iter().map(|row| row.token).collect();
        if !remove_reviewed_candidate(&tokens, &mut draft, review.token) {
            return;
        }
    }
    if !reload {
        if let Err(error) = valid_candidate(view, &draft) {
            view.pipelines.feedback = Some(format!("Pipeline validation failed: {error}"));
            cx.notify();
            return;
        }
    }
    let Some(runtime) = view.runtime.as_ref() else {
        return;
    };
    let store = runtime.store.clone();
    let Some(baseline) = view.pipelines.baseline.clone() else {
        return;
    };
    let Some(root) = view.root.upgrade() else {
        return;
    };
    let root = root.read(cx);
    let Some(host_authority) = host::HostAuthority::capture(view, window, cx) else { return; };
    let native = host_authority.native;
    let Some(permit) = root.restart_admission.admit() else {
        return;
    };
    let admission = root.restart_admission.clone();
    let ack_admission = admission.clone();
    let ack_host = host_authority.clone();
    let host = host_authority.host;
    let receipt = view.mount.clone();
    let worker_host = host_authority;
    let authority = view.management.authority.clone();
    let ack_authority = authority.clone();
    let epoch = authority.load(Ordering::Acquire);
    let revision = view.pipelines.revision;
    let section = view.section_revision;
    view.management.serial = view.management.serial.wrapping_add(1);
    let serial = view.management.serial;
    view.management.pending = Some(serial);
    view.pipelines.feedback = None;
    view.sync_editor_readonly(cx);
    enum Outcome {
        Save(
            Result<
                CheckedCatalogSave,
                multi_launcher::clipboard_modify::store::CheckedCatalogError,
            >,
        ),
        Reload(
            Result<
                CheckedCatalogRead,
                multi_launcher::clipboard_modify::store::CheckedCatalogError,
            >,
        ),
    }
    let task = cx.background_executor().spawn(async move {
        let current = || {
            authority.load(Ordering::Acquire) == epoch && worker_host.current()
        };
        if reload {
            Outcome::Reload(store.refresh_checked(&current))
        } else {
            Outcome::Save(store.save_checked(
                CheckedCatalogEdit::Pipelines {
                    baseline: &baseline,
                    draft,
                },
                &current,
            ))
        }
    });
    cx.spawn(async move |weak, cx| {
        let _permit = permit; let result = task.await; let Some(view) = weak.upgrade() else { return; };
        let delivered = update_kit_window_content(host, &view, cx, |view, window, cx| {
            if view.management.pending != Some(serial) { return; } view.management.pending = None;
            if matches!(&result, Outcome::Save(Ok(CheckedCatalogSave::Saved(_))) | Outcome::Reload(Ok(CheckedCatalogRead::Loaded(_)))) { refresh_catalog_search(view, cx); }
            let current = receipt.is_current() && receipt.matches(&view.mount) && view.section == Section::ManagePipelines && view.section_revision == section && view.pipelines.revision == revision;
            match result {
                Outcome::Save(Ok(CheckedCatalogSave::Saved(catalog))) => {
                    view.pipelines.baseline = Some(catalog.pipelines.clone()); view.catalog = Some(catalog);
                    view.sections.template = None; view.sections.pipeline = None; view.preview.cancel_and_invalidate();
                    let decision_current = delete.is_none_or(|expected| view.pipelines.review.is_some_and(|actual| actual.serial == expected.serial && actual.token == expected.token && actual.revision == expected.revision));
                    if current && decision_current {
                        if let Some(expected) = delete {
                            if let Some(index) = view.pipelines.rows.iter().position(|row| row.token == expected.token) {
                                if window.is_window_active() && current_foreground_identity() == Some(native.identity) && (view.pipelines.focus.is_focused(window) || view.pipelines.focus.contains_focused(window, cx)) { view.focus.focus(window, cx); }
                                view.pipelines.rows.remove(index); view.pipelines.duplicate.remove(&expected.token); view.pipelines.review = None; view.pipelines.revision = view.pipelines.revision.wrapping_add(1);
                            }
                        }
                        view.pipelines.dirty = false; view.pipelines.duplicate.clear(); view.pipelines.feedback = Some("Pipelines saved.".into());
                    }
                    else { view.pipelines.dirty = true; view.pipelines.feedback = Some("Saved earlier edits; newer draft changes remain unsaved.".into()); }
                }
                Outcome::Save(Ok(CheckedCatalogSave::Conflict)) => view.pipelines.feedback = Some("Pipelines changed on disk. Draft retained; reload explicitly to discard it.".into()),
                Outcome::Save(Ok(CheckedCatalogSave::Superseded)) => view.pipelines.feedback = Some("Save was superseded. Draft retained.".into()),
                Outcome::Save(Err(_)) => view.pipelines.feedback = Some("Pipelines could not be saved. Draft retained.".into()),
                Outcome::Reload(Ok(CheckedCatalogRead::Loaded(catalog))) => {
                    view.catalog = Some(catalog); view.sections.template = None; view.sections.pipeline = None; view.preview.cancel_and_invalidate();
                    let allowed = current && ack_authority.load(Ordering::Acquire) == epoch && !ack_admission.is_fenced() && manage::host_current(view, window, cx) && window.window_handle().window_id() == host.window_id() && current_foreground_identity() == Some(native.identity) && ack_host.current_for(view, window, cx);
                    if allowed { if view.focus.is_focused(window) || view.focus.contains_focused(window, cx) { view.focus.focus(window, cx); } view.pipelines.clear(); view.section_revision = view.section_revision.wrapping_add(1); ensure(view, window, cx); view.pipelines.feedback = Some("Reloaded pipelines from disk.".into()); }
                    else { view.pipelines.feedback = Some("Catalog refreshed; draft retained because its window or contents changed.".into()); }
                }
                Outcome::Reload(_) => view.pipelines.feedback = Some("Catalog reload failed or was superseded. Draft retained.".into()),
            }
            view.clear_block_wait(cx); cx.notify();
        });
        if delivered.is_err() { let _ = view.update(cx, |view, cx| { if view.management.pending == Some(serial) { view.management.pending = None; view.pipelines.feedback = Some("Operation could not be delivered. Draft retained.".into()); view.clear_block_wait(cx); cx.notify(); } }); }
    }).detach();
    cx.notify();
}

pub(super) fn handle_review_key(
    view: &mut ClipboardModifyView,
    event: &KeyDownEvent,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) -> bool {
    if view.section != Section::ManagePipelines
        || view.pipelines.review.is_none()
        || !(view.pipelines.focus.is_focused(window)
            || view.pipelines.focus.contains_focused(window, cx))
    {
        return false;
    }
    if event.keystroke.modifiers != Modifiers::default() {
        return false;
    }
    let key = event.keystroke.key.as_str();
    if view.management.pending.is_some() && matches!(key, "enter" | "space") {
        window.prevent_default();
        cx.stop_propagation();
        return true;
    }
    if key == "escape" {
        window.prevent_default();
        cx.stop_propagation();
        if !event.is_held && view.management.pending.is_none() {
            view.pipelines.review = None;
            view.pipelines.review_serial = view.pipelines.review_serial.wrapping_add(1);
            view.clear_block_wait(cx);
            view.focus.focus(window, cx);
            cx.notify();
        }
        return true;
    }
    if held_activation(event.is_held, event.keystroke.modifiers, key) {
        window.prevent_default();
        cx.stop_propagation();
        return true;
    }
    false
}

fn action_current(
    view: &ClipboardModifyView,
    receipt: &utility_mount::MountReceipt,
    section: u64,
    revision: u64,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) -> bool {
    !view.blocked()
        && view.section == Section::ManagePipelines
        && view.section_revision == section
        && view.pipelines.revision == revision
        && receipt.is_current()
        && receipt.matches(&view.mount)
        && manage::host_current(view, window, cx)
}

pub(super) fn render(
    view: &mut ClipboardModifyView,
    mut panel: Stateful<Div>,
    cx: &mut Context<ClipboardModifyView>,
) -> Stateful<Div> {
    if view.pipelines.baseline.is_none() {
        return panel.child(Label::new("Loading pipelines…"));
    }
    let receipt = view.mount.clone();
    let section = view.section_revision;
    let revision = view.pipelines.revision;
    let busy = view.management.pending.is_some();
    let launcher_effect = manage::launcher_effect_pending(view, cx);
    if let Some(review) = view.pipelines.review {
        let reviewed = view
            .pipelines
            .rows
            .iter()
            .find(|row| row.token == review.token)
            .map(|row| {
                format!(
                    "Delete pipeline {} ({})?",
                    row.label.read(cx).value(),
                    row.id.read(cx).value()
                )
            })
            .unwrap_or_else(|| "Pipeline is no longer available.".into());
        let cancel = receipt.clone();
        let confirm = receipt.clone();
        return panel.child(
            div()
                .track_focus(&view.pipelines.focus)
                .flex()
                .flex_col()
                .gap_2()
                .child(Label::new(reviewed))
                .children(
                    view.pipelines
                        .feedback
                        .as_ref()
                        .map(|message| Label::new(message.clone())),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "cm-pipeline-delete-cancel-{}",
                        review.serial
                    )))
                    .label("Cancel")
                    .disabled(busy)
                    .track_focus(&view.pipelines.cancel_focus)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if cancel.is_current()
                            && cancel.matches(&view.mount)
                            && view.management.pending.is_none()
                            && view
                                .pipelines
                                .review
                                .is_some_and(|current| current.serial == review.serial)
                            && manage::host_current(view, window, cx)
                        {
                            view.pipelines.review = None;
                            view.clear_block_wait(cx);
                            view.focus.focus(window, cx);
                            cx.notify();
                        }
                    })),
                )
                .child(
                    Button::new(SharedString::from(format!(
                        "cm-pipeline-delete-confirm-{}",
                        review.serial
                    )))
                    .label("Confirm Delete")
                    .disabled(busy)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !confirm.is_current()
                            || view.management.pending.is_some()
                            || !confirm.matches(&view.mount)
                            || !manage::host_current(view, window, cx)
                            || manage::launcher_effect_pending(view, cx)
                            || view.section != Section::ManagePipelines
                            || view.section_revision != section
                            || view.pipelines.revision != review.revision
                            || !view.pipelines.review.is_some_and(|current| {
                                current.serial == review.serial && current.token == review.token
                            })
                        {
                            return;
                        }
                        let Some(index) = view
                            .pipelines
                            .rows
                            .iter()
                            .position(|row| row.token == review.token)
                        else {
                            return;
                        };
                        let other_duplicates = view
                            .pipelines
                            .duplicate
                            .iter()
                            .any(|token| *token != review.token);
                        if can_auto_save(other_duplicates) {
                            submit(view, false, Some(review), window, cx);
                        } else {
                            view.pipelines.review = None;
                            view.focus.focus(window, cx);
                            view.pipelines.rows.remove(index);
                            view.pipelines.duplicate.remove(&review.token);
                            changed(view, cx);
                            view.clear_block_wait(cx);
                            view.pipelines.feedback =
                                Some("Deletion staged; duplicate requires explicit Save.".into());
                            cx.notify();
                        }
                    })),
                ),
        );
    }
    panel = panel.child(Label::new(
        "Manage pipelines · Aliases are comma-separated.",
    ));
    if let Some(feedback) = &view.pipelines.feedback {
        panel = panel.child(Label::new(feedback.clone()));
    }
    let validation = valid(view, cx).err();
    if let Some(error) = &validation {
        panel = panel.child(Label::new(format!("Pipeline validation failed: {error}")));
    }
    if !view.pipelines.duplicate.is_empty() {
        panel = panel.child(Label::new("Duplicate is staged until you explicitly save."));
    }
    let reload_receipt = receipt.clone();
    panel = panel.child(
        Button::new("cm-pipelines-reload")
            .label(if view.pipelines.dirty {
                "Discard pipeline draft and reload from disk"
            } else {
                "Reload pipelines from disk"
            })
            .disabled(busy || launcher_effect)
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &reload_receipt, section, revision, window, cx) {
                    submit(view, true, None, window, cx);
                }
            })),
    );
    for (index, row) in view.pipelines.rows.iter().enumerate() {
        let token = row.token;
        let mut controls = div().flex().gap_2();
        let duplicate = receipt.clone();
        controls = controls.child(
            Button::new(SharedString::from(format!("cm-pipeline-duplicate-{token}")))
                .label("Duplicate")
                .disabled(busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &duplicate, section, revision, window, cx) {
                        return;
                    }
                    let Some(index) = view
                        .pipelines
                        .rows
                        .iter()
                        .position(|row| row.token == token)
                    else {
                        return;
                    };
                    let drafts = view.pipelines.snapshot(cx);
                    let Some(catalog) = view.catalog.clone() else {
                        return;
                    };
                    let mut copy = drafts[index].clone();
                    copy.id = unique_id(&copy.id, &catalog, &drafts);
                    copy.label = format!("{} Copy", copy.label);
                    let row = make_row(view, &copy, window, cx);
                    view.pipelines.duplicate.insert(row.token);
                    view.pipelines.rows.push(row);
                    changed(view, cx);
                })),
        );
        for (label, direction) in [("Move up", -1isize), ("Move down", 1isize)] {
            let move_receipt = receipt.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!(
                    "cm-pipeline-move-{token}-{direction}"
                )))
                .label(label)
                .disabled(
                    busy || launcher_effect
                        || (direction < 0 && index == 0)
                        || (direction > 0 && index + 1 == view.pipelines.rows.len()),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &move_receipt, section, revision, window, cx)
                        || manage::launcher_effect_pending(view, cx)
                    {
                        return;
                    }
                    let Some(index) = view
                        .pipelines
                        .rows
                        .iter()
                        .position(|row| row.token == token)
                    else {
                        return;
                    };
                    let Some(target) = index
                        .checked_add_signed(direction)
                        .filter(|target| *target < view.pipelines.rows.len())
                    else {
                        return;
                    };
                    view.pipelines.rows.swap(index, target);
                    changed(view, cx);
                    if can_auto_save(!view.pipelines.duplicate.is_empty()) {
                        submit(view, false, None, window, cx);
                    } else {
                        view.pipelines.feedback =
                            Some("Reorder staged; duplicate requires explicit Save.".into());
                        cx.notify();
                    }
                })),
            );
        }
        let delete = receipt.clone();
        controls = controls.child(
            Button::new(SharedString::from(format!("cm-pipeline-delete-{token}")))
                .label("Delete")
                .disabled(busy || launcher_effect)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &delete, section, revision, window, cx)
                        || manage::launcher_effect_pending(view, cx)
                    {
                        return;
                    }
                    view.pipelines.review_serial = view.pipelines.review_serial.wrapping_add(1);
                    view.pipelines.review = Some(DeleteReview {
                        token,
                        revision,
                        serial: view.pipelines.review_serial,
                    });
                    view.sync_editor_readonly(cx);
                    view.pipelines.cancel_focus.focus(window, cx);
                    cx.notify();
                })),
        );
        let preview = receipt.clone();
        controls = controls.child(
            Button::new(SharedString::from(format!("cm-pipeline-preview-{token}")))
                .label("Preview complete output")
                .disabled(busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &preview, section, revision, window, cx)
                        || !view.resolved
                    {
                        return;
                    }
                    let source = view.source.read(cx).value().to_string();
                    if source.len() > LARGE_SOURCE && !view.large_allowed {
                        view.pipelines.feedback =
                            Some("Approve large source preview first.".into());
                        cx.notify();
                        return;
                    }
                    let Some(row) = view.pipelines.rows.iter().find(|row| row.token == token)
                    else {
                        return;
                    };
                    let stages = row.stages.iter().map(|stage| stage.spec.clone()).collect();
                    let Some(catalog) = view.catalog.clone() else {
                        return;
                    };
                    view.preview
                        .request(source, ClipboardModifyIntent::Stages(stages), catalog);
                    view.pump(cx);
                    cx.notify();
                })),
        );
        let mut body = div()
            .id(SharedString::from(format!("cm-manage-pipeline-{token}")))
            .flex()
            .flex_col()
            .gap_1()
            .child(Input::new(&row.id).readonly(busy).aria_label("Pipeline ID"))
            .child(
                Input::new(&row.label)
                    .readonly(busy)
                    .aria_label("Pipeline label"),
            )
            .child(
                Input::new(&row.aliases)
                    .readonly(busy)
                    .aria_label("Pipeline aliases"),
            )
            .child(controls);
        for stage in &row.stages {
            body = render_stage(view, body, token, stage, section, revision, cx);
        }
        let add_stage = receipt.clone();
        body = body.child(
            Button::new(SharedString::from(format!("cm-pipeline-add-stage-{token}")))
                .label("Add stage")
                .disabled(busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &add_stage, section, revision, window, cx) {
                        return;
                    }
                    let Some(index) = view
                        .pipelines
                        .rows
                        .iter()
                        .position(|row| row.token == token)
                    else {
                        return;
                    };
                    view.pipelines.next_stage = view.pipelines.next_stage.wrapping_add(1);
                    let stage = view.build_stage_for(
                        StageOwner::Pipeline(token),
                        view.pipelines.next_stage,
                        StageSpec {
                            operation: OperationId::Uppercase,
                            arguments: StageArguments::default(),
                        },
                        window,
                        cx,
                    );
                    view.pipelines.rows[index].stages.push(stage);
                    changed(view, cx);
                })),
        );
        panel = panel.child(body);
    }
    let add = receipt.clone();
    panel = panel.child(
        Button::new("cm-pipeline-add")
            .label("Add pipeline")
            .disabled(busy)
            .on_click(cx.listener(move |view, _, window, cx| {
                if !action_current(view, &add, section, revision, window, cx) {
                    return;
                }
                let Some(catalog) = view.catalog.clone() else {
                    return;
                };
                let drafts = view.pipelines.snapshot(cx);
                let value = SavedPipeline {
                    id: unique_id("new-pipeline", &catalog, &drafts),
                    label: "New Pipeline".into(),
                    aliases: Vec::new(),
                    stages: Vec::new(),
                };
                let row = make_row(view, &value, window, cx);
                view.pipelines.rows.push(row);
                changed(view, cx);
            })),
    );
    let save = receipt.clone();
    panel = panel.child(
        Button::new("cm-pipeline-save")
            .label("Save pipelines")
            .disabled(busy || launcher_effect || !view.pipelines.dirty || validation.is_some())
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &save, section, revision, window, cx) {
                    submit(view, false, None, window, cx);
                }
            })),
    );
    let cancel_preview = receipt.clone();
    let operation = view.preview.current_operation();
    panel = panel.child(
        Button::new("cm-pipeline-cancel-preview")
            .label("Cancel preview")
            .disabled(operation.is_none())
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &cancel_preview, section, revision, window, cx)
                    && operation.is_some_and(|id| view.preview.cancel_if_current(id))
                {
                    cx.notify();
                }
            })),
    );
    let wrap = receipt.clone();
    panel = panel.child(
        Button::new("cm-pipeline-wrap")
            .label(if view.wrap {
                "Wrap preview: On"
            } else {
                "Wrap preview: Off"
            })
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &wrap, section, revision, window, cx) {
                    view.wrap = !view.wrap;
                    cx.notify();
                }
            })),
    );
    if view.source.read(cx).value().len() > LARGE_SOURCE && !view.large_allowed {
        let allow = receipt.clone();
        let source_revision = view.source_revision;
        panel = panel.child(
            Button::new("cm-pipeline-large")
                .label("Preview large source")
                .on_click(cx.listener(move |view, _, window, cx| {
                    if action_current(view, &allow, section, revision, window, cx)
                        && consent_is_current(
                            source_revision,
                            view.source_revision,
                            view.source.read(cx).value().len(),
                        )
                    {
                        view.large_allowed = true;
                        cx.notify();
                    }
                })),
        );
    }
    let close = receipt.clone();
    panel.child(
        Button::new("cm-pipeline-close")
            .label("Close")
            .disabled(busy)
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &close, section, revision, window, cx) {
                    view.close(window, cx);
                }
            })),
    )
}

fn render_stage(
    view: &ClipboardModifyView,
    mut body: Stateful<Div>,
    row: u64,
    stage: &StageEditor,
    section: u64,
    revision: u64,
    cx: &mut Context<ClipboardModifyView>,
) -> Stateful<Div> {
    let busy = view.blocked();
    let id = stage.id;
    let mount = view.mount.clone();
    let choose = mount.clone();
    let remove = mount.clone();
    let info = catalog::operation_by_id(stage.spec.operation);
    let mut line = div()
        .id(SharedString::from(format!("cm-pipeline-stage-{row}-{id}")))
        .flex()
        .flex_col()
        .gap_1()
        .child(Label::new(info.map_or("Unknown operation", |op| op.label)))
        .child(
            Button::new(SharedString::from(format!(
                "cm-pipeline-stage-change-{row}-{id}"
            )))
            .label("Change operation")
            .on_click(cx.listener(move |view, _, window, cx| {
                if action_current(view, &choose, section, revision, window, cx)
                    && view.pipelines.stage(row, id).is_some()
                {
                    view.pipelines.picker = if view.pipelines.picker == Some((row, id)) {
                        None
                    } else {
                        Some((row, id))
                    };
                    cx.notify();
                }
            })),
        )
        .child(
            Button::new(SharedString::from(format!(
                "cm-pipeline-stage-remove-{row}-{id}"
            )))
            .label("Remove stage")
            .on_click(cx.listener(move |view, _, window, cx| {
                if !action_current(view, &remove, section, revision, window, cx) {
                    return;
                }
                let Some(item) = view
                    .pipelines
                    .rows
                    .iter_mut()
                    .find(|item| item.token == row)
                else {
                    return;
                };
                let Some(index) = item.stages.iter().position(|stage| stage.id == id) else {
                    return;
                };
                view.focus.focus(window, cx);
                item.stages.remove(index);
                view.pipelines.picker = None;
                changed(view, cx);
            })),
        );
    if let Some(info) = info {
        line = match info.argument_requirements {
            ArgumentRequirements::CustomWrap => line
                .child(
                    Textarea::new(&stage.prefix)
                        .readonly(busy)
                        .aria_label("Prefix"),
                )
                .child(
                    Textarea::new(&stage.suffix)
                        .readonly(busy)
                        .aria_label("Suffix"),
                ),
            ArgumentRequirements::NamedWrap | ArgumentRequirements::Template => line.child(
                Textarea::new(&stage.name)
                    .readonly(busy)
                    .aria_label("Template or wrapper"),
            ),
            ArgumentRequirements::CodeBlock => line.child(
                Textarea::new(&stage.language)
                    .readonly(busy)
                    .aria_label("Language"),
            ),
            ArgumentRequirements::RequiredSeparator => line.child(
                Textarea::new(&stage.separator)
                    .readonly(busy)
                    .aria_label("Separator"),
            ),
            ArgumentRequirements::OptionalSeparator => {
                let present = stage.spec.arguments.separator.is_some();
                let toggle = mount.clone();
                let line = line.child(
                    Button::new(SharedString::from(format!(
                        "cm-pipeline-separator-{row}-{id}"
                    )))
                    .label(if present {
                        "Use default separator"
                    } else {
                        "Set separator"
                    })
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !action_current(view, &toggle, section, revision, window, cx) {
                            return;
                        }
                        let Some(stage) = view.pipelines.stage_mut(row, id) else {
                            return;
                        };
                        stage.spec.arguments.separator =
                            if present { None } else { Some(String::new()) };
                        stage
                            .separator
                            .update(cx, |input, cx| input.set_value("", window, cx));
                        changed(view, cx);
                    })),
                );
                if present {
                    line.child(
                        Textarea::new(&stage.separator)
                            .readonly(busy)
                            .aria_label("Separator"),
                    )
                } else {
                    line
                }
            }
            ArgumentRequirements::None => line,
        };
    }
    if view.pipelines.picker == Some((row, id)) {
        for op in catalog::operations()
            .iter()
            .filter(|op| op.pipeline_available)
        {
            let receipt = mount.clone();
            let operation = op.id;
            line = line.child(
                Button::new(SharedString::from(format!(
                    "cm-pipeline-operation-{row}-{id}-{operation:?}"
                )))
                .label(op.label)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !action_current(view, &receipt, section, revision, window, cx)
                        || view.pipelines.picker != Some((row, id))
                    {
                        return;
                    }
                    let Some(row_index) = view
                        .pipelines
                        .rows
                        .iter()
                        .position(|item| item.token == row)
                    else {
                        return;
                    };
                    let Some(stage_index) = view.pipelines.rows[row_index]
                        .stages
                        .iter()
                        .position(|stage| stage.id == id)
                    else {
                        return;
                    };
                    if view.pipelines.rows[row_index].stages[stage_index]
                        .spec
                        .operation
                        != operation
                    {
                        let stage = view.build_stage_for(
                            StageOwner::Pipeline(row),
                            id,
                            StageSpec {
                                operation,
                                arguments: StageArguments::default(),
                            },
                            window,
                            cx,
                        );
                        view.pipelines.rows[row_index].stages[stage_index] = stage;
                        view.pipelines.picker = None;
                        changed(view, cx);
                    } else {
                        view.pipelines.picker = None;
                        cx.notify();
                    }
                })),
            );
        }
    }
    body = body.child(line);
    body
}

#[cfg(test)]
mod tests {
    use super::{can_auto_save, remove_reviewed_candidate, unique_id};
    use multi_launcher::clipboard_modify::defaults::default_catalog;
    #[test]
    fn duplicate_names_respect_canonical_and_draft_aliases_and_block_auto_save() {
        let catalog = default_catalog();
        let mut drafts = catalog.pipelines.clone();
        let base = drafts[0].id.clone();
        let first = unique_id(&base, &catalog, &drafts);
        drafts[0].aliases.push(first.clone());
        let second = unique_id(&base, &catalog, &drafts);
        assert_ne!(first, second);
        assert!(can_auto_save(false));
        assert!(!can_auto_save(true));
    }
    #[test]
    fn pipeline_preview_retains_explicit_empty_separator() {
        use multi_launcher::clipboard_modify::{
            defaults::default_catalog,
            executor::execute_stages,
            model::{OperationId, StageArguments, StageSpec},
        };
        let catalog = default_catalog();
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let stage = StageSpec {
            operation: OperationId::JoinLines,
            arguments: StageArguments {
                separator: Some(String::new()),
                ..Default::default()
            },
        };
        assert_eq!(
            execute_stages("a\nb", &[stage], &catalog, &cancel).unwrap(),
            "ab"
        );
        let wrap = StageSpec {
            operation: OperationId::CustomWrap,
            arguments: StageArguments {
                prefix: Some("before\n".into()),
                suffix: Some("\nafter".into()),
                ..Default::default()
            },
        };
        assert_eq!(
            execute_stages("é", &[wrap], &catalog, &cancel).unwrap(),
            "before\né\nafter"
        );
    }
    #[test]
    fn reviewed_delete_builds_a_candidate_without_mutating_retained_rows() {
        let retained = vec!["first", "reviewed", "last"];
        let mut candidate = retained.clone();
        assert!(remove_reviewed_candidate(&[7, 8, 9], &mut candidate, 8));
        assert_eq!(candidate, vec!["first", "last"]);
        assert_eq!(retained, vec!["first", "reviewed", "last"]);
        assert!(!remove_reviewed_candidate(&[7, 8, 9], &mut candidate, 8));
    }
}
