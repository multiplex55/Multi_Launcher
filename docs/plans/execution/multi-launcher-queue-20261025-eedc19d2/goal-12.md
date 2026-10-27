# Goal12 execution record

Queue: multi-launcher-queue-20261025-eedc19d2
Branch: codex/screenshot-redaction-queue-20261027
BASE12_OVERRIDE=CARRY=ceff1f17fb09545d8b9a6010aae9eca7892a7ea7; FINAL11 unavailable (deferred incomplete).
M0-A:75550afb9b90a48aa9ca10470980c722decf0553, parent CARRY/same source tree; pushed.

M1-A accepted: compact validated redaction operations compose Solid/Blur/Pixelate through current raster/history. No selectable UI/preview before M1-B. Solid alpha255; blur radius1..32/default8, pixel blocks2..64/default8; region-clamped RGBA means/partial blocks. Region scratch ceiling32MiB, explicit errors through Save/Copy/QA preserve editable state. No Cargo/dependency/vendor/default-setting changes.

Verification: fast-dev lib + screenshot_plugin integration target; features dev-smoke-binaries,qa-framebuffer-capture. Checked compile PASS690.520908s (2700s ceiling), eleven exact ordinary tests PASS1.215779s/zero ignored (480s ceiling). Cleanup5s/drains verified. Compile572Absent+1Reused; original PID identity independently absent, unrelated replacement untouched. Tests all15Absent. Exact pixel/clipping/invalid/overflow/scratch/overlap/history/crop/PNG and preserved marker/direct caller evidence. No repeated builds/tests. Authorized parent-review fallback, not independent Reviewer acceptance.

External evidence: C:\Users\Jay\.codex\visualizations\2026\10\25\01a17319-6f7d-7052-b54e-1bb4a92c5fd0\multi-launcher-queue-20261025-eedc19d2\g12
See M1A_ACCEPTANCE.json, M1A_PARENT_REVIEW.md, m1a-test-events.json and measured receipts. Checkpoint SHA/push outcome recorded externally after commit.

M1-B pending controls and faithful cached preview. M2-A pending output audit/help and mandatory Windows workflow. End-only Clippy not yet appropriate. No full-Goal acceptance/CARRY advancement.

## M1-B accepted

M1-A checkpoint c21f01eff5a3d12a9fb6249fa19608b6ffd5babc pushed.
M1-B adds actual Redact controls, captured-per-operation region gestures, and a revision-keyed shared-raster preview. One source RGBA image/texture handle; no permanently retained second CPU ColorImage. History mutations invalidate pixel output; zoom/crop/control-only frames reuse it. Redaction history flattens all committed layers once and suppresses duplicate vector painting; final-redaction undo restores original+ordinary overlays. Error revision drops stale texture and caches failure without idle retry. Closing clears gesture/texture/cache. Ctrl+Z is distinct from Ctrl+Shift+Z redo. New fill swatches use contrasting text.

Scoped verification: nine distinct ordinary screenshot cases passed cumulatively, exact inventories/per-case events retained. Prepare1 failed on two fixture compiler errors (158.255924s), fixed narrowly; prepare2 passed291.449114s. Run1 sevenPASS/twoFAIL7.057556s due missing real egui ModifiersChanged events; fixture-only correction; prepare3 passed143.793977s, only failed two rerunPASS9.568417s. Original large-image-drag criterion required extending the same 4K fixture; prepare4 library-test-only PASS105.12661s, only extended4Kcase PASS10.265778s. Other eight passing results retained; no production change after prepare2. All measured cleanup/output drains verified; all post-cleanup descriptors Absent for these runs.

The 3840x2160 fixture proves actual observed-canvas drag press/motion causes zero renders/uploads, release one of each and one compact operation; idle/view-only frames reuse texture; undo/redo updates correct revisions, final redaction undo restores ordinary overlay, close emits texture-free delta. Payload telemetry33,177,600 RGBA bytes is not process RAM/driver VRAM. No native evidence is claimed. Original test budget480s/cleanup5s; one focused compile correction and one focused test correction used, no budget reset. Authorized parent review fallback, not independent Reviewer acceptance.

Evidence: external g12/M1B_ACCEPTANCE.json, M1B_PARENT_REVIEW.md, M1B_IMPLEMENTER_NOTES.md, measured prepare/inventory/run receipts and per-case events. Commit/push identities recorded externally after this checkpoint. M2-A output/help/Windows native and end-only Clippy remain pending; CARRY unchanged.
