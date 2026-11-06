use crate::file_operations::NativeFileIdentity;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const MAX_FILE_LOCK_PATH_UTF16_UNITS: usize =
    crate::batch_rename::MAX_WINDOWS_PATH_UTF16_UNITS_WITH_NUL;
pub const MAX_REPORTED_FILE_USERS: usize = 256;
const MAX_RM_LIST_ATTEMPTS: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLockProcess {
    pub process_id: u32,
    /// Raw Windows creation `FILETIME`, retained without epoch conversion.
    pub creation_filetime: u64,
    pub application_name: String,
    pub service_name: String,
    /// Included only when the PID still names the exact reported process.
    pub executable_path: Option<PathBuf>,
}

#[derive(Debug, Clone)]
pub struct FileLockInspection {
    pub path: PathBuf,
    pub identity: NativeFileIdentity,
    pub inspected_at: SystemTime,
    pub processes: Vec<FileLockProcess>,
}

impl FileLockInspection {
    pub fn reports_no_processes(&self) -> bool {
        self.processes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FileLockInspectionError {
    UnsupportedPlatform,
    InvalidPath,
    PathTooLong,
    ProbeFailed(u32),
    NotRegularDiskFile,
    UnsupportedFileIdentity,
    FileIdentityRecheckFailed(u32),
    StartSessionFailed(u32),
    RegisterResourceFailed(u32),
    SessionCleanupFailed(u32),
    FileChangedDuringInspection,
    ListFailed(u32),
    TooManyProcesses(usize),
    MalformedProcessCount { reported: usize, capacity: usize },
    ProcessListDidNotStabilize,
    MalformedProcessRecord,
}

impl std::fmt::Display for FileLockInspectionError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedPlatform => {
                formatter.write_str("file-user inspection is available on Windows only")
            }
            Self::InvalidPath => formatter.write_str(
                "select an absolute, non-empty file path without embedded NUL characters",
            ),
            Self::PathTooLong => {
                formatter.write_str("the selected file path exceeds the Windows UTF-16 path limit")
            }
            Self::ProbeFailed(code) => write!(
                formatter,
                "could not inspect the selected file handle (Windows error {code})"
            ),
            Self::NotRegularDiskFile => {
                formatter.write_str("the selected path is not a regular disk file")
            }
            Self::UnsupportedFileIdentity => formatter.write_str(
                "the file system did not provide a stable file identity for the selected file",
            ),
            Self::FileIdentityRecheckFailed(code) => write!(
                formatter,
                "the file identity could not be rechecked during inspection (Windows error {code})"
            ),
            Self::StartSessionFailed(code) => write!(
                formatter,
                "Restart Manager could not start an inspection session (Windows error {code})"
            ),
            Self::RegisterResourceFailed(code) => write!(
                formatter,
                "Restart Manager could not register the selected file (Windows error {code})"
            ),
            Self::SessionCleanupFailed(code) => write!(
                formatter,
                "Restart Manager could not end the inspection session (Windows error {code})"
            ),
            Self::FileChangedDuringInspection => formatter.write_str(
                "the selected file changed while its reported users were being inspected",
            ),
            Self::ListFailed(code) => write!(
                formatter,
                "Restart Manager could not list file users (Windows error {code})"
            ),
            Self::TooManyProcesses(count) => write!(
                formatter,
                "Restart Manager reported {count} processes, exceeding the inspection limit"
            ),
            Self::MalformedProcessCount { reported, capacity } => write!(
                formatter,
                "Restart Manager returned {reported} process rows for a buffer with capacity {capacity}"
            ),
            Self::ProcessListDidNotStabilize => formatter
                .write_str("the Restart Manager process list kept changing during bounded retries"),
            Self::MalformedProcessRecord => {
                formatter.write_str("Restart Manager returned a malformed process record")
            }
        }
    }
}

impl std::error::Error for FileLockInspectionError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLockInspectionFailure {
    pub error: FileLockInspectionError,
    /// A cleanup failure is retained independently from the inspection error.
    pub cleanup_error: Option<u32>,
}

impl std::fmt::Display for FileLockInspectionFailure {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{}", self.error)?;
        if let Some(code) = self
            .cleanup_error
            .filter(|code| self.error != FileLockInspectionError::SessionCleanupFailed(*code))
        {
            write!(
                formatter,
                "; Restart Manager session cleanup failed (Windows error {code})"
            )?;
        }
        Ok(())
    }
}

impl std::error::Error for FileLockInspectionFailure {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct RawProcessRecord {
    process_id: u32,
    creation_filetime: u64,
    application_name: String,
    service_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ProcessMetadata {
    creation_filetime: u64,
    executable_path: PathBuf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ListCall {
    Complete { reported_count: usize },
    MoreData { needed: usize },
}

trait RestartManagerApi {
    type Probe;

    fn probe_file(
        &mut self,
        path: &[u16],
    ) -> Result<(Self::Probe, NativeFileIdentity), FileLockInspectionError>;
    fn start_session(&mut self) -> Result<u32, u32>;
    fn register_file(&mut self, session: u32, path: &[u16]) -> Result<(), u32>;
    fn get_list(&mut self, session: u32, output: &mut [RawProcessRecord]) -> Result<ListCall, u32>;
    fn process_metadata(
        &mut self,
        process_id: u32,
        expected_creation_filetime: u64,
    ) -> Option<ProcessMetadata>;
    fn end_session(&mut self, session: u32) -> Result<(), u32>;
}

struct RmSession<'a, A: RestartManagerApi> {
    api: &'a mut A,
    id: u32,
    end_attempted: bool,
}

impl<'a, A: RestartManagerApi> RmSession<'a, A> {
    fn new(api: &'a mut A, id: u32) -> Self {
        Self {
            api,
            id,
            end_attempted: false,
        }
    }

