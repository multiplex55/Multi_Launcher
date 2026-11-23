use super::Settings;
use crate::common::persistence::{LoadState, PersistenceError, load_json, save_json_atomic};
use anyhow::Context;
use once_cell::sync::Lazy;
use std::collections::BTreeMap;
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryAutocompleteSaveRequest {
    expected_baseline: bool,
    desired: bool,
}

impl QueryAutocompleteSaveRequest {
    pub fn new(expected_baseline: bool, desired: bool) -> Self {
        Self {
            expected_baseline,
            desired,
        }
    }

    pub fn expected_baseline(self) -> bool {
        self.expected_baseline
    }

    pub fn desired(self) -> bool {
        self.desired
    }
}

const SAVE_PENDING: u8 = 0;
const SAVE_WAITING_FOR_SETTINGS: u8 = 1;
const SAVE_CANCELED: u8 = 2;
const SAVE_COMMITTING: u8 = 3;

/// A registry-owned cancellation fence for one query-autocomplete write.
/// Cancellation wins only until the worker claims its atomic commit boundary.
#[derive(Clone, Debug)]
pub struct QueryAutocompleteSaveToken {
    id: u64,
    state: Arc<AtomicU8>,
}

impl QueryAutocompleteSaveToken {
    pub(crate) fn new(id: u64) -> Self {
        Self {
            id,
            state: Arc::new(AtomicU8::new(SAVE_PENDING)),
        }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    /// Returns true when cancellation is accepted or was already accepted.
    /// False means the worker has crossed the commit boundary.
    pub fn request_cancel(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING | SAVE_WAITING_FOR_SETTINGS => {
                    let state = self.state.load(Ordering::Acquire);
                    if state == SAVE_PENDING || state == SAVE_WAITING_FOR_SETTINGS {
                        if self
                            .state
                            .compare_exchange(
                                state,
                                SAVE_CANCELED,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            return true;
                        }
                    }
                }
                SAVE_CANCELED => return true,
                SAVE_COMMITTING => return false,
                _ => return false,
            }
        }
    }

    fn is_canceled(&self) -> bool {
        self.state.load(Ordering::Acquire) == SAVE_CANCELED
    }

    fn mark_waiting_for_settings(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING => {
                    if self
                        .state
                        .compare_exchange(
                            SAVE_PENDING,
                            SAVE_WAITING_FOR_SETTINGS,
                            Ordering::AcqRel,
                            Ordering::Acquire,
                        )
                        .is_ok()
                    {
                        return true;
                    }
                }
                SAVE_WAITING_FOR_SETTINGS => return true,
                SAVE_CANCELED | SAVE_COMMITTING => return false,
                _ => return false,
            }
        }
    }

    fn begin_irrevocable_commit(&self) -> bool {
        loop {
            match self.state.load(Ordering::Acquire) {
                SAVE_PENDING | SAVE_WAITING_FOR_SETTINGS => {
                    let state = self.state.load(Ordering::Acquire);
                    if state == SAVE_PENDING || state == SAVE_WAITING_FOR_SETTINGS {
                        if self
                            .state
                            .compare_exchange(
                                state,
                                SAVE_COMMITTING,
                                Ordering::AcqRel,
                                Ordering::Acquire,
                            )
                            .is_ok()
                        {
                            return true;
                        }
                    }
                }
                SAVE_CANCELED => return false,
                SAVE_COMMITTING => return true,
                _ => return false,
            }
        }
    }

    #[cfg(test)]
    fn is_waiting_for_settings(&self) -> bool {
        self.state.load(Ordering::Acquire) == SAVE_WAITING_FOR_SETTINGS
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueryAutocompleteSaveAcknowledgement {
    value: bool,
    persisted: bool,
}

impl QueryAutocompleteSaveAcknowledgement {
    pub fn value(self) -> bool {
        self.value
    }

    pub fn persisted(self) -> bool {
        self.persisted
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum QueryAutocompleteSaveError {
    Cancelled,
    Conflict { expected: bool, actual: bool },
    Store(String),
}

impl std::fmt::Display for QueryAutocompleteSaveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => formatter.write_str("query autocomplete save was canceled"),
            Self::Conflict { expected, actual } => write!(
                formatter,
                "query autocomplete changed from the editor baseline ({expected}) to {actual}"
            ),
            Self::Store(message) => formatter.write_str(message),
        }
    }
}

