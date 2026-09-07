use crate::platform::app_data::AppDataRoot;
use anyhow::Context;
use siphasher::sip::SipHasher13;
use std::hash::Hasher;
use windows::Win32::Foundation::{CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE};
use windows::Win32::System::Threading::CreateMutexW;
use windows::core::PCWSTR;

const MUTEX_NAMESPACE: &str = "Local\\MultiLauncher.SingleInstance";
const HASH_KEY_0: u64 = 0x4d75_6c74_694c_6e63;
const HASH_KEY_1: u64 = 0x6872_2e44_6174_6152;

#[derive(Debug)]
#[must_use = "an acquired guard must be retained for the application lifetime"]
pub enum SingleInstanceAcquire {
    Acquired(SingleInstanceGuard),
    AlreadyRunning,
}

/// Keeps the process's named mutex object alive until application shutdown.
#[derive(Debug)]
pub struct SingleInstanceGuard {
    handle: HANDLE,
}

impl SingleInstanceGuard {
    pub fn acquire(data_root: &AppDataRoot) -> anyhow::Result<SingleInstanceAcquire> {
        acquire_named(&mutex_name(data_root))
    }
}

impl Drop for SingleInstanceGuard {
    fn drop(&mut self) {
        // SAFETY: `handle` is a valid handle returned by CreateMutexW and is
        // stored in exactly one non-Clone guard. Drop closes it exactly once.
        if let Err(error) = unsafe { CloseHandle(self.handle) } {
            tracing::error!(?error, "failed to close single-instance mutex handle");
        }
    }
}

fn mutex_name(data_root: &AppDataRoot) -> String {
    let mut hasher = SipHasher13::new_with_keys(HASH_KEY_0, HASH_KEY_1);
    hasher.write(data_root.normalized_identity().as_bytes());
    format!("{MUTEX_NAMESPACE}.{:016x}", hasher.finish())
}

fn acquire_named(name: &str) -> anyhow::Result<SingleInstanceAcquire> {
    let wide_name: Vec<u16> = name.encode_utf16().chain(Some(0)).collect();

    // SAFETY: `wide_name` is NUL-terminated and remains alive for the call. No
    // security attributes are supplied, and the returned handle is owned by
    // either the guard or the AlreadyRunning cleanup path below.
    let handle_result = unsafe { CreateMutexW(None, false, PCWSTR(wide_name.as_ptr())) };
    // CreateMutexW communicates a successful pre-existing open via last-error.
    // Capture it before any other Windows call can replace the thread-local value.
    let last_error = unsafe { GetLastError() };
    let handle = handle_result.with_context(|| format!("create named mutex {name}"))?;

    if last_error == ERROR_ALREADY_EXISTS {
        // SAFETY: this successful CreateMutexW call returned a valid handle,
        // which must not remain open when no guard is returned.
        unsafe { CloseHandle(handle) }
            .with_context(|| format!("close duplicate named mutex handle {name}"))?;
        Ok(SingleInstanceAcquire::AlreadyRunning)
    } else {
        Ok(SingleInstanceAcquire::Acquired(SingleInstanceGuard {
            handle,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::{SingleInstanceAcquire, SingleInstanceGuard, acquire_named, mutex_name};
    use crate::platform::app_data::AppDataRoot;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    static NEXT_TEST_ID: AtomicU64 = AtomicU64::new(1);

    fn unique_name(label: &str) -> String {
        format!(
            "Local\\MultiLauncher.SingleInstance.Test.{}.{}.{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed),
            label
        )
    }

    fn unique_root(label: &str) -> AppDataRoot {
        let directory = std::env::temp_dir().join(format!(
            "multi-launcher-single-instance-{}-{}-{label}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        ));
        AppDataRoot::from_settings_path(directory.join("settings.json")).unwrap()
    }

    fn expect_acquired(result: SingleInstanceAcquire) -> SingleInstanceGuard {
        match result {
            SingleInstanceAcquire::Acquired(guard) => guard,
            SingleInstanceAcquire::AlreadyRunning => panic!("expected first acquisition"),
        }
    }

    #[test]
    fn first_named_mutex_acquisition_succeeds() {
        let _guard = expect_acquired(acquire_named(&unique_name("first")).unwrap());
    }

    #[test]
    fn duplicate_named_mutex_reports_already_running() {
        let name = unique_name("duplicate");
        let _guard = expect_acquired(acquire_named(&name).unwrap());
        assert!(matches!(
            acquire_named(&name).unwrap(),
            SingleInstanceAcquire::AlreadyRunning
        ));
    }

    #[test]
    fn duplicate_data_root_reports_already_running() {
        let root = unique_root("duplicate-root");
        let _guard = expect_acquired(SingleInstanceGuard::acquire(&root).unwrap());
        assert!(matches!(
            SingleInstanceGuard::acquire(&root).unwrap(),
            SingleInstanceAcquire::AlreadyRunning
        ));
    }

    #[test]
    fn dropping_guard_allows_reacquisition() {
        let name = unique_name("drop-reacquire");
        drop(expect_acquired(acquire_named(&name).unwrap()));
        let _guard = expect_acquired(acquire_named(&name).unwrap());
    }

    #[test]
    fn different_names_and_roots_do_not_conflict() {
        let first_root = unique_root("first-root");
        let second_root = unique_root("second-root");
        assert_ne!(mutex_name(&first_root), mutex_name(&second_root));

        let _first = expect_acquired(SingleInstanceGuard::acquire(&first_root).unwrap());
        let _second = expect_acquired(SingleInstanceGuard::acquire(&second_root).unwrap());
    }

    #[test]
    fn equivalent_root_spellings_have_stable_mutex_identity() {
        let unique = format!(
            "multi-launcher-single-instance-{}-{}",
            std::process::id(),
            NEXT_TEST_ID.fetch_add(1, Ordering::Relaxed)
        );
        let direct = PathBuf::from(&unique).join("settings.json");
        let dotted = PathBuf::from(".")
            .join(&unique)
            .join("child")
            .join("..")
            .join("settings.json");
        let direct = AppDataRoot::from_settings_path(direct).unwrap();
        let dotted = AppDataRoot::from_settings_path(dotted).unwrap();

        assert_eq!(mutex_name(&direct), mutex_name(&dotted));
    }

    #[test]
    fn filesystem_aliases_share_mutex_identity() {
        let temp = tempfile::tempdir().expect("create temporary directory");
        let real_root = temp.path().join("real-root");
        let alias_root = temp.path().join("alias-root");
        fs::create_dir(&real_root).expect("create real data root");

        if let Err(error) = std::os::windows::fs::symlink_dir(&real_root, &alias_root) {
            // Creating symbolic links can require Windows Developer Mode or
            // elevated privileges. The deterministic AppDataRoot resolver tests
            // still cover canonical identity selection on such hosts.
            eprintln!("skipping filesystem alias assertion: {error}");
            return;
        }

        let real = AppDataRoot::from_settings_path(real_root.join("settings.json"))
            .expect("resolve real root");
        let alias = AppDataRoot::from_settings_path(alias_root.join("settings.json"))
            .expect("resolve alias root");

        assert_eq!(mutex_name(&real), mutex_name(&alias));
        let _guard = expect_acquired(SingleInstanceGuard::acquire(&real).unwrap());
        assert!(matches!(
            SingleInstanceGuard::acquire(&alias).unwrap(),
            SingleInstanceAcquire::AlreadyRunning
        ));
    }
}
