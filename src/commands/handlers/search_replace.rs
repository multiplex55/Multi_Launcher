use crate::commands::{CommandOutcome, SearchReplaceCommand, SearchReplaceCommandHost};

pub(crate) fn handle_search_replace<H>(
    host: &mut H,
    command: SearchReplaceCommand,
) -> CommandOutcome
where
    H: SearchReplaceCommandHost + ?Sized,
{
    match command {
        SearchReplaceCommand::Open => host.open_search_replace(),
    }
    CommandOutcome::default()
}
