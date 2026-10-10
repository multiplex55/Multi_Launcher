# Multi Launcher — Track B Compilation & Build-Time Source Notes

**Execution baseline:** **`HEAD` of the currently checked-out branch in the live repository when Codex starts.** Capture its actual SHA and branch name before creating `build-optimization`. All before/after comparisons must be traceable to that SHA and a consistent build environment. **Do not change to `master`, pull, or assume the remote branch contains local work.**

**Objective:** Reduce Rust compile, code-generation, link, and Cargo Nextest *build* latency while preserving functionality, test coverage and the existing **production `release` profile behavior**. The user reports ~6 minutes for warmed release builds after Rust source edits or branch switching, not for a provably unchanged no-op build. Windows machine: 32 GB RAM, eight logical processors in existing reports, storage described only as “disk”, compiler described as “latest” and linker as “llvm”; **verify** exact details on the actual host.

**Scope interpretation:** Compilation first. Rust-analyzer/Neovim-specific setting changes are explicitly **deferred**. Document an optional future editor tuning path, but do not edit Neovim/rustaceanvim settings or treat analyzer startup as a required execution checkpoint. Keep ordinary cargo check coverage where compilation measurements benefit from it.

> **Provenance and limitations:** Paths and source facts below were inspected as planning context. They must be rechecked against live branch `HEAD` before edits. No compile/link benchmark was performed for this planning package; six minutes is user-reported, not a measured value from the present execution environment. The live checked-out repository, not historical documents, is authoritative.

## 1. Verified source inventory and implications

| Source / configuration | Observed pattern | Why it matters / scope |
| --- | --- | --- |
| `Cargo.toml` | Single `[package]` (`multi_launcher`, edition 2024); `build = "build.rs"`; no `[profile.*]`, `[workspace]`, `[lib]`, or explicit `[[bin]]` stanzas | Cargo defaults apply; release uses its default optimization/incremental policy unless host config/env overrides. Keep production release unchanged. |
| `Cargo.toml` dependencies | 58 direct runtime dependencies, 3 dev dependencies (`image`, `criterion`, `serial_test`), 1 build dependency (`embed-resource`); `Cargo.lock` contains 710 package records | The lockfile is **not** evidence all 710 are compiled by a given command. Need `cargo tree`, `cargo --timings` and fingerprint evidence. |
| `build.rs` | Calls `embed_resource::compile("Resources/windows.rc", embed_resource::NONE)` without explicit `cargo:rerun-if-changed` | Cargo's default build-script rerun detection can watch the package broadly; narrow to actual resource inputs, after verifying dependencies. |
| `Resources/windows.rc` | One Windows ICON entry referencing `Resources/Green_MultiLauncher.ico` | Both resource script and icon must invalidate resource build; validate executable icon after modifications. |
| `src/main.rs` | 4,269 lines | Main launcher executable is separate from substantial library; explicit target selection can avoid unrelated bins, not eliminate shared library recompilation. |
| `src/lib.rs` | 88-line module export hub, but many large implementation modules behind it | The library's **effective** size is much larger than its root file; a change anywhere in the library may trigger final library code generation/linking. Measure instead of inferring. |
| `src/bin/` | `coordinate_tool_smoke.rs` (1,256 lines), `passive_overlay_smoke.rs` (16 lines), `radial_acceptance.rs` (40,331 lines) | Default `cargo build` includes multiple bin targets; measure `--bin multi_launcher` against default build. The huge acceptance binary merits profiling but not deletion. |
| `tests/` | 65 top-level `tests/*.rs` integration targets plus two explicit grouped suites `plugin_queries` and `domain` (67 total) | Each separate integration target is a distinct compiled/linked executable. Target selection is a likely immediate win. Do not assume target count alone explains six-minute release builds. |
| `tests/suites/plugin_queries.rs` | Aggregates roughly 35 `tests/plugin_cases` modules through `#[path]` | Existing precedent for grouping independent tests. Preserve test names/semantics when extending. |
| `tests/suites/domain.rs` | Aggregates roughly 24 `tests/domain_cases` modules through `#[path]` | Existing second grouped target; observe test isolation and category-specific conditions before moving anything. |
| `benches/` | Five benchmark sources; four explicitly declared Criterion-style benches, another auto-discovered benchmark file | Avoid accidentally building benches in normal iteration. Confirm Cargo target discovery before adding any configuration. |
| `.codex/config.toml`, `.codex/agents/` | Planner, implementer and reviewer agent definitions | Use existing orchestration, one writer per shared milestone, independent reviews when risks warrant. |
| `AGENTS.md` | Checked-out repository is authoritative; scoped review/verification; preserve behavior, local changes, and branch ownership | Do not inherit unrelated Track A validation regimes or launch a full regression suite on every change. |
| `dev/run_owned_process.ps1`, `dev/test_owned_process.ps1` | Existing Windows owned-process verification helpers | If runtime smoke needed for a linker/profile change, use bounded owned processes; do not terminate arbitrary processes by name. |
| `docs/performance/track_a_*` | Completed Track A result, checkpoint, native and benchmark evidence | These are **reference only**. No changes to Track A algorithms or snapshots are authorized by Track B. Use narrow relevant smoke cases to guard compilation/toolchain changes. |

