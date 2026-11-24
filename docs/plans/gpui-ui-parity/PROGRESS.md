# Progress

## Current checkpoint

G06 is completing the saved custom App Editor catalog and checked Edit/Remove. The first native Add App, folder and bookmark Favorites authoring and G05 compact launcher width have scoped Windows evidence. Runtime inventory closure, broader providers, mixed DPI and active UI Automation focus remain open. [App Editor checkpoint](CHECKPOINTS.md#g06-saved-custom-catalog-and-checked-editing) · [Add App evidence](VALIDATION_LOG.md#g06-first-native-add-app) · [Favorites provider evidence](VALIDATION_LOG.md#g06-folder-and-bookmark-favorites-authoring)

## Execution status

| Measure | Current value |
| --- | --- |
| Initial work goals specified | 27 |
| Goals with active or completed work | G00–G06 and the G23 Toast Log slice. G00 source discovery and representative baseline are accepted. G01–G06 and G23 have open criteria. |
| Migration branch / execution parent | `feat/gpui-ui-parity` / `98bbc0ccc22a297e8eafae7e844c2f7e45854856` |
| Runtime-confirmed complete GUI inventory | Not yet established |
| Source-backed GUI records with tracked criteria | 219 unique entries in the [parity matrix](PARITY_MATRIX.md), including 39 Dashboard widgets, 17 built-in Settings owners and one dynamic-library capability slot. |
| GUI matrix stages | 207 Specified, 11 In implementation, 1 Validation pending, 0 fully accepted. Individual records track the implemented and verified behaviors. |
| Shared-engine evidence | Both frontends use the shared Search composition, typed command outcomes, Hash session and canonical action mutations. See the validation log for covered routes and gaps. |
| Tests/builds/native checks run for the implementation | [Validation log](VALIDATION_LOG.md) |
| Known migration-caused regressions | The compact Add App viewport regression was corrected and verified in its Windows replay. Open parity gaps are tracked in the matrix and current checkpoints. |
| Required native verification | Partially exercised. See the [validation log](VALIDATION_LOG.md) for exact receipts and open gates. |
| Evidence requiring revalidation | Recheck only affected prior evidence when an ownership or behavior change makes it stale. Each validation entry names its source and artifact. |
| Current next action | Finish the focused App Editor test batch and native replay. Then prioritize general application execution, broader provider admission, global invocation and window restoration, and the Hash Generator pilot. |

The source candidate table remains a discovery aid while runtime inventory is open.

## Update format at implementation checkpoints

Report current goal/checkpoint, execution SHA/dirty fingerprint, confirmed GUI count, implemented count, accepted count, original-scope gaps, open migration regressions, native checks pending/blocked, criteria requiring revalidation, latest review and next action.

Keep accepted counts derived from GUI records and parity criteria. Do not infer progress from files changed, lines of code, screenshots, empty windows or successful compiles. Report effort separately from coverage if estimates are useful.

Append or link meaningful checkpoints; do not duplicate complete test receipts here.
