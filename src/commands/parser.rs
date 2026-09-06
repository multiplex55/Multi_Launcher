use crate::actions::Action;
use crate::clipboard_modify::actions::{
    ClipboardModifyActionPayload, ClipboardModifySectionPayload,
};
use crate::file_search::actions::{FileSearchModePayload, FileSearchStartPayload};
use crate::mouse_gestures::selection::{GestureFocusArgs, GestureToggleArgs};
use crate::plugins::note::NoteNewPayload;

use super::*;

pub fn parse_command(
    action: Action,
    query_override: Option<String>,
    source: ActivationSource,
) -> Result<CommandInvocation, CommandError> {
    let command = parse_action(&action)?;
    Ok(CommandInvocation {
        command,
        original_action: action,
        query_override,
        source,
    })
}

pub fn parse_action(action: &Action) -> Result<Command, CommandError> {
    let s = action.action.as_str();

    // These protocols bypass query-override application in the existing activation path.
    if let Some(command) = parse_clipboard_modify(action) {
        return Ok(Command::ClipboardModify(command));
    }
    if let Some(command) = parse_file_search(s) {
        return Ok(Command::FileSearch(command));
    }
    if let Some(encoded) = s.strip_prefix(crate::diff::query::OPEN_PREFIX) {
        let command = match crate::diff::query::decode_payload(encoded) {
            Ok(payload) => DiffCommand::Open(payload),
            Err(error) => DiffCommand::Invalid {
                raw: s.to_string(),
                error,
            },
        };
        return Ok(Command::Diff(command));
    }

    let command = match s {
        "launcher:toggle" => Command::Launcher(LauncherCommand::Toggle),
        "launcher:show" => Command::Launcher(LauncherCommand::Show {
            query: action.args.clone(),
        }),
        "launcher:hide" => Command::Launcher(LauncherCommand::Hide),
        "launcher:focus" => Command::Launcher(LauncherCommand::Focus),
        "launcher:restore" => Command::Launcher(LauncherCommand::Restore),
        _ if s.starts_with("queryexec:") => Command::Query(QueryCommand::ExecuteFirst {
            query: s[10..].to_string(),
        }),
        _ if s.starts_with("query:") => Command::Query(QueryCommand::Set {
            query: s[6..].to_string(),
            argument: json_string_field(action.args.as_deref(), "query"),
        }),
        "help:show" => Command::Dialog(DialogCommand::Help),
        "timer:dialog:timer" => Command::Timer(TimerCommand::TimerDialog),
        "timer:dialog:alarm" => Command::Timer(TimerCommand::AlarmDialog),
        "shell:dialog" => Command::Shell(ShellCommand::Dialog),
        "note:dialog" => Command::Note(NoteCommand::Dialog),
        "note:graph_dialog" => Command::Note(NoteCommand::GraphDialog {
            args: action.args.clone(),
        }),
        "note:unused_assets" => Command::Note(NoteCommand::UnusedAssets),
        "bookmark:dialog" => Command::Storage(StorageCommand::BookmarkDialog),
        "snippet:dialog" => Command::Storage(StorageCommand::SnippetDialog),
        "macro:dialog" => Command::Macro(MacroCommand::LegacyDialog),
        "mkmacro:dialog" => Command::Macro(MacroCommand::MkDialog),
        "todo:dialog" => Command::Todo(TodoCommand::Dialog),
        "todo:view" => Command::Todo(TodoCommand::View),
        "clipboard:dialog" => Command::Clipboard(ClipboardCommand::Dialog),
        "convert:panel" => Command::Dialog(DialogCommand::Convert),
        "tempfile:dialog" => Command::Storage(StorageCommand::TempfileDialog),
        "settings:dialog" => Command::Dialog(DialogCommand::Settings),
        "dashboard:settings" => Command::Dialog(DialogCommand::DashboardSettings),
        "theme:dialog" => Command::Dialog(DialogCommand::Theme),
        "volume:dialog" => Command::System(SystemCommand::VolumeDialog),
        "brightness:dialog" => Command::System(SystemCommand::BrightnessDialog),
        "calendar:open" => Command::Calendar(CalendarCommand::Open {
            view: "default".into(),
        }),
        "calendar:upcoming" => Command::Calendar(CalendarCommand::Upcoming),
        "note:templates_disabled" => Command::Note(NoteCommand::TemplatesDisabled),
        "note:tags" => Command::Note(NoteCommand::Tags),
        "note:reload" => Command::Note(NoteCommand::Reload),
        "mg:dialog" => Command::MouseGesture(MouseGestureCommand::Dialog),
        "mg:dialog:add" => Command::MouseGesture(MouseGestureCommand::Add),
        "mg:dialog:binding" => Command::MouseGesture(MouseGestureCommand::Binding),
        "mg:dialog:settings" => Command::MouseGesture(MouseGestureCommand::Settings),
        "mg:dialog:focus" => Command::MouseGesture(MouseGestureCommand::Focus {
            args: decode_json_args::<GestureFocusArgs>(action.args.as_deref()),
        }),
        "mg:toggle" => Command::MouseGesture(MouseGestureCommand::Toggle {
            args: decode_json_args::<GestureToggleArgs>(action.args.as_deref()),
        }),
        "mm:open" => Command::MultiManager(MultiManagerCommand::Open),
        "mm:settings" => Command::MultiManager(MultiManagerCommand::Settings),
        "mm:save" => Command::MultiManager(MultiManagerCommand::Save),
        "mm:reload" => Command::MultiManager(MultiManagerCommand::Reload),
        "mm:send-all-home" => Command::MultiManager(MultiManagerCommand::SendAllHome),
        "mm:reconnect" => Command::MultiManager(MultiManagerCommand::Reconnect),
        "mm:save-bindings" => Command::MultiManager(MultiManagerCommand::SaveBindings),
        "mm:restore-bindings" => Command::MultiManager(MultiManagerCommand::RestoreBindings),
        "mm:import" => Command::MultiManager(MultiManagerCommand::Import),
        "mm:recapture-all" => Command::MultiManager(MultiManagerCommand::RecaptureAll),
        "crop:image" => Command::Crop(CropCommand::Image),
        "crop:screenshot" => Command::Crop(CropCommand::Screenshot),
        _ => return parse_prefixed(action),
    };
    Ok(command)
}

