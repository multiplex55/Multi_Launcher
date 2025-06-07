# Multi_Launcher

This tool lets you launch predefined actions with a global hotkey.

## Configuration

`settings.json` specifies the hotkey to open the launcher:

```json
{
  "hotkey_key": "CapsLock"
}
```

`hotkey_key` accepts any key variant defined by the [`rdev`](https://docs.rs/rdev) crate.
Examples include `F1`, `F12`, `CapsLock`, `KeyA`, `LeftArrow`, etc. The value is case
insensitive. If an unknown key name is used, the application will return an error
on startup.

## Running

Provide `actions.json` with your actions and run `cargo run`.
