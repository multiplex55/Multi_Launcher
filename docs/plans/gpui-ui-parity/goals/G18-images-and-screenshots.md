# G18 — Screenshots, image editing, comparison, color, and QR

**Status: not started.** All checkpoints are not started. Use the initiative's single branch and native GPUI frontend. egui remains the no-argument default.

## Objective and dependencies

Recreate all image-oriented GUIs and their child interactions without changing capture, image arithmetic, export, clipboard, or resource limits. Include screenshot countdown/capture/editor, generic crop and image panels, comparison, screen color selection/results, and QR generation. Each operation must retain its existing deliberate trigger and cancellation behavior.

Requires G00–G04 foundation gates and G05 common routes, input, dialogs, resources, and utility windows. Consume the capture/native integration contracts proven in G01; coordinate with G22 for shared capture exclusion and overlays, and G19 for macro image authoring. G18 owns generic image editors; G19 owns the macro-specific orchestration and prompts. Follow [REQUIREMENTS](../REQUIREMENTS.md) and [VALIDATION_STRATEGY](../VALIDATION_STRATEGY.md).

G06 owns the Settings shell; G18 owns screenshot/color settings controls and their end-to-end evidence. Shell registration alone does not accept those controls.

## Source seeds and ownership

Baseline: `multiplex55/Multi_Launcher` at `f7bc610a86bcad1c14b69dce65b1d804df1b7934`.

- [Screenshot editor](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/screenshot_editor.rs), plus `src/gui/screenshot_capture.rs` and `screenshot_countdown.rs`.
- [Image comparison UI](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/image_compare_dialog.rs), backed by `src/image_compare/`.
- [Crop editor](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/image_crop_editor.rs), `crop_dialog.rs`, `image_panel.rs`, and `src/image_crop.rs`.
- [Color picker](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/color_pick.rs) and [QR dialog](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/gui/qr_dialog.rs).
- Settings-only UI outside `src/gui`: [screenshot settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/screenshot.rs) and [color settings](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/plugins/color_picker.rs).

Existing domain/capture services retain pixel processing, limits, file publication, and clipboard policy. GPUI owns texture lifetime, display transforms, control state, and event routing. Framework-specific GPU fences may change implementation; the associated memory-bound and stale-image protections must survive.

## Per-GUI parity scope

Track screenshot source/region selection, countdown, capture failure, markup editor, crop editor, image panel, comparison, color picking, color settings popup, QR editor, and each destination/confirmation as distinct rows. Include entry points from commands and other utilities.

Screenshot editing includes every existing annotation and redaction tool, marker numbering, text sizing, zoom, selection/crop, undo/redo, copy, save, and toast feedback. Preserve toolbar wrapping and reachability in narrow windows. Crop coordinates and exported pixels must agree with the displayed selection at non-default zoom and DPI.

Image Comparison includes independent Open/Paste A/B, A/B/Wipe/Difference, failure-retained inputs, loading/status, checkerboards, zoom/pan controls found in source, and session closure. Preserve top-left unscaled alignment and exact RGBA comparison, including transparent pixels; do not substitute a visually similar perceptual diff. Retain current pixel and allocation limits.

QR includes exact source text, correction-level selection, cached preview, capacity errors, fresh invocation/reset, explicit Copy Text/Copy QR/Save PNG, and chooser cancellation. Color includes screen selection, preview/results, supported formats, cancellation and restoring the launcher, plus persisted RGBA settings. Register the plugin settings renderers in GPUI Settings rather than hiding them behind the old egui callback.

## Checkpoints

### G18-A — Establish image and capture boundaries

**Status: not started.** **Owner:** G18 implementer, shared image/capture services.

**Work:** enumerate all image surfaces and callers; record reference cases; define GPUI image admission, replacement, retirement, clipboard, and capture-restoration adapters.

**Invariants:** explicit actions alone access clipboard/files; failed replacement preserves accepted inputs; capture cannot bypass another owner's exclusion.

**Evidence:** source mappings, reference images, and existing resource/cancellation tests.

**Done:** every surface and effect has an owner, and a bounded GPUI image presentation uses the existing image model.

### G18-B — Recreate screenshot and crop workflows

**Status: not started.** **Owner:** GPUI capture/editor adapters.

**Work:** implement countdown, region selection integration, markup, redaction, crop, history, destinations, errors, and settings. Connect native dialogs through the common host.

**Invariants:** cancel creates no output; view zoom never changes export resolution; delayed responses cannot update a closed or replaced editor.

**Evidence:** representative annotated/cropped PNG comparisons, production-view interactions, native capture/focus checks.

**Done:** complete screenshot-to-editor-to-clipboard/file and standalone crop flows match the baseline.

### G18-C — Recreate comparison, color, and QR

**Status: not started.** **Owner:** GPUI utility adapters, with existing computation services authoritative.

**Work:** implement all controls, failure states, textures, source editing, color results, QR correction, settings, and explicit export actions.

**Invariants:** exact image/source semantics; bounded resources; no stale QR output after capacity failure; no changed settings serialization.

**Evidence:** small exact-pixel fixtures, capacity/failure cases, complementary wipe endpoints, and native clipboard checks.

**Done:** all three utilities and their settings/children pass parity without opening egui UI.

### G18-D — Verify integration and resource cleanup

**Status: not started.** **Owner:** G18 implementer, reviewed for image lifetime and capture ownership.

**Work:** exercise repeated open/replace/close, other-tool handoffs, DPI/window resizing, fallback egui, and memory recovery using representative images.

**Invariants:** closing sessions releases owned resources; asynchronous completion cannot revive UI; unchanged images do not re-upload each frame.

**Evidence:** scoped test log, native screenshots, bounded resource observations, and completed parity rows.

**Done:** all mandatory interactions, exports, failures, and cleanup are verified.

## Focused tests and completion

Existing targets include `screenshot_plugin` and `screenshot_cancel`. Existing exact unit-test names include `screenshot_redaction_apply_edits_crops_after_effect_and_saved_png_matches`, `screenshot_chrome_scaled_toolbar_controls_remain_reachable_without_editing_image`, `imgdiff_wipe_clips_are_complementary_and_endpoints_show_one_source`, `imgdiff_actual_file_and_paste_responses_reach_the_shared_domain`, `qr_text_clipboard_actions_are_explicit_exact_and_preserve_state_on_failure`, and `qr_capacity_failure_clears_stale_output_and_recovers_without_truncation`. Select the affected subset after listing current test targets. The existing `image_compare_smoke` binary is a possible scoped native aid, not an automatic historical acceptance campaign.

Completion requires actual pixel/export behavior, control parity, settings, and applicable Windows evidence. Record accepted commits and continue the queue; keep any missing capture or clipboard proof explicitly pending. Full nextest runs follow the shared strategy, not each image-control edit.
