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
| M4-A | Complete and pushed at `4966bdfdf68cf982a0e77e5880126dad70675b55`. 70 focused tests and six final targeted cases passed. |
| M4-B | Complete and pushed at `8df4f947190a4caf0632d29cd8613433dc5baa0c`. 73 focused and 4 final targeted cases passed. |
| M4-C | Complete and pushed at `1bde28061bb62465b5832fa20a2feb2f3bb1c581`. 123 focused and 9 final relevant cases passed. |
| M5-A | Complete and pushed at `06ba8cfa9327c6adde4b1ed9effe5a26b6a6f320`. Final affected QR cases passed in the 46-test integration run. |
| M6-A-R1 | Complete and pushed at `1d7bf67cc73c6865e6b04a1081cc57904b507faf`. Folded typing with passive pointer movement passed in both event orders. |
| M6-A-R2 | Complete and pushed at `fe3ef8c381b771ecc7f82bec6fefb2584993c1dc`. Introduced Clippy warnings corrected. The 14 directly affected cases passed. |
| M6-A | IMPLEMENTED / NATIVE ACCEPTANCE LIMITED. Seven of eight Windows scenarios passed. Native in-flight supersession and divider drag persistence remain unverified. |

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

## M3-C through M4-C evidence

- Folding and smart input use source-coordinate projections and retained edits. M3-C passed 70 selected tests after correcting a folded boundary fixture, then 12 final targeted tests.
- Inputs and output watches preserve selected source identity and resolve final bindings from the active evaluation. M4-A passed 70 selected and six final targeted tests. M4-B passed 73 selected and four final targeted tests for display-only formatting and canonical copy.
- M4-C passed 123 selected and nine final relevant tests, including v1 reads, v2 writes, strict view metadata, Save As, recovery isolation and Pad settings. One broad selector also ran two unrelated cases, both passing. The final GUI selectors were exact.

## M5-A evidence

- QR uses the shared utility host with the existing geometry and presentation key. Source, correction, Advanced, validation and cached raster follow the same session across host moves. Retained commands own clipboard and file effects, and the QA capture adapter observes the live native child.
- The first batch passed 15/17 and exposed two launcher close-routing failures. The QA-enabled correction passed both routing cases and 25/26 overall, with one focus assertion remaining. The second correction passed 28/28 in 13.349 seconds of Nextest runtime, including focus, failure-preserving Cut, raster availability and QR-specific QA probes. The process runner verified teardown and output drain in each execution batch with the 100-second cap.

## Final automated verification

- The cumulative reviewer confirmed that current-source evaluation and presentation/persistence paths retain their ownership. Findings on fresh QR Undo and folded typing during mouse movement were corrected, then independently rechecked.
- The final QA-feature run passed 46/46 targeted Nextest cases in 14.522 seconds. Its selector covers revised QR and QA capture behavior, root close/focus, Calc bindings, loops, worker, editor, format and v1 persistence, and native-flowchart parser, layout and comment-preserving transactions at the domain level. The 100-second watchdog verified cleanup and output drain.
- A selective rustfmt attempt spliced structural edits and failed at compilation. The affected files were restored or rebuilt from canonical formatter output, then rechecked for syntax and unexpected semantic changes. Three QR integration files carry whole-file formatting as a result. QR/Calc settings writes during app fixtures were detected and the local ignored settings file was restored to its exact pre-test SHA-256.
- Touched-file rustfmt with edition 2024 and skip_children checked 33 first-party files. Thirteen matched rustfmt exactly. Sixteen have residual discrepancies overlapping Goal edits, recorded in the final formatting receipt.
- Evidence directory is the task's `calc-scripting` visualization directory. See `m6-final-r1-build`, `m6-final-r1-list`, `m6-final-r1-run`, `m6-lint-fix-run`, `m6-clippy-recheck`, `m6-format-lint-fix/receipt.json`, and `final-working-tree-source-sha256.json`.

- First-party Clippy exited 0 for library and app targets with the QA feature. The targeted cleanup removed 11 library warnings. The rerun retained 114 library and three binary warnings at pre-existing sites. The 14-case post-cleanup Nextest batch passed in 1.119 seconds with verified process teardown and drain.

## Windows native verification

