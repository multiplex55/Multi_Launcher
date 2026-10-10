# Track B frozen baseline

Initial branch: `cargo-build-perf`
Initial HEAD: `6a6be519685652a282f7d0a411008601f158d075`
Initial subject: `6a6be519 md`
Initial working tree: clean (git status --short produced no entries).
Remote: origin, GitHub multiplex55/Multi_Launcher (read-only provenance; no fetch/pull/push).
Recorded before branch creation. Implementation branch: `build-optimization`, from the exact initial HEAD.

## Environment verified

- Windows MSVC host: x86_64-pc-windows-msvc.
- rustc 1.97.1, commit 8bab26f4f68e0e26f0bb7960be334d5b520ea452, LLVM 22.1.6.
- cargo 1.97.1 (c980f4866 2026-06-30).
- cargo-nextest 0.9.135 (610eefb88762529a316373f4a50f5fd9194c3c35).
- Actual linker: NOT YET VERIFIED. LLVM version alone is not linker evidence.
- Profiles: no explicit profiles in baseline Cargo.toml; canonical release must remain unchanged.

## Measurements

B0 inventory/protocol in progress. No build measurements yet. Runtime preservation, target counts, host constraints and selected linker remain under inspection. Six-minute reported scenario is a warm-dependency source-edit or branch-switch release rebuild, not a measured no-op.

## Host and target inventory

- Intel Core i7-7700K, 4 physical cores / 8 logical processors.
- Physical memory: 34,318,487,552 bytes (approximately 32 GiB).
- Windows 10 Home 10.0.19045; Balanced power scheme.
- G: NTFS, 4,000,768,323,584 bytes; approximately 3.19 TB free at inventory.
- G: maps to WDC WD40EZRZ-19GXCB0 HDD. An SSD exists but is not the repository drive.
- Defender antivirus and real-time protection enabled; no security settings changed.
- Active toolchain stable-x86_64-pc-windows-msvc (default).
- link, lld-link and rust-lld were not found on the shell PATH; selection remains pending compilation evidence.
- No repository/ancestor or default Cargo-home config found in inspected paths. No RUSTFLAGS, RUSTC_WRAPPER, CARGO_TARGET_DIR, CARGO_BUILD_JOBS or CARGO_ENCODED_RUSTFLAGS set at initial inventory.

Cargo metadata: 78 targets = 1 library + 4 binaries + 67 integration tests + 5 benchmarks + 1 build script.
Binaries: multi_launcher, coordinate_tool_smoke, passive_overlay_smoke, radial_acceptance.
Integration tests: 65 top-level targets plus domain and plugin_queries aggregators (24/35 modules respectively).
Benchmarks: search, omni_search, todo_widget_filtering, radial_runtime, macros_search.

Track A preservation confirmed by planner source/history inspection: completed runtime commits precede HEAD; revisioned note cache, bounded history preparation, GUI geometry caches, bounded indexer worker and native effect refresh ordering remain present. This is source provenance, not a newly run runtime verification.

## Ownership and protocol

Owned detached benchmark worktree: target/track-b/baseline at initial HEAD. Raw evidence lives outside that scratch source root under ignored target/track-b. No benchmark fixture will be applied to active source. Existing primary target dependencies are reused serially (no clean/deletion); first scratch-path build is labelled cache transition, not a warm edit or no-op. Path changes may rebuild the application. All subsequent comparisons keep the same scratch source and target paths. No concurrent Cargo jobs.

Selected real edit fixtures (restore exact bytes in finally): src/color.rs RGB formatting expression; src/gui/mod.rs RADIAL_DESIGNER_WINDOW_TITLE string. They alter generated behavior only in isolated benchmarks; native acceptance runs only with originals restored. Two comparable edited samples where feasible; three cheap no-op samples. Resource fixtures separately track RC/icon/build.rs invalidation. Timing HTML measures combined compile/codegen/link unless independent process evidence supports attribution.

Git metadata and ignored target writes need sandbox escalation on this host. Initial non-escalated branch creation and log writes failed before a build ran; the successfully escalated branch/worktree operations preserved the initial HEAD. Read-only host CIM queries also needed escalation. These are permission/environment facts, not benchmark failures.
