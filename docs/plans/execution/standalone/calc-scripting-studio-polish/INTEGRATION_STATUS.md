# Calc scripting and studio polish execution

- Parent branch `feat/calculation-pad-editor-usability`, tip `7e55f5299122228e6d218118c4816f5f251cef46`. The preceding standalone Goal's archived execution handoff calls for its final tip. The local and remote-tracking branches agree, and the primary branch has integrated this exact tip. The predecessor reports limited native acceptance.
- Active branch `feat/calc-scripting-studio-polish` in the existing checkout. The starting checkout and index were clean.
- M0-A baseline `a2ced169aa3962ac2295b31494fe17dd70875cf3`, verified as a direct child of the recorded parent. `git push -u origin HEAD` succeeded and set upstream to `origin/feat/calc-scripting-studio-polish`.
- The Goal and Kickstart are available via `git show 93261f1b66c9462b7bb269f40e19cea9b074a632:docs/plans/CODEX_GOAL_CALC_SCRIPTING_STUDIO_POLISH.md` and the corresponding Kickstart path. That commit was on the initial checkout.

| Checkpoint | State |
| --- | --- |
| M0-A | Complete and pushed |
| M0-B | Complete and pushed at `8249b28fa91bc603a4067db6b7d90cc4adad150a`, 8 focused baseline tests passed |
| M1-A | Complete and pushed at `7ef7818fb2e1577d08b1bcbeb9fd33ff5530e347`, 29 focused tests passed after review fixes |
| M1-B | Complete and pushed at `68bc7de9a5921b904b66c1fdb3d6dd6a1c64d52a`. 73 focused tests passed plus 1 exact codec rerun. UI-7 pending. |
| M2-A | Complete and pushed at `6406529aacf75c16de0cb900d5121f711ee3a0a2`, 53 selected tests passed plus the corrected alias case. |
| M2-B | Complete and pushed at `79016f86dd2720e7ecce38d3cd0f438e6c998a07`, 60 focused tests passed plus the corrected typed composition fixture. |
| M2-C | Complete and pushed at `2d5c6c563159004f5a4dba97d7610253b75f604e`, 72 focused tests passed. |
| M2-D | Complete and pushed at `6066a7e88b701b4262fcbc549dfd226f1bcab528`. Final affected navigation cases passed 3/3. UI-6 pending. |
| M3-A | Complete and pushed at `ed6c91fd6e83df0fecefddc38b45170344e5d464`. 56 unaffected cases passed and 6 final targeted cases passed. |
| M3-B | Complete and pushed at `29abbc253090a74f9f738bd8df2f02ac8b893244`. 65 focused cases and 11 final targeted cases passed. |
| M3-C | Complete and pushed at `110910179822ef3edb05da9f827882722944c160`. Native folded input remains to be exercised. |
| M4-A | Complete in memory. 70 focused tests and six final targeted cases passed. |
| M4-B through M6-A | Pending |

## M1-A evidence

- Editable flowchart parsing now uses original source spans for semicolon statements and bounded arrow chains. It accepts `graph`, ordinary comments and direction tokens RL/BT. The normal editable geometry and comment-safe visual round trip are owned by M1-B.
- The selected parser, assistance, style, stale-draft, codec and renderer batches passed 28/28 initially and 29/29 after review corrections. Cases include Unicode/CRLF diagnostics, quoted and pipe labels, color style separators, the edge limit reached mid-chain, and non-flowchart preview regressions. Each batch used the 100-second watchdog with verified cleanup and output drain.
- Independent review fixes cover hex colors, header whitespace, and subgraph assistance at semicolon boundaries. A remaining Unicode subgraph-whitespace assistance case is assigned to M1-B. Touched-file rustfmt checks confirmed inherited formatting differences in `document.rs`, `model.rs`, `session.rs`, and `style.rs`.

## Affected baseline

