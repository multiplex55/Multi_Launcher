//! Owned command-domain model and the single side-effect-free action parser.

mod bus;
mod error;
pub(crate) mod headless;
mod host;
mod model;
mod outcome;
mod parser;

pub mod handlers;

pub use bus::CommandBus;
pub use error::CommandError;
pub use host::{
    CalendarCommandHost, CommandHost, CropCommandHost, DialogCommandHost, HeadlessCommandHost,
    LauncherCommandHost, LegacyCommandHost, MouseGestureCommandHost, NoteCommandHost,
    TodoCommandHost,
};
pub use model::*;
pub use outcome::*;
pub use parser::{parse_action, parse_command};
