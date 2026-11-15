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
| M0-B | Verified | Live owners below. Five Calc Pad and three File Search baseline cases passed. |
| M1-A | In progress | Logical statement and physical source mapping domain. |
| M1-B through M13-B | Pending | Sequential checkpoints follow the Goal contract. |

## Requirements and live owners

| ID | Implementation | Verification | Owners and boundaries |
| --- | --- | --- | --- |
| C20 | Pending | Pending | Calc Pad document, nested script/loops evaluators, editor and observer mappings |
| C24 | Pending | Pending | Session scheduling, worker stamps, persistence/recovery and companion observers |
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
