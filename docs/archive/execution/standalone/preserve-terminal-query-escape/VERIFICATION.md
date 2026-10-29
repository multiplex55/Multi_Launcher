# AUD-F001 verification record

Source/base: 59156d96adcc73939c7171487a0cb582e3286587. Branch: codex/query-terminal-escape. Date: 2026-10-29. Current user authorizes the implementation package; prior research-only execution exclusions do not apply to its explicitly required focused tests/lint.

## Tool and target preparation

Parent ran tool version/help probes only before M1-A: cargo1.99.1, cargo-nextest0.9.135 (host x86_64-pc-windows-msvc), clippy0.1.99, rustfmt1.10.0-stable. Installed Nextest list help confirms -p/--lib/-E, --locked/--offline and JSON output support. At that preparation point no test discovery/compilation/execution or Clippy lint had run. Cargo.toml confirms multi_launcher, edition 2024; the implementer subsequently verified default target/features from current local configuration as recorded below. No manifest, lockfile or configuration changes authorized.

Required scoped commands (new test names contain terminal_escape):

```text
cargo nextest list -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo nextest run -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo clippy -p multi_launcher --lib --no-deps
```

Use available locked/offline flags when suitable to avoid dependency installation/lock drift. Verify nonzero selected parser tests and new provider/snippet cases. Existing quote/filter tests remain. Format-check touched Rust files only, edition 2024. Initial selected-target compilation/listing cap20min; needed incremental correction/scoped lint cap10min; built selected test batch cap120s; owned fixture cleanup cap30s. Capture actual command, start/end, process/session ownership, exit, first diagnostic and selected results in this file or linked bounded receipts. No whole-suite/native/app test is requested. No app fixture expected; any justified fixture must disable Paste Sequences next/cancel shortcuts before construction.

## Results

M1-A source and scoped verification complete: nonzero 11-case discovery, 11/11 selected tests, required scoped Clippy and touched-file formatting pass. Separate independent implementation review/publication remains parent-owned; no final review acceptance is claimed here.

## M1-A effective target and process ownership

Author checked current effective settings before execution: no repository/ancestor/user Cargo config files were found; CARGO_BUILD_TARGET, CARGO_TARGET_DIR, RUSTFLAGS and CARGO_ENCODED_RUSTFLAGS are unset. `rustc -vV` reports 1.99.1, host **x86_64-pc-windows-msvc**, LLVM 23.1.1. Selected commands use that native default target, library/default features and no additional profile or configuration override. Cargo/Nextest/Clippy/rustfmt versions/support from parent preparation are reused; no install/manifest/lockfile change.

Production correction is one pending-EOF raw append removed; five regression tests are authored across the three allowed files. `rustfmt --edition 2024 --check --config skip_children=true src/common/query.rs src/plugin.rs src/plugins/snippets.rs` passed standalone with exit 0, tool wall 0.2842296s; all files parsed without formatting changes. An earlier combined formatting/diff probe was also quiet, but the standalone command supplies the unambiguous exit receipt.

Initial selected-target compilation/discovery started **2026-10-29T09:59:55.183651+00:00**, owned cargo PID 36996, tool session 39570. Exact command:

```text
cargo nextest list --locked --offline -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
```

The task-owned Python subprocess wrapper records PID/start/end/exit/monotonic duration and automatically terminates only that cargo descendant tree on the 1200-second deadline. Raw output and receipt are ignored local artifacts at `target/aud-f001-verification/nextest-list.{log,json}`. This is the initial selected test build/list, not a separate ceremonial build/check. Subsequent already-built selected test batch 120s and scoped Clippy 600s have separate owned-process deadlines. The initial process completed normally with exit 0; its final timing/inventory and subsequent test results follow. Warnings occurred in unchanged vendor/table-core/table-utility source; no failing diagnostic or source correction was needed.

## Actual discovery and selected execution

| Phase | Start/end UTC | Owned cargo PID | Exit / elapsed / cap | Observation |
|---|---|---:|---|---|
| Initial compile/list | 2026-10-29T09:59:55.183651+00:00 → 2026-10-29T10:07:09.785904+00:00 | 36996 | 0 / 434.594000s /1200s | Cargo compile reported 7m03s; 11 selected cases |
| Already-built selected run | 2026-10-29T10:07:32.520885+00:00 → 2026-10-29T10:07:34.775033+00:00 | 5276 | 0 / 2.266000s /120s | Test execution 0.102s; 11 passed, 6440 skipped |

