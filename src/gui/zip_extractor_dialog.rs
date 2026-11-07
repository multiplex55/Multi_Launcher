use std::path::PathBuf;
use std::time::{Duration, Instant};

use eframe::egui::{self, Id, TextStyle, Vec2, Window};

use crate::gui::utility_dialog_size::{
    UtilityDialogSizeWindowExt as _, UtilityWindowSizeDefault, UtilityWindowSizeRecipe,
};

const WINDOW_SIZE_KEY: &str = "zip-extractor";
const PROGRESS_REFRESH_INTERVAL: Duration = Duration::from_millis(120);
const MAX_FILTER_BYTES: usize = 1024;
const MAX_DESTINATION_TEXT_BYTES: usize = 128 * 1024;

#[cfg(windows)]
struct ZipHostFrame {
    root: super::utility_host::ViewportPassProvenance,
    owner: super::utility_host::ViewportPassProvenance,
    host: super::utility_host::HostIdentity,
    epoch: u64,
    source_revision: Option<crate::archive::ArchiveRevision>,
    destination_revision: u64,
    selection_revision: u64,
    actions: Vec<UiAction>,
    copies: Vec<String>,
    request: Option<super::utility_host::UtilityPresentation>,
    close: bool,
    info: Option<egui::ViewportInfo>,
    native: bool,
}
#[cfg(windows)]
#[derive(Default)]
pub(super) struct ZipHostOutcome {
    pub(super) closed: bool,
    pub(super) show_root: bool,
    pub(super) copies: Vec<String>,
}

#[derive(Default)]
pub(crate) struct ZipExtractorDialog {
    pub(crate) open: bool,
    host: super::utility_host::UtilityHostState,
    host_epoch: u64,
    #[cfg(windows)]
    host_pending: Option<ZipHostFrame>,
    #[cfg(windows)]
    host_os_close_pending: bool,
    #[cfg(windows)]
    host_focus: bool,
    #[cfg(windows)]
    host_jump: crate::gui::jump_mode::ui::JumpModeUi,
    #[cfg(windows)]
    host_jump_frame: Option<u64>,
    #[cfg(all(windows, test))]
    test_host_button: Option<(egui::Rect, egui::Rect, bool)>,
    #[cfg(all(windows, test))]
    test_picker_paths: std::collections::VecDeque<Option<PathBuf>>,
    filter: String,
    source_path: Option<PathBuf>,
    destination_path: Option<PathBuf>,
    destination_text: String,
    sort_by_name: bool,
    visible_rows: Vec<usize>,
    status_message: Option<String>,
    #[cfg(windows)]
    confirmed_token: Option<crate::archive::PreviewToken>,
    last_progress_snapshot: Option<Instant>,
    #[cfg(windows)]
    snapshot: Option<crate::archive::ArchiveSessionSnapshot>,
    #[cfg(windows)]
    catalog: Option<std::sync::Arc<crate::archive::ArchiveCatalog>>,
    #[cfg(windows)]
    selection_revision: Option<u64>,
    #[cfg(windows)]
    selected_entries: std::collections::HashSet<crate::archive::ArchiveEntryId>,
    #[cfg(windows)]
    selected_folders: std::collections::HashSet<crate::archive::ArchiveEntryId>,
    #[cfg(windows)]
    report_summary: Option<WriteReportSummary>,
    #[cfg(windows)]
    session_needs_polling: bool,
}

#[cfg(windows)]
#[derive(Clone, Copy)]
struct WriteReportSummary {
    status: crate::archive::ArchiveWriteStatus,
    published_count: usize,
    unstarted_count: usize,
    actual_bytes_written: u64,
    failure: Option<crate::archive::ArchiveWriteFailureKind>,
    cleanup_failed: bool,
}

#[cfg(windows)]
fn summarize_write_report(report: &crate::archive::ArchiveWriteReport) -> WriteReportSummary {
    WriteReportSummary {
        status: report.status,
        published_count: report.published_entries.len(),
        unstarted_count: report.unstarted_entries.len(),
        actual_bytes_written: report.actual_bytes_written,
        failure: report.failure.as_ref().map(|(kind, _)| *kind),
        cleanup_failed: report.cleanup_failed,
    }
}

#[cfg(windows)]
#[derive(Clone, Copy)]
enum SessionAction {
    OpenSelected,
    Preview,
    Confirm(crate::archive::PreviewToken),
    Cancel,
    SetSelection {
        target: crate::archive::SelectionTarget,
        selected: bool,
    },
}

#[cfg(windows)]
enum UiAction {
    PickArchive,
    PickDestination,
    Session(SessionAction),
    SetFilter(String),
    SetDestination(String),
    SetSortByName(bool),
    SetConfirmation {
        token: crate::archive::PreviewToken,
        confirmed: bool,
    },
    Close,
}

impl ZipExtractorDialog {
    pub(crate) fn open(&mut self) {
        if !self.open {
            self.host_epoch = self.host_epoch.wrapping_add(1).max(1);
        }
        self.open = true;
    }

    #[cfg(windows)]
    pub(crate) fn open_with_snapshot(&mut self, snapshot: crate::archive::ArchiveSessionSnapshot) {
        self.open();
        self.update_session_snapshot(snapshot, Instant::now());
    }

    pub(crate) fn clear_presentation(&mut self) {
        self.open = false;
        self.filter.clear();
        self.source_path = None;
        self.destination_path = None;
        self.destination_text.clear();
        self.sort_by_name = false;
        self.visible_rows.clear();
        self.status_message = None;
        self.last_progress_snapshot = None;
        #[cfg(windows)]
        {
            self.confirmed_token = None;
            self.snapshot = None;
            self.catalog = None;
            self.selection_revision = None;
            self.selected_entries.clear();
            self.selected_folders.clear();
            self.report_summary = None;
            self.session_needs_polling = false;
        }
    }

    #[cfg(windows)]
    pub(crate) fn update_session_snapshot(
        &mut self,
        snapshot: crate::archive::ArchiveSessionSnapshot,
        now: Instant,
    ) {
        let catalog_changed = match (&self.catalog, &snapshot.catalog) {
            (Some(current), Some(next)) => !std::sync::Arc::ptr_eq(current, next),
            (None, None) => false,
            _ => true,
        };
        if catalog_changed {
            self.catalog = snapshot.catalog.clone();
            self.filter.clear();
            self.rebuild_visible_rows();
        }
        if self.selection_revision != Some(snapshot.selection_revision) {
            self.selected_entries.clear();
            self.selected_folders.clear();
            for target in snapshot.selection.iter().copied() {
                match target {
                    crate::archive::SelectionTarget::Entry(id) => {
                        self.selected_entries.insert(id);
                    }
                    crate::archive::SelectionTarget::FolderSubtree(id) => {
                        self.selected_folders.insert(id);
                    }
                }
            }
            self.selection_revision = Some(snapshot.selection_revision);
        }

        let next_token = snapshot.preview.as_ref().map(|preview| preview.token);
        if self.confirmed_token != next_token {
            self.confirmed_token = None;
        }
        self.report_summary = snapshot.write_report.as_ref().map(summarize_write_report);
        self.session_needs_polling = matches!(
            snapshot.phase,
            crate::archive::ArchiveSessionPhase::Opening
                | crate::archive::ArchiveSessionPhase::PreparingPreview
                | crate::archive::ArchiveSessionPhase::Extracting
                | crate::archive::ArchiveSessionPhase::Closing
        );
        self.snapshot = Some(snapshot);
        self.last_progress_snapshot = Some(now);
        if let Some(snapshot) = self.snapshot.as_ref()
            && snapshot.catalog.is_none()
        {
            self.catalog = None;
            self.visible_rows.clear();
        }
    }

    #[cfg(windows)]
    pub(crate) fn session_needs_polling(&self) -> bool {
        self.session_needs_polling
    }

    #[cfg(windows)]
    pub(crate) fn progress_refresh_due(&self, now: Instant) -> bool {
        self.open
            && self.snapshot.as_ref().is_some_and(|snapshot| {
                snapshot.phase == crate::archive::ArchiveSessionPhase::Extracting
            })
            && self
                .last_progress_snapshot
                .is_none_or(|last| now.saturating_duration_since(last) >= PROGRESS_REFRESH_INTERVAL)
    }

    #[cfg(windows)]
    fn rebuild_visible_rows(&mut self) {
        self.visible_rows.clear();
        let Some(catalog) = self.catalog.as_ref() else {
            return;
        };
        let query = self.filter.trim().to_lowercase();
        for (index, entry) in catalog.entries().iter().enumerate() {
            if query.is_empty() || entry.name().display_lossy().to_lowercase().contains(&query) {
                self.visible_rows.push(index);
            }
        }
        if self.sort_by_name {
            self.visible_rows.sort_by(|left, right| {
                let left_name = catalog.entries()[*left].name().display_lossy();
                let right_name = catalog.entries()[*right].name().display_lossy();
                left_name
                    .to_lowercase()
                    .cmp(&right_name.to_lowercase())
                    .then_with(|| {
                        catalog.entries()[*left]
                            .id()
                            .ordinal()
                            .cmp(&catalog.entries()[*right].id().ordinal())
                    })
            });
        }
    }

