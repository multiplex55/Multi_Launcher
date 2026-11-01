use crate::commands::{
    ActivationSource, CommandError, CommandOutcome, DialogCommandHost, ZipExtractorCommand,
};

pub(crate) fn handle_zip_extractor<H>(
    host: &mut H,
    command: ZipExtractorCommand,
    source: ActivationSource,
) -> Result<CommandOutcome, CommandError>
where
    H: DialogCommandHost + ?Sized,
{
    if matches!(
        source,
        ActivationSource::Macro | ActivationSource::HotFolder
    ) {
        return Err(CommandError::new(
            "zip_extractor",
            "ZIP Extractor cannot be opened from this command source",
        )
        .with_gui_failure_policy());
    }

    match command {
        ZipExtractorCommand::Open => host.open_zip_extractor_dialog().map_err(|_| {
            CommandError::new(
                "zip_extractor",
                "ZIP extraction requires the launcher interface",
            )
            .with_gui_failure_policy()
        })?,
    }
    Ok(CommandOutcome::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{HistoryPolicy, QueryPolicy};

    #[derive(Default)]
    struct Host {
        opens: usize,
        fail: bool,
    }

    impl DialogCommandHost for Host {
        fn open_zip_extractor_dialog(&mut self) -> Result<(), String> {
            self.opens += 1;
            if self.fail {
                Err("private fixture detail must not escape".into())
            } else {
                Ok(())
            }
        }

        fn open_qr_dialog(&mut self, _: Option<&str>) {}
        fn open_regex_tester_dialog(&mut self) {}
        fn open_help_dialog(&mut self) {}
        fn open_timer_dialog(&mut self) {}
        fn open_alarm_dialog(&mut self) {}
        fn open_shell_dialog(&mut self) {}
        fn open_bookmark_dialog(&mut self) {}
        fn open_snippet_dialog(&mut self) {}
        fn open_snippet_editor(&mut self, _: &str) {}
        fn open_favorite_dialog(&mut self, _: &str) {}
        fn open_legacy_macro_dialog(&mut self) {}
        fn open_mkmacro_dialog(&mut self) {}
        fn open_todo_dialog(&mut self) {}
        fn open_clipboard_dialog(&mut self) {}
        fn open_convert_dialog(&mut self) {}
        fn open_tempfile_dialog(&mut self) {}
        fn open_settings_dialog(&mut self) {}
        fn open_settings_search(&mut self, _: &str) {}
        fn open_dashboard_settings_dialog(&mut self) {}
        fn open_theme_dialog(&mut self) {}
        fn open_volume_dialog(&mut self) {}
        fn open_brightness_dialog(&mut self) {}
        fn open_cpu_list_dialog(&mut self, _: usize) {}
    }

    #[test]
    fn zip_open_is_payload_free_session_only_and_refuses_automation_sources() {
        let mut host = Host::default();
        let outcome = handle_zip_extractor(
            &mut host,
            ZipExtractorCommand::Open,
            ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(host.opens, 1);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert!(outcome.toasts.is_empty());

        for source in [ActivationSource::Macro, ActivationSource::HotFolder] {
            let error = handle_zip_extractor(&mut host, ZipExtractorCommand::Open, source)
                .expect_err("automation sources must not open the chooser");
            assert_eq!(error.domain, "zip_extractor");
            assert!(!error.message.contains("private"));
        }
        assert_eq!(host.opens, 1);
    }

    #[test]
    fn zip_host_failure_is_reported_without_backend_detail() {
        let mut host = Host {
            fail: true,
            ..Host::default()
        };
        let error = handle_zip_extractor(
            &mut host,
            ZipExtractorCommand::Open,
            ActivationSource::Click,
        )
        .expect_err("the host refusal is surfaced");
        assert_eq!(host.opens, 1);
        assert_eq!(
            error.message,
            "ZIP extraction requires the launcher interface"
        );
    }
}
