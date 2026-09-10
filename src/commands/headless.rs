//! Typed execution for callers that do not own a [`crate::gui::LauncherApp`].
//!
//! The canonical parser recognizes GUI protocols as well as commands that have
//! always been executable through [`crate::launcher::launch_action`]. This
//! module preserves that historical direct-call boundary: GUI-only commands
//! retain their former external fallback, while the static command families
//! execute without inspecting their wire strings again.

use crate::actions::Action;
use crate::plugins::calc_history::{self, CALC_HISTORY_FILE, CalcHistoryEntry, MAX_ENTRIES};

use super::*;

pub(crate) fn execute(command: Command, original_action: &Action) -> anyhow::Result<()> {
    execute_with_external(command, original_action, &mut |target, args| {
        crate::actions::exec::launch(target, args)
    })
}

fn execute_with_external(
    command: Command,
    original_action: &Action,
    external: &mut dyn FnMut(&str, Option<&str>) -> anyhow::Result<()>,
) -> anyhow::Result<()> {
    match command {
        Command::Todo(TodoCommand::Compatibility { .. })
        | Command::Timer(
            TimerCommand::InvalidCancel | TimerCommand::InvalidPause | TimerCommand::InvalidResume,
        )
        | Command::System(SystemCommand::InvalidCpuList)
        | Command::BrowserTab(BrowserTabCommand::InvalidSwitch)
        | Command::Storage(StorageCommand::InvalidTempfileAlias) => {
            external(&original_action.action, original_action.args.as_deref())
        }
        Command::Shell(command) => execute_shell(command),
        Command::Clipboard(command) => execute_clipboard(command),
        Command::Calculator(command) => execute_calculator(command),
        Command::Storage(command) => execute_storage(command, original_action),
        Command::Timer(command) => execute_timer(command, original_action),
        Command::System(command) => execute_system(command, original_action),
        Command::BrowserTab(command) => execute_browser_tab(command, original_action),
        Command::Media(command) => execute_media(command),
        Command::Layout(command) => execute_layout(command),
        Command::Macro(command) => execute_macro(command),
        Command::Todo(command) => execute_todo(command, original_action),
        Command::Note(NoteCommand::Reload) => reload_notes(),
        Command::Note(_) => execute_external(original_action),
        Command::ClipboardModify(command) => execute_clipboard_modify(command, original_action),
        Command::Screenshot(command) => execute_screenshot(command, original_action),
        Command::Data(_) => anyhow::bail!("data commands require the launcher interface"),
        Command::External(command) => external(&command.target, command.args.as_deref()),

        // These families require LauncherApp state. Before the typed parser
        // recognized them, direct launch_action callers passed their original
        // protocol string to the OS launcher, so retain that entry-point
        // behavior until their GUI handlers migrate to the command bus.
        Command::Launcher(_)
        | Command::Query(_)
        | Command::Dialog(_)
        | Command::Calendar(_)
        | Command::Link(_)
        | Command::MouseGesture(_)
        | Command::MultiManager(_)
        | Command::FileSearch(_)
        | Command::Diff(_)
        | Command::Crop(_) => external(&original_action.action, original_action.args.as_deref()),
    }
}

fn execute_shell(command: ShellCommand) -> anyhow::Result<()> {
    use crate::actions::shell;
    match command {
        ShellCommand::Dialog => shell::run("dialog", false),
        ShellCommand::Run { command, keep_open } => shell::run(&command, keep_open),
        ShellCommand::Add { name, args } => shell::add(&name, &args),
        ShellCommand::Remove { name } => shell::remove(&name),
    }
}

fn execute_clipboard(command: ClipboardCommand) -> anyhow::Result<()> {
    use crate::actions::clipboard;
    match command {
        ClipboardCommand::Dialog => clipboard::set_text("dialog"),
        ClipboardCommand::Clear => clipboard::clear_history(),
        ClipboardCommand::Copy { index } => clipboard::copy_entry(index),
        ClipboardCommand::SetText { text } => clipboard::set_text(&text),
    }
}

fn execute_calculator(command: CalculatorCommand) -> anyhow::Result<()> {
    use crate::actions::clipboard;
    match command {
        CalculatorCommand::CopyResult { result, expression } => {
            if let Some(expression) = expression {
                let entry = CalcHistoryEntry {
                    expr: expression,
                    result: result.clone(),
                };
                let _ = calc_history::append_entry(CALC_HISTORY_FILE, entry, MAX_ENTRIES);
            }
            clipboard::calc_to_clipboard(&result)
        }
        CalculatorCommand::CopyHistory { index } => {
            crate::actions::calc::copy_history_result(index)
        }
    }
}

