//! Retained Notification Inbox integration with current domain owners.
use super::jump_mode::ui::{
    NavigationDialogKind, NavigationOwner, focused_response_layer, visible_top_middle_layer,
};
use super::notification_inbox_dialog::{Command, Runtime};
use super::utility_host::{AcceptedParentOpen, ChildOpenOrigin, UtilityPresentation};
use super::utility_window_host::HostInput;
use super::{LauncherApp, Panel};
use crate::commands::DialogCommandHost;
use crate::notifications::{
    Category, FactId, InboxCommandOutcome, InternalEvent, InternalFact, Record, RelatedItem,
    Status, SubmitError, WindowsAccess, unix_now,
};
use eframe::egui;
use egui_toast::ToastKind;

impl LauncherApp {
    pub(super) fn open_notification_inbox(&mut self) {
        let was_open = self.notification_inbox_dialog.open;
        self.notification_inbox_dialog.ensure_open();
        let presentation = self.notification_inbox_host.open_from_origin(
            &self.egui_ctx,
            &self.notification_inbox_dialog,
            was_open,
            &ChildOpenOrigin::<()>::Root,
        );
        if presentation == Some(UtilityPresentation::Docked) {
            self.request_launcher_visibility(true);
        }
    }

    pub(super) fn close_notification_inbox(&mut self) {
        self.notification_inbox_dialog.close();
        self.notification_inbox_host.close_from_app(&self.egui_ctx);
        self.panel_stack
            .retain(|panel| *panel != Panel::NotificationInboxDialog);
    }

    pub(super) fn refresh_inbox_windows_access(&mut self, ctx: &egui::Context) {
        let enabled = self
            .notification_inbox
            .as_ref()
            .is_some_and(|service| service.snapshot().inbox.settings().external_enabled);
        if let Some(delay) = self.notification_windows_consent.tick(enabled) {
            ctx.request_repaint_after(delay);
        }
    }

    pub(super) fn wake_inbox_calendar_if_changed(&mut self) {
        let version = crate::plugins::calendar::calendar_version();
        if self.notification_calendar_version == version {
            return;
        }
        self.notification_calendar_version = version;
        if let Some(inbox) = &self.notification_inbox {
            inbox.calendar_changed();
        }
    }

    pub(super) fn record_launcher_notification(&mut self, kind: ToastKind) {
        if self.notification_session == [0; 16] {
            return;
        }
        let Some(inbox) = &self.notification_inbox else {
            return;
        };
        let Some(sequence) = self.notification_alert_sequence.checked_add(1) else {
            return;
        };
        let (category, status, fact) = match kind {
            ToastKind::Error => (
                Category::Failed,
                Status::Failed,
                InternalFact::LauncherError,
            ),
            ToastKind::Warning => (
                Category::Warning,
                Status::Current,
                InternalFact::LauncherWarning,
            ),
            _ => (
                Category::Information,
                Status::Current,
                InternalFact::LauncherInfo,
            ),
        };
        self.notification_alert_sequence = sequence;
        let _ = inbox.publisher().try_publish(InternalEvent {
            id: FactId::Launcher {
                session: self.notification_session,
                sequence,
            },
            occurred_at: unix_now(),
            category,
            status,
            fact,
            related: None,
        });
    }

    pub(super) fn show_notification_inbox(
        &mut self,
        ctx: &egui::Context,
        first: bool,
        root_visible: bool,
    ) {
        let open = self.notification_inbox_dialog.open;
        if !open || (!root_visible && !self.notification_inbox_host.needs_root_ui_when_hidden(open))
        {
            return;
        }
        let snapshot = self
            .notification_inbox
            .as_ref()
            .map(|inbox| inbox.snapshot());
        if first
            && let Some(request) = self.notification_request
            && let Some(receipt) = snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.receipts.iter().find(|r| r.request == request))
        {
            self.notification_notice = receipt_notice(receipt.outcome).map(str::to_owned);
            self.notification_request = None;
        }

        let mut windows_capture = snapshot
            .as_ref()
            .map_or_else(Default::default, |view| view.windows_capture);
        if windows_capture.access == WindowsAccess::PermissionNotGranted {
            let consent = self.notification_windows_consent.access();
            if matches!(consent, WindowsAccess::Requesting | WindowsAccess::Denied) {
                windows_capture.access = consent;
            }
        }

