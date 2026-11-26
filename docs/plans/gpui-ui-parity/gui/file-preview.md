# File Search Preview

G07 owns `file-preview`. A current File Search Preview action or primary content match opens the owned File preview. The accepted parent supplies exact path, optional content match and presentation.

| Criterion | Source-backed behavior |
| --- | --- |
| FILE-PREVIEW-01 | Preserve bounded complete, surrounding context, beginning, metadata and failure, exact match highlight and read-only keyboard scroll. Reuse an identical live request and reject retired generation completions. Explorer, Copy Full Path and Copy Matching Line are explicit. |

Select `jump_mode_file_search_results_focus_then_enter_previews_one_content_match`, `file_search_native_children_inherit_and_keep_live_workers`, `preview_host_rejects_same_request_from_retired_load` and `oversized_file_returns_context_around_late_match`.

`G07-EGUI-CONTENT-MATCH` verifies the exact content result in File Search. `G07-EGUI-NATIVE-PREVIEW` verifies keyboard activation in the native File Search popout, one inherited preview window and independent preview close. The controller exposed preview window chrome.

The retained GPUI child opens selected-match context from a focused Content row or the Preview button. The same button accepts Filename files and directories. Preview uses the shared coverage and exact Unicode highlights and offers document-owned line and column navigation. Open in Explorer, Copy Full Path and Copy Matching Line use checked selections and captured text. A separate Preview keeps its document, load and geometry while the File Search parent moves and resolves checked return to the original result or control on the accepted mount. Matched native replay and moving File Search with a live inline Preview remain open. [Navigation checkpoint](../CHECKPOINTS.md#g07-bounded-preview-navigation) · [Retained Preview checkpoint](../CHECKPOINTS.md#g07-retained-native-preview-while-file-search-moves)
