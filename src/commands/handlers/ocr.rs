use crate::commands::{CommandError, CommandOutcome, HistoryPolicy, OcrCommand, OcrCommandHost};

pub(crate) fn handle_ocr<H: OcrCommandHost + ?Sized>(
    host: &mut H,
    command: &OcrCommand,
) -> Result<CommandOutcome, CommandError> {
    let started = match command {
        OcrCommand::Start => host.start_ocr_selection(),
    }
    .map_err(|error| CommandError::new("ocr", error).with_refocus_policy())?;
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
        result: Result<bool, String>,
        calls: usize,
    }
    impl OcrCommandHost for Host {
        fn start_ocr_selection(&mut self) -> Result<bool, String> {
            self.calls += 1;
            self.result.clone()
        }
    }
    #[test]
    fn ocr_handler_preserves_launcher_state_and_records_new_invocation_only() {
        for started in [true, false] {
            let mut host = Host {
                result: Ok(started),
                calls: 0,
            };
            let outcome = handle_ocr(&mut host, &OcrCommand::Start).unwrap();
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
            result: Err("selector conflict".into()),
            calls: 0,
        };
        let error = handle_ocr(&mut host, &OcrCommand::Start).unwrap_err();
        assert_eq!(error.domain, "ocr");
        assert_eq!(error.message, "selector conflict");
    }
}
