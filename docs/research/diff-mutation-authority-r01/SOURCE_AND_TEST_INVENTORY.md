# AUD-F002 M1-A — source and existing-test inventory

Inspected 2026-10-29 at source/base `59156d96adcc73939c7171487a0cb582e3286587`, author starting HEAD `1725f0fdab6de1993cad9dc99e164467454c4a41`. Parent owns branch `codex/research-diff-mutation-authority-r01`. No source, test, manifest or dependency edits belong to this milestone.

`S` IDs below are current repository sources; locators are one-based source lines. Hashes use UTF-8 source bytes after CRLF→LF normalization, SHA-256, with no other content transformation. They identify the inspected content across checkout newline settings, not a build/test result. Hash generation is a read-only source receipt, not an optional proof-model invocation or a second validation campaign.

## Source index

| ID | Path and inspected responsibility | Principal locators |
|---|---|---|
| S1 | [Diff plan/executor](../../../src/diff/file_ops.rs) | 30–65 identity; 137–207 root/relative/containment; 209–479 planning/walk/add_file; 485–553 report/fresh/dirty/root; 555–698 copy/cancel/worker; 705–898 delete/backend/worker |
| S2 | [Diff GUI controller](../../../src/gui/diff_dialog/mod.rs) | 629–632 error display; 1018–1178 plan/dirty/preview dispatch; 1249–1272 completion; 1448–1460 rescan; 1831–2064 inspected GUI tests |
| S3 | [Operation preview](../../../src/gui/diff_dialog/operation_preview.rs) | 1–132 captured plan, direction/counts/overwrite/recycle wording and execute gates |
| S4 | [Folder runtime](../../../src/diff/folder_runtime.rs) | 148–160 activity; 288–320 cancellation/generation/rescan; runtime ownership only, no unrelated refinement audit |
| S5 | [Existing native file operations](../../../src/file_operations.rs) | 49 native ID; 1150–1270 guarded Rename; 2942 onward preparation; 4725 onward chain reopening; 6004 onward source open; 8718 onward no-replace rename; 9020 onward directory/object open; 9231 ID; 9257 final path |
| S6 | [Diff domain tests](../../../tests/domain_cases/diff_file_ops.rs) | Seven exact test bodies below, read only |
| S7 | [Cargo manifest](../../../Cargo.toml) | 108 requested native windows crate; 181 trash version/features |
| S8 | [Cargo lock](../../../Cargo.lock) | 5540 trash 5.2.4 and its windows 0.56.0 dependency; dependency resolution only |
| S9 | [Domain suite inclusion](../../../tests/suites/domain.rs) | 13 includes Diff file-ops tests |

