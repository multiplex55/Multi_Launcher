//! Owned command-domain model and the single side-effect-free action parser.

mod error;
pub(crate) mod headless;
mod model;
mod parser;

pub use error::CommandError;
pub use model::*;
pub use parser::{parse_action, parse_command};
