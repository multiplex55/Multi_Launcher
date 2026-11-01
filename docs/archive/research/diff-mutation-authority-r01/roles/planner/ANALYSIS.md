# AUD-F002 bounded planner analysis

Authored by `/root/audit_planner`; persisted by `/root` on 2026-10-29.

## Result

M1-A can proceed as one bounded research-author checkpoint. The responsible production owner is `src/diff/file_ops.rs`; the GUI captures confirmation and coordinates execution. Existing safeguards are substantive but answer different questions. The research must distinguish metadata freshness, path containment, dirty-editor protection, mutation admission, object identity and authority at the instant of effect.

No stronger product contract is assumed. H0 documented best-effort behavior remains valid. M1-A supplies evidence for the later decision; it does not implement H1/H2.

All source locators below refer to `59156d96adcc73939c7171487a0cb582e3286587`.

## Corroborated current state

| Owner / locator | Fact relevant to the handoff |
|---|---|
| `src/diff/file_ops.rs:30`, `FileIdentity::read` | Captures kind, length and optional modification time; Unix additionally captures device/inode. It rejects a directly observed symlink. The Windows representation has no native object identifier. Do not generalize that limitation to the Unix representation. |
| `src/diff/file_ops.rs:68`, `CapturedRoot`; `:137`, `root`; `:177`, `ensure_contained` | Plans retain root path, canonical path and identity. Containment validates captured relative components and canonicalizes the nearest existing ancestor. These are path checks at observation time, not held-object authority. |
| `src/diff/file_ops.rs:209`, `plan_copy`; `:430`, `add_file` | Plans normalize/deduplicate selection and retain source/destination expectations. Existing destination directories conflict; equality of source/destination `FileIdentity` skips the file; otherwise an existing destination is an approved overwrite, while `NotFound` produces an expected-absent destination. Construct witnesses from a plan that this admission actually produces. |
| `src/diff/file_ops.rs:506`, `fresh`; `:532`, `mutation_contains_dirty_path`; `:540`, `same_root_identity` | Per-file validation checks roots, containment, captured paths, source identity, dirty destination/descendants and destination expectation. Unix root comparison uses device/inode; non-Unix root comparison requires a directory and matching canonical root path. Do not describe the latter as native root-object identity. |
| `src/diff/file_ops.rs:555`, `execute_copy` | Directory creation has its own containment/path checks followed by `create_dir_all`; it does not run per-file `fresh`. File copies run `fresh` and then path-based `fs::copy`. Permission propagation subsequently reads the source path again. There is no copy scratch-file/atomic-commit protocol here. |
| `src/diff/file_ops.rs:737`, `plan_delete`; `:804`, `execute_delete` | Delete planning captures normalized/deduplicated targets and expectations. Execution checks the root, cancellation, containment/path equality, dirty descendants and current identity before calling the selected backend by path. The backend call is separate from those observations. |
| `src/diff/file_ops.rs:718`, `TrashBackend`; `:723`, `SystemTrash`; `:879`, `spawn_recycle_delete` | System recycle calls `trash::delete`. The GUI worker boundary refuses a permanent-delete plan. Recycle failure has no permanent-delete fallback. The shared executor's explicit Permanent mode must not be confused with GUI admission. |
| `src/gui/diff_dialog/mod.rs:1018`, `plan_mutation`; `:1129`, `dirty_paths`; `:1146`, `show_operation_preview` | The GUI refuses planning while a mutation is active, snapshots applicable selection, creates the previewed plan and recomputes dirty paths when execution starts. It consumes that same plan; the execution dirty set is a snapshot, not a continuously updated editor lock. Both GUI delete directions request Recycle. |
| `src/gui/diff_dialog/operation_preview.rs`, `PreviewPlan`, `OperationPreview::show` | Confirmation presents copy/overwrite or Recycle Bin intent, rejects non-executable plans and prevents Execute while an operation is active. Preview cancellation discards the preview. UI wording is evidence of presented intent, not proof of atomic authority. |
| `src/diff/file_ops.rs:669`, `OperationHandle`; `:678`, `spawn_copy`; `src/diff/folder_runtime.rs:288`, `cancel` | Cancellation sets an atomic flag; the executors check it between work items. It is not rollback or interruption of an already entered filesystem call. The worker can have completed earlier items. |
| `src/gui/diff_dialog/mod.rs:1249`, `poll_folder_runtime`; `:1448`, `refresh_after_mutation` | Matching-generation completion clears the active operation and refreshes affected state. Root/runtime changes request cancellation. The author must trace what per-item failure/cancellation information actually reaches the user; an executor report alone does not establish GUI feedback. |
| `src/file_operations.rs:1150`, `GuardedRenameFile`; `:1214`, `rename_to_leaf`; `:2942`, `prepare_guarded_rename_batch_native` | Rename has held source and directory guards, native identity, proof-chain checks and same-parent restrictions. Its complete preparation/transaction model is not automatically reusable for Diff copying or recycling. |
| `src/file_operations.rs:8718`, `rename_open_file_no_replace`; `:9231`, `native_file_identity` | The selected rename call uses a held source handle, `ReplaceIfExists = 0`, an absolute destination name, and null `RootDirectory`, alongside separately held/revalidated directory guards. Do not claim the API receives a relative destination parent handle. Comments about prior fixtures are historical source evidence, not current native proof. |

