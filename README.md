# Multi Launcher
<img width="480" height="480" alt="Green_MultiLauncher" src="https://github.com/user-attachments/assets/8a68f544-536c-4eb5-8c0a-c5ef43e21c2d" />

Multi Launcher is a lightweight application launcher built with Rust and `eframe`.
It supports configurable hotkeys, basic plugin architecture and file indexing to
quickly open applications or files.


## Use Cases

- Launch installed applications or custom commands from anywhere using a single hotkey.
- Search your clipboard history or saved bookmarks to quickly paste or open items.
- Run shell commands without opening a terminal.
- Perform web searches or look up documentation directly.
- Jump to frequently used folders with the folders plugin.
- Set timers or alarms from the launcher. Type `timer` or `alarm` and press
  <kbd>Enter</kbd> to open the creation dialog.
- Keep track of quick todo items or notes.
- Insert saved text snippets, add new ones with `cs add <alias> <text>` or edit them with `cs edit`.


## Building

Requirements:
- Rust toolchain

```
cargo build --release
```

To capture `CapsLock` reliably and suppress its normal toggle, build with the
optional `unstable_grab` feature. Without this feature some systems may ignore
the `CapsLock` hotkey:

```
cargo build --release --features unstable_grab
```

This feature is defined in `Cargo.toml` and enables the underlying `rdev`
capability used to grab keyboard events.

To show a system notification when a timer or alarm fires, build with the
`notify` feature. This pulls in the optional `notify-rust` dependency:

```
cargo build --release --features notify
```

For debugging, enable **Debug logging** in the settings window. When this
option is active, you can further adjust the verbosity by setting the
`RUST_LOG` environment variable before running the program:

```bash
RUST_LOG=info cargo run --release --features unstable_grab
```

If hotkeys do nothing, check the output for warnings starting with
`Hotkey listener failed`. When using `CapsLock` as the hotkey you almost
always need to build with `--features unstable_grab` so the listener can
grab the key.

## Settings

Multi Launcher automatically creates a `settings.json` file next to the binary
on first run. Edit this file or open the **Settings** window to customise the
launcher. The default hotkey is `F2`. To use a different key, set the `hotkey`
value as shown below:

```json
{
  "hotkey": "F2",
  "quit_hotkey": "Shift+Escape",
  "help_hotkey": "F1",
  "index_paths": ["C:/ProgramData/Microsoft/Windows/Start Menu/Programs"],
  "plugin_dirs": ["./plugins"],
  "enabled_plugins": [
    "web_search",
    "calculator",
    "unit_convert",
    "clipboard",
    "bookmarks",
    "folders",
    "shell",
    "runescape_search",
    "weather",
    "system",
    "timer",
    "history",
    "help"
  ],
  "enabled_capabilities": {"folders": ["search", "show_full_path"]},
  "enable_toasts": true,
  "fuzzy_weight": 1.0,
  "usage_weight": 1.0,
  "debug_logging": false,
  "offscreen_pos": [2000, 2000],
  "window_size": [400, 220],
  "query_scale": 1.0,
  "list_scale": 1.0,
  "history_limit": 100,
  "clipboard_limit": 20,
  "preserve_command": false,
  "follow_mouse": true,
  "static_location_enabled": false,
  "static_pos": [0, 0],
  "static_size": [400, 220],
  "screenshot_dir": "C:/Users/YourName/Pictures",
  "screenshot_save_file": true
}
```

The `hotkey` value accepts a base key with optional modifiers separated by `+`.
Examples include `"Ctrl+Shift+Space"` or `"Alt+F1"`. Supported modifiers are
`Ctrl`, `Shift` and `Alt`. Valid keys cover alphanumeric characters, function
keys (`F1`-`F12`) and common keys like `Space`, `Tab`, `Return`, `Escape`,
`Delete`, arrow keys and `CapsLock`.

`quit_hotkey` can be set to another key combination to close the launcher from
anywhere. If omitted, the application only quits when the window is closed
through the GUI.
`help_hotkey` toggles a quick overlay listing commands. Set it to `null` or
uncheck the *Enable help hotkey* box in the Settings window to disable this
shortcut.

`offscreen_pos` specifies where the window is moved when hiding it. Choose
coordinates outside the visible monitor area so the window stays accessible but
off-screen. The default is `[2000, 2000]`.

`window_size` stores the size of the launcher window when it was last closed.
The window is restored to this size on the next start. The default is
`[400, 220]` if the value is missing.

When `follow_mouse` is `true` the window is centered on the mouse cursor
whenever it becomes visible. To keep the launcher at a specific
position instead, set `follow_mouse` to `false` and enable
`static_location_enabled`. Provide the desired coordinates in `static_pos`
and optionally a fixed size via `static_size`. The **Settings** window now
includes a *Snapshot* button to capture the current window position and size
for these fields.

