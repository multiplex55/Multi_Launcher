# Calculation Pad expansion and File Search metadata filters

The execution contract is preserved in commit `09ca2427c5da5782eb540a538ce0f1bb71a39434` at `docs/plans/CODEX_GOAL_CALC_PAD_AND_FILE_SEARCH.md`. The task branch starts at the previous Goal's final committed tip.

| Git field | Value |
| --- | --- |
| parent_branch | `codex/calc-pad-examples-reference` |
| parent_tip_sha | `aeec0fb900364a6018cade76a1017fb2deeaa469` |
| task branch | `feat/calc-pad-and-file-search` |
| baseline_sha | `d17b595053ce809a772e0fab0905d1268bf9c8aa` |
| baseline push | `git push -u origin HEAD` succeeded and configured the matching upstream |

The previous Goal's final handoff in Codex chat `01a1dc67-898d-7e81-9fa1-0105f290aafc` names the parent SHA. The local and remote-tracking predecessor refs agreed. The checkout was clean before and after the switch, and the baseline's direct parent was verified. One existing checkout was used.

| Checkpoint | State | Evidence |
| --- | --- | --- |
| M0-A | Verified | Exact parent and baseline, fresh task branch, successful push |
| M0-B | Verified and pushed | `8d76bb28e0f7dd82229d6d3bd53e1d473d569079`. Five Calc Pad and three File Search baseline cases passed. |
| M1-A | Verified and pushed | `80d790e5af3eab84b5b2435a6f4385646c1be2cf`. Shared analysis and evaluator routes. 13 focused tests passed. |
| M1-B | Verified and pushed | `5a724285e402db21411c5cf3454ed9a797e22c61`. Shared editor/observer mapping; focused batches passed 20/20 and 3/3. |
| M2-A | Verified and pushed | `ffbc28d6a0fc3feecca7ed489846dd9dd177d026`. Versioned options/recovery, evaluation identity and related-reference edit seam. Focused batch passed 20/20. |
| M2-B | Verified and pushed | `54567303d7ff18aa5ce751a044cf103093b4e30e`. Manual lifecycle, immutable stale results, guarded actions. Focused batches passed 18/18 and 4/4. |
| M3-A | Verified and pushed | `fad44d1cf51e88a2093eb9c1d74de86a1e2fa13c`. Exact arithmetic, bounds, canonical copy and approximation. Batches passed 12/12 and 7/7. |
| M3-B | Verified and pushed | `7499ede1b9c064030df389bea999e3bf9993a8d8`. Exact display, comparison and current copy. Batches passed 13/13 and 5/5. |
| M4-A | Verified and pushed | `7fa01717e6fa7fd8e298255aaf67b18b24937e42`. All ten helpers. Focused batch passed 10/10. |
| M5-A | Verified and pushed | `0cdba26a511b5c3abb3c0e14b9b433bbb2608a9a`. Formula runtime. Batches passed 11/11 and 7/7. |
| M5-B | Verified and pushed | `c5a7fbfed5fe53dbd31ee0944ff6232c088d647b`. Formula editor and examples. Focused batch passed 13/13. |
| M5-C | Verified and pushed | `3aa2402776a075c74673cc4e792535923871902d`. Conditional blocks, loop flow and recovery. Final batch passed 16/16. |
| M6-A | Verified and pushed | `da6d58cca7453770c95079c3a031dee10941f608`. Calendar domain. Batches passed 11/11 and 5/5. |
| M6-B | Verified | Calendar presentation, comparison, copy and examples. Focused batch passed 14/14. |
| M7-A | In progress | Extended source admission, evaluation, worker and persistence. |
| M7-B through M13-B | Pending | Sequential checkpoints follow the Goal contract. |

## Requirements and live owners

