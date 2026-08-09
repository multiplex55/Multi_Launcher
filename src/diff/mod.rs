//! Native two-way file and folder comparison.
//!
//! Deliberately out of the two-way completion scope: three-way merge (tracked as
//! a deferred text-only initiative in `docs/diff-three-way-initiative.md`),
//! synchronization, archive virtual folders, Git-specific UI, media/image
//! comparison, cloud protocols, fuzzy filename pairing, and a hex editor.

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