    #[cfg(windows)]
    fn render_host_body(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        owner: bool,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        child_ui: Option<&mut egui::Ui>,
        presentation: Option<super::utility_host::UtilityPresentation>,
    ) -> (
        Vec<UiAction>,
        Vec<String>,
        Option<super::utility_host::UtilityPresentation>,
        bool,
    ) {
        if !self.open {
            return (Vec::new(), Vec::new(), None, false);
        }
        let snapshot = self.snapshot.as_ref();
        let catalog = self.catalog.clone();
        let visible_rows = &self.visible_rows;
        let selected_entries = &self.selected_entries;
        let selected_folders = &self.selected_folders;
        let phase = snapshot.map(|snapshot| snapshot.phase);
        let failure = snapshot.and_then(|snapshot| snapshot.failure);
        let preview = snapshot.and_then(|snapshot| snapshot.preview.clone());
        let plan_issues = snapshot.and_then(|snapshot| snapshot.plan_issues.clone());
        let progress = snapshot.and_then(|snapshot| {
            snapshot.write_progress.as_ref().map(|progress| {
                (
                    progress.current_entry,
                    progress.published_count,
                    progress.actual_bytes_written,
                )
            })
        });
        let selection_count = snapshot.map_or(0, |snapshot| snapshot.selection.len());
        let report_summary = self.report_summary;
        let status_message = self.status_message.as_deref();
        let source_display = self.source_path.as_ref().map(|path| path.to_string_lossy());
        let sort_by_name = self.sort_by_name;
        let filter = &self.filter;
        let destination_text = self.destination_text.clone();
        let mut destination_ready = !destination_text.trim().is_empty();
        let confirmed_token = &self.confirmed_token;
        let mut open = self.open;
        let mut actions = Vec::new();
        let mut visual_order = 0;
        let recipe = UtilityWindowSizeRecipe::new(
            UtilityWindowSizeDefault::outer(Vec2::new(900.0, 620.0)),
            Vec2::new(560.0, 380.0),
            [
                Some(ctx.content_rect().width().max(1.0)),
                Some(ctx.content_rect().height().max(1.0)),
            ],
        );
        let mut request = None;
        let output_start = ctx.output(|o| o.commands.len());
        let mut draw = |ui: &mut egui::Ui| {
            let visible = owner && ui.is_visible() && !ui.is_sizing_pass();
            if let Some(presentation) = presentation {
                let (button, target) =
                    super::utility_host::presentation_button(ui, presentation, visible);
                register_target(
                    jump_scope,
                    ui,
                    &button,
                    "host-presentation",
                    if presentation == super::utility_host::UtilityPresentation::Docked {
                        "Pop Out"
                    } else {
                        "Dock"
                    },
                    crate::gui::jump_mode::TargetKind::Action,
                    &mut visual_order,
                );
                #[cfg(test)]
                {
                    self.test_host_button = Some((button.rect, ui.clip_rect(), button.enabled()));
                }
                if button.clicked() && visible {
                    request = Some(target);
                }
            }
            ui.add_enabled_ui(visible, |ui| {
                    ui.label("Choose a ZIP archive, select entries, and review the destination before extraction.");
                    ui.label("Only supported Stored entries can be extracted. Existing files are never overwritten.");

                    ui.horizontal(|ui| {
                        let response = ui.button("Choose ZIP…");
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "open-archive",
                            "Choose ZIP archive",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::PickArchive);
                        }
                        if let Some(path) = source_display.as_ref() {
                            ui.label(path.as_ref());
                            let response = ui.button("Load / Reload selected ZIP");
                            register_target(
                                jump_scope,
                                ui,
                                &response,
                                "reload-archive",
                                "Load or reload selected ZIP",
                                crate::gui::jump_mode::TargetKind::Action,
                                &mut visual_order,
                            );
                            if response.clicked() {
                                actions.push(UiAction::Session(SessionAction::OpenSelected));
                            }
                        }
                    });

                    ui.horizontal(|ui| {
                        let response = ui.button("Choose destination…");
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "choose-destination",
                            "Choose destination folder",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::PickDestination);
                        }
                        let mut destination_draft = destination_text.clone();
                        let destination_response = ui.add_enabled(
                            !matches!(
                                phase,
                                Some(
                                    crate::archive::ArchiveSessionPhase::Extracting
                                        | crate::archive::ArchiveSessionPhase::Closing
                                )
                            ),
                            egui::TextEdit::singleline(&mut destination_draft)
                                .desired_width(340.0)
                                .hint_text("Destination folder path"),
                        );
                        register_target(
                            jump_scope,
                            ui,
                            &destination_response,
                            "destination-path",
                            "Destination folder path",
                            crate::gui::jump_mode::TargetKind::TextEntry,
                            &mut visual_order,
                        );
                        if destination_response.changed() {
                            if destination_draft.len() > MAX_DESTINATION_TEXT_BYTES {
                                let mut end = MAX_DESTINATION_TEXT_BYTES;
                                while !destination_draft.is_char_boundary(end) {
                                    end -= 1;
                                }
                                destination_draft.truncate(end);
                            }
                            destination_ready = !destination_draft.trim().is_empty();
                            actions.push(UiAction::SetDestination(destination_draft));
                        }
                    });

