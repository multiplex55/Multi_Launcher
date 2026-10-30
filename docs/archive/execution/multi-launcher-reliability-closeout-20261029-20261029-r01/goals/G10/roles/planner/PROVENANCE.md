# G10 provenance requirements and observed baseline

Date: 2026-10-30. Evidence: `SOURCE_OBSERVED` / current parent receipts; no new execution qualification by Planner.

## Exact authority

- G10 branch: `codex/closeout-g10-residual-defect-triage-integration-20261029-r01` at baseline `535993a4fa32a5646e0ade6076c6941a71d47230`.
- Dependency base: accepted integration `ea3a7007e23090c1c2b13ea9018b29754579e4ef`, not bootstrap. Integration branch: `codex/closeout-integration-20261029-r01`.
- Fetched master/origin/master: `4a1283e6edd2c245f562a07dbe3dcc6a61061eed`; bootstrap: `02d0ac64d51655f43d6e958ea551f5bf658ee48d`.
- Newest accepted test checkpoint: `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43`. Last production behavior checkpoint: `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7`. These are not interchangeable with the latest documentation tip.
- Parent owns Git, authoritative ledgers, final integration and PR. Child proposals on the G10 branch do not become integration authority until parent promotion/import.

Reuse [BASELINE_BRANCH_RECEIPT.json](../orchestrator/BASELINE_BRANCH_RECEIPT.json) for all eleven exact branch names, full closing SHAs and matching remote-tracking tips. Its timestamp is `2026-10-30T07:46:39.1610151Z`; it is a baseline snapshot, not a future final-push claim. Parent also supplied [BASELINE_SOURCE_PARITY.json](../orchestrator/BASELINE_SOURCE_PARITY.json): 20 accepted paths match closing G01/G05/G06/G09 donors, 11 excluded prototype paths are baseline-identical or absent, and both Cargo manifests/lock inputs are unchanged. Do not rerun its enumeration merely to regenerate the same baseline.

## Current disposition and evidence table

The implementer must join this table to the branch receipt by G00–G10, preserving full refs and adding exact evidence links in its final proposal. `ACCEPTED_CHECKPOINTS.md` supplies complete import provenance and each goal's `VERIFICATION.md` supplies commands/receipts. Counts below are separate checkpoint results; do not sum them into a claim of unique program-wide test coverage.

