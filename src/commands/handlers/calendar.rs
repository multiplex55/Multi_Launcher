use std::collections::HashMap;

use chrono::{Duration, Local};

use crate::actions::Action;
use crate::commands::{
    CalendarCommand, CommandHost, CommandOutcome, QueryPolicy, ResultsPolicy, ToastPolicy,
};
use crate::plugins::calendar::{
    CALENDAR_DATA, CALENDAR_STATE_FILE, add_event, expand_instances, format_event_label,
    load_state, parse_calendar_add, parse_calendar_search, parse_date_reference,
    parse_duration_spec, save_state, search_events, snooze_event,
};

pub(crate) fn handle_calendar(
    host: &mut dyn CommandHost,
    command: &CalendarCommand,
) -> CommandOutcome {
    match command {
        CalendarCommand::Open { view } => open(host, view),
        CalendarCommand::Jump { reference } => jump(host, reference),
        CalendarCommand::Add { input } => add(host, input),
        CalendarCommand::Search { input } => search(host, input),
        CalendarCommand::Upcoming => upcoming(),
        CalendarCommand::Snooze { input } => snooze(host, input),
    }
}

fn open(host: &mut dyn CommandHost, view: &str) -> CommandOutcome {
    let now = Local::now().naive_local();
    let mut outcome = CommandOutcome::default();
    let mut state = load_state(CALENDAR_STATE_FILE).unwrap_or_default();
    state.last_opened = Some(now);
    state.last_viewed_day = Some(now.date());
    if let Err(error) = save_state(CALENDAR_STATE_FILE, &state) {
        outcome
            .toasts
            .push(ToastPolicy::Error(format!("Calendar state error: {error}")));
    }
    if host.calendar_dashboard_enabled() {
        refresh_query(&mut outcome, String::new());
    }
    host.open_calendar_popover(now.date());
    let label = if view == "default" {
        "Opened calendar".to_string()
    } else {
        format!("Opened calendar ({view} view)")
    };
    outcome.toasts.push(ToastPolicy::Success(label));
    outcome
}

fn jump(host: &mut dyn CommandHost, reference: &str) -> CommandOutcome {
    let now = Local::now().naive_local();
    let mut outcome = CommandOutcome::default();
    match parse_date_reference(reference, now.date()) {
        Some(date) => {
            let mut state = load_state(CALENDAR_STATE_FILE).unwrap_or_default();
            state.last_opened = Some(now);
            state.last_viewed_day = Some(date);
            if let Err(error) = save_state(CALENDAR_STATE_FILE, &state) {
                outcome
                    .toasts
                    .push(ToastPolicy::Error(format!("Calendar state error: {error}")));
            }
            if host.calendar_dashboard_enabled() {
                refresh_query(&mut outcome, String::new());
            } else {
                outcome.focus = host.launcher_should_refocus();
            }
            outcome.toasts.push(ToastPolicy::Success(format!(
                "Jumped to {}",
                date.format("%Y-%m-%d")
            )));
        }
        None => {
            outcome.toasts.push(ToastPolicy::Error(format!(
                "Invalid date reference: {reference}"
            )));
            outcome.focus = host.launcher_should_refocus();
        }
    }
    outcome
}

fn add(host: &mut dyn CommandHost, input: &str) -> CommandOutcome {
    let now = Local::now().naive_local();
    let mut outcome = CommandOutcome::default();
    match parse_calendar_add(input, now) {
        Ok(request) => match add_event(request, now) {
            Ok(event) => {
                host.refresh_calendar_cache();
                let query = if host.calendar_preserve_command() {
                    "cal add ".to_string()
                } else {
                    String::new()
                };
                refresh_query(&mut outcome, query);
                outcome
                    .toasts
                    .push(ToastPolicy::Success(format!("Added {}", event.title)));
            }
            Err(error) => {
                outcome
                    .toasts
                    .push(ToastPolicy::Error(format!("Calendar add failed: {error}")));
                outcome.focus = host.launcher_should_refocus();
            }
        },
        Err(error) => {
            outcome.toasts.push(ToastPolicy::Error(error));
            outcome.focus = host.launcher_should_refocus();
        }
    }
    outcome
}

fn search(host: &mut dyn CommandHost, input: &str) -> CommandOutcome {
    let mut outcome = CommandOutcome::default();
    match parse_calendar_search(input) {
        Ok(request) => {
            let actions = search_events(&request)
                .into_iter()
                .map(|event| Action {
                    label: format_event_label(&event),
                    desc: "Calendar".into(),
                    action: format!("calendar:jump:{}", event.start.format("%Y-%m-%d")),
                    args: None,
                })
                .collect::<Vec<_>>();
            let count = actions.len();
            outcome.query = QueryPolicy::Set(format!("cal find {input}"));
            outcome.results = ResultsPolicy::Replace(actions);
            outcome.focus = true;
            outcome
                .toasts
                .push(ToastPolicy::Info(format!("Found {count} events")));
        }
        Err(error) => {
            outcome.toasts.push(ToastPolicy::Error(error));
            outcome.focus = host.launcher_should_refocus();
        }
    }
    outcome
}

