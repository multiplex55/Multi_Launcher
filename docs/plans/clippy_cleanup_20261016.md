# Workspace Clippy cleanup — October 16, 2026

## Contract and baseline

- User requests all reported workspace Clippy warnings resolved idiomatically, preserving functionality, performance and public APIs wherever possible; structural warnings need deliberate ownership choices.
- Explicit exclusion: do not handle or modify any vendor crate. Vendor dependency warnings are reported separately, not silenced through configuration or source edits.
- Branch: `codex/clippy-cleanup-20261016` from `4b5f96bf7f42fedf305a341995a0ecd184392f6a`.
- Pre-existing user work: untracked root `clippy_output.txt`,731200bytes, SHA256 `345b82956b6e7f7a1533eea0f937c901d3371edff5bfc5af5230a52a2877f6e9`. Preserve; never stage/edit/delete.
- Supplied log is UTF16. Inventory records542 distinct located owned diagnostics, including170collapsible_if,97too_many_arguments and17large_enum_variant. Vendor image summary9 is excluded; no vendor primary diagnostic blocks are in the supplied log. Inventory: ignored `target/clippy-cleanup-20261016/input-inventory.json`.
- Cargo metadata: sole workspace member is `multi_launcher`; vendor paths are patched dependencies, not workspace members. Features: `unstable_grab`, `qa-framebuffer-capture`.
- Required final gates: `cargo fmt --all -- --check`; `cargo clippy --workspace --all-targets --all-features -- -D warnings`; `cargo nextest run` (full suite explicitly authorized for this task). Adjust only for demonstrated repository configuration constraints and document why.
- No pushes, history rewrites, discarded user work, blanket lint suppressions, or unrelated changes. Narrow documented exemptions only if demonstrably necessary to preserve an established API/performance contract.
- Existing Planner/Implementer/Reviewer reused; one source writer at a time. Root owns this ledger, Git checkpoints and final reporting. Earlier UI-polish original-recording blocker is separate and remains unresolved.

## Execution plan

| Checkpoint | State | Scope / evidence |
|---|---|---|
| CL1-A | complete | Eight equivalent Atomic::fetch_update→try_update substitutions in six owners; preserve orderings/callback/Result semantics; four existing targeted tests |
| CL1-B | pending | Core/native idioms; preserve pixel remainders, cfg branches, side effects and test instrumentation |
| CL2 | pending | Separate runtime/payload checkpoints for recording/native effects, persistence/event errors, radial authoring/handoff; review public API and hot-path allocation decisions |
| CL3 | pending | Separate MkMacro, radial editor and remaining launcher/dialog idioms; preserve borrow/drop timing, widget identity and event ordering |
| CL4 | pending | Acceptance/smoke idioms followed by argument ownership at existing native-click, trace-observation and suite-run boundaries |
| CL-QA | pending | Final formatting, strict all-workspace/all-target/all-feature Clippy, full Nextest, cumulative review and warning reconciliation |

### CL1-A handoff

Files: `src/performance/track_c.rs`, `src/screen_draw/recovery.rs`, `src/gui/screen_draw_toolbar.rs`, `src/gui/event_channel.rs`, `src/gui/mod.rs`, `src/thread_reaper.rs`.

Replace only the eight deprecated atomic method names. Preserve both memory orderings, callbacks, saturation/overflow behavior and returned-result handling. No new tests or payload/layout/cfg changes. Verify existing event depth/race/reaper cases, formatting and diff; commit a coherent checkpoint before the next source batch.

Record actual commands, test IDs/counts, lint residuals, structural decisions and commits below. No checkpoint is complete merely because code was written.
### CL1-A verification

- Exact eight method-name substitutions; both orderings, callback bodies, return handling and saturation/overflow behavior unchanged. Root inspected the full six-file diff.
- `cargo fmt --all -- --check` and `git diff --check`: pass.
- `cargo clippy -p multi_launcher --lib --message-format=json`: exit 0; 354 existing warnings, zero primary spans on changed lines. Diagnostic JSON: `target/ui-polish-evidence/cl1a-clippy-20261016.jsonl` (historical evidence directory used by Implementer for this first packet only).
- Targeted library Nextest union: four event-depth/race/reaper cases pass; 5367 skipped. Run ID `5748ccdf-3e21-4f40-9cfe-91a6a29e7d9a`.
- Commit subject: `refactor(core): [CL1-A] migrate deprecated atomic updates`. Hash recorded in next ledger update to avoid self-reference.