# G02 planner role record and M1-A handoff

Context /root/g00_planner reused. Requested gpt-6-astra/max; configured gpt-6.1-sol/high; serving backend/effort UNVERIFIED. One read-only planning pass. Parent persisted returned content; no implementation review by planner.

Source 50608bd417b34854358989a5d4b1325e95293063 on codex/closeout-g02-table-publication-safety-20261029-r01, bootstrap 02d0ac64, master 4a1283e6. Planner observed a clean source tree; parent authored progress/role documentation during planning. Sibling global ledger is bootstrap snapshot; integration authoritative.

Read complete G02 Goal/Kickstart/policy. Inspected session admission/worker/receipt/lifecycle, compact GUI, atomic helper, file-backed source/cursor/selection, temp resource ownership, native helpers and directly relevant tests. Missing historical reports not reconstructed.

Conclusion: bounded new-file-only foundation viable; not already accepted. Temp-file handoff and native adapter gaps are active G02 scope; parent explicitly permits additive ownership adapters, not engine/framework refactor. NO_ACTION: no distinct additional finding. No edits/Cargo/native probes/Git mutation/publication/agent dispatch. SOURCE_OBSERVED only; proposed tests are not proof.

## M1-A objective and ownership

Establish one owner-private staged-output reservation preserving source identity, native file ownership and quota through streaming, new-only publication or cleanup. Keep unreachable from production UI until lifecycle/proof acceptance.

| Scope | Responsibility |
|---|---|
| src/table_utility_session.rs | Immutable admitted session/source/view/selection/operation manifest and private reservation entrypoint; preserve compact paths. |
| src/table_utility_session/publication.rs (optional focused module) | Table-specific reservation/stage/outcome; not scheduler/engine. |
| crates/multi_launcher_table_core/src/resource.rs | Narrow TableTempWorkspace/TableTempFile handoff retaining TableResourceLease and failed-cleanup owner. |
| crates/multi_launcher_table_core/src/lib.rs | Mechanical re-export only if required. |
| src/file_operations.rs | Small crate-private adapter over existing identity/parent-chain/handle rename/delete helpers; preserve all old callers. |
| G02 execution docs | Source/proof/risk and own role records. |

## Required sequence

1. Capture immutable manifest: actual lifetime, draft/accepted source revisions, TableSourceId/native source identity, retained Arc source, path, view/selection revisions, operation ID, captured source-bound rows/columns and resource admission. No page-preview strings as full cells.
2. Reserve absent supported destination; validate regular source/destination leaf, reject existing/same-source aliases; inspection failure refuses. Hold ancestry through effect.
3. Resource-owner staging on destination filesystem; bounded chunks charged before growth, retained after partial-write failure.
4. Explicit handoff keeps original file/workspace/charged lease/cleanup metadata. No staging-path reopen and assumed identity.
5. Reuse native_file_identity, parent proofs/guards, rename_open_file_no_replace and delete_open_file INSIDE existing file_operations owner. No FFI copy into table module.
6. Finish/flush/sync while retaining identity handle; native no-replace publish; close only after outcome/cleanup established.
7. Failure cleanup exact owned stage; unresolved cleanup retains identity capability plus charges in existing owner; blocks new publication. No arbitrary remembered-path deletion or duplicate recovery queue.
8. Deterministic test seams around stream/seal/publish/cleanup. Synthetic chunks sufficient here; CSV formatting/UI later.

Invariants: no full-output buffer, parser/dependency/native framework, source mutation, destructive replacement, path-only publication or altered compact behavior. Acceptance: coherent owner path, no caller bypasses later session gate; parent commits before M1-B. M1-A is not G02 completion.

Stop gate: inability to retain guarded stage/parent identity through publication and failed cleanup means stop before a caller, preserve first concrete API/error, create complete narrow future pair; no path-based fallback.

## Subsequent M1-B boundary

Existing worker stages; session owner adopts only after all manifest identities still current. Stale results clean safely. Before-publication cancellation refuses; after-success cancellation retains truthful receipt. Complete close/shutdown, last-good state and unresolved recovery before foundation acceptance. Return a fresh bounded handoff after M1-A source confirms interfaces.

## Focused proof inventory

New table_publication_ prefix plus directly affected existing cases, narrow if owner unchanged:

```text
resource::tests::resource_pool_leases_share_usage_and_release_on_last_owner
resource::tests::partial_temp_append_failure_keeps_charge_until_owned_file_is_removed
resource::tests::failed_temp_deletion_keeps_a_retryable_charge_until_removed
table_utility_session::tests::cloned_snapshot_handles_keep_each_allocation_charged_after_session_drop
table_utility_session::tests::pending_job_releases_its_resource_leases_before_confirmed_activity_retirement
table_utility_session::tests::file_page_failure_and_cancellation_preserve_the_last_good_page
table_utility_session::tests::file_page_completion_from_closed_lifetime_is_ignored_after_reopen
table_utility_session::tests::table_session_draft_edits_revoke_queued_export_receipts
gui::table_utility_dialog::tests::table_file_save_refuses_to_replace_the_opened_source
gui::table_utility_dialog::tests::table_file_save_requires_confirmation_and_publishes_the_captured_output
```

cargo nextest list/run -p multi_launcher -p multi_launcher_table_core --cargo-profile fast-dev --lib -E <new prefix and affected named controls>. Confirm nonzero actual inventory; no commands executed by planner. Actual Windows adapter probes mandatory for its native guarantees. No full domain target, broad GUI/table/native suite, ceremonial build or default lint campaign.

Next: parent commits contract, dispatches only M1-A, checkpoints coherent source before M1-B. G03 strictly gated.
