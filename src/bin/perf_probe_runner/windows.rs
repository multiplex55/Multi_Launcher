use multi_launcher::perf_harness::{
    ProbeFailure, ProbeStatus, ProcessContainment, ProcessManifest, Scenario,
};
use sha2::{Digest, Sha256};
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read};
use std::os::windows::ffi::OsStrExt;
use std::os::windows::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use windows::Win32::Foundation::{
    BOOL, CloseHandle, FILETIME, HANDLE, HANDLE_FLAG_INHERIT, HANDLE_FLAGS, SetHandleInformation,
    WAIT_FAILED, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::Security::SECURITY_ATTRIBUTES;
use windows::Win32::Storage::FileSystem::ReadFile;
use windows::Win32::System::JobObjects::{
    AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
    JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, JOBOBJECT_EXTENDED_LIMIT_INFORMATION,
    JobObjectBasicAccountingInformation, JobObjectExtendedLimitInformation,
    QueryInformationJobObject, SetInformationJobObject, TerminateJobObject,
};
use windows::Win32::System::Pipes::{CreatePipe, PeekNamedPipe};
use windows::Win32::System::Threading::{
    CREATE_NO_WINDOW, CREATE_SUSPENDED, CreateProcessW, DeleteProcThreadAttributeList,
    EXTENDED_STARTUPINFO_PRESENT, GetExitCodeProcess, GetProcessTimes,
    InitializeProcThreadAttributeList, PROC_THREAD_ATTRIBUTE_HANDLE_LIST, PROCESS_CREATION_FLAGS,
    PROCESS_INFORMATION, ResumeThread, STARTF_USESTDHANDLES, STARTUPINFOEXW, TerminateProcess,
    UpdateProcThreadAttribute, WaitForSingleObject,
};
use windows::core::{Error as WindowsError, PCWSTR, PWSTR};

const ERROR_ACCESS_DENIED_CODE: u32 = 5;
const ERROR_INSUFFICIENT_BUFFER_CODE: u32 = 122;
const MAX_CAPTURE_BYTES: usize = 30 * 1024 * 1024;
const ROOT_WAIT_SLICE: Duration = Duration::from_millis(100);
const CLEANUP_WAIT: Duration = Duration::from_secs(5);
const NORMAL_TREE_GRACE: Duration = Duration::from_millis(200);

pub enum LaunchTarget {
    Probe(&'static str, Vec<String>),
    RunnerFixture(Vec<String>),
}

impl LaunchTarget {
    fn basename(&self) -> &'static str {
        match self {
            Self::Probe(name, _) => name,
            Self::RunnerFixture(_) => "perf_probe_runner.exe",
        }
    }

    fn arguments(&self) -> &[String] {
        match self {
            Self::Probe(_, arguments) | Self::RunnerFixture(arguments) => arguments,
        }
    }

    fn is_fixture(&self) -> bool {
        matches!(self, Self::RunnerFixture(_))
    }
}

pub struct ChildOutcome {
    pub manifest: ProcessManifest,
    pub stdout: Vec<u8>,
    pub stderr: Vec<u8>,
    pub output_truncated: bool,
    pub elapsed: Duration,
    pub process_cpu_ms: Option<f64>,
    pub process_cpu_reason: Option<String>,
    pub failure: Option<ProbeFailure>,
}

#[derive(Clone)]
struct ApiFailure {
    status: ProbeStatus,
    reason: String,
}

impl ApiFailure {
    fn from_windows(api: &str, error: WindowsError) -> Self {
        let hresult = error.code().0 as u32;
        let code = hresult_to_win32(hresult);
        let status = if code == Some(ERROR_ACCESS_DENIED_CODE) {
            ProbeStatus::BlockedEnvironment
        } else {
            ProbeStatus::Error
        };
        let reason = match code {
            Some(code) => format!("{api} failed with Win32 error {code} (HRESULT 0x{hresult:08X})"),
            None => format!("{api} failed with HRESULT 0x{hresult:08X}"),
        };
        Self { status, reason }
    }

    fn from_win32(api: &str, code: u32) -> Self {
        let status = if code == ERROR_ACCESS_DENIED_CODE {
            ProbeStatus::BlockedEnvironment
        } else {
            ProbeStatus::Error
        };
        Self {
            status,
            reason: format!("{api} failed with Win32 error {code}"),
        }
    }

    fn from_io(api: &str, error: io::Error) -> Self {
        match error.raw_os_error() {
            Some(code) if code >= 0 => Self::from_win32(api, code as u32),
            _ => Self {
                status: ProbeStatus::Error,
                reason: format!("{api} failed: {error}"),
            },
        }
    }
}

fn hresult_to_win32(hresult: u32) -> Option<u32> {
    (hresult & 0xFFFF_0000 == 0x8007_0000).then_some(hresult & 0xFFFF)
}

pub fn run_owned(
    workspace: &Path,
    target: LaunchTarget,
    argv_redacted: Vec<String>,
    scenario: Scenario,
    _warmups: usize,
    _samples: usize,
    timeout_seconds: u64,
) -> ChildOutcome {
    let started = Instant::now();
    let mut manifest = ProcessManifest::not_attempted("waiting for checked Job Object launch");
    manifest.executable_basename = Some(target.basename().to_string());
    manifest.argv_redacted = argv_redacted;
    manifest.job_created = Some(false);
    manifest.job_assigned = Some(false);
    manifest.resumed = Some(false);
    manifest.cleanup_verified = Some(true);
    manifest.cleanup_reason = Some("no child process was created".into());

    let layout = match RunnerLayout::discover(workspace) {
        Ok(layout) => layout,
        Err(error) => return failed_before_spawn(manifest, error, started.elapsed()),
    };
    let executable = if target.is_fixture() {
        layout.runner_executable.clone()
    } else {
        layout.profile_dir.join(target.basename())
    };
    let executable = match validate_executable(&layout, &executable, target.basename()) {
        Ok(path) => path,
        Err(error) => return failed_before_spawn(manifest, error, started.elapsed()),
    };
    match hash_file(&executable) {
        Ok(hash) => manifest.executable_sha256 = Some(hash),
        Err(error) => return failed_before_spawn(manifest, error, started.elapsed()),
    }

    let job_handle = match create_configured_job() {
        Ok(job) => job,
        Err(error) => {
            manifest.process_containment = containment_for(&error);
            manifest.process_containment_reason = error.reason.clone();
            return failed_before_spawn(manifest, error, started.elapsed());
        }
    };
    manifest.job_created = Some(true);
    manifest.process_containment = ProcessContainment::Failed;
    manifest.process_containment_reason = "job created; child launch has not completed".into();

    let mut pipes = match ChildPipes::create() {
        Ok(pipes) => pipes,
        Err(error) => {
            manifest.process_containment = containment_for(&error);
            manifest.process_containment_reason = error.reason.clone();
            return failed_before_spawn(manifest, error, started.elapsed());
        }
    };
    let inherited_handles = pipes.inherited_handles();
    let mut attribute_list = match StartupAttributeList::new(&inherited_handles) {
        Ok(list) => list,
        Err(error) => {
            manifest.process_containment = containment_for(&error);
            manifest.process_containment_reason = error.reason.clone();
            return failed_before_spawn(manifest, error, started.elapsed());
        }
    };

    let mut startup = STARTUPINFOEXW::default();
    startup.StartupInfo.cb = std::mem::size_of::<STARTUPINFOEXW>() as u32;
    startup.StartupInfo.dwFlags = STARTF_USESTDHANDLES;
    startup.StartupInfo.hStdInput = pipes.stdin.read().raw();
    startup.StartupInfo.hStdOutput = pipes.stdout.write().raw();
    startup.StartupInfo.hStdError = pipes.stderr.write().raw();
    startup.lpAttributeList = attribute_list.as_ptr();

    let mut process_info = PROCESS_INFORMATION::default();
    let mut command_line = match build_command_line(&executable, target.arguments()) {
        Ok(command_line) => command_line,
        Err(error) => return failed_before_spawn(manifest, error, started.elapsed()),
    };
    let executable_wide = wide_nul(executable.as_os_str());
    let workspace_wide = wide_nul(layout.workspace.as_os_str());
    let created = unsafe {
        CreateProcessW(
            PCWSTR::from_raw(executable_wide.as_ptr()),
            PWSTR::from_raw(command_line.as_mut_ptr()),
            None,
            None,
            BOOL(1),
            PROCESS_CREATION_FLAGS(
                CREATE_SUSPENDED.0 | EXTENDED_STARTUPINFO_PRESENT.0 | CREATE_NO_WINDOW.0,
            ),
            None,
            PCWSTR::from_raw(workspace_wide.as_ptr()),
            &startup.StartupInfo,
            &mut process_info,
        )
    };
    if let Err(error) = created {
        let failure = ApiFailure::from_windows("CreateProcessW", error);
        manifest.process_containment = containment_for(&failure);
        manifest.process_containment_reason = failure.reason.clone();
        return failed_before_spawn(manifest, failure, started.elapsed());
    }

    // A successful CreateProcessW call returns both handles. They remain owned
    // until after cleanup and process-time collection on every path below.
    let process = OwnedProcess::from_info(process_info);
    manifest.child_pid = Some(process.pid);
    manifest.cleanup_verified = None;
    manifest.cleanup_reason = Some("process was created; cleanup is pending".into());
    drop(pipes.stdin.read.take());
    drop(pipes.stdin.write.take());
    drop(pipes.stdout.write.take());
    drop(pipes.stderr.write.take());

    let mut captured = CapturedOutput::new();

    let mut failure = None;
    let mut descendant_found = false;
    let mut root_signaled;
    let mut job_cleanup_verified = false;
    let mut timeout = false;

    match unsafe { AssignProcessToJobObject(job_handle.raw(), process.process.raw()) } {
        Ok(()) => {
            manifest.job_assigned = Some(true);
            let resume_result = unsafe { ResumeThread(process.thread.raw()) };
            if resume_result != 1 {
                let error = if resume_result == u32::MAX {
                    ApiFailure::from_win32("ResumeThread", unsafe {
                        windows::Win32::Foundation::GetLastError().0
                    })
                } else {
                    ApiFailure {
                        status: ProbeStatus::Error,
                        reason: format!(
                            "ResumeThread returned unexpected previous suspend count {resume_result}; expected 1"
                        ),
                    }
                };
                failure = Some(ProbeFailure::new(error.status, error.reason.clone()));
                let cleanup = terminate_and_verify(&job_handle, &process, &pipes, &mut captured);
                root_signaled = cleanup.root_signaled;
                job_cleanup_verified = cleanup.empty_verified;
                if let Some(cleanup_error) = cleanup.failure {
                    failure = Some(merge_failure(failure.take(), cleanup_error));
                }
            } else {
                manifest.resumed = Some(true);
                match wait_for_root(
                    &process,
                    Duration::from_secs(timeout_seconds),
                    &pipes,
                    &mut captured,
                ) {
                    Ok(true) => {
                        root_signaled = true;
                        let root_accounting = match query_job_accounting(&job_handle) {
                            Ok(accounting) => Some(accounting),
                            Err(error) => {
                                failure =
                                    Some(ProbeFailure::new(error.status, error.reason.clone()));
                                if let Err(cleanup_error) =
                                    unsafe { TerminateJobObject(job_handle.raw(), 1) }
                                {
                                    failure = Some(merge_failure(
                                        failure.take(),
                                        ApiFailure::from_windows(
                                            "TerminateJobObject after root accounting query failure",
                                            cleanup_error,
                                        ),
                                    ));
                                }
                                // Do not repeat a denied/failed query. Closing the job below
                                // applies KILL_ON_JOB_CLOSE if termination did not complete.
                                None
                            }
                        };
                        if let Some(accounting) = root_accounting {
                            manifest.job_total_processes = Some(accounting.TotalProcesses);
                            manifest.job_active_processes_at_root_exit =
                                Some(accounting.ActiveProcesses);
                            match wait_for_empty_job(
                                &job_handle,
                                NORMAL_TREE_GRACE,
                                &pipes,
                                &mut captured,
                            ) {
                                Ok(true) => job_cleanup_verified = true,
                                Ok(false) => {
                                    descendant_found = true;
                                    let cleanup = terminate_and_verify(
                                        &job_handle,
                                        &process,
                                        &pipes,
                                        &mut captured,
                                    );
                                    root_signaled |= cleanup.root_signaled;
                                    job_cleanup_verified = cleanup.empty_verified;
                                    if let Some(error) = cleanup.failure {
                                        failure = Some(merge_failure(failure.take(), error));
                                    }
                                }
                                Err(error) => {
                                    failure =
                                        Some(ProbeFailure::new(error.status, error.reason.clone()));
                                    if let Err(cleanup_error) =
                                        unsafe { TerminateJobObject(job_handle.raw(), 1) }
                                    {
                                        failure = Some(merge_failure(
                                            failure.take(),
                                            ApiFailure::from_windows(
                                                "TerminateJobObject after job-empty query failure",
                                                cleanup_error,
                                            ),
                                        ));
                                    }
                                    // Do not repeat the denied/failed query. The job's
                                    // kill-on-close limit remains the final owner cleanup.
                                }
                            }
                        }
                    }
                    Ok(false) => {
                        timeout = true;
                        manifest.deadline_reached = Some(true);
                        let cleanup =
                            terminate_and_verify(&job_handle, &process, &pipes, &mut captured);
                        root_signaled = cleanup.root_signaled;
                        job_cleanup_verified = cleanup.empty_verified;
                        if let Some(error) = cleanup.failure {
                            failure = Some(ProbeFailure::new(
                                error.status,
                                format!("runner deadline reached; cleanup: {}", error.reason),
                            ));
                        } else if !cleanup.empty_verified {
                            failure = Some(ProbeFailure::new(
                                ProbeStatus::Error,
                                "deadline reached; owned job did not verify empty within the cleanup deadline",
                            ));
                        } else {
                            failure = Some(ProbeFailure::timeout(
                                "runner deadline reached; terminated the owned Job Object and verified ActiveProcesses == 0",
                            ));
                        }
                    }
                    Err(error) => {
                        failure = Some(ProbeFailure::new(error.status, error.reason.clone()));
                        let cleanup =
                            terminate_and_verify(&job_handle, &process, &pipes, &mut captured);
                        root_signaled = cleanup.root_signaled;
                        job_cleanup_verified = cleanup.empty_verified;
                        if let Some(cleanup_error) = cleanup.failure {
                            failure = Some(merge_failure(failure.take(), cleanup_error));
                        }
                    }
                }
            }
        }
        Err(error) => {
            let assignment = ApiFailure::from_windows("AssignProcessToJobObject", error);
            // Assignment failure leaves the root suspended and uncontained. It
            // is terminated through its owned process handle and never resumed.
            let terminated = unsafe { TerminateProcess(process.process.raw(), 1) };
            let waited = wait_for_root(&process, CLEANUP_WAIT, &pipes, &mut captured);
            root_signaled = matches!(waited, Ok(true));
            let cleanup_error = match (terminated, waited) {
                (Ok(()), Ok(true)) => None,
                (Err(error), _) => Some(ApiFailure::from_windows(
                    "TerminateProcess after assignment failure",
                    error,
                )),
                (_, Err(error)) => Some(error),
                (_, Ok(false)) => Some(ApiFailure {
                    status: ProbeStatus::Error,
                    reason:
                        "suspended root remained alive after assignment failure cleanup deadline"
                            .into(),
                }),
            };
            job_cleanup_verified = root_signaled && cleanup_error.is_none();
            let mut assignment_failure =
                ProbeFailure::new(assignment.status, assignment.reason.clone());
            if let Some(cleanup) = cleanup_error {
                assignment_failure = merge_failure(Some(assignment_failure), cleanup);
            }
            failure = Some(assignment_failure);
            manifest.job_assigned = Some(false);
        }
    }

    let exit_code = if root_signaled {
        match unsafe { get_exit_code(&process.process) } {
            Ok(code) => Some(code as i32),
            Err(error) => {
                failure = Some(merge_failure(failure.take(), error));
                None
            }
        }
    } else {
        None
    };
    manifest.exit_code = exit_code;
    manifest.deadline_reached.get_or_insert(timeout);

    let (cpu_ms, cpu_reason, creation_time) = if root_signaled {
        match process_times(&process.process) {
            Ok((created, cpu)) => (Some(cpu), None, Some(created)),
            Err(error) => {
                if error.status == ProbeStatus::BlockedEnvironment {
                    failure = Some(merge_failure(failure.take(), error.clone()));
                }
                (None, Some(error.reason), None)
            }
        }
    } else {
        (
            None,
            Some("root process handle was not signaled before cleanup verification ended".into()),
            None,
        )
    };
    manifest.child_creation_time_100ns = creation_time;
    manifest.child_creation_time_reason = creation_time.is_none().then(|| {
        cpu_reason
            .clone()
            .unwrap_or_else(|| "GetProcessTimes did not return a creation time".into())
    });
    manifest.root_process_cpu_ms = cpu_ms;
    manifest.root_process_cpu_reason = cpu_reason.clone();

    if manifest.job_assigned == Some(true) {
        manifest.cleanup_verified = Some(job_cleanup_verified);
        manifest.cleanup_reason = Some(if job_cleanup_verified {
            if descendant_found {
                "root exited with contained descendants; runner terminated the owned tree and verified ActiveProcesses == 0".into()
            } else if timeout {
                "deadline reached; runner terminated the owned tree and verified ActiveProcesses == 0".into()
            } else {
                "owned job reached ActiveProcesses == 0".into()
            }
        } else {
            "owned job cleanup could not be verified; KILL_ON_JOB_CLOSE remains enabled".into()
        });
        if job_cleanup_verified {
            manifest.process_containment = ProcessContainment::JobObjectVerified;
            manifest.process_containment_reason = "root was assigned while suspended, resumed only after assignment, and job cleanup was verified".into();
        }
    } else {
        manifest.cleanup_verified = Some(job_cleanup_verified);
        manifest.cleanup_reason = Some(if job_cleanup_verified {
            "assignment failed; suspended root was terminated and its process handle signaled"
                .into()
        } else {
            "assignment failed; suspended-root termination could not be verified".into()
        });
        manifest.process_containment = containment_for_failure(failure.as_ref());
        manifest.process_containment_reason = failure
            .as_ref()
            .map(|failure| failure.reason.clone())
            .unwrap_or_else(|| "process assignment did not succeed".into());
    }

    // Closing the owned Job Object applies KILL_ON_JOB_CLOSE on every path,
    // including query or termination failures. The final pipe drain below is
    // polling-only and has a fixed deadline, so a surviving writer cannot hang
    // the runner while cleanup is uncertain.
    drop(job_handle);
    if let Err(error) = drain_until_quiet(&pipes, &mut captured, Duration::from_millis(250)) {
        failure = Some(merge_failure(failure.take(), error));
    }
    let stdout = std::mem::take(&mut captured.stdout);
    let stderr = std::mem::take(&mut captured.stderr);
    let output_truncated = captured.truncated;

    if failure.is_none() {
        if descendant_found {
            failure = Some(ProbeFailure::error(
                "child exited while its Job Object still contained descendants; the tree was terminated and verified empty",
            ));
        } else if let Some(code) = exit_code {
            if code != 0 {
                failure = Some(ProbeFailure::error(format!(
                    "child exited with code {code}"
                )));
            }
        } else {
            failure = Some(ProbeFailure::error(
                "child exit code was unavailable after its process handle signaled",
            ));
        }
    }

    // The process handle stays alive through identity and CPU queries.
    drop(process);
    drop(pipes);
    let _ = scenario;
    ChildOutcome {
        manifest,
        stdout,
        stderr,
        output_truncated,
        elapsed: started.elapsed(),
        process_cpu_ms: cpu_ms,
        process_cpu_reason: cpu_reason,
        failure,
    }
}

fn failed_before_spawn(
    mut manifest: ProcessManifest,
    error: ApiFailure,
    elapsed: Duration,
) -> ChildOutcome {
    manifest.cleanup_verified = Some(true);
    manifest.cleanup_reason = Some("no child process was created".into());
    manifest.process_containment = containment_for(&error);
    manifest.process_containment_reason = error.reason.clone();
    ChildOutcome {
        manifest,
        stdout: Vec::new(),
        stderr: Vec::new(),
        output_truncated: false,
        elapsed,
        process_cpu_ms: None,
        process_cpu_reason: Some("no owned process handle was available".into()),
        failure: Some(ProbeFailure::new(error.status, error.reason)),
    }
}

fn containment_for(error: &ApiFailure) -> ProcessContainment {
    if error.status == ProbeStatus::BlockedEnvironment {
        ProcessContainment::BlockedEnvironment
    } else {
        ProcessContainment::Failed
    }
}

fn containment_for_failure(failure: Option<&ProbeFailure>) -> ProcessContainment {
    if failure.is_some_and(|failure| failure.status == ProbeStatus::BlockedEnvironment) {
        ProcessContainment::BlockedEnvironment
    } else {
        ProcessContainment::Failed
    }
}

fn merge_failure(primary: Option<ProbeFailure>, additional: ApiFailure) -> ProbeFailure {
    match primary {
        Some(mut failure) => {
            if additional.status == ProbeStatus::BlockedEnvironment {
                failure.status = ProbeStatus::BlockedEnvironment;
            }
            failure.reason = format!(
                "{}; additional failure: {}",
                failure.reason, additional.reason
            );
            failure
        }
        None => ProbeFailure::new(additional.status, additional.reason),
    }
}

struct RunnerLayout {
    workspace: PathBuf,
    target_dir: PathBuf,
    profile_dir: PathBuf,
    runner_executable: PathBuf,
}

impl RunnerLayout {
    fn discover(requested_workspace: &Path) -> Result<Self, ApiFailure> {
        let current_exe =
            std::env::current_exe().map_err(|error| ApiFailure::from_io("current_exe", error))?;
        let runner_executable = canonical_regular_file(&current_exe, "runner executable")?;
        if runner_executable.file_name().and_then(OsStr::to_str) != Some("perf_probe_runner.exe") {
            return Err(ApiFailure {
                status: ProbeStatus::Error,
                reason: "runner executable name is not perf_probe_runner.exe".into(),
            });
        }
        let profile_dir = runner_executable
            .parent()
            .ok_or_else(|| ApiFailure {
                status: ProbeStatus::Error,
                reason: "runner executable has no parent directory".into(),
            })?
            .to_path_buf();
        let profile = profile_dir
            .file_name()
            .and_then(OsStr::to_str)
            .unwrap_or("");
        if !matches!(profile, "debug" | "fast-dev" | "release") {
            return Err(ApiFailure {
                status: ProbeStatus::Error,
                reason:
                    "runner must be launched from target/debug, target/fast-dev, or target/release"
                        .into(),
            });
        }
        let target_dir = profile_dir
            .parent()
            .ok_or_else(|| ApiFailure {
                status: ProbeStatus::Error,
                reason: "runner profile has no target directory".into(),
            })?
            .to_path_buf();
        if target_dir.file_name().and_then(OsStr::to_str) != Some("target") {
            return Err(ApiFailure {
                status: ProbeStatus::Error,
                reason: "runner is not in the repository target directory".into(),
            });
        }
        let workspace = target_dir
            .parent()
            .ok_or_else(|| ApiFailure {
                status: ProbeStatus::Error,
                reason: "target directory has no workspace parent".into(),
            })?
            .to_path_buf();
        let canonical_workspace = workspace
            .canonicalize()
            .map_err(|error| ApiFailure::from_io("canonicalize workspace", error))?;
        let requested_workspace = requested_workspace
            .canonicalize()
            .map_err(|error| ApiFailure::from_io("canonicalize requested workspace", error))?;
        if canonical_workspace != requested_workspace {
            return Err(ApiFailure {
                status: ProbeStatus::Error,
                reason: "runner workspace does not match the executable's repository".into(),
            });
        }
        for path in [&target_dir, &profile_dir] {
            reject_reparse_directory(path)?;
        }
        Ok(Self {
            workspace: canonical_workspace,
            target_dir,
            profile_dir,
            runner_executable,
        })
    }
}

fn reject_reparse_directory(path: &Path) -> Result<(), ApiFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ApiFailure::from_io("inspect target directory", error))?;
    if !metadata.is_dir() || metadata.file_attributes() & 0x400 != 0 {
        return Err(ApiFailure {
            status: ProbeStatus::Error,
            reason: "target/profile directories must be regular directories without reparse points"
                .into(),
        });
    }
    Ok(())
}

