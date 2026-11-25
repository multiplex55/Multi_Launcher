# Progress

## Current checkpoint

G05 inline Calculator now preserves both stock history timings and typed copy behavior. Matched Windows runs at 1516 × 937 verified the exact value, launcher accounting and visible root with Hide After Run enabled. [Calculator evidence](VALIDATION_LOG.md#g05-stock-inline-calculator-search)

G05 Date Arithmetic, Base Convert and inline Unit Convert use shared Search and checked clipboard completion. Matched Windows replay verified their exact payloads, including Base Convert Unicode whitespace, Unit approximation and tied fallback order. [Date evidence](VALIDATION_LOG.md#g05-stock-date-arithmetic-search) · [Base Convert evidence](VALIDATION_LOG.md#g05-stock-base-convert-search) · [Unit Convert evidence](VALIDATION_LOG.md#g05-stock-inline-unit-convert-search)

G04 Hash File mode now shows a visual and accessible Kit progress indicator. Matched Windows File-mode runs at 1516 × 937 streamed and canceled the same owned file in both frontends. [Hash progress evidence](VALIDATION_LOG.md#g04-visual-and-accessible-hash-progress)

G05 keeps Search pinned above the scrolling body and places Foundation controls in File and Help. At 1516 × 937, both frontends fit 22 complete List rows with 32-pixel pitch. Windows verified List and Grid scrolling, exact row actions, and Hash dock reveal and focus return. Header alignment, completion spacing, scale settings and styling remain open. [Anchored Search evidence](VALIDATION_LOG.md#g05-top-centered-search-and-scroll-ownership) · [List density evidence](VALIDATION_LOG.md#g05-ordinary-list-density-and-launcher-menu-controls)

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
| Current next action | Align the pinned G05 Search header and completion spacing at 1516 × 937 while retaining List density and active input ownership. Continue broader provider admission and remaining launcher actions afterward. Mixed DPI and G01 known-duration F2 remain open. The owned external editor retry confirmed launch-hide and retained Hash, and its discrete F2 presses produced no wake. |

The source candidate table remains a discovery aid while runtime inventory is open.

## Update format at implementation checkpoints

Report current goal/checkpoint, execution SHA/dirty fingerprint, confirmed GUI count, implemented count, accepted count, original-scope gaps, open migration regressions, native checks pending/blocked, criteria requiring revalidation, latest review and next action.

Keep accepted counts derived from GUI records and parity criteria. Do not infer progress from files changed, lines of code, screenshots, empty windows or successful compiles. Report effort separately from coverage if estimates are useful.

Append or link meaningful checkpoints; do not duplicate complete test receipts here.
