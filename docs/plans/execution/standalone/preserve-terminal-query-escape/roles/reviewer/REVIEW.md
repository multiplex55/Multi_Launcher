# AUD-F001 M1-A implementation review

Authored by `/root/research_reviewer` on 2026-10-29; persisted by `/root`.

## Disposition

**ACCEPTED — no substantive findings or required corrections.**

The reviewed implementation satisfies the approved narrow correction and regression-proof requirements. Parent-owned final progress updates, commit and normal push remain publication steps; this review does not claim they have occurred.

Reviewed state: branch `codex/query-terminal-escape`, HEAD `42fe7d2a50c59f67e4ae628de845142c1cab9e54`, and its uncommitted source diff in `src/common/query.rs`, `src/plugin.rs` and `src/plugins/snippets.rs`. Production source base is `59156d96adcc73939c7171487a0cb582e3286587`.

## Substantive evidence

1. **The correction is at the first responsible owner.** In `tokenize_query_with_raw` (`src/common/query.rs:113–173`), scanning already appends each source backslash to `raw`. Removing only the pending-EOF raw append preserves that spelling while retaining the required deferred append to decoded `value`. Scanning, quote state, token emission and decoded behavior are unchanged. There is no downstream workaround or second parser.
2. **The tokenizer tests distinguish the defect from legitimate decoding.** The regression at `src/common/query.rs:399` uses explicit decoded expectations for terminal runs of one through four backslashes: one, one, two and two. It checks exact raw spelling, Unicode, completed and unclosed quotes, escaped space and escaped quote. Odd terminal cases would fail against the removed raw append; even and nonterminal cases protect unchanged behavior. This counterfactual is established statically, not by a claimed pre-fix test run.
3. **Filter transport remains compatible.** The regression at `src/common/query.rs:448` checks retained-token order, quoting, existing single-space reconstruction and the complete expected normalized positive/negative kind/id metadata for all four terminal runs. The implementation does not change filter recognition or normalization.
4. **Provider assertions observe both real query paths.** The ActionFilters test at `src/plugin.rs:1481` exercises synchronous and snapshot search, requires exactly one result, and checks its received query against an independent expected string. The fixed `echo:terminal` identity satisfies the selected filters, so empty-result parity cannot conceal a transport failure. The Literal test at `src/plugin.rs:1509` uses an actual case-insensitive matching prefix and checks original whitespace, filter-looking payload, Unicode and terminal escape. Its rejecting kind/id filters would remove the result if filtering incorrectly entered the Literal path. Existing routing and policy code remains unchanged.
5. **Fixture construction is appropriately isolated.** Both provider tests use owned temporary roots and `new_inert_for_test_at`. The inspected constructor uses `MkMacroStore::empty_for_test`, an inert window snapshot and no production provider loading. Registration adds only the synthetic provider. The macro store has no watcher; no global macro runtime is installed. Both tests drop the manager before requiring successful temporary-directory closure. No application fixture or native shortcut setup is needed.
6. **Snippet coverage respects the helper boundary.** The pure regression at `src/plugins/snippets.rs:1067` checks normalized positive/negative tags and retained raw spelling for all four runs. Its no-tag control checks exact preservation of that helper's input, including whitespace. The records correctly avoid extending this guarantee across upstream manager reconstruction. No persistence fixture or snippet policy change was introduced.

The complete source diff contains only the EOF correction/invariant comment and five new tests with their synthetic provider. Existing tests remain intact. No migration, dependency, schema, default, serialization or unrelated policy change is needed.

## Verification and limits

Saved receipts and run output corroborate:

| Check | Recorded result |
|---|---|
| Initial selected-target compile/list | Exit 0; 434.594s within 1200s; 11 selected cases |
| Selected Nextest run | Exit 0; 11/11 passed, 6440 skipped; 2.266s within 120s; test execution 0.102s |
| Scoped library Clippy | Exit 0; 90.266s within 600s |

The selected inventory contains six existing parser tests and all five new regressions. Both provider tests include successful explicit cleanup within their recorded total durations of 0.051s and 0.044s. VERIFICATION additionally records successful touched-file formatting and scoped whitespace checks.

This reviewer did not rerun those checks or perform an independent baseline warning comparison. Recorded warnings point outside the changed code. No full-suite, application or native validation was performed or is required for this pure string-handling correction.

No actionable amendment, new package or additional verification campaign is recommended.
