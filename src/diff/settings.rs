use serde::{Deserialize, Serialize};

pub const FOLDER_COLUMN_MIN: f32 = 56.0;
pub const FOLDER_COLUMN_MAX: f32 = 1200.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum FolderSortColumn {
    #[default]
    #[serde(alias = "relative_path", alias = "left_path", alias = "right_path")]
    Path,
    Status,
    #[serde(alias = "size")]
    LeftSize,
    RightSize,
    #[serde(alias = "left_modified_time")]
    #[serde(alias = "modified", alias = "mtime")]
    LeftModified,
    #[serde(alias = "right_modified_time")]
    RightModified,
}

#[cfg(test)]
mod folder_table_settings_tests {
    use super::*;
    #[test]
    fn migrates_legacy_sort_names_and_round_trips_typed_state() {
        for (name, expected) in [
            ("path", FolderSortColumn::Path),
            ("relative_path", FolderSortColumn::Path),
            ("size", FolderSortColumn::LeftSize),
            ("modified", FolderSortColumn::LeftModified),
        ] {
            let value: FolderSortStateV1 =
                serde_json::from_str(&format!(r#"{{"column":"{name}","descending":true}}"#))
                    .unwrap();
            assert_eq!(value.column, expected);
            assert_eq!(
                serde_json::from_str::<FolderSortStateV1>(&serde_json::to_string(&value).unwrap())
                    .unwrap(),
                value
            );
        }
    }
    #[test]
    fn invalid_widths_clamp_and_defaults_are_deterministic() {
        let invalid = FolderColumnWidthsV1 {
            left_path: f32::NAN,
            right_path: f32::INFINITY,
            left_size: -4.0,
            ..Default::default()
        }
        .validated();
        assert!(
            invalid
                .as_array()
                .iter()
                .all(|x| x.is_finite() && *x >= FOLDER_COLUMN_MIN && *x <= FOLDER_COLUMN_MAX)
        );
        assert_eq!(
            FolderColumnWidthsV1::for_viewport(800.0),
            FolderColumnWidthsV1::for_viewport(800.0)
        );
        let mut dragged = invalid;
        dragged.set(0, 99_999.0);
        assert_eq!(dragged.left_path, FOLDER_COLUMN_MAX);
    }
    #[test]
    fn config_persistence_round_trip_keeps_table_preferences() {
        let mut config = DiffConfigV1::default();
        config.folder_sort = FolderSortStateV1 {
            column: FolderSortColumn::RightModified,
            descending: true,
        };
        config.folder_column_widths.left_path = 333.0;
        let decoded: DiffConfigV1 =
            serde_json::from_str(&serde_json::to_string(&config).unwrap()).unwrap();
        assert_eq!(decoded, config);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderSortStateV1 {
    pub column: FolderSortColumn,
    pub descending: bool,
}
impl Default for FolderSortStateV1 {
    fn default() -> Self {
        Self {
            column: FolderSortColumn::Path,
            descending: false,
        }
    }
}

/// Durable widths for the eight table columns. Values are validated after load.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FolderColumnWidthsV1 {
    pub left_path: f32,
    pub left_size: f32,
    pub left_modified: f32,
    pub status: f32,
    pub right_path: f32,
    pub right_size: f32,
    pub right_modified: f32,
    pub action: f32,
}
impl Default for FolderColumnWidthsV1 {
    fn default() -> Self {
        Self::for_viewport(900.0)
    }
}
impl FolderColumnWidthsV1 {
    pub fn for_viewport(viewport: f32) -> Self {
        let fixed = 110.0 * 2.0 + 170.0 * 2.0 + 130.0 + 72.0 + 7.0 * 8.0;
        let path = ((if viewport.is_finite() {
            viewport.max(0.0)
        } else {
            900.0
        } - fixed)
            / 2.0)
            .max(190.0);
        Self {
            left_path: path,
            left_size: 110.0,
            left_modified: 170.0,
            status: 130.0,
            right_path: path,
            right_size: 110.0,
            right_modified: 170.0,
            action: 72.0,
        }
    }
    pub fn validated(self) -> Self {
        fn v(x: f32) -> f32 {
            if x.is_finite() {
                x.clamp(FOLDER_COLUMN_MIN, FOLDER_COLUMN_MAX)
            } else {
                FOLDER_COLUMN_MIN
            }
        }
        Self {
            left_path: v(self.left_path),
            left_size: v(self.left_size),
            left_modified: v(self.left_modified),
            status: v(self.status),
            right_path: v(self.right_path),
            right_size: v(self.right_size),
            right_modified: v(self.right_modified),
            action: v(self.action),
        }
    }
    pub fn as_array(self) -> [f32; 8] {
        [
            self.left_path,
            self.left_size,
            self.left_modified,
            self.status,
            self.right_path,
            self.right_size,
            self.right_modified,
            self.action,
        ]
    }
    pub fn set(&mut self, index: usize, value: f32) {
        let value = if value.is_finite() {
            value.clamp(FOLDER_COLUMN_MIN, FOLDER_COLUMN_MAX)
        } else {
            FOLDER_COLUMN_MIN
        };
        match index {
            0 => self.left_path = value,
            1 => self.left_size = value,
            2 => self.left_modified = value,
            3 => self.status = value,
            4 => self.right_path = value,
            5 => self.right_size = value,
            6 => self.right_modified = value,
            7 => self.action = value,
            _ => {}
        }
    }
}

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
    pub folder_sort: FolderSortStateV1,
    pub folder_column_widths: FolderColumnWidthsV1,
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
            folder_sort: FolderSortStateV1::default(),
            folder_column_widths: FolderColumnWidthsV1::default(),
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
