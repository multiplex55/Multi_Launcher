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
    /// Files at or above either byte/estimated-row threshold use the reduced
    /// large-file presentation. These are deliberately user configurable.
    pub large_file_bytes: u64,
    pub extreme_file_bytes: u64,
    pub large_file_estimated_rows: u64,
    pub extreme_file_estimated_rows: u64,
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
            large_file_bytes: 8 * 1024 * 1024,
            extreme_file_bytes: 128 * 1024 * 1024,
            large_file_estimated_rows: 200_000,
            extreme_file_estimated_rows: 2_000_000,
        }
    }
}

impl DiffConfigV1 {
    pub fn large_file_policy(&self) -> crate::diff::worker::LargeFilePolicy {
        crate::diff::worker::LargeFilePolicy {
            large_bytes: self.large_file_bytes,
            extreme_bytes: self.extreme_file_bytes.max(self.large_file_bytes),
            large_estimated_rows: self.large_file_estimated_rows,
            extreme_estimated_rows: self
                .extreme_file_estimated_rows
                .max(self.large_file_estimated_rows),
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