`query_scale` and `list_scale` control the size of the search field and the results list separately. Values around `1.0` keep the default look while higher numbers enlarge the respective element up to five times.
`enable_toasts` controls short pop-up notifications when saving settings or commands. Set it to `false` to disable these messages.
`fuzzy_weight` and `usage_weight` adjust how results are ranked. The fuzzy weight multiplies the match score while the usage weight favours frequently launched actions. Setting `"fuzzy_weight": 0` in `settings.json` forces case-insensitive substring matching across all plugins.
Command aliases are checked first and a matching alias ranks above other results.

Example: typing `test` will only list entries containing `test`. If an alias matches this word it appears before the other results.
`history_limit` defines how many entries the history plugin keeps.
`clipboard_limit` sets how many clipboard entries are persisted for the clipboard plugin.
`preserve_command` keeps the typed command prefix (like `bm add` or `f add`) in the search field after running an action.
`enabled_capabilities` maps plugin names to capability identifiers so features can be toggled individually. The folders plugin, for example, exposes `show_full_path`.
`screenshot_dir` sets the directory used when saving screenshots. If omitted, the application uses a `MultiLauncher_Screenshots` folder in the current working directory.
`screenshot_save_file` determines whether screenshots copied to the clipboard are also written to disk. The default is `true`.


If you choose `CapsLock` as the hotkey, the launcher suppresses the normal
CapsLock toggle **when compiled with the `unstable_grab` feature enabled**.
Press `Shift`+`CapsLock` to change the keyboard state while the application is
running. The launcher only responds when `CapsLock` is pressed on its own; any
other modifier keys will simply toggle the caps lock state without showing the
window.

## Plugins

```mermaid
flowchart LR
    A[Hotkey pressed] --> B(Show launcher)
    B --> C{User query}
    C --> D[Plugin manager searches]
    D --> E[Display results]
    E --> F[Run action]
```

```mermaid
graph TD
    S[Startup] --> B1[Register built-in plugins]
    S --> B2[Load plugin_dirs]
    B2 --> L[Load dynamic plugins]
    B1 --> PM[Plugin manager ready]
    L --> PM
```

Built-in plugins and their command prefixes are:

- Google web search (`g rust`)
- Calculator (`= 2+2`)
- Unit conversions (`conv 10 km to mi`)
- Drop rate calculator (`drop 1/128 128`)
- RuneScape Wiki search (`rs item` or `osrs item`)
- YouTube search (`yt rust`)
- Reddit search (`red cats`)
- Wikipedia search (`wiki rust`)
- Clipboard history (`cb`) - entries persist in `clipboard_history.json`. Use `cb list` to show all entries or `cb clear` to wipe them. Right-click items to edit or delete.
- Bookmarks (`bm add <url>`, `bm rm [pattern]` or `bm list`)
- Folder shortcuts (`f`, `f add <path>`, `f rm <pattern>`)
- System actions (`sys shutdown`)
- Process list (`ps`), providing "Switch to" and "Kill" actions
- System information (`info`, `info cpu`, `info mem`, `info disk`)
- Shell commands (`sh echo hi`)
- Search history (`hi`)
- Quick notes (`note add <text>`, `note list`, `note rm <pattern>`)
- Todo items (`todo add <task> p=<n> #tag`, `todo list`, `todo rm <pattern>`, `todo pset <idx> <n>`, `todo tag <idx> #tag`, `todo clear`)
- Text snippets (`cs`, `cs list`, `cs rm`, `cs add <alias> <text>`, `cs edit`)
- Recycle Bin cleanup (`rec`)
- Temporary files (`tmp`, `tmp new [name]`, `tmp open`, `tmp clear`, `tmp list`, `tmp rm`)
- ASCII art (`ascii text`)
- Saved apps (`app <filter>` or just `app`)
- Volume control (`vol 50`) *(Windows only)*
- Brightness control (`bright 50`) *(Windows only)*
- Task Manager (`tm`) *(Windows only)*
- Window management (`win <title>` to switch or close) *(Windows only)*
- Screenshot capture (`ss`, `ss clip`) *(Windows only)*
- Command overview (`help`)
- Timers and alarms (`timer add 5m tea`, `timer add 1:30`, `alarm 07:30`). Type `timer` or
  `alarm` and press <kbd>Enter</kbd> to open the creation dialog. Use `timer list` to view
  remaining time or `timer rm` to remove timers. Alarms are stored in `alarms.json`
  and reload automatically when the launcher starts. The timer plugin exposes a
  `completion_dialog` capability that toggles pop-up notifications when a timer
  completes.
- Weather lookup (`weather Berlin`)

### Screenshot Plugin (Windows only)
Use `ss` to capture the active window, a custom region or the whole desktop. Add `clip` to copy the result to the clipboard.
Screenshots are saved in a `MultiLauncher_Screenshots` folder in the current working directory by default or the path set in `screenshot_dir`.
Set `screenshot_save_file` to `true` to always keep a file when copying to the clipboard.

