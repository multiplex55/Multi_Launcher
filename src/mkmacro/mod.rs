//! Versioned model, validation, compilation, and persistence for the new macro system.
pub mod asset_authoring;
pub mod compiler;
pub mod coordinates;
pub mod executor;
pub mod hotkeys;
pub mod image_search;
pub mod input;
pub mod interpolation;
pub mod model;
pub mod notifications;
pub mod prompt;
pub mod recorder;
pub mod recorder_hooks;
pub mod recorder_hotkeys;
pub mod recorder_runtime;
pub mod recorder_windows;
pub mod runtime;
pub mod screen;
pub mod store;
pub mod structure;
pub mod uia;
pub mod validation;
pub mod variables;
pub mod virtual_desktops;
pub mod windows;

pub use asset_authoring::*;
pub use compiler::*;
pub use coordinates::*;
pub use executor::*;
pub use image_search::*;
pub use interpolation::*;
pub use model::*;
pub use notifications::*;
pub use prompt::*;
pub use recorder::*;
pub use recorder_hooks::*;
pub use recorder_runtime::*;
pub use recorder_windows::*;
pub use runtime::{
    CommandResult, DiagnosticKey, MacroRuntime, RuntimeCommand, RuntimeSnapshot, RuntimeState,
    StepState,
};
pub use screen::*;
pub use store::*;
pub use structure::*;
pub use uia::*;
pub use validation::*;
pub use variables::*;
pub use virtual_desktops::*;
pub use windows::*;
