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
| M2-B | Verified | Manual scheduling, immutable stale results, guarded actions and recovery. Focused batches passed 18/18 and 4/4. |
| M3-A | In progress | Bounded exact decimal/fraction domain and evaluator integration. |
| M3-B through M13-B | Pending | Sequential checkpoints follow the Goal contract. |

## Requirements and live owners

| ID | Implementation | Verification | Owners and boundaries |
| --- | --- | --- | --- |
| C20 | Implemented | Focused automated domain/editor checks passed. Native integrated check pending. | Calc Pad document, nested script/loops evaluators, editor and observer mappings |
| C24 | Implemented | Focused scheduler, persistence and host tests passed. Native integrated check pending. | Session scheduling, worker stamps, persistence/recovery and companion observers |
| C25 | Pending | Pending | Typed document/script arithmetic, existing numeric dependencies, result formatting and canonical copy |
| C21 | Pending | Pending | Script function catalog and signatures, editor hints, executable examples |
| C18 | Pending | Pending | Shared syntax and bindings, typed script invocation, editor and dependencies |
| C27 | Pending | Pending | Existing loop block parser, transactional execution and editor structure |
| C22 | Pending | Pending | Existing calendar and named-zone domain, typed Pad operations and formatting |
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
