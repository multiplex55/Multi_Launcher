//! Checked persistence for Calculation Pad result formatting.
use super::*;
use crate::settings::{
    CalculationPadDisplayPreferences, CalculationPadExactMode, CalculationPadNumberMode,
};
use std::io::Write;
use std::path::PathBuf;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CalculationPadResultFormat {
    pub number_mode: CalculationPadNumberMode,
    pub exact_mode: CalculationPadExactMode,
    pub decimal_places: u8,
    pub group_digits: bool,
    pub percent: bool,
}
impl From<CalculationPadDisplayPreferences> for CalculationPadResultFormat {
    fn from(value: CalculationPadDisplayPreferences) -> Self {
        Self {
            number_mode: value.number_mode,
            exact_mode: value.exact_mode,
            decimal_places: value.decimal_places,
            group_digits: value.group_digits,
            percent: value.percent,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CalculationPadFormatSnapshot {
    pub display: CalculationPadDisplayPreferences,
    path: PathBuf,
    raw: Option<String>,
    owned: Option<[Option<String>; 5]>,
    positional: bool,
}
const FORMAT_KEYS: [&str; 5] = [
    "number_mode",
    "exact_mode",
    "decimal_places",
    "group_digits",
    "percent",
];
fn same_baseline(
    expected: &CalculationPadFormatSnapshot,
    actual: &CalculationPadFormatSnapshot,
) -> bool {
    expected.path == actual.path
        && expected.positional == actual.positional
        && CalculationPadResultFormat::from(expected.display)
            == CalculationPadResultFormat::from(actual.display)
        && match (&expected.owned, &actual.owned) {
            (Some(left), Some(right)) => left == right,
            (None, None) => expected.raw == actual.raw,
            _ => false,
        }
}
#[derive(Clone, Debug, PartialEq)]
pub struct CalculationPadFormatAcknowledgement {
    pub snapshot: CalculationPadFormatSnapshot,
    pub persisted: bool,
}
#[derive(Debug)]
pub enum CalculationPadFormatError {
    Canceled,
    Conflict,
    InvalidPrecision,
    Store(String),
    CommittedUnverified(String),
}
impl std::fmt::Display for CalculationPadFormatError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Canceled => f.write_str("Calculation Pad format save was canceled"),
            Self::Conflict => {
                f.write_str("Calculation Pad display settings changed; reload before saving")
            }
            Self::InvalidPrecision => f.write_str("Decimal places must be between 0 and 15"),
            Self::Store(error) => f.write_str(error),
            Self::CommittedUnverified(error) => write!(
                f,
                "Calculation Pad format was saved but could not be verified: {error}"
            ),
        }
    }
}
impl std::error::Error for CalculationPadFormatError {}

