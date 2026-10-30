# G07/M1-A — Retained physical source and guarded output reservation

Status: READY_FOR_ONE_WRITER after the parent commits M0-A. Date: 2026-10-30. Baseline and binding security/format decisions: [SECURITY_FORMAT_DECISION.md](SECURITY_FORMAT_DECISION.md). Active contracts: [GOAL](../../../../../../reliability-feature-closeout-20261029/goals/G07-zip-extraction/GOAL.md) and [KICKSTART](../../../../../../reliability-feature-closeout-20261029/goals/G07-zip-extraction/KICKSTART.md). Parent owns source checkpoints, Git and authoritative ledger; do not use the sibling bootstrap status as integration truth.

## Objective and ownership

Create one coherent foundation that retains a native source while cataloging/planning, validates selected physical Stored ZIP32 local records, and reserves only identity-owned staging beneath a held existing destination. Establish actual Windows source holding, containment, no-clobber and cleanup evidence in repository-owned fixtures. Return a typed preparation/abort result. No extraction writer or user-facing write path is enabled in this checkpoint.

`src/archive` owns the session, physical-entry validation and captured plan/limits. `src/file_operations.rs` owns the existing Windows primitives and the narrow archive adapter. The implementer is the sole source writer but is not alone in the repository: preserve parent-owned documents and do not revert others' work.

## Exact current state and scope

- `ArchiveRevision` is currently caller-owned; `ArchiveEntryId` is revision plus central ordinal. `ArchiveCatalog::entry` checks revision and ordinal. `read_catalog` intentionally never reads a local header or payload.
- `ExtractionPlan` contains source, outputs and captured no-overwrite/byte/ratio policy, not a native source or destination. `PlannedOutput::source_entry` keeps the physical ID. `build_extraction_plan` already rejects selected duplicate normalized names, file ancestors, unsafe kind/path/encoding and sampled occupancy.
- Relevant native symbols: `NativeRenameScanBackend::{admit_root,reopen_chain}`, `RootAdmission`, `RootProof`, `OpenDirectoryChain`, `DirectoryOpenAccess`, `open_native_object`, `open_native_readonly_content_file`, `native_file_identity`, `final_path_for_handle`, `create_native_replace_stage`, `rename_open_file_no_replace`, `delete_open_file`, `rename_error_is_destination_collision`.
- `open_native_object` currently uses FILE_SHARE_READ | FILE_SHARE_WRITE and no delete sharing. `rename_open_file_no_replace` uses held-source FileRenameInfo with ReplaceIfExists false, an absolute canonical parent path and retained ancestor handles. Neither the planner nor a lexical path test has proved reparse-conversion safety.
- Allowed source: a focused new archive preparation/session module with `src/archive/mod.rs` wiring; minimal `model.rs`/`catalog.rs`/`plan.rs` changes only if needed for a real ownership integration; `src/file_operations.rs` narrow native adapter and tests. Keep unrelated native callers/defaults intact. No Cargo dependency, GUI, command, settings, table, Hot Folder authority or atomic-file changes.

Private helper/type naming and equivalent Rust ownership choices belong to the implementer. Do not expose low-level handles or a public generic filesystem framework.

## Required implementation

1. **Bind source before catalog.** Add a narrow opaque native archive read owner that retains the full admitted local ancestor chain plus the no-follow, deny-write/delete read handle and its native identity/path/length evidence. Reuse current primitives; reject unsupported roots/identities/cloud/reparse paths. Enforce the 512 MiB archive ceiling before catalog work. Feed this exact handle into `read_catalog`; the archive session owns its catalog and revision. Plans eligible for preparation must be created from that session/catalog, not an arbitrary caller-supplied catalog paired with a matching integer revision. A replaced/reopened path requires a new session and selection. Do not hash or buffer the whole archive merely to create a second identity source.

2. **Apply the captured execution subset.** Build the selected plan through the existing planner with the bounded execution profile from the decision record. Enforce 128 selected flat files, no output directories, Stored/ZIP32/flags/creator/version/extra restrictions, 64 MiB entry and 256 MiB total limits, ratio 1 and `no_overwrite`. Preserve the broader read-only API. Reject all unsupported selections before creating any stage.

