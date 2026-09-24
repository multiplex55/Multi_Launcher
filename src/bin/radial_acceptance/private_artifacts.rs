use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, Write};
use std::path::{Path, PathBuf};
use tempfile::{Builder, TempDir};

pub(super) const MAX_PRIVATE_ARTIFACT_FILES: usize = 128;
pub(super) const MAX_PRIVATE_ARTIFACT_BYTES: u64 = 128 * 1024 * 1024;
const MAX_SCREENSHOT_BYTES: u64 = 16 * 1024 * 1024;
const MAX_TRACE_BYTES: u64 = 128 * 1024;
const MAX_LOG_BYTES: u64 = 64 * 1024;
const MAX_TEXT_BYTES: u64 = 64 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum PrivateArtifactStatus {
    NotRun,
    EphemeralValidated,
    Retained,
    Failed,
}

#[derive(Clone, Debug, serde::Serialize)]
pub(super) struct PrivateArtifactSummary {
    pub status: PrivateArtifactStatus,
    pub artifact_id: Option<String>,
    pub file_count: usize,
    pub total_bytes: u64,
    pub manifest_sha256: Option<String>,
}

impl PrivateArtifactSummary {
    pub(super) fn not_run() -> Self {
        Self {
            status: PrivateArtifactStatus::NotRun,
            artifact_id: None,
            file_count: 0,
            total_bytes: 0,
            manifest_sha256: None,
        }
    }

    pub(super) fn failed() -> Self {
        Self {
            status: PrivateArtifactStatus::Failed,
            ..Self::not_run()
        }
    }
}

pub(super) struct RetainedPrivateArtifacts {
    pub summary: PrivateArtifactSummary,
    #[allow(dead_code)]
    pub directory: PathBuf,
}

pub(super) struct StagedPrivateArtifacts {
    directory: TempDir,
    summary: PrivateArtifactSummary,
    control_artifacts_complete: bool,
}

impl StagedPrivateArtifacts {
    #[cfg(test)]
    pub(super) fn summary(&self) -> &PrivateArtifactSummary {
        &self.summary
    }

    pub(super) fn control_artifacts_complete(&self) -> bool {
        self.control_artifacts_complete
    }

    pub(super) fn ephemeral_summary(&self) -> PrivateArtifactSummary {
        let mut summary = self.summary.clone();
        summary.status = PrivateArtifactStatus::EphemeralValidated;
        summary.artifact_id = None;
        summary
    }

    pub(super) fn verify_after_profile_cleanup(&self) -> Result<(), String> {
        verify_retained_artifacts(self.directory.path(), &self.summary)
    }

    pub(super) fn retain(self) -> RetainedPrivateArtifacts {
        let directory = self.directory.keep();
        RetainedPrivateArtifacts {
            summary: self.summary,
            directory,
        }
    }
}

