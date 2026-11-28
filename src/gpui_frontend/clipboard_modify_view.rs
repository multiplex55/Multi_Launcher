//! Retained first Clipboard Modify workflow using the shared engine and service.
use super::*;
use gpui_kit::base::Selectable;
use multi_launcher::clipboard_modify::{
    catalog::{self, ArgumentRequirements},
    clipboard::{CheckedClipboardEffect, CheckedClipboardOutcome, ClipboardReview},
    coordinator::{PreviewCoordinator, PreviewState},
    model::{ClipboardModifierCatalog, OperationId, StageArguments, StageSpec},
    parser::ClipboardModifyIntent,
    runtime::{self, ClipboardModifyRuntime},
};
use std::time::Duration;
#[path = "clipboard_modify_manage.rs"]
mod manage;
#[path = "clipboard_modify_host.rs"]
mod host;
#[path = "clipboard_modify_pipelines.rs"]
mod pipelines;
#[path = "clipboard_modify_sections.rs"]
mod sections;
use multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload as Section;
fn redacted_read_error(
    error: multi_launcher::clipboard_modify::clipboard::ClipboardError,
) -> String {
    use multi_launcher::clipboard_modify::clipboard::ClipboardError;
    match error {
        ClipboardError::NonText => "Clipboard does not contain text.",
        ClipboardError::ProtectedItem => "This clipboard item was copied privately.",
        ClipboardError::TimedOut => "The clipboard did not respond in time.",
        ClipboardError::Busy(_) | ClipboardError::Transient(_) => {
            "The clipboard is busy. Try again."
        }
        ClipboardError::InvalidContent(_) => "Clipboard text is invalid.",
        _ => "Could not read clipboard text.",
    }
    .into()
}
pub(super) const BLOCK_WAIT: &str = "Wait for Clipboard Modify to finish or cancel its review.";
fn refresh_catalog_search(view: &ClipboardModifyView, cx: &mut Context<ClipboardModifyView>) {
    let _ = view.root.update(cx, |root, cx| {
        let query = root.query.read(cx).value().to_string();
        if clipboard_modify_startup::is_clipboard_modify_search(&query) {
            root.refresh_launcher_results_with_viewport(query, false, cx);
            cx.notify();
        }
    });
}
pub(super) const LAUNCHER_EFFECT_WAIT: &str = "Wait for the Clipboard Modify operation to finish.";
const LARGE_SOURCE: usize = 5 * 1024 * 1024;
const EFFECT_WAIT: &str = "Wait for the Clipboard Modify operation to finish.";
#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectKind {
    Apply,
    ApplyClose,
    Copy,
}
fn section_allows_effect(section: Section, kind: EffectKind) -> bool {
    match section {
        Section::Modify => true,
        Section::Templates | Section::SavedPipelines => kind == EffectKind::Apply,
        _ => false,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct EffectReceipt {
    source: u64,
    stages: u64,
    preview: multi_launcher::clipboard_modify::coordinator::OperationId,
    section_revision: u64,
}
struct Review {
    id: u64,
    effect: EffectKind,
    receipt: EffectReceipt,
    review: Arc<ClipboardReview>,
}
fn held_activation(held: bool, modifiers: Modifiers, key: &str) -> bool {
    held && modifiers == Modifiers::default() && matches!(key, "enter" | "space")
}
fn current_completion(pending: Option<u64>, serial: u64) -> bool {
    pending == Some(serial)
}
fn rehost_idle(open: bool, rehosting: bool, reading: bool, effect: bool, management: bool, same_mode: bool) -> bool {
    open && !rehosting && !reading && !effect && !management && !same_mode
}
fn configuration_reload_blocked(rehosting: bool, effect: bool, review: bool, management: bool, pipeline_review: bool) -> bool {
    rehosting || effect || review || management || pipeline_review
}
fn queued_read_can_start(queued: Option<bool>, reading: bool, runtime_ready: bool) -> bool {
    queued.is_some() && !reading && runtime_ready
}
fn dock_reveal_is_current(
    expected: &utility_mount::MountReceipt,
    mounted: &utility_mount::MountReceipt,
    popout: bool,
) -> bool {
    !popout && expected.is_current() && expected.matches(mounted)
}
fn pump_is_current(
    epoch: u64,
    live_epoch: u64,
    open: bool,
    receipt: &utility_mount::MountReceipt,
    mounted: &utility_mount::MountReceipt,
) -> bool {
    epoch == live_epoch && open && receipt.is_current() && receipt.matches(mounted)
}
fn consent_is_current(expected: u64, current: u64, bytes: usize) -> bool {
    expected == current && bytes > LARGE_SOURCE
}
fn read_result_current(
    open: bool,
    receipt: &utility_mount::MountReceipt,
    mounted: &utility_mount::MountReceipt,
    request: u64,
    current_request: u64,
    source: u64,
    current_source: u64,
) -> bool {
    open && receipt.is_current()
        && receipt.matches(mounted)
        && request == current_request
        && source == current_source
}
enum ReadWork {
    NotStarted,
    Attempted(Result<String, String>),
}
fn deferred_read_after_worker(
    initial: bool,
    not_started: bool,
    exact: bool,
    queued: Option<bool>,
) -> Option<bool> {
    if !not_started || !exact {
        return None;
    }
    if initial && queued != Some(false) {
        Some(true)
    } else if !initial && queued.is_none() {
        Some(false)
    } else {
        None
    }
}
fn effective_read_intent(initial: bool, queued: Option<bool>) -> bool {
    initial && queued != Some(false)
}
fn begin_read_intent(initial: bool, queued: &mut Option<bool>) -> bool {
    let effective = effective_read_intent(initial, *queued);
    *queued = None;
    effective
}
pub(super) fn current_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|enabled| enabled.contains("clipboard_modify"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "clipboard_modify")
        && multi_launcher::plugins::clipboard_modify::bare_modify_opener(query).as_ref()
            == Some(action)
        && matches!(
            parse_action(action),
            Ok(Command::ClipboardModify(multi_launcher::commands::ClipboardModifyCommand::Open { section: multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::Modify | multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::Templates | multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::SavedPipelines | multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::ManageTemplates | multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::ManagePipelines | multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::Help }))
        )
}
pub(super) fn current_undo_provider(
    query: &str,
    action: &Action,
    enabled: Option<&HashSet<String>>,
    caps: Option<&HashMap<String, Vec<String>>>,
) -> bool {
    enabled.is_none_or(|enabled| enabled.contains("clipboard_modify"))
        && multi_launcher::settings::provider_search_capability_enabled(caps, "clipboard_modify")
        && multi_launcher::plugins::clipboard_modify::bare_undo_action(query).as_ref()
            == Some(action)
        && matches!(
            parse_action(action),
            Ok(Command::ClipboardModify(
                multi_launcher::commands::ClipboardModifyCommand::Undo { .. }
            ))
        )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestartDraft {
    source: String,
    stages: Vec<StageSpec>,
    templates: Option<manage::DraftSnapshot>,
    pipelines: Option<pipelines::DraftSnapshot>,
}
struct StageEditor {
    id: u64,
    spec: StageSpec,
    prefix: Entity<TextareaState>,
    suffix: Entity<TextareaState>,
    name: Entity<TextareaState>,
    language: Entity<TextareaState>,
    separator: Entity<TextareaState>,
    _subscriptions: Vec<Subscription>,
}
#[derive(Clone, Copy)]
enum Arg {
    Prefix,
    Suffix,
    Name,
    Language,
    Separator,
}
#[derive(Clone, Copy)]
enum StageOwner {
    Modify,
    Pipeline(u64),
}
fn stage_receipt_current(
    owner: StageOwner,
    section: Section,
    expected_revision: u64,
    current_revision: u64,
    mounted: bool,
) -> bool {
    mounted
        && current_revision == expected_revision
        && section
            == match owner {
                StageOwner::Modify => Section::Modify,
                StageOwner::Pipeline(_) => Section::ManagePipelines,
            }
}
pub(super) struct ClipboardModifyView {
    pub open: bool,
    pub(super) popout: bool,
    pub(super) rehosting: bool,
    rehost_readonly: bool,
    pending_dock_reveal: Option<utility_mount::MountReceipt>,
    pub(super) presentation_error: Option<String>,
    pub(super) geometry_error: Option<String>,
    pub(super) host: AnyWindowHandle,
    native: Option<NativeWindow>,
    activation_revision: Arc<AtomicU64>,
    pub(super) root: WeakEntity<RootWindow>,
    pub(super) admission: Arc<RestartAdmission>,
    owner: utility_mount::MountOwner,
    mount: utility_mount::MountReceipt,
    focus: FocusHandle,
    review_focus: FocusHandle,
    review_cancel_focus: FocusHandle,
    apply_focus: FocusHandle,
    apply_close_focus: FocusHandle,
    copy_focus: FocusHandle,
    effect_epoch: Arc<AtomicU64>,
    effect_pending: Option<u64>,
    effect_serial: u64,
    review: Option<Review>,
    review_serial: u64,
    source: Entity<TextareaState>,
    reset_source: String,
    source_revision: u64,
    read_revision: u64,
    read_authority: Arc<AtomicU64>,
    reading: bool,
    initial_read_started: bool,
    active_read: Option<u64>,
    queued_read: Option<bool>,
    resolved: bool,
    reset_resolved: bool,
    stages: Vec<StageEditor>,
    next_stage: u64,
    picker: Option<u64>,
    stage_revision: u64,
    section: Section,
    section_revision: u64,
    sections: sections::SectionState,
    management: manage::Management,
    pipelines: pipelines::PipelineManagement,
    catalog: Option<Arc<ClipboardModifierCatalog>>,
    runtime: Option<ClipboardModifyRuntime>,
    preview: PreviewCoordinator,
    pumping: bool,
    pump_epoch: u64,
    wrap: bool,
    large_allowed: bool,
    catalog_feedback: Option<String>,
    feedback: Option<String>,
    _source_sub: Subscription,
}
impl ClipboardModifyView {
    pub(super) fn configuration_reload_blocked(&self) -> bool {
        configuration_reload_blocked(self.rehosting, self.effect_pending.is_some(), self.review.is_some(),
            self.management.pending.is_some(), self.pipelines.review.is_some())
    }
    pub(super) fn adopt_configuration_reload(&mut self, catalog: Arc<ClipboardModifierCatalog>, cx: &mut Context<Self>) {
        if self.runtime.as_ref().is_none_or(|runtime| !clipboard_modify_startup::catalog_is_current(runtime, &catalog)) { return; }
        self.catalog = Some(catalog);
        self.catalog_feedback = None;
        self.sections.template = None;
        self.sections.pipeline = None;
        self.preview.cancel_and_invalidate();
        cx.notify();
    }
    pub(super) fn mount_receipt(&self) -> utility_mount::MountReceipt {
        self.mount.clone()
    }
    pub(super) fn focus_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.sync_editor_readonly(cx);
        if self.pipelines.review.is_some() {
            self.pipelines.focus_review(window, cx);
        } else if self.review.is_some() {
            self.review_cancel_focus.focus(window, cx);
        } else {
            self.focus_section(window, cx);
        }
    }
    pub(super) fn contains_focus(&self, window: &Window, cx: &App) -> bool {
        let selected = if self.pipelines.review.is_some() {
            self.pipelines.review_cancel_is_focused(window)
        } else if self.review.is_some() {
            self.review_cancel_focus.is_focused(window)
        } else {
            match self.section {
                Section::Modify => self.source.focus_handle(cx).is_focused(window),
                Section::Templates => self.sections.template_filter.focus_handle(cx).is_focused(window),
                Section::SavedPipelines => self.sections.pipeline_filter.focus_handle(cx).is_focused(window),
                Section::Help => self.sections.help_filter.focus_handle(cx).is_focused(window),
                Section::ManageTemplates | Section::ManagePipelines => self.focus.is_focused(window),
            }
        };
        selected || self.focus.is_focused(window) || self.focus.contains_focused(window, cx)
    }
    pub(super) fn after_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !window.is_window_active() || host::HostAuthority::capture(self, window, cx).is_none() {
            return;
        }
        self.focus_host(window, cx);
        self.wake_pending_read(window, cx);
    }
    pub(super) fn wake_pending_read(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.wake_dock_reveal(window, cx);
        if !window.is_window_active() || self.rehosting || host::HostAuthority::capture(self, window, cx).is_none() {
            return;
        }
        if self.runtime.is_none() {
            let ready = self.root.upgrade().and_then(|root| {
                let root = root.read(cx);
                root.clipboard_modify_startup.runtime.clone()
                    .map(|runtime| (runtime, root.clipboard_modify_startup.diagnostic.clone()))
            });
            if let Some((runtime, diagnostic)) = ready {
                self.install_startup(runtime, diagnostic, window, cx);
            }
        }
        if queued_read_can_start(self.queued_read, self.reading, self.runtime.is_some()) {
            if let Some(initial) = self.queued_read.take() {
                self.read_source(initial, window, cx);
            }
        }
    }
    fn queue_dock_reveal(&mut self, window: &Window, cx: &mut Context<Self>) {
        if !self.popout {
            self.pending_dock_reveal = Some(self.mount.clone());
            self.wake_dock_reveal(window, cx);
        }
    }
    fn wake_dock_reveal(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some(receipt) = self.pending_dock_reveal.as_ref() else { return; };
        if !dock_reveal_is_current(receipt, &self.mount, self.popout) {
            self.pending_dock_reveal = None;
            return;
        }
        if self.reveal_current_dock(window, cx) {
            self.pending_dock_reveal = None;
        }
    }
    fn reveal_current_dock(&self, window: &Window, cx: &mut Context<Self>) -> bool {
        if self.popout || !window.is_window_active()
            || host::HostAuthority::capture(self, window, cx).is_none()
        {
            return false;
        }
        let id = window.window_handle().window_id();
        self.root.update(cx, |root, cx| {
            let history = root.clipboard_history.as_ref()
                .is_some_and(|view| view.read(cx).is_mounted_in(id, false));
            let item = root.clipboard_scroll_item(id, cx) + usize::from(history);
            root.root_scroll.scroll_to_top_of_item(item);
            cx.notify();
        }).is_ok()
    }
    fn request_rehost(&mut self, popout: bool, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !rehost_idle(self.open, self.rehosting, self.reading, self.effect_pending.is_some(), self.management.pending.is_some(), self.popout == popout) || window.has_active_dialog(cx)
            || !window.is_window_active() || host::HostAuthority::capture(self, window, cx).is_none() { return false; }
        let Some(native) = self.native else { return false; };
        let Some(root) = self.root.upgrade() else { return false; };
        let visibility_revision = root.read(cx).root_visibility.request_revision.load(Ordering::Acquire);
        let gesture = Some(hash_dock::Gesture { source: native, foreground_authorized: true, visibility_revision });
        let receipt = self.mount.clone();
        self.rehosting = true;
        self.pending_dock_reveal = None;
        self.rehost_readonly = true;
        self.sync_editor_readonly(cx);
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        self.focus.focus(window, cx);
        let root = self.root.clone();
        cx.spawn(async move |_, cx| { let _ = root.update(cx, |root, cx| root.start_clipboard_modify_rehost(popout, gesture, receipt, cx)); }).detach();
        cx.notify();
        true
    }
    pub(super) fn shutdown_host(&mut self) {
        self.open = false;
        self.pending_dock_reveal = None;
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        self.owner.retire();
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        self.effect_epoch.fetch_add(1, Ordering::AcqRel);
        self.queued_read = None;
        self.initial_read_started = false;
        self.resolved = false;
        self.reset_resolved = false;
        self.reset_source.clear();
        self.stages.clear();
        self.management.clear();
        self.pipelines.clear();
        self.review = None;
        self.picker = None;
        self.large_allowed = false;
        self.feedback = None;
        self.preview.cancel_and_invalidate();
        self.pump_epoch = self.pump_epoch.wrapping_add(1);
        self.pumping = false;
    }
    pub(super) fn is_mounted_in(&self, id: WindowId, popout: bool) -> bool {
        self.open && self.popout == popout && self.mount.is_current() && self.host.window_id() == id
    }
    pub(super) fn replace_mount(&mut self, host: AnyWindowHandle, native: Option<NativeWindow>, window: &mut Window, cx: &mut Context<Self>) {
        let source = self.source.read(cx).value().to_string();
        let stages: Vec<_> = self.stages.iter().map(|stage| (stage.id, stage.spec.clone())).collect();
        let was_pumping = self.pumping;
        self.pump_epoch = self.pump_epoch.wrapping_add(1);
        self.pumping = false;
        self.mount = self.owner.attach();
        self.host = host;
        self.native = native;
        self.focus = cx.focus_handle();
        self.review_focus = cx.focus_handle();
        self.review_cancel_focus = cx.focus_handle();
        self.apply_focus = cx.focus_handle();
        self.apply_close_focus = cx.focus_handle();
        self.copy_focus = cx.focus_handle();
        self.source = cx.new(|cx| TextareaState::new(window, cx).auto_grow(6, 14).placeholder("Source text"));
        self.source.update(cx, |input, cx| input.set_value(source, window, cx));
        self._source_sub = Self::subscribe_source(&self.source, self.mount.clone(), self.section_revision, cx);
        self.stages = stages.into_iter().map(|(id, spec)| self.build_stage_for(StageOwner::Modify, id, spec, window, cx)).collect();
        self.sections.remount(window, cx);
        manage::remount(self, window, cx);
        pipelines::remount(self, window, cx);
        self.sync_editor_readonly(cx);
        self.focus.focus(window, cx);
        if was_pumping { self.pump(cx); }
        cx.notify();
    }
    fn defer_initial_read(&mut self, cx: &mut Context<Self>) {
        if !self.open || !self.mount.is_current() || self.initial_read_started {
            return;
        }
        if self.queued_read.is_none() {
            self.queued_read = Some(true);
        }
        self.feedback =
            Some("Clipboard was not loaded; activate this window and reopen the section.".into());
        cx.notify();
    }
    fn defer_explicit_read(&mut self, cx: &mut Context<Self>) {
        if !self.open || !self.mount.is_current() {
            return;
        }
        self.queued_read = Some(false);
        self.feedback = Some(
            "Clipboard reload is waiting; activate this window and reopen the section.".into(),
        );
        cx.notify();
    }
    pub(super) fn startup_failed(&mut self, cx: &mut Context<Self>) {
        self.queued_read = None;
        self.feedback = Some("Clipboard Modify could not be initialized.".into());
        cx.notify();
    }
    pub(super) fn startup_delivery_failed(&mut self, cx: &mut Context<Self>) {
        if self.open {
            self.feedback =
                Some("Clipboard Modify is ready; reopen the section to continue.".into());
        }
        cx.notify();
    }
    pub(super) fn install_startup(
        &mut self,
        runtime: ClipboardModifyRuntime,
        diagnostic: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.runtime.is_some() {
            return;
        }
        self.catalog = Some(runtime.catalog_snapshot());
        self.runtime = Some(runtime);
        self.catalog_feedback = diagnostic;
        if self.feedback.as_deref() == Some("Loading Clipboard Modify settings…") {
            self.feedback = None;
        }
        if self.open {
            if self.section == Section::ManageTemplates {
                manage::ensure_templates(self, window, cx);
            }
            if self.section == Section::ManagePipelines {
                pipelines::ensure(self, window, cx);
            }
            if self.resolved {
                self.schedule(cx);
            }
            if !self.rehosting { if let Some(initial) = self.queued_read.take() {
                let receipt = self.mount.clone();
                let source_revision = self.source_revision;
                let weak = cx.entity().downgrade();
                window.defer(cx, move |window, app| {
                    let _ = weak.update(app, |view, cx| {
                        if !view.open
                            || !receipt.is_current()
                            || !receipt.matches(&view.mount)
                            || view.source_revision != source_revision
                        {
                            return;
                        }
                        if !window.is_window_active() {
                            if initial {
                                view.defer_initial_read(cx);
                            } else {
                                view.defer_explicit_read(cx);
                            }
                            return;
                        }
                        view.read_source(initial, window, cx);
                    });
                });
            } }
        }
        cx.notify();
    }
    fn subscribe_source(
        source: &Entity<TextareaState>,
        receipt: utility_mount::MountReceipt,
        section_revision: u64,
        cx: &mut Context<Self>,
    ) -> Subscription {
        cx.subscribe(
            source,
            move |view: &mut Self, input: Entity<TextareaState>, event: &InputEvent, cx| {
                if receipt.is_current()
                    && receipt.matches(&view.mount)
                    && view.open
                    && view.section == Section::Modify
                    && view.section_revision == section_revision
                    && input.entity_id() == view.source.entity_id()
                    && matches!(event, InputEvent::Change)
                {
                    view.retire_effect_for_edit(cx);
                    view.source_revision = view.source_revision.wrapping_add(1);
                    view.read_revision = view.read_revision.wrapping_add(1);
                    view.read_authority.fetch_add(1, Ordering::AcqRel);
                    view.queued_read = None;
                    view.resolved = true;
                    view.large_allowed = false;
                    view.schedule(cx);
                }
            },
        )
    }
    fn subscribe_stage(
        owner: StageOwner,
        id: u64,
        inputs: [&Entity<TextareaState>; 5],
        receipt: utility_mount::MountReceipt,
        section_revision: u64,
        cx: &mut Context<Self>,
    ) -> Vec<Subscription> {
        inputs
            .into_iter()
            .zip([
                Arg::Prefix,
                Arg::Suffix,
                Arg::Name,
                Arg::Language,
                Arg::Separator,
            ])
            .map(|(input, arg)| {
                let receipt = receipt.clone();
                cx.subscribe(
                    input,
                    move |view: &mut Self, input: Entity<TextareaState>, event: &InputEvent, cx| {
                        if !view.open
                            || !stage_receipt_current(
                                owner,
                                view.section,
                                section_revision,
                                view.section_revision,
                                receipt.is_current() && receipt.matches(&view.mount),
                            )
                            || !matches!(event, InputEvent::Change)
                        {
                            return;
                        }
                        let value = input.read(cx).value().to_string();
                        let stage = match owner {
                            StageOwner::Modify => view.stages.iter().find(|stage| stage.id == id),
                            StageOwner::Pipeline(row) => view.pipelines.stage(row, id),
                        };
                        let Some(stage) = stage else {
                            return;
                        };
                        let current = match arg {
                            Arg::Prefix => &stage.prefix,
                            Arg::Suffix => &stage.suffix,
                            Arg::Name => &stage.name,
                            Arg::Language => &stage.language,
                            Arg::Separator => &stage.separator,
                        };
                        if current.entity_id() != input.entity_id() {
                            return;
                        }
                        view.retire_effect_for_edit(cx);
                        let stage = match owner {
                            StageOwner::Modify => {
                                view.stages.iter_mut().find(|stage| stage.id == id)
                            }
                            StageOwner::Pipeline(row) => view.pipelines.stage_mut(row, id),
                        };
                        let Some(stage) = stage else {
                            return;
                        };
                        let arguments = &mut stage.spec.arguments;
                        let field = match arg {
                            Arg::Prefix => &mut arguments.prefix,
                            Arg::Suffix => &mut arguments.suffix,
                            Arg::Name => &mut arguments.name,
                            Arg::Language => &mut arguments.language,
                            Arg::Separator => &mut arguments.separator,
                        };
                        *field = Some(value);
                        match owner {
                            StageOwner::Modify => view.changed(cx),
                            StageOwner::Pipeline(_) => {
                                view.management.authority.fetch_add(1, Ordering::AcqRel);
                                view.pipelines.changed();
                                view.preview.cancel_and_invalidate();
                                cx.notify();
                            }
                        }
                    },
                )
            })
            .collect()
    }
    pub(super) fn retire_read_for_effect(&mut self) {
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        self.queued_read = None;
    }
    pub(super) fn launcher_effect_settled(&mut self, cx: &mut Context<Self>) {
        if self.feedback.as_deref() == Some(LAUNCHER_EFFECT_WAIT) {
            self.feedback = None;
        }
        cx.notify();
    }
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>, native: Option<NativeWindow>, admission: Arc<RestartAdmission>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let source = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 14)
                .placeholder("Source text")
        });
        let source_sub = Self::subscribe_source(&source, mount.clone(), 0, cx);
        Self {
            open: false,
            popout: false,
            rehosting: false,
            rehost_readonly: false,
            pending_dock_reveal: None,
            presentation_error: None,
            geometry_error: None,
            host: window.window_handle(),
            native,
            activation_revision: Arc::new(AtomicU64::new(0)),
            root,
            admission,
            owner,
            mount,
            focus: cx.focus_handle(),
            review_focus: cx.focus_handle(),
            review_cancel_focus: cx.focus_handle(),
            apply_focus: cx.focus_handle(),
            apply_close_focus: cx.focus_handle(),
            copy_focus: cx.focus_handle(),
            effect_epoch: Arc::new(AtomicU64::new(0)),
            effect_pending: None,
            effect_serial: 0,
            review: None,
            review_serial: 0,
            source,
            reset_source: String::new(),
            source_revision: 0,
            read_revision: 0,
            read_authority: Arc::new(AtomicU64::new(0)),
            reading: false,
            initial_read_started: false,
            active_read: None,
            queued_read: None,
            resolved: false,
            reset_resolved: false,
            stages: Vec::new(),
            next_stage: 0,
            picker: None,
            stage_revision: 0,
            section: Section::Modify,
            section_revision: 0,
            sections: sections::SectionState::new(window, cx),
            management: manage::Management::default(),
            pipelines: pipelines::PipelineManagement::new(cx),
            catalog: None,
            runtime: None,
            preview: PreviewCoordinator::default(),
            pumping: false,
            pump_epoch: 0,
            wrap: true,
            large_allowed: false,
            catalog_feedback: None,
            feedback: None,
            _source_sub: source_sub,
        }
    }
    fn reopen(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.open {
            if self.pipelines.review.is_some() {
                self.pipelines.focus_review(window, cx);
            } else if self.review.is_some() && self.effect_pending.is_none() {
                self.review_cancel_focus.focus(window, cx);
            } else if self.effect_pending.is_some() {
                self.focus.focus(window, cx);
            } else {
                self.focus_section(window, cx);
            }
            return;
        }
        if !self.mount.is_current() {
            self.mount = self.owner.attach();
            let source = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(6, 14)
                    .placeholder("Source text")
            });
            self._source_sub =
                Self::subscribe_source(&source, self.mount.clone(), self.section_revision, cx);
            self.source = source;
            self.focus = cx.focus_handle();
            self.review_focus = cx.focus_handle();
            self.review_cancel_focus = cx.focus_handle();
            self.apply_focus = cx.focus_handle();
            self.apply_close_focus = cx.focus_handle();
            self.copy_focus = cx.focus_handle();
        }
        self.open = true;
        self.sync_editor_readonly(cx);
        self.focus_section(window, cx);
    }
    fn schedule(&mut self, cx: &mut Context<Self>) {
        if !self.resolved {
            self.preview.cancel_and_invalidate();
            cx.notify();
            return;
        }
        let text = self.source.read(cx).value().to_string();
        let Some(catalog) = self.catalog.clone() else {
            self.preview.cancel_and_invalidate();
            cx.notify();
            return;
        };
        if text.len() > LARGE_SOURCE && !self.large_allowed {
            self.preview.cancel_and_invalidate();
            cx.notify();
            return;
        }
        let intent = match self.section {
            Section::Modify => Some(ClipboardModifyIntent::Stages(
                self.stages.iter().map(|stage| stage.spec.clone()).collect(),
            )),
            Section::Templates => self
                .sections
                .template
                .as_ref()
                .map(|name| ClipboardModifyIntent::ApplyTemplate { name: name.clone() }),
            Section::SavedPipelines => self
                .sections
                .pipeline
                .as_ref()
                .map(|name| ClipboardModifyIntent::ApplySavedPipeline { name: name.clone() }),
            _ => None,
        };
        let Some(intent) = intent else {
            self.preview.cancel_and_invalidate();
            cx.notify();
            return;
        };
        self.preview.request(text, intent, catalog);
        self.pump(cx);
        cx.notify();
    }
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.stage_revision = self.stage_revision.wrapping_add(1);
        self.schedule(cx);
    }
    fn pump(&mut self, cx: &mut Context<Self>) {
        if self.pumping {
            return;
        }
        self.pumping = true;
        self.pump_epoch = self.pump_epoch.wrapping_add(1);
        let epoch = self.pump_epoch;
        let receipt = self.mount.clone();
        cx.spawn(async move |view, cx| {
            loop {
                cx.background_executor()
                    .timer(Duration::from_millis(40))
                    .await;
                let more = view
                    .update(cx, |view, cx| {
                        if !pump_is_current(
                            epoch,
                            view.pump_epoch,
                            view.open,
                            &receipt,
                            &view.mount,
                        ) {
                            return false;
                        }
                        view.preview.tick();
                        let more = view.preview.is_active();
                        if !more {
                            view.pumping = false;
                        }
                        cx.notify();
                        more
                    })
                    .unwrap_or(false);
                if !more {
                    break;
                }
            }
        })
        .detach();
    }
    fn read_source(&mut self, initial: bool, window: &mut Window, cx: &mut Context<Self>) {
        let initial = effective_read_intent(initial, self.queued_read);
        if !self.open || (initial && self.initial_read_started) || !self.mount.is_current() {
            return;
        }
        if self.blocked() || window.has_active_dialog(cx) || !window.is_window_active() {
            if initial {
                self.defer_initial_read(cx);
            } else {
                self.defer_explicit_read(cx);
            }
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root_state = root.read(cx);
        if root_state.external_launch.clipboard_effect_pending() {
            if initial {
                self.defer_initial_read(cx);
            } else {
                self.defer_explicit_read(cx);
            }
            self.feedback = Some(LAUNCHER_EFFECT_WAIT.into());
            cx.notify();
            return;
        }
        if self.runtime.is_none() {
            if root_state.clipboard_modify_startup.pending {
                if initial {
                    if self.queued_read.is_none() {
                        self.queued_read = Some(true);
                    }
                } else {
                    self.queued_read = Some(false);
                }
                self.feedback = Some("Loading Clipboard Modify settings…".into());
            } else {
                self.feedback = Some("Clipboard Modify could not be initialized.".into());
            }
            cx.notify();
            return;
        }
        if !root_state
            .clipboard_modify_view
            .as_ref()
            .is_some_and(|current| current.entity_id() == cx.entity().entity_id())
        {
            return;
        }
        let Some(host_authority) = host::HostAuthority::capture(self, window, cx) else {
            if initial {
                self.defer_initial_read(cx);
            } else {
                self.defer_explicit_read(cx);
            }
            return;
        };
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        if self.reading {
            if initial {
                if self.queued_read.is_none() {
                    self.queued_read = Some(true);
                }
            } else {
                self.queued_read = Some(false);
            }
            return;
        }
        let admission = root_state.restart_admission.clone();
        let Some(permit) = admission.admit() else {
            if initial {
                self.defer_initial_read(cx);
            } else {
                self.defer_explicit_read(cx);
            }
            return;
        };
        let initial = begin_read_intent(initial, &mut self.queued_read);
        self.initial_read_started = true;
        let mount = self.mount.clone();
        let request = self.read_revision;
        let read_authority = self.read_authority.clone();
        let expected_authority = read_authority.load(Ordering::Acquire);
        let source_revision = self.source_revision;
        let publication_host = host_authority.clone();
        let worker_host = host_authority.clone();
        self.reading = true;
        self.active_read = Some(request);
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            let result = if read_authority.load(Ordering::Acquire) == expected_authority
                && worker_host.current()
            {
                ReadWork::Attempted(
                    runtime::clipboard_service()
                        .read_text_for_modify_once()
                        .map_err(redacted_read_error),
                )
            } else {
                ReadWork::NotStarted
            };
            result
        });
        let host = host_authority.host;
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let not_started = matches!(&result, ReadWork::NotStarted);
            let Some(view) = weak.upgrade() else {
                return;
            };
            let delivery_mount = mount.clone();
            let result_update = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if view.active_read != Some(request) {
                    return;
                }
                view.active_read = None;
                view.reading = false;
                view.clear_block_wait(cx);
                let exact = read_result_current(
                    view.open,
                    &delivery_mount,
                    &view.mount,
                    request,
                    view.read_revision,
                    source_revision,
                    view.source_revision,
                );
                let rearmed =
                    deferred_read_after_worker(initial, not_started, exact, view.queued_read);
                if let Some(initial) = rearmed {
                    if initial {
                        view.initial_read_started = false;
                        view.defer_initial_read(cx);
                    } else {
                        view.defer_explicit_read(cx);
                    }
                }
                let current = exact && window.is_window_active() && publication_host.current();
                if current {
                    match result {
                        ReadWork::NotStarted => {}
                        ReadWork::Attempted(Ok(text)) => {
                            view.reset_source = text.clone();
                            view.reset_resolved = true;
                            view.resolved = true;
                            view.large_allowed = false;
                            view.source_revision = view.source_revision.wrapping_add(1);
                            view.source
                                .update(cx, |input, cx| input.set_value(text, window, cx));
                            if initial {
                                if view.section == Section::Modify {
                                    view.preview.cancel_and_invalidate();
                                    cx.notify();
                                } else {
                                    view.schedule(cx);
                                }
                            } else {
                                view.schedule(cx);
                            }
                        }
                        ReadWork::Attempted(Err(error)) => {
                            view.feedback = Some(error);
                            cx.notify();
                        }
                    }
                }
                if rearmed.is_none() {
                    if let Some(next_initial) = view.queued_read.take() {
                        if view.open {
                            view.read_source(next_initial, window, cx);
                        }
                    }
                }
                cx.notify();
            });
            if result_update.is_err() {
                let _ = view.update(cx, |view, cx| {
                    if view.active_read == Some(request) {
                        view.active_read = None;
                        view.reading = false;
                        view.clear_block_wait(cx);
                        let exact = read_result_current(
                            view.open,
                            &mount,
                            &view.mount,
                            request,
                            view.read_revision,
                            source_revision,
                            view.source_revision,
                        );
                        if let Some(initial) = deferred_read_after_worker(
                            initial,
                            not_started,
                            exact,
                            view.queued_read,
                        ) {
                            if initial {
                                view.initial_read_started = false;
                                view.defer_initial_read(cx);
                            } else {
                                view.defer_explicit_read(cx);
                            }
                        } else if view.open {
                            view.feedback = Some("Clipboard read could not be delivered.".into());
                        }
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
    fn add_stage(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(op) = catalog::operations()
            .iter()
            .find(|op| op.pipeline_available && op.id == OperationId::Uppercase)
            .map(|op| op.id)
        else {
            return;
        };
        self.next_stage = self.next_stage.wrapping_add(1);
        let id = self.next_stage;
        let stage = self.build_stage(id, op, window, cx);
        self.stages.push(stage);
        self.changed(cx);
    }
    fn build_stage(
        &self,
        id: u64,
        op: OperationId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> StageEditor {
        self.build_stage_for(
            StageOwner::Modify,
            id,
            StageSpec {
                operation: op,
                arguments: StageArguments::default(),
            },
            window,
            cx,
        )
    }
    fn build_stage_for(
        &self,
        owner: StageOwner,
        id: u64,
        spec: StageSpec,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> StageEditor {
        let prefix = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .placeholder("Prefix")
        });
        let suffix = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .placeholder("Suffix")
        });
        let name = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .placeholder("Name")
        });
        let language = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .placeholder("Language")
        });
        let separator = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(1, 4)
                .placeholder("Separator")
        });
        for (input, value) in [
            (&prefix, &spec.arguments.prefix),
            (&suffix, &spec.arguments.suffix),
            (&name, &spec.arguments.name),
            (&language, &spec.arguments.language),
            (&separator, &spec.arguments.separator),
        ] {
            input.update(cx, |input, cx| {
                input.set_value(value.clone().unwrap_or_default(), window, cx)
            });
        }
        let inputs = [&prefix, &suffix, &name, &language, &separator];
        let subs = Self::subscribe_stage(
            owner,
            id,
            inputs,
            self.mount.clone(),
            self.section_revision,
            cx,
        );
        StageEditor {
            id,
            spec,
            prefix,
            suffix,
            name,
            language,
            separator,
            _subscriptions: subs,
        }
    }
    fn clear_block_wait(&self, cx: &mut Context<Self>) {
        self.sync_editor_readonly(cx);
        if self.blocks_close() {
            return;
        }
        let _ = self.root.update(cx, |root, cx| {
            if root.restart_status.as_deref() == Some(BLOCK_WAIT) {
                root.restart_status = None;
                cx.notify();
            }
        });
    }
    pub(super) fn blocks_close(&self) -> bool {
        self.effect_pending.is_some()
            || self.review.is_some()
            || self.management.pending.is_some()
            || self.pipelines.review.is_some()
    }
    fn blocked(&self) -> bool {
        self.rehosting || self.effect_pending.is_some()
            || self.review.is_some()
            || self.management.pending.is_some()
            || self.pipelines.review.is_some()
    }
    fn sync_editor_readonly(&self, cx: &mut Context<Self>) {
        let readonly = self.blocked();
        let modify_readonly = readonly || self.section != Section::Modify;
        self.source
            .update(cx, |input, cx| input.set_readonly(modify_readonly, cx));
        for input in [
            &self.sections.template_filter,
            &self.sections.pipeline_filter,
            &self.sections.help_filter,
        ] {
            input.update(cx, |input, cx| input.set_readonly(readonly, cx));
        }
        for stage in &self.stages {
            for input in [
                &stage.prefix,
                &stage.suffix,
                &stage.name,
                &stage.language,
                &stage.separator,
            ] {
                input.update(cx, |input, cx| input.set_readonly(modify_readonly, cx));
            }
        }
        self.management.set_readonly(
            self.blocked() || self.section != Section::ManageTemplates,
            cx,
        );
        self.pipelines.set_readonly(
            self.blocked() || self.section != Section::ManagePipelines,
            cx,
        );
    }
    fn retire_effect_for_edit(&mut self, cx: &mut Context<Self>) {
        self.effect_epoch.fetch_add(1, Ordering::AcqRel);
        if self.review.take().is_some() {
            self.review_serial = self.review_serial.wrapping_add(1);
            if self.effect_pending.is_none() {
                cx.spawn(async move |weak, cx| {
                    let _ = weak.update(cx, |view, cx| view.clear_block_wait(cx));
                })
                .detach();
            }
        }
    }
    fn effect_receipt(&self) -> Option<EffectReceipt> {
        match self.preview.state() {
            PreviewState::Completed { id, .. } if self.resolved => Some(EffectReceipt {
                source: self.source_revision,
                stages: self.stage_revision,
                preview: *id,
                section_revision: self.section_revision,
            }),
            _ => None,
        }
    }
    fn effect(
        &mut self,
        kind: EffectKind,
        expected: EffectReceipt,
        review_id: Option<u64>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.open
            || !section_allows_effect(self.section, kind)
            || !sections::selection_visible(self, cx)
            || self.effect_pending.is_some()
            || self.picker.is_some()
            || self.effect_receipt() != Some(expected)
            || !self.mount.is_current()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
        {
            return;
        }
        let review = match (review_id, &self.review) {
            (None, None) => None,
            (Some(id), Some(review))
                if review.id == id && review.effect == kind && review.receipt == expected =>
            {
                Some(review.review.clone())
            }
            _ => return,
        };
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root_state = root.read(cx);
        if root_state.external_launch.clipboard_effect_pending() {
            return;
        }
        let Some(host_authority) = host::HostAuthority::capture(self, window, cx) else {
            return;
        };
        let native = host_authority.native;
        let host = host_authority.host;
        let admission = root_state.restart_admission.clone();
        let Some(permit) = admission.admit() else {
            return;
        };
        let ack_host = host_authority.clone();
        let Some(output) = self.preview.full_output().map(str::to_owned) else {
            return;
        };
        let baseline = self.reset_source.clone();
        // A concurrently admitted Reload must not publish over this action.
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        self.queued_read = None;
        self.effect_serial = self.effect_serial.wrapping_add(1);
        let serial = self.effect_serial;
        self.effect_pending = Some(serial);
        self.feedback = None;
        self.sync_editor_readonly(cx);
        self.focus.focus(window, cx);
        let receipt = self.mount.clone();
        let worker_host = host_authority;
        let epoch = self.effect_epoch.clone();
        let expected_epoch = epoch.load(Ordering::Acquire);
        let task = cx.background_executor().spawn(async move {
            let current = || {
                epoch.load(Ordering::Acquire) == expected_epoch && worker_host.current()
            };
            let effect = match &review {
                Some(review) => CheckedClipboardEffect::ReviewedOverwrite { review },
                None if kind == EffectKind::Copy => CheckedClipboardEffect::CopyResult,
                None => CheckedClipboardEffect::Apply {
                    baseline: &baseline,
                },
            };
            runtime::clipboard_service().commit_dialog_checked(
                effect,
                &output,
                "Clipboard Modify",
                &current,
            )
        });
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let result = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let apply = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if !current_completion(view.effect_pending, serial) {
                    return;
                }
                let owns_focus =
                    view.focus.is_focused(window) || view.focus.contains_focused(window, cx);
                view.effect_pending = None;
                view.clear_block_wait(cx);
                if !receipt.is_current()
                    || !receipt.matches(&view.mount)
                    || view.effect_receipt() != Some(expected)
                {
                    cx.notify();
                    return;
                }
                match result {
                    Ok(CheckedClipboardOutcome::Written) => {
                        view.review = None;
                        view.feedback = Some("Clipboard updated.".into());
                        if kind == EffectKind::ApplyClose
                            && window.is_window_active()
                            && current_foreground_identity() == Some(native.identity)
                            && ack_host.current_for(view, window, cx)
                            && !window.has_active_dialog(cx)
                        {
                            view.close(window, cx);
                        }
                    }
                    Ok(CheckedClipboardOutcome::ReviewRequired(review)) => {
                        if !window.is_window_active()
                            || current_foreground_identity() != Some(native.identity)
                            || !ack_host.current_for(view, window, cx)
                            || window.has_active_dialog(cx)
                        {
                            view.review = None;
                            view.clear_block_wait(cx);
                            view.feedback = Some("Clipboard review expired. Try again.".into());
                            cx.notify();
                            return;
                        }
                        view.review_serial = view.review_serial.wrapping_add(1);
                        view.review = Some(Review {
                            id: view.review_serial,
                            effect: kind,
                            receipt: expected,
                            review: Arc::new(review),
                        });
                        if owns_focus {
                            let _ = view
                                .root
                                .update(cx, |root, _| root.root_scroll.scroll_to_bottom());
                        }
                        if owns_focus
                            && window.is_window_active()
                            && current_foreground_identity() == Some(native.identity)
                            && !window.has_active_dialog(cx)
                        {
                            view.review_cancel_focus.focus(window, cx);
                        }
                    }
                    Ok(CheckedClipboardOutcome::Superseded) => {
                        view.feedback =
                            Some("Clipboard operation was superseded. Try again.".into())
                    }
                    Err(error) => view.feedback = Some(redacted_read_error(error)),
                }
                view.clear_block_wait(cx);
                if view.open
                    && owns_focus
                    && window.is_window_active()
                    && current_foreground_identity() == Some(native.identity)
                    && ack_host.current_for(view, window, cx)
                    && !window.has_active_dialog(cx)
                {
                    if view.review.is_some() {
                        view.review_cancel_focus.focus(window, cx);
                    } else {
                        view.focus_section(window, cx);
                    }
                }
                cx.notify();
            });
            if apply.is_err() {
                let _ = view.update(cx, |view, cx| {
                    if view.effect_pending == Some(serial) {
                        view.effect_pending = None;
                        view.clear_block_wait(cx);
                        view.feedback = Some("Clipboard operation could not be delivered.".into());
                        cx.notify();
                    }
                });
            }
        })
        .detach();
        cx.notify();
    }
    pub(super) fn restart_draft(&self, cx: &App) -> Option<RestartDraft> {
        if !self.open {
            return None;
        }
        let source = self.source.read(cx).value().to_string();
        let templates = self.management.restart_draft(cx);
        let pipelines = self.pipelines.restart_draft(cx);
        (!source.is_empty()
            || !self.stages.is_empty()
            || templates.is_some()
            || pipelines.is_some())
        .then(|| RestartDraft {
            source,
            stages: self.stages.iter().map(|stage| stage.spec.clone()).collect(),
            templates,
            pipelines,
        })
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.blocked() || self.rehosting || !self.is_mounted_in(window.window_handle().window_id(), self.popout) {
            return;
        }
        let popout = self.popout;
        self.pending_dock_reveal = None;
        if popout { let _ = self.root.update(cx, |root, cx| root.drain_clipboard_modify_geometry(cx, true)); }
        self.activation_revision.fetch_add(1, Ordering::AcqRel);
        let owns_focus = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.initial_read_started = false;
        self.owner.retire();
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        self.queued_read = None;
        self.resolved = false;
        self.reset_resolved = false;
        self.reset_source.clear();
        self.stages.clear();
        self.management.clear();
        self.pipelines.clear();
        self.picker = None;
        self.large_allowed = false;
        self.feedback = None;
        self.source
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.preview.cancel_and_invalidate();
        self.pump_epoch = self.pump_epoch.wrapping_add(1);
        self.pumping = false;
        if popout { window.remove_window(); cx.notify(); return; }
        let _ = self.root.update(cx, |root, cx| {
            if owns_focus
                && window.is_window_active()
                && !window.has_active_dialog(cx)
                && root
                    .native_window
                    .is_some_and(|native| current_foreground_identity() == Some(native.identity))
            {
                root.query.focus_handle(cx).focus(window, cx);
            }
            cx.notify();
        });
        cx.notify();
    }
    fn select_section(&mut self, section: Section, window: &mut Window, cx: &mut Context<Self>) {
        if self.blocked() {
            return;
        }
        if self.section != section {
            self.section = section;
            self.section_revision = self.section_revision.wrapping_add(1);
            self.picker = None;
            if section == Section::Modify {
                self._source_sub = Self::subscribe_source(
                    &self.source,
                    self.mount.clone(),
                    self.section_revision,
                    cx,
                );
                for stage in &mut self.stages {
                    stage._subscriptions = Self::subscribe_stage(
                        StageOwner::Modify,
                        stage.id,
                        [
                            &stage.prefix,
                            &stage.suffix,
                            &stage.name,
                            &stage.language,
                            &stage.separator,
                        ],
                        self.mount.clone(),
                        self.section_revision,
                        cx,
                    );
                }
            }
            self.sync_editor_readonly(cx);
            self.schedule(cx);
        }
        if section == Section::ManageTemplates {
            manage::ensure_templates(self, window, cx);
            self.sync_editor_readonly(cx);
        }
        if section == Section::ManagePipelines {
            pipelines::ensure(self, window, cx);
            self.sync_editor_readonly(cx);
        }
        self.focus_section(window, cx);
        cx.notify();
    }
    fn focus_section(&self, window: &mut Window, cx: &mut Context<Self>) {
        match self.section {
            Section::Modify => self.source.focus_handle(cx).focus(window, cx),
            Section::Templates => self
                .sections
                .template_filter
                .focus_handle(cx)
                .focus(window, cx),
            Section::SavedPipelines => self
                .sections
                .pipeline_filter
                .focus_handle(cx)
                .focus(window, cx),
            Section::Help => self.sections.help_filter.focus_handle(cx).focus(window, cx),
            Section::ManageTemplates | Section::ManagePipelines => self.focus.focus(window, cx),
        }
    }
}
impl Render for ClipboardModifyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.rehost_readonly && !self.rehosting {
            self.rehost_readonly = false;
            self.sync_editor_readonly(cx);
            let mount = self.mount.clone();
            let weak = cx.entity().downgrade();
            window.defer(cx, move |window, app| {
                let _ = weak.update(app, |view, cx| {
                    if mount.is_current() && mount.matches(&view.mount) {
                        view.queue_dock_reveal(window, cx);
                        view.wake_pending_read(window, cx);
                    }
                });
            });
        }
        let mount = self.mount.clone();
        let launcher_effect_pending = self
            .root
            .upgrade()
            .is_some_and(|root| root.read(cx).external_launch.clipboard_effect_pending());
        let revision = self.stage_revision;
        let section_revision = self.section_revision;
        let key_mount = mount.clone();
        let mut panel = div()
            .id("clipboard-modify")
            .track_focus(&self.focus)
            .capture_key_down(cx.listener(move |view, event: &KeyDownEvent, window, cx| {
                if !key_mount.is_current()
                    || !key_mount.matches(&view.mount)
                    || !view.open
                    || window.default_prevented()
                    || window.has_active_dialog(cx)
                    || !window.is_window_active()
                {
                    return;
                }
                if host::HostAuthority::capture(view, window, cx).is_none() {
                    return;
                }
                let key = event.keystroke.key.as_str();
                let mods = event.keystroke.modifiers;
                if pipelines::handle_review_key(view, event, window, cx) {
                    return;
                }
                if view.review.is_some()
                    && (view.review_focus.is_focused(window)
                        || view.review_focus.contains_focused(window, cx))
                {
                    if mods == Modifiers::default() && key == "escape" {
                        window.prevent_default();
                        cx.stop_propagation();
                        if !event.is_held && view.effect_pending.is_none() {
                            view.review = None;
                            view.review_serial = view.review_serial.wrapping_add(1);
                            view.clear_block_wait(cx);
                            view.focus.focus(window, cx);
                            cx.notify();
                        }
                    } else if held_activation(event.is_held, mods, key) {
                        window.prevent_default();
                        cx.stop_propagation();
                    }
                    return;
                }
                if held_activation(event.is_held, mods, key)
                    && (view.apply_focus.is_focused(window)
                        || view.apply_close_focus.is_focused(window)
                        || view.copy_focus.is_focused(window)
                        || (view.section == Section::ManageTemplates
                            && !view.management.input_focused(window, cx))
                        || (view.section == Section::ManagePipelines
                            && !view.pipelines.input_focused(window, cx)))
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if view.blocked() || view.picker.is_some() {
                    return;
                }
                if key == "r"
                    && view.section == Section::Modify
                    && mods
                        == (Modifiers {
                            control: true,
                            shift: mods.shift,
                            ..Modifiers::default()
                        })
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held && view.source.read(cx).value() == view.reset_source {
                        view.read_source(false, window, cx);
                    }
                    return;
                }
                if key == "enter"
                    && section_allows_effect(
                        view.section,
                        if mods.shift {
                            EffectKind::ApplyClose
                        } else {
                            EffectKind::Apply
                        },
                    )
                    && mods
                        == (Modifiers {
                            control: true,
                            shift: mods.shift,
                            ..Modifiers::default()
                        })
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    if !event.is_held {
                        if let Some(receipt) = view.effect_receipt() {
                            view.effect(
                                if mods.shift {
                                    EffectKind::ApplyClose
                                } else {
                                    EffectKind::Apply
                                },
                                receipt,
                                None,
                                window,
                                cx,
                            );
                        }
                    }
                }
            }))
            .w_full()
            .flex()
            .flex_col()
            .gap_2()
            .p_3()
            .child(Label::new(match self.section {
                Section::Modify => "Clipboard Modify — Modify",
                Section::Templates => "Clipboard Modify — Templates",
                Section::SavedPipelines => "Clipboard Modify — Saved Pipelines",
                Section::Help => "Clipboard Modify — Help",
                Section::ManageTemplates => "Clipboard Modify — Manage Templates",
                Section::ManagePipelines => "Clipboard Modify — Manage Pipelines",
            }));
        let rehost_mount = mount.clone();
        panel = panel.child(Button::new("cm-rehost")
            .label(if self.popout { "Dock" } else { "Pop Out" })
            .disabled(self.rehosting || self.reading || self.effect_pending.is_some() || self.management.pending.is_some())
            .on_click(cx.listener(move |view, _, window, cx| {
                if rehost_mount.is_current() && rehost_mount.matches(&view.mount) {
                    let _ = view.request_rehost(!view.popout, window, cx);
                }
            })));
        if let Some(error) = &self.presentation_error { panel = panel.child(Label::new(error.clone())); }
        if let Some(error) = &self.geometry_error { panel = panel.child(Label::new(error.clone())); }
        if self.blocked() && self.management.pending.is_none() && self.pipelines.review.is_none() {
            if let Some(review) = &self.review {
                let id = review.id;
                let effect = review.effect;
                let expected = review.receipt;
                let confirm = mount.clone();
                let cancel = mount.clone();
                panel = panel.child(
                    div()
                        .track_focus(&self.review_focus)
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(Label::new(
                            "The clipboard changed. Replace its current contents?",
                        ))
                        .child(Label::new(format!(
                            "Original: {} bytes · Current: {} bytes",
                            review.review.baseline.bytes, review.review.current.bytes
                        )))
                        .child(
                            Button::new(SharedString::from(format!("cm-review-cancel-{id}")))
                                .label("Cancel")
                                .track_focus(&self.review_cancel_focus)
                                .disabled(self.effect_pending.is_some() || self.rehosting)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if cancel.is_current()
                                        && cancel.matches(&view.mount)
                                        && view.effect_pending.is_none()
                                        && !view.rehosting
                                        && view
                                            .review
                                            .as_ref()
                                            .is_some_and(|review| review.id == id)
                                        && !window.has_active_dialog(cx)
                                    {
                                        view.review = None;
                                        view.review_serial = view.review_serial.wrapping_add(1);
                                        view.clear_block_wait(cx);
                                        view.focus.focus(window, cx);
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new(SharedString::from(format!("cm-review-confirm-{id}")))
                                .label("Overwrite clipboard")
                                .disabled(self.effect_pending.is_some())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if confirm.is_current() && confirm.matches(&view.mount) {
                                        view.effect(effect, expected, Some(id), window, cx);
                                    }
                                })),
                        ),
                );
            }
            if self.effect_pending.is_some() {
                panel = panel.child(Label::new(EFFECT_WAIT));
            }
            if let Some(feedback) = &self.feedback {
                panel = panel.child(Label::new(feedback.clone()));
            }
            return panel;
        }
        let mut tabs = div().flex().gap_2();
        for (id, label, section) in [
            ("cm-tab-modify", "Modify", Section::Modify),
            ("cm-tab-template", "Templates", Section::Templates),
            (
                "cm-tab-pipeline",
                "Saved Pipelines",
                Section::SavedPipelines,
            ),
            (
                "cm-tab-manage-templates",
                "Manage Templates",
                Section::ManageTemplates,
            ),
            (
                "cm-tab-manage-pipelines",
                "Manage Pipelines",
                Section::ManagePipelines,
            ),
            ("cm-tab-help", "Help", Section::Help),
        ] {
            let receipt = mount.clone();
            tabs = tabs.child(
                Button::new(id)
                    .label(label)
                    .selected(self.section == section)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if receipt.is_current()
                            && receipt.matches(&view.mount)
                            && view.section_revision == section_revision
                        {
                            view.select_section(section, window, cx);
                        }
                    })),
            );
        }
        panel = panel.child(tabs);
        if launcher_effect_pending {
            panel = panel.child(Label::new(LAUNCHER_EFFECT_WAIT));
        }
        if self.section == Section::Modify {
            panel = panel.child(Textarea::new(&self.source).aria_label("Clipboard Modify source"));
            let reload = mount.clone();
            let reset = mount.clone();
            panel = panel.child(
                div()
                    .flex()
                    .gap_2()
                    .child(
                        Button::new("cm-reload")
                            .label("Reload clipboard")
                            .disabled(self.reading || launcher_effect_pending)
                            .on_click(cx.listener(move |view, _, window, cx| {
                                if view.blocked()
                                    || view.section != Section::Modify
                                    || view.section_revision != section_revision
                                {
                                    return;
                                }
                                if reload.is_current() && reload.matches(&view.mount) {
                                    view.read_source(false, window, cx);
                                }
                            })),
                    )
                    .child(Button::new("cm-reset").label("Reset").on_click(cx.listener(
                        move |view, _, window, cx| {
                            if view.blocked()
                                || view.section != Section::Modify
                                || view.section_revision != section_revision
                                || !reset.is_current()
                                || !reset.matches(&view.mount)
                                || !view.open
                            {
                                return;
                            }
                            view.read_revision = view.read_revision.wrapping_add(1);
                            view.read_authority.fetch_add(1, Ordering::AcqRel);
                            view.queued_read = None;
                            view.resolved = view.reset_resolved;
                            view.source_revision = view.source_revision.wrapping_add(1);
                            view.source.update(cx, |input, cx| {
                                input.set_value(view.reset_source.clone(), window, cx)
                            });
                            view.stages.clear();
                            view.picker = None;
                            view.large_allowed = false;
                            view.changed(cx);
                        },
                    ))),
            );
        }
        if let Some(feedback) = &self.catalog_feedback {
            panel = panel.child(Label::new(feedback.clone()));
        }
        if let Some(feedback) = &self.feedback {
            panel = panel.child(Label::new(feedback.clone()));
        }
        if self.section == Section::Modify {
            for (index, stage) in self.stages.iter().enumerate() {
                let id = stage.id;
                let info = catalog::operation_by_id(stage.spec.operation);
                let label = info.map_or("Unknown operation", |info| info.label);
                let choose = mount.clone();
                let up = mount.clone();
                let down = mount.clone();
                let remove = mount.clone();
                let mut row = div()
                    .id(SharedString::from(format!("cm-stage-{id}")))
                    .flex()
                    .flex_col()
                    .gap_1()
                    .child(
                        div()
                            .flex()
                            .gap_2()
                            .child(Label::new(format!("{}: {label}", index + 1)))
                            .child(
                                Button::new(SharedString::from(format!("cm-change-{id}")))
                                    .label("Change operation")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if view.blocked()
                                            || view.section != Section::Modify
                                            || view.section_revision != section_revision
                                        {
                                            return;
                                        }
                                        if choose.is_current()
                                            && choose.matches(&view.mount)
                                            && view.stage_revision == revision
                                        {
                                            view.picker = if view.picker == Some(id) {
                                                None
                                            } else {
                                                Some(id)
                                            };
                                            cx.notify();
                                        }
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("cm-up-{id}")))
                                    .label("Move up")
                                    .disabled(index == 0)
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if view.blocked()
                                            || view.section != Section::Modify
                                            || view.section_revision != section_revision
                                        {
                                            return;
                                        }
                                        if up.is_current()
                                            && up.matches(&view.mount)
                                            && view.stage_revision == revision
                                        {
                                            if let Some(index) =
                                                view.stages.iter().position(|stage| stage.id == id)
                                            {
                                                if index > 0 {
                                                    view.stages.swap(index, index - 1);
                                                    view.changed(cx);
                                                }
                                            }
                                        }
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("cm-down-{id}")))
                                    .label("Move down")
                                    .disabled(index + 1 == self.stages.len())
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if view.blocked()
                                            || view.section != Section::Modify
                                            || view.section_revision != section_revision
                                        {
                                            return;
                                        }
                                        if down.is_current()
                                            && down.matches(&view.mount)
                                            && view.stage_revision == revision
                                        {
                                            if let Some(index) =
                                                view.stages.iter().position(|stage| stage.id == id)
                                            {
                                                if index + 1 < view.stages.len() {
                                                    view.stages.swap(index, index + 1);
                                                    view.changed(cx);
                                                }
                                            }
                                        }
                                    })),
                            )
                            .child(
                                Button::new(SharedString::from(format!("cm-remove-{id}")))
                                    .label("Remove stage")
                                    .on_click(cx.listener(move |view, _, _, cx| {
                                        if view.blocked()
                                            || view.section != Section::Modify
                                            || view.section_revision != section_revision
                                        {
                                            return;
                                        }
                                        if remove.is_current()
                                            && remove.matches(&view.mount)
                                            && view.stage_revision == revision
                                        {
                                            if let Some(index) =
                                                view.stages.iter().position(|stage| stage.id == id)
                                            {
                                                view.stages.remove(index);
                                                view.picker = None;
                                                view.changed(cx);
                                            }
                                        }
                                    })),
                            ),
                    );
                if let Some(info) = info {
                    row = match info.argument_requirements {
                        ArgumentRequirements::CustomWrap => row
                            .child(Textarea::new(&stage.prefix).aria_label("Prefix"))
                            .child(Textarea::new(&stage.suffix).aria_label("Suffix")),
                        ArgumentRequirements::NamedWrap | ArgumentRequirements::Template => {
                            row.child(Textarea::new(&stage.name).aria_label("Template or wrapper"))
                        }
                        ArgumentRequirements::CodeBlock => {
                            row.child(Textarea::new(&stage.language).aria_label("Language"))
                        }
                        ArgumentRequirements::OptionalSeparator => {
                            let receipt = mount.clone();
                            let present = stage.spec.arguments.separator.is_some();
                            let row =
                                row.child(
                                    Button::new(SharedString::from(format!(
                                        "cm-separator-presence-{id}"
                                    )))
                                    .label(if present {
                                        "Use default separator"
                                    } else {
                                        "Set separator"
                                    })
                                    .on_click(cx.listener(move |view, _, window, cx| {
                                        if view.blocked()
                                            || view.section != Section::Modify
                                            || view.section_revision != section_revision
                                        {
                                            return;
                                        }
                                        if !receipt.is_current()
                                            || !receipt.matches(&view.mount)
                                            || view.stage_revision != revision
                                        {
                                            return;
                                        }
                                        let Some(stage) =
                                            view.stages.iter_mut().find(|stage| stage.id == id)
                                        else {
                                            return;
                                        };
                                        stage.spec.arguments.separator =
                                            if present { None } else { Some(String::new()) };
                                        stage.separator.update(cx, |input, cx| {
                                            input.set_value("", window, cx)
                                        });
                                        view.changed(cx);
                                    })),
                                );
                            if present {
                                row.child(Textarea::new(&stage.separator).aria_label("Separator"))
                            } else {
                                row
                            }
                        }
                        ArgumentRequirements::RequiredSeparator => {
                            row.child(Textarea::new(&stage.separator).aria_label("Separator"))
                        }
                        ArgumentRequirements::None => row,
                    };
                }
                if self.picker == Some(id) {
                    let mut list = div()
                        .id(SharedString::from(format!("cm-operations-{id}")))
                        .flex()
                        .flex_wrap()
                        .gap_1()
                        .max_h(px(170.))
                        .overflow_y_scroll();
                    for operation in catalog::operations()
                        .iter()
                        .filter(|op| op.pipeline_available)
                    {
                        let receipt = mount.clone();
                        let op = operation.id;
                        list = list.child(
                            Button::new(SharedString::from(format!("cm-operation-{id}-{op:?}")))
                                .label(operation.label)
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.blocked()
                                        || view.section != Section::Modify
                                        || view.section_revision != section_revision
                                    {
                                        return;
                                    }
                                    if !receipt.is_current()
                                        || !receipt.matches(&view.mount)
                                        || view.stage_revision != revision
                                        || view.picker != Some(id)
                                    {
                                        return;
                                    }
                                    let Some(index) =
                                        view.stages.iter().position(|stage| stage.id == id)
                                    else {
                                        return;
                                    };
                                    if view.stages[index].spec.operation == op {
                                        view.picker = None;
                                        cx.notify();
                                        return;
                                    }
                                    let stage = view.build_stage(id, op, window, cx);
                                    view.stages[index] = stage;
                                    view.picker = None;
                                    view.changed(cx);
                                })),
                        );
                    }
                    row = row.child(list);
                }
                panel = panel.child(row);
            }
            let add = mount.clone();
            let wrap = mount.clone();
            let allow = mount.clone();
            let consent_revision = self.source_revision;
            let cancel_preview = mount.clone();
            let preview_operation = self.preview.current_operation();
            let close = mount.clone();
            panel =
                panel.child(
                    div()
                        .flex()
                        .gap_2()
                        .child(Button::new("cm-add-stage").label("Add stage").on_click(
                            cx.listener(move |view, _, window, cx| {
                                if view.blocked()
                                    || view.section != Section::Modify
                                    || view.section_revision != section_revision
                                {
                                    return;
                                }
                                if add.is_current() && add.matches(&view.mount) && view.open {
                                    view.add_stage(window, cx);
                                }
                            }),
                        ))
                        .child(
                            Button::new("cm-wrap-preview")
                                .label(if self.wrap {
                                    "Wrap preview: On"
                                } else {
                                    "Wrap preview: Off"
                                })
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view.blocked()
                                        || view.section != Section::Modify
                                        || view.section_revision != section_revision
                                    {
                                        return;
                                    }
                                    if wrap.is_current() && wrap.matches(&view.mount) {
                                        view.wrap = !view.wrap;
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new("cm-cancel-preview")
                                .label("Cancel Preview")
                                .disabled(!self.preview.is_active())
                                .on_click(cx.listener(move |view, _, _, cx| {
                                    if view.blocked()
                                        || view.section != Section::Modify
                                        || view.section_revision != section_revision
                                    {
                                        return;
                                    }
                                    if cancel_preview.is_current()
                                        && cancel_preview.matches(&view.mount)
                                        && preview_operation
                                            .is_some_and(|id| view.preview.cancel_if_current(id))
                                    {
                                        cx.notify();
                                    }
                                })),
                        )
                        .child(
                            Button::new("cm-close")
                                .label("Close")
                                .disabled(self.blocked())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.blocked()
                                        || view.section != Section::Modify
                                        || view.section_revision != section_revision
                                    {
                                        return;
                                    }
                                    if close.is_current() && close.matches(&view.mount) {
                                        view.close(window, cx);
                                    }
                                })),
                        ),
                );
            if self.section != Section::Help
                && self.source.read(cx).value().len() > LARGE_SOURCE
                && !self.large_allowed
            {
                panel = panel
                    .child(Label::new("Source exceeds 5 MiB. Preview may take longer."))
                    .child(
                        Button::new("cm-allow-large")
                            .label("Preview large source")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if view.blocked()
                                    || view.section != Section::Modify
                                    || view.section_revision != section_revision
                                {
                                    return;
                                }
                                if allow.is_current()
                                    && allow.matches(&view.mount)
                                    && consent_is_current(
                                        consent_revision,
                                        view.source_revision,
                                        view.source.read(cx).value().len(),
                                    )
                                {
                                    view.large_allowed = true;
                                    view.schedule(cx);
                                }
                            })),
                    );
            }
        } else if self.section == Section::ManageTemplates {
            panel = manage::render_templates(self, panel, cx);
        } else if self.section == Section::ManagePipelines {
            panel = pipelines::render(self, panel, cx);
        } else {
            panel = sections::render(self, panel, cx);
            let close = mount.clone();
            let section = self.section;
            panel = panel.child(
                Button::new("cm-close-other")
                    .label("Close")
                    .disabled(self.blocked())
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if !view.blocked()
                            && view.section == section
                            && view.section_revision == section_revision
                            && close.is_current()
                            && close.matches(&view.mount)
                        {
                            view.close(window, cx);
                        }
                    })),
            );
            if self.section != Section::Help
                && self.source.read(cx).value().len() > LARGE_SOURCE
                && !self.large_allowed
            {
                let allow = mount.clone();
                let consent_revision = self.source_revision;
                panel = panel
                    .child(Label::new("Source exceeds 5 MiB. Preview may take longer."))
                    .child(
                        Button::new("cm-allow-large-other")
                            .label("Preview large source")
                            .on_click(cx.listener(move |view, _, _, cx| {
                                if !view.blocked()
                                    && view.section == section
                                    && view.section_revision == section_revision
                                    && allow.is_current()
                                    && allow.matches(&view.mount)
                                    && consent_is_current(
                                        consent_revision,
                                        view.source_revision,
                                        view.source.read(cx).value().len(),
                                    )
                                {
                                    view.large_allowed = true;
                                    view.schedule(cx);
                                }
                            })),
                    );
            }
        }
        if matches!(self.section, Section::Help | Section::ManageTemplates) {
            return panel;
        }
        let (text, summary) = match self.preview.state() {
            PreviewState::IdleMissing => ("Preview unavailable.".to_owned(), None),
            PreviewState::PendingDebounce { .. } => ("Preview pending…".into(), None),
            PreviewState::Running { .. } => ("Preview running…".into(), None),
            PreviewState::Completed { display, .. } => (
                display.text.clone(),
                Some(format!(
                    "{} characters · {} lines · {} bytes{}",
                    display.metadata.chars,
                    display.metadata.lines,
                    display.metadata.bytes,
                    if display.truncated {
                        " · truncated"
                    } else {
                        ""
                    }
                )),
            ),
            PreviewState::Cancelled { .. } => ("Preview cancelled.".into(), None),
            PreviewState::Failed { error, .. } => (format!("Preview failed: {error}"), None),
        };
        if let Some(summary) = summary {
            panel = panel.child(Label::new(summary));
        }
        let output = Label::new(text).id("cm-preview");
        panel = panel.child(if self.wrap {
            div()
                .id("cm-preview-scroll")
                .h(px(200.))
                .overflow_y_scroll()
                .child(output.whitespace_normal())
        } else {
            div()
                .id("cm-preview-scroll")
                .h(px(200.))
                .overflow_y_scroll()
                .overflow_x_scroll()
                .child(output.whitespace_nowrap())
        });
        let effect_receipt = self.effect_receipt();
        let available = effect_receipt.is_some() && self.picker.is_none() && !launcher_effect_pending;
        let apply_label = match self.section {
            Section::Templates => "Apply Template",
            Section::SavedPipelines => "Execute",
            _ => "Apply",
        };
        for (id, label, kind) in [
            ("cm-apply", apply_label, EffectKind::Apply),
            ("cm-apply-close", "Apply and Close", EffectKind::ApplyClose),
            ("cm-copy-result", "Copy Result", EffectKind::Copy),
        ] {
            if self.section != Section::Modify {
                continue;
            }
            let receipt = mount.clone();
            let focus = match kind {
                EffectKind::Apply => &self.apply_focus,
                EffectKind::ApplyClose => &self.apply_close_focus,
                EffectKind::Copy => &self.copy_focus,
            };
            panel = panel.child(
                Button::new(id)
                    .label(label)
                    .track_focus(focus)
                    .disabled(!available)
                    .on_click(cx.listener(move |view, _, window, cx| {
                        if receipt.is_current() && receipt.matches(&view.mount) {
                            if let Some(expected) = effect_receipt {
                                view.effect(kind, expected, None, window, cx);
                            }
                        }
                    })),
            );
        }
        panel
    }
}
impl RootWindow {
    pub(super) fn open_clipboard_modify(
        &mut self,
        section: Section,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.restart_admission.is_fenced() {
            return;
        }
        if self.external_launch.clipboard_effect_pending()
            && !self
                .clipboard_modify_view
                .as_ref()
                .is_some_and(|view| view.read(cx).open)
        {
            self.launcher_search_error = Some(LAUNCHER_EFFECT_WAIT.into());
            cx.notify();
            return;
        }
        if let (Some(popup), Some(view), Some(root_native)) = (
            &self.clipboard_modify_popup,
            &self.clipboard_modify_view,
            self.native_window,
        ) {
            let Some(native) = popup.native else { return; };
            if current_foreground_identity() != Some(root_native.identity) || view.read(cx).rehosting {
                return;
            }
            let Some(permit) = self.restart_admission.admit() else { return; };
            let popup_window = popup.window;
            let view = view.clone();
            let receipt = view.read(cx).mount.clone();
            let worker_receipt = receipt.clone();
            let visibility = self.root_visibility.clone();
            let visibility_revision = visibility.request_revision.load(Ordering::Acquire);
            let admission = self.restart_admission.clone();
            let guard = view.read(cx).activation_revision.clone();
            let revision = guard.fetch_add(1, Ordering::AcqRel).wrapping_add(1);
            let worker = guard.clone();
            cx.spawn(async move |_, cx| {
                let _permit = permit;
                cx.background_executor().spawn(async move {
                    activate_for_secondary_window_if(
                        &worker, revision, native, Some(root_native.identity), &|| {
                            worker_receipt.is_current()
                                && !admission.is_fenced()
                                && json_utility::native_host_current(
                                    &visibility, visibility_revision, Some(root_native), false,
                                )
                        },
                    );
                }).await;
                if !receipt.is_current() || guard.load(Ordering::Acquire) != revision
                    || current_foreground_identity() != Some(native.identity)
                {
                    return;
                }
                let _ = update_kit_window_content(popup_window, &view, cx, |view, window, cx| {
                    if view.is_mounted_in(window.window_handle().window_id(), true) && !view.rehosting {
                        view.select_section(section, window, cx);
                        view.after_host(window, cx);
                    }
                });
            }).detach();
            return;
        }
        let root = cx.entity().downgrade();
        let native = self.native_window;
        let admission = self.restart_admission.clone();
        let view = self
            .clipboard_modify_view
            .clone()
            .unwrap_or_else(|| cx.new(|cx| ClipboardModifyView::new(window, cx, root, native, admission)));
        let was_open = view.read(cx).open;
        let ready = self.clipboard_modify_startup.runtime.clone();
        let diagnostic = self.clipboard_modify_startup.diagnostic.clone();
        let waiting = self.clipboard_modify_startup.pending;
        view.update(cx, |view, cx| {
            if !was_open { view.host = window.window_handle(); view.native = native; view.popout = false; }
            if let Some(runtime) = ready {
                view.install_startup(runtime, diagnostic, window, cx);
            }
            view.reopen(window, cx);
            view.select_section(section, window, cx);
            if !was_open && waiting && view.runtime.is_none() {
                view.queued_read = Some(true);
                view.feedback = Some("Loading Clipboard Modify settings…".into());
            }
        });
        self.clipboard_modify_view = Some(view.clone());
        if !was_open {
            view.update(cx, |view, _| {
                if view.queued_read.is_none() { view.queued_read = Some(true); }
            });
            let popout = multi_launcher::settings::UtilityToolKey::parse("clipboard-modify")
                .ok()
                .is_some_and(|key| self.utility_presentations.preference(key)
                    == Some(multi_launcher::settings::RememberedPresentation::Popout));
            if popout {
                let weak = view.downgrade();
                let mount = view.read(cx).mount.clone();
                window.defer(cx, move |window, app| {
                    let _ = weak.update(app, |view, cx| {
                        if mount.is_current() && mount.matches(&view.mount)
                            && !view.request_rehost(true, window, cx)
                        {
                            view.queue_dock_reveal(window, cx);
                            view.wake_pending_read(window, cx);
                        }
                    });
                });
                cx.notify();
                return;
            }
        }
        if !was_open
            || (view.read(cx).runtime.is_some()
                && view.read(cx).queued_read.is_some()
                && !view.read(cx).reading)
        {
            if view.read(cx).rehosting { cx.notify(); return; }
            let weak = view.downgrade();
            let mount = view.read(cx).mount.clone();
            let source_revision = view.read(cx).source_revision;
            window.defer(cx, move |window, app| {
                let _ = weak.update(app, |view, cx| {
                    if view.open
                        && mount.is_current()
                        && mount.matches(&view.mount)
                        && view.source_revision == source_revision
                    {
                        let initial = view.queued_read.take().unwrap_or(true);
                        view.read_source(initial, window, cx);
                    }
                });
            });
        }
        let host = window.window_handle().window_id();
        let history = self
            .clipboard_history
            .as_ref()
            .is_some_and(|view| view.read(cx).is_mounted_in(host, false));
        self.root_scroll
            .scroll_to_top_of_item(self.clipboard_scroll_item(host, cx) + usize::from(history));
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::super::utility_mount::MountOwner;
    use super::{
        EffectReceipt, LARGE_SOURCE, consent_is_current, current_completion, current_provider,
        current_undo_provider, held_activation, pump_is_current, read_result_current,
        redacted_read_error,
    };
    #[test]
    fn rehost_requires_a_live_idle_mount_and_a_different_host() {
        assert!(super::rehost_idle(true, false, false, false, false, false));
        assert!(!super::rehost_idle(false, false, false, false, false, false));
        assert!(!super::rehost_idle(true, true, false, false, false, false));
        assert!(!super::rehost_idle(true, false, true, false, false, false));
        assert!(!super::rehost_idle(true, false, false, true, false, false));
        assert!(!super::rehost_idle(true, false, false, false, true, false));
        assert!(!super::rehost_idle(true, false, false, false, false, true));
    }
    #[test]
    fn configuration_reload_refuses_effects_and_reviews() {
        assert!(!super::configuration_reload_blocked(false, false, false, false, false));
        assert!(super::configuration_reload_blocked(false, true, false, false, false));
        assert!(super::configuration_reload_blocked(false, false, true, false, false));
        assert!(super::configuration_reload_blocked(false, false, false, true, false));
        assert!(super::configuration_reload_blocked(false, false, false, false, true));
        assert!(super::configuration_reload_blocked(true, false, false, false, false));
    }

    #[test]
    fn queued_read_wakes_only_once_after_runtime_and_prior_read_are_ready() {
        assert!(super::queued_read_can_start(Some(true), false, true));
        assert!(super::queued_read_can_start(Some(false), false, true));
        assert!(!super::queued_read_can_start(None, false, true));
        assert!(!super::queued_read_can_start(Some(true), true, true));
        assert!(!super::queued_read_can_start(Some(true), false, false));
    }
    #[test]
    fn deferred_dock_reveal_rejects_retired_or_popup_mounts() {
        let owner = MountOwner::default();
        let first = owner.attach();
        assert!(super::dock_reveal_is_current(&first, &first, false));
        assert!(!super::dock_reveal_is_current(&first, &first, true));
        let replacement = owner.attach();
        assert!(!super::dock_reveal_is_current(&first, &replacement, false));
        assert!(super::dock_reveal_is_current(&replacement, &replacement, false));
        owner.retire();
        assert!(!super::dock_reveal_is_current(&replacement, &replacement, false));
    }
    #[test]
    fn effect_receipts_and_held_confirm_are_exact() {
        use multi_launcher::clipboard_modify::coordinator::OperationId;
        let current = EffectReceipt {
            source: 1,
            stages: 2,
            preview: OperationId(3),
            section_revision: 1,
        };
        assert!(
            current
                == EffectReceipt {
                    source: 1,
                    stages: 2,
                    preview: OperationId(3),
                    section_revision: 1
                }
        );
        assert!(
            current
                != EffectReceipt {
                    source: 1,
                    stages: 2,
                    preview: OperationId(4),
                    section_revision: 1
                }
        );
        assert!(
            current
                != EffectReceipt {
                    source: 2,
                    stages: 2,
                    preview: OperationId(3),
                    section_revision: 1
                }
        );
        assert!(
            current
                != EffectReceipt {
                    source: 1,
                    stages: 2,
                    preview: OperationId(3),
                    section_revision: 2
                }
        );
        assert!(current_completion(Some(7), 7));
        assert!(!current_completion(None, 7));
        assert!(!current_completion(Some(8), 7));
        assert!(held_activation(true, Default::default(), "enter"));
        assert!(held_activation(true, Default::default(), "space"));
        assert!(!held_activation(false, Default::default(), "enter"));
    }
    #[test]
    fn section_effect_scope_excludes_wrong_controls() {
        use super::{EffectKind, Section, section_allows_effect};
        assert!(section_allows_effect(Section::Modify, EffectKind::Copy));
        assert!(section_allows_effect(Section::Templates, EffectKind::Apply));
        assert!(section_allows_effect(
            Section::SavedPipelines,
            EffectKind::Apply
        ));
        assert!(!section_allows_effect(Section::Templates, EffectKind::Copy));
        assert!(!section_allows_effect(
            Section::SavedPipelines,
            EffectKind::ApplyClose
        ));
        assert!(!section_allows_effect(Section::Help, EffectKind::Apply));
    }
    #[test]
    fn pipeline_stage_receipts_reject_old_section_and_mount() {
        use super::{Section, StageOwner, stage_receipt_current};
        assert!(stage_receipt_current(
            StageOwner::Pipeline(7),
            Section::ManagePipelines,
            2,
            2,
            true
        ));
        assert!(!stage_receipt_current(
            StageOwner::Pipeline(7),
            Section::ManagePipelines,
            2,
            3,
            true
        ));
        assert!(!stage_receipt_current(
            StageOwner::Pipeline(7),
            Section::ManagePipelines,
            2,
            2,
            false
        ));
        assert!(!stage_receipt_current(
            StageOwner::Pipeline(7),
            Section::Modify,
            2,
            2,
            true
        ));
    }
    #[test]
    fn old_mount_pump_and_old_large_consent_are_rejected() {
        let owner = MountOwner::default();
        let old = owner.attach();
        let new = owner.attach();
        assert!(!pump_is_current(3, 4, true, &old, &new));
        assert!(pump_is_current(4, 4, true, &new, &new));
        assert!(!consent_is_current(3, 4, LARGE_SOURCE + 1));
        assert!(!consent_is_current(4, 4, LARGE_SOURCE));
        assert!(consent_is_current(4, 4, LARGE_SOURCE + 1));
    }
    #[test]
    fn read_completion_rejects_edited_source_and_reopened_mount() {
        let owner = MountOwner::default();
        let old = owner.attach();
        let mut displayed = "draft";
        let completion = "clipboard";
        if read_result_current(true, &old, &old, 1, 1, 3, 4) {
            displayed = completion;
        }
        assert_eq!(displayed, "draft");
        let new = owner.attach();
        if read_result_current(true, &old, &new, 1, 2, 3, 3) {
            displayed = completion;
        }
        assert_eq!(displayed, "draft");
        assert!(read_result_current(true, &new, &new, 2, 2, 4, 4));
    }
    #[test]
    fn deferred_worker_rearms_only_current_unattempted_intents() {
        use super::deferred_read_after_worker;
        assert_eq!(
            deferred_read_after_worker(true, true, true, None),
            Some(true)
        );
        assert_eq!(deferred_read_after_worker(true, false, true, None), None);
        assert_eq!(deferred_read_after_worker(true, true, false, None), None);
        assert_eq!(
            deferred_read_after_worker(true, true, true, Some(false)),
            None
        );
        assert_eq!(
            deferred_read_after_worker(false, true, true, None),
            Some(false)
        );
        assert_eq!(deferred_read_after_worker(false, false, true, None), None);
    }
    #[test]
    fn admitted_read_consumes_only_the_request_it_supersedes() {
        use super::{begin_read_intent, effective_read_intent};
        let mut queued = Some(false);
        assert!(!effective_read_intent(true, queued));
        assert!(!begin_read_intent(true, &mut queued));
        assert_eq!(queued, None);
        assert_eq!(
            super::deferred_read_after_worker(false, false, true, queued),
            None
        );
        let mut queued = Some(true);
        assert!(!begin_read_intent(false, &mut queued));
        assert_eq!(queued, None);
        let mut queued = Some(true);
        assert!(begin_read_intent(true, &mut queued));
        assert_eq!(queued, None);
    }
    #[test]
    fn protected_read_categories_do_not_publish_backend_details() {
        use multi_launcher::clipboard_modify::clipboard::ClipboardError;
        assert_eq!(
            redacted_read_error(ClipboardError::ProtectedItem),
            "This clipboard item was copied privately."
        );
        assert!(
            !redacted_read_error(ClipboardError::Busy("synthetic secret".into()))
                .contains("synthetic secret")
        );
    }
    #[test]
    fn explicit_empty_optional_separator_differs_from_default() {
        use multi_launcher::clipboard_modify::{
            defaults::default_catalog,
            executor::execute_stages,
            model::{OperationId, StageArguments, StageSpec},
        };
        use std::sync::atomic::AtomicBool;
        let stage = |separator| StageSpec {
            operation: OperationId::JoinLines,
            arguments: StageArguments {
                separator,
                ..Default::default()
            },
        };
        let cancel = AtomicBool::new(false);
        let catalog = default_catalog();
        assert_eq!(
            execute_stages("a\nb", &[stage(None)], &catalog, &cancel).unwrap(),
            "a b"
        );
        assert_eq!(
            execute_stages("a\nb", &[stage(Some(String::new()))], &catalog, &cancel).unwrap(),
            "ab"
        );
    }
    #[test]
    fn only_exact_stock_modify_actions_are_admitted() {
        let root = multi_launcher::plugins::clipboard_modify::bare_modify_opener("cm").unwrap();
        let modify =
            multi_launcher::plugins::clipboard_modify::bare_modify_opener("cm modify").unwrap();
        assert!(current_provider("cm", &root, None, None));
        assert!(current_provider("cm modify", &modify, None, None));
        assert!(!current_provider("cm help", &root, None, None));
        assert!(!current_provider("cm modify", &root, None, None));
        for query in [
            "cm template",
            "cm apply",
            "cm help",
            "cm manage-templates",
            "cm manage-pipelines",
        ] {
            let action =
                multi_launcher::plugins::clipboard_modify::bare_modify_opener(query).unwrap();
            assert!(current_provider(query, &action, None, None));
            let mut forged = action.clone();
            forged.label.push('!');
            assert!(!current_provider(query, &forged, None, None));
        }
    }
    #[test]
    fn undo_route_requires_exact_stock_action_query_and_enablement() {
        use std::collections::HashSet;
        let action =
            multi_launcher::plugins::clipboard_modify::bare_undo_action("cm undo").unwrap();
        assert!(current_undo_provider("cm undo", &action, None, None));
        assert!(!current_undo_provider("cm undo extra", &action, None, None));
        let mut forged = action.clone();
        forged.label.push('!');
        assert!(!current_undo_provider("cm undo", &forged, None, None));
        assert!(!current_undo_provider(
            "cm undo",
            &action,
            Some(&HashSet::new()),
            None
        ));
        let commands = multi_launcher::plugins::clipboard_modify::admitted_modify_commands();
        assert_eq!(
            commands
                .iter()
                .map(|action| action.action.as_str())
                .collect::<Vec<_>>(),
            vec![
                "query:cm",
                "query:cm modify",
                "query:cm template",
                "query:cm apply",
                "query:cm manage-templates",
                "query:cm manage-pipelines",
                "query:cm help",
                "query:cm undo"
            ]
        );
    }
}