impl std::error::Error for QueryAutocompleteSaveError {}

static SETTINGS_TRANSACTION: Lazy<Mutex<()>> = Lazy::new(|| Mutex::new(()));

impl Settings {
    /// Load settings while preserving the generic missing/empty distinction.
    pub fn load_typed(path: impl AsRef<Path>) -> Result<LoadState<Self>, PersistenceError> {
        load_json(path)
    }

    /// Compatibility loader: missing and empty settings use defaults, while an
    /// existing malformed or unreadable file remains an error.
    pub fn load(path: &str) -> anyhow::Result<Self> {
        load_effective(path)
    }

    /// Atomically save settings under the settings-owned transaction lock.
    pub fn save(&self, path: &str) -> anyhow::Result<()> {
        self.save_typed(path).map_err(Into::into)
    }

    pub(crate) fn save_typed(&self, path: impl AsRef<Path>) -> Result<(), PersistenceError> {
        let _transaction = transaction_guard();
        save_json_atomic(path, self)
    }

    /// Serialize a complete settings read-modify-write transaction.
    ///
    /// The committed value is returned only after its atomic write succeeds, so
    /// callers can safely publish or apply that returned value.
    pub fn update(
        path: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<Self> {
        let _transaction = transaction_guard();
        let mut settings = load_effective(path)?;
        mutate(&mut settings)?;
        save_json_atomic(path, &settings)?;
        Ok(settings)
    }

    /// Apply a settings transaction and skip the disk write when its persisted
    /// representation is unchanged. The comparison and optional commit share
    /// the same transaction lock as every other settings update.
    pub(crate) fn update_if_changed<T>(
        path: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<T>,
    ) -> anyhow::Result<(bool, T)> {
        let _transaction = transaction_guard();
        let mut settings = load_effective(path)?;
        let before = serde_json::to_value(&settings)
            .context("failed to serialize settings before update")?;
        let result = mutate(&mut settings)?;
        let after =
            serde_json::to_value(&settings).context("failed to serialize settings after update")?;
        if before == after {
            return Ok((false, result));
        }
        save_json_atomic(path, &settings)?;
        Ok((true, result))
    }

    /// Read the latest settings while serialized with settings updates.
    pub fn read_current(path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let _transaction = transaction_guard();
        load_effective(path)
    }

    /// Acknowledge one GPUI-owned Search & Results preference without
    /// reserializing any of the other Settings values. Both the typed
    /// baseline check and raw JSON patch share the canonical Settings lock.
    pub fn save_query_autocomplete_checked(
        path: impl AsRef<Path>,
        request: QueryAutocompleteSaveRequest,
    ) -> Result<QueryAutocompleteSaveAcknowledgement, QueryAutocompleteSaveError> {
        Self::save_query_autocomplete_checked_inner(path.as_ref(), request, None)
    }

    pub(crate) fn save_query_autocomplete_checked_cancellable(
        path: impl AsRef<Path>,
        request: QueryAutocompleteSaveRequest,
        token: &QueryAutocompleteSaveToken,
    ) -> Result<QueryAutocompleteSaveAcknowledgement, QueryAutocompleteSaveError> {
        if !token.mark_waiting_for_settings() {
            return Err(QueryAutocompleteSaveError::Cancelled);
        }
        Self::save_query_autocomplete_checked_inner(path.as_ref(), request, Some(token))
    }

    fn save_query_autocomplete_checked_inner(
        path: &Path,
        request: QueryAutocompleteSaveRequest,
        token: Option<&QueryAutocompleteSaveToken>,
    ) -> Result<QueryAutocompleteSaveAcknowledgement, QueryAutocompleteSaveError> {
        let _transaction = transaction_guard();
        if token.is_some_and(QueryAutocompleteSaveToken::is_canceled) {
            return Err(QueryAutocompleteSaveError::Cancelled);
        }
        let source_bytes = read_exact_bytes(path)
            .map_err(|error| QueryAutocompleteSaveError::Store(error.to_string()))?;
        let source_settings = parse_effective_settings(path, &source_bytes)?;
        let actual = source_settings.query_autocomplete;

        // A second writer may already have committed the requested value.
        // That state is an acknowledgement, and does not require a rewrite.
        if actual == request.desired {
            if token.is_some_and(QueryAutocompleteSaveToken::is_canceled) {
                return Err(QueryAutocompleteSaveError::Cancelled);
            }
            return Ok(QueryAutocompleteSaveAcknowledgement {
                value: actual,
                persisted: false,
            });
        }
        if actual != request.expected_baseline {
            return Err(QueryAutocompleteSaveError::Conflict {
                expected: request.expected_baseline,
                actual,
            });
        }

        let raw_source = if source_bytes.iter().all(u8::is_ascii_whitespace) {
            // A missing or empty store starts with the full effective defaults.
            // A sparse JSON object can have different deserialization defaults.
            serde_json::to_vec(&source_settings).map_err(|error| {
                QueryAutocompleteSaveError::Store(format!(
                    "failed to serialize default Settings for {}: {error}",
                    path.display()
                ))
            })?
        } else {
            source_bytes
        };
        let mut raw_settings =
            serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
                &raw_source,
            )
            .map_err(|error| {
                QueryAutocompleteSaveError::Store(format!(
                    "failed to preserve raw Settings values from {}: {error}",
                    path.display()
                ))
            })?;
        let raw_value = serde_json::value::RawValue::from_string(request.desired.to_string())
            .map_err(|error| QueryAutocompleteSaveError::Store(error.to_string()))?;
        raw_settings.insert("query_autocomplete".to_owned(), raw_value);
        if token.is_some_and(|token| !token.begin_irrevocable_commit()) {
            return Err(QueryAutocompleteSaveError::Cancelled);
        }
        save_json_atomic(path, &raw_settings)
            .map_err(|error| QueryAutocompleteSaveError::Store(error.to_string()))?;

        // Read the committed typed value back before returning an
        // acknowledgement to the editor.
        let committed_bytes = std::fs::read(path).map_err(|error| {
            QueryAutocompleteSaveError::Store(format!(
                "failed to verify Settings write at {}: {error}",
                path.display()
            ))
        })?;
        let committed = parse_effective_settings(path, &committed_bytes)?;
        if committed.query_autocomplete != request.desired {
            return Err(QueryAutocompleteSaveError::Store(
                "query autocomplete did not match the requested Settings value after writing"
                    .to_owned(),
            ));
        }
        Ok(QueryAutocompleteSaveAcknowledgement {
            value: committed.query_autocomplete,
            persisted: true,
        })
    }

