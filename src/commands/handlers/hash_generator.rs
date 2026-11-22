use crate::commands::{CommandOutcome, HashGeneratorCommand};

pub fn handle_hash_generator(
    command: &HashGeneratorCommand,
    open: impl FnOnce() -> Result<(), String>,
) -> Result<CommandOutcome, String> {
    match command {
        HashGeneratorCommand::Open => open()?,
    }

    Ok(CommandOutcome::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn open_dispatch_calls_host_and_keeps_launcher_outcome() {
        let mut opened = 0;
        let outcome = handle_hash_generator(&HashGeneratorCommand::Open, || {
            opened += 1;
            Ok(())
        })
        .unwrap();

        assert_eq!(opened, 1);
        assert!(outcome.toasts.is_empty());
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
    }

    #[test]
    fn open_dispatch_returns_presentation_failure() {
        let result = handle_hash_generator(&HashGeneratorCommand::Open, || {
            Err("GPUI window could not be opened".into())
        });

        assert_eq!(result.unwrap_err(), "GPUI window could not be opened");
    }
}
