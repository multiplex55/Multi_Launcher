# Integration manifest

Observation: parent branch receipt `2026-10-30T07:46:39.1610151Z`, source parity receipt `2026-10-30T07:49:29.0534420Z` at G10 baseline `535993a4fa32a5646e0ade6076c6941a71d47230`. [Branch receipt](goals/G10/roles/orchestrator/BASELINE_BRANCH_RECEIPT.json) and [source parity](goals/G10/roles/orchestrator/BASELINE_SOURCE_PARITY.json) are reused without a new Git/source audit. Parent later supplied G10 packet `874065dbe21f227c4c3189dc859ba5521b210eb5` (pushed); this proposal makes no final-head assertion.

| Identity | Exact ref / state |
|---|---|
| Fetched master / origin/master | `4a1283e6edd2c245f562a07dbe3dcc6a61061eed` |
| Program bootstrap | `02d0ac64d51655f43d6e958ea551f5bf658ee48d` |
| Accepted integration docs baseline | `ea3a7007e23090c1c2b13ea9018b29754579e4ef` |
| Newest accepted test change | `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43` |
| Last production behavior change | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G10 original dependency baseline | `535993a4fa32a5646e0ade6076c6941a71d47230` from accepted integration |
| G10 supplied planner packet | `874065dbe21f227c4c3189dc859ba5521b210eb5`; parent reported committed/pushed |
| Accepted G10 review/closing import | Review7318e862; closing `792b1a531a4b5a392be977de94afee4d745b8685` -> `2234ed892f8a3a5a6b2feb0579ac160f32e26aad`, normally pushed; PR1459 open, creation head `b128db10b0cb1652c4f28c96570ef282014c18c9`, base `4a1283e6edd2c245f562a07dbe3dcc6a61061eed` |

## Accepted source/test delta — 21 unique Rust paths (20 src + 1 external test)

Each row records the approved closing source donor, its exact blob, current baseline blob and accepted cumulative import carrying those bytes. Closing donor equality proves only the listed accepted path; it never authorizes wholesale import of a rejected donor tree. G01 and G05 earlier source lineage and all correction donors remain in [accepted checkpoint history](ACCEPTED_CHECKPOINTS.md). G09 rows are test-only. The four accepted owners are disjoint: G01 four paths, G05 four, G06 ten (nine src plus one external test), G09 three.

