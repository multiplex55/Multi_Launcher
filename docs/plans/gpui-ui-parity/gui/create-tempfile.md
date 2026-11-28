# Create Temp File

G13 owns `create-tempfile`. Bare `tmp` offers `tmp: create` and the dashboard Tempfiles Add action reaches the same typed `tempfile:dialog` route. `TempfileDialog` owns the root egui utility. The existing Tempfile plugin owns the configured directory, name validation and write.

| Criterion | Source-backed behavior |
| --- | --- |
| TEMPFILE-01 | Open the current root form with blank Alias and multiline Text. Preserve remembered logical size, scroll and reachable Save/Cancel. Cancel and title close create no file. True reopen clears both fields. |
| TEMPFILE-02 | Save requires a nonblank alias, validates forbidden filename characters, writes exact UTF-8 and uses the configured directory. Shared no-clobber publication preserves the first available sequential filename and retries a late collision without replacing the winner. |
| TEMPFILE-03 | Open the file association after a successful write. Successful open refreshes search, optionally toasts, closes and restores query focus. Write failure keeps the draft. Open failure reports the error and retains the file and draft. |

Select `search_tmp_returns_dialog`, `create_named_file_rejects_invalid_alias`, `typed_simple_dialogs_preserve_interactive_lifecycle` and `long_text_scrolls_above_reachable_footer_without_render_mutation`. `G13-EGUI-SNIPPET-TEMPFILE-DRAFTS` covers native bare route, multiline Unicode draft, Cancel and blank reopen in an explicit empty scratch `ML_TMP_DIR`. The [GPUI checkpoint](../CHECKPOINTS.md#g13-checked-create-temp-file) records tested Windows publication and fake association. Matched native association, dashboard invocation and resize persistence remain open.

## Typed creation

`tmp new` and `tmp create` admit the exact stock row with or without an alias. The shared owner publishes an empty `.txt` without replacing an existing file and opens the exact created object in the registered editor. Successful handoff uses stock Search and History behavior. Partial publication keeps the file and records its path. Paired native handoff and editor replay remain open. [Typed creation checkpoint](../CHECKPOINTS.md#p016-checked-typed-temporary-file-creation)

## Listing and opening temporary text files

Stock `tmp list [filter]` shows the current ordinary text files from the configured store. The exact result uses checked Windows association through the existing external-effect owner. Create refreshes the list when the relevant query still owns the launcher. Stock `tmp open` uses the same configured directory and prepares a missing local directory through held relative handles before checked Shell handoff. [Temporary directory checkpoint](../CHECKPOINTS.md#p016-checked-temporary-directory-open) Matched native association and the wider management and alias routes remain open. [List and Open checkpoint](../CHECKPOINTS.md#p016-checked-temporary-file-list-and-open)

## Direct Alias and Remove

The checked `tmp alias <filename> <alias>` and `tmp rm [filter]` commands act on the exact current local text file. The command receives the same filename grammar and alias as stock egui. The dedicated Set Alias form retains its original criteria. [Alias and removal checkpoint](../CHECKPOINTS.md#p016-checked-temporary-file-alias-and-removal)
