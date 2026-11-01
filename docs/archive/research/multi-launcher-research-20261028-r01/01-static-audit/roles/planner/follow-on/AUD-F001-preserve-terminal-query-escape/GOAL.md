# Goal AUD-F001 — Preserve terminal escapes in raw query tokens

## 0. Identity, provenance and readiness

- Package: `AUD-F001-preserve-terminal-query-escape`.
- Finding: planner `AUD-001`, in [Planner analysis](../../ANALYSIS.md).
- Type: narrow correctness fix at an existing pure parsing boundary.
- State: `PROPOSED_NOT_AUTHORIZED`.
- Readiness: `READY_FOR_IMPLEMENTATION_REVIEW`; this does not authorize execution.
- Proposed canonical directory: `docs/research/multi-launcher-research-20261028-r01/01-static-audit/follow-on/AUD-F001-preserve-terminal-query-escape/`.
- Priority: modest, independent correction; useful before extending query interpretation, but not a prerequisite for research.
- Severity: low; query reconstruction can change a terminal character sequence. No data loss, native execution failure or broad Windows-path failure is established.
- Confidence: high for the source-level raw-token inconsistency; frequency and user impact unmeasured.
- Complexity: small. Regression risk: low but shared-parser compatibility matters. Validation burden: pure parser and inert consumer tests; no native proof needed.
- Source provenance: `59156d96adcc73939c7171487a0cb582e3286587`; revalidate current code when authorized instead of resetting to this SHA.
- Original author: `/root/audit_planner`, requested/configured `gpt-6-astra` / `max`; resolved model/effort unverified. Authored 2026-10-28. Independent implementation review has not occurred.
- Companion: [KICKSTART.md](KICKSTART.md).

## 1. Objective and user-visible outcome

Preserve the exact source spelling of each retained token in `RawToken.raw` when a query ends with a pending escape. Keep decoded `RawToken.value` and all existing query/filter grammar unchanged.

For input bytes `78 5C` (x followed by one backslash), the current source appends the backslash once while scanning and once again at EOF. The intended raw token is the original two bytes, not three. `split_action_filters` must therefore retain that terminal backslash exactly once while still removing action-filter tokens normally.

The proven benefit is correcting a source-level raw-token invariant. More predictable matching for affected query strings is anticipated. No runtime reproduction, native path failure, latency improvement or usage frequency was measured during research.

## 2. Current architecture and evidence

| Owner/path/symbol | Current responsibility | Required change | Relevant consumers/contracts |
|---|---|---|---|
| `src/common/query.rs::tokenize_query_with_raw` | Produces source spelling and decoded value together | Correct terminal-escape finalization without changing decoding | `split_action_filters`; snippet tag filtering |
| `src/common/query.rs::split_action_filters` | Removes `kind:`/`id:` tokens and reconstructs remaining raw tokens | Add regression proof; production logic should remain unless necessary | GUI local search, sync/snapshot plugin search, dashboard paths |
| `src/plugin.rs::{plugin_query_plan,PluginSearchSnapshot::search,PluginManager::search_filtered}` | Applies shared prefix/filter policy; matching-prefix Literal plugins receive original input | Inert consumer tests; no policy change | Explicit prefixes, enabled/capability filters, query payload fidelity |
| `src/plugins/snippets.rs::parse_snippet_query_filters` | Reconstructs raw text when tag filters exist; otherwise preserves original input | Focused tag-filter regression proof | Existing snippet query semantics |

The scanning branch appends a backslash to `raw` at lines 128–131; the pending EOF escape branch appends it again at lines 163–166. `value` needs its final literal backslash because scanning only armed the escape flag. Correcting `raw` must not remove that decoded value. [S1]

`split_action_filters` reconstructs from `token.raw`; its existing preservation test covers quotes and filter stripping but not a pending EOF escape. [S2], [S3] Both normal and snapshot plugin search use the split function, while `plugin_query_plan` returns original query text for an explicitly matching Literal prefix. [S4], [S5] Snippet filtering reconstructs from `raw` only when a tag token was supplied; otherwise it returns original input. [S6]

These are important limits: even-length terminal backslash runs clear the escape flag before EOF; not every escaped token takes the defective branch. Literal routes bypass the reconstructed query, and snippet no-tag parsing preserves its original input locally. A downstream consumer may normalize or ignore the changed text; this package does not claim all such consumers visibly fail.

No old queue plan is required. This is a newly scoped pure-parser correction, not a restart of a historical native investigation or a replacement query subsystem. At authorization, search current source and relevant issue/plan records once; if already fixed, document the no-op or remaining test gap.

