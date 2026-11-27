//! Typed sparse edits shared by the Settings frontends.
use super::*;
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ScopedSetting {
    ClearQueryAfterRun,
    HideAfterRun,
    EnableToasts,
    ToastDuration,
    ShowInlineErrors,
    ShowErrorToasts,
    QueryAutocomplete,
    FuzzyWeight,
    UsageWeight,
    MatchExact,
    PageJump,
    GridEnabled,
    GridRows,
    GridColumns,
    GridRespectCapability,
    GridPluginOptOut,
    FollowMouse,
    StaticLocationEnabled,
    StaticPosition,
    StaticSize,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScopedSettingValue {
    Bool(bool),
    Float(f32),
    Count(usize),
    Strings(Vec<String>),
    OptionalPair(Option<(i32, i32)>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScopedSettingChange {
    pub field: ScopedSetting,
    pub baseline: ScopedSettingValue,
    pub desired: ScopedSettingValue,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ScopedSettingsAcknowledgement {
    pub values: Vec<(ScopedSetting, ScopedSettingValue)>,
    pub persisted: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ScopedSettingsError {
    Cancelled,
    Conflict(ScopedSetting),
    Invalid(ScopedSetting),
    Store(String),
}
impl std::fmt::Display for ScopedSettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Cancelled => write!(f, "Settings save was canceled"),
            Self::Conflict(field) => write!(f, "{field:?} changed since the editor was opened"),
            Self::Invalid(field) => write!(f, "{field:?} has an invalid value"),
            Self::Store(message) => f.write_str(message),
        }
    }
}
impl std::error::Error for ScopedSettingsError {}

impl ScopedSetting {
    fn keys(self) -> (&'static str, Option<&'static str>) {
        match self {
            Self::ClearQueryAfterRun => ("clear_query_after_run", None),
            Self::HideAfterRun => ("hide_after_run", None),
            Self::EnableToasts => ("enable_toasts", None),
            Self::ToastDuration => ("toast_duration", None),
            Self::ShowInlineErrors => ("show_inline_errors", None),
            Self::ShowErrorToasts => ("show_error_toasts", None),
            Self::QueryAutocomplete => ("query_autocomplete", None),
            Self::FuzzyWeight => ("fuzzy_weight", None),
            Self::UsageWeight => ("usage_weight", None),
            Self::MatchExact => ("match_exact", None),
            Self::PageJump => ("page_jump", None),
            Self::GridEnabled => ("query_results_layout", Some("enabled")),
            Self::GridRows => ("query_results_layout", Some("rows")),
            Self::GridColumns => ("query_results_layout", Some("cols")),
            Self::GridRespectCapability => {
                ("query_results_layout", Some("respect_plugin_capability"))
            }
            Self::GridPluginOptOut => ("query_results_layout", Some("plugin_opt_out")),
            Self::FollowMouse => ("follow_mouse", None),
            Self::StaticLocationEnabled => ("static_location_enabled", None),
            Self::StaticPosition => ("static_pos", None),
            Self::StaticSize => ("static_size", None),
        }
    }
    pub fn value(self, settings: &Settings) -> ScopedSettingValue {
        use ScopedSettingValue::*;
        match self {
            Self::ClearQueryAfterRun => Bool(settings.clear_query_after_run),
            Self::HideAfterRun => Bool(settings.hide_after_run),
            Self::EnableToasts => Bool(settings.enable_toasts),
            Self::ToastDuration => Float(settings.toast_duration),
            Self::ShowInlineErrors => Bool(settings.show_inline_errors),
            Self::ShowErrorToasts => Bool(settings.show_error_toasts),
            Self::QueryAutocomplete => Bool(settings.query_autocomplete),
            Self::FuzzyWeight => Float(settings.fuzzy_weight),
            Self::UsageWeight => Float(settings.usage_weight),
            Self::MatchExact => Bool(settings.match_exact),
            Self::PageJump => Count(settings.page_jump),
            Self::GridEnabled => Bool(settings.query_results_layout.enabled),
            Self::GridRows => Count(settings.query_results_layout.rows),
            Self::GridColumns => Count(settings.query_results_layout.cols),
            Self::GridRespectCapability => {
                Bool(settings.query_results_layout.respect_plugin_capability)
            }
            Self::GridPluginOptOut => Strings(settings.query_results_layout.plugin_opt_out.clone()),
            Self::FollowMouse => Bool(settings.follow_mouse),
            Self::StaticLocationEnabled => Bool(settings.static_location_enabled),
            Self::StaticPosition => OptionalPair(settings.static_pos),
            Self::StaticSize => OptionalPair(settings.static_size),
        }
    }
    fn valid(self, value: &ScopedSettingValue) -> bool {
        use ScopedSettingValue::*;
        match (self, value) {
            (Self::ToastDuration, Float(v)) => v.is_finite() && (0.1..=5.0).contains(v),
            (Self::FuzzyWeight | Self::UsageWeight, Float(v)) => {
                v.is_finite() && (0.0..=5.0).contains(v)
            }
            (Self::PageJump | Self::GridRows | Self::GridColumns, Count(v)) => {
                (1..=100).contains(v)
            }
            (Self::GridPluginOptOut, Strings(_)) => true,
            (Self::StaticPosition | Self::StaticSize, OptionalPair(_)) => true,
            (
                Self::ClearQueryAfterRun
                | Self::HideAfterRun
                | Self::EnableToasts
                | Self::ShowInlineErrors
                | Self::ShowErrorToasts
                | Self::QueryAutocomplete
                | Self::MatchExact
                | Self::GridEnabled
                | Self::GridRespectCapability,
                Bool(_),
            ) => true,
            (Self::FollowMouse | Self::StaticLocationEnabled,
                Bool(_),
            ) => true,
            _ => false,
        }
    }
}

impl ScopedSettingValue {
    fn raw(&self) -> Result<Box<serde_json::value::RawValue>, ScopedSettingsError> {
        let text = match self {
            Self::Bool(value) => serde_json::to_string(value),
            Self::Float(value) => serde_json::to_string(value),
            Self::Count(value) => serde_json::to_string(value),
            Self::Strings(value) => serde_json::to_string(value),
            Self::OptionalPair(value) => serde_json::to_string(value),
        }
        .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        serde_json::value::RawValue::from_string(text)
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))
    }
}

