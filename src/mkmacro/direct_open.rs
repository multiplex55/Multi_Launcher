//! Typed, validated targets for the Macro direct-open actions.
//!
//! This module owns template preparation and target validation. Native handoff
//! stays in the executor's launcher backend so it remains injectable in tests.

use super::{
    DiagnosticKind, RuntimeVariables,
    interpolation::{TemplatePart, interpolate, scan_template},
};
use std::{
    fmt, fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectOpenError {
    Empty,
    MalformedTemplate,
    MissingVariable,
    NullVariable,
    InvalidUrl,
    InvalidFolderPath,
    FolderUnavailable,
    NotDirectory,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DirectOpenField {
    FolderPath,
    Url,
}

impl DirectOpenField {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::FolderPath => "folder.path",
            Self::Url => "url.url",
        }
    }
}

impl DirectOpenError {
    pub(crate) fn safe_message(self, field: DirectOpenField) -> &'static str {
        match (field, self) {
            (_, Self::Empty) => "A value is required",
            (_, Self::MalformedTemplate) => "The interpolation template is malformed",
            (_, Self::MissingVariable) => "A referenced variable is unavailable",
            (_, Self::NullVariable) => "A referenced value cannot be null",
            (DirectOpenField::Url, _) => "Enter a valid absolute HTTP or HTTPS URL",
            (DirectOpenField::FolderPath, Self::InvalidFolderPath) => {
                "Enter an absolute folder path without a device namespace"
            }
            (DirectOpenField::FolderPath, Self::FolderUnavailable) => {
                "The folder does not exist or cannot be accessed"
            }
            (DirectOpenField::FolderPath, Self::NotDirectory) => {
                "The path does not refer to a folder"
            }
            _ => "The direct-open target is invalid",
        }
    }
}

/// A validated absolute directory. Debug output intentionally omits the path.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenFolderTarget(PathBuf);

impl OpenFolderTarget {
    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl fmt::Debug for OpenFolderTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OpenFolderTarget")
            .field(&"[redacted]")
            .finish()
    }
}

/// A validated absolute HTTP(S) URL. Debug output intentionally omits the URL.
#[derive(Clone, PartialEq, Eq)]
pub struct OpenUrlTarget(String);

impl OpenUrlTarget {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for OpenUrlTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_tuple("OpenUrlTarget").field(&"[redacted]").finish()
    }
}

pub(crate) fn validate_open_folder_template(template: &str) -> Result<(), DirectOpenError> {
    if template.trim().is_empty() {
        return Err(DirectOpenError::Empty);
    }
    for part in scan_template(template) {
        part.map_err(|_| DirectOpenError::MalformedTemplate)?;
    }
    Ok(())
}

pub(crate) fn validate_open_url_template(template: &str) -> Result<(), DirectOpenError> {
    if template.trim().is_empty() {
        return Err(DirectOpenError::Empty);
    }

    let mut literal = String::with_capacity(template.len());
    let mut has_reference = false;
    for part in scan_template(template) {
        match part.map_err(|_| DirectOpenError::MalformedTemplate)? {
            TemplatePart::Text(text) => literal.push_str(text),
            TemplatePart::EscapedReference(name) => {
                literal.push_str("${");
                literal.push_str(name);
                literal.push('}');
            }
            TemplatePart::Reference(_) => has_reference = true,
        }
    }

    // A real variable can alter the URL authority, so defer URL semantics until
    // the one runtime expansion has produced the final value. Escaped
    // references remain literal text and are validated now.
    if !has_reference {
        validate_absolute_http_url(&literal)?;
    }
    Ok(())
}

pub(crate) fn interpolate_template(
    template: &str,
    variables: &RuntimeVariables,
) -> Result<String, DirectOpenError> {
    let output = interpolate(template, variables).map_err(|error| match error.kind {
        DiagnosticKind::TypeMismatch => DirectOpenError::NullVariable,
        DiagnosticKind::InvalidTarget if error.context.contains_key("variable") => {
            DirectOpenError::MissingVariable
        }
        _ => DirectOpenError::MalformedTemplate,
    })?;
    if output.trim().is_empty() {
        return Err(DirectOpenError::Empty);
    }
    Ok(output)
}

