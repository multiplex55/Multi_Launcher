# M1-D source verification

Base: local M1-C df9bdd9172a44c63ec7a9fd6638e55e70dedb264. Windows MSVC stable Rust/Cargo1.99.0, egui/eframe0.36.2 with explicit Glow. Source acceptance and native acceptance are distinct; native/screenshot smoke remains pending.

- `cargo check --all-targets --locked --offline`: PASS1m24s; check-all-targets-final-vendor-toast-retry2.log.
- Focused library Nextest: PASS23/23,5206 skipped,0.742s; nextest-m1d-final-focused-parity.log. Covers actual fragment/scroll/viewer identity, shared cache/link/checkbox semantics, static JPEG/GIF/WebP/PNG loading, DDS file/MIME/malformed admission, QR reuse/PNG, MKMacro decoding and crop geometry, toast anchor/stack/hover/expiry.
- Dedicated toast reproduction: PASS1/1,0.174s; nextest-m1d-toast-vendor-fix.log. This test is also in the23-case set, not another unique case.
- toast_log integration: PASS2/2,0.272s; nextest-m1d-toast-log-integration.log.
- file_drop integration: PASS1/1,0.168s; nextest-m1d-file-drop-integration.log.
- image package DDS on/off: reading_enabled/writing_enabled PASS2 in each configuration, plus dds_decode_capability_is_feature_independent PASS1 in each. Four image-capability-dds-{enabled,disabled}-{focused,contract}.log receipts.
- Root formatting, vendored toast rustfmt and diff whitespace checks: PASS.
- image-025-patched-feature-tree.log and egui-toast-patched-tree-final.log confirm actual local path packages. Application image0.24 remains registry. No AVIF/animated extras loader enabled.

All raw receipts are in ignored target/egui-modernization/m1d. Earlier compiler, interrupted-build and failed fixture/runtime receipts remain there. Initial toast fixtures accidentally included constructor errors; the final fixture disables them during construction, uses explicit timestamps/initial sizing, observes actual controlled messages, and has no diagnostic trace. The valid DDS decoder was retained; public reporting is corrected at image ownership. Toast hover considers both Area and body responses while preserving geometry/timing/close callbacks.

Independent final review accepted fragment, image and toast source, provenance and meaningful tests. See vendor/image/PATCH.md and vendor/egui-toast/PATCH.md for the narrow patches and removal conditions. Normal and benchmark builds still contain no screenshot probe.

## Reproduction

Prepend the direct stable toolchain bin to PATH and set RUSTC to its rustc.exe. Rustup shims are unavailable on this host.

```powershell
cargo check --all-targets --locked --offline
cargo fmt --all -- --check
cargo nextest run --locked --offline --lib --no-fail-fast -E 'test(/(gui::note_panel::tests::classify_markdown_link_references_and_external_urls|gui::mkmacro_dialog::image_preview::tests::explicit_invalidation_forces_fresh_decode_when_metadata_is_unchanged|gui::mkmacro_dialog::image_preview::tests::overwrite_of_same_filename_redecodes_and_corruption_is_not_cached_as_old_pixels|gui::note_panel::tests::extras_image_loader_admits_dds_extension_and_mime_before_decoding|gui::note_panel::tests::extras_image_loader_decodes_static_jpeg_gif_webp_and_png_asynchronously|gui::note_panel::tests::markdown_analysis_is_reused_until_content_changes|gui::note_panel::tests::preprocess_preview_markdown_uses_same_link_and_todo_mapping|gui::note_panel::tests::preview_preprocess_keeps_current_note_wiki_link_literal|gui::note_panel::tests::preview_preprocess_preserves_markdown_blocks_and_raw_html|gui::note_panel::tests::programmatic_content_replacement_invalidates_cached_markdown_analysis|gui::note_panel::tests::rendered_checkbox_toggle_marks_content_changed_and_invalidates_analysis|gui::note_panel::tests::disabled_interactive_checkboxes_do_not_mutate_during_preview_render|gui::note_panel::tests::handle_markdown_links_promotes_www|gui::qr_dialog::tests::qr_editor_refreshes_exact_source_and_reuses_unchanged_generation_and_texture|gui::note_panel::tests::disabled_task_lists_use_normal_markdown_without_cached_task_analysis|gui::note_panel::tests::horizontal_markdown_fragments_shrink_vertically_with_long_links|image_crop::tests::crop_has_exact_dimensions_and_preserves_source_pixels|screen_draw::geometry::tests::crop_plan_clips_and_translates_a_negative_origin_selection|gui::note_panel::tests::markdown_fragments_keep_distinct_stable_ids_and_prepare_shared_cache|gui::qr_dialog::tests::qr_png_save_roundtrips_cached_raster_without_changing_confirmed_path|gui::render::tests::root_toasts_preserve_anchor_stack_hover_pause_and_configured_duration|gui::note_panel::tests::split_preview_long_unbroken_content_does_not_expand_dialog|gui::mkmacro_dialog::image_preview::tests::decode_allocates_only_thumbnail_rgba)/)'
cargo nextest run --locked --offline --test toast_log --no-fail-fast
cargo nextest run --locked --offline --test file_drop --no-fail-fast -E 'test(dropping_file_opens_add_dialog)'
```

## Executed image capability commands

These ran with an already resolved standalone vendor/image/Cargo.lock (ignored). A fresh checkout must first resolve that package's test lock/cache; initial online resolution receipts are retained. The application root lock remains tracked and unchanged by standalone test resolution.

```powershell
cargo test --manifest-path vendor/image/Cargo.toml --no-default-features --features dds --lib enabled --locked --offline
cargo test --manifest-path vendor/image/Cargo.toml --no-default-features --lib enabled --locked --offline
cargo test --manifest-path vendor/image/Cargo.toml --no-default-features --features dds --lib dds_decode_capability_is_feature_independent --locked --offline
cargo test --manifest-path vendor/image/Cargo.toml --no-default-features --lib dds_decode_capability_is_feature_independent --locked --offline
```
