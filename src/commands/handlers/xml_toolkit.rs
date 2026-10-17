use crate::commands::{CommandOutcome, XmlToolkitCommand, XmlToolkitCommandHost};

pub(crate) fn handle_xml_toolkit<H>(host: &mut H, command: XmlToolkitCommand) -> CommandOutcome
where
    H: XmlToolkitCommandHost + ?Sized,
{
    match command {
        XmlToolkitCommand::Open => host.open_xml_toolkit(),
    }
    CommandOutcome::default()
}
