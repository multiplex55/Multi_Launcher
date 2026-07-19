use serde::{Deserialize, Deserializer, Serialize};
use std::sync::atomic::{AtomicU64, Ordering};

pub fn default_true() -> bool {
    true
}

static NEXT_WORKSPACE_ID: AtomicU64 = AtomicU64::new(1);

pub fn new_workspace_id() -> String {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or_default();
    let sequence = NEXT_WORKSPACE_ID.fetch_add(1, Ordering::Relaxed);
    format!("mmws-{now:x}-{sequence:x}")
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct MmRect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl<'de> Deserialize<'de> for MmRect {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum RectCompat {
            Named { x: i32, y: i32, w: i32, h: i32 },
            Tuple([i32; 4]),
        }

        match RectCompat::deserialize(deserializer)? {
            RectCompat::Named { x, y, w, h } => Ok(MmRect { x, y, w, h }),
            RectCompat::Tuple([x, y, w, h]) => Ok(MmRect { x, y, w, h }),
        }
    }
}

fn deserialize_optional_rect<'de, D>(deserializer: D) -> Result<Option<MmRect>, D::Error>
where
    D: Deserializer<'de>,
{
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?
        .and_then(|value| serde_json::from_value(value).ok()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MmHotkey {
    #[serde(default)]
    pub key: String,
    #[serde(default)]
    pub ctrl: bool,
    #[serde(default)]
    pub shift: bool,
    #[serde(default)]
    pub alt: bool,
    #[serde(default)]
    pub win: bool,
}

impl MmHotkey {
    pub fn sequence(&self) -> Option<String> {
        let key = self.key.trim();
        if key.is_empty() {
            return None;
        }

        let mut parts = Vec::new();
        if self.ctrl {
            parts.push("Ctrl");
        }
        if self.shift {
            parts.push("Shift");
        }
        if self.alt {
            parts.push("Alt");
        }
        if self.win {
            parts.push("Win");
        }
        parts.push(key);

        Some(parts.join("+"))
    }

    pub fn validate(&self) -> MmHotkeyValidation {
        let key = self.key.trim();
        if key.is_empty() {
            return MmHotkeyValidation::MissingKey;
        }
        if key.contains('+') {
            return MmHotkeyValidation::KeyContainsPlus;
        }
        if crate::window_manager::virtual_key_from_string(key).is_none() {
            return MmHotkeyValidation::UnsupportedKey(key.to_string());
        }

        MmHotkeyValidation::Valid
    }

    pub fn is_valid(&self) -> bool {
        self.validate() == MmHotkeyValidation::Valid
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MmHotkeyValidation {
    Valid,
    MissingKey,
    KeyContainsPlus,
    UnsupportedKey(String),
}

impl MmHotkeyValidation {
    pub fn label(&self) -> &'static str {
        match self {
            MmHotkeyValidation::Valid => "valid",
            MmHotkeyValidation::MissingKey => "missing key",
            MmHotkeyValidation::KeyContainsPlus => "put modifiers in checkboxes",
            MmHotkeyValidation::UnsupportedKey(_) => "unsupported key",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum MmBindingStatus {
    Bound,
    #[default]
    Missing,
    Closed,
    Ambiguous,
    Reconnected,
    MetadataMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MmWindow {
    #[serde(default)]
    pub alias: String,
    #[serde(rename = "title", default)]
    pub captured_title: String,
    #[serde(default)]
    pub executable: String,
    #[serde(default)]
    pub class_name: String,
    #[serde(default)]
    pub process_path: String,
    #[serde(default, deserialize_with = "deserialize_optional_rect")]
    pub home_rect: Option<MmRect>,
    #[serde(default, deserialize_with = "deserialize_optional_rect")]
    pub target_rect: Option<MmRect>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default = "default_true")]
    pub valid: bool,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub hwnd: usize,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub live_title: String,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub binding_status: MmBindingStatus,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub binding_verified: bool,
}

impl Default for MmWindow {
    fn default() -> Self {
        Self {
            alias: String::new(),
            captured_title: String::new(),
            executable: String::new(),
            class_name: String::new(),
            process_path: String::new(),
            home_rect: None,
            target_rect: None,
            disabled: false,
            valid: true,
            hwnd: 0,
            live_title: String::new(),
            binding_status: MmBindingStatus::Missing,
            binding_verified: false,
        }
    }
}

impl MmWindow {
    pub fn fallback_title(&self) -> &str {
        self.captured_title.trim()
    }

    pub fn current_display_title(&self) -> &str {
        let live_title = self.live_title.trim();
        if live_title.is_empty() {
            self.fallback_title()
        } else {
            live_title
        }
    }

    pub fn has_bound_hwnd(&self) -> bool {
        self.hwnd != 0 && self.valid
    }

    pub fn can_activate(&self) -> bool {
        !self.disabled && self.has_bound_hwnd()
    }

    pub fn mark_bound(&mut self, hwnd: usize) {
        self.hwnd = hwnd;
        self.valid = hwnd != 0;
        self.binding_status = if hwnd == 0 {
            MmBindingStatus::Missing
        } else {
            MmBindingStatus::Bound
        };
        self.binding_verified = hwnd != 0;
    }

    pub fn mark_reconnected(&mut self, hwnd: usize) {
        self.hwnd = hwnd;
        self.valid = hwnd != 0;
        self.binding_status = if hwnd == 0 {
            MmBindingStatus::Missing
        } else {
            MmBindingStatus::Reconnected
        };
        self.binding_verified = hwnd != 0;
    }

    pub fn mark_closed(&mut self) {
        self.hwnd = 0;
        self.valid = false;
        self.binding_status = MmBindingStatus::Closed;
        self.binding_verified = false;
    }

    pub fn mark_missing(&mut self) {
        self.hwnd = 0;
        self.valid = false;
        self.binding_status = MmBindingStatus::Missing;
        self.binding_verified = false;
    }

    pub fn mark_ambiguous(&mut self) {
        self.hwnd = 0;
        self.valid = false;
        self.binding_status = MmBindingStatus::Ambiguous;
        self.binding_verified = false;
    }

    pub fn mark_metadata_mismatch(&mut self) {
        self.hwnd = 0;
        self.valid = false;
        self.binding_status = MmBindingStatus::MetadataMismatch;
        self.binding_verified = false;
    }

    pub fn display_label(&self) -> &str {
        let alias = self.alias.trim();
        if alias.is_empty() {
            self.current_display_title()
        } else {
            alias
        }
    }

    pub fn sync_alias_from_title_if_missing(&mut self) {
        if self.alias.trim().is_empty() {
            self.alias = self.captured_title.trim().to_string();
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MmWorkspace {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub hotkey: Option<MmHotkey>,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub windows: Vec<MmWindow>,
    #[serde(default, deserialize_with = "deserialize_optional_rect")]
    pub home_rect: Option<MmRect>,
    #[serde(default, deserialize_with = "deserialize_optional_rect")]
    pub target_rect: Option<MmRect>,
    #[serde(default)]
    pub disabled: bool,
    #[serde(default = "default_true")]
    pub valid: bool,
    #[serde(default)]
    pub rotate: bool,
    #[serde(default, skip_serializing, skip_deserializing)]
    pub rotation_offset: usize,
}

impl Default for MmWorkspace {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            hotkey: None,
            aliases: Vec::new(),
            windows: Vec::new(),
            home_rect: None,
            target_rect: None,
            disabled: false,
            valid: true,
            rotate: false,
            rotation_offset: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "type")]
pub enum PendingCaptureAction {
    CaptureOneWindow {
        workspace_id: String,
    },
    CaptureMultipleWindows {
        workspace_id: String,
    },
    RecaptureWindow {
        workspace_id: String,
        window_index: usize,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RecaptureQueueItem {
    #[serde(default)]
    pub workspace_id: String,
    #[serde(default)]
    pub window_index: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_display_label_uses_alias_when_non_empty() {
        let window = MmWindow {
            alias: "  Editor  ".into(),
            captured_title: "Code".into(),
            ..MmWindow::default()
        };
        assert_eq!(window.display_label(), "Editor");
    }

    #[test]
    fn window_display_label_falls_back_to_title_when_alias_absent_or_blank() {
        let titled = MmWindow {
            captured_title: "  Browser  ".into(),
            ..MmWindow::default()
        };
        let blank_alias = MmWindow {
            alias: " \t ".into(),
            captured_title: "Terminal".into(),
            ..MmWindow::default()
        };
        assert_eq!(titled.display_label(), "Browser");
        assert_eq!(blank_alias.display_label(), "Terminal");
    }

    #[test]
    fn sync_alias_from_title_if_missing_does_not_overwrite_non_empty_aliases() {
        let mut window = MmWindow {
            alias: "Docs".into(),
            captured_title: "Untitled - Notepad".into(),
            ..MmWindow::default()
        };
        window.sync_alias_from_title_if_missing();
        assert_eq!(window.alias, "Docs");
    }

    #[test]
    fn sync_alias_from_title_if_missing_copies_title_for_blank_alias() {
        let mut window = MmWindow {
            alias: "  ".into(),
            captured_title: "  Notes  ".into(),
            ..MmWindow::default()
        };
        window.sync_alias_from_title_if_missing();
        assert_eq!(window.alias, "Notes");
    }

    #[test]
    fn current_display_title_prefers_live_title() {
        let window = MmWindow {
            captured_title: "Captured".into(),
            live_title: " Live ".into(),
            ..MmWindow::default()
        };

        assert_eq!(window.current_display_title(), "Live");
    }

    #[test]
    fn current_display_title_falls_back_to_captured_title() {
        let window = MmWindow {
            captured_title: " Captured ".into(),
            live_title: " \t ".into(),
            ..MmWindow::default()
        };

        assert_eq!(window.current_display_title(), "Captured");
    }

    #[test]
    fn runtime_window_fields_are_not_serialized() {
        let mut window = MmWindow {
            captured_title: "Captured".into(),
            live_title: "Live".into(),
            ..MmWindow::default()
        };
        window.mark_bound(42);

        let json = serde_json::to_value(&window).unwrap();

        assert_eq!(json["title"], "Captured");
        assert!(json.get("hwnd").is_none());
        assert!(json.get("live_title").is_none());
        assert!(json.get("binding_status").is_none());
        assert!(json.get("binding_verified").is_none());
    }

    #[test]
    fn old_json_title_loads_into_captured_title_with_runtime_defaults() {
        let window: MmWindow =
            serde_json::from_str(r#"{ "title": "GitHub - Mozilla Firefox" }"#).unwrap();

        assert_eq!(window.captured_title, "GitHub - Mozilla Firefox");
        assert_eq!(window.hwnd, 0);
        assert_eq!(window.live_title, "");
        assert_eq!(window.binding_status, MmBindingStatus::Missing);
        assert!(!window.binding_verified);
    }

    #[test]
    fn status_helpers_keep_runtime_binding_state_consistent() {
        let mut window = MmWindow::default();

        window.mark_bound(7);
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (true, 7, MmBindingStatus::Bound, true)
        );
        assert!(window.has_bound_hwnd());
        assert!(window.can_activate());

        window.mark_reconnected(8);
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (true, 8, MmBindingStatus::Reconnected, true)
        );
        assert!(window.has_bound_hwnd());
        assert!(window.can_activate());

        window.mark_closed();
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (false, 0, MmBindingStatus::Closed, false)
        );

        window.mark_missing();
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (false, 0, MmBindingStatus::Missing, false)
        );

        window.mark_ambiguous();
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (false, 0, MmBindingStatus::Ambiguous, false)
        );

        window.mark_metadata_mismatch();
        assert_eq!(
            (
                window.valid,
                window.hwnd,
                window.binding_status,
                window.binding_verified
            ),
            (false, 0, MmBindingStatus::MetadataMismatch, false)
        );
    }

    #[test]
    fn activation_helpers_keep_legacy_valid_hwnd_compatibility() {
        let window = MmWindow {
            hwnd: 9,
            valid: true,
            binding_verified: false,
            ..MmWindow::default()
        };

        assert!(window.has_bound_hwnd());
        assert!(window.can_activate());
    }

    #[test]
    fn new_workspace_ids_are_non_empty_and_unique() {
        let first = new_workspace_id();
        let second = new_workspace_id();

        assert!(!first.is_empty());
        assert!(!second.is_empty());
        assert_ne!(first, second);
    }

    #[test]
    fn hotkey_validation_accepts_supported_combinations() {
        for hotkey in [
            MmHotkey {
                key: "F9".into(),
                ctrl: true,
                ..MmHotkey::default()
            },
            MmHotkey {
                key: "Space".into(),
                shift: true,
                alt: true,
                ..MmHotkey::default()
            },
            MmHotkey {
                key: "A".into(),
                win: true,
                ..MmHotkey::default()
            },
        ] {
            assert_eq!(hotkey.validate(), MmHotkeyValidation::Valid, "{hotkey:?}");
            assert!(hotkey.is_valid(), "{hotkey:?}");
        }
    }

    #[test]
    fn hotkey_validation_accepts_valid_key_with_all_modifiers() {
        let hotkey = MmHotkey {
            key: "F12".into(),
            ctrl: true,
            shift: true,
            alt: true,
            win: true,
        };

        assert_eq!(hotkey.validate(), MmHotkeyValidation::Valid);
        assert!(hotkey.is_valid());
    }

    #[test]
    fn hotkey_validation_reports_empty_key_as_missing() {
        let hotkey = MmHotkey::default();

        assert_eq!(hotkey.validate(), MmHotkeyValidation::MissingKey);
        assert_eq!(hotkey.validate().label(), "missing key");
        assert!(!hotkey.is_valid());
    }

    #[test]
    fn hotkey_validation_rejects_modifiers_entered_in_key_field() {
        let hotkey = MmHotkey {
            key: "Ctrl+A".into(),
            ..MmHotkey::default()
        };

        assert_eq!(hotkey.validate(), MmHotkeyValidation::KeyContainsPlus);
        assert_eq!(hotkey.validate().label(), "put modifiers in checkboxes");
        assert!(!hotkey.is_valid());
    }

    #[test]
    fn hotkey_validation_reports_unknown_key_as_unsupported() {
        let hotkey = MmHotkey {
            key: "NoSuchKey".into(),
            ctrl: true,
            ..MmHotkey::default()
        };

        assert_eq!(
            hotkey.validate(),
            MmHotkeyValidation::UnsupportedKey("NoSuchKey".into())
        );
        assert_eq!(hotkey.validate().label(), "unsupported key");
        assert!(!hotkey.is_valid());
    }

    #[test]
    fn hotkey_sequence_preserves_modifier_order() {
        let hotkey = MmHotkey {
            key: " Key ".into(),
            ctrl: true,
            shift: true,
            alt: true,
            win: true,
        };

        assert_eq!(hotkey.sequence().as_deref(), Some("Ctrl+Shift+Alt+Win+Key"));
    }

    #[test]
    fn valid_hotkey_sequence_formats_key_and_modifiers() {
        let hotkey = MmHotkey {
            key: "F9".into(),
            ctrl: true,
            alt: true,
            ..MmHotkey::default()
        };

        assert_eq!(hotkey.sequence().as_deref(), Some("Ctrl+Alt+F9"));
    }

    #[test]
    fn hotkey_sequence_returns_none_for_blank_key() {
        let hotkey = MmHotkey {
            key: "   ".into(),
            ctrl: true,
            shift: true,
            alt: true,
            win: true,
        };

        assert_eq!(hotkey.sequence(), None);
    }
}