3. **Validate physical local records.** Resolve each planned ID through the owned catalog. On the same source handle, recheck its physical central record where omitted fields are needed, then range-check the 30-byte local header before reading it. Require local signature, allowed extraction version, exact flags/method/CRC/sizes/raw name agreement and zero local/selected central extras. Use checked arithmetic for `local offset + fixed + name + extra + compressed size`; every selected range must be before the central directory and within the captured archive. Reject overlapping selected physical records/payload ranges and ambiguous reused selected offsets. No name lookup, ZipArchive map, ZipFile Drop or convenience extract path. Retain validated payload offset/length/CRC with its ID in the preparation; do not read payload bytes yet. Revalidate held source identity/length/parent before accepting preparation.

4. **Reserve with native ownership.** Admit an existing destination directory and retain its complete chain. Add an archive-specific stronger sharing/admission policy only if needed to exclude in-place reparse conversion; use the existing native owner/FFI and preserve other defaults. Fail closed if the policy cannot establish containment. Recheck occupancy through the held directory; vacant is the only allowed final-file state. Use safe generated stage leaves, bounded collision retries and CREATE_NEW. Capture identity immediately and hold each stage handle with destination proof, immutable final leaf, entry ID and policy. Existing `create_native_replace_stage` requests security-copy rights; a minimal archive-only rights option may be introduced in that owner if necessary, without changing replacement behavior. Never create a final-name placeholder. No archive data is written in production M1-A.

5. **Make cleanup explicit.** Any preparation failure or cancellation closes/aborts all stages already created through held-file deletion. Return typed cancellation/failure plus any exact owned retained-stage receipts when cleanup cannot complete. Do not delete an occupied final target or path substitute. A successful abort means files/handles are retired; otherwise report retained state. Do not let Drop silently convert an unobserved cleanup failure into success. Carry preparation ownership forward so later streaming can use the same source and stages without reopening them or inventing a second authority.

6. **Prove the native effect boundary without enabling extraction.** The native adapter may expose a private stage/no-replace operation to the archive owner for later consumption. Its M1-A tests may write fixed synthetic bytes and exercise the actual no-replace primitive; this is native primitive proof, not archive extraction acceptance. No external caller receives a way to publish an unverified archive preparation. Keep the eventual payload-verification-to-publication gate in the archive owner.

## Invariants and exclusions

Read-only catalog and pure plan remain usable and retain their current signature/semantics unless a necessary narrow change is explicitly tested. Existing defaults and physical duplicate identity survive. No new data interpretation in UI, no Deflate decoder, no overwrite, no directory creation, no attribute/permission copying, no whole-output buffers, no unsafe fallback, no historical patch import, no generic I/O layer, no change to existing Hot Folder/DFF/rename/replace behavior.

Use cooperative cancellation checks around work and before effects. Synchronous blocked I/O is not cancelled merely because the token changes; do not return a terminal cancellation while a worker still owns operations. M1-A does not require or claim a hard in-flight kernel-I/O cancellation facility. Later GUI cancellation must retain/join the owner before reporting terminal cleanup.

## Focused proof

Use new test names beginning `g07_` so selection is auditable. Keep cases in a few meaningful tests rather than a separate test for every field.

1. Pure/in-memory physical validation: two entries with the same text but distinct physical IDs; selecting one binds its own local range. Reject stale session/ID, local name/flags/method/CRC/size mismatch, out-of-range/truncated/overlapping header ranges and unsupported execution formats. Rejections produce no reservation callback/effect. Preserve valid flat zero-byte Stored preparation.
2. Preparation/resource/cleanup: tighter captured limits cannot be raised; selection/byte limits refuse early; cancellation after one reserved stage aborts it; injected stage-create/cleanup failure preserves existing output and reports retained owned state truthfully. Reuse catalog fixture-building mechanics, but supply correct CRC in fixtures used later for integrity proof.
3. **Actual Windows native source/parent test:** owned ordinary ZIP source and destination beneath `target/productivity_expansion/archive_native`, using the existing `m10c_native...` repository-tempdir pattern. Successful admission is mandatory. While the source lease is held, attempt competing write/truncate and rename/delete, including a hard-link alias where supported; attempts must be refused without changing bytes/identity. A competing already-open writer must cause source admission to fail. Exercise ancestor replacement and in-place reparse conversion against the held source/destination chain, with a successful unguarded adversarial-fixture control. A preexisting junction/reparse ancestor must refuse before source reads/stage effects. Record which operations actually ran; unsupported fixture construction is a missing proof, not a pass. All alternate destinations are inside the same owned fixture.
4. **Actual Windows native stage test:** create a stage, verify its handle identity and destination containment, write fixed synthetic bytes, then publish through the existing no-replace primitive into a vacant leaf. Verify bytes, final identity and stage disappearance. Separately create the final target after reservation; publication must refuse and leave its sentinel bytes/identity unchanged. Abort the held stage and verify removal after handles close. Exercise destination-chain replacement/conversion around stage create/publication; either the competing mutation is rejected or the operation must refuse before any escaped effect. A post-effect detection of escaped creation is a failure. Existing ordinary operation must still succeed.

