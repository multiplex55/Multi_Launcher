use crate::commands::{CommandOutcome, MultiManagerCommand, MultiManagerCommandHost};

pub(crate) fn handle_multi_manager<H>(host: &mut H, command: &MultiManagerCommand) -> CommandOutcome
where
    H: MultiManagerCommandHost + ?Sized,
{
    match command {
        MultiManagerCommand::Open => host.open_multi_manager(),
        MultiManagerCommand::Settings => host.open_multi_manager_settings(),
        MultiManagerCommand::Save => host.multi_manager_save(),
        MultiManagerCommand::Reload => host.multi_manager_reload(),
        MultiManagerCommand::SendAllHome => host.multi_manager_send_all_home(),
        MultiManagerCommand::Reconnect => host.multi_manager_start_manual_reconnect(),
        MultiManagerCommand::SaveBindings => host.multi_manager_save_bindings(),
        MultiManagerCommand::RestoreBindings => host.multi_manager_restore_bindings(),
        MultiManagerCommand::Import => host.multi_manager_import(),
        MultiManagerCommand::RecaptureAll => host.multi_manager_start_recapture_all(),
        MultiManagerCommand::Toggle(workspace_id) => {
            host.multi_manager_toggle_workspace(workspace_id)
        }
        MultiManagerCommand::Home(workspace_id) => host.multi_manager_send_home(workspace_id),
        MultiManagerCommand::Target(workspace_id) => host.multi_manager_send_target(workspace_id),
        MultiManagerCommand::Capture(workspace_id) => {
            host.multi_manager_start_capture(workspace_id)
        }
        MultiManagerCommand::Disable(workspace_id) => {
            host.multi_manager_set_workspace_disabled(workspace_id, true)
        }
        MultiManagerCommand::Enable(workspace_id) => {
            host.multi_manager_set_workspace_disabled(workspace_id, false)
        }
    }

    CommandOutcome {
        focus: host.multi_manager_launcher_should_refocus(),
        ..CommandOutcome::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{HistoryPolicy, QueryPolicy, VisibilityPolicy};

    #[derive(Default)]
    struct Host {
        operation: Option<String>,
        refocus: bool,
    }

    impl MultiManagerCommandHost for Host {
        fn open_multi_manager(&mut self) {
            self.operation = Some("open".into());
        }
        fn open_multi_manager_settings(&mut self) {
            self.operation = Some("settings".into());
        }
        fn multi_manager_save(&mut self) {
            self.operation = Some("save".into());
        }
        fn multi_manager_reload(&mut self) {
            self.operation = Some("reload".into());
        }
        fn multi_manager_send_all_home(&mut self) {
            self.operation = Some("send_all_home".into());
        }
        fn multi_manager_start_manual_reconnect(&mut self) {
            self.operation = Some("reconnect".into());
        }
        fn multi_manager_save_bindings(&mut self) {
            self.operation = Some("save_bindings".into());
        }
        fn multi_manager_restore_bindings(&mut self) {
            self.operation = Some("restore_bindings".into());
        }
        fn multi_manager_import(&mut self) {
            self.operation = Some("import".into());
        }
        fn multi_manager_start_recapture_all(&mut self) {
            self.operation = Some("recapture_all".into());
        }
        fn multi_manager_toggle_workspace(&mut self, workspace_id: &str) {
            self.operation = Some(format!("toggle:{workspace_id}"));
        }
        fn multi_manager_send_home(&mut self, workspace_id: &str) {
            self.operation = Some(format!("home:{workspace_id}"));
        }
        fn multi_manager_send_target(&mut self, workspace_id: &str) {
            self.operation = Some(format!("target:{workspace_id}"));
        }
        fn multi_manager_start_capture(&mut self, workspace_id: &str) {
            self.operation = Some(format!("capture:{workspace_id}"));
        }
        fn multi_manager_set_workspace_disabled(&mut self, workspace_id: &str, disabled: bool) {
            self.operation = Some(format!("disabled:{workspace_id}:{disabled}"));
        }
        fn multi_manager_launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }

    #[test]
    fn every_variant_routes_through_the_typed_host() {
        let cases = [
            (MultiManagerCommand::Open, "open"),
            (MultiManagerCommand::Settings, "settings"),
            (MultiManagerCommand::Save, "save"),
            (MultiManagerCommand::Reload, "reload"),
            (MultiManagerCommand::SendAllHome, "send_all_home"),
            (MultiManagerCommand::Reconnect, "reconnect"),
            (MultiManagerCommand::SaveBindings, "save_bindings"),
            (MultiManagerCommand::RestoreBindings, "restore_bindings"),
            (MultiManagerCommand::Import, "import"),
            (MultiManagerCommand::RecaptureAll, "recapture_all"),
            (MultiManagerCommand::Toggle("alpha".into()), "toggle:alpha"),
            (MultiManagerCommand::Home("alpha".into()), "home:alpha"),
            (MultiManagerCommand::Target("alpha".into()), "target:alpha"),
            (
                MultiManagerCommand::Capture("alpha".into()),
                "capture:alpha",
            ),
            (
                MultiManagerCommand::Disable("alpha".into()),
                "disabled:alpha:true",
            ),
            (
                MultiManagerCommand::Enable("alpha".into()),
                "disabled:alpha:false",
            ),
        ];

        for (command, expected) in cases {
            let mut host = Host::default();
            let outcome = handle_multi_manager(&mut host, &command);
            assert_eq!(host.operation.as_deref(), Some(expected));
            assert_eq!(outcome, CommandOutcome::default());
        }
    }

    #[test]
    fn multi_manager_only_applies_the_final_refocus_policy() {
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        let outcome = handle_multi_manager(&mut host, &MultiManagerCommand::Save);

        assert!(outcome.focus);
        assert_eq!(outcome.history, HistoryPolicy::Skip);
        assert_eq!(outcome.query, QueryPolicy::Keep);
        assert_eq!(outcome.visibility, VisibilityPolicy::Keep);
    }
}
