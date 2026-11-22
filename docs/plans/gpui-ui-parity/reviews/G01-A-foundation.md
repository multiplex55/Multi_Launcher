# G01-A foundation review

The configured reviewer inspected `Cargo.toml`, `Cargo.lock`, `src/main.rs` and `src/gpui_frontend.rs` against the source-based startup handoff. The production path keeps the helper first, parses the selector before data access, and holds the canonical data-root lock across the selected runtime. The selected GPUI branch enters the Kit event loop on the process main thread before the egui worker and app-only service initialization.

Kit 0.10.0 resolved its GPUI family to 0.3.12. Shared dependency resolution includes `futures` 0.3.34, `getrandom` 0.3.4 and `serde_json` 1.0.154. The selected GPUI manifest constrains those dependencies above some baseline versions. Source and dev link evidence support G01-A.

The reviewer flagged ambiguous lifetime elision in the argument fixture. The implementer used owned test arguments and the focused binary tests passed. No production code correction was required by this review.

The open-window error path gives explicit egui recovery guidance. The pinned Windows backend can fail earlier during platform or DirectX initialization. G01-D must exercise recovery after a controlled GPUI startup failure. G01-B still needs editable input. G01-C owns hotkey, native focus, hidden-root utility and overlay evidence. See [validation receipts](../VALIDATION_LOG.md).