fn parse_prefixed(action: &Action) -> Result<Command, CommandError> {
    let s = action.action.as_str();
    macro_rules! tail {
        ($prefix:literal) => {
            s.strip_prefix($prefix).map(str::to_string)
        };
    }

    if let Some(view) = tail!("calendar:open:") {
        return Ok(Command::Calendar(CalendarCommand::Open { view }));
    }
    if let Some(reference) = tail!("calendar:jump:") {
        return Ok(Command::Calendar(CalendarCommand::Jump { reference }));
    }
    if let Some(input) = tail!("calendar:add:") {
        return Ok(Command::Calendar(CalendarCommand::Add { input }));
    }
    if let Some(input) = tail!("calendar:search:") {
        return Ok(Command::Calendar(CalendarCommand::Search { input }));
    }
    if let Some(input) = tail!("calendar:snooze:") {
        return Ok(Command::Calendar(CalendarCommand::Snooze { input }));
    }
    if let Some(slug) = tail!("note:open:") {
        return Ok(Command::Note(NoteCommand::Open { slug }));
    }
    if let Some(encoded) = s.strip_prefix(crate::plugins::note::NOTE_NEW_JSON_PREFIX) {
        let payload = match crate::plugins::note::decode_note_new_payload(encoded) {
            Ok(payload) => payload,
            Err(error) => {
                return Ok(Command::Note(NoteCommand::MalformedNew {
                    raw: s.to_string(),
                    args: action.args.clone(),
                    error: format!("malformed new-note payload: {error}"),
                }));
            }
        };
        if let Err(error) = validate_note_new(&payload) {
            return Ok(Command::Note(NoteCommand::MalformedNew {
                raw: s.to_string(),
                args: action.args.clone(),
                error: error.message,
            }));
        }
        return Ok(Command::Note(NoteCommand::New {
            slug: payload.slug,
            template: payload.template,
        }));
    }
    if let Some(raw) = s.strip_prefix("note:new:") {
        let slug = match urlencoding::decode(raw.trim()) {
            Ok(slug) => slug.into_owned(),
            Err(_) => {
                return Ok(Command::Note(NoteCommand::MalformedNew {
                    raw: s.to_string(),
                    args: action.args.clone(),
                    error: format!("malformed action: {s}"),
                }));
            }
        };
        let payload = NoteNewPayload {
            slug,
            template: json_string_field(action.args.as_deref(), "template"),
        };
        if let Err(error) = validate_note_new(&payload) {
            return Ok(Command::Note(NoteCommand::MalformedNew {
                raw: s.to_string(),
                args: action.args.clone(),
                error: error.message,
            }));
        }
        return Ok(Command::Note(NoteCommand::New {
            slug: payload.slug,
            template: payload.template,
        }));
    }
    if let Some(link) = tail!("note:link:") {
        return Ok(Command::Note(NoteCommand::OpenLink { link }));
    }
    if let Some(slug) = tail!("note:meta:wrap-links:") {
        return Ok(Command::Note(NoteCommand::WrapLinks { slug }));
    }
    if let Some(slug) = tail!("note:remove:") {
        return Ok(Command::Note(NoteCommand::Remove { slug }));
    }
    if let Some(id) = tail!("link:open:") {
        return Ok(Command::Link(LinkCommand::Open { id }));
    }
    if let Some(raw_index) = s.strip_prefix("todo:edit:") {
        return Ok(Command::Todo(match raw_index.parse::<usize>() {
            Ok(index) => TodoCommand::Edit { index },
            Err(_) => TodoCommand::Compatibility {
                kind: TodoCompatibilityKind::Edit,
            },
        }));
    }
    if let Some(cmd) = parse_todo(s) {
        return Ok(Command::Todo(cmd));
    }
    if let Some(label) = tail!("fav:dialog:") {
        return Ok(Command::Storage(StorageCommand::FavoriteDialog(label)));
    }
    if let Some(raw_count) = s.strip_prefix("sysinfo:cpu_list:") {
        return Ok(Command::System(match raw_count.parse::<usize>() {
            Ok(count) => SystemCommand::CpuList(count),
            Err(_) => SystemCommand::InvalidCpuList,
        }));
    }
    if let Some(v) = parse_multi_manager(s) {
        return Ok(Command::MultiManager(v));
    }
    if let Some(v) = parse_screenshot(s) {
        return Ok(Command::Screenshot(v));
    }
    if let Some(v) = parse_shell(s) {
        return Ok(Command::Shell(v));
    }
    if let Some(v) = parse_clipboard(s) {
        return Ok(Command::Clipboard(v));
    }
    if let Some(v) = parse_calculator(action) {
        return Ok(Command::Calculator(v));
    }
    if let Some(v) = parse_storage(s) {
        return Ok(Command::Storage(v));
    }
    if let Some(v) = parse_timer(s) {
        return Ok(Command::Timer(v));
    }
    if let Some(v) = parse_system(s) {
        return Ok(Command::System(v));
    }
    if let Some(v) = parse_tab(s) {
        return Ok(Command::BrowserTab(v));
    }
    if let Some(v) = parse_media(s) {
        return Ok(Command::Media(v));
    }
    if let Some(v) = parse_layout(s) {
        return Ok(Command::Layout(v));
    }
    if let Some(v) = parse_macro(s)? {
        return Ok(Command::Macro(v));
    }
    Ok(Command::External(ExternalCommand {
        target: s.to_string(),
        args: action.args.clone(),
        namespace: s.starts_with("fav:").then_some(ExternalNamespace::Favorite),
    }))
}

fn parse_clipboard_modify(action: &Action) -> Option<ClipboardModifyCommand> {
    let s = action.action.as_str();
    if !s.starts_with("clipboard_modify:") {
        return None;
    }
    if s == "clipboard_modify:error" {
        return Some(ClipboardModifyCommand::Error {
            message: action.desc.clone(),
        });
    }
    if s == "clipboard_modify:open" || s.starts_with("clipboard_modify:open:") {
        let decoded = action.args.as_deref().map(
            crate::clipboard_modify::actions::decode_action_payload::<ClipboardModifyActionPayload>,
        );
        let section = match decoded.and_then(Result::ok) {
            Some(ClipboardModifyActionPayload::OpenDialogSection { section }) => section,
            _ if s.ends_with(":templates") => ClipboardModifySectionPayload::Templates,
            _ if s.ends_with(":saved-pipelines") => ClipboardModifySectionPayload::SavedPipelines,
            _ if s.ends_with(":manage-templates") => ClipboardModifySectionPayload::ManageTemplates,
            _ if s.ends_with(":manage-pipelines") => ClipboardModifySectionPayload::ManagePipelines,
            _ if s.ends_with(":help") => ClipboardModifySectionPayload::Help,
            _ => ClipboardModifySectionPayload::Modify,
        };
        return Some(ClipboardModifyCommand::Open { section });
    }
    if s == "clipboard_modify:undo" || s.starts_with("clipboard_modify:undo:") {
        return Some(ClipboardModifyCommand::Undo {
            raw_argument: action
                .args
                .clone()
                .or_else(|| s.strip_prefix("clipboard_modify:undo:").map(str::to_string)),
        });
    }
    if s == "clipboard_modify:execute" || s.starts_with("clipboard_modify:execute:") {
        let raw_argument = action.args.clone().or_else(|| {
            s.strip_prefix("clipboard_modify:execute:")
                .map(str::to_string)
        });
        let decoded = crate::clipboard_modify::runtime::decode_execute_payload_for_gui(
            raw_argument.as_deref().unwrap_or(""),
        );
        let (payload, payload_error) = match decoded {
            Ok(payload) => (Some(payload), None),
            Err(error) => (None, Some(error)),
        };
        return Some(ClipboardModifyCommand::Execute {
            payload,
            raw_argument,
            payload_error,
        });
    }
    None
}

