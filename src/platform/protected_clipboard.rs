//! Protected clipboard boundary. No value is logged or included in errors.
use crate::password_generator::{SecretResult, SecretResultId};
use std::{fmt, time::Duration};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CopyStatus {
    AcceptedAndPersisted,
    AcceptedButNotPersisted,
}
#[derive(PartialEq, Eq)]
pub enum ClipboardRead {
    Ordinary(String),
    Private,
    NonText,
}
impl fmt::Debug for ClipboardRead {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Ordinary(_) => f.write_str("Ordinary([redacted])"),
            Self::Private => f.write_str("Private"),
            Self::NonText => f.write_str("NonText"),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClipboardError {
    Busy,
    NotAccepted,
    Unsupported,
    Unavailable,
    TimedOut,
}
impl ClipboardError {
    pub fn is_busy(self) -> bool {
        matches!(self, Self::Busy)
    }
}
impl fmt::Display for ClipboardError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Busy => "The clipboard is busy.",
            Self::NotAccepted => "Windows could not set the clipboard.",
            Self::Unsupported => "Protected clipboard copying is unavailable on this platform.",
            Self::Unavailable => "The clipboard could not be accessed.",
            Self::TimedOut => "The clipboard did not respond in time.",
        })
    }
}
impl std::error::Error for ClipboardError {}

/// A rejected headless copy retains its generated result for an explicit retry.
pub struct RejectedSecretCopy {
    error: ClipboardError,
    result: SecretResult,
}

impl RejectedSecretCopy {
    pub(crate) fn new(error: ClipboardError, result: SecretResult) -> Self {
        Self { error, result }
    }

    pub fn result_id(&self) -> SecretResultId {
        self.result.id()
    }

    pub fn retry(self) -> Result<CopyStatus, Self> {
        copy_secret(&self.result).map_err(|error| Self {
            error,
            result: self.result,
        })
    }
}

impl fmt::Debug for RejectedSecretCopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("RejectedSecretCopy")
            .field("error", &self.error)
            .finish_non_exhaustive()
    }
}

impl fmt::Display for RejectedSecretCopy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.error, f)
    }
}

impl std::error::Error for RejectedSecretCopy {}

/// Copy only a generator-owned result. A successful return means the OS
/// accepted the package; persistence failure is a separate accepted status.
pub fn copy_secret(secret: &SecretResult) -> Result<CopyStatus, ClipboardError> {
    platform::copy(secret)
}
/// Read one clipboard snapshot. Call from a worker when possible: cross-process
/// clipboard providers may take time even though the async wait is bounded.
pub fn read_clipboard() -> Result<ClipboardRead, ClipboardError> {
    platform::read(Duration::from_millis(750))
}

