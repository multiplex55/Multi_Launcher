//! Versioned model, validation, compilation, and persistence for the new macro system.
pub mod compiler;
pub mod coordinates;
pub mod executor;
pub mod hotkeys;
pub mod input;
pub mod model;
pub mod runtime;
pub mod store;
pub mod validation;
pub mod variables;
pub mod windows;

pub use compiler::*;
pub use coordinates::*;
pub use executor::*;
pub use model::*;
pub use runtime::{
    CommandResult, MacroRuntime, RuntimeCommand, RuntimeSnapshot, RuntimeState, StepState,
};
pub use store::*;
pub use validation::*;
pub use variables::*;
pub use windows::*;
