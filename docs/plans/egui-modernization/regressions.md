# Current defect and performance classification — revised policy2026-10-14

Authority: [PERFORMANCE_ACCEPTANCE_ADDENDUM.md](PERFORMANCE_ACCEPTANCE_ADDENDUM.md). Documented synthetic ROOT/list performance differences are accepted debt, not current release blockers by themselves. Preserve original failures below. Actual crashes/data loss/input/focus/lifecycle defects, repeated visible freezes, sustained true-idle work or unbounded resources still require correction; unavailable observations remain explicit QA gaps.

## Historical record — unchanged, not operative

# Baseline failures and migration regressions

No dependency or application implementation changes have been made at the time of
these original failures. Source baseline: `72a01bd5`; local inventory checkpoint:
`050a922f`. Preserve failed receipts rather than replacing them with later passes.

## Original baseline Nextest diagnostics

`cargo nextest run --locked --offline --profile default --cargo-profile fast-dev`,
with `ML_SKIP_CLIPBOARD_SYNC=1` and default disabled performance diagnostics:
build 7m30s; test summary 64.501s, 2653/6442 run, 2651 passed, 2 failed, 41 skipped,
3789 not run due to fail-fast. Raw local log:
`target/egui-modernization/baseline/nextest.log`.

Failures:

- `gui::watch::tests::track_c_oracle_action_publication_preserves_complete_consumers`
- `gui::watch::tests::track_c_oracle_local_equal_empty_prefix_and_recovery_publication`

Both initially fail queue-depth assertions (`None` vs `Some(1)`). Independent
review traced the fixture's ordinary event channel to diagnostics-dependent depth
tracking; these oracles require explicitly observed fixture channels rather than
depending on the user's environment.

The exact two-test rerun with process `MULTI_LAUNCHER_PERF=1` also failed, after
passing those queue assertions. Raw log:
`target/egui-modernization/baseline/telemetry-rerun.log` (2 run, 2 failed).
Review traced the next failures to stale geometry-generation expectations:

- The changed fixture modifies action 0's description while the active query
  returns action 9; complete current results stay equal.
- An equal local save republishes catalog state but likewise preserves current
  query results.

`search.rs` deliberately preserves geometry when complete same-query results are
equal. The stale assertions predate that optimization (`508bcf47a` vs the later
`da838c6d5`). The focused correction preserves full catalog, version, Arc, queue
and consumer assertions, derives expected geometry changes independently from
complete query-result equality, and retains all fixture cases. The production
equality optimization is unchanged; see the focused correction receipt below.

The full original `--no-fail-fast` run completed: 6442 run, 6439 passed, 3 failed,
41 skipped; 134.496s. Raw log: `target/egui-modernization/baseline/nextest-complete.log`.
It reproduced both watch failures and found one further preexisting stale test:
`plugin::tests::mouse_builtin_registration_and_filtered_routing` expects 13 commands,
but the intentional halo/zoom/effects additions in `7f906416` make 20. Existing mouse
unit tests independently verify those 20 and namespace routing. The focused
correction now compares the manager's complete ordered inventory with
`MousePlugin.commands()` while retaining settings-first and invalid-route
assertions; no production change was needed.

The original test run generated a default `clipboard_modifiers.json` in the initially
clean repository root. After all tests stopped, this task-owned artifact was moved
to ignored `target/egui-modernization/baseline/generated-clipboard_modifiers.json`.
It was preserved, not incorporated into source or deleted.

## Focused baseline fixture correction

The two watch oracles now create explicitly observed event channels for their
test-only Custom cases before enqueueing the fixture event. Their assertions still
check queued depth before and after consumption. The ignored Track C owner retains
its original channel setup, so the measured event path and performance record
semantics are unchanged.

The watch oracle now compares the complete eager before/after exact-query result
vectors from the fixture's row-9 `Action`. Root-result geometry generation is
expected to change only when those vectors differ, independently of whether the
whole action catalog publishes. Catalog versions, Arc identity, complete consumer
projections, popup/selection behavior, persistence diagnostics, and all five cases
remain asserted. The equality optimization in production is unchanged.

The plugin-manager test retains settings-first and all invalid-route assertions,
expects the current 20 built-in entries, and compares the complete ordered manager
result to `MousePlugin.commands()`. Production plugin inventory and routing were
not changed.

Focused verification ran with default performance telemetry (unset),
`ML_SKIP_CLIPBOARD_SYNC=1`, and the direct Rust 1.99.0 MSVC toolchain:

```text
cargo nextest run --locked --offline --cargo-profile fast-dev --lib -E \
  'test(track_c_oracle_action_publication_preserves_complete_consumers) | \
test(track_c_oracle_local_equal_empty_prefix_and_recovery_publication) | \
test(track_c2c_equal_refresh_installation_semantics) | \
test(mouse_builtin_registration_and_filtered_routing)'
```

Result: **4 passed, 5,207 filtered out**, exit 0; compile 1m10s and test summary
0.595s. Raw output is preserved at
`target/egui-modernization/baseline/fixture-repair.log`. Existing Rust 1.99
`Atomic::fetch_update` deprecations and unrelated library-test warnings remain.
These focused passes do not rewrite the original full-suite receipt or establish
that the original 6,442-test run passed; that run still records three stale fixture
failures as documented above. No production behavior, dependency, benchmark, or
native-test result changed in this correction.

## Migration regressions

Not evaluated: migration has not started.
