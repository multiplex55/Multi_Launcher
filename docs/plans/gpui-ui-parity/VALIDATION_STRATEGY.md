# Validation strategy

**Planning status:** no validation in this document has been run. Commands below are verified source-based examples or clearly identified requirements for future tests. Evidence fields must remain pending until an execution receipt exists.

**Reference inspected:** `multiplex55/Multi_Launcher` master at `f7bc610a86bcad1c14b69dce65b1d804df1b7934`, merged November 21, 2026. The implementation bootstrap records its actual base and reconciles later changes before relying on this reference.

## 1. Acceptance objective and authority

Protect the working launcher engine and egui path while recreating every inventoried GUI, control, state and interaction in GPUI using GPUI Kit. Functional and interaction parity are mandatory. A cleaner visual treatment is allowed without silently dropping controls, changing workflow, or treating a screenshot as proof of behavior.

Current task instructions and [AGENTS.md](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/AGENTS.md) govern verification. Old project plans and native campaigns are references, not automatically inherited gates. The size of this migration does not itself mandate a complete Nextest run.

There are three distinct proof questions:

1. Does the shared engine still produce the established result for the same input and persisted data?
2. Does each frontend correctly translate user actions and engine events?
3. Does each native frontend actually draw, accept input, and perform the required Windows behavior?

A common backend bug can make both frontends agree incorrectly. Compare results to a recorded pre-migration expectation as well as to each other.

## 2. One machine, bounded ownership

Use the one authorized checkout and one task branch; no worktrees. Follow the repository's configured planner, implementer and reviewer roles. At the researched reference these are read-only planner/reviewer and one implementation writer; re-read their actual definitions at execution rather than hard-coding stale model names.

The parent coordinates scope, checkpoints and Git. Only one implementer modifies source for the active milestone. Assign one owner for Cargo builds/tests and one owner for native desktop control; serialize work that competes for the same target directory, fixture data, hooks, system clipboard, foreground window or application instance. Independent read-only investigation/review may run concurrently.

Do not run both UI implementations concurrently against the same data root as a parity experiment. Use sequential runs with cloned synthetic fixtures or distinct disposable roots. Keep the existing canonical-data-root single-instance rule.

## 3. Baseline before extraction

For the next bounded slice, record:

- Actual base commit, branch, relevant dirty state, toolchain/target and build feature/profile.
- Existing source owner and entry points, affected core contracts and GUI states.
- Smallest existing tests that exercise those contracts, actual selected count and result.
- Required native baseline observations that cannot be inferred from code, with fixture and environment.
- Known failures with a reproducible symptom, their baseline provenance, and whether they obstruct the slice.

Do not run a full suite merely to obtain a baseline. A historical pass is useful context but not a new run. A pre-existing failure is not automatically excused when migration touches its contract; classify it and resolve any risk it creates for the current acceptance. Avoid unrelated repair campaigns.

Capture behavior before moving ownership out of LauncherApp. Preserve expected command results, persistence round trips, cancellation/late-result behavior, session state, focus/visibility intent and lifecycle invariants. Early core extraction can pass a shared-core-plus-egui gate while GPUI remains explicitly pending. It must not claim two-frontend parity before GPUI exists.

## 4. Select the target before the case filter

Case filters restrict execution but may not sufficiently restrict Cargo compilation. Select a package and library/integration target first, then a case filter. Reuse the current warmed Cargo profile where appropriate. `fast-dev` exists and is an available option, not a compulsory new cache build for every task.

Current registered grouped integration targets:

| Target | Actual source | Example covered families |
|---|---|---|
| `--test domain` | `tests/suites/domain.rs` | Typed command bus, clipboard modify runtime/config, file search, diff, hotkeys, indexer, selection, settings. |
| `--test plugin_queries` | `tests/suites/plugin_queries.rs` | Plugin parsing, command/query discovery and help. |
| `--test pure` | `tests/suites/pure.rs` | Query autocomplete, hotkey events, quit hotkeys, macro compiler. |
| `--test focus_visibility`, `--test gui_visibility`, `--test hide_after_run` | Matching top-level `tests/*.rs` files | Individual integration targets, not members of the grouped suites. |
| `-p multi_launcher_table_core --lib` | `crates/multi_launcher_table_core/src/lib.rs` | Separate table-engine package. |

Source: [Cargo.toml](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/Cargo.toml) and [test performance guidance](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/dev/test_perf/README.md).