impl Settings {
    pub fn save_scoped_preferences_checked(
        path: impl AsRef<Path>,
        changes: &[ScopedSettingChange],
    ) -> Result<ScopedSettingsAcknowledgement, ScopedSettingsError> {
        Self::save_scoped_preferences_inner(path.as_ref(), changes, None)
    }
    pub(crate) fn save_scoped_preferences_cancellable(
        path: &Path,
        changes: &[ScopedSettingChange],
        token: &SettingsBooleanSaveToken,
    ) -> Result<ScopedSettingsAcknowledgement, ScopedSettingsError> {
        if !token.mark_waiting_for_settings() {
            return Err(ScopedSettingsError::Cancelled);
        }
        Self::save_scoped_preferences_inner(path, changes, Some(token))
    }
    fn save_scoped_preferences_inner(
        path: &Path,
        changes: &[ScopedSettingChange],
        token: Option<&SettingsBooleanSaveToken>,
    ) -> Result<ScopedSettingsAcknowledgement, ScopedSettingsError> {
        let mut seen = HashSet::new();
        for change in changes {
            if !seen.insert(change.field)
                || !change.field.valid(&change.desired)
                || std::mem::discriminant(&change.baseline)
                    != std::mem::discriminant(&change.desired)
            {
                return Err(ScopedSettingsError::Invalid(change.field));
            }
        }
        let placement = [ScopedSetting::FollowMouse, ScopedSetting::StaticLocationEnabled, ScopedSetting::StaticPosition, ScopedSetting::StaticSize];
        let grouped = placement.iter().any(|field| seen.contains(field));
        if grouped && !placement.iter().all(|field| seen.contains(field)) {
            return Err(ScopedSettingsError::Invalid(ScopedSetting::FollowMouse));
        }
        let _transaction = transaction_guard();
        if token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
            return Err(ScopedSettingsError::Cancelled);
        }
        let bytes = read_exact_bytes(path)
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        let settings = parse_effective_settings(path, &bytes)
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        let mut pending = Vec::new();
        for change in changes.iter().filter(|change| change.desired != change.baseline || (grouped && placement.contains(&change.field))) {
            let actual = change.field.value(&settings);
            if actual == change.desired && change.desired != change.baseline {
                continue;
            }
            if actual != change.baseline {
                return Err(ScopedSettingsError::Conflict(change.field));
            }
            if change.desired != change.baseline { pending.push(change); }
        }
        if pending.is_empty() {
            if token.is_some_and(SettingsBooleanSaveToken::is_canceled) {
                return Err(ScopedSettingsError::Cancelled);
            }
            return Ok(ScopedSettingsAcknowledgement {
                values: changes
                    .iter()
                    .filter(|change| change.desired != change.baseline || (grouped && placement.contains(&change.field)))
                    .map(|change| (change.field, change.desired.clone()))
                    .collect(),
                persisted: false,
            });
        }
        let raw_source = if bytes.iter().all(u8::is_ascii_whitespace) {
            serde_json::to_vec(&settings)
                .map_err(|error| ScopedSettingsError::Store(error.to_string()))?
        } else {
            bytes.clone()
        };
        let mut root: RawSettingsObject = match serde_json::from_slice(&raw_source) {
            Ok(root) => root,
            Err(_) => raw_object(
                raw_value(&settings)
                    .map_err(|error| ScopedSettingsError::Store(error.to_string()))?
                    .as_ref(),
            )
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))?,
        };
        for change in pending {
            let (key, member) = change.field.keys();
            if let Some(member) = member {
                let mut nested: RawSettingsObject = match root.get(key) {
                    Some(value) => match serde_json::from_str(value.get()) {
                        Ok(object) => object,
                        Err(_) => raw_object(
                            raw_value(&settings.query_results_layout)
                                .map_err(|error| ScopedSettingsError::Store(error.to_string()))?
                                .as_ref(),
                        )
                        .map_err(|error| ScopedSettingsError::Store(error.to_string()))?,
                    },
                    None => RawSettingsObject::new(),
                };
                nested.insert(member.to_owned(), change.desired.raw()?);
                let raw = serde_json::to_string(&nested)
                    .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
                root.insert(
                    key.to_owned(),
                    serde_json::value::RawValue::from_string(raw)
                        .map_err(|error| ScopedSettingsError::Store(error.to_string()))?,
                );
            } else {
                root.insert(key.to_owned(), change.desired.raw()?);
            }
        }
        let candidate = serde_json::to_vec(&root)
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        let candidate_settings = parse_effective_settings(path, &candidate)
            .map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        if changes
            .iter()
            .filter(|change| change.desired != change.baseline || (grouped && placement.contains(&change.field)))
            .any(|change| change.field.value(&candidate_settings) != change.desired)
        {
            return Err(ScopedSettingsError::Store(
                "Settings patch did not produce the requested values".into(),
            ));
        }
        if grouped {
            let failure = std::cell::RefCell::new(None);
            let committed = std::cell::Cell::new(false);
            let result = crate::common::atomic_file::save_atomic_if(path, &candidate, || {
                let error = match read_exact_bytes(path) {
                    Ok(current) if current == bytes => if !committed.get() && token.is_some_and(|token| !token.begin_irrevocable_commit()) { Some(ScopedSettingsError::Cancelled) } else { committed.set(true); None },
                    Ok(current) => match parse_effective_settings(path, &current) {
                        Ok(settings) => changes.iter().find(|change| placement.contains(&change.field) && change.field.value(&settings) != change.baseline).map(|change| ScopedSettingsError::Conflict(change.field)).or_else(|| Some(ScopedSettingsError::Store("Settings changed while saving; retry the save".into()))),
                        Err(error) => Some(ScopedSettingsError::Store(error.to_string())),
                    },
                    Err(error) => Some(ScopedSettingsError::Store(error.to_string())),
                };
                if let Some(error) = error { *failure.borrow_mut() = Some(error); false } else { true }
            });
            if let Some(error) = failure.into_inner() { return Err(error); }
            result.map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        } else {
            if token.is_some_and(|token| !token.begin_irrevocable_commit()) { return Err(ScopedSettingsError::Cancelled); }
            save_json_atomic(path, &root).map_err(|error| ScopedSettingsError::Store(error.to_string()))?;
        }
        Ok(ScopedSettingsAcknowledgement {
            values: changes
                .iter()
                .filter(|change| change.desired != change.baseline || (grouped && placement.contains(&change.field)))
                .map(|change| (change.field, change.desired.clone()))
                .collect(),
            persisted: true,
        })
    }
}
#[cfg(test)]
mod tests {
    use super::{
        ScopedSetting as Field, ScopedSettingChange as Change, ScopedSettingValue as Value, *,
    };

