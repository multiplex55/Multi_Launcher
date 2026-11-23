# gpui-pre-windows 0.3.12 local patch

This directory vendors the normalized crates.io package `gpui-pre-windows`
version `0.3.12`. Its upstream Zed source revision is
`d2a4fb6ded7c470be25d1541281da3882495b65b`. The package archive SHA-256 is
`7201762420222d5b5ce5bdf0056996199212a3349bfb2457a3a0238c688681f3`.

The application and `gpui-pre-platform` resolve the Windows backend at the
same exact package version, `0.3.12`; the repository lockfile records that one
version through this path patch. The archive's normalized `Cargo.toml`,
`build.rs`, `LICENSE-APACHE`, and complete `src/` tree (including shader
inputs) are retained. Cargo cache markers, `Cargo.toml.orig`, and the package's
generated standalone `Cargo.lock` are omitted.

The local changes are limited to:

- `src/platform.rs` passes the original `MSG.hwnd` into the existing keydown
  translation boundary used by both Windows message pumps.
- `src/events.rs` captures the actual HWND, virtual key, scan code, extended
  bit, and pre-normalization modifiers for a keydown. A thread-local RAII scope
  exposes the receipt only during that input callback, hides it in nested
  absent/key-up callbacks, restores its prior scope on return, and provides a
  one-shot accessor.
- `src/gpui_windows.rs` re-exports only the receipt type and one-shot accessor.

No additional keyboard hook, queue, text filtering, or input logging is added.
