//! Secure, bounded local XML file loading shared by both frontends.
use crate::xml_source::{validate_local_path, validate_utf8_declaration};
use crate::xml_toolkit_engine::MAX_XML_INPUT_BYTES;
use std::fs;
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::Path;
#[cfg(not(windows))]
use std::path::PathBuf;

pub fn read_xml_file(path: &Path) -> Result<String, String> {
    let file = open_local_xml_file(path)?;
    let raw_limit = MAX_XML_INPUT_BYTES.saturating_add(4);
    let mut bytes = Vec::with_capacity(raw_limit.min(64 * 1024));
    file.take(raw_limit as u64)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Could not read XML file: {error}"))?;

    let content = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        &bytes[3..]
    } else {
        bytes.as_slice()
    };
    if content.len() > MAX_XML_INPUT_BYTES {
        return Err(format!(
            "XML files are limited to {MAX_XML_INPUT_BYTES} UTF-8 bytes."
        ));
    }
    let source = String::from_utf8(content.to_vec())
        .map_err(|_| "XML file is not valid UTF-8. Save it as UTF-8 and try again.".to_owned())?;
    validate_utf8_declaration(&source)?;
    Ok(source)
}

fn open_local_xml_file(path: &Path) -> Result<File, String> {
    validate_local_path(path)?;
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;

        crate::xml_toolkit_engine::validate_local_xml_path(path)?;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        let file = OpenOptions::new()
            .read(true)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)
            .map_err(|error| format!("Could not open XML file: {error}"))?;
        crate::xml_toolkit_engine::validate_local_opened_xml_file(path, &file)?;
        Ok(file)
    }
    #[cfg(not(windows))]
    {
        reject_symlink_components(path)?;
        let file = File::open(path).map_err(|error| format!("Could not open XML file: {error}"))?;
        if !file
            .metadata()
            .map_err(|error| format!("Could not inspect XML file: {error}"))?
            .is_file()
        {
            return Err("The selected XML path is not a regular file.".into());
        }
        path.canonicalize()
            .map_err(|error| format!("Could not resolve XML file: {error}"))?;
        Ok(file)
    }
}

#[cfg(not(windows))]
pub(crate) fn reject_symlink_components(path: &Path) -> Result<(), String> {
    let mut current = PathBuf::new();
    for component in path.components() {
        current.push(component.as_os_str());
        if fs::symlink_metadata(&current)
            .map_err(|error| format!("Could not inspect XML path: {error}"))?
            .file_type()
            .is_symlink()
        {
            return Err("Symbolic links are not supported for XML file paths.".into());
        }
    }
    Ok(())
}