pub(crate) fn prepare_open_folder(
    template: &str,
    variables: &RuntimeVariables,
) -> Result<OpenFolderTarget, DirectOpenError> {
    validate_open_folder_template(template)?;
    let expanded = interpolate_template(template, variables)?;
    validate_folder_path(&expanded)?;

    let canonical = fs::canonicalize(&expanded).map_err(|_| DirectOpenError::FolderUnavailable)?;
    let metadata = fs::metadata(&canonical).map_err(|_| DirectOpenError::FolderUnavailable)?;
    if !metadata.is_dir() {
        return Err(DirectOpenError::NotDirectory);
    }
    if !canonical.is_absolute() {
        return Err(DirectOpenError::InvalidFolderPath);
    }

    Ok(OpenFolderTarget(normalize_explorer_path(canonical)))
}

pub(crate) fn prepare_open_url(
    template: &str,
    variables: &RuntimeVariables,
) -> Result<OpenUrlTarget, DirectOpenError> {
    validate_open_url_template(template)?;
    let expanded = interpolate_template(template, variables)?;
    validate_absolute_http_url(&expanded)?;
    Ok(OpenUrlTarget(expanded))
}

fn validate_folder_path(value: &str) -> Result<(), DirectOpenError> {
    if value.is_empty()
        || value.chars().any(char::is_control)
        || !Path::new(value).is_absolute()
        || is_device_namespace(value)
    {
        return Err(DirectOpenError::InvalidFolderPath);
    }
    Ok(())
}

fn is_device_namespace(value: &str) -> bool {
    value.starts_with(r"\\.\") || value.starts_with(r"\\?\") || value.starts_with(r"\??\")
}

fn validate_absolute_http_url(value: &str) -> Result<(), DirectOpenError> {
    if value.is_empty()
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || value.contains('\\')
        || !valid_percent_escapes(value)
    {
        return Err(DirectOpenError::InvalidUrl);
    }

    let Some((scheme, rest)) = value.split_once(':') else {
        return Err(DirectOpenError::InvalidUrl);
    };
    if !matches!(scheme.to_ascii_lowercase().as_str(), "http" | "https") || !rest.starts_with("//")
    {
        return Err(DirectOpenError::InvalidUrl);
    }
    let authority = rest[2..].split(['/', '?', '#']).next().unwrap_or_default();
    if authority.is_empty() {
        return Err(DirectOpenError::InvalidUrl);
    }

    let parsed = url::Url::parse(value).map_err(|_| DirectOpenError::InvalidUrl)?;
    if !matches!(parsed.scheme(), "http" | "https") || parsed.host_str().is_none() {
        return Err(DirectOpenError::InvalidUrl);
    }
    Ok(())
}

fn valid_percent_escapes(value: &str) -> bool {
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            if index + 2 >= bytes.len()
                || !bytes[index + 1].is_ascii_hexdigit()
                || !bytes[index + 2].is_ascii_hexdigit()
            {
                return false;
            }
            index += 3;
        } else {
            index += 1;
        }
    }
    true
}