                    ui.separator();
                    let mut filter_draft = filter.clone();
                    let filter_response = ui.add_enabled(
                        owner,
                        egui::TextEdit::singleline(&mut filter_draft)
                            .hint_text("Filter catalog names"),
                    );
                    register_target(
                        jump_scope,
                        ui,
                        &filter_response,
                        "filter",
                        "Filter archive entries",
                        crate::gui::jump_mode::TargetKind::TextEntry,
                        &mut visual_order,
                    );
                    if filter_response.changed() {
                        if filter_draft.len() > MAX_FILTER_BYTES {
                            let mut end = MAX_FILTER_BYTES;
                            while !filter_draft.is_char_boundary(end) {
                                end -= 1;
                            }
                            filter_draft.truncate(end);
                        }
                        actions.push(UiAction::SetFilter(filter_draft));
                    }
                    ui.horizontal(|ui| {
                        if let Some(catalog) = catalog.as_ref() {
                            ui.label(format!(
                                "{} catalog entries · {} visible · {} selected",
                                catalog.entries().len(),
                                visible_rows.len(),
                                selection_count
                            ));
                        } else {
                            ui.label("No archive catalog is loaded.");
                        }
                        let response = ui.button(if sort_by_name {
                            "Sort: name"
                        } else {
                            "Sort: archive order"
                        });
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "sort",
                            "Change catalog sort order",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::SetSortByName(!sort_by_name));
                        }
                    });

                    if let Some(phase) = phase {
                        ui.label(session_phase_label(phase));
                    }
                    if let Some(failure) = failure {
                        ui.colored_label(ui.visuals().error_fg_color, session_failure_label(failure));
                    }
                    if let Some(message) = status_message {
                        ui.colored_label(ui.visuals().error_fg_color, message);
                    }
                    if let Some((entry, published, bytes)) = progress {
                        ui.label(format!(
                            "Extracting: {} completed · {} written",
                            published,
                            format_bytes(bytes)
                        ));
                        if let Some(entry) = entry {
                            ui.label(format!("Current physical entry {}", entry.ordinal()));
                        }
                    }

                    if let Some(preview) = preview.as_ref() {
                        ui.separator();
                        ui.heading("Preview");
                        ui.label(format!(
                            "{} outputs: {} files, {} folders",
                            preview.output_count, preview.file_count, preview.directory_count
                        ));
                        ui.label(format!(
                            "Declared output: {} · actual limits: {} per file, {} total",
                            format_bytes(preview.declared_output_bytes),
                            format_bytes(preview.captured_policy.max_actual_entry_output_bytes),
                            format_bytes(preview.captured_policy.max_actual_total_output_bytes)
                        ));
                        ui.label("Existing files will be left unchanged. Review the destination before confirming.");
                        if preview.requires_destination_confirmation {
                            ui.label("This destination requires explicit confirmation.");
                        }
                        let mut confirmed = *confirmed_token == Some(preview.token);
                        let response = ui.checkbox(
                            &mut confirmed,
                            "I reviewed this preview and destination",
                        );
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "confirm-destination",
                            "Confirm preview and destination",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.changed() {
                            actions.push(UiAction::SetConfirmation {
                                token: preview.token,
                                confirmed,
                            });
                        }
                        let response = ui.add_enabled(
                            confirmed && owner,
                            egui::Button::new("Confirm and extract"),
                        );
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "confirm-extract",
                            "Confirm and extract selected entries",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::Session(SessionAction::Confirm(preview.token)));
                        }
                    }

                    ui.separator();
                    ui.horizontal(|ui| {
                        let can_preview = catalog.is_some()
                            && selection_count > 0
                            && destination_ready
                            && owner
                            && !matches!(
                                phase,
                                Some(crate::archive::ArchiveSessionPhase::Opening
                                    | crate::archive::ArchiveSessionPhase::PreparingPreview
                                    | crate::archive::ArchiveSessionPhase::Extracting
                                    | crate::archive::ArchiveSessionPhase::Closing)
                            );
                        let response = ui.add_enabled(can_preview, egui::Button::new("Preview selection"));
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "preview",
                            "Preview selected entries",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::Session(SessionAction::Preview));
                        }
                        let can_cancel = matches!(
                            phase,
                            Some(crate::archive::ArchiveSessionPhase::Opening
                                | crate::archive::ArchiveSessionPhase::PreparingPreview
                                | crate::archive::ArchiveSessionPhase::Extracting
                                | crate::archive::ArchiveSessionPhase::Closing)
                        );
                        let response = ui.add_enabled(can_cancel && owner, egui::Button::new("Cancel operation"));
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "cancel",
                            "Cancel archive operation",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::Session(SessionAction::Cancel));
                        }
                        let response = ui.button("Close");
                        register_target(
                            jump_scope,
                            ui,
                            &response,
                            "close",
                            "Close ZIP Extractor",
                            crate::gui::jump_mode::TargetKind::Action,
                            &mut visual_order,
                        );
                        if response.clicked() {
                            actions.push(UiAction::Close);
                        }
                    });

                    if let Some(report) = report_summary {
                        ui.separator();
                        ui.heading(write_status_label(report.status));
                        ui.label(format!(
                            "{} files published · {} not started · {} actual bytes written",
                            report.published_count,
                            report.unstarted_count,
                            format_bytes(report.actual_bytes_written)
                        ));
                        if let Some(failure) = report.failure {
                            ui.colored_label(ui.visuals().error_fg_color, write_failure_label(failure));
                        }
                        if report.cleanup_failed {
                            ui.colored_label(
                                ui.visuals().error_fg_color,
                                "An incomplete staging file could not be cleaned up.",
                            );
                        }
                    }

                    if let Some(issues) = plan_issues.as_ref() {
                        ui.separator();
                        ui.collapsing(
                            format!("Preview found {} issues", issues.issues().len() + issues.omitted_issues()),
                            |ui| {
                                for issue in issues.issues() {
                                    let entry = issue.entry.map_or_else(
                                        || "Selection".to_owned(),
                                        |entry| format!("Entry {}", entry.ordinal()),
                                    );
                                    ui.label(format!("{entry}: {}", plan_issue_label(issue.reason)));
                                }
                                if issues.is_truncated() {
                                    ui.label(format!("{} additional issues omitted", issues.omitted_issues()));
                                }
                            },
                        );
                    }

                    let row_height = ui
                        .text_style_height(&TextStyle::Body)
                        .max(ui.spacing().interact_size.y)
                        .max(1.0);
                    let list_max_height = ui.available_height().max(row_height);
                    egui::ScrollArea::vertical()
                        .id_salt("zip_extractor_catalog_rows")
                        .auto_shrink([false, false])
                        .max_height(list_max_height)
                        .show_rows(ui, row_height, visible_rows.len(), |ui, row_range| {
                            let Some(catalog) = catalog.as_ref() else {
                                return;
                            };
                            for visible_index in row_range {
                                let Some(entry_index) = visible_rows.get(visible_index).copied()
                                else {
                                    continue;
                                };
                                let Some(entry) = catalog.entries().get(entry_index) else {
                                    continue;
                                };
                                let id = entry.id();
                                let target = if entry.kind() == crate::archive::EntryKind::Directory {
                                    crate::archive::SelectionTarget::FolderSubtree(id)
                                } else {
                                    crate::archive::SelectionTarget::Entry(id)
                                };
                                let selected = match target {
                                    crate::archive::SelectionTarget::Entry(id) => {
                                        selected_entries.contains(&id)
                                    }
                                    crate::archive::SelectionTarget::FolderSubtree(id) => {
                                        selected_folders.contains(&id)
                                    }
                                };
                                let mut next_selected = selected;
                                let mut name_clicked = false;
                                let response = ui
                                    .push_id(id, |ui| {
                                        ui.horizontal(|ui| {
                                            let checkbox = ui.add_enabled(
                                                owner,
                                                egui::Checkbox::new(&mut next_selected, ""),
                                            );
                                            let name = entry.name().display_lossy();
                                            name_clicked = ui
                                                .add(
                                                    egui::Label::new(name.as_ref())
                                                        .truncate()
                                                        .sense(egui::Sense::click()),
                                                )
                                                .clicked();
                                            ui.label(format!("{} bytes", entry.uncompressed_size()));
                                            ui.label(entry_support_label(entry));
                                            checkbox
                                        })
                                        .inner
                                    })
                                    .inner;
                                register_target(
                                    jump_scope,
                                    ui,
                                    &response,
                                    format!("selection/{}", id.ordinal()),
                                    format!("Select archive entry {}", entry.name().display_lossy()),
                                    crate::gui::jump_mode::TargetKind::Action,
                                    &mut visual_order,
                                );
                                if response.changed() || name_clicked {
                                    actions.push(UiAction::Session(SessionAction::SetSelection {
                                        target,
                                        selected: if name_clicked {
                                            !selected
                                        } else {
                                            next_selected
                                        },
                                    }));
                                }
                            }
                        });
            });
        };
        if let Some(ui) = child_ui {
            draw(ui);
        } else {
            Window::new("ZIP Extractor")
                .id(Id::new("zip_extractor_dialog"))
                .resizable(true)
                .default_size(Vec2::new(900.0, 620.0))
                .show_utility(
                    ctx,
                    WINDOW_SIZE_KEY,
                    Id::new("zip_extractor_dialog"),
                    Some(&mut open),
                    recipe,
                    |ui| draw(ui),
                );
        }
        let copies = ctx.output_mut(|output| {
            let mut copies = Vec::new();
            for command in output.commands.split_off(output_start) {
                match command {
                    egui::OutputCommand::CopyText(text) => copies.push(text),
                    other => output.commands.push(other),
                }
            }
            copies
        });
        let allowed = first_ui_pass && owner && !ctx.will_discard();
        (
            if allowed { actions } else { Vec::new() },
            if allowed { copies } else { Vec::new() },
            if allowed { request } else { None },
            allowed && !open,
        )
    }
    #[cfg(windows)]
    pub(crate) fn target_generation(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut h = std::collections::hash_map::DefaultHasher::new();
        self.host_epoch.hash(&mut h);
        self.host.identity().generation.hash(&mut h);
        if let Some(snapshot) = &self.snapshot {
            snapshot.source_revision.hash(&mut h);
            snapshot.destination_revision.hash(&mut h);
            snapshot.selection_revision.hash(&mut h);
            snapshot
                .catalog
                .as_ref()
                .map(|catalog| std::sync::Arc::as_ptr(catalog) as usize)
                .hash(&mut h);
            snapshot
                .preview
                .as_ref()
                .map(|preview| preview.token)
                .hash(&mut h);
        }
        h.finish()
    }
    #[cfg(not(windows))]
    pub(crate) fn target_generation(&self) -> u64 {
        self.host_epoch
    }
    pub(crate) fn root_layer(&self) -> Option<egui::LayerId> {
        (self.open
            && (self.host.presentation() == super::utility_host::UtilityPresentation::Docked
                || self.host.embedded_root_layer().is_some()))
        .then(|| crate::gui::jump_mode::ui::NavigationDialogKind::ZipExtractor.layer_id())
        .flatten()
    }
    pub(crate) fn needs_root_ui_when_hidden(&self) -> bool {
        self.open && self.host.presentation() == super::utility_host::UtilityPresentation::Popout
    }
    pub(crate) fn needs_viewport_bootstrap(&self) -> bool {
        self.open && self.host.bootstrap_pending()
    }
    pub(crate) fn request_host_focus(&mut self) {
        self.host.request_focus();
        #[cfg(windows)]
        {
            self.host_focus = true;
        }
    }
    #[cfg(windows)]
    pub(crate) fn viewport() -> egui::ViewportId {
        egui::ViewportId::from_hash_of("zip-extractor-viewport")
    }
    #[cfg(windows)]
    fn native_identity(
        &self,
        host: super::utility_host::HostIdentity,
    ) -> super::utility_window_geometry::NativeViewportIdentity {
        super::utility_window_geometry::NativeViewportIdentity {
            key: crate::settings::UtilityToolKey::parse(WINDOW_SIZE_KEY).expect("stable zip key"),
            viewport_id: Self::viewport(),
            owner_generation: self.host_epoch,
            host_generation: host.generation,
        }
    }
    pub(crate) fn retire_host(&mut self, ctx: &egui::Context) {
        #[cfg(windows)]
        {
            if self.host.presentation() == super::utility_host::UtilityPresentation::Popout {
                let identity = self.native_identity(self.host.identity());
                super::utility_dialog_size::close_native_utility_geometry_viewport(ctx, identity);
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
            }
            self.host_pending = None;
            self.host_os_close_pending = false;
            self.host_focus = false;
            self.host_jump.cancel_all();
            self.host_jump_frame = None;
        }
        #[cfg(not(windows))]
        let _ = ctx;
        self.host.retire_to_docked();
    }
    #[cfg(all(windows, test))]
    pub(crate) fn test_host_button(&self) -> Option<(egui::Rect, egui::Rect, bool)> {
        self.test_host_button
    }
    #[cfg(all(windows, test))]
    pub(crate) fn test_native_jump(&self) -> &crate::gui::jump_mode::ui::JumpModeUi {
        &self.host_jump
    }
    #[cfg(all(windows, test))]
    pub(crate) fn test_picker_queue_len(&self) -> usize {
        self.test_picker_paths.len()
    }
    #[cfg(all(windows, test))]
    pub(super) fn test_presentation(&self) -> super::utility_host::UtilityPresentation {
        self.host.presentation()
    }
    #[cfg(all(windows, test))]
    pub(crate) fn queue_picker_for_test(&mut self, path: Option<PathBuf>) {
        self.test_picker_paths.push_back(path);
    }
    #[cfg(windows)]
    pub(super) fn show_hosted(
        &mut self,
        ctx: &egui::Context,
        session: &crate::archive::ArchiveSession,
        root_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
        instance: Option<u64>,
        root_owner: crate::gui::jump_mode::ui::NavigationOwner,
        input_allowed: bool,
        root_native_owner: bool,
        launcher_jump: &crate::gui::jump_mode::ui::JumpModeUi,
        jump_enabled: bool,
        root_visible: bool,
        first: bool,
        pinned: bool,
        root_shortcut_allowed: bool,
    ) {
        use super::utility_host::{UtilityPresentation, ViewportPassProvenance};
        use crate::gui::jump_mode::ui::{
            NavigationDialogKind, NavigationOwner, NavigationViewportBinding,
        };
        self.host_pending = None;
        if !self.open {
            return;
        }
        if pinned {
            self.host_os_close_pending = false;
        }
        let snapshot = session.snapshot();
        let host = self.host.identity();
        let root = ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
        let mut owner = None;
        let mut staged = None;
        let mut info = None;
        let mut native_owner = false;
        #[cfg(test)]
        {
            self.test_host_button = None;
        }
        if host.host == UtilityPresentation::Docked {
            if root_visible {
                let allowed = root_native_owner && input_allowed && !egui::Popup::is_any_open(ctx);
                let scope = root_scope.map(|s| {
                    if first && !ctx.will_discard() {
                        s.clone()
                    } else {
                        s.registration_only()
                    }
                });
                let shortcut = first
                    && allowed
                    && root_shortcut_allowed
                    && !pinned
                    && ctx.input_mut(|i| {
                        i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                            || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                    });
                let mut result = self.render_host_body(
                    ctx,
                    first,
                    allowed,
                    scope.as_ref(),
                    None,
                    Some(host.host),
                );
                result.3 |= shortcut;
                staged = Some(result);
                owner = Some(root);
            }
        } else {
            let viewport = Self::viewport();
            let builder = egui::ViewportBuilder::default()
                .with_title("ZIP Extractor")
                .with_inner_size([900.0, 620.0])
                .with_min_inner_size([560.0, 380.0])
                .with_resizable(true);
            ctx.show_viewport_immediate(viewport, builder, |child, class| {
                let c = child.ctx().clone();
                let native = class == egui::ViewportClass::Immediate && c.viewport_id() == viewport;
                let child_first = first && child.current_pass_index() == 0;
                if child_first {
                    owner = Some(ViewportPassProvenance::current(&c, c.viewport_id()));
                    native_owner = native;
                    if native {
                        info = Some(child.input(|i| i.viewport().clone()));
                    }
                } else {
                    owner = None;
                    info = None;
                    staged = None;
                }
                let allowed = if native {
                    input_allowed
                        && c.input(|i| i.focused)
                        && c.memory(|m| m.top_modal_layer().is_none())
                        && super::jump_mode::ui::visible_top_middle_layer(&c).is_none()
                        && !egui::Popup::is_any_open(&c)
                        && super::jump_mode::ui::focused_response_layer(&c)
                            .is_none_or(|l| l == egui::LayerId::background())
                } else {
                    input_allowed && root_native_owner && !egui::Popup::is_any_open(&c)
                };
                if native && child_first && child.input(|i| i.viewport().close_requested()) {
                    child.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                    if !pinned {
                        self.host_os_close_pending = true;
                    }
                }
                if native {
                    let (chord, alphabet) = launcher_jump.configuration();
                    self.host_jump.set_configuration(chord, alphabet);
                    self.host_jump.set_enabled(jump_enabled);
                    let frame = c.cumulative_frame_nr();
                    if self.host_jump_frame != Some(frame) {
                        self.host_jump.begin_frame();
                        self.host_jump_frame = Some(frame);
                    }
                    let binding = instance.map(|instance| NavigationViewportBinding {
                        viewport,
                        owner: NavigationOwner::Dialog {
                            kind: NavigationDialogKind::ZipExtractor,
                            instance,
                        },
                        base_layer: egui::LayerId::background(),
                    });
                    let nav = binding.map_or(NavigationOwner::Blocked, |b| {
                        self.host_jump.update_navigation_owner_for_viewport(
                            &c,
                            b,
                            input_allowed && jump_enabled,
                        )
                    });
                    let active = self.host_jump.active_session().is_some();
                    self.host_jump.handle_viewport_input(
                        &c,
                        binding.is_some_and(|b| b.owner == nav),
                        child_first,
                        viewport,
                    );
                    let shortcut = child_first
                        && allowed
                        && !pinned
                        && !active
                        && self.host_jump.active_session().is_none()
                        && c.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    self.host_jump.begin_ui_pass();
                    let generation = self.target_generation();
                    let scope = binding
                        .filter(|b| b.owner == nav)
                        .and_then(|b| {
                            self.host_jump.scope_for_viewport_binding(
                                crate::gui::jump_mode::SurfaceId::dialog(
                                    "zip-extractor",
                                    instance.unwrap_or_default(),
                                ),
                                crate::gui::jump_mode::TargetGeneration(generation),
                                "zip-extractor",
                                b,
                            )
                        })
                        .map(|s| {
                            if child_first && !c.will_discard() {
                                s
                            } else {
                                s.registration_only()
                            }
                        });
                    egui::CentralPanel::default().show(child, |ui| {
                        staged = Some(self.render_host_body(
                            &c,
                            child_first,
                            allowed,
                            scope.as_ref(),
                            Some(ui),
                            Some(host.host),
                        ));
                    });
                    if let Some(result) = &mut staged {
                        result.3 |= shortcut || (!pinned && self.host_os_close_pending);
                    }
                    self.host_jump.finish_ui_pass();
                    if let Some(binding) = binding {
                        let current = self.host_jump.update_navigation_owner_for_viewport(
                            &c,
                            binding,
                            input_allowed && jump_enabled,
                        );
                        if self.host_jump.prepare_pass_for_viewport(
                            &c,
                            binding,
                            current,
                            input_allowed && jump_enabled,
                        ) {
                            super::jump_mode::paint::paint(&c, &self.host_jump);
                        }
                        if !c.will_discard() {
                            self.host_jump.complete_frame_for_viewport(
                                &c,
                                binding,
                                current,
                                input_allowed && jump_enabled,
                            );
                        }
                    }
                } else {
                    let scope = root_scope.map(|s| {
                        if child_first && !c.will_discard() {
                            s.clone()
                        } else {
                            s.registration_only()
                        }
                    });
                    let shortcut = child_first
                        && allowed
                        && root_shortcut_allowed
                        && !pinned
                        && c.input_mut(|i| {
                            i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)
                                || i.consume_key(egui::Modifiers::COMMAND, egui::Key::W)
                        });
                    let mut result = self.render_host_body(
                        &c,
                        child_first,
                        allowed,
                        scope.as_ref(),
                        None,
                        Some(host.host),
                    );
                    result.3 |= shortcut;
                    staged = Some(result);
                }
            });
        }
        let _ = root_owner;
        if let (Some(owner), Some((mut actions, copies, request, mut close))) = (owner, staged) {
            if pinned {
                close = false;
                actions.retain(|action| !matches!(action, UiAction::Close));
            }
            if first && !ctx.will_discard() {
                self.host_pending = Some(ZipHostFrame {
                    root,
                    owner,
                    host,
                    epoch: self.host_epoch,
                    source_revision: snapshot.source_revision,
                    destination_revision: snapshot.destination_revision,
                    selection_revision: snapshot.selection_revision,
                    actions,
                    copies,
                    request,
                    close,
                    info,
                    native: native_owner,
                });
            }
        }
    }
    #[cfg(windows)]
    pub(super) fn on_hosted_frame_complete(
        &mut self,
        ctx: &egui::Context,
        session: &mut crate::archive::ArchiveSession,
    ) -> ZipHostOutcome {
        use super::utility_host::UtilityPresentation;
        let Some(frame) = self.host_pending.take() else {
            return ZipHostOutcome::default();
        };
        let now = session.snapshot();
        if !self.open
            || frame.epoch != self.host_epoch
            || frame.host != self.host.identity()
            || frame.source_revision != now.source_revision
            || frame.destination_revision != now.destination_revision
            || frame.selection_revision != now.selection_revision
            || !frame.root.was_retained(ctx)
            || !frame.owner.was_retained(ctx)
        {
            return ZipHostOutcome::default();
        }
        let identity = self.native_identity(frame.host);
        if let Some(info) = frame.info.as_ref() {
            super::utility_dialog_size::apply_native_utility_geometry_restore(
                ctx,
                egui::ViewportClass::Immediate,
                true,
                "ZIP Extractor",
                identity,
                info,
                egui::vec2(900.0, 620.0),
                Some(egui::vec2(560.0, 380.0)),
                None,
            );
            if self.host_focus {
                ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Focus);
            }
        } else if self.host_focus {
            if let Some(layer) = self.root_layer() {
                ctx.move_to_top(layer);
            }
        }
        self.host_focus = false;
        if frame.close || frame.actions.iter().any(|a| matches!(a, UiAction::Close)) {
            self.retire_host(ctx);
            session.close();
            self.clear_presentation();
            self.update_session_snapshot(session.snapshot(), Instant::now());
            return ZipHostOutcome {
                closed: true,
                ..Default::default()
            };
        }
        let show_root = frame.request == Some(UtilityPresentation::Docked);
        if let Some(next) = frame.request {
            if self.host.transition(next) {
                self.host_jump.cancel_all();
                self.host_jump_frame = None;
                if next == UtilityPresentation::Docked {
                    super::utility_dialog_size::close_native_utility_geometry_viewport(
                        ctx, identity,
                    );
                    ctx.send_viewport_cmd_to(identity.viewport_id, egui::ViewportCommand::Close);
                }
                self.host_focus = true;
                ctx.request_repaint();
            }
        } else {
            for action in frame.actions {
                self.apply_host_action(action, session, ctx, frame.host, frame.epoch, frame.native);
            }
        }
        if frame.host.host == UtilityPresentation::Popout {
            self.host.complete_bootstrap(
                frame.host,
                (!frame.native)
                    .then(|| {
                        crate::gui::jump_mode::ui::NavigationDialogKind::ZipExtractor.layer_id()
                    })
                    .flatten(),
                None,
            );
        }
        ZipHostOutcome {
            closed: false,
            show_root,
            copies: if frame.request.is_none() {
                frame.copies
            } else {
                Vec::new()
            },
        }
    }
    #[cfg(windows)]
    fn apply_host_action(
        &mut self,
        action: UiAction,
        session: &mut crate::archive::ArchiveSession,
        ctx: &egui::Context,
        host: super::utility_host::HostIdentity,
        epoch: u64,
        native: bool,
    ) {
        if matches!(action, UiAction::PickArchive | UiAction::PickDestination) {
            let choose_archive = matches!(action, UiAction::PickArchive);
            let before = session.snapshot();
            let identity = native.then(|| self.native_identity(host));
            let hwnd = super::utility_dialog_size::utility_picker_hwnd(ctx, identity);
            #[cfg(test)]
            let queued = self.test_picker_paths.pop_front();
            #[cfg(test)]
            let selected = if let Some(path) = queued {
                path
            } else {
                self.host_picker(hwnd, choose_archive)
            };
            #[cfg(not(test))]
            let selected = self.host_picker(hwnd, choose_archive);
            let after = session.snapshot();
            if !self.open
                || self.host_epoch != epoch
                || self.host.identity() != host
                || before.source_revision != after.source_revision
                || before.destination_revision != after.destination_revision
                || before.selection_revision != after.selection_revision
            {
                return;
            }
            #[cfg(not(test))]
            if hwnd.is_none()
                || hwnd != super::utility_dialog_size::utility_picker_hwnd(ctx, identity)
            {
                return;
            }
            if let Some(path) = selected {
                if choose_archive {
                    self.load_archive_path(path, session);
                } else {
                    self.select_destination_path(path, session);
                }
            }
        } else {
            self.apply_ui_action(action, session, ctx);
        }
    }
    #[cfg(windows)]
    fn host_picker(&mut self, hwnd: Option<usize>, archive: bool) -> Option<PathBuf> {
        let Some(hwnd) = hwnd.and_then(|h| std::num::NonZeroIsize::new(h as isize)) else {
            self.status_message = Some(
                "Cannot verify the ZIP picker window. Focus the extractor and try again.".into(),
            );
            return None;
        };
        let picker =
            rfd::FileDialog::new().set_parent(&super::add_action_dialog::NativeDialogParent(hwnd));
        if archive {
            picker.add_filter("ZIP archives", &["zip"]).pick_file()
        } else {
            picker.pick_folder()
        }
    }
    #[cfg(windows)]
    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        session: &mut crate::archive::ArchiveSession,
        first: bool,
        scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if !self.open {
            return;
        }
        let snapshot = session.snapshot();
        let replay = scope.map(|scope| {
            if first && !ctx.will_discard() {
                scope.clone()
            } else {
                scope.registration_only()
            }
        });
        let (actions, copies, request, close) =
            self.render_host_body(ctx, first, true, replay.as_ref(), None, None);
        if first && !ctx.will_discard() {
            let root =
                super::utility_host::ViewportPassProvenance::current(ctx, egui::ViewportId::ROOT);
            self.host_pending = Some(ZipHostFrame {
                root,
                owner: root,
                host: self.host.identity(),
                epoch: self.host_epoch,
                source_revision: snapshot.source_revision,
                destination_revision: snapshot.destination_revision,
                selection_revision: snapshot.selection_revision,
                actions,
                copies,
                request,
                close,
                info: None,
                native: false,
            });
        }
    }

    #[cfg(windows)]
    fn apply_ui_action(
        &mut self,
        action: UiAction,
        session: &mut crate::archive::ArchiveSession,
        ctx: &egui::Context,
    ) {
        match action {
            UiAction::PickArchive | UiAction::PickDestination => {
                self.status_message = Some("ZIP pickers require a verified window owner.".into());
            }
            UiAction::Session(action) => self.apply_session_action(action, session),
            UiAction::SetFilter(filter) => {
                if self.filter != filter {
                    self.filter = filter;
                    self.rebuild_visible_rows();
                }
            }
            UiAction::SetDestination(destination) => {
                self.set_destination_text(destination, session);
            }
            UiAction::SetSortByName(sort_by_name) => {
                self.sort_by_name = sort_by_name;
                self.rebuild_visible_rows();
            }
            UiAction::SetConfirmation { token, confirmed } => {
                self.confirmed_token = (confirmed
                    && session
                        .snapshot()
                        .preview
                        .as_ref()
                        .is_some_and(|preview| preview.token == token))
                .then_some(token);
            }
            UiAction::Close => {
                session.close();
                self.clear_presentation();
                self.update_session_snapshot(session.snapshot(), Instant::now());
            }
        }
        ctx.request_repaint();
    }

    #[cfg(windows)]
    fn load_archive_path(&mut self, path: PathBuf, session: &mut crate::archive::ArchiveSession) {
        match session.request_open(path.clone()) {
            Ok(()) => {
                self.source_path = Some(path);
                self.confirmed_token = None;
                self.status_message = None;
                self.update_session_snapshot(session.snapshot(), Instant::now());
            }
            Err(error) => self.status_message = Some(request_error_label(error).into()),
        }
    }

    #[cfg(windows)]
    fn select_destination_path(
        &mut self,
        path: PathBuf,
        session: &mut crate::archive::ArchiveSession,
    ) {
        if self.destination_path.as_ref() == Some(&path) {
            self.destination_text = path.to_string_lossy().into_owned();
            return;
        }
        match session.select_destination(path.clone()) {
            Ok(()) => {
                self.destination_text = path.to_string_lossy().into_owned();
                self.destination_path = Some(path);
                self.confirmed_token = None;
                self.status_message = None;
                self.update_session_snapshot(session.snapshot(), Instant::now());
            }
            Err(error) => self.status_message = Some(request_error_label(error).into()),
        }
    }

    #[cfg(windows)]
    fn set_destination_text(&mut self, text: String, session: &mut crate::archive::ArchiveSession) {
        let next_path = if text.trim().is_empty() {
            None
        } else {
            Some(PathBuf::from(&text))
        };
        if self.destination_path != next_path {
            let session_path = next_path.clone().unwrap_or_default();
            if let Err(error) = session.select_destination(session_path) {
                self.status_message = Some(request_error_label(error).into());
                self.destination_text = self
                    .destination_path
                    .as_ref()
                    .map(|path| path.to_string_lossy().into_owned())
                    .unwrap_or_default();
                return;
            }
            self.destination_path = next_path;
            self.confirmed_token = None;
            self.status_message = None;
            self.update_session_snapshot(session.snapshot(), Instant::now());
        }
        self.destination_text = text;
    }

    #[cfg(windows)]
    fn apply_session_action(
        &mut self,
        action: SessionAction,
        session: &mut crate::archive::ArchiveSession,
    ) {
        let result = match action {
            SessionAction::OpenSelected => {
                let Some(path) = self.source_path.clone() else {
                    self.status_message = Some("Choose a ZIP archive first.".into());
                    return;
                };
                session.request_open(path)
            }
            SessionAction::Preview => session.request_preview(),
            SessionAction::Confirm(token) => {
                if self.confirmed_token != Some(token) {
                    self.status_message =
                        Some("Review and confirm the current preview first.".into());
                    return;
                }
                session.confirm(token)
            }
            SessionAction::Cancel => {
                session.cancel();
                Ok(())
            }
            SessionAction::SetSelection { target, selected } => {
                let Some(snapshot) = self.snapshot.as_ref() else {
                    self.status_message = Some("Load an archive before selecting entries.".into());
                    return;
                };
                let mut next = crate::archive::ArchiveSelection::new();
                for existing in snapshot
                    .selection
                    .iter()
                    .copied()
                    .filter(|item| *item != target)
                {
                    if let Err(error) = next.add(existing, &catalog_limits(snapshot)) {
                        self.status_message = Some(selection_error_label(error).into());
                        return;
                    }
                }
                if selected && let Err(error) = next.add(target, &catalog_limits(snapshot)) {
                    self.status_message = Some(selection_error_label(error).into());
                    return;
                }
                session.set_selection(next)
            }
        };
        match result {
            Ok(()) => {
                self.status_message = None;
                if matches!(action, SessionAction::Preview | SessionAction::OpenSelected) {
                    self.confirmed_token = None;
                }
                if matches!(action, SessionAction::Confirm(_)) {
                    self.source_path = None;
                    self.confirmed_token = None;
                }
                self.update_session_snapshot(session.snapshot(), Instant::now());
            }
            Err(error) => self.status_message = Some(request_error_label(error).into()),
        }
    }

    #[cfg(not(windows))]
    pub(crate) fn show_unsupported(
        &mut self,
        ctx: &egui::Context,
        first_ui_pass: bool,
        jump_scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ) {
        if !self.open {
            return;
        }
        let mut open = self.open;
        let was_open = open;
        let mut close_clicked = false;
        let mut visual_order = 0;
        let recipe = UtilityWindowSizeRecipe::new(
            UtilityWindowSizeDefault::outer(Vec2::new(620.0, 280.0)),
            Vec2::ZERO,
            [None, None],
        );
        Window::new("ZIP Extractor")
            .id(Id::new("zip_extractor_dialog"))
            .default_size(Vec2::new(620.0, 280.0))
            .show_utility(
                ctx,
                WINDOW_SIZE_KEY,
                Id::new("zip_extractor_dialog"),
                Some(&mut open),
                recipe,
                |ui| {
                    ui.label("ZIP extraction is available only in the Windows launcher.");
                    let response = ui.button("Close");
                    register_target(
                        jump_scope,
                        ui,
                        &response,
                        "close",
                        "Close ZIP Extractor",
                        crate::gui::jump_mode::TargetKind::Action,
                        &mut visual_order,
                    );
                    if response.clicked() && first_ui_pass {
                        close_clicked = true;
                    }
                },
            );
        if first_ui_pass {
            if close_clicked {
                open = false;
            }
            self.open = open;
            if was_open && !open {
                self.clear_presentation();
            }
        }
    }
}