## 3. Scope, non-goals and dependencies

Allowed production owner: `src/common/query.rs`. Allowed directly relevant test changes: existing inline test modules in that file, `src/plugin.rs`, and `src/plugins/snippets.rs`. Update task documentation under `docs/plans/execution/standalone/preserve-terminal-query-escape/`.

Do not change query prefixes, whitespace normalization, filter grammar, quoting/escape grammar, snippet tags, Action serialization, command execution, ranking, confirmation, dashboard query-policy semantics, Windows APIs, settings defaults, Cargo manifests, lockfiles, vendored code or global configuration. The dashboard Literal-policy question in the parent audit is separate and unaccepted by this package.

There is no production prerequisite. Start from then-current verified master, or a specifically approved integrated base if the user supplies one. The research SHA is historical evidence only. A missing research document on that base does not authorize importing unaccepted source patches.

## 4. Proposed design and migration

The existing tokenizer remains the sole owner. No public signature or type changes are needed.

1. Revalidate the two append branches and all callers of `tokenize_query_with_raw` and `split_action_filters`.
2. Establish the invariant: every raw token retains exactly the token's source characters, including quotes and backslashes; separator whitespace is outside the token. This does not require `split_action_filters` to preserve original inter-token whitespace, which it currently normalizes with `join(" ")`.
3. At EOF with a pending escape, complete only the decoded value that was deferred while scanning. Do not synthesize another source character in `raw`. Leave quote handling, escape consumption and token emission unchanged.
4. Preserve filter interpretation and normalized filter values. Preserve the current prefix-scoped Literal exception and enabled/capability behavior; consumers should receive corrected raw text through the existing shared function.
5. Add focused tests before or with the change. Prefer parameterized cases for odd/even terminal escape runs and combined quote/Unicode/filter inputs over a large combinatorial matrix.
6. Verify the consumer contract using `PluginManager::new_inert_for_test_at` with an owned temporary root and a synthetic echo provider. Compare normal and snapshot results, plus a matching-prefix Literal case. This test need not initialize `LauncherApp`, global macro runtime, native hotkeys or real clipboard services. [S7]
7. Cover snippet tag-filter reconstruction directly via its pure parser, with a no-tag control. Preserve existing tests and assertions.

There is no persistence migration, asynchronous state transition, native resource owner or new resource budget. The algorithm and allocations stay linear in the input size; no performance benefit is claimed. Source correction and tests land together through the existing boundary; there is no compatibility adapter to retain or remove.

## 5. Alternatives and reversal conditions

- **No change:** retains a directly visible raw/value mismatch; not preferred once the contract is accepted.
- **Fix the tokenizer:** selected. All consumers of raw tokens benefit without duplicating repair logic.
- **Normalize callers separately:** rejected; it obscures the meaning of `RawToken.raw` and can double-correct some paths.
- **Rewrite the parser or expand Literal routing:** rejected; unnecessary compatibility and product changes for an EOF bookkeeping defect.

Reverse or narrow this recommendation if current source has already fixed the defect, or source-backed tests show a deliberately documented competing raw-token contract. Do not keep a speculative bug label when source revalidation disproves it.

## 6. Invariants and acceptance behavior

- A single pending terminal backslash appears exactly once in `raw` and once as the pending literal in decoded `value`.
- For odd/even terminal backslash runs, `raw` matches source bytes and decoded `value` retains the established escape semantics.
- Quotes and nonterminal escapes keep existing tokenization behavior; Unicode is preserved.
- Filter removal still removes only the established action-filter tokens, preserves retained-token order and keeps normalized filter values.
- Normal and snapshot ActionFilters providers observe the same corrected query. Matching-prefix Literal providers observe the original query unchanged.
- Snippet tag-filter reconstruction preserves the terminal raw token; the no-tag branch still returns its original input.
- No saved data, default, Action wire protocol, ranking or execution-authority behavior changes.
- All required selected tests have nonzero discovery and pass; no native or whole-repository validation is implied.

## 7. Branch and checkpoint execution contract

Before editing, inspect repository identity, current master/base, worktrees, status, index, upstream and active Git operations. Preserve unrelated changes. For a new authorized run use branch stem `fix/query-terminal-escape`, choosing the first unused numeric suffix only for a genuinely different existing branch. Resume an identified interrupted instance instead of creating another baseline. Use an isolated worktree when needed; never stash, reset, discard, force-push or rewrite unrelated history.

