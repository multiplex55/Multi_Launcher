use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const CLEANUP_METADATA_RESERVATION_BYTES: u64 = 8 * 1024;
const MAX_CLEANUP_PATH_BYTES: usize = 4 * 1024;
const MAX_CLEANUP_MESSAGE_CHARS: usize = 256;
static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_FILE_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TableResourcePolicy {
    ram_limit_bytes: u64,
    temp_limit_bytes: u64,
    min_free_bytes: u64,
}

impl TableResourcePolicy {
    pub const DEFAULT_RAM_BYTES: u64 = 256 * MIB;
    pub const DEFAULT_TEMP_BYTES: u64 = 8 * GIB;
    pub const MIN_FREE_BYTES: u64 = GIB;

    pub fn new(ram_limit_bytes: u64, temp_limit_bytes: u64) -> Result<Self, TableResourceError> {
        if !(64 * MIB..=1024 * MIB).contains(&ram_limit_bytes) {
            return Err(TableResourceError::InvalidPolicy(
                "RAM must be between 64 and 1024 MiB.",
            ));
        }
        if !(1 * GIB..=64 * GIB).contains(&temp_limit_bytes) {
            return Err(TableResourceError::InvalidPolicy(
                "Temporary storage must be between 1 and 64 GiB.",
            ));
        }
        Ok(Self {
            ram_limit_bytes,
            temp_limit_bytes,
            min_free_bytes: Self::MIN_FREE_BYTES,
        })
    }

    pub const fn ram_limit_bytes(self) -> u64 {
        self.ram_limit_bytes
    }

    pub const fn temp_limit_bytes(self) -> u64 {
        self.temp_limit_bytes
    }

    pub const fn min_free_bytes(self) -> u64 {
        self.min_free_bytes
    }
}

impl Default for TableResourcePolicy {
    fn default() -> Self {
        Self {
            ram_limit_bytes: Self::DEFAULT_RAM_BYTES,
            temp_limit_bytes: Self::DEFAULT_TEMP_BYTES,
            min_free_bytes: Self::MIN_FREE_BYTES,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct TableResourceUsage {
    pub ram_bytes: u64,
    pub temp_bytes: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum TableResourceError {
    InvalidPolicy(&'static str),
    RamQuota {
        requested: u64,
        used: u64,
        limit: u64,
    },
    TempQuota {
        requested: u64,
        used: u64,
        limit: u64,
    },
    InsufficientFreeSpace {
        available: u64,
        required: u64,
    },
    ArithmeticOverflow,
    PoolMismatch,
    CleanupPending {
        path: PathBuf,
        message: String,
    },
    Io(String),
}

impl std::fmt::Display for TableResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidPolicy(message) => f.write_str(message),
            Self::RamQuota {
                requested,
                used,
                limit,
            } => write!(
                f,
                "Table operation needs {requested} more RAM bytes ({used} already reserved; limit {limit})."
            ),
            Self::TempQuota {
                requested,
                used,
                limit,
            } => write!(
                f,
                "Table operation needs {requested} more temporary bytes ({used} already reserved; limit {limit})."
            ),
            Self::InsufficientFreeSpace {
                available,
                required,
            } => write!(
                f,
                "The temporary volume has {available} free bytes; at least {required} must remain available."
            ),
            Self::ArithmeticOverflow => f.write_str("Table resource accounting overflowed."),
            Self::PoolMismatch => {
                f.write_str("The temporary workspace and source use different resource pools.")
            }
            Self::CleanupPending { path, message } => write!(
                f,
                "Temporary table file cleanup is pending for {}: {message}",
                path.display()
            ),
            Self::Io(message) => write!(f, "Temporary table storage failed: {message}"),
        }
    }
}

impl std::error::Error for TableResourceError {}

struct PoolState {
    policy: TableResourcePolicy,
    usage: TableResourceUsage,
    #[cfg(test)]
    peak_usage: TableResourceUsage,
    pending_temp_cleanup: BTreeMap<PathBuf, PendingTempCleanup>,
}

struct PendingTempCleanup {
    bytes: u64,
    metadata_bytes: u64,
    message: String,
}

struct PoolInner {
    state: Mutex<PoolState>,
}

/// One pool is shared by the session, admitted jobs, accepted sources and
/// cursors. Cloning this handle never resets aggregate usage.
#[derive(Clone)]
pub struct TableResourcePool {
    inner: Arc<PoolInner>,
    /// A job-scoped view preserves the policy in force when that job was
    /// admitted while continuing to charge the shared pool.
    captured_policy: Option<TableResourcePolicy>,
}

impl TableResourcePool {
    pub fn new(policy: TableResourcePolicy) -> Self {
        Self {
            inner: Arc::new(PoolInner {
                state: Mutex::new(PoolState {
                    policy,
                    usage: TableResourceUsage {
                        ram_bytes: 0,
                        temp_bytes: 0,
                    },
                    #[cfg(test)]
                    peak_usage: TableResourceUsage::default(),
                    pending_temp_cleanup: BTreeMap::new(),
                }),
            }),
            captured_policy: None,
        }
    }

    pub fn policy(&self) -> TableResourcePolicy {
        self.captured_policy.unwrap_or_else(|| {
            self.inner
                .state
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .policy
        })
    }

    /// Capture the effective limits for one operation without creating a
    /// second usage pool. Every later reservation through this handle still
    /// charges the same shared usage counters.
    pub fn capture_policy(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            captured_policy: Some(self.policy()),
        }
    }

    /// Return a shared-usage handle whose future admissions observe the latest
    /// policy. Accepted file sources use this when they create new cursors.
    pub(crate) fn current_policy_view(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            captured_policy: None,
        }
    }

