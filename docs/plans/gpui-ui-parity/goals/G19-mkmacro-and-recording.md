# G19 — Macro authoring, recording review, and Screen Recorder

**Status: not started.** Checkpoints A–D are not started. Work on the initiative's single branch; egui remains the no-argument default and GPUI remains native and opt-in.

## Objective and dependencies

Recreate the complete macro authoring family and Screen Recorder, including every editor, picker, prompt, review, controller, inspector, package/library/template workflow, help surface, and capture child. The result must author and run the same macro documents and recordings through the existing engines. A step list with only common actions is not feature parity.

Requires G00–G04 and G05 common routes/hosts/input. Use G01's native feasibility results and shared capture ownership; use G18's generic image adapters where appropriate. G19 owns macro-specific image authoring and overlays, even when those consume shared G22 platform infrastructure. Follow [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

G06 owns the Settings shell; G19 owns specialized macro/recording controls and their end-to-end evidence. Existing independent native capture services may be reused without retaining egui controls.

## Actual source seeds and ownership

Baseline: `multiplex55/Multi_Launcher`, `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [MkMacro root and child inventory](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mkmacro_dialog/mod.rs): inspect every declared child module and its open routes.
- [Action catalog](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mkmacro_dialog/action_catalog.rs), [recording review](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mkmacro_dialog/recording_review.rs), and [parameter prompt](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/mkmacro_dialog/parameter_prompt.rs).
- [Screen Recorder UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/recording_dialog.rs), backed by `src/recording/`.
- [Legacy macro dialog](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/macro_dialog.rs) and [MkMacro plugin settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/mkmacro.rs).

The macro compiler/runtime/store/recorder and media-capture engines remain shared owners. GPUI owns editing buffers, selections, focus, presentations, and commands into those services. Preserve stable identities and transactional edits instead of translating a document into a second UI-specific macro model.

## Per-GUI scope

Track the macro/folder list, properties, toolbar, structured step table, folding/outline, Find/Replace/Jump, action catalog/editor, conditions, call/signature editors, typed values, variable catalog, runtime inspector, parameter prompts, package/library/template/help surfaces, and dirty/destructive confirmations individually. Inventory every action variant and field from the current catalog; do not infer completeness from a representative demonstration.

Include key capture, launcher action/query pickers, window matcher/picker, UI Automation editor, image assets/previews/search/crop, image destinations and overwrite decisions, OCR controls/test jobs, and point/rectangle capture. Preserve child docking/pop-out, draft ownership, Escape precedence, multiselection moves, clipboard operations, bookmarks/breakpoints, and text-versus-key distinctions.

Recording Review includes start/pause/stop, timing, range selection, markers, annotations, optional cleanup suggestions, trial playback, and explicit anchored Apply. Recording must not insert or save without that action. Preserve macro parameter File/Folder/Choice presenters and independent Run, cancellation/editable fallback, isolated call frames, dependency import previews, template fresh identities, cleared copied hotkeys, and sensitive runtime-value masking.

Screen Recorder includes setup, asynchronous window list/refresh, display/rectangle selection, GIF/MP4, FPS, duration editing/wheel steps, size/cursor options, output chooser, countdown, region preview/active outlines, compact controller, Stop/Discard, saved/failure result, and hotkey settings. Opening setup or canceling output selection starts nothing. Preserve frozen run parameters, no-clobber output, target-loss handling, capture exclusion, and controller access while the launcher is hidden.

## Checkpoints

### G19-A — Inventory action coverage and share authoritative state

**Status: not started.** **Owner:** G19 implementer and existing macro/recording services.

**Work:** map every child module, action variant, command, settings callback, and host. Establish GPUI draft adapters and service subscriptions, with identity-aware cancellation.

**Invariants:** no format or runtime semantic change; no duplicated hotkey/capture owner; no document mutation from rendering.

**Evidence:** exhaustive catalog-to-parity rows, representative recordings/documents, and focused existing domain tests.

**Done:** all surfaces have explicit owners and the GPUI editor opens existing documents safely.

### G19-B — Complete authoring and reusable macro workflows

**Status: not started.** **Owner:** GPUI editor and child-view adapters.

**Work:** implement all action fields, structured editing, navigation, pickers, prompts, signatures/calls, assets, packages/templates/help, and runtime inspection. Integrate ordinary and native child hosts.

**Invariants:** stale pickers cannot update new selections; clipboard failure cannot destroy a selection; import/Apply/Run remain separate deliberate actions.

**Evidence:** production-view interactions, fixture round trips, caller/child focus review, and catalog coverage reconciliation.

**Done:** every authoring parity row has working controls, correct effects, and error/cancel behavior.

### G19-C — Complete both recording workflows

**Status: not started.** **Owner:** GPUI macro-recording and Screen Recorder adapters; existing workers own capture.

**Work:** implement Review/annotation/controller flows and full media-recorder setup/countdown/controller/results. Preserve hotkey conflicts, capture admission, outlines, and visibility behavior.

**Invariants:** explicit Apply alone inserts recorded steps; file chooser cancel admits no recording; Stop/Discard occur once for the current run.

**Evidence:** focused state tests plus native record/pause/review/apply and GIF/MP4 smoke cases with disposable outputs.

**Done:** recordings complete or cancel with correct cleanup and all controllers remain reachable.

### G19-D — Close children, compatibility, and fallback gaps

**Status: not started.** **Owner:** G19 implementer; review focuses on service ownership and stale completions.

**Work:** reconcile every catalog row, nested surface, settings callback, small-window interaction, and async failure. Verify egui still uses the same saved macros.

**Invariants:** no GPUI path opens egui; closed reviews release observations; native hooks/resources settle on cancellation.

**Evidence:** scoped test log, native interaction captures, compatibility results, and checkpoint commits.

**Done:** mandatory authoring, recording, native, and fallback evidence is complete.

## Focused validation and continuation

Existing integration targets include `mkmacro_authoring`, `mkmacro_recorder`, `mkmacro_runtime`, `mkmacro_store`, `mkmacro_visual`, and `mkmacro_launcher_integration`; select only affected tests. Recorder unit tests include `recording_duration_actual_edit_step_wheel_and_invalid_input`, `recording_setup_cancelled_path_does_not_admit_work`, and `recording_ui_reciprocal_capture_guards_reject_before_mutation`. Discover current compiled names before filtering. Add GPUI behavior cases for changed boundaries, not copies of rendering internals.

Completion requires every editor and child, not merely successful macro execution. Persist progress and accepted commits, then continue. If a native capability is blocked, preserve partial work and record its dependent rows; do not silently omit that editor or launch an egui substitute. Broad nextest follows the shared strategy.
