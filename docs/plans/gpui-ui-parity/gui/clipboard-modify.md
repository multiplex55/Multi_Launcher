# Clipboard Modify

G13 owns `clipboard-modify`. The Clipboard Modify plugin exposes `cm`/`cm modify`, `cm template`, `cm apply`, `cm manage-templates`, `cm manage-pipelines` and `cm help` as typed `ClipboardModifyCommand::Open` with the selected section. Six sections share one Dock/Pop Out host. Preview, protection and overwrite decisions are inline. Named execution and `cm undo` use separate typed operation paths.

| Criterion | Source-backed behavior |
| --- | --- |
| CLIP-MODIFY-01 | Retain source, stable ordered stages, operation and argument controls, Reset, Reload Clipboard, Wrap preview, metadata, errors and the Apply/Apply and Close/Copy Result/Cancel footer. Large source requires explicit per-source acknowledgement. Preserve the supported keyboard actions. |
| CLIP-MODIFY-02 | The first open of any tab queues one asynchronous Load. Later reads are explicit Reload. Current request/session/source/mutation authority rejects superseded completion and never replaces user drafts with a stale read. Failure preserves existing preview and undo. |
| CLIP-MODIFY-03 | Use the existing preview coordinator and bounded debounce, cancellation and immutable input identity. Limit displayed head/tail at UTF-8 boundaries and retain full exact output for write. Show pending, running, finished, canceled and failure states and feed completion to the current retained host. |
| CLIP-MODIFY-04 | Protected source and private markers are authoritative before text fetch. Preserve busy, timeout, unavailable, nontext and empty-text distinctions. Apply preflights existing clipboard and requires exact current review on conflict. Copy Result follows the existing explicit fresh-preflight path. A successful write installs exact prior text for one Undo. Failed/private operations preserve it. Apply and Close closes only the exact current eligible host after success. |
| CLIP-MODIFY-05 | Templates and Saved Pipelines preview the current source and execute the exact selected identity. Management uses typed validation, CRUD and the canonical store. Help uses the same operation registry. Preserve host geometry, retained draft and stale retained action fences. |

Select `initial_source_read_runs_off_thread_and_repeated_open_is_first_open_only`, `superseded_source_read_success_and_failure_cannot_replace_current_state`, `failed_clipboard_reload_preserves_source_without_writing`, `hosted_overwrite_requires_the_same_preview_and_reviewed_clipboard`, `guarded_apply_preserves_undo_on_protected_or_timed_out_preflight_then_retries`, `clipboard_modify_detached_preview_and_apply_require_retained_passes`, `full_output_not_duplicated_and_apply_copy_use_full` and `native_format_inspection_fails_closed_and_recognizes_absence`. A no-system-clipboard fixture must use the existing fake `ClipboardService` with synthetic multiline input. Production first open reads asynchronously even for Help or Templates.

## GPUI Dock, protected source and preview

The exact stock `cm` and `cm modify` search actions open the retained docked Modify pane. The pane uses the shared runtime and protected clipboard service for first Load and explicit Reload, with one active read across a pane close and reopen. Initial Load establishes the Reset baseline. Editing, stage changes and successful Reload can request preview. Failed or stale reads preserve the current source and preview. Root Close, Quit and restart wait for canonical runtime initialization to settle.

Stages use the shared catalog and typed arguments, including the distinction between an unset separator and an explicit empty one. Preview uses the shared coordinator, with wrap control, bounded Unicode-safe display, counts and exact full output. The displayed preview operation owns Cancel. Large input approval belongs to the exact source revision.

The scoped implementation evidence is in the [preview checkpoint](../CHECKPOINTS.md#g13-clipboard-modify-protected-source-and-preview). Matched Windows Load, Reload, editable stages, focus, Reset, Cancel and compact layout remain pending.

## GPUI Apply, Copy Result and overwrite review

An accepted completed preview can Apply, Apply and Close or Copy Result through the shared clipboard service. Apply compares the loaded baseline and uses the same current preview for an explicit Overwrite review. The approved clipboard item is checked by text and Windows stamp. Copy Result refreshes its protected preflight and keeps the pane open. Effects use the full output and preserve the prior Undo slot when a write is refused. Real Kit editor state is held read-only while a worker or review owns the action.

The scoped implementation evidence is in the [checked effects checkpoint](../CHECKPOINTS.md#g13-clipboard-modify-checked-clipboard-effects). Matched Windows guarded writes, held keys, Cancel and focus remain pending.

## GPUI Launcher Search Undo

`cm undo` runs through the guarded launcher effect owner even when Clipboard Modify is closed. It restores the exact previous text after a stamped protected preflight and consumes Undo only after the accepted writer succeeds. Success keeps Search state and requests Hide for the exact initiating scope. Refused or stale work preserves Undo and stays visible when the Search receipt is current.

The scoped evidence is in the [Undo checkpoint](../CHECKPOINTS.md#g13-clipboard-modify-typed-undo). Matched Windows replay remains open.

## GPUI Templates, Saved Pipelines and Help

`cm template`, `cm apply` and `cm help` share the Modify workspace's protected first Load and retained source. Templates and Saved Pipelines use current canonical identities and the shared preview coordinator. Apply Template and Execute require the completed current preview for a visible selected row, and use the same checked Apply and Overwrite review. Saved Pipelines display the stock bounded source card. Help filters the real registry data and preserves distinct row identities.

The scoped evidence is in the [catalog checkpoint](../CHECKPOINTS.md#g13-clipboard-modify-templates-saved-pipelines-and-help). Windows replay remains open.

## GPUI Manage Templates and Manage Pipelines

Both editors retain ordered, editable rows with stable identity. Drafts use the canonical model and checked store. Save compares the edited collection with its baseline and merges the latest validated sibling. Explicit reload reads the current file and offers draft replacement only while the original scope is current. Referenced templates keep their draft rows. A durable Pipeline Delete uses explicit review and retains the row until a checked success. Duplicate waits for explicit Save. Pipeline stages use shared typed editors and complete-output preview.

The scoped evidence is in the [management checkpoint](../CHECKPOINTS.md#g13-clipboard-modify-catalog-management). Windows replay, direct execution, Settings, Pop Out, pins and Jump remain open.
