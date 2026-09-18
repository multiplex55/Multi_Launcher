# Radial stabilization ledger

Status: **S2 complete — tooltip metrics and stable hover presentation.**
S0 and S1 remain complete; no native pass is claimed. This ledger records
the current checkout, ownership boundaries, source-backed causes, and the
evidence still needed for S1–S5. The current checkout is authoritative; the
supplied source snapshot and reference archives are historical/visual inputs.

## Identity and compatibility anchors

| Field | Recorded value |
|---|---|
| branch | `radial-menu-2` |
| `STABILIZATION_START_HEAD` | `1f032ec9083e59ad3a99dd513d2c15ab0239e0e1` (`update`) |
| initial worktree | clean: no staged, unstaged, or untracked changes |
| immutable feature baseline | `0d0acaf471a52f49a7ebdff61879416eefa9fc9b` (preserved) |
| branch point | `f7c5f61ed2faa2288f5c19ddeaea66de6f760a2b` (preserved) |
| historical radial ledgers | `f50fb305:docs/plans/radial-menu.md`; `f50fb305:docs/plans/radial-repair-and-designer.md` |
| current source | checkout at `STABILIZATION_START_HEAD`; no source files changed in S0 |

The historical ledgers above are intentionally referenced by commit-qualified
path. Deleted historical ledgers are not restored. The initial identity was
confirmed with an empty `git status --short`, `git diff`, and
`git diff --staged` result. No Cargo/build/test process was run for S0.

Direct dependency/runtime anchors from the current checkout:

| Anchor | Version/source |
|---|---|
| Rust package edition | `2024` |
| `egui` / `eframe` | `0.27.2` resolved in `Cargo.lock` (`Cargo.toml` requests `0.27`) |
| `windows` | `0.58.0` resolved for the direct Win32 integration |
| `image` | `0.24.9` resolved for the direct image integration |

## Reference inventory and evidence limits

Reference root: `docs/references/`. Archives and images were inspected as
read-only data references; no reference script, installer, callback, or
administrative example was executed or extracted into the repository.

The supplied stabilization source notes identify the application snapshot as
`multi_launcher(20260918-003954).zip`, 26,702,086 bytes,
SHA-256 `b2b22e846b57c73ffe9a668a185fed5a09e623410fcc1402c25cf03c0523f070`.
The local reference archives differ from the supplied snapshot-note archive
identity. The local historical-manifest identities are:

| Local path | Bytes | SHA-256 | Role |
|---|---:|---|---|
| `docs/references/Radial menu v4.zip` | 4,009,952 | `442A8A16128C64012C85AC520A6B4AF49027A70BC39B9F362DA40628E8FCEFDA` | RM4 source/settings/skins reference |
| `docs/references/RadifyClass-RadifySkinEditor-main.zip` | 15,213,205 | `16F67A5100ADA57915966017EDA56626F77A7668D6CC583C99B82432716B920C` | Radify source/editor reference |
| `docs/references/screenshots/reference1.png` | 451,287 | `65E533D509465D5C6DBCBB01CAF7CDB72069639BF0D26198B23DE1736FF9FAC2` | RM4 ornate multi-ring collage |
| `docs/references/screenshots/reference2.png` | 55,584 | `17CDA73BE314BEACE70330714EA650B5FD676E67AA4A04563CD563FCE9C6AD09` | compact Carbon concentric menu |
| `docs/references/screenshots/reference3.png` | 126,625 | `67D047A99EE4AC6AD9E9EAC0DD8C6B1BE353A4241B023854AAA8CF343813D286` | editor UI inspiration |
| `docs/references/screenshots/reference4.png` | 83,189 | `1E408AF0323862D2BBAF0E8EC67EB19A4A385A805CC9F2F3B77EE7BD4BECDCB6` | dense emoji rings |
| `docs/references/screenshots/reference5.png` | 97,168 | `ED7A3E0FA3EF9A1A8C2FF500E6ADB19EB98C3B2E04507D6EEC7227F73859E171` | RM4 light/plain menu |
| `docs/references/screenshots/reference6.png` | 1,386,949 | `0A09CCCE0B607D87AF563EA2869522BB94F7EA37622D664A567FEFA582679808` | Carbon/wood variants |
| `docs/references/screenshots/reference7.png` | 63,948 | `B19CD5B865D4FC38A902634C909F815470DCFABD106195C8E4CE25A76CDC2A41` | Carbon symbol-output use case |

