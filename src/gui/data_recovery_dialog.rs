use crate::commands::{DataDialogFocus, DataRecoveryCommand, DataRecoveryConfirmation};
use crate::persistence::{
    BackupPolicy, DataRequestId, DataService, DataServiceActivity, DataServiceFailure,
    DataServiceRequest, DataServiceResult, PersistentStoreId, RecoveryGroupId, RecoveryTarget,
    SnapshotRecord, SnapshotStatus, StagedRecoveryAction, StoreHealth, StoreHealthReport,
    StoreKind, StoreOwnership,
};
use crate::platform::app_data::AppDataRoot;
use crate::settings::Settings;
use eframe::egui;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DataDialogSection {
    StorageHealth,
    Backups,
    Recovery,
    Diagnostics,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum PendingRecoveryIntent {
    Restore {
        target: RecoveryTarget,
        snapshot_id: String,
    },
    Reset {
        store_id: PersistentStoreId,
    },
}

impl PendingRecoveryIntent {
    pub(crate) fn confirmed_command(self) -> DataRecoveryCommand {
        let confirmation = DataRecoveryConfirmation::from_explicit_user_confirmation();
        match self {
            Self::Restore {
                target,
                snapshot_id,
            } => DataRecoveryCommand::Restore {
                target,
                snapshot_id,
                confirmation,
            },
            Self::Reset { store_id } => DataRecoveryCommand::Reset {
                store_id,
                confirmation,
            },
        }
    }

    pub(crate) fn confirmation_copy(&self, label: &str) -> (String, String) {
        let action = match self {
            Self::Restore { snapshot_id, .. } => {
                format!("Restore {label} from backup {snapshot_id}?")
            }
            Self::Reset { .. } => format!("Reset {label} to its default state?"),
        };
        (
            action,
            "Current data will be preserved first. The change is staged for the next launch and requires restarting Multi Launcher.".into(),
        )
    }
}

pub(crate) fn resolve_confirmed_intent(
    pending: &mut Option<PendingRecoveryIntent>,
    confirmed: bool,
) -> Option<DataRecoveryCommand> {
    let intent = pending.take()?;
    confirmed.then(|| intent.confirmed_command())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DataRecoveryUiAction {
    OpenPath(PathBuf),
    Confirm(PendingRecoveryIntent),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SafeStartupDiagnostic {
    pub label: String,
    pub path: Option<PathBuf>,
    pub summary: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DataUiNotice {
    pub message: String,
    pub error: bool,
}

pub(crate) struct DataRecoveryDialog {
    pub open: bool,
    section: DataDialogSection,
    root: AppDataRoot,
    settings: Settings,
    service: Option<DataService>,
    repaint: Arc<dyn Fn() + Send + Sync>,
    service_error: Option<String>,
    health: Vec<StoreHealthReport>,
    snapshots: Vec<SnapshotRecord>,
    selected_snapshot: Option<String>,
    latest_health: Option<DataRequestId>,
    latest_snapshots: Option<DataRequestId>,
    latest_backup: Option<DataRequestId>,
    latest_recovery: Option<DataRequestId>,
    health_deferred: bool,
    snapshot_list_deferred: bool,
    backup_summary: Option<String>,
    recovery_summary: Option<String>,
    startup_diagnostics: Vec<SafeStartupDiagnostic>,
    startup_notice_pending: bool,
}

impl DataRecoveryDialog {
    pub(crate) fn new(
        root: AppDataRoot,
        settings: Settings,
        repaint: impl Fn() + Send + Sync + 'static,
    ) -> Self {
        Self {
            open: false,
            section: DataDialogSection::StorageHealth,
            root,
            settings,
            service: None,
            repaint: Arc::new(repaint),
            service_error: None,
            health: Vec::new(),
            snapshots: Vec::new(),
            selected_snapshot: None,
            latest_health: None,
            latest_snapshots: None,
            latest_backup: None,
            latest_recovery: None,
            health_deferred: false,
            snapshot_list_deferred: false,
            backup_summary: None,
            recovery_summary: None,
            startup_diagnostics: Vec::new(),
            startup_notice_pending: false,
        }
    }

    pub(crate) fn root(&self) -> &Path {
        self.root.path()
    }

    pub(crate) fn store_label(&self, id: PersistentStoreId) -> &'static str {
        self.health
            .iter()
            .find(|report| report.id == id)
            .map_or("Selected store", |report| report.label)
    }

    pub(crate) fn set_startup_diagnostics(&mut self, diagnostics: Vec<SafeStartupDiagnostic>) {
        self.startup_notice_pending = !diagnostics.is_empty();
        self.startup_diagnostics = diagnostics;
    }

    pub(crate) fn take_startup_notice(&mut self) -> Option<String> {
        if !self.startup_notice_pending {
            return None;
        }
        self.startup_notice_pending = false;
        Some(format!(
            "Multi Launcher started with {} persistence diagnostic{}. Open Data & Recovery for details.",
            self.startup_diagnostics.len(),
            if self.startup_diagnostics.len() == 1 {
                ""
            } else {
                "s"
            }
        ))
    }

    pub(crate) fn open(&mut self, focus: DataDialogFocus) -> Result<(), String> {
        let was_open = self.open;
        self.open = true;
        self.section = match focus {
            DataDialogFocus::Overview => DataDialogSection::StorageHealth,
            DataDialogFocus::Health => DataDialogSection::StorageHealth,
        };
        if !was_open {
            self.health_deferred = true;
            self.snapshot_list_deferred = true;
            self.schedule_deferred();
        }
        Ok(())
    }

    pub(crate) fn refresh_health(&mut self) -> Result<(), String> {
        self.latest_health = Some(self.submit(DataServiceRequest::ScanHealth)?);
        Ok(())
    }

    pub(crate) fn list_snapshots(&mut self) -> Result<(), String> {
        self.latest_snapshots = Some(self.submit(DataServiceRequest::ListSnapshots)?);
        Ok(())
    }

    fn request_snapshot_list_or_defer(&mut self) {
        let plan = self
            .service
            .as_ref()
            .map(DataService::activity)
            .map_or(ListRequestPlan::Submit, snapshot_list_plan);
        match plan {
            ListRequestPlan::AlreadyRunning => self.snapshot_list_deferred = false,
            ListRequestPlan::Defer => self.snapshot_list_deferred = true,
            ListRequestPlan::Submit => {
                self.snapshot_list_deferred = self.list_snapshots().is_err();
            }
        }
    }

    fn schedule_deferred(&mut self) {
        if self.any_request_busy() {
            return;
        }
        if self.health_deferred {
            self.health_deferred = self.refresh_health().is_err();
        } else if self.snapshot_list_deferred {
            self.request_snapshot_list_or_defer();
        }
    }

    pub(crate) fn request_backup(&mut self) -> Result<(), String> {
        self.latest_backup = Some(self.submit(DataServiceRequest::CreateSnapshot)?);
        self.backup_summary = Some("Creating backup…".into());
        Ok(())
    }

    pub(crate) fn stage_recovery(&mut self, command: &DataRecoveryCommand) -> Result<(), String> {
        let action = match command {
            DataRecoveryCommand::Restore {
                target,
                snapshot_id,
                ..
            } => StagedRecoveryAction::Restore {
                target: *target,
                snapshot_id: snapshot_id.clone(),
            },
            DataRecoveryCommand::Reset { store_id, .. } => StagedRecoveryAction::Reset {
                store_id: *store_id,
            },
        };
        self.latest_recovery = Some(self.submit(DataServiceRequest::StageRecovery(action))?);
        self.recovery_summary = Some("Validating and staging recovery…".into());
        Ok(())
    }

    fn submit(&mut self, request: DataServiceRequest) -> Result<DataRequestId, String> {
        if self.service.is_none() {
            let repaint = Arc::clone(&self.repaint);
            self.service = Some(
                DataService::start_lazy(self.root.clone(), self.settings.clone(), move || {
                    repaint()
                })
                .map_err(|error| format!("Data service could not start: {error}"))?,
            );
        }
        let service = self.service.as_ref().ok_or_else(|| {
            self.service_error
                .clone()
                .unwrap_or_else(|| "Data service is unavailable".into())
        })?;
        let activity = service.activity();
        if activity.active.is_some() || activity.pending.is_some() {
            return Err("Another Data & Recovery operation is already running".into());
        }
        service
            .submit(request)
            .map(|submission| submission.request_id)
            .map_err(|error| format!("Data service rejected the request: {error:?}"))
    }

    pub(crate) fn shutdown(&mut self) {
        if let Some(service) = self.service.as_mut() {
            service.shutdown();
        }
    }

    fn drain_results(&mut self) -> Vec<DataUiNotice> {
        let Some(service) = self.service.as_ref() else {
            return Vec::new();
        };
        let completions = service.drain_results();
        let mut notices = Vec::new();
        for completion in completions {
            let expected = match &completion.request {
                DataServiceRequest::ScanHealth => self.latest_health,
                DataServiceRequest::CreateSnapshot => self.latest_backup,
                DataServiceRequest::ListSnapshots => self.latest_snapshots,
                DataServiceRequest::StageRecovery(_) => self.latest_recovery,
            };
            if expected != Some(completion.request_id) {
                continue;
            }
            match completion.result {
                Ok(DataServiceResult::Health(health)) => {
                    self.health = health;
                }
                Ok(DataServiceResult::Snapshots(snapshots)) => {
                    self.snapshots = snapshots;
                    if self.selected_snapshot.as_ref().is_none_or(|selected| {
                        !self
                            .snapshots
                            .iter()
                            .any(|snapshot| &snapshot.manifest.snapshot_id == selected)
                    }) {
                        self.selected_snapshot = self
                            .snapshots
                            .first()
                            .map(|snapshot| snapshot.manifest.snapshot_id.clone());
                    }
                }
                Ok(DataServiceResult::Snapshot(snapshot)) => {
                    let summary = format!(
                        "Backup {} finished with status {:?}",
                        snapshot.manifest.snapshot_id, snapshot.manifest.status
                    );
                    self.backup_summary = Some(summary.clone());
                    notices.push(DataUiNotice {
                        message: summary,
                        error: snapshot.manifest.status == SnapshotStatus::Failed,
                    });
                    if let Some(warning) = snapshot.retention_warning {
                        notices.push(DataUiNotice {
                            message: warning,
                            error: true,
                        });
                    }
                    self.health_deferred = true;
                    self.snapshot_list_deferred = true;
                }
                Ok(DataServiceResult::RecoveryStaged(pending)) => {
                    let summary = format!(
                        "{:?} is staged. Restart Multi Launcher to apply it; current data will be preserved first.",
                        pending.action
                    );
                    self.recovery_summary = Some(summary.clone());
                    notices.push(DataUiNotice {
                        message: summary,
                        error: false,
                    });
                    self.health_deferred = true;
                    self.snapshot_list_deferred = true;
                }
                Err(failure) => {
                    let message = failure_message(&completion.request, failure);
                    if matches!(completion.request, DataServiceRequest::CreateSnapshot) {
                        self.backup_summary = Some(message.clone());
                    }
                    if matches!(completion.request, DataServiceRequest::StageRecovery(_)) {
                        self.recovery_summary = Some(message.clone());
                    }
                    notices.push(DataUiNotice {
                        message,
                        error: true,
                    });
                    if matches!(
                        completion.request,
                        DataServiceRequest::CreateSnapshot | DataServiceRequest::StageRecovery(_)
                    ) {
                        self.health_deferred = true;
                        self.snapshot_list_deferred = true;
                    }
                }
            }
        }
        self.schedule_deferred();
        notices
    }

    pub(crate) fn ui(
        &mut self,
        ctx: &egui::Context,
    ) -> (Vec<DataRecoveryUiAction>, Vec<DataUiNotice>) {
        let notices = self.drain_results();
        if !self.open {
            return (Vec::new(), notices);
        }
        let mut actions = Vec::new();
        let mut open = self.open;
        egui::Window::new("Data & Recovery")
            .open(&mut open)
            .default_size([760.0, 560.0])
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (section, label) in [
                        (DataDialogSection::StorageHealth, "Storage Health"),
                        (DataDialogSection::Backups, "Backups"),
                        (DataDialogSection::Recovery, "Recovery"),
                        (DataDialogSection::Diagnostics, "Diagnostics"),
                    ] {
                        if ui
                            .selectable_label(self.section == section, label)
                            .clicked()
                        {
                            self.section = section;
                            if section == DataDialogSection::Backups
                                || section == DataDialogSection::Recovery
                            {
                                self.request_snapshot_list_or_defer();
                            }
                        }
                    }
                });
                ui.separator();
                match self.section {
                    DataDialogSection::StorageHealth => self.health_ui(ui, &mut actions),
                    DataDialogSection::Backups => self.backups_ui(ui),
                    DataDialogSection::Recovery => self.recovery_ui(ui, &mut actions),
                    DataDialogSection::Diagnostics => self.diagnostics_ui(ui, &mut actions),
                }
            });
        self.open = open;
        (actions, notices)
    }

    fn health_ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<DataRecoveryUiAction>) {
        let busy = self.any_request_busy();
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!busy, egui::Button::new("Refresh"))
                .clicked()
            {
                if let Err(error) = self.refresh_health() {
                    self.service_error = Some(error);
                }
            }
            if busy {
                ui.label("Checking storage…");
            }
        });
        if let Some(error) = &self.service_error {
            ui.colored_label(egui::Color32::RED, error);
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            egui::Grid::new("data-health-grid")
                .striped(true)
                .show(ui, |ui| {
                    ui.strong("Store");
                    ui.strong("Status");
                    ui.strong("Ownership");
                    ui.strong("Backup");
                    ui.strong("Actions");
                    ui.strong("Path");
                    ui.end_row();
                    for report in &self.health {
                        ui.label(report.label);
                        ui.label(health_label(&report.health));
                        ui.label(format!("{:?}", report.ownership));
                        ui.label(format!("{:?}", report.backup_policy));
                        let targets = store_open_targets(report);
                        ui.horizontal(|ui| {
                            if report.ownership == StoreOwnership::External {
                                ui.label("Inspect only");
                            }
                            if targets.is_empty() {
                                ui.label("Unavailable");
                            }
                            for target in targets {
                                match target {
                                    StoreOpenTarget::File(path) => {
                                        if ui.button("Open file").clicked() {
                                            actions.push(DataRecoveryUiAction::OpenPath(path));
                                        }
                                    }
                                    StoreOpenTarget::Folder(path) => {
                                        if ui.button("Open folder").clicked() {
                                            actions.push(DataRecoveryUiAction::OpenPath(path));
                                        }
                                    }
                                }
                            }
                        });
                        ui.label(report.path.display().to_string()).on_hover_text(
                            match (report.restore_eligible, report.reset_eligible) {
                                (true, true) => "Recovery actions: Restore / Reset",
                                (true, false) => "Recovery actions: Restore",
                                (false, true) => "Recovery actions: Reset",
                                (false, false) => "No automatic recovery actions",
                            },
                        );
                        ui.end_row();
                    }
                });
        });
    }

    fn backups_ui(&mut self, ui: &mut egui::Ui) {
        let backup_busy = self.any_request_busy();
        let list_busy = backup_busy;
        ui.horizontal(|ui| {
            if ui
                .add_enabled(!backup_busy, egui::Button::new("Create backup"))
                .clicked()
            {
                if let Err(error) = self.request_backup() {
                    self.service_error = Some(error);
                }
            }
            if ui
                .add_enabled(!list_busy, egui::Button::new("Refresh list"))
                .clicked()
            {
                if let Err(error) = self.list_snapshots() {
                    self.service_error = Some(error);
                }
            }
        });
        if backup_busy {
            ui.label("Backup worker is active…");
        }
        if let Some(summary) = &self.backup_summary {
            ui.label(summary);
        }
        for snapshot in &self.snapshots {
            ui.radio_value(
                &mut self.selected_snapshot,
                Some(snapshot.manifest.snapshot_id.clone()),
                format!(
                    "{} — {:?} — {} stores",
                    snapshot.manifest.snapshot_id,
                    snapshot.manifest.status,
                    snapshot.manifest.included.len()
                ),
            );
        }
    }

    fn recovery_ui(&mut self, ui: &mut egui::Ui, actions: &mut Vec<DataRecoveryUiAction>) {
        ui.label("Restore or reset is validated now, staged without changing live data, and applied on the next launch. Multi Launcher preserves current data before replacement.");
        if let Some(summary) = &self.recovery_summary {
            ui.colored_label(egui::Color32::YELLOW, summary);
        }
        let selected = self.selected_snapshot.clone();
        let recovery_busy = self.any_request_busy();
        if recovery_busy {
            ui.label("Recovery validation is active…");
        }
        egui::ScrollArea::vertical().show(ui, |ui| {
            for store in &self.health {
                let Some(target) = recovery_target_for_row(store.id) else {
                    continue;
                };
                let target_eligible = target.members().into_iter().all(|member| {
                    self.health.iter().any(|report| {
                        report.id == member
                            && report.ownership == StoreOwnership::ApplicationOwned
                            && report.restore_eligible
                    })
                });
                let restore = restore_enabled(
                    target,
                    store.ownership,
                    target_eligible,
                    selected.as_deref(),
                    &self.snapshots,
                );
                let reset =
                    store.ownership == StoreOwnership::ApplicationOwned && store.reset_eligible;
                ui.horizontal(|ui| {
                    let label = if target == RecoveryTarget::Group(RecoveryGroupId::MkMacro) {
                        "MkMacro document + assets"
                    } else {
                        store.label
                    };
                    ui.label(label)
                        .on_hover_text(store.path.display().to_string());
                    if ui
                        .add_enabled(
                            restore && !recovery_busy,
                            egui::Button::new("Restore selected backup"),
                        )
                        .clicked()
                    {
                        actions.push(DataRecoveryUiAction::Confirm(
                            PendingRecoveryIntent::Restore {
                                target,
                                snapshot_id: selected
                                    .clone()
                                    .expect("enabled restore has snapshot"),
                            },
                        ));
                    }
                    if ui
                        .add_enabled(reset && !recovery_busy, egui::Button::new("Reset"))
                        .clicked()
                    {
                        actions.push(DataRecoveryUiAction::Confirm(
                            PendingRecoveryIntent::Reset { store_id: store.id },
                        ));
                    }
                    if store.ownership == StoreOwnership::External {
                        ui.label("External path — automatic backup/restore disabled");
                    }
                });
            }
        });
    }

    fn diagnostics_ui(&self, ui: &mut egui::Ui, actions: &mut Vec<DataRecoveryUiAction>) {
        ui.label(format!("Multi Launcher {}", env!("CARGO_PKG_VERSION")));
        ui.label(format!("Data root: {}", self.root.path().display()));
        ui.horizontal(|ui| {
            if ui.button("Open data folder").clicked() {
                actions.push(DataRecoveryUiAction::OpenPath(
                    self.root.path().to_path_buf(),
                ));
            }
            if ui.button("Copy diagnostics").clicked() {
                let diagnostics = self.safe_diagnostics_text();
                ui.output_mut(|output| output.copied_text = diagnostics);
            }
        });
        ui.separator();
        for diagnostic in &self.startup_diagnostics {
            ui.strong(&diagnostic.label);
            if let Some(path) = &diagnostic.path {
                ui.label(path.display().to_string());
            }
            ui.label(&diagnostic.summary);
        }
        if let Some(summary) = &self.backup_summary {
            ui.label(format!("Backup: {summary}"));
        }
        if let Some(summary) = &self.recovery_summary {
            ui.label(format!("Recovery: {summary}"));
        }
        for report in &self.health {
            ui.label(format!(
                "{} | {} | {}",
                report.label,
                report.path.display(),
                health_label(&report.health)
            ));
        }
    }

    pub(crate) fn safe_diagnostics_text(&self) -> String {
        let mut lines = vec![
            format!("Multi Launcher {}", env!("CARGO_PKG_VERSION")),
            format!("Data root: {}", self.root.path().display()),
        ];
        for diagnostic in &self.startup_diagnostics {
            lines.push(format!("Startup: {}", diagnostic.label));
            if let Some(path) = &diagnostic.path {
                lines.push(format!("Path: {}", path.display()));
            }
            lines.push(format!("Summary: {}", diagnostic.summary));
        }
        for report in &self.health {
            lines.push(format!(
                "Store: {} | Path: {} | Health: {} | Ownership: {:?} | Backup: {:?}",
                report.label,
                report.path.display(),
                health_label(&report.health),
                report.ownership,
                report.backup_policy,
            ));
        }
        for snapshot in &self.snapshots {
            lines.push(format!(
                "Snapshot: {} | Status: {:?}",
                snapshot.manifest.snapshot_id, snapshot.manifest.status
            ));
        }
        if let Some(summary) = &self.backup_summary {
            lines.push(format!("Backup status: {summary}"));
        }
        if let Some(summary) = &self.recovery_summary {
            lines.push(format!("Recovery status: {summary}"));
        }
        lines.join("\n")
    }

    fn any_request_busy(&self) -> bool {
        self.service.as_ref().is_some_and(|service| {
            let activity = service.activity();
            activity.active.is_some() || activity.pending.is_some()
        })
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum StoreOpenTarget {
    File(PathBuf),
    Folder(PathBuf),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListRequestPlan {
    AlreadyRunning,
    Defer,
    Submit,
}

fn snapshot_list_plan(activity: DataServiceActivity) -> ListRequestPlan {
    if activity
        .active
        .as_ref()
        .is_some_and(|(request, _)| matches!(request, DataServiceRequest::ListSnapshots))
        || activity
            .pending
            .as_ref()
            .is_some_and(|(request, _)| matches!(request, DataServiceRequest::ListSnapshots))
    {
        ListRequestPlan::AlreadyRunning
    } else if activity.active.is_some() || activity.pending.is_some() {
        ListRequestPlan::Defer
    } else {
        ListRequestPlan::Submit
    }
}

fn store_open_targets(report: &StoreHealthReport) -> Vec<StoreOpenTarget> {
    let mut targets = Vec::new();
    let exists = !matches!(report.health, StoreHealth::Missing);
    match report.kind {
        StoreKind::File if exists => targets.push(StoreOpenTarget::File(report.path.clone())),
        StoreKind::Directory if exists => {
            targets.push(StoreOpenTarget::Folder(report.path.clone()));
            return targets;
        }
        _ => {}
    }
    if report.parent_exists
        && let Some(parent) = report.path.parent()
    {
        targets.push(StoreOpenTarget::Folder(parent.to_path_buf()));
    }
    targets
}

fn restore_enabled(
    target: RecoveryTarget,
    ownership: StoreOwnership,
    eligible: bool,
    selected: Option<&str>,
    snapshots: &[SnapshotRecord],
) -> bool {
    ownership == StoreOwnership::ApplicationOwned
        && eligible
        && selected.is_some_and(|selected| {
            snapshots.iter().any(|snapshot| {
                snapshot.manifest.snapshot_id == selected
                    && snapshot.manifest.status != SnapshotStatus::Failed
                    && target.members().into_iter().all(|id| {
                        let name = format!("{id:?}");
                        !snapshot
                            .manifest
                            .failed
                            .iter()
                            .any(|entry| entry.store_id == name)
                            && !snapshot
                                .manifest
                                .skipped
                                .iter()
                                .any(|entry| entry.store_id == name)
                            && snapshot
                                .manifest
                                .included
                                .iter()
                                .any(|entry| entry.store_id == name)
                    })
            })
        })
}

fn recovery_target_for_row(id: PersistentStoreId) -> Option<RecoveryTarget> {
    match id {
        PersistentStoreId::MkMacroDocument => Some(RecoveryTarget::Group(RecoveryGroupId::MkMacro)),
        PersistentStoreId::MkMacroAssets => None,
        id => Some(RecoveryTarget::Store(id)),
    }
}

fn health_label(health: &StoreHealth) -> String {
    match health {
        StoreHealth::Healthy => "Healthy".into(),
        StoreHealth::Missing => "Missing (valid until created)".into(),
        StoreHealth::Empty => "Empty".into(),
        StoreHealth::Malformed { message } => format!("Malformed: {message}"),
        StoreHealth::Unreadable { message } => format!("Unreadable: {message}"),
        StoreHealth::UnsupportedSchema { version } => format!("Unsupported schema: {version}"),
    }
}

fn failure_message(request: &DataServiceRequest, failure: DataServiceFailure) -> String {
    let operation = match request {
        DataServiceRequest::ScanHealth => "Storage health scan",
        DataServiceRequest::CreateSnapshot => "Backup creation",
        DataServiceRequest::ListSnapshots => "Backup listing",
        DataServiceRequest::StageRecovery(_) => "Recovery staging",
    };
    match failure {
        DataServiceFailure::Operation(message) => format!("{operation} failed: {message}"),
        DataServiceFailure::Panicked => format!("{operation} failed unexpectedly"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::persistence::{SnapshotEntry, SnapshotManifest};

    fn record(id: &str, store: PersistentStoreId) -> SnapshotRecord {
        SnapshotRecord {
            path: PathBuf::from(id),
            manifest: SnapshotManifest {
                product: "Multi Launcher".into(),
                format_version: 1,
                app_version: "test".into(),
                snapshot_id: id.into(),
                created_unix_millis: 1,
                source_root: PathBuf::from("root"),
                consistency: "per-file".into(),
                status: SnapshotStatus::Complete,
                included: vec![SnapshotEntry {
                    store_id: format!("{store:?}"),
                    source_path: PathBuf::from("private-content-must-not-be-read.json"),
                    snapshot_path: Some(PathBuf::from("stores/value")),
                    detail: None,
                }],
                missing: vec![],
                skipped: vec![],
                failed: vec![],
                external: vec![],
            },
        }
    }

    #[test]
    fn action_enablement_requires_owned_eligible_included_store() {
        let snapshots = [record("one", PersistentStoreId::Settings)];
        assert!(restore_enabled(
            RecoveryTarget::Store(PersistentStoreId::Settings),
            StoreOwnership::ApplicationOwned,
            true,
            Some("one"),
            &snapshots
        ));
        assert!(!restore_enabled(
            RecoveryTarget::Store(PersistentStoreId::Actions),
            StoreOwnership::ApplicationOwned,
            true,
            Some("one"),
            &snapshots
        ));
        assert!(!restore_enabled(
            RecoveryTarget::Store(PersistentStoreId::Settings),
            StoreOwnership::External,
            true,
            Some("one"),
            &snapshots
        ));
        assert!(!restore_enabled(
            RecoveryTarget::Store(PersistentStoreId::Settings),
            StoreOwnership::ApplicationOwned,
            false,
            Some("one"),
            &snapshots
        ));
    }

    #[test]
    fn mkmacro_group_requires_complete_manifested_document_and_assets() {
        let target = RecoveryTarget::Group(RecoveryGroupId::MkMacro);
        let mut snapshot = record("one", PersistentStoreId::MkMacroDocument);
        assert!(!restore_enabled(
            target,
            StoreOwnership::ApplicationOwned,
            true,
            Some("one"),
            std::slice::from_ref(&snapshot),
        ));
        snapshot.manifest.included.push(SnapshotEntry {
            store_id: format!("{:?}", PersistentStoreId::MkMacroAssets),
            source_path: PathBuf::from("mkmacro_assets"),
            snapshot_path: Some(PathBuf::from("stores/MkMacroAssets")),
            detail: None,
        });
        assert!(restore_enabled(
            target,
            StoreOwnership::ApplicationOwned,
            true,
            Some("one"),
            std::slice::from_ref(&snapshot),
        ));
        snapshot
            .manifest
            .skipped
            .push(crate::persistence::SnapshotEntry {
                store_id: format!("{:?}", PersistentStoreId::MkMacroAssets),
                source_path: PathBuf::from("mkmacro_assets"),
                snapshot_path: None,
                detail: None,
            });
        assert!(!restore_enabled(
            target,
            StoreOwnership::ApplicationOwned,
            true,
            Some("one"),
            std::slice::from_ref(&snapshot),
        ));
    }

    #[test]
    fn mkmacro_has_one_explicit_document_and_assets_restore_row() {
        assert_eq!(
            recovery_target_for_row(PersistentStoreId::MkMacroDocument),
            Some(RecoveryTarget::Group(RecoveryGroupId::MkMacro))
        );
        assert_eq!(
            recovery_target_for_row(PersistentStoreId::MkMacroAssets),
            None
        );
    }

    #[test]
    fn confirmation_cancel_has_no_command_and_confirm_mints_one_typed_command() {
        let intent = PendingRecoveryIntent::Reset {
            store_id: PersistentStoreId::Settings,
        };
        let mut cancelled = Some(intent.clone());
        assert!(resolve_confirmed_intent(&mut cancelled, false).is_none());
        assert!(cancelled.is_none());
        let mut confirmed = Some(intent);
        assert!(matches!(
            resolve_confirmed_intent(&mut confirmed, true),
            Some(DataRecoveryCommand::Reset {
                store_id: PersistentStoreId::Settings,
                ..
            })
        ));
        assert!(
            resolve_confirmed_intent(&mut confirmed, true).is_none(),
            "one confirmation stages exactly once"
        );
    }

    #[test]
    fn confirmation_and_completion_explain_preservation_next_launch_and_restart() {
        let intent = PendingRecoveryIntent::Restore {
            target: RecoveryTarget::Store(PersistentStoreId::Settings),
            snapshot_id: "one".into(),
        };
        let (description, warning) = intent.confirmation_copy("Settings");
        let copy = format!("{description} {warning}").to_lowercase();
        assert!(
            copy.contains("preserved") && copy.contains("next launch") && copy.contains("restart")
        );
    }

    #[test]
    fn grouped_confirmation_preserves_explicit_target() {
        let target = RecoveryTarget::Group(RecoveryGroupId::MkMacro);
        let command = PendingRecoveryIntent::Restore {
            target,
            snapshot_id: "one".into(),
        }
        .confirmed_command();
        assert!(matches!(
            command,
            DataRecoveryCommand::Restore {
                target: RecoveryTarget::Group(RecoveryGroupId::MkMacro),
                ..
            }
        ));
    }

    #[test]
    fn safe_diagnostic_model_contains_metadata_not_store_contents() {
        let diagnostic = SafeStartupDiagnostic {
            label: "Settings startup".into(),
            path: Some(PathBuf::from("settings.json")),
            summary: "malformed JSON at line 1".into(),
        };
        let rendered = format!(
            "{} {:?} {}",
            diagnostic.label, diagnostic.path, diagnostic.summary
        );
        assert!(!rendered.contains("private-content-must-not-be-read"));
    }

    #[test]
    fn missing_health_is_described_as_legitimate() {
        assert_eq!(
            health_label(&StoreHealth::Missing),
            "Missing (valid until created)"
        );
    }

    #[test]
    fn startup_diagnostic_notice_is_one_shot_while_details_remain_available() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let mut dialog = DataRecoveryDialog::new(root, Settings::default(), || {});
        dialog.set_startup_diagnostics(vec![SafeStartupDiagnostic {
            label: "Recovery".into(),
            path: None,
            summary: "pending recovery failed".into(),
        }]);
        assert!(dialog.take_startup_notice().is_some());
        assert!(dialog.take_startup_notice().is_none());
        assert_eq!(dialog.startup_diagnostics.len(), 1);
        dialog.shutdown();
    }

    #[test]
    fn health_scan_is_requested_on_open_transition_not_while_already_open() {
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let mut dialog = DataRecoveryDialog::new(root, Settings::default(), || {});
        assert!(
            dialog.service.is_none(),
            "construction adds no startup worker"
        );
        dialog.open(DataDialogFocus::Overview).unwrap();
        let first = dialog.latest_health;
        assert!(first.is_some());
        dialog.open(DataDialogFocus::Health).unwrap();
        assert_eq!(dialog.latest_health, first);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while dialog.any_request_busy() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let _ = dialog.drain_results();
        assert!(
            dialog.latest_snapshots.is_some(),
            "initial health completion must sequence snapshot listing"
        );
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        while dialog.any_request_busy() {
            assert!(std::time::Instant::now() < deadline);
            std::thread::yield_now();
        }
        let _ = dialog.drain_results();
        dialog.open = false;
        dialog.open(DataDialogFocus::Overview).unwrap();
        assert_ne!(dialog.latest_health, first);
        dialog.shutdown();
    }

    fn health_report(
        path: PathBuf,
        kind: StoreKind,
        health: StoreHealth,
        parent_exists: bool,
        ownership: StoreOwnership,
    ) -> StoreHealthReport {
        StoreHealthReport {
            id: PersistentStoreId::Settings,
            label: "Settings",
            path,
            kind,
            parent_exists,
            restore_eligible: true,
            reset_eligible: true,
            criticality: crate::persistence::StoreCriticality::Critical,
            ownership,
            backup_policy: if ownership == StoreOwnership::External {
                BackupPolicy::ExcludeExternal
            } else {
                BackupPolicy::Include
            },
            privacy: crate::persistence::StorePrivacy::Ordinary,
            externally_configured: ownership == StoreOwnership::External,
            health,
        }
    }

    #[test]
    fn store_open_targets_follow_kind_health_and_catalog_path_without_io() {
        let file = PathBuf::from("profile/settings.json");
        let existing = health_report(
            file.clone(),
            StoreKind::File,
            StoreHealth::Healthy,
            true,
            StoreOwnership::ApplicationOwned,
        );
        assert_eq!(
            store_open_targets(&existing),
            [
                StoreOpenTarget::File(file.clone()),
                StoreOpenTarget::Folder(PathBuf::from("profile")),
            ]
        );

        let missing = health_report(
            file,
            StoreKind::File,
            StoreHealth::Missing,
            true,
            StoreOwnership::ApplicationOwned,
        );
        assert_eq!(
            store_open_targets(&missing),
            [StoreOpenTarget::Folder(PathBuf::from("profile"))]
        );

        let directory = health_report(
            PathBuf::from("external-notes"),
            StoreKind::Directory,
            StoreHealth::Healthy,
            true,
            StoreOwnership::External,
        );
        assert_eq!(
            store_open_targets(&directory),
            [StoreOpenTarget::Folder(PathBuf::from("external-notes"))],
            "external paths remain inspectable"
        );
    }

    #[test]
    fn tab_snapshot_request_defers_behind_busy_health_instead_of_being_lost() {
        assert_eq!(
            snapshot_list_plan(DataServiceActivity {
                active: Some((DataServiceRequest::ScanHealth, DataRequestId(4))),
                pending: None,
            }),
            ListRequestPlan::Defer
        );
        assert_eq!(
            snapshot_list_plan(DataServiceActivity {
                active: Some((DataServiceRequest::ListSnapshots, DataRequestId(5))),
                pending: None,
            }),
            ListRequestPlan::AlreadyRunning
        );
    }

    #[test]
    fn copied_diagnostics_include_required_metadata_without_reading_store_contents() {
        const PRIVATE_CONTENT_SENTINEL: &str = "TOP-SECRET-STORE-CONTENT-7F9A";
        let directory = tempfile::tempdir().unwrap();
        let root = AppDataRoot::from_path(directory.path());
        let settings_path = directory.path().join("settings.json");
        std::fs::write(&settings_path, PRIVATE_CONTENT_SENTINEL).unwrap();
        let mut dialog = DataRecoveryDialog::new(root, Settings::default(), || {});
        dialog.health = vec![health_report(
            settings_path.clone(),
            StoreKind::File,
            StoreHealth::Malformed {
                message: "expected JSON object".into(),
            },
            true,
            StoreOwnership::ApplicationOwned,
        )];
        dialog.backup_summary = Some("latest backup Complete".into());
        dialog.recovery_summary = Some("no recovery pending".into());

        let text = dialog.safe_diagnostics_text();

        assert!(text.contains(env!("CARGO_PKG_VERSION")));
        assert!(text.contains(&directory.path().display().to_string()));
        assert!(text.contains("Store: Settings"));
        assert!(text.contains(&settings_path.display().to_string()));
        assert!(text.contains("Malformed: expected JSON object"));
        assert!(text.contains("Backup status: latest backup Complete"));
        assert!(text.contains("Recovery status: no recovery pending"));
        assert!(!text.contains(PRIVATE_CONTENT_SENTINEL));
        dialog.shutdown();
    }
}
