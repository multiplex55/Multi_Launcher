# G08 R1 — History toolbar diagnostic boundary

State: OPEN_NAMED_CORRECTION; source acceptance withheld. Author: independent G08 reviewer, 2026-10-30. Companion: [KICKSTART.md](KICKSTART.md); evidence: [ROLE_RECORD](../../ROLE_RECORD.md). This is one bounded correction to active G08 M1, and a complete continuation only if that correction cannot be verified within its remaining allowance.

## Outcome and source evidence

Ensure the real MkMacro History surface obeys the existing [history privacy contract](../../../planner/HISTORY_CONTRACT.md), including its shared toolbar, while retaining Editor diagnostics and existing runtime controls. Reviewed prototype `8d035538ba67f66565074a71b741bb0da2ff9d96`, base `826e418b9a812128801cbbe742410889212d87bd`, branch `codex/closeout-g08-mkmacro-run-history-20261029-r01`, independent bootstrap `02d0ac64`; these are provenance, not reset or accepted-source targets. Parent must identify the current source checkpoint before continuation.

High-confidence concrete counterexample: keep `dialog.command_error = Some("PRIVATE_DIAGNOSTIC_SENTINEL")` and select History. `mod.rs:7201` still calls the toolbar; `toolbar.rs:524–526` passes the arbitrary string into a displayed semantic status. Adjacent lines render arbitrary recording status and the current draft's unrestricted macro name from a runtime snapshot. The existing privacy test omits this toolbar, so its sentinel assertion does not cover the production composition. This is a missing presentation guard, not a defect in canonical summary storage or justification for a second history owner.

## Ownership and required correction

The existing `src/gui/mkmacro_dialog/toolbar.rs::show_with_jump_mode` owns the shared controls/status presentation; `mod.rs::show_contents_with_table_viewport` owns composition. `history.rs` owns the History body and its test fixture. Keep runtime/summary diagnostics and storage owned by their existing layers.

1. At that presentation boundary, keep arbitrary `command_error` and recording-status details in Editor only, or render fixed non-payload status labels on History. Do not clear or rewrite the underlying errors merely to hide them.
2. Do not use the unrestricted current draft name as History status metadata. A fixed safe runtime-state/count status suffices; preserve the normal Editor behavior and captured historical name from the existing bounded projection.
3. Keep Run/Stop/Pause/Resume, recording/debug behavior, control enablement, modal rules and stable Jump targets. Do not hide or disable the entire toolbar, alter summary accounting, add caches or run execution from History.
4. Add directly relevant deterministic egui coverage of the actual toolbar and History composition. Seed synthetic command/recording diagnostic sentinels and a hostile/unbounded current name through an existing or narrow test seam; prove no excluded payload on History, unchanged stored state, and retained Editor presentation. Retain assertions for bounded historical names/safe labels and relevant control behavior. Test setup must not require real recording, hooks, input injection or native APIs.

Non-goals: recipes/replay, new store, logging, runtime admission/execution redesign, new presentation framework, broad toolbar refactor, native campaign and unrelated tests. Existing fixed projection, original-generation release, terminal publication and completed-only clear remain unchanged.

## Verification and stop rule

The current packet already consumed 567.882/600 seconds of aggregate correction preparation; **32.118 seconds remain**. The reviewer authorizes no new Cargo run or renewal. Parent must hand the single writer an exact narrow selection and enforce the actual remaining preparation cap and existing 180-second built-run cap. Do not claim the previous 14/14 result verifies changed production code. Record exact selected names, command, source blobs, times/exits and cleanup; no ignored tests, weakened assertions or broad retries.

If the correction cannot be compiled and meaningfully verified in that remainder, preserve its source and evidence as unaccepted and stop source acceptance. Parent records the precise unrun/failed gate and cleanup. A future restart needs explicitly renewed, finite verification authority and a fresh source/ownership packet; this document supplies no automatic renewed budget. No app build, domain/full suite, native/manual test or dependency installation is necessary for this boundary. If any app fixture is genuinely used, disable Paste Sequence next/cancel hotkeys before constructing it.

Done requires the concrete leakage path closed at its actual owner, real-composition regression and preservation controls passing on exact corrected source, one named-issue reviewer confirmation, and parent acceptance/integration receipts. Mark `RESOLVED_IN_G08` if done in the active goal; otherwise retain this complete pair once, without a duplicate recommendation. Parent owns Git/ledgers/integration, keeps one source writer, commits coherent checkpoints and attempts one ordinary best-effort push without rewriting history. Requested reviewer Astra/max and implementer Astra/xhigh; configured pins stay unchanged and actual backend remains separately recorded.