fn upcoming() -> CommandOutcome {
    let now = Local::now().naive_local();
    let events = CALENDAR_DATA
        .read()
        .map(|data| data.clone())
        .unwrap_or_default();
    let instances = expand_instances(&events, now, now + Duration::days(7), 50);
    let titles: HashMap<_, _> = events
        .into_iter()
        .map(|event| (event.id, event.title))
        .collect();
    let actions = instances
        .into_iter()
        .map(|instance| {
            let title = titles
                .get(&instance.source_event_id)
                .cloned()
                .unwrap_or_else(|| "Calendar event".to_string());
            let label = if instance.all_day {
                format!("{} ({} all-day)", title, instance.start.format("%Y-%m-%d"))
            } else {
                format!(
                    "{} ({} {})",
                    title,
                    instance.start.format("%Y-%m-%d"),
                    instance.start.format("%H:%M")
                )
            };
            Action {
                label,
                desc: "Calendar".into(),
                action: format!("calendar:jump:{}", instance.start.format("%Y-%m-%d")),
                args: None,
            }
        })
        .collect();
    CommandOutcome {
        query: QueryPolicy::Set("cal upcoming".into()),
        results: ResultsPolicy::Replace(actions),
        focus: true,
        ..CommandOutcome::default()
    }
}

fn snooze(host: &mut dyn CommandHost, input: &str) -> CommandOutcome {
    let mut outcome = CommandOutcome::default();
    let mut parts = input.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(duration_spec), Some(event_id)) => match parse_duration_spec(duration_spec) {
            Some(duration) => match snooze_event(event_id, duration) {
                Ok(true) => {
                    host.refresh_calendar_cache();
                    outcome
                        .toasts
                        .push(ToastPolicy::Success(format!("Snoozed event {event_id}")));
                }
                Ok(false) => outcome
                    .toasts
                    .push(ToastPolicy::Error(format!("Event not found: {event_id}"))),
                Err(error) => outcome
                    .toasts
                    .push(ToastPolicy::Error(format!("Snooze failed: {error}"))),
            },
            None => outcome.toasts.push(ToastPolicy::Error(
                "Invalid snooze duration (use 10m, 1h, 2d)".into(),
            )),
        },
        _ => outcome.toasts.push(ToastPolicy::Error(
            "Provide a duration and event id to snooze".into(),
        )),
    }
    outcome.focus = host.launcher_should_refocus();
    outcome
}

