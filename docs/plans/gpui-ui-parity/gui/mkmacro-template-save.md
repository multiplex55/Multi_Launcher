# Save Macro Template

G19 owns `mkmacro-template-save`. [MkMacro](mkmacro.md) Reuse → Save as Template opens the current selected macro's owned description/name form. The template store owns atomic publication to `mkmacro_templates.json`.

| Criterion | Source-backed behavior |
| --- | --- |
| MK-TEMPLATE-SAVE-01 | Prefill the current macro name and description, retain the private editable fields and exact stable root/draft/host. Cancel closes without publication. |
| MK-TEMPLATE-SAVE-02 | Explicit Save Template validates nonempty and case-insensitive unique name, reads the complete typed closure/assets and writes the separate versioned catalog. Reject invalid/future catalogs and preserve the source macro draft. |

Select `templates_survive_reload_and_capture_the_full_draft_closure`, `package_children_keep_distinct_presentation_and_live_drafts` and `mkmacro_native_parent_opens_ordinary_children_with_retained_origin`. Native and GPUI evidence are pending.
