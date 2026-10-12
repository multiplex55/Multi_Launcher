# M0-A dependency and API surface inventory

Inventory is from `Cargo.toml`, `Cargo.lock`, source searches, and the locked offline tree at HEAD `72a01bd5451060d3f6b640b83acd081d7db76e77`. It records the pre-migration graph only.

## egui family and active eframe features

| Crate | Manifest requirement | Locked version / observation |
|---|---|---|
| `eframe` | `0.27` (default features) | `0.27.2`; active features include `default`, `accesskit`, `default_fonts`, `glow`, `wayland`, `web_screen_reader`, and `x11` |
| `egui` | Transitive; no direct manifest dependency | One version in the UI graph: `0.27.2` |
| `egui_extras` | `0.27` | `0.27.2` |
| `egui-toast` | `0.13` | `0.13.0`, uses egui `0.27.2` |
| `egui_commonmark` | `0.15.0` | `0.15.0`, uses egui/egui_extras `0.27.2` |
| `winit` | Direct `0.29` | `0.29.15` |
| `raw-window-handle` | Direct `0.6` | Direct `0.6.2`; the existing eframe/glutin path also brings `0.5.2` |

The locked tree contains the `egui_glow 0.27.2` path. The feature tree confirms eframe's `glow` feature is active; renderer behavior is not otherwise inferred from this inventory. Raw command receipts: [`cargo-tree-duplicates.txt`](receipts/cargo-tree-duplicates.txt), [`cargo-tree-egui.txt`](receipts/cargo-tree-egui.txt), [`cargo-tree-eframe.txt`](receipts/cargo-tree-eframe.txt), and [`cargo-tree-eframe-features.txt`](receipts/cargo-tree-eframe-features.txt).

Other duplicate package versions reported by `cargo tree -d` include `image` 0.24.9/0.25.6, `raw-window-handle` 0.5.2/0.6.2, multiple `windows`/`windows-sys` families, and expected utility crates. These are inventory findings, not a declaration that each duplicate is an egui migration blocker.

The parent separately reports successful `cargo info` lookups for eframe 0.36.2, egui-toast 0.22.0, and egui_commonmark 0.25.0. The two 0.36.2-era candidates report Rust 1.95 minimums. This confirms published candidate metadata only; compatibility/API and feature parity are still M0-C work.

## Direct native API surface

- `winit` is directly used by `src/main.rs` for `EventLoopBuilderExtWindows`.
- Raw window handles are directly used in `src/window_manager.rs` and `src/visibility.rs`; `RawWindowHandle::Win32` is converted or inspected there.
- `Cargo.toml` pins `raw-window-handle = "0.6"` and `winit = "0.29"` independently of eframe's own graph. Their compatibility and continued need are migration worklist items, not preflight changes.

## Manifest controls

- Edition remains `2024`.
- `Cargo.toml` declares no `[patch]` section.
- The manifest has one git dependency: `rdev`, pinned to revision `c14f2dc5c8100a96c5d7e3013de59d6aa0b9eae2`, with `x11` enabled.
- No dependency or feature was edited during M0-A.

All requested tree commands used the direct Rust 1.99 toolchain with `--locked --offline`; each exited 0. The lockfile digest is recorded in [baseline.md](baseline.md) and [`git-state.txt`](receipts/git-state.txt).
