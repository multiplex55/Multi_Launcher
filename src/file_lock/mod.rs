mod session;
mod windows;

pub use session::{
    FileLockSession, FileLockSessionError, FileLockSessionStatus, FileLockSessionView,
};
pub use windows::{
    FileLockInspection, FileLockInspectionError, FileLockInspectionFailure, FileLockProcess,
    MAX_FILE_LOCK_PATH_UTF16_UNITS, MAX_REPORTED_FILE_USERS, ValidatedFileLockExecutable,
    inspect_file, validate_file_path, validate_reported_executable,
};