pub(super) fn write_bounded_run_log_snapshots(
    profile_root: &Path,
    run_id: &str,
) -> Result<Vec<PathBuf>, String> {
    if run_id.is_empty()
        || run_id.len() > 64
        || !run_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
    {
        return Err("private log snapshot identity is malformed".into());
    }
    let specifications = [
        (
            "acceptance-runner.log",
            "runner-log-private.log",
            MAX_LOG_BYTES,
        ),
        (
            "acceptance.log",
            "child-acceptance-trace.log",
            MAX_TRACE_BYTES,
        ),
        (
            "child.stdout.log",
            "child-stdout-private.log",
            MAX_LOG_BYTES,
        ),
        (
            "child.stderr.log",
            "child-stderr-private.log",
            MAX_LOG_BYTES,
        ),
    ];
    let root = profile_root
        .canonicalize()
        .map_err(|_| "resolve isolated copy for bounded log snapshots".to_string())?;
    let mut written = Vec::with_capacity(specifications.len());
    for (source_name, destination_suffix, maximum) in specifications {
        let source = root.join(source_name);
        let source_metadata = match fs::symlink_metadata(&source) {
            Ok(metadata) => metadata,
            Err(error)
                if error.kind() == std::io::ErrorKind::NotFound
                    && source_name != "acceptance-runner.log" =>
            {
                // A child may fail before it creates its own logs. Keep every log
                // that exists; the runner log is always required for a failure bundle.
                continue;
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Err("required private runner/child log is missing".into());
            }
            Err(_) => return Err("inspect private runner/child log".into()),
        };
        if !source_metadata.is_file() || is_reparse_point(&source_metadata) {
            return Err("private runner/child log is not a regular file".into());
        }
        let mut input = super::copied_profile::open_regular_profile_file(
            &source,
            &root,
            Path::new(source_name),
        )
        .map_err(|_| {
            "open private runner/child log without following reparse points".to_string()
        })?;
        let opened = input
            .metadata()
            .map_err(|_| "inspect opened private runner/child log".to_string())?;
        if !opened.is_file() || is_reparse_point(&opened) {
            return Err("opened private runner/child log changed type".into());
        }
        let start = opened.len().saturating_sub(maximum);
        input
            .seek(std::io::SeekFrom::Start(start))
            .map_err(|_| "seek bounded private runner/child log tail".to_string())?;
        let mut bytes = Vec::with_capacity(usize::try_from(opened.len().min(maximum)).unwrap_or(0));
        input
            .take(maximum)
            .read_to_end(&mut bytes)
            .map_err(|_| "read bounded private runner/child log tail".to_string())?;
        if bytes.is_empty() {
            bytes.extend_from_slice(b"(empty log)\n");
        }
        if bytes.len() as u64 > maximum {
            return Err("bounded runner/child log snapshot exceeded its byte limit".into());
        }
        let destination = root.join(format!("case-{run_id}-{destination_suffix}"));
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|_| "create bounded private runner/child log snapshot".to_string())?;
        output
            .write_all(&bytes)
            .and_then(|()| output.flush())
            .map_err(|_| "write bounded private runner/child log snapshot".to_string())?;
        written.push(destination);
    }
    Ok(written)
}

