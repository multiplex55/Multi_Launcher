# Frontend selection and restart contract

Status: specified product policy; implementation and validation not started.

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

## Acceptance scenarios

Selection tests cover every table row; native scenarios cover the same release artifact with each frontend, clean shutdown/reopen, saved-GPUI plus bare launch selecting egui, current GPUI owner plus explicit egui, and manual fallback after GPUI initialization failure.

Use an isolated instance/data context. Record binary path/hash, commit, actual command and outcome. The default should be checked again at final release closure. No acceptance evidence exists yet.
