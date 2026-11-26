//! Retained first Clipboard Modify workflow using the shared engine and service.
use super::*;
use multi_launcher::clipboard_modify::{
    catalog::{self, ArgumentRequirements},
    clipboard::{CheckedClipboardEffect, CheckedClipboardOutcome, ClipboardReview},
    coordinator::{PreviewCoordinator, PreviewState},
    model::{ClipboardModifierCatalog, OperationId, StageArguments, StageSpec},
    parser::ClipboardModifyIntent,
    runtime::{self, ClipboardModifyRuntime},
};
use std::time::Duration;
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
const LARGE_SOURCE: usize = 5 * 1024 * 1024;
const EFFECT_WAIT: &str = "Wait for the Clipboard Modify operation to finish.";
#[derive(Clone, Copy, PartialEq, Eq)]
enum EffectKind {
    Apply,
    ApplyClose,
    Copy,
}
#[derive(Clone, Copy, PartialEq, Eq)]
struct EffectReceipt {
    source: u64,
    stages: u64,
    preview: multi_launcher::clipboard_modify::coordinator::OperationId,
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
            Ok(Command::ClipboardModify(
                multi_launcher::commands::ClipboardModifyCommand::Open {
                    section: multi_launcher::clipboard_modify::actions::ClipboardModifySectionPayload::Modify
                }
            ))
        )
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct RestartDraft {
    source: String,
    stages: Vec<StageSpec>,
}
struct StageEditor {
    id: u64,
    spec: StageSpec,
    prefix: Entity<InputState>,
    suffix: Entity<InputState>,
    name: Entity<InputState>,
    language: Entity<InputState>,
    separator: Entity<InputState>,
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
pub(super) struct ClipboardModifyView {
    pub open: bool,
    root: WeakEntity<RootWindow>,
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
    initializing: bool,
    active_read: Option<u64>,
    queued_read: Option<bool>,
    resolved: bool,
    reset_resolved: bool,
    stages: Vec<StageEditor>,
    next_stage: u64,
    picker: Option<u64>,
    stage_revision: u64,
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
    fn new(window: &mut Window, cx: &mut Context<Self>, root: WeakEntity<RootWindow>) -> Self {
        let owner = utility_mount::MountOwner::default();
        let mount = owner.attach();
        let source = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(6, 14)
                .placeholder("Source text")
        });
        let source_mount = mount.clone();
        let source_sub = cx.subscribe(
            &source,
            move |view: &mut Self, _: Entity<TextareaState>, event: &InputEvent, cx| {
                if source_mount.is_current()
                    && source_mount.matches(&view.mount)
                    && view.open
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
        );
        Self {
            open: false,
            root,
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
            initializing: false,
            active_read: None,
            queued_read: None,
            resolved: false,
            reset_resolved: false,
            stages: Vec::new(),
            next_stage: 0,
            picker: None,
            stage_revision: 0,
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
            if self.review.is_some() && self.effect_pending.is_none() {
                self.review_cancel_focus.focus(window, cx);
            } else if self.effect_pending.is_some() {
                self.focus.focus(window, cx);
            } else {
                self.source.focus_handle(cx).focus(window, cx);
            }
            return;
        }
        if !self.mount.is_current() {
            self.mount = self.owner.attach();
            let receipt = self.mount.clone();
            let source = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .auto_grow(6, 14)
                    .placeholder("Source text")
            });
            self._source_sub = cx.subscribe(
                &source,
                move |view: &mut Self, _: Entity<TextareaState>, event: &InputEvent, cx| {
                    if receipt.is_current()
                        && receipt.matches(&view.mount)
                        && view.open
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
            );
            self.source = source;
            self.focus = cx.focus_handle();
            self.review_focus = cx.focus_handle();
            self.review_cancel_focus = cx.focus_handle();
            self.apply_focus = cx.focus_handle();
            self.apply_close_focus = cx.focus_handle();
            self.copy_focus = cx.focus_handle();
        }
        self.open = true;
        self.source.focus_handle(cx).focus(window, cx);
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
        self.preview.request(
            text,
            ClipboardModifyIntent::Stages(
                self.stages.iter().map(|stage| stage.spec.clone()).collect(),
            ),
            catalog,
        );
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
        if !self.open
            || self.blocked()
            || !self.mount.is_current()
            || window.has_active_dialog(cx)
            || !window.is_window_active()
        {
            return;
        }
        let Some(root) = self.root.upgrade() else {
            return;
        };
        let root_state = root.read(cx);
        if !root_state
            .clipboard_modify_view
            .as_ref()
            .is_some_and(|current| current.entity_id() == cx.entity().entity_id())
        {
            return;
        }
        let Some(native) = root_state.native_window else {
            return;
        };
        if root_state
            .root_window
            .is_none_or(|host| host.window_id() != window.window_handle().window_id())
            || current_foreground_identity() != Some(native.identity)
        {
            return;
        }
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        if self.reading {
            self.queued_read = Some(initial);
            return;
        }
        let settings = root_state.settings_path.clone();
        let visibility = root_state.root_visibility.clone();
        let admission = root_state.restart_admission.clone();
        let Some(permit) = admission.admit() else {
            return;
        };
        let visibility_revision = visibility.request_revision.load(Ordering::Acquire);
        let mount = self.mount.clone();
        let worker_mount = mount.clone();
        let request = self.read_revision;
        let read_authority = self.read_authority.clone();
        let expected_authority = read_authority.load(Ordering::Acquire);
        let source_revision = self.source_revision;
        let publication_visibility = visibility.clone();
        let publication_admission = admission.clone();
        let initialize = self.runtime.is_none();
        self.initializing = initialize;
        self.reading = true;
        self.active_read = Some(request);
        self.feedback = None;
        let task = cx.background_executor().spawn(async move {
            let initialized = initialize.then(|| {
                ClipboardModifyRuntime::new(
                    std::path::Path::new(&settings),
                    runtime::shared_default_catalog(),
                )
            });
            let result = if worker_mount.is_current()
                && read_authority.load(Ordering::Acquire) == expected_authority
                && !admission.is_fenced()
                && json_utility::native_host_current(
                    &visibility,
                    visibility_revision,
                    Some(native),
                    false,
                )
                && current_foreground_identity() == Some(native.identity)
            {
                runtime::clipboard_service()
                    .read_text_for_modify_once()
                    .map_err(redacted_read_error)
            } else {
                Err("Clipboard read was superseded.".into())
            };
            (initialized, result)
        });
        let host = window.window_handle();
        cx.spawn(async move |weak, cx| {
            let _permit = permit;
            let (initialized, result) = task.await;
            let Some(view) = weak.upgrade() else {
                return;
            };
            let result_update = update_kit_window_content(host, &view, cx, |view, window, cx| {
                if view.active_read != Some(request) { return; }
                view.active_read = None; view.reading = false; view.initializing = false; view.clear_block_wait(cx);
                let installed = initialized.is_some() && view.runtime.is_none();
                if view.runtime.is_none() { if let Some((runtime, loaded)) = initialized { view.catalog = Some(runtime.catalog_snapshot()); view.runtime = Some(runtime); if loaded.state.startup_diagnostic().is_some() { view.catalog_feedback = Some("Clipboard Modify settings could not be loaded; defaults are in use.".into()); } } }
                let current = read_result_current(view.open, &mount, &view.mount, request, view.read_revision, source_revision, view.source_revision)
                    && window.is_window_active() && !publication_admission.is_fenced()
                    && json_utility::native_host_current(&publication_visibility, visibility_revision, Some(native), false)
                    && current_foreground_identity() == Some(native.identity);
                let mut accepted_initial = false;
                if current { match result {
                    Ok(text) => { view.reset_source = text.clone(); view.reset_resolved = true; view.resolved = true; view.large_allowed = false;
                        view.source_revision = view.source_revision.wrapping_add(1); view.source.update(cx, |input, cx| input.set_value(text, window, cx));
                        if initial { accepted_initial = true; view.preview.cancel_and_invalidate(); cx.notify(); } else { view.schedule(cx); }
                    },
                    Err(error) => { view.feedback = Some(error); cx.notify(); }
                } }
                if installed && view.open && view.resolved && !accepted_initial { view.schedule(cx); }
                if let Some(next_initial) = view.queued_read.take() { if view.open { view.read_source(next_initial, window, cx); } }
                cx.notify();
            });
            if result_update.is_err() { let _ = view.update(cx, |view, cx| { if view.active_read == Some(request) { view.active_read = None; view.reading = false; view.initializing = false; view.clear_block_wait(cx); if view.open { view.feedback = Some("Clipboard read could not be delivered.".into()); } cx.notify(); } }); }

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
        let mount = self.mount.clone();
        let prefix = cx.new(|cx| InputState::new(window, cx).placeholder("Prefix"));
        let suffix = cx.new(|cx| InputState::new(window, cx).placeholder("Suffix"));
        let name = cx.new(|cx| InputState::new(window, cx).placeholder("Name"));
        let language = cx.new(|cx| InputState::new(window, cx).placeholder("Language"));
        let separator = cx.new(|cx| InputState::new(window, cx).placeholder("Separator"));
        let mut subs = Vec::new();
        for (input, arg) in [
            (&prefix, Arg::Prefix),
            (&suffix, Arg::Suffix),
            (&name, Arg::Name),
            (&language, Arg::Language),
            (&separator, Arg::Separator),
        ] {
            let receipt = mount.clone();
            subs.push(cx.subscribe(
                input,
                move |view: &mut Self, input: Entity<InputState>, event: &InputEvent, cx| {
                    if !receipt.is_current()
                        || !receipt.matches(&view.mount)
                        || !view.open
                        || !matches!(event, InputEvent::Change)
                    {
                        return;
                    }
                    let value = input.read(cx).value().to_string();
                    let Some(index) = view.stages.iter().position(|stage| stage.id == id) else {
                        return;
                    };
                    let stage = &view.stages[index];
                    let current_input = match arg {
                        Arg::Prefix => &stage.prefix,
                        Arg::Suffix => &stage.suffix,
                        Arg::Name => &stage.name,
                        Arg::Language => &stage.language,
                        Arg::Separator => &stage.separator,
                    };
                    if current_input.entity_id() != input.entity_id() {
                        return;
                    }
                    view.retire_effect_for_edit(cx);
                    let stage = &mut view.stages[index];
                    let field = match arg {
                        Arg::Prefix => &mut stage.spec.arguments.prefix,
                        Arg::Suffix => &mut stage.spec.arguments.suffix,
                        Arg::Name => &mut stage.spec.arguments.name,
                        Arg::Language => &mut stage.spec.arguments.language,
                        Arg::Separator => &mut stage.spec.arguments.separator,
                    };
                    *field = Some(value);
                    view.changed(cx);
                },
            ));
        }
        StageEditor {
            id,
            spec: StageSpec {
                operation: op,
                arguments: StageArguments::default(),
            },
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
        self.initializing || self.effect_pending.is_some() || self.review.is_some()
    }
    fn blocked(&self) -> bool {
        self.effect_pending.is_some() || self.review.is_some()
    }
    fn sync_editor_readonly(&self, cx: &mut Context<Self>) {
        let readonly = self.blocked();
        self.source
            .update(cx, |input, cx| input.set_readonly(readonly, cx));
        for stage in &self.stages {
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
        let Some(native) = root_state.native_window else {
            return;
        };
        let host = window.window_handle();
        if root_state
            .root_window
            .is_none_or(|w| w.window_id() != host.window_id())
            || current_foreground_identity() != Some(native.identity)
            || !root_state
                .clipboard_modify_view
                .as_ref()
                .is_some_and(|v| v.entity_id() == cx.entity().entity_id())
        {
            return;
        }
        let admission = root_state.restart_admission.clone();
        let Some(permit) = admission.admit() else {
            return;
        };
        let visibility = root_state.root_visibility.clone();
        let visibility_revision = visibility.request_revision.load(Ordering::Acquire);
        let ack_visibility = visibility.clone();
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
        let worker_mount = receipt.clone();
        let epoch = self.effect_epoch.clone();
        let expected_epoch = epoch.load(Ordering::Acquire);
        let task = cx.background_executor().spawn(async move {
            let current = || {
                worker_mount.is_current()
                    && epoch.load(Ordering::Acquire) == expected_epoch
                    && !admission.is_fenced()
                    && json_utility::native_host_current(
                        &visibility,
                        visibility_revision,
                        Some(native),
                        false,
                    )
                    && current_foreground_identity() == Some(native.identity)
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
                            && json_utility::native_host_current(
                                &ack_visibility,
                                visibility_revision,
                                Some(native),
                                false,
                            )
                            && !window.has_active_dialog(cx)
                        {
                            view.close(window, cx);
                        }
                    }
                    Ok(CheckedClipboardOutcome::ReviewRequired(review)) => {
                        if !window.is_window_active()
                            || current_foreground_identity() != Some(native.identity)
                            || !json_utility::native_host_current(
                                &ack_visibility,
                                visibility_revision,
                                Some(native),
                                false,
                            )
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
                    && json_utility::native_host_current(
                        &ack_visibility,
                        visibility_revision,
                        Some(native),
                        false,
                    )
                    && !window.has_active_dialog(cx)
                {
                    if view.review.is_some() {
                        view.review_cancel_focus.focus(window, cx);
                    } else {
                        view.source.focus_handle(cx).focus(window, cx);
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
        (!source.is_empty() || !self.stages.is_empty()).then(|| RestartDraft {
            source,
            stages: self.stages.iter().map(|stage| stage.spec.clone()).collect(),
        })
    }
    fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.open || self.initializing || self.effect_pending.is_some() || self.review.is_some()
        {
            return;
        }
        let owns_focus = self.focus.is_focused(window) || self.focus.contains_focused(window, cx);
        self.open = false;
        self.owner.retire();
        self.read_revision = self.read_revision.wrapping_add(1);
        self.read_authority.fetch_add(1, Ordering::AcqRel);
        self.queued_read = None;
        self.resolved = false;
        self.reset_resolved = false;
        self.reset_source.clear();
        self.stages.clear();
        self.picker = None;
        self.large_allowed = false;
        self.feedback = None;
        self.source
            .update(cx, |input, cx| input.set_value("", window, cx));
        self.preview.cancel_and_invalidate();
        self.pump_epoch = self.pump_epoch.wrapping_add(1);
        self.pumping = false;
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
}
impl Render for ClipboardModifyView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mount = self.mount.clone();
        let revision = self.stage_revision;
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
                let Some(root) = view.root.upgrade() else {
                    return;
                };
                let root = root.read(cx);
                if root
                    .root_window
                    .is_none_or(|host| host.window_id() != window.window_handle().window_id())
                    || !root.native_window.is_some_and(|native| {
                        current_foreground_identity() == Some(native.identity)
                    })
                {
                    return;
                }
                let key = event.keystroke.key.as_str();
                let mods = event.keystroke.modifiers;
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
                        || view.copy_focus.is_focused(window))
                {
                    window.prevent_default();
                    cx.stop_propagation();
                    return;
                }
                if view.blocked() || view.picker.is_some() {
                    return;
                }
                if key == "r"
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
            .child(Label::new("Clipboard Modify — Modify"));
        if self.blocked() {
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
                                .disabled(self.effect_pending.is_some())
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if cancel.is_current()
                                        && cancel.matches(&view.mount)
                                        && view.effect_pending.is_none()
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
                        .disabled(self.reading)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.blocked() {
                                return;
                            }
                            if reload.is_current() && reload.matches(&view.mount) {
                                view.read_source(false, window, cx);
                            }
                        })),
                )
                .child(Button::new("cm-reset").label("Reset").on_click(cx.listener(
                    move |view, _, window, cx| {
                        if !reset.is_current() || !reset.matches(&view.mount) || !view.open {
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
        if let Some(feedback) = &self.catalog_feedback {
            panel = panel.child(Label::new(feedback.clone()));
        }
        if let Some(feedback) = &self.feedback {
            panel = panel.child(Label::new(feedback.clone()));
        }
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
                                    if view.blocked() {
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
                                    if view.blocked() {
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
                                    if view.blocked() {
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
                                    if view.blocked() {
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
                        .child(Input::new(&stage.prefix).aria_label("Prefix"))
                        .child(Input::new(&stage.suffix).aria_label("Suffix")),
                    ArgumentRequirements::NamedWrap | ArgumentRequirements::Template => {
                        row.child(Input::new(&stage.name).aria_label("Template or wrapper"))
                    }
                    ArgumentRequirements::CodeBlock => {
                        row.child(Input::new(&stage.language).aria_label("Language"))
                    }
                    ArgumentRequirements::OptionalSeparator => {
                        let receipt = mount.clone();
                        let present = stage.spec.arguments.separator.is_some();
                        let row = row.child(
                            Button::new(SharedString::from(format!("cm-separator-presence-{id}")))
                                .label(if present {
                                    "Use default separator"
                                } else {
                                    "Set separator"
                                })
                                .on_click(cx.listener(move |view, _, window, cx| {
                                    if view.blocked() {
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
                                    stage
                                        .separator
                                        .update(cx, |input, cx| input.set_value("", window, cx));
                                    view.changed(cx);
                                })),
                        );
                        if present {
                            row.child(Input::new(&stage.separator).aria_label("Separator"))
                        } else {
                            row
                        }
                    }
                    ArgumentRequirements::RequiredSeparator => {
                        row.child(Input::new(&stage.separator).aria_label("Separator"))
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
                                if view.blocked() {
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
        panel = panel.child(
            div()
                .flex()
                .gap_2()
                .child(
                    Button::new("cm-add-stage")
                        .label("Add stage")
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.blocked() {
                                return;
                            }
                            if add.is_current() && add.matches(&view.mount) && view.open {
                                view.add_stage(window, cx);
                            }
                        })),
                )
                .child(
                    Button::new("cm-wrap-preview")
                        .label(if self.wrap {
                            "Wrap preview: On"
                        } else {
                            "Wrap preview: Off"
                        })
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if view.blocked() {
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
                            if view.blocked() {
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
                        .disabled(self.initializing)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            if view.blocked() {
                                return;
                            }
                            if close.is_current() && close.matches(&view.mount) {
                                view.close(window, cx);
                            }
                        })),
                ),
        );
        if self.source.read(cx).value().len() > LARGE_SOURCE && !self.large_allowed {
            panel = panel
                .child(Label::new("Source exceeds 5 MiB. Preview may take longer."))
                .child(
                    Button::new("cm-allow-large")
                        .label("Preview large source")
                        .on_click(cx.listener(move |view, _, _, cx| {
                            if view.blocked() {
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
        let available = effect_receipt.is_some() && self.picker.is_none();
        for (id, label, kind) in [
            ("cm-apply", "Apply", EffectKind::Apply),
            ("cm-apply-close", "Apply and Close", EffectKind::ApplyClose),
            ("cm-copy-result", "Copy Result", EffectKind::Copy),
        ] {
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
    pub(super) fn open_clipboard_modify(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.restart_admission.is_fenced() {
            return;
        }
        let root = cx.entity().downgrade();
        let view = self
            .clipboard_modify_view
            .clone()
            .unwrap_or_else(|| cx.new(|cx| ClipboardModifyView::new(window, cx, root)));
        let was_open = view.read(cx).open;
        view.update(cx, |view, cx| view.reopen(window, cx));
        self.clipboard_modify_view = Some(view.clone());
        if !was_open {
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
                        view.read_source(true, window, cx);
                    }
                });
            });
        }
        self.root_scroll.scroll_to_bottom();
        cx.notify();
    }
}
#[cfg(test)]
mod tests {
    use super::super::utility_mount::MountOwner;
    use super::{
        EffectReceipt, LARGE_SOURCE, consent_is_current, current_completion, current_provider,
        held_activation, pump_is_current, read_result_current, redacted_read_error,
    };
    #[test]
    fn effect_receipts_and_held_confirm_are_exact() {
        use multi_launcher::clipboard_modify::coordinator::OperationId;
        let current = EffectReceipt {
            source: 1,
            stages: 2,
            preview: OperationId(3),
        };
        assert!(
            current
                == EffectReceipt {
                    source: 1,
                    stages: 2,
                    preview: OperationId(3)
                }
        );
        assert!(
            current
                != EffectReceipt {
                    source: 1,
                    stages: 2,
                    preview: OperationId(4)
                }
        );
        assert!(
            current
                != EffectReceipt {
                    source: 2,
                    stages: 2,
                    preview: OperationId(3)
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
    }
}
