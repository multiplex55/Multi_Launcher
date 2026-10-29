# AUD-F001 — M1-A implementation handoff

Authored by `/root/audit_planner`; persisted by `/root` on 2026-10-29.

## Decision and current evidence

Proceed with the existing tokenizer correction. No architecture change or additional milestone is needed.

Source locators refer to current base `59156d96adcc73939c7171487a0cb582e3286587`:

| Owner | Confirmed current behavior |
|---|---|
| `src/common/query.rs:113–173`, `tokenize_query_with_raw` | Scanning a backslash appends it to `raw` at 128–131 and arms `escape`. EOF at 163–166 appends it to both `raw` and `value`. Only the decoded-value append is needed there. |
| `src/common/query.rs:230–263`, `split_action_filters` | Removes recognized action filters using decoded values, then joins retained raw tokens with one space. Original inter-token whitespace is intentionally not preserved. |
| `src/plugin.rs:535–590,1291–1303,1331–1386` | Snapshot and synchronous searches use the shared splitter and query plan. Literal bypass applies only when the original query head matches a declared prefix, case-insensitively. |
| `src/plugin.rs:619–645,736–746` | `new_inert_for_test_at(directory: impl AsRef<Path>)` uses an empty test store and inert window catalog. `search_snapshot(enabled_plugins, enabled_caps).search(query)` returns `.actions`. |
| `src/plugins/snippets.rs:407–445` | Explicit tag tokens cause raw-token reconstruction. Without a tag token, this helper returns its own input unchanged. |
| `src/plugins/snippets.rs:478–486` | `starts_with_tag_filter` inspects the first raw token's prefix. No production change is required here. |

A scoped source search also found splitter consumers in GUI local search, GUI query classification and dashboard widgets. They inherit the tokenizer correction; they do not require policy changes or individual application tests for this task.

The existing plugin parity tests at `src/plugin.rs:1474–1483,1694–1709` illustrate synchronous/snapshot comparisons but use `PluginManager::new()`. Do not copy that constructor into the new tests. Even `new_inert_for_test()` opens a store at `"."`; use the explicitly rooted `_at` variant.

## Objective and ownership

Make every emitted `RawToken.raw` retain its token's exact source spelling when EOF follows a pending escape. Preserve decoded values, token boundaries, quote/escape grammar, filter normalization and existing consumer policies.

The first responsible owner is `tokenize_query_with_raw`. Correct its EOF bookkeeping; do not normalize downstream callers or introduce another parser.

## Scope and dependencies

Production change: only `src/common/query.rs::tokenize_query_with_raw`.

Directly relevant inline tests may change in:

- `src/common/query.rs::tests`
- `src/plugin.rs::tests`
- `src/plugins/snippets.rs::persistence_tests`

The snippets module is named `persistence_tests`, but the new helper test must remain pure and need not use its persistence fixtures or global test mutex.

Dependency: parent acceptance/publication of M0-A. No other package is required. The parent owns Git and execution progress; one implementer owns source changes.

## Required changes and tests

1. Add compact tokenizer regressions with `terminal_escape` in their names. Cover terminal runs of one, two, three and four backslashes. Raw spelling must match the original token; established decoded trailing counts are respectively one, one, two and two. Include Unicode, quoted-token controls and a nonterminal escaped-character control. Use explicit expected decoded values rather than treating another parser as the sole oracle.
2. Correct pending-escape EOF finalization so it completes the deferred decoded literal without synthesizing another raw character. Leave scanning, quote state and token emission unchanged.
3. Add a `split_action_filters` regression containing retained terminal-escape text and positive/negative `kind:`/`id:` filters. Assert normalized include/exclude lists, retained-token order and existing single-space reconstruction. Include odd/even terminal controls. Preserve existing parser tests at `common/query.rs:392–482`.
4. Add inert provider regressions in `plugin.rs`. Own a `tempfile::TempDir`, construct `PluginManager::new_inert_for_test_at(tempdir.path())`, and register only synthetic echo providers. A provider can return a fixed Action identity with the received query in `args`; make that fixed identity satisfy any selected action filters so filtering cannot hide the observation.

   Compare `manager.search_filtered(query, None, None)` and `manager.search_snapshot(None, None).search(query).actions`. Require nonempty results and equal expected received text for the ActionFilters path. Add a Literal echo case with a declared matching prefix and original whitespace/filter-looking text/Unicode/terminal escape preserved exactly. Merely declaring Literal without a matching prefix does not activate the bypass.

   Relevant defaults are ActionFilters policy, empty/global prefixes and `always_search = false` (`plugin.rs:101–142`). Override the prefix and policy deliberately for the Literal control. No production provider, refresh worker, application, clipboard or native runtime is needed.