- Calc bindings: `src/calculation_pad/document.rs` owns preparation, definition and reference resolution, and evaluation. `session.rs` attaches stable line IDs and owns cached evaluations, frozen comparison context, and dependency snapshots. Both document and comparison resolution currently rebuild duplicate-name definitions.
- Pad editor effects: `src/gui/calculation_pad_dialog.rs::complete_pad_host` checks retained viewport pass, live host, document incarnation, and revision. It applies source before resolving accepted result actions.
- Mermaid: `src/diagram/mermaid.rs` owns bounded parsing and formatting. `session.rs::apply_transaction` validates a regenerated candidate before publication. `rendering.rs` routes source-rendered families separately. Legacy codec snapshots include stale drafts which can become valid under the expanded grammar.
- QR: `src/gui/qr_dialog.rs` owns fresh reset, raster/texture cache, and current immediate viewport effects. The migration joins `HostedUtilityDialog` through the launcher command, panel lifecycle, rendering, and QA adapters, retaining the existing `qr-generator` size key.
- Nextest 0.9.135 is available. Use the `fast-dev` library binary and bounded Windows process runner. Reuse prepared Nextest binary metadata so the 100-second watchdog applies to each execution batch separately from compilation.
- Library preparation used `cargo nextest list --list-type binaries-only --message-format json -p multi_launcher --lib --cargo-profile fast-dev --offline`. Cargo exited 0. The owned runner reported descendants present at root exit and verified cleanup and output drain. The emitted binary metadata listed and ran the selected tests successfully.
- Nextest selected eight cases covering legacy binding diagnostics, cached evaluation, Mermaid grammar, assistance and stale preview, retained same-pass Pad copying, and QR fresh reset and cache reuse. All eight passed in 0.245 seconds with one test thread, retries disabled, and a 100-second batch watchdog. No app-level fixtures were constructed.
- Mermaid syntax was cross-checked against the [official flowchart reference](https://mermaid.js.org/syntax/flowchart.html).

## M1-B evidence

- Visual transactions preserve full-line comment text and order using source-derived anchors. Deleted targets leave standalone comments. The formatter checks combined source bounds before publishing a transaction. Reverse direction controls and explicit layout reach first-party scene and export geometry.
- The codec retains readable legacy stale drafts and history. The retained Refresh preview action re-admits now-supported exact source as one undoable operation. It leaves the saved file untouched until an explicit save.
- Nextest passed 59/59 in the first batch and 73/73 after review corrections, including comment order and limits, reverse arrowheads, source/image export ownership, legacy Refresh preview, and Notes/non-flowchart regressions. The sharpened legacy codec guard then passed 1/1 in isolation. Batches used a 100-second execution watchdog. The batches with an app fixture used a process-local 16 MiB test-thread stack. The touched Notes app fixture now uses a temporary profile and already disables Paste Sequence shortcuts.
- Review findings on exact-source activation, legacy guard specificity, trace traversal and header help were corrected. UI-7 remains scheduled for native verification.
- A separate native build was mistakenly started after handing off M2-A. It was stopped as soon as the overlap was identified, before it produced a build for acceptance. The owned runner verified cleanup and output drain. The M2-A edits were preserved and work resumed after the build exited.

## M2-A evidence

- The document resolves named reads to the nearest preceding assignment and keeps earlier physical row values and dependency edges. A valid assignment with an invalid RHS publishes a failed latest binding. Comparison expression names use the final immutable map from the same session evaluation, while explicit result references still address their physical rows.
- `CalculationEvaluation`, `SessionEvaluation` and `ComparisonContext` carry the internal binding map. The added crate-private fields change external struct-literal construction. There is no persistence format change at this checkpoint.
- Nextest covered document, comparison, dependency and selected session tests. The initial fail-fast run hit the outdated duplicate-name expectation in the `pie` alias test. The other 53 selected tests passed. The corrected alias test passed in isolation, retaining the prior alias/forward-reference checks. The grammar README documents reassignment and failed latest bindings.

## M2-B evidence

- Calc Pad adds lazy `if`, comparisons, `&&`/`||`/`!`, checked half-open scalar ranges and six collection statistics. The population standard deviation convention, 1,024 generated element ceiling, and exact sequence generation rules are documented. Legacy scalar, exact integer, programmer, vector and unit evaluation remain the typed arithmetic owners.
- `CalculationValue` now includes Boolean and sequence variants. Sequence previews show a bounded representation. Pad and Inspector result copying use complete parseable range syntax. No document wire change is involved.
- The initial compile found a missing Dependency Explorer type-label match, which was corrected. The first compiled batch passed 59/61. Its two outdated expectations were updated for the new unary collection signature and the three added executable examples. The next batch passed 60/61. The remaining new fixture accidentally used a shadowed Boolean name as a scalar condition. After changing those fixture conditions to comparisons, its exact rerun passed, including direct unit/base conversion and skipped malformed exponent assertions.
- Review corrections covered unique lowered expression slots, exact prefixed integers, contextual programmer operators, existing alias behavior, checked work, median precision and full sequence copy. Runtime batches used the 100-second watchdog. Loop execution and current-revision worker integration remain in M2-C and M2-D.

## M2-C evidence

- Calc Pad parses space-indented multi-line `for` blocks into bounded prepared statements. A block reads its iterable once and publishes its final results on success. Local failure discards attempted values, marks intended output names failed, preserves unrelated bindings, and distinguishes attempted from nonexecuted physical rows. Zero iterations preserve earlier bindings.
- Limits are 8 nesting levels, 10,000 cumulative loop iterations and 50,000 work units, in addition to the existing source limits and 1,024 generated element cap. The pure evaluator observes cancellation. Named results carry their own typed outcomes and source provenance. Dependency Explorer labels loop rows and their source-line projection.
- Nextest ran focused Calc document, loop, comparison, dependency, session, examples and Dependency Explorer tests. Initial 66/66 passed. Review remediation expanded coverage of malformed and empty blocks, contextual names, attempted rollback, nested physical reads, global limits and final failed bindings. A stale example count and an admission bug were fixed after the 69/71 batch. The corrected batch passed 72/72 within the 100-second watchdog.

## M2-D evidence

- The Pad session owns one evaluator with one replaceable pending request. Source/document identity and revision fence publication, with cancellation of superseded requests and completion-driven repaint. Small legacy documents stay synchronously cached. The same evaluator and checked row projection serve both paths.
- Current Results, clipboard actions, Inspector and Dependency Explorer use accepted snapshots. Pending/failed states are explicit, stale comparison copy is disabled, and dependency actions retain their exact source revision and original full-line selection behavior.
- The initial focused batch passed 51/51. The review-corrected batch passed 52/53 and caught a full-line selection regression. The final affected integration and two retained navigation cases passed 3/3 after separating selection mode from revision validation. Tests cover coalescing, cancellation, worker panic recovery, Save As/reopen identity, stale comparison copy and pending dependency behavior. UI-6 remains scheduled for native verification.

## M3-A evidence

- Pad-local Ctrl+F/H/G opens literal search, replacement or physical-line navigation. The editor maps UTF-8 source offsets to caret positions while retaining CRLF and final newlines. Replace All is a single session-owned source transaction. Source/result selection and diagnostics use retained, revision-checked navigation with one-shot scrolling.
- Review corrections let source navigation work while evaluation is pending, fence queued actions during host retirement, preserve native widget focus and make Escape dismiss the focused tools. The reference catalogue keeps its existing first group and appends the new examples.
- The first run passed 51/55 and exposed four reference-catalogue order regressions. Restoring its order passed those cases. The next run passed 56/57 and caught immediate Escape after Ctrl+H. The final six targeted cases passed, including the keyboard sequence, live-source navigation, retirement, and detached launcher query.

## M3-B evidence

- The editor highlights syntax using the active egui theme, offers scope-aware names and built-ins, and shows call signatures with the active parameter. Completion replaces the whole token and preserves existing parentheses. Completion and hints default on and can be disabled independently.
- The signature catalogue supplies parser arities and assistance. Scalar/vector calls follow their established case policy. Exact programmer constructors and helpers keep their existing case sensitivity.
- Compilation initially found four egui character-index mismatches. After correction, 65 Pad, settings and script cases passed. The final 11 targeted checks passed, including compact completion, disabled Enter, case policy, nested hints, braced loop scope and retained source boundaries. UI-2 remains pending.

| Requirement | Checkpoints | Implemented | Automated verification | Native verification |
| --- | --- | --- | --- | --- |
| MMD-01 | M1-A, M1-B | Yes | Passed | Pending |
| MMD-02 | M1-A | Yes | Passed | Pending |
| MMD-03 | M1-A | Yes | Passed | Pending |
| MMD-04 | M1-A | Yes | Passed | Pending |
| MMD-05 | M1-B | Yes | Passed | Pending |
| CP-01 | M2-A/C/D | Yes | Passed | Pending |
| CP-02 | M4-A/C | In-memory panel | Passed for M4-A | Pending |
| CP-03 | M3-B, M4-C | Assistance and toggles | Passed for M3-B | Pending |
| CP-04 | M3-B | Yes | Passed | Pending |
| CP-05 | M4-A/C | In-memory watches | Passed for M4-A | Pending |
| CP-06 | M3-A/C | Yes | Passed | Pending |
| CP-07 | M3-A/C | Yes | Passed | Pending |
| CP-08 | M3-B | Yes | Passed | Pending |
| CP-09 | M4-B/C | Pending | Pending | Pending |
| CP-10 | M3-C | Yes | Passed | Pending |
| CP-11 | M3-C | Yes | Passed | Pending |
| CP-12 | M3-A/C | Yes | Passed | Pending |
| CP-13 | M2-B | Yes | Passed | Pending |
| CP-14 | M2-B | Yes | Passed | Pending |
| CP-15 | M2-C/D | Yes | Passed | Pending |
| CP-16 | M2-B/C | Yes | Passed | Pending |
| QR-01 | M5-A | Pending | Pending | Pending |
