use crate::brightness_service::{
    BrightnessCompletion, BrightnessFailure, BrightnessFailureKind, BrightnessRequestTicket,
    BrightnessService, BrightnessSetDisposition, BrightnessWakeCallback, MonitorInventory,
    MonitorInventoryEntry, MonitorTarget, PhysicalMonitorIdentity,
};
use eframe::egui;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Default)]
pub struct BrightnessDialog {
    pub open: bool,
    service: Option<BrightnessService>,
    service_closing: bool,
    lifetime: u64,
    inventory: Option<Arc<MonitorInventory>>,
    inventory_fresh: bool,
    selected: Option<PhysicalMonitorIdentity>,
    drafts: HashMap<PhysicalMonitorIdentity, u8>,
    row_status: HashMap<PhysicalMonitorIdentity, String>,
    pending: Option<PendingBrightnessRequest>,
    message: Option<String>,
    retirement_notice: Option<String>,
    #[cfg(test)]
    test_rows: Vec<BrightnessTestRowResponse>,
    #[cfg(test)]
    test_refresh_response: Option<egui::Response>,
    #[cfg(test)]
    test_cancel_response: Option<egui::Response>,
    #[cfg(test)]
    test_preloaded_inventory: bool,
    #[cfg(test)]
    test_reopen_service: Option<BrightnessService>,
}

struct PendingBrightnessRequest {
    ticket: BrightnessRequestTicket,
    lifetime: u64,
    kind: PendingBrightnessKind,
}

enum PendingBrightnessKind {
    Refresh,
    Set {
        target: MonitorTarget,
        requested_percent: u8,
        draft_at_submit: u8,
    },
}

#[cfg(test)]
#[derive(Clone)]
pub(crate) struct BrightnessTestRowResponse {
    pub identity: Option<PhysicalMonitorIdentity>,
    pub display_name: String,
    pub selection: Option<egui::Response>,
    pub slider: Option<egui::Response>,
    pub set: Option<egui::Response>,
}

impl BrightnessDialog {
    /// Opens the owner without assuming that a monitor exists or has a known
    /// level. The first inventory request is started from the shared runtime
    /// tick, where the root repaint callback can be installed.
    pub fn open(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        self.lifetime = self.lifetime.wrapping_add(1);
        self.message = None;
        if self.service_closing {
            self.message =
                Some("Waiting for the previous monitor session to finish cleanup.".into());
        } else {
            self.inventory_fresh = false;
        }
    }

    /// Invalidates all UI authority immediately and asks the owned service to
    /// stop. The service remains retained until the shared runtime tick joins
    /// its worker and consumes the shutdown result.
    pub fn close(&mut self) {
        if !self.open && !self.service_closing && self.service.is_none() {
            return;
        }
        self.open = false;
        self.lifetime = self.lifetime.wrapping_add(1);
        self.pending = None;
        self.inventory_fresh = false;
        self.inventory = None;
        self.selected = None;
        self.drafts.clear();
        self.row_status.clear();
        self.message = None;
        #[cfg(test)]
        {
            self.test_preloaded_inventory = false;
            self.test_rows.clear();
            self.test_refresh_response = None;
            self.test_cancel_response = None;
        }
        if let Some(service) = self.service.as_ref() {
            service.request_shutdown();
            self.service_closing = true;
        }
    }

    /// Called from the shared visible/hidden runtime tick. It is deliberately
    /// the only automatic service consumer: drawing the dialog never polls
    /// monitor APIs or starts a second worker.
    pub fn runtime_tick(&mut self, ctx: &egui::Context) {
        if self.service_closing {
            let joined = self
                .service
                .as_mut()
                .map(BrightnessService::try_join_shutdown);
            match joined {
                Some(Ok(Some(_))) => self.finish_retirement(None, ctx),
                Some(Err(failure)) => self.finish_retirement(Some(failure_message(&failure)), ctx),
                Some(Ok(None)) => {
                    if self
                        .service
                        .as_ref()
                        .is_some_and(BrightnessService::shutdown_terminal_published)
                    {
                        // The service has published its terminal state, but its
                        // owned thread may not yet report `is_finished`. Keep the
                        // root alive for the immediate follow-up join attempt.
                        ctx.request_repaint_of(egui::ViewportId::ROOT);
                    }
                }
                None => {
                    self.service.take();
                    self.service_closing = false;
                    self.pending = None;
                    self.inventory = None;
                    self.inventory_fresh = false;
                    self.selected = None;
                    self.drafts.clear();
                    self.row_status.clear();
                    if self.open {
                        self.start_service_and_refresh(ctx);
                    }
                }
            }
            return;
        }

        #[cfg(test)]
        if self.open && self.test_preloaded_inventory && self.service.is_none() {
            return;
        }

        if self.open && self.service.is_none() {
            self.start_service_and_refresh(ctx);
        }

        let completion = self
            .pending
            .as_ref()
            .and_then(|pending| pending.ticket.try_recv());
        if let Some(completion) = completion {
            let Some(pending) = self.pending.take() else {
                return;
            };
            if pending.lifetime != self.lifetime || !self.open {
                return;
            }
            self.accept_completion(pending.kind, completion);
        }
    }

    fn start_service_and_refresh(&mut self, _ctx: &egui::Context) {
        if self.service_closing || !self.open || self.service.is_some() {
            return;
        }
        #[cfg(not(test))]
        let wake: BrightnessWakeCallback = {
            let repaint = _ctx.clone();
            Arc::new(move || repaint.request_repaint_of(egui::ViewportId::ROOT))
        };
        #[cfg(test)]
        let Some(service) = self.test_reopen_service.take() else {
            // Unit tests must install an owned fake explicitly. In particular,
            // a close/reopen test must never fall through to the native helper.
            self.message = Some("Monitor service is not configured for this test.".into());
            return;
        };
        #[cfg(not(test))]
        let service = BrightnessService::new_with_wake_callback(wake);
        self.service = Some(service);
        self.inventory_fresh = false;
        self.request_refresh();
    }