The follow-up rebuilt the pushed tip `2e265c91ab9dc3c5068f3e5b1b2c61b92c7c2f9f` with the `fast-dev` profile and `qa-framebuffer-capture` feature. The executable SHA-256 was `E7879FA77F9F51E5BFCA8BEC15B0C87F12AB1E35433AA6E2E94D080798698E63`. The disposable profile disabled both Paste Sequence shortcuts before construction. App framebuffer captures and window-relative input provided evidence where Windows Graphics Capture and detached accessibility could not.

| Scenario | Outcome |
| --- | --- |
| UI-1 | Passed. The H2 script produced the expected results and source Undo/Redo. Pinning `answer` showed 120. Editing `unit_price` from 12.5 to 20 updated its source assignment and preserved both `count` assignments. The binding fixture produced 2, 2, 5, 2, 5. Programmer Inspector compared `r1` and `x` as 2 and 5. |
| UI-2 | Passed in the closeout. Escape dismissal, completion-off keys, nested parameter hints and dark/light readability were exercised. |
| UI-3 | Passed in the closeout. Direct and context-menu diagnostic navigation visibly highlighted line 3, and clicking a folded diagnostic visibly revealed and highlighted line 4. |
| UI-4 | Passed. Showed input and watch panels, rejected an invalid numeric edit without changing source, saved a named Pad, opened a blank Pad, and reopened the saved document. Its selected input and named watch returned when the panels were shown. In the 640 × 460 profile, both `x` assignments were separately selectable by line label and Results remained reachable by scrolling. |
| UI-5 | Passed. Verified Auto, fixed decimal, grouping, scientific and scalar percent display. The exact integer `9007199254740993` retained all digits, the unit remained attached and the vector stayed canonical. Pasting canonical and formatted scalar copies yielded `12345.1` and `1.23e6%` respectively. Format changes preserved the source. |
| UI-6 | Partial. Detached Keep Editing, Save, Discard, reopen, current revision and launcher responsiveness passed. Deterministic worker supersession passed. Native in-flight supersession remains unverified. |
| UI-7 | Passed. The H2 Mermaid fixture had four nodes and three edges. Explicit RL layout placed arrows right to left. Added a node visually, changed to BT, and verified upward arrows after Auto-layout. Undo restored the previous geometry and Redo reapplied BT. Saved, opened a blank draft, and reopened from Recent. Both comments survived and the exported Mermaid source matched the saved draft exactly. A legacy sequence diagram rendered Alice, Bob and the Hello message. |
| UI-8 | Passed. Changed synthetic source and pasted back the exact copied bytes. Copy QR reported a successful image clipboard write. Save PNG wrote a 232 × 232 PNG, while cancel left no destination file. Changed correction to High and verified the same text, High selection and Advanced state after Pop Out → Dock. Popped out again, typed in the independent launcher query, and closed the native QR window. A fresh command opened a distinct window in the remembered presentation. Docking it showed an empty source and default Medium correction. Raster clipboard fidelity remains covered by the automated tests. |

Evidence is under the task visualization directory `calc-scripting/ui-followup`. See `root-captures-r1/framebuffer-004.png` and `framebuffer-006.png` for RL/BT, `framebuffer-012.png` for the legacy preview, `bt-roundtrip.mld` and `diagram.mmd` for matched source, `qa-native-42.png` for QR export, `qr-final-captures/framebuffer-003.png` and `framebuffer-004.png` for retained and reset QR correction, and `calc-captures` and `compact-light-captures` for Calc. The follow-up used synthetic clipboard content only. Process receipts are in the sibling `ui-followup-process*` directories.

## Windows acceptance closeout

The closeout began at `b4e2592602b5deac23eb8a1b8362de56fff6c554`. Only the ledger differed from the source commit of the original executable. The targeted Escape correction is `7c8f7fe10c549531999031474ce2e48153d5160a`. Both builds used a disposable profile with Paste Sequence next/cancel shortcuts disabled and Remember last source off. Evidence paths below are relative to the task visualization directory `calc-scripting`. Normal dark and light rendering was exercised using the disposable profile across launches.

