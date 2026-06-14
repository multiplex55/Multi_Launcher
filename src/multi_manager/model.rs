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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MmWindow {
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
    CaptureWorkspace {
        workspace_id: String,
    },
    CaptureWindow {
        workspace_id: String,
        window_id: String,
    },
    RecaptureWorkspace {
        workspace_id: String,
    },
}
