# Kickstart SEARCH-F001 — Builtin discovery metadata foundation

**Execute only after the user selects and authorizes SEARCH-F001.** Read [GOAL.md](GOAL.md) completely. This is the eleven-descriptor metadata package, not authorization to implement natural-language search, change canonical routing, or execute other research recommendations.

## 1. Resolve current source and authority

Locate the repository, applicable AGENTS instructions, current worktree/branch and Git state, active Git operations, role configuration and intended current master base. The research source `59156d96adcc73939c7171487a0cb582e3286587` is evidence, not an instruction to reset current development. Preserve all unrelated work.

Revalidate `PluginCatalogSnapshot` and its revisioned manager API in `src/plugin.rs:382–422,1108–1159`; the ambiguity handling pattern in `src/settings_editor/catalog.rs:272–318`; `Action` in `src/actions/mod.rs`; and the nine plugin owners in the Goal. Confirm the plugin names, exact opener Actions, separate `query:...` shortcuts and typed parser/handler targets. If equivalent metadata already exists, document a no-op or the exact remaining delta; do not duplicate it.

No audit parser fix, Diff authority spike, ranking algorithm or UI package is a prerequisite. A changed source premise should narrow or stop this package's affected boundary, not authorize a general plugin/dispatcher migration.

## 2. M0-A — create or resume the task safely

For a new authorized instance, use a fresh branch with stem `codex/discovery-catalog` from the verified then-current master or explicitly approved integrated base. Resolve collisions without overwriting a branch. Use an isolated worktree when needed to preserve unrelated dirty work. For a confirmed interrupted instance, verify identity/ancestry and resume its existing task branch; do not recreate its baseline or rewrite history.

Create the execution record under `docs/implementation/SEARCH-F001-built-in-discovery-metadata/`. Preserve the approved Goal/Kickstart and research provenance. Record branch/base/worktree, current source, original research source, scope, configured models, unknown serving identity where applicable, and the section 13 milestone table. Inspect the staged docs and commit `docs(search): record SEARCH-F001 implementation baseline`; attempt one normal push. A failed push is recorded and does not block local progress.

## 3. Use sequential roles and truthful model records

Recommended task-scoped assignments are Astra xhigh for bounded planning and review, Sol xhigh for implementation, and an available supported parent such as Sol high. Check actual role pins: a configured Astra max planner/reviewer is acceptable when recorded; do not assume prompt wording overrides it. Never change global configuration or permissions just to match this recommendation.

Use one source writer. Read-only planner/reviewer roles return content for persistence in `roles/<role>/`. Record requested/configured/resolved-or-unverified identity and distinguish separate review from parent self-review. At most one supported role-capacity recovery is allowed before a clearly labeled available fallback or limitation report.

## 4. M1-A — implement the complete metadata boundary

Inspect and modify only the allowed owners:

- New crate-private `src/search_discovery/` module and its declaration in `src/lib.rs`.
- `src/plugins/json_utility.rs`, `hash_generator.rs`, `xml_toolkit.rs`, `regex_tester.rs`, `table_utility.rs`, `csv.rs`, `image_compare.rs`, `duplicate_finder.rs`, and `activity.rs` for pure opening-Action extraction and directly relevant tests.
- Existing manager snapshot APIs and inert test helpers as consumers/test seams; no production lifecycle or Plugin trait change.

First extract pure plugin-owned opener constructors. JSON accepts the existing `JsonUtilityIntent`; other constructors accept no query/path/content input. Make existing `search()` branches reuse them while preserving full Action fields and exact query matching. Leave all `commands()` shortcuts, their ordering and cardinality unchanged.

Add exactly the Goal's eleven typed discovery IDs and bounded static vocabulary. Distinguish general JSON, format and minify; keep Table and CSV profiles distinct. Describe duplicate discovery, Activity and image comparison honestly. Do not add ZIP extraction, automatic deletion, Macro run-history or full-file export claims.

Implement pure projection from the existing immutable plugin snapshot and the caller's current enabled-provider/capability policy. Require unique builtin origin/name, current epoch, enabled state, declared search capability and configured search permission. A same-named dynamic/registered entry does not inherit builtin annotations; duplicate names fail closed. Preserve current missing-capability-restriction semantics, while a present list without `search` denies discovery.

