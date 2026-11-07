use super::utility_dialog_size::UtilityDialogSizeWindowExt as _;
use super::utility_host::{
    HostIdentity, HostedCopyCapture, HostedCopyReceipt, UtilityPresentation, ViewportPassProvenance,
};
use crate::explorer_tabs::{
    ExplorerTabsJob, ExplorerTabsJobState, FolderListId, FolderListRunSnapshot, FolderListStore,
    FolderPath, SavedFolderList,
};
use crate::platform::app_data::AppDataRoot;
use eframe::egui;

fn outcome_label(outcome: crate::explorer_tabs::OperationOutcome) -> &'static str {
    use crate::explorer_tabs::{OperationFailure, OperationOutcome};
    match outcome {
        OperationOutcome::Running => "Opening folders",
        OperationOutcome::Completed => "Completed",
        OperationOutcome::Cancelled => "Cancelled",
        OperationOutcome::DeadlineExpired => "Timed out",
        OperationOutcome::Failed(failure) => match failure {
            OperationFailure::SnapshotIncomplete(_) => "Could not verify Explorer state",
            OperationFailure::TargetChanged | OperationFailure::ShellBindingChanged => {
                "The Explorer window changed"
            }
            OperationFailure::WrongTargetEvidence
            | OperationFailure::WrongRequestEvidence
            | OperationFailure::UnexpectedTabOrdinal
            | OperationFailure::ReusedTabIdentity
            | OperationFailure::StaleRequestResult
            | OperationFailure::UnexpectedObservation => "Could not verify the requested tabs",
            OperationFailure::WrongFolder => "A tab showed a different folder",
            OperationFailure::FolderIdentityUnavailable => "Could not verify a folder",
            OperationFailure::RequestRejected => "Explorer rejected a request",
            OperationFailure::FinalVerificationFailed => "Final tab verification failed",
            OperationFailure::CapabilityUnavailable => "Explorer capability unavailable",
            OperationFailure::NativeOperationFailed => "Explorer operation failed",
        },
    }
}

fn window_status(state: crate::explorer_tabs::BootstrapState) -> &'static str {
    match state {
        crate::explorer_tabs::BootstrapState::NotStarted => "No window requested",
        crate::explorer_tabs::BootstrapState::Attempted => "Window creation unconfirmed",
        crate::explorer_tabs::BootstrapState::Attributed => "New window identified",
    }
}

#[derive(Clone, Default)]
struct Draft {
    selected: Option<FolderListId>,
    baseline: Option<SavedFolderList>,
    name: String,
    folders: Vec<String>,
    generation: u64,
    editor_incarnation: u64,
    row_ids: Vec<u64>,
    next_row_id: u64,
}
impl Draft {
    fn from_saved(list: &SavedFolderList, generation: u64) -> Self {
        Self {
            selected: Some(list.id()),
            baseline: Some(list.clone()),
            name: list.name().into(),
            folders: list
                .folders()
                .iter()
                .map(|path| path.as_str().to_owned())
                .collect(),
            generation,
            editor_incarnation: generation,
            row_ids: (0..list.folders().len() as u64).collect(),
            next_row_id: list.folders().len() as u64,
        }
    }
    fn dirty(&self) -> bool {
        match &self.baseline {
            Some(saved) => {
                self.name != saved.name()
                    || self
                        .folders
                        .iter()
                        .map(String::as_str)
                        .ne(saved.folders().iter().map(FolderPath::as_str))
            }
            None => !self.name.is_empty() || !self.folders.is_empty(),
        }
    }
}
#[derive(Clone)]
enum Intent {
    New,
    Select(FolderListId),
    Save,
    Delete,
    Reorder(FolderListId, usize, Vec<FolderListId>, SavedFolderList),
    Run(FolderListId),
    Cancel(u64),
    DecisionSave,
    DecisionDiscard,
    DecisionCancel,
    Reload,
}
#[derive(Clone, Copy)]
enum Replacement {
    New,
    Select(FolderListId),
    Delete,
}

