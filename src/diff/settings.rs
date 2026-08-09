use serde::{Deserialize, Serialize};

pub const DIFF_CONFIG_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DiffConfigV1 {
    pub version: u32,
    pub pane_split: f32,
    pub wrap_text: bool,
    pub syntax_highlighting: bool,
    pub syntax_theme: String,
    pub ignore_whitespace: bool,
    pub case_sensitive: bool,
    pub max_recent_comparisons: usize,
}

impl Default for DiffConfigV1 {
    fn default() -> Self {
        Self {
            version: DIFF_CONFIG_VERSION,
            pane_split: 0.5,
            wrap_text: false,
            syntax_highlighting: true,
            syntax_theme: "InspiredGitHub".into(),
            ignore_whitespace: false,
            case_sensitive: true,
            max_recent_comparisons: 20,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReplacementRuleV1 {
    pub id: String,
    pub pattern: String,
    pub replacement: String,
    pub enabled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnimportantSectionRuleV1 {
    pub id: String,
    pub pattern: String,
    pub enabled: bool,
}
