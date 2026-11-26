//! Retained management drafts and checked catalog saves.
use super::*;
use multi_launcher::clipboard_modify::{
    model::{ClipboardTemplate, TemplateProcessor},
    store::{CheckedCatalogEdit, CheckedCatalogError, CheckedCatalogRead, CheckedCatalogSave},
};

fn mutation_allowed(
    section: Section,
    section_revision: u64,
    expected_section: u64,
    revision: u64,
    expected_revision: u64,
    blocked: bool,
    mounted: bool,
) -> bool {
    section == Section::ManageTemplates
        && section_revision == expected_section
        && revision == expected_revision
        && !blocked
        && mounted
}
fn reorder_auto_saves(staged_delete: bool) -> bool {
    !staged_delete
}
fn accepted_identity<'a>(
    token: u64,
    tokens: &[u64],
    templates: &'a [ClipboardTemplate],
) -> Option<&'a str> {
    tokens
        .iter()
        .position(|candidate| *candidate == token)
        .and_then(|index| templates.get(index))
        .map(|template| template.id.as_str())
}
fn reload_receipt_current(
    mount: bool,
    section: bool,
    draft: bool,
    authority: bool,
    host: bool,
) -> bool {
    mount && section && draft && authority && host
}
fn referencing_pipelines(
    catalog: &ClipboardModifierCatalog,
    persisted_id: Option<&str>,
    draft: &ClipboardTemplate,
) -> Vec<String> {
    use multi_launcher::clipboard_modify::{
        catalog::normalize_name, model::OperationId, pipeline::find_template,
    };
    let names: Vec<_> = std::iter::once(&draft.id)
        .chain(draft.aliases.iter())
        .map(|name| normalize_name(name))
        .collect();
    catalog
        .pipelines
        .iter()
        .filter(|pipeline| {
            pipeline.stages.iter().any(|stage| {
                if !matches!(
                    stage.operation,
                    OperationId::Template | OperationId::NamedWrap
                ) {
                    return false;
                }
                let Some(name) = stage.arguments.name.as_deref() else {
                    return false;
                };
                persisted_id.is_some_and(|id| {
                    find_template(catalog, name).is_some_and(|template| template.id == id)
                }) || names.contains(&normalize_name(name))
            })
        })
        .map(|pipeline| format!("{} ({})", pipeline.label, pipeline.id))
        .collect()
}
pub(super) fn host_current(
    view: &ClipboardModifyView,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) -> bool {
    if window.has_active_dialog(cx) || !window.is_window_active() {
        return false;
    }
    host::HostAuthority::capture(view, window, cx).is_some()
}
pub(super) fn launcher_effect_pending(view: &ClipboardModifyView, cx: &App) -> bool {
    view.root
        .upgrade()
        .is_some_and(|root| root.read(cx).external_launch.clipboard_effect_pending())
}
fn validation_error(view: &ClipboardModifyView, cx: &App) -> Option<String> {
    let catalog = view.catalog.as_ref()?;
    let model = multi_launcher::clipboard_modify::config::VersionedClipboardModifiersFile {
        schema_version: multi_launcher::clipboard_modify::config::CURRENT_SCHEMA_VERSION,
        templates: view.management.snapshot(cx),
        pipelines: catalog.pipelines.clone(),
    };
    multi_launcher::clipboard_modify::config::validate_model(&model)
        .err()
        .map(|error| error.to_string())
}