#[cfg(windows)]
fn catalog_limits(
    snapshot: &crate::archive::ArchiveSessionSnapshot,
) -> crate::archive::ArchiveLimits {
    snapshot
        .catalog
        .as_ref()
        .map(|catalog| *catalog.limits())
        .unwrap_or_default()
}

#[cfg(windows)]
fn selection_error_label(error: crate::archive::SelectionAddError) -> &'static str {
    match error {
        crate::archive::SelectionAddError::LimitExceeded => {
            "The selection reached the configured entry limit."
        }
        crate::archive::SelectionAddError::Allocation => {
            "The selection could not be updated because memory is unavailable."
        }
    }
}

#[cfg(windows)]
fn request_error_label(error: crate::archive::ArchiveSessionRequestError) -> &'static str {
    match error {
        crate::archive::ArchiveSessionRequestError::Busy => {
            "An archive operation is still retiring. Wait, then try again."
        }
        crate::archive::ArchiveSessionRequestError::NoCatalog => {
            "Load an archive before continuing."
        }
        crate::archive::ArchiveSessionRequestError::NoDestination => {
            "Choose a destination folder before previewing."
        }
        crate::archive::ArchiveSessionRequestError::NoPreview => {
            "Preview the current selection before confirming extraction."
        }
        crate::archive::ArchiveSessionRequestError::StalePreview => {
            "The preview changed. Build and review a new preview."
        }
        crate::archive::ArchiveSessionRequestError::GenerationExhausted => {
            "This archive session cannot accept more changes. Close and reopen it."
        }
        crate::archive::ArchiveSessionRequestError::SpawnFailed => {
            "The archive operation could not be started."
        }
    }
}

