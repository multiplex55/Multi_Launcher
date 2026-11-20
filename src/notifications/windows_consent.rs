//! Foreground consent and identity for the optional Windows notification listener.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) enum WindowsAccess {
    #[default]
    Off,
    SetupRequired,
    PermissionNotGranted,
    Requesting,
    Denied,
    Allowed,
    Active,
    Disconnected,
    Unsupported,
    Error,
}

impl WindowsAccess {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Off => "Off",
            Self::SetupRequired => "Setup required",
            Self::PermissionNotGranted => "Permission not granted",
            Self::Requesting => "Waiting for Windows consent",
            Self::Denied => "Denied",
            Self::Allowed => "Permission granted",
            Self::Active => "Active",
            Self::Disconnected => "Disconnected",
            Self::Unsupported => "Unsupported",
            Self::Error => "Error",
        }
    }
}

#[cfg(windows)]
mod native {
    use super::WindowsAccess;
    use std::{marker::PhantomData, rc::Rc, time::Duration};
    use windows::{
        Foundation::{AsyncOperationCompletedHandler, AsyncStatus, IAsyncOperation},
        UI::Notifications::Management::{
            UserNotificationListener, UserNotificationListenerAccessStatus,
        },
        Win32::{
            Foundation::{
                APPMODEL_ERROR_NO_APPLICATION, APPMODEL_ERROR_NO_PACKAGE, E_NOTIMPL, ERROR_SUCCESS,
                REGDB_E_CLASSNOTREG,
            },
            Storage::Packaging::Appx::{
                GetCurrentApplicationUserModelId, GetCurrentPackageFamilyName,
            },
            System::{
                Threading::GetCurrentThreadId,
                WinRT::{RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize},
            },
            UI::WindowsAndMessaging::{GetForegroundWindow, GetWindowThreadProcessId},
        },
        core::{Error, PWSTR},
    };

    const FAMILY_PREFIX: &str = "MultiLauncher.NotificationInbox_";
    #[cfg(debug_assertions)]
    const FIXTURE_FAMILY_PREFIX: &str = "MultiLauncher.NotificationFixture_";
    const REFRESH: Duration = Duration::from_secs(3);

    pub(super) fn inbox_identity(family: &str, application: &str) -> bool {
        let publisher = family.strip_prefix(FAMILY_PREFIX);
        #[cfg(debug_assertions)]
        let publisher = publisher.or_else(|| family.strip_prefix(FIXTURE_FAMILY_PREFIX));
        let valid_family = publisher.is_some_and(|key| {
            key.len() == 13
                && key
                    .bytes()
                    .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
        });
        valid_family && application.strip_prefix(family) == Some("!MultiLauncher")
    }

    pub(crate) fn runtime_aumid() -> Result<Option<String>, WindowsAccess> {
        let mut family = [0u16; 128];
        let mut family_size = family.len() as u32;
        // Both writable buffers have the capacity supplied to their respective APIs.
        let status =
            unsafe { GetCurrentPackageFamilyName(&mut family_size, PWSTR(family.as_mut_ptr())) };
        if status == APPMODEL_ERROR_NO_PACKAGE {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(WindowsAccess::Error);
        }
        let mut application = [0u16; 256];
        let mut application_size = application.len() as u32;
        let status = unsafe {
            GetCurrentApplicationUserModelId(&mut application_size, PWSTR(application.as_mut_ptr()))
        };
        if status == APPMODEL_ERROR_NO_APPLICATION {
            return Ok(None);
        }
        if status != ERROR_SUCCESS {
            return Err(WindowsAccess::Error);
        }
        let decode = |buffer: &[u16], size: u32| {
            let end = usize::try_from(size).ok()?.checked_sub(1)?;
            if buffer.get(end) != Some(&0) {
                return None;
            }
            String::from_utf16(buffer.get(..end)?).ok()
        };
        let family = decode(&family, family_size).ok_or(WindowsAccess::Error)?;
        let application = decode(&application, application_size).ok_or(WindowsAccess::Error)?;
        Ok(inbox_identity(&family, &application).then_some(application))
    }

    pub(crate) fn access_status(access: UserNotificationListenerAccessStatus) -> WindowsAccess {
        if access == UserNotificationListenerAccessStatus::Allowed {
            WindowsAccess::Allowed
        } else if access == UserNotificationListenerAccessStatus::Denied {
            WindowsAccess::Denied
        } else if access == UserNotificationListenerAccessStatus::Unspecified {
            WindowsAccess::PermissionNotGranted
        } else {
            WindowsAccess::Unsupported
        }
    }
    pub(crate) fn windows_error(error: Error) -> WindowsAccess {
        match error.code() {
            E_NOTIMPL | REGDB_E_CLASSNOTREG => WindowsAccess::Unsupported,
            _ => WindowsAccess::Error,
        }
    }

    struct ForegroundApartment(PhantomData<Rc<()>>);
    impl ForegroundApartment {
        fn initialize() -> Result<Self, WindowsAccess> {
            // The app owns this STA initialization on its UI thread.
            unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.map_err(windows_error)?;
            Ok(Self(PhantomData))
        }
    }
    impl Drop for ForegroundApartment {
        fn drop(&mut self) {
            // This non-Send token balances its own successful initialization.
            unsafe { RoUninitialize() };
        }
    }

