use crate::commands::{CommandOutcome, DialogCommandHost, HashGeneratorCommand};

pub(crate) fn handle_hash_generator<H>(
    host: &mut H,
    command: &HashGeneratorCommand,
) -> CommandOutcome
where
    H: DialogCommandHost + ?Sized,
{
    match command {
        HashGeneratorCommand::Open => host.open_hash_generator_dialog(),
    }

    CommandOutcome::default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Default)]
    struct Host {
        opened: usize,
    }

    impl DialogCommandHost for Host {
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

        fn open_hash_generator_dialog(&mut self) {
            self.opened += 1;
        }
    }

    #[test]
    fn open_dispatch_calls_host_and_keeps_launcher_outcome() {
        let mut host = Host::default();
        let outcome = handle_hash_generator(&mut host, &HashGeneratorCommand::Open);

        assert_eq!(host.opened, 1);
        assert!(outcome.toasts.is_empty());
        assert_eq!(outcome.query, crate::commands::QueryPolicy::Keep);
        assert_eq!(outcome.visibility, crate::commands::VisibilityPolicy::Keep);
    }
}