#[cfg(windows)]
fn session_phase_label(phase: crate::archive::ArchiveSessionPhase) -> &'static str {
    use crate::archive::ArchiveSessionPhase as Phase;
    match phase {
        Phase::Closed | Phase::Empty => "Choose a ZIP archive to begin.",
        Phase::Opening => "Opening and cataloging the selected archive…",
        Phase::Ready => "Archive catalog is ready.",
        Phase::PreparingPreview => "Checking the selected entries and destination…",
        Phase::PreviewReady => "Review the preview before extraction.",
        Phase::PreviewRejected => "The current selection has preview issues.",
        Phase::Extracting => "Extraction is in progress.",
        Phase::Finished => "The extraction run has retired.",
        Phase::Failed => "The archive operation failed.",
        Phase::Closing => "Cancelling and retiring the archive operation…",
    }
}

#[cfg(windows)]
fn session_failure_label(failure: crate::archive::ArchiveSessionFailure) -> &'static str {
    use crate::archive::ArchiveSessionFailure as Failure;
    match failure {
        Failure::Cancelled => "Cancellation was requested; waiting for safe retirement.",
        Failure::SourceAdmission => {
            "The selected file could not be admitted as a safe archive source."
        }
        Failure::CatalogRead => "The archive catalog could not be read safely.",
        Failure::SourceChanged => {
            "The archive changed during this session. Reload it before continuing."
        }
        Failure::DestinationAdmission => "The destination folder could not be admitted safely.",
        Failure::PlanAllocation => {
            "The preview could not be allocated within the available memory."
        }
        Failure::PlanLimit => "The preview exceeded a configured safety limit.",
        Failure::Layout(kind) => write_failure_label(kind),
        Failure::PreparationStart => "The archive preparation operation could not start.",
        Failure::WorkerPanicked => "The archive worker stopped unexpectedly.",
        Failure::WriterStart => "The extraction worker could not start.",
    }
}