    fn finish_retirement(&mut self, error: Option<String>, ctx: &egui::Context) {
        self.service.take();
        self.service_closing = false;
        self.pending = None;
        self.inventory = None;
        self.inventory_fresh = false;
        self.selected = None;
        self.drafts.clear();
        self.row_status.clear();
        #[cfg(test)]
        {
            self.test_rows.clear();
            self.test_refresh_response = None;
            self.test_cancel_response = None;
        }
        if let Some(error) = error {
            self.retirement_notice = Some(format!(
                "The previous monitor session reported cleanup failure: {error}"
            ));
        }
        if self.open {
            self.start_service_and_refresh(ctx);
        }
    }

    fn request_refresh(&mut self) {
        if !self.open || self.service_closing {
            return;
        }
        let Some(service) = self.service.as_ref() else {
            return;
        };
        self.inventory_fresh = false;
        match service.refresh() {
            Ok(ticket) => {
                self.pending = Some(PendingBrightnessRequest {
                    ticket,
                    lifetime: self.lifetime,
                    kind: PendingBrightnessKind::Refresh,
                });
                self.message = Some("Checking connected monitors…".into());
            }
            Err(failure) => {
                self.message = Some(failure_message(&failure));
            }
        }
    }

    fn request_set(&mut self, target: MonitorTarget, requested_percent: u8, draft_at_submit: u8) {
        if !self.can_set_target(&target) {
            self.message = Some("Refresh monitor information before setting a level.".into());
            return;
        }
        let Some(service) = self.service.as_ref() else {
            self.message = Some("The monitor service is not ready.".into());
            return;
        };
        match service.set(&target, requested_percent) {
            Ok(ticket) => {
                self.pending = Some(PendingBrightnessRequest {
                    ticket,
                    lifetime: self.lifetime,
                    kind: PendingBrightnessKind::Set {
                        target: target.clone(),
                        requested_percent,
                        draft_at_submit,
                    },
                });
                self.row_status
                    .insert(target.identity, format!("Setting to {requested_percent}%…"));
                self.message = None;
            }
            Err(failure) => {
                self.inventory_fresh = false;
                self.row_status
                    .insert(target.identity, failure_message(&failure));
            }
        }
    }

    fn accept_completion(&mut self, kind: PendingBrightnessKind, completion: BrightnessCompletion) {
        match (kind, completion) {
            (PendingBrightnessKind::Refresh, BrightnessCompletion::Refreshed(Ok(inventory))) => {
                let selected_removed = self.accept_inventory(inventory);
                self.message = if selected_removed {
                    Some("The selected monitor is no longer available; choose another row.".into())
                } else if self
                    .inventory
                    .as_ref()
                    .is_some_and(|inventory| inventory.entries.is_empty())
                {
                    Some("No physical monitors were found.".into())
                } else if self.inventory_fresh {
                    None
                } else {
                    Some("Monitor identities are incomplete; Set is disabled until a complete refresh.".into())
                };
            }
            (PendingBrightnessKind::Refresh, BrightnessCompletion::Refreshed(Err(failure))) => {
                self.inventory_fresh = false;
                self.message = Some(failure_message(&failure));
            }
            (
                PendingBrightnessKind::Set {
                    target,
                    requested_percent,
                    draft_at_submit,
                },
                BrightnessCompletion::Set(Ok(acknowledgement)),
            ) => {
                let target_is_current = self.inventory.as_ref().is_some_and(|inventory| {
                    self.inventory_fresh
                        && inventory.session == target.session
                        && inventory.generation == target.generation
                        && inventory.target(target.id) == Some(&target)
                        && acknowledgement.target == target
                });
                if !target_is_current {
                    self.message = Some(
                        "The monitor list changed before the Set result was accepted; refresh before another Set.".into(),
                    );
                    self.inventory_fresh = false;
                    return;
                }
                if self.drafts.get(&target.identity).copied() == Some(draft_at_submit) {
                    self.drafts
                        .insert(target.identity.clone(), acknowledgement.readback_percent);
                }
                self.row_status.insert(
                    target.identity,
                    format!(
                        "Applied {}; monitor reports {}%.",
                        requested_percent, acknowledgement.readback_percent
                    ),
                );
                self.message = Some("Set confirmed; checking the monitor readback…".into());
                self.request_refresh();
            }
            (
                PendingBrightnessKind::Set { target, .. },
                BrightnessCompletion::Set(Err(failure)),
            ) => {
                self.inventory_fresh = false;
                if failure.set_disposition == Some(BrightnessSetDisposition::Unknown) {
                    self.row_status
                        .insert(target.identity, unknown_set_failure_message(&failure));
                } else {
                    self.row_status
                        .insert(target.identity, failure_message(&failure));
                }
            }
            (_, BrightnessCompletion::Cancelled | BrightnessCompletion::Superseded { .. }) => {
                self.message = Some("The monitor request was cancelled or superseded.".into());
            }
            _ => {
                self.message = Some("The monitor service returned an unexpected response.".into());
                self.inventory_fresh = false;
            }
        }
    }