Exact run command:

```text
cargo nextest run --locked --offline -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
```

Nonzero selected inventory: 8 parser cases (6 unchanged plus 2 new), 2 new inert provider cases and 1 new pure snippet case; all 5 new terminal_escape tests were discovered and passed.

```text
common::query::tests::action_matches_filters_uses_metadata_for_id_and_kind
common::query::tests::apply_action_filters_with_metadata_filters_without_recomputing
common::query::tests::parse_query_filters_handles_negated_tags_and_kinds
common::query::tests::parse_query_filters_handles_quoted_tags_and_mixed_ops
common::query::tests::split_action_filters_preserves_query_tokens
common::query::tests::split_action_filters_terminal_escape_preserves_order_and_metadata
common::query::tests::tokenize_handles_quotes_and_escapes
common::query::tests::tokenize_raw_terminal_escape_preserves_spelling_and_decoding
plugin::tests::action_filters_terminal_escape_matches_synchronous_and_snapshot_input
plugin::tests::matching_prefix_literal_terminal_escape_preserves_original_input
plugins::snippets::persistence_tests::snippet_tag_terminal_escape_preserves_reconstruction_and_no_tag_input
```

Run ID `fc723836-a11c-417f-a06c-0cc42de27e4f`, Nextest default profile. Required run used the unchanged built target: Cargo reported Finished test profile in 1.00s and emitted no Compiling lines. No separate cargo check/build or correction compile was run. Both manager tests drop manager then require TempDir.close success; their complete test durations 0.051s/0.044s bound their successful cleanup below 30s. No lingering fixture or external cleanup action is required. The snippet test is a pure helper call, with no storage/application fixture.

Raw ignored logs/receipts: `target/aud-f001-verification/nextest-list.{log,json}` and `nextest-run.{log,json}`. Log SHA-256 list `9e35c10b60635313bc48fb79d9824bc35ecfdd8b60c7d57fe2693509ee9bdfbf`; run `612e380925eb1d19f78c77bb673840fa3a13b77d51b43f82e91992a4132bdb87`. First build warning: dead-code variant Unknown in vendor/epaint/src/text/font.rs361; other warnings are unchanged vendor lifetimes/deprecation, table-core dead code and table_utility_session.rs unused replaced_scan_job. No baseline lint/build rerun is claimed; these locations lie outside this patch. No warning suppression/unrelated cleanup.

Scoped Clippy completed with owned PID 25904/session 46887 and independent 600s deadline:

```text
cargo clippy --locked --offline -p multi_launcher --lib --no-deps
```

Exit 0, elapsed 90.266000s /600s; start 2026-10-29T10:07:49.966926+00:00, end 2026-10-29T10:09:20.237810+00:00. Cargo reported 1m30s dev-profile lint/check. All owned command sessions ended normally; no timeout, termination, leaked fixture or failed command.

Nine first-party library warnings point to unchanged source: three unused locals and three collapsible-if suggestions in src/gui/table_utility_dialog.rs, one collapsible-if in src/archive/plan.rs, result_large_err in src/file_search/coordinator.rs, manual_inspect in src/mkmacro/runtime/summary.rs. Dependency/vendor warnings are also outside this patch. No warning suppression, automatic cargo fix or unrelated edits; no independent baseline-Clippy comparison was run. Log SHA-256 `051574529194b724121c7f3990b09ee191cbf8017181f6c4ec28a629557b131d`; raw ignored receipt/log at target/aud-f001-verification/clippy.{json,log}.

Final source-prefix comparison to HEAD confirms plugin.rs/snippets.rs production code unchanged; common/query.rs production differs only by the EOF raw append removal/invariant comment. Scoped git diff --check passes; Cargo.toml/Cargo.lock/.cargo/.codex have no task changes. No pre-fix runtime/red test claim: source revalidation established the defect, regression tests ran with the correction, avoiding a duplicate library build. No corrective compile/rerun was needed.

No whole-suite/native/application/benchmark execution, new dependency installation, manifest/lockfile/config/default/schema change, Git mutation or other follow-on execution. Required scoped checks are complete; source and role verification records are ready for parent's separate review. One focused substantive review/correction pass remains only if that review finds a required issue; do not expand testing otherwise.