| ID | Implementation | Verification | Owners and boundaries |
| --- | --- | --- | --- |
| C20 | Implemented | Focused automated domain/editor checks passed. Native integrated check pending. | Calc Pad document, nested script/loops evaluators, editor and observer mappings |
| C24 | Implemented | Focused scheduler, persistence and host tests passed. Native integrated check pending. | Session scheduling, worker stamps, persistence/recovery and companion observers |
| C25 | Implemented | Focused exact/legacy, formatting, settings, copy and example checks passed. Native integrated check pending. | Typed document/script arithmetic, existing numeric dependencies, result formatting and canonical copy |
| C21 | Implemented | Focused numeric, compatibility, hint and executable example checks passed. | Script function catalog and signatures, editor hints, executable examples |
| C18 | Implemented | Focused runtime, editor, navigation and examples passed. Native integrated check pending. | Shared syntax and bindings, typed script invocation, editor and dependencies |
| C27 | Implemented | Focused conditional, nested flow, rollback and source/editor regressions passed. Native integrated check pending. | Existing loop block parser, transactional execution and editor structure |
| C22 | Implemented | Focused arithmetic, DST, comparison, copy/observer and examples checks passed. Native integrated check pending. | Existing calendar and named-zone domain, typed Pad operations and formatting |
| C26 | Pending | Pending | Source limits, session/history, worker, saved and recovery envelopes, editor. Synthetic value slots require review when expanding physical row capacity. |
| C14 | Pending | Pending | Binding identity, session history/metadata, guarded editor preview and apply |
| C6 | Pending | Pending | Literal input discovery and replacement, persisted Pad input references, existing input drafts |
| C4 | Pending | Pending | Session-owned bounded worker evaluation and accepted Config Menu actions |
| C5 | Pending | Pending | Committed snapshots, utility window host and render lifecycle, size/presentation settings and clipboard |
| F3 | Pending | Pending | Search model/settings, executor/coordinator, WalkDir/native/ripgrep/Everything, dialog filters and `domain` cases |

## Verification evidence

Local receipts are stored under `target/calc-pad-and-file-search-evidence`. The predecessor's diagram library test still includes an old fixture path. Scoped test builds may use the byte-identical archived fixture temporarily. Keep that copy out of Git and remove it after use.

### M0-B baseline

- `cargo nextest run -p multi_launcher --lib --test domain --cargo-profile fast-dev -E <eight intended cases> --test-threads 1 --no-run` completed with compiler exit 0 in 678.6 seconds under the 900-second compile deadline. It produced fresh library and domain binaries. The wrapper drained output and found 31 observed processes absent. It could not classify one process identity, PID 16032, at cleanup; a subsequent lookup found that PID absent. The temporary fixture was removed and its absence checked.
- Listed the cases in the fresh binaries and used their exact names to run the scoped baseline directly, avoiding a rebuild after fixture cleanup. The library batch passed 5/5 in 0.21 seconds under a 100-second deadline, covering example validation, v1 loading and upgrading, stable source line IDs, full-line comments and physical references, and typed loop rollback. The domain batch passed 3/3 in 0.05 seconds under the same deadline, covering filename extension filters, native content fallback, and criteria dirty state. Both runs exited 0 with verified cleanup and output drain. See `m0b-build`, `m0b-lib`, and `m0b-domain` receipts under the evidence root.
- Pre-existing compiler warnings were observed in vendored and first-party code. The baseline produced no feature-related test failures. Native interaction and final Clippy are later checkpoints.

### M1-A domain

- Scoped library `cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev -E <M1-A selectors> --test-threads 1` passed 13/13 with exit 0 in 2.28 seconds under the 100-second watchdog. The separate fixture-assisted compilation exited 0 in 283.84 seconds under a 900-second deadline. Both receipts confirmed cleanup and output drain. Selectors and results are in `m1a-build` and `m1a-run`.
- Coverage includes six physical/logical source cases, typed values, loop/header grouping, continuation errors, legacy row references, laziness, loop rollback, and existing examples. Editor integration remains M1-B.

### M1-B editor and observers

- Scoped library Nextest passed 20/20 in 2.58 seconds, including folding and projected highlighting, multiline hints and references, original-byte literal edit/undo, and physical result copying and navigation. Diff inspection exposed a dedented-continuation completion issue. The correction uses the logical owner's indentation and excludes the current assignment as a prior binding. Its three-test focused rerun passed in 1.95 seconds.
- Both batches used a 100-second watchdog. Separate compile stages finished in 330.53 and 210.43 seconds under the 900-second deadline. Receipts are in `m1b-build`, `m1b-run`, `m1b-correction-build`, and `m1b-correction-run`; cleanup and output drain passed.

