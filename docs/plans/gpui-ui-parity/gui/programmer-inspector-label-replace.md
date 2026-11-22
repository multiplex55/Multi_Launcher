# Replace Bit Labels?

G15 owns `programmer-inspector-label-replace`. [Programmer Inspector](programmer-inspector.md) opens it from a current owned label-file import when different nonempty labels would be replaced. The Windows native decision uses the exact Inspector owner and defaults to No.

| Criterion | Source-backed behavior |
| --- | --- |
| BIT-LABEL-REPLACE-01 | Bind selected import, exact active host, epoch and draft generation before and after native picker and confirmation. Yes applies the prepared labels only. No, native close, invalid file or current-owner loss preserves the draft. |
| BIT-LABEL-REPLACE-02 | Preserve version/validation and atomic replacement of annotations. Source/input and comparison data stay separately owned. Show actionable import outcome. |

Select `inspector_label_file_import_export_is_labels_only_and_confirmed`, `inspector_label_file_stale_picker_and_confirmation_cannot_replace_new_work` and `bit_labels_reject_invalid_content_and_preserve_destination`. Native and GPUI evidence are pending.
