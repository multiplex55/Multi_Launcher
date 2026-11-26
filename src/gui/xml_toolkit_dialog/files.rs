use std::fs;
use std::io::Write;
use std::path::Path;

#[cfg(test)]
use crate::xml_toolkit_engine::MAX_XML_INPUT_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SaveOutcome {
    Saved,
    OverwriteRequired,
}

pub(super) use crate::xml_source::validate_local_path;

#[cfg(test)]
use crate::xml_file_load::read_xml_file;
#[cfg(not(windows))]
use crate::xml_file_load::reject_symlink_components;

pub(super) fn save_xml_file(
    path: &Path,
    payload: &[u8],
    overwrite_confirmed: bool,
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

    if overwrite_confirmed {
        crate::common::atomic_file::save_atomic(&target, payload)
            .map_err(|error| format!("Could not save XML file: {error:#}"))?;
    } else {
        let mut temporary = tempfile::NamedTempFile::new_in(&resolved_parent)
            .map_err(|error| format!("Could not prepare XML file: {error}"))?;
        temporary
            .write_all(payload)
            .and_then(|()| temporary.flush())
            .and_then(|()| temporary.as_file().sync_all())
            .map_err(|error| format!("Could not write XML file: {error}"))?;
        temporary.persist_noclobber(&target).map_err(|error| {
            format!(
                "Could not create XML file without overwriting: {}",
                error.error
            )
        })?;
    }
    Ok(SaveOutcome::Saved)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn m7f_xml_utf8_files_accept_bom_and_utf8_declarations_and_reject_unsupported_encoding() {
        let directory = tempfile::tempdir().expect("disposable XML directory");
        let path = directory.path().join("source.xml");
        fs::write(
            &path,
            "\u{FEFF}<?xml version='1.0' encoding='UTF-8'?><root>é🦀</root>".as_bytes(),
        )
        .expect("write UTF-8 BOM fixture");
        assert_eq!(
            read_xml_file(&path).expect("read UTF-8 XML"),
            "<?xml version='1.0' encoding='UTF-8'?><root>é🦀</root>"
        );

        fs::write(&path, b"<?xml version='1.0' encoding='UTF-16'?><root/>")
            .expect("write unsupported declaration fixture");
        assert!(
            read_xml_file(&path)
                .unwrap_err()
                .contains("only UTF-8 is supported")
        );
        fs::write(&path, [0xFF, 0xFE, b'<', 0]).expect("write invalid UTF-8 fixture");
        assert!(
            read_xml_file(&path)
                .unwrap_err()
                .contains("not valid UTF-8")
        );
    }

    #[test]
    fn m7f_xml_file_size_and_atomic_save_confirmation_are_enforced() {
        let directory = tempfile::tempdir().expect("disposable XML directory");
        let input = directory.path().join("oversized.xml");
        let output = directory.path().join("result.xml");
        fs::write(&input, vec![b'x'; MAX_XML_INPUT_BYTES + 1])
            .expect("write bounded oversize fixture");
        assert!(
            read_xml_file(&input)
                .unwrap_err()
                .contains("limited to 4194304")
        );

        assert_eq!(
            save_xml_file(&output, "<root>é</root>".as_bytes(), false)
                .expect("create a new result"),
            SaveOutcome::Saved
        );
        assert_eq!(
            fs::read(&output).expect("read saved result"),
            "<root>é</root>".as_bytes()
        );
        assert_eq!(
            save_xml_file(&output, b"<root/>", false)
                .expect("existing destination needs confirmation"),
            SaveOutcome::OverwriteRequired
        );
        assert_eq!(
            fs::read(&output).expect("overwrite was not implicit"),
            "<root>é</root>".as_bytes()
        );
        assert_eq!(
            save_xml_file(&output, b"<root/>", true).expect("confirmed overwrite"),
            SaveOutcome::Saved
        );
        assert_eq!(fs::read(&output).expect("read replacement"), b"<root/>");
    }

    #[cfg(windows)]
    #[test]
    fn m7f_xml_file_paths_reject_unc_streams_and_dos_devices() {
        assert!(validate_local_path(Path::new(r"\\server\share\source.xml")).is_err());
        assert!(validate_local_path(Path::new(r"C:\local\source.xml:payload")).is_err());
        assert!(validate_local_path(Path::new(r"C:\local\NUL.xml")).is_err());
        assert!(validate_local_path(Path::new(r"C:\local\COM1.txt")).is_err());
        assert!(validate_local_path(Path::new(r"C:\local\source.xml")).is_ok());
    }
}