The following are examples to select according to the actual diff. They are not a checklist to run in full for every milestone.

```powershell
# Initial type/build check for the existing library and app target.
cargo check -p multi_launcher --lib --bin multi_launcher

# Typed routing and architecture when the shared command seam changes.
cargo nextest run -p multi_launcher --test domain -E 'test(command_bus_architecture)'

# File-search or diff changes: select the grouped target, then the family.
cargo nextest run -p multi_launcher --test domain -E 'test(file_search_)'
cargo nextest run -p multi_launcher --test domain -E 'test(diff_)'

# Query/plugin behavior when dispatch or registration changes.
cargo nextest run -p multi_launcher --test plugin_queries -E 'test(help_plugin::search_returns_help_action)'

# Focus/visibility intent tests, only when that seam changes.
cargo nextest run -p multi_launcher --test focus_visibility
cargo nextest run -p multi_launcher --test gui_visibility

# Existing focused GUI/core regressions; combine relevant same-target cases.
cargo nextest run -p multi_launcher --lib -E 'test(=gui::render::tests::keyboard_navigation_is_consistent_between_grid_and_list_modes) | test(=gui::render::tests::root_list_viewport_builds_bounded_rows_with_absolute_ids_and_selection)'
cargo nextest run -p multi_launcher --lib -E 'test(=settings_save::tests::hidden_settings_editor_completes_and_a_second_save_uses_the_new_baseline)'
cargo nextest run -p multi_launcher --lib -E 'test(=gui::notes_dialog::tests::quick_notes_wrap_links_preserves_open_panel_and_view_mode)'
cargo nextest run -p multi_launcher --lib -E 'test(=gui::calculation_pad_dialog::tests::studio_usability_calculation_host_migration_preserves_cached_session_and_pending_decision)'

# Clipboard shared-service family, when clipboard behavior or ownership changes.
cargo nextest run -p multi_launcher --lib -E 'test(clipboard_modify) | test(protected_clipboard)'
cargo nextest run -p multi_launcher --test domain -E 'test(clipboard_modify_)'

# Narrow separate-package control when table ownership or workspace setup changes.
cargo nextest run -p multi_launcher_table_core --lib -E 'test(=tests::table_parse_csv_quotes_escaped_quotes_and_embedded_newlines_preserve_strings)'
```

Confirm the selected names/count in the runner output or a targeted inventory if needed. A zero-test selection is failed verification, even if a command exits successfully. A broad substring that misses required cases is also insufficient. If extraction legitimately moves tests, update the selector and record the mapping; do not report the old empty selector as success.

Use the existing helpers when useful:

```powershell
& .\dev\test_perf\run.ps1 -TargetKind Library -Filter 'test(=gui::render::tests::keyboard_navigation_is_consistent_between_grid_and_list_modes)'
& .\dev\test_perf\run.ps1 -TargetKind Integration -Target plugin_queries -Filter 'test(help_plugin::search_returns_help_action)'
```

`dev/test_perf/profile.ps1` supports selected build/run receipts. Inspect its current arguments before using it; its examples are not universal timeout requirements. Do not force `--offline` when new dependencies must be obtained. Use a locked dependency graph once the intentional GPUI/GPUI Kit dependency update has been recorded.

## 5. Preserve the current Nextest repair

The current [.config/nextest.toml](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/.config/nextest.toml) declares `nextest-version = "0.9.98"` and experimental wrapper scripts. Windows default-profile tests in `multi_launcher` use [scripts/nextest-test-stack.ps1](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/scripts/nextest-test-stack.ps1) with `target-runner = "within-wrapper"`.

Preserve these contracts:

- Test child `RUST_MIN_STACK` has a 4 MiB floor; larger valid values survive and parsed values are normalized.
- Configured Cargo target runners remain in the execution chain.
- Production app behavior is unchanged by this test-only configuration.
- New workspace packages do not inherit a broken relative script path. Validate wrapper resolution/scope if package structure changes.
- Do not treat a stack-aborted test as an expected migration regression or replace the wrapper with a production-wide environment hack.

The repair covered concrete sample failures; it did not establish that every historical failure shared that cause. Historical failing counts are not a mandate to rerun the old recovery campaign.

## 6. Fixtures and meaningful tests

Before constructing every new/touched app fixture or test profile:

```rust
settings.paste_sequence.next_hotkey = None;
settings.paste_sequence.cancel_hotkey = None;
```