For comparison, the stabilization source notes list a supplied RM4 archive of
4,015,391 bytes with SHA-256
`efe57915bbc5f68fcd23d20eb66db4025d8c8fd7eaf26dd176acfb38a3fa1c19`.
The local archives above are therefore the usable current references. No local
approved blue overlapping-Cascade image was found; the blue overlap is a
behavioral/visual requirement, not an available local asset.

## Ownership map

| Surface | State/command owner | Wake, window, and teardown boundary |
|---|---|---|
| Root grid/list viewport | `LauncherApp` in `src/gui/render.rs`; eframe `ViewportId::ROOT`; `visible_flag`, `restore_flag`, and shared `ctx_handle` | Main-loop visibility routing uses `VisibilityToggleBatch`/`handle_legacy_grid_toggle`; `src/visibility.rs` emits viewport commands; native HWND lookup is the existing root window manager. GUI restore/apply also participates. |
| Deferred Designer viewport | `RadialEditorState` in `src/gui/radial_editor/mod.rs`; stable `radial_designer_viewport_id()` through `show_deferred` | Designer reply wake targets the Designer viewport; `DesignerIntentBridge` wakes ROOT for intents/preferences. The editor owns its draft/session, close prompt, preview, callbacks, and resource release. |
| Embedded Designer preview | `EmbeddedPreview` in `src/gui/radial_editor/preview.rs`, rendered inside the Designer viewport | `RadialAuthoringSession::embedded_preview` owns the token and prepared frame; `PrepareEmbeddedPreview` is read-only preparation. It shares the radial renderer but does not own a native desktop HWND or dispatch leaf actions. |
| Native desktop preview | Main loop owns `AuthoringClient`/`AuthoringMainEndpoint` and `NativePreviewCoordinator` (`src/radial/authoring/native_preview.rs`); session tracks lease/pending preview | Main services start/update/stop requests and owns the `NativeHost` lease. `AuthoringResourceDemand::Release` cancels preview/auditions and restores the runtime document; late replies are session/generation-correlated. |
| Runtime radial/native host | `RadialController` owns runtime preparation/session/lifecycle; `NativeHost` in `src/radial/native.rs` owns input and visual HWNDs and emits `NativeEvent` | Main routes `InvocationIntent` to the controller and feeds `Opened`/`Closed` back to the invocation service. `NativeCommand::Close` must receive terminal `NativeEvent::Closed`; the current send-error path retires the host without that feedback. |
| External target | Captured/sanitized `InvocationContext` and Universal Action dispatch, not launcher GUI state | The foreground/under-pointer target is sampled by the preview/runtime service and carried with typed dispatch requests. The target application owns its own window/input; no external target HWND is a Designer or root viewport. |

## Gesture and close traces

### Short tap

`LauncherInvocationAdapter::process` claims the configured primary only after
the configured modifiers match. For the reported mapping the primary is
`End` (`0x23`) with `Shift+Alt+Win`; the adapter emits one
`ScheduleDeadline` and owns the matching primary release. A release before the
configured threshold reduces through `InvocationReducer::release` to
`CancelDeadline` plus `ToggleLegacyLauncher`. The main loop converts that to a
single `VisibilityToggleBatch` entry, then `ControllerEvent::ToggleLegacyLauncher`
and the root visibility path. It does not intentionally focus away or wiggle
the mouse. A later GUI `restore_flag`/`apply_visibility` edge can still change
the visible result, and the shared `egui::Context` currently does not make the
command target as explicit as the ROOT wake target.

The user reports that this baseline works with Designer and native desktop
preview both closed. S0 did not run a source-matched executable, so that report
is recorded as user observation, not a native pass.

### Long hold