The two native tests are acceptance gates for M1-A's native claim. A fake backend cannot substitute. Test-only Windows fixture support may use existing bindings, but no driver/service/install, external downloads, privilege/policy changes or unrelated user paths. Preserve fixtures/receipts needed to diagnose a failure; cleanup only verified owned paths.

Run these existing tests alongside the new selection:

- `archive::catalog::tests::metadata_catalog_preserves_physical_duplicates_and_never_reads_payloads`
- `archive::catalog::tests::unsupported_encrypted_unsafe_and_bad_encoding_entries_remain_visible`
- `archive::plan::tests::physical_duplicate_names_keep_distinct_ids_and_conflict_only_when_both_selected`
- `archive::plan::tests::windows_case_separator_and_file_ancestor_collisions_are_rejected`
- `archive::plan::tests::read_only_occupancy_probe_blocks_existing_file_and_is_bounded`
- `file_operations::tests::m10c_native_copy_move_rename_collision_and_stream_refusal_stay_in_fixture` only when shared low-level access/sharing code is changed, to verify the unchanged default path.

## Verification and stop boundary

One nonzero selected preparation and one focused run using package `multi_launcher`, library target, fast-dev profile, offline/locked:

```text
cargo nextest list -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E '<new g07_ tests plus the exact existing tests selected above>'
cargo nextest run -p multi_launcher --lib --cargo-profile fast-dev --offline --locked -E '<same verified selection>'
```

The filter placeholder is intentional: fill with actual test names introduced by the implementation, using valid Nextest expressions. Parent/implementer records exact invocation and selected nonzero inventory. Initial preparation cap 1200s; focused execution cap 180s. A 300s run requires a concrete bounded ZIP-resource reason, not ordinary small-fixture native tests. Incremental correction cap 600s aggregate, 1–2 localized reruns only for actual errors. No full suite/domain/build, clean target, dependency installation, broad lint or application/manual-user run. No app construction is needed; if one is introduced, disable Paste Sequence next/cancel shortcuts first.

Inspect the diff and stale-reference search. Parent commits the coherent checkpoint before streaming work; suggested subject `feat(archive): [G07/M1-A] bind physical entries to guarded extraction reservations`.

Done requires same-source ownership, strict subset/local-header proof, identity-owned staging/abort, actual native success and adversarial no-redirection/no-clobber proof, selected tests passing and independent review. Report M1-A acceptance only, not writer/UI completion. A known native safety failure or unavailable required probe makes the affected source nonintegrable until resolved in the allowed correction budget; preserve exact error/source/cleanup and the narrow complete continuation pair. Do not weaken assertions or substitute a path-only implementation.

Genuine uncertainties are limited to actual host support for the archive-specific ancestor guard policy and required adversarial fixture, and the stage access rights needed by the existing native primitive. They are implementer proof/stop gates, not speculative blockers. Deflate/nested/overwrite support is intentionally excluded, not a new initiative.

## Required next checkpoint after acceptance

Only after accepted M1-A, hand off M1-B against its actual interfaces: stream the held Stored ranges into the held stages; enforce actual counts/EOF/CRC/cancellation, verify every file before its no-replace publication, and record per-file success/retained-stage/partial-batch outcomes. Then run the directly applicable adversarial writer proof. UI remains gated until that accepted writer exists; a later minimal UI packet must use its single owner and truthful lifecycle. Do not pre-implement either step now.