    fn accept_inventory(&mut self, inventory: MonitorInventory) -> bool {
        let previous_selected = self.selected.clone();
        let inventory = Arc::new(inventory);
        self.selected = if let Some(previous_selected) = previous_selected.as_ref() {
            inventory
                .entries
                .iter()
                .find(|entry| {
                    entry.identity.as_ref() == Some(previous_selected)
                        && entry.target.is_some()
                        && entry.native_range.is_some()
                })
                .and_then(|entry| entry.identity.clone())
        } else {
            inventory
                .entries
                .iter()
                .find(|entry| entry.target.is_some() && entry.native_range.is_some())
                .and_then(|entry| entry.identity.clone())
        };
        let selected_removed = previous_selected.is_some() && self.selected.is_none();
        for entry in &inventory.entries {
            if let Some(identity) = entry.identity.as_ref()
                && let Some(percent) = entry.percent
            {
                self.drafts.entry(identity.clone()).or_insert(percent);
            }
        }
        self.drafts.retain(|identity, _| {
            inventory
                .entries
                .iter()
                .any(|entry| entry.identity.as_ref() == Some(identity))
        });
        self.row_status.clear();
        self.inventory_fresh = inventory.topology_complete;
        self.inventory = Some(inventory);
        selected_removed
    }

    fn can_set_target(&self, target: &MonitorTarget) -> bool {
        self.open
            && !self.service_closing
            && self.service.is_some()
            && self.pending.is_none()
            && self.inventory_fresh
            && self.inventory.as_ref().is_some_and(|inventory| {
                inventory.session == target.session
                    && inventory.generation == target.generation
                    && inventory.topology_complete
                    && inventory.target(target.id) == Some(target)
                    && inventory.entries.iter().any(|entry| {
                        entry.target.as_ref() == Some(target) && entry.native_range.is_some()
                    })
            })
    }

    /// `None` means the dialog has not yet established a current inventory;
    /// callers may open it to perform the initial explicit discovery. A
    /// settled inventory with no exact controllable row is `Some(false)`.
    pub(crate) fn settings_control_available(&self) -> Option<bool> {
        if !self.open {
            return None;
        }
        let inventory = self.inventory.as_ref()?;
        if !inventory.topology_complete {
            return Some(false);
        }
        if !self.inventory_fresh {
            return None;
        }
        Some(self.selected_target().is_some())
    }

    /// Discovery is the only asynchronous prerequisite for the real Settings
    /// Level control. While it is in flight, the Settings handoff keeps its
    /// finite focus budget paused; the service completion wake resumes it.
    pub(crate) fn awaiting_initial_inventory(&self) -> bool {
        self.open
            && self.inventory.is_none()
            && (self.service_closing
                || self
                    .pending
                    .as_ref()
                    .is_some_and(|pending| matches!(&pending.kind, PendingBrightnessKind::Refresh)))
    }

    fn selected_target(&self) -> Option<&MonitorTarget> {
        let identity = self.selected.as_ref()?;
        let inventory = self.inventory.as_ref()?;
        if !self.inventory_fresh || !inventory.topology_complete {
            return None;
        }
        inventory.entries.iter().find_map(|entry| {
            (entry.identity.as_ref() == Some(identity) && entry.native_range.is_some())
                .then_some(entry.target.as_ref())
                .flatten()
        })
    }

    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.open {
            return;
        }
        let inventory = self.inventory.clone();
        let inventory_fresh = self.inventory_fresh;
        let request_pending = self.pending.is_some();
        let mut window_open = self.open;
        let mut refresh_requested = false;
        let mut set_requested: Option<(MonitorTarget, u8, u8)> = None;
        let mut selected_requested: Option<PhysicalMonitorIdentity> = None;
        let mut draft_updates = Vec::new();
        let first_pass = ctx.current_pass_index() == 0;
        #[cfg(test)]
        let mut test_rows = Vec::new();
        #[cfg(test)]
        let mut refresh_response = None;
        #[cfg(test)]
        let mut cancel_response = None;
        let mut cancel_requested = false;