    fn finish(&mut self) -> Result<(), u32> {
        if self.end_attempted {
            return Ok(());
        }
        self.end_attempted = true;
        self.api.end_session(self.id)
    }
}

impl<A: RestartManagerApi> Drop for RmSession<'_, A> {
    fn drop(&mut self) {
        if !self.end_attempted {
            self.end_attempted = true;
            let _ = self.api.end_session(self.id);
        }
    }
}

fn validate_resource_path(path: &[u16]) -> Result<(), FileLockInspectionError> {
    if path.len() < 2
        || path.len() > MAX_FILE_LOCK_PATH_UTF16_UNITS
        || path.last().copied() != Some(0)
        || path[..path.len() - 1].contains(&0)
        || !is_absolute_windows_path(&path[..path.len() - 1])
    {
        return Err(if path.len() > MAX_FILE_LOCK_PATH_UTF16_UNITS {
            FileLockInspectionError::PathTooLong
        } else {
            FileLockInspectionError::InvalidPath
        });
    }
    Ok(())
}

fn is_absolute_windows_path(path: &[u16]) -> bool {
    let is_drive_rooted = |path: &[u16]| {
        path.len() >= 3
            && ((b'A' as u16..=b'Z' as u16).contains(&path[0])
                || (b'a' as u16..=b'z' as u16).contains(&path[0]))
            && path[1] == b':' as u16
            && matches!(path[2], 0x5c | 0x2f)
            && !path[3..].contains(&(b':' as u16))
    };
    let starts_ascii_case_insensitive = |value: &[u16], prefix: &[u16]| {
        value.len() >= prefix.len()
            && value[..prefix.len()]
                .iter()
                .zip(prefix)
                .all(|(actual, expected)| ascii_lower(*actual) == ascii_lower(*expected))
    };
    if path.len() >= 4 && path[..4] == [0x5c, 0x5c, b'?' as u16, 0x5c] {
        let extended = &path[4..];
        if is_drive_rooted(extended) {
            return true;
        }
        if starts_ascii_case_insensitive(extended, &[b'U' as u16, b'N' as u16, b'C' as u16, 0x5c]) {
            return is_unc_disk_path(&extended[4..]);
        }
        return false;
    }
    if is_drive_rooted(path) {
        return true;
    }
    path.len() >= 3 && path[..2] == [0x5c, 0x5c] && is_unc_disk_path(&path[2..])
}

fn ascii_lower(unit: u16) -> u16 {
    if (b'A' as u16..=b'Z' as u16).contains(&unit) {
        unit + (b'a' as u16 - b'A' as u16)
    } else {
        unit
    }
}

fn is_unc_disk_path(unc_tail: &[u16]) -> bool {
    let mut components = unc_tail.split(|unit| matches!(*unit, 0x5c | 0x2f));
    let Some(server) = components.next() else {
        return false;
    };
    let Some(share) = components.next() else {
        return false;
    };
    let server = trim_windows_component(server);
    !server.is_empty()
        && !share.is_empty()
        && !unc_tail.contains(&(b':' as u16))
        && server != [b'.' as u16]
        && server != [b'?' as u16]
        && !windows_component_is(share, &[b'p' as u16, b'i' as u16, b'p' as u16, b'e' as u16])
        && !windows_component_is(
            share,
            &[
                b'm' as u16,
                b'a' as u16,
                b'i' as u16,
                b'l' as u16,
                b's' as u16,
                b'l' as u16,
                b'o' as u16,
                b't' as u16,
            ],
        )
        && !windows_component_is(share, &[b'i' as u16, b'p' as u16, b'c' as u16, b'$' as u16])
}

fn trim_windows_component(component: &[u16]) -> &[u16] {
    let mut end = component.len();
    while end > 0 && (component[end - 1] == b'.' as u16 || component[end - 1] == b' ' as u16) {
        end -= 1;
    }
    &component[..end]
}

fn windows_component_is(component: &[u16], expected: &[u16]) -> bool {
    let component = trim_windows_component(component);
    component.len() == expected.len()
        && component
            .iter()
            .zip(expected)
            .all(|(actual, expected)| ascii_lower(*actual) == ascii_lower(*expected))
}

fn verify_file_identity<A: RestartManagerApi>(
    api: &mut A,
    path: &[u16],
    expected: NativeFileIdentity,
) -> Result<(), FileLockInspectionError> {
    let (probe, actual) = match api.probe_file(path) {
        Ok(probe) => probe,
        Err(FileLockInspectionError::NotRegularDiskFile) => {
            return Err(FileLockInspectionError::FileChangedDuringInspection);
        }
        Err(FileLockInspectionError::ProbeFailed(code)) => {
            return Err(FileLockInspectionError::FileIdentityRecheckFailed(code));
        }
        Err(FileLockInspectionError::UnsupportedFileIdentity) => {
            return Err(FileLockInspectionError::FileIdentityRecheckFailed(1));
        }
        Err(error) => return Err(error),
    };
    drop(probe);
    if actual != expected || actual.file_id() == [0; 16] {
        Err(FileLockInspectionError::FileChangedDuringInspection)
    } else {
        Ok(())
    }
}

fn inspect_with_api<A: RestartManagerApi>(
    api: &mut A,
    path: &[u16],
) -> Result<(NativeFileIdentity, Vec<FileLockProcess>), FileLockInspectionFailure> {
    validate_resource_path(path).map_err(failure)?;

    let (probe, identity_before) = api.probe_file(path).map_err(failure)?;
    drop(probe);
    if identity_before.file_id() == [0; 16] {
        return Err(failure(FileLockInspectionError::UnsupportedFileIdentity));
    }

    let session_id = api
        .start_session()
        .map_err(|code| failure(FileLockInspectionError::StartSessionFailed(code)))?;
    let mut session = RmSession::new(api, session_id);
    let operation = (|| {
        session
            .api
            .register_file(session.id, path)
            .map_err(|code| FileLockInspectionError::RegisterResourceFailed(code))?;

        verify_file_identity(session.api, path, identity_before)?;
        let records = read_process_list(session.api, session.id)?;
        let processes = records
            .into_iter()
            .map(|record| {
                let executable_path = session
                    .api
                    .process_metadata(record.process_id, record.creation_filetime)
                    .filter(|metadata| metadata.creation_filetime == record.creation_filetime)
                    .map(|metadata| metadata.executable_path);
                FileLockProcess {
                    process_id: record.process_id,
                    creation_filetime: record.creation_filetime,
                    application_name: record.application_name,
                    service_name: record.service_name,
                    executable_path,
                }
            })
            .collect();
        verify_file_identity(session.api, path, identity_before)?;
        Ok((identity_before, processes))
    })();
    let cleanup = session.finish();

    match (operation, cleanup) {
        (Ok(value), Ok(())) => Ok(value),
        (Err(error), cleanup) => Err(FileLockInspectionFailure {
            error,
            cleanup_error: cleanup.err(),
        }),
        (Ok(_), Err(cleanup_error)) => Err(FileLockInspectionFailure {
            error: FileLockInspectionError::SessionCleanupFailed(cleanup_error),
            cleanup_error: None,
        }),
    }
}

fn read_process_list<A: RestartManagerApi>(
    api: &mut A,
    session: u32,
) -> Result<Vec<RawProcessRecord>, FileLockInspectionError> {
    let mut buffer = Vec::new();
    for _ in 0..MAX_RM_LIST_ATTEMPTS {
        let response = api
            .get_list(session, &mut buffer)
            .map_err(FileLockInspectionError::ListFailed)?;
        match response {
            ListCall::Complete { reported_count } => {
                if reported_count > buffer.len() {
                    return Err(FileLockInspectionError::MalformedProcessCount {
                        reported: reported_count,
                        capacity: buffer.len(),
                    });
                }
                if reported_count > MAX_REPORTED_FILE_USERS {
                    return Err(FileLockInspectionError::TooManyProcesses(reported_count));
                }
                buffer.truncate(reported_count);
                return Ok(buffer);
            }
            ListCall::MoreData { needed } => {
                if needed > MAX_REPORTED_FILE_USERS {
                    return Err(FileLockInspectionError::TooManyProcesses(needed));
                }
                let next_len = if needed > buffer.len() {
                    needed
                } else if buffer.is_empty() {
                    1
                } else {
                    buffer.len().saturating_mul(2).min(MAX_REPORTED_FILE_USERS)
                };
                if next_len <= buffer.len() {
                    return Err(FileLockInspectionError::ProcessListDidNotStabilize);
                }
                buffer.resize_with(next_len, || RawProcessRecord {
                    process_id: 0,
                    creation_filetime: 0,
                    application_name: String::new(),
                    service_name: String::new(),
                });
            }
        }
    }
    Err(FileLockInspectionError::ProcessListDidNotStabilize)
}

fn failure(error: FileLockInspectionError) -> FileLockInspectionFailure {
    FileLockInspectionFailure {
        error,
        cleanup_error: None,
    }
}

#[cfg(windows)]
mod native {
    use super::*;
    use std::fs::File;
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    use std::os::windows::io::FromRawHandle;
    use windows::Win32::Foundation::{CloseHandle, FILETIME, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        CreateFileW, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
        FILE_ATTRIBUTE_TAG_INFO, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAGS_AND_ATTRIBUTES,
        FILE_READ_ATTRIBUTES, FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ,
        FILE_SHARE_WRITE, FILE_TYPE_DISK, FileAttributeTagInfo, GetFileInformationByHandleEx,
        GetFileType, OPEN_EXISTING,
    };
    use windows::Win32::System::RestartManager::{
        CCH_RM_SESSION_KEY, RM_PROCESS_INFO, RmEndSession, RmGetList, RmRegisterResources,
        RmStartSession,
    };
    use windows::Win32::System::Threading::{
        GetProcessTimes, OpenProcess, PROCESS_NAME_FORMAT, PROCESS_QUERY_LIMITED_INFORMATION,
        QueryFullProcessImageNameW,
    };
    use windows::core::{PCWSTR, PWSTR};

