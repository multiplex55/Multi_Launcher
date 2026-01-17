use std::path::{Path, PathBuf};

pub enum ConfigFileResult {
    Opened { path: PathBuf },
    Created { path: PathBuf },
}

pub struct ConfigFileSpec<'a> {
    pub label: &'a str,
    pub relative_path: &'a str,
    pub default_contents: &'a str,
}

impl<'a> ConfigFileSpec<'a> {
    pub const fn new(label: &'a str, relative_path: &'a str, default_contents: &'a str) -> Self {
        Self {
            label,
            relative_path,
            default_contents,
        }
    }
}

pub fn resolve_config_path(settings_path: &Path, spec: &ConfigFileSpec<'_>) -> PathBuf {
    let base_dir = settings_path.parent().unwrap_or_else(|| Path::new("."));
    base_dir.join(spec.relative_path)
}

pub fn ensure_config_file(
    settings_path: &Path,
    spec: &ConfigFileSpec<'_>,
) -> anyhow::Result<ConfigFileResult> {
    let path = resolve_config_path(settings_path, spec);
    if path.exists() {
        return Ok(ConfigFileResult::Opened { path });
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, spec.default_contents)?;
    Ok(ConfigFileResult::Created { path })
}

#[cfg(test)]
mod tests {
    use super::{ensure_config_file, resolve_config_path, ConfigFileResult, ConfigFileSpec};
    use std::path::Path;

    #[test]
    fn resolves_path_relative_to_settings_dir() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        let spec = ConfigFileSpec::new("test", "configs/test.json", "{}");
        let resolved = resolve_config_path(&settings_path, &spec);
        assert_eq!(
            resolved,
            dir.path().join(Path::new("configs").join("test.json"))
        );
    }

    #[test]
    fn creates_and_reuses_config_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        let settings_path = dir.path().join("settings.json");
        let spec = ConfigFileSpec::new("test", "test.json", "default");

        let created = ensure_config_file(&settings_path, &spec).expect("create file");
        let path = match created {
            ConfigFileResult::Created { path } => path,
            ConfigFileResult::Opened { .. } => panic!("expected create"),
        };
        let contents = std::fs::read_to_string(&path).expect("read file");
        assert_eq!(contents, "default");

        let opened = ensure_config_file(&settings_path, &spec).expect("open file");
        match opened {
            ConfigFileResult::Opened { path: opened_path } => {
                assert_eq!(opened_path, path);
            }
            ConfigFileResult::Created { .. } => panic!("expected open"),
        }
    }
}
