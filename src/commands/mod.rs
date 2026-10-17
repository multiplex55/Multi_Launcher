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
    BatchRenameCommandHost, CalendarCommandHost, ClipboardModifyCommandHost, ColorPickCommandHost,
    CommandHost, CropCommandHost, DataCommandHost, DialogCommandHost, DiffCommandHost,
    FileSearchCommandHost, HeadlessCommandHost, HotFolderCommandHost, JsonUtilityCommandHost,
    LauncherCommandHost,
    MouseGestureCommandHost, MultiManagerCommandHost, NoteCommandHost, OcrCommandHost,
    PinboardCommandHost, RadialCommandHost, ScreenDrawCommandHost, ScreenshotCommandHost, ScreenshotCommandResult,
    SearchReplaceCommandHost, SnippetPromptIntent, TodoCommandHost, XmlToolkitCommandHost,
};
pub use model::*;
pub use outcome::*;
pub(crate) use parser::parse_mouse_wire;
pub use parser::{parse_action, parse_command};
