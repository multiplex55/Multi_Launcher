//! Native desktop notification presentation and registration.
use super::{
    DiagnosticKind, ExecResult, ExecutionDiagnostic, MkNotificationDuration, NotificationBackend,
    ResolvedNotification,
};

pub const DESKTOP_AUMID: &str = "MultiLauncher.MultiLnchr";

/// Produces the complete toast payload without touching platform APIs. Keeping
/// this pure makes escaping and presentation independently testable.
pub fn toast_xml(notification: &ResolvedNotification) -> String {
    fn escape(value: &str) -> String {
        value
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&apos;")
    }
    let title = if notification.show_symbol {
        format!("{} {}", notification.kind.symbol(), notification.title)
    } else {
        notification.title.clone()
    };
    let duration = match notification.duration {
        MkNotificationDuration::Short => "short",
        MkNotificationDuration::Long => "long",
    };
    format!(
        "<toast duration=\"{duration}\"><visual><binding template=\"ToastGeneric\"><text>{}</text><text>{}</text></binding></visual><audio silent=\"true\"/></toast>",
        escape(&title),
        escape(&notification.description)
    )
}

#[cfg(windows)]
mod platform {
    use super::*;
    use ::windows::{
        Data::Xml::Dom::XmlDocument,
        UI::Notifications::{ToastNotification, ToastNotificationManager},
        Win32::{
            System::Com::{
                CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
                IPersistFile,
            },
            UI::Shell::{IShellLinkW, ShellLink},
        },
        core::{HSTRING, Interface, PCWSTR},
    };
    use std::{os::windows::ffi::OsStrExt, sync::OnceLock};

    fn diagnostic(stage: &'static str, error: impl std::fmt::Display) -> ExecutionDiagnostic {
        ExecutionDiagnostic::new(DiagnosticKind::Backend, error.to_string())
            .context("backend", "notification")
            .context("stage", stage)
            .context("aumid", DESKTOP_AUMID)
    }

    /// Establishes the unpackaged application's stable Start Menu identity.
    /// The shortcut is overwritten, which both repairs stale targets and makes
    /// the operation naturally idempotent.
    fn register() -> ExecResult {
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }
            .ok()
            .map_err(|e| diagnostic("COM initialization", e))?;
        let executable = std::env::current_exe().map_err(|e| diagnostic("executable path", e))?;
        let programs = dirs_next::data_dir()
            .ok_or_else(|| {
                diagnostic("Start Menu path", "roaming application data is unavailable")
            })?
            .join("Microsoft/Windows/Start Menu/Programs");
        std::fs::create_dir_all(&programs).map_err(|e| diagnostic("Start Menu path", e))?;
        let shortcut = programs.join("Multi Launcher.lnk");