Keep production defaults and ordinary error notifications. Do not mute all toasts to hide registration errors.

Use explicit temporary roots, synthetic data and injected fakes for ordinary tests. Avoid process-wide current-directory changes in new fixtures when explicit-root APIs exist. Shutdown app-owned services before removing their fixture root. Tests must not casually read persisted user history, clipboard data, or real launcher profiles.

Add tests only for material contracts at risk: routing, state transitions, persistence, cancellation, host lifecycle, event translation, accessibility semantics where newly introduced, and a bug's meaningful regression. Do not add assertions that merely repeat implementation details or test each cosmetic constant.

Preserve recent non-vacuous assertions. Calculation Pad host-migration tests must still exercise a real pending decision and preserve the session across dock/popout changes. Panel inventory must use each surface's real ownership/open seam; opening a registry ID is not proof that every child/hosted surface opened. Recent panel fixtures deliberately avoid live clipboard/history access.

Some current architectural tests inspect source paths or egui-specific commands. Move assertions to the new canonical owner when necessary while preserving the invariant. Keep egui adapter tests, introduce framework-neutral intent tests, and add GPUI adapter coverage. A mocked egui viewport-command pass is not GPUI native focus evidence.

## 7. Layered checks by change type

| Change | Smallest sufficient starting checks | Reason to add more |
|---|---|---|
| Pure GPUI layout/theme/control mapping | Compile affected frontend; exercise changed GUI state and interaction; inspect visual result. | Shared adapter, focus semantics, keyboard behavior or data conversion changed. |
| Shared model/service extraction | Focused domain tests plus current egui caller checks and baseline-output comparison. | More consumers affected than the chosen contracts cover, or collateral failure appears. |
| Typed command/event boundary | Parser/dispatch and relevant handler tests; one end-to-end action in each affected frontend. | New async lifecycle, destructive confirmation, or session ownership. |
| Persistence/settings | Defaults and old-data compatibility, save/reload, both-frontends round trip on disposable roots. | Schema migration or data-root ownership change. |
| Input/window lifecycle | Deterministic event/intent cases plus native Windows checks for the changed behavior. | New host/popout, DPI, focus, system-menu, keyboard-layout or hook boundary. |
| Build/feature/dependency changes | Required supported frontend build configurations and runner resolution. | Package coupling accidentally forces GPUI into the fallback artifact or breaks supported feature combinations. |

Do not demand a GPUI test binary or feature name before architecture chooses it. Each milestone replaces generic future commands with actual package/target/feature/filter arguments.

## 8. Native and visual evidence

Run native checks in an interactive Windows environment with controlled synthetic fixtures. Headless logic tests, fixture-only harness output and screenshots of mockups do not satisfy native acceptance.

For each GUI specification, identify which native properties actually apply: opening/closing, keyboard/pointer interaction, focus return, modal confirmation, cancel/discard, child/popout lifecycle, resize/scroll, DPI/monitor behavior, hotkeys, selection/caret/IME, drag-and-drop, tray or Windows integration. Do not require every property for every static surface.

Record actual frontend, artifact/source identity, fixture, Windows/display environment, steps, expected outcome, observed outcome, capture/log links and cleanup. Screenshots establish presentation; concise event/state observations establish behavior. Pixel-identical output across frameworks is not required. Control/feature/interaction parity is.

Native controller owns only the app/test processes and fixtures it started. Do not kill unrelated user processes or launch competing hooks. Clipboard tests requiring real system access run in a disposable/isolated session with synthetic content; store redacted diagnostics rather than clipboard payloads.

The existing real clipboard interoperability test is deliberately ignored:

```powershell
cargo nextest run -p multi_launcher --lib --run-ignored ignored-only -E 'test(=platform::protected_clipboard::platform::tests::native_reader_interoperates_with_protected_publisher)'
```

Run it only when its isolated Windows prerequisites are satisfied and the current slice changes that boundary. It initializes a GUI-like STA and exercises the real protected publisher/ordinary reader. Fake tests and in-memory WinRT packages do not replace it. UI Load/Reload/Apply/Undo behavior still needs the relevant GPUI native flow when that GUI is migrated.

Preserve current bounded asynchronous clipboard admission/completion, stale-result fences, draft retention after failed reads, reviewed overwrite checks, private-marker refusal and Undo preservation. Do not reintroduce synchronous GUI waits while recreating the surface.