fn parse_file_search(s: &str) -> Option<FileSearchCommand> {
    use crate::file_search::actions::{
        CANCEL_ACTION, MODE_PREFIX, OPEN_ACTION, START_PREFIX, decode_action_payload,
    };
    let command = if s == OPEN_ACTION {
        Some(FileSearchCommand::Open)
    } else if s == CANCEL_ACTION {
        Some(FileSearchCommand::Cancel)
    } else if let Some(encoded) = s.strip_prefix(MODE_PREFIX) {
        let payload = match decode_action_payload::<FileSearchModePayload>(encoded).and_then(|p| {
            p.validate()?;
            Ok(p)
        }) {
            Ok(payload) => FileSearchCommand::SetMode(payload),
            Err(error) => FileSearchCommand::Invalid {
                raw: s.to_string(),
                error,
            },
        };
        Some(payload)
    } else if let Some(encoded) = s.strip_prefix(START_PREFIX) {
        let payload = match decode_action_payload::<FileSearchStartPayload>(encoded).and_then(|p| {
            p.validate()?;
            Ok(p)
        }) {
            Ok(payload) => FileSearchCommand::Start(payload),
            Err(error) => FileSearchCommand::Invalid {
                raw: s.to_string(),
                error,
            },
        };
        Some(payload)
    } else {
        None
    };
    command
}

fn parse_todo(s: &str) -> Option<TodoCommand> {
    if let Some(rest) = s.strip_prefix("todo:add:") {
        if let Some(p) = crate::plugins::todo::decode_todo_add_action_payload(rest) {
            return Some(TodoCommand::Add {
                text: p.text,
                priority: p.priority,
                tags: p.tags,
                refs: p.refs,
                toast_text: rest.to_string(),
            });
        }
        let mut parts = rest.splitn(3, '|');
        if let (Some(text), Some(priority), Some(tags)) = (parts.next(), parts.next(), parts.next())
            && let Ok(priority) = priority.parse::<u8>()
        {
            return Some(TodoCommand::Add {
                text: text.into(),
                priority,
                tags: tags
                    .split(',')
                    .map(str::trim)
                    .filter(|v| !v.is_empty())
                    .map(str::to_string)
                    .collect(),
                refs: Vec::new(),
                toast_text: text.to_string(),
            });
        }
        return Some(TodoCommand::Compatibility {
            kind: TodoCompatibilityKind::Add {
                toast_text: rest.split('|').next().unwrap_or_default().to_string(),
            },
        });
    }
    if let Some(rest) = s.strip_prefix("todo:pset:") {
        if let Some((idx, p)) = rest.split_once('|')
            && let (Ok(index), Ok(priority)) = (idx.parse(), p.parse())
        {
            return Some(TodoCommand::SetPriority { index, priority });
        }
        return Some(TodoCommand::Compatibility {
            kind: TodoCompatibilityKind::SetPriority,
        });
    }
    if let Some(rest) = s.strip_prefix("todo:tag:") {
        if let Some(p) = crate::plugins::todo::decode_todo_tag_action_payload(rest) {
            return Some(TodoCommand::SetTags {
                index: p.idx,
                tags: p.tags,
            });
        }
        return Some(TodoCommand::Compatibility {
            kind: TodoCompatibilityKind::SetTags,
        });
    }
    if let Some(raw) = s.strip_prefix("todo:remove:") {
        return Some(match raw.parse() {
            Ok(index) => TodoCommand::Remove { index },
            Err(_) => TodoCommand::Compatibility {
                kind: TodoCompatibilityKind::Remove,
            },
        });
    }
    if let Some(raw) = s.strip_prefix("todo:done:") {
        return Some(match raw.parse() {
            Ok(index) => TodoCommand::Done { index },
            Err(_) => TodoCommand::Compatibility {
                kind: TodoCompatibilityKind::Done,
            },
        });
    }
    match s {
        "todo:clear" => Some(TodoCommand::Clear),
        "todo:export" => Some(TodoCommand::Export),
        _ => None,
    }
}

fn parse_multi_manager(s: &str) -> Option<MultiManagerCommand> {
    for (prefix, ctor) in [
        ("mm:toggle:", MultiManagerCommand::Toggle as fn(String) -> _),
        ("mm:home:", MultiManagerCommand::Home),
        ("mm:target:", MultiManagerCommand::Target),
        ("mm:capture:", MultiManagerCommand::Capture),
        ("mm:disable:", MultiManagerCommand::Disable),
        ("mm:enable:", MultiManagerCommand::Enable),
    ] {
        if let Some(v) = s.strip_prefix(prefix) {
            return Some(ctor(v.into()));
        }
    }
    None
}

fn parse_screenshot(s: &str) -> Option<ScreenshotCommand> {
    let raw = s.strip_prefix("screenshot:")?;
    use ScreenshotDestination as D;
    use ScreenshotMarkup as K;
    use ScreenshotMode as M;
    let known = match raw {
        "window" => (M::Window, D::Editor, K::Rectangle),
        "region" => (M::Region, D::Editor, K::Rectangle),
        "region_markup" => (M::Region, D::Editor, K::Pen),
        "desktop" => (M::Desktop, D::Editor, K::Rectangle),
        "window_clip" => (M::Window, D::Clipboard, K::Rectangle),
        "region_clip" => (M::Region, D::Clipboard, K::Rectangle),
        "desktop_clip" => (M::Desktop, D::Clipboard, K::Rectangle),
        _ => {
            return Some(ScreenshotCommand::UnknownMode { raw: raw.into() });
        }
    };
    Some(ScreenshotCommand::Capture {
        mode: known.0,
        destination: known.1,
        markup: known.2,
        compatibility: if raw == "region_markup" {
            ScreenshotCompatibility::GuiOnly
        } else {
            ScreenshotCompatibility::Shared
        },
    })
}

fn parse_shell(s: &str) -> Option<ShellCommand> {
    if let Some(rest) = s.strip_prefix("shell:add:")
        && let Some((name, args)) = rest.split_once('|')
    {
        return Some(ShellCommand::Add {
            name: name.into(),
            args: args.into(),
        });
    }
    if let Some(name) = s.strip_prefix("shell:remove:") {
        return Some(ShellCommand::Remove { name: name.into() });
    }
    if let Some(command) = s.strip_prefix("shell_keep:") {
        return Some(ShellCommand::Run {
            command: command.into(),
            keep_open: true,
        });
    }
    s.strip_prefix("shell:").map(|command| ShellCommand::Run {
        command: command.into(),
        keep_open: false,
    })
}