struct TemplateRow {
    token: u64,
    persisted_id: Option<String>,
    id: Entity<InputState>,
    label: Entity<InputState>,
    aliases: Entity<InputState>,
    body: Entity<TextareaState>,
    processor: Option<TemplateProcessor>,
    subscriptions: Vec<Subscription>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DraftSnapshot(Vec<(String, String, String, String, Option<TemplateProcessor>)>);
#[derive(Default)]
pub(super) struct Management {
    rows: Vec<TemplateRow>,
    baseline: Option<Vec<ClipboardTemplate>>,
    revision: u64,
    next_row: u64,
    pub pending: Option<u64>,
    pub(super) serial: u64,
    pub(super) authority: Arc<AtomicU64>,
    dirty: bool,
    staged_delete: bool,
    conflict: bool,
    feedback: Option<String>,
}
impl Management {
    fn apply_saved_ids(&mut self, tokens: &[u64], templates: &[ClipboardTemplate]) {
        for row in &mut self.rows {
            if let Some(id) = accepted_identity(row.token, tokens, templates) {
                row.persisted_id = Some(id.to_owned());
            }
        }
    }
    fn snapshot_row(row: &TemplateRow, cx: &App) -> ClipboardTemplate {
        ClipboardTemplate {
            id: row.id.read(cx).value().to_string(),
            label: row.label.read(cx).value().to_string(),
            aliases: row
                .aliases
                .read(cx)
                .value()
                .split(',')
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
                .collect(),
            template: row.body.read(cx).value().to_string(),
            processor: row.processor,
        }
    }
    pub fn input_focused(&self, window: &Window, cx: &App) -> bool {
        self.rows.iter().any(|row| {
            row.id.focus_handle(cx).is_focused(window)
                || row.label.focus_handle(cx).is_focused(window)
                || row.aliases.focus_handle(cx).is_focused(window)
                || row.body.focus_handle(cx).is_focused(window)
        })
    }
    pub fn clear(&mut self) {
        *self = Self::default();
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
                            row.body.read(cx).value().to_string(),
                            row.processor,
                        )
                    })
                    .collect(),
            )
        })
    }
    fn snapshot(&self, cx: &App) -> Vec<ClipboardTemplate> {
        self.rows
            .iter()
            .map(|row| Self::snapshot_row(row, cx))
            .collect()
    }
    pub fn set_readonly(&self, readonly: bool, cx: &mut Context<ClipboardModifyView>) {
        for row in &self.rows {
            for input in [&row.id, &row.label, &row.aliases] {
                input.update(cx, |input, cx| input.set_readonly(readonly, cx));
            }
            row.body
                .update(cx, |input, cx| input.set_readonly(readonly, cx));
        }
    }
    fn mark_changed(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.authority.fetch_add(1, Ordering::AcqRel);
        self.dirty = true;
        self.feedback = None;
    }
}

