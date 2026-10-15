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

## M1-A resolved graph

The manifest now selects the released 0.36.2 egui family: `eframe`, `egui_extras`, `egui`, `egui-toast` 0.22.0, and `egui_commonmark` 0.25.0. Direct `winit` is 0.30.13 and `raw-window-handle` remains 0.6. The resolved lockfile SHA-256 is `15edbfc05ac4d528c5ce63e96c69236d302ecd07c9caa88c109e04fd14a5b21a`.

`eframe` disables defaults and explicitly enables `accesskit`, `default_fonts`, `glow`, `links`, `wayland`, `web_screen_reader`, and `x11`. Its new default `wgpu` feature is absent from the resolved eframe feature receipt. No `winit/default` dependency-of-dependency feature is declared; the direct `winit = "0.30.13"` dependency retains its own defaults, including `rwh_06`, X11, and Wayland. The inverse tree has one `egui v0.36.2` family, and one `raw-window-handle v0.6.2` is resolved for the typed-handle callers.

The `egui_extras` 0.27.2 manifest's default was `dep:mime_guess2`; 0.36.2 still maps its default to `dep:mime_guess2`, and its explicit `file` feature maps to the same dependency. The manifest disables extras defaults and enables `file` explicitly to preserve that behavior. CommonMark's former defaults are retained explicitly as `load-images` and `pulldown_cmark`; `load-images` enables extras `image` and `file`. The resolved graph therefore contains image 0.24.9 for existing application code and image 0.25.6 for the new extras image loader. Decoder-format parity on image 0.25 remains an M1-D decision; this graph checkpoint does not claim it.

The lockfile update selected 160 packages compatible with the installed Rust 1.99 toolchain. Remaining duplicates in [`m1a-cargo-tree-duplicates.txt`](receipts/m1a-cargo-tree-duplicates.txt), including image 0.24/0.25 and several platform-support families, are not duplicate egui types. Feature/inverse receipts are [`m1a-cargo-tree-egui-features.txt`](receipts/m1a-cargo-tree-egui-features.txt), [`m1a-cargo-tree-eframe-features.txt`](receipts/m1a-cargo-tree-eframe-features.txt), [`m1a-cargo-tree-extras-features.txt`](receipts/m1a-cargo-tree-extras-features.txt), [`m1a-cargo-tree-winit-features.txt`](receipts/m1a-cargo-tree-winit-features.txt), and [`m1a-cargo-tree-raw-window-handle-features.txt`](receipts/m1a-cargo-tree-raw-window-handle-features.txt). Each was generated with the direct Cargo 1.99.0 binary and `--locked`; each command exited 0.

The Windows `cargo check --all-targets` resolved and checked this graph but exited 101 on the expected source/API backlog. The unchanged source is not an accepted or runnable migration. The raw output is preserved at `target/egui-modernization/m1a/cargo-check.log`; the categorized inventory is in [m1a-api-errors.md](m1a-api-errors.md). No application source, edition, project feature, Windows API feature, profile, or `rdev` pin was changed in M1-A.
