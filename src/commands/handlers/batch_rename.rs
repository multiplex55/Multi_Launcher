use crate::commands::{BatchRenameCommand, BatchRenameCommandHost, CommandOutcome};

pub(crate) fn handle_batch_rename<H>(host: &mut H, command: BatchRenameCommand) -> CommandOutcome
where
    H: BatchRenameCommandHost + ?Sized,
{
    match command {
        BatchRenameCommand::Open => host.open_batch_rename(),
    }
    CommandOutcome::default()
}