If native access is unavailable, label affected checks **blocked/unverified** and continue independent work. Do not mark full parity complete, silently waive the requirement, or impose a new approval ritual for routine safe test fixtures. Ask only when a genuine access, scope or safety decision is necessary after authorized preparation is complete.

## 9. Frontend selection and single-instance acceptance

Required future deterministic cases include:

- No frontend selector, including a saved GPUI last-choice, selects egui for now.
- Explicit `--ui=gpui` selects GPUI; explicit `--ui=egui` selects egui.
- Remembered chooser state can produce an explicit restart command but does not change bare-launch behavior.
- Invalid/unavailable choices produce the documented visible result; they do not silently launch an unrelated frontend.
- Existing non-UI CLI/headless commands retain their behavior and argument routing.
- UI selection occurs before frontend initialization. Only the chosen frontend owns its event loop and Windows effects.
- A second instance for the same canonical data root cannot bypass ownership by choosing a different frontend.
- Deliberate restart preserves the data root/required arguments, handles old-process shutdown and mutex release, and leaves a recoverable path if new startup fails.

The current [single-instance implementation](https://github.com/multiplex55/Multi_Launcher/blob/f7bc610a86bcad1c14b69dce65b1d804df1b7934/src/platform/single_instance.rs) keys ownership by canonical AppDataRoot, not frontend. Existing unit tests cover duplicate acquisition, release/reacquisition and equivalent identities. Actual launch/restart behavior also needs Windows evidence.

## 10. Budgets, timeouts and wider runs

No fixed 100-second timeout applies to an entire Cargo invocation. Separate dependency preparation/build, test inventory, test execution and native-operation budgets. Set bounds using the selected work and observed cache condition. Compilation can legitimately dominate a tiny test run.

Keep progress observable. If a phase appears stuck, inspect the owning process/output and concrete wait before extending or terminating it. Preserve diagnostics and clean up only owned processes. Distinguish compilation failure, test assertion failure, test-process crash, timeout, zero selection and unavailable environment.

Do not rerun expensive unchanged builds just to reproduce a timestamp. Existing helpers can reuse build/list metadata and record source fingerprints. Do not clear caches as routine troubleshooting.

A wider run is justified by an explicit current requirement, evidence of collateral failure, or a stated shared-impact risk that focused contracts cannot adequately cover. Record the unresolved question, chosen scope and expected value first. A final release/build gate may need broader compilation without implying all tests must run. Full Nextest is not compulsory per goal, per GUI family, or merely because the branch is large.

## 11. Evidence freshness and completion

Every receipt identifies the tested commit or exact source/dirty fingerprint. Record the actual final checkpoint separately if the run preceded the commit. A later source change requires an applicability decision:

- **Still applicable:** the evidence's contract and runtime path are unchanged; record why.
- **Needs targeted revalidation:** affected adapter/service/theme/input/lifecycle path changed; list impacted GUI and evidence IDs.
- **Superseded:** replacement evidence directly covers the same requirement.

Do not invalidate all screens after every harmless edit, and do not keep all old evidence valid after a shared dispatcher or lifecycle change. Maintain one authoritative evidence index; link raw receipts instead of duplicating them into competing ledgers.

A GUI row is verified only after its specification, required behavior/native/visual evidence and independent review are complete. “Implemented,” “compiles,” “reviewed,” and “verified” are distinct states. Explicit deferment requires an actual authorized scope decision; it does not satisfy original full parity.

Before final completion, verify the full inventory is accounted for, required frontends launch, core/egui regression checks remain applicable, blocking findings are closed, and mandatory native checks have evidence. Summarize remaining limitations honestly. User acceptance cannot be invented.

## 12. Continuation and genuine blockers

Normal compiler errors, focused test failures and review findings stay in the implementation/review loop. Do not end the task after a fixed number of routine iterations or one successful GUI. Continue to the next authorized goal/checkpoint automatically.

A true stop requires a concrete unavailable dependency, unsafe unresolved state, missing necessary authority/access, or decision that cannot reasonably be inferred. Finish independent authorized work, preserve partial changes and record the smallest next action. Follow AGENTS' tracked stall-report convention. Include actual branch/SHAs, Git state, last accepted checkpoint, failure/missing evidence, attempted fixes, cleanup and continuation instructions.

A reporting-only commit is not a completed implementation checkpoint. Commit locally before the normal one best-effort push attempt; an unavailable push does not block continued local work. Never claim remote visibility without a successful push.

