# Multi Launcher

Multi Launcher is a lightweight application launcher built with Rust and `eframe`.
It supports configurable hotkeys, basic plugin architecture and file indexing to
quickly open applications or files.

## Building

Requirements:
- Rust toolchain
- On Linux you may need X11 development libraries (`libxcb` and friends).

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
`Hotkey listener failed`. Lack of permissions is a common cause on Linux
(running under `sudo` or granting access to `/dev/input` may be required).
When using `CapsLock` as the hotkey you almost always need to build with
`--features unstable_grab` so the listener can grab the key.

## Settings

Create a `settings.json` next to the binary to customise the launcher. The
default hotkey is `F2`. To use a different key, set the `hotkey` value in
`settings.json` as shown below:

```json
{
  "hotkey": "F2",
  "quit_hotkey": "Shift+Escape",
  "index_paths": ["/usr/share/applications"],
  "plugin_dirs": ["./plugins"],
  "debug_logging": false
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

If you choose `CapsLock` as the hotkey, the launcher suppresses the normal
CapsLock toggle **when compiled with the `unstable_grab` feature enabled**.
Press `Shift`+`CapsLock` to change the keyboard state while the application is
running.

## Window Visibility

Press the launcher hotkey to toggle the window. When hidden the window is
minimized rather than closed. Triggering the hotkey again restores the launcher
at the current mouse cursor position and focuses the search box.
Pressing the hotkey again after a minimize restores the window beneath the mouse.

## Plugins

Built-in plugins provide Google web search (`g query`) and an inline calculator
(using the `=` prefix). Additional plugins can be added by building separate
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

## Editing Commands

The launcher stores its custom actions in `actions.json`. While running the
application you can manage this list using the **Edit Commands** dialog. Open
the launcher with the configured hotkey and press the *Edit Commands* button to
add or remove entries. A **Browse** button next to the path field opens a native
file dialog so you can select the executable or file interactively. Changes are
saved back to `actions.json` immediately.

## Packaging

The project can be compiled for Windows, macOS and Linux using `cargo build
--release`. Afterwards bundle the binary for distribution (e.g. using `cargo
bundle` on macOS or `cargo wix` on Windows).

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