### Cargo target and linker realities

- A plain `cargo build --release` selects the package's normal build targets, including auxiliary executables. `cargo build --release --bin multi_launcher` still compiles the dependencies and library needed by that binary; it only avoids unrelated binary targets. **Do not promise to eliminate a 6-minute library rebuild solely by changing target selection.**
- `cargo nextest run -E 'test(name)'` filters **test cases**, not necessarily the Cargo build target set. Prefer `cargo nextest run --lib -E 'test(name)'` or `cargo nextest run --test history` (or a named grouped target) to constrain compiled executables.
- Test execution and compilation are different clocks. Existing Track A reports included a five-test `history` integration run where tests finished in fractions of a second and the build took minutes; benchmark compile/link vs execution separately.
- Unlike typical `cargo check` work, optimized native `release` compilation includes LLVM codegen and linking. Default release builds disable incremental compilation; there is no free incremental-production build without changing those defaults. An **additional iteration profile** can use alternate knobs without changing canonical release.
- Codegen-unit increases can improve build latency at the expense of resulting code quality/runtime and binary size; reduce codegen units only after time+runtime measurements support it. Linker experiments can change debug symbol output and native library behavior; keep reversible.
- `sccache` can cache compiler work across compatible invocations but does **not** accelerate Rust incremental crate compilation directly. It is an experiment for non-incremental/dependency-cache paths; do not prescribe as a compulsory dependency.

## 2. Current build script and proposed narrow fix

Observed:

```rust
fn main() {
    // no custom macros are passed to the resource compiler
    embed_resource::compile("Resources/windows.rc", embed_resource::NONE);
}
```

Candidate implementation (only after capturing current behavior):

```rust
fn main() {
    println!("cargo:rerun-if-changed=Resources/windows.rc");
    println!("cargo:rerun-if-changed=Resources/Green_MultiLauncher.ico");
    embed_resource::compile("Resources/windows.rc", embed_resource::NONE);
}
```

Validation must prove (1) changing unrelated Rust source does not re-execute this script unnecessarily, (2) resource/icon edits do re-execute and embed into the production launcher, and (3) changing `build.rs` itself is still tracked automatically. **Do not** extrapolate from a build-script skip to a faster final library codegen; source changes still invalidate the crate being built. Do not add a broad `rerun-if-changed=Resources` unless needed by dependencies.

## 3. Profile and caching facts requiring local measurement

