# Progress

## Current checkpoint

G02 and G05 now open supported indexed local documents through checked Search and Action Sheet Execute. At 1516 × 937, both frontends opened the same two `.txt` files in the registered editor and retained exact stock History and Usage identities. GPUI refused an indexed document replaced with a directory. [Indexed document evidence](VALIDATION_LOG.md#g02-and-g05-checked-indexed-local-documents)

G02 and G05 now open ordinary indexed Windows shortcuts through checked Search and Action Sheet Execute. Both frontends preserved the authored Start In directory and exact arguments. GPUI refused a missing Start In and a shortcut replaced with a directory. [Indexed shortcut evidence](VALIDATION_LOG.md#g02-and-g05-checked-indexed-windows-shortcuts)

G05 Window Search now switches to the exact external window on the current virtual desktop. Both frontends restored the same minimized window at 1516 × 937, kept it foreground and cleared Search while the launcher left view. GPUI declined a closed target without adding a success receipt. [Window Search evidence](VALIDATION_LOG.md#g05-checked-window-search-and-foreground-handoff)

G04 Hash now uses the resolved monospace style for exact data. Both frontends displayed the full SHA-512 digest in matched docked and popout hosts. GPUI exposed one exact accessible digest and wrapped all 128 digits at Comfortable 150% without displacing Search. [Hash exact-data evidence](VALIDATION_LOG.md#g04-hash-exact-data-typography-and-digest)

G05 Action Sheet Execute now launches checked saved and indexed executables. At 1516 × 937, both frontends preserved saved arguments, recorded the correct keyboard or pointer source, and applied Clear After Run. GPUI also rejected an indexed file replaced while its sheet was open. [Action Sheet evidence](VALIDATION_LOG.md#g05-checked-application-execution-from-the-action-sheet)

G02 and G05 now launch stock-indexed local Windows executables through the checked process worker. At 1516 × 937, both frontends completed the same query with Tab and launched the same executable once with zero arguments. GPUI refused a second indexed path after it became a directory. [Indexed execution evidence](VALIDATION_LOG.md#g02-and-g05-indexed-local-windows-executable-execution)

G05 now populates blank Search on the first frame from admitted commands and saved actions. Matched Windows runs at 1516 × 937 verified the common row order, pinned Search during Paging and History recall, and an exact folder activation from a three-space query. GPUI also kept Search pinned with Docked Hash open. [Blank Search evidence](VALIDATION_LOG.md#g05-blank-search-installed-inventory)

G05 inline Calculator now preserves both stock history timings and typed copy behavior. Matched Windows runs at 1516 × 937 verified the exact value, launcher accounting and visible root with Hide After Run enabled. [Calculator evidence](VALIDATION_LOG.md#g05-stock-inline-calculator-search)

G05 Calculator history queries now publish indexed stock rows and copy only a current matching store entry. Both frontends copied the same selected history result at 1516 × 937. The GPUI replay refused a reordered entry even when its replacement had the same result text. [Calculator history evidence](VALIDATION_LOG.md#g05-checked-calculator-history-search)

G02 and G05 saved local folders now open through the checked external worker. Windows verified Add, Edit and Refresh publication from the real App Editor, the same Explorer destination in both frontends, and refusal after a saved folder was replaced. [Saved folder evidence](VALIDATION_LOG.md#g02-and-g05-checked-saved-local-folder-activation)

G04 places Hash progress, Cancel, digest and Copy in the initial view of the standard popout. Both frontends passed native close during active File calculation and reopened with a fresh Text result. The corrected GPUI layout also passed docked and Comfortable 150% compact checks. [Hash lifecycle evidence](VALIDATION_LOG.md#g04-native-hash-close-and-reopen-during-file-calculation) · [Hash layout evidence](VALIDATION_LOG.md#g04-compact-hash-viewport-actions)

G05 Date Arithmetic, Base Convert and inline Unit Convert use shared Search and checked clipboard completion. Matched Windows replay verified their exact payloads, including Base Convert Unicode whitespace, Unit approximation and tied fallback order. [Date evidence](VALIDATION_LOG.md#g05-stock-date-arithmetic-search) · [Base Convert evidence](VALIDATION_LOG.md#g05-stock-base-convert-search) · [Unit Convert evidence](VALIDATION_LOG.md#g05-stock-inline-unit-convert-search)

G04 Hash File mode now shows a visual and accessible Kit progress indicator. Matched Windows File-mode runs at 1516 × 937 streamed and canceled the same owned file in both frontends. [Hash progress evidence](VALIDATION_LOG.md#g04-visual-and-accessible-hash-progress)

G05 keeps Search pinned above the scrolling body and places Foundation controls in File and Help. The header now honors the stored launcher icon and title settings. At 1516 × 937, both frontends fit 22 complete List rows with five suggestions and Tab completes the same query. Windows also verified List and Grid scrolling, exact row actions, and Hash dock reveal and focus return. Styling, scale settings and wheel travel remain open. [Pinned header evidence](VALIDATION_LOG.md#g05-pinned-launcher-header-and-completion-spacing) · [Anchored Search evidence](VALIDATION_LOG.md#g05-top-centered-search-and-scroll-ownership) · [List density evidence](VALIDATION_LOG.md#g05-ordinary-list-density-and-launcher-menu-controls)

G11 Downloads Triage now delivers a checked regular file to G04 Hash. Windows verified exact digests, refusal of replaced files, focus return, and restart into the egui Downloads launcher. G07 File Search delivers the exact current file to G04 Hash in Docked and Pop Out. Focused tests and Windows replay verified both matched digests, same-file retention, changed-file invalidation, restart, Escape and closed-selector Calculate. G04 main and dropdown Jump, G01/G03 retirement, G05 external execution and G06 checked App Editor retain their scoped evidence. The checked web provider packet matched egui launches, and fresh Windows SDK queries verified Hash editor focus in both hosts. Runtime inventory closure, broader providers, mixed DPI and global invocation remain open. [Downloads Inspect](VALIDATION_LOG.md#g11-and-g04-checked-downloads-triage-inspect-to-hash) · [File Search handoff](VALIDATION_LOG.md#g07-and-g04-checked-file-search-to-hash-handoff) · [Hash dropdowns](VALIDATION_LOG.md#g04-hash-dropdown-item-jump-mode) · [Provider evidence](VALIDATION_LOG.md#g05-folder-and-bookmark-search-admission) · [App Editor evidence](VALIDATION_LOG.md#g06-saved-custom-app-editor-catalog-and-checked-editing)

## Execution status

| Measure | Current value |
| --- | --- |
| Initial work goals specified | 27 |
| Goals with active or completed work | G00–G07, the G11 Downloads Triage slice and the G23 Toast Log slice. G00 source discovery and representative baseline are accepted. G01–G07, G11 and G23 have open criteria. |
| Migration branch / execution parent | `feat/gpui-ui-parity` / `98bbc0ccc22a297e8eafae7e844c2f7e45854856` |
| Runtime-confirmed complete GUI inventory | Not yet established |
| Source-backed GUI records with tracked criteria | 219 unique entries in the [parity matrix](PARITY_MATRIX.md), including 39 Dashboard widgets, 17 built-in Settings owners and one dynamic-library capability slot. |
| GUI matrix stages | 204 Specified, 14 In implementation, 1 Validation pending, 0 fully accepted. Individual records track the implemented and verified behaviors. |
| Shared-engine evidence | Both frontends use the shared Search composition, typed command outcomes, Hash session and canonical action mutations. See the validation log for covered routes and gaps. |
| Tests/builds/native checks run for the implementation | [Validation log](VALIDATION_LOG.md) |
| Known migration-caused regressions | The compact Add App viewport regression was corrected and verified in its Windows replay. Open parity gaps are tracked in the matrix and current checkpoints. |
| Required native verification | Partially exercised. See the [validation log](VALIDATION_LOG.md) for exact receipts and open gates. |
| Evidence requiring revalidation | Recheck only affected prior evidence when an ownership or behavior change makes it stale. Each validation entry names its source and artifact. |
| Current next action | Continue G06 with the read-only indexed App Editor catalog. Mixed DPI and G01 known-duration F2 remain open. Window Search has separate native evidence for selected-target foreground and launcher hide. |

The source candidate table remains a discovery aid while runtime inventory is open.

## Update format at implementation checkpoints

Report current goal/checkpoint, execution SHA/dirty fingerprint, confirmed GUI count, implemented count, accepted count, original-scope gaps, open migration regressions, native checks pending/blocked, criteria requiring revalidation, latest review and next action.

Keep accepted counts derived from GUI records and parity criteria. Do not infer progress from files changed, lines of code, screenshots, empty windows or successful compiles. Report effort separately from coverage if estimates are useful.

Append or link meaningful checkpoints; do not duplicate complete test receipts here.
