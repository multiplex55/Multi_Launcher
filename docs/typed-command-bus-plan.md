# Typed Command Bus Implementation Ledger

This is the durable execution ledger for the Phase 1 typed command bus and launcher command-execution extraction. The original feature request is retained in the task attachment; this ledger records the accepted repository-grounded milestones, their objective acceptance criteria, actual verification, and commits.

## Global invariants

- `Action` remains the serialized/search/plugin compatibility representation and retains its four-field serde shape.
- `Plugin::search()` and `Plugin::commands()` continue to return `Vec<Action>`; the external DLL ABI is unchanged.
- Every activation parses its action exactly once. A pending destructive confirmation retains that parsed invocation.
- `ACTIVATION_HOOK` and `EXECUTE_ACTION_HOOK` keep observing the original `Action` at their existing semantic boundaries.
- History stores the original `Action` and source label; usage keys remain the original `action.action` string.
- QueryExec recursively performs a normal activation of the first result with the original source.
- File Search, Diff, and Clipboard Modify retain their current query-override precedence.
- Unknown launchable actions retain external fallback behavior; explicitly malformed MkMacro actions remain errors.
- The bus is stateless typed routing. Domain behavior lives in handlers behind coherent capability traits.
- Write-heavy milestones execute sequentially, receive targeted verification, and produce one coherent commit each.

## Milestones

### 1. Characterize activation lifecycle and shared policies

- Status: complete
- Dependencies: none
- Likely areas: `src/gui/actions.rs`, `src/gui/render.rs`, `tests/history.rs`, `tests/dashboard_config.rs`, `tests/hide_after_run.rs`, `tests/preserve_command.rs`
- Work: add activation-level characterization for Query/QueryExec focus, restore, search, recursion/source; query-override ordering; real hide/preserve behavior; success-only history/usage; original Action retention; errors; hidden external execution. Preserve confirmation, hook, macro, and panel-restoration contracts.
- Acceptance: tests pass against the legacy implementation; production behavior is unchanged; hide/preserve assertions exercise normal activation instead of duplicated prefix logic.
- Verification: targeted history/hide/preserve/dashboard, activation, and query Nextest filters.
- Commit: `66cc83c test(commands): characterize activation lifecycle policies`
- Verification record: `cargo fmt --all --check` passed; `git diff --check` passed; history/hide/preserve/dashboard integration targets passed 26/26; six focused GUI activation tests passed 6/6; final isolated hide/preserve rerun passed 12/12.

### 2. Add owned command model and canonical parser

- Status: complete
- Dependencies: 1
- Likely areas: new `src/commands/{mod,model,parser,error}.rs`, `src/lib.rs`, `src/gui/state.rs`, parser tests in launcher modules.
- Work: add owned nested command enums, `CommandInvocation`, stable domain/kind metadata, neutral `ActivationSource`, unified error, and a complete side-effect-free parser with typed structured payloads and compatible precedence/fallback.
- Acceptance: every produced protocol maps to an owned command or intentional external fallback; Action/Plugin APIs and JSON remain unchanged; comprehensive compatibility/parser tests pass.
- Verification: command parser/source-label/legacy parse tests and `cargo check`.
- Commit: `06e3ce5 refactor(commands): add owned command model and parser`
- Verification record: parser/model `cargo nextest run --lib commands::parser::tests` passed 17/17; source-label Nextest passed 1/1; `cargo check`, `cargo fmt --all --check`, and `git diff --check` passed. A focused parity audit found and drove fixes for the public headless parse boundary, typed system variants, malformed known-protocol compatibility, explicit `timer:show` fallback, semantic payload naming, and broader protocol coverage.

### 3. Migrate headless execution and remove old parser/plan

- Status: complete
- Dependencies: 2
- Likely areas: new `src/commands/headless.rs`, `src/launcher.rs`, `src/launcher/exec.rs`; remove `src/launcher/parse.rs` and `src/launcher/plan.rs`.
- Work: make `launch_action` a parser-plus-typed-headless compatibility facade; preserve direct-call, no-op, args, fallback, legacy macro, favorite, and Clipboard Modify semantics.
- Acceptance: one parser remains; no `ActionKind`/`LaunchPlan` references; direct non-GUI callers retain behavior.
- Verification: launcher and affected plugin Nextest tests, `cargo check`, stale-reference search.
- Commit: `478bdb1 refactor(launcher): execute headless actions through typed commands`
- Verification record: typed headless `cargo nextest run --lib commands::headless::tests` passed 8/8; affected Shell/Snippets/Tempfile/Favorites/Recycle Nextest targets passed 39/39; `cargo check`, `cargo fmt --all --check`, `git diff --check`, and legacy-symbol searches passed. A parity audit found no behavior regressions and its execution-boundary test gap was remediated with an injected external-launch seam and facade compatibility tests.

