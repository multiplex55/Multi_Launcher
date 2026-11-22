# QR Generator

G18 owns `qr-generator`. `qr` emits the typed GUI route `qr:open`. The payload route consumes one separator and retains the remaining exact Unicode/whitespace text. The shared QR engine performs deterministic local encoding and rasterization. `QrDialogState` owns the current editor, metadata, correction, cached output and exact accepted effect.

| Criterion | Source-backed behavior |
| --- | --- |
| QR-01 | Open a fresh blank or exact supplied draft. Show character and UTF-8 byte counts, multiline editing/undo, local preview and Low/Medium/Quartile/High correction. Empty or over-capacity input retires the old raster and keeps source available for editing. |
| QR-02 | Explicit Paste/Copy Text, Copy QR and Save PNG use exact accepted source/raster and current owner/session. Failed cut restores source and undo. Report errors without disclosing payload. Save honors the chosen PNG path, including case-insensitive extension, and canceled chooser has no file effect. |
| QR-03 | Preserve remembered host and focus with exact retained owner/epoch. Fresh invocation resets correction to Medium and clears prior source/output. Generic action labels, feedback and navigation do not record payload in history/usage. |

Select `literal_qr_payload_survives_exact_sync_and_snapshot_search`, `qr_bus_opens_once_with_exact_payload_and_skips_history`, `qr_searched_action_opens_exact_payload_without_history_and_reopens_fresh`, `qr_jump_replay_retains_targets_without_effects`, `qr_capacity_failure_clears_stale_output_and_recovers_without_truncation`, `failed_cut_preserves_source_and_reports_clipboard_error`, `qr_png_save_roundtrips_cached_raster_without_changing_confirmed_path` and `qr_png_cancel_invalid_paths_and_write_errors_have_no_unrequested_effects`. `G18-EGUI-QR-PNG` verifies native exact payload counts, correction UI, PNG output and fresh reopen. Clipboard, invalid/large input, host and GPUI remain pending.