| Subtest | Result | Fixture, input and observation |
| --- | --- | --- |
| UI-2A | PASS after fix | In `total = 20` / `tot`, Escape twice reproduced the close prompt in separate trials, with completion visible before each trial. After the fix, one Escape hid the `total` suggestion, kept source focus and unchanged text, and stayed dismissed across later frames. Left moved the caret and the suggestion returned. Before-fix `ui-closeout/assist/framebuffer-001.png`, `002` and `003`. Corrected before/dismissed/returned `ui-closeout/retest/framebuffer-001.png` through `003`. Docked and pop-out regression tests passed. |
| UI-2B | PASS | With completion unchecked, added `to`, pressed Enter, typed `another_value = 5`, then entered a new `tot` and pressed Tab. Exact source selection showed Enter inserted a separate line and Tab moved focus without expanding `tot` or activating the launcher. Re-enabled completion, refocused Source and confirmed `total` returned. See `ui-closeout/assist/framebuffer-004.png` and `005`. |
| UI-2C | PASS | With `result = sum(range(1, 10, 2))`, Left moved from the end to the surrounding `sum(values)` hint, then into `range` where moving between arguments changed `step` to `stop`. Disabled and re-enabled hints independently, verifying disappearance and return with source unchanged. Dark evidence `ui-closeout/assist/framebuffer-006.png` and `007`. Light editor evidence `ui-closeout/retest/framebuffer-001.png` and the prior compact-light nested hint `ui-followup/compact-light-captures/framebuffer-001.png`. |
| UI-3A | PASS | `seed = 5`, `good = seed * 2`, `bad = missing_input + 1`, `final = seed + 1` produced `good = 10`, `final = 6` and the `r3` diagnostic. Clicking it visibly highlighted only physical line 3. See `ui-closeout/assist/framebuffer-008.png` and `009`. |
| UI-3B | PASS | Right-clicked `r3`, chose Go to source, and captured line 3 highlighted with editor focus. Appended `typing_check = 9` successfully afterward. See `ui-closeout/assist/framebuffer-010.png` and `011`. |
| UI-3C | PASS | Inserted `# region hidden` before `good` and `# endregion` before `final`. Folded the region, then clicked `r4`. The source expanded and highlighted physical line 4. Ctrl+G to previously hidden line 3 selected its result. Full source selection still matched the original script with no placeholder. See `ui-closeout/assist/framebuffer-012.png` through `015`. |
| UI-6A | PASS | Nextest ran only `worker_cancels_old_work_and_coalesces_latest_pending_revision` and `worker_reports_panics_and_accepts_later_revisions`. Both passed in 0.066 seconds. The synchronized test cancels held work, coalesces the pending revision and checks completion eligibility. See `ui-closeout-worker-run/stderr.log`. |
| UI-6B | UNVERIFIED in flight | The 1,000-by-1,000 nested loop reached the explicit work limit before replacement in this follow-up as well. The replacement `fresh_marker = 777` / `fresh_marker` yielded 777 on both result rows with no obsolete total result. Pop Out → Dock and independent launcher input passed in the same native session. See `ui6-lifecycle/root-captures/framebuffer-007.png` and `008`. |
| UI-6C | PASS | Saved Native Lifecycle Test with `total = 10` / `total`, edited it to 20, then popped out and requested Windows close. The detached capture shows the dirty 20 and the three close choices. Keep Editing canceled close. Docking the same window showed the draft still dirty, with both results at 20 and the saved source still at 10 until the subsequent Save. See `ui6-lifecycle/captures/multi_launcher_20261114_145717.png` and `multi_launcher_20261114_145836.png`. |
| UI-6D | PASS | Popped out the retained dirty draft again, requested Windows close and chose Save. The detached window retired and the saved file contained `total = 20` / `total`. Reopened from the launcher and selected Native Lifecycle Test. Both results were 20 and the draft was clean. See `ui6-lifecycle/root-captures/framebuffer-003.png` and `004`. |
| UI-6E | PASS | Changed the reopened draft to 30, verified both results and dirty state, popped out, requested Windows close and chose Discard. The native window retired and the saved file still contained 20. Reopened the named document and verified the clean source and both results returned to 20. See `ui6-lifecycle/root-captures/framebuffer-005.png` and `006`. |
| UI-6F | PASS | Edited the reopened source to 21 and verified both current results. Popped out, confirmed exactly one live Pad and edited the independent launcher query. Docking retired that child and retained source/results at 21. No older 30 result or duplicate window appeared. See `ui6-lifecycle/root-captures/framebuffer-007.png`. |
| CP-10 | PASS | Disabled auto-close inserted only `(` or `[`. Enabled pairing produced `(5 + 3)` and `[5, 3]` with typed closers reused. Selecting `10 + 2` and typing `(` wrapped it, Undo restored it, and Redo restored the wrap. With completion off, Enter after `for n in range(3):` indented the body four spaces, and Enter after `total = total + n` indented the next line four spaces as confirmed by full source selection containing `    # marker`. Copy/paste round-tripped the indented source. |

