# Multi Launcher
<img width="480" height="480" alt="Green_MultiLauncher" src="https://github.com/user-attachments/assets/8a68f544-536c-4eb5-8c0a-c5ef43e21c2d" />

Multi Launcher is a lightweight application launcher for Windows built with Rust
and `eframe`. The project targets Windows exclusively. It supports configurable
hotkeys, basic plugin architecture and file indexing to quickly open
applications or files.

It’s designed to be “one hotkey away” from:
- launching apps / files / bookmarks
- running small utilities (calc, convert, clipboard tools, etc.)
- driving **dashboard widgets** (notes, todo, system status, browser tabs, gestures, layouts, …)
- optionally triggering actions via **mouse gestures**

---


## Table of contents

- [Quick start](#quick-start)
- [Core workflow](#core-workflow)
- [Command prefixes cheat sheet](#command-prefixes-cheat-sheet)
- [Cookbook examples](#cookbook-examples)
- [File-search plugin](#file-search-plugin)
- [Clipboard Modify](#clipboard-modify)
- [Dashboard](#dashboard)
- [Mouse gestures](#mouse-gestures)
- [Layouts](#layouts)
- [MultiManager](#multimanager)
- [Calendar](#calendar)
- [Screenshot capture + markup editor](#screenshot-capture--markup-editor)
- [Configuration](#configuration)
- [Data files](#data-files)
- [Building](#building)
- [Troubleshooting](#troubleshooting)
- [Manual smoke tests](#manual-smoke-tests)

---

## Quick start

### Run
1. Build (see [Building](#building)) and run the app.
2. Press **`F2`** to show the launcher (default hotkey).
3. Start typing to filter results.
4. Press **Enter** to execute the selected result.

### Discoverability
- Press **`F1`** (default) to open help.
- Type **`help`** in the launcher to show a quick command/prefix overview.

---

## Core workflow

Multi Launcher is centered around a **single query box**:

- Results come from:
  - your `actions.json` (custom actions you define)
  - built-in commands (calculator, converters, utilities)
  - plugins (notes, todo, clipboard, browser tabs, layouts, etc.)
  - optional indexing of folders (fast file search)

- Most functionality is accessed via **prefix commands** like:
  - `bm ...` (bookmarks)
  - `note ...` (notes)
  - `todo ...` (tasks)
  - `tab ...` (browser tabs)
  - `mg ...` (mouse gestures)
  - `layout ...` (window layouts)
  - `mm ...` (MultiManager window workspaces)

---

## Command prefixes cheat sheet

> This is the “most-used” surface area. Many prefixes also support additional subcommands—type the prefix and read the result list.

| Prefix | What it does | Examples |
|---|---|---|
| `g` | Search Google | `g rust borrow checker` |
| `=` | Calculator | `= (145*3) / 7` |
| `= history` / `calc list` | Calculator history | `= history` |
| `bm` | Bookmarks | `bm youtube` |
| `f` | Saved folders | `f downloads` |
| `cb` | Clipboard history | `cb list` / `cb clear` |
| `cm` | Clipboard Modify operations, templates, pipelines, and undo | `cm trim | uppercase` / `cm template prompt-context` |
| `ss` / `shot` | Screenshot actions | `ss` / `shot region markup` |
| `conv` / `convert` | Conversion panel + converters | `conv` / `conv 10 km to mi` |
| `case` | Text case tools | `case snake Hello World` |
| `ts` | Timestamp helpers | `ts` / `ts 1700000000` |
| `emoji` | Emoji search | `emoji shrug` |
| `ascii` | ASCII art | `ascii hello` |
| `lorem` | Lorem ipsum generator | `lorem 40` |
| `note` | Notes | `note list` / `note add project ideas` |
| `todo` | Todo/tasks | `todo add p2 #work fix indexing` |
| `cs` | Snippets | `cs json` / `cs list` |
| `macro` | Macros | `macro add` / `macro list` |
| `tab` | Browser tabs (UIA) | `tab slack` / `tab cache` |
| `fav` | Favorites (pinned commands) | `fav` / `fav add build` |
| `mg` | Mouse gesture management | `mg settings` / `mg add` |
| `mm` | MultiManager window workspaces | `mm` / `mm reconnect` / `mm send all home` |
| `keys` / `key` | Send keystrokes | `keys ctrl+shift+t` |
| `layout` | Window layouts | `layout save work` / `layout load work` |
| `win` | Window list / focus | `win terminal` |
| `ps` | Processes list | `ps chrome` |
| `tm` | Task Manager | `tm` |
| `sys` | System actions | `sys lock` |
| `info` | System info | `info` |
| `net` | Network info | `net` |
| `ip` | Show local/public IP | `ip` |
| `bright` | Brightness control | `bright` |
| `vol` | Volume control | `vol` |
| `media` | Media keys | `media next` |
| `yt` | YouTube search | `yt rust egui` |
| `wiki` | Wikipedia search | `wiki egui` |
| `red` | Reddit search | `red egui` |
| `drop` | Drop-rate calculator | `drop 1/128` |
| `rand` | Random helpers | `rand 1..100` |
| `tmp` | Temp file manager | `tmp new log` / `tmp list` |
| `recycle` | Recycle bin tools | `recycle` |
| `rs` / `osrs` | RuneScape helpers | `osrs wiki karamja gloves` |
| `cal` | Calendar/reminders | `cal` / `cal add today 5pm Pay rent` |
| `fs` | File-search plugin | `fs` / `fs file main` / `fs content TODO` |

---

## Cookbook examples

### 1) Power search & launch
- Type part of an app name (from your `actions.json`) and hit Enter:
  - `steam`
  - `vscode`
- Search indexed files (if enabled via `index_paths`):
  - `resume` → `Resume.pdf`

### 2) Calculator (with history)
- `= 12*7 + 19`
- `= history` (or `calc list`) to open the calculator history panel.

### 3) Convert things quickly
- Unit conversion:
  - `conv 225 lb to kg`
  - `conv 10 km to mi`
- Base conversion:
  - `conv ff hex to dec`
  - `conv 255 dec to hex`
- Open the conversion panel (good for repeated conversions):
  - `conv`

### 4) Notes (markdown files)
- Create a new note:
  - `note add Meeting notes`
  - `note new Sprint plan --template meeting`
- List notes:
  - `note list`
- Search notes (title/content):
  - `note rustdoc`
- Show aliases/templates:
  - `note alias project`
  - `note aliases`
  - `note template list` (or legacy `note templates`)
- Inspect links around a note (linked todos/notes/mentions):
  - `note links roadmap`
  - `note links slug:roadmap-2026`

> Notes are markdown files stored in `notes/` by default. Set `ML_NOTES_DIR` to override.

Notes support an in-app markdown workspace with **Edit**, **Preview**, and
**Split** modes. Markdown task lists (`- [ ]` / `- [x]`) render as interactive
checkboxes, headings can appear in the outline sidebar, and sections can be
collapsed while reading or editing longer notes. Callouts use blockquote-style
markers such as `> [!NOTE]` or `> [!WARNING]`.

Use wiki links (`[[Roadmap]]`) and canonical links (`link://note/roadmap`) for
backlinks. Add aliases near the top of a note with either `Alias: Display Name`
or `Aliases: Alpha, Beta`; note search, open, backlinks, and relationship
commands resolve aliases case-insensitively. Templates live in the note
templates directory as `.md` files and expand variables like `{{title}}`,
`{{slug}}`, `{{date}}`, and `{{datetime}}` when creating notes.

### 5) Todos (tags + priority)
- Add tasks:
  - `todo add p1 #work fix mouse gesture stutter`
  - `todo add p3 #home buy coffee`
- Filter:
  - `todo #work`
  - `todo p1`
- Mark complete:
  - `todo done fix mouse gesture stutter` (select matching item)
- Inspect note attachments/anchors for a todo:
  - `todo links release checklist`
  - `todo links id:todo-1730000000-1 --json`

### 5.1) Canonical links (copy/paste workflow)
- Resolve and open canonical IDs:
  - `link link://note/roadmap-2026`
  - `link link://note/roadmap-2026#milestones`
- Typical workflow:
  - run `note links roadmap`
  - copy the `target` value (for example `link://note/roadmap-2026#milestones`)
  - paste into `link <id>` to jump directly to the target.

### 6) Favorites (pin “commands you actually use”)
Favorites are shortcuts that point at an action string (anything the launcher can execute).

- Open favorites manager:
  - `fav`
- Add a favorite with a prefilled label:
  - `fav add Build`
  - then set Action to something like: `shell:cargo build`
- Remove favorites quickly:
  - `fav rm build`

Good favorites to create:
- “Open project folder”
- “Run tests”
- “Open notes”
- “Screenshot region markup”
- “Layout: Work”

### 7) Browser tabs (UI Automation)
- Search tabs:
  - `tab youtube`
  - `tab docs`
- Refresh tab cache:
  - `tab cache`
- Clear tab cache:
  - `tab clear`

> If UI Automation can’t activate a tab directly, the app may simulate a click (cursor may briefly move).

### 8) Temp files (scratch logs, copy/paste buffers, etc.)
- Create a temp file:
  - `tmp new scratch`
- Open temp directory:
  - `tmp open`
- List and open:
  - `tmp list`
- Remove:
  - `tmp rm scratch`

---

## File-search plugin

Open the dedicated file-search UI from the launcher with the file-search action, then choose **Filename** or **Content** mode and **Global** or **Directory** scope. File search is intentionally explicit: edit the query and filters, then press **Search** or **Enter** in the search/root field to run it. It does not automatically search while typing, rerun when filters change, or persist typed search text.

### Launcher commands

- `fs` opens the file-search dialog with the last saved UI preferences.
- `fs file` opens the dialog in **Filename** mode.
- `fs content` opens the dialog in **Content** mode.
- `fs here file <query>` or `fs here content <query>` prompts for a folder, then searches that folder in **Directory** scope.
- `fs file <query> [root]` and `fs content <query> [root]` start a search immediately. If the final argument is an existing directory, it becomes the temporary **Directory** root; otherwise the search uses **Global** scope.

Examples: `fs`, `fs file README`, `fs content "TODO item"`, `fs here content launch_action`.

### Search roots and scope

- **Global** scope searches only the configured roots in `settings.json` at `plugin_settings.file_search.global_search_roots`; it does not mean the whole computer or every indexed drive. Invalid or duplicate roots are ignored at request time, and the UI warns when no valid global roots remain.
- Configure multiple permanent global roots by adding multiple paths to `global_search_roots` in settings.
- **Directory** scope uses custom temporary roots. Use **Add folder…** repeatedly to add multiple roots, or type/paste roots in the **Root** fields. These roots apply to the current search session and are not saved as history.
- The search text, selected result rows, custom directory-root selections, and file-search query history are not persisted. Only explicit UI preferences such as sort/filter defaults are saved.

### Filename search

**Filename** mode searches file and directory names under the selected roots.

- **Ranked substring** is the default **Filename matching** mode. It is case-aware according to **Case-sensitive** and ranks stronger matches first: exact filename, filename starts with the query, filename contains the query, then path contains the query. Highlighting shows the matching filename/path ranges.
- **Fuzzy** filename matching is available from **Filename matching** for typo-tolerant ordered-character matching. Use it when a filename is approximate or partially remembered; relevance still controls the default ordering.
- Use the **Type** filter to choose **Files**, **Directories**, or **Files and directories**.
- **Sort** options for filename results are **Relevance**, **Filename ↑**, **Filename ↓**, **Path ↑**, **Modified newest**, **Modified oldest**, **Size largest**, and **Size smallest**.
- Filename columns are configurable from the result header/menu preferences and saved in UI preferences. Supported columns are **Name**, **Directory**, **Kind**, **Match quality**, **Size**, **Modified**, and **Path**; defaults are **Name**, **Directory**, and **Match quality**.

### Content search

**Content** mode searches text inside files under the selected roots.

- **Exact phrase** treats the search text as one fixed string phrase.
- **Match any term** splits the query on whitespace and returns files containing any non-empty term.
- **Whole word** requires word-boundary content matches; combine it with either **Exact phrase** or **Match any term**.
- Content search reads files only; the **Type** filter is disabled in this mode.
- Content results are grouped by file. Each group header shows the path and match count, followed by displayed match rows with line previews. Per-file match limits can truncate large groups.
- **Sort** options for content results are **Discovery**, **Path then line**, **Match count**, **Modified newest**, **Filename relevance**, and **Line number**.
- Content search uses ripgrep when available and automatically falls back to the native content-search backend when ripgrep is missing or unavailable.

### Filters and refinement

- **Include extensions** and **Exclude extensions** accept comma-separated extensions. Leading dots are optional and normalized, so `rs, .md, toml` is valid. Include filters limit results to those extensions; exclude filters remove matching extensions.
- **Excluded directories** contains directory names to skip, not paths or globs. Use **Add exclusion** to add names such as `.git`, `target`, `node_modules`, `bin`, or `obj`; use **Remove** per entry, **Restore defaults** to return to `settings.json`, or **Clear** to temporarily search without those exclusions.
- Directory-exclusion edits in the dialog are temporary UI overrides for the next search request and do not rewrite the configured defaults unless preferences are explicitly saved by the app.
- The **Filter** field performs search-within-results refinement on the current visible result set. It does not start a backend search; use **Clear** to remove the refinement and **Search** again to apply changed backend filters.

### ripgrep discovery and fallback

For content search, Multi Launcher resolves ripgrep in this order:

1. Absolute `plugin_settings.file_search.ripgrep_executable_path` if configured and valid.
2. Fixed sidecar `rg.exe` next to the launcher executable.
3. Fixed portable location `tools/ripgrep/rg.exe` next to the launcher executable.
4. `rg.exe`, then `rg`, on the process `PATH`.
5. Native content-search fallback when ripgrep cannot be found or validated.

A configured bare command such as `rg` is allowed so PATH/sidecar discovery can run, but arbitrary relative configured paths with directory components, such as `tools/rg.exe` or `..\rg.exe`, are rejected. Use an absolute path for a custom executable location, or leave the setting empty/defaulted for auto-discovery.

If ripgrep is missing, content search starts with the native backend automatically and shows a non-blocking prompt offering **Locate rg.exe** for faster future searches. Dismissing or ignoring that prompt does not stop the active search. The native fallback is portable and does not require external tools, but may be slower than ripgrep.

### Everything CLI expectations

When `plugin_settings.file_search.everything_enabled` is true, **Global** **Filename** searches in **Ranked substring** mode may use the Everything ES CLI before falling back to the walkdir backend. Everything is not used for **Fuzzy** filename searches, **Directory** custom-root searches, content searches, or global filename searches whose include-extension/type combination cannot be represented safely.

Expected CLI setup:

- Install or provide Everything's command-line tool `es.exe`; the GUI executable `Everything.exe` is not a substitute.
- Configure `plugin_settings.file_search.everything_executable_path` with an absolute path or a bare command name, or put `es.exe` on `PATH`.
- Common Windows install locations under `Program Files`, `Program Files (x86)`, and `LOCALAPPDATA` are also checked.
- Multi Launcher still restricts Everything queries to the configured **Global** roots.

### Keyboard shortcuts

- **Up/Down** moves the selected visible result.
- **Enter** starts a search when focus is in **Search** or a **Root** field; otherwise it opens the selected result.
- **Ctrl+Enter** opens the selected result in the configured editor, including line/column for content matches when available.
- **Alt+Enter** reveals the selected result in Explorer.
- **Ctrl+C** copies the selected result path when the focus is not editing text.
- **Ctrl+Shift+C** copies the selected matching line for content results when available.
- **Ctrl+F** focuses the **Search** field.
- **Ctrl+L** focuses the first **Root** field in **Directory** scope.
- **Tab/Shift+Tab** follows normal UI focus traversal between fields and controls.
- **Escape** cancels an active search; when idle, it closes the dialog.

### Export and copy actions

Use the **Export** menu for visible-result exports:

- **Copy visible results** copies TSV for currently visible rows after **Filter** refinement.
- **Save visible results as TSV…** writes the visible TSV to `filename-results.tsv` or `content-results.tsv` by default.
- **Copy selected result** copies the selected filename path or selected content match line.
- **Copy visible full paths** copies only full paths for the currently visible selectable rows.

Result context menus can also copy the full path, filename, and matching line for content results.

### Diagnostics

Open **Diagnostics** in the dialog to inspect the active/last backend:

- Backend identity, executable path, version, resolution source, roots, start/end time, and cancellation state.
- Command details, including a query-redacted command in copied diagnostics and **Copy full command (may include query)** when a literal command is needed.
- Truncation details for global result limits, filename result limits, and per-file content match limits.
- Inaccessible paths and sampled path errors.
- Backend stderr snippets.
- Search summary details such as duration, files/directories scanned, result count, displayed rows, and cancellation.
- **Copy diagnostics** places the diagnostic report on the clipboard; it includes stderr and inaccessible-path samples, so review it before sharing.

### Deferred features

The improved file-search plugin does **not** include these deferred features yet:

- Regex search.
- Search history.
- Replace across results.
- Automatic search while typing.
- Automatic reruns when filters change.
- Performance benchmark infrastructure.

---


## Clipboard Modify

Clipboard Modify is a clipboard transformation surface available from the launcher with `cm` and from the Clipboard Modify dialog. Help in the dialog is generated from the same operation registry, template catalog, and saved-pipeline catalog used by execution, so custom templates and pipelines appear after configuration reloads. See [docs/clipboard_modify.md](docs/clipboard_modify.md) for the full operation catalog, syntax, schema, validation, undo, privacy, large-input, race-behavior, and recovery details.

Common examples:

- `cm trim | unique-lines | sort-ascending` trims each line, removes duplicates, then sorts.
- `cm wrap "<!-- " " -->"` uses custom wrapper quoting for prefixes/suffixes containing spaces.
- `cm template prompt-context` applies a configured template immediately.
- `cm apply clean-lines` runs a saved pipeline immediately.
- `cm undo` restores the clipboard text captured before the last Clipboard Modify write.

## Dashboard

The dashboard is a set of configurable widgets you can pin and keep visible as an “at a glance” control panel.

### Built-in widgets (current set)
- **Bookmarks / Folders / Commands**
  - bookmarks list, folders list, recent commands, frequent commands
- **Notes / Todo**
  - scratchpad, recent notes, todo list, recent todos
- **System / Diagnostics**
  - system status, CPU/RAM, network status, process list, diagnostics
- **Windows / Layouts**
  - window list, layouts widget (apply saved layouts)
- **Browser**
  - browser tabs widget
- **Mouse gestures**
  - gesture cheat sheet, recent gestures, gesture health/stats
- **Utilities**
  - stopwatch widget, volume widget, recycle bin widget, tempfiles widget, system controls/actions

> Use the dashboard editor UI to add/remove widgets and configure layout.

---

## Mouse gestures

Mouse gestures are a **right-click draw** interaction that can execute launcher actions.

### How it works
- Hold **Right Mouse Button** and move the mouse to draw a gesture.
- The gesture is tokenized (default is 4-direction):
  - `L`, `R`, `U`, `D`
- When you release, the best match binding is chosen and executed.

### Manage gestures
- Open settings dialog:
  - `mg settings`
- Open gesture editor dialog:
  - `mg` (or `mg gesture`)
- Add/edit:
  - `mg add`
  - `mg edit <filter>`
- Find/conflicts:
  - `mg find <filter>`
  - `mg conflicts`

### Binding kinds (what a gesture can do)
Gestures can map to:
- **Execute** an action (run something immediately)
- **SetQuery** (populate launcher query)
- **SetQueryAndShow** (populate + show launcher)
- **SetQueryAndExecute** (populate + run)
- **ToggleLauncher** (show/hide launcher)

This makes gestures useful for both:
- “Do the thing now”
- “Bring up the launcher already pre-filtered to the thing”

### Files
- Gestures: `mouse_gestures.json`
- Usage stats: `mouse_gestures_usage.json`

---

## Layouts

Layouts let you capture and restore a **window arrangement** (great for “work mode” setups).

### Commands
- Create a layout from current windows:
  - `layout save Work`
- List layouts:
  - `layout list`
- Run (apply) a layout:
  - `layout load Work`
- Edit layouts file:
  - `layout edit`

### Useful flags
- Dry run (preview without changing anything):
  - `layout load Work --dry-run`
- Don’t launch missing apps:
  - `layout load Work --no-launch`
- Only affect the active monitor:
  - `layout load Work --only-active-monitor`
- Filter windows included:
  - `layout load Work --filter chrome`

### File
- `layouts.json`

---

## MultiManager

MultiManager is a **Windows-oriented embedded window workspace manager** for keeping groups of real application windows organized inside named workspaces. It is designed for day-to-day window orchestration: capture the windows you care about, define where they should live, assign shortcuts, and quickly move or recover them later.

MultiManager is separate from saved `layout` commands. A saved `layout` is a named window arrangement that can be loaded from `layouts.json`; a MultiManager workspace tracks windows as workspace members, including their current Win32 window bindings and per-window home/target rectangles. Use `layout ...` for simple saved arrangements, and use `mm ...` when you want an interactive workspace manager for explicitly reconnecting or recapturing tracked windows.

Because MultiManager works with live Windows desktop windows, it uses Win32 concepts such as:

- **HWNDs** as the native identifiers for tracked windows.
- **Foreground-window capture** to add the currently active window to a workspace.
- **Top-level window enumeration** to find candidate windows and recover missing entries.
- **Explicitly reconnecting stale window handles** when a previously captured window was closed, relaunched, or received a new HWND.

### Commands

- `mm` — open MultiManager.
- `mm settings` — open MultiManager settings.
- `mm save` — save workspaces.
- `mm reload` — reload workspaces from disk.
- `mm reconnect` — reconnect missing/stale windows.
- `mm send all home` — send tracked windows to home rectangles.
- `mm save bindings` — save HWND binding snapshot.
- `mm restore bindings` — restore HWND binding snapshot.
- `mm recapture all` — recapture missing/stale windows.

### Typical workflow

1. Run `mm` to open MultiManager.
2. Add a workspace for a task or context.
3. Capture windows into that workspace.
4. Set each window's home and target rectangles.
5. Assign a hotkey for quick workspace actions.
6. Toggle, send home, send target, rotate, reconnect, or recapture windows explicitly as your session changes.


### Reconnect behavior

MultiManager reconnect is intentionally explicit and bounded:

- When workspaces load or reload, MultiManager can perform **one optional reconnect pass** if `auto_reconnect_on_load` is enabled. This pass enumerates visible top-level windows once and tries to match missing or stale entries.
- Failed automatic matches remain disconnected. MultiManager does not retry after that pass.
- Applications opened after the load/reload reconnect pass require manual reconnect. Use **Reconnect Windows** in the UI or run `mm reconnect`.
- **Reconnect Windows** and `mm reconnect` explicitly enumerate current windows and apply the same matching rules used by the load/reload reconnect pass.
- Toggle, home, target, and rotate actions validate existing HWNDs and clear invalid HWNDs before acting, but they do not search for replacement windows.
- Exact-title and stable-metadata matching rules are unchanged: exact-title candidates still need compatible stable metadata, duplicate exact-title candidates are ambiguous, and incompatible metadata remains a mismatch.

### Capture and recapture controls

- **Enter** captures the active foreground window.
- **Escape** cancels the current capture or recapture flow.
- **S** skips the current recapture item.

### Files

- `multi_manager_workspaces.json` — saved MultiManager workspaces.
- `multi_manager_bindings.json` — saved HWND binding snapshots.

## Calendar

Lightweight reminders/events that show up in search and can be displayed via widgets.

### Commands
- Open calendar UI:
  - `cal`
- Views:
  - `cal day`
  - `cal week`
  - `cal month`
- Upcoming / overdue:
  - `cal upcoming`
  - `cal overdue`
- Find:
  - `cal find dentist`
- Add:
  - `cal add today 5pm Pay rent`
  - `cal add tomorrow 09:30 Standup | daily sync`
  - `cal add 2026-02-05 all-day Vacation`

### Snooze
- `cal snooze 15m`
- `cal snooze 1h`
- `cal snooze tomorrow 9am`

### Files
- Events: `calendar/events.json`
- State: `calendar/state.json`

---

## Screenshot capture + markup editor

Screenshots can be taken to:
- clipboard
- file (auto-save supported)
- optional **built-in editor** for markup and quick annotations

### Commands
- `ss` → shows all screenshot actions
- Common actions include:
  - screen → clipboard
  - screen → file
  - region → clipboard
  - region → file
  - region → **markup** (opens editor)

### Markup editor highlights
- Draw markup (pen/shape tools)
- Copy to clipboard
- Save to file
- Optional toasts:
  - “Copied to clipboard”
  - “Saved screenshot”

Screenshot behavior is controlled by settings:
- `screenshot_dir`
- `screenshot_auto_save`
- `screenshot_use_editor`

---

## Configuration

### `settings.json`
This controls hotkeys, plugin enablement, UI behavior, dashboard, and more.

Minimal example:
```json
{
  "hotkey": "F2",
  "enable_toasts": true,
  "index_paths": ["C:\\Workspaces", "C:\\Users\\You\\Documents"]
}
```

Notable settings (high impact):

* `hotkey` / `quit_hotkey` / `help_hotkey`
* `index_paths` (file indexing for search)
* `enabled_plugins` (allowlist)
* `plugin_dirs` (external plugins)
* `enable_toasts` + `toast_duration`
* `follow_mouse`, `always_on_top`, `hide_after_run`
* screenshot settings (`screenshot_dir`, `screenshot_auto_save`, `screenshot_use_editor`)
* note settings (`note.*`)
* dashboard settings (`dashboard.*`)
* MultiManager settings (`multi_manager.*`)
* file-search plugin settings (`plugin_settings.file_search.*`)

Note behavior can be customized under the nested `note` settings object:

```json
{
  "note": {
    "external_open": "Wezterm",
    "backlinks_enabled": true,
    "aliases_enabled": true,
    "templates_enabled": true
  }
}
```

Legacy top-level note settings are still accepted for compatibility. Turning off
note features only hides or disables their UI/actions; it does not delete note
markdown content, aliases, backlinks, templates, or other metadata already on
disk.

File search can be customized under the nested `plugin_settings.file_search` settings object:

```json
{
  "plugin_settings": {
    "file_search": {
      "global_search_roots": ["C:\\Workspaces", "C:\\Users\\You\\Documents"],
      "ripgrep_executable_path": "rg",
      "excluded_directory_names": [".git", "target", "node_modules"],
      "max_search_results": 500,
      "max_matches_per_content_file": 25,
      "max_content_search_file_size_bytes": 2097152,
      "include_hidden_files": false,
      "case_sensitive": false,
      "ui_preferences": {
        "filename_match_mode": "ranked_substring",
        "content_match_mode": "exact_phrase",
        "whole_word": false,
        "file_type_filter": "files_and_directories",
        "included_extensions": [],
        "excluded_extensions": [],
        "excluded_directory_names": []
      }
    }
  }
}
```

`global_content_search_roots` is still accepted as a compatibility alias for `global_search_roots`. UI preferences store durable filter defaults only; search text, selections, custom directory-root entries, and search history are intentionally not written to `settings.json`.

MultiManager paths can be customized under the `multi_manager` settings object:

```json
{
  "multi_manager": {
    "enabled": true,
    "workspaces_path": "multi_manager_workspaces.json",
    "bindings_path": "multi_manager_bindings.json",
    "auto_save": true,
    "save_on_exit": true,
    "auto_reconnect_on_load": true,
    "ignore_launcher_window_on_capture": true
  }
}
```

`workspaces_path` controls where MultiManager stores workspace state, and `bindings_path` controls the optional live-window binding snapshot location. `auto_reconnect_on_load` enables the single load/reload reconnect pass described above. `ignore_launcher_window_on_capture` is a safety setting that helps prevent capture flows from saving the launcher window instead of the intended target window.

Disable the default hotkey entirely (useful if you bind your own trigger elsewhere):

* Set env var `ML_DEFAULT_HOTKEY_NONE=1`

### `actions.json`

Actions are your custom launch targets / macros / shell entries.

Each entry looks like:

```json
{
  "label": "Notepad",
  "desc": "Windows Notepad",
  "action": "notepad.exe",
  "args": null
}
```

---

## Data files

These are created/updated as you use the app (typically in the working directory alongside `settings.json`):

* `actions.json` — your defined actions
* `bookmarks.json` — saved bookmarks
* `folders.json` — saved folders
* `snippets.json` — snippets database
* `macros.json` — macro definitions
* `todo.json` — todo list
* `alarms.json` — timers/alarms
* `history.json` — command history
* `history_pins.json` — pinned history items
* `usage.json` — usage scoring data
* `clipboard_history.json` — clipboard history
* `calc_history.json` — calculator history
* `fav.json` — favorites
* `layouts.json` — window layouts
* `multi_manager_workspaces.json` — stores MultiManager workspaces, captured windows, aliases, hotkeys, and home/target rectangles
* `multi_manager_bindings.json` — optional HWND binding snapshot used to restore live window handles
* `mouse_gestures.json` — mouse gestures
* `mouse_gestures_usage.json` — mouse gesture usage stats
* `calendar/events.json` — calendar events
* `calendar/state.json` — calendar UI state
* `toast.log` — toast debug log (viewable from UI)

---

## Building

### Requirements

* Rust stable toolchain
* Windows (recommended; several features use Win32/UI Automation)

### Build

```bash
cargo build --release
```

### Run from source

```bash
cargo run
```

### Notes

* The project uses `rdev` and may require the `unstable_grab` feature for global input capture in some environments.
* Some plugins depend on Windows-specific APIs (window management, browser tab activation, etc.).

---

## Troubleshooting

### “Nothing happens when I press F2”

* Confirm `settings.json` is being loaded from the directory you’re running in.
* Check `hotkey` in `settings.json`.
* If you set `ML_DEFAULT_HOTKEY_NONE`, the default hotkey is disabled.

### Mouse gestures don’t trigger

* Ensure the **mouse_gestures plugin** is enabled (if you use `enabled_plugins`).
* Open `mg settings` and confirm “Enable mouse gestures” is checked.
* Try enabling debug logging in `mg settings` and inspect logs.

### Browser tabs can’t activate

* Run `tab cache` to rebuild the UI Automation cache.
* Some browsers / window states may block UIA access; the plugin may fall back to click simulation.

### MultiManager cannot find or move a window

* Run `mm reconnect` after restarting apps or the launcher so MultiManager can explicitly enumerate windows and refresh stale window handles. Apps opened after workspace load/reload remain disconnected until this manual reconnect succeeds.
* Use `mm recapture all` when a window is missing, closed and reopened, or ambiguous.
* Ensure the target app is not running elevated while Multi Launcher is running non-elevated.
* Check whether the workspace or window is disabled before sending, restoring, or moving it.
* Use **Refresh Titles** if a captured app changed its window title.
* If capture keeps selecting the launcher, keep `ignore_launcher_window_on_capture` enabled, focus the target window, and then press `Enter`.

---

## Manual smoke tests

Win32/UI Automation behavior is intentionally validated manually instead of in CI. Use [`docs/manual-smoke-tests.md`](docs/manual-smoke-tests.md) for MultiManager capture/reconnect/recapture checks, browser-tab activation, and mouse gesture verification.