fn refresh_query(outcome: &mut CommandOutcome, query: String) {
    outcome.query = QueryPolicy::Set(query);
    outcome.search = true;
    outcome.invalidate_results = true;
    outcome.focus = true;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::{
        CalendarCommandHost, CropCommandHost, DialogCommandHost, HeadlessCommandHost,
        LauncherCommandHost, LegacyCommandHost, NoteCommandHost, TodoCommandHost,
    };

    #[derive(Default)]
    struct Host {
        dashboard: bool,
        preserve: bool,
        refocus: bool,
        opened: Option<chrono::NaiveDate>,
        refreshes: usize,
    }

    impl LauncherCommandHost for Host {
        fn launcher_is_visible(&self) -> bool {
            self.refocus
        }
    }
    impl CalendarCommandHost for Host {
        fn calendar_dashboard_enabled(&self) -> bool {
            self.dashboard
        }
        fn calendar_preserve_command(&self) -> bool {
            self.preserve
        }
        fn open_calendar_popover(&mut self, date: chrono::NaiveDate) {
            self.opened = Some(date);
        }
        fn refresh_calendar_cache(&mut self) {
            self.refreshes += 1;
        }
    }
    impl NoteCommandHost for Host {
        fn open_notes_dialog(&mut self) {}
        fn open_note_graph_dialog(&mut self, _: Option<&str>) {}
        fn open_unused_note_assets_dialog(&mut self) {}
        fn open_note_panel(&mut self, _: &str, _: Option<&str>) {}
        fn open_note_tags(&mut self) {}
        fn open_note_link(&mut self, _: &str) {}
        fn wrap_note_plain_links(&mut self, _: &str) {}
        fn delete_note(&mut self, _: &str) {}
    }
    impl CropCommandHost for Host {
        fn crop_image(&mut self) {}
        fn crop_screenshot(&mut self) {}
    }
    impl DialogCommandHost for Host {
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
        fn open_dashboard_settings_dialog(&mut self) {}
        fn open_theme_dialog(&mut self) {}
        fn open_volume_dialog(&mut self) {}
        fn open_brightness_dialog(&mut self) {}
        fn open_cpu_list_dialog(&mut self, _: usize) {}
    }
    impl TodoCommandHost for Host {
        fn open_todo_view(&mut self) {}
        fn open_todo_editor(&mut self, _: usize) {}
    }
    impl crate::commands::MouseGestureCommandHost for Host {
        fn open_mouse_gesture_dialog(&mut self) {}
        fn open_mouse_gesture_add_dialog(&mut self) {}
        fn open_mouse_gesture_binding_dialog(&mut self) {}
        fn open_mouse_gesture_focus(
            &mut self,
            _: &crate::mouse_gestures::selection::GestureFocusArgs,
        ) {
        }
        fn open_mouse_gesture_settings_dialog(&mut self) {}
        fn set_mouse_gesture_enabled(
            &mut self,
            _: &crate::mouse_gestures::selection::GestureToggleArgs,
        ) -> Result<(), String> {
            Ok(())
        }
        fn mouse_gesture_launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl crate::commands::MultiManagerCommandHost for Host {
        fn open_multi_manager(&mut self) {}
        fn open_multi_manager_settings(&mut self) {}
        fn multi_manager_save(&mut self) {}
        fn multi_manager_reload(&mut self) {}
        fn multi_manager_send_all_home(&mut self) {}
        fn multi_manager_start_manual_reconnect(&mut self) {}
        fn multi_manager_save_bindings(&mut self) {}
        fn multi_manager_restore_bindings(&mut self) {}
        fn multi_manager_import(&mut self) {}
        fn multi_manager_start_recapture_all(&mut self) {}
        fn multi_manager_toggle_workspace(&mut self, _: &str) {}
        fn multi_manager_send_home(&mut self, _: &str) {}
        fn multi_manager_send_target(&mut self, _: &str) {}
        fn multi_manager_start_capture(&mut self, _: &str) {}
        fn multi_manager_set_workspace_disabled(&mut self, _: &str, _: bool) {}
        fn multi_manager_launcher_should_refocus(&self) -> bool {
            false
        }
    }
    impl HeadlessCommandHost for Host {
        fn execute_headless_command(
            &mut self,
            _: &crate::commands::Command,
            _: &Action,
        ) -> anyhow::Result<()> {
            Ok(())
        }
        fn spawn_headless_command(&mut self, _: crate::commands::Command, _: Action) {}
        fn clear_query_after_run(&self) -> bool {
            false
        }
        fn hide_after_run(&self) -> bool {
            false
        }
        fn preserve_command(&self) -> bool {
            self.preserve
        }
        fn current_query(&self) -> &str {
            ""
        }
        fn launcher_should_refocus(&self) -> bool {
            self.refocus
        }
    }
    impl LegacyCommandHost for Host {
        fn execute_legacy_command(
            &mut self,
            _: &crate::commands::CommandInvocation,
        ) -> Result<CommandOutcome, crate::commands::CommandError> {
            unreachable!()
        }
    }

    #[test]
    fn invalid_jump_preserves_calendar_error_and_refocus_policy() {
        let mut host = Host {
            refocus: true,
            ..Host::default()
        };
        let outcome = handle_calendar(
            &mut host,
            &CalendarCommand::Jump {
                reference: "not-a-date".into(),
            },
        );
        assert_eq!(
            outcome.toasts,
            vec![ToastPolicy::Error(
                "Invalid date reference: not-a-date".into()
            )]
        );
        assert!(outcome.focus);
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Skip);
    }

    #[test]
    fn invalid_add_and_snooze_remain_no_history_error_outcomes() {
        let mut host = Host::default();
        let add = handle_calendar(
            &mut host,
            &CalendarCommand::Add {
                input: String::new(),
            },
        );
        assert!(matches!(add.toasts.as_slice(), [ToastPolicy::Error(_)]));
        assert_eq!(add.history, crate::commands::HistoryPolicy::Skip);

        let snooze = handle_calendar(
            &mut host,
            &CalendarCommand::Snooze {
                input: "bad".into(),
            },
        );
        assert_eq!(
            snooze.toasts,
            vec![ToastPolicy::Error(
                "Provide a duration and event id to snooze".into()
            )]
        );
        assert_eq!(snooze.history, crate::commands::HistoryPolicy::Skip);
    }

    #[test]
    fn search_replaces_results_without_running_launcher_search() {
        let mut host = Host::default();
        let outcome = handle_calendar(
            &mut host,
            &CalendarCommand::Search {
                input: "definitely-unmatched-calendar-query".into(),
            },
        );
        assert_eq!(
            outcome.query,
            QueryPolicy::Set("cal find definitely-unmatched-calendar-query".into())
        );
        assert!(matches!(outcome.results, ResultsPolicy::Replace(_)));
        assert!(!outcome.search);
        assert!(outcome.focus);
        assert_eq!(outcome.history, crate::commands::HistoryPolicy::Skip);
    }
}
