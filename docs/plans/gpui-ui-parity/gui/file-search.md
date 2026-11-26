# File Search

G07 owns `file-search`, also the canonical remembered presentation and geometry key. `G07-EGUI-FILE-SEARCH` records native egui directory search and refinement. `G07-EGUI-NATIVE-PREVIEW` verifies Pop Out retains the content match and opens its owned native preview by keyboard. `G11-EGUI-FILE-OPS-DUPLICATE-REVIEW` verifies keyboard selection and delivery of the exact file through the toolbar. Hash Generator and File Operations Center own their target workflows. [Validation log](../VALIDATION_LOG.md) · [Hash Generator](hash-generator.md) · [File Operations Center](file-operations-center.md)

## Routes and owners

- The `fs` plugin opens or focuses File Search. `fs file` and `fs content` select the mode. Adding a query starts the typed search. An existing quoted trailing directory sets Directory scope with that root. With no explicit root, use the current scope. `fs here` with a term and no directory advertises a chooser, but the inspected typed route passes only the search kind. Verify that discrepancy before changing the route. See `src/plugins/file_search.rs`, `src/file_search/query.rs` and `src/commands/handlers/file_search_diff.rs`.
- `SearchCoordinator` owns the worker, executor policy, request identity and cancellation. `FileSearchDialogState` owns form, result projection, selection and diagnostics. `FileSearchWindowHost` validates frame ownership, session instance, host generation and child opens. The independent `File Search — {instance}` popout starts at 760 × 560 with a 520 × 360 minimum and can outlive a hidden root.

## Parity criteria

| Criterion | Required behavior |
| --- | --- |
| FILE-SEARCH-01 | Preserve Filename and Content, Global and Directory, validated editable roots and picker, case and hidden controls, file type, extensions and excluded directories. Include filename ranked substring, contains and fuzzy modes, plus content phrase, any term and whole word. Preserve date/size options and frozen request parameters. |
| FILE-SEARCH-02 | Start an explicit search, show progress, backend diagnostics, completion and truncation or error. Running Escape cancels and keeps the tool open. Idle Escape closes it. Changing search parameters asks for another search. Current-results refinement updates only the visible projection and reconciles its selection. |
| FILE-SEARCH-03 | Preserve filename rows with original path, directory and match quality. Content groups retain matching lines and byte spans. Show column selection/reset, supported widths and sort order. During search, defer sorting until the result set can reconcile a stable selection. |
| FILE-SEARCH-04 | Preserve Ctrl+F and Ctrl+L focus, keyboard row navigation, ordinary activation, configured-editor and Explorer routes, and owned right-click menu actions. Export the visible or selected results through the existing clipboard/save routes. Copy diagnostics redacts the query by default and uses an explicit action for the full command. |
| FILE-SEARCH-05 | Calculate hash is available on eligible non-directory rows. It captures the clicked row identity and exact original `PathBuf`, revalidates the current search, visible row and originating host, and opens Hash Generator in File mode without starting a hash. Same path preserves an existing Hash session. A new path invalidates earlier Hash work. A live native child or accepted popout origin uses the existing presentation policy. |
| FILE-SEARCH-06 | Dock and Pop Out preserve the active request, draft, retained results, selection and eligible child context. Closing or clearing retires requests according to their existing owners. Reject old row actions after search, host or result revision changes. Keep the same presentation key and native geometry format. |

## Verification

Use `file_search::query::tests` and the typed handler coverage for command changes. For host and handoff, select `hosted_file_search_row_action_only_dispatches_for_its_current_search`, `file_search_native_children_inherit_and_keep_live_workers`, `hash_host_same_file_keeps_worker_and_stale_actions_cannot_mutate`, and the actual File Search keyboard test where applicable.