### M2-A document and run identity

- The initial library compile exited 101 on a missed fixture call to the expanded Save As helper. After updating that call, an all-case diagnostic run passed 19/20. Its lone failure was the new v2 fixture converting a full-width document ID through `serde_json::Value`. The fixture now emits the exact numeric ID and asserts selected input and watch retention.
- Final library Nextest passed 20/20 with exit 0 in 3.03 seconds under the 100-second watchdog. The final separate compile exited 0 in 182.59 seconds under the 900-second deadline. Both cleanup and output drain were verified. Receipts are in `m2a-build`, `m2a-correction-build`, `m2a-correction-run`, `m2a-failure-diagnosis`, `m2a-final-build`, and `m2a-final-run`.
- Coverage includes v1 and v2 migration, options and recovery, rejection/preservation of future records, atomic writes, same-revision worker retirement, Save As and derived receipt lifetimes, and the source/reference undo seam. C14 must handle the case where the old watch name is pinned separately after a rename. The M2-A transaction currently refuses conflicting replay to avoid dropping that later pin.

### M2-B Manual scheduling

- Scoped library Nextest passed 18/18 in 2.58 seconds. The correction kept the already-finished but undelivered worker request coalesced and retained the accurate pending companion notice. Its four-test rerun passed in 2.10 seconds.
- Both batches used a 100-second watchdog. Separate compilation ran 246.89 and 159.13 seconds under the 900-second deadline. Cleanup and output drain passed. Receipts are in `m2b-build`, `m2b-run`, `m2b-correction-build`, and `m2b-correction-run`.
- Coverage includes six edits and read-only observers producing zero Manual attempts, explicit calculation, immutable stale source, cancel/coalesce/retry generations, Automatic restoration, result navigation, and preservation of Automatic canonical same-frame copy. Native controls and cross-feature behavior remain for integrated acceptance.

### M3-A exact-number domain

- Promoted already-locked numeric crates to direct dependencies. `Cargo.lock` changes only those dependency edges. Exact decimal/fraction arithmetic uses bounded rational operands, opted-in constructors, canonical copy, explicit approximate conversion and shared evaluation work.
- Scoped library Nextest initially passed 12/12 in 2.57 seconds (separate compile 433.96 seconds). Code inspection found a script-fragment panic path, a decimal canonical length mismatch at the admitted bit limit, and missing approximate scalar comparison. The correction batch passed 6/7; the sole failing assertion used the wrong expected sum for `range(4)`, which is 0 through 3. After correcting the fixture, the focused batch passed 7/7 with exit 0 in 2.37 seconds (final compile 66.84 seconds).
- Verified bounded process cleanup and drained output. Cached batches used the 100-second deadline and separate compiles used 900 seconds. Receipts are under `m3a-build`, `m3a-run`, `m3a-correction-build`, `m3a-correction-run`, `m3a-final-build`, and `m3a-final-run`.

### M3-B exact presentation and reference

- Exact, Decimal and Mixed Number are persisted independent display choices. Formatting never converts fractions to binary float, marks actual approximation and leaves canonical copy parseable. Current-only copy and observer guards remain in effect. The linked catalogue now contains 81 executable examples.
- Scoped library Nextest passed 13/13 in 2.61 seconds. A focused refinement for decimal-origin mixed text, bounded differences and pure literal validation passed 5/5 in 2.30 seconds. Separate compiles ran 367.61 and 196.34 seconds. Tests used 100-second deadlines and compiles 900-second deadlines. Cleanup and output drain passed. Receipts are in `m3b-build`, `m3b-run`, `m3b-correction-build`, and `m3b-correction-run`.

### M4-A numeric helpers

- Registered all ten helpers in the shared runtime and hint catalog and added all ten executable examples, for 91 total. Exact arithmetic and combinatorics preserve bounded integer/rational results above binary-float precision. Scalar and explicitly approximate behavior stays distinct.
- Scoped library Nextest passed 10/10 with exit 0 in 2.48 seconds under the 100-second watchdog. Separate compile finished in 327.12 seconds under the 900-second deadline. Cleanup and output drain passed. See `m4a-build` and `m4a-run`.

