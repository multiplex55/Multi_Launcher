# Multi Launcher

Multi Launcher is a lightweight application launcher built with Rust and `eframe`.
It supports configurable hotkeys, basic plugin architecture and file indexing to quickly open applications or files.

## Building

Requirements:
- Rust toolchain
- On Linux you may need X11 development libraries (`libxcb` and friends).

```
cargo build --release
```

## Settings

Create a `settings.json` next to the binary to customise the launcher. Example:

```json
{
  "hotkey": "CapsLock",
  "index_paths": ["/usr/share/applications"]
}
```

The `hotkey` accepts any key variant defined by the [`rdev`](https://docs.rs/rdev) crate such as `F1`, `F12`, `CapsLock`, `KeyA` or `LeftArrow`. Names are case insensitive. If an unknown key is specified, the application returns an error on startup.

## Plugins

Built-in plugins provide Google web search (`g query`) and an inline calculator (using the `=` prefix). Additional plugins can be added by extending the `Plugin` trait.

## Packaging

The project can be compiled for Windows, macOS and Linux using `cargo build --release`. Afterwards bundle the binary for distribution (e.g. using `cargo bundle` on macOS or `cargo wix` on Windows).
