use crate::mouse_gestures::engine::DirMode;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureFocusArgs {
    pub label: String,
    pub tokens: String,
    pub dir_mode: DirMode,
    #[serde(default)]
    pub binding_idx: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GestureToggleArgs {
    pub label: String,
    pub tokens: String,
    pub dir_mode: DirMode,
    pub enabled: bool,
}
