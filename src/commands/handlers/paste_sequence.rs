use crate::commands::{
    ActivationSource, CommandError, CommandOutcome, DialogCommandHost, PasteSequenceCommand,
};

pub(crate) fn handle_paste_sequence(
    host: &mut dyn DialogCommandHost,
    command: PasteSequenceCommand,
    source: ActivationSource,
) -> Result<CommandOutcome, CommandError> {
    let result = match command {
        PasteSequenceCommand::Open => {
            host.open_paste_sequence_dialog();
            Ok(())
        }
        PasteSequenceCommand::Run(id) => host.run_paste_sequence(id, source),
        PasteSequenceCommand::Start => host.start_paste_sequence(),
        PasteSequenceCommand::Next => host.next_paste_sequence(),
        PasteSequenceCommand::Cancel => {
            host.cancel_paste_sequence();
            Ok(())
        }
        PasteSequenceCommand::InvalidRun => Err("invalid paste sequence ID".to_owned()),
    };
    result
        .map(|()| CommandOutcome::default())
        .map_err(|error| CommandError::new("paste_sequence", error).with_gui_failure_policy())
}