Three directly affected post-fix tests passed in 0.951 seconds, including the new docked and pop-out Escape regressions. Three selected retained-host lifecycle cases also passed in 0.306 seconds as automated evidence. See `ui-closeout-fix-run/stderr.log` and `ui-closeout-lifecycle-run/stderr.log`. These results do not substitute for the detached Windows close decisions.

The UI-6 lifecycle follow-up used the existing executable at source commit `7c8f7fe10` from ledger tip `02349afea`. The test profile disabled both Paste Sequence hotkeys and Remember last source. A window-only application capture provided detached close-control coordinates, and root framebuffers and saved-file reads verified the outcomes. The process exited 0 with cleanup and output drain verified in `ui6-lifecycle-process/result.json`. Native in-flight supersession is the only remaining UI-6 gap. A pass requires observing the older revision pending when replacement is submitted. No work limits or production source were changed.

## Final verification and handoff

**IMPLEMENTED / NATIVE ACCEPTANCE LIMITED.** This verification-only pass used branch `feat/calc-scripting-studio-polish` at `7dc4fc5ae73a10bda6063962ed274c0bb9ff42ca`, also the upstream tip at entry. The checkout was clean and ahead/behind was 0/0. The executable was the existing Windows `fast-dev` build with `qa-framebuffer-capture`, sourced at `7c8f7fe10c549531999031474ce2e48153d5160a`, SHA-256 `B49E1571D761382FAC6AF8E66ABBD3264A9FD540BDF423838604036AAAE8BD95`. All intervening commits changed only this ledger. Both owned-process receipts show exit 0, verified cleanup, and verified output drain. The disposable profile disabled both Paste Sequence shortcuts. The verification made no production code changes.

| Check | Status | Observation |
| --- | --- | --- |
| Git and prior evidence | PASS | Start HEAD and upstream matched. The only source change since the original polish build was the Escape correction already verified in the preceding closeout. |
| Reassignment and loop | PASS | The exact script below produced r2 subtotal 10, r3 reassigned subtotal 11, r6 sum_items 6, and r7 answer 17. This verifies that the final expression uses the latest subtotal. |
| Editable inputs and output watches | PASS | Opened View, enabled both panels, selected price on line 1, and pinned answer. Applied 8 in the input control. Source changed from price 5 to 8, subtotal rows became 16 and 17, loop sum remained 6, and answer in Results and the watch became 23. |
| Undo/Redo | PASS | With focus in Source, Ctrl+Z restored price 5 and answer 17. Ctrl+Shift+Z restored price 8 and answer 23. The input control and watch reflected both steps. |
| Layout and compact reopen | PASS | The 1150 × 850 launcher displayed Source and Results side by side. Restarted the same disposable profile at 640 × 560 and observed the stacked layout with the remembered source, then closed and reopened the Pad and observed both panes and answer 23. The compact layout used its ordinary scroll areas. The resize handle drag was not confirmed; the compact launcher size was set via the disposable profile before restart. |
| Native divider adjustment and independent persistence | UNVERIFIED | Attempted visible divider drags in both layouts. Screenshots showed the same divider positions and the profile ratios remained 0.50 wide and 0.48 stacked. A native-window retry reported the pointer was over the launcher. This did not establish that a divider drag reached its widget. No application defect was isolated, and persistence of user-adjusted ratios across close/reopen or restart remains unverified. |
| Focused divider tests | PASS | Used the retained QA test binary. Three selected tests passed in 0.247 seconds, exit 0. Their output and process receipt are indexed below. |
| UI-2, UI-3, detached lifecycle and CP-10 | PREVIOUSLY VERIFIED | Reviewed the evidence indexed in Windows acceptance closeout and its lifecycle follow-up. It remains applicable at the tested source revision. |
| UI-6B native in-flight supersession | UNVERIFIED | The previous Windows runs showed the old script hitting its work limit before replacement. UI-6A worker supersession passed deterministically. There was no new native attempt in this verification-only pass. |

