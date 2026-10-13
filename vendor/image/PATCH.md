# Local image crate patch

This directory vendors the crates.io `image` 0.25.6 package. The unmodified
archive checksum is
`db35664ce6b9810857a38a906215e75a9c879f0696556a39f59c62829710251a`, and
`.cargo_vcs_info.json` identifies upstream commit
`f337e27aadaae8b86484429bc6020fef8a019c95`. These values identify the original
archive and revision, not the locally patched tree.

`src/image.rs` fixes the DDS capability declarations to match the existing
decoder: `can_read` is true, and `reading_enabled` reflects the `dds` feature.
The crate does not provide a DDS encoder, so `can_write` and `writing_enabled`
remain false. The decoder and its feature wiring are unchanged. These methods
are used by `egui_extras` to admit image URIs and MIME types before passing
bytes to that decoder.

The patch applies only to image 0.25.x; the application's separate image 0.24
dependency remains the crates.io package. Remove this vendor override once an
upstream image release reports DDS decoder capability accurately and the
locked `egui_extras` image loader admits enabled DDS extensions and MIME types.
