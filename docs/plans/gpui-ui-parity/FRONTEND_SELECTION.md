# Frontend selection and restart contract

The basic selector and both launch paths have Windows evidence from G01. G03-B verifies lock-conflict feedback in both directions. G03-C1 verifies a confirmed restart from the GPUI preview to explicit egui with multiple windows and the HUD active. The canonical Hash presentation completed an egui→GPUI→egui round trip. The in-app reverse restart, remembered chooser behavior and broader cross-frontend settings remain pending. See the [validation log](VALIDATION_LOG.md).

## Controlling precedence

The user specifically requires an ordinary executable launch to stay on egui for now. Apply this truth table, including after all migration goals finish:

| Invocation | Remembered choice | Required frontend |
| --- | --- | --- |
| `multi_launcher.exe` | None | egui |
| `multi_launcher.exe` | egui | egui |
| `multi_launcher.exe` | GPUI | egui |
| `multi_launcher.exe --ui=egui` | Any | egui |
| `multi_launcher.exe --ui=gpui` | Any | GPUI |
| Invalid or missing `--ui` value | Any | Clear error/help; no accidental framework initialization |
| Repeated/conflicting selectors | Any | Deterministic documented parse error or existing parser policy; no ambiguous selection |

Preserve existing noninteractive/helper modes and argument parsing. Do not assume every invocation opens a frontend. Specify how a selector interacts with those modes after inspecting real startup code.

A remembered selection is allowed as UI preference: show it in a chooser, let a settings action request a controlled restart, or create an explicitly selected launch shortcut where an existing shortcut workflow supports that. Such a restart supplies `--ui=gpui` or `--ui=egui`. Remembering a last choice does not change the no-selector rule. No new “preferred” CLI mode is required.

## Required lifecycle

Read minimal startup options without starting either GUI. Preserve the existing early application-data ownership lock and recovery-before-load ordering. Create common services exactly once at the selected framework's supported lifecycle point and on the appropriate owning thread; that may be before its event loop or inside its application callback as the service requires. Make actual thread ownership explicit rather than imposing one startup order on every service.

A retained egui state object must not be the hidden process-wide engine in GPUI mode. A retained GPUI entity must not be the engine owner required for egui mode. Shared service lifetime must not end merely because one utility window closes.

## Existing-instance behavior

`main` retains selector explicitness at the canonical `SingleInstanceGuard::acquire` match. A bare duplicate exits silently with status 0. An explicit duplicate shows a native lock-conflict response naming the requested selector, asks the user to close the current owner and rerun from the same working directory, and exits with an error. The same rule applies to both selectors because the mutex identifies the data directory. The conflict response appears before recovery, settings access or frontend initialization. See `G03-B-INSTANCE-WINDOWS` in the [validation log](VALIDATION_LOG.md).

For an unresponsive owner, use Windows Task Manager to identify the running `multi_launcher.exe` from the matching installation and session. End only the confirmed unresponsive instance. Forced termination can lose unsaved edits and in-flight work. After it exits, rerun `multi_launcher.exe --ui=egui` from the original working directory to use the fallback. A GPUI initialization error that has already exited can be retried with that command immediately.

Verify the conflict on one controllable Windows desktop and one scratch data root. With GPUI open, try a bare launch and then explicit egui. Confirm that the native response belongs to the rejected child and that the GPUI owner stays running. Close that owner, launch explicit egui, then try explicit GPUI to verify the reverse conflict. Use the selected-binary parser tests and the existing mutex identity coverage. Keep restart changes in G03-C so each process behavior has independent evidence.

## Safe switch sequence

1. Validate the requested frontend before disrupting the running session.
2. Identify unsaved edits and active writes, transfers, recording, automation or other work.
3. Use existing save/finish/cancel/abort choices and honor cancellation.
4. Persist committed semantic data and appropriate presentation state through existing stores.
5. Release owned services and the application-data lock in an intentional order.
6. Relaunch the same application/distribution with an explicit selector and the correct working directory/data context.
7. Avoid forwarding actions that would replay a completed side effect.
8. Report launch failure with a usable explicit-egui recovery route.

Do not imply jobs continue through restart unless the existing job format/engine actually supports durable resumption.

The first G03-C checkpoint can exercise the GPUI foundation to egui. Capture the executable and original working directory when planning the switch. Confirm discard with the owner of each retained draft, including the Hash input and proof utility editor. Respect pending Hash worker retirement even after the utility closes. After confirmation, close further admission and pass a one-shot restart disposition from the selected runtime to `main`. Retire owned services and writers, require successful teardown, release the canonical data guard, then spawn the same executable with only the requested selector. A canceled confirmation or failed teardown leaves no child launch. A failed spawn gives an actionable explicit-egui recovery command. The current Coordinate HUD teardown reports failures through logging; a controlled restart must observe its result.

Kit 0.10.0 exposes `WindowExt::open_alert_dialog` for confirmation. The builder can rerender, so accept the restart only in its confirmation callback. Recheck the Hash job slot and pending secondary, HUD and admitted foreground effects before closing further requests. The hotkey stop join needs a checked outcome. Retire owned HWNDs while the event loop can still process native teardown. Exercise normal cancellation, failed teardown, failed spawn, and an injected spawn that reacquires the same canonical data root after the outgoing guard drops. Use the original working directory for the child and the canonical root for identity checks.

