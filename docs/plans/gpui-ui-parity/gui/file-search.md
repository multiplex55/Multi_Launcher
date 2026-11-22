# File Search

G07 owns `file-search`, also the canonical remembered presentation and geometry key. `G07-EGUI-FILE-SEARCH` records the native egui directory search and result refinement baseline. Hash Generator owns the target utility and the File-mode calculation. See the [validation log](../VALIDATION_LOG.md) and [Hash Generator](hash-generator.md).

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

On Windows, search a single disposable file through the quoted Directory route and inspect its result/refinement. Exercise a right-click on that exact filename and open Calculate hash. Verify the chosen path, fresh or retained session, and known digest for exact file bytes. The available desktop tool reached the result but reported unavailable coordinate geometry for the context menu. The actual native handoff still needs pointer access.