The trash dependency is declared at `Cargo.toml:181`; `Cargo.lock:5540` resolves it to 5.2.4. Its actual Windows backend and guarantees remain for the author's bounded dependency-source/documentation inspection. Do not guess its shell API from the crate name.

## M1-A handoff

### Objective and ownership

Create `CONTRACT_AND_EVIDENCE.md` and `IDENTITY_PROOF.md` under this task's output root, with current primary-documentation evidence and a source-test inventory. Establish the evidence and proof obligations needed for M2-A to choose H0/H1/H2.

Diff owns selection, confirmation expectations and mutation policy. A potential native primitive would own only the specific enforceable filesystem operation and handle lifetime it can support. The GUI must not become a second filesystem-policy owner.

### Scope and dependencies

Use the source owners above, directly relevant tests, the pinned historical finding and current official documentation. Inspect additional helper bodies only when necessary to complete a named control-flow or authority claim.

M0-A authorization and source-baseline identification are prerequisites. Neither completed AUD-F001 nor closed SEARCH-F002 is a code dependency. Parent owns the branch and Git operations.

Allowed author writes are task documentation and the optional bounded pure proof model. Production source, tests, manifests, dependencies and native wrappers remain read-only.

### Required work, in order

1. Build a compact authority matrix for copying to an expected-absent destination; approved overwrite; planned directory creation and recursive selection; recycle; cancellation and partial completion. For each, identify approval state, execution admission, last validation, effect-bearing API, outcome/reporting and remaining external-writer interval. Separate root, ancestor, source, destination and any proposed scratch ownership. The current copy path has no scratch protocol.
2. Preserve the guard distinctions in the source table. In particular, do not substitute directory containment checks for per-file freshness, recycle-only admission for identity binding, or generation filtering for cancellation/join guarantees.
3. Write two mandatory identity witnesses in `IDENTITY_PROOF.md`:
   - **Prevalidation replacement:** an admitted plan approves object A; another object B replaces its path while preserving the fields actually compared on the relevant platform; validation cannot distinguish those objects under that representation.
   - **Postvalidation replacement:** validation observes A, replacement occurs before path-based use, and the model distinguishes the object selected by the path operation from the approved object.

   Keep `Approved(path,A,m)`, `Validate(path,m)`, `ExternalReplace(path,B,m)`, `Mutate(path)` and `MutateHeld(A)` separate. Distinguish source from destination, and expected absence from approved overwrite. A held source does not by itself bind destination or parent authority. Include counterexamples in which changed compared metadata, dirty descendants, invalid containment or inadmissible execution are refused. Constrain the same-metadata prevalidation example to the relevant representation; Unix device/inode checks must not silently disappear.
4. Compare H0/H1/H2 without choosing a stronger requirement merely because a stronger mechanism exists:
   - **H0:** retain best-effort external-writer semantics and document confirmation, tolerated substitutions, failure and partial-outcome limits.
   - **H1:** identify whether a local Diff-owned admission/effect design can enforce a specifically stated stronger contract. Another stat followed by the same path operation is not sufficient evidence.
   - **H2:** consider narrowly reusing/extracting a native primitive only if necessary. Specify its inputs, outputs, held objects, release points, failure semantics and exact later caller migration. Do not import Rename's journal, undo or transaction architecture wholesale.
5. Resolve only load-bearing API facts using current primary sources. Record URL, API/library version or applicable platform, access date, claim supported and remaining limits. Obligations include native identifier meaning and lifetime; actual handle-open access/share/delete rights; source, parent and destination binding; replacement versus no-replace; reparse/path-resolution and failure/partial-effect limits; resolved trash 5.2.4 Windows targeting/completion guarantees. Follow the actual wrapper's calls before selecting documentation. Starting seams include `GetFileInformationByHandleEx`/file IDs and `SetFileInformationByHandle`/`FILE_RENAME_INFO`; their availability does not establish suitability for copy or recycle. Check Rust filesystem documentation where its path-copy contract is load-bearing.
6. Record existing tests as source evidence only. Identify exact missing evidence rather than inventing a broad future regression campaign.