fn parse_clipboard(s: &str) -> Option<ClipboardCommand> {
    let rest = s.strip_prefix("clipboard:")?;
    if rest == "clear" {
        Some(ClipboardCommand::Clear)
    } else if let Some(index) = rest.strip_prefix("copy:").and_then(|v| v.parse().ok()) {
        Some(ClipboardCommand::Copy { index })
    } else {
        Some(ClipboardCommand::SetText { text: rest.into() })
    }
}

fn parse_calculator(a: &Action) -> Option<CalculatorCommand> {
    if let Some(index) = a
        .action
        .strip_prefix("calc:history:")
        .and_then(|v| v.parse().ok())
    {
        return Some(CalculatorCommand::CopyHistory { index });
    }
    a.action
        .strip_prefix("calc:")
        .map(|result| CalculatorCommand::CopyResult {
            result: result.into(),
            expression: a.args.clone(),
        })
}

fn parse_storage(s: &str) -> Option<StorageCommand> {
    macro_rules! map {
        ($p:literal, $v:path) => {
            if let Some(x) = s.strip_prefix($p) {
                return Some($v(x.into()));
            }
        };
    }
    map!("bookmark:add:", StorageCommand::BookmarkAdd);
    map!("bookmark:remove:", StorageCommand::BookmarkRemove);
    map!("folder:add:", StorageCommand::FolderAdd);
    map!("folder:remove:", StorageCommand::FolderRemove);
    map!("snippet:remove:", StorageCommand::SnippetRemove);
    map!("snippet:edit:", StorageCommand::SnippetEdit);
    map!("fav:remove:", StorageCommand::FavoriteRemove);
    map!("tempfile:remove:", StorageCommand::TempfileRemove);
    if s == "history:clear" {
        return Some(StorageCommand::HistoryClear);
    }
    if let Some(i) = s.strip_prefix("history:").and_then(|v| v.parse().ok()) {
        return Some(StorageCommand::HistoryLaunch(i));
    }
    if let Some(rest) = s.strip_prefix("snippet:add:")
        && let Some((alias, text)) = rest.split_once('|')
    {
        return Some(StorageCommand::SnippetAdd {
            alias: alias.into(),
            text: text.into(),
        });
    }
    if let Some(rest) = s.strip_prefix("fav:add:") {
        let mut p = rest.splitn(3, '|');
        return Some(StorageCommand::FavoriteAdd {
            label: p.next().unwrap_or("").into(),
            command: p.next().unwrap_or("").into(),
            args: p.next().map(str::to_string),
        });
    }
    if let Some(alias) = s.strip_prefix("tempfile:new:") {
        return Some(StorageCommand::TempfileNew(Some(alias.into())));
    }
    if s == "tempfile:new" {
        return Some(StorageCommand::TempfileNew(None));
    }
    if s == "tempfile:open" {
        return Some(StorageCommand::TempfileOpen);
    }
    if let Some(path) = s.strip_prefix("tempfile:open:") {
        return Some(StorageCommand::TempfileOpenFile(path.into()));
    }
    if s == "tempfile:clear" {
        return Some(StorageCommand::TempfileClear);
    }
    if let Some(rest) = s.strip_prefix("tempfile:alias:") {
        return Some(match rest.split_once('|') {
            Some((path, alias)) => StorageCommand::TempfileAlias {
                path: path.into(),
                alias: alias.into(),
            },
            None => StorageCommand::InvalidTempfileAlias,
        });
    }
    (s == "recycle:clean").then_some(StorageCommand::RecycleClean)
}

fn parse_timer(s: &str) -> Option<TimerCommand> {
    macro_rules! id {
        ($p:literal,$v:path) => {
            if let Some(x) = s.strip_prefix($p).and_then(|v| v.parse().ok()) {
                return Some($v(x));
            }
        };
    }
    for (prefix, invalid) in [
        ("timer:cancel:", TimerCommand::InvalidCancel),
        ("timer:pause:", TimerCommand::InvalidPause),
        ("timer:resume:", TimerCommand::InvalidResume),
    ] {
        if let Some(raw) = s.strip_prefix(prefix) {
            return Some(match raw.parse() {
                Ok(id) => match invalid {
                    TimerCommand::InvalidCancel => TimerCommand::Cancel(id),
                    TimerCommand::InvalidPause => TimerCommand::Pause(id),
                    TimerCommand::InvalidResume => TimerCommand::Resume(id),
                    _ => unreachable!(),
                },
                Err(_) => invalid,
            });
        }
    }
    id!("stopwatch:pause:", TimerCommand::StopwatchPause);
    id!("stopwatch:resume:", TimerCommand::StopwatchResume);
    id!("stopwatch:stop:", TimerCommand::StopwatchStop);
    id!("stopwatch:show:", TimerCommand::StopwatchShow);
    if let Some(rest) = s.strip_prefix("timer:start:") {
        let (duration, name) = rest.split_once('|').unwrap_or((rest, ""));
        return Some(TimerCommand::Start {
            duration: duration.into(),
            name: name.into(),
        });
    }
    if let Some(rest) = s.strip_prefix("alarm:set:") {
        let (time, name) = rest.split_once('|').unwrap_or((rest, ""));
        return Some(TimerCommand::AlarmSet {
            time: time.into(),
            name: name.into(),
        });
    }
    s.strip_prefix("stopwatch:start:")
        .map(|v| TimerCommand::StopwatchStart(v.into()))
}

fn parse_system(s: &str) -> Option<SystemCommand> {
    if let Some(v) = s.strip_prefix("system:") {
        return Some(match v {
            "shutdown" => SystemCommand::Shutdown,
            "reboot" => SystemCommand::Reboot,
            "lock" => SystemCommand::Lock,
            "logoff" => SystemCommand::Logoff,
            _ => SystemCommand::Unknown(v.into()),
        });
    }
    macro_rules! n {
        ($p:literal,$t:ty,$v:path) => {
            if let Some(x) = s.strip_prefix($p).and_then(|v| v.parse::<$t>().ok()) {
                return Some($v(x));
            }
        };
    }
    n!("process:kill:", u32, SystemCommand::ProcessKill);
    n!("process:switch:", u32, SystemCommand::ProcessSwitch);
    n!("window:switch:", isize, SystemCommand::WindowSwitch);
    n!("window:close:", isize, SystemCommand::WindowClose);
    n!("brightness:set:", u32, SystemCommand::Brightness);
    n!("volume:set:", u32, SystemCommand::Volume);
    n!(
        "volume:pid_toggle_mute:",
        u32,
        SystemCommand::ProcessToggleMute
    );
    if let Some(rest) = s.strip_prefix("volume:pid:")
        && let Some((pid, level)) = rest.split_once(':')
        && let (Ok(pid), Ok(level)) = (pid.parse(), level.parse())
    {
        return Some(SystemCommand::ProcessVolume { pid, level });
    }
    if s == "volume:mute_active" {
        return Some(SystemCommand::MuteActive);
    }
    if s == "volume:toggle_mute" {
        return Some(SystemCommand::ToggleMute);
    }
    if let Some(v) = s.strip_prefix("power:plan:set:") {
        return Some(SystemCommand::PowerPlan(v.into()));
    }
    s.strip_prefix("keys:")
        .map(|v| SystemCommand::Keys(v.into()))
}