fn subscribe_row(
    row: &TemplateRow,
    receipt: utility_mount::MountReceipt,
    section: u64,
    cx: &mut Context<ClipboardModifyView>,
) -> Vec<Subscription> {
    let token = row.token;
    let mut subscriptions = Vec::new();
    for input in [&row.id, &row.label, &row.aliases] {
        let receipt = receipt.clone();
        subscriptions.push(cx.subscribe(
            input,
            move |view: &mut ClipboardModifyView,
                  input: Entity<InputState>,
                  event: &InputEvent,
                  cx| {
                if !matches!(event, InputEvent::Change)
                    || !receipt.is_current()
                    || !receipt.matches(&view.mount)
                    || !view.open
                    || view.section != Section::ManageTemplates
                    || view.section_revision != section
                {
                    return;
                }
                if !view.management.rows.iter().any(|row| {
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
                view.management.mark_changed();
                cx.notify();
            },
        ));
    }
    subscriptions.push(cx.subscribe(
        &row.body,
        move |view: &mut ClipboardModifyView,
              input: Entity<TextareaState>,
              event: &InputEvent,
              cx| {
            if !matches!(event, InputEvent::Change)
                || !receipt.is_current()
                || !receipt.matches(&view.mount)
                || !view.open
                || view.section != Section::ManageTemplates
                || view.section_revision != section
            {
                return;
            }
            if !view
                .management
                .rows
                .iter()
                .any(|row| row.token == token && row.body.entity_id() == input.entity_id())
            {
                return;
            }
            view.management.mark_changed();
            cx.notify();
        },
    ));
    subscriptions
}
fn make_row(
    value: &ClipboardTemplate,
    token: u64,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
    receipt: utility_mount::MountReceipt,
    section: u64,
) -> TemplateRow {
    let id = cx.new(|cx| InputState::new(window, cx));
    let label = cx.new(|cx| InputState::new(window, cx));
    let aliases = cx.new(|cx| InputState::new(window, cx));
    let body = cx.new(|cx| TextareaState::new(window, cx).auto_grow(3, 10));
    id.update(cx, |input, cx| {
        input.set_value(value.id.clone(), window, cx)
    });
    label.update(cx, |input, cx| {
        input.set_value(value.label.clone(), window, cx)
    });
    aliases.update(cx, |input, cx| {
        input.set_value(value.aliases.join(", "), window, cx)
    });
    body.update(cx, |input, cx| {
        input.set_value(value.template.clone(), window, cx)
    });
    let mut row = TemplateRow {
        token,
        persisted_id: Some(value.id.clone()),
        id,
        label,
        aliases,
        body,
        processor: value.processor,
        subscriptions: Vec::new(),
    };
    row.subscriptions = subscribe_row(&row, receipt, section, cx);
    row
}

pub(super) fn ensure_templates(
    view: &mut ClipboardModifyView,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) {
    if view.management.baseline.is_none() {
        let Some(catalog) = view.catalog.as_ref() else {
            return;
        };
        let templates = catalog.templates.clone();
        view.management.baseline = Some(templates.clone());
        for template in &templates {
            view.management.next_row = view.management.next_row.wrapping_add(1);
            let row = make_row(
                template,
                view.management.next_row,
                window,
                cx,
                view.mount.clone(),
                view.section_revision,
            );
            view.management.rows.push(row);
        }
    } else {
        for row in &mut view.management.rows {
            row.subscriptions = subscribe_row(row, view.mount.clone(), view.section_revision, cx);
        }
    }
}

pub(super) fn remount(view: &mut ClipboardModifyView, window: &mut Window, cx: &mut Context<ClipboardModifyView>) {
    let rows: Vec<_> = view.management.rows.iter().map(|row| (
        row.token, row.persisted_id.clone(), Management::snapshot_row(row, cx), row.aliases.read(cx).value().to_string()
    )).collect();
    view.management.rows = rows.into_iter().map(|(token, persisted, value, aliases)| {
        let mut row = make_row(&value, token, window, cx, view.mount.clone(), view.section_revision);
        row.persisted_id = persisted;
        row.aliases.update(cx, |input, cx| input.set_value(aliases, window, cx));
        row
    }).collect();
}

fn save_templates(
    view: &mut ClipboardModifyView,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) {
    if view.blocked()
        || view.section != Section::ManageTemplates
        || !view.mount.is_current()
        || !window.is_window_active()
        || window.has_active_dialog(cx)
    {
        return;
    }
    if let Some(error) = validation_error(view, cx) {
        view.management.feedback = Some(format!("Template validation failed: {error}"));
        cx.notify();
        return;
    }
    let Some(runtime) = view.runtime.as_ref() else {
        return;
    };
    let Some(baseline) = view.management.baseline.clone() else {
        return;
    };
    let Some(root) = view.root.upgrade() else {
        return;
    };
    let root = root.read(cx);
    let Some(host_authority) = host::HostAuthority::capture(view, window, cx) else { return; };
    let host = host_authority.host;
    if root.external_launch.clipboard_effect_pending() { return; }
    let Some(permit) = root.restart_admission.admit() else {
        return;
    };
    let store = runtime.store.clone();
    let draft = view.management.snapshot(cx);
    let draft_tokens: Vec<_> = view.management.rows.iter().map(|row| row.token).collect();
    view.management.serial = view.management.serial.wrapping_add(1);
    let serial = view.management.serial;
    let revision = view.management.revision;
    view.management.pending = Some(serial);
    view.management.feedback = None;
    view.sync_editor_readonly(cx);
    let receipt = view.mount.clone();
    let worker_host = host_authority;
    let authority = view.management.authority.clone();
    let epoch = authority.load(Ordering::Acquire);
    let task = cx.background_executor().spawn(async move {
        let current = || {
            authority.load(Ordering::Acquire) == epoch && worker_host.current()
        };
        store.save_checked(
            CheckedCatalogEdit::Templates {
                baseline: &baseline,
                draft,
            },
            &current,
        )
    });
    cx.spawn(async move |weak, cx| {
        let _permit = permit;
        let result = task.await;
        let Some(view) = weak.upgrade() else { return; };
        let delivered = update_kit_window_content(host, &view, cx, |view, window, cx| {
            if view.management.pending != Some(serial) { return; }
            view.management.pending = None;
            if matches!(&result, Ok(CheckedCatalogSave::Saved(_))) { refresh_catalog_search(view, cx); }
            if !receipt.is_current() || !receipt.matches(&view.mount) || view.section != Section::ManageTemplates { view.clear_block_wait(cx); cx.notify(); return; }
            if view.management.revision != revision {
                if let Ok(CheckedCatalogSave::Saved(catalog)) = result {
                    view.management.apply_saved_ids(&draft_tokens, &catalog.templates);
                    view.management.baseline = Some(catalog.templates.clone());
                    view.catalog = Some(catalog);
                    view.sections.template = None; view.sections.pipeline = None;
                    view.preview.cancel_and_invalidate();
                    view.management.dirty = true;
                    view.management.feedback = Some("Saved earlier edits; newer draft changes remain unsaved.".into());
                }
                view.clear_block_wait(cx); cx.notify(); return;
            }
            match result {
                Ok(CheckedCatalogSave::Saved(catalog)) => {
                    view.management.baseline = Some(catalog.templates.clone());
                    view.management.dirty = false;
                    view.management.staged_delete = false;
                    view.management.conflict = false;
                    view.management.apply_saved_ids(&draft_tokens, &catalog.templates);
                    view.management.feedback = Some("Templates saved.".into());
                    view.catalog = Some(catalog);
                    view.sections.template = None; view.sections.pipeline = None;
                    view.section_revision = view.section_revision.wrapping_add(1);
                    view.preview.cancel_and_invalidate();
                    ensure_templates(view, window, cx);
                }
                Ok(CheckedCatalogSave::Conflict) => { view.management.conflict = true; view.management.feedback = Some("Templates changed on disk. Draft retained. Use Discard draft and reload to accept disk changes.".into()); },
                Ok(CheckedCatalogSave::Superseded) => view.management.feedback = Some("Save was superseded. Draft retained.".into()),
                Err(CheckedCatalogError::InvalidDraft) => view.management.feedback = Some("Templates are invalid or referenced by a saved pipeline. Draft retained.".into()),
                Err(_) => view.management.feedback = Some("Templates could not be saved. Draft retained.".into()),
            }
            view.clear_block_wait(cx); cx.notify();
        });
        if delivered.is_err() { let _ = view.update(cx, |view, cx| { if view.management.pending == Some(serial) { view.management.pending = None; view.management.feedback = Some("Save result could not be delivered. Draft retained.".into()); view.clear_block_wait(cx); cx.notify(); } }); }
    }).detach();
    cx.notify();
}

fn discard_and_reload(
    view: &mut ClipboardModifyView,
    window: &mut Window,
    cx: &mut Context<ClipboardModifyView>,
) {
    if view.blocked()
        || view.section != Section::ManageTemplates
        || !host_current(view, window, cx)
        || launcher_effect_pending(view, cx)
    {
        return;
    }
    let Some(runtime) = view.runtime.as_ref() else {
        return;
    };
    let store = runtime.store.clone();
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
    let host = host_authority.host;
    let ack_host = host_authority.clone();
    let receipt = view.mount.clone();
    let worker_host = host_authority;
    let authority = view.management.authority.clone();
    let ack_authority = authority.clone();
    let epoch = authority.load(Ordering::Acquire);
    let revision = view.management.revision;
    let section_revision = view.section_revision;
    view.management.serial = view.management.serial.wrapping_add(1);
    let serial = view.management.serial;
    view.management.pending = Some(serial);
    view.sync_editor_readonly(cx);
    let task = cx.background_executor().spawn(async move {
        let current = || {
            authority.load(Ordering::Acquire) == epoch && worker_host.current()
        };
        store.refresh_checked(&current)
    });
    cx.spawn(async move |weak, cx| {
        let _permit = permit;
        let result = task.await;
        let Some(view) = weak.upgrade() else {
            return;
        };
        let delivered = update_kit_window_content(host, &view, cx, |view, window, cx| {
            if view.management.pending != Some(serial) {
                return;
            }
            view.management.pending = None;
            if matches!(&result, Ok(CheckedCatalogRead::Loaded(_))) { refresh_catalog_search(view, cx); }
            let loaded = if let Ok(CheckedCatalogRead::Loaded(catalog)) = &result {
                view.catalog = Some(catalog.clone());
                view.sections.template = None;
                view.sections.pipeline = None;
                view.preview.cancel_and_invalidate();
                true
            } else { false };
            let may_replace = reload_receipt_current(
                receipt.is_current() && receipt.matches(&view.mount),
                view.section == Section::ManageTemplates && view.section_revision == section_revision,
                view.management.revision == revision,
                ack_authority.load(Ordering::Acquire) == epoch && !ack_admission.is_fenced(),
                host_current(view, window, cx) && window.window_handle().window_id() == host.window_id()
                    && current_foreground_identity() == Some(native.identity)
                    && ack_host.current_for(view, window, cx),
            );
            if !may_replace {
                view.management.feedback = Some(if loaded { "Catalog refreshed; the draft was retained because its window or contents changed." } else { "Reload was superseded. Draft retained." }.into());
                view.clear_block_wait(cx); cx.notify(); return;
            }
            match result {
                Ok(CheckedCatalogRead::Loaded(catalog)) => {
                    if window.is_window_active()
                        && current_foreground_identity() == Some(native.identity)
                        && !window.has_active_dialog(cx)
                        && (view.focus.is_focused(window)
                            || view.focus.contains_focused(window, cx))
                    {
                        view.focus.focus(window, cx);
                    }
                    view.catalog = Some(catalog);
                    view.sections.template = None;
                    view.sections.pipeline = None;
                    view.preview.cancel_and_invalidate();
                    view.management.rows.clear();
                    view.management.baseline = None;
                    view.management.dirty = false;
                    view.management.staged_delete = false;
                    view.management.conflict = false;
                    view.management.revision = view.management.revision.wrapping_add(1);
                    view.section_revision = view.section_revision.wrapping_add(1);
                    ensure_templates(view, window, cx);
                    view.management.feedback = Some("Reloaded templates from disk.".into());
                }
                Ok(CheckedCatalogRead::Superseded) => {
                    view.management.feedback = Some("Reload was superseded. Draft retained.".into())
                }
                Ok(CheckedCatalogRead::Changed) => {
                    view.management.feedback =
                        Some("Catalog changed during reload. Draft retained; try again.".into())
                }
                Err(_) => {
                    view.management.feedback =
                        Some("Catalog could not be reloaded. Draft retained.".into())
                }
            }
            view.clear_block_wait(cx);
            cx.notify();
        });
        if delivered.is_err() {
            let _ = view.update(cx, |view, cx| {
                if view.management.pending == Some(serial) {
                    view.management.pending = None;
                    view.management.feedback =
                        Some("Reload result could not be delivered. Draft retained.".into());
                    view.clear_block_wait(cx);
                    cx.notify();
                }
            });
        }
    })
    .detach();
    cx.notify();
}

pub(super) fn render_templates(
    view: &mut ClipboardModifyView,
    mut panel: Stateful<Div>,
    cx: &mut Context<ClipboardModifyView>,
) -> Stateful<Div> {
    if view.management.baseline.is_none() {
        return panel.child(Label::new("Loading templates…"));
    }
    let mount = view.mount.clone();
    let section = view.section_revision;
    let revision = view.management.revision;
    let busy = view.management.pending.is_some();
    let launcher_effect_busy = launcher_effect_pending(view, cx);
    let validation = validation_error(view, cx);
    panel = panel.child(Label::new(
        "Manage templates · Aliases are comma-separated.",
    ));
    if let Some(feedback) = &view.management.feedback {
        panel = panel.child(Label::new(feedback.clone()));
    }
    {
        let receipt = mount.clone();
        panel = panel.child(
            Button::new("cm-template-discard-reload")
                .label(if view.management.dirty {
                    "Discard draft and reload from disk"
                } else {
                    "Reload from disk"
                })
                .disabled(busy || launcher_effect_busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if receipt.is_current()
                        && receipt.matches(&view.mount)
                        && view.section_revision == section
                        && view.management.revision == revision
                    {
                        discard_and_reload(view, window, cx);
                    }
                })),
        );
    }
    if let Some(error) = &validation {
        panel = panel.child(Label::new(format!("Template validation failed: {error}")));
    }
    for (index, row) in view.management.rows.iter().enumerate() {
        let token = row.token;
        let mut controls = div().flex().gap_2();
        for (label, direction) in [("Move up", -1isize), ("Move down", 1isize)] {
            let receipt = mount.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!(
                    "cm-template-move-{token}-{direction}"
                )))
                .label(label)
                .disabled(
                    busy || launcher_effect_busy
                        || (direction < 0 && index == 0)
                        || (direction > 0 && index + 1 == view.management.rows.len()),
                )
                .on_click(cx.listener(move |view, _, window, cx| {
                    if view.blocked()
                        || !host_current(view, window, cx)
                        || launcher_effect_pending(view, cx)
                        || view.section != Section::ManageTemplates
                        || view.section_revision != section
                        || view.management.revision != revision
                        || !receipt.is_current()
                        || !receipt.matches(&view.mount)
                    {
                        return;
                    }
                    let Some(index) = view
                        .management
                        .rows
                        .iter()
                        .position(|row| row.token == token)
                    else {
                        return;
                    };
                    let Some(target) = index
                        .checked_add_signed(direction)
                        .filter(|target| *target < view.management.rows.len())
                    else {
                        return;
                    };
                    view.management.rows.swap(index, target);
                    view.management.mark_changed();
                    if !reorder_auto_saves(view.management.staged_delete) {
                        view.management.feedback = Some(
                            "Reorder staged with deletion. Select Save templates to commit.".into(),
                        );
                        cx.notify();
                    } else {
                        save_templates(view, window, cx);
                    }
                })),
            );
        }
        let delete = mount.clone();
        controls = controls.child(
            Button::new(SharedString::from(format!("cm-template-delete-{token}")))
                .label("Delete")
                .disabled(busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if !host_current(view, window, cx)
                        || !mutation_allowed(
                            view.section,
                            view.section_revision,
                            section,
                            view.management.revision,
                            revision,
                            view.blocked(),
                            delete.is_current() && delete.matches(&view.mount),
                        )
                    {
                        return;
                    }
                    if let Some(index) = view
                        .management
                        .rows
                        .iter()
                        .position(|row| row.token == token)
                    {
                        let Some(catalog) = view.catalog.as_ref() else {
                            return;
                        };
                        let row = &view.management.rows[index];
                        let references = referencing_pipelines(
                            catalog,
                            row.persisted_id.as_deref(),
                            &Management::snapshot_row(row, cx),
                        );
                        if !references.is_empty() {
                            view.management.feedback = Some(format!(
                                "Cannot delete this template; saved pipelines reference it: {}",
                                references.join(", ")
                            ));
                            cx.notify();
                            return;
                        }
                        view.focus.focus(window, cx);
                        view.management.rows.remove(index);
                        view.management.mark_changed();
                        view.management.staged_delete = true;
                        cx.notify();
                    }
                })),
        );
        for (label, processor) in [
            ("Literal", TemplateProcessor::Literal),
            ("Rust raw string", TemplateProcessor::RustRawString),
        ] {
            let receipt = mount.clone();
            controls = controls.child(
                Button::new(SharedString::from(format!(
                    "cm-template-processor-{token}-{processor:?}"
                )))
                .label(label)
                .selected(row.processor == Some(processor))
                .disabled(busy)
                .on_click(cx.listener(move |view, _, window, cx| {
                    if view.blocked()
                        || !host_current(view, window, cx)
                        || view.section != Section::ManageTemplates
                        || view.section_revision != section
                        || view.management.revision != revision
                        || !receipt.is_current()
                        || !receipt.matches(&view.mount)
                    {
                        return;
                    }
                    if let Some(row) = view
                        .management
                        .rows
                        .iter_mut()
                        .find(|row| row.token == token)
                    {
                        row.processor = Some(processor);
                        view.management.mark_changed();
                        cx.notify();
                    }
                })),
            );
        }
        panel = panel.child(
            div()
                .id(SharedString::from(format!("cm-manage-template-{token}")))
                .flex()
                .flex_col()
                .gap_1()
                .child(Label::new("ID"))
                .child(Input::new(&row.id).readonly(busy).aria_label("Template ID"))
                .child(Label::new("Label"))
                .child(
                    Input::new(&row.label)
                        .readonly(busy)
                        .aria_label("Template label"),
                )
                .child(Label::new("Aliases"))
                .child(
                    Input::new(&row.aliases)
                        .readonly(busy)
                        .aria_label("Template aliases"),
                )
                .child(Label::new("Template body"))
                .child(
                    Textarea::new(&row.body)
                        .readonly(busy)
                        .aria_label("Template body"),
                )
                .child(controls),
        );
    }
    let add = mount.clone();
    panel = panel.child(
        Button::new("cm-template-add")
            .label("Add template")
            .disabled(busy)
            .on_click(cx.listener(move |view, _, window, cx| {
                if view.blocked()
                    || !host_current(view, window, cx)
                    || view.section != Section::ManageTemplates
                    || view.section_revision != section
                    || view.management.revision != revision
                    || !add.is_current()
                    || !add.matches(&view.mount)
                {
                    return;
                }
                let value = ClipboardTemplate {
                    id: "new-template".into(),
                    label: "New Template".into(),
                    aliases: Vec::new(),
                    template: "{{clipboard}}".into(),
                    processor: Some(TemplateProcessor::Literal),
                };
                view.management.next_row = view.management.next_row.wrapping_add(1);
                let mut row = make_row(
                    &value,
                    view.management.next_row,
                    window,
                    cx,
                    view.mount.clone(),
                    section,
                );
                row.persisted_id = None;
                view.management.rows.push(row);
                view.management.mark_changed();
                cx.notify();
            })),
    );
    let save = mount.clone();
    panel = panel.child(
        Button::new("cm-template-save")
            .label("Save templates")
            .disabled(busy || launcher_effect_busy || !view.management.dirty || validation.is_some())
            .on_click(cx.listener(move |view, _, window, cx| {
                if save.is_current()
                    && save.matches(&view.mount)
                    && view.section_revision == section
                    && view.management.revision == revision
                {
                    save_templates(view, window, cx);
                }
            })),
    );
    let close = mount.clone();
    panel.child(
        Button::new("cm-template-manage-close")
            .label("Close")
            .disabled(busy)
            .on_click(cx.listener(move |view, _, window, cx| {
                if host_current(view, window, cx)
                    && close.is_current()
                    && close.matches(&view.mount)
                    && view.section_revision == section
                {
                    view.close(window, cx);
                }
            })),
    )
}