The egui switch needs authoritative editor and job decisions. The existing ordinary exit guard handles some runtime teardown and cancellations. It does not provide a save/discard decision for user documents. `ActivityCenter` snapshots cannot stand in for job admission and retirement. Wire a settings action through its existing owner and accepted actions. `SettingsWindowHost::complete` temporarily takes the editor out of `LauncherApp`, so draft preflight must inspect the actual editor or run after it is restored. Preserve pending writes and prevent a new admission between the accepted switch and shutdown. Keep two-way switching and each unresolved owner visible in G03 and its feature goal until verified.

### Reverse switch implementation boundaries

The first egui-to-GPUI checkpoint can admit a provably idle session. Stage Restart in GPUI in `SettingsHostAction`, accept it through the retained Settings host and attach consent to its current owner and accepted pass. Compare the actual Settings save projection and plugin inputs with their baseline. Resolve dirty Settings through its existing Save or Cancel; unresolved document drafts, recording reviews, active or queued file transfers, active or retiring Hash work and sequence playback need owner decisions before admission. A hidden editor can still own a draft.

After confirmation, recheck the editors and reserve the relevant process owners before requesting root close. Main still accepts app, hotkey and radial actions while the egui worker runs. MkMacro brings up its process runtime and pollers even when the visible plugin is disabled. Its `SharedOperationGuard` is the actual playback and recording admission boundary, and `set_shared_store_components` shows the poller, stop-coordinator and worker retirement order. An active operation or autonomous owner without a checked fence needs actionable feedback and leaves the current frontend usable.

Carry a one-shot typed restart result through `GuiStartupServices` and the `GuiWorker` return. Require successful `eframe::run_native`, successful join, a checked receipt for existing shutdown hooks and a completed canonical settings drain. `LauncherInvocationService::stop` and `RadialController::shutdown` can hand native threads to a reaper. Confirm actual retirement when those owners are active. Reuse the typed outer relaunch for both directions. Capture the executable and original working directory, release the data guard at the existing boundary and launch only `--ui=gpui` for the reverse direction. A user cancel, normal close or incomplete shutdown cannot spawn a child.

Use focused Settings host action, `gui_worker_` and `gpui_restart` cases. Native proof needs dirty Settings to defer restart, a resolved draft, a canceled confirmation, one accepted same-root child, and retained bare-launch egui selection. Source inspection at `848c6756d` established these ownership boundaries. No reverse switch is currently implemented.

#### First idle egui restart checkpoint

Split the implementation into two dependent source checkpoints. C2a first preserves the actual `eframe::run_native` outcome through the existing `GuiWorker` completion and join boundary, including a separate panic outcome. Observe it after the current producer shutdown and settings drain without changing ordinary egui close or exit status. Type the existing relaunch target as `Frontend`, derive its selector and recovery command from the same value, and reuse the verified captured executable, original working directory and guard-release spawn path with `Frontend::Egui`. Select binary Nextest `test(gui_worker_) | test(gpui_restart::tests)`, including injected lock reacquisition and exactly one spawn for each target. C2a provides live worker and forward relaunch correctness. The actual Settings route and reverse native evidence follow in C2b.

C2b implements the following idle admission and retirement contract. Add checked persistence outcomes for the final Settings writer and close-preference drain where they are currently unavailable.

Reconcile the preceding boundaries with the current `aeb726d5a` source. Use a dedicated Settings-owned confirmation and defer preflight until both `SettingsWindowHost` and `SettingsEditor` are restored to `LauncherApp`. Validate saved projection, current plugin drafts and invalid raw input, pending settings saves and retained document/live-work owners. An owner without a current idle receipt leaves restart unavailable with an actionable reason.

Once confirmed, use one reversible prepare/ack through `GuiStartupServices` and the existing wake. Main processes preparation before further route dispatch, gates current hotkey/radial/session transitions and freezes MkMacro through the existing `SharedOperationGuard`. The GUI rechecks its current state after the ack. Denial or loss of the exact owner restores admission and egui interaction before any effective close.

The first bounded profile requires verified absence of invocation service and live native radial/preview hosts that may defer retirement. `invocation_service_needed` also checks the Paste Sequences plugin. Separately retire the always-started Radial Gallery worker with a checked join, since its current Drop signals stop. MkMacro pollers may start with the plugin hidden. Use the existing order to stop them and retire the coordinator/runtime/recorder with checked outcomes. Exclude an active Hot Folder driver until it has a checked retirement path. Preserve legacy hotkey stop, required native window retirement and final settings drain.

Make `GuiWorker` return the typed disposition after successful eframe return and expose required `on_exit` receipts. Main must join the worker, confirm actual owner retirement and drain final writes before returning the existing relaunch plan. Parameterize that plan with the requested frontend. After the outgoing runtime scope and canonical lock have ended, launch the captured exact binary once with `--ui=gpui` and original working directory. Record discard/cancel, empty and busy admission, native teardown/persistence failure and accepted-ack races in focused tests and one eligible Windows round-trip.

## Acceptance scenarios

Selection tests cover every table row; native scenarios cover the same release artifact with each frontend, clean shutdown/reopen, saved-GPUI plus bare launch selecting egui, current GPUI owner plus explicit egui, and manual fallback after GPUI initialization failure.

Use an isolated instance/data context. Record binary path/hash, commit, actual command and outcome. The default should be checked again at final release closure. G01 startup receipts cover the current no-preference selector paths. G03-specific acceptance remains pending.
