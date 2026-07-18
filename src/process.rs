use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// Configure a command intended to run in the background without interactive UI.
pub fn configure_background_command(command: &mut Command) {
    #[cfg(windows)]
    {
        command.creation_flags(CREATE_NO_WINDOW);
    }

    #[cfg(not(windows))]
    {
        let _ = command;
    }
}

#[cfg(test)]
mod tests {
    use super::configure_background_command;
    use std::process::Command;

    #[test]
    #[cfg(not(windows))]
    fn configure_background_command_preserves_arguments_on_non_windows() {
        let mut command = Command::new("rg");
        command.arg("--json").arg("needle");

        configure_background_command(&mut command);

        let args: Vec<_> = command
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["--json", "needle"]);
    }
}