fn normalize_explorer_path(path: PathBuf) -> PathBuf {
    #[cfg(windows)]
    {
        let text = path.to_string_lossy();
        if let Some(rest) = text.strip_prefix(r"\\?\UNC\") {
            return PathBuf::from(format!(r"\\{rest}"));
        }
        if let Some(rest) = text.strip_prefix(r"\\?\") {
            return PathBuf::from(rest);
        }
    }
    path
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mkmacro::{MkPoint, MkValue};

    #[test]
    fn direct_open_url_accepts_http_and_https_without_rewriting_query_or_fragment() {
        for value in [
            "http://example.test/path?q=one%20two#section",
            "HTTPS://example.test:8443/path?token=a%2Fb&x=1#fragment",
        ] {
            let target = prepare_open_url(value, &RuntimeVariables::new()).unwrap();
            assert_eq!(target.as_str(), value);
            assert!(!format!("{target:?}").contains("example.test"));
        }
    }

    #[test]
    fn direct_open_url_rejects_non_absolute_malformed_and_unsafe_values() {
        for value in [
            "example.test",
            "http:/example.test",
            "https:example.test",
            "https:///missing-host",
            "http://",
            "http://example.test/%GG",
            "http://exa mple.test/",
            "http://example.test/path\nnext",
            "http://example.test/path\\other",
            "file:///C:/secret.txt",
            "javascript:alert(1)",
            "custom://example.test/path",
            "example search terms",
        ] {
            assert_eq!(
                prepare_open_url(value, &RuntimeVariables::new()).unwrap_err(),
                DirectOpenError::InvalidUrl,
                "{value:?}"
            );
        }
    }

    #[test]
    fn direct_open_url_draft_validation_distinguishes_real_and_escaped_variables() {
        assert!(validate_open_url_template("https://example.test/path?q=${query}").is_ok());
        assert!(validate_open_url_template("https://${host}/$${literal}").is_ok());
        assert!(validate_open_url_template("https://example.test/$${literal}").is_ok());
        assert_eq!(
            validate_open_url_template("example.test").unwrap_err(),
            DirectOpenError::InvalidUrl
        );
        assert_eq!(
            validate_open_url_template("https://example.test/${").unwrap_err(),
            DirectOpenError::MalformedTemplate
        );
    }

    #[test]
    fn direct_open_template_uses_shared_interpolation_formatting_and_redacts_errors() {
        let variables = [
            ("host".into(), MkValue::String("example.test".into())),
            ("number".into(), MkValue::Number(3.0)),
            ("enabled".into(), MkValue::Boolean(true)),
            ("mouse.x".into(), MkValue::Point(MkPoint { x: 2, y: 5 })),
            ("empty".into(), MkValue::Null),
        ]
        .into_iter()
        .collect();
        let target = prepare_open_url(
            "https://${host}/$${literal}?n=${missing_literal}%20x",
            &[
                ("host".into(), MkValue::String("example.test".into())),
                ("missing_literal".into(), MkValue::String("a%2Fb".into())),
            ]
            .into_iter()
            .collect(),
        )
        .unwrap();
        assert_eq!(
            target.as_str(),
            "https://example.test/${literal}?n=a%2Fb%20x"
        );
        let target = prepare_open_url(
            "https://example.test/?page=${number}&enabled=${enabled}&point=${mouse.x}",
            &variables,
        )
        .unwrap();
        assert_eq!(
            target.as_str(),
            "https://example.test/?page=3&enabled=true&point=2,5"
        );

        assert_eq!(
            interpolate_template("${missing}", &variables),
            Err(DirectOpenError::MissingVariable)
        );
        assert_eq!(
            interpolate_template("${empty}", &variables),
            Err(DirectOpenError::NullVariable)
        );
        assert!(
            !DirectOpenError::MissingVariable
                .safe_message(DirectOpenField::Url)
                .contains("missing")
        );
    }

    #[test]
    fn direct_open_folder_requires_existing_absolute_directories() {
        let root = tempfile::tempdir().unwrap();
        let folder = root.path().join("folder with spaces – café");
        fs::create_dir(&folder).unwrap();
        let file = folder.join("file.txt");
        fs::write(&file, "fixture").unwrap();

        let target =
            prepare_open_folder(folder.to_str().unwrap(), &RuntimeVariables::new()).unwrap();
        assert!(target.path().is_absolute());
        assert!(target.path().is_dir());
        assert!(!format!("{target:?}").contains("café"));

        assert_eq!(
            prepare_open_folder(file.to_str().unwrap(), &RuntimeVariables::new()).unwrap_err(),
            DirectOpenError::NotDirectory
        );
        assert_eq!(
            prepare_open_folder(
                root.path().join("missing").to_str().unwrap(),
                &RuntimeVariables::new()
            )
            .unwrap_err(),
            DirectOpenError::FolderUnavailable
        );
        assert_eq!(
            prepare_open_folder("relative folder", &RuntimeVariables::new()).unwrap_err(),
            DirectOpenError::InvalidFolderPath
        );
    }

    #[test]
    fn direct_open_folder_path_policy_accepts_unc_without_accessing_a_share() {
        #[cfg(windows)]
        {
            assert!(validate_folder_path(r"\\server\share\folder").is_ok());
            assert_eq!(
                validate_folder_path(r"\\.\C:\Windows"),
                Err(DirectOpenError::InvalidFolderPath)
            );
            assert_eq!(
                validate_folder_path(r"\\?\C:\Windows"),
                Err(DirectOpenError::InvalidFolderPath)
            );
        }
    }
}
