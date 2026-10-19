# Action parity, live pins, and note Find execution ledger

Active scope: the user-provided 2026-10-18 goal. Only action execution parity,
opt-in window-attached live cropped pins, and transient in-note Find.

Baseline: clean `live-pinboard` branch at `96d4926e`. Preserve static pins,
radial presentation policy, explicit action settings, and note persistence.

## Checkpoints

| Stage | Objective / owner | State | Acceptance and verification |
| --- | --- | --- | --- |
| M0 | Reconcile checkout, types, callers and tests | complete | Read-only ownership inspection; no broad build |
| M1-A/B | Measure action effects separately from presentation; fix demonstrated gaps in existing dispatch owners | complete | Compact parity cases, directly affected paste/radial tests; combine tests/fix if needed |
| M2-A | Typed live intent through existing selector/parking lifecycle; unambiguous external window binding | pending | Parser/handler and selection/restoration tests; static path unchanged |
| M2-B | Window-content capture and bounded latest-frame sessions owned by pinboard | pending | Fake stream/budget/pause/identity/cancellation tests; Windows compile |
| M2-C/D | Child texture updates, status, Pause/Resume, latest-frame copy/save and cleanup | pending | Presenter/model/lifecycle tests, mixed static/live limits and shutdown |
| M3-A | Per-panel Unicode-safe source matching and navigation | pending | Literal case-insensitive range/navigation tests; no content changes |
| M3-B | Find strip, scoped keys, Edit/Preview/Split navigation | pending | Targeted note-panel interactions, honest source-only fallback |
| M4 | Independent focused review and user documentation | pending | Final targeted integration, fmt, one Clippy attempt; report native smoke limitations |

Implementation checkpoints execute sequentially. Each coherent verified checkpoint
is committed using `<type>(<scope>): [M#-X] ...`; one normal push is attempted
after each commit. Push failure does not block continuation. No history rewriting.

Verification budget: one focused pass per materially completed subsystem plus
one final targeted integration pass and one final Clippy attempt. At most two
bounded diagnosis/remediation/recheck cycles per distinct feature failure.
No full suite, all-targets/release builds, benchmarks or unrelated cleanup.

## Evidence and decisions

- M0: attached source findings match the existing pinboard model/lifecycle and
  independent viewport presenter. Existing Windows crate is 0.58; WGC/D3D
  features are not enabled. Existing regex dependency can support note matching.
- Tests, commits, push outcomes and concrete limitations will be recorded here
  at their checkpoint boundaries.

### M1 compact effect / presentation matrix

| Case | Shared effect contract | Deliberate presentation / existing coverage |
| --- | --- | --- |
| External action | Same original action and arguments, once; retain activation source | List/grid/gesture/macro share `activate_action`; radial uses prepared dispatch |
| Launcher dialog | Same reachable panel and focus intent | Root restoration must not cover independent native editors |
| Saved paste sequence | Same prepared sequence and captured external target | Effective keep-dialog-visible setting governs playback |
| Clipboard modification | Same operation and captured history query | Ordinary hide preference; radial root preservation |
| Destructive action | Cancel zero, confirm once with retained invocation | Additional radial confirmation remains valid |
| Exclusive capture | One input owner after overlay teardown | Radial KeepOpen incompatibility remains explicit |
| Stale / disabled action | No operation or false successful history | Existing prepared-lease revalidation rejects stale targets |

M1 inspection found a concrete capture prerequisite mismatch: the shared
`PinboardCaptureLifecycle::begin` rejects hidden roots, while eligible radial
capture preserves a hidden root. Repair belongs to M2-A's parking/restoration
boundary; M1 does not add a per-surface visibility workaround.


M1-A: six-source primary contract regression added; radial Help asserts reachable
panel and one attributed dispatch. Five selected Nextest tests passed twice after
one borrow-check correction (owned history query). Command: `cargo nextest run
--lib -E 'test(action_parity_) | test(primary_universal_action_delegates_to_exact_activation_path) | test(ordinary_radial_execution_preserves_root_launcher_state) | test(saved_paste_to_advance_run_repeats_three_times_through_app_and_physical_admission) | test(saved_run_prompt_continuation_opens_closed_grid_route_and_keeps_root_visible)'`.
Touched-file rustfmt and diff whitespace checks passed. `cargo fmt --check`
fails on preexisting formatting in `src/bin/radial_acceptance/native.rs` and
other untouched files; no unrelated formatting edits made. Test-generated
`clipboard_modifiers.json` removed after provenance confirmed. M1-B has no
production change; hidden-root capture gap proceeds to M2-A.
