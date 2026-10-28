use std::time::{Duration, Instant};

use crate::activity::{ActivityCenter, ActivityOwner, ActivityRow, ActivitySnapshot};
use eframe::egui;
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

pub(crate) const ACTIVITY_REFRESH_INTERVAL: Duration = Duration::from_millis(250);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ActivityDialogAction {
    Navigate(ActivityOwner),
}

#[derive(Default)]
pub(crate) struct ActivityDialogState {
    pub(crate) open: bool,
    filter: String,
    snapshot: ActivitySnapshot,
    current_process_working_set_bytes: Option<u64>,
    current_process_private_bytes: Option<u64>,
    pinboard_accounted_image_bytes: Option<u64>,
    last_refresh: Option<Instant>,
    #[cfg(test)]
    refresh_count: usize,
}

impl ActivityDialogState {
    pub(crate) fn open(&mut self) {
        self.open = true;
        self.last_refresh = None;
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    pub(crate) fn filter(&self) -> &str {
        &self.filter
    }

    pub(crate) fn set_filter(&mut self, filter: String) {
        self.filter = filter;
    }

    pub(crate) fn snapshot(&self) -> &ActivitySnapshot {
        &self.snapshot
    }

    /// Refresh is demand-driven by a visible dialog frame and limited to 4 Hz.
    /// Closed dialogs do no reads and do not create repaint demand.
    pub(crate) fn refresh_if_due(
        &mut self,
        now: Instant,
        center: &ActivityCenter,
        view_visible: bool,
    ) -> bool {
        self.refresh_if_due_with_sampler(now, center, view_visible, sample_current_process_memory)
    }

    fn refresh_if_due_with_sampler(
        &mut self,
        now: Instant,
        center: &ActivityCenter,
        view_visible: bool,
        sample_process_memory: impl FnOnce() -> (Option<u64>, Option<u64>),
    ) -> bool {
        if !self.open
            || !view_visible
            || self
                .last_refresh
                .is_some_and(|last| now.saturating_duration_since(last) < ACTIVITY_REFRESH_INTERVAL)
        {
            return false;
        }
        self.snapshot = center.snapshot();
        (
            self.current_process_working_set_bytes,
            self.current_process_private_bytes,
        ) = sample_process_memory();
        self.last_refresh = Some(now);
        #[cfg(test)]
        {
            self.refresh_count = self.refresh_count.saturating_add(1);
        }
        true
    }

    pub(crate) fn show(
        &mut self,
        ctx: &egui::Context,
        center: &ActivityCenter,
        view_visible: bool,
        screenshot_editor_available: bool,
        sample_pinboard_bytes: impl FnOnce() -> Option<u64>,
    ) -> Vec<ActivityDialogAction> {
        self.show_with_samplers(
            ctx,
            center,
            view_visible,
            screenshot_editor_available,
            sample_current_process_memory,
            sample_pinboard_bytes,
        )
    }

    fn show_with_samplers(
        &mut self,
        ctx: &egui::Context,
        center: &ActivityCenter,
        view_visible: bool,
        screenshot_editor_available: bool,
        sample_process_memory: impl FnOnce() -> (Option<u64>, Option<u64>),
        sample_pinboard_bytes: impl FnOnce() -> Option<u64>,
    ) -> Vec<ActivityDialogAction> {
        if !self.open || !view_visible {
            return Vec::new();
        }
        if self.refresh_if_due_with_sampler(
            Instant::now(),
            center,
            view_visible,
            sample_process_memory,
        ) {
            self.pinboard_accounted_image_bytes = sample_pinboard_bytes();
        }
        let mut actions = Vec::new();
        let mut open = self.open;
        egui::Window::new("Activity")
            .id(egui::Id::new("activity_dialog"))
            .open(&mut open)
            .default_size([620.0, 480.0])
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label("Filter");
                    ui.text_edit_singleline(&mut self.filter);
                    if ui.button("Clear history").clicked() {
                        center.clear_terminal_history();
                        self.snapshot = center.snapshot();
                    }
                    if ui.button("Copy sanitized diagnostics").clicked() {
                        ctx.copy_text(sanitized_diagnostics(&self.snapshot));
                    }
                });
                ui.small(format!(
                    "Current process working set: {}",
                    format_bytes(self.current_process_working_set_bytes)
                ));
                ui.small(format!(
                    "Current process private bytes (Windows): {}",
                    format_bytes(self.current_process_private_bytes)
                ));
                ui.small(format!(
                    "Pinboard-accounted image bytes (global): {}",
                    format_bytes(self.pinboard_accounted_image_bytes)
                ));
                ui.separator();
                for omitted in &self.snapshot.omissions_by_owner {
                    let mut facts = Vec::new();
                    if omitted.active > 0 {
                        facts.push(format!("{} active", omitted.active));
                    }
                    if omitted.terminal > 0 {
                        facts.push(format!("{} terminal", omitted.terminal));
                    }
                    ui.label(format!(
                        "{}: {} activity facts omitted by the observer's retention limits.",
                        omitted.owner.label(),
                        facts.join(" and ")
                    ));
                }
                let needle = self.filter.trim().to_ascii_lowercase();
                let rows = self
                    .snapshot
                    .rows
                    .iter()
                    .filter(|row| matches_filter(row, &needle))
                    .cloned()
                    .collect::<Vec<_>>();
                if rows.is_empty() {
                    ui.label("No matching activity.");
                } else {
                    egui::ScrollArea::vertical()
                        .auto_shrink([false; 2])
                        .show(ui, |ui| {
                            for row in rows {
                                ui.group(|ui| {
                                    ui.horizontal_wrapped(|ui| {
                                        ui.strong(row.key.owner().label());
                                        ui.label(row.operation.label());
                                        ui.label(row.lifecycle.label());
                                        ui.label(row.retirement.label());
                                        ui.label(format_elapsed(row.elapsed));
                                    });
                                    if let Some(progress) = row.progress {
                                        if let Some(total) = progress.total {
                                            ui.label(format!(
                                                "{} / {} {}",
                                                progress.completed,
                                                total,
                                                progress.unit.label()
                                            ));
                                        } else {
                                            ui.label(format!(
                                                "{} {}",
                                                progress.completed,
                                                progress.unit.label()
                                            ));
                                        }
                                    }
                                    if let Some(diagnostic) = row.diagnostic {
                                        ui.label(diagnostic.label());
                                    }
                                    render_resources(ui, &row);
                                    ui.horizontal(|ui| {
                                        if row.key.owner() == ActivityOwner::Screenshot
                                            && !screenshot_editor_available
                                        {
                                            ui.label("No screenshot editor is open");
                                        } else if ui.button("Open owner tool").clicked() {
                                            actions.push(ActivityDialogAction::Navigate(
                                                row.key.owner(),
                                            ));
                                        }
                                        if row.can_cancel
                                            && ui.button("Cancel current operation").clicked()
                                        {
                                            let _ = center.request_cancel(row.key);
                                        }
                                    });
                                });
                            }
                        });
                }
            });
        if self.open != open {
            self.open = open;
        }
        if self.open {
            ctx.request_repaint_after(ACTIVITY_REFRESH_INTERVAL);
        }
        actions
    }

    #[cfg(test)]
    fn refresh_count(&self) -> usize {
        self.refresh_count
    }
}