pub(super) fn stage_diagnostics(
    copied_profile_root: &Path,
    artifact_paths: impl IntoIterator<Item = PathBuf>,
) -> Result<StagedPrivateArtifacts, String> {
    let copied_profile_root = copied_profile_root
        .canonicalize()
        .map_err(|_| "resolve copied profile before retaining diagnostics".to_string())?;
    let mut source_paths = BTreeMap::<String, PathBuf>::new();
    for path in artifact_paths {
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "diagnostic artifact has no safe file name".to_string())?;
        if !is_safe_artifact_name(file_name) {
            return Err(
                "diagnostic artifact name is outside the bounded case artifact format".into(),
            );
        }
        let parent = path
            .parent()
            .ok_or_else(|| "diagnostic artifact has no parent directory".to_string())?
            .canonicalize()
            .map_err(|_| "resolve diagnostic artifact parent".to_string())?;
        if parent != copied_profile_root {
            return Err("diagnostic artifact escaped the isolated copied profile".into());
        }
        let path = parent.join(file_name);
        let metadata = fs::symlink_metadata(&path)
            .map_err(|_| "inspect bounded diagnostic artifact".to_string())?;
        if !metadata.is_file() || is_reparse_point(&metadata) {
            return Err("diagnostic artifact is not a regular file".into());
        }
        if source_paths
            .insert(file_name.to_ascii_lowercase(), path)
            .is_some()
        {
            return Err("diagnostic artifact names collide without regard to case".into());
        }
    }
    if source_paths.is_empty() {
        return Err("no bounded copied-profile diagnostic artifacts were available".into());
    }

    let required = [
        "case-R1-trace.log",
        "case-R1-windows.json",
        "case-R1-private.log",
        "case-R1.png",
    ];
    let control_artifacts_complete = required
        .iter()
        .all(|name| source_paths.contains_key(&name.to_ascii_lowercase()));
    if source_paths.len() > MAX_PRIVATE_ARTIFACT_FILES {
        return Err("private diagnostic artifact count exceeded its bound".into());
    }

    let directory = Builder::new()
        .prefix("multi-launcher-private-evidence-")
        .tempdir_in(std::env::temp_dir())
        .map_err(|_| "create private diagnostic evidence directory".to_string())?;
    restrict_directory_acl(directory.path())?;

    let mut manifest_entries = Vec::with_capacity(source_paths.len());
    let mut total_bytes = 0_u64;
    for (key, source) in source_paths {
        let file_name = source
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| "diagnostic artifact name changed during staging".to_string())?;
        let mut input = super::copied_profile::open_regular_profile_file(
            &source,
            &copied_profile_root,
            Path::new(file_name),
        )
        .map_err(|_| {
            "open a bounded diagnostic artifact without following reparse points".to_string()
        })?;
        let input_metadata = input
            .metadata()
            .map_err(|_| "inspect opened diagnostic artifact".to_string())?;
        let maximum = max_file_bytes(file_name)?;
        if !input_metadata.is_file()
            || is_reparse_point(&input_metadata)
            || input_metadata.len() == 0
            || input_metadata.len() > maximum
        {
            return Err(
                "diagnostic artifact is empty, nonregular, or exceeds its byte bound".into(),
            );
        }
        let copy_limit = artifact_copy_limit(total_bytes, input_metadata.len(), maximum)
            .ok_or_else(|| "private diagnostic artifact bytes exceeded their bound".to_string())?;
        let destination = directory.path().join(file_name);
        let mut output = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&destination)
            .map_err(|_| "create private diagnostic artifact".to_string())?;
        let (digest, copied_bytes) = copy_and_hash(&mut input, &mut output, copy_limit)
            .map_err(|_| "copy bounded private diagnostic artifact".to_string())?;
        output
            .flush()
            .map_err(|_| "flush private diagnostic artifact".to_string())?;
        let copied_metadata = fs::symlink_metadata(&destination)
            .map_err(|_| "recheck retained private artifact".to_string())?;
        if !copied_metadata.is_file()
            || is_reparse_point(&copied_metadata)
            || copied_bytes != input_metadata.len()
            || copied_metadata.len() != input_metadata.len()
            || hash_regular_file(&destination)? != digest
        {
            return Err("retained private artifact did not match its copied source bytes".into());
        }
        total_bytes = total_bytes
            .checked_add(copied_bytes)
            .filter(|bytes| *bytes <= MAX_PRIVATE_ARTIFACT_BYTES)
            .ok_or_else(|| "private diagnostic artifact bytes exceeded their bound".to_string())?;
        manifest_entries.push((key, input_metadata.len(), digest));
    }
    manifest_entries.sort_by(|left, right| left.0.cmp(&right.0));
    let mut manifest = Sha256::new();
    for (name, bytes, digest) in &manifest_entries {
        manifest.update(name.as_bytes());
        manifest.update([0]);
        manifest.update(bytes.to_le_bytes());
        manifest.update(digest.as_bytes());
        manifest.update([b'\n']);
    }
    let manifest_sha256 = hex::encode(manifest.finalize());
    let artifact_id = directory
        .path()
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| is_opaque_id(name))
        .ok_or_else(|| {
            "private diagnostic directory did not receive an opaque identifier".to_string()
        })?
        .to_string();
    Ok(StagedPrivateArtifacts {
        directory,
        summary: PrivateArtifactSummary {
            status: PrivateArtifactStatus::Retained,
            artifact_id: Some(artifact_id),
            file_count: manifest_entries.len(),
            total_bytes,
            manifest_sha256: Some(manifest_sha256),
        },
        control_artifacts_complete,
    })
}

