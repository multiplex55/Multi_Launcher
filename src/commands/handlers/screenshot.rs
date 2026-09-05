use crate::commands::{
    CommandError, CommandOutcome, HistoryPolicy, ScreenshotCommand, ScreenshotCommandHost,
    ScreenshotCommandResult, ScreenshotDestination, ScreenshotMarkup, ScreenshotMode,
};

pub(crate) fn handle_screenshot<H>(
    host: &mut H,
    command: &ScreenshotCommand,
) -> Result<CommandOutcome, CommandError>
where
    H: ScreenshotCommandHost + ?Sized,
{
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
        result: Result<ScreenshotCommandResult, String>,
        refocus: bool,
    }

    impl Default for Host {
        fn default() -> Self {
            Self {
                call: None,
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
            let outcome = handle_screenshot(&mut host, &command).unwrap();
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
        )
        .unwrap_err();

        assert_eq!(error.domain, "launcher");
        assert_eq!(error.message, "Failed: capture unavailable");
        assert!(error.refocus);
        assert!(!error.toast);
    }
}
