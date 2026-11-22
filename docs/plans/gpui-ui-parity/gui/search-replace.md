# Search and Replace

G09 owns `search-replace`. The `search_replace` plugin matches trimmed, case-insensitive `replace` and emits the payload-free `replace:open` action. `src/gui/search_replace_dialog.rs` owns the draft, retained host and result presentation. `src/search_replace.rs` owns exact search snapshots and replacement candidates. `src/replace_executor.rs` owns guarded writes and durable receipts.

| Criterion | Source-backed behavior |
| --- | --- |
| REPLACE-01 | Open with no chosen root or implicit search. Use an accepted native folder picker for the current dock/popout owner. Default to direct children, case-insensitive Literal search, Literal replacement and the supported `rs, txt, md, toml, json` extension filter. Show meaningful readiness if an owner or root is unavailable. |
| REPLACE-02 | Preserve search and replacement editors, Include subfolders, Rust regex and Case sensitive, Literal or Rust capture substitution, supported extensions and root-relative include/exclude rules. Changing root or search draft requires explicit Search / Refresh. Replacement or approval edits recompute only the appropriate candidate. |
| REPLACE-03 | Keep Search read-only, asynchronous and cancelable. Report progress, limits, issues and partial results. Files with Matches supports filtering, sorting and bulk controls for the visible set. File Approvals starts with none selected and binds approvals to exact match identities independently of the active inspection row. |
| REPLACE-04 | Preserve bounded Original and Proposed panes and unified preview, independent scrolling, folding and exact current selection. Shared candidate construction preserves UTF-8/BOM, line endings and unselected bytes. The preview grants review only when the current candidate is eligible. |
| REPLACE-05 | Use the remembered utility presentation, retain eligible draft/results on a host move, and keep a live native host functional with the root hidden. Close retires the session and active approvals, cancels current workers through their owners, and leaves durable receipts intact. |
| REPLACE-06 | Use the [confirmation record](search-replace-confirm.md) for Apply, Restore and Recovery. Maintain operation progress, guarded stale-file outcomes and partial-batch feedback. The tool owns its advanced scope and the existing app-owned executor. |

`G09-EGUI-REPLACE-ROOT-GATE` records the real launcher route, both editable drafts and the native root picker/cancel with unchanged fixture bytes. Search, approval, preview and GPUI remain pending. Use `m9d_search_replace_command_opens_without_root_or_implicit_search`, `m9d_selected_match_freezes_bounded_candidate_without_writing_source` and `replace_result_folds_filter_sort_and_virtual_scroll_preserve_exact_approvals` for affected paths.