        let link: IShellLinkW = unsafe { CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER) }
            .map_err(|e| diagnostic("shortcut creation", e))?;
        let target: Vec<u16> = executable
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        unsafe { link.SetPath(PCWSTR(target.as_ptr())) }
            .map_err(|e| diagnostic("shortcut target", e))?;

        // Set the documented PKEY_AppUserModel_ID through IPropertyStore.
        use ::windows::{
            Win32::UI::Shell::PropertiesSystem::{IPropertyStore, PROPERTYKEY},
            core::{GUID, PROPVARIANT},
        };
        const PKEY_APP_USER_MODEL_ID: PROPERTYKEY = PROPERTYKEY {
            fmtid: GUID::from_u128(0x9f4c2855_9f79_4b39_a8d0_e1d42de1d5f3),
            pid: 5,
        };
        let store: IPropertyStore = link.cast().map_err(|e| diagnostic("property store", e))?;
        let value = PROPVARIANT::from(DESKTOP_AUMID);
        unsafe { store.SetValue(&PKEY_APP_USER_MODEL_ID, &value) }
            .map_err(|e| diagnostic("AUMID property", e))?;
        unsafe { store.Commit() }.map_err(|e| diagnostic("property assignment", e))?;

        let persist: IPersistFile = link
            .cast()
            .map_err(|e| diagnostic("shortcut persistence", e))?;
        let path: Vec<u16> = shortcut.as_os_str().encode_wide().chain(Some(0)).collect();
        unsafe { persist.Save(PCWSTR(path.as_ptr()), true) }
            .map_err(|e| diagnostic("shortcut save", e))
    }

    static REGISTRATION: OnceLock<ExecResult> = OnceLock::new();
    pub fn initialize_desktop_notifications() -> ExecResult {
        REGISTRATION.get_or_init(register).clone()
    }

    pub struct WindowsNotificationBackend {
        initialization: ExecResult,
    }
    impl WindowsNotificationBackend {
        pub fn new() -> Self {
            Self {
                initialization: initialize_desktop_notifications(),
            }
        }
    }
    impl NotificationBackend for WindowsNotificationBackend {
        fn notify(&self, notification: &ResolvedNotification) -> ExecResult {
            self.initialization.clone()?;
            ::windows::core::initialize_sta().map_err(|e| diagnostic("WinRT initialization", e))?;
            let document = XmlDocument::new().map_err(|e| diagnostic("XML creation", e))?;
            document
                .LoadXml(&HSTRING::from(toast_xml(notification)))
                .map_err(|e| diagnostic("XML creation", e))?;
            let toast = ToastNotification::CreateToastNotification(&document)
                .map_err(|e| diagnostic("toast creation", e))?;
            let notifier =
                ToastNotificationManager::CreateToastNotifierWithId(&HSTRING::from(DESKTOP_AUMID))
                    .map_err(|e| diagnostic("notifier creation", e))?;
            notifier
                .Show(&toast)
                .map_err(|e| diagnostic("toast submission", e))
        }
    }
}

#[cfg(windows)]
pub use platform::{WindowsNotificationBackend, initialize_desktop_notifications};

#[cfg(not(windows))]
pub fn initialize_desktop_notifications() -> ExecResult {
    Err(ExecutionDiagnostic::new(
        DiagnosticKind::UnsupportedOperation,
        "native notifications are only available on Windows",
    )
    .context("backend", "notification")
    .context("platform", std::env::consts::OS))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::MkNotificationKind;

    fn notification(kind: MkNotificationKind) -> ResolvedNotification {
        ResolvedNotification {
            title: "Title".into(),
            description: "Body".into(),
            kind,
            duration: MkNotificationDuration::Short,
            show_symbol: true,
        }
    }

    #[test]
    fn symbol_prefixes_and_opt_out() {
        for (kind, symbol) in [
            (MkNotificationKind::Information, "ℹ"),
            (MkNotificationKind::Success, "✓"),
            (MkNotificationKind::Warning, "⚠"),
            (MkNotificationKind::Error, "✕"),
        ] {
            assert!(
                toast_xml(&notification(kind)).contains(&format!("<text>{symbol} Title</text>"))
            );
        }
        let mut n = notification(MkNotificationKind::Error);
        n.show_symbol = false;
        assert!(toast_xml(&n).contains("<text>Title</text>"));
    }

    #[test]
    fn xml_is_escaped_unicode_silent_and_has_both_durations() {
        let mut n = notification(MkNotificationKind::Information);
        n.title = "<&quot; ' 雪".into();
        n.description = "A & B < C".into();
        let short = toast_xml(&n);
        assert!(short.contains("duration=\"short\""));
        assert!(short.contains("silent=\"true\""));
        assert!(short.contains("&lt;&amp;quot; &apos; 雪"));
        assert!(short.contains("A &amp; B &lt; C"));
        n.duration = MkNotificationDuration::Long;
        assert!(toast_xml(&n).contains("duration=\"long\""));
    }

    #[cfg(not(windows))]
    #[test]
    fn unsupported_platform_is_explicit() {
        let error = initialize_desktop_notifications().unwrap_err();
        assert_eq!(error.kind, DiagnosticKind::UnsupportedOperation);
        assert_eq!(
            error.context.get("backend").map(String::as_str),
            Some("notification")
        );
    }
}