| Goal | Closing donor / accepted import or source boundary | Current disposition | Focused / native evidence and gate |
|---|---|---|---|
| G00 | `7e53f4d28ca5b66a21e30e41545692cce5fa6b6a` → `41cc682e` | ACCEPTED_COMPLETE, documentation | Static continuation/links and independent review; Cargo/native NOT_RUN. |
| G01 | `13243515329ce4010bd6db981f1fd3bcc26f2dec` → corrected cumulative `fd300ff748e54cee167a038337220e33beba2322`, plus M1-A `ddbbf4c4` | ACCEPTED_COMPLETE for two bounded Diff fixes | Corrected 32/32: 25 library + 7 existing Diff cases. Original domain target File Search E0308 remains; native NOT_RUN. Rejected standalone intermediate was not independently imported. |
| G02 | docs donor `011af0c448dfdcdc0eb747083f909d3bfa6c1ab2`; source `f879a2a880d59e941486241e652299dabc002365` excluded | DEFERRED_WITH_DESIGN / NATIVE_NOT_PROVEN | Both 19-case runs: 15 pass / 4 fail, parent admission OS5 before staging; M1-B unstarted. G03 gate remains unmet. |
| G03 | docs donor `e0f556cd8041c502d63479d89d0e68301cceb3b2` → closure `d326679d536008167a294880e5bd9b16b502e732` | DEFERRED_DEPENDENCY | Documentation only, no source/Cargo/native/large-file operation. Original complete conditional pair retained. |
| G04 | docs donor `ea27ce749318ab2c261384385fd03ef05f882b4f` → `0b57567e83ee5e7f7f94a34c3c45db37ec3a53ac` | NATIVE_PROOF_DEFERRED | Source/pixel protocol only. No held-secondary drag capability, exact binary/output/clipboard proof; no native input or Cargo acceptance. |
| G05 | M1-A `8b4d01a4cff48c2bdfaa30f760c03e91cbb6f9be` → `8f6befe70c402157f99bdae3404513ca02e46ecd`; corrected B1 `944f9d0ef4ed5ceee54c9a3d255c40e9a220494d` → `4999a47c68a9e4e5b8531cf31e018eda3eefda4f`; docs closure `a78895c63f31c9f48ae579e3203c14fa3faa8c35` | PARTIAL_ACCEPTED / NATIVE_PROOF_DEFERRED | M1-A 19/19; corrected B1 22/22. Native pending-read retirement, retained source/effect/queued-start binding, editor/Test Rule and enablement not accepted. All condition refusal gates retained. |
| G06 | corrected source `7cb16df951cb2eac1642fb428dbce84559f77562`, confirmation `5c1bf6805c0b111fec93a1909238b45bf66d8dc2` → cumulative `b9f5fa4bfe4508bfc4cea8fcb99d007d8b862ad7`; docs closure `d5f7a67fead714fd465cb9ed0b9ac5a4e68fc436` | PARTIAL_ACCEPTED / NATIVE_PROOF_DEFERRED | Corrected 29/29: 26 library + 3 actual main resolver tests. Exact application build passed; native owned-window discovery failed, no input/focus/Save proof. |
| G07 | docs donor `5d4d3054c5b52510a38d96faf201c6c0ca2e07f7` → `51caaca89b74428b2891737dbf5cdd2536b53063`; prototype `e4e74ec8675c070e165849477fd33d9f490b63d2` excluded | DEFERRED_WITH_DESIGN / FAILED_VERIFICATION / WRITER_DEFERRED | Final 10/12, guarded publication OS32 and unknown source/alias deletion endpoint. All source excluded; writer/UI unstarted. |
| G08 | docs donor `e505f961849475ecf3affda40064b96a60243f9d` → `f933d255ddad67024042f1629d834f2e02a4fc21`; prototype `8d035538` and correction `bb8c5720` excluded | DEFERRED_WITH_DESIGN / VERIFICATION_DEFERRED | Initial 14/14 predates R1. Correction preparation timed out at 32 s and drained after 101.880 s with late exit 0; no qualifying R1 run. History and recipes not shipped. |
| G09 | tests `090df0d0859189d63804c4e79524fb7a3be2478b` → `fc9e5c7cbb509397b47c4d7f2902899c9d04dd43`; closure `174b6fff5eba1a28de6fb8b73d3a5ea77c91187a` → `8f7c10a307a3d157ba863d9263c1964bac7032a5` | NO_CHANGE_REVALIDATED / TEST_STRENGTHENING_ACCEPTED | 12/12. Native acquired synthetic-file handle at pre-read gate, OS32 while held, successful exclusive opens after join/reopen, warm restart and cleanup. Not pending ReadFile, capture, apartment or all-owner proof. |
| G10 | baseline `535993a4fa32a5646e0ade6076c6941a71d47230` from accepted integration `ea3a7007e23090c1c2b13ea9018b29754579e4ef` | IN_PROGRESS, documentation reconciliation | No source/Cargo/native work planned. Final independent review and host-confirmed PR/head/check state remain parent actions. |

## Accepted source manifest

Planner independently observed the current master-to-G10 changed-path manifest. These 20 Rust paths are the entire accepted source/test delta:

- G01: `src/diff/file_ops.rs`, `src/diff/folder_runtime.rs`, `src/gui/diff_dialog/mod.rs`, `src/gui/diff_dialog/operation_preview.rs`.
- G05: `src/hotfolder/conditions.rs`, `src/hotfolder/execution.rs`, `src/hotfolder/mod.rs`, `src/hotfolder/model.rs`.
- G06: `src/hotkey/validation.rs`, `src/main.rs`, `src/settings_editor.rs`, `src/settings_editor/catalog.rs`, `src/settings_editor/mapping.rs`, `src/settings_editor/render.rs`, `src/settings_editor/save.rs`, `src/settings_editor/state.rs`, `src/settings_save.rs`.
- G09 test-only: `src/duplicate_finder/tests.rs`, `src/gui/screenshot_capture.rs`, `src/thread_reaper.rs`.

No changed `crates`, `Cargo.toml`, `Cargo.lock`, `.cargo` or `.codex` path was reported by that scoped diff. Distinct ownership plus accepted donor parity provides the current reason for **no new integration Cargo run**. Shared architectural references alone are not changed-owner overlap. Final review may identify a concrete conflict; only then select its narrow test, with the stated bounded budget, before adding execution.

## Required static provenance output

The implementer consumes the existing parent JSON receipts and reports their schema/completeness, exact source/manifest blob IDs, donor/import mapping, excluded-source boundary and any discrepancy. It does not silently repair a blob mismatch or copy a rejected patch. The final manifest must distinguish: original source donor; accepted cumulative import; latest test change; latest documentation tip; fetched master; proposed PR head. Every item has an observation timestamp and evidence link.

Parent refreshes final head/upstream/PR facts after its commits/imports. A baseline receipt never becomes a final receipt by renaming it. Do not embed a guessed SHA for the commit that will contain the document itself.