        egui::Window::new("Brightness")
            .resizable(true)
            .open(&mut window_open)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    let refresh = ui.add_enabled(
                        !request_pending && !self.service_closing,
                        egui::Button::new("Refresh monitors"),
                    );
                    #[cfg(test)]
                    {
                        refresh_response = Some(refresh.clone());
                    }
                    if refresh.clicked() && first_pass {
                        refresh_requested = true;
                    }
                    if request_pending {
                        ui.spinner();
                    }
                    let cancel = ui.button("Cancel");
                    #[cfg(test)]
                    {
                        cancel_response = Some(cancel.clone());
                    }
                    if cancel.clicked() && first_pass {
                        cancel_requested = true;
                    }
                });
                if let Some(message) = self.message.as_deref() {
                    ui.label(message);
                }
                if let Some(notice) = self.retirement_notice.as_deref() {
                    ui.colored_label(egui::Color32::YELLOW, notice);
                }

                let Some(inventory) = inventory.as_ref() else {
                    ui.label("Monitor levels are not available until discovery completes.");
                    return;
                };
                if inventory.entries.is_empty() {
                    ui.label("No physical monitors were found.");
                    return;
                }
                let duplicate_names = inventory
                    .entries
                    .iter()
                    .fold(HashMap::<&str, usize>::new(), |mut counts, entry| {
                        *counts.entry(entry.display_name.as_str()).or_default() += 1;
                        counts
                    });
                egui::ScrollArea::vertical()
                    .id_salt(("brightness-monitor-rows", inventory.session))
                    .show(ui, |ui| {
                        for (row_index, entry) in inventory.entries.iter().enumerate() {
                            let identity = entry.identity.clone();
                            let row_id = identity
                                .clone()
                                .map(|identity| ui.make_persistent_id(("brightness-row", identity)))
                                .unwrap_or_else(|| {
                                    ui.make_persistent_id((
                                        "brightness-unidentified-row",
                                        inventory.session,
                                        inventory.generation,
                                        row_index,
                                    ))
                                });
                            ui.push_id(row_id, |ui| {
                                ui.group(|ui| {
                                    let display_name = display_name(entry, &duplicate_names);
                                    let selection_response = if entry.target.is_some() {
                                        let response = ui.selectable_label(
                                            self.selected.as_ref() == identity.as_ref(),
                                            &display_name,
                                        );
                                        if response.clicked()
                                            && first_pass
                                            && entry.native_range.is_some()
                                            && let Some(identity) = identity.clone()
                                        {
                                            selected_requested = Some(identity);
                                        }
                                        Some(response)
                                    } else {
                                        ui.label(&display_name);
                                        None
                                    };

                                    ui.horizontal(|ui| {
                                        if let Some(percent) = entry.percent {
                                            ui.label(format!("Current: {percent}%"));
                                        } else {
                                            ui.label("Current level unavailable");
                                        }
                                        if !inventory.topology_complete {
                                            ui.label("Set disabled: monitor identities are incomplete.");
                                        }
                                    });

                                    let (slider_response, set_response) =
                                        if let (Some(identity), Some(_range), Some(target)) =
                                            (identity.as_ref(), entry.native_range, entry.target.as_ref())
                                        {
                                            let mut draft = self
                                                .drafts
                                                .get(identity)
                                                .copied()
                                                .or(entry.percent)
                                                .unwrap_or(0);
                                            let slider = ui.add_enabled(
                                                inventory_fresh
                                                    && inventory.topology_complete
                                                    && !self.service_closing,
                                                egui::Slider::new(&mut draft, 0..=100).text("Level"),
                                            );
                                            if slider.changed() && first_pass {
                                                draft_updates.push((identity.clone(), draft));
                                            }
                                            if self.selected.as_ref() == Some(identity) {
                                                crate::settings_editor::render::observe_scoped_settings_navigation_response(
                                                    ui,
                                                    &slider,
                                                    "brightness.level",
                                                    crate::gui::jump_mode::TargetKind::Slider,
                                                );
                                            }
                                            let can_set = inventory_fresh
                                                && !request_pending
                                                && inventory.topology_complete
                                                && self.can_set_target(target);
                                            let set = ui.add_enabled(can_set, egui::Button::new("Set"));
                                            if set.clicked() && first_pass {
                                                set_requested = Some((target.clone(), draft, draft));
                                            }
                                            (Some(slider), Some(set))
                                        } else {
                                            if let Some(reason) = entry.unavailable.as_ref() {
                                                ui.label(unavailable_message(reason));
                                            } else {
                                                ui.label("No controllable level is available.");
                                            }
                                            (None, None)
                                        };
                                    if let Some(identity) = identity.clone()
                                        && let Some(status) = self.row_status.get(&identity)
                                    {
                                        ui.label(status);
                                    }
                                    #[cfg(test)]
                                    test_rows.push(BrightnessTestRowResponse {
                                        identity: identity.clone(),
                                        display_name,
                                        selection: selection_response,
                                        slider: slider_response,
                                        set: set_response,
                                    });
                                    #[cfg(not(test))]
                                    let _ = (selection_response, slider_response, set_response);
                                });
                                ui.add_space(4.0);
                            });
                        }
                    });
            });

        if cancel_requested {
            window_open = false;
        }
        self.open = window_open;
        #[cfg(test)]
        {
            self.test_rows = test_rows;
            self.test_refresh_response = refresh_response;
            self.test_cancel_response = cancel_response;
        }
        for (identity, draft) in draft_updates {
            self.drafts.insert(identity, draft);
        }
        if let Some(identity) = selected_requested {
            self.selected = Some(identity);
        }
        if !window_open {
            self.close();
            return;
        }
        if refresh_requested {
            self.request_refresh();
        }
        if let Some((target, requested_percent, draft_at_submit)) = set_requested {
            self.request_set(target, requested_percent, draft_at_submit);
        }
    }

    pub fn shutdown(&mut self) -> Result<(), BrightnessFailure> {
        self.open = false;
        self.lifetime = self.lifetime.wrapping_add(1);
        self.pending = None;
        self.service_closing = false;
        self.inventory = None;
        self.inventory_fresh = false;
        self.selected = None;
        self.drafts.clear();
        self.row_status.clear();
        let mut shutdown_failure = None;
        if let Some(mut service) = self.service.take() {
            if let Err(failure) = service.shutdown() {
                self.retain_shutdown_failure(&failure);
                shutdown_failure = Some(failure);
            }
        }
        #[cfg(test)]
        if let Some(mut service) = self.test_reopen_service.take() {
            if let Err(failure) = service.shutdown() {
                self.retain_shutdown_failure(&failure);
                shutdown_failure.get_or_insert(failure);
            }
        }
        if let Some(failure) = shutdown_failure {
            self.inventory_fresh = false;
            self.inventory = None;
            self.selected = None;
            self.drafts.clear();
            Err(failure)
        } else {
            Ok(())
        }
    }

    fn retain_shutdown_failure(&mut self, failure: &BrightnessFailure) {
        let mut notice = format!(
            "Final monitor helper cleanup failed: {}",
            failure_message(failure)
        );
        if let Some(operation) = failure.operation {
            notice.push_str(" during ");
            notice.push_str(operation_label(operation));
        }
        if let Some(native_code) = failure.native_code {
            notice.push_str(&format!(" (native code {native_code})"));
        }
        if let Some(cleanup) = failure.process_cleanup {
            notice.push_str(". Cleanup status: ");
            notice.push_str(cleanup_label(cleanup));
        }
        notice.push('.');
        self.retirement_notice = Some(notice);
    }

    #[cfg(test)]
    pub(crate) fn test_install_service(&mut self, service: BrightnessService) {
        if let Some(mut previous) = self.service.replace(service) {
            let _ = previous.shutdown();
        }
        self.service_closing = false;
        self.inventory = None;
        self.inventory_fresh = false;
        self.pending = None;
        self.selected = None;
        self.drafts.clear();
        self.row_status.clear();
        self.test_preloaded_inventory = false;
        if self.open {
            self.request_refresh();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_queue_reopen_service(&mut self, service: BrightnessService) {
        if let Some(mut previous) = self.test_reopen_service.replace(service) {
            let _ = previous.shutdown();
        }
    }

    #[cfg(test)]
    pub(crate) fn test_load_preopened_inventory(&mut self, inventory: MonitorInventory) {
        let inventory = Arc::new(inventory);
        self.inventory_fresh = inventory.topology_complete;
        self.selected = inventory
            .entries
            .iter()
            .find(|entry| entry.target.is_some() && entry.native_range.is_some())
            .and_then(|entry| entry.identity.clone());
        self.drafts.clear();
        for entry in &inventory.entries {
            if let (Some(identity), Some(percent)) = (entry.identity.as_ref(), entry.percent) {
                self.drafts.insert(identity.clone(), percent);
            }
        }
        self.inventory = Some(inventory);
        self.test_preloaded_inventory = true;
    }

    #[cfg(test)]
    pub(crate) fn test_rows(&self) -> &[BrightnessTestRowResponse] {
        &self.test_rows
    }

    #[cfg(test)]
    pub(crate) fn test_refresh_response(&self) -> Option<&egui::Response> {
        self.test_refresh_response.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_cancel_response(&self) -> Option<&egui::Response> {
        self.test_cancel_response.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_retirement_notice(&self) -> Option<&str> {
        self.retirement_notice.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_level_response(&self) -> Option<&egui::Response> {
        let selected = self.selected.as_ref()?;
        self.test_rows
            .iter()
            .find(|row| row.identity.as_ref() == Some(selected))?
            .slider
            .as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_inventory(&self) -> Option<&MonitorInventory> {
        self.inventory.as_deref()
    }

    #[cfg(test)]
    pub(crate) fn test_selected_identity(&self) -> Option<&PhysicalMonitorIdentity> {
        self.selected.as_ref()
    }

    #[cfg(test)]
    pub(crate) fn test_draft(&self, identity: &PhysicalMonitorIdentity) -> Option<u8> {
        self.drafts.get(identity).copied()
    }

    #[cfg(test)]
    pub(crate) fn test_status(&self, identity: &PhysicalMonitorIdentity) -> Option<&str> {
        self.row_status.get(identity).map(String::as_str)
    }

    #[cfg(test)]
    pub(crate) fn test_has_service(&self) -> bool {
        self.service.is_some()
    }

    #[cfg(test)]
    pub(crate) fn test_service_closing(&self) -> bool {
        self.service_closing
    }

    #[cfg(test)]
    pub(crate) fn test_inventory_fresh(&self) -> bool {
        self.inventory_fresh
    }

    #[cfg(test)]
    pub(crate) fn test_shutdown_terminal_published(&self) -> bool {
        self.service
            .as_ref()
            .is_some_and(BrightnessService::shutdown_terminal_published)
    }
}

fn display_name(entry: &MonitorInventoryEntry, duplicate_names: &HashMap<&str, usize>) -> String {
    if duplicate_names
        .get(entry.display_name.as_str())
        .is_some_and(|count| *count > 1)
        && let Some(identity) = entry.identity.as_ref()
    {
        format!(
            "{} · Display {} · Monitor {}",
            entry.display_name,
            identity.logical_index.saturating_add(1),
            identity.physical_index.saturating_add(1)
        )
    } else {
        entry.display_name.clone()
    }
}

fn unavailable_message(reason: &crate::brightness_service::MonitorUnavailable) -> String {
    use crate::brightness_service::MonitorUnavailable;
    match reason {
        MonitorUnavailable::NoPhysicalMonitors => "No physical monitors on this display.".into(),
        MonitorUnavailable::EnumerationFailed { .. } => "Could not enumerate this display.".into(),
        MonitorUnavailable::PhysicalEnumerationFailed { .. } => {
            "Could not enumerate physical monitors for this display.".into()
        }
        MonitorUnavailable::ReadFailed { .. } => "Could not read this monitor's level.".into(),
        MonitorUnavailable::InvalidNativeRange => {
            "The monitor returned an invalid level range.".into()
        }
        MonitorUnavailable::IdentityUnavailable => {
            "This monitor could not be identified safely.".into()
        }
    }
}

fn failure_message(failure: &BrightnessFailure) -> String {
    match failure.kind {
        BrightnessFailureKind::Unsupported => {
            "The monitor does not support brightness control.".into()
        }
        BrightnessFailureKind::NativeFailure => "The monitor operation failed.".into(),
        BrightnessFailureKind::HandleCleanupFailure
        | BrightnessFailureKind::HelperCleanupFailure => {
            "Monitor cleanup failed; refresh before another Set.".into()
        }
        BrightnessFailureKind::InvalidRange => {
            "The monitor returned an invalid brightness range.".into()
        }
        BrightnessFailureKind::StaleTarget => {
            "The monitor list changed; refresh before setting a level.".into()
        }
        BrightnessFailureKind::SetOutcomeUnknown => {
            "Set outcome is unknown; refresh before another Set.".into()
        }
        BrightnessFailureKind::ProtocolFailure => {
            "The monitor helper returned an invalid response.".into()
        }
        BrightnessFailureKind::HelperStartFailure => {
            "The monitor helper could not be started.".into()
        }
        BrightnessFailureKind::HelperTimeout => "The monitor helper timed out.".into(),
        BrightnessFailureKind::Superseded => "The monitor request was superseded.".into(),
        BrightnessFailureKind::Cancelled => "The monitor request was cancelled.".into(),
        BrightnessFailureKind::ServiceShutdown => "The monitor service is shutting down.".into(),
    }
}

fn unknown_set_failure_message(failure: &BrightnessFailure) -> String {
    let mut message = format!(
        "{} Set outcome is unknown; refresh before another Set.",
        failure_message(failure)
    );
    if let Some(cause) = failure.set_cause.as_ref() {
        message.push_str(" Reported cause: ");
        message.push_str(failure_kind_label(cause.kind));
        if let Some(operation) = cause.operation {
            message.push_str(" during ");
            message.push_str(operation_label(operation));
        }
        if let Some(code) = cause.native_code {
            message.push_str(&format!(" (native code {code})"));
        }
        message.push('.');
    }
    if let Some(cleanup) = failure.process_cleanup {
        message.push_str(" Cleanup status: ");
        message.push_str(cleanup_label(cleanup));
        message.push('.');
    }
    message
}

fn operation_label(operation: crate::brightness_service::BrightnessOperation) -> &'static str {
    use crate::brightness_service::BrightnessOperation;
    match operation {
        BrightnessOperation::EnumerateLogical => "logical display enumeration",
        BrightnessOperation::GetPhysicalCount | BrightnessOperation::GetPhysicalMonitors => {
            "physical monitor enumeration"
        }
        BrightnessOperation::ReadBrightness => "brightness readback",
        BrightnessOperation::SetBrightness => "brightness Set",
        BrightnessOperation::DestroyPhysicalMonitors => "monitor cleanup",
        BrightnessOperation::VerifyHelperExit => "helper exit verification",
    }
}

fn cleanup_label(cleanup: crate::brightness_service::HelperCleanupDisposition) -> &'static str {
    use crate::brightness_service::HelperCleanupDisposition;
    match cleanup {
        HelperCleanupDisposition::NormalExitVerified => "normal exit verified",
        HelperCleanupDisposition::ProcessExitVerified => "process exit verified",
        HelperCleanupDisposition::ForcedTerminationExitVerified => "forced termination verified",
        HelperCleanupDisposition::ExitUnverified => "exit unverified",
    }
}

fn failure_kind_label(kind: BrightnessFailureKind) -> &'static str {
    match kind {
        BrightnessFailureKind::Unsupported => "unsupported control",
        BrightnessFailureKind::NativeFailure => "a monitor operation failure",
        BrightnessFailureKind::HandleCleanupFailure
        | BrightnessFailureKind::HelperCleanupFailure => "a cleanup failure",
        BrightnessFailureKind::InvalidRange => "an invalid monitor range",
        BrightnessFailureKind::StaleTarget => "a stale monitor target",
        BrightnessFailureKind::SetOutcomeUnknown => "an unknown Set result",
        BrightnessFailureKind::ProtocolFailure => "an invalid helper response",
        BrightnessFailureKind::HelperStartFailure => "a helper startup failure",
        BrightnessFailureKind::HelperTimeout => "a helper timeout",
        BrightnessFailureKind::Superseded => "a superseded request",
        BrightnessFailureKind::Cancelled => "a cancelled request",
        BrightnessFailureKind::ServiceShutdown => "service shutdown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::brightness_service::tests::{FakeServiceMode, fake_service_for_dialog};

    fn run_frame(
        ctx: &egui::Context,
        dialog: &mut BrightnessDialog,
        frame: &mut usize,
        events: Vec<egui::Event>,
    ) {
        let input = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(
                egui::Pos2::ZERO,
                egui::vec2(900.0, 760.0),
            )),
            time: Some(1.0 + *frame as f64 / 60.0),
            focused: true,
            events,
            ..Default::default()
        };
        *frame += 1;
        let output = ctx.run_ui(input, |ctx| {
            dialog.runtime_tick(ctx);
            dialog.ui(ctx);
        });
        output.drop_without_applying_deltas();
    }

    fn pointer_event(position: egui::Pos2, pressed: bool) -> egui::Event {
        egui::Event::PointerButton {
            pos: position,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        }
    }

    fn click_at(
        ctx: &egui::Context,
        dialog: &mut BrightnessDialog,
        frame: &mut usize,
        position: egui::Pos2,
    ) {
        run_frame(
            ctx,
            dialog,
            frame,
            vec![egui::Event::PointerMoved(position)],
        );
        run_frame(ctx, dialog, frame, vec![pointer_event(position, true)]);
        run_frame(ctx, dialog, frame, vec![pointer_event(position, false)]);
    }

    fn click_response(
        ctx: &egui::Context,
        dialog: &mut BrightnessDialog,
        frame: &mut usize,
        response: &egui::Response,
    ) {
        assert!(response.enabled());
        click_at(ctx, dialog, frame, response.rect.center());
    }

    fn row_for<'a>(
        dialog: &'a BrightnessDialog,
        identity: &PhysicalMonitorIdentity,
    ) -> &'a BrightnessTestRowResponse {
        dialog
            .test_rows()
            .iter()
            .find(|row| row.identity.as_ref() == Some(identity))
            .expect("the exact physical monitor row was rendered")
    }

    fn open_fake_dialog(
        mode: FakeServiceMode,
    ) -> (
        egui::Context,
        BrightnessDialog,
        Arc<crate::brightness_service::tests::FakeServiceHarness>,
        usize,
    ) {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let wake_context = ctx.clone();
        let wake: BrightnessWakeCallback = Arc::new(move || {
            wake_context.request_repaint_of(egui::ViewportId::ROOT);
        });
        let (service, harness) = fake_service_for_dialog(mode, wake);
        let mut dialog = BrightnessDialog::default();
        dialog.open();
        dialog.test_install_service(service);
        assert!(harness.wait_for_completed_operation_count(1));
        let mut frame = 0;
        let mut interactive_level_ready = false;
        for _ in 0..8 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            interactive_level_ready = dialog.test_inventory().is_some()
                && dialog
                    .test_level_response()
                    .is_some_and(|response| response.enabled());
            if interactive_level_ready {
                break;
            }
            std::thread::yield_now();
        }
        assert!(dialog.test_inventory().is_some());
        assert!(
            interactive_level_ready,
            "fixture waits for a current painted interactive Level response, not just inventory data"
        );
        (ctx, dialog, harness, frame)
    }

    #[test]
    fn brightness_dialog_cancel_remains_available_while_refresh_is_loading() {
        let ctx = egui::Context::default();
        ctx.global_style_mut(|style| style.animation_time = 0.0);
        let wake_context = ctx.clone();
        let wake: BrightnessWakeCallback = Arc::new(move || {
            wake_context.request_repaint_of(egui::ViewportId::ROOT);
        });
        let (service, harness) = fake_service_for_dialog(FakeServiceMode::GateFirstRefresh, wake);
        let mut dialog = BrightnessDialog::default();
        dialog.open();
        dialog.test_install_service(service);
        assert!(harness.wait_for_gate());

        let mut frame = 0;
        // The first Window pass only establishes its retained size; use the
        // following painted response for the real Cancel click.
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        let cancel = dialog
            .test_cancel_response()
            .expect("the loading dialog renders its actual Cancel button")
            .clone();
        assert!(cancel.enabled());
        click_response(&ctx, &mut dialog, &mut frame, &cancel);

        assert!(!dialog.open, "Cancel closes through the dialog lifecycle");
        assert!(dialog.test_service_closing());
        assert!(harness.wait_for_live_sessions(0));
        for _ in 0..4 {
            if !dialog.test_service_closing() {
                break;
            }
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            std::thread::yield_now();
        }
        assert!(!dialog.test_service_closing());
        assert_eq!(harness.snapshot().set_count, 0);
        dialog.shutdown().unwrap();
    }

    #[test]
    fn brightness_dialog_reports_final_fake_cleanup_failure() {
        let (_ctx, mut dialog, harness, _frame) = open_fake_dialog(FakeServiceMode::FailShutdown);

        let failure = dialog
            .shutdown()
            .expect_err("final helper cleanup failure is returned to the owner");
        assert_eq!(failure.kind, BrightnessFailureKind::HelperCleanupFailure);
        assert_eq!(
            failure.operation,
            Some(crate::brightness_service::BrightnessOperation::VerifyHelperExit)
        );
        assert_eq!(
            failure.process_cleanup,
            Some(crate::brightness_service::HelperCleanupDisposition::ExitUnverified)
        );
        assert!(dialog.test_retirement_notice().is_some_and(|notice| {
            notice.contains("Final monitor helper cleanup failed")
                && notice.contains("exit unverified")
        }));
        assert_eq!(harness.snapshot().live_sessions, 0);
    }

    #[test]
    fn brightness_dialog_rows_keep_independent_drafts_until_explicit_set() {
        let (ctx, mut dialog, harness, mut frame) =
            open_fake_dialog(FakeServiceMode::MultiMonitorUi);
        let second_identity = dialog
            .test_inventory()
            .unwrap()
            .entries
            .iter()
            .filter_map(|entry| entry.identity.as_ref())
            .find(|identity| identity.logical_index == 1)
            .unwrap()
            .clone();
        assert_eq!(dialog.test_inventory().unwrap().entries.len(), 3);
        assert_ne!(
            dialog.test_inventory().unwrap().entries[0].identity,
            dialog.test_inventory().unwrap().entries[1].identity,
            "duplicate display names still identify distinct physical rows"
        );
        let rows = dialog.test_rows();
        assert!(
            rows.iter()
                .any(|row| row.display_name.contains("Display 1"))
        );
        assert!(
            rows.iter()
                .any(|row| row.display_name.contains("Display 2"))
        );
        let unsupported = dialog
            .test_inventory()
            .unwrap()
            .entries
            .iter()
            .find(|entry| entry.display_name == "Projector")
            .unwrap()
            .identity
            .as_ref()
            .unwrap()
            .clone();
        let unsupported_row = row_for(&dialog, &unsupported);
        assert!(unsupported_row.slider.is_none());
        assert!(unsupported_row.set.is_none());

        let select = row_for(&dialog, &second_identity)
            .selection
            .as_ref()
            .unwrap()
            .clone();
        click_response(&ctx, &mut dialog, &mut frame, &select);
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        assert_eq!(
            dialog.test_selected_identity(),
            Some(&second_identity),
            "selection follows the exact monitor identity"
        );

        let slider = row_for(&dialog, &second_identity)
            .slider
            .as_ref()
            .unwrap()
            .clone();
        assert!(slider.enabled());
        ctx.memory_mut(|memory| memory.request_focus(slider.id));
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        let focused_slider = row_for(&dialog, &second_identity).slider.as_ref().unwrap();
        assert!(
            focused_slider.has_focus(),
            "the actual enabled Level slider response owns keyboard focus before input"
        );
        run_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: true,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        run_frame(
            &ctx,
            &mut dialog,
            &mut frame,
            vec![egui::Event::Key {
                key: egui::Key::ArrowRight,
                physical_key: None,
                pressed: false,
                repeat: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        let edited_draft = dialog.test_draft(&second_identity).unwrap();
        assert_ne!(
            edited_draft, 25,
            "a fresh ArrowRight on the focused real slider changes only its draft"
        );
        assert_eq!(harness.snapshot().set_count, 0);

        let set = row_for(&dialog, &second_identity)
            .set
            .as_ref()
            .unwrap()
            .clone();
        assert!(set.enabled());
        click_response(&ctx, &mut dialog, &mut frame, &set);
        assert!(harness.wait_for_set_count(1));
        assert!(harness.wait_for_completed_operation_count(2));
        for _ in 0..12 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            if harness.snapshot().operations.len() >= 3 {
                break;
            }
            std::thread::yield_now();
        }
        assert!(harness.wait_for_operation_count(3));
        assert!(harness.wait_for_completed_operation_count(3));
        for _ in 0..8 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            if dialog
                .test_inventory()
                .is_some_and(|inventory| inventory.generation == 2)
            {
                break;
            }
            std::thread::yield_now();
        }

        let state = harness.snapshot();
        assert_eq!(state.set_count, 1, "explicit Set dispatches once");
        assert_eq!(state.set_targets[0].identity, second_identity);
        assert_eq!(state.set_percents, vec![edited_draft]);
        let inventory = dialog.test_inventory().unwrap();
        let updated = inventory
            .entries
            .iter()
            .find(|entry| entry.identity.as_ref() == Some(&second_identity))
            .unwrap();
        assert_eq!(updated.percent, Some(edited_draft));
        assert_eq!(dialog.test_draft(&second_identity), Some(edited_draft));
        let stale_target = updated.target.as_ref().unwrap().clone();
        let settled_operations = harness.snapshot().operations;
        for _ in 0..4 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        }
        assert_eq!(
            harness.snapshot().operations,
            settled_operations,
            "idle frames do not poll physical monitors"
        );

        dialog.accept_completion(
            PendingBrightnessKind::Set {
                target: stale_target.clone(),
                requested_percent: edited_draft,
                draft_at_submit: edited_draft,
            },
            BrightnessCompletion::Set(Err(BrightnessFailure {
                kind: BrightnessFailureKind::StaleTarget,
                operation: Some(crate::brightness_service::BrightnessOperation::SetBrightness),
                native_code: None,
                process_cleanup: None,
                set_disposition: Some(BrightnessSetDisposition::NotAttempted),
                set_cause: None,
            })),
        );
        assert!(!dialog.test_inventory_fresh());
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
        let disabled_set = row_for(&dialog, &second_identity).set.as_ref().unwrap();
        assert!(
            !disabled_set.enabled(),
            "a failed Set revokes old inventory authority"
        );
        let refresh = dialog.test_refresh_response().unwrap().clone();
        assert!(refresh.enabled());
        click_response(&ctx, &mut dialog, &mut frame, &refresh);
        assert!(harness.wait_for_completed_operation_count(4));
        for _ in 0..8 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            if dialog
                .test_inventory()
                .is_some_and(|inventory| inventory.generation == 3)
            {
                break;
            }
            std::thread::yield_now();
        }
        assert_eq!(dialog.test_inventory().unwrap().generation, 3);
        assert!(dialog.test_inventory_fresh());
        assert!(
            row_for(&dialog, &second_identity)
                .set
                .as_ref()
                .unwrap()
                .enabled()
        );
        assert_eq!(
            harness.snapshot().set_count,
            1,
            "Refresh does not replay Set"
        );
        dialog.shutdown().unwrap();
    }

    #[test]
    fn brightness_dialog_refresh_does_not_retarget_a_removed_selection() {
        let (ctx, mut dialog, harness, mut frame) =
            open_fake_dialog(FakeServiceMode::RemoveFirstOnSecondRefresh);
        let removed_identity = dialog
            .test_selected_identity()
            .expect("initial inventory selects its first controllable target")
            .clone();
        let refresh = dialog.test_refresh_response().unwrap().clone();
        click_response(&ctx, &mut dialog, &mut frame, &refresh);
        assert!(harness.wait_for_completed_operation_count(2));
        for _ in 0..8 {
            run_frame(&ctx, &mut dialog, &mut frame, Vec::new());
            if dialog
                .test_inventory()
                .is_some_and(|inventory| inventory.generation == 2)
            {
                break;
            }
            std::thread::yield_now();
        }

        let inventory = dialog.test_inventory().unwrap();
        assert_eq!(inventory.generation, 2);
        assert!(
            inventory
                .entries
                .iter()
                .all(|entry| entry.identity.as_ref() != Some(&removed_identity))
        );
        assert!(dialog.test_selected_identity().is_none());
        assert_eq!(harness.snapshot().set_count, 0);
        assert_eq!(dialog.settings_control_available(), Some(false));
        dialog.shutdown().unwrap();
    }

    #[test]
    fn brightness_dialog_unknown_set_keeps_the_typed_cause_visible() {
        let ctx = egui::Context::default();
        let mut dialog = BrightnessDialog::default();
        dialog.open();
        let inventory = crate::brightness_service::tests::fake_inventory_for_dialog(
            FakeServiceMode::MultiMonitorUi,
        );
        let target = inventory.targets().next().unwrap().clone();
        dialog.test_load_preopened_inventory(inventory);
        let failure = BrightnessFailure {
            kind: BrightnessFailureKind::HelperTimeout,
            operation: Some(crate::brightness_service::BrightnessOperation::SetBrightness),
            native_code: None,
            process_cleanup: Some(
                crate::brightness_service::HelperCleanupDisposition::ExitUnverified,
            ),
            set_disposition: Some(BrightnessSetDisposition::Unknown),
            set_cause: Some(crate::brightness_service::BrightnessSetFailureCause {
                kind: BrightnessFailureKind::NativeFailure,
                operation: Some(crate::brightness_service::BrightnessOperation::SetBrightness),
                native_code: Some(55),
            }),
        };
        dialog.accept_completion(
            PendingBrightnessKind::Set {
                target: target.clone(),
                requested_percent: 75,
                draft_at_submit: 50,
            },
            BrightnessCompletion::Set(Err(failure)),
        );
        let mut frame = 0;
        run_frame(&ctx, &mut dialog, &mut frame, Vec::new());

        let status = dialog.test_status(&target.identity).unwrap();
        assert!(status.contains("timed out"));
        assert!(status.contains("Set outcome is unknown"));
        assert!(status.contains("monitor operation failure"));
        assert!(status.contains("native code 55"));
        assert!(status.contains("exit unverified"));
        assert!(!dialog.inventory_fresh);
        assert!(!dialog.test_has_service());
        let set = row_for(&dialog, &target.identity).set.as_ref().unwrap();
        assert!(
            !set.enabled(),
            "Unknown Set is blocked until explicit Refresh"
        );
    }
}
