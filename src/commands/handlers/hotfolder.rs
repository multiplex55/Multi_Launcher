use crate::commands::{
    CommandError, CommandOutcome, HotFolderCommand, HotFolderCommandHost, VisibilityPolicy,
};

pub(crate) fn handle_hotfolder<H>(
    host: &mut H,
    command: HotFolderCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: HotFolderCommandHost + ?Sized,
{
    host.execute_hotfolder_command(command)
        .map_err(|error| CommandError::new("hotfolder", error).with_gui_failure_policy())?;
    Ok(CommandOutcome {
        visibility: matches!(command, HotFolderCommand::Open)
            .then_some(VisibilityPolicy::Show)
            .unwrap_or(VisibilityPolicy::Keep),
        restore: matches!(command, HotFolderCommand::Open),
        ..CommandOutcome::default()
    })
}