fn matches_filter(row: &ActivityRow, needle: &str) -> bool {
    needle.is_empty()
        || row
            .key
            .owner()
            .label()
            .to_ascii_lowercase()
            .contains(needle)
        || row.operation.label().to_ascii_lowercase().contains(needle)
        || row.lifecycle.label().to_ascii_lowercase().contains(needle)
        || row.retirement.label().to_ascii_lowercase().contains(needle)
        || row
            .diagnostic
            .is_some_and(|diagnostic| diagnostic.label().to_ascii_lowercase().contains(needle))
        || format_elapsed(row.elapsed)
            .to_ascii_lowercase()
            .contains(needle)
        || row.progress.is_some_and(|progress| {
            format!(
                "{} {} {}",
                progress.completed,
                progress
                    .total
                    .map_or_else(String::new, |total| total.to_string()),
                progress.unit.label()
            )
            .to_ascii_lowercase()
            .contains(needle)
        })
        || {
            let resources = row.resources;
            format!(
                "owner accounted {} estimate {}",
                format_bytes(resources.owner_accounted_bytes),
                format_bytes(resources.estimated_bytes)
            )
            .to_ascii_lowercase()
            .contains(needle)
        }
}

fn render_resources(ui: &mut egui::Ui, row: &ActivityRow) {
    let resources = row.resources;
    ui.horizontal_wrapped(|ui| {
        ui.small(format!(
            "Owner-accounted: {}",
            format_bytes(resources.owner_accounted_bytes)
        ));
        ui.small(format!(
            "Estimate: {}",
            format_bytes(resources.estimated_bytes)
        ));
    });
}

fn sample_current_process_memory() -> (Option<u64>, Option<u64>) {
    let pid = sysinfo::Pid::from_u32(std::process::id());
    let mut system = System::new();
    system.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    let Some(process) = system.process(pid) else {
        return (None, None);
    };
    (Some(process.memory()), process_private_bytes(process))
}

