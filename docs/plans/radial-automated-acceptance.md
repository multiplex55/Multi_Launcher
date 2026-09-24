# Radial automated acceptance execution ledger

Approved specification: [automated acceptance plan](../multi_launcher_radial_automated_acceptance_codex_plan.md).

## Start state

- `AUTOMATED_ACCEPTANCE_START_HEAD`: `55120289c9ef9ec5c026d6348a0050388ff3e1fa`
- Initial branch: `radial-menu-2` (tracking `origin/radial-menu-2`).
- Initial working tree: clean; no staged, unstaged, or untracked files.
- Input-recovery ledger: Packages A/B/C implemented; final historical Nextest recorded 4,635 passed and eight skipped; native focused ROOT and Designer interaction still pending. Later diagnostic traces and candidate corrections are historical evidence, not a current native acceptance pass.
- Immutable radial feature baseline: `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` as recorded in the stabilization ledger.

## Milestones

| Milestone | State | Evidence |
| --- | --- | --- |
| A. Deterministic input and headless Designer acceptance; native runner foundation | complete | Exact fake-time chord regression and retained `viewport_ui` driver passed the focused two-test gate. The opt-in runner currently reports preflight only; native acceptance is pending B. |
| B. Isolated native runner and failure evidence | complete | Source-matched Win32 run produced case reports, checked per-edge input evidence, trace/window artifacts and screenshots; the failed cases now identify hook admission and Designer semantic discovery rather than relying on manual interaction. |
| C. Focused ROOT and Designer root-cause remediation | complete | Native immediate and quiescent runs each passed all 13 current cases, including H2/H3/H6, Designer interaction, and cleanup. H6 observes both radial HWNDs and rejects any newly active radial-class HWND. |
| D. Basic authoring and copied-profile acceptance | complete | D1 geometry and D2 authoring/lifecycle passed. D3 source-matched deterministic 31/31 and supplied copied-profile 28/28 native cases passed, with source integrity, cleanup, and independent review. |
| Final verification and independent review | pending | |

## Milestone A verification

- `cargo check --lib --bin multi_launcher --bin radial_acceptance`: passed.
- Focused selector preflight: selected exactly the two new tests.
- `cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/^(hotkey::launcher_invocation::tests::exact_shift_alt_win_end_chord_uses_fake_time_for_tap_and_hold|gui::radial_editor::tests::retained_headless_viewport_accepts_semantic_click_after_snapshot_and_tabs)$/)'`: 2 passed, 4,647 skipped, exit 0. Durable record: `target/radial-acceptance-a-focused-final7.{meta,log,exit}`.
- `cargo fmt --all -- --check` and `git diff --check`: passed.
- Earlier focused attempts exposed test assumptions about AccessKit disabled/focus metadata; the final test instead verifies pending click inactivity, authoritative reply acceptance, semantic New Menu mutation, pointer-acquired TextInput focus, and Tab focus movement through production `viewport_ui`.
- Native Windows input and HWND acceptance: not run in this milestone.

## Milestone B verification

- `cargo fmt --all`: passed.
- `cargo check --lib --bin multi_launcher --bin radial_acceptance`: passed (eight driver dead-code warnings remain for later cases).
- `cargo build --bin multi_launcher --bin radial_acceptance`: passed.
- Source-matched native runner: produced `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-b-final2-a1b12086d47d4569a9878daebfa72e18\report.json` against revision `6229ba7a`. H0 failed at hook admission despite checked F11 down/up on the child ROOT HWND and input desktop. D0 and D5 passed. D1/D2 could not discover the semantic Tree control and D4 could not discover the Skins command. The run exited nonzero, as expected for the unresolved production cases. Child/profile/cursor/input-desktop cleanup passed; previous foreground restoration was attempted but Windows reported no foreground HWND. These failures are remediation input for C/D, not an acceptance pass.

## Milestone C progress