### M5-A formula runtime

- Compiled earlier-only document callables with isolated typed parameters and shared work/call limits. Definition rows carry no value; call dependencies identify invoked physical definitions. The function declaration grammar preserves legacy uses of the variable `fn`.
- Initial scoped library Nextest passed 11/11 in 2.71 seconds (332.25-second separate compile). Diff review exposed assignment syntax inside bodies and a missing structural boundary/rollback for declarations next to or inside loops. The correction's initial compile failed with E0425 because a new test was outside its own module. After moving the test, final focused Nextest passed 7/7 in 2.61 seconds after 213.56 seconds of compilation.
- Cleanup and output drain passed. The cached deadline was 100 seconds, with 900-second compile bounds. Receipts are in `m5a-build`, `m5a-run`, `m5a-correction-build`, `m5a-final-build`, and `m5a-final-run`. Current results from formula calls are available to comparison by row or binding. Fresh Inspector expressions keep the existing frozen-result scope.

### M5-B formula editor

- Shared authored declarations and mapped UTF-8 source spans between runtime and editor. Completion respects local parameters and earlier callable visibility. Structured hints preserve punctuation within braced parameters. The catalogue now includes 95 executable examples.
- Initial compile exited 101 because a local variable in a new test shadowed the completion function. After renaming it and reusing one scope per completion, scoped library Nextest passed 13/13 with exit 0 in 2.73 seconds. Compile took 338.21 seconds under the 900-second deadline; the cached run used a 100-second deadline. Cleanup and output drain passed. Receipts are in `m5b-build`, `m5b-correction-build` and `m5b-correction-run`.

### M5-C conditional control flow

- Added contextual blocks, Boolean condition evaluation, nearest-loop control outcomes, and physical/non-value control rows. Successful early exit preserves completed work. Existing error rollback and iteration accounting remain active, including structural errors in skipped branches.
- Initial focused run passed 13/14; a legacy loop-validation case exposed an earlier source-mapping regression where an unclosed delimiter absorbed subsequent clear dedented assignments. Recovery now reports the opening physical location and resumes at the new assignment without affecting valid dedented continuations. The first correction compile identified a missing explicit owner type, corrected to `Option<usize>`.
- Final Nextest passed 16/16 in 2.66 seconds with verified cleanup and output drain. Final separate compile ran 189.57 seconds. Cached tests used 100 seconds, compiles 900 seconds. Receipts are in `m5c-build`, `m5c-run`, `m5c-correction-build`, `m5c-final-build`, and `m5c-final-run`.

### M6-A typed calendar domain

- Added deterministic calendar constructors and operations, with compatibility for existing physical-time quantities. Named-zone resolution uses explicit fold/gap semantics and distinguishes local calendar shifts from elapsed time. Output constructors retain kind, zone/fold, offset and supported fractional precision.
- Initial scoped library Nextest passed 11/11 in 2.53 seconds. Corrected false duration precision rejection caused by f64 representation and blocked excess fractional clock digits before parsing. The focused correction run passed 5/5 in 2.26 seconds. Separate compilation ran 282.86 and 193.37 seconds under 900-second deadlines. Cached tests used 100-second deadlines; cleanup and output drain passed. Receipts are in `m6a-build`, `m6a-run`, `m6a-correction-build`, and `m6a-correction-run`.

### M6-B calendar presentation

- Show distinct zone/fold offsets and named weekdays, typed temporal comparison, and canonical versus formatted copy with current evaluation guards. Added eight calendar examples for 103 total; retained the original Variables group first for existing reference-window interactions.
- After fixing the inferred index in a new comparison test (initial compile exit 101), scoped Nextest passed 14/14 with exit 0 in 3.31 seconds. The successful separate compile took 219.13 seconds. Tested canonical/Manual copy, hints, same-instant comparisons, all executable examples, and legacy reference-window search/copy. Cleanup and output drain passed. Receipts are in `m6b-build`, `m6b-correction-build`, and `m6b-correction-run`.