At the deadline (`press timestamp + configured threshold`), the reducer
promotes the already-captured `HeldRadialAction`: a closed lifecycle emits
`OpenRadial`; an existing opening/active/closing lifecycle captures a close
action. An active runtime close becomes `NativeCommand::Close` through
`RadialController::close`; successful native acknowledgement feeds
`InvocationEvent::RadialSessionClosed`. The owned key release is drained through
`TriggerReleased`/`AwaitingOwnedRelease`, so release must not reopen an old
surface. Opening cancellation invalidates its generation instead of closing a
different session.

### Designer X and teardown

`show_deferred` observes the child viewport close request, calls
`RadialEditorState::request_close`, and sends `CancelClose` while the editor
remains open. `request_close` cancels embedded-preview preparation, flushes UI
preferences, and consults `RadialAuthoringSession::close_decision`. Today any
pending request returns `AwaitingRequest`, whose GUI branch is empty; clean and
dirty branches are the only terminal/prompt paths. If clean, native preview is
stopped, embedded preview disposed, authoring callbacks/resources released,
and the deferred viewport receives `Close`. Main-owner `Release` then cancels
native preview/auditions and restores the runtime document. Runtime close is a
separate `RadialController`/`NativeHost` lifecycle and must not be conflated
with Designer disposal.

## Pending request inventory

The actual `src/radial/authoring.rs::PendingRequestKind` variants are classified
for the S1 close policy as follows:

| Class | Variants | Close implication to verify in S1 |
|---|---|---|
| Disposable/read-only preparation | `Snapshot`, `FontCatalog`, `PrepareEmbeddedPreview`, `ExportPackage`, `ExportSkin`, `AuditionManagedAsset` | Cancel/invalidate or reconcile the exact request; do not make clean X wait forever. Audition resources must also be released. |
| Preview lifecycle / cleanup | `LivePreview`, `CancelPreview`, `StartNativePreview`, `UpdateNativePreview`, `StopNativePreview` | Stop/cancel must supersede start/update as appropriate; stale preview replies cannot recreate a closed surface. |
| Durable transactions | `Commit(CommitDisposition)`, `ReplacePackage` | Preserve Save/Apply/Cancel and revert-after-Apply semantics; latch close intent, show status, and finish or reconcile safely. |

The current broad `close_decision` check does not distinguish these classes.
`send_commit` and `send_native_preview` record a send error without clearing or
reconciling the exact pending request; `stop_native_preview` currently ignores a
send failure. `poll_replies` can request `FontCatalog` immediately after a
reply drain whenever the catalog is unloaded and no request remains, including
after a close attempt. These are source-confirmed S1 seams, not native pass
claims.

## Preference audit

Designer preferences are UI-only (`RadialDesignerPreferences`): tree/inspector
visibility, widths, mode, zoom/pan, skins pane, window size/position, and scale.
`mark_preferences_changed` updates the snapshot and starts a 300 ms debounce;
the Designer bridge wakes ROOT, and `LauncherApp` calls `Settings::update` only
when `take_preferences_for_persist` says the change is ready or close forces a
flush. Unchanged geometry is compared before marking dirty. Close queues the
final preference snapshot before callback disposal.

The audit found no path from this Designer-only update to
`request_hotkey_restart`, `settings_generation`, launcher route re-registration,
or radial document reload: the radial watcher classifies `radial.json` and the
asset tree, not `settings.json`. Visibility restoration remains a separate
`restore_flag`/main-plus-GUI owner path. Thus repeated preference writes or
preference-triggered hotkey/config cancellation are **not established causes**;
the split visibility ownership remains a separate S1 investigation.

## Reproduction matrix (S0 status)

No source-matched executable was safely observed in S0. Every row remains
**source-only / unverified** unless explicitly noted as a user observation.