#[cfg(windows)]
fn write_status_label(status: crate::archive::ArchiveWriteStatus) -> &'static str {
    match status {
        crate::archive::ArchiveWriteStatus::Completed => "Extraction completed",
        crate::archive::ArchiveWriteStatus::Cancelled => "Extraction cancelled",
        crate::archive::ArchiveWriteStatus::Partial => "Extraction partially completed",
        crate::archive::ArchiveWriteStatus::Failed => "Extraction failed",
    }
}

#[cfg(windows)]
fn write_failure_label(kind: crate::archive::ArchiveWriteFailureKind) -> &'static str {
    use crate::archive::ArchiveWriteFailureKind as Failure;
    match kind {
        Failure::Cancelled => "The current file was cancelled before publication.",
        Failure::InvalidPlan => "The extraction plan was no longer valid.",
        Failure::SourceChanged => "The archive source changed or failed validation.",
        Failure::UnsupportedZipLayout => "An entry uses an unsupported ZIP layout.",
        Failure::TruncatedPayload => "An entry ended before its declared data was complete.",
        Failure::IntegrityMismatch => "An entry failed its integrity check.",
        Failure::QuotaExceeded => "Actual output exceeded a configured safety limit.",
        Failure::NativeIo => "A native file operation failed.",
        Failure::Allocation => "The extraction could not allocate bounded working memory.",
        Failure::WorkerPanicked => "The extraction worker stopped unexpectedly.",
    }
}

#[cfg(windows)]
fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    if bytes >= 1024 * 1024 * 1024 {
        format!("{:.2} GiB", bytes as f64 / GIB)
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MiB", bytes as f64 / (1024.0 * 1024.0))
    } else if bytes >= 1024 {
        format!("{:.1} KiB", bytes as f64 / 1024.0)
    } else {
        format!("{bytes} B")
    }
}

#[cfg(windows)]
fn entry_support_label(entry: &crate::archive::ArchiveEntry) -> String {
    use crate::archive::{
        EntryCompression, NameEncodingProblem, UnsafeEntryReason, UnsafePathReason,
    };
    let mut details = match entry.compression() {
        EntryCompression::Stored => "Stored".to_owned(),
        EntryCompression::Deflated => "Deflated · preview only".to_owned(),
        EntryCompression::Unsupported(code) => format!("Unsupported method {code}"),
    };
    let problems = entry.problems();
    if problems.encrypted {
        details.push_str(" · encrypted");
    }
    if problems.unsupported_flags != 0 {
        details.push_str(" · unsupported flags");
    }
    if problems.inconsistent_stored_size {
        details.push_str(" · inconsistent Stored sizes");
    }
    if let Some(reason) = problems.name_encoding {
        details.push_str(match reason {
            NameEncodingProblem::InvalidUtf8 => " · invalid UTF-8 name",
            NameEncodingProblem::UnsupportedLegacyEncoding => " · unsupported name encoding",
        });
    }
    if let Some(reason) = problems.unsafe_path {
        details.push_str(" · ");
        details.push_str(match reason {
            UnsafePathReason::Empty => "empty path",
            UnsafePathReason::Rooted => "rooted path",
            UnsafePathReason::EmptyComponent => "empty path component",
            UnsafePathReason::CurrentDirectoryComponent => "current-directory component",
            UnsafePathReason::ParentDirectoryComponent => "parent-directory component",
            UnsafePathReason::ForbiddenCharacter => "forbidden path character",
            UnsafePathReason::ControlCharacter => "control character in path",
            UnsafePathReason::TrailingDotOrSpace => "trailing dot or space",
            UnsafePathReason::ReservedDeviceName => "reserved device name",
            UnsafePathReason::ComponentTooLong => "path component too long",
            UnsafePathReason::TooDeep => "path too deep",
            UnsafePathReason::RelativePathTooLong => "relative path too long",
        });
    }
    if let Some(reason) = problems.unsafe_kind {
        details.push_str(" · ");
        details.push_str(match reason {
            UnsafeEntryReason::SymbolicLink => "symbolic link",
            UnsafeEntryReason::ReparsePoint => "reparse point",
            UnsafeEntryReason::DeviceOrSpecialFile => "device or special file",
            UnsafeEntryReason::UnsupportedFileKind => "unsupported file kind",
        });
    }
    if entry.kind() == crate::archive::EntryKind::Directory {
        details.push_str(" · folder subtree");
    }
    details
}