- Production fixes now keep the deferred Designer in ROOT's configured z-order band, export opt-in semantic bounds from real egui controls, wake the child viewport after a Skins command, and let a clean close remove its HWND. The native runner waits for production pointer movement before pressing, verifies semantic selection and Tab focus, and records hook/owner/deadline evidence.
- `cargo fmt --all -- --check`, `git diff --check`, `cargo check --lib --bin multi_launcher --bin radial_acceptance`, `cargo build --bin multi_launcher --bin radial_acceptance`, and focused Nextest batches passed; the final focused batch passed 2/2.
- Final source-matched checkpoint report: `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-c-review-final-20260923-217cc222170f424e9edf7302c4eb7ae6\report.json`. H0-H5, D0-D2, D4/D5, and CLEANUP passed; H6 failed at hook admission and H3 is downstream. D2 now includes native Unicode editing of a real Designer TextEdit, model/value agreement, clean restoration, and Tab focus. The deferred HWND exposes no UIA Edit at the target point, so the report explicitly identifies its production semantic-trace fallback. All durable UIA snapshots redact free-form Name/Class/Framework/AutomationId fields. H6's checked second-hold key-up remains unseen by both independent and production low-level hooks despite live hook threads and ROOT receiving later F24 window input. C remains in progress; this package is not native acceptance sign-off.

## Milestone C completion

- Root cause: transient mouse-gesture suppression uninstalled and reinstalled its low-level keyboard hook while the radial was open. The native backend now keeps the installed hook inert during suppression and resumes it in place; backends without that capability retain the prior uninstall path. An opt-in trace records ROOT menu state changes without consuming the bounded trace budget per frame.
- `cargo fmt --all -- --check`, `git diff --check`, `cargo check --lib --bin multi_launcher --bin radial_acceptance`, and `cargo build --lib --bin multi_launcher --bin radial_acceptance`: passed.
- Focused Nextest gate: 5/5 passed, covering the retained-hook lifecycle, legacy fallback, menu trace deduplication, and both radial HWND transition oracles.
- Source-matched native immediate report: `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-c-hook-final2-immediate-20260923-01\report.json`; all 13 cases passed, H5-to-H6 interval 9 ms, no trace-budget exhaustion.
- Source-matched native quiescent report: `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-c-hook-final2-quiescent-20260923-01\report.json`; all 13 cases passed with a checked F24 between holds, no trace-budget exhaustion.
- Both reports identify candidate SHA256 `e74be2fced49cb2e46dd4cf77d82ea6587d6578d1c8b4587b0962195b419790a` and runner SHA256 `4ae464e84ebe792a22f3a8e2b0f1b0825de7531d547094304621d592c62808de` against base commit `140474fa93f9a2b6d05a8dffad113b0622282875`.

## Milestone D1 completion: native geometry authoring

- The runner uses session-scoped, typed production Designer controls and geometry trace, then performs process/client-validated native input. A0 New Menu, A1 ready nonmutating Add Ring proposal, G0 explicit Apply, A2 Slots growth with committed stable cell IDs, G1 populated shrink Cancel and Move to overflow, and G2 compact controls/canvas bounds all passed. The native report also retained the H/D cases and verified production Discard plus terminal Designer HWND close.
- Review found and resolved three oracle gaps: G2 now requires fresh controls rendered at the checked compact client size; G0/A2/G1 compare privacy-safe committed cell-ID fingerprints after Apply; and blocked case reporting fills missing IDs including G0. The compact Designer pane now reserves actual trailing egui item spacing. The retained headless AccessKit driver enables accessibility before its initial frame.
- `cargo test --bin radial_acceptance`: 22/22 passed. `cargo test --lib gui::radial_editor::tests::`: 31/31 passed. `cargo fmt --all -- --check`, `git diff --check`, and `cargo build --bin multi_launcher --bin radial_acceptance`: passed. The complete Nextest gate remains for final verification.
- Source-matched native report: `C:\Users\Jay\AppData\Local\Temp\multi-launcher-radial-acceptance-d1-review-20260923-08\report.json`, 19/19 cases passed with normal child close, temporary profile removal, foreground and cursor restoration, and input desktop release. Bounded 4096-event trace did not exhaust. Candidate SHA256 `5c9568d56e035888ee62611003f818de752ee1c2c381ff3d6f316b61104af638`; runner SHA256 `d67e855ce2a2ac491fe751b7f192fc34bc7740d02294beb231ea8145e5d02b6a`; report SHA256 `04e4cf554ace4d99f1d32063473e8821ec0dec65803ad6230eac51e2d8eb9a5d`.
- Independent read-only review closed all three D1 findings and identified no remaining actionable issue in this milestone.

## Milestone D2 completion: native authoring and lifecycle

