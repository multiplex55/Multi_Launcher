# Kickstart AUD-F001 — Preserve terminal escapes in raw query tokens

**Use only after the user selects and authorizes AUD-F001.** Read sibling [GOAL.md](GOAL.md) completely. Execute the narrow parser correction, not the research roadmap or historical implementation queue. Initial state is `PROPOSED_NOT_AUTHORIZED`.

## 1. Resolve current source and bootstrap M0-A

Locate the repository and current applicable AGENTS instructions. Inspect branch/HEAD, worktrees, staged and unstaged state, upstream and active Git operations. Resolve then-current verified master or the user's explicitly approved integrated base. The research source `59156d96adcc73939c7171487a0cb582e3286587` is evidence, not an instruction to reset the worktree.

Read `src/common/query.rs::tokenize_query_with_raw` and `split_action_filters`, their callers in `src/plugin.rs`, and `src/plugins/snippets.rs::parse_snippet_query_filters`. Verify that the scan branch and pending EOF branch still append the same terminal backslash to `raw` twice. Confirm matching-prefix Literal plugins use original query text and snippet no-tag filtering locally preserves its own input argument. Snippets uses default ActionFilters policy, so manager reconstruction may already have changed an untagged query; the direct-parser no-tag control is not an end-to-end exemption. If corrected already, document the remaining regression-proof gap or a source-backed no-op.

Create the fresh task branch using stem `codex/query-terminal-escape`, with a numeric suffix only if a genuinely different branch occupies that name. Resume an identified interrupted instance instead. Preserve unrelated dirty work and use an isolated worktree if necessary. Copy the approved contract/Kickstart into `docs/plans/execution/standalone/preserve-terminal-query-escape/` for execution progress; preserve a prior different run using the same numeric suffix as the branch when needed.

Record actual base/branch/worktree, revalidation disposition and model provenance. Commit `docs(query): [M0-A] establish terminal-escape fix` and attempt one normal push. A failed push does not block the local task.

## 2. Use bounded roles

Recommended parent/planner is `gpt-6.1-sol` high, implementer and separate reviewer `gpt-6.1-sol` xhigh. Check supported task-scoped role settings; existing pinned Astra roles are a valid fallback. Record configured versus observed model identity, leaving unavailable values unverified. Do not change global configuration or permissions. Use one source writer and a bounded independent review when available; parent self-review must be labeled honestly.

## 3. Execute M1-A at the existing parser owner

Own `src/common/query.rs` and only directly relevant inline tests in it, `src/plugin.rs` and `src/plugins/snippets.rs`.

1. Add regression cases showing that `RawToken.raw` equals source spelling for single/odd/even terminal backslash runs and appropriate quote/Unicode controls. Preserve existing decoded-value behavior.
2. Correct EOF finalization so only the deferred decoded literal is completed; do not add a second source character to raw text. Do not change general escaping, quoting or tokenization.
3. Cover `split_action_filters` with retained terminal-escape text alongside existing action filters. Expected filters/order/normalization remain the same, while retained raw text has no extra backslash.
4. Use `PluginManager::new_inert_for_test_at` with an owned temporary root and a synthetic echo provider to compare normal and snapshot input. Preserve the matching-prefix Literal exception. No LauncherApp/global macro runtime/native input/clipboard construction is needed.
5. Test snippet tag-filter reconstruction directly and keep a no-tag control. Do not modify snippet schemas or launch semantics.

New regression test names should contain `terminal_escape` so the scoped run can identify them. Do not absorb dashboard Literal-policy changes, new ranking, command-bus changes, path normalization or unrelated parser cleanup.

Done means the raw spelling invariant and all controls pass through the existing owner, not merely that a particular consumer compensates for the extra character.

## 4. Verify the exact behavior

Verify installed tool/target support and follow the Goal's separate compile, execution and cleanup bounds. List selected cases before reporting a passing run:

```text
cargo nextest list -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo nextest run -p multi_launcher --lib -E 'test(common::query::tests::) | test(terminal_escape)'
cargo clippy -p multi_launcher --lib --no-deps
```

Require nonzero parser cases and the new provider/snippet regression cases. If Nextest is unavailable, record that fact and use supported equivalent filtered Cargo tests without installing tools. Preserve meaningful assertions and first failure evidence. Keep formatting checks to changed Rust files. Do not run the full suite or native tests.

Initial selected-target compile/list cap is 20 minutes; a needed incremental compile or scoped lint cap is 10 minutes; already-built selected test execution cap is 120 seconds; owned-fixture cleanup cap is 30 seconds. Monitor these separately without inventing timeout flags. Missing mandatory proof or a deadline hit leaves acceptance incomplete, even if source looks correct.

No app fixture is expected. If one is genuinely required, document why, use an isolated profile, and disable Paste Sequences next/cancel shortcuts before application construction while preserving production defaults.

## 5. Review, commit and close

Review exact raw/decoded semantics, retained filter behavior, the Literal counterguard, snippet branch distinction and fixture isolation. Allow one focused substantive correction pass and rerun only affected checks. Inspect cumulative diff/staged paths for unnecessary files, parser-policy drift or global-state dependencies.

Update execution Goal progress with actual commands, results, review disposition, known previous checkpoint and limitations. Commit `fix(query): [M1-A] preserve terminal escapes in raw tokens` only when the required scoped acceptance succeeds, then attempt one normal push. Preserve local commits after push failure; do not force-push, reset, auto-merge or reopen unrelated goals.

For a real pause/block/defer, preserve work and publish a factual `stalls/M1-A-UTC-timestamp.md` report inside the execution docs directory when Git-safe. State the exact diagnostic/unrun command, branch/worktree, budget, cleanup state and smallest resume step. A documentation-only stop commit is not implementation acceptance.

Final delivery reports actual branch/base/checkpoint/final SHA and push status; changed raw-token behavior; preserved grammar/serialization/defaults; selected tests and scoped lint; review provenance; remaining limits and one next action. No native claim is needed for this pure string correction. Do not open/merge a PR, release or execute other proposed packages without appropriate separate authorization.