```text
price = 5
subtotal = price * 2
subtotal = subtotal + 1
sum_items = 0
for n in range(1, 4):
    sum_items = sum_items + n
answer = subtotal + sum_items
```

Evidence paths below are relative to the existing `calc-scripting` visualization directory.

- `final-calc-verification/initial/framebuffer-001.png` is the baseline script and numerical results. `framebuffer-002.png` through `framebuffer-005.png` are input/watch before Apply, after Apply, after Undo, and after Redo. `framebuffer-006.png` and `framebuffer-007.png` through `framebuffer-010.png` record the wide divider baseline and unchanged observations after attempts.
- `final-calc-verification/compact/framebuffer-001.png` is the stacked layout after restart, `framebuffer-002.png` through `framebuffer-004.png` show the unchanged divider after attempts, and `framebuffer-005.png` shows the compact Pad after close/reopen.
- `final-calc-verification-process/result.json` and `final-calc-verification-process-compact/result.json` identify the process, executable, profile, and teardown. `final-calc-divider-run/result.json` holds the exact arguments, exit code and watchdog receipt. `final-calc-divider-run/stderr.log` records the three individual passes. The selected command was `cargo nextest run --binaries-metadata <evidence>/ui-closeout-fix-build/stdout.log -E 'test(splitters_drag_in_both_layouts_and_save_once_on_release) | test(splitter_retains_discarded_motion_and_move_coalesced_with_release) | test(split_preview_rejects_retired_passes_and_clamps_extremes)' --no-tests fail --no-fail-fast --retries 0 --test-threads 1`.
- Prior UI-2 evidence is under `ui-closeout/retest` and `ui-closeout/assist`. UI-3 highlights are in `ui-closeout/assist/framebuffer-009.png`, `framebuffer-011.png`, and `framebuffer-014.png`. Detached lifecycle evidence is in `ui6-lifecycle/captures` and `ui6-lifecycle/root-captures`. The prior worker and acceptance test logs and exact command receipts remain in `ui-closeout-worker-run`, `ui-closeout-fix-run`, and `ui-closeout-lifecycle-run`.

Recommend closing as **IMPLEMENTED / NATIVE ACCEPTANCE LIMITED** with the two remaining native observations retained. The manual divider check is to drag each orientation mouse-down to mouse-up, then confirm both distinct adjusted positions after Pad reopen and application restart. Native UI-6B requires visible Pending for the old revision when its replacement is submitted. The actual post-reporting tip is recorded in the external handoff receipt rather than inside its own commit.

## Requirement matrix

| Requirement | Checkpoints | Implemented | Automated verification | Native verification |
| --- | --- | --- | --- | --- |
| MMD-01 | M1-A, M1-B | Yes | Passed | Passed |
| MMD-02 | M1-A | Yes | Passed | Passed |
| MMD-03 | M1-A | Yes | Passed | Passed |
| MMD-04 | M1-A | Yes | Passed | Passed |
| MMD-05 | M1-B | Yes | Passed | Passed |
| CP-01 | M2-A/C/D | Yes | Passed | Passed |
| CP-02 | M4-A/C | Yes | Passed | Passed |
| CP-03 | M3-B, M4-C | Yes | Passed | Passed |
| CP-04 | M3-B | Yes | Passed | Passed |
| CP-05 | M4-A/C | Yes | Passed | Passed |
| CP-06 | M3-A/C | Yes | Passed | Passed |
| CP-07 | M3-A/C | Yes | Passed | Passed |
| CP-08 | M3-B | Yes | Passed | Passed |
| CP-09 | M4-B/C | Yes | Passed | Passed |
| CP-10 | M3-C | Yes | Passed | Passed |
| CP-11 | M3-C | Yes | Passed | Passed |
| CP-12 | M3-A/C | Yes | Passed | Passed |
| CP-13 | M2-B | Yes | Passed | Passed |
| CP-14 | M2-B | Yes | Passed | Passed |
| CP-15 | M2-C/D | Yes | Passed | Partial, UI-1, UI-6 |
| CP-16 | M2-B/C | Yes | Passed | Passed |
| QR-01 | M5-A | Yes | Passed | Passed |