fn execute_storage(command: StorageCommand, original: &Action) -> anyhow::Result<()> {
    use crate::actions::{bookmarks, folders, history, snippets, tempfiles};
    match command {
        StorageCommand::BookmarkAdd(url) => bookmarks::add(&url),
        StorageCommand::BookmarkRemove(url) => bookmarks::remove(&url),
        StorageCommand::FolderAdd(path) => folders::add(&path),
        StorageCommand::FolderRemove(path) => folders::remove(&path),
        StorageCommand::HistoryClear => history::clear(),
        StorageCommand::HistoryLaunch(index) => history::launch_index(index),
        StorageCommand::SnippetAdd { alias, text } => snippets::add(&alias, &text),
        StorageCommand::SnippetEdit(_) => Ok(()),
        StorageCommand::SnippetRemove(alias) => snippets::remove(&alias),
        StorageCommand::FavoriteAdd {
            label,
            command,
            args,
        } => crate::actions::fav::add(&label, &command, args.as_deref()),
        StorageCommand::FavoriteRemove(label) => crate::actions::fav::remove(&label),
        StorageCommand::TempfileNew(alias) => tempfiles::new(alias.as_deref()),
        StorageCommand::TempfileOpen => tempfiles::open_dir(),
        StorageCommand::TempfileOpenFile(path) => tempfiles::open_file(&path),
        StorageCommand::TempfileClear => tempfiles::clear(),
        StorageCommand::TempfileRemove(path) => tempfiles::remove(&path),
        StorageCommand::TempfileAlias { path, alias } => tempfiles::set_alias(&path, &alias),
        StorageCommand::InvalidTempfileAlias => execute_external(original),
        StorageCommand::RecycleClean => {
            crate::actions::system::recycle_clean();
            Ok(())
        }
        StorageCommand::BookmarkDialog
        | StorageCommand::SnippetDialog
        | StorageCommand::FavoriteDialog(_)
        | StorageCommand::TempfileDialog => execute_external(original),
    }
}

fn execute_timer(command: TimerCommand, original: &Action) -> anyhow::Result<()> {
    use crate::actions::{stopwatch, timer};
    match command {
        TimerCommand::TimerDialog
        | TimerCommand::AlarmDialog
        | TimerCommand::InvalidCancel
        | TimerCommand::InvalidPause
        | TimerCommand::InvalidResume => execute_external(original),
        TimerCommand::Cancel(id) => effect(|| timer::cancel(id)),
        TimerCommand::Pause(id) => effect(|| timer::pause(id)),
        TimerCommand::Resume(id) => effect(|| timer::resume(id)),
        TimerCommand::Start { duration, name } => effect(|| timer::start(&duration, &name)),
        TimerCommand::AlarmSet { time, name } => effect(|| timer::set_alarm(&time, &name)),
        TimerCommand::StopwatchPause(id) => effect(|| stopwatch::pause(id)),
        TimerCommand::StopwatchResume(id) => effect(|| stopwatch::resume(id)),
        TimerCommand::StopwatchStop(id) => effect(|| stopwatch::stop(id)),
        TimerCommand::StopwatchStart(name) => effect(|| stopwatch::start(&name)),
        TimerCommand::StopwatchShow(_) => Ok(()),
    }
}

fn execute_system(command: SystemCommand, original: &Action) -> anyhow::Result<()> {
    use crate::actions::system;
    match command {
        SystemCommand::Shutdown => system::run_system("shutdown"),
        SystemCommand::Reboot => system::run_system("reboot"),
        SystemCommand::Lock => system::run_system("lock"),
        SystemCommand::Logoff => system::run_system("logoff"),
        SystemCommand::Unknown(command) => system::run_system(&command),
        SystemCommand::ProcessKill(pid) => effect(|| system::process_kill(pid)),
        SystemCommand::ProcessSwitch(pid) => effect(|| system::process_switch(pid)),
        SystemCommand::WindowSwitch(hwnd) => effect(|| system::window_switch(hwnd)),
        SystemCommand::WindowClose(hwnd) => effect(|| system::window_close(hwnd)),
        SystemCommand::Brightness(value) => effect(|| system::set_brightness(value)),
        SystemCommand::Volume(value) => effect(|| system::set_volume(value)),
        SystemCommand::ProcessVolume { pid, level } => {
            effect(|| system::set_process_volume(pid, level))
        }
        SystemCommand::ProcessToggleMute(pid) => effect(|| system::toggle_process_mute(pid)),
        SystemCommand::MuteActive => effect(system::mute_active_window),
        SystemCommand::ToggleMute => effect(system::toggle_system_mute),
        SystemCommand::PowerPlan(guid) => system::set_power_plan(&guid),
        SystemCommand::Keys(spec) => crate::actions::keys::send(&spec),
        SystemCommand::BrightnessDialog
        | SystemCommand::VolumeDialog
        | SystemCommand::CpuList(_)
        | SystemCommand::InvalidCpuList => execute_external(original),
    }
}