    /// Update settings only when the raw bytes still match the caller's
    /// inspected SHA-256. The exact serialized bytes are atomically committed
    /// and read back before their digest is returned.
    pub fn update_checked(
        path: impl AsRef<Path>,
        expected_sha256: &str,
        mutate: impl FnOnce(&mut Self) -> anyhow::Result<()>,
    ) -> anyhow::Result<(Self, String)> {
        let _transaction = transaction_guard();
        let path = path.as_ref();
        let source_bytes = read_exact_bytes(path)?;
        let source_sha256 = crate::radial::package::sha256_hex(&source_bytes);
        anyhow::ensure!(
            source_sha256 == expected_sha256,
            "settings changed since the checked update was prepared"
        );
        let mut settings = if source_bytes.iter().all(u8::is_ascii_whitespace) {
            Self::default()
        } else {
            serde_json::from_slice(&source_bytes)
                .map_err(|error| anyhow::anyhow!("failed to parse settings: {error}"))?
        };
        mutate(&mut settings)?;
        let bytes = serde_json::to_vec_pretty(&settings)
            .map_err(|error| anyhow::anyhow!("failed to serialize settings: {error}"))?;
        crate::common::atomic_file::save_atomic(path, &bytes)
            .map_err(|error| anyhow::anyhow!("failed to atomically save settings: {error}"))?;
        let saved = std::fs::read(path)
            .map_err(|error| anyhow::anyhow!("failed to verify saved settings: {error}"))?;
        let saved_sha256 = crate::radial::package::sha256_hex(&saved);
        anyhow::ensure!(
            saved == bytes,
            "settings changed while verifying the checked update"
        );
        Ok((settings, saved_sha256))
    }
}

