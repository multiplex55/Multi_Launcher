use crate::commands::{CommandOutcome, TableUtilityCommand, TableUtilityCommandHost};

pub(crate) fn handle_table_utility<H>(host: &mut H, command: TableUtilityCommand) -> CommandOutcome
where
    H: TableUtilityCommandHost + ?Sized,
{
    match command {
        TableUtilityCommand::Open => host.open_table_utility(),
    }
    CommandOutcome::default()
}
