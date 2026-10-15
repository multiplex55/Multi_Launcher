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

`Toasts::show_in_rect` adds an opt-in bounded display path for ROOT's startup
warning. It reuses the existing stack, Area IDs, content callback, progress,
hover, lifetime, and close handling while constraining the Areas and wrapping
default text to the available width. The existing `Toasts::show` path remains
unconstrained and source-compatible. Remove this addition once upstream offers
an equivalent bounded API that preserves these interaction owners.

Classification: UI correctness/egui compatibility, not benchmark-parity work.
Application coverage is `root_toasts_preserve_anchor_stack_hover_pause_and_configured_duration`
and `root_toast_long_startup_notice_fits_below_header_and_dismisses`; they retain
stack/hover/TTL behavior and check complete default text below the actual header.
