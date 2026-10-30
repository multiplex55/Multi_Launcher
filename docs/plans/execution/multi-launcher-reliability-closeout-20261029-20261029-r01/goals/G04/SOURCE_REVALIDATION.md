# G04 source revalidation baseline

Run20261029-r01; branch codex/closeout-g04-screenshot-native-redaction-20261029-r01 from bootstrap02d0ac64d51655f43d6e958ea551f5bf658ee48d. Fetched master/origin/master4a1283e6edd2c245f562a07dbe3dcc6a61061eed unchanged. Clean sole worktree. Authoritative integrationdf94f192290ff14bdd6cc083f8aa0a62644f09ae: G00/G01 accepted, G02 documentation-only deferral, G03 dependency deferral; sourcefd300ff7. G04 independent of Table.

Read complete G04 Goal/Kickstart. Planner source inventory is now persisted in [ROLE_RECORD](roles/planner/ROLE_RECORD.md) against baseline `06d23fc8a520d91f71d02cd1cc2fb95cc08500b4`; implementer read that complete packet and the future protocol. No old excluded screenshot candidate imported. Sibling global ledger is bootstrap snapshot; integration is authoritative.

Planner-observed current ownership, carried as **SOURCE_OBSERVED**, not runtime proof:

- `src/gui/screenshot_editor.rs::ScreenshotEditor::ui` distinguishes primary Redact from secondary crop; primary operations capture effect/transform/history. Marker selection exposes an explicit secondary-press crop origin used by the future protocol.
- `MarkupHistory` Undo/Redo owns markup; crop is separate. The future sequence expects crop to remain when redaction is undone/redone.
- `apply_edits` renders layers then crops. `save_image` and `copy_to_clipboard` share that composed image path. Copy directly writes the OS clipboard through arboard; no controlled test backend was observed. The `screenshot_save_file` setting can add a save during Copy and must be disabled in the isolated future profile.
- `src/annotation/redaction.rs::apply_redaction` Solid replaces selected RGB with alpha 255 over clipped floor/ceil half-open bounds. Blur/Pixelate are visual effects, not guaranteed secret erasure.
- Immediate capture delivery in `src/plugins/screenshot.rs` and editor construction in `src/gui/mod.rs` were inspected by planner; no broader capture audit or owner migration was needed.

No source defect is established. Implementer did not duplicate the bounded planner inspection or exercise native actions. Current source, Cargo/unit tests, native gestures, saved images and clipboard pixels have not been runtime-verified in G04. Disposition **NATIVE_PROOF_DEFERRED**; [NATIVE_CAPABILITY](NATIVE_CAPABILITY.md) remains the parent's original capability evidence.
