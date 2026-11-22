# Convert Plain Links

G12 owns `note-plain-link-conversion`, runtime ID `note-plain-link-prompt`. In a current [Note Editor](note-editor.md) Edit/Split, Markdown → Convert plain links reviews the entire unsaved source. `notes_markdown::convert_plain_links` owns detection and output.

| Criterion | Source-backed behavior |
| --- | --- |
| NOTE-CONVERSION-01 | Report eligible web links, directories and files, protected/already-linked text and skipped or invalid paths. Preserve existing Markdown/code/wiki links. No eligible candidates yields feedback without a decision. |
| NOTE-CONVERSION-02 | Bind confirmation to exact current source and owner. If source changed, recompute the review and require another explicit confirmation, or dismiss it when no candidates remain. |
| NOTE-CONVERSION-03 | Convert applies a normal unsaved draft change and restores source focus. Cancel, Escape and title close leave content alone. Disk bytes change only through existing save behavior. |

Select `plain_link_conversion_starts_from_entire_unsaved_buffer`, `plain_link_conversion_noop_has_no_dialog_or_mutation_and_can_notify`, `cancelling_plain_link_conversion_only_clears_transient_state`, `confirming_plain_link_conversion_uses_mutation_invalidation_without_saving`, `stale_plain_link_confirmation_refreshes_then_requires_second_click`, `stale_plain_link_confirmation_closes_when_candidates_were_removed` and `converter_protects_markdown_code_wiki_and_is_idempotent`. A native smoke can cancel, retry and convert an inert synthetic URL, then discard the note draft with its file hash unchanged. Native and GPUI evidence are pending.