On Windows, search disposable files through the quoted Directory route and inspect their result/refinement. Exercise a right-click on an exact filename and open Calculate hash. Verify the chosen path, fresh or retained session, and known digest for exact file bytes. The matched typed replay exercised the native egui context-menu handoff. [Matched typed replay](../VALIDATION_LOG.md#g07-and-g04-typed-directory-filename-search)

## First GPUI selected-file checkpoint

This checkpoint contributes to FILE-SEARCH-01/02/03/05 and the G04 Hash pilot. Bare `fs` opens the installed provider's retained docked view. Start a Filename search with one explicit directory and a term. Use the committed File Search settings and production coordinator. Keep root validation and any filesystem-based identity work off the UI thread. Keep the egui Global/Directory and Filename/Content request behavior through the shared request construction.

The view owns the form, cancellation, results, stable selection and current search identity. Edits, cancellation and close revoke prior result actions. The launcher admits only the typed opener in this checkpoint and retains its plugin and Search capability gates. Calculate hash resolves the current selected file before delivering its original path to the existing Hash owner. The existing session starts in File mode for a fresh open, retains work and digest for the same path, and invalidates the prior result for a changed path. The source action never starts calculation. Existing Hash presentation and native effect fences apply.

Use scoped locked fast-dev checks after coherent source increments, then one targeted Nextest packet and one native candidate. Search the owned corpus with two same-named files in distinct subdirectories and check each original path against its independently computed digest. Verify repeated delivery retains the first digest, changed delivery clears it before calculation, and invalid or retired results cannot dispatch. Exercise the matching egui route where the Windows control supports it. The rest of the G07 form, diagnostics, action menus, preview and host criteria remain tracked above.

The focused batch and Windows replay verified the docked File Search handoff to both Hash hosts, including exact paths and digest retention, cancelled and edited searches, and restart. The later quoted Directory Filename replay also verified the actual egui right-click Hash action. [First handoff](../VALIDATION_LOG.md#g07-and-g04-checked-file-search-to-hash-handoff) · [Matched typed replay](../VALIDATION_LOG.md#g07-and-g04-typed-directory-filename-search)

## Typed Directory Filename Search

`fs file "Café Marker" "C:\Owned Corpus"` and the existing Filename aliases can start a search in the retained GPUI pane. The provider checks an explicit local directory off the UI thread and retains only the current query and action. The visible pane uses the existing request builder and coordinator and checks the directory again before submitting. The shared Exact Match scorer retains the stock typed action when the decoded term and root match, including quoted terms. [Focused and Windows evidence](../VALIDATION_LOG.md#g07-and-g04-typed-directory-filename-search)

## Checked Filename result Open in Explorer

GPUI can open the selected ordinary local directory or reveal the exact selected ordinary local file. It rechecks the search identity, row and file type, launcher ownership, foreground and request authority. File reveals use shared Windows Shell selection from a worker. Egui File Search keeps its toolbar, row activation and context routes, and can still open legacy path forms through the existing adapter. Both File Search hosts revoke pending selections when their source retires. Calculate hash still admits only a file. The matched Windows replay at 1516 × 937 verified an exact Unicode filename with a same-name decoy, a directory and an object replaced after publication. [Windows evidence](../VALIDATION_LOG.md#g07-checked-filename-result-open-in-explorer)

## Manual Directory Content Search

Open bare `fs`, select Content, enter a directory and a term, then start Search. The retained pane submits the request through the shared builder and coordinator and uses the configured backend policy, limits and fallback. It streams paged groups with original file paths, line and column, and separate shown and total match counts. Rendered match excerpts are bounded and preserve UTF-8. The explicit Explorer action uses the selected match's exact owning file and the existing checked reveal. Editing the draft, changing mode, changing page, cancellation and close revoke the corresponding stale actions.

Quoted `fs content "term" "C:\Owned Corpus"` and the stock aliases take the term, mode and original path from the admitted typed row. Enter on a focused match or the Preview button opens exact selected-match context. Matched Windows verification remains open. [Typed Content](../CHECKPOINTS.md#g07-typed-directory-content-search) · [File Preview](file-preview.md)

## Remembered Dock and Pop Out

Bare `fs` and admitted quoted searches use the same remembered presentation. Dock and Pop Out keep the active request, draft, results, selection and any existing native Preview. A separate popup remains available with the root hidden and supports checked Explorer for the exact selected row. It can open a separate Preview, deliver a file after exact Hash popup activation, or move a live Docked Hash into the inherited popup. Inherited Hash close can return to the exact parent and action. Close an inline Preview before moving File Search. Automatic restore of a minimized Hash destination, independent multiple instances and paired Windows verification remain open. [Focused lifecycle evidence](../CHECKPOINTS.md#g07-remembered-file-search-dock-and-pop-out) · [Retained Preview checkpoint](../CHECKPOINTS.md#g07-retained-native-preview-while-file-search-moves)
