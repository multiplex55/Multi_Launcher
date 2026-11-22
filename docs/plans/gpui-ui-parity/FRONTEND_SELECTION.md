# Frontend selection and restart contract

The basic selector and both launch paths have Windows debug evidence from G01-A/B. G03 still owns lock-conflict feedback, remembered chooser behavior and controlled restart. See [Validation log](VALIDATION_LOG.md).

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

The inspected main startup returns when the same data directory is already owned. G03 must make an explicit UI selection observable: a GPUI owner cannot cause `--ui=egui` to disappear silently.

Preferred minimum: a clear actionable existing-instance response identifying that the current owner must close before the requested frontend can start. A supported controlled restart path is also acceptable. Use current single-instance primitives and authenticated/local coordination only where necessary; do not build a new daemon or general IPC framework.

Specify the same-frontend invocation behavior consistently with the baseline. Distinguish a data-lock conflict from a frontend initialization error. For an unresponsive owner, provide recovery instructions that do not require its UI. Automatic termination or arbitrary crash recovery is not required.

The G03-B implementation boundary is `main`'s canonical `SingleInstanceGuard::acquire` match. Preserve selector explicitness. Bare duplicates follow the existing silent exit. An explicit request gets a native response naming the requested selector and telling the user to close the current owner, then rerun from the same working directory. The current mutex identifies the data directory and does not report the owning frontend. The response should work for either requested frontend without changing the mutex namespace or adding an IPC protocol.

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

The egui switch needs authoritative editor and job decisions. The existing ordinary exit guard handles some runtime teardown and cancellations. It does not provide a save/discard decision for user documents. `ActivityCenter` snapshots cannot stand in for job admission and retirement. Wire a settings action through its existing owner and accepted actions. `SettingsWindowHost::complete` temporarily takes the editor out of `LauncherApp`, so draft preflight must inspect the actual editor or run after it is restored. Preserve pending writes and prevent a new admission between the accepted switch and shutdown. Keep two-way switching and each unresolved owner visible in G03 and its feature goal until verified.

## Acceptance scenarios

Selection tests cover every table row; native scenarios cover the same release artifact with each frontend, clean shutdown/reopen, saved-GPUI plus bare launch selecting egui, current GPUI owner plus explicit egui, and manual fallback after GPUI initialization failure.

Use an isolated instance/data context. Record binary path/hash, commit, actual command and outcome. The default should be checked again at final release closure. G01 startup receipts cover the current no-preference selector paths. G03-specific acceptance remains pending.