Pinned historical reference: [AUD-002 at research commit cc94dbf](https://github.com/multiplex55/Multi_Launcher/blob/cc94dbf4f98f155bcdef81165f647cbafb472eb0/docs/research/multi-launcher-research-20261028-r01/01-static-audit/findings/AUD-002-diff-mutation-identity.md), read by `git show` because that research tree is absent from the fresh master baseline. It is reference material; current packet and source govern. Current inspection sharpens its broad executor summary: directory creation does not run all per-file guards, and executor reports are not displayed per item by the current GUI.

## Exact dependency source

Public cached registry root inspected read-only: `C:/Users/Jay/.cargo/registry/src/index.crates.io-1949cf8c6b5b557f/trash-5.2.4/`. Package source metadata identifies author repository `https://github.com/ArturKovacs/trash`. Cargo.lock package checksum: `0102cfb632fbe1adefa067036d4f383045091ae8d4fc4c2c14d3c0f92d878972`.

| ID | File | Relevant scope |
|---|---|---|
| D1 | `src/lib.rs` | 84–126 context/public delete; 228–260 canonicalize parent/rejoin leaf and root rejection |
| D2 | `src/windows.rs` | 39–76 IFileOperation delete path/flags/completion; 272–318 thread-local COM initialization feature/lifetime. Other recycle-bin list/restore/purge methods were not adopted as Diff execution evidence. |
| D3 | `Cargo.toml.orig` | Exact author package/dependency/feature metadata; no installation or modification |

The unavailable remote source retrieval and successful public docs are recorded in [API references](API_REFERENCES.md). Cache existence and source inspection do not claim a native call ran, or that a cache hash independently authenticates a published package archive.

### Source hash receipt

| ID | LF-normalized SHA-256 | Lines |
|---|---|---:|
| S1 | `5b10ed70763350aed1111f393e4cd28e32edb0c21e175cd331080cd0e844db18` | 898 |
| S2 | `a1a3bbd448985cbede5075e1c128ab0c5a936eb82e97aa04f9309402625b9bb0` | 2330 |
| S3 | `610f0369dd8768f61e2509f336279ed2cd1fc66a32260b6270b066f3d9ee0b4c` | 436 |
| S4 | `2ff3cfb73f3133583c0a971f8ad883c1c16c4e0be6401723bc0270de75f7ee86` | 611 |
| S5 | `4c6dff5ce9a430b4de6d5ce84dad6354463e68af559c79758e93019f88dce2b9` | 11111 |
| S6 | `86fbc5fac8d7a109ec20e0889f209106cb5344a3b7793a273274d9963452094a` | 186 |
| S7 | `c5dff09ba93725cbbfa859c753de3e7fb17a405dd515e713cc309a162dd539ed` | 300 |
| S8 | `5c83274678ce6bb1906384722f8e156b5b858027881456671da619bb1b62b8af` | 7524 |
| S9 | `6eb42c5544b981f54ba532dc30fa73099543223c5d5b230b2b5c0d1b484679ad` | 71 |
| D1 | `16c8e444f8b0f3acddc94fb42b99af1dabec333a0d109a036eaae4aa7eb2b97a` | 545 |
| D2 | `deb46739391f920f944604d35a171ce2dd9093679a9b55529fea8c592bfbeb31` | 318 |
| D3 | `f11d9560ce2c763d12ac7b4e9e745e73e8e89984f455fd94d1fa96cd2695e25d` | 75 |

## Existing tests — source inspected, NOT_RUN

All seven domain tests are in S6 and included by S9. The two GUI tests are in S2. Counts here are a bounded source inventory, not test discovery or runner output.

| Test and locator | What the inspected assertions establish as test intent | Evidence they do not provide |
|---|---|---|
| `copies_file_and_plans_overwrite`, S6:8 | Counts overwrite; requires confirmation; ordinary executor result and destination bytes | No replacement interleave or effect-time destination binding |
| `recursive_copy_deduplicates_nested_selection`, S6:29 | Parent/child selection becomes one file and one directory; ordinary recursive copy result | No atomic subtree snapshot or partial-failure cleanup |
| `rejects_escape_spellings_and_destination_conflict`, S6:56 | Refuses traversal/absolute/drive/backslash forms and destination-directory conflict | No concurrent ancestor/reparse replacement fixture |
| `recycle_failure_never_falls_back_to_permanent_delete`, S6:95 | Fake recycle returns failure, Failed result, permanent count stays zero, original fixture remains | Fake backend does not prove native Shell targeting, reversibility or flags |
| `right_to_left_plan_retains_direction_and_executes_previewed_object`, S6:121 | Captured plan clone equality, right-to-left copy and ordinary bytes | Its name does not prove native object identity across external replacement |
| `delete_normalizes_parent_child_and_dirty_descendant_blocks_parent`, S6:144 | Parent/child normalization; dirty descendant blocks backend invocation | No ongoing dirty-editor lock or per-descendant identity contract |
| `gui_spawn_boundary_rejects_permanent_deletion`, S6:168 | GUI worker rejects Permanent and fixture remains | Does not remove the direct executor's explicit Permanent mode |
| `principal_folder_workflow_uses_injected_controller_boundaries`, S2:1831 | Injected scanner/selection flow, retained child view, captured preview copy passed to synchronous executor, rescan/selection retention | No worker/native timing or displayed failed/cancelled report assertion; its ordinary copy result is checked by the test itself |
| `gui_delete_planning_is_recycle_only`, S2:2031 | GUI plan is Recycle | No guarantee that the Windows Shell recycles every possible target |

No new/changed tests were required for this documentation milestone. Direct source guards for changed metadata/root identity and cancellation were also inspected; this inventory does not invent dedicated tests or passing results for them. Native Rename tests were not executed or imported as Diff acceptance.

## Precisely missing evidence

Deterministic pre/postvalidation substitutions, independent source/destination authority, effect-time no-replace versus approved overwrite, directory/partial-failure ownership, actual Shell targeting/recycling, and GUI presentation of failure/cancelled reports are not established by these nine source-inspected tests. If M2 selects a stronger future contract, the existing domain fake-backend seam and GUI controller seam are the focused locations for changed behavior; the [single conditional native protocol](IDENTITY_PROOF.md#5-one-conditional-later-native-protocol--not_run) covers only a load-bearing native claim. No broad regression campaign is inferred.
