# Create Temp File

G13 owns `create-tempfile`. Bare `tmp` offers `tmp: create` and the dashboard Tempfiles Add action reaches the same typed `tempfile:dialog` route. `TempfileDialog` owns the root egui utility. The existing Tempfile plugin owns the configured directory, name validation and write.

| Criterion | Source-backed behavior |
| --- | --- |
| TEMPFILE-01 | Open the current root form with blank Alias and multiline Text. Preserve remembered logical size, scroll and reachable Save/Cancel. Cancel and title close create no file. True reopen clears both fields. |
| TEMPFILE-02 | Save requires a nonblank alias, validates forbidden filename characters, writes exact UTF-8 and uses the first available sequential filename. Use the configured directory. The existing sequential collision check has no external-writer serialization. |
| TEMPFILE-03 | Open the file association after a successful write. Successful open refreshes search, optionally toasts, closes and restores query focus. Write failure keeps the draft. Open failure reports the error and retains the file and draft. |

Select `search_tmp_returns_dialog`, `create_named_file_rejects_invalid_alias`, `typed_simple_dialogs_preserve_interactive_lifecycle` and `long_text_scrolls_above_reachable_footer_without_render_mutation`. `G13-EGUI-SNIPPET-TEMPFILE-DRAFTS` covers native bare route, multiline Unicode draft, Cancel and blank reopen in an explicit empty scratch `ML_TMP_DIR`. Dashboard route, successful write/open and GPUI remain pending.
