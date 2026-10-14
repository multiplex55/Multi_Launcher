# Current migration decisions — revised policy2026-10-14

Authority: [PERFORMANCE_ACCEPTANCE_ADDENDUM.md](PERFORMANCE_ACCEPTANCE_ADDENDUM.md). Retain current integrated work and explicit working Glow; renderer competition deferred. Live correctness closeout then functional M2-D and immediate Track B. Local-only publication remains unchanged. Earlier source/graph/gate statements below describe their historical checkpoints.

## Historical record — unchanged, not operative

# M0-A preflight decisions and unresolved items

**Checkpoint publication:** User explicitly chose “Keep checkpoints local” after
automatic approval review rejected the initial push of `050a922f`. Do not push
subsequent checkpoints without a new user instruction. The first checkpoint is
preserved locally; this does not block implementation or review.

1. **Source basis:** The mission objective says to use current repository HEAD as the baseline; this run uses `72a01bd5451060d3f6b640b83acd081d7db76e77` accordingly. The companion contract also names an archive for comparison, but it was not present in the supplied attachment inventory or available repository paths. Keep archive-to-HEAD identity unverified until the parent resolves its clarification request.
2. **Working revision:** Parent created `egui-modernization` from current HEAD `72a01bd5451060d3f6b640b83acd081d7db76e77` after the fetch check. The scoped preflight made no Git mutations or commits.
3. **Local compiler:** Rust/Cargo 1.99.0 on `x86_64-pc-windows-msvc` meets the contract's 1.95 minimum. `rustup show` could not run because the shim received access denied while handling `rustup-init.exe`; direct stable toolchain binaries were used for tree inspection. No remote Windows builder is configured or supplied; the local Windows MSVC host is the migration host.
4. **Dependency target:** The current graph is the locked 0.27.2 family. The parent reports successful metadata lookups for eframe 0.36.2 (Rust minimum 1.95), egui-toast 0.22.0, and egui_commonmark 0.25.0 (Rust minimum 1.95), and confirmation from the official docs.rs page. Candidate wrapper compatibility is still unaudited; no dependency edits belong to M0-A.
5. **Renderer:** eframe's current default feature set includes `glow`; the tree contains `egui_glow`. Preserve this as the pre-migration comparison point, then explicitly verify backend feature selection during M1/M2. This inventory does not establish runtime renderer selection or performance.
6. **Historical performance material:** `docs/performance/` is absent at HEAD. The selected historical Track C protocol documents can inform later measurement design, but their old data must not be treated as the egui baseline and the directory was not restored.
7. **Gate state:** The parent-owned all-target `cargo check --locked --all-targets --offline` passed with exit 0 in 4m11s. Original full Nextest diagnostics completed with 6,439 passes and three preexisting stale fixture failures. The reviewed test-only correction passed its four focused tests; original failed receipts remain preserved. No Windows-native pass, screenshot pass, or performance parity claim is made. M0-A remains `in_progress` while tests, archive clarification, and checkpoint handling remain open.

## M1-A dependency graph decisions

1. **UI family and renderer:** Align eframe and extras to 0.36.2, toast to 0.22.0, CommonMark to 0.25.0, and direct winit to 0.30.13. Disable eframe's defaults and explicitly retain accessibility, default fonts, links, Glow, Wayland, web screen reader, and X11. Do not enable Wgpu as a migration default; its renderer comparison remains M2-C.
2. **Event loop and handles:** Keep direct winit because `src/main.rs` uses `EventLoopBuilderExtWindows::with_any_thread`; direct winit defaults unify with eframe's 0.30.13 dependency and select `rwh_06`. Keep direct `raw-window-handle = "0.6"`; no adapter or raw FFI change is part of this checkpoint.
3. **Wrapper features:** Disable wrapper defaults where they are not needed. Preserve `egui_extras`' old default MIME detection explicitly with its `file` feature (`default` and `file` both activate `mime_guess2` in the resolved versions). Preserve CommonMark's prior `load-images` and `pulldown_cmark` defaults explicitly. Keep toast 0.22.0 on the same egui family.
4. **Image boundary:** CommonMark's `load-images` activates extras' image 0.25.6 loader alongside the app's existing image 0.24.9 dependency. Keep image 0.24 decoding unchanged in M1-A; inventory required image 0.25 formats for the M1-D integration before claiming loader parity.
5. **Resolution and diagnostics:** Cargo updated the lockfile during Windows `cargo check --all-targets`. All duplicate, inverse, and feature-tree commands passed against the locked graph. The all-target compile exited 101 with the expected egui/eframe API errors, which are inventoried without fixes in [m1a-api-errors.md](m1a-api-errors.md). This checkpoint establishes dependency resolution only; it does not claim a runnable application.