| Case | Designer | Desktop preview | Focus before tap | Runtime radial | Current status |
|---|---|---|---|---|---|
| H0 | closed | stopped | grid | closed | Source-only/unverified; user reports the both-closed tap baseline works. |
| H1 | open, idle | stopped | grid | closed | Source-only/unverified. |
| H2 | open, idle | stopped | Designer | closed | Source-only/unverified. |
| H3 | open | active | grid | closed | Source-only/unverified; preview/Designer routing still needs native trace. |
| H4 | open | active | external app | closed | Source-only/unverified; target ownership/cross-process delivery still needs native trace. |
| H5 | closed | stopped | grid/external | active | Source-only/unverified; runtime hold-close is a separate lifecycle. |
| H6 | open | stopped or active as supported | grid/Designer | active | Source-only/unverified; test only the supported coexistence contract. |

Invalid steady-state boundary established by the current lifecycle: “Designer
closed + native desktop preview active” is not a valid preview-only case. Designer
resource release calls `NativePreviewCoordinator::cancel_all`; record it as
not applicable rather than inventing a coexistence repro. No other preview/
runtime combination is marked invalid without a lifecycle trace.

## Confirmed source causes and remaining hypotheses

These are audit findings for subsequent milestones, not claims that S0 repaired
them:

1. **Confirmed source ownership defect:** `ViewportCtx for egui::Context` in
   `src/visibility.rs` uses unqualified current-viewport commands/repaint,
   while the wake bridge can explicitly target ROOT. A shared deferred Designer
   context can therefore receive a command in the wrong viewport scope.
2. **Contributory split ownership:** visibility restore/apply is duplicated
   across main-loop ownership (`src/main.rs`/`src/visibility.rs`) and GUI
   update/restore (`src/gui/render.rs`, `restore_flag`); the exact native
   tap symptom is unverified.
3. **Confirmed close-intent loss:** `request_close` has an empty
   `AwaitingRequest` branch, and `poll_replies` can immediately start
   `FontCatalog` while close is being processed.
4. **Confirmed request reconciliation gap:** ordinary commit and native-preview
   send failures can leave the exact pending request installed, despite an error
   being recorded; native-preview stop send failure is currently ignored.
5. **Confirmed runtime terminal-feedback gap:** a failed runtime `Close` send
   calls `RadialController::retire_host`, retiring the host/active state without
   emitting a terminal `NativeEvent::Closed`/invocation feedback event.
6. **Confirmed tooltip dimensional defect:** `font_cache` multiplies the
   already-milli font size by `1_200` and stores it as milli-units, while the
   consumer divides by `1_000`; a 13-unit one-line label becomes 15,600 logical
   units instead of approximately 15.6 before padding.
7. **Confirmed native presentation ordering defect:** `NativeCommand::Present`
   reconfigures input and moves/shows the visual host before publishing the new
   layered pixels. Whether this produces the reported stale visual/phantom
   frame on the target machine still requires a native trace.
8. **Confirmed Designer layout/default defect:** the current Designer allocates
   tree/canvas/inspector children under a horizontal parent and defaults the
   optional tree/inspector expanded/visible, matching the observed overflow and
   character-wrapped columns.
9. **Confirmed Cascade composition defect:** `open_submenu` anchors Cascade at
   the selected parent cell and `cascade_layout` flattens ancestor cells/input
   bounds into a child layout, retaining one menu-level style; this spreads or
   detaches menus instead of composing complete overlapping frames.

Hypotheses requiring native/production traces remain separate: the actual
Designer-only versus preview-active state that triggers the tap failure; whether
the wrong viewport, a later restore, or native delivery is the final tap cause;
the user's exact mapped-key producer and provenance (`Physical` versus
`ExternalInjected` is not established, and no AHK/Talon/raw-chord assumption is
valid); the pending state present during the reported Designer X; the visible
effect of stale Present ordering; and real DPI, cross-process, compact-layout,
Cascade-overlap, and responsiveness behavior. The trace must record bounded
monotonic time, invocation/provenance class, owner/decision/target viewport and
HWND, settings generation, session IDs, desired/actual visibility plus later
restore, Designer request ID/kind/generation and wake/reply/close result, and
tooltip/layer generations/bounds. It must not record clipboard text, arbitrary
keystrokes, or sensitive window titles.

## Milestone status

