# Frontend selection and restart contract

The basic selector and both launch paths have Windows evidence from G01. G03-B verifies lock-conflict feedback in both directions. G03-C1 verifies a confirmed restart from the GPUI preview to explicit egui with multiple windows and the HUD active. The reverse direction, remembered chooser behavior and cross-frontend persistence remain pending. See the [validation log](VALIDATION_LOG.md).

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

## Acceptance scenarios

Selection tests cover every table row; native scenarios cover the same release artifact with each frontend, clean shutdown/reopen, saved-GPUI plus bare launch selecting egui, current GPUI owner plus explicit egui, and manual fallback after GPUI initialization failure.

Use an isolated instance/data context. Record binary path/hash, commit, actual command and outcome. The default should be checked again at final release closure. G01 startup receipts cover the current no-preference selector paths. G03-specific acceptance remains pending.
