# Add or Edit App

G06 owns `add-action`. The App Editor owns the Add App, Edit App and Repair Target forms. A dropped synthetic file can prefill Path. Repair can enter through a current launcher result.

| Criterion | Source-backed behavior |
| --- | --- |
| APP-FORM-01 | Validate path and fields before Save. Browse preserves native ownership. Repair keeps an exact validated record and typed path chooser, then explicit Confirm. After submission, a late terminal result cannot mutate a retired form. |

Select `target_repair_ui_uses_explicit_typed_picker_and_confirm`, `target_repair_ui_folder_rows_only_offer_folder_picker_and_ignore_stale_completion` and `target_repair_ui_editor_close_retires_child_and_rejects_late_completion`.

`G06-EGUI-PLUGIN-APP-EDITORS` verifies the actual owned Add App draft label and Cancel. `G06-EGUI-CUSTOM-ACTION-SAVE` verifies an invalid path keeps the draft and an exact valid path persists it. `G06-EGUI-EXISTING-ACTION-EDIT` verifies prefilled values, Cancel and Save with the exact optional arguments. Native picker, Repair and GPUI remain pending. [Validation log](../VALIDATION_LOG.md)
