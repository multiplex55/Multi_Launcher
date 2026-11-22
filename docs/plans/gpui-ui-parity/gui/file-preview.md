# File Search Preview

G07 owns `file-preview`. A current File Search Preview action or primary content match opens the owned File preview. The accepted parent supplies exact path, optional content match and presentation.

| Criterion | Source-backed behavior |
| --- | --- |
| FILE-PREVIEW-01 | Preserve bounded complete, surrounding context, beginning, metadata and failure, exact match highlight and read-only keyboard scroll. Reuse an identical live request and reject retired generation completions. Explorer, Copy Full Path and Copy Matching Line are explicit. |

Select `jump_mode_file_search_results_focus_then_enter_previews_one_content_match`, `file_search_native_children_inherit_and_keep_live_workers`, `preview_host_rejects_same_request_from_retired_load` and `oversized_file_returns_context_around_late_match`.

`G07-EGUI-CONTENT-MATCH` verifies the exact content result in File Search. Preview activation and GPUI remain pending.
