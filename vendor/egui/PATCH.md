# Vendored egui 0.36.2

Source: crates.io egui 0.36.2, archive SHA-256 dc938cc27cd911415e1e4d151bc56ff9df063d8db081374e87aeb805ea74b2ba.
Upstream: 49682f8baa058bf49e011035cfbd6e825f88a5ef, crates/egui.
License: MIT OR Apache-2.0. The license files are copied from the same upstream workspace distribution used for vendored epaint.

Local change: src/widget_text.rs shares WidgetText preparation between full galley construction and exact logical size measurement, preserving released WidgetText formatting, fallback, line-height, wrapping, and Galley pass-through behavior. Focused contract tests cover those behaviors.
