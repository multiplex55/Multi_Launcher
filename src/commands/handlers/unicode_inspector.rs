use crate::commands::{
    ActivationSource, CommandError, CommandOutcome, DialogCommandHost, UnicodeInspectorCommand,
};

pub(crate) fn handle_unicode_inspector<H>(
    host: &mut H,
    command: &UnicodeInspectorCommand,
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
            "unicode_inspector",
            "Unicode Inspector cannot be opened by automated actions",
        ));
    }

    match command {
        UnicodeInspectorCommand::Open => host.open_unicode_inspector(),
    }
    Ok(CommandOutcome::default())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{HistoryPolicy, PendingQueryPolicy, QueryPolicy, ResultsPolicy};

    #[derive(Default)]
    struct Host {
        opens: usize,
    }

    impl DialogCommandHost for Host {
        fn open_qr_dialog(&mut self, _: Option<&str>) {}

        fn open_regex_tester_dialog(&mut self) {}

        fn open_unicode_inspector(&mut self) {
            self.opens += 1;
        }

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
    fn unicode_inspector_handler_denies_automation_before_host_and_opens_once_for_user() {
        for source in [ActivationSource::Macro, ActivationSource::HotFolder] {
            let mut host = Host::default();
            let error = handle_unicode_inspector(&mut host, &UnicodeInspectorCommand::Open, source)
                .unwrap_err();
            assert_eq!(error.domain, "unicode_inspector");
            assert!(error.message.contains("automated actions"));
            assert_eq!(host.opens, 0, "source {source:?}");
        }

        let mut host = Host::default();
        let outcome = handle_unicode_inspector(
            &mut host,
            &UnicodeInspectorCommand::Open,
            ActivationSource::Click,
        )
        .unwrap();
        assert_eq!(host.opens, 1);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.pending_query, PendingQueryPolicy::Keep);
        assert_eq!(outcome.results, ResultsPolicy::Keep);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
    }
}
