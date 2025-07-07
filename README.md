# Multi Launcher

Multi Launcher is a lightweight application launcher built with Rust and `eframe`.
It supports configurable hotkeys, basic plugin architecture and file indexing to
quickly open applications or files.

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
    "system",
    "history",
    "help"
  ],
  "debug_logging": false,
  "offscreen_pos": [2000, 2000],
  "window_size": [400, 220],
  "query_scale": 1.0,
  "list_scale": 1.0
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

`offscreen_pos` specifies where the window is moved when hiding it. Choose
coordinates outside the visible monitor area so the window stays accessible but
off-screen. The default is `[2000, 2000]`.

`window_size` stores the size of the launcher window when it was last closed.
The window is restored to this size on the next start. The default is
`[400, 220]` if the value is missing.

`query_scale` and `list_scale` control the size of the search field and the results list separately. Values around `1.0` keep the default look while higher numbers enlarge the respective element up to five times.

If you choose `CapsLock` as the hotkey, the launcher suppresses the normal
CapsLock toggle **when compiled with the `unstable_grab` feature enabled**.
Press `Shift`+`CapsLock` to change the keyboard state while the application is
running.

## Plugins

Built-in plugins and their command prefixes are:

- Google web search (`g rust`)
- RuneScape Wiki search (`rs item` or `osrs item`)
- Calculator (`= 2+2`)
- Clipboard history (`cb`)
- Bookmarks (`bm add <url>` or `bm rm <pattern>`)
- Folder shortcuts (`f`, `f add <path>`, `f rm <pattern>`)
- Shell commands (`sh echo hi`)
- System actions (`sys shutdown`)
- Search history (`hi`)
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
The bookmarks plugin uses the `bm` prefix. Use `bm add <url>` to save a link and
`bm rm <pattern>` to remove one via fuzzy search.
### Security Considerations
The shell plugin runs commands using the system shell without sanitising input. Only enable it if you trust the commands you type. Errors while spawning the process are logged.
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

## Manual Test Plan

1. Build and run the project with `cargo run`.
2. **Before** the launcher window appears, press the configured hotkey once.
3. Observe the log output. There should be a message indicating a visibility
   change was queued.
4. When the GUI finishes initialising, it should immediately apply the queued
   visibility change and the window becomes visible. A log entry confirms this.
5. Press the hotkey again to ensure normal toggling after start-up.
