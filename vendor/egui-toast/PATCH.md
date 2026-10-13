# Local egui-toast patch

This directory vendors the crates.io `egui-toast` 0.22.0 package. The original
archive checksum is
`b3df032159d3428ab76bdb8c580b2f807cc10976cc09e815abdce74d640f7c0f`, and
`.cargo_vcs_info.json` identifies upstream commit
`4e2f34103adc02e547e50e15df21482a4901aa91`. These identify the unmodified
archive and revision.

The tracked copy excludes Cargo's registry cache marker and the upstream
prebuilt browser demo under `docs/`; neither is part of the library build.

`src/lib.rs` preserves toast hover pause when egui's close interaction is
hovered: decrement the lifetime only if neither the toast Area nor the toast
contents response is hovered. In egui 0.36 the full-rect close interaction is
the top interactive widget, so the Area response alone does not report hover
over the visible toast body.

The patch changes no layout, stacking, lifetime source, close behavior, or
rendering. Remove it after an upstream egui-toast release uses both hover
responses when applying toast lifetime updates.