        let root_input = self.native_ui_host_input_allowed(ctx, root_visible);
        let layer = self.notification_inbox_host.root_layer();
        let parent_owner = root_input
            && layer.is_some()
            && visible_top_middle_layer(ctx) == layer
            && !egui::Popup::is_any_open(ctx);
        let keyboard_owner = parent_owner
            && focused_response_layer(ctx)
                .is_none_or(|focus| Some(focus) == layer || focus.order == egui::Order::Background);
        let native_input = self.native_ui_application_input_allowed(ctx)
            && !self.ocr_surface_visible()
            && !self.confirm_modal.is_open()
            && !self.action_sheet.is_open()
            && !self.pin_list_open;
        let navigation_owner = self.resolve_jump_mode_navigation_owner(ctx, root_visible);
        let scope = match navigation_owner {
            NavigationOwner::Dialog {
                kind: NavigationDialogKind::NotificationInbox,
                instance,
            } if keyboard_owner => self.jump_mode_ui.scope_for_navigation_owner(
                super::jump_mode::SurfaceId::dialog("notification-inbox", instance),
                super::jump_mode::TargetGeneration(instance),
                "notification-inbox/form",
                ctx.viewport_id(),
                navigation_owner,
                false,
            ),
            _ => None,
        };
        let pinned = self.pinned_panels.contains(&Panel::NotificationInboxDialog);
        self.notification_inbox_host
            .configure_jump(&self.jump_mode_ui, self.jump_mode_enabled());
        self.notification_inbox_host.show(
            ctx,
            &mut self.notification_inbox_dialog,
            &Runtime {
                snapshot,
                notice: self.notification_notice.clone(),
                now_unix: Some(unix_now()),
                busy: self.notification_request.is_some(),
                windows_capture,
            },
            HostInput {
                first,
                root_visible,
                root_input,
                native_input,
                root_owner: navigation_owner,
                parent_owner,
                root_jump_active: self.jump_mode_ui.active_session().is_some(),
                close_shortcut: !pinned && (layer.is_none() || keyboard_owner),
                allow_os_close: !pinned,
                confirmation_owner: NavigationOwner::Blocked,
                parent_instance: 0,
                root_scopes: (scope.as_ref(), None),
            },
        );
    }

    pub(super) fn complete_notification_inbox(&mut self, ctx: &egui::Context) {
        let Some(completion) = self
            .notification_inbox_host
            .complete(ctx, &mut self.notification_inbox_dialog)
        else {
            return;
        };
        for command in completion.commands {
            if matches!(command, Command::Closed) {
                self.close_notification_inbox();
            } else if let Some(origin) = &completion.origin
                && self
                    .notification_inbox_host
                    .accepts_origin(&self.notification_inbox_dialog, origin)
            {
                self.dispatch_inbox(command, origin);
                ctx.request_repaint_of(completion.owner_viewport);
            }
        }
        if completion.show_root {
            self.request_launcher_visibility(true);
        }
    }

    fn dispatch_inbox(&mut self, command: Command, origin: &AcceptedParentOpen<u64>) {
        if self.notification_request.is_some() {
            return;
        }
        match command {
            Command::Closed => self.close_notification_inbox(),
            Command::RequestWindowsAccess => {
                let enabled = self
                    .notification_inbox
                    .as_ref()
                    .is_some_and(|service| service.snapshot().inbox.settings().external_enabled);
                let ctx = self.egui_ctx.clone();
                self.notification_notice = self
                    .notification_windows_consent
                    .request(enabled, move || ctx.request_repaint())
                    .err()
                    .map(str::to_owned);
            }
            Command::Submit(command) => {
                let Some(inbox) = &self.notification_inbox else {
                    return;
                };
                match inbox.submit(command) {
                    Ok(request) => {
                        self.notification_request = Some(request);
                        self.notification_notice = None;
                    }
                    Err(error) => {
                        self.notification_notice = Some(
                            match error {
                                SubmitError::Full => "The Inbox is busy. Try again shortly.",
                                SubmitError::Closed => "The Inbox worker is unavailable.",
                                SubmitError::Invalid => "The Inbox request is invalid.",
                                SubmitError::Exhausted => {
                                    "The Inbox has exhausted its request IDs."
                                }
                            }
                            .into(),
                        )
                    }
                }
            }
            Command::OpenRelated(record) => {
                let is_current = self.notification_inbox.as_ref().is_some_and(|inbox| {
                    inbox
                        .snapshot()
                        .inbox
                        .records()
                        .iter()
                        .find(|current| current.id == record.id)
                        == Some(&record)
                });
                if !is_current || !self.open_inbox_related(&record, origin) {
                    self.notification_notice =
                        Some("The related item is no longer available to open.".into());
                } else {
                    self.notification_notice = None;
                }
            }
        }
    }

    fn open_inbox_related(&mut self, record: &Record, origin: &AcceptedParentOpen<u64>) -> bool {
        if record.status == Status::SourceRemoved {
            return false;
        }
        match &record.related {
            Some(RelatedItem::Timer { .. }) => {
                match crate::plugins::timer::related_inbox_timer(&record.id) {
                    Some(true) => self.open_alarm_dialog(),
                    Some(false) => self.open_timer_dialog(),
                    None => return false,
                }
                true
            }
            Some(RelatedItem::Calendar { .. }) => {
                let Ok(profile) =
                    crate::platform::app_data::AppDataRoot::from_settings_path(&self.settings_path)
                else {
                    return false;
                };
                if let Some(instance) = crate::plugins::calendar::related_inbox_calendar_occurrence(
                    &profile, &record.id,
                ) {
                    self.open_calendar_details_from(
                        instance,
                        &ChildOpenOrigin::Parent(origin.clone()),
                    );
                    true
                } else {
                    false
                }
            }
            Some(RelatedItem::Macro { id, .. }) => {
                let exists = self
                    .plugins
                    .internal_services()
                    .mkmacro_store
                    .snapshot()
                    .macros
                    .iter()
                    .any(|entry| entry.id == *id);
                if !exists {
                    return false;
                }
                self.open_mkmacro_dialog();
                self.mkmacro_dialog.set_selected_macro(Some(*id));
                self.mkmacro_dialog
                    .selected_macro()
                    .is_some_and(|entry| entry.id == *id)
            }
            Some(RelatedItem::HotFolder { rule }) => {
                let Some(store) = self.hotfolder_runtime.snapshot().store else {
                    return false;
                };
                let Some(current) = store.rules.iter().find(|item| item.id.as_str() == rule) else {
                    return false;
                };
                if !self
                    .hotfolder_dialog
                    .open_inbox_rule(store.revision, current.clone())
                {
                    return false;
                }
                self.open_hotfolder_manager();
                self.request_launcher_visibility(true);
                true
            }
            Some(RelatedItem::Transfer { session, job }) => {
                #[cfg(windows)]
                {
                    if *session != self.notification_session {
                        return false;
                    }
                    let snapshot = self.transfer_jobs.snapshot();
                    let Some(info) = snapshot
                        .active
                        .iter()
                        .chain(&snapshot.queued)
                        .chain(&snapshot.history)
                        .find(|entry| entry.id.get() == *job)
                    else {
                        return false;
                    };
                    self.file_operations_center_dialog.focus_job(Some(info.id));
                    self.open_file_operations_from_origin(
                        None,
                        &ChildOpenOrigin::Parent(origin.clone()),
                    );
                    true
                }
                #[cfg(not(windows))]
                {
                    let _ = (session, job);
                    false
                }
            }
            Some(RelatedItem::Activity { session, id })
                if *session == self.notification_session =>
            {
                let snapshot = self.activity_center.snapshot();
                let Some(row) = snapshot.rows.iter().find(|row| row.key.operation_id().get() == *id
                    && matches!(&record.id, FactId::Activity { generation, .. } if row.key.incarnation().get() == *generation))
                else { return false };
                self.activity_dialog
                    .set_filter(row.operation.label().into());
                self.open_activity_dialog();
                true
            }
            _ => false,
        }
    }
}

fn receipt_notice(outcome: InboxCommandOutcome) -> Option<&'static str> {
    use InboxCommandOutcome as Outcome;
    match outcome {
        Outcome::Applied => None,
        Outcome::NotFound | Outcome::Stale => {
            Some("The notification changed or is no longer available.")
        }
        Outcome::Invalid => Some("This action is unavailable or has already been applied."),
        Outcome::TodoCreated => Some("The follow-up task is in Todo."),
        Outcome::TodoFailed => Some("Could not create the follow-up task."),
        Outcome::Snoozed => Some("Reminder snoozed."),
        Outcome::SnoozeFailed => {
            Some("The reminder could not be snoozed. It may have changed or expired.")
        }
        Outcome::SourceOpened => Some("Windows opened the source app."),
        Outcome::SourceDismissRequested => {
            Some("Windows accepted the request to dismiss this notification.")
        }
        Outcome::SourceUnavailable => Some(
            "The source action is unavailable. Windows may have removed the notification or revoked access.",
        ),
        Outcome::StoreFailed => Some("The Inbox could not save this change."),
    }
}