pub(super) fn verify_retained_artifacts(
    directory: &Path,
    expected: &PrivateArtifactSummary,
) -> Result<(), String> {
    if expected.status != PrivateArtifactStatus::Retained
        || expected
            .artifact_id
            .as_deref()
            .is_none_or(|id| !is_opaque_id(id))
        || directory.file_name().and_then(|name| name.to_str()) != expected.artifact_id.as_deref()
    {
        return Err("private artifact bundle identity is invalid".into());
    }
    let root = directory
        .canonicalize()
        .map_err(|_| "private artifact bundle is missing after profile cleanup".to_string())?;
    let metadata = fs::symlink_metadata(&root)
        .map_err(|_| "inspect retained private artifact directory".to_string())?;
    if !metadata.is_dir() || is_reparse_point(&metadata) {
        return Err("retained private artifact directory is not regular".into());
    }
    let mut entries = BTreeMap::new();
    let mut total_bytes = 0_u64;
    for entry in fs::read_dir(&root)
        .map_err(|_| "enumerate retained private diagnostic evidence".to_string())?
    {
        let entry = entry.map_err(|_| "read retained private artifact entry".to_string())?;
        let name = entry
            .file_name()
            .into_string()
            .map_err(|_| "retained private artifact name is invalid".to_string())?;
        if !is_safe_artifact_name(&name) || entries.len() >= MAX_PRIVATE_ARTIFACT_FILES {
            return Err(
                "retained private artifact list exceeded its bounded naming contract".into(),
            );
        }
        let path = root.join(&name);
        let file_metadata = fs::symlink_metadata(&path)
            .map_err(|_| "inspect retained private artifact type".to_string())?;
        let size_limit = max_file_bytes(&name)?;
        if !file_metadata.is_file()
            || is_reparse_point(&file_metadata)
            || file_metadata.len() == 0
            || file_metadata.len() > size_limit
        {
            return Err("retained private artifact is nonregular or out of bounds".into());
        }
        total_bytes = total_bytes
            .checked_add(file_metadata.len())
            .filter(|bytes| *bytes <= MAX_PRIVATE_ARTIFACT_BYTES)
            .ok_or_else(|| "retained private evidence exceeded its total bound".to_string())?;
        let digest = hash_regular_file(&path)?;
        if entries
            .insert(name.to_ascii_lowercase(), (file_metadata.len(), digest))
            .is_some()
        {
            return Err("retained private artifacts contain a case-insensitive collision".into());
        }
    }
    let required = [
        "case-r1-trace.log",
        "case-r1-windows.json",
        "case-r1-private.log",
        "case-r1.png",
    ];
    if entries.len() != expected.file_count
        || total_bytes != expected.total_bytes
        || required.iter().any(|name| !entries.contains_key(*name))
    {
        return Err("retained private artifact count or size changed after profile cleanup".into());
    }
    let mut manifest = Sha256::new();
    for (name, (bytes, digest)) in entries {
        manifest.update(name.as_bytes());
        manifest.update([0]);
        manifest.update(bytes.to_le_bytes());
        manifest.update(digest.as_bytes());
        manifest.update([b'\n']);
    }
    if Some(hex::encode(manifest.finalize())).as_deref() != expected.manifest_sha256.as_deref() {
        return Err("retained private artifact manifest changed after profile cleanup".into());
    }
    Ok(())
}

fn is_safe_artifact_name(name: &str) -> bool {
    if !name.starts_with("case-")
        || name.len() > 160
        || !name
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
    {
        return false;
    }
    matches!(
        Path::new(name).extension().and_then(|ext| ext.to_str()),
        Some("png" | "log" | "json" | "txt")
    )
}

fn max_file_bytes(name: &str) -> Result<u64, String> {
    match Path::new(name).extension().and_then(|ext| ext.to_str()) {
        Some("png") => Ok(MAX_SCREENSHOT_BYTES),
        Some("log") if name.ends_with("-trace.log") => Ok(MAX_TRACE_BYTES),
        Some("log") => Ok(MAX_LOG_BYTES),
        Some("json" | "txt") => Ok(MAX_TEXT_BYTES),
        _ => Err("diagnostic artifact extension is not supported".into()),
    }
}

fn artifact_copy_limit(
    total_bytes_before: u64,
    current_bytes: u64,
    per_file_limit: u64,
) -> Option<u64> {
    let remaining = MAX_PRIVATE_ARTIFACT_BYTES.checked_sub(total_bytes_before)?;
    (current_bytes <= per_file_limit && current_bytes <= remaining).then_some(current_bytes)
}

pub(super) fn is_opaque_id(value: &str) -> bool {
    value.starts_with("multi-launcher-private-evidence-")
        && value.len() <= 96
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
}

fn copy_and_hash(
    input: &mut File,
    output: &mut File,
    max_bytes: u64,
) -> std::io::Result<(String, u64)> {
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    let mut copied = 0_u64;
    loop {
        let count = input.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        copied = copied
            .checked_add(u64::try_from(count).unwrap_or(u64::MAX))
            .filter(|bytes| *bytes <= max_bytes)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "diagnostic artifact grew beyond its byte bound while copying",
                )
            })?;
        output.write_all(&buffer[..count])?;
        digest.update(&buffer[..count]);
    }
    Ok((hex::encode(digest.finalize()), copied))
}

fn hash_regular_file(path: &Path) -> Result<String, String> {
    let metadata = fs::symlink_metadata(path)
        .map_err(|_| "inspect retained private evidence file".to_string())?;
    if !metadata.is_file() || is_reparse_point(&metadata) {
        return Err("retained private evidence changed type".into());
    }
    let mut input = super::copied_profile::open_regular_profile_file(
        path,
        path.parent().unwrap_or_else(|| Path::new(".")),
        Path::new(path.file_name().unwrap_or_default()),
    )
    .map_err(|_| "reopen retained private evidence without following reparse points".to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0_u8; 32 * 1024];
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|_| "verify retained private evidence bytes".to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(hex::encode(digest.finalize()))
}