### 4. Establish bus, host traits, outcomes, and typed activation seam

- Status: complete
- Dependencies: 1-3
- Likely areas: new `src/commands/{outcome,host,bus}.rs`, launcher/query handler, `src/gui/command_host.rs`, GUI state/actions/confirmation.
- Work: add stateless bus, coherent host traits, `Arc<CommandBus>`, typed Launcher/Query handling, parsed pending confirmation, centralized outcome/error application, and structured tracing. A temporary typed-to-legacy bridge may exist only for not-yet-migrated domains.
- Acceptance: Launcher/Query bypass raw routing; destructive classification is typed; confirmation never reparses; hooks remain compatible; bus contains typed routing only.
- Verification: bus, destructive, query, macro-launcher tests and `cargo check`.
- Commit: `8e22b46 refactor(commands): establish typed activation bus`
- Verification record: `cargo check`, `cargo fmt --all --check`, and `git diff --check` passed; focused bus, destructive, query, pending-confirmation, parser-error, and macro-launcher Nextest filters passed; affected history/hide/preserve/dashboard/visibility/MultiManager integrations passed 34/34; stale-route audit found no production Launcher/Query raw routing.

### 5. Migrate headless-backed GUI execution and generic post-policy

- Status: complete
- Dependencies: 4
- Likely areas: external/storage/timer/system handlers, outcome, GUI host/actions, history/hide/preserve tests.
- Work: migrate generic/static execution families and encode favorite logging, toasts, history/usage, clear/hide exemptions, refresh, focus, and browser-tab async behavior as typed outcomes. Preserve the Action-based execution hook without reparsing.
- Acceptance: scoped families leave the legacy chain; success/error/history/hide/preserve behavior matches characterization; no generic second parse.
- Verification: history/hide/preserve and affected domain tests plus `cargo check`.
- Commit: `7c104bf refactor(commands): migrate generic GUI execution policy`
- Verification record: `cargo check`, formatting, and diff checks passed; focused handler/query/bus tests passed 8/8; Snippet/history/hide/preserve tests 26/26; Storage 31/31; Timer/System/Shell 34/34; Media/Macro 8/8; GUI actions 16/16. Stale-route and parse-boundary audits passed; review findings for multi-toast parity and explicit result invalidation were remediated.

### 6. Migrate low-risk dialogs and crop commands

- Status: complete
- Dependencies: 4-5
- Likely areas: dialog handler, GUI host/actions, dialog/settings/crop tests.
- Work: migrate simple dialogs, settings/theme/convert/crop commands through typed host methods.
- Acceptance: no scoped raw string checks; hidden-launcher panel restoration and history/clear/hide exemptions are preserved.
- Verification: dialog/settings/theme/crop and migrated-action tests plus `cargo check`.
- Commit: `cbbb400 refactor(commands): migrate interactive dialogs and crop`
- Verification record: `cargo check`, formatting, and diff checks passed; handler/bus 5/5, GUI lifecycle 5/5, help/convert/timer/shell/storage 67/67, macro/MkMacro/todo/clipboard/system 53/53, settings/theme 14/14, and crop 16/16. Review corrected generic clear/hide policy and expanded Stage C coverage to all assigned simple dialogs; stale-route audit passed.

### 7. Migrate Calendar

- Status: complete
- Dependencies: 4, 6
- Work: characterize then migrate open/jump/add/search/upcoming/snooze together, preserving persistence, relative-time evaluation, results, focus, toast/error, and no-history behavior.
- Acceptance: one typed Calendar handler owns the family; no Calendar raw parsing remains; schemas remain compatible.
- Verification: Calendar Nextest tests and `cargo check`.
- Commit: `refactor(commands): migrate calendar domain` (hash recorded after commit)
- Verification record: Calendar handler 3/3, GUI parity 3/3, bus 1/1, activation 21/21, dashboard 37/37, and omni-search 13/13 passed; `cargo check`, formatting, diff, and zero-stale-route audits passed. Parent review confirmed non-fatal persistence errors, result metadata, execution-time relative dates, and error-toast gating.

### 8. Migrate Notes and linking

