# G20 — Radial runtime and Radial Designer

**Status: not started.** All four checkpoints are not started. Use the initiative's single branch. egui remains the default; GPUI is explicitly selected and uses native presentation.

## Objective and dependencies

Provide both halves of the radial feature from GPUI startup: the invoked radial interface and the complete stable-ID Radial Designer with all children. Preserve activation, hit testing, submenu navigation, action execution, editing, previews, assets, shortcuts, import/export, and persistence. Reuse an existing independent non-egui native runtime substrate where appropriate; recreate all egui control surfaces in GPUI. A visually polished static wheel is not an acceptable replacement for the current interaction model.

Requires G00–G04 and G05 common routes/hosts/focus. G01 must already establish the feasible Windows presentation, input, and lifecycle approach; this goal completes the real feature instead of discovering fundamental overlay feasibility late. Coordinate capture handoffs with G19/G22 and action-selection services with their existing owners. Apply [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

G06 owns the Settings shell; G20 owns specialized radial controls and their end-to-end evidence.

## Source seeds and architectural boundary

Baseline: `multiplex55/Multi_Launcher`, `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [Designer root](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/radial_editor/mod.rs), with `action_editor`, `appearance`, `asset_picker`, `audio_controls`, `canvas`, `import_export`, `preview`, `skin_editor`, and `work_area` children.
- [Runtime rendering](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/radial/render.rs), [controller](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/radial/controller.rs), and [native integration](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/radial/native.rs).
- [Authoring service](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/radial/authoring.rs), `src/radial/authoring/menu.rs`, and `src/gui/radial_actions.rs`.

Keep radial documents, stable selection, invocation context, authoring transactions, action dispatch, geometry, and package validation in their shared owners. The active frontend owns its presentation and input adapter. Existing independent native rendering/services may remain; GPUI must never route into an egui designer or initialize an eframe overlay as a shortcut. This goal does not require rewriting a working non-egui native compositor merely to rename its owner.

## Per-GUI parity scope

Inventory each runtime layout/interaction mode, ring/cell state, submenu presentation, dynamic-source presentation, tooltip, disabled state, control, invocation path, and after-action policy found in the current model. Preserve frontmost ownership, protective gaps, true exterior dismissal, ancestor behavior, transparent-cell hit regions, text/media/audio presentation, and action handoffs. Preserve capture cleanup ordering when radial launches another native tool.

Designer scope includes menu/tree search, selection/reveal, rings/cells/submenus, canvas editing/placement, navigation history, action/query bindings, bulk edits, properties, appearance/skin/font/assets/audio, shortcuts/triggers, work-area geometry, embedded/live/native preview, import/export, Apply/Save/revert, and conflicts. Track all named children, including Cell properties, Unsaved cell properties, Add to radial, Unsaved radial changes, Delete impact, Delete populated ring, and Resolve populated cells.

Preserve draft guards when navigating or closing, the distinction between Apply and Save, explicit conflict resolution, stable IDs across reorder/rename, and pending-request identities. Source-specific dynamic settings and radial settings belong in the inventory even when their data is outside `src/gui`. Match topmost/placement policy and independent child focus; no missing editor may be replaced by raw JSON editing.

## Checkpoints

### G20-A — Bind the runtime to the selected frontend

**Status: not started.** **Owner:** G20 implementer; shared radial controller owns behavior.

**Work:** reconcile G01's proof with actual invocation/controller/native paths; map complete runtime and designer surfaces; establish GPUI scene/input adapters and command handoff.

**Invariants:** one invocation/session owner; no duplicated action dispatch; geometry and identity remain shared; inactive frontend creates no windows.

**Evidence:** source maps, reference scenes, existing geometry tests, and a real GPUI runtime invocation.

**Done:** the current radial document opens through GPUI with correct cancellation and native cleanup.

### G20-B — Complete runtime behavior

**Status: not started.** **Owner:** GPUI radial presentation/input adapter.

**Work:** implement all discovered layouts, modes, submenus, dynamic entries, hit regions, tooltips, control states, audio/media, shortcuts, and after-action transitions.

**Invariants:** frontmost visual/interactive ownership agrees; protective gaps never launch an unintended action; stale sessions cannot dispatch.

**Evidence:** scene/hit-test fixtures and native pointer/keyboard sequences, including cancel and a capture-tool handoff.

**Done:** every runtime parity row meets its interaction and native acceptance cases.

### G20-C — Complete Designer and all children

**Status: not started.** **Owner:** GPUI Designer adapters; shared authoring service remains authoritative.

**Work:** implement tree/canvas/properties, catalogs, appearance, packages, previews, shortcuts, and every confirmation/picker. Preserve draft state through resize, host changes, and async preview completion.

**Invariants:** unsaved navigation guards survive; bulk edit binds to the exact selection/generation; imports and deletes remain explicit; preview cancellation restores authoritative state.

**Evidence:** production-view workflows, saved-package comparisons, child-focus captures, and reference visual review.

**Done:** a user can author, preview, save, reload, and invoke equivalent menus entirely in GPUI.

### G20-D — Reconcile parity and native stability

**Status: not started.** **Owner:** G20 implementer; bounded review of sessions and action handoffs.

**Work:** resolve coverage gaps; check monitor placement, repeated opening, foreground changes, hidden launcher operation, external file conflicts, and egui fallback.

**Invariants:** no orphan windows/hooks; no unbounded idle refresh; existing documents and settings remain compatible.

**Evidence:** scoped test results, representative native observations, completed parity ledger, and checkpoint commits.

**Done:** runtime and all Designer surfaces satisfy mandatory behavior and Windows evidence.

## Focused validation and continuation

Actual tests include `layered_scene_preserves_frame_identity_and_frontmost_hit_priority`, `internal_gap_is_protective_and_true_exterior_is_not_owned`, `transparent_pixels_inside_a_circle_still_belong_to_the_cell`, `bulk_edit_buffers_are_bound_to_exact_selection_and_generation`, `tree_search_navigation_guard_supports_cancel_discard_and_apply`, and `designer_viewport_matches_launcher_topmost_policy`. Discover their current targets and run affected filters. Retain their behavioral purpose when changing rendering internals.

`radial_acceptance` and `perf_radial_probe` exist as feature-gated binaries. Reuse only bounded checks that address this migration's actual changes; historical acceptance suites do not become mandatory in full. Record native evidence separately from headless results. Complete the ledger and continue the next ready goal; unresolved native or child-surface behavior remains an explicit blocker rather than an accepted omission.
