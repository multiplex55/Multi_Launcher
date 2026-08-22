//! Versioned model, validation, compilation, and persistence for the new macro system.
pub mod compiler;
pub mod coordinates;
pub mod executor;
pub mod hotkeys;
pub mod image_search;
pub mod input;
pub mod model;
pub mod recorder;
pub mod recorder_hooks;
pub mod recorder_hotkeys;
pub mod recorder_runtime;
pub mod recorder_windows;
pub mod runtime;
pub mod screen;
pub mod store;
pub mod uia;
pub mod validation;
pub mod variables;
pub mod virtual_desktops;
pub mod windows;

pub use compiler::*;
pub use coordinates::*;
pub use executor::*;
pub use image_search::*;
pub use model::*;
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
pub use uia::*;
pub use validation::*;
pub use variables::*;
pub use virtual_desktops::*;
pub use windows::*;