    /// New settings affect later admissions; already-issued leases remain
    /// charged and valid until their owners release them.
    pub fn update_policy(&self, policy: TableResourcePolicy) {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .policy = policy;
    }

    pub fn usage(&self) -> TableResourceUsage {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .usage
    }

    /// High-water mark of reserved aggregate usage, available to core tests
    /// that prove temporary decoder allocations were charged before use.
    #[cfg(test)]
    pub(crate) fn peak_usage(&self) -> TableResourceUsage {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .peak_usage
    }

    pub fn reserve_ram(&self, bytes: u64) -> Result<TableResourceLease, TableResourceError> {
        self.reserve(ResourceKind::Ram, bytes)
    }

    pub fn reserve_temp(&self, bytes: u64) -> Result<TableResourceLease, TableResourceError> {
        self.reserve(ResourceKind::Temp, bytes)
    }

    /// Retry deletions that failed during a temporary-file owner's Drop.
    /// Their byte reservations remain charged until a retry succeeds.
    pub fn retry_failed_temp_cleanup(&self) -> Vec<TableResourceError> {
        let pending = {
            let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
            std::mem::take(&mut state.pending_temp_cleanup)
        };
        // Keep both the retry diagnostics and their temporary allocation
        // bounded even if many external locks remain held across retries.
        let mut first_failure = None;
        for (path, cleanup) in pending {
            match fs::remove_file(&path) {
                Ok(()) => {
                    let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.usage.temp_bytes = state.usage.temp_bytes.saturating_sub(cleanup.bytes);
                    state.usage.ram_bytes =
                        state.usage.ram_bytes.saturating_sub(cleanup.metadata_bytes);
                    drop(state);
                    remove_empty_table_workspace(path.parent());
                }
                Err(error) if error.kind() == io::ErrorKind::NotFound => {
                    let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.usage.temp_bytes = state.usage.temp_bytes.saturating_sub(cleanup.bytes);
                    state.usage.ram_bytes =
                        state.usage.ram_bytes.saturating_sub(cleanup.metadata_bytes);
                    drop(state);
                    remove_empty_table_workspace(path.parent());
                }
                Err(_error) => {
                    first_failure.get_or_insert_with(|| TableResourceError::CleanupPending {
                        path: path.clone(),
                        message: cleanup.message.clone(),
                    });
                    let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
                    state.pending_temp_cleanup.insert(
                        path,
                        PendingTempCleanup {
                            bytes: cleanup.bytes,
                            metadata_bytes: cleanup.metadata_bytes,
                            // The first failure is useful; do not append a new
                            // OS message on every retry.
                            message: cleanup.message,
                        },
                    );
                }
            }
        }
        first_failure.into_iter().collect()
    }