    struct Probe(File);

    struct NativeApi;

    fn error_code(error: &windows::core::Error) -> u32 {
        let code = error.code().0 as u32;
        if code & 0xffff_0000 == 0x8007_0000 {
            code & 0xffff
        } else {
            code
        }
    }

    fn encode_path(path: &Path) -> Result<Vec<u16>, FileLockInspectionError> {
        if !path.is_absolute() {
            return Err(FileLockInspectionError::InvalidPath);
        }
        let units: Vec<u16> = path
            .as_os_str()
            .encode_wide()
            .take(MAX_FILE_LOCK_PATH_UTF16_UNITS)
            .collect();
        if units.is_empty() || units.contains(&0) {
            return Err(FileLockInspectionError::InvalidPath);
        }
        if units.len() >= MAX_FILE_LOCK_PATH_UTF16_UNITS {
            return Err(FileLockInspectionError::PathTooLong);
        }
        let mut nul_terminated = units;
        nul_terminated.push(0);
        validate_resource_path(&nul_terminated)?;
        Ok(nul_terminated)
    }

    fn filetime_value(time: FILETIME) -> u64 {
        (u64::from(time.dwHighDateTime) << 32) | u64::from(time.dwLowDateTime)
    }

    fn process_image_path(
        process_id: u32,
        expected_creation_filetime: u64,
    ) -> Option<ProcessMetadata> {
        let process =
            unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }.ok()?;
        let _process = ProcessHandle(process);

