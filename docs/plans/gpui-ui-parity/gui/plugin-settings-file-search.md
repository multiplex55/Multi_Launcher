# File Search Plugin Settings

G07 owns `plugin-settings-file-search` in the common Settings host. Feature effects stay with the existing runtime owner.

| Criterion | Source-backed behavior |
| --- | --- |
| FILE-SEARCH-SETTINGS-01 | Preserve roots and exclusions, result and content limits, preview bytes, hidden and case options, Everything and ripgrep paths, editor/terminal arguments and diagnostics. Browse, Detect, Test and Refresh follow the current explicit effects. Keep test status tied to the exact tested path and apply only accepted settings to the shared search owner. |

Select `settings_search_cross_owner_file_search_focuses_collection_header_and_ripgrep_field`, `settings_reflow_chrome_ripgrep_form_stays_bounded_without_probing_or_editing_state` and `configured_ripgrep_result_is_valid_only_for_last_tested_path`. Use a fake probe.

Native and GPUI remain pending.
