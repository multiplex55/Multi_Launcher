# Verification

No tests yet; targeted nonzero Nextest only after owner inspection. No native race claims. Preparation cap 20 min; built test execution 180 s; only actual failure corrections.

## M1-A source checkpoint — verification PENDING

The parent approved one combined G01 focused preparation/run after M1-B. No Cargo, Nextest, compiler, ACL or native experiment was run for M1-A. This source checkpoint is not accepted implementation evidence until that batch and review succeed.

Six inline tests authored under `diff::file_ops::tests`:

- `destination_not_found_allows_new_copy`: injected absence reaches the actual executor and copies to an absent destination.
- `destination_read_errors_prevent_new_copy`: both PermissionDenied and InvalidInput return the exact relative path and contextual destination error; destination remains absent.
- `destination_read_errors_prevent_overwrite`: both read errors retain their context and preserve the existing destination bytes.
- `destination_missing_after_preview_prevents_overwrite`: expected existing destination plus injected NotFound refuses to copy, preserving the copyable destination.
- `destination_changed_identity_prevents_overwrite`: a real length-changing edit after preview refuses overwrite through the public executor.
- `destination_matching_identity_allows_overwrite`: unchanged existing destination is overwritten through the public executor.

Failure assertions also check no affected subtree and no cancellation claim. Injected-read counts/path assertions ensure the tested observations occur at the destination boundary while root/source validation stays real. Fixtures use only task-created temporary directories and ordinary bytes; no application is constructed. Existing domain tests remain unchanged and are included in the planner's combined selection.

Authored test count: 6; executed test count: 0 (`NOT_RUN`). Static diff review and `git diff --check` passed during implementation. Final scoped static checks are recorded in the implementer role record. Source checkpoint acceptance, test inventory, combined run and independent review remain pending.
