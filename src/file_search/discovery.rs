use crate::file_search::error::FileSearchError;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExecutableResolutionSource {
    ConfiguredPath,
    LauncherSidecar,
    PortableToolsDirectory,
    ProcessPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipgrepResolution {
    pub path: PathBuf,
    pub source: ExecutableResolutionSource,
    pub version: Option<String>,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutableSearchContext {
    pub launcher_directory: PathBuf,
    pub path_directories: Vec<PathBuf>,
}

impl ExecutableSearchContext {
    pub fn from_process() -> Self {
        let launcher_directory = env::current_exe()
            .ok()
            .and_then(|path| path.parent().map(Path::to_path_buf))
            .or_else(|| env::current_dir().ok())
            .unwrap_or_default();
        let path_directories = env::var_os("PATH")
            .map(|paths| env::split_paths(&paths).collect())
            .unwrap_or_default();
        Self {
            launcher_directory,
            path_directories,
        }
    }
}

pub fn resolve_ripgrep_executable(configured: &Path) -> Result<PathBuf, FileSearchError> {
    resolve_ripgrep(configured).map(|resolution| resolution.path)
}

pub fn resolve_ripgrep(configured: &Path) -> Result<RipgrepResolution, FileSearchError> {
    resolve_ripgrep_with_context(configured, &ExecutableSearchContext::from_process())
}

pub fn resolve_ripgrep_with_context(
    configured: &Path,
    context: &ExecutableSearchContext,
) -> Result<RipgrepResolution, FileSearchError> {
    discover_ripgrep(configured, context)
        .filter(|resolution| resolution.version.is_some())
        .ok_or_else(|| {
            FileSearchError::BackendUnavailable {
                backend: "ripgrep".to_owned(),
                message: format!(
                    "ripgrep executable was not found; checked configured path '{}', launcher sidecars, portable tools, and PATH",
                    configured.display()
                ),
            }
        })
}

pub fn discover_ripgrep(
    configured: &Path,
    context: &ExecutableSearchContext,
) -> Option<RipgrepResolution> {
    let mut warnings = Vec::new();

    if !configured.as_os_str().is_empty() {
        if configured.is_absolute() {
            match validate_ripgrep_candidate(configured) {
                CandidateValidation::Valid { version } => {
                    return Some(RipgrepResolution {
                        path: configured.to_path_buf(),
                        source: ExecutableResolutionSource::ConfiguredPath,
                        version,
                        warnings,
                    });
                }
                CandidateValidation::Invalid(message) => warnings.push(format!(
                    "Configured ripgrep path '{}' is invalid: {message}",
                    configured.display()
                )),
            }
        } else if path_contains_directory_components(configured) {
            warnings.push(format!(
                "Configured ripgrep path '{}' is relative with directory components; use an absolute path or leave it empty for auto-detection",
                configured.display()
            ));
        }
    }

    let sidecar = context.launcher_directory.join("rg.exe");
    if let CandidateValidation::Valid { version } = validate_ripgrep_candidate(&sidecar) {
        return Some(RipgrepResolution {
            path: sidecar,
            source: ExecutableResolutionSource::LauncherSidecar,
            version,
            warnings,
        });
    }

    let portable = context
        .launcher_directory
        .join("tools")
        .join("ripgrep")
        .join("rg.exe");
    if let CandidateValidation::Valid { version } = validate_ripgrep_candidate(&portable) {
        return Some(RipgrepResolution {
            path: portable,
            source: ExecutableResolutionSource::PortableToolsDirectory,
            version,
            warnings,
        });
    }

    for name in ["rg.exe", "rg"] {
        if let Some(path) = find_on_path(Path::new(name), context.path_directories.clone())
            && let CandidateValidation::Valid { version } = validate_ripgrep_candidate(&path) {
                return Some(RipgrepResolution {
                    path,
                    source: ExecutableResolutionSource::ProcessPath,
                    version,
                    warnings,
                });
            }
    }

    if warnings.is_empty() {
        None
    } else {
        Some(RipgrepResolution {
            path: configured.to_path_buf(),
            source: ExecutableResolutionSource::ConfiguredPath,
            version: None,
            warnings,
        })
    }
}

pub fn detect_ripgrep_executable(configured: &Path) -> Result<PathBuf, FileSearchError> {
    resolve_ripgrep_executable(configured)
}

pub fn probe_ripgrep_version(executable: &Path) -> Option<String> {
    match validate_ripgrep_candidate(executable) {
        CandidateValidation::Valid { version } => version,
        CandidateValidation::Invalid(_) => None,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum CandidateValidation {
    Valid { version: Option<String> },
    Invalid(String),
}

fn validate_ripgrep_candidate(executable: &Path) -> CandidateValidation {
    if !executable.is_file() {
        return CandidateValidation::Invalid("path does not exist or is not a file".to_owned());
    }
    let output = match Command::new(executable).arg("--version").output() {
        Ok(output) => output,
        Err(error) => return CandidateValidation::Invalid(format!("failed to start: {error}")),
    };
    if !output.status.success() {
        return CandidateValidation::Invalid(format!(
            "version probe exited with {}",
            output.status
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}\n{stderr}");
    if !combined.to_ascii_lowercase().contains("ripgrep") {
        return CandidateValidation::Invalid("version output did not identify ripgrep".to_owned());
    }
    CandidateValidation::Valid {
        version: combined
            .lines()
            .find(|line| !line.trim().is_empty())
            .map(str::to_owned),
    }
}

fn path_contains_directory_components(path: &Path) -> bool {
    path.components().count() > 1
}

pub(crate) fn find_on_process_path(name: &Path) -> Option<PathBuf> {
    let paths = env::var_os("PATH")?;
    find_on_path(name, env::split_paths(&paths))
}

pub(crate) fn find_on_path(
    name: &Path,
    paths: impl IntoIterator<Item = PathBuf>,
) -> Option<PathBuf> {
    for dir in paths {
        let candidate = dir.join(name);
        if candidate.is_file() {
            return Some(candidate);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[cfg(unix)]
    fn make_rg(path: &Path, first_line: &str) -> bool {
        use std::os::unix::fs::PermissionsExt;
        fs::write(
            path,
            format!("#!/bin/sh\necho '{first_line}'\necho 'second line'\n"),
        )
        .unwrap();
        let mut perms = fs::metadata(path).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(path, perms).unwrap();
        true
    }

    #[cfg(windows)]
    fn make_rg(path: &Path, _first_line: &str) -> bool {
        let Some(source) = find_on_process_path(Path::new("rg.exe")) else {
            return false;
        };
        fs::copy(source, path).unwrap();
        true
    }

    #[cfg(windows)]
    fn exe_name(stem: &str) -> String {
        format!("{stem}.exe")
    }

    #[cfg(not(windows))]
    fn exe_name(stem: &str) -> String {
        stem.to_owned()
    }

    fn ctx(launcher: &Path, path_dir: &Path) -> ExecutableSearchContext {
        ExecutableSearchContext {
            launcher_directory: launcher.to_path_buf(),
            path_directories: vec![path_dir.to_path_buf()],
        }
    }

    #[test]
    fn configured_path_wins_over_sidecar_and_path() {
        let temp = tempfile::tempdir().unwrap();
        let configured = temp.path().join(exe_name("configured rg"));
        let sidecar = temp.path().join("rg.exe");
        let path_dir = temp.path().join("bin");
        fs::create_dir(&path_dir).unwrap();
        let path_rg = path_dir.join(exe_name("rg"));
        if !make_rg(&configured, "ripgrep configured") {
            return;
        }
        if !make_rg(&sidecar, "ripgrep sidecar") {
            return;
        }
        if !make_rg(&path_rg, "ripgrep path") {
            return;
        }
        let resolution =
            resolve_ripgrep_with_context(&configured, &ctx(temp.path(), &path_dir)).unwrap();
        assert_eq!(resolution.path, configured);
        assert_eq!(
            resolution.source,
            ExecutableResolutionSource::ConfiguredPath
        );
    }

    #[test]
    fn sidecar_wins_over_path() {
        let temp = tempfile::tempdir().unwrap();
        let path_dir = temp.path().join("bin");
        fs::create_dir(&path_dir).unwrap();
        let sidecar = temp.path().join("rg.exe");
        if !make_rg(&sidecar, "ripgrep sidecar") {
            return;
        }
        if !make_rg(&path_dir.join(exe_name("rg")), "ripgrep path") {
            return;
        }
        let resolution =
            resolve_ripgrep_with_context(Path::new(""), &ctx(temp.path(), &path_dir)).unwrap();
        assert_eq!(resolution.path, sidecar);
        assert_eq!(
            resolution.source,
            ExecutableResolutionSource::LauncherSidecar
        );
    }

    #[test]
    fn portable_tools_rg_exe_is_found() {
        let temp = tempfile::tempdir().unwrap();
        let path_dir = temp.path().join("bin");
        let tools = temp.path().join("tools/ripgrep");
        fs::create_dir(&path_dir).unwrap();
        fs::create_dir_all(&tools).unwrap();
        let portable = tools.join("rg.exe");
        if !make_rg(&portable, "ripgrep portable") {
            return;
        }
        let resolution =
            resolve_ripgrep_with_context(Path::new(""), &ctx(temp.path(), &path_dir)).unwrap();
        assert_eq!(resolution.path, portable);
        assert_eq!(
            resolution.source,
            ExecutableResolutionSource::PortableToolsDirectory
        );
    }

    #[test]
    fn invalid_explicit_path_falls_through_with_warning() {
        let temp = tempfile::tempdir().unwrap();
        let path_dir = temp.path().join("bin");
        fs::create_dir(&path_dir).unwrap();
        let configured = temp.path().join("missing rg");
        let path_rg = path_dir.join(exe_name("rg"));
        if !make_rg(&path_rg, "ripgrep path") {
            return;
        }
        let resolution =
            resolve_ripgrep_with_context(&configured, &ctx(temp.path(), &path_dir)).unwrap();
        assert_eq!(resolution.path, path_rg);
        assert_eq!(resolution.source, ExecutableResolutionSource::ProcessPath);
        assert!(resolution
            .warnings
            .iter()
            .any(|w| w.contains(&configured.display().to_string())));
    }

    #[test]
    fn arbitrary_relative_configured_paths_are_rejected() {
        let temp = tempfile::tempdir().unwrap();
        let resolution =
            discover_ripgrep(Path::new("relative/rg"), &ctx(temp.path(), temp.path())).unwrap();
        assert!(resolution.version.is_none());
        assert!(resolution.warnings[0].contains("relative"));
    }

    #[test]
    fn empty_configured_path_still_detects_path() {
        let temp = tempfile::tempdir().unwrap();
        let path_dir = temp.path().join("bin");
        fs::create_dir(&path_dir).unwrap();
        let path_rg = path_dir.join(exe_name("rg"));
        if !make_rg(&path_rg, "ripgrep path") {
            return;
        }
        let resolution =
            resolve_ripgrep_with_context(Path::new(""), &ctx(temp.path(), &path_dir)).unwrap();
        assert_eq!(resolution.path, path_rg);
    }

    #[test]
    fn stores_only_first_version_line() {
        let temp = tempfile::tempdir().unwrap();
        let rg = temp.path().join(exe_name("rg"));
        if !make_rg(&rg, "ripgrep 13.0.0") {
            return;
        }
        let version = probe_ripgrep_version(&rg).expect("ripgrep version");
        assert!(version.to_ascii_lowercase().contains("ripgrep"));
        assert!(!version.contains("second line"));
    }

    #[test]
    fn paths_containing_spaces_work() {
        let temp = tempfile::tempdir().unwrap();
        let rg = temp.path().join(exe_name("rg with spaces"));
        if !make_rg(&rg, "ripgrep spaces") {
            return;
        }
        let resolution = resolve_ripgrep_with_context(&rg, &ctx(temp.path(), temp.path())).unwrap();
        assert_eq!(resolution.path, rg);
    }

    #[test]
    fn unicode_paths_work() {
        let temp = tempfile::tempdir().unwrap();
        let rg = temp.path().join(exe_name("rg-搜索"));
        if !make_rg(&rg, "ripgrep unicode") {
            return;
        }
        let resolution = resolve_ripgrep_with_context(&rg, &ctx(temp.path(), temp.path())).unwrap();
        assert_eq!(resolution.path, rg);
    }
}