fn parse_tab(s: &str) -> Option<BrowserTabCommand> {
    if let Some(rest) = s.strip_prefix("tab:switch:") {
        let ids = rest
            .split('_')
            .filter_map(|v| v.parse().ok())
            .collect::<Vec<_>>();
        return Some(if ids.is_empty() {
            BrowserTabCommand::InvalidSwitch
        } else {
            BrowserTabCommand::Switch(ids)
        });
    }
    match s {
        "tab:cache" => Some(BrowserTabCommand::Cache),
        "tab:clear" => Some(BrowserTabCommand::Clear),
        _ => None,
    }
}
fn parse_media(s: &str) -> Option<MediaCommand> {
    match s {
        "media:play" => Some(MediaCommand::Play),
        "media:pause" => Some(MediaCommand::Pause),
        "media:next" => Some(MediaCommand::Next),
        "media:prev" => Some(MediaCommand::Previous),
        _ => None,
    }
}
fn parse_layout(s: &str) -> Option<LayoutCommand> {
    fn fields(rest: &str) -> (String, Option<String>) {
        let (n, f) = rest.split_once('|').unwrap_or((rest, ""));
        (n.into(), (!f.is_empty()).then(|| f.into()))
    }
    for (p, c) in [
        ("layout:save:", 0),
        ("layout:load:", 1),
        ("layout:show:", 2),
        ("layout:rm:", 3),
    ] {
        if let Some(r) = s.strip_prefix(p) {
            let (n, flags) = fields(r);
            return Some(match c {
                0 => LayoutCommand::Save { name: n, flags },
                1 => LayoutCommand::Load { name: n, flags },
                2 => LayoutCommand::Show { name: n, flags },
                _ => LayoutCommand::Remove { name: n, flags },
            });
        }
    }
    if let Some(rest) = s.strip_prefix("layout:list") {
        if rest.is_empty() {
            return Some(LayoutCommand::List { flags: None });
        }
        if let Some(f) = rest.strip_prefix('|') {
            return Some(LayoutCommand::List {
                flags: (!f.is_empty()).then(|| f.into()),
            });
        }
    }
    (s == "layout:edit").then_some(LayoutCommand::Edit)
}
fn parse_macro(s: &str) -> Result<Option<MacroCommand>, CommandError> {
    let v = match s {
        "mkmacro:pause" => Some(MacroCommand::MkPause),
        "mkmacro:resume" => Some(MacroCommand::MkResume),
        "mkmacro:stop" => Some(MacroCommand::MkStop),
        "mkmacro:record" => Some(MacroCommand::MkRecord),
        "mkmacro:record-stop" => Some(MacroCommand::MkRecordStop),
        _ => None,
    };
    if v.is_some() {
        return Ok(v);
    }
    if let Some(raw) = s.strip_prefix("mkmacro:run:") {
        return Ok(Some(match raw.parse::<u64>() {
            Ok(id) if id != 0 => MacroCommand::MkRun(id),
            _ => MacroCommand::Invalid { raw: s.into() },
        }));
    }
    if s.starts_with("mkmacro:") {
        return Ok(Some(MacroCommand::Invalid { raw: s.into() }));
    }
    Ok(s.strip_prefix("macro:")
        .map(|v| MacroCommand::RunLegacy(v.into())))
}

