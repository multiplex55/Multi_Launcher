use anyhow::Context;
use std::path::{Component, Path, PathBuf};

/// Identifies the application-owned data directory without changing how
/// existing relative persistence paths are resolved.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct AppDataRoot {
    path: PathBuf,
}

impl AppDataRoot {
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
        // Windows paths are case-insensitive by default. Normalize separators
        // and case so equivalent spelling does not create a second mutex name.
        self.path
            .to_string_lossy()
            .replace('/', "\\")
            .to_lowercase()
    }
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
}