    fn change(field: Field, baseline: Value, desired: Value) -> Change {
        Change {
            field,
            baseline,
            desired,
        }
    }
    #[test]
    fn mixed_conflict_keeps_exact_source_and_commits_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let raw = br#"{ "clear_query_after_run": false, "toast_duration": 2.0, "opaque": 123456789012345678901 }"#;
        std::fs::write(&path, raw).unwrap();
        let result = Settings::save_scoped_preferences_checked(
            &path,
            &[
                change(
                    Field::ClearQueryAfterRun,
                    Value::Bool(false),
                    Value::Bool(true),
                ),
                change(Field::ToastDuration, Value::Float(1.0), Value::Float(3.0)),
            ],
        );
        assert_eq!(
            result,
            Err(ScopedSettingsError::Conflict(Field::ToastDuration))
        );
        assert_eq!(std::fs::read(&path).unwrap(), raw);
    }
    #[test]
    fn nested_and_top_level_patches_keep_opaque_siblings_and_ack_without_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"query_results_layout":{"enabled":false,"custom":123456789012345678901},"opaque":{"n":123456789012345678901}}"#).unwrap();
        let changes = [
            change(Field::GridEnabled, Value::Bool(false), Value::Bool(true)),
            change(Field::EnableToasts, Value::Bool(true), Value::Bool(false)),
        ];
        assert!(
            Settings::save_scoped_preferences_checked(&path, &changes)
                .unwrap()
                .persisted
        );
        let saved = std::fs::read_to_string(&path).unwrap();
        assert!(saved.contains("123456789012345678901"));
        assert!(
            Settings::read_current(&path)
                .unwrap()
                .query_results_layout
                .enabled
        );
        assert!(!Settings::read_current(&path).unwrap().enable_toasts);
        assert!(
            !Settings::save_scoped_preferences_checked(&path, &changes)
                .unwrap()
                .persisted
        );
        assert_eq!(std::fs::read_to_string(&path).unwrap(), saved);
    }
    #[test]
    fn absent_grid_object_writes_only_the_owned_leaf() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, br#"{"opaque":{"n":123456789012345678901}}"#).unwrap();
        let change = change(Field::GridEnabled, Value::Bool(false), Value::Bool(true));
        assert!(
            Settings::save_scoped_preferences_checked(&path, &[change])
                .unwrap()
                .persisted
        );
        let raw: super::RawSettingsObject =
            serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
        let nested: super::RawSettingsObject =
            serde_json::from_str(raw["query_results_layout"].get()).unwrap();
        assert_eq!(nested.len(), 1);
        assert_eq!(nested["enabled"].get(), "true");
        assert!(raw["opaque"].get().contains("123456789012345678901"));
    }

    #[test]
    fn checked_patch_accepts_valid_legacy_positional_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let defaults = Settings::default();
        std::fs::write(&path, super::super::legacy_positional_settings(&defaults)).unwrap();
        let change = change(
            Field::HideAfterRun,
            Value::Bool(defaults.hide_after_run),
            Value::Bool(!defaults.hide_after_run),
        );
        assert!(
            Settings::save_scoped_preferences_checked(&path, &[change])
                .unwrap()
                .persisted
        );
        assert_eq!(
            Settings::read_current(path).unwrap().hide_after_run,
            !defaults.hide_after_run
        );
    }

    #[test]
    fn missing_defaults_and_cancel_preserve_source() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let defaults = Settings::read_current(&path).unwrap();
        let changes = [change(
            Field::HideAfterRun,
            Value::Bool(defaults.hide_after_run),
            Value::Bool(!defaults.hide_after_run),
        )];
        let token = SettingsBooleanSaveToken::new(99);
        assert!(token.request_cancel());
        assert_eq!(
            Settings::save_scoped_preferences_cancellable(&path, &changes, &token),
            Err(ScopedSettingsError::Cancelled)
        );
        assert!(!path.exists());
        assert!(
            Settings::save_scoped_preferences_checked(&path, &changes)
                .unwrap()
                .persisted
        );
        assert_eq!(
            Settings::read_current(path).unwrap().hide_after_run,
            !defaults.hide_after_run
        );
    }

    #[test]
    fn placement_group_checks_untouched_dependencies_and_keeps_opaque_data() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let raw = br#"{"follow_mouse":false,"static_location_enabled":true,"static_pos":[-40,20],"static_size":[600,400],"opaque":123456789012345678901}"#;
        std::fs::write(&path, raw).unwrap();
        let changes = [
            change(Field::FollowMouse, Value::Bool(false), Value::Bool(true)),
            change(Field::StaticLocationEnabled, Value::Bool(true), Value::Bool(false)),
            change(Field::StaticPosition, Value::OptionalPair(Some((-40, 20))), Value::OptionalPair(None)),
            change(Field::StaticSize, Value::OptionalPair(Some((600, 400))), Value::OptionalPair(None)),
        ];
        let token = SettingsBooleanSaveToken::new(123);
        assert!(token.request_cancel());
        assert_eq!(Settings::save_scoped_preferences_cancellable(&path, &changes, &token), Err(ScopedSettingsError::Cancelled));
        assert_eq!(std::fs::read(&path).unwrap(), raw);
        let ack = Settings::save_scoped_preferences_checked(&path, &changes).unwrap();
        assert_eq!(ack.values.len(), 4);
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.contains("123456789012345678901"));
        let settings = Settings::read_current(&path).unwrap();
        assert!(settings.follow_mouse);
        assert!(!settings.static_location_enabled);
        assert_eq!(settings.static_pos, None);
        assert_eq!(settings.static_size, None);
        std::fs::write(&path, raw).unwrap();
        let mut changed = changes.clone();
        changed[3].desired = changed[3].baseline.clone();
        std::fs::write(&path, br#"{"follow_mouse":false,"static_location_enabled":true,"static_pos":[-40,20],"static_size":[601,400]}"#).unwrap();
        assert_eq!(Settings::save_scoped_preferences_checked(&path, &changed), Err(ScopedSettingsError::Conflict(Field::StaticSize)));
    }
}