fn execute_browser_tab(command: BrowserTabCommand, original: &Action) -> anyhow::Result<()> {
    match command {
        BrowserTabCommand::Switch(ids) => {
            crate::actions::system::browser_tab_switch(&ids);
            Ok(())
        }
        BrowserTabCommand::InvalidSwitch => execute_external(original),
        BrowserTabCommand::Cache => {
            crate::plugins::browser_tabs::rebuild_cache();
            Ok(())
        }
        BrowserTabCommand::Clear => {
            crate::plugins::browser_tabs::clear_cache();
            Ok(())
        }
    }
}

fn execute_media(command: MediaCommand) -> anyhow::Result<()> {
    match command {
        MediaCommand::Play => crate::actions::media::play(),
        MediaCommand::Pause => crate::actions::media::pause(),
        MediaCommand::Next => crate::actions::media::next(),
        MediaCommand::Previous => crate::actions::media::prev(),
    }
}

fn execute_layout(command: LayoutCommand) -> anyhow::Result<()> {
    use crate::actions::layout;
    match command {
        LayoutCommand::Save { name, flags } => layout::save_layout(&name, flags.as_deref()),
        LayoutCommand::Load { name, flags } => layout::load_layout(&name, flags.as_deref()),
        LayoutCommand::Show { name, flags } => layout::show_layout(&name, flags.as_deref()),
        LayoutCommand::Remove { name, flags } => layout::remove_layout(&name, flags.as_deref()),
        LayoutCommand::List { flags } => layout::list_layouts(flags.as_deref()),
        LayoutCommand::Edit => layout::edit_layouts(),
    }
}

fn execute_macro(command: MacroCommand) -> anyhow::Result<()> {
    match command {
        MacroCommand::LegacyDialog => crate::plugins::macros::run_macro("dialog"),
        MacroCommand::MkDialog => Ok(()),
        MacroCommand::RunLegacy(name) => crate::plugins::macros::run_macro(&name),
        MacroCommand::MkRun(id) => crate::mkmacro::runtime::run(id),
        MacroCommand::MkPause => crate::mkmacro::runtime::pause(),
        MacroCommand::MkResume => crate::mkmacro::runtime::resume(),
        MacroCommand::MkStop => crate::mkmacro::runtime::stop(),
        MacroCommand::MkRecord => anyhow::bail!("recording requires a target macro"),
        MacroCommand::MkRecordStop => crate::mkmacro::runtime::record_stop_for_review(),
        MacroCommand::Invalid { raw } => anyhow::bail!("invalid mkmacro action: {raw}"),
    }
}

fn execute_todo(command: TodoCommand, original: &Action) -> anyhow::Result<()> {
    use crate::actions::todo;
    match command {
        TodoCommand::Add {
            text,
            priority,
            tags,
            refs,
            ..
        } => todo::add(&text, priority, &tags, &refs),
        TodoCommand::SetPriority { index, priority } => todo::set_priority(index, priority),
        TodoCommand::SetTags { index, tags } => todo::set_tags(index, &tags),
        TodoCommand::Remove { index } => todo::remove(index),
        TodoCommand::Done { index } => todo::mark_done(index),
        TodoCommand::Clear => todo::clear_done(),
        TodoCommand::Export => todo::export().map(|_| ()),
        TodoCommand::Dialog
        | TodoCommand::View
        | TodoCommand::Edit { .. }
        | TodoCommand::Compatibility { .. } => execute_external(original),
    }
}