    #[derive(Default)]
    pub(crate) struct WindowsConsent {
        pending: Option<IAsyncOperation<UserNotificationListenerAccessStatus>>,
        listener: Option<UserNotificationListener>,
        apartment: Option<ForegroundApartment>,
        access: WindowsAccess,
    }

    impl WindowsConsent {
        pub(crate) fn access(&self) -> WindowsAccess {
            self.access
        }

        pub(crate) fn tick(&mut self, enabled: bool) -> Option<Duration> {
            if !enabled {
                self.stop();
                return None;
            }
            let operation = self.pending.as_ref()?;
            self.access = match operation.Status() {
                Ok(AsyncStatus::Started) => return Some(REFRESH),
                Ok(AsyncStatus::Completed) => operation
                    .GetResults()
                    .map(access_status)
                    .unwrap_or_else(windows_error),
                Ok(_) | Err(_) => WindowsAccess::Error,
            };
            self.pending = None;
            self.release_platform();
            None
        }

        pub(crate) fn request(
            &mut self,
            enabled: bool,
            wake: impl Fn() + Send + 'static,
        ) -> Result<(), &'static str> {
            if !enabled || self.pending.is_some() {
                return Err("Windows access cannot be requested in the current Inbox state.");
            }
            // Request consent only on the foreground launcher window's UI thread.
            let foreground = unsafe { GetForegroundWindow() };
            if foreground.is_invalid()
                || unsafe { GetWindowThreadProcessId(foreground, None) != GetCurrentThreadId() }
            {
                return Err("Focus the Inbox window before requesting Windows access.");
            }
            self.initialize().map_err(|access| {
                self.access = access;
                "The Windows notification listener is unavailable."
            })?;
            let Some(listener) = &self.listener else {
                return Err("The Windows notification listener is unavailable.");
            };
            self.access = listener
                .GetAccessStatus()
                .map(access_status)
                .unwrap_or_else(windows_error);
            if self.access != WindowsAccess::PermissionNotGranted {
                self.release_platform();
                return Err("Windows access cannot be requested in the current Inbox state.");
            }
            let operation = match listener.RequestAccessAsync() {
                Ok(operation) => operation,
                Err(failure) => {
                    self.access = windows_error(failure);
                    self.release_platform();
                    return Err("Windows could not start the notification access request.");
                }
            };
            let handler = AsyncOperationCompletedHandler::new(move |_, _| {
                wake();
                Ok(())
            });
            // If Windows declines the completion callback, the UI heartbeat still observes it.
            let _ = operation.SetCompleted(&handler);
            self.pending = Some(operation);
            self.access = WindowsAccess::Requesting;
            Ok(())
        }

        pub(crate) fn stop(&mut self) {
            if let Some(operation) = self.pending.take() {
                let _ = operation.Cancel();
            }
            self.release_platform();
            self.access = WindowsAccess::Off;
        }

        fn release_platform(&mut self) {
            self.listener = None;
            self.apartment = None;
        }

        fn initialize(&mut self) -> Result<(), WindowsAccess> {
            if self.listener.is_some() {
                return Ok(());
            }
            if runtime_aumid()?.is_none() {
                return Err(WindowsAccess::SetupRequired);
            }
            let apartment = ForegroundApartment::initialize()?;
            let listener = UserNotificationListener::Current().map_err(windows_error)?;
            self.apartment = Some(apartment);
            self.listener = Some(listener);
            Ok(())
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn notification_inbox_windows_requires_exact_current_app_identity() {
            let family = "MultiLauncher.NotificationInbox_123456789abcd";
            assert!(inbox_identity(family, &format!("{family}!MultiLauncher")));
            assert!(!inbox_identity(family, &format!("{family}!AnotherApp")));
            assert!(!inbox_identity(family, "MultiLauncher.MultiLnchr"));
            assert!(!inbox_identity(
                "MultiLauncher.NotificationInbox_other",
                family
            ));
        }

        #[test]
        fn notification_inbox_windows_disabled_consent_never_initializes_winrt() {
            let mut consent = WindowsConsent::default();
            assert_eq!(consent.tick(false), None);
            assert_eq!(consent.access(), WindowsAccess::Off);
            assert!(
                consent
                    .request(false, || panic!("Consent must remain idle."))
                    .is_err()
            );
            assert!(consent.apartment.is_none());
            assert!(consent.listener.is_none());
        }
    }
}

#[cfg(windows)]
pub(crate) use native::WindowsConsent;
#[cfg(windows)]
pub(super) use native::{access_status, runtime_aumid, windows_error};

#[cfg(not(windows))]
#[derive(Default)]
pub(crate) struct WindowsConsent {
    access: WindowsAccess,
}
#[cfg(not(windows))]
impl WindowsConsent {
    pub(crate) fn tick(&mut self, enabled: bool) -> Option<std::time::Duration> {
        self.access = if enabled {
            WindowsAccess::Unsupported
        } else {
            WindowsAccess::Off
        };
        None
    }
    pub(crate) fn access(&self) -> WindowsAccess {
        self.access
    }
    pub(crate) fn request(
        &mut self,
        _: bool,
        _: impl Fn() + Send + 'static,
    ) -> Result<(), &'static str> {
        Err("Windows notification access is unavailable on this platform.")
    }
    pub(crate) fn stop(&mut self) {
        self.access = WindowsAccess::Off;
    }
}