fn canonical_regular_file(path: &Path, label: &str) -> Result<PathBuf, ApiFailure> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|error| ApiFailure::from_io("inspect registered executable", error))?;
    if !metadata.is_file() || metadata.file_attributes() & 0x400 != 0 {
        return Err(ApiFailure {
            status: ProbeStatus::Error,
            reason: format!("{label} must be a regular, non-reparse file"),
        });
    }
    path.canonicalize()
        .map_err(|error| ApiFailure::from_io("canonicalize registered executable", error))
}

fn validate_executable(
    layout: &RunnerLayout,
    path: &Path,
    expected_name: &str,
) -> Result<PathBuf, ApiFailure> {
    let canonical = canonical_regular_file(path, "registered executable")?;
    if canonical.file_name().and_then(OsStr::to_str) != Some(expected_name)
        || canonical.parent() != Some(layout.profile_dir.as_path())
        || !canonical.starts_with(&layout.target_dir)
    {
        return Err(ApiFailure {
            status: ProbeStatus::Error,
            reason:
                "registered executable is outside the runner's canonical target profile directory"
                    .into(),
        });
    }
    Ok(canonical)
}

fn hash_file(path: &Path) -> Result<String, ApiFailure> {
    let mut file = File::open(path)
        .map_err(|error| ApiFailure::from_io("open registered executable for SHA-256", error))?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| {
            ApiFailure::from_io("read registered executable for SHA-256", error)
        })?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn create_configured_job() -> Result<OwnedHandle, ApiFailure> {
    let raw = unsafe { CreateJobObjectW(None, PCWSTR::null()) }
        .map_err(|error| ApiFailure::from_windows("CreateJobObjectW", error))?;
    let job = OwnedHandle::new(raw);
    let mut limits = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
    limits.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
    unsafe {
        SetInformationJobObject(
            job.raw(),
            JobObjectExtendedLimitInformation,
            (&limits as *const JOBOBJECT_EXTENDED_LIMIT_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
        )
    }
    .map_err(|error| {
        ApiFailure::from_windows("SetInformationJobObject(KILL_ON_JOB_CLOSE)", error)
    })?;
    Ok(job)
}

struct PipePair {
    read: Option<OwnedHandle>,
    write: Option<OwnedHandle>,
}

impl PipePair {
    fn create() -> Result<Self, ApiFailure> {
        let mut raw_read = HANDLE::default();
        let mut raw_write = HANDLE::default();
        let mut security = SECURITY_ATTRIBUTES::default();
        security.nLength = std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32;
        security.bInheritHandle = BOOL(1);
        unsafe { CreatePipe(&mut raw_read, &mut raw_write, Some(&security), 0) }
            .map_err(|error| ApiFailure::from_windows("CreatePipe", error))?;
        Ok(Self {
            read: Some(OwnedHandle::new(raw_read)),
            write: Some(OwnedHandle::new(raw_write)),
        })
    }

    fn read(&self) -> &OwnedHandle {
        self.read.as_ref().expect("pipe read handle")
    }
    fn write(&self) -> &OwnedHandle {
        self.write.as_ref().expect("pipe write handle")
    }
}

struct ChildPipes {
    stdin: PipePair,
    stdout: PipePair,
    stderr: PipePair,
}

impl ChildPipes {
    fn create() -> Result<Self, ApiFailure> {
        let stdin = PipePair::create()?;
        let stdout = PipePair::create()?;
        let stderr = PipePair::create()?;
        for handle in [stdin.write(), stdout.read(), stderr.read()] {
            unsafe { SetHandleInformation(handle.raw(), HANDLE_FLAG_INHERIT.0, HANDLE_FLAGS(0)) }
                .map_err(|error| {
                ApiFailure::from_windows("SetHandleInformation(clear inherit)", error)
            })?;
        }
        Ok(Self {
            stdin,
            stdout,
            stderr,
        })
    }

    fn inherited_handles(&self) -> [HANDLE; 3] {
        [
            self.stdin.read().raw(),
            self.stdout.write().raw(),
            self.stderr.write().raw(),
        ]
    }
}

struct StartupAttributeList {
    storage: Vec<usize>,
    initialized: bool,
}

impl StartupAttributeList {
    fn new(handles: &[HANDLE]) -> Result<Self, ApiFailure> {
        let mut required = 0usize;
        let size_probe = unsafe {
            InitializeProcThreadAttributeList(
                windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST(
                    std::ptr::null_mut(),
                ),
                1,
                0,
                &mut required,
            )
        };
        match size_probe {
            Err(error)
                if hresult_to_win32(error.code().0 as u32)
                    == Some(ERROR_INSUFFICIENT_BUFFER_CODE) => {}
            Err(error) => {
                return Err(ApiFailure::from_windows(
                    "InitializeProcThreadAttributeList(size query)",
                    error,
                ));
            }
            Ok(()) => {
                return Err(ApiFailure {
                    status: ProbeStatus::Error,
                    reason: "attribute-list size query unexpectedly succeeded without storage"
                        .into(),
                });
            }
        }
        if required == 0 {
            return Err(ApiFailure {
                status: ProbeStatus::Error,
                reason: "attribute-list size query returned zero bytes".into(),
            });
        }
        let words = required.div_ceil(std::mem::size_of::<usize>());
        let mut list = Self {
            storage: vec![0usize; words],
            initialized: false,
        };
        unsafe {
            InitializeProcThreadAttributeList(list.as_ptr(), 1, 0, &mut required).map_err(
                |error| ApiFailure::from_windows("InitializeProcThreadAttributeList", error),
            )?;
            list.initialized = true;
            UpdateProcThreadAttribute(
                list.as_ptr(),
                0,
                PROC_THREAD_ATTRIBUTE_HANDLE_LIST as usize,
                Some(handles.as_ptr().cast()),
                std::mem::size_of_val(handles),
                None,
                None,
            )
            .map_err(|error| {
                ApiFailure::from_windows("UpdateProcThreadAttribute(handle list)", error)
            })?;
        }
        Ok(list)
    }

    fn as_ptr(&mut self) -> windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST {
        windows::Win32::System::Threading::LPPROC_THREAD_ATTRIBUTE_LIST(
            self.storage.as_mut_ptr().cast(),
        )
    }
}

impl Drop for StartupAttributeList {
    fn drop(&mut self) {
        if self.initialized {
            unsafe { DeleteProcThreadAttributeList(self.as_ptr()) };
        }
    }
}

struct OwnedHandle(HANDLE);

impl OwnedHandle {
    fn new(handle: HANDLE) -> Self {
        Self(handle)
    }
    fn raw(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        if !self.0.0.is_null() {
            let _ = unsafe { CloseHandle(self.0) };
            self.0 = HANDLE::default();
        }
    }
}

struct OwnedProcess {
    process: OwnedHandle,
    thread: OwnedHandle,
    pid: u32,
}

impl OwnedProcess {
    fn from_info(info: PROCESS_INFORMATION) -> Self {
        Self {
            process: OwnedHandle::new(info.hProcess),
            thread: OwnedHandle::new(info.hThread),
            pid: info.dwProcessId,
        }
    }
}

struct CapturedOutput {
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    remaining: usize,
    truncated: bool,
    stdout_eof: bool,
    stderr_eof: bool,
}

impl CapturedOutput {
    fn new() -> Self {
        Self {
            stdout: Vec::new(),
            stderr: Vec::new(),
            remaining: MAX_CAPTURE_BYTES,
            truncated: false,
            stdout_eof: false,
            stderr_eof: false,
        }
    }
}

const PIPE_READ_CHUNK: usize = 16 * 1024;
const PIPE_READ_BUDGET_PER_POLL: usize = 128 * 1024;

fn drain_available(pipes: &ChildPipes, capture: &mut CapturedOutput) -> Result<bool, ApiFailure> {
    let mut read_any = false;
    let CapturedOutput {
        stdout,
        remaining,
        truncated,
        stdout_eof,
        ..
    } = capture;
    read_any |= drain_pipe(
        pipes.stdout.read(),
        stdout,
        remaining,
        truncated,
        stdout_eof,
    )?;
    let CapturedOutput {
        stderr,
        remaining,
        truncated,
        stderr_eof,
        ..
    } = capture;
    read_any |= drain_pipe(
        pipes.stderr.read(),
        stderr,
        remaining,
        truncated,
        stderr_eof,
    )?;
    Ok(read_any)
}

fn drain_pipe(
    pipe: &OwnedHandle,
    output: &mut Vec<u8>,
    remaining: &mut usize,
    truncated: &mut bool,
    eof: &mut bool,
) -> Result<bool, ApiFailure> {
    if *eof {
        return Ok(false);
    }
    let mut read_any = false;
    let mut read_this_poll = 0usize;
    let mut buffer = [0u8; PIPE_READ_CHUNK];
    while read_this_poll < PIPE_READ_BUDGET_PER_POLL {
        let mut available = 0u32;
        match unsafe { PeekNamedPipe(pipe.raw(), None, 0, None, Some(&mut available), None) } {
            Ok(()) if available == 0 => break,
            Ok(()) => {}
            Err(error) if hresult_to_win32(error.code().0 as u32) == Some(109) => {
                *eof = true;
                break;
            }
            Err(error) => {
                return Err(ApiFailure::from_windows(
                    "PeekNamedPipe(child output)",
                    error,
                ));
            }
        }
        let requested = (available as usize)
            .min(buffer.len())
            .min(PIPE_READ_BUDGET_PER_POLL - read_this_poll);
        if requested == 0 {
            break;
        }
        let mut read = 0u32;
        match unsafe {
            ReadFile(
                pipe.raw(),
                Some(&mut buffer[..requested]),
                Some(&mut read),
                None,
            )
        } {
            Ok(()) => {}
            Err(error) if hresult_to_win32(error.code().0 as u32) == Some(109) => {
                *eof = true;
                break;
            }
            Err(error) => return Err(ApiFailure::from_windows("ReadFile(child output)", error)),
        }
        if read == 0 {
            break;
        }
        read_any = true;
        read_this_poll += read as usize;
        let accepted = (*remaining).min(read as usize);
        output.extend_from_slice(&buffer[..accepted]);
        *remaining -= accepted;
        if accepted < read as usize {
            *truncated = true;
        }
    }
    Ok(read_any)
}

fn drain_until_quiet(
    pipes: &ChildPipes,
    capture: &mut CapturedOutput,
    timeout: Duration,
) -> Result<(), ApiFailure> {
    let deadline = Instant::now() + timeout;
    loop {
        let read_any = drain_available(pipes, capture)?;
        if (capture.stdout_eof && capture.stderr_eof) || Instant::now() >= deadline {
            return Ok(());
        }
        if !read_any {
            std::thread::sleep(Duration::from_millis(10));
        }
    }
}

struct CleanupResult {
    root_signaled: bool,
    empty_verified: bool,
    failure: Option<ApiFailure>,
}

fn terminate_and_verify(
    job: &OwnedHandle,
    process: &OwnedProcess,
    pipes: &ChildPipes,
    capture: &mut CapturedOutput,
) -> CleanupResult {
    let termination_error = unsafe { TerminateJobObject(job.raw(), 1) }
        .err()
        .map(|error| ApiFailure::from_windows("TerminateJobObject", error));
    let root_wait = wait_for_root(process, CLEANUP_WAIT, pipes, capture);
    let root_signaled = matches!(root_wait, Ok(true));
    let empty = wait_for_empty_job(job, CLEANUP_WAIT, pipes, capture);
    let empty_verified = matches!(empty, Ok(true));
    let failure = termination_error
        .or_else(|| root_wait.err())
        .or_else(|| empty.err())
        .or_else(|| (!root_signaled).then(|| ApiFailure {
            status: ProbeStatus::Error,
            reason: "root process did not signal before the cleanup deadline".into(),
        }))
        .or_else(|| (!empty_verified).then(|| ApiFailure {
            status: ProbeStatus::Error,
            reason: "QueryInformationJobObject did not verify ActiveProcesses == 0 before the cleanup deadline".into(),
        }));
    CleanupResult {
        root_signaled,
        empty_verified,
        failure,
    }
}

fn query_job_accounting(
    job: &OwnedHandle,
) -> Result<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION, ApiFailure> {
    let mut accounting = JOBOBJECT_BASIC_ACCOUNTING_INFORMATION::default();
    unsafe {
        QueryInformationJobObject(
            job.raw(),
            JobObjectBasicAccountingInformation,
            (&mut accounting as *mut JOBOBJECT_BASIC_ACCOUNTING_INFORMATION).cast(),
            std::mem::size_of::<JOBOBJECT_BASIC_ACCOUNTING_INFORMATION>() as u32,
            None,
        )
    }
    .map_err(|error| {
        ApiFailure::from_windows("QueryInformationJobObject(ActiveProcesses)", error)
    })?;
    Ok(accounting)
}

fn wait_for_empty_job(
    job: &OwnedHandle,
    timeout: Duration,
    pipes: &ChildPipes,
    capture: &mut CapturedOutput,
) -> Result<bool, ApiFailure> {
    let deadline = Instant::now() + timeout;
    loop {
        let _ = drain_available(pipes, capture)?;
        let accounting = query_job_accounting(job)?;
        if accounting.ActiveProcesses == 0 {
            return Ok(true);
        }
        let now = Instant::now();
        if now >= deadline {
            return Ok(false);
        }
        std::thread::sleep((deadline - now).min(Duration::from_millis(20)));
    }
}

fn wait_for_root(
    process: &OwnedProcess,
    timeout: Duration,
    pipes: &ChildPipes,
    capture: &mut CapturedOutput,
) -> Result<bool, ApiFailure> {
    let deadline = Instant::now() + timeout;
    loop {
        let _ = drain_available(pipes, capture)?;
        let now = Instant::now();
        let wait_time = if now >= deadline {
            0
        } else {
            (deadline - now)
                .min(ROOT_WAIT_SLICE)
                .as_millis()
                .max(1)
                .min(u32::MAX as u128) as u32
        };
        let result = unsafe { WaitForSingleObject(process.process.raw(), wait_time) };
        if result == WAIT_OBJECT_0 {
            let _ = drain_available(pipes, capture)?;
            return Ok(true);
        }
        if result == WAIT_TIMEOUT {
            if Instant::now() >= deadline {
                return Ok(false);
            }
            continue;
        }
        if result == WAIT_FAILED {
            let code = unsafe { windows::Win32::Foundation::GetLastError().0 };
            return Err(ApiFailure::from_win32("WaitForSingleObject(process)", code));
        }
        return Err(ApiFailure {
            status: ProbeStatus::Error,
            reason: format!(
                "WaitForSingleObject returned unexpected result 0x{:08X}",
                result.0
            ),
        });
    }
}

unsafe fn get_exit_code(process: &OwnedHandle) -> Result<u32, ApiFailure> {
    let mut exit_code = 0u32;
    unsafe { GetExitCodeProcess(process.raw(), &mut exit_code) }
        .map_err(|error| ApiFailure::from_windows("GetExitCodeProcess", error))?;
    Ok(exit_code)
}

fn process_times(process: &OwnedHandle) -> Result<(u64, f64), ApiFailure> {
    let mut creation = FILETIME::default();
    let mut exit = FILETIME::default();
    let mut kernel = FILETIME::default();
    let mut user = FILETIME::default();
    unsafe {
        GetProcessTimes(
            process.raw(),
            &mut creation,
            &mut exit,
            &mut kernel,
            &mut user,
        )
    }
    .map_err(|error| ApiFailure::from_windows("GetProcessTimes(root process handle)", error))?;
    let creation = filetime_ticks(creation);
    let cpu_ticks = filetime_ticks(kernel).saturating_add(filetime_ticks(user));
    Ok((creation, cpu_ticks as f64 / 10_000.0))
}

fn filetime_ticks(value: FILETIME) -> u64 {
    ((value.dwHighDateTime as u64) << 32) | value.dwLowDateTime as u64
}

fn create_command_line(executable: &Path, arguments: &[String]) -> Result<Vec<u16>, ApiFailure> {
    let mut command = quote_windows_arg(executable.as_os_str());
    for argument in arguments {
        command.push(' ');
        command.push_str(&quote_windows_arg(OsStr::new(argument)));
    }
    Ok(command.encode_utf16().chain(std::iter::once(0)).collect())
}

fn build_command_line(executable: &Path, arguments: &[String]) -> Result<Vec<u16>, ApiFailure> {
    create_command_line(executable, arguments)
}

fn quote_windows_arg(argument: &OsStr) -> String {
    let text = argument.to_string_lossy();
    if !text.is_empty() && !text.chars().any(char::is_whitespace) && !text.contains('"') {
        return text.into_owned();
    }
    let mut quoted = String::from("\"");
    let mut slashes = 0usize;
    for character in text.chars() {
        match character {
            '\\' => slashes += 1,
            '"' => {
                quoted.extend(std::iter::repeat_n('\\', slashes * 2 + 1));
                quoted.push('"');
                slashes = 0;
            }
            _ => {
                quoted.extend(std::iter::repeat_n('\\', slashes));
                slashes = 0;
                quoted.push(character);
            }
        }
    }
    quoted.extend(std::iter::repeat_n('\\', slashes * 2));
    quoted.push('"');
    quoted
}

fn wide_nul(value: &OsStr) -> Vec<u16> {
    value.encode_wide().chain(std::iter::once(0)).collect()
}
