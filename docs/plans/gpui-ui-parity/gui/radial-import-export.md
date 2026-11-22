# Radial Import and Export

G20 owns `radial-import-export` in the Designer Resources section, including its inline review and native file choices. `radial skins` opens Resources directly. The launcher root pass opens the choosers and returns their outcomes to the Designer.

| Criterion | Source-backed behavior |
| --- | --- |
| RADIAL-PACKAGE-01 | Preserve package and multi-file legacy selection, prepared read-only preview, mappings, warnings and Cancel. Retain typed input, archive size and path bounds, validation, portable dependencies and stable ID remapping. |
| RADIAL-PACKAGE-02 | Merge a current checked review as one undoable draft edit. Whole replacement requires explicit confirmation and a chosen backup verified against the current bytes before validated publication. Preserve transactional rollback and correlated outcome. Bind replacement consent and backup selection to the exact current review. |
| RADIAL-PACKAGE-03 | Export selected menu, complete package or selected skin from a clean persisted revision, with the expected dependency and media scope. Retain the selected destination until the corresponding result terminates, then report the write or failure for that path. |

Source inspection found two baseline risks that need reproduction. An intermediate poll can consume the export destination before the asynchronous bytes arrive. Replacement confirmation and backup selection can remain set when a new review is assigned. Keep both ownership checks open for parity verification.

Select `file_dialog_enqueue_and_return_wake_the_correct_owners`, `package_preview_does_not_mutate_and_accept_is_one_undo_unit`, `changed_draft_and_hostile_or_unsupported_package_are_rejected`, `package_requests_require_confirmation_and_reject_stale_results`, `canonical_export_import_export_and_full_submenu_dependency_closure`, `replace_service_requires_current_revision_and_verified_backup_and_rolls_back_failure` and `package_apply_rechecks_disk_and_replace_backup_receipt`. Native and GPUI remain pending.
