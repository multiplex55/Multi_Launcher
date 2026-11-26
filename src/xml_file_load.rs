//! Secure, bounded local XML file loading shared by both frontends.
use crate::xml_source::{validate_local_path, validate_utf8_declaration};
use crate::xml_toolkit_engine::MAX_XML_INPUT_BYTES;
#[cfg(not(windows))]
use std::fs;
use std::fs::File;
#[cfg(windows)]
use std::fs::OpenOptions;
use std::io::Read;
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

#[cfg(test)]
mod tests {
    use super::run_xml_load;
    use crate::activity::{
        ActivityCenter, ActivityIncarnation, ActivityLifecycle, ActivityOperation, ActivityOutcome,
        ActivityOwner, ActivitySlot,
    };

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