- The native runner now covers action search beyond the first 50 entries, exact action binding and after-action policy, style preview and persistence, Save/reopen, Undo/Redo, no leaf dispatch, ROOT parking during Designer activity, dirty Keep Editing/discard, and close while a real disposable preview request is pending. A bounded acceptance-only service gate makes the pending-close race deterministic, then verifies cancellation, rejected late reply, and a full one-second no-reopen interval.
- Production fixes include filtering the action catalog before its display limit, rearming an early native hotkey timer to the exact remaining deadline, and typed bounded acceptance traces for Designer state and preview lifecycle. The report preserves compact decisive evidence within its privacy and size limits. The documented optional source revision is derived for both report output forms.
- `cargo fmt --all -- --check`, `cargo nextest run -p multi_launcher --bin radial_acceptance` (47/47), `cargo check --bin radial_acceptance --bin multi_launcher --lib`, `cargo build --bin radial_acceptance`, and `git diff --check`: passed. Full Nextest remains for final verification.
- Source-matched elevated native report: `C:\Users\Jay\AppData\Local\Temp\radial-acceptance-d2-run36\report.json`, 31/31 distinct cases passed, no trace/report capacity saturation, and all cleanup conditions true. Copied-profile status is explicitly `not_run` pending D3. Source revision `fe07b197d5d10e4b3e803abb3d951d72cd7bfe85+dirty`; candidate SHA256 `a23dcded33cf655e8fc83b72a3c1814175cbd452af4f4e3e1cf0288610003c4d`; runner SHA256 `dc1beafc45e6c8c7209907093a397ff7bf55568d5e5786ff88286f9e723594d6`; report SHA256 `f617af9ba225e7abff7c2f171dae790a0c7608399ecc7c41e7ba042ea698c677`.
- Independent read-only review identified and verified fixes for stale or impossible D6/D7 oracles, A5 preview evidence, the optional CLI revision, policy-choice duplication, and truncated passed-case reports. Its final re-review found no substantive D2 issue.

## Milestone D3 completion: copied-profile acceptance

- `radial_acceptance --profile-copy` now runs mandatory deterministic acceptance first, then a separate native pass against an isolated copy of the supplied test profile. A bounded regular-file inventory rejects reparse points, collisions, excessive trees, and output overlap; Windows no-follow handles and source/copy hashes protect the source boundary. Typed settings/radial validation and copy-only normalization confine paths and disable global item inputs while preserving completed migration receipts and the built-in radial command.
- The copied pass uses checked native input for focused ROOT, Designer pointer/Tab and Menus/Skins interactions, derived-menu authoring, style preview, Save/reopen, Undo/Redo, dirty and pending close, and no leaf dispatch. Its report is separate and privacy-safe. Failed runs retain bounded diagnostics in an owner-restricted private directory after the temporary copy is removed; successful runs validate then delete staged private artifacts. Aggregate PASS is published only after both reports are written.
- `cargo fmt --all -- --check`, `cargo test --bin radial_acceptance` (75/75), `cargo check --lib --bin multi_launcher --bin radial_acceptance`, `cargo build --bin multi_launcher --bin radial_acceptance`, and `git diff --check`: passed. The full Nextest gate remains for final verification.
- Final source-matched native invocation exited 0. Aggregate/deterministic report `target/radial-acceptance-d3-final-0924-5/report.json` passed 31/31 cases; copied report `target/radial-acceptance-d3-final-0924-5/report.copied-profile.json` passed 28/28, including both R0 cases and cleanup. The supplied source tree and exact initial copy shared SHA256 `859ccce406467bb72160ab5b4383d85495ef823c4bbc9e0095719384cf685786`; the source postrun hash matched. The temporary copy and child windows/process were removed, cursor/foreground restored, and input desktop released. Candidate SHA256 `3a8a56f48991b166a988bd05dddb4b6f3a7c00f6e189e8102d7149eb750f782d`; runner SHA256 `e855b02fa878fbfb99bd212f49c8e3226dde0e2707407d492e11ccac58a36bcc`; aggregate report SHA256 `539cb5c723fd4f05b8e2d658d1269705d3f9557e064136d8d39bc56af5728297`; copied report SHA256 `fe8523683f25ab560119187e585be02f95ee0441535db456401cc0276ada662b`.
- Independent read-only review resolved safety, compatibility, privacy, report-order, pointer-oracle, and cleanup-only diagnostic-retention findings. Final re-review found no remaining concrete issue.
