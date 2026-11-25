# G07 — File Search, results, previews, and diagnostics

**Status:** In implementation. **Owner:** G07 implementer with the shared search-service owner. The current scope covers quoted Directory Filename search, checked delivery to the G04 Hash pilot and checked file and directory Explorer actions. [File Search record](../gui/file-search.md)

## Objective and dependencies

Recreate File Search in GPUI Kit with both filename and content modes, global and directory scopes, every existing filter and result action, and the separate preview surface. Preserve backend selection, result meaning, cancellation, diagnostics, and live-session behavior. The objective includes large-result usability and exact activation targets, not only a visually similar table.

Depend on G00–G04 and G05's launcher/action routing. Use G02's authoritative coordinator and G04's native utility host/virtual list foundation. Apply the [requirements](../REQUIREMENTS.md), [validation strategy](../VALIDATION_STRATEGY.md), and [master goal](../../../CODEX_GOAL_GPUI_UI_PARITY.md). G00 expands all child modes and configuration-dependent routes before acceptance.

## Verified source entry points

Pinned baseline: `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

| Area | Existing source |
|---|---|
| Main GUI and children | [src/gui/file_search_dialog.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/file_search_dialog.rs), `src/gui/file_search_dialog/{filters,results,keyboard,diagnostics}.rs` |
| Preview and windows | `src/gui/file_search_preview_dialog.rs`, `src/gui/file_search_window_host.rs` |
| Domain/session | [src/file_search/coordinator.rs](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/file_search/coordinator.rs), `src/file_search/{model,query,predicate,sorting,matching,preview,actions,export,settings}.rs` |
| Backends and routes | `src/file_search/{everything,ripgrep,walkdir,native,executor}.rs`, `src/plugins/file_search.rs`, `src/commands/handlers/file_search_diff.rs` |

The inspected GUI distinguishes filename rows, content group headers, and content-match rows. Result identity includes search ID, result key, optional match index, path, and line/column location. Preserve those distinctions; a displayed row number is not a stable target identity.

## Per-GUI scope

Inventory filename/content mode switching, global/directory inputs, metadata filters, sorting, result columns, match highlighting, grouped content results, truncation indicators, progress, cancellation, and diagnostics. Preserve error distinctions for empty search text, empty roots, and invalid metadata. Include file preview, its loading/error states, keyboard routes, copy/open/reveal/export and every other actual row action discovered in source.

Recreate all supported docked/popout/independent arrangements. Preserve whether Escape cancels a running search or closes the view in each state. Window mode transitions and opening a preview must retain the correct source result and query. Do not make a missing optional backend appear as a successful empty search.

## Bounded checkpoint packets

### A — Freeze the query/result/preview contract

**Owner:** G07 with the search-service owner. Trace launcher payloads through parsing, settings, coordinator submission, progress/results, cancellation, and preview requests. Extract remaining egui-bound intent handling while retaining existing backends. Document current scope/filter defaults, row identity, error states, and stale-result rejection.

**Invariants:** A completion belongs to one search generation; changing query/scope cannot leave old results actionable as current. Content headers are not match rows. Reported counts distinguish matched files, displayed rows, and truncation.

**Evidence and acceptance:** Shared fixtures demonstrate identical queries, result identity, ordering, and cancellation semantics through the old path. **Commit boundary:** Contract/extraction with focused regression evidence; no backend algorithm rewrite.

### B — Port the complete main search surface

**Owner:** G07 frontend implementer. Build search/scopes/filters, statuses, tables, grouping, match highlights, sort controls, and keyboard traversal with Kit. Use stable identities and bounded rendering; retain source strings separately from shortened presentation.

**Invariants:** Scrolling or sorting never changes the operation target; long paths and Unicode highlights stay accurate. Repeated idle frames do not resubmit work. All meaningful filters and diagnostics remain reachable at supported small window sizes.

**Evidence and acceptance:** Matched fixtures cover filename matches, multiple content matches per file, no results, backend failure, truncation, and a large collection. Confirm controls and result actions against the inventory. **Commit boundary:** Main search interface with full form/result behavior, leaving preview acceptance explicitly pending if necessary.

### C — Recreate previews, actions, and host transitions

**Owner:** G07 with the native-window adapter owner. Port preview loading, display, navigation, copy/open/reveal actions and any discovered specialized preview modes. Wire every accepted action through shared services. Exercise popout/dock movement, multiple supported windows, and root-hidden operation.

**Invariants:** A late preview cannot appear for another selected result or reopened session. File actions use the preserved path/location and execute once. A picker or external opener returns focus according to the reference; cancel is nonmutating.

**Evidence and acceptance:** Native recordings show result → preview → action and close/reopen while work is pending; action traces verify exact targets without relying only on screenshots. **Commit boundary:** Preview/action/window integration and scoped fixes.

### D — Accept behavioral and performance-sensitive parity

**Owner:** G07 acceptance owner. Compare both frontends against the baseline on the same disposable corpus and settings. Check keyboard-only navigation, menu dismissal, nested scrolling, scaling, hidden-root completion, and cancellation at different progress points. Record backend availability explicitly.

**Invariants:** No GPUI frame performs unbounded filesystem search; results remain responsive; an unavailable backend is an observed limitation, not fabricated successful coverage. **Evidence and acceptance:** Row-level records, selected test output, native observations, and bounded performance measurements for the agreed corpus. **Commit boundary:** Remaining parity fixes and evidence; refer final broad performance comparison to G26.

## Focused validation

Actual integration sources are `tests/domain_cases/file_search_e2e.rs` and `file_search_state.rs`, aggregated into `--test domain`. List a nonzero selection before using `cargo nextest run -p multi_launcher --test domain -E 'test(file_search_)'`, narrowing further where the edit affects only one path. Discover relevant inline preview/coordinator/GUI cases and use a bounded fixture directory. Preserve egui regression coverage, add meaningful GPUI row/action tests, and require Windows evidence for shell opening and native focus. A full nextest run is not the default checkpoint.

## Exit and continuation

All assigned main/preview/diagnostic/action/window rows must be implemented and evidenced, including backend error and cancellation paths. Unavailable native prerequisites remain clearly blocked instead of marked passed. Continue G08 and other file tools, and carry exact File Search-to-Diff or file-operation handoffs into their owning goals and G25 integration until exercised end to end.
