use crate::actions::Action;

#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ActionKind<'a> {
    Shell {
        cmd: &'a str,
        keep_open: bool,
    },
    ShellAdd {
        name: &'a str,
        args: &'a str,
    },
    ShellRemove(&'a str),
    ClipboardClear,
    ClipboardCopy(usize),
    ClipboardText(&'a str),
    Calc {
        result: &'a str,
        expr: Option<&'a str>,
    },
    CalcHistory(usize),
    BookmarkAdd(&'a str),
    BookmarkRemove(&'a str),
    FolderAdd(&'a str),
    FolderRemove(&'a str),
    HistoryClear,
    HistoryIndex(usize),
    System(&'a str),
    ProcessKill(u32),
    ProcessSwitch(u32),
    TimerCancel(u64),
    TimerPause(u64),
    TimerResume(u64),
    TimerStart {
        dur: &'a str,
        name: &'a str,
    },
    AlarmSet {
        time: &'a str,
        name: &'a str,
    },
    StopwatchPause(u64),
    StopwatchResume(u64),
    StopwatchStop(u64),
    StopwatchStart {
        name: &'a str,
    },
    StopwatchShow(u64),
    TodoAdd {
        text: String,
        priority: u8,
        tags: Vec<String>,
        refs: Vec<crate::common::entity_ref::EntityRef>,
    },
    TodoSetPriority {
        idx: usize,
        priority: u8,
    },
    TodoSetTags {
        idx: usize,
        tags: Vec<String>,
    },
    TodoRemove(usize),
    TodoDone(usize),
    TodoClear,
    TodoExport,
    SnippetRemove(&'a str),
    SnippetEdit(&'a str),
    SnippetAdd {
        alias: &'a str,
        text: &'a str,
    },
    FavAdd {
        label: &'a str,
        command: &'a str,
        args: Option<&'a str>,
    },
    FavRemove(&'a str),
    BrightnessSet(u32),
    VolumeSet(u32),
    VolumeSetProcess {
        pid: u32,
        level: u32,
    },
    VolumeToggleMuteProcess {
        pid: u32,
    },
    VolumeMuteActive,
    VolumeToggleMute,
    PowerPlanSet {
        guid: &'a str,
    },
    Screenshot {
        mode: crate::actions::screenshot::Mode,
        clip: bool,
    },
    MediaPlay,
    MediaPause,
    MediaNext,
    MediaPrev,
    RecycleClean,
    WindowSwitch(isize),
    WindowClose(isize),
    BrowserTabSwitch(Vec<i32>),
    BrowserTabCache,
    BrowserTabClear,
    TempfileNew(Option<&'a str>),
    TempfileOpen,
    TempfileOpenFile(&'a str),
    TempfileClear,
    TempfileRemove(&'a str),
    TempfileAlias {
        path: &'a str,
        alias: &'a str,
    },
    LayoutSave {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutLoad {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutShow {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutRemove {
        name: &'a str,
        flags: Option<&'a str>,
    },
    LayoutList {
        flags: Option<&'a str>,
    },
    LayoutEdit,
    NoteReload,
    Keys(&'a str),
    ExecPath {
        path: &'a str,
        args: Option<&'a str>,
    },
    Macro(&'a str),
}

pub(crate) fn parse_action_kind(action: &Action) -> ActionKind<'_> {
    let s = action.action.as_str();
    if let Some(rest) = s.strip_prefix("shell:add:")
        && let Some((name, args)) = rest.split_once('|')
    {
        return ActionKind::ShellAdd { name, args };
    }
    if let Some(name) = s.strip_prefix("shell:remove:") {
        return ActionKind::ShellRemove(name);
    }
    if let Some(cmd) = s.strip_prefix("shell_keep:") {
        return ActionKind::Shell {
            cmd,
            keep_open: true,
        };
    }
    if let Some(cmd) = s.strip_prefix("shell:") {
        return ActionKind::Shell {
            cmd,
            keep_open: false,
        };
    }
    if let Some(rest) = s.strip_prefix("clipboard:") {
        if rest == "clear" {
            return ActionKind::ClipboardClear;
        }
        if let Some(idx) = rest.strip_prefix("copy:")
            && let Ok(i) = idx.parse::<usize>()
        {
            return ActionKind::ClipboardCopy(i);
        }
        return ActionKind::ClipboardText(rest);
    }
    if let Some(spec) = s.strip_prefix("keys:") {
        return ActionKind::Keys(spec);
    }
    if let Some(idx) = s.strip_prefix("calc:history:")
        && let Ok(i) = idx.parse::<usize>()
    {
        return ActionKind::CalcHistory(i);
    }
    if let Some(val) = s.strip_prefix("calc:") {
        return ActionKind::Calc {
            result: val,
            expr: action.args.as_deref(),
        };
    }
    if let Some(url) = s.strip_prefix("bookmark:add:") {
        return ActionKind::BookmarkAdd(url);
    }
    if let Some(url) = s.strip_prefix("bookmark:remove:") {
        return ActionKind::BookmarkRemove(url);
    }
    if let Some(path) = s.strip_prefix("folder:add:") {
        return ActionKind::FolderAdd(path);
    }
    if let Some(path) = s.strip_prefix("folder:remove:") {
        return ActionKind::FolderRemove(path);
    }
    if s == "history:clear" {
        return ActionKind::HistoryClear;
    }
    if let Some(idx) = s.strip_prefix("history:")
        && let Ok(i) = idx.parse::<usize>()
    {
        return ActionKind::HistoryIndex(i);
    }
    if let Some(cmd) = s.strip_prefix("system:") {
        return ActionKind::System(cmd);
    }
    if let Some(pid) = s.strip_prefix("process:kill:")
        && let Ok(p) = pid.parse::<u32>()
    {
        return ActionKind::ProcessKill(p);
    }
    if let Some(pid) = s.strip_prefix("process:switch:")
        && let Ok(p) = pid.parse::<u32>()
    {
        return ActionKind::ProcessSwitch(p);
    }
    if let Some(hwnd) = s.strip_prefix("window:switch:")
        && let Ok(h) = hwnd.parse::<isize>()
    {
        return ActionKind::WindowSwitch(h);
    }
    if let Some(hwnd) = s.strip_prefix("window:close:")
        && let Ok(h) = hwnd.parse::<isize>()
    {
        return ActionKind::WindowClose(h);
    }
    if let Some(ids) = s.strip_prefix("tab:switch:") {
        let parts: Vec<i32> = ids
            .split('_')
            .filter_map(|p| p.parse::<i32>().ok())
            .collect();
        if !parts.is_empty() {
            return ActionKind::BrowserTabSwitch(parts);
        }
    }
    if s == "tab:cache" {
        return ActionKind::BrowserTabCache;
    }
    if s == "tab:clear" {
        return ActionKind::BrowserTabClear;
    }
    if let Some(id) = s.strip_prefix("timer:cancel:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::TimerCancel(i);
    }
    if let Some(id) = s.strip_prefix("timer:pause:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::TimerPause(i);
    }
    if let Some(id) = s.strip_prefix("timer:resume:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::TimerResume(i);
    }
    if let Some(arg) = s.strip_prefix("timer:start:") {
        let (dur, name) = arg.split_once('|').unwrap_or((arg, ""));
        return ActionKind::TimerStart { dur, name };
    }
    if let Some(arg) = s.strip_prefix("alarm:set:") {
        let (time, name) = arg.split_once('|').unwrap_or((arg, ""));
        return ActionKind::AlarmSet { time, name };
    }
    if let Some(id) = s.strip_prefix("stopwatch:pause:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::StopwatchPause(i);
    }
    if let Some(id) = s.strip_prefix("stopwatch:resume:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::StopwatchResume(i);
    }
    if let Some(id) = s.strip_prefix("stopwatch:stop:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::StopwatchStop(i);
    }
    if let Some(name) = s.strip_prefix("stopwatch:start:") {
        return ActionKind::StopwatchStart { name };
    }
    if let Some(id) = s.strip_prefix("stopwatch:show:")
        && let Ok(i) = id.parse::<u64>()
    {
        return ActionKind::StopwatchShow(i);
    }
    if let Some(rest) = s.strip_prefix("todo:add:") {
        if let Some(payload) = crate::plugins::todo::decode_todo_add_action_payload(rest) {
            return ActionKind::TodoAdd {
                text: payload.text,
                priority: payload.priority,
                tags: payload.tags,
                refs: payload.refs,
            };
        }
        // Backward compatibility for legacy wire format: `todo:add:<text>|<priority>|<csv_tags>`.
        let mut parts = rest.splitn(3, '|');
        if let (Some(text), Some(priority_raw), Some(tags_raw)) =
            (parts.next(), parts.next(), parts.next())
            && let Ok(priority) = priority_raw.parse::<u8>()
        {
            let tags = tags_raw
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(ToString::to_string)
                .collect();
            return ActionKind::TodoAdd {
                text: text.to_string(),
                priority,
                tags,
                refs: Vec::new(),
            };
        }
    }
    if let Some(rest) = s.strip_prefix("todo:pset:")
        && let Some((idx, p)) = rest.split_once('|')
        && let (Ok(i), Ok(pr)) = (idx.parse::<usize>(), p.parse::<u8>())
    {
        return ActionKind::TodoSetPriority {
            idx: i,
            priority: pr,
        };
    }
    if let Some(rest) = s.strip_prefix("todo:tag:")
        && let Some(payload) = crate::plugins::todo::decode_todo_tag_action_payload(rest)
    {
        return ActionKind::TodoSetTags {
            idx: payload.idx,
            tags: payload.tags,
        };
    }
    if let Some(idx) = s.strip_prefix("todo:remove:")
        && let Ok(i) = idx.parse::<usize>()
    {
        return ActionKind::TodoRemove(i);
    }
    if let Some(idx) = s.strip_prefix("todo:done:")
        && let Ok(i) = idx.parse::<usize>()
    {
        return ActionKind::TodoDone(i);
    }
    if s == "todo:clear" {
        return ActionKind::TodoClear;
    }
    if s == "todo:export" {
        return ActionKind::TodoExport;
    }
    if let Some(alias) = s.strip_prefix("snippet:remove:") {
        return ActionKind::SnippetRemove(alias);
    }
    if let Some(alias) = s.strip_prefix("snippet:edit:") {
        return ActionKind::SnippetEdit(alias);
    }
    if let Some(rest) = s.strip_prefix("snippet:add:")
        && let Some((alias, text)) = rest.split_once('|')
    {
        return ActionKind::SnippetAdd { alias, text };
    }
    if let Some(rest) = s.strip_prefix("fav:add:") {
        let mut parts = rest.splitn(3, '|');
        let label = parts.next().unwrap_or("");
        let cmd = parts.next().unwrap_or("");
        let args = parts.next();
        return ActionKind::FavAdd {
            label,
            command: cmd,
            args,
        };
    }
    if let Some(label) = s.strip_prefix("fav:remove:") {
        return ActionKind::FavRemove(label);
    }
    if let Some(val) = s.strip_prefix("brightness:set:")
        && let Ok(v) = val.parse::<u32>()
    {
        return ActionKind::BrightnessSet(v);
    }
    if let Some(val) = s.strip_prefix("volume:set:")
        && let Ok(v) = val.parse::<u32>()
    {
        return ActionKind::VolumeSet(v);
    }
    if let Some(rest) = s.strip_prefix("volume:pid:")
        && let Some((pid_str, level_str)) = rest.split_once(':')
        && let (Ok(pid), Ok(level)) = (pid_str.parse::<u32>(), level_str.parse::<u32>())
    {
        return ActionKind::VolumeSetProcess { pid, level };
    }
    if let Some(pid) = s.strip_prefix("volume:pid_toggle_mute:")
        && let Ok(pid) = pid.parse::<u32>()
    {
        return ActionKind::VolumeToggleMuteProcess { pid };
    }
    if s == "volume:mute_active" {
        return ActionKind::VolumeMuteActive;
    }
    if s == "volume:toggle_mute" {
        return ActionKind::VolumeToggleMute;
    }
    if let Some(guid) = s.strip_prefix("power:plan:set:") {
        return ActionKind::PowerPlanSet { guid };
    }
    if let Some(mode) = s.strip_prefix("screenshot:") {
        use crate::actions::screenshot::Mode as ScreenshotMode;
        return match mode {
            "window" => ActionKind::Screenshot {
                mode: ScreenshotMode::Window,
                clip: false,
            },
            "region" => ActionKind::Screenshot {
                mode: ScreenshotMode::Region,
                clip: false,
            },
            "desktop" => ActionKind::Screenshot {
                mode: ScreenshotMode::Desktop,
                clip: false,
            },
            "window_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Window,
                clip: true,
            },
            "region_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Region,
                clip: true,
            },
            "desktop_clip" => ActionKind::Screenshot {
                mode: ScreenshotMode::Desktop,
                clip: true,
            },
            _ => ActionKind::ExecPath {
                path: s,
                args: action.args.as_deref(),
            },
        };
    }
    if s == "media:play" {
        return ActionKind::MediaPlay;
    }
    if s == "media:pause" {
        return ActionKind::MediaPause;
    }
    if s == "media:next" {
        return ActionKind::MediaNext;
    }
    if s == "media:prev" {
        return ActionKind::MediaPrev;
    }
    if s == "recycle:clean" {
        return ActionKind::RecycleClean;
    }
    if s == "note:reload" {
        return ActionKind::NoteReload;
    }
    if let Some(alias) = s.strip_prefix("tempfile:new:") {
        return ActionKind::TempfileNew(Some(alias));
    }
    if s == "tempfile:new" {
        return ActionKind::TempfileNew(None);
    }
    if let Some(path) = s.strip_prefix("tempfile:open:") {
        return ActionKind::TempfileOpenFile(path);
    }
    if s == "tempfile:open" {
        return ActionKind::TempfileOpen;
    }
    if s == "tempfile:clear" {
        return ActionKind::TempfileClear;
    }
    if let Some(p) = s.strip_prefix("tempfile:remove:") {
        return ActionKind::TempfileRemove(p);
    }
    if let Some(rest) = s.strip_prefix("tempfile:alias:")
        && let Some((path, alias)) = rest.split_once('|')
    {
        return ActionKind::TempfileAlias { path, alias };
    }
    if let Some(rest) = s.strip_prefix("layout:save:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutSave {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:load:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutLoad {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:show:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutShow {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:rm:") {
        let (name, flags) = rest.split_once('|').unwrap_or((rest, ""));
        return ActionKind::LayoutRemove {
            name,
            flags: if flags.is_empty() { None } else { Some(flags) },
        };
    }
    if let Some(rest) = s.strip_prefix("layout:list") {
        if rest.is_empty() {
            return ActionKind::LayoutList { flags: None };
        }
        if let Some(flags) = rest.strip_prefix('|') {
            return ActionKind::LayoutList {
                flags: if flags.is_empty() { None } else { Some(flags) },
            };
        }
    }
    if s == "layout:edit" {
        return ActionKind::LayoutEdit;
    }
    if let Some(name) = s.strip_prefix("macro:") {
        return ActionKind::Macro(name);
    }
    ActionKind::ExecPath {
        path: s,
        args: action.args.as_deref(),
    }
}