When the search box is empty the launcher shows these shortcuts along with `app <alias>` entries for saved actions.

On Windows the optional `vol` and `bright` plugins allow changing system volume
and display brightness. These plugins are stubbed on other platforms and simply
return no results.

Selecting a clipboard entry copies it back to the clipboard. Type `help` and press <kbd>Enter</kbd> to open the command list. The help window groups commands by plugin name and can optionally display example queries. Additional plugins can be added by building
shared libraries. Each plugin crate should be compiled as a `cdylib` and export
a `create_plugin` function returning `Box<dyn Plugin>`:

```rust
#[no_mangle]
pub extern "C" fn create_plugin() -> Box<dyn Plugin> {
    Box::new(MyPlugin::default())
}
```

Place the resulting library file in one of the directories listed under
`plugin_dirs` in `settings.json`.

Plugins can be enabled or disabled from the **Settings** window. The list of
active plugins is stored in the `enabled_plugins` section of `settings.json`.
The **Plugin Settings** dialog provides a graphical way to manage plugin directories, enable or disable plugins and toggle capabilities like `show_full_path`.

Changes take effect immediately once the dialog is closed. Use this window to
enable additional plugins, such as a dynamic `envvar` plugin that exposes
environment variables through the `env` prefix. After placing the compiled
plugin in one of the configured directories simply check its box in the dialog
and close it to reload the plugin list.


Example:

```json
{
  "enabled_plugins": [
    "web_search",
    "calculator",
    "clipboard",
    "bookmarks",
    "folders",
    "shell",
    "runescape_search",
    "system",
    "processes",
    "weather",
    "timer",
    "envvar",
    "history",
    "help"
  ]
}
```
The folders plugin recognises the `f` prefix. Use `f add <path>` to add a folder
shortcut and `f rm <pattern>` to remove one via fuzzy search. Custom entries can
be aliased by right clicking them in the results list. Hovering a folder result
shows its full path. A plugin setting "show full path always" controls whether
the full path is displayed next to an alias or only as a tooltip.
The bookmarks plugin uses the `bm` prefix. Use `bm add <url>` to save a link,
`bm rm` to list and remove bookmarks (optionally filtering with a pattern) or
`bm list` to show all bookmarks. Searching with `bm <term>` matches both URLs
and aliases.
The envvar plugin uses the `env` prefix and shows matching environment variables
and their values.
The system information plugin uses the `info` prefix. Type `info` to show CPU,
memory and disk usage or `info cpu` for a single metric.
### Security Considerations
The shell plugin runs commands using the system shell without sanitising input. Only enable it if you trust the commands you type. Errors while spawning the process are logged.
Type `sh` in the launcher to open the shell command editor for managing predefined commands. Saved commands can also be added with `sh add <name> <command>`, removed via `sh rm <pattern>` or listed with `sh list`.
## Editing Apps
The launcher stores its custom actions in `actions.json` next to the
executable. This file is created automatically the first time you save an
app. While running the application you can manage this list through
**Edit Apps**. Open the launcher with the configured hotkey and choose
*Edit Apps* from the menu.
Use the **New App** button to open the *Add App* dialog where you enter
a label, description and the executable path. Enable **Add arguments** to supply
extra command line parameters. The **Browse** button lets you
select the file interactively. Existing entries can be edited via the **Edit**
button or by right clicking an app in the results list and choosing *Edit
App*. Apps can also be removed from the list. All changes are written to
`actions.json` immediately.

Type `app <filter>` in the launcher to search these saved entries. Typing `app`
alone lists all saved apps.

## Packaging

The project can be compiled for Windows using `cargo build --release`.
Afterwards bundle the binary for distribution using a Windows packaging tool
such as `cargo wix`.
When compiled this way the executable is built with `windows_subsystem = "windows"`, which prevents an extra console window from appearing.

## Troubleshooting

When diagnosing hotkey issues it can be helpful to enable info level logging:

```bash
RUST_LOG=info cargo run
```
## Tips

- Press the help hotkey (F1 by default) to display a quick list of available commands.
- Right click a folder result to set a custom alias for easier access.
- Use the *Snapshot* button in Settings when adjusting static window placement.
- Searches are case-insensitive and also match on command aliases.
- Tweak `fuzzy_weight` and `usage_weight` if you want results to favour name matches or past usage differently.


## Manual Test Plan

1. Build and run the project with `cargo run`.
2. **Before** the launcher window appears, press the configured hotkey once.
3. Observe the log output. There should be a message indicating a visibility
   change was queued.
4. When the GUI finishes initialising, it should immediately apply the queued
   visibility change and the window becomes visible. A log entry confirms this.
5. Press the hotkey again to ensure normal toggling after start-up.

On Windows the launcher also checks which virtual desktop the window belongs to
whenever it becomes visible. If it is on another desktop it is automatically
moved to the active one before being shown.