- No user-authored release/dev/test profiles are present in the inspected `Cargo.toml`. Host-level `CARGO_PROFILE_*`, `RUSTFLAGS`, `CARGO_ENCODED_RUSTFLAGS`, `RUSTC_WRAPPER`, `CARGO_TARGET_DIR`, `.cargo/config.toml`, global `~/.cargo/config.toml`, environment, and rustup/toolchain config **may override behavior**; capture them before attributing costs.
- Do not treat “latest” as a Rust version; capture `rustc -Vv`, `cargo -V`, `cargo nextest --version`, `rustup show active-toolchain`, `rustc -vV`, and actual linker invocation (Cargo `-vv`, `cargo --timings`, optional process observation). LLVM version from `rustc -Vv` does not prove which **linker executable** is in use.
- 32 GB memory plus eight logical processors calls for **measuring** `--jobs` variants rather than blindly increasing job parallelism. Caching/interference with rust-analyzer, filesystem scanners, MSVC/linker, and antivirus may matter; avoid enabling/disabling security software automatically.
- For release-like iteration, benchmark a named `[profile.iteration]` inheriting release but with varied codegen settings and `incremental=true`. This creates a separate cache directory (`target/iteration`) and a different executable; **do not label it equivalent to real release**.
- Debug-oriented choices such as `debug = "line-tables-only"` may reduce debug symbols for normal development while preserving a separate `debugging` profile with full symbols. Full-debug remains available for CodeLLDB and native debugging. Avoid disabling debug assertions/overflow checks on test workflows.
- Increasing source partitioning or introducing crates is a **conditional, high-risk architectural experiment**, not an automatic consequence of the project's size. Measure recompilation dependence/monomorphization/link costs and calculate added maintenance and feature-boundary burden before doing it.

## 4. Test isolation risks and safe grouping

- The 65 top-level integration targets are not interchangeable with 65 pure unit tests. Individual binaries confer process-wide isolation for environment mutation, registry/filesystem, Windows window ownership, hotkeys, COM, global statics, worker lifetimes and library initialization. A grouped suite still runs individual Nextest test cases in separate processes, but **build-time/test enumeration, name qualification and module scoping** may change; verify rather than assume.
- Existing grouped suites (`tests/suites/domain.rs`, `tests/suites/plugin_queries.rs`) demonstrate the `[path = "../...rs"] mod ...` pattern. Keep original test module source until migration parity is established; do not build two copies of the same tests accidentally.
- Before grouping additional targets, inventory each candidate's global-state use, `serial_test`, environment/working-directory changes, native windows, isolation requirements, and dependencies on `CARGO_BIN_EXE_*` and other Cargo test target behavior. Group only demonstrably compatible cases.
- Freeze inventory of `cargo nextest list` *binary targets and individual tests* before any grouping. Compare complete sets of logical test identifiers using a deliberate old→new naming map; do not rely on raw string equality if names gain a module prefix. Count disabled/ignored tests too. **No test may vanish.** Keep native/global-state targets separate when uncertain.
- Avoid `autotests=false` or broad `[[test]]` manifests solely to hide targets. A target that becomes unbuildable or no longer discoverable is a loss of coverage, not an optimization.
- A targeted command's improved time may result simply from avoiding unrelated targets. Record that as a workflow improvement distinct from changes to rustc performance on the same targets.

## 5. Required benchmark matrix and experimental discipline

| Workload | Baseline command family | Cache state / notes |
| --- | --- | --- |
| Release default target build | `cargo build --release --timings` | Typical cached dependencies; may build 4 bin targets |
| Release launcher-only build | `cargo build --release --bin multi_launcher --timings` | Same code/cache state where fair; compare after equivalent edits |
| No-op | repeat exact preceding command, with no file/env/toolchain changes | Target should report freshness; diagnose if it compiles |
| Small source change | representative small non-build-script Rust edit (in an isolated scratch worktree) | Rebuild relevant crate, measure compile vs link |
| Large module change | representative large GUI/plugin or lib module change | Compare stable warm deps; restore only owned scratch edits |
| Branch switch | bounded transitions among known relevant commits in disposable worktree | Prevent comparing different caches and unrelated source layouts without accounting |
| Narrow library test | `cargo nextest run --lib -E 'test(...)'` | Build and execution separately |
| Single integration target | `cargo nextest run --test history` | Build and execution separately |
| Grouped suite | `cargo nextest run --test domain` or `--test plugin_queries` | Expensive group; benchmark before deciding further grouping |
| Broad test selection | `cargo nextest run --tests` **only at approved measurement gate** | Quantify target fanout vs filtered build |
| Release-like iteration | `cargo build --profile iteration --bin multi_launcher` | Distinct profile/cache; compare repeated like-for-like modifications |