fn decode_json_args<T: serde::de::DeserializeOwned>(raw: Option<&str>) -> Option<T> {
    raw.and_then(|v| serde_json::from_str(v).ok())
}
fn json_string_field(raw: Option<&str>, field: &str) -> Option<String> {
    raw.and_then(|v| serde_json::from_str::<serde_json::Value>(v).ok())
        .and_then(|v| v.get(field).and_then(|v| v.as_str()).map(str::to_string))
}
fn validate_note_new(p: &NoteNewPayload) -> Result<(), CommandError> {
    if p.slug.is_empty()
        || p.slug.chars().any(char::is_whitespace)
        || p.slug.contains(':')
        || p.template.as_deref().is_some_and(|v| v.trim().is_empty())
    {
        Err(CommandError::new("note", "malformed note action"))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clipboard_modify::actions::{
        encode_action_payload as encode_cm, execute_template_payload,
    };
    use crate::file_search::actions::{
        encode_action_payload as encode_fs, mode_action_payload, start_action_payload,
    };
    use crate::file_search::model::SearchKind;

    fn action(value: &str) -> Action {
        Action {
            label: "label".into(),
            desc: "desc".into(),
            action: value.into(),
            args: None,
        }
    }
    fn parse(value: &str) -> CommandInvocation {
        parse_command(action(value), None, ActivationSource::Enter).unwrap()
    }
    fn parse_with_args(value: &str, args: &str) -> CommandInvocation {
        let mut action = action(value);
        action.args = Some(args.into());
        parse_command(action, None, ActivationSource::Click).unwrap()
    }

    #[test]
    fn metadata_is_stable_across_every_command_domain() {
        let cases = [
            ("launcher:toggle", "launcher", "toggle"),
            ("query:abc", "query", "set"),
            ("help:show", "dialog", "help"),
            ("calendar:open", "calendar", "open"),
            ("note:open:a", "note", "open"),
            ("link:open:note:a", "link", "open"),
            ("todo:done:1", "todo", "done"),
            ("mg:dialog", "mouse_gesture", "dialog"),
            ("mm:open", "multi_manager", "open"),
            ("file_search:open", "file_search", "open"),
            ("clipboard_modify:open", "clipboard_modify", "open"),
            ("screenshot:window", "screenshot", "capture"),
            ("shell:dir", "shell", "run"),
            ("clipboard:text", "clipboard", "set_text"),
            ("calc:4", "calculator", "copy_result"),
            ("history:clear", "storage", "history_clear"),
            ("timer:cancel:1", "timer", "cancel"),
            ("system:shutdown", "system", "shutdown"),
            ("tab:cache", "browser_tab", "cache"),
            ("media:play", "media", "play"),
            ("layout:edit", "layout", "edit"),
            ("macro:name", "macro", "run_legacy"),
            ("crop:image", "crop", "image"),
            ("https://example.com", "external", "launch"),
        ];
        for (raw, domain, kind) in cases {
            let parsed = parse(raw);
            assert_eq!(
                (parsed.domain(), parsed.kind_name()),
                (domain, kind),
                "{raw}"
            );
        }
        let diff = crate::diff::query::encode_payload(&crate::diff::query::DiffOpenPayload {
            left: None,
            right: None,
        })
        .unwrap();
        let parsed = parse(&format!("diff:open:{diff}"));
        assert_eq!((parsed.domain(), parsed.kind_name()), ("diff", "open"));
    }

    #[test]
    fn invocation_retains_original_action_override_and_source() {
        let original = Action {
            label: "L".into(),
            desc: "D".into(),
            action: "file_search:open".into(),
            args: Some("A".into()),
        };
        let got = parse_command(
            original.clone(),
            Some("override".into()),
            ActivationSource::Dashboard,
        )
        .unwrap();
        assert_eq!(got.original_action, original);
        assert_eq!(got.query_override.as_deref(), Some("override"));
        assert_eq!(got.source, ActivationSource::Dashboard);
        assert!(matches!(
            got.command,
            Command::FileSearch(FileSearchCommand::Open)
        ));
    }

    #[test]
    fn parse_action_is_a_direct_side_effect_free_boundary() {
        let original = action("launcher:hide");
        assert_eq!(
            parse_action(&original).unwrap(),
            Command::Launcher(LauncherCommand::Hide)
        );
        assert_eq!(original.action, "launcher:hide");
    }

    #[test]
    fn action_json_shape_and_round_trip_are_unchanged() {
        let original = Action {
            label: "L".into(),
            desc: "D".into(),
            action: "app.exe".into(),
            args: Some("--x".into()),
        };
        let json = serde_json::to_value(&original).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"label":"L","desc":"D","action":"app.exe","args":"--x"})
        );
        assert_eq!(serde_json::from_value::<Action>(json).unwrap(), original);
        let no_args = serde_json::to_value(action("noop")).unwrap();
        assert!(no_args.get("args").is_none());
    }

    #[test]
    fn preserves_delimiters_and_action_args() {
        assert!(
            matches!(parse("shell:add:name|a|b").command,Command::Shell(ShellCommand::Add{name,args}) if name=="name"&&args=="a|b")
        );
        assert!(
            matches!(parse("timer:start:5m|tea|later").command,Command::Timer(TimerCommand::Start{duration,name}) if duration=="5m"&&name=="tea|later")
        );
        assert!(
            matches!(parse("fav:add:L|cmd|--a|--b").command,Command::Storage(StorageCommand::FavoriteAdd{label,command,args}) if label=="L"&&command=="cmd"&&args.as_deref()==Some("--a|--b"))
        );
        assert!(
            matches!(parse_with_args("calc:42","1+41").command,Command::Calculator(CalculatorCommand::CopyResult{result,expression}) if result=="42"&&expression.as_deref()==Some("1+41"))
        );
        assert!(
            matches!(parse_with_args("C:\\app.exe","--flag").command,Command::External(ExternalCommand{target,args,namespace: None}) if target=="C:\\app.exe"&&args.as_deref()==Some("--flag"))
        );
    }

    #[test]
    fn external_fallback_retains_parser_classified_favorite_namespace() {
        assert!(matches!(
            parse("fav:future:payload").command,
            Command::External(ExternalCommand {
                namespace: Some(ExternalNamespace::Favorite),
                ..
            })
        ));
        assert!(matches!(
            parse("plain-tool").command,
            Command::External(ExternalCommand {
                namespace: None,
                ..
            })
        ));
        assert!(matches!(
            parse("fav:remove:7").command,
            Command::Storage(StorageCommand::FavoriteRemove(_))
        ));
    }

    #[test]
    fn query_args_are_typed_but_invalid_json_keeps_plain_query() {
        assert!(
            matches!(parse_with_args("query:note links",r#"{"query":"alpha"}"#).command,Command::Query(QueryCommand::Set{query,argument}) if query=="note links"&&argument.as_deref()==Some("alpha"))
        );
        assert!(matches!(
            parse_with_args("query:q", "bad").command,
            Command::Query(QueryCommand::Set { argument: None, .. })
        ));
        assert!(
            matches!(parse("queryexec:q").command,Command::Query(QueryCommand::ExecuteFirst{query}) if query=="q")
        );
    }

    #[test]
    fn todo_supports_encoded_legacy_and_typed_malformed_payloads() {
        let encoded = crate::plugins::todo::encode_todo_add_action_payload(
            &crate::plugins::todo::TodoAddActionPayload {
                text: "a|b".into(),
                priority: 2,
                tags: vec!["x,y".into()],
                refs: vec![],
            },
        )
        .unwrap();
        assert!(
            matches!(parse(&format!("todo:add:{encoded}")).command,Command::Todo(TodoCommand::Add{text,tags,toast_text,..}) if text=="a|b"&&tags==["x,y"]&&toast_text==encoded)
        );
        assert!(
            matches!(parse("todo:add:legacy|3| a, b ,, ").command,Command::Todo(TodoCommand::Add{text,priority,tags,toast_text,..}) if text=="legacy"&&priority==3&&tags==["a","b"]&&toast_text=="legacy")
        );
        assert_eq!(
            parse("todo:add:broken").command,
            Command::Todo(TodoCommand::Compatibility {
                kind: TodoCompatibilityKind::Add {
                    toast_text: "broken".into(),
                },
            })
        );
        let tag = crate::plugins::todo::encode_todo_tag_action_payload(
            &crate::plugins::todo::TodoTagActionPayload {
                idx: 7,
                tags: vec!["one,two".into()],
            },
        )
        .unwrap();
        assert!(
            matches!(parse(&format!("todo:tag:{tag}")).command,Command::Todo(TodoCommand::SetTags{index:7,tags}) if tags==["one,two"])
        );
    }

    #[test]
    fn note_supports_encoded_and_legacy_forms_and_rejects_malformed_claims() {
        let encoded = crate::plugins::note::encode_note_new_payload(&NoteNewPayload {
            slug: "alpha".into(),
            template: Some("daily".into()),
        })
        .unwrap();
        assert!(
            matches!(parse(&encoded).command,Command::Note(NoteCommand::New{slug,template}) if slug=="alpha"&&template.as_deref()==Some("daily"))
        );
        assert!(
            matches!(parse_with_args("note:new:hello%2Dworld",r#"{"template":"weekly"}"#).command,Command::Note(NoteCommand::New{slug,template}) if slug=="hello-world"&&template.as_deref()==Some("weekly"))
        );
        assert!(
            matches!(parse("note:new:bad slug").command, Command::Note(NoteCommand::MalformedNew { raw, .. }) if raw == "note:new:bad slug")
        );
        assert!(
            matches!(parse("note:new-json:not-base64").command, Command::Note(NoteCommand::MalformedNew { error, .. }) if error.contains("malformed new-note payload"))
        );
        assert!(matches!(
            parse("note:template:create").command,
            Command::External(_)
        ));
    }

    #[test]
    fn structured_file_search_and_diff_payloads_decode_or_error() {
        let mode = encode_fs(&mode_action_payload(SearchKind::Content)).unwrap();
        assert!(matches!(
            parse(&format!("file_search:mode:{mode}")).command,
            Command::FileSearch(FileSearchCommand::SetMode(_))
        ));
        let start = encode_fs(&start_action_payload(
            SearchKind::Filename,
            Some("C:/tmp".into()),
            "needle".into(),
        ))
        .unwrap();
        assert!(
            matches!(parse(&format!("file_search:start:{start}")).command,Command::FileSearch(FileSearchCommand::Start(FileSearchStartPayload{text,..})) if text=="needle")
        );
        let invalid = parse_command(
            action("file_search:start:bad"),
            Some("kept".into()),
            ActivationSource::Enter,
        )
        .unwrap();
        assert_eq!(invalid.query_override.as_deref(), Some("kept"));
        assert!(
            matches!(invalid.command, Command::FileSearch(FileSearchCommand::Invalid { raw, .. }) if raw == "file_search:start:bad")
        );
        assert!(
            matches!(parse("diff:open:bad").command, Command::Diff(DiffCommand::Invalid { raw, .. }) if raw == "diff:open:bad")
        );
    }

    #[test]
    fn clipboard_modify_preserves_gui_and_headless_compatibility_metadata() {
        let payload = execute_template_payload("email".into());
        let encoded = encode_cm(&payload).unwrap();
        assert!(
            matches!(parse_with_args("clipboard_modify:execute",&encoded).command,Command::ClipboardModify(ClipboardModifyCommand::Execute{payload:Some(ClipboardModifyActionPayload::ExecuteTemplate{name,..}),raw_argument:Some(raw),payload_error:None}) if name=="email"&&raw==encoded)
        );
        assert!(
            matches!(parse("clipboard_modify:execute:cm upper").command,Command::ClipboardModify(ClipboardModifyCommand::Execute{payload:None,raw_argument:Some(raw),payload_error:Some(_)}) if raw=="cm upper")
        );
        assert!(matches!(
            parse_with_args("clipboard_modify:execute", "bad").command,
            Command::ClipboardModify(ClipboardModifyCommand::Execute {
                payload: None,
                payload_error: Some(_),
                raw_argument: Some(_)
            })
        ));
        assert!(matches!(
            parse("clipboard_modify:open:templates").command,
            Command::ClipboardModify(ClipboardModifyCommand::Open {
                section: ClipboardModifySectionPayload::Templates
            })
        ));
    }

    #[test]
    fn mouse_gesture_malformed_args_preserve_historical_fallbacks() {
        assert!(matches!(
            parse_with_args("mg:dialog:focus", "bad").command,
            Command::MouseGesture(MouseGestureCommand::Focus { args: None })
        ));
        assert!(matches!(
            parse_with_args("mg:toggle", "bad").command,
            Command::MouseGesture(MouseGestureCommand::Toggle { args: None })
        ));
    }

    #[test]
    fn screenshot_represents_gui_headless_unknown_mode_difference() {
        assert!(matches!(
            parse("screenshot:region_markup").command,
            Command::Screenshot(ScreenshotCommand::Capture {
                mode: ScreenshotMode::Region,
                destination: ScreenshotDestination::Editor,
                markup: ScreenshotMarkup::Pen,
                compatibility: ScreenshotCompatibility::GuiOnly,
            })
        ));
        assert!(matches!(
            parse("screenshot:window").command,
            Command::Screenshot(ScreenshotCommand::Capture {
                compatibility: ScreenshotCompatibility::Shared,
                ..
            })
        ));
        assert!(
            matches!(parse("screenshot:future").command,Command::Screenshot(ScreenshotCommand::UnknownMode{raw}) if raw=="future")
        );
    }

    #[test]
    fn malformed_legacy_protocols_keep_typed_compatibility_provenance() {
        assert!(matches!(
            parse("mkmacro:run:9").command,
            Command::Macro(MacroCommand::MkRun(9))
        ));
        for raw in ["mkmacro:run:0", "mkmacro:run:nope", "mkmacro:future"] {
            assert_eq!(
                parse(raw).command,
                Command::Macro(MacroCommand::Invalid { raw: raw.into() })
            );
        }
        for (raw, kind) in [
            (
                "todo:add:bad",
                TodoCompatibilityKind::Add {
                    toast_text: "bad".into(),
                },
            ),
            ("todo:pset:bad", TodoCompatibilityKind::SetPriority),
            ("todo:tag:bad", TodoCompatibilityKind::SetTags),
            ("todo:remove:bad", TodoCompatibilityKind::Remove),
            ("todo:done:bad", TodoCompatibilityKind::Done),
            ("todo:edit:bad", TodoCompatibilityKind::Edit),
        ] {
            assert_eq!(
                parse(raw).command,
                Command::Todo(TodoCommand::Compatibility { kind }),
                "{raw}"
            );
        }
        assert_eq!(
            parse("timer:cancel:nope").command,
            Command::Timer(TimerCommand::InvalidCancel)
        );
        assert_eq!(
            parse("timer:pause:nope").command,
            Command::Timer(TimerCommand::InvalidPause)
        );
        assert_eq!(
            parse("timer:resume:nope").command,
            Command::Timer(TimerCommand::InvalidResume)
        );
        assert_eq!(
            parse("tab:switch:nope").command,
            Command::BrowserTab(BrowserTabCommand::InvalidSwitch)
        );
        assert_eq!(
            parse("sysinfo:cpu_list:nope").command,
            Command::System(SystemCommand::InvalidCpuList)
        );
        assert_eq!(
            parse("tempfile:alias:missing-delimiter").command,
            Command::Storage(StorageCommand::InvalidTempfileAlias)
        );
    }

    #[test]
    fn numeric_and_delimited_protocols_match_legacy_partial_parse_rules() {
        assert!(
            matches!(parse("tab:switch:1_bad_3").command,Command::BrowserTab(BrowserTabCommand::Switch(ids)) if ids==[1,3])
        );
        assert!(
            matches!(parse("clipboard:copy:nope").command,Command::Clipboard(ClipboardCommand::SetText{text}) if text=="copy:nope")
        );
        assert!(
            matches!(parse("layout:list|one,two").command,Command::Layout(LayoutCommand::List{flags:Some(v)}) if v=="one,two")
        );
        assert!(
            matches!(parse("shell:add:missing-delimiter").command,Command::Shell(ShellCommand::Run{command,..}) if command=="add:missing-delimiter")
        );
    }

    #[test]
    fn exact_launcher_calendar_note_and_todo_variants_are_owned() {
        for (raw, expected) in [
            ("launcher:toggle", LauncherCommand::Toggle),
            ("launcher:hide", LauncherCommand::Hide),
            ("launcher:focus", LauncherCommand::Focus),
            ("launcher:restore", LauncherCommand::Restore),
        ] {
            assert_eq!(parse(raw).command, Command::Launcher(expected));
        }
        assert_eq!(
            parse_with_args("launcher:show", "todo list").command,
            Command::Launcher(LauncherCommand::Show {
                query: Some("todo list".into())
            })
        );
        for (raw, expected) in [
            (
                "calendar:open:week",
                CalendarCommand::Open {
                    view: "week".into(),
                },
            ),
            (
                "calendar:jump:tomorrow",
                CalendarCommand::Jump {
                    reference: "tomorrow".into(),
                },
            ),
            (
                "calendar:add:tea tomorrow",
                CalendarCommand::Add {
                    input: "tea tomorrow".into(),
                },
            ),
            (
                "calendar:search:project x",
                CalendarCommand::Search {
                    input: "project x".into(),
                },
            ),
            (
                "calendar:snooze:10m event-1",
                CalendarCommand::Snooze {
                    input: "10m event-1".into(),
                },
            ),
        ] {
            assert_eq!(parse(raw).command, Command::Calendar(expected));
        }
        assert_eq!(
            parse("note:dialog").command,
            Command::Note(NoteCommand::Dialog)
        );
        assert_eq!(
            parse("note:unused_assets").command,
            Command::Note(NoteCommand::UnusedAssets)
        );
        assert_eq!(parse("note:tags").command, Command::Note(NoteCommand::Tags));
        assert_eq!(
            parse("note:link:https://x").command,
            Command::Note(NoteCommand::OpenLink {
                link: "https://x".into()
            })
        );
        assert_eq!(
            parse("note:meta:wrap-links:a").command,
            Command::Note(NoteCommand::WrapLinks { slug: "a".into() })
        );
        assert_eq!(
            parse("note:remove:a").command,
            Command::Note(NoteCommand::Remove { slug: "a".into() })
        );
        assert_eq!(
            parse("todo:dialog").command,
            Command::Todo(TodoCommand::Dialog)
        );
        assert_eq!(parse("todo:view").command, Command::Todo(TodoCommand::View));
        assert_eq!(
            parse("todo:edit:8").command,
            Command::Todo(TodoCommand::Edit { index: 8 })
        );
        assert_eq!(
            parse("todo:pset:8|2").command,
            Command::Todo(TodoCommand::SetPriority {
                index: 8,
                priority: 2
            })
        );
        assert_eq!(
            parse("todo:remove:8").command,
            Command::Todo(TodoCommand::Remove { index: 8 })
        );
        assert_eq!(
            parse("todo:clear").command,
            Command::Todo(TodoCommand::Clear)
        );
        assert_eq!(
            parse("todo:export").command,
            Command::Todo(TodoCommand::Export)
        );
    }

    #[test]
    fn exact_manager_storage_timer_system_and_layout_variants_are_owned() {
        for (raw, expected) in [
            ("mm:toggle:a", MultiManagerCommand::Toggle("a".into())),
            ("mm:home:a", MultiManagerCommand::Home("a".into())),
            ("mm:target:a", MultiManagerCommand::Target("a".into())),
            ("mm:capture:a", MultiManagerCommand::Capture("a".into())),
            ("mm:disable:a", MultiManagerCommand::Disable("a".into())),
            ("mm:enable:a", MultiManagerCommand::Enable("a".into())),
        ] {
            assert_eq!(parse(raw).command, Command::MultiManager(expected));
        }
        assert_eq!(
            parse("bookmark:add:https://x").command,
            Command::Storage(StorageCommand::BookmarkAdd("https://x".into()))
        );
        assert_eq!(
            parse("history:4").command,
            Command::Storage(StorageCommand::HistoryLaunch(4))
        );
        assert_eq!(
            parse("snippet:add:a|b|c").command,
            Command::Storage(StorageCommand::SnippetAdd {
                alias: "a".into(),
                text: "b|c".into()
            })
        );
        assert_eq!(
            parse("tempfile:alias:C:/x|alias").command,
            Command::Storage(StorageCommand::TempfileAlias {
                path: "C:/x".into(),
                alias: "alias".into()
            })
        );
        assert_eq!(
            parse("timer:pause:7").command,
            Command::Timer(TimerCommand::Pause(7))
        );
        assert_eq!(
            parse("timer:resume:7").command,
            Command::Timer(TimerCommand::Resume(7))
        );
        assert_eq!(
            parse("alarm:set:08:00|work").command,
            Command::Timer(TimerCommand::AlarmSet {
                time: "08:00".into(),
                name: "work".into()
            })
        );
        assert_eq!(
            parse("stopwatch:show:7").command,
            Command::Timer(TimerCommand::StopwatchShow(7))
        );
        assert!(
            matches!(parse("timer:show:7").command, Command::External(ExternalCommand { target, .. }) if target == "timer:show:7")
        );
        for (raw, expected) in [
            ("system:shutdown", SystemCommand::Shutdown),
            ("system:reboot", SystemCommand::Reboot),
            ("system:lock", SystemCommand::Lock),
            ("system:logoff", SystemCommand::Logoff),
            ("system:future", SystemCommand::Unknown("future".into())),
        ] {
            assert_eq!(parse(raw).command, Command::System(expected));
        }
        assert_eq!(
            parse("volume:pid:42:75").command,
            Command::System(SystemCommand::ProcessVolume { pid: 42, level: 75 })
        );
        assert_eq!(
            parse("layout:save:name|x").command,
            Command::Layout(LayoutCommand::Save {
                name: "name".into(),
                flags: Some("x".into())
            })
        );
        assert_eq!(
            parse("layout:load:name").command,
            Command::Layout(LayoutCommand::Load {
                name: "name".into(),
                flags: None
            })
        );
        assert_eq!(
            parse("layout:rm:name|x").command,
            Command::Layout(LayoutCommand::Remove {
                name: "name".into(),
                flags: Some("x".into())
            })
        );
    }

    #[test]
    fn special_families_win_before_query_override_without_consuming_it() {
        let payload = crate::diff::query::encode_payload(&crate::diff::query::DiffOpenPayload {
            left: Some("a".into()),
            right: None,
        })
        .unwrap();
        for raw in [
            "file_search:open".to_string(),
            format!("diff:open:{payload}"),
            "clipboard_modify:undo".to_string(),
        ] {
            let invocation = parse_command(
                action(&raw),
                Some("preserve me".into()),
                ActivationSource::Gesture,
            )
            .unwrap();
            assert_eq!(invocation.query_override.as_deref(), Some("preserve me"));
            assert!(matches!(
                invocation.command,
                Command::FileSearch(_) | Command::Diff(_) | Command::ClipboardModify(_)
            ));
        }
    }
}
