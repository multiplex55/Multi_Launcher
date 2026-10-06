use crate::commands::{
    ColorPickCommand, ColorPickCommandHost, CommandError, CommandOutcome, HistoryPolicy,
};

pub(crate) fn handle_color_pick<H: ColorPickCommandHost + ?Sized>(
    host: &mut H,
    command: &ColorPickCommand,
) -> Result<CommandOutcome, CommandError> {
    let started = match command {
        ColorPickCommand::Pick => host.start_color_pick(),
    }
    .map_err(|error| CommandError::new("color_pick", error).with_refocus_policy())?;
    Ok(CommandOutcome {
        history: if started {
            HistoryPolicy::Record
        } else {
            HistoryPolicy::Skip
        },
        ..CommandOutcome::default()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{QueryPolicy, ResultsPolicy, VisibilityPolicy};
    struct Host {
        started: bool,
        error: bool,
        calls: usize,
    }
    impl ColorPickCommandHost for Host {
        fn start_color_pick(&mut self) -> Result<bool, String> {
            self.calls += 1;
            if self.error {
                Err("conflict".into())
            } else {
                Ok(self.started)
            }
        }
    }
    #[test]
    fn color_pick_handler_keeps_launcher_state_and_records_only_new_session() {
        for started in [true, false] {
            let mut host = Host {
                started,
                error: false,
                calls: 0,
            };
            let outcome = handle_color_pick(&mut host, &ColorPickCommand::Pick).unwrap();
            assert_eq!(host.calls, 1);
            assert_eq!(outcome.query, QueryPolicy::Keep);
            assert_eq!(outcome.results, ResultsPolicy::Keep);
            assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
            assert!(!outcome.focus && !outcome.restore && !outcome.search);
            assert_eq!(
                outcome.history,
                if started {
                    HistoryPolicy::Record
                } else {
                    HistoryPolicy::Skip
                }
            );
        }
        let mut host = Host {
            started: false,
            error: true,
            calls: 0,
        };
        let error = handle_color_pick(&mut host, &ColorPickCommand::Pick).unwrap_err();
        assert_eq!(error.domain, "color_pick");
        assert_eq!(error.message, "conflict");
    }
}
