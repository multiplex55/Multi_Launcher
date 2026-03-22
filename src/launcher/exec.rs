use crate::actions::Action;
use crate::plugins::calc_history::{self, CalcHistoryEntry, CALC_HISTORY_FILE, MAX_ENTRIES};

use super::parse::ActionKind;
use super::plan::{plan_action, LaunchPlan};

/// Launch an [`Action`], interpreting a variety of custom prefixes.
///
/// Depending on the prefix, this may spawn external processes, modify
/// bookmarks or folders, copy text to the clipboard or evaluate calculator
/// expressions. Shell commands are only executed on Windows.
///
/// Returns an error if spawning an external process or interacting with the
/// clipboard fails.
pub fn launch_action(action: &Action) -> anyhow::Result<()> {
    execute_plan(plan_action(action))
}

pub(crate) fn execute_plan(plan: LaunchPlan<'_>) -> anyhow::Result<()> {
    use crate::actions::*;
    match plan.action {
        ActionKind::Shell { cmd, keep_open } => shell::run(cmd, keep_open),
        ActionKind::ShellAdd { name, args } => shell::add(name, args),
        ActionKind::ShellRemove(name) => shell::remove(name),
        ActionKind::ClipboardClear => clipboard::clear_history(),
        ActionKind::ClipboardCopy(i) => clipboard::copy_entry(i),
        ActionKind::ClipboardText(text) => clipboard::set_text(text),
        ActionKind::Calc { result, expr } => {
            if let Some(e) = expr {
                let entry = CalcHistoryEntry {
                    expr: e.to_string(),
                    result: result.to_string(),
                };
                let _ = calc_history::append_entry(CALC_HISTORY_FILE, entry, MAX_ENTRIES);
            }
            clipboard::calc_to_clipboard(result)
        }
        ActionKind::CalcHistory(i) => crate::actions::calc::copy_history_result(i),
        ActionKind::BookmarkAdd(url) => bookmarks::add(url),
        ActionKind::BookmarkRemove(url) => bookmarks::remove(url),
        ActionKind::FolderAdd(path) => folders::add(path),
        ActionKind::FolderRemove(path) => folders::remove(path),
        ActionKind::HistoryClear => history::clear(),
        ActionKind::HistoryIndex(i) => history::launch_index(i),
        ActionKind::System(cmd) => system::run_system(cmd),
        ActionKind::ProcessKill(pid) => {
            system::process_kill(pid);
            Ok(())
        }
        ActionKind::ProcessSwitch(pid) => {
            system::process_switch(pid);
            Ok(())
        }
        ActionKind::WindowSwitch(hwnd) => {
            system::window_switch(hwnd);
            Ok(())
        }
        ActionKind::WindowClose(hwnd) => {
            system::window_close(hwnd);
            Ok(())
        }
        ActionKind::BrowserTabSwitch(ids) => {
            system::browser_tab_switch(&ids);
            Ok(())
        }
        ActionKind::BrowserTabCache => {
            crate::plugins::browser_tabs::rebuild_cache();
            Ok(())
        }
        ActionKind::BrowserTabClear => {
            crate::plugins::browser_tabs::clear_cache();
            Ok(())
        }
        ActionKind::TimerCancel(id) => {
            timer::cancel(id);
            Ok(())
        }
        ActionKind::TimerPause(id) => {
            timer::pause(id);
            Ok(())
        }
        ActionKind::TimerResume(id) => {
            timer::resume(id);
            Ok(())
        }
        ActionKind::TimerStart { dur, name } => {
            timer::start(dur, name);
            Ok(())
        }
        ActionKind::AlarmSet { time, name } => {
            timer::set_alarm(time, name);
            Ok(())
        }
        ActionKind::StopwatchPause(id) => {
            stopwatch::pause(id);
            Ok(())
        }
        ActionKind::StopwatchResume(id) => {
            stopwatch::resume(id);
            Ok(())
        }
        ActionKind::StopwatchStop(id) => {
            stopwatch::stop(id);
            Ok(())
        }
        ActionKind::StopwatchStart { name } => {
            stopwatch::start(name);
            Ok(())
        }
        ActionKind::StopwatchShow(_id) => Ok(()),
        ActionKind::TodoAdd {
            text,
            priority,
            tags,
            refs,
        } => todo::add(&text, priority, &tags, &refs),
        ActionKind::TodoSetPriority { idx, priority } => todo::set_priority(idx, priority),
        ActionKind::TodoSetTags { idx, tags } => todo::set_tags(idx, &tags),
        ActionKind::TodoRemove(i) => todo::remove(i),
        ActionKind::TodoDone(i) => todo::mark_done(i),
        ActionKind::TodoClear => todo::clear_done(),
        ActionKind::TodoExport => {
            todo::export()?;
            Ok(())
        }
        ActionKind::SnippetRemove(alias) => snippets::remove(alias),
        ActionKind::SnippetEdit(_alias) => Ok(()),
        ActionKind::SnippetAdd { alias, text } => snippets::add(alias, text),
        ActionKind::FavAdd {
            label,
            command,
            args,
        } => crate::actions::fav::add(label, command, args),
        ActionKind::FavRemove(label) => crate::actions::fav::remove(label),
        ActionKind::BrightnessSet(v) => {
            system::set_brightness(v);
            Ok(())
        }
        ActionKind::VolumeSet(v) => {
            system::set_volume(v);
            Ok(())
        }
        ActionKind::VolumeSetProcess { pid, level } => {
            system::set_process_volume(pid, level);
            Ok(())
        }
        ActionKind::VolumeToggleMuteProcess { pid } => {
            system::toggle_process_mute(pid);
            Ok(())
        }
        ActionKind::VolumeMuteActive => {
            system::mute_active_window();
            Ok(())
        }
        ActionKind::VolumeToggleMute => {
            system::toggle_system_mute();
            Ok(())
        }
        ActionKind::Screenshot { mode, clip } => {
            crate::actions::screenshot::capture(mode, clip)?;
            Ok(())
        }
        ActionKind::MediaPlay => {
            crate::actions::media::play()?;
            Ok(())
        }
        ActionKind::MediaPause => {
            crate::actions::media::pause()?;
            Ok(())
        }
        ActionKind::MediaNext => {
            crate::actions::media::next()?;
            Ok(())
        }
        ActionKind::MediaPrev => {
            crate::actions::media::prev()?;
            Ok(())
        }
        ActionKind::RecycleClean => {
            system::recycle_clean();
            Ok(())
        }
        ActionKind::NoteReload => {
            crate::plugins::note::load_notes()?;
            crate::plugins::note::refresh_cache()?;
            Ok(())
        }
        ActionKind::TempfileNew(alias) => tempfiles::new(alias),
        ActionKind::TempfileOpen => tempfiles::open_dir(),
        ActionKind::TempfileOpenFile(path) => tempfiles::open_file(path),
        ActionKind::TempfileClear => tempfiles::clear(),
        ActionKind::TempfileRemove(path) => tempfiles::remove(path),
        ActionKind::TempfileAlias { path, alias } => tempfiles::set_alias(path, alias),
        ActionKind::LayoutSave { name, flags } => layout::save_layout(name, flags),
        ActionKind::LayoutLoad { name, flags } => layout::load_layout(name, flags),
        ActionKind::LayoutShow { name, flags } => layout::show_layout(name, flags),
        ActionKind::LayoutRemove { name, flags } => layout::remove_layout(name, flags),
        ActionKind::LayoutList { flags } => layout::list_layouts(flags),
        ActionKind::LayoutEdit => layout::edit_layouts(),
        ActionKind::Macro(name) => {
            crate::plugins::macros::run_macro(name)?;
            Ok(())
        }
        ActionKind::PowerPlanSet { guid } => system::set_power_plan(guid),
        ActionKind::Keys(spec) => keys::send(spec),
        ActionKind::ExecPath { path, args } => exec::launch(path, args),
    }
}
