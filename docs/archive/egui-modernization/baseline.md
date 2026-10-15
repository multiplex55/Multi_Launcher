# M0-A preflight baseline

Recorded 2026-10-12 from the repository worktree. This includes baseline compile evidence, but it is not an interaction, screenshot, or performance acceptance report.

## Repository and source identity

- Branch: `egui-modernization`.
- HEAD: `72a01bd5451060d3f6b640b83acd081d7db76e77`.
- The parent's initial pre-receipt status check showed a clean `egui-modernization` branch. The raw receipt was captured after this documentation directory had been created; it therefore records `?? docs/plans/egui-modernization/` as untracked, with no staged files and no tracked staged or unstaged diff filenames. That is the exact current receipt state, not a source change.
- `Cargo.lock` SHA-256: `A9DA377E18592D87734FF50B14F051E59E0E602E5C94D860183891C0E7682DF1`.
- Raw Git state and lock digest: [`receipts/git-state.txt`](receipts/git-state.txt). The tracked `Cargo.lock` at the recorded HEAD is the dependency snapshot; no duplicate lockfile was written.

## Supplied snapshot reconciliation

The mission objective says to use the current repository HEAD as the baseline, so the working baseline for this run is the recorded HEAD. The companion contract also names `multi_launcher(20261012-203406).zip` as a source snapshot and asks for reconciliation. The attachment directory's recursive inventory contained only `goal-objective.md`; a repository search for that exact archive name returned no result. The parent also searched the repository parent and Downloads without finding a `multi_launcher*.zip`. Therefore no archive hash or file comparison was possible, and HEAD is **not verified** as matching that archive; the parent requested clarification asynchronously.

The parent reports that a read-only `git diff 00f2fc53 HEAD --numstat -- src Cargo.toml Cargo.lock tests` produced no output, indicating no differences in those paths between those two revisions. This does not establish identity with the missing archive. The parent created `egui-modernization` at the recorded HEAD after fetch succeeded.

`docs/performance/` is absent at this HEAD. Historical Track C workload and native-recipe documents are available in commit `00f2fc53` and were read as references only; they were not restored. Their historical measurements are not a current egui baseline. No screenshot, native interaction pass, or performance result is claimed here.

## Toolchain and host

The direct stable toolchain binaries reported:

```text
rustc 1.99.0 (b940084d7 2026-09-28)
host: x86_64-pc-windows-msvc
cargo 1.99.0 (5f94df478 2026-08-27)
```

This local compiler exceeds the contract's Rust 1.95 minimum; no local toolchain upgrade was needed. The `rustup show` shim invocation failed with `Access is denied (os error 5)` while attempting to remove `rustup-init.exe` in the Cargo bin directory. The direct toolchain binaries were used for the read-only Cargo tree receipts. No remote Windows builder was configured or supplied; this local Windows MSVC host is the migration host.

The parent reports that `cargo info` resolved eframe 0.36.2 with `rust-version = 1.95`, egui-toast 0.22.0, and egui_commonmark 0.25.0 with `rust-version = 1.95`. The official latest eframe docs page also identifies 0.36.2: [docs.rs/eframe/latest/eframe](https://docs.rs/eframe/latest/eframe/). These are availability/version receipts only; wrapper API and feature compatibility remain for M0-C. No dependency was changed.

## Baseline build and tests

The parent completed `cargo check --locked --all-targets --offline` in session 55341: **PASS**, exit 0, dev profile, 4m11s. Rust 1.99 emitted eight existing library `Atomic::fetch_update` deprecation warnings, plus test-target warnings: `unused_mut` and `dead_code` in the binary test target, and unused commands plus ignored `egui::Context::run` results in the library test target. These were recorded without code changes; no warning cleanup was requested. The command compiles all targets but does not execute tests.

The original `cargo nextest run --locked --offline --profile default --cargo-profile fast-dev` used `ML_SKIP_CLIPBOARD_SYNC=1`. Its fail-fast run reported 2,653 tests run, 2,651 passed and 2 failed; the later no-fail-fast diagnostic completed with 6,442 run, 6,439 passed, 3 failed and 41 skipped. The three failures were classified as stale test fixtures; see [`regressions.md`](regressions.md). The original raw logs remain at `target/egui-modernization/baseline/nextest.log` and `nextest-complete.log`.

A source-test-only fixture correction then passed the four focused watch, search-equality and mouse-routing tests (5,207 filtered out; exit 0; compile 1m10s; test summary 0.595s). Its output is preserved at `target/egui-modernization/baseline/fixture-repair.log`; see [`receipts/nextest-status.txt`](receipts/nextest-status.txt). This focused pass does not replace or rewrite the original failing full-suite receipt. No native UI pass, screenshot, or runtime benchmark was performed as part of M0-A. The M0-A exit remains open pending archive clarification and parent-owned checkpoint handling.
