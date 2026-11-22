# Sensitivity Tables

G15 owns `calculation-pad-sensitivity`, runtime navigation `calculation-sensitivity`. [Calculation Pad](calculation-pad.md) Tools → Sensitivity Tables opens the independently hosted companion. The Pad session owns exact job, definition and results, with existing bounded per-sample evaluation and persisted `PadViewMetadata.sensitivity`.

| Criterion | Source-backed behavior |
| --- | --- |
| CALC-SENS-01 | Create/remove a definition with exact current literal input, range and step/count, and up to eight ordered named outputs. Support typed output add and watched choices. Show distinct pending, progress, canceled, error, current and outdated states. |
| CALC-SENS-02 | Generate/Cancel applies to the exact current document, evaluation, input and job revision. Use isolated per-sample substitution, keep main source/results unchanged, and publish only a complete current output. Preserve explicit Copy Table/CSV/Markdown for the settled table and the current table as a [Charts](calculation-pad-charts.md) data source. |
| CALC-SENS-03 | Persist the definition through the document metadata, retain a current job/table through companion reopen and keep exact host/pending ownership. Retire dialog when the source owner closes. Hidden-root work wakes the actual live owners. |

Select `sensitivity_open_requires_retained_current_pad_owner`, `sensitivity_retained_generate_copy_and_stale_guards_in_manual_mode`, `native_pad_sensitivity_companion_reopens_current_table_and_retires_with_parent`, `sensitivity_jump_input_option_focuses_without_changing_settings`, `sensitivity_cancel_and_retry_retire_the_exact_run`, `aggregate_budget_and_cancellation_do_not_publish_partial_tables` and `sensitivity_definition_persists_and_rename_undo_preserves_later_settings`. Native and GPUI evidence are pending.