#[cfg(windows)]
fn process_private_bytes(process: &sysinfo::Process) -> Option<u64> {
    Some(process.virtual_memory())
}

#[cfg(not(windows))]
fn process_private_bytes(_process: &sysinfo::Process) -> Option<u64> {
    None
}

fn format_bytes(value: Option<u64>) -> String {
    value.map_or_else(|| "unknown".into(), |bytes| format!("{bytes} B"))
}

fn sanitized_diagnostics(snapshot: &ActivitySnapshot) -> String {
    snapshot
        .rows
        .iter()
        .map(|row| {
            let progress = row.progress.map_or_else(
                || "progress unavailable".to_owned(),
                |progress| match progress.total {
                    Some(total) => format!(
                        "{} / {} {}",
                        progress.completed,
                        total,
                        progress.unit.label()
                    ),
                    None => format!("{} {}", progress.completed, progress.unit.label()),
                },
            );
            format!(
                "{} | {} | {} | {} | {} | {} | {}",
                row.key.owner().label(),
                row.operation.label(),
                row.lifecycle.label(),
                row.retirement.label(),
                row.elapsed
                    .map(|elapsed| format!("{:.1}s", elapsed.as_secs_f32()))
                    .unwrap_or_else(|| "unknown".to_owned()),
                progress,
                row.diagnostic.map_or("no diagnostic", |item| item.label())
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn format_elapsed(elapsed: Option<std::time::Duration>) -> String {
    elapsed.map_or_else(
        || "Elapsed unknown".to_owned(),
        |elapsed| format!("{:.1}s", elapsed.as_secs_f32()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::{ActivityIncarnation, ActivityOperation, ActivityOutcome, ActivitySlot};
    use std::cell::Cell;

    fn center_with_terminal_row() -> ActivityCenter {
        let center = ActivityCenter::new();
        center
            .start(
                ActivityOwner::FileSearch,
                ActivitySlot::Primary,
                ActivityIncarnation::new(1),
                ActivityOperation::SearchFiles,
            )
            .expect("test operation id")
            .finish(
                ActivityOutcome::Completed,
                None,
                crate::activity::ActivityRetirement::Unavailable,
            );
        center
    }

    #[test]
    fn visible_refresh_is_throttled_and_closed_refresh_is_inert() {
        let center = center_with_terminal_row();
        let now = Instant::now();
        let mut dialog = ActivityDialogState::default();
        assert!(!dialog.refresh_if_due_with_sampler(now, &center, true, || (None, None)));
        assert_eq!(dialog.refresh_count(), 0);
        dialog.open();
        assert!(!dialog.refresh_if_due_with_sampler(now, &center, false, || (None, None)));
        assert_eq!(dialog.refresh_count(), 0);
        assert!(dialog.refresh_if_due_with_sampler(now, &center, true, || (None, None)));
        assert!(!dialog.refresh_if_due_with_sampler(
            now + Duration::from_millis(249),
            &center,
            true,
            || (None, None)
        ));
        assert!(dialog.refresh_if_due_with_sampler(
            now + Duration::from_millis(250),
            &center,
            true,
            || (None, None)
        ));
        dialog.close();
        assert!(!dialog.refresh_if_due_with_sampler(
            now + Duration::from_secs(1),
            &center,
            true,
            || (None, None)
        ));
        assert_eq!(dialog.refresh_count(), 2);
    }

    #[test]
    fn reopening_preserves_filter_and_loads_current_history() {
        let center = center_with_terminal_row();
        let mut dialog = ActivityDialogState::default();
        dialog.set_filter("file".into());
        dialog.open();
        assert!(dialog.refresh_if_due_with_sampler(Instant::now(), &center, true, || (None, None)));
        assert_eq!(dialog.snapshot().rows.len(), 1);
        dialog.close();
        dialog.open();
        assert_eq!(dialog.filter(), "file");
        assert!(dialog.refresh_if_due_with_sampler(Instant::now(), &center, true, || (None, None)));
        assert_eq!(dialog.snapshot().rows.len(), 1);
    }

    #[test]
    fn copied_text_contains_only_safe_owner_activity_metadata() {
        let center = center_with_terminal_row();
        let snapshot = center.snapshot();
        let copied = sanitized_diagnostics(&snapshot);
        assert!(copied.contains("File search"));
        assert!(copied.contains("Search files"));
        assert!(!copied.contains("path"));
    }

    #[test]
    fn filter_matches_safe_labels_and_excludes_other_rows() {
        let center = center_with_terminal_row();
        let snapshot = center.snapshot();
        assert!(matches_filter(&snapshot.rows[0], "search"));
        assert!(matches_filter(&snapshot.rows[0], "unavailable"));
        assert!(matches_filter(&snapshot.rows[0], "unknown"));
        assert!(!matches_filter(&snapshot.rows[0], "process working set"));
        assert!(!matches_filter(&snapshot.rows[0], "secret"));
    }

    #[test]
    fn process_memory_sampler_obeys_visible_open_250ms_refresh_gate() {
        let center = center_with_terminal_row();
        let now = Instant::now();
        let mut dialog = ActivityDialogState::default();
        let mut samples = 0;

        assert!(!dialog.refresh_if_due_with_sampler(now, &center, true, || {
            samples += 1;
            (Some(100), Some(80))
        }));
        dialog.open();
        assert!(
            !dialog.refresh_if_due_with_sampler(now, &center, false, || {
                samples += 1;
                (Some(100), Some(80))
            })
        );
        assert!(dialog.refresh_if_due_with_sampler(now, &center, true, || {
            samples += 1;
            (Some(100), Some(80))
        }));
        assert_eq!(samples, 1);
        assert_eq!(dialog.current_process_working_set_bytes, Some(100));
        assert_eq!(dialog.current_process_private_bytes, Some(80));

        assert!(!dialog.refresh_if_due_with_sampler(
            now + ACTIVITY_REFRESH_INTERVAL - Duration::from_millis(1),
            &center,
            true,
            || {
                samples += 1;
                (Some(200), Some(160))
            }
        ));
        assert_eq!(samples, 1);
        assert!(dialog.refresh_if_due_with_sampler(
            now + ACTIVITY_REFRESH_INTERVAL,
            &center,
            true,
            || {
                samples += 1;
                (Some(200), Some(160))
            }
        ));
        assert_eq!(samples, 2);

        dialog.close();
        assert!(!dialog.refresh_if_due_with_sampler(
            now + Duration::from_secs(1),
            &center,
            true,
            || {
                samples += 1;
                (Some(300), Some(240))
            }
        ));
        assert_eq!(samples, 2);
    }

    #[test]
    fn activity_dialog_renders_open_filter_and_close_states() {
        let ctx = egui::Context::default();
        let center = center_with_terminal_row();
        let mut dialog = ActivityDialogState::default();
        let screen_rect = egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(800.0, 600.0));
        let raw_input = || egui::RawInput {
            screen_rect: Some(screen_rect),
            ..egui::RawInput::default()
        };
        let process_samples = Cell::new(0);
        let pinboard_samples = Cell::new(0);
        let render = |dialog: &mut ActivityDialogState| {
            ctx.begin_pass(raw_input());
            let actions = dialog.show_with_samplers(
                &ctx,
                &center,
                true,
                false,
                || {
                    process_samples.set(process_samples.get() + 1);
                    (Some(4096), Some(2048))
                },
                || {
                    pinboard_samples.set(pinboard_samples.get() + 1);
                    Some(128)
                },
            );
            let output = ctx.end_pass();
            (actions, output)
        };

        dialog.open();
        let (actions, opened) = render(&mut dialog);
        assert!(actions.is_empty());
        assert!(
            !opened.shapes.is_empty(),
            "the open Activity dialog renders"
        );
        assert_eq!(dialog.snapshot().rows.len(), 1);
        assert_eq!(process_samples.get(), 1);
        assert_eq!(pinboard_samples.get(), 1);
        opened.drop_without_applying_deltas();

        dialog.set_filter("not a matching operation".into());
        let (actions, filtered) = render(&mut dialog);
        assert!(actions.is_empty());
        assert!(
            !filtered.shapes.is_empty(),
            "the filtered Activity dialog renders"
        );
        assert_eq!(dialog.filter(), "not a matching operation");
        assert_eq!(
            process_samples.get(),
            1,
            "refresh remains bounded to 250 ms"
        );
        assert_eq!(
            pinboard_samples.get(),
            1,
            "global byte sampling shares the refresh gate"
        );
        filtered.drop_without_applying_deltas();

        dialog.close();
        let (actions, closed) = render(&mut dialog);
        assert!(actions.is_empty());
        assert!(
            closed.shapes.is_empty(),
            "a closed Activity dialog paints nothing"
        );
        assert_eq!(process_samples.get(), 1);
        assert_eq!(pinboard_samples.get(), 1);
        closed.drop_without_applying_deltas();
    }
}