#[cfg(windows)]
fn plan_issue_label(reason: crate::archive::PlanIssueReason) -> &'static str {
    use crate::archive::PlanIssueReason as Issue;
    match reason {
        Issue::EmptySelection => "No entries were selected.",
        Issue::StaleRevision => "The selection belongs to an older archive revision.",
        Issue::UnknownEntryOrdinal => "The selected physical entry no longer exists.",
        Issue::FolderSelectionRequiresDirectory => "A folder selection did not name a directory.",
        Issue::InvalidNameEncoding(_) => "The entry name encoding is unsupported.",
        Issue::UnsafePath(_) => "The entry path is unsafe.",
        Issue::UnsafeEntryKind(_) => "The entry type is unsupported.",
        Issue::EncryptedEntry => "Encrypted entries cannot be extracted.",
        Issue::UnsupportedFlags(_) => "The entry uses unsupported ZIP flags.",
        Issue::UnsupportedCompression(_) => "Only Stored entries can be extracted.",
        Issue::InconsistentStoredSize => "The Stored entry has inconsistent sizes.",
        Issue::DirectoryHasPayload => "A directory entry contains unexpected data.",
        Issue::EntryDeclaredOutputTooLarge => "The entry exceeds its declared output limit.",
        Issue::ExpansionRatioExceeded => "The entry exceeds its expansion-ratio limit.",
        Issue::TotalDeclaredOutputTooLarge => "The selection exceeds its declared output limit.",
        Issue::DuplicateFileOutputPath => "Selected entries collide at the same output path.",
        Issue::FileDirectoryOutputCollision => "A file and directory share an output path.",
        Issue::FileIsOutputAncestor => "A file would be an ancestor of another output.",
        Issue::ExistingFileAtOutput => "A file already exists at an output path.",
        Issue::ExistingDirectoryAtFileOutput => "A directory already exists at a file output.",
        Issue::ExistingFileAncestor => "A file blocks a required parent directory.",
        Issue::ExistingUnsupportedObject => "An unsupported object exists at an output path.",
        Issue::ExistingReparsePoint => "A reparse point exists in the destination path.",
        Issue::OccupancyProbeFailed(_) => "The destination could not be checked safely.",
    }
}