fn execute_clipboard_modify(
    command: ClipboardModifyCommand,
    original: &Action,
) -> anyhow::Result<()> {
    match command {
        ClipboardModifyCommand::Execute {
            payload: Some(payload),
            ..
        } => {
            use crate::clipboard_modify::actions::ClipboardModifyActionPayload;
            use crate::clipboard_modify::parser::ClipboardModifyIntent;
            let intent = match payload {
                ClipboardModifyActionPayload::ExecuteAdHocStages { stages, .. } => {
                    ClipboardModifyIntent::Stages(stages)
                }
                ClipboardModifyActionPayload::ExecuteTemplate { name, .. } => {
                    ClipboardModifyIntent::ApplyTemplate { name }
                }
                ClipboardModifyActionPayload::ExecuteSavedPipeline { name, .. } => {
                    ClipboardModifyIntent::ApplySavedPipeline { name }
                }
                ClipboardModifyActionPayload::Undo => ClipboardModifyIntent::Undo,
                ClipboardModifyActionPayload::OpenDialogSection { .. } => {
                    return Err(crate::clipboard_modify::clipboard::ClipboardError::Config(
                        "open-dialog payload cannot be executed".into(),
                    )
                    .into());
                }
            };
            let cancellation = std::sync::atomic::AtomicBool::new(false);
            crate::clipboard_modify::runtime::execute_intent(
                intent,
                &crate::clipboard_modify::store::shared_default_catalog(),
                &cancellation,
            )?;
            Ok(())
        }
        ClipboardModifyCommand::Execute {
            payload_error: Some(error),
            ..
        } => Err(crate::clipboard_modify::clipboard::ClipboardError::Config(error).into()),
        ClipboardModifyCommand::Execute { raw_argument, .. } => {
            crate::clipboard_modify::runtime::execute_action_args(
                raw_argument.as_deref(),
                &crate::clipboard_modify::store::shared_default_catalog(),
            )?;
            Ok(())
        }
        ClipboardModifyCommand::Undo { .. } => {
            crate::clipboard_modify::runtime::undo()?;
            Ok(())
        }
        ClipboardModifyCommand::Open { .. } | ClipboardModifyCommand::Error { .. } => {
            execute_external(original)
        }
    }
}

fn execute_screenshot(command: ScreenshotCommand, original: &Action) -> anyhow::Result<()> {
    match command {
        ScreenshotCommand::Capture {
            mode,
            destination,
            compatibility: ScreenshotCompatibility::Shared,
            ..
        } => {
            use crate::actions::screenshot::Mode;
            let mode = match mode {
                ScreenshotMode::Window => Mode::Window,
                ScreenshotMode::Region => Mode::Region,
                ScreenshotMode::Desktop => Mode::Desktop,
            };
            let clipboard = destination == ScreenshotDestination::Clipboard;
            crate::actions::screenshot::capture(mode, clipboard).map(|_| ())
        }
        ScreenshotCommand::Capture {
            compatibility: ScreenshotCompatibility::GuiOnly,
            ..
        }
        | ScreenshotCommand::UnknownMode { .. } => execute_external(original),
    }
}

fn reload_notes() -> anyhow::Result<()> {
    crate::plugins::note::load_notes()?;
    crate::plugins::note::refresh_cache()?;
    Ok(())
}

fn execute_external(action: &Action) -> anyhow::Result<()> {
    crate::actions::exec::launch(&action.action, action.args.as_deref())
}

