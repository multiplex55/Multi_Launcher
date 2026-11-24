# Add or Edit App

G06 owns `add-action`. The App Editor owns the Add App, Edit App and Repair Target forms. A dropped synthetic file can prefill Path. Repair can enter through a current launcher result.

| Criterion | Source-backed behavior |
| --- | --- |
| APP-FORM-01 | Validate path and fields before Save. Browse preserves native ownership. Repair keeps an exact validated record and typed path chooser, then explicit Confirm. After submission, a late terminal result cannot mutate a retired form. |

Select `target_repair_ui_uses_explicit_typed_picker_and_confirm`, `target_repair_ui_folder_rows_only_offer_folder_picker_and_ignore_stale_completion` and `target_repair_ui_editor_close_retires_child_and_rejects_late_completion`.

`G06-EGUI-PLUGIN-APP-EDITORS` verifies the actual owned Add App draft label and Cancel. `G06-EGUI-CUSTOM-ACTION-SAVE` verifies an invalid path keeps the draft and an exact valid path persists it. `G06-EGUI-EXISTING-ACTION-EDIT` verifies prefilled values, Cancel and Save with the exact optional arguments. `G06-EGUI-ACTION-REMOVE-REPAIR` verifies the typed replacement path and saved row. `G06-EGUI-REPAIR-FILE-PICKER-CANCEL` verifies owned picker cancellation. `G06-EGUI-REPAIR-NATIVE-FILE-SELECTION` verifies the selected exact file populates the draft, restores focus and commits only after explicit confirmation. Folder selection, repair failure/recovery and GPUI remain pending. [Validation log](../VALIDATION_LOG.md)

## First native Add App packet

1. Move Add App candidate validation into `src/actions` and reuse it from the existing egui Add adapter. Require an existing nonempty Path, retain Label and Description exactly, and keep exact nonblank Args when enabled. Add appends one row through `actions::update_actions`, including duplicate labels or paths. Preserve the egui indexed-tail publication and the separate Edit and Repair owners.
2. Place the retained App Editor entry and Add App form in a focused `src/gpui_frontend/add_action.rs` child of the root. Keep existing scroll anchors stable. Render New App, Label, Description, editable Path, Add arguments, optional Args, Add, Cancel and status. Return from the child to New App on Cancel or owned Escape, and from the editor to its live origin on Close. Keep form body and footer reachable in a compact viewport.
3. Add the real Kit File → Apps → Edit Apps route through `LauncherMenuBar` and the existing root focus action pattern. Preserve the handoff through submenu dismissal. Capture one draft and request through `RestartAdmission` and the existing background write pattern. Apply only the exact acknowledgement, retain the draft on failure, and fence conflicting Add, dismiss, native close and restart. A live acknowledgement can settle while hidden. Add focused validation, atomic write and receipt-order cases.

Run one focused `fast-dev` Nextest batch with the standard stack wrapper and one linked Windows replay. Use an owned existing file, an Actions seed, and both Paste Sequence shortcuts disabled. Verify invalid Path feedback, Cancel with unchanged bytes, actual atomic-replace refusal using a held owned file, exact checked retry and explicit egui `app <label>` readback without activating the row. Ordinary filesystem actions remain outside current GPUI Search admission. App Editor listing, Edit, Remove, Repair, Browse and full APP-FORM-01 acceptance remain pending. Explorer drop needs separate native evidence because the current desktop drag tool is bound to one window. [Shared Actions owner](../../../../src/actions/mod.rs) · [Baseline authoring evidence](../VALIDATION_LOG.md)

The owned `g06-add-app-candidate` profile started with both Paste Sequence shortcuts disabled, seven PageFixture Actions at `49F5F420FF8B16DE91046A065AC33756AE4707002DCC878175888F4545542CFD`, and the inert Path fixture `Owned Add App Target.exe` at `93ACD85887AE3A694354F167738655DFAA4B5F32F87E6FDC7A42B1614DA37861`. The separate `g06-add-app-nextest` workspace remap has the manifest and standard stack wrapper.

The 10 focused tests and the corrected Windows replay verified the first Add scope. The owned profile retains the exact eight-row Actions acknowledgement, with the original seven rows and one new action. Explicit egui read it from the same store. Both processes retired and the owned file lock released. The saved custom catalog and checked editing packet follows. [Native evidence](../VALIDATION_LOG.md#g06-first-native-add-app) · [Custom catalog handoff](actions-editor.md#saved-custom-catalog-and-checked-editing-packet)