| Milestone | Status | S0 boundary |
|---|---|---|
| S0 | `complete` | Identity, ownership map, source-only H0–H6 matrix, request classification, preference audit, causes, hypotheses, and planned trace recorded. |
| S1 | `complete` | Focused hotkey/visibility and Designer/runtime close lifecycle repairs and gate. |
| S2 | `complete` | Tooltip units and stable hover/native presentation repairs and gate. |
| S3 | `pending` | Slot-first compact Designer layout/interaction repairs and gate. |
| S4 | `pending` | Complete overlapping Cascade scenes and safe ancestor Back repairs and gate. |
| S5 | `pending` | Integrated full verification, native matrix, responsiveness/resource evidence, and independent review. |

## Slow-job policy and active-job template

This stabilization uses observation intervals **600 → 900 → 1,200 seconds** for
the same still-running job: first observation 600 seconds after launch, second
900 seconds after the first, then 1,200 seconds between later observations. They
are not kill deadlines, application timers, or unit-test sleeps. Never start a
duplicate Cargo process because output is quiet; retain and reattach to the same
job and preserve its true exit status.

```text
Job purpose / milestone:
Command and cwd:
Profile / features / target:
Source SHA + source/untracked diff fingerprint:
Session/PID + process start and identity:
Durable stdout/stderr log:
Exit-code record:
Launch time:
First observation due: launch + 600 seconds
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification:
Actual result / all failures / next corrective batch:
```

### S1 active-job record

Implementation completed from committed source HEAD
`1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9`. This post-gate lifecycle fix has
source fingerprint `88BFDBC594EAC94EEFBBBE69E798F78C770B1A6538DE20C6C45D75CF676CA3F9`.
The process identity and true exit result below are from the one sequential
post-fix gate; no duplicate Cargo/Nextest process was started.