    pub fn pending_temp_cleanup_count(&self) -> usize {
        self.inner
            .state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .pending_temp_cleanup
            .len()
    }

    fn reserve(
        &self,
        kind: ResourceKind,
        bytes: u64,
    ) -> Result<TableResourceLease, TableResourceError> {
        let captured_limit = self.captured_policy.map(|policy| match kind {
            ResourceKind::Ram => policy.ram_limit_bytes,
            ResourceKind::Temp => policy.temp_limit_bytes,
        });
        self.reserve_with_limit(kind, bytes, captured_limit)
    }

    fn reserve_with_limit(
        &self,
        kind: ResourceKind,
        bytes: u64,
        captured_limit: Option<u64>,
    ) -> Result<TableResourceLease, TableResourceError> {
        let mut state = self.inner.state.lock().unwrap_or_else(|e| e.into_inner());
        let limit = captured_limit.unwrap_or(match kind {
            ResourceKind::Ram => state.policy.ram_limit_bytes,
            ResourceKind::Temp => state.policy.temp_limit_bytes,
        });
        let used = match kind {
            ResourceKind::Ram => state.usage.ram_bytes,
            ResourceKind::Temp => state.usage.temp_bytes,
        };
        let next = used
            .checked_add(bytes)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        if next > limit {
            return Err(match kind {
                ResourceKind::Ram => TableResourceError::RamQuota {
                    requested: bytes,
                    used,
                    limit,
                },
                ResourceKind::Temp => TableResourceError::TempQuota {
                    requested: bytes,
                    used,
                    limit,
                },
            });
        }
        match kind {
            ResourceKind::Ram => state.usage.ram_bytes = next,
            ResourceKind::Temp => state.usage.temp_bytes = next,
        }
        #[cfg(test)]
        {
            state.peak_usage.ram_bytes = state.peak_usage.ram_bytes.max(state.usage.ram_bytes);
            state.peak_usage.temp_bytes = state.peak_usage.temp_bytes.max(state.usage.temp_bytes);
        }
        Ok(TableResourceLease {
            pool: Arc::clone(&self.inner),
            kind,
            bytes,
            limit_bytes: limit,
        })
    }
}

#[derive(Clone, Copy)]
enum ResourceKind {
    Ram,
    Temp,
}

/// Non-cloneable accounting lease. Growth is reserved before the corresponding
/// allocation or write; Drop releases exactly the bytes charged to this owner.
pub struct TableResourceLease {
    pool: Arc<PoolInner>,
    kind: ResourceKind,
    bytes: u64,
    /// The operation's admission-time policy survives later preference edits.
    limit_bytes: u64,
}

/// A shared immutable allocation paired with the reservation that accounts for
/// it. Cloning this handle also retains the reservation, so callers cannot
/// outlive the charge by cloning only a snapshot's data pointer.
pub struct TableResourceOwned<T: ?Sized> {
    value: Arc<T>,
    lease: Option<Arc<TableResourceLease>>,
}

impl<T: ?Sized> Clone for TableResourceOwned<T> {
    fn clone(&self) -> Self {
        Self {
            value: Arc::clone(&self.value),
            lease: self.lease.clone(),
        }
    }
}

impl<T: ?Sized> TableResourceOwned<T> {
    pub fn new(value: Arc<T>, lease: Arc<TableResourceLease>) -> Self {
        Self {
            value,
            lease: Some(lease),
        }
    }

    /// For zero-payload structural values such as the initial empty draft.
    pub fn without_payload_charge(value: Arc<T>) -> Self {
        Self { value, lease: None }
    }

    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

impl<T: ?Sized> std::ops::Deref for TableResourceOwned<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.value
    }
}

impl<T: ?Sized> AsRef<T> for TableResourceOwned<T> {
    fn as_ref(&self) -> &T {
        &self.value
    }
}

impl<T: ?Sized + std::fmt::Debug> std::fmt::Debug for TableResourceOwned<T> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        std::fmt::Debug::fmt(&self.value, formatter)
    }
}

impl<T: ?Sized + PartialEq> PartialEq for TableResourceOwned<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl<T: ?Sized + Eq> Eq for TableResourceOwned<T> {}

impl TableResourceLease {
    pub fn bytes(&self) -> u64 {
        self.bytes
    }