#[cfg(test)]
mod tests {
    use super::{
        Section, accepted_identity, mutation_allowed, referencing_pipelines,
        reload_receipt_current, reorder_auto_saves,
    };
    #[test]
    fn deletion_finds_saved_alias_and_named_wrap_references() {
        use multi_launcher::clipboard_modify::{
            defaults::default_catalog,
            model::{OperationId, StageArguments, StageSpec},
        };
        let mut catalog = default_catalog();
        let template = catalog.templates[0].clone();
        catalog.pipelines[0].stages.push(StageSpec {
            operation: OperationId::NamedWrap,
            arguments: StageArguments {
                name: Some(template.aliases[0].clone()),
                ..Default::default()
            },
        });
        let mut edited = template.clone();
        edited.id = "new-id".into();
        edited.aliases.clear();
        assert_eq!(
            referencing_pipelines(&catalog, Some(&template.id), &edited),
            vec![format!(
                "{} ({})",
                catalog.pipelines[0].label, catalog.pipelines[0].id
            )]
        );
        assert!(reorder_auto_saves(false));
        assert!(!reorder_auto_saves(true));
        let accepted = catalog.templates.clone();
        assert_eq!(
            accepted_identity(9, &[4, 9], &accepted),
            Some(accepted[1].id.as_str())
        );
        assert_eq!(accepted_identity(3, &[4, 9], &accepted), None);
    }
    #[test]
    fn staged_mutation_requires_current_section_draft_and_mount() {
        assert!(reload_receipt_current(true, true, true, true, true));
        assert!(!reload_receipt_current(true, true, true, true, false));
        assert!(!reload_receipt_current(true, true, false, true, true));
        assert!(mutation_allowed(
            Section::ManageTemplates,
            2,
            2,
            3,
            3,
            false,
            true
        ));
        assert!(!mutation_allowed(
            Section::Templates,
            2,
            2,
            3,
            3,
            false,
            true
        ));
        assert!(!mutation_allowed(
            Section::ManageTemplates,
            3,
            2,
            3,
            3,
            false,
            true
        ));
        assert!(!mutation_allowed(
            Section::ManageTemplates,
            2,
            2,
            4,
            3,
            false,
            true
        ));
        assert!(!mutation_allowed(
            Section::ManageTemplates,
            2,
            2,
            3,
            3,
            true,
            true
        ));
        assert!(!mutation_allowed(
            Section::ManageTemplates,
            2,
            2,
            3,
            3,
            false,
            false
        ));
    }
}
