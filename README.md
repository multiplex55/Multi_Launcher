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
- [Dashboard](#dashboard)
- [Mouse gestures](#mouse-gestures)
- [Layouts](#layouts)
- [Calendar](#calendar)
- [Screenshot capture + markup editor](#screenshot-capture--markup-editor)
- [Configuration](#configuration)
- [Data files](#data-files)
- [Building](#building)
- [Contributor notes](#contributor-notes)
- [Troubleshooting](#troubleshooting)

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
  - `b ...` (bookmarks)
  - `note ...` (notes)
  - `todo ...` (tasks)
  - `tab ...` (browser tabs)
  - `mg ...` (mouse gestures)
  - `layout ...` (window layouts)

---

## Command prefixes cheat sheet

> This is the “most-used” surface area. Many prefixes also support additional subcommands—type the prefix and read the result list.

| Prefix | What it does | Examples |
|---|---|---|
| `?` | Search web (default browser) | `? rust egui` |
| `s` | Search with Google | `s rust borrow checker` |
| `d` | Search with DuckDuckGo | `d windows ui automation` |
| `=` | Calculator | `= (145*3) / 7` |
| `= history` / `calc list` | Calculator history | `= history` |
| `b` | Bookmarks | `b youtube` |
| `f` | Saved folders | `f downloads` |
| `cb` | Clipboard history | `cb list` / `cb clear` |
| `ss` / `shot` | Screenshot actions | `ss` / `shot region markup` |
| `conv` / `convert` | Conversion panel + converters | `conv` / `conv 10 km to mi` |
| `case` | Text case tools | `case snake Hello World` |
| `ts` | Timestamp helpers | `ts` / `ts 1700000000` |
| `emoji` | Emoji search | `emoji shrug` |
| `ascii` | ASCII art | `ascii hello` |
| `lorem` | Lorem ipsum generator | `lorem 40` |
| `note` | Notes | `note list` / `note add project ideas` |
| `todo` | Todo/tasks | `todo add p2 #work fix indexing` |
| `snip` | Snippets | `snip json` |
| `macro` | Macros | `macro add` / `macro list` |
| `tab` | Browser tabs (UIA) | `tab slack` / `tab cache` |
| `fav` | Favorites (pinned commands) | `fav` / `fav add build` |
| `mg` | Mouse gesture management | `mg settings` / `mg add` |
| `keys` / `key` | Send keystrokes | `keys ctrl+shift+t` |
| `layout` | Window layouts | `layout add work` / `layout run work` |
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
- List notes:
  - `note list`
- Search notes (title/content):
  - `note rustdoc`
- Inspect links around a note (linked todos/notes/mentions):
  - `note links roadmap`
  - `note links slug:roadmap-2026`

> Notes are markdown files stored in `notes/` by default. Set `ML_NOTES_DIR` to override.

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

## Contributor notes

- Draw architecture entrypoint: `src/draw/mod.rs` (wired from `src/lib.rs` via `#[path = "draw/mod.rs"] pub mod draw;`).
- Keep draw implementation files under `src/draw/*`.
- `src/_deprecated_draw_stub.rs` is legacy reference material and must remain outside module resolution (do not restore it as `src/draw.rs`).

Quick verification commands:

```bash
rg -n '#\[path = "draw/mod.rs"\]' src/lib.rs
rg -n 'src/draw\.rs|path\s*=\s*"draw\.rs"' src tests README.md
```

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
  - `layout add Work`
- List layouts:
  - `layout list`
- Run (apply) a layout:
  - `layout run Work`
- Edit layouts file:
  - `layout edit`

### Useful flags
- Dry run (preview without changing anything):
  - `layout run Work --dry-run`
- Don’t launch missing apps:
  - `layout run Work --no-launch`
- Only affect the active monitor:
  - `layout run Work --only-active-monitor`
- Filter windows included:
  - `layout run Work --filter chrome`

### File
- `layouts.json`

---

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
````

Notable settings (high impact):

* `hotkey` / `quit_hotkey` / `help_hotkey`
* `index_paths` (file indexing for search)
* `enabled_plugins` (allowlist)
* `plugin_dirs` (external plugins)
* `enable_toasts` + `toast_duration`
* `follow_mouse`, `always_on_top`, `hide_after_run`
* screenshot settings (`screenshot_dir`, `screenshot_auto_save`, `screenshot_use_editor`)
* dashboard settings (`dashboard.*`)

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

---
