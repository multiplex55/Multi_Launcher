# Local eframe patch

This directory vendors `eframe` 0.36.2 with a narrow native lifecycle extension
used by Multi Launcher. The original crate was copied from the crates.io package
archive with checksum
`55ad41fac6e3149abb4866e93df37dbd33baa95db525c3368c944584e95a5dc2`.
Its `.cargo_vcs_info.json` identifies upstream commit
`49682f8baa058bf49e011035cfbd6e825f88a5ef` at `crates/eframe`. Those values
identify the unmodified upstream archive and source revision; they are not a
checksum or revision for this locally patched tree.

The local source changes are limited to:

- `src/epi.rs`: adds opt-in app hooks for hidden logic dispatch, stale hidden
  input invalidation, hidden-root UI bootstrap, and CPU-side UI-frame completion.
- `src/native/epi_integration.rs`: dispatches the hooks, clears stale retained
  input only for apps that opt in, retains viewport identity and commands while
  discarding unconsumed hidden action input, and exposes one shared visibility
  predicate for native renderers. Focused lifecycle tests live here.
- `src/native/glow_integration.rs` and `src/native/wgpu_integration.rs`: use the
  shared predicate so an opted-in hidden-root bootstrap can run the root UI
  pass without making the native root visible or forcing hidden child UI.
- `src/native/glow_integration.rs`: fulfill screenshot actions requested by an
  immediate native child after its paint and before buffer swap, while leaving
  other queued native actions untouched. A focused test checks the child
  viewport identity, screenshot user data, and action preservation.

The hooks preserve existing behavior by default: `logic_only` forwards to
`logic`, and all three policy/completion hooks are no-ops or return `false`.
The hidden-root request exists to register a child viewport requested by hidden
logic. It does not show, focus, or paint the root window. The completion hook
runs after `run_ui` has returned its final output on the CPU, before
tessellation, GPU submission, or presentation.

Opted-in input discard clears retained keys, modifiers, pointer buttons,
scroll, files, events, and stale frame deltas before hidden logic runs, and
cancels an active egui drag through `Context::stop_dragging`. The absolute
`InputState.time` is retained so the next visible pass remains monotonic.
Viewport metadata and commands remain available to hidden logic, including
close-request cancellation.

Focused tests were run on the vendored crate with the repository's locked
dependency graph. In this offline environment, the run used a short-lived
`vendor/Cargo.toml` containing `[workspace]` and `members = ["eframe"]`, plus a
copy of the repository `Cargo.lock` at `vendor/Cargo.lock`. With those temporary
files in place, the command was:

```text
cargo test --manifest-path vendor/Cargo.toml -p eframe --no-default-features --features default_fonts,glow,x11,wayland --lib --locked --offline
```

The temporary workspace manifest and lockfile were removed after the run and
are not part of the patch. The test output and exit code are recorded in
`target/egui-modernization/m1b-vendor-tests.log` (4 passed, 0 failed).
