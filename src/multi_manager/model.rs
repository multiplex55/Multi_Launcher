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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MmWindow {
    #[serde(default)]
    pub alias: String,
    #[serde(default)]
    pub title: String,
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
}

impl MmWindow {
    pub fn display_label(&self) -> &str {
        let alias = self.alias.trim();
        if alias.is_empty() {
            self.title.trim()
        } else {
            alias
        }
    }

    pub fn sync_alias_from_title_if_missing(&mut self) {
        if self.alias.trim().is_empty() {
            self.alias = self.title.trim().to_string();
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
            title: "Code".into(),
            ..MmWindow::default()
        };
        assert_eq!(window.display_label(), "Editor");
    }

    #[test]
    fn window_display_label_falls_back_to_title_when_alias_absent_or_blank() {
        let titled = MmWindow {
            title: "  Browser  ".into(),
            ..MmWindow::default()
        };
        let blank_alias = MmWindow {
            alias: " \t ".into(),
            title: "Terminal".into(),
            ..MmWindow::default()
        };
        assert_eq!(titled.display_label(), "Browser");
        assert_eq!(blank_alias.display_label(), "Terminal");
    }

    #[test]
    fn sync_alias_from_title_if_missing_does_not_overwrite_non_empty_aliases() {
        let mut window = MmWindow {
            alias: "Docs".into(),
            title: "Untitled - Notepad".into(),
            ..MmWindow::default()
        };
        window.sync_alias_from_title_if_missing();
        assert_eq!(window.alias, "Docs");
    }

    #[test]
    fn sync_alias_from_title_if_missing_copies_title_for_blank_alias() {
        let mut window = MmWindow {
            alias: "  ".into(),
            title: "  Notes  ".into(),
            ..MmWindow::default()
        };
        window.sync_alias_from_title_if_missing();
        assert_eq!(window.alias, "Notes");
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