pub(super) struct ExplorerTabsRuntime {
    pub root: ViewportPassProvenance,
    pub host: HostIdentity,
    pub pinned: bool,
}
pub(super) struct ExplorerTabsActions {
    pub host_request: Option<UtilityPresentation>,
    pub close: bool,
    draft: Option<Draft>,
    expected_generation: u64,
    intent: Option<Intent>,
    copies: Vec<String>,
    ctx: egui::Context,
    receipt: Option<HostedCopyReceipt>,
    #[cfg(test)]
    pub host_button: Option<(egui::Rect, egui::Rect, bool)>,
}
pub(super) struct ExplorerTabsDialog {
    pub open: bool,
    close_requested: bool,
    epoch: u64,
    root: AppDataRoot,
    store: Option<FolderListStore>,
    draft: Draft,
    replacement: Option<Replacement>,
    notice: Option<String>,
    pub job: ExplorerTabsJob,
    job_active: bool,
    wake_configured: bool,
    run: Option<(u64, FolderListRunSnapshot)>,
    pending_external: Option<(
        ViewportPassProvenance,
        HostIdentity,
        u64,
        u64,
        crate::commands::ExplorerTabsCommand,
        Option<u64>,
    )>,
    #[cfg(test)]
    test_controls: std::collections::BTreeMap<String, (egui::Rect, egui::Rect, bool)>,
    #[cfg(test)]
    test_control_ids: std::collections::BTreeMap<String, egui::Id>,
    #[cfg(test)]
    test_gate: Option<std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>>,
}
impl ExplorerTabsDialog {
    pub fn new(root: AppDataRoot) -> Self {
        let (store, notice) = match FolderListStore::open(&root) {
            Ok(store) => (Some(store), None),
            Err(error) => (None, Some(error.to_string())),
        };
        Self {
            open: false,
            close_requested: false,
            epoch: 0,
            root,
            store,
            draft: Draft::default(),
            replacement: None,
            notice,
            job: ExplorerTabsJob::default(),
            job_active: false,
            wake_configured: false,
            run: None,
            pending_external: None,
            #[cfg(test)]
            test_controls: Default::default(),
            #[cfg(test)]
            test_control_ids: Default::default(),
            #[cfg(test)]
            test_gate: None,
        }
    }
    pub fn open(&mut self) {
        if !self.open {
            self.open = true;
            self.epoch = self.epoch.wrapping_add(1).max(1);
        }
    }
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
    pub fn has_pending_decision(&self) -> bool {
        self.replacement.is_some()
    }
    #[cfg(test)]
    pub fn test_control(&self, name: &str) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_controls.get(name).copied()
    }
    #[cfg(test)]
    pub fn test_folders(&self) -> Vec<String> {
        self.draft.folders.clone()
    }
    #[cfg(test)]
    pub fn test_name(&self) -> &str {
        &self.draft.name
    }
    #[cfg(test)]
    pub fn test_notice(&self) -> Option<&str> {
        self.notice.as_deref()
    }
    #[cfg(test)]
    pub fn test_run_snapshot(&self) -> Option<FolderListRunSnapshot> {
        self.run.as_ref().map(|(_, snapshot)| snapshot.clone())
    }
    #[cfg(test)]
    pub fn test_control_id(&self, name: &str) -> Option<egui::Id> {
        self.test_control_ids.get(name).copied()
    }
    #[cfg(test)]
    pub fn test_use_held_backend(
        &mut self,
        gate: std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>,
    ) {
        self.test_gate = Some(gate);
    }
    pub fn request_close(&mut self) {
        self.close_requested = true;
    }
    pub fn finish_close(&mut self) {
        self.open = false;
        self.close_requested = false;
    }
    fn replace(&mut self, replacement: Replacement) -> bool {
        let next = self.draft.generation.wrapping_add(1);
        match replacement {
            Replacement::New => {
                self.draft = Draft {
                    generation: next,
                    editor_incarnation: next,
                    ..Draft::default()
                };
                false
            }
            Replacement::Select(id) => {
                if let Some(list) = self
                    .store
                    .as_ref()
                    .and_then(|s| s.lists().iter().find(|list| list.id() == id))
                {
                    self.draft = Draft::from_saved(list, next);
                }
                false
            }
            Replacement::Delete => self.delete(),
        }
    }
    fn save(&mut self) -> bool {
        let paths = self
            .draft
            .folders
            .iter()
            .map(|raw| {
                FolderPath::parse(raw).map_err(|error| format!("Invalid folder path: {error:?}"))
            })
            .collect::<Result<Vec<_>, _>>();
        let result = paths.and_then(|paths| {
            let store = self
                .store
                .as_mut()
                .ok_or_else(|| "Folder list storage is unavailable".to_owned())?;
            match self.draft.selected {
                Some(id) => {
                    if store.lists().iter().find(|list| list.id() == id)
                        != self.draft.baseline.as_ref()
                    {
                        return Err("Saved list changed; reload before saving".into());
                    }
                    store
                        .update(id, self.draft.name.clone(), paths)
                        .map_err(|e| e.to_string())?;
                    Ok(id)
                }
                None => store
                    .create(self.draft.name.clone(), paths)
                    .map_err(|e| e.to_string()),
            }
        });
        match result {
            Ok(id) => {
                if let Some(list) = self
                    .store
                    .as_ref()
                    .and_then(|s| s.lists().iter().find(|list| list.id() == id))
                {
                    self.draft = Draft::from_saved(list, self.draft.generation.wrapping_add(1));
                }
                self.notice = Some("Saved folder list".into());
                true
            }
            Err(error) => {
                self.notice = Some(error);
                false
            }
        }
    }
    fn delete(&mut self) -> bool {
        let result = self
            .draft
            .selected
            .ok_or_else(|| "Choose a saved list".to_owned())
            .and_then(|id| {
                let store = self
                    .store
                    .as_mut()
                    .ok_or_else(|| "Folder list storage is unavailable".to_owned())?;
                if store.lists().iter().find(|list| list.id() == id) != self.draft.baseline.as_ref()
                {
                    return Err("Saved list changed; reload before deleting".into());
                }
                store.remove(id).map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => {
                self.draft = Draft {
                    generation: self.draft.generation.wrapping_add(1),
                    editor_incarnation: self.draft.generation.wrapping_add(1),
                    ..Draft::default()
                };
                self.notice = Some("Deleted folder list".into());
                true
            }
            Err(error) => {
                self.notice = Some(error);
                false
            }
        }
    }
    pub fn start_saved(&mut self, id: FolderListId) {
        if matches!(self.job.poll(), ExplorerTabsJobState::Running { .. }) {
            self.notice = Some("An Explorer Tabs run is already active or stopping".into());
            return;
        }
        let result = FolderListStore::open(&self.root)
            .map_err(|error| error.to_string())
            .and_then(|store| store.launch_snapshot(id).map_err(|error| error.to_string()));
        match result {
            Ok(snapshot) => match {
                #[cfg(test)]
                if let Some(gate) = self.test_gate.clone() {
                    self.job.start_test_held(snapshot.request().clone(), gate)
                } else {
                    self.job.start(
                        snapshot.request().clone(),
                        std::time::Duration::from_secs(30),
                    )
                }
                #[cfg(not(test))]
                {
                    self.job.start(
                        snapshot.request().clone(),
                        std::time::Duration::from_secs(30),
                    )
                }
            } {
                Ok(id) => {
                    self.job_active = true;
                    self.run = Some((id, snapshot));
                    self.notice = Some("Opening the saved folder list".into());
                }
                Err(error) => self.notice = Some(format!("Cannot start Explorer Tabs: {error:?}")),
            },
            Err(error) => self.notice = Some(error),
        }
    }
    pub fn cancel_current(&mut self, captured: Option<u64>) {
        if let ExplorerTabsJobState::Running { id, .. } = self.job.poll()
            && captured == Some(id)
        {
            self.job.cancel();
        }
    }
    pub fn current_job_id(&mut self) -> Option<u64> {
        match self.job.poll() {
            ExplorerTabsJobState::Running { id, .. } => Some(id),
            _ => None,
        }
    }
    pub fn has_active_job(&mut self) -> bool {
        self.current_job_id().is_some()
    }
    pub fn needs_root_ui_when_hidden(&self) -> bool {
        self.job_active || self.pending_external.is_some()
    }
    fn configure_wake(&mut self, ctx: &egui::Context) {
        if !self.wake_configured {
            let wake_ctx = ctx.clone();
            self.job.set_wake(std::sync::Arc::new(move || {
                wake_ctx.request_repaint_of(egui::ViewportId::ROOT)
            }));
            self.wake_configured = true;
        }
    }
    pub fn poll_background(&mut self, ctx: &egui::Context) {
        self.configure_wake(ctx);
        if self.job_active {
            self.job_active = self.current_job_id().is_some();
            if self.job_active {
                ctx.request_repaint_after_for(
                    std::time::Duration::from_millis(100),
                    egui::ViewportId::ROOT,
                );
            }
        }
    }
    pub fn stage_command(
        &mut self,
        ctx: &egui::Context,
        host: HostIdentity,
        command: crate::commands::ExplorerTabsCommand,
    ) {
        if ctx.viewport_id() == egui::ViewportId::ROOT {
            self.configure_wake(ctx);
            let job = self.current_job_id();
            self.pending_external = Some((
                ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT),
                host,
                self.epoch,
                self.draft.generation,
                command,
                job,
            ));
            ctx.request_repaint();
        }
    }
    pub fn complete_external(
        &mut self,
        ctx: &egui::Context,
        host: HostIdentity,
        closed: bool,
    ) -> bool {
        let Some((root, captured_host, epoch, generation, command, job)) =
            self.pending_external.take()
        else {
            return false;
        };
        if closed
            || captured_host != host
            || epoch != self.epoch
            || generation != self.draft.generation
            || !root.was_retained(ctx)
        {
            return false;
        }
        self.open();
        match command {
            crate::commands::ExplorerTabsCommand::Open => {}
            crate::commands::ExplorerTabsCommand::Run(id) => self.start_saved(id),
            crate::commands::ExplorerTabsCommand::Cancel => self.cancel_current(job),
        }
        true
    }
    pub fn render_host(
        &mut self,
        ctx: &egui::Context,
        runtime: &ExplorerTabsRuntime,
        owner: bool,
        first: bool,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> ExplorerTabsActions {
        let mut draft = self.draft.clone();
        let expected_generation = draft.generation;
        let mut changed = false;
        let mut intent = None;
        let mut host_request = None;
        let mut window_open = self.open;
        let mut visible = false;
        #[cfg(test)]
        let mut host_button = None;
        #[cfg(test)]
        let mut controls = std::collections::BTreeMap::new();
        #[cfg(test)]
        let mut control_ids = std::collections::BTreeMap::new();
        macro_rules! observe {
            ($ui:expr, $name:expr, $response:expr) => {{
                let response = $response;
                #[cfg(test)]
                {
                    let name = $name.to_owned();
                    controls.insert(
                        name.clone(),
                        (response.rect, $ui.clip_rect(), response.enabled()),
                    );
                    control_ids.insert(name, response.id);
                }
                response
            }};
        }
        let start = ctx.output(|output| output.commands.len());
        let lists = self
            .store
            .as_ref()
            .map(|s| s.lists().to_vec())
            .unwrap_or_default();
        let (active, report, diagnostic, worker_failed) = match self.job.poll() {
            ExplorerTabsJobState::Running { id, progress } => {
                (Some(id), progress.cloned(), None, false)
            }
            ExplorerTabsJobState::Finished { report, .. } => (
                None,
                Some(report.operation.clone()),
                report.diagnostic.clone(),
                false,
            ),
            ExplorerTabsJobState::WorkerFailed { .. } => (None, None, None, true),
            _ => (None, None, None, false),
        };
        let run_name = self
            .run
            .as_ref()
            .map(|(_, snapshot)| snapshot.name().to_owned());
        let notice = self.notice.clone();
        let replacement = self.replacement;
        let mut body = |ui: &mut egui::Ui| {
            if ctx.viewport_id() == egui::ViewportId::ROOT {
                ui.style_mut().interaction.selectable_labels = false;
            }
            visible = ui.is_visible() && !ui.is_sizing_pass();
            let allowed = owner && first && visible && !ctx.will_discard();
            ui.add_enabled_ui(owner, |ui| {
                let (button, next) = super::utility_host::presentation_button(
                    ui,
                    presentation,
                    allowed && replacement.is_none(),
                );
                #[cfg(test)]
                {
                    host_button = Some((button.rect, ui.clip_rect(), button.enabled()));
                }
                if button.clicked() && allowed && replacement.is_none() {
                    host_request = Some(next);
                }
                if let Some(notice) = &notice {
                    ui.label(notice);
                }
                if worker_failed {
                    ui.label(
                        "The operation stopped without a report; Explorer effects are unknown.",
                    );
                }
                if let Some(report) = &report {
                    ui.label(format!(
                        "{} — {}",
                        run_name.as_deref().unwrap_or("Run"),
                        outcome_label(report.outcome())
                    ));
                    for tab in report.verified_prefix() {
                        ui.label(format!(
                            "Verified tab {}: {}",
                            tab.ordinal(),
                            tab.folder().as_str()
                        ));
                    }
                    if let Some(step) = report.attempted_unverified() {
                        ui.label(format!(
                            "Requested but unverified: {}",
                            step.folder().as_str()
                        ));
                    }
                    for folder in report.remaining() {
                        ui.label(format!("Not attempted: {}", folder.as_str()));
                    }
                }
                if let Some(diagnostic) = &diagnostic {
                    ui.label(format!(
                        "{} ({})",
                        diagnostic.reason,
                        window_status(diagnostic.bootstrap)
                    ));
                    if let Some(path) = &diagnostic.owned_resource_path {
                        ui.label(format!(
                            "Owned temporary directory remains: {}",
                            path.display()
                        ));
                    }
                }
                if let Some(replacement) = replacement {
                    let _ = replacement;
                    if allowed
                        && !egui::Popup::is_any_open(ctx)
                        && ctx.input_mut(|input| {
                            input.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                        })
                    {
                        intent = Some(Intent::DecisionCancel);
                    }
                    ui.label("Save changes before switching or deleting?");
                    ui.horizontal(|ui| {
                        if ui.button("Save").clicked() && allowed {
                            intent = Some(Intent::DecisionSave);
                        }
                        if observe!(ui, "decision-discard", ui.button("Discard")).clicked()
                            && allowed
                        {
                            intent = Some(Intent::DecisionDiscard);
                        }
                        if ui.button("Cancel").clicked() && allowed {
                            intent = Some(Intent::DecisionCancel);
                        }
                    });
                    return;
                }
                ui.horizontal_wrapped(|ui| {
                    if ui.button("New").clicked() && allowed {
                        intent = Some(Intent::New);
                    }
                    for (index, list) in lists.iter().enumerate() {
                        if observe!(
                            ui,
                            format!("list-{}", list.id().get()),
                            ui.selectable_label(draft.selected == Some(list.id()), list.name())
                        )
                        .clicked()
                            && allowed
                        {
                            intent = Some(Intent::Select(list.id()));
                        }
                        if ui.small_button("↑").clicked() && allowed && index > 0 {
                            intent = Some(Intent::Reorder(
                                list.id(),
                                index - 1,
                                lists.iter().map(SavedFolderList::id).collect(),
                                list.clone(),
                            ));
                        }
                        if observe!(
                            ui,
                            format!("list-down-{}", list.id().get()),
                            ui.small_button("↓")
                        )
                        .clicked()
                            && allowed
                            && index + 1 < lists.len()
                        {
                            intent = Some(Intent::Reorder(
                                list.id(),
                                index + 1,
                                lists.iter().map(SavedFolderList::id).collect(),
                                list.clone(),
                            ));
                        }
                    }
                });
                ui.label("List name");
                changed |= observe!(
                    ui,
                    "name",
                    ui.add(
                        egui::TextEdit::singleline(&mut draft.name)
                            .id(egui::Id::new((
                                "explorer-tabs",
                                "name",
                                draft.editor_incarnation
                            )))
                            .char_limit(128)
                    )
                )
                .changed();
                let mut remove = None;
                for index in 0..draft.folders.len() {
                    ui.push_id((draft.editor_incarnation, draft.row_ids[index]), |ui| {
                        ui.horizontal(|ui| {
                            changed |= observe!(
                                ui,
                                format!("folder-{index}"),
                                ui.add(
                                    egui::TextEdit::singleline(&mut draft.folders[index])
                                        .id(egui::Id::new((
                                            "explorer-tabs",
                                            "folder",
                                            draft.editor_incarnation,
                                            draft.row_ids[index]
                                        )))
                                        .char_limit(32767)
                                )
                            )
                            .changed();
                            if ui.small_button("Remove").clicked() && allowed {
                                remove = Some(index);
                            }
                            if ui.small_button("↑").clicked() && allowed && index > 0 {
                                draft.folders.swap(index, index - 1);
                                draft.row_ids.swap(index, index - 1);
                                changed = true;
                            }
                            if observe!(ui, format!("folder-down-{index}"), ui.small_button("↓"))
                                .clicked()
                                && allowed
                                && index + 1 < draft.folders.len()
                            {
                                draft.folders.swap(index, index + 1);
                                draft.row_ids.swap(index, index + 1);
                                changed = true;
                            }
                        });
                    });
                }
                if let Some(index) = remove {
                    draft.folders.remove(index);
                    draft.row_ids.remove(index);
                    changed = true;
                }
                if observe!(ui, "add-folder", ui.button("Add folder")).clicked()
                    && allowed
                    && draft.folders.len() < 32
                {
                    draft.folders.push(String::new());
                    draft.row_ids.push(draft.next_row_id);
                    draft.next_row_id = draft.next_row_id.wrapping_add(1);
                    changed = true;
                }
                ui.horizontal_wrapped(|ui| {
                    if observe!(ui, "save", ui.button("Save list")).clicked() && allowed {
                        intent = Some(Intent::Save);
                    }
                    if observe!(
                        ui,
                        "delete",
                        ui.add_enabled(draft.selected.is_some(), egui::Button::new("Delete list"))
                    )
                    .clicked()
                        && allowed
                    {
                        intent = Some(Intent::Delete);
                    }
                    if observe!(
                        ui,
                        "run",
                        ui.add_enabled(
                            draft.selected.is_some() && !draft.dirty() && active.is_none(),
                            egui::Button::new("Run saved list"),
                        )
                    )
                    .clicked()
                        && allowed
                    {
                        intent = draft.selected.map(Intent::Run);
                    }
                    if observe!(
                        ui,
                        "cancel-run",
                        ui.add_enabled(active.is_some(), egui::Button::new("Cancel run"))
                    )
                    .clicked()
                        && allowed
                    {
                        intent = active.map(Intent::Cancel);
                    }
                    if ui.button("Reload storage").clicked() && allowed {
                        intent = Some(Intent::Reload);
                    }
                });
            });
        };
        if let Some(ui) = child {
            body(ui);
        } else {
            egui::Window::new("Explorer Tabs")
                .id(egui::Id::new("explorer-tabs-window"))
                .resizable(true)
                .show_utility(
                    ctx,
                    "explorer-tabs",
                    egui::Id::new("explorer-tabs-window"),
                    Some(&mut window_open),
                    super::utility_dialog_size::UtilityWindowSizeRecipe::new(
                        super::utility_dialog_size::UtilityWindowSizeDefault::outer(egui::vec2(
                            640.0, 520.0,
                        )),
                        egui::vec2(320.0, 240.0),
                        [None, None],
                    ),
                    |ui| {
                        egui::ScrollArea::vertical().show(ui, |ui| body(ui));
                    },
                );
        }
        let mut copies = Vec::new();
        ctx.output_mut(|output| {
            let tail = output.commands.split_off(start.min(output.commands.len()));
            for command in tail {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
        });
        let receipt = if ctx.viewport_id() != egui::ViewportId::ROOT {
            ctx.add_plugin(HostedCopyCapture::default());
            let receipt = HostedCopyReceipt {
                root: runtime.root,
                owner: ViewportPassProvenance::current(ctx, ctx.viewport_id()),
                host: runtime.host,
                epoch: self.epoch,
                eligible: owner && first && visible && !ctx.will_discard(),
            };
            ctx.with_plugin(|plugin: &mut HostedCopyCapture| {
                plugin
                    .captured
                    .retain(|(old, _)| old.owner.viewport_id != ctx.viewport_id());
                plugin.armed.insert(ctx.viewport_id(), receipt);
            });
            Some(receipt)
        } else {
            None
        };
        if changed {
            draft.generation = draft.generation.wrapping_add(1);
        }
        #[cfg(test)]
        {
            self.test_controls = controls;
            self.test_control_ids = control_ids;
        }
        ExplorerTabsActions {
            host_request,
            close: !runtime.pinned && (!window_open || self.close_requested),
            draft: changed.then_some(draft),
            expected_generation,
            intent,
            copies,
            ctx: ctx.clone(),
            receipt,
            #[cfg(test)]
            host_button,
        }
    }
    pub fn complete(&mut self, mut actions: ExplorerTabsActions) -> (bool, Vec<String>) {
        let mut copies = Vec::new();
        if let Some(receipt) = actions.receipt {
            actions.ctx.with_plugin(|plugin: &mut HostedCopyCapture| {
                plugin.captured.retain(|(candidate, values)| {
                    let matches = candidate.root == receipt.root
                        && candidate.owner == receipt.owner
                        && candidate.host == receipt.host
                        && candidate.epoch == receipt.epoch;
                    if matches {
                        copies.extend(values.iter().cloned());
                    }
                    !matches
                });
            });
        }
        if self.draft.generation != actions.expected_generation {
            return (false, Vec::new());
        }
        if let Some(draft) = actions.draft.take() {
            self.draft = draft;
        }
        if actions.close || actions.host_request.is_some() {
            return (false, Vec::new());
        }
        copies.extend(actions.copies);
        let mut changed_store = false;
        match actions.intent {
            Some(Intent::New) => {
                if self.draft.dirty() {
                    self.replacement = Some(Replacement::New);
                } else {
                    self.replace(Replacement::New);
                }
            }
            Some(Intent::Select(id)) if Some(id) != self.draft.selected => {
                if self.draft.dirty() {
                    self.replacement = Some(Replacement::Select(id));
                } else {
                    self.replace(Replacement::Select(id));
                }
            }
            Some(Intent::Save) => changed_store = self.save(),
            Some(Intent::Delete) => {
                if self.draft.dirty() {
                    self.replacement = Some(Replacement::Delete);
                } else {
                    changed_store = self.delete();
                }
            }
            Some(Intent::Reorder(id, index, order, basis)) => {
                if let Some(store) = &mut self.store {
                    if store.lists().iter().map(SavedFolderList::id).eq(order)
                        && store.lists().iter().find(|item| item.id() == id) == Some(&basis)
                    {
                        match store.reorder(id, index) {
                            Ok(()) => changed_store = true,
                            Err(e) => self.notice = Some(e.to_string()),
                        }
                    } else {
                        self.notice = Some("Saved lists changed; reload before reordering".into());
                    }
                }
            }
            Some(Intent::Run(id)) if !self.draft.dirty() && self.draft.selected == Some(id) => {
                self.start_saved(id)
            }
            Some(Intent::Cancel(id)) => self.cancel_current(Some(id)),
            Some(Intent::DecisionSave) => {
                if self.save() {
                    changed_store = true;
                    if let Some(next) = self.replacement.take() {
                        self.replace(next);
                    }
                }
            }
            Some(Intent::DecisionDiscard) => {
                if let Some(next) = self.replacement.take() {
                    changed_store = self.replace(next);
                }
            }
            Some(Intent::DecisionCancel) => self.replacement = None,
            Some(Intent::Reload) => {
                if !self.draft.dirty() {
                    match FolderListStore::open(&self.root) {
                        Ok(store) => {
                            self.store = Some(store);
                            self.notice = None;
                            self.draft = Draft {
                                generation: self.draft.generation.wrapping_add(1),
                                editor_incarnation: self.draft.generation.wrapping_add(1),
                                ..Draft::default()
                            };
                        }
                        Err(error) => self.notice = Some(error.to_string()),
                    }
                } else {
                    self.notice = Some("Save or discard your draft before reloading".into());
                }
            }
            _ => {}
        }
        (changed_store, copies)
    }
}

pub(super) type ExplorerTabsWindowHost =
    super::utility_window_host::UtilityWindowHost<ExplorerTabsDialog>;
pub(super) enum ExplorerTabsHostCommand {
    Closed,
    Apply(ExplorerTabsActions),
}
impl super::utility_window_host::HostedUtilityDialog for ExplorerTabsDialog {
    const KEY: &'static str = "explorer-tabs";
    const TITLE: &'static str = "Explorer Tabs";
    const SIZE: egui::Vec2 = egui::vec2(640.0, 520.0);
    const MINIMUM: egui::Vec2 = egui::vec2(320.0, 240.0);
    const SCROLL_NATIVE_WINDOW: bool = true;
    type Runtime = ExplorerTabsRuntime;
    type Actions = ExplorerTabsActions;
    type Checkpoint = ();
    type Command = ExplorerTabsHostCommand;
    fn layer() -> egui::LayerId {
        egui::LayerId::new(egui::Order::Middle, egui::Id::new("explorer-tabs-window"))
    }
    fn navigation_owner(_: u64) -> super::jump_mode::ui::NavigationOwner {
        super::jump_mode::ui::NavigationOwner::Blocked
    }
    fn is_open(&self) -> bool {
        self.open
    }
    fn session(&self) -> u64 {
        self.epoch
    }
    fn parent_targets_allowed(&self, _: &Self::Runtime) -> bool {
        false
    }
    fn parent_input_allowed(&self) -> bool {
        true
    }
    fn native_parent_current(
        &self,
        binding: super::jump_mode::ui::NavigationViewportBinding,
        ctx: &egui::Context,
        _: Option<egui::LayerId>,
    ) -> bool {
        ctx.viewport_id() == binding.viewport
            && ctx.input(|input| input.focused)
            && ctx.memory(|memory| memory.top_modal_layer().is_none())
            && super::jump_mode::ui::visible_top_middle_layer(ctx)
                .is_none_or(|layer| layer == binding.base_layer)
            && super::jump_mode::ui::focused_response_layer(ctx).is_none_or(|layer| {
                layer == binding.base_layer || layer == egui::LayerId::background()
            })
    }
    fn popup_layer(&self, _: &egui::Context, _: egui::LayerId) -> Option<egui::LayerId> {
        None
    }
    fn confirmation_binding(
        &self,
        _: egui::ViewportId,
        _: u64,
    ) -> Option<super::jump_mode::ui::NavigationViewportBinding> {
        None
    }
    fn capture(&self) {}
    fn restore(&mut self, _: ()) {}
    fn render_host(
        &mut self,
        ctx: &egui::Context,
        _: (
            Option<&super::jump_mode::ui::JumpModeScope>,
            Option<&super::jump_mode::ui::JumpModeScope>,
        ),
        _: super::jump_mode::ui::NavigationOwner,
        parent: bool,
        _: super::jump_mode::ui::NavigationOwner,
        first: bool,
        _: u64,
        runtime: &Self::Runtime,
        child: Option<&mut egui::Ui>,
        presentation: UtilityPresentation,
    ) -> Self::Actions {
        self.render_host(ctx, runtime, parent, first, child, presentation)
    }
    fn host_request(actions: &Self::Actions) -> Option<UtilityPresentation> {
        actions.host_request
    }
    fn closed_command() -> Self::Command {
        ExplorerTabsHostCommand::Closed
    }
    fn command_closes(command: &Self::Command) -> bool {
        matches!(command, ExplorerTabsHostCommand::Closed)
    }
    fn complete_actions(&mut self, actions: Self::Actions, _: Option<usize>) -> Vec<Self::Command> {
        if actions.close {
            vec![ExplorerTabsHostCommand::Closed]
        } else {
            vec![ExplorerTabsHostCommand::Apply(actions)]
        }
    }
    #[cfg(test)]
    fn test_button(actions: &Self::Actions) -> Option<(egui::Rect, egui::Rect, bool)> {
        actions.host_button
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explorer_tabs_run_reloads_saved_id_and_refuses_deleted_or_corrupt_store() {
        struct Release(std::sync::Arc<(std::sync::Mutex<bool>, std::sync::Condvar)>);
        impl Drop for Release {
            fn drop(&mut self) {
                let (lock, cv) = &*self.0;
                *lock.lock().unwrap() = true;
                cv.notify_all();
            }
        }
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let mut writer = FolderListStore::open(&root).unwrap();
        let id = writer
            .create("Saved", vec![FolderPath::parse(r"C:\Old").unwrap()])
            .unwrap();
        let mut dialog = ExplorerTabsDialog::new(root.clone());
        let gate = std::sync::Arc::new((std::sync::Mutex::new(false), std::sync::Condvar::new()));
        let _release = Release(gate.clone());
        dialog.test_use_held_backend(gate);
        writer.remove(id).unwrap();
        dialog.start_saved(id);
        assert!(dialog.current_job_id().is_none());
        assert!(dialog.notice.is_some());
        std::fs::write(
            directory
                .path()
                .join(crate::explorer_tabs::FOLDER_LIST_FILE_NAME),
            b"not json",
        )
        .unwrap();
        dialog.start_saved(id);
        assert!(dialog.current_job_id().is_none());
        assert!(dialog.notice.is_some());
    }
}
