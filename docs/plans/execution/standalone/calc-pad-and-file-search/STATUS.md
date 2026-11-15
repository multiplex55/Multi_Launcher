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
| M6-B | Verified and pushed | `ae72a9246fdd000ba41fc7750e6f49e43c5cfcb0`. Calendar presentation and examples. Focused batch passed 14/14. |
| M7-A | Verified and pushed | `07857478c05296eda7b3554b3399d27ee09887a5`. Extended source lifecycle, worker, row slots, save/recovery and atomic edits. Batch passed 19/19. |
| M7-B | Verified and pushed | `9039f3296226184de3aaa37cf45fd12f0b919fd5`. Profile and usage controls, independent admission ceilings and capacity checks. Final scoped batch passed 9/9. |
| M8-A | Verified and pushed | `f0f87495d5cf2d2bafd1de456804cca2f188476a`. Scope-aware source plans and atomic source/reference history. Focused batches passed 12/12 and 6/6. |
| M8-B | Verified and pushed | `c796de9d7d049fcf81ec26ea8c36d0c7a72d6ea7`. F2 and right-click preview, retained host actions, stale and history protection. |
| M9-A | Verified and pushed | `c48bed1d9c7df339e6939ace09a70728401180be`. Typed input definitions, retained v3 configuration and source projection. Focused batch passed 12/12. |
| M9-B | Verified | Preferred controls, exact/local drafts, guarded Apply and stable IDs. |
| M10-A | In progress | Bounded immutable sensitivity analysis and current-only publication. |
| M10-B through M13-B | Pending | Sequential checkpoints follow the Goal contract. |

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
| C26 | Implemented | Focused edit, worker, recovery, storage, usage and capacity checks passed. Native integrated check pending. | Extended evaluates at 128 KiB/1,024 rows. Edit and storage admission retain 256 KiB/2,048, with a 4 MiB envelope. |
| C14 | Implemented | Focused scope, contextual syntax, transaction, persistence and editor/host tests passed. Native integrated check pending. | Binding identity, session history/metadata, guarded editor preview and Apply |
| C6 | Implemented | Focused typed control, persistence, source Apply, stale metadata and Undo checks passed. Native integrated check pending. | Source-authoritative typed input configuration and accepted draft Apply |
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

### M7-A source capacity lifecycle

- Standard evaluation remains 64 KiB/256 rows; Extended and absolute source admission use 256 KiB/2,048. Both records use a shared 4 MiB envelope. Physical rows remain distinct from runtime binding slots. GUI Extended uses the coalescing worker even without control flow. Undo keeps its existing entry and byte budget.
- Initial compile exited 101 on a fixture calling private recovery store methods. The fixture now uses the production writer boundary. The correction also rejects oversized Find/Replace content atomically, including when the widget would otherwise truncate a paste. Preflight avoids analyzing over-profile Standard source before returning its diagnostic.
- Scoped Nextest passed 19/19 with exit 0 in 2.90 seconds after a 317.37-second separate compile. It covers high rows, UTF-8 and trailing rows, profile retirement, save/reopen/recovery, exact source and stable IDs, atomic oversized edits, older versions and envelope checks. Cleanup and output drain passed. Receipts in `m7a-build`, `m7a-correction-build`, and `m7a-correction-run`.

### M7-B usage and capacity

- Added per-document profile controls, source/evaluation usage and independent 90% warnings. Large Results snapshots draw visible rows, including stale Extended results still displayed after switching to Standard. Navigation can jump to a physical row outside the painted range.
- The initial measurement at 255,647 bytes and 2,000 rows showed about 482 ms for the first result frame, reduced to about 221 ms with visible-row rendering. A subsequent expanded-editor pass still took about 264 ms warmed and 636 ms across three offscreen-navigation frames. Based on those measurements, the Extended evaluation ceiling is 128 KiB and 1,024 physical rows. Text above it can still be edited, saved and reopened within the independent 256 KiB/2,048-row absolute admission limits. The 4 MiB serialized envelope and existing work/history limits remain in force.
- Final deterministic headless observations in the same fast-dev profile: Standard, 62,751 bytes/248 rows, 10.38 ms folded typing, 18.52 ms calculation, 88.64 ms first populated frame. Extended, 130,638 bytes/1,000 rows, 22.81 ms folded typing, 70.13 ms worker calculation, 113.69 ms first populated frame, 112.83 ms warmed expanded frame, 304.79 ms across the three-frame offscreen jump. Extended process working set went from 29.07 MB to 35.14 MB. The fully expanded near-ceiling editor shows perceptible lag in this headless fixture. Windows interaction checks are pending.
- Channel-controlled Extended worker cancellation and replacement passed at 130,999 bytes/1,000 rows in 144.06 ms end to end. The retired generation never published. Tests also cover byte/row admission boundaries, over-profile source saving/reopening, folding, Find, full-source Copy, navigation and result-basis isolation.
- Focused library Nextest final batch passed 9/9 with exit 0, 2.05 seconds reported by Nextest and 4.21 seconds owned elapsed under the 100-second deadline. Separate final compile took 348.45 seconds under a 900-second deadline. Cleanup and output drain passed. Receipts: `m7b-build`, `m7b-correction-*`, `m7b-final-*`, `m7b-stale-*`, `m7b-editor-cache-*`, `m7b-validated-ceiling-*`.