```text
Job purpose / milestone: S1 focused lifecycle gate
Command and cwd: cargo nextest list/run, cargo check, cargo fmt --all -- --check; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; filter verification precedes focused `--no-fail-fast` batch
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + `88BFDBC594EAC94EEFBBBE69E798F78C770B1A6538DE20C6C45D75CF676CA3F9`
Session/PID + process start and identity: PTY session `66043`; wrapper PID `28828` (`pwsh`), start `2026-09-18T02:48:29.8994822Z`; one sequential Cargo tree
Durable stdout/stderr log: target/s1-focused-postfix2.log (ignored build-artifact path)
Exit-code record: target/s1-focused-postfix2.exit (ignored build-artifact path)
Launch time: `2026-09-18T02:48:29.8994822Z`
First observation due: `2026-09-18T02:58:29.8994822Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: selector preflight exit 0; final focused post-fix gate completed 127/127 tests with cargo check and fmt clean; no corrective batch remains within S1.
```

### S1 gate attempt 1 record

```text
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + CC64B1E1DEE28D15052F8CA911E26987909A5745F9B01DF7A3BA55C2085A3A23
Session/PID + process start and identity: PTY 14230; wrapper PID 29144 (pwsh); 2026-09-18T02:24:58.7452131Z
Durable stdout/stderr log: target/s1-focused.log
Exit-code record: target/s1-focused.exit
Command/result: focused 125-test nextest batch --no-fail-fast: exit 100, 123 passed / 2 failed; cargo check: 0; cargo fmt --all -- --check: 1
Diagnosed failures: native preview stop supersession test needed the new stop policy; child-context visibility test needed a registered deferred viewport. Both were corrected before rerun.
```

### S1 gate attempt 2 record

```text
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + 2D5FC3B26507147018B682B1A0455D4A86916A3F7F228FB07FE332DEEE0BB4D6
Session/PID + process start and identity: PTY 31024; wrapper PID 30472 (pwsh); 2026-09-18T02:28:12.5919060Z
Durable stdout/stderr log: target/s1-focused-rerun.log
Exit-code record: target/s1-focused-rerun.exit
Command/result: focused 125-test nextest batch --no-fail-fast: exit 100, 124 passed / 1 failed; cargo check: 0; cargo fmt --all -- --check: 1
Diagnosed failure: child-context visibility fixture needed the child `ViewportInfo` entry in `RawInput`; corrected before final gate.
```

### S1 gate attempt 3 record

```text
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + 178FFA99D17985595B4E64C6089C55BF5CCCA78B97294215C79F7F96CC6A6B5C
Session/PID + process start and identity: PTY 67229; wrapper PID 26284 (pwsh); 2026-09-18T02:33:30.1198934Z
Durable stdout/stderr log: target/s1-focused-final.log
Exit-code record: target/s1-focused-final.exit
Command/result: focused --no-fail-fast batch: exit 0, 126 passed / 3,858 skipped; cargo check: 0; cargo fmt --all -- --check: 0
Diagnosed result: clean before the additional close-time native supersession and discard-intent correction.
```

### S1 gate attempt 4 record

```text
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + 972346392AE24E683F2EDACC3A26D0CDF6C751B9A08080C22CA3CD583D21DA9B
Session/PID + process start and identity: PTY 49602; wrapper PID 30228 (pwsh); 2026-09-18T02:43:37.9023731Z
Durable stdout/stderr log: target/s1-focused-postfix.log
Exit-code record: target/s1-focused-postfix.exit
Command/result: selector preflight exit 0; focused --no-fail-fast batch exit 0, 127 passed / 3,858 skipped; cargo check exit 0; cargo fmt --all -- --check exit 0; overall exit 0 at 2026-09-18T02:46:03.3130224Z
Diagnosed result: close-time native Start/Update supersession and typed discard-close intent compile and pass the focused gate. Native H/C remains explicitly unverified.
```

### S1 gate attempt 5 record

```text
Source SHA + source/untracked diff fingerprint: 1250338dbb60cf5d01bd1b05dbebf5cbff20d2d9 + 88BFDBC594EAC94EEFBBBE69E798F78C770B1A6538DE20C6C45D75CF676CA3F9
Session/PID + process start and identity: PTY 66043; wrapper PID 28828 (pwsh); 2026-09-18T02:48:29.8994822Z
Durable stdout/stderr log: target/s1-focused-postfix2.log
Exit-code record: target/s1-focused-postfix2.exit
Command/result: selector preflight exit 0; focused --no-fail-fast batch exit 0, 127 passed / 3,858 skipped; cargo check exit 0; cargo fmt --all -- --check exit 0; overall exit 0 at 2026-09-18T02:50:00.1457544Z
Diagnosed result: Save/Discard failure prompts remain actionable after the close prompt is dismissed for the send attempt. Native H/C remains explicitly unverified.
```

### S2 active-job record

The first authoritative S2 gate completed on the durable identity below. Its
focused batch found one test assertion defect; Cargo check, fmt, and diff
check still completed cleanly. The test assertion has now been corrected to
compare physical glyph bounds after the same physical-to-logical conversion
used by production metrics. A single corrective focused gate is recorded
below; no overlapping Cargo process is allowed.

```text
Job purpose / milestone: S2 focused tooltip metrics and native presentation gate
Command and cwd: corrected selector preflight passed with `cargo nextest list -p multi_launcher -E 'test(/^(radial::(font_cache|tooltip|preparation|render|compositor|native|controller)|radial::authoring::native_preview|gui::radial_editor::preview)::tests::/)'`; authoritative `cargo nextest run -p multi_launcher --no-fail-fast -E 'test(/^(radial::(font_cache|tooltip|preparation|render|compositor|native|controller)|radial::authoring::native_preview|gui::radial_editor::preview)::tests::/)'`, then `cargo check`, `cargo fmt --all -- --check`, and `git diff --check`; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; selector preflight before one focused --no-fail-fast batch
Source SHA + source/untracked diff fingerprint: 1500af056b8939fdc11c481bb0e19daa8c70b85c + font_cache.rs SHA256 CA9989801FEA229D7B1DE08A8B5E016F72CAECD2C79074E26CAAC6097CDE29C2; native.rs SHA256 B710B906B818FC518E8DF608A2567849A12FC8E9BC9A334106165D2950540414
Session/PID + process start and identity: PTY session `47827`; wrapper PID `2784` (`pwsh`), start `2026-09-18T03:59:27.7936870Z`; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s2-focused-final.log
Exit-code record: target/s2-focused-final.exit
Launch time: `2026-09-18T03:59:27.7936870Z`
First observation due: `2026-09-18T04:09:27.7936870Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Corrected selector preflight: exit 0; 127 focused tests listed; `-E/--filterset` is the installed Nextest flag (the abandoned `--filter-expr` attempt selected 508 workspace tests and was terminated before execution).
Actual result / all failures / next corrective batch: focused batch exit 100, 130 tests run (129 passed, 1 failed), with `NEXTTEST_EXIT=100`; failure was the new DPI test comparing logical milli-width to physical-pixel glyph bounds. `CHECK_EXIT=0`, `FMT_EXIT=0`, `DIFF_CHECK_EXIT=0`, overall exit 100 at `2026-09-18T04:21:45.3675104Z`. Corrected assertion is recorded in the next gate.
```

