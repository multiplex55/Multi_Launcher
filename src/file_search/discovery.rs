use crate::file_search::error::FileSearchError;
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RipgrepResolutionSource {
    ConfiguredPath,
    ProcessPath,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RipgrepResolution {
    pub executable: PathBuf,
    pub source: RipgrepResolutionSource,
    pub version: Option<String>,
}

pub fn resolve_ripgrep_executable(configured: &Path) -> Result<PathBuf, FileSearchError> {
    resolve_ripgrep(configured).map(|resolution| resolution.executable)
}

pub fn resolve_ripgrep(configured: &Path) -> Result<RipgrepResolution, FileSearchError> {
    let configured = if configured.as_os_str().is_empty() {
        Path::new("rg")
    } else {
        configured
    };

    if path_contains_directory_components(configured) {
        if configured.is_file() {
            return Ok(RipgrepResolution {
                executable: configured.to_path_buf(),
                source: RipgrepResolutionSource::ConfiguredPath,
                version: probe_ripgrep_version(configured),
            });
        }
        return Err(FileSearchError::BackendUnavailable {
            backend: "ripgrep".to_owned(),
            message: format!(
                "configured ripgrep executable '{}' does not exist or is not a file",
                configured.display()
            ),
        });
    }

    find_on_process_path(configured)
        .map(|executable| RipgrepResolution {
            version: probe_ripgrep_version(&executable),
            executable,
            source: RipgrepResolutionSource::ProcessPath,
        })
        .ok_or_else(|| FileSearchError::BackendUnavailable {
            backend: "ripgrep".to_owned(),
            message: format!(
                "ripgrep executable '{}' was not found on PATH",
                configured.display()
            ),
        })
}

pub fn detect_ripgrep_executable(configured: &Path) -> Result<PathBuf, FileSearchError> {
    resolve_ripgrep_executable(configured)
}

pub fn probe_ripgrep_version(executable: &Path) -> Option<String> {
    let output = Command::new(executable).arg("--version").output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout)
        .ok()
        .and_then(|s| s.lines().next().map(str::to_owned))
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
        #[cfg(windows)]
        if name.extension().is_none() {
            let exe = dir.join(format!("{}.exe", name.as_os_str().to_string_lossy()));
            if exe.is_file() {
                return Some(exe);
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn resolves_bare_rg_from_supplied_path() {
        let temp = tempfile::tempdir().unwrap();
        let executable = temp.path().join("rg");
        std::fs::write(&executable, "").unwrap();
        assert_eq!(
            find_on_path(Path::new("rg"), [temp.path().to_path_buf()]),
            Some(executable)
        );
    }
}
