# Progress

## Current checkpoint

G07 File Search now delivers the exact current file to G04 Hash in Docked and Pop Out. Focused tests and Windows replay verified both matched digests, same-file retention, changed-file invalidation, restart, Escape and closed-selector Calculate. G04 main and dropdown Jump, G01/G03 retirement, G05 external execution and G06 checked App Editor retain their scoped evidence. Runtime inventory closure, broader providers, mixed DPI, global invocation and active UI Automation focus remain open. [File Search handoff](VALIDATION_LOG.md#g07-and-g04-checked-file-search-to-hash-handoff) · [Hash dropdowns](VALIDATION_LOG.md#g04-hash-dropdown-item-jump-mode) · [Provider evidence](VALIDATION_LOG.md#g05-folder-and-bookmark-search-admission) · [App Editor evidence](VALIDATION_LOG.md#g06-saved-custom-app-editor-catalog-and-checked-editing)

## Execution status

| Measure | Current value |
| --- | --- |
| Initial work goals specified | 27 |
| Goals with active or completed work | G00–G07, the G11 Downloads Triage slice and the G23 Toast Log slice. G00 source discovery and representative baseline are accepted. G01–G07, G11 and G23 have open criteria. |
| Migration branch / execution parent | `feat/gpui-ui-parity` / `98bbc0ccc22a297e8eafae7e844c2f7e45854856` |
| Runtime-confirmed complete GUI inventory | Not yet established |
| Source-backed GUI records with tracked criteria | 219 unique entries in the [parity matrix](PARITY_MATRIX.md), including 39 Dashboard widgets, 17 built-in Settings owners and one dynamic-library capability slot. |
| GUI matrix stages | 205 Specified, 13 In implementation, 1 Validation pending, 0 fully accepted. Individual records track the implemented and verified behaviors. |
| Shared-engine evidence | Both frontends use the shared Search composition, typed command outcomes, Hash session and canonical action mutations. See the validation log for covered routes and gaps. |
| Tests/builds/native checks run for the implementation | [Validation log](VALIDATION_LOG.md) |
| Known migration-caused regressions | The compact Add App viewport regression was corrected and verified in its Windows replay. Open parity gaps are tracked in the matrix and current checkpoints. |
| Required native verification | Partially exercised. See the [validation log](VALIDATION_LOG.md) for exact receipts and open gates. |
| Evidence requiring revalidation | Recheck only affected prior evidence when an ownership or behavior change makes it stale. Each validation entry names its source and artifact. |
| Current next action | Mount the real Downloads Triage scan and checked Inspect file handoff for the remaining Hash provider dependency. Stale accessibility focus, native egui Clipboard and mixed DPI remain open for Hash. The matched F2 probe left native delivery and foreground restoration open. |

The source candidate table remains a discovery aid while runtime inventory is open.

## Update format at implementation checkpoints

Report current goal/checkpoint, execution SHA/dirty fingerprint, confirmed GUI count, implemented count, accepted count, original-scope gaps, open migration regressions, native checks pending/blocked, criteria requiring revalidation, latest review and next action.

Keep accepted counts derived from GUI records and parity criteria. Do not infer progress from files changed, lines of code, screenshots, empty windows or successful compiles. Report effort separately from coverage if estimates are useful.

Append or link meaningful checkpoints; do not duplicate complete test receipts here.
