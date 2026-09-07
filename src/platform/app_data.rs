use anyhow::Context;
use std::io;
use std::path::{Component, Path, PathBuf};

/// Identifies the application-owned data directory without changing how
/// existing relative persistence paths are resolved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AppDataRoot {
    path: PathBuf,
}

impl AppDataRoot {
    #[cfg(test)]
    pub(crate) fn from_path(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Derive the data root from the directory containing the settings file.
    ///
    /// Relative settings paths continue to resolve from the process current
    /// working directory. This type records that existing behavior; it does not
    /// relocate files or change the current directory.
    pub fn from_settings_path(settings_path: impl AsRef<Path>) -> anyhow::Result<Self> {
        let current_dir = std::env::current_dir().context("resolve process current directory")?;
        let settings_path = settings_path.as_ref();
        let absolute_settings = if settings_path.is_absolute() {
            settings_path.to_path_buf()
        } else {
            current_dir.join(settings_path)
        };
        let root = absolute_settings.parent().unwrap_or(current_dir.as_path());

        Ok(Self {
            path: lexically_normalize(root),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub(crate) fn normalized_identity(&self) -> String {
        self.normalized_identity_with(|path| std::fs::canonicalize(path))
    }

    fn normalized_identity_with(
        &self,
        canonicalize: impl FnOnce(&Path) -> io::Result<PathBuf>,
    ) -> String {
        // Resolve filesystem aliases (including junctions and symbolic links)
        // when the data root already exists. A root may legitimately be absent
        // during early startup, so retain the stable lexical identity on any
        // resolution failure.
        let identity_path = canonicalize(&self.path).unwrap_or_else(|_| self.path.clone());

        // Windows paths are case-insensitive by default. Normalize separators
        // and case so equivalent spelling does not create a second mutex name.
        normalize_windows_identity_path(&identity_path)
    }
}

fn normalize_windows_identity_path(path: &Path) -> String {
    let normalized = path.to_string_lossy().replace('/', "\\").to_lowercase();

    if let Some(unc_path) = normalized.strip_prefix(r"\\?\unc\") {
        return format!(r"\\{unc_path}");
    }

    if let Some(disk_path) = normalized.strip_prefix(r"\\?\") {
        let bytes = disk_path.as_bytes();
        if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            return disk_path.to_owned();
        }
    }

    normalized
}

fn lexically_normalize(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                // The input is absolute, so a parent at the filesystem root is
                // safely ignored while ordinary parents remove one component.
                if normalized.file_name().is_some() {
                    normalized.pop();
                }
            }
            Component::Prefix(_) | Component::RootDir | Component::Normal(_) => {
                normalized.push(component.as_os_str());
            }
        }
    }
    normalized
}

#[cfg(test)]
mod tests {
    use super::AppDataRoot;
    use std::io;
    use std::path::{Path, PathBuf};

    #[test]
    fn bare_relative_settings_path_preserves_current_directory_root() {
        let root = AppDataRoot::from_settings_path("settings.json").expect("resolve root");
        assert_eq!(root.path(), std::env::current_dir().unwrap());
    }

    #[test]
    fn nested_relative_settings_path_is_resolved_without_relocating_it() {
        let root = AppDataRoot::from_settings_path("profile/../profile/settings.json")
            .expect("resolve root");
        assert_eq!(
            root.path(),
            std::env::current_dir().unwrap().join("profile")
        );
    }

    #[test]
    fn existing_root_identity_uses_filesystem_canonical_path() {
        let root = AppDataRoot::from_path(PathBuf::from(r"C:\alias\data"));

        let identity = root.normalized_identity_with(|path| {
            assert_eq!(path, Path::new(r"C:\alias\data"));
            Ok(PathBuf::from(r"\\?\C:\real\data"))
        });

        assert_eq!(identity, r"c:\real\data");
    }

    #[test]
    fn missing_root_identity_falls_back_to_lexical_path() {
        let root = AppDataRoot::from_path(PathBuf::from(r"C:\missing\data"));

        let identity = root.normalized_identity_with(|_| {
            Err(io::Error::new(io::ErrorKind::NotFound, "missing root"))
        });

        assert_eq!(identity, r"c:\missing\data");
    }

    #[test]
    fn root_identity_is_stable_when_missing_root_becomes_existing() {
        let root = AppDataRoot::from_path(PathBuf::from(r"C:\data\profile"));
        let missing_identity = root.normalized_identity_with(|_| {
            Err(io::Error::new(io::ErrorKind::NotFound, "missing root"))
        });
        let existing_identity =
            root.normalized_identity_with(|_| Ok(PathBuf::from(r"\\?\C:\data\profile")));

        assert_eq!(missing_identity, existing_identity);
    }

    #[test]
    fn verbatim_unc_identity_matches_standard_unc_path() {
        let standard = AppDataRoot::from_path(PathBuf::from(r"\\server\share\profile"));
        let verbatim = AppDataRoot::from_path(PathBuf::from(r"\\?\UNC\server\share\profile"));

        assert_eq!(
            standard.normalized_identity_with(|path| Ok(path.to_path_buf())),
            verbatim.normalized_identity_with(|path| Ok(path.to_path_buf()))
        );
    }
}
