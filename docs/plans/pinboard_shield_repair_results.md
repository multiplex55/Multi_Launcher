# Screenshot Pin shield repair results

Branch: `pinboard-repair`. Execution plan: `multi_launcher_pinboard_shield_repair_plan.md`.

- M0-A complete: clean checkout; Windows MSVC Rust 1.99.0, Nextest 0.9.135; production owner, shared selector and owned opt-in fixture inspected.
- M1-A complete: typed strict per-monitor diagnostics shared by startup and display-change acknowledgement; exact API errors and DPI context, no input-ready bypass.
- M1-B complete (reported failure unobserved): `cargo run --features dev-smoke-binaries --bin passive_overlay_smoke -- --rectangle-shield-probe` passed startup and cleanup. One available monitor: requested/observed `(0, 0, 2560, 1440)`, visible=true, extended styles=134742152 (`0x08080088`), window DPI=96, thread/window contexts both 24592, no API error. No cursor query or injection. No evidence justifies changing global or thread DPI policy.
- Initial filtered Nextest command omitted `--lib` and spent time linking unrelated integration targets; canceled its owned process tree before test execution. `cargo nextest run --lib -E 'test(rectangle_shield_acknowledgement)'` passed 1 test (5764 skipped). The new failed-start/retry case will run in the M2 guarded-lifecycle slice. No full Nextest suite has run.
- M2-A/M2-B complete: live strict probes now decide topology replacements; failed/invalid monitor queries are terminal. No DPI, alpha, coordinate tolerance or gesture-state changes. Read-only core review found no introduced defects.
- M2 targeted command: `cargo nextest run --lib -E 'test(rectangle_shield_) or test(rectangle_guard_) or test(rectangle_startup_baseline) or test(shield_plan_preserves)'`: **17 passed**, 5749 skipped. Covers rejection/retry, monitor-query omission, held/rapid gestures, capture loss, Escape, replacement, shutdown and signed bounds. Corrected native no-input probe **passed**: deliberately shortened observed width to 2559 against required 2560; exact `RectMismatch` rejected; topology repair installed a verified replacement (2 shields retained until cleanup); all shield, visual and tooltip HWNDs were destroyed. This is the single corrected diagnostic retry, without cursor queries/injection. The original startup failure remains unobserved.
- M3–M4 pending. Final Clippy must run after all ordinary edits, tests and commits.

Checkpoint: `5161d5ce` — M1-A diagnostics/probe, pushed normally to `origin/pinboard-repair`.

Review identified silent monitor-info omission during enumeration and stale cached-bounds-only repair selection; address within the shared strict coverage contract. No DPI root cause has been established.