Emit deterministic available/unavailable metadata in stable ID order. Materialization must compare the expected binding to current identity/revision/policy and return only the exact plugin-owned opening Action, or a typed unavailable/stale result. It never calls search, command enumeration, a service, filesystem, clipboard, worker or dispatcher. Do not retain provider locks/handles or private configuration values.

Do not connect the catalog to `gui/search.rs`, command/completion caches, `self.results`, GUI rendering, Macro brokers, radial/dashboard/hotfolder or Universal Actions. This package does not implement complete protected-prefix admission, a new query-origin discriminator or native selection handling. Those are later integration prerequisites, not unfinished work to absorb here.

Add focused tests for eleven-ID completeness, full Action/typed-command parity, unchanged shortcuts/matching, source origin/duplicate/missing cases, capability/enablement restrictions, stale epoch/revision/policy rejection, stable ordering and metadata bounds. Use plain snapshots and stateless plugin fixtures. If a manager fixture is needed, use `new_inert_for_test_at` with a temporary root and the existing builtin registration test seam. Do not construct the production manager or load user data.

M1-A is done when these invariants have meaningful selected proof, no accidental integration exists, and the actual diff contains only the coherent metadata change. Commit `feat(search): add typed builtin discovery metadata` and attempt one normal push.

## 5. Verify proportionately

Confirm current package/target/tool support and list the selected tests before running them. The package is currently `multi_launcher`, library tests. Select the new `search_discovery` tests and the nine modified plugin modules; verify that the filter resolves to the intended nonzero inventory. Use the installed Nextest CLI's supported syntax rather than guessing test names.

Use the Goal's matrix: one initial selected batch and at most three focused correction reruns. Selected library tests provide compilation; a separate `cargo check -p multi_launcher --lib` is only needed for a concrete compile-only diagnosis and is limited to one initial invocation. Preserve the first meaningful failure and correct the owner; do not weaken equality, shortcut, availability or stale-binding assertions.

Run final scoped first-party library Clippy after code changes, using supported flags such as `cargo clippy -p multi_launcher --lib --no-deps`, with one correction rerun if necessary. Check formatting only for touched Rust files. Identify unrelated pre-existing warnings without refactoring vendored or unrelated code. No full suite, benchmark, application launch, screenshots or native/manual campaign is required.

No app fixture is expected. If one becomes necessary, explain why a pure fixture cannot establish the invariant, use isolated synthetic state, and disable Paste Sequences' next/cancel hotkeys before construction. Preserve production defaults. Clean up only task-owned temporary fixtures.

Missing tools or an exhausted allowance require an honest incomplete-verification record, not installation, invented success or silently renewed budgets. Continue unaffected documentation/review.

## 6. M2-A — review, close and publish progress

Give a separate reviewer the exact diff, contract and targeted evidence. Review source-owned Action construction, current snapshot/policy semantics, stale bindings, unavailable cases, metadata truthfulness and absence of live search/Macro integration. Do not turn this into another architecture audit. The sole writer resolves required findings and reruns only the affected checks within the remaining budget.

Update section 13 of the execution Goal at real transitions with accepted checkpoint, changes, actual commands/results, remaining issue, next action and commit/push visibility. A genuine stop gets `stalls/<checkpoint>-<UTC>.md` with sanitized diagnostic, expected/actual result, branch/worktree, preserved state, ownership/cleanup facts, remaining allowance and exact safe resume instruction. A partial report does not imply implementation acceptance.

Before closure inspect the cumulative diff and Git status, verify unchanged shortcut/canonical-search/automatic-consumer boundaries, reconcile every invariant, and record reviewer disposition and final first-party lint. Commit `docs(search): close SEARCH-F001 metadata review` when accepted, or use a clearly labeled incomplete report subject if required proof remains unavailable. Attempt one normal push; preserve local commits on failure. Never force-push, reset user work, auto-merge or repeat network recovery loops.

Report final branch/base/checkpoints/SHA, actual push status, changed owners, tests and lint run/not run, limitations, reviewer independence and rollback. The result is a metadata foundation with unchanged user-visible behavior. Do not automatically proceed to ranking/UI integration, AUD-F001/AUD-F002, PR creation, release or external changes. Those require separate appropriate authorization.

## 7. Initial state

`PROPOSED_NOT_AUTHORIZED`. This kickstart has not created a branch, changed source, run tests or accepted implementation. Its next action is user selection/authorization, followed by M0-A current-source revalidation.