/// Give interactive search and command capture a short wait for delayed text.
pub(crate) fn read_clipboard_briefly() -> Result<ClipboardRead, ClipboardError> {
    platform::read(Duration::from_millis(32))
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::time::Instant;
    use windows::{
        ApplicationModel::DataTransfer::{
            Clipboard, ClipboardContentOptions, DataPackage, DataPackageView, StandardDataFormats,
        },
        Foundation::{AsyncStatus, IAsyncOperation},
        Storage::Streams::InMemoryRandomAccessStream,
        Win32::{
            Foundation::RPC_E_CHANGED_MODE,
            System::WinRT::{
                RO_INIT_MULTITHREADED, RO_INIT_SINGLETHREADED, RoInitialize, RoUninitialize,
            },
        },
        core::{HSTRING, IInspectable, Interface},
    };
    const PRIVATE_FORMAT: &str = "application/x-multi-launcher-private-text-v1";
    fn map_error(error: windows::core::Error) -> ClipboardError {
        match error.code().0 as u32 {
            0x800401d0 | 0x80010001 | 0x800700aa => ClipboardError::Busy,
            _ => ClipboardError::Unavailable,
        }
    }
    struct Apartment;
    impl Apartment {
        fn enter() -> Result<Self, ClipboardError> {
            match unsafe { RoInitialize(RO_INIT_MULTITHREADED) } {
                Ok(()) => Ok(Self),
                Err(error) if error.code() == RPC_E_CHANGED_MODE => {
                    unsafe { RoInitialize(RO_INIT_SINGLETHREADED) }.map_err(map_error)?;
                    Ok(Self)
                }
                Err(error) => Err(map_error(error)),
            }
        }
    }
    impl Drop for Apartment {
        fn drop(&mut self) {
            unsafe {
                RoUninitialize();
            }
        }
    }
    fn package(text: &str) -> Result<(DataPackage, ClipboardContentOptions), ClipboardError> {
        let package = DataPackage::new().map_err(map_error)?;
        package.SetText(&HSTRING::from(text)).map_err(map_error)?;
        // A one-byte stream is a WinRT-marshallable custom-format payload.
        let marker = InMemoryRandomAccessStream::new().map_err(map_error)?;
        marker.SetSize(1).map_err(map_error)?;
        marker.Seek(0).map_err(map_error)?;
        let marker: IInspectable = marker.cast().map_err(map_error)?;
        package
            .SetData(&HSTRING::from(PRIVATE_FORMAT), &marker)
            .map_err(map_error)?;
        let options = ClipboardContentOptions::new().map_err(map_error)?;
        options.SetIsAllowedInHistory(false).map_err(map_error)?;
        options.SetIsRoamable(false).map_err(map_error)?;
        Ok((package, options))
    }
    fn publish(
        set: impl FnOnce() -> Result<bool, ClipboardError>,
        flush: impl FnOnce() -> Result<(), ClipboardError>,
    ) -> Result<CopyStatus, ClipboardError> {
        if !set()? {
            return Err(ClipboardError::NotAccepted);
        }
        Ok(if flush().is_ok() {
            CopyStatus::AcceptedAndPersisted
        } else {
            CopyStatus::AcceptedButNotPersisted
        })
    }
    pub(super) fn copy(secret: &SecretResult) -> Result<CopyStatus, ClipboardError> {
        let _apartment = Apartment::enter()?;
        let (content, options) = package(secret.expose())?;
        publish(
            || Clipboard::SetContentWithOptions(&content, &options).map_err(map_error),
            || Clipboard::Flush().map_err(map_error),
        )
    }
    trait View {
        fn private(&self) -> Result<bool, ClipboardError>;
        fn has_text(&self) -> Result<bool, ClipboardError>;
        fn text(&self, timeout: Duration) -> Result<String, ClipboardError>;
    }
    impl View for DataPackageView {
        fn private(&self) -> Result<bool, ClipboardError> {
            self.Contains(&HSTRING::from(PRIVATE_FORMAT))
                .map_err(map_error)
        }
        fn has_text(&self) -> Result<bool, ClipboardError> {
            self.Contains(&StandardDataFormats::Text().map_err(map_error)?)
                .map_err(map_error)
        }
        fn text(&self, timeout: Duration) -> Result<String, ClipboardError> {
            wait_text(self.GetTextAsync().map_err(map_error)?, timeout)
        }
    }
    fn classify(view: &impl View, timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        if view.private()? {
            return Ok(ClipboardRead::Private);
        }
        if !view.has_text()? {
            return Ok(ClipboardRead::NonText);
        }
        Ok(ClipboardRead::Ordinary(view.text(timeout)?))
    }
    fn wait_text(
        operation: IAsyncOperation<HSTRING>,
        timeout: Duration,
    ) -> Result<String, ClipboardError> {
        let deadline = Instant::now() + timeout;
        loop {
            match operation.Status().map_err(map_error)? {
                AsyncStatus::Completed => {
                    return operation
                        .GetResults()
                        .map(|s| s.to_string_lossy())
                        .map_err(map_error);
                }
                AsyncStatus::Started if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(5))
                }
                AsyncStatus::Started => {
                    let _ = operation.Cancel();
                    return Err(ClipboardError::TimedOut);
                }
                _ => return Err(ClipboardError::Unavailable),
            }
        }
    }
    pub(super) fn read(timeout: Duration) -> Result<ClipboardRead, ClipboardError> {
        let _apartment = Apartment::enter()?;
        let view = Clipboard::GetContent().map_err(map_error)?;
        classify(&view, timeout)
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        const TEST_WAIT: Duration = Duration::from_millis(750);

        #[test]
        fn protected_clipboard_package_contains_marker_and_disables_history() {
            let _apartment = Apartment::enter().unwrap();
            let (package, options) = package("fixture text").unwrap();
            assert!(!options.IsAllowedInHistory().unwrap());
            assert!(!options.IsRoamable().unwrap());
            let view = package.GetView().unwrap();
            assert!(view.Contains(&HSTRING::from(PRIVATE_FORMAT)).unwrap());
            assert!(
                view.Contains(&StandardDataFormats::Text().unwrap())
                    .unwrap()
            );
            assert_eq!(classify(&view, TEST_WAIT).unwrap(), ClipboardRead::Private);
            assert_eq!(
                wait_text(view.GetTextAsync().unwrap(), TEST_WAIT).unwrap(),
                "fixture text"
            );
            let ordinary = DataPackage::new().unwrap();
            ordinary.SetText(&HSTRING::from("fixture text")).unwrap();
            assert_eq!(
                classify(&ordinary.GetView().unwrap(), TEST_WAIT).unwrap(),
                ClipboardRead::Ordinary("fixture text".into())
            );
        }
        struct Fake {
            private: bool,
            fetched: std::cell::Cell<bool>,
        }
        impl View for Fake {
            fn private(&self) -> Result<bool, ClipboardError> {
                Ok(self.private)
            }
            fn has_text(&self) -> Result<bool, ClipboardError> {
                Ok(true)
            }
            fn text(&self, _: Duration) -> Result<String, ClipboardError> {
                self.fetched.set(true);
                Ok("fixture".into())
            }
        }
        #[test]
        fn protected_clipboard_private_never_fetches_text_and_refusal_never_flushes() {
            let view = Fake {
                private: true,
                fetched: std::cell::Cell::new(false),
            };
            assert_eq!(classify(&view, TEST_WAIT), Ok(ClipboardRead::Private));
            assert!(!view.fetched.get());
            let flushed = std::cell::Cell::new(false);
            assert_eq!(
                publish(
                    || Ok(false),
                    || {
                        flushed.set(true);
                        Ok(())
                    }
                ),
                Err(ClipboardError::NotAccepted)
            );
            assert!(!flushed.get());
            assert_eq!(
                publish(|| Ok(true), || Err(ClipboardError::Unavailable)),
                Ok(CopyStatus::AcceptedButNotPersisted)
            );
        }
    }
}
#[cfg(not(windows))]
mod platform {
    use super::*;
    pub(super) fn copy(_: &SecretResult) -> Result<CopyStatus, ClipboardError> {
        Err(ClipboardError::Unsupported)
    }
    pub(super) fn read(_: Duration) -> Result<ClipboardRead, ClipboardError> {
        let mut clipboard = arboard::Clipboard::new().map_err(|_| ClipboardError::Unavailable)?;
        match clipboard.get_text() {
            Ok(text) => Ok(ClipboardRead::Ordinary(text)),
            Err(arboard::Error::ContentNotAvailable) => Ok(ClipboardRead::NonText),
            Err(arboard::Error::ClipboardOccupied) => Err(ClipboardError::Busy),
            Err(_) => Err(ClipboardError::Unavailable),
        }
    }
}