5. Add a pure snippet parser regression beside `snippet_tag_query_parsing_preserves_other_query_syntax` at `snippets.rs:1047–1063`. With positive/negative tag filters, assert normalized tags and exact retained terminal-token spelling. Include a no-tag control with meaningful whitespace and terminal escape, asserting byte-for-byte equality with the helper input.

The no-tag guarantee is helper-local: Snippets uses default ActionFilters policy upstream. Do not describe it as an end-to-end exemption from manager reconstruction. All new regression test names must contain `terminal_escape`.

## Invariants and non-goals

Preserve filter grammar, whitespace normalization, prefixes, Literal matching, enabled/capability policy, snippet schemas/tags, Action serialization, ranking, execution and defaults.

No dashboard policy changes, parser rewrite, path normalization, source cleanup, dependency/configuration edits, native/app tests or full-suite expansion. New app fixtures are unnecessary; if one becomes genuinely necessary, document why and disable Paste Sequences next/cancel shortcuts before construction.

## Verification and exact limits

Source confirms package `multi_launcher`, edition 2024, implicit library target `src/lib.rs`, existing `tempfile = "3"`, and no extra feature needed for the `#[cfg(test)]` inert fixture. The manifest declares optional smoke/performance features but no default feature set. Installed tool versions and effective target/environment remain unverified by this planner; inspect them before execution without changing configuration.

Use the execution contract's selection:

```text
cargo nextest list -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo nextest run -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo clippy -p multi_launcher --lib --no-deps
```

Require nonzero existing parser cases and the new provider and snippet cases in discovery. The latter should appear under `plugin::tests` and `plugins::snippets::persistence_tests`. Listing may compile the library; do not run a duplicate ceremonial build.

Check formatting only for the three touched Rust files with edition 2024 and supported installed tooling. No warning suppression or unrelated formatting churn.

Separate operational caps:

- Initial selected-target compile/list: 20 minutes.
- Needed incremental correction compilation or scoped Clippy invocation: 10 minutes.
- Already-built selected test execution: 120 seconds.
- Owned temporary-fixture cleanup: 30 seconds.

These are monitored deadlines, not invented CLI timeout flags. Use one focused substantive review/correction pass and rerun only affected checks. If Nextest is unavailable, record the limitation and use supported equivalent filtered Cargo tests without installing tools. Missing mandatory proof or an exhausted deadline leaves acceptance incomplete.

## Done criteria

- Raw EOF spelling is corrected at the tokenizer; decoded controls remain unchanged.
- Parser/filter, inert synchronous/snapshot, matching Literal and pure snippet tag/no-tag regressions pass with nonzero discovery.
- Required scoped lint and touched-file formatting succeed, with unrelated pre-existing findings distinguished.
- Independent focused review and cumulative diff inspection find no policy drift, broader source changes or unsafe fixture initialization.
- Parent updates progress, commits `fix(query): [M1-A] preserve terminal escapes in raw tokens`, and attempts one normal push; push failure preserves accepted local work.

No unresolved architecture or product decision remains. Only installed tooling/effective target and actual verification results require confirmation during implementation. This planner ran no tests.