fn effect(f: impl FnOnce()) -> anyhow::Result<()> {
    f();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn action(raw: &str) -> Action {
        Action {
            label: String::new(),
            desc: String::new(),
            action: raw.into(),
            args: None,
        }
    }

    #[test]
    fn migrated_volume_power_and_todo_cases_use_owned_commands() {
        assert_eq!(
            crate::commands::parse_action(&action("volume:pid_toggle_mute:42")).unwrap(),
            Command::System(SystemCommand::ProcessToggleMute(42))
        );
        assert_eq!(
            crate::commands::parse_action(&action("volume:toggle_mute")).unwrap(),
            Command::System(SystemCommand::ToggleMute)
        );
        assert_eq!(
            crate::commands::parse_action(&action("power:plan:set:balanced")).unwrap(),
            Command::System(SystemCommand::PowerPlan("balanced".into()))
        );

        let payload = crate::plugins::todo::TodoAddActionPayload {
            text: "ship | release, notes now".into(),
            priority: 9,
            tags: vec!["team|alpha,beta".into(), "has space".into()],
            refs: Vec::new(),
        };
        let encoded = crate::plugins::todo::encode_todo_add_action_payload(&payload).unwrap();
        assert_eq!(
            crate::commands::parse_action(&action(&format!("todo:add:{encoded}"))).unwrap(),
            Command::Todo(TodoCommand::Add {
                text: payload.text,
                priority: payload.priority,
                tags: payload.tags,
                refs: payload.refs,
                toast_text: encoded,
            })
        );
    }

    #[test]
    fn migrated_plan_cases_preserve_external_args_and_static_routing() {
        let mut external = action("notepad.exe");
        external.args = Some("foo.txt".into());
        assert_eq!(
            crate::commands::parse_action(&external).unwrap(),
            Command::External(ExternalCommand {
                target: "notepad.exe".into(),
                args: Some("foo.txt".into()),
                namespace: None,
            })
        );
        assert_eq!(
            crate::commands::parse_action(&action("volume:toggle_mute")).unwrap(),
            Command::System(SystemCommand::ToggleMute)
        );
    }

    #[test]
    fn intentional_headless_no_ops_succeed() {
        for command in [
            Command::Timer(TimerCommand::StopwatchShow(1)),
            Command::Storage(StorageCommand::SnippetEdit("hello".into())),
            Command::Macro(MacroCommand::MkDialog),
        ] {
            execute(command, &action("unused")).unwrap();
        }
    }

    #[test]
    fn external_and_gui_fallbacks_forward_the_expected_arguments() {
        let mut original = action("query:notes");
        original.args = Some("original args".into());
        let gui_command = crate::commands::parse_action(&original).unwrap();
        let mut calls = Vec::new();
        execute_with_external(gui_command, &original, &mut |target, args| {
            calls.push((target.to_string(), args.map(str::to_string)));
            Ok(())
        })
        .unwrap();

        let mut external_action = action("tool.exe");
        external_action.args = Some("--flag value".into());
        let external_command = crate::commands::parse_action(&external_action).unwrap();
        execute_with_external(external_command, &external_action, &mut |target, args| {
            calls.push((target.to_string(), args.map(str::to_string)));
            Ok(())
        })
        .unwrap();

        assert_eq!(
            calls,
            [
                ("query:notes".into(), Some("original args".into())),
                ("tool.exe".into(), Some("--flag value".into())),
            ]
        );
    }

    #[test]
    fn data_commands_never_fall_back_to_raw_headless_execution() {
        let original = action("data:backup");
        let command = crate::commands::parse_action(&original).unwrap();
        let mut calls = Vec::new();
        let error = execute_with_external(command, &original, &mut |target, args| {
            calls.push((target.to_owned(), args.map(str::to_owned)));
            Ok(())
        })
        .unwrap_err();
        assert!(error.to_string().contains("require the launcher interface"));
        assert!(calls.is_empty());
    }

    #[test]
    fn dialog_prefix_quirks_remain_typed_non_external_commands() {
        assert_eq!(
            crate::commands::parse_action(&action("shell:dialog")).unwrap(),
            Command::Shell(ShellCommand::Dialog)
        );
        assert_eq!(
            crate::commands::parse_action(&action("clipboard:dialog")).unwrap(),
            Command::Clipboard(ClipboardCommand::Dialog)
        );
    }

    #[test]
    fn malformed_compatibility_commands_keep_headless_external_fallback() {
        for raw in [
            "todo:add:bad",
            "todo:remove:bad",
            "todo:edit:bad",
            "timer:cancel:bad",
            "tab:switch:bad",
            "sysinfo:cpu_list:bad",
            "tempfile:alias:bad",
        ] {
            let original = action(raw);
            let command = crate::commands::parse_action(&original).unwrap();
            let mut calls = Vec::new();
            execute_with_external(command, &original, &mut |target, args| {
                calls.push((target.to_string(), args.map(str::to_string)));
                Ok(())
            })
            .unwrap();
            assert_eq!(calls, [(raw.into(), None)], "{raw}");
        }
    }
    #[test]
    fn invalid_mkmacro_facade_error_keeps_legacy_wording() {
        let raw = "mkmacro:future";
        let error = crate::launcher::launch_action(&action(raw)).unwrap_err();
        assert_eq!(error.to_string(), format!("invalid mkmacro action: {raw}"));
    }

    #[test]
    fn clipboard_modify_action_args_take_precedence_over_legacy_suffix() {
        let mut original = action("clipboard_modify:execute:cm lower");
        original.args = Some("cm upper".into());
        assert!(matches!(
            crate::commands::parse_action(&original).unwrap(),
            Command::ClipboardModify(ClipboardModifyCommand::Execute {
                raw_argument: Some(raw),
                ..
            }) if raw == "cm upper"
        ));
    }
    #[test]
    fn clipboard_modify_suffix_is_already_typed() {
        let parsed =
            crate::commands::parse_action(&action("clipboard_modify:execute:cm upper")).unwrap();
        assert!(matches!(
            parsed,
            Command::ClipboardModify(ClipboardModifyCommand::Execute {
                raw_argument: Some(raw),
                ..
            }) if raw == "cm upper"
        ));
    }
}