### S2 gate corrective batch

```text
Job purpose / milestone: S2 corrective focused tooltip metrics and native presentation gate
Command and cwd: same verified selector and command as the active-job record above; G:\Repos\rust\Multi_Launcher
Profile / features / target: default target; one focused --no-fail-fast batch, then cargo check, cargo fmt --all -- --check, and git diff --check
Source SHA + source/untracked diff fingerprint: 1500af056b8939fdc11c481bb0e19daa8c70b85c + font_cache.rs SHA256 EB4D96066521C427B19137B8D095AF02A2FD6C0BD2AE1AA8CDC57D84E66364D2; native.rs SHA256 B710B906B818FC518E8DF608A2567849A12FC8E9BC9A334106165D2950540414
Session/PID + process start and identity: PTY session `55619`; wrapper PID `27048` (`pwsh`), start `2026-09-18T04:48:46.7783802Z`; one sequential Cargo/Nextest tree
Durable stdout/stderr log: target/s2-focused-corrective.log
Exit-code record: target/s2-focused-corrective.exit
Launch time: `2026-09-18T04:48:46.7783802Z`
First observation due: `2026-09-18T04:58:46.7783802Z` (launch + 600 seconds)
Second observation due: first check + 900 seconds
Later observations due: previous check + 1,200 seconds
Completion notification: parent agent /root
Actual result / all failures / next corrective batch: focused nextest completed with 130/130 passed and 4,456 skipped; cargo check, fmt, and diff check completed with exit 0; overall exit 0 at `2026-09-18T04:49:45.2060065Z`. Native H/C remains explicitly unverified; no S3 work was started.
```

### S2 implementation and evidence

The producer now keeps line height in logical milli-pixels by dividing before
line multiplication (`13_000 * 1_200 / 1_000 = 15_600`). Retained tooltip
width and height use the actual raster glyph extents and line advances,
converted from physical pixels to logical milli-pixels once and rounded
outward. Source text, grapheme handling, explicit breaks, fallback
diagnostics, label ellipsis, placement, and tooltip-only visual overflow
contracts remain unchanged.

Native presentation now retains a typed input/style/layout snapshot. A
same-layout visual update is `PixelOnly` and publishes the complete new
layered bitmap without input hide/show, region rebuild, input reposition,
activation/style changes, or animation-epoch reset. Layout/style/ownership
changes remain `Structural` and reconcile the input proxy after the new frame
has been published. The call-plan regression covers same-layout hover and
structural anchor/policy changes. The shared compositor/native-preview/
embedded paths continue to consume the same measured layout and visual scene.

The first gate exposed only a test-unit mismatch: the new DPI regression
compared logical milli-width to physical-pixel glyph bounds. The assertion was
corrected to use the production physical-to-logical conversion; the corrective
gate passed. No native desktop execution or pixel trace was performed, so the
native H/C rows and T1–T9 remain explicitly unverified.

## S0 verification record

Only the requested read-only Git checks are used for this milestone:

```text
git status --short --branch
git diff
git diff --staged
git diff --check -- docs/plans/radial-stabilization.md
```

S0 does not run Cargo, build, Nextest, native acceptance, reference scripts, or
real user automation. The ledger is the sole intended S0 file change.