| Goal | Path | Approved closing donor | Donor blob | Current baseline blob | Accepted cumulative source/test import |
|---|---|---|---|---|---|
| G01 | `src/diff/file_ops.rs` | `13243515329ce4010bd6db981f1fd3bcc26f2dec` | `7e9fab3d4eb09a735f4c347ab86e0832fd3f8a66` | `7e9fab3d4eb09a735f4c347ab86e0832fd3f8a66` | `fd300ff748e54cee167a038337220e33beba2322` |
| G01 | `src/diff/folder_runtime.rs` | `13243515329ce4010bd6db981f1fd3bcc26f2dec` | `e7cac72dbc89977556a475045d7ab0410b5c59e3` | `e7cac72dbc89977556a475045d7ab0410b5c59e3` | `fd300ff748e54cee167a038337220e33beba2322` |
| G01 | `src/gui/diff_dialog/mod.rs` | `13243515329ce4010bd6db981f1fd3bcc26f2dec` | `ecfd6e76f38cbcf16c5b914b610cc47d1fa61fa8` | `ecfd6e76f38cbcf16c5b914b610cc47d1fa61fa8` | `fd300ff748e54cee167a038337220e33beba2322` |
| G01 | `src/gui/diff_dialog/operation_preview.rs` | `13243515329ce4010bd6db981f1fd3bcc26f2dec` | `a1d93a6951a0e335768ccbc3a7e39b6438440db8` | `a1d93a6951a0e335768ccbc3a7e39b6438440db8` | `fd300ff748e54cee167a038337220e33beba2322` |
| G05 | `src/hotfolder/conditions.rs` | `a78895c63f31c9f48ae579e3203c14fa3faa8c35` | `0559e8f9cc77430b210ab7949967ab8582cd6c99` | `0559e8f9cc77430b210ab7949967ab8582cd6c99` | `4999a47c68a9e4e5b8531cf31e018eda3eefda4f` |
| G05 | `src/hotfolder/execution.rs` | `a78895c63f31c9f48ae579e3203c14fa3faa8c35` | `4de797b4626218d13dcc1e080c24276f8350d7f3` | `4de797b4626218d13dcc1e080c24276f8350d7f3` | `4999a47c68a9e4e5b8531cf31e018eda3eefda4f` |
| G05 | `src/hotfolder/mod.rs` | `a78895c63f31c9f48ae579e3203c14fa3faa8c35` | `1bac86b002a8b90eaf0ec9855b757006ad43f38d` | `1bac86b002a8b90eaf0ec9855b757006ad43f38d` | `4999a47c68a9e4e5b8531cf31e018eda3eefda4f` |
| G05 | `src/hotfolder/model.rs` | `a78895c63f31c9f48ae579e3203c14fa3faa8c35` | `306d2cb9b4295e0e0a26cd38a9f782113294df76` | `306d2cb9b4295e0e0a26cd38a9f782113294df76` | `4999a47c68a9e4e5b8531cf31e018eda3eefda4f` |
| G06 | `src/hotkey/validation.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `acaeccaca9b7e923af40585df9a050d22b32eeb3` | `acaeccaca9b7e923af40585df9a050d22b32eeb3` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/main.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `b5306acb3060edf0cd189c111b9dc4e611dec6de` | `b5306acb3060edf0cd189c111b9dc4e611dec6de` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `aeff64e58ea0ae323891a11420ea1fdf541e84f7` | `aeff64e58ea0ae323891a11420ea1fdf541e84f7` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor/catalog.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `d7efb88647716a987b2015b5ab5c80019c9c8eda` | `d7efb88647716a987b2015b5ab5c80019c9c8eda` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor/mapping.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `9ba092473a809f25536b2ed4e846f893f2586dd4` | `9ba092473a809f25536b2ed4e846f893f2586dd4` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor/render.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `5bc4d19a6bc7acaae8f15cebe062f73b45867bec` | `5bc4d19a6bc7acaae8f15cebe062f73b45867bec` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor/save.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `482da92cdb0c1bf07751502c75c9fcfcbfe61f82` | `482da92cdb0c1bf07751502c75c9fcfcbfe61f82` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_editor/state.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `981e8d23308ea121cba97ece47138d07836d8237` | `981e8d23308ea121cba97ece47138d07836d8237` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `src/settings_save.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `443d33aaa82cf3fad6eca2559ccab4163ebc469b` | `443d33aaa82cf3fad6eca2559ccab4163ebc469b` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G06 | `tests/settings_editor.rs` | `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | `8ab1163c63889beebf622d5d47303fd97ce97306` | `8ab1163c63889beebf622d5d47303fd97ce97306` | `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7` |
| G09 | `src/duplicate_finder/tests.rs` | `174b6fff5eba1a28de6fb8b73d3a5ea77c91187a` | `18788605f3024df573c89c8703c520e4cf3c903d` | `18788605f3024df573c89c8703c520e4cf3c903d` | `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43` |
| G09 | `src/gui/screenshot_capture.rs` | `174b6fff5eba1a28de6fb8b73d3a5ea77c91187a` | `168cb728eedc115f001880e15a9fd5d6d7f30172` | `168cb728eedc115f001880e15a9fd5d6d7f30172` | `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43` |
| G09 | `src/thread_reaper.rs` | `174b6fff5eba1a28de6fb8b73d3a5ea77c91187a` | `ab1efa9dc88b3efd87ba7d9986fc052a29acf128` | `ab1efa9dc88b3efd87ba7d9986fc052a29acf128` | `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43` |

## Excluded prototype proof — 13 owner/path entries across two receipts

The original receipt covers 11 `src` owner/path entries over 10 physical paths (`src/file_operations.rs` is shared by G02/G07). It is not complete prototype coverage alone. The parent [supplemental excluded-crates receipt](goals/G10/roles/orchestrator/SUPPLEMENTAL_EXCLUDED_CRATES.json), observed at `2026-10-30T08:10:13.3087040Z` on `874065dbe21f227c4c3189dc859ba5521b210eb5`, adds two G02 crate files. Together these prove **13 owner/path entries over 12 physical paths**, including all five G02 prototype source files. Three paths are absent; all other listed paths remain baseline-identical. This corrects documentation coverage, not an imported-source defect; the original receipt is unchanged. G02 source `f879a2a880d59e941486241e652299dabc002365`, G07 prototype `e4e74ec8675c070e165849477fd33d9f490b63d2`, G08 prototype `8d035538ba67f66565074a71b741bb0da2ff9d96` and correction `bb8c572026016d3042efce29e112f8199de41742` remain unaccepted/evidence-only. Their reviewed documentation imports do not accept source.

| Goal | Excluded path | Baseline blob | Current baseline blob | Receipt result |
|---|---|---|---|---|
| G02 | `src/file_operations.rs` | `b7d67bf4d49f81a8c4492991a95892a8dda4835b` | `b7d67bf4d49f81a8c4492991a95892a8dda4835b` | unchanged_or_absent = true |
| G02 | `src/table_utility_session.rs` | `b7f5bb8b525d0786067fb30d6d25199c6fb5ce4d` | `b7f5bb8b525d0786067fb30d6d25199c6fb5ce4d` | unchanged_or_absent = true |
| G02 | `src/table_utility_session/publication.rs` | `ABSENT` | `ABSENT` | unchanged_or_absent = true |
| G07 | `src/archive/mod.rs` | `267a87a87552d63c5f8c67d76448a43c01eeccee` | `267a87a87552d63c5f8c67d76448a43c01eeccee` | unchanged_or_absent = true |
| G07 | `src/archive/preparation.rs` | `ABSENT` | `ABSENT` | unchanged_or_absent = true |
| G07 | `src/file_operations.rs` | `b7d67bf4d49f81a8c4492991a95892a8dda4835b` | `b7d67bf4d49f81a8c4492991a95892a8dda4835b` | unchanged_or_absent = true |
| G08 | `src/gui/mkmacro_dialog/history.rs` | `ABSENT` | `ABSENT` | unchanged_or_absent = true |
| G08 | `src/gui/mkmacro_dialog/mod.rs` | `a9f2335418f69f72dc407a686f6ca4f8956a4960` | `a9f2335418f69f72dc407a686f6ca4f8956a4960` | unchanged_or_absent = true |
| G08 | `src/gui/mkmacro_dialog/toolbar.rs` | `4cf3875d7014fd443916987b34f97978035907f3` | `4cf3875d7014fd443916987b34f97978035907f3` | unchanged_or_absent = true |
| G08 | `src/mkmacro/runtime.rs` | `3a454dfc77b4a5572bf0250ac6a4d1d661290ccd` | `3a454dfc77b4a5572bf0250ac6a4d1d661290ccd` | unchanged_or_absent = true |
| G08 | `src/mkmacro/runtime/summary.rs` | `269d50f56344620fd59b0d4976077a821e725e2a` | `269d50f56344620fd59b0d4976077a821e725e2a` | unchanged_or_absent = true |

### Supplemental G02 crate exclusions

| Goal | Excluded path | Baseline blob | Current baseline blob | Receipt result |
|---|---|---|---|---|
| G02 | `crates/multi_launcher_table_core/src/lib.rs` | `0cba07df6636a0922ac7ad815c80918f54ce10d5` | `0cba07df6636a0922ac7ad815c80918f54ce10d5` | unchanged_or_absent = true |
| G02 | `crates/multi_launcher_table_core/src/resource.rs` | `74def7e006cffddff576bab08563bd284c52374b` | `74def7e006cffddff576bab08563bd284c52374b` | unchanged_or_absent = true |

## Manifest and lock inputs

| Path | Baseline blob | Current baseline blob | Equal |
|---|---|---|---|
| `Cargo.toml` | `994a55fea039ca79614fbb019ac1b05cfab7381b` | `994a55fea039ca79614fbb019ac1b05cfab7381b` | true |
| `Cargo.lock` | `648e143ad2ca902e04ae9b7d4cf57899a2db8380` | `648e143ad2ca902e04ae9b7d4cf57899a2db8380` | true |

Planner's scoped changed-path observation reports no changed `crates`, `.cargo` or `.codex` path. The exact 21-path accepted delta and the two unchanged manifest/lock identities support evidence reuse; editing documentation does not require a new Cargo run. Shared references are not changed-owner overlap. Only an actual conflict or source interaction identified by final review warrants a newly scoped check decided by parent.

## Verification and exclusion truth

- [G01](goals/G01/VERIFICATION.md): corrected32/32, native NOT_RUN; original domain File Search fixture compile failure retained.
- [G05](goals/G05/VERIFICATION.md): M1-A19/19 and corrected B1 22/22, pure/injected foundations only, all condition refusals remain.
- [G06](goals/G06/VERIFICATION.md): corrected29/29 and necessary exact app build; native editor/focus proof deferred.
- [G09](goals/G09/VERIFICATION.md):12/12 and one actual pre-read handle case; [preparation receipt](goals/G09/roles/implementer/verification/initial-list.receipt.json) has numeric exit null, while [matching run](goals/G09/roles/implementer/verification/initial-run.receipt.json) has exit0. Preserve both facts.
- [G08 R1 preparation](goals/G08/roles/implementer/verification/r1-prepare.receipt.json): timed_out true, cap32s, actual drain101.880s, late exit0 and no qualifying R1 test run. Late successful compilation does not accept source or revive budget.

Counts are separate checkpoint evidence, not one combined unique suite. No G10 source, Cargo, build, native or CI execution. Parent must record final source/no-config parity after materialization/import and host-confirmed PR facts; this baseline is never renamed as a final receipt.


## Complete-delta amendment and publication

The original baseline source receipt enumerated20 `src` paths, not the complete Rust delta. The [external-test supplement](goals/G10/roles/orchestrator/SUPPLEMENTAL_ACCEPTED_EXTERNAL_TEST.json) adds the required G06 fixture caller migration above: three `.unwrap()` adaptations, assertions unchanged, exact donor/import/current blob equality, already present in G06 R1 source identities. This external fixture was NOT selected by the recorded29 lib/bin cases; no execution is inferred. The [reviewer amendment](goals/G10/roles/reviewer/ROLE_RECORD.md) accepts this provenance correction. Full non-documentation Git delta is21 Rust paths and no other files; all remain at accepted donor bytes. Original receipts remain historical and unchanged.

[PR1459](https://github.com/multiplex55/Multi_Launcher/pull/1459) is open, ready for review and unmerged. [Publication receipt](goals/G10/roles/orchestrator/PUBLICATION_RECEIPT.json) records creation head b128db10b0cb1652c4f28c96570ef282014c18c9, base 4a1283e6edd2c245f562a07dbe3dcc6a61061eed, later mergeable=true, no reviews and no PR-triggered workflow runs returned. Empty workflows are not a CI pass. Final documentation commits do not change source; the live PR head and orchestrator final response identify the last receipt commit without embedding its own unknown SHA here.
