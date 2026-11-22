# Explorer Tabs Saved Folders

G11 owns `explorer-tabs-saved-folders`. Add saved folder opens this inline picker in the current Explorer Tabs owner. The `folders` plugin supplies a versioned snapshot and the editor owns the receiving draft.

| Criterion | Source-backed behavior |
| --- | --- |
| TAB-FOLDERS-01 | Search the saved label, alias and path case-insensitively. Show the current result selection and support Refresh, Cancel and Escape. Refresh clears the previous row and uses the new version. |
| TAB-FOLDERS-02 | Select by the actual row or the owned Arrow/Page/Enter routes. Validate snapshot version/index and exact picker, editor and draft generation before appending the path. Respect the current folder-row limit. |
| TAB-FOLDERS-03 | Preserve the inline picker and focus policy through accepted host actions. Cancel or owner retirement leaves the draft intact. Appending copies a folder path from the snapshot and preserves bookmark ownership. |

Use `explorer_tabs_saved_folder_picker_accepts_actual_clicks` and `saved_folder_picker_search_copy_staleness_and_limit`. `G11-EGUI-EXPLORER-TABS-DRAFT` proves native refresh, alias filtering and keyboard activation of the exact focused result. It appended the path to the draft while preserving the bookmark store. Stale refresh, focus transfer across hosts and GPUI remain pending.