#[cfg(windows)]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(metadata: &std::fs::Metadata) -> bool {
    metadata.file_type().is_symlink()
}

#[cfg(windows)]
fn restrict_directory_acl(path: &Path) -> Result<(), String> {
    use std::os::windows::ffi::OsStrExt;
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Authorization::{
        ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
    };
    use windows::Win32::Security::{
        DACL_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
        SetFileSecurityW,
    };
    use windows::core::HSTRING;

    // TempDir is created by this user, so Owner Rights and SYSTEM are the only
    // principals granted access. Protected inheritance prevents the parent
    // temp directory's broader ACL from adding access to retained diagnostics.
    let descriptor_sddl = HSTRING::from("D:P(A;OICI;FA;;;OW)(A;OICI;FA;;;SY)");
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &descriptor_sddl,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(|error| format!("build private evidence ACL: {error}"))?;

    let path_wide = path
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect::<Vec<_>>();
    let set_result = unsafe {
        SetFileSecurityW(
            windows::core::PCWSTR(path_wide.as_ptr()),
            DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            descriptor,
        )
    };
    unsafe {
        LocalFree(HLOCAL(descriptor.0));
    }
    set_result
        .ok()
        .map_err(|error| format!("restrict private evidence directory ACL: {error}"))
}

#[cfg(not(windows))]
fn restrict_directory_acl(_path: &Path) -> Result<(), String> {
    Err("private evidence ACL enforcement is only supported on Windows".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(windows)]
    #[test]
    fn private_artifacts_are_bounded_hashed_and_never_return_paths_in_summary() {
        let profile = tempfile::tempdir().unwrap();
        for (name, content) in [
            ("case-R1-trace.log", "bounded trace\n"),
            ("case-R1-windows.json", "{\"windows\":[]}\n"),
            ("case-R1-private.log", "bounded log\n"),
            ("case-R1.png", "png fixture\n"),
            ("case-D1-uia-tree.txt", "private uia text\n"),
        ] {
            fs::write(profile.path().join(name), content).unwrap();
        }
        let paths = fs::read_dir(profile.path())
            .unwrap()
            .map(|entry| entry.unwrap().path())
            .collect::<Vec<_>>();
        let staged = stage_diagnostics(profile.path(), paths).unwrap();
        assert!(staged.control_artifacts_complete());
        let summary = staged.summary();
        assert_eq!(summary.status, PrivateArtifactStatus::Retained);
        assert_eq!(summary.file_count, 5);
        assert_eq!(summary.total_bytes, 70);
        assert!(summary.manifest_sha256.as_deref().is_some_and(is_sha256));
        let serialized = serde_json::to_string(summary).unwrap();
        assert!(!serialized.contains(profile.path().to_string_lossy().as_ref()));
        assert!(!serialized.contains("private uia text"));
    }

    fn is_sha256(value: &str) -> bool {
        value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
    }

    #[test]
    fn private_artifact_collector_rejects_outside_links_and_overlarge_files() {
        let profile = tempfile::tempdir().unwrap();
        let outside = tempfile::NamedTempFile::new().unwrap();
        fs::write(outside.path(), b"outside").unwrap();
        assert!(stage_diagnostics(profile.path(), [outside.path().to_path_buf()]).is_err());

        let large = profile.path().join("case-D2-uia-tree.txt");
        fs::write(&large, vec![b'x'; MAX_TEXT_BYTES as usize + 1]).unwrap();
        assert!(stage_diagnostics(profile.path(), [large]).is_err());
    }

    #[test]
    fn private_artifact_budget_allows_current_file_up_to_remaining_capacity() {
        let current_bytes = 4 * 1024;
        let total_before = MAX_PRIVATE_ARTIFACT_BYTES - current_bytes;
        assert_eq!(
            artifact_copy_limit(total_before, current_bytes, MAX_SCREENSHOT_BYTES),
            Some(current_bytes)
        );
        assert_eq!(
            artifact_copy_limit(total_before + 1, current_bytes, MAX_SCREENSHOT_BYTES),
            None
        );
    }

    #[test]
    fn failure_log_snapshots_include_available_logs_and_only_copy_bounded_tails() {
        let profile = tempfile::tempdir().unwrap();
        fs::write(
            profile.path().join("acceptance-runner.log"),
            vec![b'r'; MAX_LOG_BYTES as usize + 17],
        )
        .unwrap();
        fs::write(profile.path().join("acceptance.log"), b"child trace\n").unwrap();
        fs::write(profile.path().join("child.stdout.log"), b"child stdout\n").unwrap();

        let snapshots = write_bounded_run_log_snapshots(profile.path(), "run-123").unwrap();
        assert_eq!(snapshots.len(), 3);
        let runner = fs::read(profile.path().join("case-run-123-runner-log-private.log")).unwrap();
        assert_eq!(runner.len(), MAX_LOG_BYTES as usize);
        assert!(runner.iter().all(|byte| *byte == b'r'));
        assert_eq!(
            fs::read_to_string(
                profile
                    .path()
                    .join("case-run-123-child-acceptance-trace.log")
            )
            .unwrap(),
            "child trace\n"
        );
        assert_eq!(
            fs::read_to_string(profile.path().join("case-run-123-child-stdout-private.log"))
                .unwrap(),
            "child stdout\n"
        );
        assert!(
            !profile
                .path()
                .join("case-run-123-child-stderr-private.log")
                .exists()
        );
    }

    #[cfg(windows)]
    #[test]
    fn retained_artifacts_survive_copied_profile_removal() {
        let profile = tempfile::tempdir().unwrap();
        for name in [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ] {
            fs::write(profile.path().join(name), name.as_bytes()).unwrap();
        }
        for (name, content) in [
            ("acceptance-runner.log", "runner details"),
            ("acceptance.log", "child trace"),
            ("child.stdout.log", "child stdout"),
            ("child.stderr.log", "child stderr"),
        ] {
            fs::write(profile.path().join(name), content).unwrap();
        }
        let snapshots = write_bounded_run_log_snapshots(profile.path(), "failed-run").unwrap();
        let mut paths = [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ]
        .into_iter()
        .map(|name| profile.path().join(name))
        .collect::<Vec<_>>();
        paths.extend(snapshots);
        let retained = stage_diagnostics(profile.path(), paths).unwrap().retain();
        let profile_path = profile.path().to_path_buf();
        profile.close().unwrap();
        assert!(!profile_path.exists());
        assert_eq!(retained.summary.file_count, 8);
        verify_retained_artifacts(&retained.directory, &retained.summary).unwrap();
        assert!(retained.directory.join("case-R1.png").is_file());
        for suffix in [
            "runner-log-private.log",
            "child-acceptance-trace.log",
            "child-stdout-private.log",
            "child-stderr-private.log",
        ] {
            assert!(
                retained
                    .directory
                    .join(format!("case-failed-run-{suffix}"))
                    .is_file()
            );
        }
        fs::remove_dir_all(&retained.directory).unwrap();
    }

    #[cfg(windows)]
    #[test]
    fn successful_ephemeral_bundle_is_deleted_after_validation_and_profile_removal() {
        let profile = tempfile::tempdir().unwrap();
        for name in [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ] {
            fs::write(profile.path().join(name), name.as_bytes()).unwrap();
        }
        for (name, content) in [
            ("acceptance-runner.log", "runner details"),
            ("acceptance.log", "child trace"),
            ("child.stdout.log", "child stdout"),
            ("child.stderr.log", "child stderr"),
        ] {
            fs::write(profile.path().join(name), content).unwrap();
        }
        let snapshots = write_bounded_run_log_snapshots(profile.path(), "passed-run").unwrap();
        let mut paths = [
            "case-R1-trace.log",
            "case-R1-windows.json",
            "case-R1-private.log",
            "case-R1.png",
        ]
        .into_iter()
        .map(|name| profile.path().join(name))
        .collect::<Vec<_>>();
        paths.extend(snapshots);
        let staged = stage_diagnostics(profile.path(), paths).unwrap();
        let artifact_id = staged.summary().artifact_id.clone().unwrap();
        let bundle_path = std::env::temp_dir().join(artifact_id);
        assert!(bundle_path.is_dir());

        let profile_path = profile.path().to_path_buf();
        profile.close().unwrap();
        assert!(!profile_path.exists());
        staged.verify_after_profile_cleanup().unwrap();
        let public_summary = staged.ephemeral_summary();
        assert_eq!(
            public_summary.status,
            PrivateArtifactStatus::EphemeralValidated
        );
        assert_eq!(public_summary.artifact_id, None);
        assert_eq!(public_summary.file_count, 8);
        drop(staged);
        assert!(!bundle_path.exists());
    }
}
