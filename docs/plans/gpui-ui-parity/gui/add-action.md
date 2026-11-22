# Add or Edit App

G06 owns `add-action`. The App Editor owns the Add App, Edit App and Repair Target forms. A dropped synthetic file can prefill Path. Repair can enter through a current launcher result.

| Criterion | Source-backed behavior |
| --- | --- |
| APP-FORM-01 | Validate path and fields before Save. Browse preserves native ownership. Repair keeps an exact validated record and typed path chooser, then explicit Confirm. After submission, a late terminal result cannot mutate a retired form. |

Select `target_repair_ui_uses_explicit_typed_picker_and_confirm`, `target_repair_ui_folder_rows_only_offer_folder_picker_and_ignore_stale_completion` and `target_repair_ui_editor_close_retires_child_and_rejects_late_completion`.

Native and GPUI remain pending.