Copy this approved contract and its Kickstart into `docs/plans/execution/standalone/preserve-terminal-query-escape/` for the execution instance. That Goal's `Execution progress — mutable` section is the task ledger; the research proposal remains evidence, not a competing completion record. If that execution directory already belongs to a different run, preserve it and use the branch's same numeric suffix for the new directory.

| Checkpoint | Objective / owner | Required outputs and acceptance | Commit subject |
|---|---|---|---|
| M0-A | Safe bootstrap and bounded revalidation / parent | Correct fresh or resumed branch, observed base, current issue disposition, task Goal/Kickstart and role provenance; no feature edits | `docs(query): [M0-A] establish terminal-escape fix` |
| M1-A | Correct raw-token preservation / one implementer | Tokenizer correction, focused unit/consumer proof, review, scoped lint/diff and final progress; all invariants above satisfied | `fix(query): [M1-A] preserve terminal escapes in raw tokens` |

### M1-A implementation handoff

**Objective:** correct `RawToken.raw` at pending-escape EOF without changing any other grammar.

**Ownership/current facts:** `common/query.rs` creates raw and decoded representations; both plugin search paths and snippet tag filtering consume them. Matching-prefix Literal routing and snippet no-tag behavior are counterguards, not targets to rewrite.

**Required changes:** perform the current-owner correction and test flow in section 4; keep changes limited to the named production function and directly related test modules. Inspect the cumulative diff for unintended parser changes.

**Invariants/non-goals:** section 6 and section 3 are binding. No source changes outside that scope without a concrete necessary dependency, documented before expanding.

**Dependencies:** accepted M0-A revalidation; current tokenizer still violates the raw spelling contract.

**Tests/verification:** use section 8's parser, provider and snippet cases. No new app fixture is needed.

**Done criteria:** the original adverse input and controls pass; consumers receive identical intended payloads across sync/snapshot routes; scoped checks and substantive review complete; source/docs diff is intentional; checkpoint committed and one normal push attempted.

**Genuine uncertainties:** whether the bug is still present when executed and which installed tool/target configuration is then supported. Ordinary private helper names and Rust mechanics are implementer choices.

After each coherent checkpoint, parent verifies staged scope, commits locally first, and attempts one ordinary push. Push failure preserves the commit and does not block remaining authorized work. No automatic PR, merge or release.

## 8. Targeted future verification — not run during research

| Behavior | Test owner and setup | Expected observation |
|---|---|---|
| Pending EOF escape and controls | `src/common/query.rs` inline tests; synthetic strings with one and odd/even terminal backslash runs, including Unicode/quotes | `raw` equals the original token spelling; decoded values follow existing escape grammar |
| Filter reconstruction | Same module; retained terminal-escape token alongside existing `kind:`/`id:` and negated filter tokens | Removed filters and normalized metadata unchanged; retained token contains no extra backslash |
| Provider boundary | `src/plugin.rs` inline test; inert manager at owned temporary root, synthetic ActionFilters and matching-prefix Literal providers | Sync/snapshot queries agree; Literal preserves original payload; no application/global runtime construction |
| Snippet tag boundary | `src/plugins/snippets.rs` inline test calling `parse_snippet_query_filters` directly | Tag filtering preserves remaining raw token; no-tag query remains exact original input |

Give new regression test names the substring `terminal_escape` so selection is auditable. Reuse existing test modules; no new test target or dependency is needed. Existing quote/filter tests remain valid and must not be weakened.

At future execution, first inspect installed tool help/version and current target configuration. The intended narrow selection is:

```text
cargo nextest list -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo nextest run -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo clippy -p multi_launcher --lib --no-deps
```

Check that the listing includes the new provider and snippet cases and a nonzero parser set. If Nextest is unavailable, record the limitation and use equivalent filtered `cargo test -p multi_launcher --lib` runs only when supported; do not install tools or claim zero selected tests as success. Targeted compilation already occurs for the selected tests; do not add a duplicate build merely for ceremony. Check formatting only for touched Rust files with the current edition/tooling; do not format unrelated files or add warning suppressions.

Proposed execution caps: at most 20 minutes for the initial selected-target compilation/listing; at most 10 minutes for a needed incremental correction or scoped Clippy invocation; at most 120 seconds for the already-built selected test batch; at most 30 seconds for owned temporary-fixture cleanup. These are operational bounds, not measured runtimes or CLI flags. Monitor compile, execution and cleanup separately using available supported process ownership tools. A cap hit is preserved incomplete evidence, not a pass. Do not proceed into a full-suite run or a dependency installation to overcome it.