### Existing test seams

`tests/domain_cases/diff_file_ops.rs` is included by `tests/suites/domain.rs:13`.

| Test | Source-level evidence |
|---|---|
| `copies_file_and_plans_overwrite` | Planned overwrite and ordinary copy result. |
| `recursive_copy_deduplicates_nested_selection` | Recursive selection normalization and ordinary execution. |
| `rejects_escape_spellings_and_destination_conflict` | Invalid relative inputs and destination-directory conflict. |
| `recycle_failure_never_falls_back_to_permanent_delete` | Injected backend refusal; no permanent fallback. It does not invoke native trash. |
| `right_to_left_plan_retains_direction_and_executes_previewed_object` | Direction and reuse of the captured plan. Its name does not establish competing-writer object identity. |
| `delete_normalizes_parent_child_and_dirty_descendant_blocks_parent` | Parent/child normalization and dirty-descendant refusal. |
| `gui_spawn_boundary_rejects_permanent_deletion` | Recycle-only worker admission. |

GUI tests: `src/gui/diff_dialog/mod.rs:1831`, `principal_folder_workflow_uses_injected_controller_boundaries`, and `:2031`, `gui_delete_planning_is_recycle_only`.

These inspected tests are not deterministic pre/postvalidation replacement proofs. None was run. Additional directly relevant existing tests may be inventoried by the author, but discovering more tests does not authorize executing them.

### Optional model and later native proof

A hand-reviewed transition table is sufficient. Prefer recording executable model `NOT_NEEDED` when it would merely repeat the table.

If justified, `proof/identity_model.py` must remain a standard-library, pure in-memory model: no filesystem, network, subprocess, environment or source imports; at most 100 states; one initial run and at most one localized repair rerun; 30 seconds per run and 60 seconds aggregate, enforced by the runner. Preserve failed output and the corrected assumption. Missing tooling does not authorize installation or native fallback.

A model demonstrates its assumptions, not Windows interleaving feasibility, sharing behavior, reparse safety or actual trash targeting. If a decision needs native evidence, describe one later disposable synthetic-fixture protocol with the two exact interleavings, expected identities/outcomes, owned handles and release/cleanup rules. Refuse cleanup of an unexpected identity. Mark native execution `NOT_RUN`; do not execute or create that future implementation goal here.

### Invariants and non-goals

Preserve current confirmation intent, direction, selection normalization, containment checks, dirty-descendant refusal, exact preview-plan reuse, GUI Recycle-only admission, absence of permanent fallback, generation handling and partial-completion/cancellation distinctions.

No production fixes, new wrappers, hashing policy, persistent schema/default changes, broad filesystem audit, native experiment, application fixture, benchmark, dependency installation, provider invocation or later catalog work belongs to M1-A. No new tests or app fixtures are required for this documentation checkpoint.

### Verification and done criteria

Use read-only source references and documentation checks; inspect the resulting documentation diff and links. No Cargo/rustc/Nextest/Clippy or native gate applies.

M1-A is done when:

- both required artifacts contain the five operation cases and explicit owner boundaries;
- both identity witnesses and meaningful refusal counterexamples are present;
- object identity, content identity, path freshness and effect-time authority remain distinct;
- primary sources support load-bearing native/dependency claims, or the claims remain explicitly unproven;
- test claims are labeled source-inspected, model status/budget is truthful, and native execution is `NOT_RUN`;
- H0 remains viable absent evidence of a required stronger contract;
- production code/configuration/tests remain unchanged.

Return completed M1-A evidence to the parent for its checkpoint commit and normal best-effort push. Stop before M2-A decision, independent review or closure.

## Genuine uncertainties reserved for the author

- Which external-writer guarantees are actually required, beyond current preview wording and best-effort checks.
- Whether a proposed native mechanism can preserve chosen source, destination, parent and overwrite/absence authority through effect, including the resolved trash backend.
- Which executor failure and cancellation details the existing GUI presents after partial completion.
- Whether native evidence is necessary for a later stronger-contract recommendation.

These are bounded contract/proof questions. They are not permission to expand implementation, assume a defect requiring repair, or claim a reproduced Windows failure.