        let mut created = FILETIME::default();
        let mut exited = FILETIME::default();
        let mut kernel = FILETIME::default();
        let mut user = FILETIME::default();
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) }
            .ok()?;
        let creation_filetime = filetime_value(created);
        if creation_filetime != expected_creation_filetime {
            return None;
        }

        let mut buffer = vec![0_u16; 512];
        loop {
            let mut length = u32::try_from(buffer.len()).ok()?;
            let result = unsafe {
                QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_FORMAT(0),
                    PWSTR(buffer.as_mut_ptr()),
                    &mut length,
                )
            };
            match result {
                Ok(()) if length > 0 && (length as usize) < buffer.len() => {
                    let length = length as usize;
                    if buffer[length] != 0 || buffer[..length].contains(&0) {
                        return None;
                    }
                    buffer.truncate(length);
                    let executable_path = PathBuf::from(std::ffi::OsString::from_wide(&buffer));
                    if !executable_path.is_absolute() {
                        return None;
                    }
                    return Some(ProcessMetadata {
                        creation_filetime,
                        executable_path,
                    });
                }
                Ok(()) if buffer.len() < MAX_FILE_LOCK_PATH_UTF16_UNITS => {
                    let next = buffer
                        .len()
                        .saturating_mul(2)
                        .min(MAX_FILE_LOCK_PATH_UTF16_UNITS);
                    if next <= buffer.len() {
                        return None;
                    }
                    buffer.resize(next, 0);
                }
                Err(error) => {
                    if error_code(&error) != windows::Win32::Foundation::ERROR_INSUFFICIENT_BUFFER.0
                        || buffer.len() >= MAX_FILE_LOCK_PATH_UTF16_UNITS
                    {
                        return None;
                    }
                    let next = buffer
                        .len()
                        .saturating_mul(2)
                        .min(MAX_FILE_LOCK_PATH_UTF16_UNITS);
                    if next <= buffer.len() {
                        return None;
                    }
                    buffer.resize(next, 0);
                }
                Ok(()) => return None,
            }
        }
    }

    struct ProcessHandle(HANDLE);

    impl Drop for ProcessHandle {
        fn drop(&mut self) {
            let _ = unsafe { CloseHandle(self.0) };
        }
    }

    impl RestartManagerApi for NativeApi {
        type Probe = Probe;

        fn probe_file(
            &mut self,
            path: &[u16],
        ) -> Result<(Self::Probe, NativeFileIdentity), FileLockInspectionError> {
            let handle = unsafe {
                CreateFileW(
                    PCWSTR(path.as_ptr()),
                    FILE_READ_ATTRIBUTES.0,
                    FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0),
                    None,
                    OPEN_EXISTING,
                    FILE_FLAGS_AND_ATTRIBUTES(
                        FILE_FLAG_OPEN_REPARSE_POINT.0
                            | windows::Win32::Storage::FileSystem::FILE_FLAG_BACKUP_SEMANTICS.0,
                    ),
                    HANDLE::default(),
                )
            }
            .map_err(|error| FileLockInspectionError::ProbeFailed(error_code(&error)))?;
            let file = unsafe { File::from_raw_handle(handle.0) };
            if unsafe { GetFileType(handle) } != FILE_TYPE_DISK {
                return Err(FileLockInspectionError::NotRegularDiskFile);
            }
            let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
            unsafe {
                GetFileInformationByHandleEx(
                    handle,
                    FileAttributeTagInfo,
                    std::ptr::from_mut(&mut info).cast(),
                    std::mem::size_of::<FILE_ATTRIBUTE_TAG_INFO>() as u32,
                )
            }
            .map_err(|error| FileLockInspectionError::ProbeFailed(error_code(&error)))?;
            if info.FileAttributes & (FILE_ATTRIBUTE_DIRECTORY.0 | FILE_ATTRIBUTE_REPARSE_POINT.0)
                != 0
            {
                return Err(FileLockInspectionError::NotRegularDiskFile);
            }
            let identity =
                crate::file_operations::native_file_identity(&file).map_err(|error| {
                    FileLockInspectionError::ProbeFailed(
                        error
                            .raw_os_error()
                            .and_then(|code| u32::try_from(code).ok())
                            .unwrap_or(1),
                    )
                })?;
            Ok((Probe(file), identity))
        }

        fn start_session(&mut self) -> Result<u32, u32> {
            let mut session = 0_u32;
            let mut key = vec![0_u16; CCH_RM_SESSION_KEY as usize + 1];
            let result = unsafe { RmStartSession(&mut session, 0, PWSTR(key.as_mut_ptr())) };
            if result.0 == 0 {
                Ok(session)
            } else {
                Err(result.0)
            }
        }

        fn register_file(&mut self, session: u32, path: &[u16]) -> Result<(), u32> {
            let paths = [PCWSTR(path.as_ptr())];
            let result = unsafe { RmRegisterResources(session, Some(&paths), None, None) };
            if result.0 == 0 { Ok(()) } else { Err(result.0) }
        }

        fn get_list(
            &mut self,
            session: u32,
            output: &mut [RawProcessRecord],
        ) -> Result<ListCall, u32> {
            let mut needed = 0_u32;
            let mut count = u32::try_from(output.len()).map_err(|_| u32::MAX)?;
            let mut reboot_reasons = 0_u32;
            let mut native = vec![RM_PROCESS_INFO::default(); output.len()];
            let result = unsafe {
                RmGetList(
                    session,
                    &mut needed,
                    &mut count,
                    (!native.is_empty()).then_some(native.as_mut_ptr()),
                    &mut reboot_reasons,
                )
            };
            if result == windows::Win32::Foundation::ERROR_MORE_DATA {
                return Ok(ListCall::MoreData {
                    needed: needed as usize,
                });
            }
            if result.0 != 0 {
                return Err(result.0);
            }
            let reported = count as usize;
            if reported > output.len() {
                return Ok(ListCall::Complete {
                    reported_count: reported,
                });
            }
            for (destination, source) in output.iter_mut().zip(native.into_iter()).take(reported) {
                let app_end = source
                    .strAppName
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(source.strAppName.len());
                let service_end = source
                    .strServiceShortName
                    .iter()
                    .position(|unit| *unit == 0)
                    .unwrap_or(source.strServiceShortName.len());
                let application_name = String::from_utf16_lossy(&source.strAppName[..app_end]);
                let service_name =
                    String::from_utf16_lossy(&source.strServiceShortName[..service_end]);
                *destination = RawProcessRecord {
                    process_id: source.Process.dwProcessId,
                    creation_filetime: filetime_value(source.Process.ProcessStartTime),
                    application_name,
                    service_name,
                };
            }
            Ok(ListCall::Complete {
                reported_count: reported,
            })
        }

        fn process_metadata(
            &mut self,
            process_id: u32,
            expected_creation_filetime: u64,
        ) -> Option<ProcessMetadata> {
            process_image_path(process_id, expected_creation_filetime)
        }

        fn end_session(&mut self, session: u32) -> Result<(), u32> {
            let result = unsafe { RmEndSession(session) };
            if result.0 == 0 { Ok(()) } else { Err(result.0) }
        }
    }

    pub(super) fn inspect(path: &Path) -> Result<FileLockInspection, FileLockInspectionFailure> {
        let wide = encode_path(path).map_err(super::failure)?;
        let mut api = NativeApi;
        let (identity, processes) = super::inspect_with_api(&mut api, &wide)?;
        Ok(FileLockInspection {
            path: path.to_path_buf(),
            identity,
            inspected_at: SystemTime::now(),
            processes,
        })
    }
}