pub fn run_xml_load(
    path: &Path,
    activity: Option<&crate::activity::ActivityTask>,
) -> Result<String, String> {
    use crate::activity::{ActivityDiagnostic, ActivityOutcome, ActivityRetirement};
    if let Some(activity) = activity {
        activity.mark_running();
    }
    let result = read_xml_file(path);
    if let Some(activity) = activity {
        let outcome = if result.is_ok() {
            ActivityOutcome::Completed
        } else {
            ActivityOutcome::Failed
        };
        activity.finish(
            outcome,
            (outcome == ActivityOutcome::Failed).then_some(ActivityDiagnostic::WorkerFailed),
            ActivityRetirement::Unavailable,
        );
    }
    result
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SaveOutcome {
    Saved,
    OverwriteRequired,
}

fn save_paths_current(parent: &Path, target: &Path) -> bool {
    if validate_local_path(parent).is_err() || validate_local_path(target).is_err() {
        return false;
    }
    #[cfg(windows)]
    if crate::xml_toolkit_engine::validate_local_xml_path(parent).is_err() {
        return false;
    }
    #[cfg(not(windows))]
    if reject_symlink_components(parent).is_err() {
        return false;
    }
    if !fs::metadata(parent).is_ok_and(|metadata| metadata.is_dir()) {
        return false;
    }
    match fs::symlink_metadata(target) {
        Ok(metadata) => {
            if !metadata.is_file() || metadata.file_type().is_symlink() {
                return false;
            }
            #[cfg(windows)]
            if crate::xml_toolkit_engine::validate_local_xml_path(target).is_err() {
                return false;
            }
            #[cfg(not(windows))]
            if reject_symlink_components(target).is_err() {
                return false;
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => true,
        Err(_) => false,
    }
}

pub fn save_xml_file(
    path: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
) -> Result<SaveOutcome, String> {
    save_xml_file_if(path, payload, overwrite_confirmed, || true)
}

pub fn save_xml_file_if(
    path: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
    is_current: impl Fn() -> bool,
) -> Result<SaveOutcome, String> {
    validate_local_path(path)?;
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    validate_local_path(parent)?;
    #[cfg(windows)]
    crate::xml_toolkit_engine::validate_local_xml_path(parent)?;
    #[cfg(not(windows))]
    reject_symlink_components(parent)?;
    let resolved_parent = parent
        .canonicalize()
        .map_err(|error| format!("Could not resolve the destination folder: {error}"))?;
    validate_local_path(&resolved_parent)?;
    #[cfg(windows)]
    crate::xml_toolkit_engine::validate_local_xml_path(&resolved_parent)?;
    let parent_metadata = fs::metadata(&resolved_parent)
        .map_err(|error| format!("Could not inspect the destination folder: {error}"))?;
    if !parent_metadata.is_dir() {
        return Err("The destination folder is not a directory.".into());
    }
    let file_name = path
        .file_name()
        .ok_or_else(|| "Choose a destination file name.".to_owned())?;
    let target = resolved_parent.join(file_name);
    validate_local_path(&target)?;
    let target_exists = match fs::symlink_metadata(&target) {
        Ok(metadata) => {
            #[cfg(windows)]
            crate::xml_toolkit_engine::validate_local_xml_path(&target)?;
            #[cfg(not(windows))]
            reject_symlink_components(&target)?;
            if !metadata.is_file() {
                return Err("The destination must be a regular file.".into());
            }
            true
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => return Err(format!("Could not inspect the destination file: {error}")),
    };
    if target_exists && !overwrite_confirmed {
        return Ok(SaveOutcome::OverwriteRequired);
    }

    let can_commit = || save_paths_current(&resolved_parent, &target) && is_current();
    if !can_commit() {
        return Err(
            "XML save request was superseded or its target is no longer admissible.".into(),
        );
    }
    if overwrite_confirmed {
        crate::common::atomic_file::save_atomic_if(&target, payload, &can_commit)
            .map_err(|error| format!("Could not save XML file: {error:#}"))?;
    } else {
        let mut temporary = tempfile::NamedTempFile::new_in(&resolved_parent)
            .map_err(|error| format!("Could not prepare XML file: {error}"))?;
        temporary
            .write_all(payload)
            .and_then(|()| temporary.flush())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| format!("Could not write XML file: {error}"))?;
        if !can_commit() {
            return Err(
                "XML save request was superseded or its target is no longer admissible.".into(),
            );
        }
        if let Err(error) = temporary.persist_noclobber(&target) {
            if error.error.kind() == std::io::ErrorKind::AlreadyExists {
                return Ok(SaveOutcome::OverwriteRequired);
            }
            return Err(format!(
                "Could not create XML file without overwriting: {}",
                error.error
            ));
        }
    }
    Ok(SaveOutcome::Saved)
}

pub fn run_xml_save(
    path: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
    activity: Option<&crate::activity::ActivityTask>,
) -> Result<SaveOutcome, String> {
    run_xml_save_if(path, payload, overwrite_confirmed, activity, || true)
}

pub fn run_xml_save_if(
    path: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
    activity: Option<&crate::activity::ActivityTask>,
    is_current: impl Fn() -> bool,
) -> Result<SaveOutcome, String> {
    use crate::activity::{ActivityDiagnostic, ActivityOutcome, ActivityRetirement};
    if let Some(activity) = activity {
        activity.mark_running();
    }
    let result = save_xml_file_if(path, payload, overwrite_confirmed, is_current);
    if let Some(activity) = activity {
        let outcome = if result.is_ok() {
            ActivityOutcome::Completed
        } else {
            ActivityOutcome::Failed
        };
        activity.finish(
            outcome,
            (outcome == ActivityOutcome::Failed).then_some(ActivityDiagnostic::WorkerFailed),
            ActivityRetirement::Unavailable,
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::run_xml_load;
    use crate::activity::{
        ActivityCenter, ActivityIncarnation, ActivityLifecycle, ActivityOperation, ActivityOutcome,
        ActivityOwner, ActivitySlot,
    };

    #[test]
    fn xml_save_race_never_clobbers_and_explicit_confirmation_replaces() {
        use super::{save_xml_file, save_xml_file_if, SaveOutcome};
        use std::cell::Cell;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("race.xml");
        let calls = Cell::new(0);
        let outcome = save_xml_file_if(&path, b"frozen", false, || {
            calls.set(calls.get() + 1);
            if calls.get() == 2 {
                std::fs::write(&path, b"other").unwrap();
            }
            true
        })
        .unwrap();
        assert_eq!(outcome, SaveOutcome::OverwriteRequired);
        assert_eq!(std::fs::read(&path).unwrap(), b"other");
        assert_eq!(
            save_xml_file(&path, b"frozen", true).unwrap(),
            SaveOutcome::Saved
        );
        assert_eq!(std::fs::read(&path).unwrap(), b"frozen");
    }
    #[test]
    fn xml_save_revoked_before_commit_leaves_target_unchanged() {
        use super::save_xml_file_if;
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("target.xml");
        std::fs::write(&path, b"original").unwrap();
        let calls = std::cell::Cell::new(0);
        assert!(save_xml_file_if(&path, b"replacement", true, || {
            calls.set(calls.get() + 1);
            calls.get() == 1
        })
        .is_err());
        assert_eq!(calls.get(), 2);
        assert_eq!(std::fs::read(&path).unwrap(), b"original");
    }

    #[test]
    fn xml_load_runner_publishes_activity_for_exact_secure_read() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("source.xml");
        std::fs::write(
            &path,
            "\u{feff}<?xml version='1.0' encoding='UTF-8'?><root>é</root>",
        )
        .unwrap();
        let center = ActivityCenter::new();
        let task = center
            .start(
                ActivityOwner::Xml,
                ActivitySlot::Primary,
                ActivityIncarnation::new(91),
                ActivityOperation::LoadXmlFile,
            )
            .unwrap();
        assert_eq!(
            run_xml_load(&path, Some(&task)).unwrap(),
            "<?xml version='1.0' encoding='UTF-8'?><root>é</root>"
        );
        assert!(center
            .snapshot()
            .rows
            .iter()
            .any(|row| row.operation == ActivityOperation::LoadXmlFile
                && row.lifecycle == ActivityLifecycle::Terminal(ActivityOutcome::Completed)));
    }
}