- Status: pending
- Dependencies: 4-7
- Work: characterize then migrate dialogs/graph/assets/open/new/tags/links/wrap/remove/reload, including legacy payloads, mutation ownership, typed confirmation, errors, query behavior, and external `note:template:*` compatibility.
- Acceptance: Note/Link routing is typed; confirmation retains invocation; persistence/panel behavior remains compatible.
- Verification: note, wrap-links, confirmation, and note integration tests plus `cargo check`.
- Commit: pending
- Verification record: pending

### 9. Migrate Todo

- Status: pending
- Dependencies: 4-5
- Work: migrate dialog/view/edit/add/priority/tags/remove/done/clear/export with encoded and legacy delimiter compatibility and typed post-policy.
- Acceptance: GUI/headless share typed operations; no Todo raw post-policy; confirmation, persistence, pending query, toasts, history, preserve and hide rules match existing behavior.
- Verification: Todo plugin/dialog/hide/preserve tests and `cargo check`.
- Commit: pending
- Verification record: pending

### 10. Migrate Mouse Gestures

- Status: pending
- Dependencies: 4, 6
- Work: characterize then migrate dialogs/focus/settings/toggle; decode JSON once and preserve malformed/missing claimed no-op behavior, persistence, dashboard refresh, and source behavior.
- Acceptance: typed payloads reach handler; no raw JSON execution parsing; behavior remains compatible.
- Verification: mouse gesture suites and `cargo check`.
- Commit: pending
- Verification record: pending

### 11. Migrate MultiManager

- Status: pending
- Dependencies: 4, 6
- Work: characterize and migrate the full `mm:*` namespace while retaining operational state/lifecycle in existing domain methods.
- Acceptance: every variant routes through the typed handler; bus contains no implementation logic; async/error/no-history/focus behavior is preserved.
- Verification: MultiManager launcher/plugin tests and `cargo check`.
- Commit: pending
- Verification record: pending

### 12. Migrate File Search and Diff

- Status: pending
- Dependencies: 2, 4, 6
- Work: reuse typed wire payloads, decode once, preserve malformed wording/claimed behavior/query-override exemption, and remove raw GUI helpers.
- Acceptance: `handle_file_search_action` and `handle_diff_action` are gone; GUI receives typed payloads; plugin Action strings remain unchanged.
- Verification: File Search/Diff suites and `cargo check`.
- Commit: pending
- Verification record: pending

### 13. Migrate Screenshot

- Status: pending
- Dependencies: 3-5
- Work: represent mode/destination/markup explicitly; preserve GUI/headless unknown-mode differences, editor/capture outcomes, completed-only history, cancellation, errors, and panel restoration.
- Acceptance: typed screenshot execution has deterministic host coverage and no generic clear/hide behavior.
- Verification: Screenshot tests and `cargo check`.
- Commit: pending
- Verification record: pending

### 14. Migrate Clipboard Modify asynchronous dispatch

- Status: pending
- Dependencies: 2, 4-5
- Work: migrate open/execute/undo/error protocols into typed commands while retaining the coordinator/runtime; store original Action/source/canonical query/hide preference through deferred completion; remove raw helper.
- Acceptance: decode once; no query misclassification or premature history; async success/failure/visibility and legacy headless behavior remain compatible.
- Verification: all Clipboard Modify suites and `cargo check`.
- Commit: pending
- Verification record: pending

### 15. Remove legacy router and enforce architecture

- Status: pending
- Dependencies: 1-14
- Work: delete temporary bridge and remaining feature string routing; reduce activation to lifecycle; keep bus exhaustive and short; migrate helper-coupled tests; add architectural regression coverage; remove stale compatibility paths.
- Acceptance: raw action routing lives only in canonical parser; no giant GUI chain, old parser/plan, raw helper, or legacy bridge remains; compatibility formats are unchanged.
- Verification: stale-reference searches, `cargo fmt --all --check`, `cargo check`, `cargo nextest run`, and `cargo clippy --all-targets`.
- Commit: pending
- Verification record: pending

## Integration and independent review

- Status: pending
- After milestone 15, inspect the cumulative diff and stale-reference searches; run full formatting, check, Nextest, and Clippy verification.
- Spawn an independent high-reasoning reviewer against the original request, this ledger, cumulative branch diff, surrounding architecture, and tests.
- Remediate substantive findings sequentially with an implementer and a separate commit when meaningful; rerun affected and full verification until review is clear.
- Final state requires every milestone complete, all intended commits present, `cargo nextest run` passing, and a clean working tree.

## Review record

Pending.