enum Root {
    Object(RawSettingsObject),
    Positional(Vec<Box<serde_json::value::RawValue>>),
}
fn source(path: &Path) -> std::io::Result<Option<Vec<u8>>> {
    match std::fs::read(path) {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e),
    }
}
fn parse_root(bytes: Option<&[u8]>) -> anyhow::Result<Root> {
    match bytes {
        None => Ok(Root::Object(RawSettingsObject::new())),
        Some(v) if v.iter().all(u8::is_ascii_whitespace) => {
            Ok(Root::Object(RawSettingsObject::new()))
        }
        Some(v) if v.iter().copied().find(|b| !b.is_ascii_whitespace()) == Some(b'[') => {
            Ok(Root::Positional(serde_json::from_slice(v)?))
        }
        Some(v) => Ok(Root::Object(super::omni_preferences::unique_object(
            std::str::from_utf8(v)?,
        )?)),
    }
}
fn inspect(
    path: &Path,
    bytes: Option<&[u8]>,
    root: &Root,
) -> anyhow::Result<(
    CalculationPadFormatSnapshot,
    usize,
    Vec<(String, Box<serde_json::value::RawValue>)>,
)> {
    let settings = parse_effective_settings(path, bytes.unwrap_or_default())?;
    let fields = super::omni_preferences::ordered_fields(&settings)?;
    let position = fields
        .iter()
        .position(|(key, _)| key == "calculation_pad_display")
        .ok_or_else(|| anyhow::anyhow!("Settings schema has no Calculation Pad display"))?;
    let raw = match root {
        Root::Object(map) => map.get("calculation_pad_display"),
        Root::Positional(values) => values.get(position),
    }
    .map(|v| v.get().to_owned());
    let owned = match &raw {
        None => Some(std::array::from_fn(|_| None)),
        Some(raw) => super::omni_preferences::unique_object(raw)
            .ok()
            .map(|object| {
                std::array::from_fn(|index| {
                    object
                        .get(FORMAT_KEYS[index])
                        .map(|value| value.get().to_owned())
                })
            }),
    };
    Ok((
        CalculationPadFormatSnapshot {
            display: settings.calculation_pad_display,
            path: path.to_owned(),
            raw,
            owned,
            positional: matches!(root, Root::Positional(_)),
        },
        position,
        fields,
    ))
}
impl Settings {
    pub fn calculation_pad_format_snapshot(
        path: impl AsRef<Path>,
    ) -> Result<CalculationPadFormatSnapshot, CalculationPadFormatError> {
        let path = path.as_ref();
        let _guard = transaction_guard();
        let bytes = source(path).map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
        let root = parse_root(bytes.as_deref())
            .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
        inspect(path, bytes.as_deref(), &root)
            .map(|v| v.0)
            .map_err(|e| CalculationPadFormatError::Store(e.to_string()))
    }
    pub fn save_calculation_pad_format_checked(
        path: impl AsRef<Path>,
        baseline: &CalculationPadFormatSnapshot,
        desired: CalculationPadResultFormat,
        current: impl Fn() -> bool,
    ) -> Result<CalculationPadFormatAcknowledgement, CalculationPadFormatError> {
        save(path.as_ref(), baseline, desired, None, current)
    }
    pub(crate) fn save_calculation_pad_format_cancellable(
        path: &Path,
        baseline: &CalculationPadFormatSnapshot,
        desired: CalculationPadResultFormat,
        token: &SettingsBooleanSaveToken,
        current: impl Fn() -> bool,
    ) -> Result<CalculationPadFormatAcknowledgement, CalculationPadFormatError> {
        if !token.mark_waiting_for_settings() {
            return Err(CalculationPadFormatError::Canceled);
        }
        save(path, baseline, desired, Some(token), current)
    }
}
fn save(
    path: &Path,
    baseline: &CalculationPadFormatSnapshot,
    desired: CalculationPadResultFormat,
    token: Option<&SettingsBooleanSaveToken>,
    current: impl Fn() -> bool,
) -> Result<CalculationPadFormatAcknowledgement, CalculationPadFormatError> {
    let _guard = transaction_guard();
    let active = || current() && !token.is_some_and(SettingsBooleanSaveToken::is_canceled);
    if desired.decimal_places > 15 {
        return Err(CalculationPadFormatError::InvalidPrecision);
    }
    if !active() {
        return Err(CalculationPadFormatError::Canceled);
    }
    let bytes = source(path).map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    let mut root = parse_root(bytes.as_deref())
        .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    let (actual, position, fields) = inspect(path, bytes.as_deref(), &root)
        .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    if !same_baseline(baseline, &actual) {
        return Err(CalculationPadFormatError::Conflict);
    }
    if CalculationPadResultFormat::from(actual.display) == desired {
        if !active() {
            return Err(CalculationPadFormatError::Canceled);
        }
        if source(path).ok().as_ref() != Some(&bytes) {
            return Err(CalculationPadFormatError::Conflict);
        }
        return Ok(CalculationPadFormatAcknowledgement {
            snapshot: actual,
            persisted: false,
        });
    }
    let mut group = match &actual.raw {
        None => RawSettingsObject::new(),
        Some(raw) => super::omni_preferences::unique_object(raw).map_err(|e| {
            CalculationPadFormatError::Store(format!(
                "Calculation Pad display must be an unambiguous object: {e}"
            ))
        })?,
    };
    let old = CalculationPadResultFormat::from(actual.display);
    macro_rules! set_changed {
        ($field:ident) => {
            if old.$field != desired.$field {
                group.insert(
                    stringify!($field).into(),
                    serde_json::value::to_raw_value(&desired.$field)
                        .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?,
                );
            }
        };
    }
    set_changed!(number_mode);
    set_changed!(exact_mode);
    set_changed!(decimal_places);
    set_changed!(group_digits);
    set_changed!(percent);
    let group = serde_json::value::to_raw_value(&group)
        .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    match &mut root {
        Root::Object(map) => {
            map.insert("calculation_pad_display".into(), group);
        }
        Root::Positional(values) => {
            while values.len() <= position {
                values.push(fields[values.len()].1.clone());
            }
            values[position] = group;
        }
    }
    let candidate = match &root {
        Root::Object(map) => serde_json::to_vec(map),
        Root::Positional(values) => serde_json::to_vec(values),
    }
    .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    let (expected, _, _) = inspect(path, Some(&candidate), &root)
        .map_err(|e| CalculationPadFormatError::Store(e.to_string()))?;
    if CalculationPadResultFormat::from(expected.display) != desired {
        return Err(CalculationPadFormatError::Store(
            "Calculation Pad format failed round trip".into(),
        ));
    }
    let claimed = std::cell::Cell::new(false);
    let canceled = std::cell::Cell::new(false);
    let conflict = std::cell::Cell::new(false);
    let final_check = || {
        if !active() || (!claimed.get() && token.is_some_and(|t| !t.begin_irrevocable_commit())) {
            canceled.set(true);
            return false;
        }
        claimed.set(true);
        if !active() {
            canceled.set(true);
            return false;
        }
        if source(path).ok().as_ref() != Some(&bytes) {
            conflict.set(true);
            return false;
        }
        true
    };
    let result: anyhow::Result<()> = if bytes.is_some() {
        crate::common::atomic_file::save_atomic_if(path, &candidate, final_check)
    } else {
        (|| {
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."));
            let mut staged = tempfile::NamedTempFile::new_in(parent)?;
            staged.write_all(&candidate)?;
            staged.flush()?;
            staged.as_file().sync_all()?;
            anyhow::ensure!(final_check(), "Calculation Pad format save superseded");
            staged.persist_noclobber(path).map_err(|e| e.error)?;
            Ok(())
        })()
    };
    if let Err(error) = result {
        if canceled.get() {
            return Err(CalculationPadFormatError::Canceled);
        }
        if conflict.get() || source(path).ok().as_ref() != Some(&bytes) {
            return Err(CalculationPadFormatError::Conflict);
        }
        return Err(CalculationPadFormatError::Store(error.to_string()));
    }
    if source(path).ok().flatten().as_deref() != Some(candidate.as_slice()) {
        return Err(CalculationPadFormatError::CommittedUnverified(
            "Settings readback differs or is unavailable".into(),
        ));
    }
    Ok(CalculationPadFormatAcknowledgement {
        snapshot: expected,
        persisted: true,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sparse_format_save_preserves_display_and_opaque_siblings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"unknown":{"large":123456789012345678901234567890},"calculation_pad_display":{"word_wrap":true,"opaque":{"duplicate":1,"duplicate":2},"decimal_places":3}}"#).unwrap();
        let baseline = Settings::calculation_pad_format_snapshot(&path).unwrap();
        std::fs::write(&path, br#"{"unknown":{"large":123456789012345678901234567890},"calculation_pad_display":{"word_wrap":false,"opaque":{"duplicate":1,"duplicate":2},"decimal_places":3}}"#).unwrap();
        let mut desired = CalculationPadResultFormat::from(baseline.display);
        desired.number_mode = CalculationPadNumberMode::Fixed;
        let ack = Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || true)
            .unwrap();
        assert!(ack.persisted && !ack.snapshot.display.word_wrap);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("123456789012345678901234567890"));
        assert!(text.contains(r#""duplicate":1,"duplicate":2"#));
        assert!(text.contains(r#""decimal_places":3"#));
        assert!(matches!(
            Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || true),
            Err(CalculationPadFormatError::Conflict)
        ));
    }
    #[test]
    fn late_settings_change_refuses_commit_and_preserves_competitor() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{}").unwrap();
        let baseline = Settings::calculation_pad_format_snapshot(&path).unwrap();
        let mut desired = CalculationPadResultFormat::from(baseline.display);
        desired.percent = true;
        let calls = std::cell::Cell::new(0);
        let result =
            Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || {
                calls.set(calls.get() + 1);
                if calls.get() == 3 {
                    std::fs::write(&path, br#"{"foreign":true}"#).unwrap();
                }
                true
            });
        assert!(matches!(result, Err(CalculationPadFormatError::Conflict)));
        assert_eq!(std::fs::read(&path).unwrap(), br#"{"foreign":true}"#);
    }
    #[test]
    fn absent_display_can_merge_new_unowned_display_sibling() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, b"{}").unwrap();
        let baseline = Settings::calculation_pad_format_snapshot(&path).unwrap();
        std::fs::write(&path, br#"{"calculation_pad_display":{"word_wrap":true}}"#).unwrap();
        let mut desired = CalculationPadResultFormat::from(baseline.display);
        desired.percent = true;
        let ack = Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || true)
            .unwrap();
        assert!(ack.snapshot.display.word_wrap && ack.snapshot.display.percent);
    }
    #[test]
    fn positional_format_preserves_prefix_and_noop_cancel_do_not_write() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.show_inline_errors = false;
        let fields = super::super::omni_preferences::ordered_fields(&settings).unwrap();
        let position = fields
            .iter()
            .position(|(name, _)| name == "calculation_pad_display")
            .unwrap();
        let original: Vec<_> = fields.iter().map(|(_, value)| value.clone()).collect();
        let bytes = serde_json::to_vec(&original).unwrap();
        std::fs::write(&path, &bytes).unwrap();
        assert!(parse_effective_settings(&path, &bytes).is_ok());
        let baseline = Settings::calculation_pad_format_snapshot(&path).unwrap();
        let mut desired = CalculationPadResultFormat::from(baseline.display);
        assert!(
            !Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || true)
                .unwrap()
                .persisted
        );
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        desired.percent = true;
        assert!(matches!(
            Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || false),
            Err(CalculationPadFormatError::Canceled)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
        Settings::save_calculation_pad_format_checked(&path, &baseline, desired, || true).unwrap();
        let result: Vec<Box<serde_json::value::RawValue>> =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(result.len(), original.len());
        for (index, (actual, expected)) in result.iter().zip(&original).enumerate() {
            if index != position {
                assert_eq!(actual.get(), expected.get(), "unowned position {index}");
            }
        }
    }
}
