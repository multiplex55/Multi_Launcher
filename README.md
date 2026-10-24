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
- [Date arithmetic](#date-arithmetic)
- [File-search plugin](#file-search-plugin)
- [Search & Replace](#search--replace)
- [Batch Renamer](#batch-renamer)
- [Hot Folder](#hot-folder)
- [Clipboard Modify](#clipboard-modify)
- [Clipboard Snippets](#clipboard-snippets)
- [Paste Sequences](#paste-sequences)
- [Hotkey Directory](#hotkey-directory)
- [Table Utility](#table-utility)
- [Image Comparison](#image-comparison)
- [JSON and screen color utilities](#json-and-screen-color-utilities)
- [XML Toolkit](#xml-toolkit)
- [Hash Generator](#hash-generator)
- [Mouse coordinates and crosshair](#mouse-coordinates-and-crosshair)
- [Regex Tester](#regex-tester)
- [Screen region OCR](#screen-region-ocr)
- [Dashboard](#dashboard)
- [Mouse gestures](#mouse-gestures)
- [MkMacro authoring and reuse](#mkmacro-authoring-and-reuse)
- [Layouts](#layouts)
- [MultiManager](#multimanager)
- [Calendar](#calendar)
- [Screenshot capture + markup editor](#screenshot-capture--markup-editor)
- [Screenshot pinboard](#screenshot-pinboard)
- [Screen Draw](#screen-draw)
- [Configuration](#configuration)
- [Data files](#data-files)
- [Data safety and recovery](#data-safety-and-recovery)
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

### Repair saved targets

Use the `missing` prefix and choose the exact **check missing** command to inspect
saved targets. From a saved custom app or folder result, choose **Repair
target…**, select a replacement path, and explicitly confirm it. Custom apps may
point to a file/application or folder; saved folder entries require a folder.
Choosing a path never launches it. If the saved record or one of its active
references cannot be verified safely, repair reports the conflict and leaves
the saved data unchanged.

### Optional keyboard hints

Experimental Jump Mode is off by default. To enable it, turn on **Experimental
Jump Mode** in Settings and save. On supported screens, **Shift+Space** shows
hints for visible, enabled controls. Typing a hint only focuses or highlights
that control; use a later, separate Enter, Space, or normal typing to interact
with it. **Escape** cancels the hint session. Existing Save, Apply, and explicit
confirmation steps still require their normal action.

Coverage includes File Search; Snippets; Clipboard History; Dashboard commands
and simple widgets; JSON, Regex, and XML tools; Mouse Settings and Gestures;
Rename and Replace; and Dashboard and Radial editors. Coverage is selective:
pointer-only canvas drags, capture or recording, native picker internals,
unknown popups, and hidden, clipped, or disabled controls are not hint targets.

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
| `json` | Local JSON formatter and minifier | `json` / `json format` / `json minify` |
| `table` | Local delimited-text table viewer and JSON exporter | `table` |
| `xml` | Local XML tree, XPath, XSD, format and file utility | `xml` |
| `regex` | Local Rust regex tester | `regex` |
| `replace` | Explicit-root multi-file replacement with per-match approval and durable recovery | `replace` |
| `color` | HEX/RGB/HSL conversion and screen eyedropper | `color #ff0000` / `color pick` |
| `mouse` | Mouse settings, coordinates, picking, and independent crosshair | `mouse settings` / `mouse coords copy` / `mouse crosshair toggle` |
| `ocr` | Local English text recognition from a screen region | `ocr` |
| `pinboard` | Session-only static and live reference pins | `pinboard capture` / `pinboard live` / `pinboard list` |
| `ss` / `shot` | Screenshot actions | `ss` / `shot region markup` |
| `sd` / `sa` | Full-desktop Screen Draw annotations | `sd` / `sd ghost` / `sa done` |
| `conv` / `convert` | Conversion panel + converters | `conv` / `conv 10 km to mi` |
| `date` | Local date arithmetic and date differences | `date tomorrow` / `date days between 2026-10-05 and 2026-12-25` |
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
| `missing` | Explicitly inspect saved targets and repair missing custom apps or folders | `missing` → `check missing` |
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
| `rename` | Explicit-root batch filename rename with preview, protected extensions, guarded Apply, and safe Undo | `rename` |
| `data` | Data health, backups, and recovery | `data` / `data health` / `data backup` / `data folder` |

---

### Brightness controls

Type `bright` and choose **edit brightness** to open the named physical-monitor list. Opening performs an initial monitor read; **Refresh** requests an updated list and reading. Each controllable monitor shows its reported percentage and has its own draft slider. Select a value and press that row's **Set** button to change only that monitor. A confirmed Set is followed by a readback and refresh. The dialog does not continuously poll. Unsupported or unavailable monitors are shown without a fabricated brightness percentage. If a Set result is uncertain, refresh before trying another Set.

Some laptop panels, internal displays, USB-C docks, remote displays, and other hardware do not expose the Windows physical-monitor brightness controls. The list reports what the current system can identify and control.

The `bright <0–100>` numeric action remains on its existing legacy command route, which targets all enumerated physical monitors and passes the integer through unchanged. It is distinct from the dialog's per-monitor **Set** action.

---

## Cookbook examples

### 1) Power search & launch
- Type part of an app name (from your `actions.json`) and hit Enter:
  - `steam`
  - `vscode`
- Search indexed files (if enabled via `index_paths`):
  - `resume` → `Resume.pdf`

### Unified launcher search

The launcher searches applications, indexed files and enabled providers in one
result list. Use `app <query>` to search the action catalog, or an enabled
plugin's prefix, such as `fs`, `note`, `o`, `cs`, `paste` or `replace`, for its
commands. Omni Search retains its own provider controls. Empty queries can
show the dashboard, and pinned queries keep their usual behavior.

The file-search plugin's **Global** and **Directory** choices select its search
roots independently.

### 2) Calculator (with history)
- `= 12*7 + 19`
- `= history` (or `calc list`) to open the calculator history panel.

### 3) Convert things quickly
- Unit conversion:
  - `conv 225 lb to kg`
  - `conv 10 km to mi`
  - `conv 1/2 cup to ml`
  - `conv 1 cup 2 tbsp to ml`
  - `conv 1 us gallon to imperial gallon`
  - `conv 1 MB to Mb` (case distinguishes megabytes from megabits)
  - `conv 1 MB/s to Mbps`
- Base conversion:
  - `conv ff hex to dec`
  - `conv 255 dec to hex`
- Open the conversion panel (good for repeated conversions):
  - `conv`

Physical-unit conversions run locally and copy the converted value with its
destination unit. Month and year conversions are fixed-duration approximations:
`conv 1 month to days` uses 30 days, while `conv 1 year to days` uses 365 days.
`ton` means a US short ton; use `tonne` or `metric ton` for the metric tonne.
Unqualified customary volume names such as `cup`, `fl oz`, `pint`, `quart`, and
`gallon` use US measures; use `imperial` names for Imperial measures.

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

Within an open note, use **Ctrl+F** or the toolbar's **Find** button for literal,
case-insensitive search of its current Markdown, including unsaved edits.
**Enter** advances and **Shift+Enter** goes back, wrapping through the matches;
**Esc** closes Find while leaving the note open. Each note keeps its own
temporary query and match count. Edit selects the exact source occurrence;
Preview and Split identify the containing rendered segment. Hidden Markdown
targets and collapsed content are identified explicitly, with **Reveal in Edit**
for exact source selection. Searching does not edit the note or switch modes.

Expand **Replace** in the Find strip to replace the selected match or all current
matches in the unsaved Markdown source. Replacement text is literal, including
`$` and backslashes; Replace All uses the original non-overlapping matches and
does not recursively search inserted text. Replacements participate in the note
editor's Undo/Redo history and follow the note's normal auto-save, manual Save, and
discard policy.

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

## Date arithmetic

Use `date` for calendar arithmetic and date differences. It runs locally and
copies dates in ISO form, local date-times in ISO-like form, and differences
with their unit. Examples:

- `date tomorrow`
- `date 30 days from today`
- `date 1 month after 2026-01-31` → February 28, 2026
- `date days between 2026-10-05 and 2026-12-25` → `81 days`

Date arithmetic treats months as calendar months, clamping to the last valid
day when needed; this differs from `conv 1 month to days`, which uses an
approximate 30-day duration. Differences are calculated as the second date
minus the first, so the result can be negative.

Anchors include `today`, `tomorrow`, `yesterday`, `now`, weekdays such as
`next Friday`, `Christmas`, `New Year's Day`, ISO dates (`YYYY-MM-DD`),
US-style dates (`M/D/YYYY`), and written dates such as `Oct 5 2026`. Local
date-times use `YYYY-MM-DD HH:MM` or `YYYY-MM-DDTHH:MM`, optionally with seconds
and fractional seconds; sub-day offsets such as hours and minutes require a
local date-time anchor. Time-zone conversion is not supported. If `settings.json`
uses an `enabled_plugins` allowlist, include `date_arithmetic` to enable the
plugin.

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


## Batch Renamer

Type `rename` and choose **Open Batch Renamer**. Choose a folder explicitly;
opening the dialog does not guess a root or start a scan. The initial scope is
the root folder's direct children. Enable **Include subfolders** to scan
recursively, or use a listed folder as a new bounded root.

After a complete scan, select files and compose ordered prefix, suffix,
numbering, literal replacement, regex replacement, or case rules. Table sorting
and filtering only change the display; numbering keeps the scan's deterministic
path order. File extensions are protected by default. Choose **Edit full
filename** to include the extension in the rules.

**Confirm and apply** freezes the exact source-to-target mapping and applies it
as one transaction. Files are staged under unique temporary names in their own
parent directories before publication, so case-only changes and selected-file
cycles are supported. Existing or newly occupied names are never overwritten.
If a move cannot be safely completed or reversed, the receipt keeps the known
recovery paths for a later explicit Undo or recovery attempt.

The preview is a snapshot, not a promise that the files remain unchanged while
the dialog is open. Apply rechecks source identities, parent folders, and target
occupancy; changed sources or newly occupied targets are refused instead of
silently applying a different mapping. Cancellation is checked between file
moves. A failed or cancelled batch can therefore be partial if safe
compensation is blocked, and its receipt reports the exact outcomes rather
than promising all-or-nothing rollback.

Receipts persist across restarts, with a limit of 32. Before a new apply,
receipts already marked Undone are dropped to make room. Undo requires the
recorded file identity, bytes, size, and last-modified time to match, and refuses
occupied original names. Content verification is limited to 1 GiB per file and
4 GiB across one batch.

---

## Search & Replace

Type `replace` and choose **Open Search and Replace**. The resizable dialog
starts without a root or an active scan. Choose a folder explicitly; the
default scope is its direct children, with recursion available as an explicit
option. The initial extension filter contains `.rs`, `.txt`, `.md`, `.toml`, and
`.json`; extension filters, root-relative include globs, excluded paths, and
excluded globs are applied when you press **Search / Refresh**. Clearing the
extension filter removes extension restrictions, including for extensionless
files; UTF-8 and binary checks still apply. Search patterns can be literal text
or Rust regular expressions with an optional case-sensitive
mode. Replacement text can be literal or use Rust capture substitutions such
as `$1`, `${name}`, and `$$`; unknown references and output limits are shown
before Apply. Matches start unchecked, so each approved match is selected
individually or through its file row.

The root and **Search / Refresh** stay above the scrollable body. Basic
settings start open; **Advanced Filters** starts closed and shows the active
filter count. **Files with Matches**, **File Approvals**, and **Original vs.
Proposed Preview** fold independently without changing approvals. Preview panes
scroll vertically and horizontally for long lines. The fixed footer keeps
**Review Apply**, **Close**, active cancellation, and **Restore / Recovery**
reachable; the latter reveals retained receipts. Apply and recovery reviews
scroll separately from their fixed confirmation and cancellation buttons.

Search reads supported UTF-8 text and UTF-8-with-BOM files. Binary and
unsupported encodings are skipped and listed. Directories are never guessed,
and opening or changing a filter does not silently start a search. Results
retain stable match IDs from the captured search. Select individual matches or
files; filtering and sorting change only the displayed rows. The exact proposed
bytes and bounded original/proposed diff update from the captured source without
rescanning. Select any match row to inspect that stable match's original and
proposed text, including distant or multiline changes. Very long match previews
are truncated with the omitted length disclosed; the bounded file diff may show
only its first change. Unselected source bytes remain intact, including BOMs,
line endings, and a missing final newline.

**Review Apply** shows the roots, files, approved match counts, captured source
hashes, and proposed output hashes. **Confirm and Apply** freezes that exact
candidate before the guarded worker checks the current source identities and
bytes again. A stale or inaccessible file is skipped and reported; refresh and
review a new candidate before trying it again. Each write records a durable
backup and receipt in the application's `replace_recovery` data directory.
Cancellation or a failure can leave a partial batch, which the receipt reports
per file. Restore is explicit and only proceeds when the written file still
matches its recorded post-write identity and bytes. Recovery reconciles only
recorded staging identities; conflicting occupants are left in place and
reported. Receipts remain available after closing the dialog or restarting the
application. Restore handles a committed replacement; Recovery handles an
interrupted Apply or Restore. The existing `fs` filename and content search
commands keep their normal behavior and scope.

---

## Hot Folder

Enable the `hotfolder` plugin, type `hotfolder`, and choose **Open Hot Folder
Manager**. New and imported rules start disabled. Choose each watched directory
and any copy or move destination explicitly. The directory picker stores the
selected path; the separate sample-file picker is read-only and supplies only
metadata and a root-relative filename to **Test Rule**.

Enabling a rule is a separate action that rechecks its current root, filter,
destination, exact action target, and configuration fingerprint. Registered
launcher actions are limited to the exact current unattended-safe action
selected in the picker. MkMacro rules bind one exact current macro and its
transitive configuration; interactive or unavailable macros remain unavailable.
Strong actions show the rule, roots, scope, selected action, and fingerprint-bound
confirmation before they can be enabled. Saving any changed rule stores it
disabled and revokes the previous authorization; it never silently re-enables
the edited rule.

The watcher starts from reported filesystem events only. On startup or resume,
previously enabled intent is restored only after fresh validation; existing
files are not scanned or backfilled. Files must settle according to the rule's
stable-observation interval and timeout (50–5,000 ms sampling, 2–10 stable
observations, and 100–30,000 ms timeout). Temporary `.part` and `.crdownload`
files, subfolders, and filenames outside the selected trigger/filter are skipped
unless the rule explicitly includes the applicable scope. Runtime queues and
history are bounded; execution is capped at 30 runs per rule and 120 total runs
per minute, with at most 512 runs in one enable epoch.

**Test Rule** evaluates one selected sample with read-only metadata. It does
not enable or reconcile a watcher, dispatch an action or macro, change stored
rules, consume a live event, or write to execution history. Its match result and
planned action are separate from any real background activity.

Closing the manager hides only the UI; the app-owned service continues. Use
**Pause** to stop new admissions and request cancellation of owned active work.
Disabling the plugin also stops watches and admissions while retaining stored
enabled intent for later revalidation. Re-enabling the plugin starts fresh watch
epochs without backfill. The typed commands `hotfolder:pause` and
`hotfolder:resume` provide the same controls.

History distinguishes in-flight work from terminal receipts. `Partial` means a
filesystem operation did not complete fully; for example, a move can report
that the destination was created while the source remained. Recovery-required
records stay visible until resolved. **Clear History** removes terminal receipt
history from the display and its history store while preserving dedupe,
run-cap, and recovery state. Reasons are sanitized and do not include file
contents or action secrets.

---

## Clipboard Modify

Clipboard Modify is a clipboard transformation surface available from the launcher with `cm` and from the Clipboard Modify dialog. Help in the dialog is generated from the same operation registry, wrapper registry, control-command metadata, template catalog, and saved-pipeline catalog used by execution, so custom templates and pipelines appear after configuration reloads. See [docs/clipboard_modify.md](docs/clipboard_modify.md) for line-operation syntax, separator behavior, and saved-pipeline compatibility.

Common examples:

- `cm trim | unique-lines | sort-ascending` trims each line, removes duplicates, then sorts.
- `cm wrap "<!-- " " -->"` uses custom wrapper quoting for prefixes/suffixes containing spaces.
- `cm template prompt-context` applies a configured template immediately.
- `cm apply clean-lines` runs a saved pipeline immediately.
- `cm undo` restores the clipboard text captured before the last Clipboard Modify write.

## Clipboard Snippets

Use `cs` to open the Snippets editor. The resizable editor keeps Edit and Remove
available for long aliases or bodies, and shows each body preview on one line.
Use the filter to search aliases and body text; **Clear Filter** resets it, and
the match count shows how many entries are visible.

Common commands:

- `cs <query>` searches aliases and bodies. Activating a plain snippet copies its
  saved text with supported automatic values resolved; activating a prompted
  snippet opens its fill form.
- `cs list [query]` lists matching snippets with the same plain or prompted
  activation behavior.
- `cs add <alias> <text>` creates a snippet or updates the existing exact alias.
- `cs edit <alias>` finds an entry; activate its Edit result to open it in the
  editor. `cs edit <alias> <text>` updates or creates that alias directly.
- `cs rm <query>` finds matching snippets to remove.

### Tags

Tags are saved labels on a snippet; they do not change its body or action
identity. Add or remove tags in the existing editor (up to 16 tags, 64
characters each; whitespace is normalized and case-insensitive duplicates are
collapsed). The saved list shows tag chips separately from the one-line body
preview, including for hidden snippets.
Select one or more chips to require every selected tag (AND matching); **Clear
Filters** clears both the text search and selected tags. Changing a filter also
cancels an outstanding Remove confirmation. Launcher searches accept explicit
tag filters such as `cs tag:work tag:"high priority"`.

### Prompted fields

Prompting is opt-in for each snippet and starts off. In the editor, enable
**Prompt for fields**, configure the discovered fields, and save. A template such
as `Hello {{name}}` asks for `name`; keys are case-sensitive ASCII identifiers
starting with a letter or `_`, followed by letters, digits, or `_`. Each distinct
  key appears once in first-use order, and repeated occurrences use the same value.
`{{date}}` is an ordinary field. Automatic values use the namespaced tokens
`{{sys.date}}`, `{{sys.time}}`, `{{sys.clipboard}}`, and
`{{sys.window_title}}`; date and time default to `%Y-%m-%d` and `%H:%M:%S`,
and may use a validated chrono format after `:`, for example
`{{sys.date:%B %d, %Y}}` or `{{sys.time:%H:%M}}`. Values are captured once per
run before a prompted form takes focus. If clipboard text or the foreground
title is unavailable, its token resolves to empty text. Plain snippets preserve
all non-system brace text literally. The editor's **Test / Preview** resolves
an eligible draft without saving or copying it.

The editor lets you set a field's display label, default, required status, and
single-line or multiline input. Defaults populate a fresh form but are not
fallbacks: clearing a required field blocks Copy, while an empty optional field
substitutes an empty string. Required values containing only whitespace are
blocked; otherwise the entered text—including whitespace, Unicode, and line
breaks—is copied exactly. Tab and Shift+Tab move through the fields and Copy;
plain Enter inserts a newline in a multiline field. **Ctrl+Enter** copies the
completed result, and **Escape** or **Cancel** closes the form without copying.
This is a copy-only form; it does not paste into another application.

To write literal placeholder text while prompting is enabled, escape its opening
with a backslash: `\{{name}}` produces `{{name}}`. Only the adjacent backslash is
consumed, so `\\{{name}}` produces `\{{name}}`. An escaped opening is literal
through its next `}}`, even if its contents are not a valid key; a dangling
escaped `\{{` remains literal. When prompting is off, braces and backslashes are
ordinary saved text and are copied unchanged.

The live preview updates as you type. The editor's **Test / Preview** opens the
same form from the unsaved draft; **Return to Editor**, **Escape**, or the window
close returns to the editor without saving, copying, or recording history. If a
template becomes invalid, a required value is blank, or the clipboard write
fails, Copy is blocked and the form keeps its values so you can correct or
retry. If the saved snippet changes or is removed while the form is open, the
form keeps its values but cannot Copy; close it and reopen the snippet to use
the current saved definition.

Prompt form values are transient in memory and are not written to snippets,
defaults, action identities, arguments, logs, or action history. The saved
snippet body and configured defaults remain plaintext in `snippets.json`; a
successful copy writes the completed text to the clipboard, and normal clipboard
history may retain that copied output.

In the editor, **Hide contents** masks that snippet's previews. A saved masked
snippet opens concealed; choose **Reveal to Edit** to show its body. Reveal lasts
only for the current editing session and resets after Save, Cancel, closing the
window, switching entries, or reopening. You can change the alias or masking
setting and save without revealing the body; those changes preserve the saved
text. The GUI rejects duplicate exact aliases when creating or renaming. Its
Remove action asks for inline **Confirm** or **Cancel** before deleting an entry.

Masking is visual only: snippet bodies remain plaintext in `snippets.json` and
hidden bodies remain searchable; text typed into the filter remains visible.
Alias-aware actions store the snippet alias, not its body. The Dashboard also
masks hidden snippet previews; its clipboard-history section remains literal and
unchanged.

Favorites and history pins saved before snippet aliases were introduced still
run their stored literal action. To make one promptable, select the current
snippet result and save it as a new favorite or pin. Replace a radial cell's
literal clipboard action with the current snippet action, or use the dynamic
**Snippets** source. Opaque older actions are not rewritten automatically.
Current alias-aware snippet actions follow later changes between plain and
prompted mode; headless activation reports that prompting requires the GUI.

## Paste Sequences

Use `paste` to open the Paste Sequences editor. Add ordered snippet, clipboard
history, or temporary text steps; review the explicit preview, then choose
**Run Once** to prepare a memory-only draft or **Save** to keep a sequence in
`paste_sequences.json`. Saved literal text is stored as plaintext. Clipboard
steps store a content digest rather than the clipboard body and can expire when
the corresponding history item is removed.

Use **Add Snippet** or **Add Clipboard Item** to open the inline source picker.
Search snippets by alias, tags, and visible text; hidden snippet bodies are not
searched or revealed. Selecting a source shows its preview, and snippet templates
remain raw and unevaluated. **Insert selected** adds a reference to the draft; it
does not copy or run the source. The source is checked again when you insert it.
Use **Add Temporary Text** when the step itself should contain new literal text.

`paste <query>` searches saved sequence names, and activating a result runs the
sequence. `paste:run:<id>` is the payload-free stable-ID action used by saved
results. `paste:start`, `paste:next`, and `paste:cancel` control a prepared or
active run. Each saved sequence can have an **Optional start hotkey**, using the
application's existing chord syntax. Content-only edits and removing an unbound
sequence save directly. If a new Start shortcut is invalid or conflicts, valid
content still saves with the previous binding and the editor explains the partial
success; concurrent changes are never overwritten.

Open **Settings > Paste Sequences**, or **Playback Settings** in the editor, to
configure Next and Cancel. Defaults are **Ctrl+Alt+Shift+PageDown** and
**Ctrl+Alt+Shift+End**. Disable either control or leave its chord empty to turn it
off. Changes are staged until **Save** succeeds; **Cancel** discards the draft.
Conflicts are checked against launcher, quit/help, Screen Draw, radial, MkMacro
and other sequence shortcuts using actual co-fire rules, including a shared
primary key with different modifiers. Conflicting optional defaults become
inactive with a named reason; custom conflicts retain the previous active routes.
The launcher hotkey is never changed to resolve a Paste conflict. Quick Help,
the editor's Playback Settings hover and the Settings section show acknowledged
active/disabled/inactive controls and saved Start bindings, so a configured chord
is not presented as active when it was omitted.

The same section stores **Escape cancels active playback** (enabled by default)
and **Keep Paste Sequences dialog visible during playback** (disabled by
default). Follow the current state and step count in the dialog's **Run status**;
enable this setting to leave the dialog open while switching to the target
application. **Cancel Run** or the configured Cancel control ends a run early;
physical **Esc** also cancels when enabled and available for that started session.

Choose **Automatic**, **Manual advance**, or **Paste-to-Advance (Ctrl+V)** in the
editor. Automatic uses the configured delay between steps; Manual uses Next
with that delay as its minimum cooldown. Each step uses the current eligible
application, so you can switch applications between pastes. Template values
remain frozen from invocation context; the origin window is not a fixed paste
destination, and opening the editor without an external origin is valid.

Paste-to-Advance waits for one real **Ctrl+V** per step. Wait for the paste to
settle, release V, then press again for the next step. This mode ignores the
stored delay, preserving it for the other modes. After the final step settles,
the run completes and leaves that item on the clipboard: a fourth Ctrl+V after
a three-step run is an ordinary paste of the final item. A newer external
clipboard copy stops the sequence and is preserved. Hidden snippet contents
remain masked in the preview.

If another active control can also fire on Ctrl+V, Paste-to-Advance refuses
Start and names the conflicting control. Existing saved shortcuts stay unchanged;
Automatic and Manual advance remain available.

Early cancellation restores the original clipboard only while the session still
owns it; newer clipboard contents are preserved.

## Hotkey Directory

In Settings, **Search settings** finds existing controls; choose a result to
reveal and focus that control in its owning section. Search and navigation do
not change or save a setting. Use the owner's normal **Save**, **Apply**, or
**Cancel** controls, and remember that search does not enable unavailable
controls.

Open **Settings > Hotkeys > Hotkey Directory** to search configured shortcuts
and actions by text such as `End`, `paste`, or `macro`. The directory reports
the current route status separately from its configured chord. **Active** means
the runtime acknowledged the route; **Pending** means it is still being applied,
and **Unknown** means no reliable acknowledgement is available. Other statuses
include **Disabled**, **Invalid**, **Conflict**, **Unsupported**, **Omitted by
precedence**, and **Unavailable**. Conflict details identify competing actions.
Entries marked **Not configurable** have no current editor. **Edit** opens the
owning settings section or editor; use that owner's existing **Save** or
**Cancel** controls to apply or discard changes.

## Table Utility

Type `table` to open the local table viewer. **Paste Text** and **Open File**
are explicit source actions; opening the dialog never reads the clipboard or
file system, and an opened source file is never modified. The default parser
detects comma or tab delimiters, treats the first record as data (**Has Header**
is off), and accepts uneven row widths with diagnostics. **Parse / Refresh**
explicitly parses the current source. You can enable headers, change delimiter
or row width policy, filter rows with case-insensitive literal text, sort a
column, and select the rows and columns to export. Sorting uses numeric order
when every populated value is finite; otherwise it uses lexical order.

**Copy Selected Columns** produces quoted TSV, while **Copy JSON** and **Save
JSON As** use the same current selection. Large sources remain intact but use
a bounded preview of up to 2,048 content characters plus a truncation marker
instead of an oversized inline editor.
The source limit is 10 MiB, with up to 50,000 data rows and 128 columns. If a
source edit or parse fails, the last successful table stays visible with its
original parse settings; outputs remain disabled until the current source
parses successfully.

## Image Comparison

Type `imgdiff` to open the session-only image comparison dialog. **Open A…**
and **Open B…** choose files independently; **Paste A** and **Paste B** read
the image clipboard only when explicitly selected. File inputs support PNG,
JPEG, BMP, and GIF, with a limit of 16,777,216 pixels per image. The bounded
clipboard reader accepts CF_DIB images. Failed replacements keep
the last accepted image.

Choose **A**, **B**, **Wipe**, or **Difference** to inspect the pair. Images are
aligned at their top-left corners without scaling or registration; uncovered
areas use a checkerboard. Difference uses exact RGBA bytes, including alpha
and RGB values in transparent pixels, and distinguishes Changed, A-only,
B-only, and Neither. Inputs and results remain in the current session and are
cleared when the dialog closes. This is separate from the text-oriented `diff`
command.

## JSON and screen color utilities

`json` opens a compact, local JSON editor. `json format` and `json pretty` prefer
two-space formatting; `json minify` prefers compact output. Each open initializes
from the clipboard only when it contains valid strict JSON. Use **Paste from
Clipboard** to load clipboard text explicitly, then edit, **Format** or
**Minify**, and **Copy Result** when ready. Formatting preserves object key order.
Invalid input remains editable and reports the parser's line and column.
Formatting and minifying do not write the clipboard. Objects, arrays, and primitive
JSON values are supported; comments and trailing commas are rejected.
Switch to **Tree** for a collapsible read-only view with JSON Pointer paths,
value/subtree/path copy actions, and virtualized rows. Tree search supports
case-sensitive literal substrings across decoded object keys, decoded string
values, and the JSON spellings of numbers, booleans, and null. Array indices
are not keys, and object/array summaries are not searched as values. **Both**
shows one result per matched node in document order; Previous/Next expands and
scrolls to each result. Search results use the parsed snapshot and clear while
the source is edited or invalid.
Tree paths use JSON Pointer: the root path is the empty string, and path
segments escape `~` as `~0` and `/` as `~1`. For string nodes, **Copy Value**
copies decoded text, while **Copy Subtree** copies the quoted and escaped JSON
string; container copies contain their JSON serialization.
`cm json-pretty` and `cm json-minify` continue using the same JSON transformation
implementation, including in saved Clipboard Modify pipelines.

## XML Toolkit

Type `xml` to open the resizable XML Toolkit. It accepts at most 4 MiB of UTF-8
source text (UTF-8 files with a BOM are accepted), parses locally with DTD and
external entity resolution disabled, and keeps Format/Minify output separate
until **Apply Output to Source** is selected. Conservative minifying preserves
all character data, mixed content, and `xml:space` semantics. Oversized output
remains available through explicit Copy or Save As actions, while Apply is
disabled above the source limit.

**Open XML**, **Paste Clipboard**, **Copy Source**, **Copy Output**, and Save As
are explicit actions; opening or rendering the utility never reads or writes
the clipboard. Loading a file does not watch or modify it. File input is
strictly decoded as UTF-8 with a bounded read. Save As writes UTF-8 to a chosen
local path and asks before replacing an existing file. Closing preserves the
authored draft for the next open but ends its XML worker session.

XPath queries run against the accepted document snapshot. XML documents with a
default namespace require a user-defined XPath prefix and namespace URI; the
tool does not invent scalar booleans or numbers as text. XSD validation uses an
explicitly chosen local schema. Relative `xs:include` and `xs:import` resources
must remain under that schema's folder; imports need an explicit
`schemaLocation`, and includes inside imported schemas are unsupported. Remote,
absolute, and instance-provided schema references are not fetched.

## Hash Generator

Type `hash` to open the resizable Hash Generator. Enter text, explicitly choose
**Load Clipboard** to hash a clipboard snapshot, or choose a local file. File
hashes stream through a bounded buffer in a worker; progress and cancellation
keep the launcher responsive. The tool supports SHA-256, SHA-512, and SHA-1 for
legacy checksum compatibility. SHA-1 is not collision-resistant for modern
security decisions. Enter an optional expected hexadecimal digest to see a
Match or Mismatch result; surrounding whitespace and letter case are ignored.
Changing the source or algorithm clears the previous digest. Use **Copy Digest**
to write the result to the clipboard. The tool does not log input text or file
names, and it never reads the clipboard until **Load Clipboard** is selected.

File Search also offers **Calculate hash** from a non-directory result's context
menu. It opens the same tool with that selected path; calculation remains an
explicit action in the dialog. `hash:open` is the payload-free launcher command.

`color pick` parks the launcher and freezes the Windows virtual desktop. Move the
pointer to inspect pixels in the magnifier; its center marker identifies the exact
pixel. Left-click selects that pixel, and Escape cancels. The launcher returns
with the usual `color #rrggbb` HEX/RGB/HSL results after selection. Choose a result
to copy it; selecting a pixel alone leaves the clipboard unchanged. The picker
supports signed desktop coordinates, including monitors to the left or above the
primary monitor, and samples the frozen image throughout the session.

## Mouse coordinates and crosshair

On Windows, `mouse` discovers the common Mouse actions. `mouse settings`
opens the focused Mouse Settings dialog; `mouse help` describes the command
hierarchy. Settings are grouped into Click Visualization, Crosshair, Cursor
Halo, Cursor Magnifier, and Coordinate Display, using the same saved appearance
preferences as commands. Appearance options primarily live in this dialog rather
than filling normal launcher discovery with parameter commands.

`mouse coords toggle` toggles a cursor-following, click-through coordinate HUD.
`mouse crosshair toggle` independently toggles a passive crosshair at the cursor. Both can run
together without taking focus or intercepting ordinary typing and clicks.
`mouse coords on|off` and `mouse crosshair on|off` set a mode explicitly. Their
Enabled checkboxes in Mouse Settings change live, transient runtime state;
opening settings does not enable a mode. Apply saves appearance changes.

`mouse click toggle|on|off` adds a transient visual at physical left, right, and
middle button-down positions. It observes through the existing shared mouse
gesture hook, so enabling it does not change which presses a configured gesture
consumes or passes through. Injected and tagged replay events are ignored, which
prevents a replayed unmatched right click from drawing twice. Apply in Mouse
Settings saves the Ripple, Circle, or Flash style, color, 4..256 physical-pixel size,
0.1..1.0 opacity, and 50..2000 ms duration; click enablement is session-only
and starts off after restart. With click as the only active effect, its worker
waits for press events and does not sample the cursor while idle.

Coordinates are signed physical pixels, including negative desktop positions.
`mouse coords space desktop|monitor|client` selects desktop coordinates, coordinates
relative to the containing monitor's full bounds, or coordinates relative to the
foreground window's client origin. While the launcher is foreground, client
space uses the last external target window. Missing geometry is reported as
unavailable rather than copied as zero.

Use Mouse Settings for the coordinate space, compact/detailed presentation and
cursor offset. Advanced queries `mouse coords compact|detailed` and
`mouse coords offset -32 48` remain available for automation.
`mouse coords freeze|unfreeze` controls the displayed sample. `mouse coords copy` writes the displayed
sample as `x,y`; a frozen HUD copies its frozen sample. Detailed presentation
includes monitor/context information and the last successful copy.

`mouse coords pick` starts a one-shot capture and temporarily parks the launcher so
targets remain visible. A fresh left click copies that pixel's live coordinates
as `x,y`, using the space selected when the session began, even if the HUD is
frozen. The capture consumes both the press and its matching release; the
underlying application does not receive the capture click. Escape or
`mouse coords cancel` cancels without writing the clipboard. The session stays active
while an already consumed press waits for release, then cleans up before
publishing a result. Repeating `mouse coords pick` keeps the existing session.

Coordinate capture cannot overlap OCR, Color Pick, Screen Draw, screenshot crop,
or a MkMacro point/rectangle overlay. A radial action that starts pick closes its
radial session first; a new radial overlay cannot open until capture cleanup finishes.
Passive HUD and crosshair operation remain independent of those capture tools.

Mouse Settings provides crosshair RGB color, thickness, arm length, opacity,
center gap in physical pixels, guides and contrast outline. The center gap is
measured from the cursor hotspot to the nearest visible colored or outlined
crosshair stroke. Advanced controls remain available as
`mouse crosshair color #ff0000`, `mouse crosshair thickness 2`,
`mouse crosshair length 12`, `mouse crosshair opacity 0.8`,
`mouse crosshair gap 16` (0..128 physical pixels; default 16 per arm),
`mouse crosshair guides on|off`, and `mouse crosshair contrast on|off`.
Guides span the virtual desktop; contrast adds an
outline for visibility. Preferences are saved, while activation and frozen/copy
state are temporary. `mouse effects off` also disables click feedback, while
keeping the HUD, frozen display, last successful copy, clipboard, and an active
pick. `mouse coords help`, `mouse crosshair help`, and `mouse click help` show
the feature controls. Old standalone `coord` and `crosshair` queries/raw actions
are removed; this branch's inspected stored data had no references requiring aliases.

Cursor Halo inverts actual desktop pixels in a circular region centered on the
live cursor. Cursor Magnifier shows real screen content at its configured
factor; Offset mode moves the lens while keeping its source centered on the
cursor, and Centered mode places the lens at the cursor. Their radius, inversion
strength, destination mode, zoom factor, diameter, outline and colors are saved
through Mouse Settings. `mouse halo toggle|on|off` and
`mouse zoom toggle|on|off` change independent session-only switches; both start
off each time the app starts. `mouse effects off` disables the crosshair, halo
and magnifier while leaving the coordinate HUD and pick/freeze/copy state alone.
The dialog reports whether a native effect is active, paused, unavailable, or
using the halo's clearly labeled **non-inverting outline fallback**. That
fallback keeps a contrasting cursor ring visible when desktop inversion cannot
be initialized or presented; it does not claim inversion is occurring.

The native effects use Windows Magnification on supported composited desktop
content. Protected surfaces, exclusive fullscreen applications, some games,
remote desktops, screenshot tools and video/streaming capture paths may omit or
alter the effect output. A screenshot or stream is not guaranteed to contain the
same composed pixels visible on the desktop; API success and saved readbacks
alone do not prove that composition was included.

For an opt-in native runtime check, run `cargo run --features dev-smoke-binaries --bin coordinate_tool_smoke`
on an interactive Windows desktop for the existing passive/capture fixture, or
`cargo run --features dev-smoke-binaries --bin coordinate_tool_smoke -- --cursor-effects` for the cursor
effects fixture. The latter opens an ordinary controlled scene with a palette,
checkerboard, text and a changing marker. F1 toggles the HUD, F2 the crosshair,
F3 the halo, F4 the magnifier, and F5 turns effects off while retaining the HUD.
F6 cycles halo inversion through 0%, 40% and 100%; F7 switches centered/offset
zoom; F8 cycles through 1.25×, 1.7×, 2× and 4×; F9 switches between 160- and
163-pixel lens diameters; F10 toggles guides. F11 writes current status and
own-process HWND/source/transform/filter diagnostics, F12 saves a bounded
desktop BMP under `target/coordinate-tool-smoke`, and Escape exits. The fixture
uses no synthetic input or clipboard access. Its GDI readback can omit
magnifier-composited output, so inspect any saved image and treat the parent
desktop observation as the native visual check.

For a finite controller-driven observation sequence, use
`cargo run --features dev-smoke-binaries --bin coordinate_tool_smoke -- --cursor-effects-auto`. It holds each
named mode/appearance stage for about 1.4 seconds, logs foreground and native
window diagnostics, saves bounded readbacks for halo strengths and zoom modes,
then turns effects off and shuts down. It does not synthesize input or move the
cursor. Saved halo defaults are 60 physical pixels and 40% inversion; the
magnifier defaults to 2×, 160 physical pixels, Offset mode, and a (+120,+80)
physical-pixel destination displacement. Supported zoom factors are 1.25× to
4×.

## Regex Tester

`regex` opens a resizable local Regex Tester with live highlighting, match
navigation, capture inspection, substitution previews, explanations, searchable
reference material, examples, recent history, and saved presets. It uses Rust's
`regex` engine; look-around and traditional backreferences are unsupported.
Test cases check whether each input contains any substring match; use `^…$` anchors
when the whole input must match. Case labels and plaintext inputs are stored only when
you explicitly save or update a preset. Presets use schema version 2; version 1 presets
load without test cases and keep their original file bytes until an explicit preset
change writes the migrated version. Clipboard writes require an explicit copy action.
See [Regex Tester](docs/regex_tester.md)
for flags, shortcuts, storage, and interactive limits.

## Screen region OCR

Type `ocr` and activate **OCR Screen Region**. The launcher moves out of the way;
drag a rectangle over the text and release to confirm. Escape cancels selection.
Recognition runs locally using an installed English Windows OCR language.

Select or edit the multiline result, then choose **Copy All** to copy the current
edited text, including its line breaks. Recognition leaves the clipboard
unchanged. **Re-capture** discards the current result and selects a fresh region;
**Close** discards the transient result.

If no English OCR language is installed, install an English language pack in
**Windows Settings > Time & language > Language & region**, then try again.

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
- **Zoom** and **Text Size** share a row above **Tools** when there is room; at
  narrow widths the controls wrap onto separate lines.
- Copy to clipboard
- Save to file
- Optional toasts:
  - “Copied to clipboard”
  - “Saved screenshot”

Screenshot behavior is controlled by settings:
- `screenshot_dir`
- `screenshot_auto_save`
- `screenshot_use_editor`

## Screenshot pinboard

Use `pinboard capture` to select a screen region and open it as an in-memory floating reference image. Multiple pins are session-only, default to Always on Top, and are limited to 8 pins and 256 MiB total image data. Each pin can be moved and resized, switched between Fit and 1:1 display, zoomed, made not-topmost or click-through, and explicitly copied or saved. Fit and 1:1 sizing use each child viewport's DPI. The image remains available without a backing temporary file; Copy Image and Save As run only when selected.

Each pin has a session-only **Opacity** control from 10% to 100%, defaulting to
100%, in both its toolbar and **Pin List** row. The requested value is shown
separately from the confirmed native value while a change is pending or has
failed. Use **Reset opacity to 100%** in a pin toolbar or **Reset 100%** in the
Pin List to restore full opacity. Opacity changes the displayed window only;
**Copy Image** and **Save As** use the original image pixels.

Use `pinboard live` to select a crop inside one external window and refresh it
through Windows Graphics Capture, capped at approximately 10 frames per second.
The crop follows that source window when it moves and can update while another
window covers it where Windows permits capture. Ambiguous, multi-window, and
self-capture selections are rejected. Live capture has no desktop-polling
fallback. Static capture remains the default.

Live pins have **Pause/Resume** controls and a status indicator in the pin and
pin list. A minimized or temporarily unavailable source keeps its last completed
frame and can recover; a closed source stays frozen until the pin is closed.
Copy Image and Save As take one completed frame as a still image, including while
paused. Static and live pins share the 8-pin and image-memory limits, including
outstanding image actions; live capture also bounds native capture resources.
Closing a live pin stops its own capture. Sources, images, and pin layout remain
session-only.

Use `pinboard list` to recover a click-through pin with its Recover control or the keyboard-accessible Recover All button, or to close pins. The list reports desired control values separately from native child-window status. Region-selection cancellation creates no pin; capture errors remain reachable even when capture began with the launcher hidden. Pin layout and images are not restored after restart. Existing `ss`, `shot`, and `screenshot` commands keep opening the screenshot capture/editor flows.

---

## Screen Draw

Screen Draw freezes one snapshot of the complete signed Windows virtual desktop and opens a
native annotation canvas plus a small always-on-top toolbar. Start it with `sd` or `sa`.
Subcommands are `toolbar`, `new capture`, `ghost`, `done` (or `finish`), `clear`, and `close`.

- **Drawing** accepts left- or right-button pen input and provides pen, highlighter, line,
  arrow, rectangle, ellipse, text, eraser, fading ink, and eyedropper tools. Undo/redo,
  annotation visibility, line thickness, colors, and frozen/white/black/custom backgrounds
  are available from the toolbar.
- **Ghost** and **Finish** hide the input canvas and show annotations in a passive,
  click-through overlay. Resume returns to the same capture and document; **New Capture**
  deliberately replaces both.
- **Escape** first cancels an active primitive or text edit, then enters safe Ghost mode.
  The emergency chord (default `Ctrl+Shift+F12`) pauses from every session mode, releases
  pointer capture and Mouse Gesture suppression, and leaves the toolbar available for recovery.
- Finish can export the whole desktop or a selected region to the clipboard, a PNG file, or
  the existing Screenshot Editor. Exports can use the frozen desktop, white, black, custom,
  or transparent background. Region selection uses the shared rectangle picker, and editor
  handoff occurs only after native Screen Draw windows have closed. PNG files use collision-safe
  names in the same configured directory as ordinary screenshots.
- Mixed-DPI and negative-origin monitor layouts use signed virtual-desktop coordinates. A
  Windows display change safely pauses native drawing and preserves the document for export;
  use **New Capture** before resuming against the new topology.

Screen Draw preferences live at `plugin_settings.screen_draw`. `launch_hotkey` is global and
disabled by default; malformed persisted chords are reported and disabled. Tool shortcuts are active
only while Drawing (`P/H/L/A/R/O/T/E/G/V`, `1`–`9`, brackets, and undo/redo). Toolbar
position/orientation, default tool/color/thickness/background, palette, fade duration, text
size, the emergency chord, and local shortcuts are persisted with backward-compatible
defaults when fields are absent.

Pressure-sensitive stylus input and simultaneous multi-touch drawing are roadmap items, not
part of the current pointer-input implementation.

---

## MkMacro authoring and reuse

Open **Mouse/Keyboard Macros** and use its **Reuse** menu for packages, libraries,
templates, and the complete in-app MkMacro help.

- `Ctrl+C`, `Ctrl+X`, `Ctrl+V`, and `Ctrl+D` operate on complete structured step
  selections. Drag the primary selected row to move a multi-selection together.
- `Ctrl+F`, `Ctrl+H`, and `Ctrl+G` open Find, schema-aware Replace, and Jump to
  Step. Blocks can be folded, and steps can carry labels, comments, accent colors,
  bookmarks, and breakpoints; the Outline provides structural navigation.
- Reusable macros declare typed parameters and named typed outputs. **Call Macro**
  uses explicit argument bindings and output mappings; **Return** publishes declared
  outputs. Calls have isolated local-variable frames, may nest, and appear in the
  Runtime Inspector. Stable IDs keep references intact across rename/reorder, while
  recursive call cycles are rejected.
- In the Signature Editor, String parameters can use **Text**, **File picker**,
  **Folder picker**, or an ordered **Choice list** with an optional default.
  **Browse** fills a File or Folder path, and selecting a Choice fills its value;
  neither action runs the macro. Review or edit the invocation values, then choose
  **Run** separately. If a chooser is canceled or unavailable, the path remains
  editable. Save or Cancel the authoring draft through the editor as usual.
- **Read Clipboard Text** reads the current clipboard text into a named String
  variable when the worker reaches that step. **Write Clipboard Text** writes an
  exact literal or String variable at its step; neither action pastes the text nor
  restores earlier clipboard contents. Busy or non-text failures use the step's
  Stop, Continue, or Retry policy, and these actions are unsupported in unattended
  Hot Folder runs. Runs whose enabled reachable macro closure contains either
  action hide runtime variable values and sanitize dependent failure details;
  ordinary macro debug values and failure details remain available.
- Exporting a macro or explicit multi-root library captures all authored Call
  dependencies and referenced images. Import shows additions, conflict-driven
  renames, asset reuse, and hotkey conflicts before an explicit Apply. Imported
  authored hotkeys are retained.
- A saved template is an independent package snapshot. Every instance receives
  fresh macro/folder/step/signature identities and independent dependencies/assets;
  all copied hotkeys are cleared. Sources, templates, and instances never maintain
  live links.
- **Record** captures keyboard, mouse, timing, and optional window context into a
  transient Recording Review; it never inserts or saves behind your back. Pause,
  markers, and annotations preserve the timeline, cleanup suggestions remain
  optional, and **Play All/Selected Range** use the normal runtime without
  publishing the draft. **Apply** performs the single anchored insertion.
- Choose physical **Key** actions for shortcuts, navigation, sided modifiers, and
  scan-code-sensitive input. Choose Unicode **Text** for typed characters and
  content. Clipboard/UI-control observations used for suggestions are transient
  and are cleared when Review closes.
- Send Keys exposes **Key Press**, **Key Down**, **Key Up**, **Hotkey**, and
  **Text**. Use Hotkey for `Ctrl+V`, `Ctrl+Shift+S`, or `Shift+F1`; use
  **Key Down Shift** and a later **Key Up Shift** for an explicit hold; use
  **Text** for Unicode content.

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
* Screen Draw settings (`plugin_settings.screen_draw.*`)

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
* `mkmacros.json` — MkMacro documents
* `mkmacro_templates.json` — versioned user-created MkMacro templates
* `mkmacro_assets/` — images referenced by MkMacro documents and packages
* `todo.json` — todo list
* `alarms.json` — timers/alarms
* `history.json` — command history
* `history_pins.json` — pinned history items
* `usage.json` — usage scoring data
* `clipboard_history.json` — clipboard history
* `calc_history.json` — calculator history
* `regex_history.json` — bounded regex patterns/flags, beside the configured settings file
* `regex_presets.json` — explicitly saved regex presets, beside the configured settings file
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

## Data safety and recovery

Multi Launcher allows only one running process to own a given application data directory. Important user-authored stores use validated, atomic replacement: a missing or intentionally empty store may initialize normally, but malformed or unreadable existing data is retained and reported instead of being silently replaced with defaults. File watchers likewise keep the last valid in-memory snapshot until a later valid file update arrives.

Use `data` or `data health` to open **Data & Recovery**. The interface scans only when opened or refreshed; backup, health, and recovery preparation run on a bounded background worker. It shows each known store's path, ownership, backup eligibility, and health without displaying stored content. `data folder` opens the application data directory, and **Copy diagnostics** copies metadata and error summaries only.

`data backup` creates a snapshot under the application data directory's `backups` folder. Snapshots include eligible application-owned critical data and a manifest. The newest five recognized snapshots are retained. Private or replaceable histories, runtime state, logs, and configured external data locations are excluded by default; unknown folders under `backups` are not pruned.

Restore and reset are explicit, confirmed, staged operations. They do not replace live data while the current process is using it. The selected operation is validated, the current destination is preserved with a reasoned backup where applicable, and the change is applied before normal data loading on the next launch. A restart is therefore required. If validation or installation fails, the pending instruction and live destination remain available for diagnosis or retry.

---

## Building

### Requirements

* Rust stable toolchain
* Windows (recommended; several features use Win32/UI Automation)

### Build and run the launcher

Ordinary debug and release builds select `multi_launcher` as the only application
binary. Cargo still builds its shared library, dependencies and build script:

```powershell
cargo build
cargo build --release
cargo run
```

The three developer smoke and acceptance binaries require the nondefault
`dev-smoke-binaries` feature and remain explicitly selectable:

```powershell
cargo build --features dev-smoke-binaries --bin coordinate_tool_smoke
cargo build --features dev-smoke-binaries --bin passive_overlay_smoke
cargo build --features dev-smoke-binaries --bin radial_acceptance
```

To run one, use `cargo run --features dev-smoke-binaries --bin <name>` and
place its arguments after `--`. Add `--release` when needed. Tests and benches
retain normal Cargo discovery; the developer feature is not required for them.

### Development checks and tests

The productivity expansion's [checkpoint index and verification evidence](docs/plans/productivity_expansion_delivery.md)
record the implemented feature scope, focused checks, and native UI limitations.

Choose the command for the work you need:

| Purpose | Command |
| --- | --- |
| Quick library type check | `cargo check --lib` |
| Reduced-symbol launcher | `cargo build --profile fast-dev --bin multi_launcher` |
| Reduced-symbol library tests | `cargo nextest run --cargo-profile fast-dev --lib -E 'test(history_prepare_)'` |
| Release-like local iteration | `cargo build --profile iteration --bin multi_launcher` |
| Full-debug launcher | `cargo build --bin multi_launcher` |
| Full-debug library tests | `cargo nextest run --lib -E 'test(history_prepare_)'` |
| Canonical production launcher | `cargo build --release --bin multi_launcher` |

The optional profiles use separate `target/fast-dev` and `target/iteration`
caches. Default dev/test artifacts use `target/debug`; production artifacts use
`target/release`. `fast-dev` keeps line tables with less variable information.
`iteration` trades optimization for local build speed and inherits release's
safety-check defaults. Use dev/test when debug assertions are needed and
canonical release before distribution. See [measured tradeoffs](dev/README.md).

Select the Cargo target before filtering test names. For example:

```powershell
cargo nextest run --lib -E 'test(history_prepare_)'
cargo nextest run --test history
```

Cargo environment variables and configuration can override manifest settings.
`CARGO_PROFILE_*`, `CARGO_INCREMENTAL`, Rust flags and Cargo configuration can
change the effective build; neither the manifest nor the workflow helper
prevents intentional overrides. Track B used guarded settings and captured
compiler invocations. Reproduce measurements in an equivalent environment.

`dev/track_b/cargo.ps1` provides fixed launcher and test-target presets. For
example, pass Nextest filters as an explicit PowerShell array:

```powershell
& .\dev\track_b\cargo.ps1 -Preset release
& .\dev\track_b\cargo.ps1 -Preset debug
& .\dev\track_b\cargo.ps1 -Preset lib-test -CargoArguments @('-E', 'test(history_prepare_)')
& .\dev\track_b\cargo.ps1 -Preset integration-test -TestTarget history
& .\dev\track_b\cargo.ps1 -Preset release -PrintCommand
```

`-PrintCommand` shows the native executable, argument array, and repository
working directory without running Cargo. Use Cargo directly for other package,
target, or Cargo compiler-profile selections. The project uses `rdev` and may require the
`unstable_grab` feature for global input capture in some environments. Some
plugins depend on Windows-specific APIs such as window management and browser
tab activation.

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