No native test is required: this changes pure string handling, not input delivery or Windows file operations. If an app fixture is unexpectedly necessary, document why, use an isolated profile and set Paste Sequences `next_hotkey = None`, `cancel_hotkey = None` before constructing it. Production defaults and error notifications remain unchanged.

## 9. Failure handling and rollback

Keep the first meaningful failing assertion/compiler diagnostic and fix the responsible parser/test boundary. Use one focused substantive review/correction pass; rerun only affected checks. A new larger grammar or authority conflict is grounds to narrow/defer the package rather than silently expand it. Do not weaken assertions, rewrite unrelated tests, alter production defaults or invent native proof.

If required verification cannot finish within the available capability/budget, preserve source/tests/docs, mark acceptance incomplete and record the exact next command or source decision. Continue unaffected documentation and review work. Unavailable optional tooling is a truthful limitation; missing mandatory behavioral proof prevents acceptance.

There is no data migration to undo. A later rollback can revert the isolated M1-A correction and its tests using normal reviewable history; do not reset shared branches or discard user work. Do not automatically roll back a local change merely because push failed.

## 10. Models, roles and provenance

Suggested setup for this narrow pure-parser task: parent/planner `gpt-6.1-sol` at `high`; implementer `gpt-6.1-sol` at `xhigh`; reviewer in a separate context `gpt-6.1-sol` at `xhigh`. The ambiguity is small and source-local; the full research program's most expensive setup is unnecessary.

Inspect actual installed dispatch/role precedence once. Existing pinned Astra roles are an acceptable supported fallback; record requested, configured and observed resolved identities separately and keep unavailable identity `unverified`. Do not edit global role configuration or permissions just to obtain the recommendation. One supported dispatch recovery is enough; parent fallback remains explicitly self-review rather than independent review.

Within the execution docs directory use `roles/planner/{ROLE_RECORD,ANALYSIS}.md`, `roles/implementer/{ROLE_RECORD,ANALYSIS}.md`, `roles/reviewer/{ROLE_RECORD,REVIEW}.md`, and parent provenance/progress in the Goal. For this small fix the planner handoff above can be reused after bounded revalidation. One writer owns overlapping source. Read-only roles return content for the authorized writer.

## 11. Progress and stalls

Update the execution Goal's `Execution progress — mutable` section at M0-A completion, M1-A implementation/verification, review disposition and final acceptance. Record actual branch/worktree/base, previous known checkpoint SHA, commands/results, current issue and exact next action. Record actual push status; a commit cannot truthfully contain its own resulting SHA.

For a genuine pause/block/defer write `stalls/M1-A-UTC-timestamp.md` inside the execution docs directory, using the actual UTC timestamp in the name. Include expected versus observed behavior, exact failed/unrun command, diagnostic and known deadline/exit, source/branch/worktree, preserved changes, cleanup/process ownership, attempted correction, remaining allowance and smallest safe resume step. Never include private payloads or uncontrolled logs. Commit a clearly labeled documentation-only report when Git-safe; it does not imply the fix is accepted. Never commit through an unresolved conflict or unsafe index.

## 12. Final delivery and authority

Deliver the accepted tokenizer correction and focused tests, execution Goal/Kickstart, role/review records, actual selected test/lint/diff results and limitations. Report base/branch/checkpoints/final SHA, push visibility, precise changed behavior, preserved grammar and lack of native testing. Mark a source-backed no-op honestly if already corrected.

Only this approved package may execute after explicit user selection. Its creation does not authorize implementation, other audit recommendations, PR opening/merge, release, deployment, external account changes or user-data operations.

## 13. Execution progress — mutable

State: `PROPOSED_NOT_AUTHORIZED`.
Current branch/base/worktree: none created for this proposal.
Tests/commits/pushes: none executed for this package.
Review: proposal awaits substantive acceptance review.

| Checkpoint | State | Evidence / next action |
|---|---|---|
| M0-A | Pending authorization | User selects package; revalidate current source and bootstrap safely |
| M1-A | Pending M0-A | Correct raw token, prove scoped behavior, review and publish |

Next action: user authorizes this package, then revalidate `tokenize_query_with_raw` and its callers against the selected current base.

## Pinned source references

[S1]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L113-L173
[S2]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L230-L263
[S3]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/common/query.rs#L417-L424
[S4]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L1290-L1437
[S5]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L535-L590
[S6]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugins/snippets.rs#L407-L445
[S7]: https://github.com/multiplex55/Multi_Launcher/blob/59156d96adcc73939c7171487a0cb582e3286587/src/plugin.rs#L619-L645
