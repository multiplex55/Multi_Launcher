use crate::commands::{
    CommandError, CommandOutcome, HistoryPolicy, ScreenshotCommand, ScreenshotCommandHost,
    ScreenshotCommandResult, ScreenshotDelay, ScreenshotDestination, ScreenshotMarkup,
    ScreenshotMode,
};

pub(crate) fn handle_screenshot<H>(
    host: &mut H,
    command: &ScreenshotCommand,
    invocation: &crate::commands::CommandInvocation,
    captured_history_query: Option<&str>,
) -> Result<CommandOutcome, CommandError>
where
    H: ScreenshotCommandHost + ?Sized,
{
    match command {
        ScreenshotCommand::Schedule {
            delay,
            mode,
            destination,
            markup,
        } => {
            host.schedule_delayed_screenshot(
                *delay,
                *mode,
                *destination,
                *markup,
                invocation,
                captured_history_query,
            )
            .map_err(|error| {
                CommandError::new("launcher", format!("Failed: {error}")).with_refocus_policy()
            })?;
            return Ok(CommandOutcome::default());
        }
        ScreenshotCommand::CancelDelayed => {
            host.cancel_delayed_screenshot().map_err(|error| {
                CommandError::new("launcher", format!("Failed: {error}")).with_refocus_policy()
            })?;
            return Ok(CommandOutcome::default());
        }
        ScreenshotCommand::InvalidDelayed { .. } => {
            return Err(CommandError::new(
                "launcher",
                "Invalid delayed screenshot request. Choose a supported delay and capture mode.",
            )
            .with_refocus_policy());
        }
        ScreenshotCommand::Capture { .. } | ScreenshotCommand::UnknownMode { .. } => {}
    }

    let (mode, destination, markup) = match command {
        ScreenshotCommand::Capture {
            mode,
            destination,
            markup,
            ..
        } => (*mode, *destination, *markup),
        // GUI activation historically treated unknown screenshot modes as a
        // desktop capture. Headless execution intentionally keeps its external
        // fallback in commands::headless.
        ScreenshotCommand::UnknownMode { .. } => (
            ScreenshotMode::Desktop,
            ScreenshotDestination::Editor,
            ScreenshotMarkup::Rectangle,
        ),
        ScreenshotCommand::Schedule { .. }
        | ScreenshotCommand::CancelDelayed
        | ScreenshotCommand::InvalidDelayed { .. } => unreachable!("handled above"),
    };

    let result = host
        .capture_screenshot(mode, destination, markup)
        .map_err(|error| {
            CommandError::new("launcher", format!("Failed: {error}")).with_refocus_policy()
        })?;

    Ok(CommandOutcome {
        focus: host.screenshot_launcher_should_refocus(),
        history: match result {
            ScreenshotCommandResult::Completed => HistoryPolicy::Record,
            ScreenshotCommandResult::Cancelled => HistoryPolicy::Skip,
        },
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{QueryPolicy, VisibilityPolicy};

    struct Host {
        call: Option<(ScreenshotMode, ScreenshotDestination, ScreenshotMarkup)>,
        scheduled: Option<(
            ScreenshotDelay,
            ScreenshotMode,
            ScreenshotDestination,
            ScreenshotMarkup,
            crate::commands::CommandInvocation,
            Option<String>,
        )>,
        cancelled: usize,
        result: Result<ScreenshotCommandResult, String>,
        refocus: bool,
    }

    impl Default for Host {
        fn default() -> Self {
            Self {
                call: None,
                scheduled: None,
                cancelled: 0,
                result: Ok(ScreenshotCommandResult::Completed),
                refocus: false,
            }
        }
    }

    impl ScreenshotCommandHost for Host {
        fn capture_screenshot(
            &mut self,
            mode: ScreenshotMode,
            destination: ScreenshotDestination,
            markup: ScreenshotMarkup,
        ) -> Result<ScreenshotCommandResult, String> {
            self.call = Some((mode, destination, markup));
            self.result.clone()
        }

        fn screenshot_launcher_should_refocus(&self) -> bool {
            self.refocus
        }

        fn schedule_delayed_screenshot(
            &mut self,
            delay: ScreenshotDelay,
            mode: ScreenshotMode,
            destination: ScreenshotDestination,
            markup: ScreenshotMarkup,
            invocation: &crate::commands::CommandInvocation,
            captured_history_query: Option<&str>,
        ) -> Result<(), String> {
            self.scheduled = Some((
                delay,
                mode,
                destination,
                markup,
                invocation.clone(),
                captured_history_query.map(str::to_owned),
            ));
            Ok(())
        }

        fn cancel_delayed_screenshot(&mut self) -> Result<(), String> {
            self.cancelled += 1;
            Ok(())
        }
    }

    fn make_invocation(command: ScreenshotCommand) -> crate::commands::CommandInvocation {
        crate::commands::CommandInvocation {
            command: crate::commands::Command::Screenshot(command),
            original_action: crate::actions::Action {
                label: "Delayed screenshot".into(),
                desc: String::new(),
                action: "screenshot:delay:3:window".into(),
                args: Some("private-query-context".into()),
            },
            query_override: None,
            source: crate::commands::ActivationSource::Enter,
        }
    }

    #[test]
    fn capture_passes_typed_mode_destination_and_markup() {
        let cases = [
            (
                ScreenshotCommand::Capture {
                    mode: ScreenshotMode::Window,
                    destination: ScreenshotDestination::Clipboard,
                    markup: ScreenshotMarkup::Rectangle,
                    compatibility: crate::commands::ScreenshotCompatibility::Shared,
                },
                (
                    ScreenshotMode::Window,
                    ScreenshotDestination::Clipboard,
                    ScreenshotMarkup::Rectangle,
                ),
            ),
            (
                ScreenshotCommand::Capture {
                    mode: ScreenshotMode::Region,
                    destination: ScreenshotDestination::Editor,
                    markup: ScreenshotMarkup::Pen,
                    compatibility: crate::commands::ScreenshotCompatibility::GuiOnly,
                },
                (
                    ScreenshotMode::Region,
                    ScreenshotDestination::Editor,
                    ScreenshotMarkup::Pen,
                ),
            ),
        ];

        for (command, expected) in cases {
            let mut host = Host::default();
            let invocation = make_invocation(command.clone());
            let outcome = handle_screenshot(&mut host, &command, &invocation, None).unwrap();
            assert_eq!(host.call, Some(expected));
            assert_eq!(outcome.history, HistoryPolicy::Record);
            assert_eq!(outcome.query, QueryPolicy::Keep);
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
            assert!(!outcome.search);
        }
    }

    #[test]
    fn unknown_gui_mode_keeps_desktop_editor_compatibility() {
        let mut host = Host::default();
        handle_screenshot(
            &mut host,
            &ScreenshotCommand::UnknownMode {
                raw: "future".into(),
            },
            &make_invocation(ScreenshotCommand::UnknownMode {
                raw: "future".into(),
            }),
            None,
        )
        .unwrap();

        assert_eq!(
            host.call,
            Some((
                ScreenshotMode::Desktop,
                ScreenshotDestination::Editor,
                ScreenshotMarkup::Rectangle,
            ))
        );
    }

    #[test]
    fn cancellation_skips_history_without_generic_clear_or_hide() {
        let mut host = Host {
            result: Ok(ScreenshotCommandResult::Cancelled),
            refocus: true,
            ..Host::default()
        };
        let outcome = handle_screenshot(
            &mut host,
            &ScreenshotCommand::Capture {
                mode: ScreenshotMode::Region,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Rectangle,
                compatibility: crate::commands::ScreenshotCompatibility::Shared,
            },
            &make_invocation(ScreenshotCommand::Capture {
                mode: ScreenshotMode::Region,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Rectangle,
                compatibility: crate::commands::ScreenshotCompatibility::Shared,
            }),
            None,
        )
        .unwrap();

        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
        assert!(!outcome.search);
        assert!(outcome.focus);
    }

    #[test]
    fn failures_keep_legacy_wording_and_refocus_without_duplicate_toast() {
        let mut host = Host {
            result: Err("capture unavailable".into()),
            ..Host::default()
        };
        let error = handle_screenshot(
            &mut host,
            &ScreenshotCommand::Capture {
                mode: ScreenshotMode::Desktop,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Rectangle,
                compatibility: crate::commands::ScreenshotCompatibility::Shared,
            },
            &make_invocation(ScreenshotCommand::Capture {
                mode: ScreenshotMode::Desktop,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Rectangle,
                compatibility: crate::commands::ScreenshotCompatibility::Shared,
            }),
            None,
        )
        .unwrap_err();

        assert_eq!(error.domain, "launcher");
        assert_eq!(error.message, "Failed: capture unavailable");
        assert!(error.refocus);
        assert!(!error.toast);
    }

    #[test]
    fn delayed_schedule_and_cancel_are_not_reported_as_capture_success() {
        let command = ScreenshotCommand::Schedule {
            delay: ScreenshotDelay::FiveSeconds,
            mode: ScreenshotMode::Region,
            destination: ScreenshotDestination::Clipboard,
            markup: ScreenshotMarkup::Pen,
        };
        let invocation = make_invocation(command.clone());
        let mut host = Host::default();
        let outcome =
            handle_screenshot(&mut host, &command, &invocation, Some("captured query")).unwrap();

        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(host.call, None);
        let scheduled = host.scheduled.as_ref().expect("request scheduled");
        assert_eq!(scheduled.0, ScreenshotDelay::FiveSeconds);
        assert_eq!(scheduled.1, ScreenshotMode::Region);
        assert_eq!(scheduled.2, ScreenshotDestination::Clipboard);
        assert_eq!(scheduled.3, ScreenshotMarkup::Pen);
        assert_eq!(scheduled.4, invocation);
        assert_eq!(scheduled.5.as_deref(), Some("captured query"));

        let cancel = ScreenshotCommand::CancelDelayed;
        let cancel_invocation = make_invocation(cancel.clone());
        let outcome = handle_screenshot(&mut host, &cancel, &cancel_invocation, None).unwrap();
        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(host.cancelled, 1);
        assert_eq!(host.call, None);
    }
}