#[cfg(windows)]
pub fn inspect_file(path: &Path) -> Result<FileLockInspection, FileLockInspectionFailure> {
    native::inspect(path)
}

#[cfg(not(windows))]
pub fn inspect_file(_path: &Path) -> Result<FileLockInspection, FileLockInspectionFailure> {
    Err(failure(FileLockInspectionError::UnsupportedPlatform))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{HashMap, VecDeque};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    const TEST_PATH: &[u16] = &[
        b'C' as u16,
        b':' as u16,
        b'\\' as u16,
        b't' as u16,
        b'm' as u16,
        b'p' as u16,
        b'\\' as u16,
        b'f' as u16,
        b'i' as u16,
        b'l' as u16,
        b'e' as u16,
        0,
    ];

    enum ScriptedList {
        MoreData(usize),
        Complete(usize, Vec<RawProcessRecord>),
        Error(u32),
    }

    struct FakeProbe(Arc<AtomicUsize>);

    impl Drop for FakeProbe {
        fn drop(&mut self) {
            self.0.fetch_sub(1, Ordering::AcqRel);
        }
    }

    struct FakeRestartManager {
        identities: VecDeque<NativeFileIdentity>,
        list: VecDeque<ScriptedList>,
        metadata: HashMap<u32, ProcessMetadata>,
        register_error: Option<u32>,
        end_error: Option<u32>,
        capacities: Vec<usize>,
        active_probes: Arc<AtomicUsize>,
        list_with_probe_open: bool,
        end_calls: usize,
    }

    impl FakeRestartManager {
        fn new(list: impl IntoIterator<Item = ScriptedList>) -> Self {
            Self {
                identities: VecDeque::from([
                    NativeFileIdentity::from_parts(1, [2; 16]),
                    NativeFileIdentity::from_parts(1, [2; 16]),
                ]),
                list: list.into_iter().collect(),
                metadata: HashMap::new(),
                register_error: None,
                end_error: None,
                capacities: Vec::new(),
                active_probes: Arc::new(AtomicUsize::new(0)),
                list_with_probe_open: false,
                end_calls: 0,
            }
        }
    }

    impl RestartManagerApi for FakeRestartManager {
        type Probe = FakeProbe;

        fn probe_file(
            &mut self,
            _path: &[u16],
        ) -> Result<(Self::Probe, NativeFileIdentity), FileLockInspectionError> {
            let identity = self
                .identities
                .pop_front()
                .unwrap_or_else(|| NativeFileIdentity::from_parts(1, [2; 16]));
            self.active_probes.fetch_add(1, Ordering::AcqRel);
            Ok((FakeProbe(Arc::clone(&self.active_probes)), identity))
        }

        fn start_session(&mut self) -> Result<u32, u32> {
            Ok(7)
        }

        fn register_file(&mut self, _session: u32, _path: &[u16]) -> Result<(), u32> {
            self.register_error.map_or(Ok(()), Err)
        }

        fn get_list(
            &mut self,
            _session: u32,
            output: &mut [RawProcessRecord],
        ) -> Result<ListCall, u32> {
            self.capacities.push(output.len());
            self.list_with_probe_open |= self.active_probes.load(Ordering::Acquire) != 0;
            match self
                .list
                .pop_front()
                .expect("scripted Restart Manager list call")
            {
                ScriptedList::MoreData(needed) => Ok(ListCall::MoreData { needed }),
                ScriptedList::Complete(reported, rows) => {
                    for (destination, row) in output.iter_mut().zip(rows) {
                        *destination = row;
                    }
                    Ok(ListCall::Complete {
                        reported_count: reported,
                    })
                }
                ScriptedList::Error(code) => Err(code),
            }
        }

        fn process_metadata(
            &mut self,
            process_id: u32,
            _expected_creation_filetime: u64,
        ) -> Option<ProcessMetadata> {
            self.metadata.get(&process_id).cloned()
        }

        fn end_session(&mut self, session: u32) -> Result<(), u32> {
            assert_eq!(session, 7);
            self.end_calls += 1;
            self.end_error.map_or(Ok(()), Err)
        }
    }

    fn record(process_id: u32, creation_filetime: u64) -> RawProcessRecord {
        RawProcessRecord {
            process_id,
            creation_filetime,
            application_name: format!("app-{process_id}"),
            service_name: String::new(),
        }
    }

    #[test]
    fn file_lock_restart_manager_grows_bounded_list_and_releases_probe_before_enumeration() {
        let mut api = FakeRestartManager::new([
            ScriptedList::MoreData(2),
            ScriptedList::Complete(2, vec![record(11, 100), record(12, 200)]),
        ]);

        let (_, processes) = inspect_with_api(&mut api, TEST_PATH).expect("inspection succeeds");

        assert_eq!(api.capacities, [0, 2]);
        assert!(!api.list_with_probe_open);
        assert_eq!(api.end_calls, 1);
        assert_eq!(processes.len(), 2);
        assert_eq!(processes[0].process_id, 11);
    }

    #[test]
    fn file_lock_restart_manager_empty_list_is_successful_and_cleanup_is_once() {
        let mut api = FakeRestartManager::new([ScriptedList::Complete(0, Vec::new())]);

        let (_, processes) = inspect_with_api(&mut api, TEST_PATH).expect("empty is success");

        assert!(processes.is_empty());
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_retains_register_and_cleanup_errors() {
        let mut api = FakeRestartManager::new([]);
        api.register_error = Some(5);
        api.end_error = Some(87);

        let error = inspect_with_api(&mut api, TEST_PATH).expect_err("registration fails");

        assert_eq!(
            error.error,
            FileLockInspectionError::RegisterResourceFailed(5)
        );
        assert_eq!(error.cleanup_error, Some(87));
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_lists_fail_with_exactly_once_cleanup() {
        let mut api = FakeRestartManager::new([ScriptedList::Error(123)]);
        api.end_error = Some(87);

        let error = inspect_with_api(&mut api, TEST_PATH).expect_err("list fails");

        assert_eq!(error.error, FileLockInspectionError::ListFailed(123));
        assert_eq!(error.cleanup_error, Some(87));
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_reports_cleanup_only_failure() {
        let mut api = FakeRestartManager::new([ScriptedList::Complete(0, Vec::new())]);
        api.end_error = Some(87);

        let error = inspect_with_api(&mut api, TEST_PATH).expect_err("cleanup fails");

        assert_eq!(
            error.error,
            FileLockInspectionError::SessionCleanupFailed(87)
        );
        assert_eq!(error.cleanup_error, None);
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_rejects_over_limit_and_malformed_counts() {
        let mut too_many =
            FakeRestartManager::new([ScriptedList::MoreData(MAX_REPORTED_FILE_USERS + 1)]);
        let error = inspect_with_api(&mut too_many, TEST_PATH).expect_err("over limit fails");
        assert_eq!(
            error.error,
            FileLockInspectionError::TooManyProcesses(MAX_REPORTED_FILE_USERS + 1)
        );
        assert_eq!(too_many.end_calls, 1);

        let mut malformed = FakeRestartManager::new([ScriptedList::Complete(1, Vec::new())]);
        let error = inspect_with_api(&mut malformed, TEST_PATH).expect_err("bad count fails");
        assert_eq!(
            error.error,
            FileLockInspectionError::MalformedProcessCount {
                reported: 1,
                capacity: 0,
            }
        );
        assert_eq!(malformed.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_retries_are_bounded_when_list_keeps_growing() {
        let mut api = FakeRestartManager::new(
            (0..MAX_RM_LIST_ATTEMPTS).map(|index| ScriptedList::MoreData(1_usize << index)),
        );

        let error = inspect_with_api(&mut api, TEST_PATH).expect_err("changing list is bounded");

        assert_eq!(
            error.error,
            FileLockInspectionError::ProcessListDidNotStabilize
        );
        assert_eq!(api.capacities, [0, 1, 2, 4]);
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_omits_unavailable_or_reused_pid_metadata() {
        let mut api = FakeRestartManager::new([
            ScriptedList::MoreData(2),
            ScriptedList::Complete(2, vec![record(11, 100), record(12, 200)]),
        ]);
        api.metadata.insert(
            12,
            ProcessMetadata {
                creation_filetime: 201,
                executable_path: PathBuf::from(r"C:\app\reused.exe"),
            },
        );

        let (_, processes) = inspect_with_api(&mut api, TEST_PATH).expect("inspection succeeds");

        assert_eq!(processes[0].executable_path, None);
        assert_eq!(processes[1].executable_path, None);
        assert_eq!(processes[0].creation_filetime, 100);
        assert_eq!(processes[1].creation_filetime, 200);
    }

    #[test]
    fn file_lock_restart_manager_rejects_changed_file_identity() {
        let mut api = FakeRestartManager::new([ScriptedList::Complete(0, Vec::new())]);
        api.identities = VecDeque::from([
            NativeFileIdentity::from_parts(1, [2; 16]),
            NativeFileIdentity::from_parts(1, [3; 16]),
        ]);

        let error = inspect_with_api(&mut api, TEST_PATH).expect_err("replacement detected");

        assert_eq!(
            error.error,
            FileLockInspectionError::FileChangedDuringInspection
        );
        assert!(api.capacities.is_empty());
        assert_eq!(api.end_calls, 1);

        let mut unavailable_identity = FakeRestartManager::new([]);
        unavailable_identity.identities =
            VecDeque::from([NativeFileIdentity::from_parts(1, [0; 16])]);
        let error = inspect_with_api(&mut unavailable_identity, TEST_PATH)
            .expect_err("an unavailable file ID cannot prove stable identity");
        assert_eq!(
            error.error,
            FileLockInspectionError::UnsupportedFileIdentity
        );
        assert_eq!(unavailable_identity.end_calls, 0);
    }

    #[test]
    fn file_lock_restart_manager_rechecks_identity_after_process_enumeration() {
        let mut api = FakeRestartManager::new([ScriptedList::Complete(0, Vec::new())]);
        api.identities = VecDeque::from([
            NativeFileIdentity::from_parts(1, [2; 16]),
            NativeFileIdentity::from_parts(1, [2; 16]),
            NativeFileIdentity::from_parts(1, [3; 16]),
        ]);

        let error =
            inspect_with_api(&mut api, TEST_PATH).expect_err("post-list replacement detected");

        assert_eq!(
            error.error,
            FileLockInspectionError::FileChangedDuringInspection
        );
        assert_eq!(api.capacities, [0]);
        assert_eq!(api.end_calls, 1);
    }

    #[test]
    fn file_lock_restart_manager_validates_absolute_bounded_nul_path() {
        assert!(validate_resource_path(TEST_PATH).is_ok());
        assert_eq!(
            validate_resource_path(&[b'f' as u16, 0]).unwrap_err(),
            FileLockInspectionError::InvalidPath
        );
        assert_eq!(
            validate_resource_path(&[b'C' as u16, b':' as u16, b'\\' as u16, 0, 0]).unwrap_err(),
            FileLockInspectionError::InvalidPath
        );
        assert_eq!(
            validate_resource_path(&vec![b'C' as u16; MAX_FILE_LOCK_PATH_UTF16_UNITS + 1])
                .unwrap_err(),
            FileLockInspectionError::PathTooLong
        );

        let wide = |value: &str| {
            let mut units: Vec<u16> = value.encode_utf16().collect();
            units.push(0);
            units
        };
        for valid in [
            r"C:\root\file.txt",
            r"\\server\share\file.txt",
            r"\\?\C:\root\file.txt",
            r"\\?\UNC\server\share\file.txt",
        ] {
            assert!(validate_resource_path(&wide(valid)).is_ok(), "{valid}");
        }
        for refused in [
            r"\\.\pipe\name",
            r"\\server\pipe\name",
            r"\\?\UNC\server\mailslot\name",
            r"\\server\IPC$\name",
            r"C:\root\file.txt:stream",
        ] {
            assert_eq!(
                validate_resource_path(&wide(refused)).unwrap_err(),
                FileLockInspectionError::InvalidPath,
                "{refused}"
            );
        }
    }

    #[cfg(windows)]
    mod native_fixture {
        use super::*;
        use std::fs::{self, OpenOptions};
        use std::io::{BufRead, BufReader, Read, Write};
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        use std::os::windows::io::AsRawHandle;
        use std::process::{Child, ChildStdin, Command, Stdio};
        use std::sync::mpsc::{self, Receiver};
        use std::thread;
        use std::time::{Duration, Instant};
        use windows::Win32::Foundation::{FILETIME, HANDLE};
        use windows::Win32::Storage::FileSystem::{
            FILE_SHARE_DELETE, FILE_SHARE_MODE, FILE_SHARE_READ, FILE_SHARE_WRITE,
        };
        use windows::Win32::System::Threading::GetProcessTimes;

        const HOLDER_PATH_ENV: &str = "MULTI_LAUNCHER_FILE_LOCK_HOLDER_PATH";
        const HOLDER_MARKER_ENV: &str = "MULTI_LAUNCHER_FILE_LOCK_HOLDER_MARKER";

        fn process_creation_filetime(child: &Child) -> Option<u64> {
            let handle = HANDLE(child.as_raw_handle());
            let mut created = FILETIME::default();
            let mut exited = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            unsafe { GetProcessTimes(handle, &mut created, &mut exited, &mut kernel, &mut user) }
                .ok()?;
            Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
        }

        struct HolderChild {
            child: Child,
            input: Option<ChildStdin>,
            acknowledgements: Receiver<String>,
            reader: Option<thread::JoinHandle<()>>,
        }

        impl HolderChild {
            fn launch(path: &Path, marker: &str) -> Self {
                let executable = std::env::current_exe().expect("current test executable");
                let (ack_tx, ack_rx) = mpsc::channel();
                let child = Command::new(executable)
                    .args([
                        "--exact",
                        "file_lock::windows::tests::native_fixture::file_lock_disposable_holder_helper",
                        "--ignored",
                        "--nocapture",
                    ])
                    .env(HOLDER_PATH_ENV, path.as_os_str())
                    .env(HOLDER_MARKER_ENV, marker)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .expect("launch ignored disposable holder test");
                // Establish ownership before pipe extraction and reader thread
                // creation so any setup failure cleans up only this child.
                let mut holder = Self {
                    child,
                    input: None,
                    acknowledgements: ack_rx,
                    reader: None,
                };
                holder.input = Some(holder.child.stdin.take().expect("helper stdin"));
                let output = holder.child.stdout.take().expect("helper stdout");
                let reader = thread::spawn(move || {
                    let mut output = BufReader::new(output);
                    let mut line = String::new();
                    loop {
                        line.clear();
                        match output.read_line(&mut line) {
                            Ok(0) | Err(_) => break,
                            Ok(_) => {
                                if ack_tx.send(line.trim().to_owned()).is_err() {
                                    break;
                                }
                            }
                        }
                    }
                });
                holder.reader = Some(reader);
                holder
            }

            fn wait_for(&self, expected: &str, timeout: Duration) {
                let deadline = Instant::now() + timeout;
                loop {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    assert!(
                        !remaining.is_zero(),
                        "timed out waiting for helper {expected}"
                    );
                    let line = self
                        .acknowledgements
                        .recv_timeout(remaining)
                        .expect("helper acknowledgement channel");
                    if line == expected {
                        return;
                    }
                }
            }

            fn send(&mut self, byte: u8) {
                let input = self.input.as_mut().expect("helper input remains open");
                input.write_all(&[byte]).expect("write helper control byte");
                input.flush().expect("flush helper control byte");
            }

            fn wait_exit(&mut self, timeout: Duration) -> std::process::ExitStatus {
                let deadline = Instant::now() + timeout;
                loop {
                    if let Some(status) = self.child.try_wait().expect("poll helper exit") {
                        if let Some(reader) = self.reader.take() {
                            let _ = reader.join();
                        }
                        return status;
                    }
                    if Instant::now() >= deadline {
                        panic!("disposable holder did not exit within {timeout:?}");
                    }
                    thread::yield_now();
                }
            }
        }

        impl Drop for HolderChild {
            fn drop(&mut self) {
                if let Some(input) = self.input.as_mut() {
                    let _ = input.write_all(b"RQ");
                    let _ = input.flush();
                }
                let deadline = Instant::now() + Duration::from_secs(1);
                loop {
                    match self.child.try_wait() {
                        Ok(Some(_)) => break,
                        Ok(None) if Instant::now() < deadline => thread::yield_now(),
                        _ => {
                            let _ = self.child.kill();
                            let _ = self.child.wait();
                            break;
                        }
                    }
                }
                if let Some(reader) = self.reader.take() {
                    let _ = reader.join();
                }
            }
        }

        #[test]
        fn native_reports_disposable_holder_then_release() {
            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("productivity_expansion");
            fs::create_dir_all(&target).expect("create isolated fixture root");
            let directory = tempfile::Builder::new()
                .prefix("file-lock-users-")
                .tempdir_in(&target)
                .expect("create isolated file-lock fixture directory");
            let path = directory.path().join("locked.bin");
            fs::write(&path, b"disposable file-lock fixture").expect("create fixture file");
            assert!(confined_disposable_fixture_path(&path).is_some());

            let marker = format!(
                "M14_FILE_LOCK_{}",
                SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .expect("clock after Unix epoch")
                    .as_nanos()
            );
            let mut holder = HolderChild::launch(&path, &marker);
            holder.wait_for(
                &format!("M14-HOLDER-READY:{marker}"),
                Duration::from_secs(10),
            );
            let process_id = holder.child.id();
            let creation_filetime = process_creation_filetime(&holder.child)
                .expect("query exact creation FILETIME from owned child handle");

            let denied = OpenOptions::new()
                .write(true)
                .share_mode(
                    FILE_SHARE_MODE(FILE_SHARE_READ.0 | FILE_SHARE_WRITE.0 | FILE_SHARE_DELETE.0).0,
                )
                .open(&path)
                .expect_err("holder denies an incompatible data write open");
            assert_eq!(
                denied.raw_os_error(),
                Some(windows::Win32::Foundation::ERROR_SHARING_VIOLATION.0 as i32)
            );

            let first = inspect_file(&path).expect("inspect live holder");
            assert!(
                first.processes.iter().any(|process| {
                    process.process_id == process_id
                        && process.creation_filetime == creation_filetime
                }),
                "Restart Manager omitted fixture PID {process_id} with creation FILETIME {creation_filetime}; returned {} rows",
                first.processes.len()
            );

            holder.send(b'R');
            holder.wait_for(
                &format!("M14-HOLDER-RELEASED:{marker}"),
                Duration::from_secs(10),
            );
            assert!(
                holder
                    .child
                    .try_wait()
                    .expect("check holder remains alive")
                    .is_none()
            );
            let second = inspect_file(&path).expect("inspect after holder closes the file");
            assert!(!second.processes.iter().any(|process| {
                process.process_id == process_id && process.creation_filetime == creation_filetime
            }));

            holder.send(b'Q');
            assert!(holder.wait_exit(Duration::from_secs(10)).success());
        }

        #[test]
        #[ignore = "launched only by native_reports_disposable_holder_then_release"]
        fn file_lock_disposable_holder_helper() {
            let path = std::env::var_os(HOLDER_PATH_ENV).expect("parent passes fixture path");
            let marker = std::env::var(HOLDER_MARKER_ENV).expect("parent passes private marker");
            let nonce = marker.strip_prefix("M14_FILE_LOCK_").unwrap_or_default();
            assert!(!nonce.is_empty() && nonce.bytes().all(|byte| byte.is_ascii_digit()));
            let path = PathBuf::from(path);
            let path = confined_disposable_fixture_path(&path)
                .expect("helper file is confined to its disposable target directory");
            let file = OpenOptions::new()
                .write(true)
                .share_mode(FILE_SHARE_READ.0)
                .open(&path)
                .expect("open holder file with read-only sharing");
            print!("\nM14-HOLDER-READY:{marker}\n");
            std::io::stdout().flush().expect("flush ready marker");
            let mut input = std::io::stdin().lock();
            let mut control = [0_u8; 1];
            input
                .read_exact(&mut control)
                .expect("receive release command");
            assert_eq!(control[0], b'R');
            drop(file);
            print!("\nM14-HOLDER-RELEASED:{marker}\n");
            std::io::stdout().flush().expect("flush released marker");
            input
                .read_exact(&mut control)
                .expect("receive exit command");
            assert_eq!(control[0], b'Q');
        }

        fn confined_disposable_fixture_path(path: &Path) -> Option<PathBuf> {
            let target = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("target")
                .join("productivity_expansion");
            let target = fs::canonicalize(target).ok()?;
            let path = fs::canonicalize(path).ok()?;
            let parent = path.parent()?;
            let parent_name = parent.file_name()?.to_str()?;
            if parent.parent()? != target
                || !path.starts_with(&target)
                || !path.file_name().is_some_and(|name| name == "locked.bin")
                || !parent_name.starts_with("file-lock-users-")
            {
                return None;
            }
            let metadata = fs::symlink_metadata(&path).ok()?;
            if !metadata.file_type().is_file()
                || metadata.file_attributes()
                    & windows::Win32::Storage::FileSystem::FILE_ATTRIBUTE_REPARSE_POINT.0
                    != 0
            {
                return None;
            }
            Some(path)
        }
    }
}
