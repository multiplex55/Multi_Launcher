//! Native two-way file and folder comparison.
//!
//! Deliberately out of scope: three-way merge, synchronization, archive virtual
//! folders, Git-specific UI, media/image comparison, cloud protocols, fuzzy
//! filename pairing, and a hex editor.

pub mod model;
pub mod persistence;
pub mod query;
pub mod settings;
pub mod worker;
