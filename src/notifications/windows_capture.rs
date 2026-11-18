//! An opt-in listener owned by the existing Inbox worker.
use super::{FactId, InboxStore, Record, Status, WindowsAccess};
use std::{sync::Arc, time::Duration};

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct WindowsCaptureView {
    pub access: WindowsAccess,
    pub started_at: Option<i64>,
    pub incomplete: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum WindowsSourceAction {
    Open,
    Dismiss,
}

#[cfg(windows)]
mod native {
    use super::*;
    use crate::mkmacro::notifications::DESKTOP_AUMID;
    use crate::notifications::{
        Content, Settings, Source,
        windows_consent::{access_status, runtime_aumid, windows_error},
        windows_toast_projection::{
            MAX_TEXT_BYTES, MAX_TEXT_PARTS, MAX_TOASTS, ObservedToast, ToastObservation,
            ToastProjection, text_from_utf16, unix_seconds,
        },
    };
    use std::{
        sync::atomic::{AtomicBool, Ordering},
        time::Instant,
    };
    use windows::{
        ApplicationModel::AppInfo,
        Foundation::{
            AsyncOperationCompletedHandler, AsyncStatus, Collections::IVectorView,
            EventRegistrationToken, IAsyncOperation, TypedEventHandler,
        },
        UI::Notifications::{
            KnownNotificationBindings, Management::UserNotificationListener, NotificationKinds,
            UserNotification,
        },
        Win32::{
            System::{
                Com::{CLSCTX_INPROC_SERVER, CoCreateInstance},
                WinRT::{RO_INIT_MULTITHREADED, RoInitialize, RoUninitialize},
            },
            UI::Shell::{
                AO_NOERRORUI, ApplicationActivationManager, IApplicationActivationManager,
            },
        },
        core::{HSTRING, PCWSTR},
    };

    const PERMISSION_INTERVAL: Duration = Duration::from_secs(4);
    const SNAPSHOT_INTERVAL: Duration = Duration::from_secs(30);
    const RETRY_INTERVAL: Duration = Duration::from_secs(12);
    const SNAPSHOT_TIMEOUT: Duration = Duration::from_secs(12);
    const CHANGE_DELAY: Duration = Duration::from_millis(250);
    const MAX_APP_ID: usize = 512;
    const MAX_SNAPSHOT_BYTES: usize = 1024 * 1024;
    #[cfg(debug_assertions)]
    const SYNTHETIC_SOURCE: &str = "MultiLauncher.Q1A.SyntheticSource.61a44f4a";

    type SnapshotRead = IAsyncOperation<IVectorView<UserNotification>>;
    type Wake = Arc<dyn Fn() + Send + Sync>;

    #[derive(Default)]
    struct Signals {
        changed: AtomicBool,
        completed: AtomicBool,
    }

    struct Apartment;
    impl Drop for Apartment {
        fn drop(&mut self) {
            // The Inbox worker balances its own successful initialization.
            unsafe { RoUninitialize() };
        }
    }

    struct Binding {
        listener: UserNotificationListener,
        subscription: Option<EventRegistrationToken>,
        own_aumid: String,
        _apartment: Apartment,
    }

    impl Binding {
        fn open() -> Result<Self, WindowsAccess> {
            let own_aumid = runtime_aumid()?.ok_or(WindowsAccess::SetupRequired)?;
            // The Inbox thread owns this MTA reference until Binding is dropped.
            unsafe { RoInitialize(RO_INIT_MULTITHREADED) }.map_err(windows_error)?;
            let apartment = Apartment;
            let listener = UserNotificationListener::Current().map_err(windows_error)?;
            Ok(Self {
                listener,
                subscription: None,
                own_aumid,
                _apartment: apartment,
            })
        }

        fn includes(&self, app_id: &str) -> bool {
            if app_id.eq_ignore_ascii_case(&self.own_aumid)
                || app_id.eq_ignore_ascii_case(DESKTOP_AUMID)
            {
                return false;
            }
            // The disposable signing identity can only read metadata and text for its
            // separately registered synthetic sender. This filter runs before visual reads.
            #[cfg(debug_assertions)]
            if self
                .own_aumid
                .starts_with("MultiLauncher.NotificationFixture_")
            {
                return app_id.eq_ignore_ascii_case(SYNTHETIC_SOURCE);
            }
            true
        }

        fn permission(&self) -> WindowsAccess {
            self.listener
                .GetAccessStatus()
                .map(access_status)
                .unwrap_or_else(windows_error)
        }

        fn start_observing(&mut self, signals: &Arc<Signals>, wake: &Wake) {
            if self.subscription.is_some() {
                return;
            }
            let signals = signals.clone();
            let wake = wake.clone();
            let handler = TypedEventHandler::new(move |_, _| {
                if !signals.changed.swap(true, Ordering::AcqRel) {
                    wake();
                }
                Ok(())
            });
            // Periodic reads also observe changes on systems that cannot deliver this event.
            self.subscription = self.listener.NotificationChanged(&handler).ok();
        }

        fn unsubscribe(&mut self) {
            if let Some(token) = self.subscription.take() {
                let _ = self.listener.RemoveNotificationChanged(token);
            }
        }

        fn read(&self, signals: &Arc<Signals>, wake: &Wake) -> windows::core::Result<SnapshotRead> {
            let read = self
                .listener
                .GetNotificationsAsync(NotificationKinds::Toast)?;
            let signals = signals.clone();
            let wake = wake.clone();
            let handler = AsyncOperationCompletedHandler::new(move |_, _| {
                signals.completed.store(true, Ordering::Release);
                wake();
                Ok(())
            });
            let _ = read.SetCompleted(&handler);
            Ok(read)
        }

        fn observe(
            &self,
            snapshot: IVectorView<UserNotification>,
            settings: &Settings,
        ) -> windows::core::Result<ToastObservation> {
            let size = snapshot.Size()?;
            let mut observation = ToastObservation {
                toasts: Vec::with_capacity(size.min(MAX_TOASTS) as usize),
                complete: size <= MAX_TOASTS,
            };
            let mut bytes = 0;
            for index in 0..size.min(MAX_TOASTS) {
                if index % 16 == 0 && self.permission() != WindowsAccess::Allowed {
                    return Err(windows::core::Error::from(
                        windows::Win32::Foundation::E_ACCESSDENIED,
                    ));
                }
                let Ok(notification) = snapshot.GetAt(index) else {
                    observation.complete = false;
                    continue;
                };
                let Some(app) = notification_aumid(&notification) else {
                    observation.complete = false;
                    continue;
                };
                if !self.includes(&app) || settings.excluded_apps.contains(&app) {
                    continue;
                }
                let Ok(time) = notification.CreationTime() else {
                    observation.complete = false;
                    continue;
                };
                let created = time.UniversalTime;
                let Some(occurred_at) = unix_seconds(created) else {
                    observation.complete = false;
                    continue;
                };
                let Ok(notification_id) = notification.Id() else {
                    observation.complete = false;
                    continue;
                };
                let (title, body) = settings
                    .retain_external_text
                    .then(|| notification_text(&notification))
                    .flatten()
                    .unwrap_or_default();
                bytes += app.len()
                    + title.as_ref().map_or(0, String::len)
                    + body.as_ref().map_or(0, String::len);
                if bytes > MAX_SNAPSHOT_BYTES {
                    observation.complete = false;
                    break;
                }
                let id = FactId::Windows {
                    app,
                    notification: notification_id,
                    created,
                };
                observation.toasts.push(ObservedToast {
                    id,
                    occurred_at,
                    title,
                    body,
                });
            }
            Ok(observation)
        }
    }

    impl Drop for Binding {
        fn drop(&mut self) {
            self.unsubscribe();
        }
    }

    struct Pending {
        read: SnapshotRead,
        limit: Instant,
    }

    #[derive(Default)]
    pub(crate) struct WindowsCapture {
        pending: Option<Pending>,
        binding: Option<Binding>,
        signals: Arc<Signals>,
        projection: ToastProjection,
        check_at: Option<Instant>,
        read_at: Option<Instant>,
        last_read: Option<Instant>,
        view: WindowsCaptureView,
    }

    impl WindowsCapture {
        pub(crate) fn view(&self) -> WindowsCaptureView {
            self.view
        }

        pub(crate) fn wait(&self) -> Option<Duration> {
            self.check_at
                .into_iter()
                .chain(self.read_at.filter(|_| self.pending.is_none()))
                .chain(self.pending.as_ref().map(|read| read.limit))
                .min()
                .map(|at| at.saturating_duration_since(Instant::now()))
        }

        pub(crate) fn tick(&mut self, store: &mut InboxStore, wake: &Wake) -> anyhow::Result<bool> {
            if !store.inbox().settings().external_enabled
                || store.inbox().settings().excluded.contains(&Source::Windows)
            {
                if self.view.access != WindowsAccess::Off {
                    self.stop();
                }
                return Ok(false);
            }
            if self.view.access == WindowsAccess::SetupRequired {
                return Ok(false);
            }
            let now = Instant::now();
            if self.binding.is_none() {
                if self.check_at.is_some_and(|at| now < at) {
                    return Ok(false);
                }
                match Binding::open() {
                    Ok(binding) => self.binding = Some(binding),
                    Err(access) => {
                        self.view.access = access;
                        self.check_at = if access == WindowsAccess::SetupRequired {
                            None
                        } else {
                            Some(now + RETRY_INTERVAL)
                        };
                        return Ok(false);
                    }
                }
            }

            if self.signals.changed.swap(false, Ordering::AcqRel)
                && matches!(
                    self.view.access,
                    WindowsAccess::Active | WindowsAccess::Allowed
                )
            {
                let due = self
                    .last_read
                    .map_or(now, |last| (last + CHANGE_DELAY).max(now));
                self.read_at = Some(self.read_at.map_or(due, |prior| prior.min(due)));
            }
            let completed = self.signals.completed.swap(false, Ordering::AcqRel);
            let read_due = self
                .read_at
                .is_some_and(|at| self.pending.is_none() && now >= at);
            let expired = self
                .pending
                .as_ref()
                .is_some_and(|pending| now >= pending.limit);
            if !completed && !read_due && !expired && self.check_at.is_some_and(|at| now < at) {
                return Ok(false);
            }
            self.check_at = Some(now + PERMISSION_INTERVAL);
            let Some(binding) = &mut self.binding else {
                return Ok(false);
            };
            let access = binding.permission();
            if access != WindowsAccess::Allowed {
                binding.unsubscribe();
                self.cancel_read();
                self.read_at = None;
                self.view = WindowsCaptureView {
                    access,
                    ..Default::default()
                };
                return Ok(false);
            }
            if self.view.access != WindowsAccess::Active {
                self.view.access = WindowsAccess::Allowed;
            }
            if self.read_at.is_none() {
                binding.start_observing(&self.signals, wake);
                self.read_at = Some(now);
            }

            if let Some(pending) = &self.pending {
                match pending.read.Status() {
                    Ok(AsyncStatus::Started) if !expired => return Ok(false),
                    Ok(AsyncStatus::Completed) => {
                        let result = pending.read.GetResults().and_then(|snapshot| {
                            binding.observe(snapshot, store.inbox().settings())
                        });
                        self.pending = None;
                        let access = binding.permission();
                        if access != WindowsAccess::Allowed {
                            binding.unsubscribe();
                            self.read_at = None;
                            self.view = WindowsCaptureView {
                                access,
                                ..Default::default()
                            };
                            return Ok(false);
                        }
                        let Ok(observation) = result else {
                            self.disconnect(now);
                            return Ok(false);
                        };
                        self.view.access = WindowsAccess::Active;
                        let wall = super::super::unix_now();
                        self.view.started_at.get_or_insert(wall);
                        self.view.incomplete = !observation.complete;
                        let mut projection = self.projection.clone();
                        let changed = store.transact(|inbox| {
                            projection.apply(inbox, observation, wall, |app| binding.includes(app))
                        })?;
                        self.projection = projection;
                        return Ok(changed);
                    }
                    _ => {
                        self.disconnect(now);
                        return Ok(false);
                    }
                }
            }

            if self.read_at.is_some_and(|at| now >= at) {
                match binding.read(&self.signals, wake) {
                    Ok(read) => {
                        self.pending = Some(Pending {
                            read,
                            limit: now + SNAPSHOT_TIMEOUT,
                        });
                        self.last_read = Some(now);
                        self.read_at = Some(now + SNAPSHOT_INTERVAL);
                    }
                    Err(_) => self.disconnect(now),
                }
            }
            Ok(false)
        }

        pub(crate) fn action(
            &mut self,
            record: &Record,
            action: WindowsSourceAction,
            retain_text: bool,
        ) -> bool {
            let FactId::Windows {
                app, notification, ..
            } = &record.id
            else {
                return false;
            };
            let Some(binding) = &self.binding else {
                return false;
            };
            if self.view.access != WindowsAccess::Active
                || record.status != Status::Current
                || !binding.includes(app)
                || binding.permission() != WindowsAccess::Allowed
            {
                return false;
            }
            let Ok(entry) = binding.listener.GetNotification(*notification) else {
                self.signals.changed.store(true, Ordering::Release);
                return false;
            };
            if notification_identity(&entry).as_ref() != Some(&record.id)
                || binding.permission() != WindowsAccess::Allowed
            {
                self.signals.changed.store(true, Ordering::Release);
                return false;
            }
            if action == WindowsSourceAction::Dismiss
                && retain_text
                && !text_matches_record(record, notification_text(&entry))
            {
                self.signals.changed.store(true, Ordering::Release);
                return false;
            }
            if binding.permission() != WindowsAccess::Allowed {
                return false;
            }
            match action {
                WindowsSourceAction::Open => open_packaged_app(app),
                WindowsSourceAction::Dismiss => {
                    if binding.listener.RemoveNotification(*notification).is_err()
                        || binding.permission() != WindowsAccess::Allowed
                    {
                        return false;
                    }
                    self.signals.changed.store(true, Ordering::Release);
                    true
                }
            }
        }

        fn cancel_read(&mut self) {
            if let Some(pending) = self.pending.take() {
                let _ = pending.read.Cancel();
            }
        }
        fn disconnect(&mut self, now: Instant) {
            self.cancel_read();
            self.binding = None;
            self.view.access = WindowsAccess::Disconnected;
            self.check_at = Some(now + RETRY_INTERVAL);
            self.read_at = None;
        }
        pub(crate) fn stop(&mut self) {
            self.cancel_read();
            *self = Self::default();
        }
    }

    fn notification_aumid(notification: &UserNotification) -> Option<String> {
        let app = notification.AppInfo().ok()?.AppUserModelId().ok()?;
        let wide = app.as_wide();
        if wide.is_empty() || wide.len() > MAX_APP_ID {
            return None;
        }
        let app = String::from_utf16(wide).ok()?;
        if app.len() > MAX_APP_ID || app.chars().any(char::is_control) {
            return None;
        }
        Some(app)
    }

    fn notification_identity(notification: &UserNotification) -> Option<FactId> {
        Some(FactId::Windows {
            app: notification_aumid(notification)?,
            notification: notification.Id().ok()?,
            created: notification.CreationTime().ok()?.UniversalTime,
        })
    }

    fn notification_text(
        notification: &UserNotification,
    ) -> Option<(Option<String>, Option<String>)> {
        let visual = notification.Notification().ok()?.Visual().ok()?;
        let name = KnownNotificationBindings::ToastGeneric().ok()?;
        let segments = visual.GetBinding(&name).ok()?.GetTextElements().ok()?;
        let count = segments.Size().ok()?.min(MAX_TEXT_PARTS);
        let title = segments
            .GetAt(0)
            .ok()
            .and_then(|part| part.Text().ok())
            .and_then(|value| text_from_utf16(value.as_wide(), MAX_TEXT_BYTES));
        let mut body = String::new();
        for index in 1..count {
            let remaining = MAX_TEXT_BYTES.saturating_sub(body.len()).saturating_sub(1);
            if remaining == 0 {
                break;
            }
            let Some(line) = segments
                .GetAt(index)
                .ok()
                .and_then(|part| part.Text().ok())
                .and_then(|value| text_from_utf16(value.as_wide(), remaining))
            else {
                continue;
            };
            if !body.is_empty() {
                body.push('\n');
            }
            body.push_str(&line);
        }
        let body = (!body.is_empty()).then_some(body);
        Some((title, body))
    }

    fn text_matches_record(
        record: &Record,
        observed: Option<(Option<String>, Option<String>)>,
    ) -> bool {
        match (&record.content, observed) {
            (Content::External { title, body, .. }, Some((current_title, current_body))) => {
                *title == current_title && *body == current_body
            }
            _ => false,
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn notification_inbox_windows_dismiss_refuses_changed_or_unavailable_text() {
            use crate::notifications::{Category, Inbox};

            let mut inbox = Inbox::default();
            let mut settings = inbox.settings().clone();
            settings.external_enabled = true;
            settings.retain_external_text = true;
            inbox.set_settings(settings, 30).unwrap();
            inbox.ingest_windows(
                FactId::Windows {
                    app: "Fixture.Synthetic".into(),
                    notification: 5,
                    created: 20,
                },
                1,
                20,
                Category::Information,
                Status::Current,
                Some("Initial"),
                Some("Body"),
                30,
            );
            let selected = &inbox.records()[0];
            assert!(text_matches_record(
                selected,
                Some((Some("Initial".into()), Some("Body".into())))
            ));
            assert!(!text_matches_record(
                selected,
                Some((Some("Changed".into()), Some("Body".into())))
            ));
            assert!(!text_matches_record(selected, None));
        }

        #[test]
        fn notification_inbox_windows_disabled_capture_never_binds_or_schedules() {
            let mut capture = WindowsCapture::default();
            let mut store = InboxStore::memory_only();
            let wake: Wake = Arc::new(|| panic!("Off capture remains idle."));
            assert!(!capture.tick(&mut store, &wake).unwrap());
            assert!(capture.binding.is_none());
            assert!(capture.pending.is_none());
            assert!(capture.wait().is_none());
            assert_eq!(capture.view().access, WindowsAccess::Off);
        }
    }

    fn open_packaged_app(app: &str) -> bool {
        let Some((family, _)) = app.split_once('!') else {
            return false;
        };
        let Ok(info) = AppInfo::GetFromAppUserModelId(&HSTRING::from(app)) else {
            return false;
        };
        if !info.AppUserModelId().is_ok_and(|value| value == app)
            || !info.PackageFamilyName().is_ok_and(|value| value == family)
        {
            return false;
        }
        // Windows activates the exact validated packaging identity with no launch arguments.
        let activation: windows::core::Result<IApplicationActivationManager> =
            unsafe { CoCreateInstance(&ApplicationActivationManager, None, CLSCTX_INPROC_SERVER) };
        let id = HSTRING::from(app);
        activation.is_ok_and(|activation| unsafe {
            activation
                .ActivateApplication(&id, PCWSTR::null(), AO_NOERRORUI)
                .is_ok()
        })
    }
}

#[cfg(windows)]
pub(crate) use native::WindowsCapture;

#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct WindowsCapture {
    view: WindowsCaptureView,
}
#[cfg(not(windows))]
impl WindowsCapture {
    pub(crate) fn view(&self) -> WindowsCaptureView {
        self.view
    }
    pub(crate) fn wait(&self) -> Option<Duration> {
        None
    }
    pub(crate) fn tick(
        &mut self,
        store: &mut InboxStore,
        _: &Arc<dyn Fn() + Send + Sync>,
    ) -> anyhow::Result<bool> {
        self.view.access = if store.inbox().settings().external_enabled {
            WindowsAccess::Unsupported
        } else {
            WindowsAccess::Off
        };
        Ok(false)
    }
    pub(crate) fn action(&mut self, _: &Record, _: WindowsSourceAction, _: bool) -> bool {
        false
    }
    pub(crate) fn stop(&mut self) {
        self.view = WindowsCaptureView::default();
    }
}
