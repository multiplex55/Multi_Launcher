# Multi Launcher

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

Create a `settings.json` next to the binary to customise the launcher. The
default hotkey is `F2`. To use a different key, set the `hotkey` value in
`settings.json` as shown below:

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
  "follow_mouse": true,
  "static_location_enabled": false,
  "static_pos": [0, 0],
  "static_size": [400, 220]
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
`fuzzy_weight` and `usage_weight` adjust how results are ranked. The fuzzy weight multiplies the match score while the usage weight favours frequently launched actions.
`history_limit` defines how many entries the history plugin keeps.
`enabled_capabilities` maps plugin names to capability identifiers so features can be toggled individually. The folders plugin, for example, exposes `show_full_path`.


If you choose `CapsLock` as the hotkey, the launcher suppresses the normal
CapsLock toggle **when compiled with the `unstable_grab` feature enabled**.
Press `Shift`+`CapsLock` to change the keyboard state while the application is
running. The launcher only responds when `CapsLock` is pressed on its own; any
other modifier keys will simply toggle the caps lock state without showing the
window.

## Plugins

Built-in plugins and their command prefixes are:

- Google web search (`g rust`)
- RuneScape Wiki search (`rs item` or `osrs item`)
- YouTube search (`yt rust`)
- Reddit search (`red cats`)
- Weather lookup (`weather Berlin`)
- Calculator (`= 2+2`)
- Clipboard history (`cb`)
- Bookmarks (`bm add <url>`, `bm rm <pattern>` or `bm list`)
- Folder shortcuts (`f`, `f add <path>`, `f rm <pattern>`)
- Shell commands (`sh echo hi`)
- System actions (`sys shutdown`)
- Process list (`ps`), providing "Switch to" and "Kill" actions
- Timers and alarms (`timer 5m tea`, `alarm 07:30`). Use `timer list` to view
  remaining time. Pending alarms are saved to `alarms.json` and resume after
  restarting the launcher. A plugin setting controls pop-up dialogs when a
  timer completes.
- Search history (`hi`)
- Quick notes (`note add <text>`, `note list`, `note rm <pattern>`)
- Command overview (`help`)

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
`bm rm <pattern>` to remove one via fuzzy search or `bm list` to show all
bookmarks. Searching with `bm <term>` matches both URLs and aliases.
### Security Considerations
The shell plugin runs commands using the system shell without sanitising input. Only enable it if you trust the commands you type. Errors while spawning the process are logged.
Type `sh` in the launcher to open the shell command editor for managing predefined commands.
## Editing Commands
The launcher stores its custom actions in `actions.json`. While running the
application you can manage this list through **Edit Commands**. Open the
launcher with the configured hotkey and choose *Edit Commands* from the menu.
Use the **New Command** button to open the *Add Command* dialog where you enter
a label, description and the executable path. Enable **Add arguments** to supply
extra command line parameters. The **Browse** button lets you
select the file interactively. Existing entries can be edited via the **Edit**
button or by right clicking a command in the results list and choosing *Edit
Command*. Commands can also be removed from the list. All changes are written to
`actions.json` immediately.

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