### M8-A rename planning and transaction

- Static preview distinguishes document reassignment families from function-local parameters. It preserves active/inactive lexical spans, protected source, CRLF, physical references, actual conditional/loop/quantity/operator grammar and programmer radix notation. Conflicts, capture, malformed source, and oversized projections are checked before application.
- Apply checks document identity, revision/run basis, current metadata and the plan. The one source edit preserves physical line IDs and the allocator. Source-aware input names and watches follow the rename with one Undo/Redo. Ephemeral watch identities preserve chains, independently pinned/removed watches and source history at full watch capacity. Saved metadata keeps its established persisted shape.
- Initial focused library run passed 10/10. The first expanded contextual-alias run passed 10/11; its fixture used bare terminal `to` in a position where the legacy evaluator attempts a conversion. The corrected fixture uses valid punctuation-adjacent references, adds radix coverage, and passed 12/12. A separate destination safeguard braces `to` so a renamed reference cannot become an attempted conversion; exact before/after value checks and directly related regressions passed 6/6. Each cached run met the 100-second deadline, final elapsed 2.32 seconds; separate compiles met 900-second deadlines, final 181.52 seconds. Cleanup and output drain passed. Receipts: `m8a-*`, `m8a-correction-*`, `m8a-final-*`, `m8a-destination-*`.

### M8-B editor rename interaction

- Exposed editor-owned F2 and `Rename Variable…` on the right-clicked source name, including projected folded text. Preview shows the resolved scope and bounded before/after physical rows. An invalid or stale preview disables Apply. A retained current host transaction refreshes authoritative input/watch metadata, and normal Undo/Redo restores it.
- The initial compile and first corrective compile identified two mismatched error-handler sites. After restoring the existing session error path, the batch passed 8/9, including F2 focus, Manual-mode Apply/Undo, stale and retired-host checks. The pointer test fixture rendered its next click after an empty injected Cancel frame; refresh of the actual editor geometry and hit targets fixed the fixture. The final directly affected Nextest batch passed 4/4, including real egui context-event targeting and the existing results context menu. Cached checks stayed within 100 seconds, final elapsed 2.69 seconds. Separate compiles stayed within 900 seconds, final elapsed 153.57 seconds. Cleanup and output drain passed. Receipts: `m8b-build`, `m8b-correction-build`, `m8b-final-*`, `m8b-context-*`. Native desktop evidence remains pending.

### M9-A typed editable inputs

- Source-backed descriptors carry exact numeric kind and original logical statement/span. Configuration persists on the selected stable input ID in v3, with conservative defaults for existing v1/v2/v3 documents and strict legacy v2 decoding. Saved bounds and positive steps validate against their type or authored unit. Labels and decorative suffixes survive source-type changes; incompatible numeric limits require review.
- Draft validation retains fixed-width bounds and exact decimal precision, preserves wrappers, unit text, comments and CRLF, and reports out-of-range source without clamping it. Private projection uses the same literal slot without mutating the session. Ordinary bare radix source still follows the existing exact-scalar range; full-width values remain available through typed integer constructors.
- Focused library Nextest passed 12/12 with exit 0, in 0.42 seconds reported by Nextest and 2.53 seconds owned elapsed. Separate compile took 353.75 seconds under its 900-second deadline. Tests cover old metadata, named save/reopen, rename, stale source/host Apply, long exact decimal, source spans and type/unit boundary. Cleanup and output drain passed. Receipts: `m9a-build` and `m9a-run`.

### M9-B input controls

- Editable Inputs now offer labels, descriptions, decorative suffixes, numeric ranges and steps, a field, step controls, or a bounded slider. Typed exact grids use small ordinal coordinates, including fixed integers above the exact-scalar range. A bare radix assignment keeps its existing ±2^53 magnitude ceiling. Unsuitable sliders retain the ordinary field.
- Step and drag update drafts. Guarded Apply verifies the current document/revision, selected input/configuration and original logical statement before one source edit. It retains physical IDs even when adjacent reassignments become identical. Accepted changes use normal Manual/Automatic, recovery and history behavior. An out-of-range current value is flagged and remains in source.
- Initial scoped library Nextest passed 8/8. A focused audit constrained bare-radix generated values and reused direct scalar token parsing in form validation. The final batch passed 9/9 with exit 0, 0.29 seconds reported by Nextest and 2.41 seconds owned elapsed. The separate final compile took 191.51 seconds. Each cached check met the 100-second deadline and compiles met 900 seconds. Cleanup and output drain passed. Receipts: `m9b-*` and `m9b-final-*`. Native desktop evidence remains pending.
