#[cfg(windows)]
use std::fs::OpenOptions;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::Path;
#[cfg(not(windows))]
use std::path::PathBuf;

use crate::xml_toolkit_engine::MAX_XML_INPUT_BYTES;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SaveOutcome {
    Saved,
    OverwriteRequired,
}

pub(super) fn validate_local_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Choose an absolute local file path.".into());
    }

    #[cfg(windows)]
    crate::xml_toolkit_engine::validate_xml_path_spelling(path)?;

    #[cfg(not(windows))]
    if path.as_os_str().to_string_lossy().starts_with("//") {
        return Err("Network paths are not supported for XML files.".into());
    }

    Ok(())
}

pub(super) fn read_xml_file(path: &Path) -> Result<String, String> {
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
fn reject_symlink_components(path: &Path) -> Result<(), String> {
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

pub(super) fn validate_utf8_declaration(source: &str) -> Result<(), String> {
    let Some(after_open) = source.strip_prefix("<?xml") else {
        return Ok(());
    };
    if !after_open.starts_with("?>") && !after_open.chars().next().is_some_and(char::is_whitespace)
    {
        // Processing-instruction targets such as `xml-stylesheet` are not an
        // XML declaration and remain the XML parser's responsibility.
        return Ok(());
    }
    let end = source
        .find("?>")
        .ok_or_else(|| "XML declaration is incomplete.".to_owned())?;
    let declaration = &source[5..end];
    let Some(encoding) = declaration_encoding(declaration)? else {
        return Ok(());
    };
    if !encoding.eq_ignore_ascii_case("utf-8") {
        return Err(format!(
            "XML declares unsupported encoding {encoding:?}; only UTF-8 is supported."
        ));
    }
    Ok(())
}

fn declaration_encoding(declaration: &str) -> Result<Option<&str>, String> {
    let bytes = declaration.as_bytes();
    let mut cursor = 0usize;
    while cursor < bytes.len() {
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor == bytes.len() {
            break;
        }
        let name_start = cursor;
        while cursor < bytes.len()
            && (bytes[cursor].is_ascii_alphanumeric()
                || bytes[cursor] == b'_'
                || bytes[cursor] == b'-')
        {
            cursor += 1;
        }
        if cursor == name_start {
            cursor += 1;
            continue;
        }
        let name = &declaration[name_start..cursor];
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        if cursor >= bytes.len() || bytes[cursor] != b'=' {
            continue;
        }
        cursor += 1;
        while cursor < bytes.len() && bytes[cursor].is_ascii_whitespace() {
            cursor += 1;
        }
        let Some(quote @ (b'\'' | b'"')) = bytes.get(cursor).copied() else {
            continue;
        };
        cursor += 1;
        let value_start = cursor;
        while cursor < bytes.len() && bytes[cursor] != quote {
            cursor += 1;
        }
        if cursor == bytes.len() {
            return Err("XML declaration has an unterminated attribute.".into());
        }
        let value = &declaration[value_start..cursor];
        cursor += 1;
        if name == "encoding" {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

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