    pub fn grow(&mut self, additional: u64) -> Result<(), TableResourceError> {
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        let limit = self.limit_bytes;
        let used = match self.kind {
            ResourceKind::Ram => state.usage.ram_bytes,
            ResourceKind::Temp => state.usage.temp_bytes,
        };
        let next_used = used
            .checked_add(additional)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        let next_owned = self
            .bytes
            .checked_add(additional)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        if next_used > limit {
            return Err(match self.kind {
                ResourceKind::Ram => TableResourceError::RamQuota {
                    requested: additional,
                    used,
                    limit,
                },
                ResourceKind::Temp => TableResourceError::TempQuota {
                    requested: additional,
                    used,
                    limit,
                },
            });
        }
        match self.kind {
            ResourceKind::Ram => state.usage.ram_bytes = next_used,
            ResourceKind::Temp => state.usage.temp_bytes = next_used,
        }
        #[cfg(test)]
        {
            state.peak_usage.ram_bytes = state.peak_usage.ram_bytes.max(state.usage.ram_bytes);
            state.peak_usage.temp_bytes = state.peak_usage.temp_bytes.max(state.usage.temp_bytes);
        }
        self.bytes = next_owned;
        Ok(())
    }

    pub(crate) fn shrink(&mut self, bytes: u64) {
        let release = bytes.min(self.bytes);
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        match self.kind {
            ResourceKind::Ram => {
                state.usage.ram_bytes = state.usage.ram_bytes.saturating_sub(release)
            }
            ResourceKind::Temp => {
                state.usage.temp_bytes = state.usage.temp_bytes.saturating_sub(release)
            }
        }
        self.bytes -= release;
    }

    fn preserve_temp_cleanup(
        &mut self,
        path: PathBuf,
        bytes: u64,
        message: String,
        metadata_lease: &mut TableResourceLease,
    ) {
        debug_assert!(matches!(self.kind, ResourceKind::Temp));
        debug_assert!(matches!(metadata_lease.kind, ResourceKind::Ram));
        debug_assert!(Arc::ptr_eq(&self.pool, &metadata_lease.pool));
        let mut state = self.pool.state.lock().unwrap_or_else(|e| e.into_inner());
        let previous = state.pending_temp_cleanup.insert(
            path,
            PendingTempCleanup {
                bytes,
                metadata_bytes: metadata_lease.bytes,
                message,
            },
        );
        debug_assert!(previous.is_none(), "temporary path identity must be unique");
        self.bytes = 0;
        // The pool keeps the numeric reservation but no lease/Arc. This avoids
        // a PoolInner -> pending entry -> lease -> PoolInner reference cycle.
        metadata_lease.bytes = 0;
    }
}

impl Drop for TableResourceLease {
    fn drop(&mut self) {
        self.shrink(self.bytes);
    }
}

struct WorkspaceRoot {
    path: PathBuf,
}

impl Drop for WorkspaceRoot {
    fn drop(&mut self) {
        let _ = fs::remove_dir(&self.path);
    }
}

fn remove_empty_table_workspace(parent: Option<&Path>) {
    if let Some(parent) = parent
        && parent
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("multi-launcher-table-"))
    {
        let _ = fs::remove_dir(parent);
    }
}

type FreeSpaceProbe = Arc<dyn Fn() -> Result<u64, TableResourceError> + Send + Sync>;

/// A single selected-volume workspace shared by sources and their indexes.
#[derive(Clone)]
pub struct TableTempWorkspace {
    root: Arc<WorkspaceRoot>,
    pool: TableResourcePool,
    free_space_probe: FreeSpaceProbe,
    min_free_bytes: u64,
    temp_limit_bytes: u64,
}

impl TableTempWorkspace {
    /// The available-byte count is supplied by the application adapter for the
    /// exact selected volume; tests can inject a deterministic value.
    pub fn create(
        base: impl AsRef<Path>,
        pool: TableResourcePool,
        available_bytes: u64,
    ) -> Result<Self, TableResourceError> {
        Self::create_with_free_space_probe(base, pool, move || Ok(available_bytes))
    }

