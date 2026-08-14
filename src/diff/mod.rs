//! Native Diff product boundary.
//!
//! The current scope is two-way text comparison, two-way binary/hex comparison,
//! recursive two-way folder comparison, and controlled, previewed folder
//! reconciliation.  The hex view is a **comparator**, not an editor.
//!
//! Three-way text comparison/merge is planned separately (see
//! `docs/diff-three-way-initiative.md`). Explicit exclusions are automatic
//! synchronization or mirroring, three-way folders, archive virtual folders and
//! archive editing, image/media comparison, cloud/FTP/SFTP, Git-specific UI,
//! and binary editing. `.zip` and every other archive are ordinary binary files:
//! Diff compares their bytes and never mounts or traverses their contents.

/// Stable, testable product-boundary facts used by documentation and controller
/// regression tests. These are capabilities, not commands: none can initiate an
/// operation without a captured plan and explicit confirmation.
pub mod scope {
    pub const CURRENT: &[&str] = &[
        "two-way text comparison",
        "two-way binary/hex comparison",
        "recursive two-way folder comparison",
        "controlled, previewed folder reconciliation",
    ];
    pub const PLANNED: &[&str] = &["three-way text comparison/merge"];
    pub const EXCLUDED: &[&str] = &[
        "automatic synchronization or mirroring",
        "three-way folders",
        "archive virtual folders and archive editing",
        "image/media comparison",
        "cloud, FTP, and SFTP",
        "Git-specific UI",
        "binary editing",
    ];

    pub const HEX_IS_READ_ONLY: bool = true;
    pub const ARCHIVES_ARE_ORDINARY_BINARY_FILES: bool = true;
    pub const GUI_DELETE_USES_RECYCLE_BIN: bool = true;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum PlannedThreeWayInput {
        Text,
        Folder,
        Binary,
        Archive,
    }

    /// Guard for the future three-way entry point. Keeping this independent of
    /// UI routing prevents accidentally widening today's two-way `DiffView`.
    pub fn validate_planned_three_way_inputs(
        inputs: [PlannedThreeWayInput; 3],
    ) -> Result<(), &'static str> {
        if inputs
            .iter()
            .all(|input| *input == PlannedThreeWayInput::Text)
        {
            Ok(())
        } else {
            Err("three-way comparison accepts text files only")
        }
    }
}

pub mod binary_compare;
pub mod file_ops;
pub mod folder_compare;
pub mod folder_export;
pub mod folder_runtime;
pub mod folder_scan;
pub mod model;
pub mod persistence;
pub mod query;
pub mod settings;
pub mod syntax;
pub mod text_compare;
pub mod text_export;
pub mod text_file;
pub mod watch;
pub mod worker;

#[cfg(test)]
mod scope_tests {
    use super::scope::*;

    #[test]
    fn product_boundary_has_no_implicit_or_unsupported_capabilities() {
        assert!(!CURRENT.iter().any(|item| item.contains("synchron")));
        assert!(EXCLUDED.contains(&"automatic synchronization or mirroring"));
        const {
            assert!(HEX_IS_READ_ONLY);
            assert!(ARCHIVES_ARE_ORDINARY_BINARY_FILES);
            assert!(GUI_DELETE_USES_RECYCLE_BIN);
        }
    }

    #[test]
    fn planned_three_way_guard_rejects_non_text_inputs() {
        assert!(validate_planned_three_way_inputs([PlannedThreeWayInput::Text; 3]).is_ok());
        for rejected in [
            PlannedThreeWayInput::Folder,
            PlannedThreeWayInput::Binary,
            PlannedThreeWayInput::Archive,
        ] {
            assert_eq!(
                validate_planned_three_way_inputs([
                    PlannedThreeWayInput::Text,
                    rejected,
                    PlannedThreeWayInput::Text,
                ]),
                Err("three-way comparison accepts text files only")
            );
        }
    }
}
