# Progress

## Current checkpoint

G06 first native Add App passed 10 focused tests and the corrected Windows replay. The real menu, compact keyboard form, held-file failure with exact draft retention, single checked retry and explicit egui readback have scoped evidence. Folder and bookmark Favorites authoring and G05 compact launcher width retain their scoped evidence. The saved custom catalog and checked Edit/Remove are next. There are 219 source-backed GUI records and zero fully accepted GPUI product GUIs. Runtime inventory closure, broader providers, mixed DPI and active UI Automation focus remain open. [Add App evidence](VALIDATION_LOG.md#g06-first-native-add-app) · [Favorites provider evidence](VALIDATION_LOG.md#g06-folder-and-bookmark-favorites-authoring)

## Execution kickoff

| Measure | Current value |
| --- | --- |
| Initial work goals specified | 27 |
| Execution goals started / accepted | 6 / 0 |
| Migration branch / execution parent | `feat/gpui-ui-parity` / `98bbc0ccc22a297e8eafae7e844c2f7e45854856` |
| Runtime-confirmed complete GUI inventory | Not yet established |
| Provisional source inventory | 217 IDs across 161 product routes and children, 17 built-in plugin settings overrides and 39 dashboard widgets. Includes one dynamic-library capability slot. |
| Source-backed GUI records with tracked criteria | 217 unique entries in the [parity matrix](PARITY_MATRIX.md), including all 39 Dashboard widgets and 17 built-in Settings owners |
| GPUI GUIs implemented / accepted | 0 / 0 |
| Shared-engine evidence | G02-B Hash session extraction verified through egui. G02-C/D native GPUI Text/SHA-256 preview matches the egui calculation baseline and exercises the same typed handler and session. The scoped library/application Nextest run passed all 39 selected tests. |
| Tests/builds/native checks run for the implementation | [Validation log](VALIDATION_LOG.md) |
| Known migration-caused regressions | No confirmed regression in the scoped startup observations |
| Required native verification | Partially exercised. See the [validation log](VALIDATION_LOG.md) for exact receipts and open gates. |
| Evidence requiring revalidation | None yet; no migration acceptance evidence exists |
| Current next action | Check the final Settings writer outcome on the live relaunch path, then implement the idle egui-to-GPUI admission and switch. C2a worker outcomes and typed forward restart passed 15 focused tests and Windows replay. Broader Search/GUI parity, native pointer, mixed DPI and G01 global invocation remain pending. |

This does not report a migration completion percentage. The source candidate table is a discovery aid, not a reliable final GUI denominator.

## Update format at implementation checkpoints

Report current goal/checkpoint, execution SHA/dirty fingerprint, confirmed GUI count, implemented count, accepted count, original-scope gaps, open migration regressions, native checks pending/blocked, criteria requiring revalidation, latest review and next action.

Keep accepted counts derived from GUI records and parity criteria. Do not infer progress from files changed, lines of code, screenshots, empty windows or successful compiles. Report effort separately from coverage if estimates are useful.

Append or link meaningful checkpoints; do not duplicate complete test receipts here.
