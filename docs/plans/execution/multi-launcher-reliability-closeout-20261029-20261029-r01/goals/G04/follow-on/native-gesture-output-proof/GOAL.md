# G04 follow-on — Native gesture and output proof

Status NOT_EXECUTED / NATIVE_PROOF_DEFERRED. Discovery G04 Planner; source observed at06d23fc8a520d91f71d02cd1cc2fb95cc08500b4. Confidence high that documented interface lacks secondary-button drag; no application defect/Windows limitation inferred. Parent persists planner-authored protocol. Companion [KICKSTART](KICKSTART.md).

## Objective and evidence

Prove actual capture -> primary Solid redaction -> secondary crop -> Undo/Redo -> Save/Copy against identified current-source binary. Compare actual saved/clipboard pixels with independent expected image. Continues original G04 contract; no historical excluded candidate/capture architecture import.

DragInput exposes from_x/from_y, optional screenshotId, to_x/to_y/window, no button selector. Right ClickInput and accessibility perform_secondary_action do not prove held secondary drag. Parent initialized documented runtime/listed apps; installed Multi Launcher not running/no windows; binary identity unestablished. No launch/gesture/capture/clipboard/output attempted. Source has primary/secondary/shared composition/relevant unit tests, supporting protocol but not native acceptance.

## Owners, scope and exclusions

src/gui/screenshot_editor.rs: ScreenshotEditor::ui, MarkupHistory, render_markup_layers, apply_edits, save_image, copy_to_clipboard. src/annotation/redaction.rs::apply_redaction only for reproduced compositor defect. Immediate capture/delivery unchanged unless direct defect separately bounded. Own docs/fixtures/supported native automation. No driver/raw SendInput/global hook/backend/renderer/clipboard framework or historical patch. No compatibility migration required.

## Prerequisites

1. Revalidate repository instructions/accepted branch/base/current owners.
2. Read current computer-use skill/API; require supported physical secondary down/move/up or button-selectable drag. Missing capability stops before build/launch; no invented fields/helpers.
3. Identify current-source executable/build provenance; installed app name insufficient.
4. Isolated task-owned profile/output and supported clipboard-image observation/restoration. Current Copy writes arboard OS clipboard directly; no injectable backend observed.
5. Visible owned synthetic capture and calibrated coordinates; capture no unrelated content.
6. Disable Paste Sequences next_hotkey/cancel_hotkey before constructing test app/profile; preserve production defaults.

## Fixed fixture and independent oracle

Opaque320x240 PNG; coordinates source-image pixels, rectangles half-open. Base B(x,y)=[32+x%192,32+y%192,64+(x+y)%128,255]. Four8x8 fiducials:
- [20,28)x[20,28):[220,40,40,255]
- [288,296)x[20,28):[40,220,40,255]
- [20,28)x[208,216):[40,40,220,255]
- [288,296)x[208,216):[220,220,40,255]

Draw synthetic FAKE1234 at(80,88), fixed5x7 bitmap font,2x2 glyph cells,12pixel advance. Occupied94x14 box [80,174)x[88,102). Record glyph bitmap, PNG and decoded RGBA hashes. RedactionR=[64,256)x[72,136), opaque black; cropC=[16,304)x[16,224). Output288x208. Independently compute final pixel(u,v) black[0,0,0,255] if48<=u<240 and56<=v<120, otherwise final fixture(u+16,v+16), including fiducials/text. Text maps[64,158)x[72,86), wholly black. Do not use apply_edits/render_markup_layers for oracle. Compare every decoded pixel. Record encoded PNG hash separately; Save/Copy equality uses decoded dimensions/RGBA, not incompatible serialization hashes.

## Exact native sequence

1. Isolated settings screenshot_use_editor=true, screenshot_auto_save=false, screenshot_save_file=false, owned screenshot_dir. Preserve clipboard restorably without logging contents. Copy can otherwise additionally save; avoid extra effect.
2. Display owned fixture verified1:1 physical pixels. Actual supported capture precisely320x240 into existing Screenshot Editor; no editor image/crop state injection.
3. Save owned baseline.png; decode exact fixture match before editing, otherwise stop capture/calibration failure.
4. Zoom1.0; establish actual canvas origin/display scale; map source coordinates to native input. Do not assume physical desktop pixels=egui points.
5. Select Redact/Solid/Black fill, physical primary drag(64,72)->(256,136), release; observe opaque rectangle.
6. Select Marker without canvas click. Physical secondary drag(16,16)->(304,224), release. Uses explicit secondary-press crop origin; must add no marker. Proof is this tool branch only.
7. Undo once: redaction disappears/crop remains. Redo once: redaction returns/crop remains. No other markup.
8. Save owned final.png; decode/compare complete288x208 oracle.
9. Copy; observe actual clipboard image using supported controlled mechanism; require288x208/all RGBA equals oracle AND saved output. Toast/internal applied-image dump insufficient.
10. Close owned resources only; restore clipboard/profile through supported mechanism, record cleanup. Retain intended owned evidence, no external uploads.

## Bounded stages and verification

F0 capability/binary/isolation gates; F1 one owned native sequence/output comparison; F2 only reproduced owner-specific bounded correction (otherwise defer); F3 independent review/true disposition.

Current multi_launcher library tests available:
- gui::screenshot_editor::tests::screenshot_redaction_drag_captures_effects_and_history_shortcuts
- gui::screenshot_editor::tests::screenshot_redaction_apply_edits_crops_after_effect_and_saved_png_matches
- gui::screenshot_editor::tests::screenshot_marker_canvas_click_is_single_shot_zoomed_and_matches_preview_export
- annotation::redaction::tests::solid_uses_opaque_black_and_clipped_half_open_bounds

These use raster/synthetic egui, not physical native input/clipboard. Run relevant tests only for correction or concrete revalidation need. Existing fast-dev/package/library/nonzero Nextest filters. Active verification policy controls one selected prep/run and justified bounded correction reruns, no broad suite/clean build. Parent Git/ledger, one writer, explicit requested/configured/unverified models no config changes; coherent checkpoints/one normal push each.

## Acceptance, stop and rollback

ACCEPTED_COMPLETE needs physical primary/secondary gestures, observed history, exact real Save/Copy pixels, cleanup, independent review. Solid replaces covered output pixels; Blur/Pixelate visual effects only. Missing input/pixel/isolation/binary/cleanup proof means NATIVE_PROOF_DEFERRED, not PASS. Preserve unrun steps/evidence; no internal setters/synthetic events/QA applied dump substitute. Proof-only plan needs no production rollback; justified later correction separate reversible checkpoint. Clean only verified owned resources; preserve user edits/history.
Canonicalized by parent after independent documentation-only review of f9e0fc3e1c9823e9b50f2e8184aecefd7590721e. Original planner-authored package preserved under roles/planner/follow-on. P1 acceptance gap; PROPOSED_NOT_EXECUTED, no automatic dispatch/budget renewal.