    /// Create a workspace whose selected-volume free-space check is refreshed
    /// before every temporary-file growth.
    pub fn create_with_free_space_probe(
        base: impl AsRef<Path>,
        pool: TableResourcePool,
        probe: impl Fn() -> Result<u64, TableResourceError> + Send + Sync + 'static,
    ) -> Result<Self, TableResourceError> {
        let policy = pool.policy();
        let free_space_probe: FreeSpaceProbe = Arc::new(probe);
        let available_bytes = free_space_probe()?;
        if available_bytes < policy.min_free_bytes() {
            return Err(TableResourceError::InsufficientFreeSpace {
                available: available_bytes,
                required: policy.min_free_bytes(),
            });
        }
        let base = base.as_ref();
        for _ in 0..8 {
            let id = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
            let path = base.join(format!("multi-launcher-table-{id:016x}"));
            match fs::create_dir(&path) {
                Ok(()) => {
                    return Ok(Self {
                        root: Arc::new(WorkspaceRoot { path }),
                        pool,
                        free_space_probe,
                        min_free_bytes: policy.min_free_bytes(),
                        temp_limit_bytes: policy.temp_limit_bytes(),
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(TableResourceError::Io(error.to_string())),
            }
        }
        Err(TableResourceError::Io(
            "could not allocate a unique workspace name".into(),
        ))
    }

    pub(crate) fn uses_pool(&self, pool: &TableResourcePool) -> bool {
        Arc::ptr_eq(&self.pool.inner, &pool.inner)
    }

    pub fn create_file(&self, label: &str) -> Result<TableTempFile, TableResourceError> {
        let lease =
            self.pool
                .reserve_with_limit(ResourceKind::Temp, 0, Some(self.temp_limit_bytes))?;
        let cleanup_metadata_lease = self.pool.reserve_ram(CLEANUP_METADATA_RESERVATION_BYTES)?;
        let safe_label = label
            .chars()
            .filter(|character| character.is_ascii_alphanumeric() || *character == '-')
            .take(32)
            .collect::<String>();
        for _ in 0..8 {
            let id = NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed);
            let name = format!("{safe_label}-{id:016x}.tmp");
            let path = self.root.path.join(name);
            if path_storage_bytes(&path) > MAX_CLEANUP_PATH_BYTES {
                return Err(TableResourceError::Io(
                    "temporary table path exceeds the bounded cleanup metadata limit".into(),
                ));
            }
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .open(&path)
            {
                Ok(file) => {
                    return Ok(TableTempFile {
                        file,
                        path,
                        _root: Arc::clone(&self.root),
                        lease: Some(lease),
                        cleanup_metadata_lease: Some(cleanup_metadata_lease),
                        charged_bytes: 0,
                        free_space_probe: Arc::clone(&self.free_space_probe),
                        min_free_bytes: self.min_free_bytes,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(TableResourceError::Io(error.to_string())),
            }
        }
        Err(TableResourceError::Io(
            "could not allocate a unique temporary file name".into(),
        ))
    }
}

/// A table-owned temporary file whose bytes remain charged until successful
/// deletion. A failed deletion intentionally leaves the reservation charged.
pub struct TableTempFile {
    file: File,
    path: PathBuf,
    _root: Arc<WorkspaceRoot>,
    lease: Option<TableResourceLease>,
    cleanup_metadata_lease: Option<TableResourceLease>,
    charged_bytes: u64,
    free_space_probe: FreeSpaceProbe,
    min_free_bytes: u64,
}

impl TableTempFile {
    pub fn append(&mut self, bytes: &[u8]) -> Result<(), TableResourceError> {
        self.append_with_writer(bytes, |file, bytes| file.write_all(bytes))
    }

    fn append_with_writer(
        &mut self,
        bytes: &[u8],
        write: impl FnOnce(&mut File, &[u8]) -> io::Result<()>,
    ) -> Result<(), TableResourceError> {
        let count =
            u64::try_from(bytes.len()).map_err(|_| TableResourceError::ArithmeticOverflow)?;
        let available = (self.free_space_probe)()?;
        let required = self
            .min_free_bytes
            .checked_add(count)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        if available < required {
            return Err(TableResourceError::InsufficientFreeSpace {
                available,
                required,
            });
        }
        self.lease
            .as_mut()
            .ok_or_else(|| TableResourceError::Io("temporary-file lease was released".into()))?
            .grow(count)?;
        if let Err(error) = write(&mut self.file, bytes) {
            // A short write can leave partial bytes behind. Keep the full
            // reservation until the file is removed.
            self.charged_bytes = self.charged_bytes.saturating_add(count);
            return Err(TableResourceError::Io(error.to_string()));
        }
        self.charged_bytes = self
            .charged_bytes
            .checked_add(count)
            .ok_or(TableResourceError::ArithmeticOverflow)?;
        Ok(())
    }

    pub fn flush_and_rewind(&mut self) -> Result<(), TableResourceError> {
        self.file
            .flush()
            .map_err(|e| TableResourceError::Io(e.to_string()))?;
        self.file
            .seek(SeekFrom::Start(0))
            .map_err(|e| TableResourceError::Io(e.to_string()))?;
        Ok(())
    }

    pub fn read_u64_at(&mut self, offset: u64) -> Result<u64, TableResourceError> {
        self.file
            .seek(SeekFrom::Start(offset))
            .map_err(|e| TableResourceError::Io(e.to_string()))?;
        let mut bytes = [0_u8; 8];
        self.file
            .read_exact(&mut bytes)
            .map_err(|e| TableResourceError::Io(e.to_string()))?;
        Ok(u64::from_le_bytes(bytes))
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn charged_bytes(&self) -> u64 {
        self.charged_bytes
    }
}

impl Drop for TableTempFile {
    fn drop(&mut self) {
        let _ = self.file.sync_all();
        if let Err(error) = fs::remove_file(&self.path) {
            if error.kind() != io::ErrorKind::NotFound
                && let Some(mut lease) = self.lease.take()
                && let Some(mut metadata_lease) = self.cleanup_metadata_lease.take()
            {
                let message = error
                    .to_string()
                    .chars()
                    .take(MAX_CLEANUP_MESSAGE_CHARS)
                    .collect::<String>();
                lease.preserve_temp_cleanup(
                    self.path.clone(),
                    self.charged_bytes,
                    message,
                    &mut metadata_lease,
                );
            }
        }
    }
}

#[cfg(windows)]
fn path_storage_bytes(path: &Path) -> usize {
    use std::os::windows::ffi::OsStrExt;
    path.as_os_str().encode_wide().count().saturating_mul(2)
}

#[cfg(not(windows))]
fn path_storage_bytes(path: &Path) -> usize {
    path.as_os_str().as_encoded_bytes().len()
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    fn tiny_pool() -> TableResourcePool {
        TableResourcePool {
            inner: Arc::new(PoolInner {
                state: Mutex::new(PoolState {
                    policy: TableResourcePolicy {
                        ram_limit_bytes: 10,
                        temp_limit_bytes: 10,
                        min_free_bytes: 4,
                    },
                    usage: TableResourceUsage::default(),
                    #[cfg(test)]
                    peak_usage: TableResourceUsage::default(),
                    pending_temp_cleanup: BTreeMap::new(),
                }),
            }),
            captured_policy: None,
        }
    }

    fn workspace_pool() -> TableResourcePool {
        TableResourcePool {
            inner: Arc::new(PoolInner {
                state: Mutex::new(PoolState {
                    policy: TableResourcePolicy {
                        ram_limit_bytes: 16 * 1024,
                        temp_limit_bytes: 10,
                        min_free_bytes: 4,
                    },
                    usage: TableResourceUsage::default(),
                    #[cfg(test)]
                    peak_usage: TableResourceUsage::default(),
                    pending_temp_cleanup: BTreeMap::new(),
                }),
            }),
            captured_policy: None,
        }
    }

    #[test]
    fn resource_pool_leases_share_usage_and_release_on_last_owner() {
        let pool = tiny_pool();
        let mut first = pool.reserve_ram(6).unwrap();
        let second = pool.reserve_ram(3).unwrap();
        assert_eq!(pool.usage().ram_bytes, 9);
        assert!(matches!(
            pool.reserve_ram(2),
            Err(TableResourceError::RamQuota { .. })
        ));
        first.grow(1).unwrap();
        assert_eq!(pool.usage().ram_bytes, 10);
        drop(second);
        assert_eq!(pool.usage().ram_bytes, 7);
        drop(first);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn quota_failed_growth_preserves_the_existing_reservation() {
        let pool = tiny_pool();
        let mut first = pool.reserve_ram(6).unwrap();
        let second = pool.reserve_ram(4).unwrap();

        assert!(matches!(
            first.grow(1),
            Err(TableResourceError::RamQuota {
                requested: 1,
                used: 10,
                limit: 10
            })
        ));
        assert_eq!(first.bytes(), 6);
        assert_eq!(pool.usage().ram_bytes, 10);

        drop(second);
        assert_eq!(pool.usage().ram_bytes, 6);
        drop(first);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn overflowing_growth_preserves_the_existing_reservation() {
        let pool = tiny_pool();
        let mut lease = pool.reserve_ram(6).unwrap();
        assert!(matches!(
            lease.grow(u64::MAX),
            Err(TableResourceError::ArithmeticOverflow)
        ));
        assert_eq!(lease.bytes(), 6);
        assert_eq!(pool.usage().ram_bytes, 6);
        drop(lease);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn temp_workspace_preflights_free_space_and_charges_until_file_drop() {
        let pool = workspace_pool();
        let base = std::env::temp_dir();
        assert!(matches!(
            TableTempWorkspace::create(&base, pool.clone(), 3),
            Err(TableResourceError::InsufficientFreeSpace { .. })
        ));
        let workspace = TableTempWorkspace::create(&base, pool.clone(), 12).unwrap();
        let mut file = workspace.create_file("index").unwrap();
        assert_eq!(pool.usage().ram_bytes, CLEANUP_METADATA_RESERVATION_BYTES);
        file.append(&1_u64.to_le_bytes()).unwrap();
        assert_eq!(pool.usage().temp_bytes, 8);
        file.flush_and_rewind().unwrap();
        assert_eq!(file.read_u64_at(0).unwrap(), 1);
        drop(file);
        assert_eq!(pool.usage().temp_bytes, 0);
        assert_eq!(pool.usage().ram_bytes, 0);
    }

    #[test]
    fn preferences_reject_values_outside_production_ranges() {
        assert!(TableResourcePolicy::new(64 * MIB, GIB).is_ok());
        assert!(TableResourcePolicy::new(1024 * MIB, 64 * GIB).is_ok());
        assert!(TableResourcePolicy::new(63 * MIB, GIB).is_err());
        assert!(TableResourcePolicy::new(256 * MIB, GIB - 1).is_err());
    }

    #[test]
    fn admitted_leases_keep_their_policy_while_future_reservations_use_updates() {
        let pool = tiny_pool();
        let mut admitted = pool.reserve_ram(8).unwrap();
        pool.update_policy(TableResourcePolicy {
            ram_limit_bytes: 5,
            temp_limit_bytes: 10,
            min_free_bytes: 4,
        });

        admitted.grow(2).unwrap();
        assert_eq!(pool.usage().ram_bytes, 10);
        assert!(matches!(
            pool.reserve_ram(1),
            Err(TableResourceError::RamQuota { limit: 5, .. })
        ));
        drop(admitted);
        let mut next_admission = pool.reserve_ram(5).unwrap();
        assert!(matches!(
            next_admission.grow(1),
            Err(TableResourceError::RamQuota { limit: 5, .. })
        ));
    }

    #[test]
    fn captured_admission_view_keeps_limits_and_shares_usage() {
        let pool = workspace_pool();
        let admitted = pool.capture_policy();
        pool.update_policy(TableResourcePolicy {
            ram_limit_bytes: 16 * 1024,
            temp_limit_bytes: 5,
            min_free_bytes: 4,
        });

        // The admitted workspace keeps the original ten-byte temp limit,
        // while all of its growth remains visible to the current pool view.
        let workspace = TableTempWorkspace::create(&std::env::temp_dir(), admitted, 12).unwrap();
        let mut file = workspace.create_file("captured-policy").unwrap();
        file.append(&[0; 6]).unwrap();
        assert_eq!(pool.usage().temp_bytes, 6);
        assert!(matches!(
            pool.reserve_temp(1),
            Err(TableResourceError::TempQuota { limit: 5, .. })
        ));
        drop(file);
        assert_eq!(pool.usage().temp_bytes, 0);
    }

    #[test]
    fn temp_file_rechecks_free_space_before_each_growth() {
        use std::sync::atomic::AtomicU64;

        let pool = workspace_pool();
        let available = Arc::new(AtomicU64::new(11));
        let probe_available = Arc::clone(&available);
        let workspace = TableTempWorkspace::create_with_free_space_probe(
            std::env::temp_dir(),
            pool.clone(),
            move || Ok(probe_available.load(Ordering::SeqCst)),
        )
        .unwrap();
        let mut file = workspace.create_file("index").unwrap();
        assert!(matches!(
            file.append(&[0; 8]),
            Err(TableResourceError::InsufficientFreeSpace {
                available: 11,
                required: 12
            })
        ));
        assert_eq!(pool.usage().temp_bytes, 0);
        available.store(12, Ordering::SeqCst);
        file.append(&[0; 8]).unwrap();
        assert_eq!(pool.usage().temp_bytes, 8);
        available.store(4, Ordering::SeqCst);
        assert!(matches!(
            file.append(&[0]),
            Err(TableResourceError::InsufficientFreeSpace {
                available: 4,
                required: 5
            })
        ));
        drop(file);
        assert_eq!(pool.usage().temp_bytes, 0);
    }

    #[test]
    fn failed_temp_deletion_keeps_a_retryable_charge_until_removed() {
        let pool = workspace_pool();
        let path = std::env::temp_dir().join(format!(
            "table-cleanup-retry-{}.tmp",
            NEXT_FILE_ID.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        fs::write(path.join("held"), b"x").unwrap();
        let mut lease = pool.reserve_temp(8).unwrap();
        let mut metadata_lease = pool
            .reserve_ram(CLEANUP_METADATA_RESERVATION_BYTES)
            .unwrap();
        lease.preserve_temp_cleanup(
            path.clone(),
            8,
            "injected first failure".into(),
            &mut metadata_lease,
        );
        drop(lease);
        drop(metadata_lease);
        assert_eq!(pool.usage().temp_bytes, 8);
        assert_eq!(pool.usage().ram_bytes, CLEANUP_METADATA_RESERVATION_BYTES);
        assert_eq!(pool.pending_temp_cleanup_count(), 1);
        assert!(matches!(
            pool.retry_failed_temp_cleanup().as_slice(),
            [TableResourceError::CleanupPending { .. }]
        ));
        assert_eq!(pool.usage().temp_bytes, 8);
        for _ in 0..16 {
            let errors = pool.retry_failed_temp_cleanup();
            assert_eq!(errors.len(), 1);
            let TableResourceError::CleanupPending { message, .. } = &errors[0] else {
                panic!("expected bounded pending cleanup diagnostic");
            };
            assert!(message.len() < 512);
            assert_eq!(pool.usage().ram_bytes, CLEANUP_METADATA_RESERVATION_BYTES);
            assert_eq!(pool.pending_temp_cleanup_count(), 1);
        }
        fs::remove_file(path.join("held")).unwrap();
        fs::remove_dir(&path).unwrap();
        fs::write(&path, b"owned temporary file").unwrap();
        assert!(pool.retry_failed_temp_cleanup().is_empty());
        assert_eq!(pool.pending_temp_cleanup_count(), 0);
        assert_eq!(pool.usage().temp_bytes, 0);
        assert_eq!(pool.usage().ram_bytes, 0);
        assert!(!path.exists());
    }

    #[test]
    fn partial_temp_append_failure_keeps_charge_until_owned_file_is_removed() {
        let pool = workspace_pool();
        let workspace = TableTempWorkspace::create(std::env::temp_dir(), pool.clone(), 64).unwrap();
        let mut file = workspace.create_file("partial-write").unwrap();
        let path = file.path().to_path_buf();

        let result = file.append_with_writer(b"abcdefgh", |file, bytes| {
            file.write_all(&bytes[..3])?;
            Err(io::Error::other("injected disk-full after short write"))
        });

        assert!(matches!(result, Err(TableResourceError::Io(_))));
        assert_eq!(fs::metadata(&path).unwrap().len(), 3);
        assert_eq!(file.charged_bytes(), 8);
        assert_eq!(pool.usage().temp_bytes, 8);
        drop(file);
        assert!(!path.exists());
        assert_eq!(pool.usage().temp_bytes, 0);
        assert_eq!(pool.usage().ram_bytes, 0);
    }
}