**Timing methodology:** log the exact command and Git SHA, Rust/Cargo/Nextest versions, build target, current power plan, `--jobs`, link executable, flags, cache directory, storage type/free space, start/end time, wall duration, Cargo-timings HTML path, compiler/linker breakdown, peak resource use (where measured), warnings, and success/failure. Do not compare first-run cache population against warmed iterations without labeling the difference. Repeat comparable small/large edits and report at least median + variability (or full repetitions), not a single cherry-picked run. A no-op baseline is separate from an after-edit incremental rebuild.

**Cache safety:** Never `cargo clean` in a user's primary target directory to create a “cold” run. Build a disposable clean output tree with explicit `CARGO_TARGET_DIR` when necessary, keeping it out of source control and preventing it from consuming the entire drive. Do not run simultaneous builds with differing experiment flags into the same target directory. Document any environment changes and restore shell-local values.

## 6. Recommended decision order

1. **B0** profile actual edit-to-build and no-op behavior; establish a usable baseline (not guessed ratios).
2. **B1** narrowly fix resource tracking and give explicit launcher-only build/test recipes; measure independently.
3. **B2** evaluate development and release-like iteration profiles; keep standard `[profile.release]` behavior unchanged.
4. **B3** optimize test target selection; consolidate test executables only if linker measurements justify and coverage is proven.
5. **B4** optionally test linker, sccache, memory-aware jobs, dependency feature constraints, crate extraction if evidence crosses a clear threshold.
6. **B5** report source-match comparisons and functionality/test-coverage preservation, document useful commands and rejected experiments.

### Specific risk caveats

- **No editor changes now:** rust-analyzer and Neovim improvements were an initial option but the user restricted the active initiative to compilation. Do not modify rustaceanvim/Nvim config, diagnostic settings or editor operation.
- **No production-release quality trade:** any experiment with optimization level, LTO, codegen units or debug symbols must be in a **new named profile**, not silently applied to the canonical production release.
- **No masking test costs:** grouping must preserve semantics and discovered test coverage, including platform/native dependencies.
- **No speculative split:** don't extract crates or replace dependencies simply to reach a percentage target.
- **No fabricated results:** this document verifies architecture and potential unnecessary work, not measured speedups.

## 7. Targeted validation anchors

- `src/bin/radial_acceptance.rs`, `src/bin/coordinate_tool_smoke.rs`, `src/bin/passive_overlay_smoke.rs`, and `src/main.rs`: confirm acceptance executables remain individually buildable/runnable and launcher binary embeds expected resources.
- `tests/suites/plugin_queries.rs`, `tests/suites/domain.rs`: grouped-test conventions and test name composition. Representative top-level `tests/history.rs`, `tests/notes_plugin.rs`, `tests/watchers.rs`: target selection and isolation examples.
- `docs/performance/track_a_results.md` and `track_a_checkpoints.md`: record existing runtime work as protected baseline evidence, **not** new compile-time measurements.
- `dev/run_owned_process.ps1`: bounded native Windows smoke when validating linker changes; not necessary for every profile documentation edit.
- `.codex/agents/*` and `AGENTS.md`: current orchestration/test discipline. Keep implementation on a dedicated feature branch with **local commits only**.

## 8. Authoritative external references (use current docs during implementation)

- Cargo build-script rerun controls: https://doc.rust-lang.org/cargo/reference/build-scripts.html
- Cargo build target selection and `--timings`: https://doc.rust-lang.org/cargo/commands/cargo-build.html
- Cargo profile reference: https://doc.rust-lang.org/cargo/reference/profiles.html
- Cargo Nextest test selection and target flags: https://nexte.st/docs/running/ and https://nexte.st/docs/selecting/
- rust-analyzer configuration (future optional research only): https://rust-analyzer.github.io/book/configuration
