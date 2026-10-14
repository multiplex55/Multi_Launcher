# M3-C transactional live preview and presets

Parent runtime checkpoint `1db24752`; local-only, migration-only source remains `af9d1183`.

Theme dialog owns a persisted baseline/draft/error/conflict session; runtime owns one preview overlay and the latest committed theme beneath it. Apply/Save atomically compare persisted theme before replacement, preserve unrelated settings, adopt only successful commits and retain failed/conflicted drafts. Apply advances baseline; Save/Cancel/X/Escape/explicit/forced close end through the shared boundary after extracted render state is restored. Pin refusal/repeated focus retain session. Cancel restores latest committed preferences, including System following current OS appearance. Failed load disables stale saves and supports explicit retry.

Saved/curated palettes are deterministic and collision-safe; manual custom edits copy the effective selected scheme and clear active preset. Separate accent leaves saved hyperlink/selection untouched. Native unmultiplied RGBA editing is guarded by response.changed, preserving original translucent channels without edits. Resolved preview displays actual semantic theme.

Targeted Windows locked/offline fast-dev Nextest **31/31 passed**, native0, preview-00367.7362s including compilation; all source bindings unchanged. Includes session/conflict/failed-load/save/close/pin/System/preset/RGBA and ROOT/Notes cache contracts plus existing theme/domain tests. Scoped rustfmt/diff pass; independent coherent review found no remaining substantive issue.

Raw input/start/result/logs: `target/egui-modernization/m3c/preview-003/nextest/`; runner preview-check-003.py. Earlier preview-001 compile101 (two E0502 borrow boundaries) and preview-00229/31 retained. Corrected actual boundary borrowing, reviewed missing-active-curated insertion change detection, and two new fixtures: real I/O failure directory instead of missing-file/default conflict, application color_row guard instead of raw-widget quantization assertion. No valid behavior assertion weakened or adverse sample removed.

No benchmark/native capture/input claim. Existing performance debt and exact native/manual qualification gaps remain documented. Proceed immediately to typography/icons and full surface rollout.