fn parse_effective_settings(
    path: &Path,
    bytes: &[u8],
) -> Result<Settings, QueryAutocompleteSaveError> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Ok(Settings::default());
    }
    serde_json::from_slice(bytes).map_err(|error| {
        QueryAutocompleteSaveError::Store(format!(
            "failed to parse Settings from {}: {error}",
            path.display()
        ))
    })
}

/// SHA-256 of the exact persisted settings bytes. Missing files hash as empty
/// bytes, matching the radial authoring store's empty-source convention.
pub fn settings_file_sha256(path: impl AsRef<Path>) -> anyhow::Result<String> {
    Ok(crate::radial::package::sha256_hex(&read_exact_bytes(
        path.as_ref(),
    )?))
}

fn read_exact_bytes(path: &Path) -> anyhow::Result<Vec<u8>> {
    match std::fs::read(path) {
        Ok(bytes) => Ok(bytes),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(error) => Err(anyhow::Error::new(error).context(format!(
            "failed to read settings bytes from {}",
            path.display()
        ))),
    }
}

fn transaction_guard() -> MutexGuard<'static, ()> {
    SETTINGS_TRANSACTION
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn load_effective(path: impl AsRef<Path>) -> anyhow::Result<Settings> {
    match Settings::load_typed(path)? {
        LoadState::Missing | LoadState::Empty => Ok(Settings::default()),
        LoadState::Loaded(settings) => Ok(settings),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        QueryAutocompleteSaveAcknowledgement, QueryAutocompleteSaveError,
        QueryAutocompleteSaveRequest, QueryAutocompleteSaveToken, Settings,
        settings_file_sha256, transaction_guard,
    };
    use crate::common::persistence::{LoadState, PersistenceError};
    use std::collections::BTreeMap;
    use std::sync::{Arc, Barrier};
    use std::time::{Duration, Instant};

    #[test]
    fn compatibility_load_defaults_only_missing_and_empty() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert_eq!(
            Settings::load(missing.to_str().unwrap()).unwrap().hotkey,
            Settings::default().hotkey
        );

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, " \r\n\t").unwrap();
        assert_eq!(
            Settings::load(empty.to_str().unwrap()).unwrap().hotkey,
            Settings::default().hotkey
        );

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "{broken").unwrap();
        assert!(Settings::load(malformed.to_str().unwrap()).is_err());
    }

    #[test]
    fn typed_load_preserves_missing_empty_valid_and_malformed() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        assert!(matches!(
            Settings::load_typed(&missing).unwrap(),
            LoadState::Missing
        ));

        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, "\n\t").unwrap();
        assert!(matches!(
            Settings::load_typed(&empty).unwrap(),
            LoadState::Empty
        ));

        let valid = directory.path().join("valid.json");
        Settings::default().save(valid.to_str().unwrap()).unwrap();
        assert!(matches!(
            Settings::load_typed(&valid).unwrap(),
            LoadState::Loaded(_)
        ));

        let malformed = directory.path().join("malformed.json");
        std::fs::write(&malformed, "not json").unwrap();
        assert!(matches!(
            Settings::load_typed(&malformed).unwrap_err(),
            PersistenceError::MalformedJson { .. }
        ));
    }

    #[test]
    fn unreadable_existing_path_is_not_missing() {
        let directory = tempfile::tempdir().unwrap();
        assert!(matches!(
            Settings::load_typed(directory.path()).unwrap_err(),
            PersistenceError::Read { .. }
        ));
    }

    #[test]
    fn save_preserves_the_existing_pretty_json_contract() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let mut settings = Settings::default();
        settings.show_examples = true;
        let expected = serde_json::to_string_pretty(&settings).unwrap();

        settings.save(path.to_str().unwrap()).unwrap();

        assert_eq!(std::fs::read_to_string(path).unwrap(), expected);
    }

    #[test]
    fn concurrent_updates_preserve_both_logical_changes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let path = Arc::new(path.to_string_lossy().into_owned());
        let barrier = Arc::new(Barrier::new(3));

        let first = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Settings::update(&path, |settings| {
                    settings.show_examples = true;
                    Ok(())
                })
                .unwrap();
            })
        };
        let second = {
            let path = Arc::clone(&path);
            let barrier = Arc::clone(&barrier);
            std::thread::spawn(move || {
                barrier.wait();
                Settings::update(&path, |settings| {
                    settings.enable_toasts = false;
                    Ok(())
                })
                .unwrap();
            })
        };

        barrier.wait();
        first.join().unwrap();
        second.join().unwrap();
        let committed = Settings::load(&path).unwrap();
        assert!(committed.show_examples);
        assert!(!committed.enable_toasts);
    }

    #[test]
    fn failed_update_returns_no_value_to_publish() {
        let directory = tempfile::tempdir().unwrap();
        let blocker = directory.path().join("not-a-directory");
        std::fs::write(&blocker, "block parent creation").unwrap();
        let path = blocker.join("settings.json");

        let result = Settings::update(path.to_str().unwrap(), |settings| {
            settings.show_examples = true;
            Ok(())
        });

        assert!(result.is_err());
        assert_eq!(
            std::fs::read_to_string(blocker).unwrap(),
            "block parent creation"
        );
    }

    #[test]
    fn checked_update_requires_exact_source_sha_and_returns_verified_target_sha() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        Settings::default().save(path.to_str().unwrap()).unwrap();
        let source_sha = settings_file_sha256(&path).unwrap();

        let (updated, target_sha) = Settings::update_checked(&path, &source_sha, |settings| {
            settings.show_examples = true;
            Ok(())
        })
        .unwrap();

        assert!(updated.show_examples);
        assert_eq!(settings_file_sha256(&path).unwrap(), target_sha);
        let before_conflict = std::fs::read(&path).unwrap();
        assert!(
            Settings::update_checked(&path, &source_sha, |settings| {
                settings.enable_toasts = false;
                Ok(())
            })
            .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), before_conflict);
    }

    #[test]
    fn checked_update_mutator_and_parse_failures_write_nothing() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        std::fs::write(&path, b"{broken").unwrap();
        let source = std::fs::read(&path).unwrap();
        let source_sha = crate::radial::package::sha256_hex(&source);
        assert!(Settings::update_checked(&path, &source_sha, |_| Ok(())).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), source);

        std::fs::write(&path, b"{}").unwrap();
        let source = std::fs::read(&path).unwrap();
        let source_sha = crate::radial::package::sha256_hex(&source);
        assert!(
            Settings::update_checked(&path, &source_sha, |_| { anyhow::bail!("no mutation") })
                .is_err()
        );
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_write_preserves_raw_unowned_json_values() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let opaque =
            r#"{ "signed": -9223372036854775809123, "nested": [9007199254740993123456789] }"#;
        let source = format!("{{\"query_autocomplete\":true,\"opaque_extension\":{opaque}}}");
        std::fs::write(&path, source).unwrap();

        let acknowledgement = Settings::save_query_autocomplete_checked(
            &path,
            QueryAutocompleteSaveRequest::new(true, false),
        )
        .unwrap();

        assert_eq!(
            acknowledgement,
            QueryAutocompleteSaveAcknowledgement {
                value: false,
                persisted: true,
            }
        );
        let bytes = std::fs::read(&path).unwrap();
        let raw = serde_json::from_slice::<BTreeMap<String, Box<serde_json::value::RawValue>>>(
            &bytes,
        )
        .unwrap();
        assert_eq!(raw["opaque_extension"].get(), opaque);
        assert!(!Settings::read_current(&path).unwrap().query_autocomplete);
    }

    #[test]
    fn query_autocomplete_acknowledges_desired_value_without_rewriting() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{ "query_autocomplete": false, "opaque": 1234567890123456789012345 }"#;
        std::fs::write(&path, source).unwrap();

        let acknowledgement = Settings::save_query_autocomplete_checked(
            &path,
            QueryAutocompleteSaveRequest::new(true, false),
        )
        .unwrap();

        assert_eq!(
            acknowledgement,
            QueryAutocompleteSaveAcknowledgement {
                value: false,
                persisted: false,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_write_uses_existing_missing_and_empty_defaults() {
        let directory = tempfile::tempdir().unwrap();
        let missing = directory.path().join("missing.json");
        let empty = directory.path().join("empty.json");
        std::fs::write(&empty, b" \r\n\t").unwrap();
        let mut expected = Settings::default();
        let baseline = expected.query_autocomplete;
        let desired = !baseline;
        expected.query_autocomplete = desired;

        for path in [&missing, &empty] {
            let acknowledgement = Settings::save_query_autocomplete_checked(
                path,
                QueryAutocompleteSaveRequest::new(baseline, desired),
            )
            .unwrap();
            assert_eq!(acknowledgement.value(), desired);
            assert!(acknowledgement.persisted());
            let reloaded = Settings::read_current(path).unwrap();
            assert_eq!(reloaded.query_autocomplete, desired);
            assert_eq!(reloaded.help_hotkey, expected.help_hotkey);
            assert_eq!(reloaded.offscreen_pos, expected.offscreen_pos);
        }

        let sparse = directory.path().join("sparse.json");
        std::fs::write(&sparse, b"{}").unwrap();
        Settings::save_query_autocomplete_checked(
            &sparse,
            QueryAutocompleteSaveRequest::new(baseline, desired),
        )
        .unwrap();
        let stored: serde_json::Value =
            serde_json::from_slice(&std::fs::read(sparse).unwrap()).unwrap();
        assert_eq!(stored, serde_json::json!({ "query_autocomplete": desired }));
    }

    #[test]
    fn pending_query_autocomplete_save_cancels_while_waiting_for_settings_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = br#"{ "query_autocomplete": true, "opaque": 1234567890123456789012345 }"#;
        std::fs::write(&path, source).unwrap();
        let settings_lock = transaction_guard();
        let workers = crate::SettingsSaveWorkerRegistry::default();
        let (receipt_rx, token) = workers
            .begin_query_autocomplete_save(
                path.to_string_lossy().into_owned(),
                QueryAutocompleteSaveRequest::new(true, false),
            )
            .unwrap();

        let deadline = Instant::now() + Duration::from_secs(5);
        while !token.is_waiting_for_settings() {
            assert!(Instant::now() < deadline, "writer did not wait on Settings lock");
            std::thread::yield_now();
        }
        assert!(token.request_cancel());
        drop(settings_lock);

        assert_eq!(
            receipt_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err(QueryAutocompleteSaveError::Cancelled)
        );
        let shutdown = workers.join_all_checked();
        assert!(shutdown.is_clean(), "{:?}", shutdown.failure_summary());
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_save_token_rejects_cancel_after_commit_claim() {
        let token = QueryAutocompleteSaveToken::new(1);
        assert!(token.begin_irrevocable_commit());
        assert!(!token.request_cancel());
    }

    #[test]
    fn checked_shutdown_reports_unacknowledged_query_autocomplete_write_failure() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let source = b"{broken";
        std::fs::write(&path, source).unwrap();
        let workers = crate::SettingsSaveWorkerRegistry::default();
        let (receipt_rx, _token) = workers
            .begin_query_autocomplete_save(
                path.to_string_lossy().into_owned(),
                QueryAutocompleteSaveRequest::new(true, false),
            )
            .unwrap();
        assert!(matches!(
            receipt_rx.recv_timeout(Duration::from_secs(5)).unwrap(),
            Err(QueryAutocompleteSaveError::Store(_))
        ));

        let shutdown = workers.join_all_checked();
        let failure = shutdown.failure_summary().unwrap();
        assert!(failure.contains("query autocomplete save request 1"));
        assert!(failure.contains("failed to parse Settings"));
        assert_eq!(std::fs::read(&path).unwrap(), source);
    }

    #[test]
    fn query_autocomplete_conflict_and_malformed_input_preserve_source_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("settings.json");
        let conflicting = br#"{ "query_autocomplete": false, "opaque": 123 }"#;
        std::fs::write(&path, conflicting).unwrap();
        assert_eq!(
            Settings::save_query_autocomplete_checked(
                &path,
                QueryAutocompleteSaveRequest::new(true, true),
            )
            .unwrap_err(),
            QueryAutocompleteSaveError::Conflict {
                expected: true,
                actual: false,
            }
        );
        assert_eq!(std::fs::read(&path).unwrap(), conflicting);

        let malformed = b"{broken";
        std::fs::write(&path, malformed).unwrap();
        assert!(matches!(
            Settings::save_query_autocomplete_checked(
                &path,
                QueryAutocompleteSaveRequest::new(true, false),
            ),
            Err(QueryAutocompleteSaveError::Store(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), malformed);
    }
}
