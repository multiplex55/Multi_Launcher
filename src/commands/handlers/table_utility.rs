use crate::commands::{CommandOutcome, TableUtilityCommand, TableUtilityCommandHost};

pub(crate) fn handle_table_utility<H>(host: &mut H, command: TableUtilityCommand) -> CommandOutcome
where
    H: TableUtilityCommandHost + ?Sized,
{
    match command {
        TableUtilityCommand::Open { profile } => host.open_table_utility(profile),
    }
    CommandOutcome::default()
}