#[cfg(windows)]
fn register_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    );
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write as _;
    use std::thread;
    use zip::write::SimpleFileOptions;

    #[test]
    fn cleanup_feedback_uses_the_typed_report_flag() {
        fn report(message: &str, cleanup_failed: bool) -> crate::archive::ArchiveWriteReport {
            crate::archive::ArchiveWriteReport {
                run_id: 1,
                revision: crate::archive::ArchiveRevision::new(1),
                generation: 1,
                status: crate::archive::ArchiveWriteStatus::Failed,
                phase: crate::archive::ArchiveWritePhase::Retired,
                published_entries: Vec::new(),
                failed_entry: None,
                cancelled_entry: None,
                unstarted_entries: Vec::new(),
                created_directories: 0,
                actual_bytes_written: 0,
                failure: Some((
                    crate::archive::ArchiveWriteFailureKind::NativeIo,
                    message.to_owned(),
                )),
                cleanup_failed,
            }
        }

        let successful_cleanup =
            summarize_write_report(&report("owned-stage cleanup completed", false));
        assert!(!successful_cleanup.cleanup_failed);
        let failed_cleanup = summarize_write_report(&report("native operation failed", true));
        assert!(failed_cleanup.cleanup_failed);
    }

    fn render_dialog_frame(
        ctx: &egui::Context,
        dialog: &mut ZipExtractorDialog,
        session: &mut crate::archive::ArchiveSession,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        frame: &mut u64,
        events: Vec<egui::Event>,
    ) {
        jump.begin_frame();
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(1_200.0, 900.0),
            )),
            time: Some(1.0 + *frame as f64 / 60.0),
            focused: true,
            events,
            ..Default::default()
        };
        *frame = (*frame).saturating_add(1);
        let output = ctx.run_ui(input, |ctx| {
            jump.begin_ui_pass();
            let kind = crate::gui::jump_mode::ui::NavigationDialogKind::ZipExtractor;
            let scope = jump
                .scope_for_navigation_owner(
                    crate::gui::jump_mode::SurfaceId::dialog("zip-extractor", 1),
                    crate::gui::jump_mode::TargetGeneration(1),
                    "zip-extractor",
                    ctx.viewport_id(),
                    crate::gui::jump_mode::ui::NavigationOwner::Dialog { kind, instance: 1 },
                    false,
                )
                .expect("enabled Jump Mode owns this actual egui pass");
            dialog.show(ctx, session, true, Some(&scope));
            jump.finish_ui_pass();
        });
        let _ = dialog.on_hosted_frame_complete(ctx, session);
        output.drop_without_applying_deltas();
    }

    fn click_registered_target(
        ctx: &egui::Context,
        dialog: &mut ZipExtractorDialog,
        session: &mut crate::archive::ArchiveSession,
        jump: &mut crate::gui::jump_mode::ui::JumpModeUi,
        frame: &mut u64,
        label: &str,
    ) {
        let target = jump
            .pass_registry()
            .expect("dialog UI pass was registered")
            .targets()
            .iter()
            .find(|target| target.metadata().label() == label)
            .cloned()
            .unwrap_or_else(|| panic!("actual dialog target {label:?} was not rendered"));
        assert!(target.enabled(), "target {label:?} must be enabled");
        assert!(target.eligible(), "target {label:?} must be reachable");
        assert_eq!(
            target.clipped_rect(),
            target.rect(),
            "target {label:?} must be entirely inside the actual UI clip"
        );
        let position = target.rect().center();
        render_dialog_frame(
            ctx,
            dialog,
            session,
            jump,
            frame,
            vec![egui::Event::PointerMoved(position)],
        );
        for pressed in [true, false] {
            render_dialog_frame(
                ctx,
                dialog,
                session,
                jump,
                frame,
                vec![egui::Event::PointerButton {
                    pos: position,
                    button: egui::PointerButton::Primary,
                    pressed,
                    modifiers: egui::Modifiers::NONE,
                }],
            );
        }
    }

    fn create_small_archive(path: &std::path::Path, entry_count: usize) {
        let file = fs::File::create(path).expect("create task-owned ZIP fixture");
        let mut zip = zip::ZipWriter::new(file);
        for index in 0..entry_count {
            zip.start_file(
                format!("file-{index}.bin"),
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("write Stored ZIP fixture entry");
            zip.write_all(b"payload")
                .expect("write task-owned ZIP payload");
        }
        zip.finish().expect("finish task-owned ZIP fixture");
    }

    fn settle(
        session: &mut crate::archive::ArchiveSession,
        dialog: &mut ZipExtractorDialog,
    ) -> crate::archive::ArchiveSessionSnapshot {
        let deadline = Instant::now() + Duration::from_secs(15);
        loop {
            if session.poll() {
                let snapshot = session.snapshot();
                dialog.update_session_snapshot(snapshot.clone(), Instant::now());
                return snapshot;
            }
            let snapshot = session.snapshot();
            if !matches!(
                snapshot.phase,
                crate::archive::ArchiveSessionPhase::Opening
                    | crate::archive::ArchiveSessionPhase::PreparingPreview
                    | crate::archive::ArchiveSessionPhase::Extracting
                    | crate::archive::ArchiveSessionPhase::Closing
            ) {
                dialog.update_session_snapshot(snapshot.clone(), Instant::now());
                return snapshot;
            }
            assert!(
                Instant::now() < deadline,
                "ZIP Extractor action did not retire within its bounded fixture wait"
            );
            thread::yield_now();
        }
    }

    fn duplicate_name_archive(path: &std::path::Path) {
        let file = fs::File::create(path).expect("create task-owned ZIP fixture");
        let mut zip = zip::ZipWriter::new(file);
        for (name, bytes) in [
            ("same.bin", &b"first"[..]),
            ("name.bin", &b"second"[..]),
            ("other.txt", &b"not selected"[..]),
        ] {
            zip.start_file(
                name,
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored),
            )
            .expect("write unique ZIP fixture entry");
            zip.write_all(bytes).expect("write fixture payload");
        }
        zip.finish().expect("finish task-owned ZIP fixture");

        // zip 9 rejects duplicate names through ZipWriter. Patch both the
        // local and central name fields to create two actual physical rows.
        let mut bytes = fs::read(path).expect("read task-owned ZIP fixture");
        let occurrences = bytes
            .windows(b"name.bin".len())
            .enumerate()
            .filter_map(|(index, window)| (window == b"name.bin").then_some(index))
            .collect::<Vec<_>>();
        assert_eq!(occurrences.len(), 2);
        for index in occurrences {
            bytes[index..index + b"same.bin".len()].copy_from_slice(b"same.bin");
        }
        fs::write(path, bytes).expect("install duplicate physical entry name");
    }

    #[test]
    fn zip_extractor_actions_select_physical_duplicate_and_publish_one_stored_file() {
        let temp = tempfile::tempdir().expect("create task-owned extractor fixture");
        let archive = temp.path().join("source.zip");
        let destination = temp.path().join("destination");
        let revised_destination = temp.path().join("revised-destination");
        fs::create_dir(&destination).expect("create task-owned destination");
        fs::create_dir(&revised_destination).expect("create task-owned revised destination");
        duplicate_name_archive(&archive);

        let ctx = egui::Context::default();
        let mut dialog = ZipExtractorDialog::default();
        dialog.open();
        let mut session =
            crate::archive::ArchiveSession::new(crate::archive::ArchiveLimits::default());
        dialog.load_archive_path(archive, &mut session);
        let opened = settle(&mut session, &mut dialog);
        assert_eq!(opened.phase, crate::archive::ArchiveSessionPhase::Ready);
        let catalog = opened.catalog.as_ref().expect("opened physical catalog");
        let first = catalog.entries()[0].id();
        let second = catalog.entries()[1].id();
        assert_eq!(catalog.entries()[0].name().display_lossy(), "same.bin");
        assert_eq!(catalog.entries()[1].name().display_lossy(), "same.bin");
        assert_ne!(first, second);

        dialog.apply_ui_action(
            UiAction::Session(SessionAction::SetSelection {
                target: crate::archive::SelectionTarget::Entry(second),
                selected: true,
            }),
            &mut session,
            &ctx,
        );
        dialog.filter = "other".into();
        dialog.rebuild_visible_rows();
        assert_eq!(dialog.visible_rows, [2]);
        assert!(dialog.selected_entries.contains(&second));
        assert!(!dialog.selected_entries.contains(&first));

        dialog.select_destination_path(destination.clone(), &mut session);
        dialog.apply_ui_action(
            UiAction::Session(SessionAction::Preview),
            &mut session,
            &ctx,
        );
        let preview = settle(&mut session, &mut dialog);
        let token = preview
            .preview
            .as_ref()
            .expect("current selection and destination produce a preview")
            .token;
        assert_eq!(preview.preview.as_ref().unwrap().file_count, 1);
        assert_eq!(
            preview.selection.as_ref(),
            &[crate::archive::SelectionTarget::Entry(second)]
        );
        assert!(
            fs::read_dir(&destination)
                .expect("inspect destination after preview")
                .next()
                .is_none(),
            "preview is read-only and creates no output or staging files"
        );

        dialog.apply_ui_action(
            UiAction::SetConfirmation {
                token,
                confirmed: true,
            },
            &mut session,
            &ctx,
        );
        dialog.apply_ui_action(
            UiAction::SetDestination(revised_destination.to_string_lossy().into_owned()),
            &mut session,
            &ctx,
        );
        assert_eq!(dialog.confirmed_token, None);
        assert_eq!(
            session.confirm(token),
            Err(crate::archive::ArchiveSessionRequestError::StalePreview),
            "changing destination invalidates the previous confirmation token"
        );
        dialog.apply_ui_action(
            UiAction::Session(SessionAction::Preview),
            &mut session,
            &ctx,
        );
        let revised_preview = settle(&mut session, &mut dialog);
        let revised_token = revised_preview
            .preview
            .as_ref()
            .expect("changed destination produces a new preview")
            .token;
        assert_ne!(revised_token, token);
        dialog.apply_ui_action(
            UiAction::SetConfirmation {
                token: revised_token,
                confirmed: true,
            },
            &mut session,
            &ctx,
        );
        dialog.apply_ui_action(
            UiAction::Session(SessionAction::Confirm(revised_token)),
            &mut session,
            &ctx,
        );
        assert_eq!(
            session.confirm(revised_token),
            Err(crate::archive::ArchiveSessionRequestError::Busy),
            "a single confirmation starts only one extraction worker"
        );
        assert_eq!(
            session.snapshot().phase,
            crate::archive::ArchiveSessionPhase::Extracting
        );
        assert!(session.snapshot().catalog.is_none());
        let completed = settle(&mut session, &mut dialog);
        assert_eq!(
            completed.phase,
            crate::archive::ArchiveSessionPhase::Finished
        );
        assert_eq!(
            dialog.report_summary.map(|summary| summary.status),
            Some(crate::archive::ArchiveWriteStatus::Completed)
        );
        assert_eq!(
            dialog.report_summary.map(|summary| summary.published_count),
            Some(1)
        );
        assert_eq!(
            fs::read(revised_destination.join("same.bin")).expect("read published output"),
            b"second"
        );
        assert!(!revised_destination.join("other.txt").exists());
        session.shutdown();
    }

    #[test]
    fn workflow_controls_stay_reachable_and_drive_preview_confirmation_and_extract() {
        for entry_count in [1, 12] {
            let temp = tempfile::tempdir().expect("create task-owned rendered workflow fixture");
            let archive = temp.path().join("source.zip");
            let destination = temp.path().join("destination");
            fs::create_dir(&destination).expect("create task-owned destination");
            create_small_archive(&archive, entry_count);

            let ctx = egui::Context::default();
            let mut jump = crate::gui::jump_mode::ui::JumpModeUi::new(true);
            let mut frame = 0;
            let mut dialog = ZipExtractorDialog::default();
            dialog.open();
            let mut session =
                crate::archive::ArchiveSession::new(crate::archive::ArchiveLimits::default());
            dialog.load_archive_path(archive, &mut session);
            let opened = settle(&mut session, &mut dialog);
            assert_eq!(opened.phase, crate::archive::ArchiveSessionPhase::Ready);
            let selected = opened
                .catalog
                .as_ref()
                .expect("rendered workflow has an opened catalog")
                .entries()[0]
                .id();
            dialog.apply_ui_action(
                UiAction::Session(SessionAction::SetSelection {
                    target: crate::archive::SelectionTarget::Entry(selected),
                    selected: true,
                }),
                &mut session,
                &ctx,
            );
            dialog.select_destination_path(destination.clone(), &mut session);

            for _ in 0..3 {
                render_dialog_frame(
                    &ctx,
                    &mut dialog,
                    &mut session,
                    &mut jump,
                    &mut frame,
                    vec![],
                );
            }
            click_registered_target(
                &ctx,
                &mut dialog,
                &mut session,
                &mut jump,
                &mut frame,
                "Preview selected entries",
            );
            let previewed = settle(&mut session, &mut dialog);
            let preview = previewed
                .preview
                .as_ref()
                .expect("actual Preview button starts a read-only plan");
            assert_eq!(preview.file_count, 1);
            assert!(
                fs::read_dir(&destination)
                    .expect("inspect preview destination")
                    .next()
                    .is_none(),
                "preview activation does not create destination output"
            );

            for _ in 0..3 {
                render_dialog_frame(
                    &ctx,
                    &mut dialog,
                    &mut session,
                    &mut jump,
                    &mut frame,
                    vec![],
                );
            }
            click_registered_target(
                &ctx,
                &mut dialog,
                &mut session,
                &mut jump,
                &mut frame,
                "Confirm preview and destination",
            );
            for _ in 0..3 {
                render_dialog_frame(
                    &ctx,
                    &mut dialog,
                    &mut session,
                    &mut jump,
                    &mut frame,
                    vec![],
                );
            }
            click_registered_target(
                &ctx,
                &mut dialog,
                &mut session,
                &mut jump,
                &mut frame,
                "Confirm and extract selected entries",
            );

            let completed = settle(&mut session, &mut dialog);
            assert_eq!(
                completed.phase,
                crate::archive::ArchiveSessionPhase::Finished
            );
            assert_eq!(
                completed.write_report.as_ref().map(|report| report.status),
                Some(crate::archive::ArchiveWriteStatus::Completed)
            );
            assert_eq!(
                fs::read(destination.join("file-0.bin")).expect("read extracted fixture"),
                b"payload"
            );
            assert_eq!(
                fs::read_dir(&destination)
                    .expect("inspect completed destination")
                    .count(),
                1,
                "one actual confirmation publishes exactly the selected file"
            );
            session.shutdown();
        }
    }
}

#[cfg(not(windows))]
fn register_target(
    scope: Option<&crate::gui::jump_mode::ui::JumpModeScope>,
    ui: &egui::Ui,
    response: &egui::Response,
    identity: impl AsRef<str>,
    label: impl Into<String>,
    kind: crate::gui::jump_mode::TargetKind,
    visual_order: &mut u64,
) {
    let _ = crate::gui::jump_mode::ui::register_and_apply_target(
        scope,
        ui,
        response,
        identity,
        label,
        kind,
        visual_order,
    );
}
