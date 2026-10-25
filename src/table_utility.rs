//! Public application module for the pure table engine.
//!
//! The implementation and its unit tests live in the independently testable
//! `multi_launcher_table_core` package. Re-exporting the core keeps existing
//! application imports and public type identities stable.

pub use multi_launcher_table_core::*;
